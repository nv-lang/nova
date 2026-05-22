/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * f0_test_d.c — Plan 82 Ф.0 test (d): аномалия Попыток 3-4 (Plan 44.3).
 *
 * Попытки 3-4 (44.3, 2026-05-13) сообщали «даже простой main_yield
 * TIMEOUT 60+ сек». Документированный root cause Попытки 4 («minicoro
 * MCO_USE_ASM не обновляет TIB») ОПРОВЕРГНУТ перепроверкой (§1.1-1.2
 * плана 82): vendored minicoro x64-asm свопает NT_TIB на каждом switch.
 * Реальная причина TIMEOUT — неизвестна. §1.4 / §9-риск-2 плана требуют:
 * Ф.0 обязана воспроизвести аномалию на изолированном харнессе ЛИБО
 * доказать, что это был баг откаченного arena-кода.
 *
 * Этот харнесс воспроизводит дизайн Попыток 3-4 на standalone-уровне
 * (вне рантайма Nova, как и f0_probe / f0_test_a):
 *   - arena 256 KB слотов на VirtualAlloc(MEM_RESERVE);    (44.3 Поп.3)
 *   - guard 16 KB PAGE_NOACCESS у низа слота;
 *   - commit usable-региона при ПЕРВОМ alloc слота (committed_bits);
 *   - bitmap free-list, reuse слотов;
 *   - БЕЗ decommit вообще (44.3 Попытка 4 — no VirtualFree);
 *   - minicoro MCO_USE_ASM на этих слотах.
 *
 * main_yield (nova_tests/concurrency/main_yield.nv) = supervised-scope
 * с 1-3 fiber'ами, каждый yield'ит 1-2 раза, round-robin-планировщик.
 * Харнесс воспроизводит ровно это ядро: minicoro-корутины на arena-
 * слотах + round-robin-резюм до завершения всех.
 *
 * Под-тесты:
 *   T1 main_yield-аналог — round-robin над 3 корутинами;
 *   T2 slot reuse        — 20000 циклов create→run→destroy на одном слоте;
 *   T3 concurrency+churn — 10 пачек по 32 корутины round-robin;
 *   T4 (child process)   — §5.1 layout-defect: deep overflow корутины,
 *                          стек растёт вниз В minicoro-header → что
 *                          происходит: чистый guard-AV или hang?
 *
 * Watchdog-поток детектит hang: если прогон не завершился за
 * WATCHDOG_SEC — печатает HANG и убивает процесс (exit 0xDEAD).
 * Soft-livelock (планировщик крутит корутину, которая не достигает
 * MCO_DEAD) ловится счётчиком раундов MAX_ROUNDS.
 */
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>

#define MINICORO_IMPL
#include "../compiler-codegen/nova_rt/minicoro.h"

/* ─── конфигурация арены — точные числа Попыток 3-4 (44.3) ─── */
#define PAGE          ((size_t)4096)
#define SLOT_SIZE     ((size_t)(256 * 1024))   /* 44.3 Поп.3: 256 KB слот   */
#define GUARD_SIZE    ((size_t)(16  * 1024))   /* 16 KB guard у низа слота   */
#define SLOT_COUNT    128                       /* 128 × 256K = 32 MB резерв */
#define HEADER_MARGIN ((size_t)(32 * 1024))    /* запас под minicoro-header  */

#define WATCHDOG_SEC  20                        /* 44.3 TIMEOUT был 60 сек    */

/* ─── per-thread arena (точная реплика 44.3 Попытки 4) ─── */
typedef struct {
    char*    base;
    size_t   slot_size;
    size_t   slot_count;
    size_t   slots_active;
    size_t   high_water;
    uint64_t free_bits[(SLOT_COUNT + 63) / 64];   /* 1 = used  */
    uint64_t committed_bits[(SLOT_COUNT + 63) / 64]; /* 1 = слот уже committed */
} Arena;

static Arena g_arena = {0};

static int arena_init(void) {
    if (g_arena.base) return 1;
    size_t vsize = SLOT_SIZE * SLOT_COUNT;
    void* p = VirtualAlloc(NULL, vsize, MEM_RESERVE, PAGE_NOACCESS);
    if (!p) {
        printf("  VirtualAlloc(RESERVE %zu) failed: %lu\n", vsize, GetLastError());
        return 0;
    }
    g_arena.base       = (char*)p;
    g_arena.slot_size  = SLOT_SIZE;
    g_arena.slot_count = SLOT_COUNT;
    return 1;
}

static size_t arena_find_free(void) {
    size_t words = (SLOT_COUNT + 63) / 64;
    for (size_t w = 0; w < words; w++) {
        uint64_t inv = ~g_arena.free_bits[w];
        if (!inv) continue;
        unsigned long bit;
        _BitScanForward64(&bit, inv);
        size_t slot = w * 64 + bit;
        if (slot < SLOT_COUNT) return slot;
    }
    return (size_t)-1;
}

