/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * f1_arena_test.c — Plan 82 Ф.1: standalone-проверка fiber_arena_win.c.
 *
 * Spike-перед-интеграцией (методологический урок 44.3 — 4 провала шли
 * сразу в интеграцию). Линкует РЕАЛЬНЫЙ compiler-codegen/nova_rt/
 * fiber_arena_win.c и гоняет на нём minicoro-корутины.
 *
 * Под-тесты:
 *   T1 basic        — alloc → shallow coroutine → dealloc;
 *   T2 deep grow    — рекурсия растит стек на ~2 MB (путь A OS-grow);
 *   T3 __chkstk     — функция с кадром >1 страницы (патч stack_limit);
 *   T4 slot reuse   — 5000 циклов create→destroy, детект утечки;
 *   T5 concurrency  — round-robin над 32 корутинами;
 *   T6 lazy commit  — 64 shallow-корутины: commit-charge на слот мал
 *                     (~28K, НЕ 8 MB) — lazy-commit работает;
 *   T7 overflow     — (child) безусловная рекурсия → STATUS_STACK_OVERFLOW
 *                     + VEH-сообщение «fiber stack overflow in slot N».
 *
 * Watchdog-поток ловит hang.
 */
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>

#define MINICORO_IMPL
#include "../compiler-codegen/nova_rt/minicoro.h"
#include "../compiler-codegen/nova_rt/fiber_arena.h"

/* ─── watchdog ─── */
static volatile LONG g_done = 0;
static DWORD WINAPI watchdog(LPVOID a) {
    (void)a;
    Sleep(25 * 1000);
    if (!g_done) {
        fprintf(stderr, "\n!!! HANG DETECTED — f1_arena_test завис\n");
        fflush(stderr);
        ExitProcess(0xDEAD);
    }
    return 0;
}

/* ─── post-create патч (реплика fibers.c::nova_fiber_post_create) ─── */
static void patch_stack_limit(mco_coro* co) {
    if (!co || !co->context) return;
    void* clow = nova_fiber_committed_low((const void*)co);
    if (clow) {
        ((_mco_context*)co->context)->ctx.stack_limit = clow;
    }
}

static mco_desc make_desc(void (*entry)(mco_coro*)) {
    size_t slot_usable = NOVA_FIBER_STACK_SIZE - NOVA_FIBER_GUARD_SIZE;
    size_t stack_size  = slot_usable - 8192;   /* = _NOVA_MCO_HEADER_OVERHEAD */
    mco_desc d = mco_desc_init(entry, stack_size);
    d.alloc_cb       = nova_fiber_alloc;
    d.dealloc_cb     = nova_fiber_dealloc;
    d.allocator_data = NULL;
    return d;
}

/* создать+пропатчить корутину; NULL при ошибке */
static mco_coro* create_coro(void (*entry)(mco_coro*), void* user) {
    mco_desc d = make_desc(entry);
    d.user_data = user;
    mco_coro* co = NULL;
    if (mco_create(&co, &d) != MCO_SUCCESS || !co) return NULL;
    patch_stack_limit(co);
    return co;
}

/* ─── рабочие нагрузки ─── */
typedef struct { int n_yields; int progress; int done; int depth_reached; }
        CoState;

static void shallow_entry(mco_coro* co) {
    CoState* s = (CoState*)mco_get_user_data(co);
    s->progress = 1;
    s->done = 1;
}

/* sub-page кадр — растит стек постранично обычными записями */
static volatile unsigned long long g_sink;
static void deep_recurse(int depth, int target, int* reached) {
    volatile char frame[3072];
    frame[0] = (char)depth;
    frame[3071] = (char)(depth * 5);
    *reached = depth;
    if (depth < target) deep_recurse(depth + 1, target, reached);
    g_sink += (unsigned long long)(unsigned char)frame[0];
}
static void deep_entry(mco_coro* co) {
    CoState* s = (CoState*)mco_get_user_data(co);
    deep_recurse(1, 700, &s->depth_reached);  /* 700*3K ≈ 2.1 MB */
    s->done = 1;
}

/* кадр >1 страницы — компилятор эмитит __chkstk-пробинг.
 * Без патча stack_limit крашит на MSVC (Ф.0 test a). */
static void chkstk_recurse(int depth, int target, int* reached) {
    volatile char frame[9000];
    frame[0] = (char)depth;
    frame[8999] = (char)(depth * 3);
    *reached = depth;
    if (depth < target) chkstk_recurse(depth + 1, target, reached);
    g_sink += (unsigned long long)(unsigned char)frame[0];
}
static void chkstk_entry(mco_coro* co) {
    CoState* s = (CoState*)mco_get_user_data(co);
    chkstk_recurse(1, 80, &s->depth_reached);  /* 80*9K ≈ 720 KB */
    s->done = 1;
}

