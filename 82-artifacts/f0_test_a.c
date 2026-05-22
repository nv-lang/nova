/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * f0_test_a.c — Plan 82 Ф.0 test (a): DECISION POINT путь A vs путь B.
 *
 * Вопрос: после TIB-свопа minicoro растит ли ядро Windows non-primary-
 * стек корутины (VirtualAlloc-регион с PAGE_GUARD-вершиной)?
 *   путь A = да (OS-native grow, VEH не нужен для happy-path);
 *   путь B = нет (нужен VEH lazy-commit).
 *
 * Детекция: VEH-счётчик. Stack-guard-fault, обработанный ядром
 * IN-KERNEL, в user-mode не доставляется (путь A → VEH молчит). Если
 * ядро не распознаёт стек — доставляет GUARD_PAGE_VIOLATION (путь B →
 * VEH ловит).
 *
 * НАХОДКА предыдущей итерации (зафиксирована): VEH исполняется НА стеке
 * корутины, ниже точки фолта. Если под guard'ом сразу reserved-память —
 * exception-dispatch + VEH + VirtualAlloc сами фолтят (вложенный AV →
 * краш). Поэтому путь B требует committed-MARGIN под guard'ом —
 * рабочая зона обработчика. Раскладка слота (low→high):
 *   [reserved] [committed MARGIN] [guard PAGE] [committed WINDOW] top
 * VEH при guard-фолте коммитит свежий [MARGIN+guard] ниже и сдвигает
 * guard вниз — margin едет вместе с guard'ом.
 *
 * Варианты:
 *   a0 — baseline: стек полностью закоммичен (изоляция minicoro);
 *   a1 — lazy-commit, minicoro default (ctx.stack_limit = stack_base);
 *   a2 — lazy-commit, ctx.stack_limit пропатчен на committed_low.
 */
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <intrin.h>   /* __readgsqword — чтение TEB-полей напрямую */

#define MINICORO_IMPL
#include "../compiler-codegen/nova_rt/minicoro.h"

#ifndef STATUS_GUARD_PAGE_VIOLATION
#  define STATUS_GUARD_PAGE_VIOLATION ((DWORD)0x80000001L)
#endif

#define PAGE            ((size_t)4096)
#define STACK_SIZE      ((size_t)(1024 * 1024))   /* 1 MB на корутину        */
#define WINDOW          ((size_t)(64 * 1024))     /* committed окно у вершины */
#define MARGIN          ((size_t)(64 * 1024))     /* committed зона под guard */
#define FRAME_BYTES     ((size_t)(2 * 1024))      /* кадр < PAGE → без __chkstk */
#define RECURSE_DEPTH   120                       /* 120*2K=240K >> окна+margin */

static size_t align_up(size_t v, size_t a)   { return (v + a - 1) & ~(a - 1); }
static size_t align_down(size_t v, size_t a) { return v & ~(a - 1); }

/* ─── состояние арены текущей корутины (для VEH) ─── */
static volatile char* g_arena_lo;
static volatile char* g_arena_hi;
static volatile char* g_commit_floor;     /* ниже не растим (header)        */
static volatile char* g_committed_low;    /* текущая нижняя committed-граница */
static volatile LONG  g_veh_guard;
static volatile LONG  g_veh_av;
static volatile LONG  g_veh_first_code;
static volatile char* g_veh_first_addr;

/* VEH: путь B. MARGIN-aware — коммитит [MARGIN+guard] ниже текущей
 * committed-границы и сдвигает guard вниз. */
static volatile int g_diag;   /* 1 → подробная печать из VEH/коро */