/* minicoro alloc_cb — реплика 44.3: commit usable при первом использовании
 * слота, reuse через bitmap, БЕЗ decommit. */
static void* arena_alloc(size_t size, void* udata) {
    (void)udata;
    if (!g_arena.base) return NULL;
    size_t usable = SLOT_SIZE - GUARD_SIZE;
    if (size > usable) {
        printf("  arena_alloc: requested %zu > usable %zu\n", size, usable);
        return NULL;
    }
    size_t slot = arena_find_free();
    if (slot == (size_t)-1) {
        printf("  arena exhausted (%zu slots active)\n", g_arena.slots_active);
        return NULL;
    }
    char* slot_base = g_arena.base + slot * SLOT_SIZE;
    size_t w = slot / 64, b = slot % 64;
    if (!(g_arena.committed_bits[w] & (1ULL << b))) {
        /* первый alloc этого слота — commit usable-региона целиком */
        char* usable_base = slot_base + GUARD_SIZE;
        if (!VirtualAlloc(usable_base, usable, MEM_COMMIT, PAGE_READWRITE)) {
            printf("  VirtualAlloc(COMMIT slot %zu) failed: %lu\n",
                   slot, GetLastError());
            return NULL;
        }
        g_arena.committed_bits[w] |= (1ULL << b);
    }
    g_arena.free_bits[w] |= (1ULL << b);
    g_arena.slots_active++;
    if (slot + 1 > g_arena.high_water) g_arena.high_water = slot + 1;
    return slot_base + GUARD_SIZE;   /* отдаём minicoro регион над guard'ом */
}

/* minicoro dealloc_cb — реплика 44.3: bitmap free, БЕЗ decommit. */
static void arena_free(void* ptr, size_t size, void* udata) {
    (void)size; (void)udata;
    if (!ptr || !g_arena.base) return;
    char* p = (char*)ptr;
    if (p < g_arena.base + GUARD_SIZE ||
        p >= g_arena.base + SLOT_SIZE * SLOT_COUNT) {
        printf("  arena_free: ptr %p вне арены\n", ptr);
        return;
    }
    size_t offset = (size_t)(p - g_arena.base - GUARD_SIZE);
    size_t slot   = offset / SLOT_SIZE;
    if (slot >= SLOT_COUNT) { printf("  arena_free: slot вне диапазона\n"); return; }
    size_t w = slot / 64, b = slot % 64;
    g_arena.free_bits[w] &= ~(1ULL << b);
    g_arena.slots_active--;
}

static size_t coro_stack_size(void) {
    return SLOT_SIZE - GUARD_SIZE - HEADER_MARGIN;
}

static mco_desc make_desc(void (*entry)(mco_coro*)) {
    mco_desc d = mco_desc_init(entry, coro_stack_size());
    d.alloc_cb       = arena_alloc;
    d.dealloc_cb     = arena_free;
    d.allocator_data = NULL;
    return d;
}

/* ─── watchdog ─── */
static volatile LONG g_done = 0;
static DWORD WINAPI watchdog(LPVOID arg) {
    (void)arg;
    Sleep(WATCHDOG_SEC * 1000);
    if (!g_done) {
        fprintf(stderr,
            "\n!!! HANG DETECTED — прогон не завершился за %d сек.\n"
            "    Аномалия Попыток 3-4 ВОСПРОИЗВЕДЕНА на standalone-харнессе.\n",
            WATCHDOG_SEC);
        fflush(stderr);
        ExitProcess(0xDEAD);
    }
    return 0;
}

/* ─── T1: main_yield-аналог — round-robin над корутинами ─── */
typedef struct { int yields; int progress; int done; } CoState;

static void rr_entry(mco_coro* co) {
    CoState* s = (CoState*)mco_get_user_data(co);
    for (int i = 0; i < s->yields; i++) {
        s->progress++;
        mco_yield(co);
    }
    s->done = 1;
}

/* round-robin планировщик — точный аналог supervised-scope main_yield.
 * Возвращает 0 = OK, -1 = livelock, -2 = ошибка. */