/* round-robin yield-нагрузка */
static void rr_entry(mco_coro* co) {
    CoState* s = (CoState*)mco_get_user_data(co);
    for (int i = 0; i < s->n_yields; i++) { s->progress++; mco_yield(co); }
    s->done = 1;
}

/* безусловная рекурсия — overflow */
static void overflow_recurse(int d) {
    volatile char frame[4096];
    frame[0] = (char)d;
    frame[4095] = (char)d;
    overflow_recurse(d + 1);
    g_sink += (unsigned long long)(unsigned char)frame[0];
}
static void overflow_entry(mco_coro* co) { (void)co; overflow_recurse(1); }

/* ─── committed-байты в диапазоне ─── */
static size_t committed_in(char* lo, char* hi) {
    size_t total = 0;
    char* p = lo;
    while (p < hi) {
        MEMORY_BASIC_INFORMATION mbi;
        if (!VirtualQuery(p, &mbi, sizeof(mbi))) break;
        char* rend = (char*)mbi.BaseAddress + mbi.RegionSize;
        if (rend > hi) rend = hi;
        if (mbi.State == MEM_COMMIT) {
            char* cs = (char*)mbi.BaseAddress < lo ? lo : (char*)mbi.BaseAddress;
            if (rend > cs) total += (size_t)(rend - cs);
        }
        p = (char*)mbi.BaseAddress + mbi.RegionSize;
    }
    return total;
}

/* ─── под-тесты ─── */
static int run_simple(const char* name, void (*entry)(mco_coro*)) {
    CoState s; memset(&s, 0, sizeof(s));
    mco_coro* co = create_coro(entry, &s);
    if (!co) { printf("  [%s] create FAIL\n", name); return -1; }
    mco_result r = mco_resume(co);
    if (r != MCO_SUCCESS) { printf("  [%s] resume FAIL %d\n", name, (int)r);
                            mco_destroy(co); return -1; }
    int ok = s.done;
    printf("  [%s] done=%d depth_reached=%d → %s\n",
           name, s.done, s.depth_reached, ok ? "OK" : "FAIL");
    mco_destroy(co);
    return ok ? 0 : -1;
}

static int run_slot_reuse(int cycles) {
    printf("  [T4 slot reuse] %d циклов create→run→destroy...\n", cycles);
    for (int i = 0; i < cycles; i++) {
        CoState s; memset(&s, 0, sizeof(s));
        mco_coro* co = create_coro(shallow_entry, &s);
        if (!co) { printf("  цикл %d: create FAIL\n", i); return -1; }
        if (mco_resume(co) != MCO_SUCCESS || !s.done) {
            printf("  цикл %d: run FAIL\n", i); mco_destroy(co); return -1;
        }
        mco_destroy(co);
    }
    NovaFiberArenaStats st = nova_fiber_arena_stats();
    int ok = (st.slots_active == 0);
    printf("  [T4 slot reuse] %d циклов; slots_active=%zu high_water=%zu → %s\n",
           cycles, st.slots_active, st.high_water,
           ok ? "OK (нет утечки слотов)" : "FAIL (утечка)");
    return ok ? 0 : -1;
}

static int run_round_robin(int n, int yields) {
    printf("  [T5 concurrency] %d корутин × %d yield...\n", n, yields);
    mco_coro* co[32]; CoState st[32];
    if (n > 32) return -1;
    for (int i = 0; i < n; i++) {
        memset(&st[i], 0, sizeof(st[i]));
        st[i].n_yields = yields;
        co[i] = create_coro(rr_entry, &st[i]);
        if (!co[i]) { printf("  create #%d FAIL\n", i); return -1; }
    }
    long rounds = 0, cap = (long)(yields + 4) * 4;
    for (;;) {
        int alive = 0;
        for (int i = 0; i < n; i++) {
            if (mco_status(co[i]) == MCO_SUSPENDED) {
                if (mco_resume(co[i]) != MCO_SUCCESS) {
                    printf("  resume #%d FAIL\n", i); return -1;
                }
                alive++;
            }
        }
        if (!alive) break;
        if (++rounds > cap) { printf("  LIVELOCK\n"); return -1; }
    }
    int ok = 1;
    for (int i = 0; i < n; i++) {
        if (!st[i].done || st[i].progress != yields) ok = 0;
        mco_destroy(co[i]);
    }
    printf("  [T5 concurrency] %ld раундов → %s\n", rounds, ok ? "OK" : "FAIL");
    return ok ? 0 : -1;
}

