/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * f0_probe.c — Plan 82 Ф.0 re-diagnosis: standalone Windows memory-behavior
 * probes. Self-contained (kernel32 only), вне рантайма Nova — в отличие от
 * 4 интеграционных провалов Plan 44.3.
 *
 * Подтверждает верифицированные посылки Plan 82:
 *   Probe 1 (§1.3): чтение MEM_RESERVE-но-не-MEM_COMMIT страницы → AV.
 *     Это ровно почему плоский GC_add_roots поверх lazy-commit-арены
 *     непереносим на Windows (byte-wise conservative scan Boehm'а упал бы).
 *   Probe 2: чтение committed PAGE_NOACCESS hard-guard → AV.
 *   Probe 3: первое касание PAGE_GUARD → GUARD_PAGE_VIOLATION, one-shot
 *     (механизм, на котором стоит lazy-commit путь A, §5.1).
 *   Probe 4: precise-push корректен — чтение ТОЛЬКО закоммиченного окна
 *     reserved-региона fault-free (модель §5.2: push [committed_low, top]).
 *
 * НЕ покрыто здесь (требует minicoro-интеграции — следующий шаг Ф.0):
 *   test (a) — растит ли ядро non-primary minicoro-стек (решение A vs B);
 *   test (d) — аномалия Попыток 3-4; test (e) — SEH через границу fiber-стека.
 */
#include <windows.h>
#include <stdio.h>

/* STATUS_GUARD_PAGE_VIOLATION — winnt.h определяет как EXCEPTION_GUARD_PAGE;
 * фиксируем литералом на случай отсутствия макро в конкретном SDK. */
#ifndef STATUS_GUARD_PAGE_VIOLATION
#  define STATUS_GUARD_PAGE_VIOLATION ((DWORD)0x80000001L)
#endif

static int g_pass = 0, g_fail = 0;
#define REPORT(ok, name) do { \
    if (ok) { g_pass++; printf("[PASS] %s\n", name); } \
    else    { g_fail++; printf("[FAIL] %s\n", name); } \
} while (0)

/* Probe 1 (§1.3) — чтение MEM_RESERVE-страницы фолтит с ACCESS_VIOLATION. */
static int probe_reserved_read_faults(void) {
    SIZE_T sz = 64 * 1024;
    volatile char* p =
        (volatile char*)VirtualAlloc(NULL, sz, MEM_RESERVE, PAGE_NOACCESS);
    if (!p) {
        printf("  VirtualAlloc(MEM_RESERVE) failed: %lu\n", GetLastError());
        return 0;
    }
    DWORD code = 0;
    __try {
        char x = p[0];
        (void)x;
    } __except (code = GetExceptionCode(),
                code == (DWORD)EXCEPTION_ACCESS_VIOLATION
                    ? EXCEPTION_EXECUTE_HANDLER : EXCEPTION_CONTINUE_SEARCH) {
    }
    VirtualFree((LPVOID)p, 0, MEM_RELEASE);
    printf("  read of MEM_RESERVE page -> 0x%08lX %s\n", code,
           code == (DWORD)EXCEPTION_ACCESS_VIOLATION ? "(ACCESS_VIOLATION)" : "");
    return code == (DWORD)EXCEPTION_ACCESS_VIOLATION;
}

/* Probe 2 — чтение committed PAGE_NOACCESS-страницы фолтит с AV. Это
 * поведение hard-guard'а (§5.1: нижние 16 KB слота — PAGE_NOACCESS навсегда). */
static int probe_noaccess_read_faults(void) {
    SIZE_T sz = 64 * 1024;
    volatile char* p = (volatile char*)VirtualAlloc(
        NULL, sz, MEM_RESERVE | MEM_COMMIT, PAGE_NOACCESS);
    if (!p) {
        printf("  VirtualAlloc(COMMIT,NOACCESS) failed: %lu\n", GetLastError());
        return 0;
    }
    DWORD code = 0;
    __try {
        char x = p[0];
        (void)x;
    } __except (code = GetExceptionCode(),
                code == (DWORD)EXCEPTION_ACCESS_VIOLATION
                    ? EXCEPTION_EXECUTE_HANDLER : EXCEPTION_CONTINUE_SEARCH) {
    }
    VirtualFree((LPVOID)p, 0, MEM_RELEASE);
    printf("  read of committed PAGE_NOACCESS page -> 0x%08lX %s\n", code,
           code == (DWORD)EXCEPTION_ACCESS_VIOLATION ? "(ACCESS_VIOLATION)" : "");
    return code == (DWORD)EXCEPTION_ACCESS_VIOLATION;
}

