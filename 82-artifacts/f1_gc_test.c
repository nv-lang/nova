/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * f1_gc_test.c — Plan 82 Ф.1: arena + Boehm GC взаимодействие.
 *
 * `nova test` повесил concurrency-тесты после интеграции fiber_arena_win.c.
 * Standalone f1_arena_test (БЕЗ Boehm) проходит полностью. Разница
 * интегрированного окружения — Boehm GC. Этот харнесс воспроизводит
 * именно arena + Boehm без оркестрации nova test:
 *
 *   - minicoro-корутины на РЕАЛЬНОМ fiber_arena_win.c;
 *   - Boehm GC активен (GC_INIT, GC_THREADS, all-interior-pointers —
 *     как alloc_boehm.c рантайма Nova);
 *   - корутины делают GC_malloc + ФОРСИРУЮТ GC_gcollect, БУДУЧИ
 *     запущенными на arena-стеке → Boehm сканирует main-thread, чей
 *     CONTEXT.Rsp лежит в arena.
 *
 * Если харнесс виснет — баг arena+Boehm воспроизведён в контролируемом
 * окружении. Если проходит — баг в scheduler/libuv-слое, не в arena+GC.
 *
 * Watchdog-поток ловит hang.
 */
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define GC_NOT_DLL
#define GC_THREADS
#include <gc/gc.h>

#define MINICORO_IMPL
#include "../compiler-codegen/nova_rt/minicoro.h"
#include "../compiler-codegen/nova_rt/fiber_arena.h"

/* ─── watchdog ─── */
static volatile LONG g_done = 0;
static DWORD WINAPI watchdog(LPVOID a) {
    (void)a;
    Sleep(150 * 1000);
    if (!g_done) {
        fprintf(stderr, "\n!!! HANG DETECTED — f1_gc_test завис "
                "(arena + Boehm баг воспроизведён)\n");
        fflush(stderr);
        ExitProcess(0xDEAD);
    }
    return 0;
}

/* ─── post-create патч (реплика fibers.c::nova_fiber_post_create) ─── */
static void patch_stack_limit(mco_coro* co) {
    if (!co || !co->context) return;
    void* clow = nova_fiber_committed_low((const void*)co);
    if (clow) ((_mco_context*)co->context)->ctx.stack_limit = clow;
}
static mco_coro* create_coro(void (*entry)(mco_coro*), void* user) {
    size_t slot_usable = NOVA_FIBER_STACK_SIZE - NOVA_FIBER_GUARD_SIZE;
    mco_desc d = mco_desc_init(entry, slot_usable - 8192);
    d.alloc_cb = nova_fiber_alloc;
    d.dealloc_cb = nova_fiber_dealloc;
    d.allocator_data = NULL;
    d.user_data = user;
    mco_coro* co = NULL;
    if (mco_create(&co, &d) != MCO_SUCCESS || !co) return NULL;
    patch_stack_limit(co);
    return co;
}

/* ─── рабочие нагрузки ─── */
typedef struct { int yields; int collects; int done; int depth; } St;

/* T1: корутина на arena-стеке делает GC_malloc + ФОРСИРУЕТ GC_gcollect.
 * Это критичный сценарий — Boehm сканирует main-thread (Rsp в arena). */
static void gc_entry(mco_coro* co) {
    St* s = (St*)mco_get_user_data(co);
    void* objs[32];
    for (int i = 0; i < 32; i++) {
        objs[i] = GC_malloc(256);
        if (objs[i]) memset(objs[i], 0xAB, 256);
    }
    fprintf(stderr, "[t1] GC_malloc x32 done, форсирую GC_gcollect "
            "на arena-стеке...\n"); fflush(stderr);
    GC_gcollect();                       /* ← Boehm сканирует Rsp-в-arena */
    s->collects++;
    fprintf(stderr, "[t1] GC_gcollect #1 вернулся\n"); fflush(stderr);
    for (int i = 0; i < 32; i++) {
        unsigned char* p = (unsigned char*)objs[i];
        if (!p || p[0] != 0xAB || p[255] != 0xAB) { s->done = -1; return; }
    }
    mco_yield(co);
    /* resumed */
    for (int i = 0; i < 32; i++) {
        objs[i] = GC_malloc(512);
        if (objs[i]) memset(objs[i], 0xCD, 512);
    }
    GC_gcollect();
    s->collects++;
    for (int i = 0; i < 32; i++) {
        unsigned char* p = (unsigned char*)objs[i];
        if (!p || p[0] != 0xCD) { s->done = -1; return; }
    }
    s->done = 1;
}