static int run_round_robin(const char* name, int n_coros, int yields_each) {
    printf("  [%s] %d корутин × %d yield...\n", name, n_coros, yields_each);
    mco_coro* cos[64];
    CoState   st[64];
    if (n_coros > 64) return -2;

    for (int i = 0; i < n_coros; i++) {
        st[i].yields = yields_each; st[i].progress = 0; st[i].done = 0;
        mco_desc d = make_desc(rr_entry);
        d.user_data = &st[i];
        cos[i] = NULL;
        mco_result r = mco_create(&cos[i], &d);
        if (r != MCO_SUCCESS || !cos[i]) {
            printf("  mco_create #%d failed: %d\n", i, (int)r);
            return -2;
        }
    }

    /* round-robin: резюмим все не-DEAD корутины, пока все не завершатся.
     * Каждая корутина: yields_each раз yield + финальный run → DEAD после
     * yields_each+1 резюмов. MAX_ROUNDS ловит soft-livelock. */
    long max_rounds = (long)(yields_each + 4) * 8;
    long rounds = 0;
    for (;;) {
        int alive = 0;
        for (int i = 0; i < n_coros; i++) {
            if (mco_status(cos[i]) == MCO_SUSPENDED) {
                mco_result r = mco_resume(cos[i]);
                if (r != MCO_SUCCESS) {
                    printf("  mco_resume #%d failed: %d\n", i, (int)r);
                    return -2;
                }
                alive++;
            }
        }
        if (!alive) break;
        if (++rounds > max_rounds) {
            printf("  LIVELOCK: %ld раундов > предел %ld — корутина не "
                   "достигает MCO_DEAD\n", rounds, max_rounds);
            return -1;
        }
    }

    int ok = 1;
    for (int i = 0; i < n_coros; i++) {
        if (!st[i].done || st[i].progress != yields_each) {
            printf("  корутина #%d: done=%d progress=%d (ожидалось %d)\n",
                   i, st[i].done, st[i].progress, yields_each);
            ok = 0;
        }
        mco_destroy(cos[i]);
    }
    printf("  [%s] %ld раундов, %s\n", name, rounds, ok ? "OK" : "ОШИБКА");
    return ok ? 0 : -2;
}

/* ─── T2: slot reuse — много циклов create→run-to-done→destroy ─── */
static void noyield_entry(mco_coro* co) {
    CoState* s = (CoState*)mco_get_user_data(co);
    s->progress = 1;
    s->done = 1;
}

static int run_slot_reuse(int cycles) {
    printf("  [T2 slot reuse] %d циклов create→run→destroy...\n", cycles);
    size_t max_hw = 0;
    for (int i = 0; i < cycles; i++) {
        CoState s = {0, 0, 0};
        mco_desc d = make_desc(noyield_entry);
        d.user_data = &s;
        mco_coro* co = NULL;
        mco_result r = mco_create(&co, &d);
        if (r != MCO_SUCCESS || !co) {
            printf("  цикл %d: mco_create failed: %d\n", i, (int)r);
            return -2;
        }
        r = mco_resume(co);
        if (r != MCO_SUCCESS) {
            printf("  цикл %d: mco_resume failed: %d\n", i, (int)r);
            return -2;
        }
        if (!s.done) { printf("  цикл %d: корутина не завершилась\n", i); return -2; }
        mco_destroy(co);
        if (g_arena.high_water > max_hw) max_hw = g_arena.high_water;
        if (g_arena.slots_active != 0) {
            printf("  цикл %d: УТЕЧКА СЛОТА — slots_active=%zu (ожидалось 0)\n",
                   i, g_arena.slots_active);
            return -1;
        }
    }
    printf("  [T2 slot reuse] %d циклов OK; high_water=%zu (нет утечки — "
           "слот переиспользуется)\n", cycles, max_hw);
    return 0;
}

/* ─── T3: concurrency + churn — пачки round-robin ─── */
static int run_churn(int batches, int per_batch, int yields_each) {
    printf("  [T3 churn] %d пачек × %d корутин × %d yield...\n",
           batches, per_batch, yields_each);
    for (int b = 0; b < batches; b++) {
        int r = run_round_robin("T3-batch", per_batch, yields_each);
        if (r != 0) { printf("  пачка %d ПРОВАЛ\n", b); return r; }
        if (g_arena.slots_active != 0) {
            printf("  пачка %d: slots_active=%zu после завершения (утечка)\n",
                   b, g_arena.slots_active);
            return -1;
        }
    }
    printf("  [T3 churn] %d пачек OK; high_water=%zu\n",
           batches, g_arena.high_water);
    return 0;
}

/* ─── T4 (child): §5.1 layout-defect — deep overflow корутины ─── */
static volatile unsigned long long g_sink;
static int g_overflow_depth;

static void overflow_recurse(int depth) {
    volatile char frame[4096];
    frame[0] = (char)depth;
    frame[4095] = (char)(depth * 7);
    g_overflow_depth = depth;
    /* безусловная рекурсия — упрётся в guard либо в minicoro-header */
    overflow_recurse(depth + 1);
    g_sink += (unsigned long long)(unsigned char)frame[0];
}

static void overflow_entry(mco_coro* co) {
    overflow_recurse(1);
    /* досюда не дойдём — но если дойдём, попробуем yield на повреждённом
     * header'е (демонстрация §5.1: стек затёр _mco_context до guard'а) */
    mco_yield(co);
}

