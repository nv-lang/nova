/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * f0_test_e.c — Plan 82 Ф.0 test (e): SEH через границу fiber-стека.
 *
 * Вопросы (§7 тест-матрица плана 82):
 *   - SEH __try/__except ВНУТРИ fiber — unwind корректен, не уходит за
 *     границы fiber-стека;
 *   - SEH unwind ЧЕРЕЗ границу fiber → caller — RtlVirtualUnwind
 *     останавливается по NT_TIB.StackBase/StackLimit;
 *   - exception, не пойманное на fiber-стеке, ведёт себя ДЕТЕРМИНИРОВАННО
 *     (чистый краш / не hang / не AV в самом unwinder'е).
 *
 * Частичные данные f0-rediagnosis §6: __try/__except внутри корутины
 * работает; __try вокруг mco_resume фолт на коро-стеке НЕ ловит. Этот
 * харнесс дозакрывает: (1) подтверждает SEH-within-fiber; (2) проверяет
 * TIB-своп; (3) cross-boundary unhandled exception — детерминизм; (4)
 * ручной RtlVirtualUnwind-walk коро-стека — bounded по StackBase/Limit.
 *
 * Корутинный стек — полностью закоммиченный VirtualAlloc (SEH-поведение
 * не зависит от lazy-commit; это test (a)/(d) территория).
 *
 * Сборка: clang-cl ОБЯЗАТЕЛЬНО с /EHa (иначе __try не ловит hardware-
 * SEH — находка f0-rediagnosis §3). MSVC cl.exe ловит в C-режиме сам.
 */
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <intrin.h>

#define MINICORO_IMPL
#include "../compiler-codegen/nova_rt/minicoro.h"

#define STACK_SIZE   ((size_t)(1024 * 1024))
#define POISON_RA    ((DWORD64)0xdeaddeaddeaddeadULL)  /* _mco_makectx-стоп */
#define E3_EXC_CODE  ((DWORD)0xE0001234)               /* кастомное SW-исключение */

/* ─── полностью закоммиченный VirtualAlloc-стек ─── */
typedef struct { void* base; size_t size; } ArenaRec;
static void* va_alloc(size_t size, void* udata) {
    ArenaRec* r = (ArenaRec*)udata;
    void* p = VirtualAlloc(NULL, size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    if (r) { r->base = p; r->size = size; }
    return p;
}
static void va_free(void* ptr, size_t size, void* udata) {
    (void)size; (void)udata;
    if (ptr) VirtualFree(ptr, 0, MEM_RELEASE);
}

/* ─── watchdog — детект hang ─── */
static volatile LONG g_done = 0;
static DWORD WINAPI watchdog(LPVOID arg) {
    (void)arg;
    Sleep(15 * 1000);
    if (!g_done) {
        fprintf(stderr, "\n!!! HANG DETECTED (SEH-unwind завис)\n");
        fflush(stderr);
        ExitProcess(0xDEAD);
    }
    return 0;
}

/* ═══ E1: SEH __try/__except полностью ВНУТРИ fiber ═══
 * Hardware-AV (запись по NULL) внутри корутины, __except на коро-стеке. */
static volatile LONG g_e1_caught;
static volatile DWORD g_e1_code;

static void e1_entry(mco_coro* co) {
    (void)co;
    __try {
        volatile int* p = (volatile int*)0;
        *p = 42;                       /* hardware AV */
    } __except (g_e1_code = GetExceptionCode(),
                EXCEPTION_EXECUTE_HANDLER) {
        InterlockedExchange(&g_e1_caught, 1);
    }
}

static int run_e1(void) {
    printf("--- E1: SEH __try/__except ВНУТРИ fiber (hardware AV) ---\n");
    g_e1_caught = 0; g_e1_code = 0;
    ArenaRec rec = {0};
    mco_desc d = mco_desc_init(e1_entry, STACK_SIZE);
    d.alloc_cb = va_alloc; d.dealloc_cb = va_free; d.allocator_data = &rec;
    mco_coro* co = NULL;
    if (mco_create(&co, &d) != MCO_SUCCESS || !co) {
        printf("  mco_create failed\n"); return -1;
    }
    mco_resume(co);
    mco_destroy(co);
    int ok = g_e1_caught && g_e1_code == (DWORD)EXCEPTION_ACCESS_VIOLATION;
    printf("  caught=%ld code=0x%08lX → %s\n", g_e1_caught, g_e1_code,
           ok ? "OK — SEH-handler на коро-стеке поймал коро-фолт"
              : "ОШИБКА");
    return ok ? 0 : -1;
}

/* ═══ E2: TIB-своп — fiber видит свои StackBase/StackLimit ═══ */
static volatile uintptr_t g_e2_base, g_e2_limit, g_e2_dealloc;

static void e2_entry(mco_coro* co) {
    (void)co;
    g_e2_base    = (uintptr_t)__readgsqword(0x08);   /* NT_TIB.StackBase */
    g_e2_limit   = (uintptr_t)__readgsqword(0x10);   /* NT_TIB.StackLimit */
    g_e2_dealloc = (uintptr_t)__readgsqword(0x1478); /* TEB.DeallocationStack */
}

static int run_e2(void) {
    printf("--- E2: TIB-своп при switch — fiber видит свои границы ---\n");
    uintptr_t main_base  = (uintptr_t)__readgsqword(0x08);
    uintptr_t main_limit = (uintptr_t)__readgsqword(0x10);
    g_e2_base = g_e2_limit = g_e2_dealloc = 0;
    ArenaRec rec = {0};
    mco_desc d = mco_desc_init(e2_entry, STACK_SIZE);
    d.alloc_cb = va_alloc; d.dealloc_cb = va_free; d.allocator_data = &rec;
    mco_coro* co = NULL;
    if (mco_create(&co, &d) != MCO_SUCCESS || !co) {
        printf("  mco_create failed\n"); return -1;
    }
    uintptr_t stk_lo = (uintptr_t)co->stack_base;
    uintptr_t stk_hi = stk_lo + co->stack_size;
    mco_resume(co);

    /* fiber StackBase/Limit должны попасть в [stack_base, stack_base+size]
     * корутины, и отличаться от main-thread TIB. */
    int swapped   = (g_e2_base != main_base) && (g_e2_limit != main_limit);
    int in_range  = (g_e2_base  > stk_lo && g_e2_base  <= stk_hi) &&
                    (g_e2_limit >= stk_lo && g_e2_limit < stk_hi);
    printf("  main TIB : base=%p limit=%p\n",
           (void*)main_base, (void*)main_limit);
    printf("  fiber TIB: base=%p limit=%p dealloc=%p\n",
           (void*)g_e2_base, (void*)g_e2_limit, (void*)g_e2_dealloc);
    printf("  coro slot: [%p, %p)\n", (void*)stk_lo, (void*)stk_hi);
    printf("  swapped=%d in-range=%d → %s\n", swapped, in_range,
           (swapped && in_range)
             ? "OK — minicoro свопнул TIB на коро-стек (SEH-bounds верны)"
             : "ОШИБКА");
    mco_destroy(co);
    return (swapped && in_range) ? 0 : -1;
}

/* ═══ E4: ручной RtlVirtualUnwind-walk коро-стека ═══
 * Изнутри fiber'а проходим стек RtlLookupFunctionEntry + RtlVirtualUnwind,
 * проверяя, что Rsp НИ РАЗУ не выходит за [StackLimit, StackBase) и что
 * walk терминируется (упирается в poison-RA _mco_makectx'а). Это §7-пункт
 * «RtlVirtualUnwind останавливается по StackBase/StackLimit». */
static volatile int g_e4_frames;
static volatile int g_e4_escaped;     /* Rsp вышел за границы коро-стека */
static volatile int g_e4_terminated;  /* walk дошёл до конца штатно      */

static MCO_NO_INLINE void e4_walk(void) {
    ULONG_PTR tib_base  = (ULONG_PTR)__readgsqword(0x08);
    ULONG_PTR tib_limit = (ULONG_PTR)__readgsqword(0x10);
    CONTEXT ctx;
    RtlCaptureContext(&ctx);
    int frames = 0, escaped = 0, terminated = 0;
    for (;;) {
        if (ctx.Rip == 0 || ctx.Rip == POISON_RA) { terminated = 1; break; }
        /* Rsp обязан лежать внутри коро-стека [limit, base). */
        if (ctx.Rsp < tib_limit || ctx.Rsp >= tib_base) { escaped = 1; break; }
        DWORD64 image_base = 0;
        PRUNTIME_FUNCTION fn = RtlLookupFunctionEntry(ctx.Rip, &image_base, NULL);
        if (!fn) {
            /* leaf-функция (нет unwind-info) — pop return-address вручную */
            if (ctx.Rsp + 8 > tib_base) { terminated = 1; break; }
            ctx.Rip = *(DWORD64*)ctx.Rsp;
            ctx.Rsp += 8;
        } else {
            PVOID handler_data = NULL;
            ULONG_PTR establisher = 0;
            RtlVirtualUnwind(UNW_FLAG_NHANDLER, image_base, ctx.Rip, fn,
                             &ctx, &handler_data, &establisher, NULL);
        }
        if (++frames > 256) { terminated = 1; break; }  /* safety */
    }
    g_e4_frames     = frames;
    g_e4_escaped    = escaped;
    g_e4_terminated = terminated;
}

/* пост-вызовный side-effect защищает кадры от tail-call-оптимизации —
 * иначе /O1 схлопывает цепочку в jmp'ы и walk видит лишь 2 кадра. */
static volatile unsigned long long g_e4_sink;
static MCO_NO_INLINE void e4_depth3(void) { e4_walk();   g_e4_sink += 3; }
static MCO_NO_INLINE void e4_depth2(void) { e4_depth3(); g_e4_sink += 2; }
static MCO_NO_INLINE void e4_depth1(void) { e4_depth2(); g_e4_sink += 1; }

static void e4_entry(mco_coro* co) {
    (void)co;
    e4_depth1();   /* несколько кадров на коро-стеке для walk'а */
}

static int run_e4(void) {
    printf("--- E4: ручной RtlVirtualUnwind-walk коро-стека ---\n");
    g_e4_frames = g_e4_escaped = g_e4_terminated = 0;
    ArenaRec rec = {0};
    mco_desc d = mco_desc_init(e4_entry, STACK_SIZE);
    d.alloc_cb = va_alloc; d.dealloc_cb = va_free; d.allocator_data = &rec;
    mco_coro* co = NULL;
    if (mco_create(&co, &d) != MCO_SUCCESS || !co) {
        printf("  mco_create failed\n"); return -1;
    }
    mco_resume(co);
    mco_destroy(co);
    int ok = !g_e4_escaped && g_e4_terminated && g_e4_frames > 0;
    printf("  frames=%d escaped=%d terminated=%d → %s\n",
           g_e4_frames, g_e4_escaped, g_e4_terminated,
           ok ? "OK — unwind-walk остался в [StackLimit,StackBase), "
                "штатно дошёл до poison-RA"
              : "ОШИБКА — walk вышел за границы коро-стека ИЛИ не "
                "терминировался");
    return ok ? 0 : -1;
}

/* ═══ E3 (child): cross-boundary unhandled exception ═══
 * fiber делает RaiseException; __try/__except — на CALLER-стеке вокруг
 * mco_resume. SEH per-stack: handler на native-стеке НЕ достижим по
 * frame-chain коро-стека → exception НЕ должно пойматься caller'ом.
 * Ожидаемо: child падает с кодом исключения. BAD: hang / AV в unwinder'е
 * / тихое «поймал caller» (значило бы, что SEH перешёл границу). */
static volatile int g_e3_caller_caught;

static void e3_entry(mco_coro* co) {
    (void)co;
    /* исключение НЕ обёрнуто __try на коро-стеке — уходит в поиск
     * handler'а по коро-стеку, которого там нет. */
    RaiseException(E3_EXC_CODE, EXCEPTION_NONCONTINUABLE, 0, NULL);
}

static int run_e3_child(void) {
    printf("=== E3 child: cross-boundary unhandled exception ===\n");
    ArenaRec rec = {0};
    mco_desc d = mco_desc_init(e3_entry, STACK_SIZE);
    d.alloc_cb = va_alloc; d.dealloc_cb = va_free; d.allocator_data = &rec;
    mco_coro* co = NULL;
    if (mco_create(&co, &d) != MCO_SUCCESS || !co) {
        printf("  mco_create failed\n"); return 2;
    }
    g_e3_caller_caught = 0;
    printf("  резюмим fiber (внутри RaiseException 0x%08lX)...\n", E3_EXC_CODE);
    __try {
        mco_resume(co);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        g_e3_caller_caught = 1;
    }
    if (g_e3_caller_caught) {
        printf("  __except CALLER'а СРАБОТАЛ — SEH ПЕРЕШЁЛ границу fiber!\n");
        printf("  (неожиданно: означало бы, что unwinder проходит switch)\n");
        return 7;   /* спец-код «caller поймал» */
    }
    printf("  mco_resume вернулся без исключения (?!)\n");
    return 8;
}

/* ─── self-spawn ─── */
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

    if (argc > 1 && strcmp(argv[1], "e3") == 0) {
        int r = run_e3_child();
        InterlockedExchange(&g_done, 1);
        return r;
    }

    printf("=== Plan 82 Ф.0 test (e) — SEH через границу fiber-стека ===\n\n");
    int rc = 0;
    if (run_e1() != 0) rc = 1;
    printf("\n");
    if (run_e2() != 0) rc = 1;
    printf("\n");
    if (run_e4() != 0) rc = 1;
    printf("\n");

    printf("--- E3: cross-boundary unhandled exception (child process) ---\n");
    int e3 = run_self("e3");
    DWORD e3u = (DWORD)e3;
    int e3_clean_crash = (e3u == E3_EXC_CODE);
    int e3_caller      = (e3u == 7);
    int e3_hang        = (e3u == 0xDEAD);
    printf("  E3 child exit = 0x%08lX  %s\n", e3u,
           e3_clean_crash ? "(чистый краш кодом исключения — ОЖИДАЕМО)" :
           e3_caller      ? "(caller поймал — SEH перешёл границу!)" :
           e3_hang        ? "(HANG — unwinder завис!)" :
                            "(иной код)");
    /* Корректный исход: чистый детерминированный краш, caller НЕ поймал,
     * нет hang'а. Это паритет с native Windows fibers (SEH per-stack). */
    int e3_ok = e3_clean_crash && !e3_caller && !e3_hang;
    if (!e3_ok) rc = 1;
    printf("\n");

    InterlockedExchange(&g_done, 1);

    printf("=== ВЕРДИКТ test (e) ===\n");
    printf("  E1 __try/__except в fiber    : см.выше\n");
    printf("  E2 TIB-своп                  : см.выше\n");
    printf("  E4 RtlVirtualUnwind-walk     : см.выше\n");
    printf("  E3 cross-boundary exception  : %s\n",
           e3_ok ? "детерминированный краш, caller НЕ поймал, нет hang"
                 : "АНОМАЛИЯ — см.выше");
    printf("\n");
    if (rc == 0) {
        printf("  => SEH на fiber-стеках Windows ведёт себя корректно и\n");
        printf("     детерминированно: handler внутри fiber'а ловит коро-\n");
        printf("     фолты; TIB-своп даёт верные SEH-границы; unwinder\n");
        printf("     bounded по StackBase/StackLimit; exception, не пойманное\n");
        printf("     на коро-стеке, даёт чистый краш (паритет с native\n");
        printf("     Windows fibers — SEH per-stack, границу не переходит).\n");
    } else {
        printf("  => АНОМАЛИЯ в SEH-поведении — см.выше; разобрать до Ф.1.\n");
    }
    return rc;
}