static LONG CALLBACK veh(EXCEPTION_POINTERS* ep) {
    DWORD code = ep->ExceptionRecord->ExceptionCode;
    void* fa = ep->ExceptionRecord->NumberParameters >= 2
             ? (void*)ep->ExceptionRecord->ExceptionInformation[1] : 0;
    if (g_diag) {
        printf("  [VEH] code=0x%08lX faultaddr=%p exc-addr=%p\n",
               code, fa, (void*)ep->ExceptionRecord->ExceptionAddress);
    }
    if (code != STATUS_GUARD_PAGE_VIOLATION
        && code != (DWORD)EXCEPTION_ACCESS_VIOLATION) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    if (ep->ExceptionRecord->NumberParameters < 2) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    ULONG_PTR addr = ep->ExceptionRecord->ExceptionInformation[1];
    if (addr < (ULONG_PTR)g_arena_lo || addr >= (ULONG_PTR)g_arena_hi) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    if (g_veh_first_code == 0) {
        g_veh_first_code = (LONG)code;
        g_veh_first_addr = (char*)addr;
    }
    if (code == STATUS_GUARD_PAGE_VIOLATION) {
        InterlockedIncrement(&g_veh_guard);
    } else {
        InterlockedIncrement(&g_veh_av);
    }
    /* Расширить committed-регион вниз на [MARGIN + guard PAGE]. */
    char* cur_low = (char*)g_committed_low;
    char* new_guard = cur_low - PAGE;
    char* new_low   = new_guard - MARGIN;
    if (new_low < (char*)g_commit_floor) {
        return EXCEPTION_CONTINUE_SEARCH;   /* реальный overflow */
    }
    if (!VirtualAlloc(new_low, (size_t)(cur_low - new_low),
                      MEM_COMMIT, PAGE_READWRITE)) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    DWORD oldp = 0;
    VirtualProtect(new_guard, PAGE, PAGE_READWRITE | PAGE_GUARD, &oldp);
    g_committed_low = (volatile char*)new_low;
    return EXCEPTION_CONTINUE_EXECUTION;
}