static int run_overflow_child(void) {
    printf("  [T4 overflow child] корутина рекурсит до упора — стек растёт\n");
    printf("    вниз В minicoro-header (§5.1 layout-defect). Наблюдаем:\n");
    printf("    чистый guard-AV (exit 0xC0000005) ИЛИ hang (watchdog 0xDEAD).\n");
    mco_desc d = make_desc(overflow_entry);
    mco_coro* co = NULL;
    mco_result r = mco_create(&co, &d);
    if (r != MCO_SUCCESS || !co) {
        printf("  mco_create failed: %d\n", (int)r);
        return 2;
    }
    printf("  резюмим overflow-корутину...\n");
    mco_resume(co);
    /* если вернулись — overflow не убил процесс; это уже само по себе
     * аномалия (повреждённый header → undefined) */
    printf("  mco_resume ВЕРНУЛСЯ (depth=%d) — overflow не дал чистого AV;\n"
           "  header повреждён, состояние неопределено\n", g_overflow_depth);
    return 3;
}

/* ─── self-spawn для child-прогона T4 ─── */
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

    if (!arena_init()) return 2;

    /* child-режим: только T4 overflow */
    if (argc > 1 && strcmp(argv[1], "overflow") == 0) {
        printf("=== T4 overflow child ===\n");
        int r = run_overflow_child();
        InterlockedExchange(&g_done, 1);
        return r;
    }

    printf("=== Plan 82 Ф.0 test (d) — аномалия Попыток 3-4 (44.3) ===\n");
    printf("Дизайн: arena 256K слотов / commit-on-first / bitmap reuse /\n");
    printf("        no-decommit — точная реплика 44.3 Попытки 4.\n\n");

    int rc = 0;

    printf("--- T1: main_yield-аналог (round-robin) ---\n");
    if (run_round_robin("T1 один fiber, 1 yield", 1, 1) != 0) rc = 1;
    if (run_round_robin("T1 три fiber'а, 2 yield", 3, 2) != 0) rc = 1;
    printf("\n");

    printf("--- T2: slot reuse (детект утечки слотов — гипотеза 44.3 Поп.3) ---\n");
    if (run_slot_reuse(20000) != 0) rc = 1;
    printf("\n");

    printf("--- T3: concurrency + churn ---\n");
    if (run_churn(10, 32, 3) != 0) rc = 1;
    printf("\n");

    printf("--- T4: §5.1 layout-defect — deep overflow (child process) ---\n");
    int t4 = run_self("overflow");
    int t4_av    = ((DWORD)t4 == 0xC0000005);
    int t4_so    = ((DWORD)t4 == 0xC00000FD);   /* STATUS_STACK_OVERFLOW */
    int t4_hang  = ((DWORD)t4 == 0xDEAD);
    printf("  T4 child exit = 0x%08lX  %s\n", (DWORD)t4,
           t4_so   ? "(STATUS_STACK_OVERFLOW — чистая детекция)" :
           t4_av   ? "(ACCESS_VIOLATION — guard-AV, чистый краш)" :
           t4_hang ? "(HANG — watchdog убил! overflow → зависание)" :
                     "(иное)");
    printf("\n");

    InterlockedExchange(&g_done, 1);

    printf("=== ВЕРДИКТ test (d) ===\n");
    printf("  T1 main_yield round-robin : %s\n", rc == 0 ? "OK" : "см.выше");
    printf("  T2 slot reuse 20000×      : см.выше\n");
    printf("  T3 concurrency + churn    : см.выше\n");
    printf("  T4 overflow (§5.1 defect) : %s\n",
           t4_hang ? "HANG — overflow зависает!" :
           (t4_av || t4_so) ? "крах (не hang)" : "иное");
    printf("\n");
    if (rc == 0 && !t4_hang) {
        printf("  => Аномалия Попыток 3-4 НЕ воспроизведена на standalone-\n");
        printf("     харнессе. minicoro-корутины на arena 256K слотов\n");
        printf("     (commit-on-first / reuse / no-decommit) round-robin'ятся\n");
        printf("     БЕЗ hang'а, БЕЗ утечки слотов. TIMEOUT 44.3 — баг\n");
        printf("     откаченного arena-кода или GC/libuv-интеграции, НЕ\n");
        printf("     фундаментальный блокер minicoro/Windows.\n");
        return rc;
    }
    if (t4_hang) {
        printf("  => T4: overflow В minicoro-header (§5.1) приводит к HANG —\n");
        printf("     кандидат-объяснение TIMEOUT 44.3, если тест уходил в\n");
        printf("     overflow. §5.1-фикс (PAGE_GUARD над header'ом) обязателен.\n");
    }
    return rc ? rc : 0;
}