/* Probe 3 — первое касание PAGE_GUARD-страницы даёт GUARD_PAGE_VIOLATION;
 * guard снимается ядром, второе касание проходит. Это primitive, на котором
 * стоит OS-native grow (путь A, §5.1). */
static int probe_guard_page_one_shot(void) {
    SIZE_T pg = 4096;
    char* p = (char*)VirtualAlloc(NULL, pg, MEM_RESERVE | MEM_COMMIT,
                                  PAGE_READWRITE | PAGE_GUARD);
    if (!p) {
        printf("  VirtualAlloc(COMMIT,GUARD) failed: %lu\n", GetLastError());
        return 0;
    }
    DWORD code1 = 0;
    __try {
        p[0] = (char)1;
    } __except (code1 = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
    }
    /* После первого касания guard снят — второй доступ обязан пройти. */
    int second_ok = 0;
    __try {
        p[0] = (char)2;
        second_ok = (p[0] == (char)2);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        second_ok = 0;
    }
    VirtualFree(p, 0, MEM_RELEASE);
    printf("  1st touch of PAGE_GUARD -> 0x%08lX %s; 2nd touch ok=%d\n",
           code1,
           code1 == STATUS_GUARD_PAGE_VIOLATION ? "(GUARD_PAGE_VIOLATION)" : "",
           second_ok);
    return code1 == STATUS_GUARD_PAGE_VIOLATION && second_ok;
}

/* Probe 4 — модель §5.2: precise push сканирует только закоммиченное окно
 * reserved-региона. Резервируем 256 KB, коммитим верхние 16 KB, читаем их
 * по-байтно (эмулируя conservative scan диапазона [committed_low, top]) —
 * fault-free. Это доказывает: push [committed_low, top] обходит §1.3-AV. */
static int probe_committed_window_scan_ok(void) {
    SIZE_T total = 256 * 1024;
    SIZE_T window = 16 * 1024;
    char* base = (char*)VirtualAlloc(NULL, total, MEM_RESERVE, PAGE_NOACCESS);
    if (!base) {
        printf("  VirtualAlloc(MEM_RESERVE 256K) failed: %lu\n", GetLastError());
        return 0;
    }
    char* win_lo = base + total - window;   /* верхнее окно (стек растёт вниз) */
    if (!VirtualAlloc(win_lo, window, MEM_COMMIT, PAGE_READWRITE)) {
        printf("  VirtualAlloc(MEM_COMMIT window) failed: %lu\n", GetLastError());
        VirtualFree(base, 0, MEM_RELEASE);
        return 0;
    }
    volatile unsigned long long acc = 0;
    int faulted = 0;
    __try {
        /* by-byte conservative scan закоммиченного [committed_low, top) */
        for (SIZE_T i = 0; i < window; i++) {
            acc += (unsigned char)win_lo[i];
        }
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        faulted = 1;
    }
    VirtualFree(base, 0, MEM_RELEASE);
    printf("  by-byte scan of committed [committed_low,top) %u bytes -> %s "
           "(acc=%llu)\n",
           (unsigned)window, faulted ? "FAULTED" : "fault-free", acc);
    return !faulted;
}

int main(void) {
    /* Небуферизованный stdout — чтобы при возможном краше probe'а вывод
     * до точки падения не терялся. */
    setvbuf(stdout, NULL, _IONBF, 0);
    printf("=== Plan 82 Ф.0 — standalone Windows memory-behavior probes ===\n\n");

    printf("Probe 1 (§1.3): read of MEM_RESERVE page must fault (AV)\n");
    REPORT(probe_reserved_read_faults(), "reserved-read-faults");
    printf("\n");

    printf("Probe 2: read of committed PAGE_NOACCESS hard-guard must fault (AV)\n");
    REPORT(probe_noaccess_read_faults(), "noaccess-read-faults");
    printf("\n");

    printf("Probe 3: PAGE_GUARD one-shot — 1st touch faults, guard cleared\n");
    REPORT(probe_guard_page_one_shot(), "guard-page-one-shot");
    printf("\n");

    printf("Probe 4 (§5.2): by-byte scan of committed window is fault-free\n");
    REPORT(probe_committed_window_scan_ok(), "committed-window-scan-ok");
    printf("\n");

    printf("=== SUMMARY: %d pass / %d fail ===\n", g_pass, g_fail);
    return g_fail == 0 ? 0 : 1;
}