/* T2: рекурсия растит arena-стек, на глубине — GC_gcollect (Boehm
 * сканирует ГЛУБОКО уросший arena-стек). */
static volatile unsigned long long g_sink;
static void deep_gc(int depth, int target, St* s) {
    volatile char frame[3072];
    frame[0] = (char)depth;
    frame[3071] = (char)(depth * 7);
    s->depth = depth;
    void* o = GC_malloc(128);
    if (o) memset(o, (int)(depth & 0xFF), 128);
    if (depth < target) {
        deep_gc(depth + 1, target, s);
    } else {
        fprintf(stderr, "[t2] глубина %d — форсирую GC_gcollect...\n", depth);
        fflush(stderr);
        GC_gcollect();                   /* ← Boehm сканирует глубокий arena-стек */
        s->collects++;
        fprintf(stderr, "[t2] GC_gcollect на глубине вернулся\n");
        fflush(stderr);
    }
    g_sink += (unsigned long long)(unsigned char)frame[0];
}
static void deep_entry(mco_coro* co) {
    St* s = (St*)mco_get_user_data(co);
    deep_gc(1, 400, s);                  /* 400*3K ≈ 1.2 MB grown */
    s->done = 1;
}

/* T3: round-robin корутины, каждая GC_malloc+yield; между раундами —
 * GC_gcollect с main-стека (корутины suspended). */
static void rr_entry(mco_coro* co) {
    St* s = (St*)mco_get_user_data(co);
    for (int i = 0; i < s->yields; i++) {
        void* o = GC_malloc(200);
        if (o) memset(o, 0x5A, 200);
        s->collects++;
        mco_yield(co);
    }
    s->done = 1;
}

/* ─── под-тесты ─── */
static int run_t1(void) {
    fprintf(stderr, "--- T1: GC_gcollect изнутри корутины на arena-стеке ---\n");
    St s; memset(&s, 0, sizeof(s));
    mco_coro* co = create_coro(gc_entry, &s);
    if (!co) { printf("  T1 create FAIL\n"); return -1; }
    mco_resume(co);   /* до yield */
    mco_resume(co);   /* после yield */
    int ok = (s.done == 1 && s.collects == 2);
    printf("  T1: done=%d collects=%d -> %s\n", s.done, s.collects,
           ok ? "OK" : "FAIL");
    mco_destroy(co);
    return ok ? 0 : -1;
}

static int run_t2(void) {
    fprintf(stderr, "--- T2: GC_gcollect на глубоко уросшем arena-стеке ---\n");
    St s; memset(&s, 0, sizeof(s));
    mco_coro* co = create_coro(deep_entry, &s);
    if (!co) { printf("  T2 create FAIL\n"); return -1; }
    mco_resume(co);
    int ok = (s.done == 1 && s.collects == 1 && s.depth == 400);
    printf("  T2: done=%d collects=%d depth=%d -> %s\n",
           s.done, s.collects, s.depth, ok ? "OK" : "FAIL");
    mco_destroy(co);
    return ok ? 0 : -1;
}

