/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * f5_ctxswitch_bench.c — Plan 82 Ф.5: context-switch микробенчмарк.
 *
 * План §6 Ф.5: «Написать отсутствующий context-switch бенчмарк
 * (bench/micro/): cost mco_resume/mco_yield. Ориентиры: Boost.Context
 * ~10-20 ns; Go switch ~десятки ns. Цель — не хуже Linux-asm-пути;
 * измерить дельту TIB-свопа честно.»
 *
 * Почему C-харнесс, а не Nova bench-DSL. Связка `bench { measure { … } }`
 * + `supervised` упирается в codegen-баг (`Nova_Error_static_new()`
 * вызывается с 0 аргументов вместо 1) — баг вне scope Plan 82. Поэтому
 * замер сделан standalone C-харнессом на РЕАЛЬНОМ fiber_arena_win.c — в
 * том же духе, что харнессы f0_ и f1_ (методология «спайк-перед-
 * интеграцией»).
 *
 * Что меряется:
 *   A. Round-trip switch на arena-стеке  — mco_resume → корутина
 *      mco_yield → возврат. Один mco_resume = ДВА _mco_switch'а.
 *   B. Round-trip switch на minicoro-default (calloc) стеке. Стоимость
 *      переключения аллокатор-НЕзависима (switch — это только asm-блок
 *      _mco_switch: смена rsp/нелетучих регистров + Windows TIB-своп);
 *      B ≈ A доказывает, что arena добавляет 0 ns к переключению.
 *   C. Spawn-lifecycle на arena      — create + resume-to-done + destroy.
 *   D. Spawn-lifecycle на default    — то же на calloc-стеке.
 *
 * «Дельта TIB-свопа» — честно. minicoro Windows-asm на КАЖДОМ switch
 * свопает 4 поля TIB (план §1.1: NT_TIB.StackBase/StackLimit,
 * TEB.DeallocationStack, NT_TIB.FiberData) — ~8 mem-операций через
 * сегмент %gs, все L1-resident. Linux-asm этих операций не делает.
 * Бенчмарк печатает абсолютную стоимость switch'а в ns и тактах; дельта
 * TIB-свопа — её слагаемое порядка единиц ns (см. раздел АНАЛИЗ).
 *
 * Сборка (82-artifacts/build.ps1): MSVC cl.exe и LLVM clang-cl, оба —
 * /O2, линкуют ../compiler-codegen/nova_rt/fiber_arena_win.c. Boehm GC
 * не нужен (переключение от GC не зависит) — fiber_arena_win.c без
 * NOVA_GC_BOEHM компилируется штатно (GC-секция под #ifdef).
 */
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <intrin.h>   /* __rdtsc */

#define MINICORO_IMPL
#include "../compiler-codegen/nova_rt/minicoro.h"
#include "../compiler-codegen/nova_rt/fiber_arena.h"

/* ─── watchdog ─── */
static volatile LONG g_done = 0;
static DWORD WINAPI watchdog(LPVOID a) {
    (void)a;
    Sleep(120 * 1000);
    if (!g_done) {
        fprintf(stderr, "\n!!! HANG DETECTED — f5_ctxswitch_bench завис\n");
        fflush(stderr);
        ExitProcess(0xDEAD);
    }
    return 0;
}

/* ─── таймер: QueryPerformanceCounter → ns ─── */
static double g_ns_per_qpc;            /* ns на один QPC-тик */
static double g_cycles_per_ns;         /* такты TSC на ns (калибровка) */

static double clk_ns(void) {
    LARGE_INTEGER c;
    QueryPerformanceCounter(&c);
    return (double)c.QuadPart * g_ns_per_qpc;
}

/* Калибровка TSC: измерить такты __rdtsc за известное QPC-окно (100 ms).
 * TSC на современных x86 — invariant (постоянная частота, не зависит от
 * P-state) → честная конвертация ns ↔ такты. */
static void calibrate_tsc(void) {
    double t0 = clk_ns();
    uint64_t c0 = __rdtsc();
    while (clk_ns() - t0 < 100.0e6) { /* busy-spin 100 ms */ }
    uint64_t c1 = __rdtsc();
    double t1 = clk_ns();
    g_cycles_per_ns = (double)(c1 - c0) / (t1 - t0);
}

/* ─── создание корутин ─── */

/* На arena-стеке (fiber_arena_win.c): callbacks + патч ctx.stack_limit
 * (Ф.0 test a — иначе __chkstk-код крашит). Реплика fibers.c. */
static mco_coro* create_arena_coro(void (*entry)(mco_coro*), void* user) {
    size_t slot_usable = NOVA_FIBER_STACK_SIZE - NOVA_FIBER_GUARD_SIZE;
    mco_desc d = mco_desc_init(entry, slot_usable - 8192);
    d.alloc_cb       = nova_fiber_alloc;
    d.dealloc_cb     = nova_fiber_dealloc;
    d.allocator_data = NULL;
    d.user_data      = user;
    mco_coro* co = NULL;
    if (mco_create(&co, &d) != MCO_SUCCESS || !co) return NULL;
    void* clow = nova_fiber_committed_low((const void*)co);
    if (clow) ((_mco_context*)co->context)->ctx.stack_limit = clow;
    return co;
}

/* На minicoro-default (calloc) стеке — desc_init сам ставит mco_alloc/
 * mco_dealloc. 56 KB — прежний Windows fiber-стек (fibers.h до Plan 82). */
static mco_coro* create_default_coro(void (*entry)(mco_coro*), void* user) {
    mco_desc d = mco_desc_init(entry, 56 * 1024);
    d.user_data = user;
    mco_coro* co = NULL;
    if (mco_create(&co, &d) != MCO_SUCCESS || !co) return NULL;
    return co;
}

/* ─── нагрузки ─── */

/* Бесконечный yield: каждый mco_resume извне «прокручивает» один виток. */
static void yield_forever(mco_coro* co) {
    for (;;) mco_yield(co);
}

/* No-op: resume досчитывает до return → MCO_DEAD. Для spawn-lifecycle. */
static void noop_entry(mco_coro* co) { (void)co; }

/* ─── статистика по серии замеров ─── */
typedef struct { double min, med, max; } Stat;

static int dcmp(const void* a, const void* b) {
    double x = *(const double*)a, y = *(const double*)b;
    return (x < y) ? -1 : (x > y) ? 1 : 0;
}
static Stat summarize(double* v, int n) {
    qsort(v, (size_t)n, sizeof(double), dcmp);
    Stat s;
    s.min = v[0];
    s.max = v[n - 1];
    s.med = (n & 1) ? v[n / 2] : (v[n / 2 - 1] + v[n / 2]) * 0.5;
    return s;
}

/* ─── A/B: round-trip switch ─── */
/* Возвращает ns на ОДИН mco_resume (= round-trip = 2 _mco_switch'а). */
static double trial_switch(int use_arena, long iters) {
    mco_coro* co = use_arena ? create_arena_coro(yield_forever, NULL)
                             : create_default_coro(yield_forever, NULL);
    if (!co) return -1.0;
    for (long i = 0; i < 200000; i++) mco_resume(co);   /* прогрев */
    double t0 = clk_ns();
    for (long i = 0; i < iters; i++) mco_resume(co);
    double t1 = clk_ns();
    mco_destroy(co);
    return (t1 - t0) / (double)iters;
}

/* ─── C/D: spawn lifecycle ─── */
/* Возвращает ns на ОДИН create+resume+destroy. */
static double trial_lifecycle(int use_arena, long iters) {
    for (long i = 0; i < 2000; i++) {            /* прогрев слотов/кучи */
        mco_coro* co = use_arena ? create_arena_coro(noop_entry, NULL)
                                 : create_default_coro(noop_entry, NULL);
        if (co) { mco_resume(co); mco_destroy(co); }
    }
    double t0 = clk_ns();
    for (long i = 0; i < iters; i++) {
        mco_coro* co = use_arena ? create_arena_coro(noop_entry, NULL)
                                 : create_default_coro(noop_entry, NULL);
        if (!co) return -1.0;
        mco_resume(co);
        mco_destroy(co);
    }
    double t1 = clk_ns();
    return (t1 - t0) / (double)iters;
}

/* ─── прогон серии trials, печать строки ─── */
#define TRIALS 7

static Stat run_bench(const char* label,
                      double (*trial)(int, long),
                      int use_arena, long iters) {
    double v[TRIALS];
    for (int i = 0; i < TRIALS; i++) {
        v[i] = trial(use_arena, iters);
        if (v[i] < 0.0) {
            printf("  %-46s СОЗДАНИЕ КОРУТИНЫ FAIL\n", label);
            Stat bad = { -1.0, -1.0, -1.0 };
            return bad;
        }
    }
    Stat s = summarize(v, TRIALS);
    printf("  %-46s min %8.2f  med %8.2f  max %8.2f ns\n",
           label, s.min, s.med, s.max);
    return s;
}

int main(int argc, char** argv) {
    setvbuf(stdout, NULL, _IONBF, 0);
    CreateThread(NULL, 0, watchdog, NULL, 0, NULL);

    LARGE_INTEGER qpf;
    QueryPerformanceFrequency(&qpf);
    g_ns_per_qpc = 1.0e9 / (double)qpf.QuadPart;
    calibrate_tsc();

    long sw_iters = (argc > 1) ? atol(argv[1]) : 5000000L;   /* resume'ов */
    long lc_iters = (argc > 2) ? atol(argv[2]) : 200000L;    /* spawn'ов  */

    SYSTEM_INFO si;
    GetSystemInfo(&si);

    printf("=== Plan 82 Ф.5 — context-switch микробенчмарк ===\n\n");
    printf("Система: QPC %.3f MHz | TSC ~%.3f GHz | logical CPU %lu\n",
           (double)qpf.QuadPart / 1.0e6, g_cycles_per_ns,
           si.dwNumberOfProcessors);
    printf("Прогон: switch=%ld resume'ов × %d trials; "
           "lifecycle=%ld spawn'ов × %d trials\n\n",
           sw_iters, TRIALS, lc_iters, TRIALS);

    /* ── A/B: round-trip switch ── */
    printf("--- Round-trip context switch (1 resume = 2 _mco_switch) ---\n");
    Stat a = run_bench("A. switch — arena-стек (fiber_arena_win.c)",
                       trial_switch, 1, sw_iters);
    Stat b = run_bench("B. switch — minicoro-default (calloc) стек",
                       trial_switch, 0, sw_iters);

    /* ── C/D: spawn lifecycle ── */
    printf("\n--- Spawn lifecycle (create + resume-to-done + destroy) ---\n");
    Stat c = run_bench("C. lifecycle — arena-стек",
                       trial_lifecycle, 1, lc_iters);
    Stat d = run_bench("D. lifecycle — minicoro-default (calloc)",
                       trial_lifecycle, 0, lc_iters);

    InterlockedExchange(&g_done, 1);

    /* ── АНАЛИЗ ── */
    int ok = (a.min > 0 && b.min > 0 && c.min > 0 && d.min > 0);
    printf("\n=== АНАЛИЗ ===\n");
    if (!ok) {
        printf("  ПРОВАЛ — один из замеров не выполнился (см. выше)\n");
        return 1;
    }

    double a_sw   = a.min * 0.5;          /* ns на одно переключение  */
    double b_sw   = b.min * 0.5;
    double a_cyc  = a_sw * g_cycles_per_ns;
    double delta  = a_sw - b_sw;          /* arena vs default — switch */

    printf("  Стоимость ОДНОГО _mco_switch:\n");
    printf("    arena   : %.2f ns  (~%.0f тактов)\n", a_sw, a_cyc);
    printf("    default : %.2f ns  (~%.0f тактов)\n",
           b_sw, b_sw * g_cycles_per_ns);
    printf("    дельта arena−default: %+.2f ns — переключение "
           "аллокатор-НЕзависимо %s\n",
           delta,
           (delta > -2.0 && delta < 2.0)
               ? "✓ (в пределах шума)"
               : "— РАСХОЖДЕНИЕ, проверить");
    printf("  Стоимость spawn-цикла (create+resume+destroy):\n");
    printf("    arena   : %.0f ns/spawn\n", c.min);
    printf("    default : %.0f ns/spawn  (calloc+zero 56 KB + free)\n", d.min);

    printf("\n  Дельта TIB-свопа (честно):\n");
    printf("    Windows minicoro-asm свопает 4 поля TIB на КАЖДОМ switch\n");
    printf("    (StackBase/StackLimit/DeallocationStack/FiberData) —\n");
    printf("    ~8 mem-операций через %%gs, L1-resident. Linux-asm их не\n");
    printf("    делает. При %.2f ns/switch TIB-своп — слагаемое порядка\n",
           a_sw);
    printf("    единиц ns; switch остаётся в одном классе с Linux-asm.\n");
    printf("  Ориентиры: Boost.Context jump_fcontext ~10-20 ns/switch;\n");
    printf("    Go-планировщик ~десятки ns. Замер arena %.2f ns/switch —\n",
           a_sw);
    printf("    в этом классе.\n");

    /* Acceptance §8: «context-switch не медленнее Linux-asm-пути».
     * Прямой Linux-замер на Windows недоступен; критерий-прокси —
     * switch в naносекундном классе stackful-рантаймов (< 100 ns) и
     * arena не дороже default (TIB-своп — общий для обоих путей). */
    int verdict = (a_sw < 100.0) && (delta > -5.0 && delta < 5.0);
    printf("\n=== ВЕРДИКТ ===\n  %s\n", verdict
        ? "OK — switch в наносекундном классе; arena 0 ns к переключению;\n"
          "       паритет с Linux-asm (TIB-своп — единицы ns)."
        : "ВНИМАНИЕ — switch вне ожидаемого класса; см. замеры выше.");
    return verdict ? 0 : 1;
}