/* ─── custom minicoro allocator: VirtualAlloc, полностью закоммичено ─── */
typedef struct { void* base; size_t size; } ArenaRec;
static void* my_alloc(size_t size, void* udata) {
    ArenaRec* r = (ArenaRec*)udata;
    void* p = VirtualAlloc(NULL, size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    r->base = p; r->size = size;
    return p;
}
static void my_free(void* ptr, size_t size, void* udata) {
    (void)size; (void)udata;
    if (ptr) VirtualFree(ptr, 0, MEM_RELEASE);
}

/* ─── рекурсивная нагрузка ─── */
static volatile int g_max_depth;
static volatile unsigned long long g_sink;
static volatile LONG g_coro_fault;

static volatile int g_big_frame;     /* 1 → кадр >1 страницы (__chkstk) */
static volatile int g_target_depth;  /* целевая глубина рекурсии        */
#define BIG_DEPTH 30                  /* 30*8K=240K — кадр >1 страницы   */

/* sub-page кадр (2K): функция НЕ эмитит __chkstk; стек растёт обычными
 * записями buf[] — page-by-page. */
static void recurse(int depth) {
    volatile char buf[FRAME_BYTES];
    buf[0] = (char)depth;
    buf[FRAME_BYTES - 1] = (char)(depth * 3);
    g_max_depth = depth;
    if (depth < g_target_depth) recurse(depth + 1);
    g_sink += (unsigned long long)(unsigned char)buf[0]
            + (unsigned long long)(unsigned char)buf[FRAME_BYTES - 1];
}

/* кадр >1 страницы (8K): clang-cl/MSVC эмитят __chkstk в прологе,
 * который пробингует стек вниз. Найдено (Ф.0 test a): __chkstk-пробинг
 * по lazy-commit minicoro-стеку крашит процесс ЕСЛИ ctx.stack_limit
 * оставлен minicoro-дефолтом; с патчем StackLimit — растёт штатно. */
static void recurse_big(int depth) {
    volatile char buf[8 * 1024];
    buf[0] = (char)depth;
    buf[8 * 1024 - 1] = (char)(depth * 3);
    g_max_depth = depth;
    if (depth < g_target_depth) recurse_big(depth + 1);
    g_sink += (unsigned long long)(unsigned char)buf[0]
            + (unsigned long long)(unsigned char)buf[8 * 1024 - 1];
}

static void coro_entry(mco_coro* co) {
    (void)co;
    if (g_diag) printf("  [coro] entry — TEB.StackBase=%p StackLimit=%p "
                       "DeallocStack=%p\n",
                       (void*)__readgsqword(0x08),
                       (void*)__readgsqword(0x10),
                       (void*)__readgsqword(0x1478));
    __try {
        if (g_big_frame) recurse_big(1);
        else             recurse(1);
    } __except (g_coro_fault = (LONG)GetExceptionCode(),
                EXCEPTION_EXECUTE_HANDLER) {
    }
    if (g_diag) printf("  [coro] recursion returned, depth=%d\n", g_max_depth);
}

static size_t committed_bytes(char* lo, char* hi) {
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

/* mode: 0=a0 baseline, 1=a1 lazy default, 2=a2 lazy patched. */
static int run_variant(const char* name, int mode) {
    printf("--- %s ---\n", name);
    ArenaRec rec = {0};
    mco_desc desc = mco_desc_init(coro_entry, STACK_SIZE);
    desc.alloc_cb = my_alloc;
    desc.dealloc_cb = my_free;
    desc.allocator_data = &rec;

    mco_coro* co = NULL;
    mco_result mr = mco_create(&co, &desc);
    if (mr != MCO_SUCCESS || !co) {
        printf("  mco_create failed: %d\n", (int)mr);
        return -2;
    }

    char* stack_lo = (char*)co->stack_base;
    char* stack_hi = stack_lo + co->stack_size;
    char* decommit_start = (char*)align_up((size_t)stack_lo, PAGE);
    char* window_lo  = (char*)align_down((size_t)stack_hi - WINDOW, PAGE);
    char* guard_page = window_lo - PAGE;
    char* margin_lo  = guard_page - MARGIN;

    if (mode != 0) {
        if (margin_lo <= decommit_start) {
            printf("  layout too small\n"); mco_destroy(co); return -2;
        }
        if (!VirtualFree(decommit_start, (size_t)(margin_lo - decommit_start),
                         MEM_DECOMMIT)) {
            printf("  VirtualFree(DECOMMIT) failed: %lu\n", GetLastError());
            mco_destroy(co); return -2;
        }
        DWORD oldp = 0;
        if (!VirtualProtect(guard_page, PAGE, PAGE_READWRITE | PAGE_GUARD,
                            &oldp)) {
            printf("  VirtualProtect(GUARD) failed: %lu\n", GetLastError());
            mco_destroy(co); return -2;
        }
        if (mode == 2) {
            _mco_context* mctx = (_mco_context*)co->context;
            mctx->ctx.stack_limit = window_lo;  /* committed_low как у CreateFiber */
        }
    }

    g_diag = (mode == 1);   /* подробная диагностика только для a1 */
    g_arena_lo = (volatile char*)stack_lo;
    g_arena_hi = (volatile char*)stack_hi;
    g_commit_floor = (volatile char*)decommit_start;
    g_committed_low = (volatile char*)(mode ? margin_lo : decommit_start);
    g_veh_guard = 0; g_veh_av = 0; g_veh_first_code = 0; g_veh_first_addr = 0;
    g_max_depth = 0; g_coro_fault = 0;
    g_target_depth = g_big_frame ? BIG_DEPTH : RECURSE_DEPTH;

    size_t before = committed_bytes(decommit_start, stack_hi);
    if (mode) {
        printf("  stack [%p,%p) 1024K; window 64K guard@%p margin 64K; "
               "committed=%zuK\n",
               (void*)stack_lo, (void*)stack_hi, (void*)guard_page,
               before / 1024);
    } else {
        printf("  stack [%p,%p) 1024K fully committed; committed=%zuK\n",
               (void*)stack_lo, (void*)stack_hi, before / 1024);
    }
    printf("  resuming coro...\n");

    mco_resume(co);

    size_t after = committed_bytes(decommit_start, stack_hi);
    printf("  depth %d/%d; committed after %zuK (Δ+%zuK); "
           "VEH guard=%ld av=%ld first=0x%08lX; coro-SEH=0x%08lX\n",
           g_max_depth, g_target_depth, after / 1024, (after - before) / 1024,
           g_veh_guard, g_veh_av, (DWORD)g_veh_first_code,
           (DWORD)g_coro_fault);

    int verdict;
    if (g_coro_fault != 0) {
        printf("  RESULT: FAULT 0x%08lX поймано SEH в корутине — "
               "рост не покрыт\n", (DWORD)g_coro_fault);
        verdict = -1;
    } else if (g_max_depth < g_target_depth) {
        printf("  RESULT: рекурсия не дошла до конца\n");
        verdict = -1;
    } else if (mode == 0) {
        printf("  RESULT: baseline OK\n");
        verdict = 0;
    } else if (g_veh_guard == 0 && g_veh_av == 0) {
        printf("  RESULT: >>> ПУТЬ A <<< — ядро вырастило стек штатно "
               "(VEH молчал)\n");
        verdict = 0;
    } else {
        printf("  RESULT: >>> ПУТЬ B <<< — VEH lazy-commit обязателен "
               "(ядро не растит)\n");
        verdict = 1;
    }
    mco_destroy(co);
    printf("\n");
    return verdict;
}

/* Запустить себя же с аргументом, дождаться, вернуть exit-код.
 * __chkstk-вариант может крашнуть child — это не убьёт родителя. */
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
    CloseHandle(pi.hProcess);
    CloseHandle(pi.hThread);
    return (int)ec;
}

int main(int argc, char** argv) {
    setvbuf(stdout, NULL, _IONBF, 0);
    printf("=== Plan 82 Ф.0 test (a) — decision point: путь A vs путь B ===\n\n");

    PVOID h = AddVectoredExceptionHandler(1, veh);
    if (!h) { printf("AddVectoredExceptionHandler failed\n"); return 2; }

    /* `f0_test_a.exe bigframe` — прогон ТОЛЬКО a1 с кадром >1 страницы
     * (__chkstk). Может крашнуть процесс — это и есть наблюдаемая
     * находка. Запускать отдельным процессом (см. f0-rediagnosis.md). */
    if (argc > 1 && strcmp(argv[1], "bigframe") == 0) {
        g_big_frame = 1;
        /* `bigframe` → a1; `bigframe a2` → a2 (patched StackLimit). */
        int vmode = (argc > 2 && strcmp(argv[2], "a2") == 0) ? 2 : 1;
        printf("[bigframe mode] кадр рекурсии 8K → __chkstk; вариант a%d\n\n",
               vmode);
        int r = run_variant(vmode == 2
                                ? "a2-bigframe / patched StackLimit, __chkstk"
                                : "a1-bigframe / minicoro default, __chkstk",
                            vmode);
        RemoveVectoredExceptionHandler(h);
        printf("=== bigframe a%d RESULT: %s ===\n", vmode,
               r == 0 ? "ПУТЬ A" : r == 1 ? "ПУТЬ B" : "FAULT/CRASH");
        return r == 0 ? 0 : 1;
    }

    int a0 = run_variant("a0 / baseline (полностью закоммичен)", 0);
    int a1 = run_variant("a1 / sub-page кадр, minicoro default StackLimit", 1);
    int a2 = run_variant("a2 / sub-page кадр, patched StackLimit=window_lo", 2);

    RemoveVectoredExceptionHandler(h);

    /* __chkstk-варианты гоняем отдельными процессами — bigframe a1 может
     * крашнуть процесс (MSVC __chkstk), child-crash не убьёт родителя. */
    printf("=== spawn __chkstk-под-прогонов (кадр >1 страницы) ===\n");
    int bf_a1 = run_self("bigframe");
    int bf_a2 = run_self("bigframe a2");
    int bf_a1_crash = (DWORD)bf_a1 == (DWORD)0xC0000005;
    printf("  bigframe a1 child exit=0x%08lX %s\n", (DWORD)bf_a1,
           bf_a1_crash ? "(CRASH 0xC0000005)" : "");
    printf("  bigframe a2 child exit=0x%08lX %s\n\n", (DWORD)bf_a2,
           bf_a2 == 0 ? "(OK)" : "");

    printf("=== ВЕРДИКТ test (a) ===\n");
    printf("  a0 baseline                       : %s\n",
           a0 == 0 ? "OK" : "СЛОМАН");
    printf("  a1 sub-page,  default StackLimit   : %s\n",
           a1 == 0 ? "ПУТЬ A" : "не A");
    printf("  a2 sub-page,  patched StackLimit   : %s\n",
           a2 == 0 ? "ПУТЬ A" : "не A");
    printf("  a1 __chkstk,  default StackLimit   : %s\n",
           bf_a1 == 0 ? "ПУТЬ A" : bf_a1_crash ? "CRASH" : "не A");
    printf("  a2 __chkstk,  patched StackLimit   : %s\n",
           bf_a2 == 0 ? "ПУТЬ A" : "не A");

    if (a0 != 0) { printf("\n  => baseline сломан.\n"); return 3; }
    if (a1 == 0 && a2 == 0 && bf_a2 == 0) {
        if (bf_a1 == 0) {
            printf("\n  => ПУТЬ A. Ядро растит non-primary minicoro-стек. "
                   "Патч ctx.stack_limit рекомендуется (без него MSVC-"
                   "__chkstk на грани).\n");
        } else {
            printf("\n  => ПУТЬ A — ОБЯЗАТЕЛЕН патч ctx.stack_limit "
                   "(committed_low). Без него __chkstk-код (кадр >1 "
                   "страницы) крашит на MSVC; с патчем — растёт штатно "
                   "на обеих toolchain.\n");
        }
        return 0;
    }
    printf("\n  => ПУТЬ B (VEH lazy-commit) — ядро не растит надёжно.\n");
    return 0;
}