static int run_t3(int n) {
    fprintf(stderr, "--- T3: round-robin %d корутин + GC между раундами ---\n", n);
    mco_coro* co[16]; St st[16];
    if (n > 16) return -1;
    for (int i = 0; i < n; i++) {
        memset(&st[i], 0, sizeof(st[i]));
        st[i].yields = 4;
        co[i] = create_coro(rr_entry, &st[i]);
        if (!co[i]) { printf("  T3 create #%d FAIL\n", i); return -1; }
    }
    long rounds = 0;
    for (;;) {
        int alive = 0;
        for (int i = 0; i < n; i++) {
            if (mco_status(co[i]) == MCO_SUSPENDED) {
                mco_resume(co[i]);
                alive++;
            }
        }
        GC_gcollect();    /* collect между раундами — корутины suspended */
        if (!alive) break;
        if (++rounds > 64) { printf("  T3 LIVELOCK\n"); return -1; }
    }
    int ok = 1;
    for (int i = 0; i < n; i++) {
        if (st[i].done != 1) ok = 0;
        mco_destroy(co[i]);
    }
    printf("  T3: %ld раундов -> %s\n", rounds, ok ? "OK" : "FAIL");
    return ok ? 0 : -1;
}

/* T4: МНОГО созданных-но-не-запущенных корутин + GC_gcollect.
 * Воспроизводит sleep_bench: supervised создаёт тысячи fiber'ов до
 * запуска, scope-grow триггерит GC → push-колбэк пушит N fiber-стеков.
 * sleep_bench виснет в колбэке при ~2048 fiber'ах. */
static int run_t4(int n) {
    fprintf(stderr, "--- T4: %d созданных fiber'ов + GC_gcollect ---\n", n);
    mco_coro** co = (mco_coro**)malloc(sizeof(mco_coro*) * (size_t)n);
    St* st = (St*)calloc((size_t)n, sizeof(St));
    for (int i = 0; i < n; i++) {
        co[i] = create_coro(rr_entry, &st[i]);
        if (!co[i]) { printf("  T4 create #%d FAIL (exhausted?)\n", i);
                      return -1; }
        if (i > 0 && i % 256 == 0) {
            fprintf(stderr, "[t4] создано %d, форсирую GC_gcollect...\n", i);
            fflush(stderr);
            GC_gcollect();   /* ← push-колбэк пушит i fiber-стеков */
            fprintf(stderr, "[t4] GC_gcollect при %d fiber'ах вернулся\n", i);
            fflush(stderr);
        }
    }
    fprintf(stderr, "[t4] все %d созданы — финальный GC_gcollect...\n", n);
    fflush(stderr);
    GC_gcollect();
    fprintf(stderr, "[t4] финальный GC_gcollect вернулся\n"); fflush(stderr);
    for (int i = 0; i < n; i++) mco_destroy(co[i]);
    free(co); free(st);
    printf("  T4: %d fiber'ов + GC -> OK\n", n);
    return 0;
}

int main(int argc, char** argv) {
    setvbuf(stdout, NULL, _IONBF, 0);
    CreateThread(NULL, 0, watchdog, NULL, 0, NULL);

    /* Boehm-инициализация — как alloc_boehm.c рантайма Nova. */
    GC_set_all_interior_pointers(1);
    GC_INIT();
    GC_allow_register_threads();          /* для GC_register_my_thread из потоков */
    fprintf(stderr, "[gc] Boehm инициализирован\n"); fflush(stderr);

    int t4n = (argc > 1) ? atoi(argv[1]) : 3000;
    printf("=== Plan 82 Ф.1 — arena + Boehm GC harness ===\n\n");
    int rc = 0;
    if (run_t1() != 0) rc = 1;
    printf("\n");
    if (run_t2() != 0) rc = 1;
    printf("\n");
    if (run_t3(8) != 0) rc = 1;
    printf("\n");
    if (run_t4(t4n) != 0) rc = 1;
    printf("\n");

    InterlockedExchange(&g_done, 1);
    printf("=== ВЕРДИКТ ===\n  %s\n", rc == 0
        ? "OK — arena + Boehm работают вместе; hang НЕ в arena+GC"
        : "FAIL — arena + Boehm баг воспроизведён");
    return rc;
}