static int run_lazy_commit(int n) {
    printf("  [T6 lazy commit] %d shallow-корутин, замер commit/слот...\n", n);
    mco_coro* co[64]; CoState st[64];
    if (n > 64) return -1;
    for (int i = 0; i < n; i++) {
        memset(&st[i], 0, sizeof(st[i]));
        co[i] = create_coro(shallow_entry, &st[i]);
        if (!co[i]) { printf("  create #%d FAIL\n", i); return -1; }
        mco_resume(co[i]);   /* shallow — не растит стек */
    }
    /* commit на слот = committed-байты блока корутины */
    size_t worst = 0;
    for (int i = 0; i < n; i++) {
        char* lo = (char*)co[i]->stack_base;
        char* hi = lo + co[i]->stack_size;
        size_t c = committed_in(lo, hi);
        if (c > worst) worst = c;
    }
    /* shallow-корутина не должна закоммитить близко к 8 MB слота.
     * Порог щедрый — 512 KB; реально ожидается ~24-40 KB. */
    int ok = (worst < (size_t)(512 * 1024));
    printf("  [T6 lazy commit] worst commit/слот = %zu KB (порог 512 KB) → %s\n",
           worst / 1024,
           ok ? "OK — lazy-commit работает (НЕ 8 MB/слот)" : "FAIL");
    for (int i = 0; i < n; i++) mco_destroy(co[i]);
    return ok ? 0 : -1;
}

/* ─── self-spawn для T7 ─── */
static int run_self(const char* arg) {
    char exe[MAX_PATH];
    if (!GetModuleFileNameA(NULL, exe, MAX_PATH)) return -1;
    char cmd[MAX_PATH + 64];
    snprintf(cmd, sizeof(cmd), "\"%s\" %s", exe, arg);
    STARTUPINFOA si; ZeroMemory(&si, sizeof(si)); si.cb = sizeof(si);
    PROCESS_INFORMATION pi; ZeroMemory(&pi, sizeof(pi));
    if (!CreateProcessA(NULL, cmd, NULL, NULL, TRUE, 0, NULL, NULL, &si, &pi))
        return -1;
    WaitForSingleObject(pi.hProcess, INFINITE);
    DWORD ec = 0;
    GetExitCodeProcess(pi.hProcess, &ec);
    CloseHandle(pi.hProcess); CloseHandle(pi.hThread);
    return (int)ec;
}

int main(int argc, char** argv) {
    setvbuf(stdout, NULL, _IONBF, 0);
    CreateThread(NULL, 0, watchdog, NULL, 0, NULL);

    if (argc > 1 && strcmp(argv[1], "overflow") == 0) {
        printf("=== T7 overflow child ===\n");
        CoState s; memset(&s, 0, sizeof(s));
        mco_coro* co = create_coro(overflow_entry, &s);
        if (!co) { printf("  create FAIL\n"); InterlockedExchange(&g_done,1);
                   return 2; }
        printf("  резюмим overflow-корутину (ожидаем STATUS_STACK_OVERFLOW)...\n");
        mco_resume(co);
        printf("  mco_resume ВЕРНУЛСЯ — overflow не сработал (?!)\n");
        InterlockedExchange(&g_done, 1);
        return 3;
    }

    printf("=== Plan 82 Ф.1 — standalone-проверка fiber_arena_win.c ===\n\n");
    int rc = 0;

    printf("--- T1: basic alloc + shallow coroutine ---\n");
    if (run_simple("T1 shallow", shallow_entry) != 0) rc = 1;
    printf("\n--- T2: deep recursion — OS-native grow (~2 MB) ---\n");
    if (run_simple("T2 deep", deep_entry) != 0) rc = 1;
    printf("\n--- T3: __chkstk-кадр >1 страницы (патч stack_limit) ---\n");
    if (run_simple("T3 chkstk", chkstk_entry) != 0) rc = 1;
    printf("\n");
    if (run_slot_reuse(5000) != 0) rc = 1;
    printf("\n");
    if (run_round_robin(32, 5) != 0) rc = 1;
    printf("\n");
    if (run_lazy_commit(64) != 0) rc = 1;
    printf("\n");

    printf("--- T7: overflow detection (child process) ---\n");
    int t7 = run_self("overflow");
    DWORD u = (DWORD)t7;
    int t7_so   = (u == 0xC00000FD);   /* STATUS_STACK_OVERFLOW */
    int t7_av   = (u == 0xC0000005);   /* ACCESS_VIOLATION      */
    int t7_hang = (u == 0xDEAD);
    printf("  T7 child exit = 0x%08lX  %s\n", u,
           t7_so   ? "(STATUS_STACK_OVERFLOW — детерминированная детекция)" :
           t7_av   ? "(ACCESS_VIOLATION — guard-AV)" :
           t7_hang ? "(HANG!)" : "(иное)");
    int t7_ok = (t7_so || t7_av) && !t7_hang;
    if (!t7_ok) rc = 1;
    printf("\n");

    InterlockedExchange(&g_done, 1);

    printf("=== ВЕРДИКТ Ф.1 standalone ===\n");
    printf("  %s\n", rc == 0
        ? "ВСЕ ПОД-ТЕСТЫ OK — fiber_arena_win.c готов к интеграции"
        : "ЕСТЬ ПРОВАЛЫ — см.выше");
    return rc;
}
