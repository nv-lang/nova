/* SPDX-License-Identifier: MIT OR Apache-2.0
 * Plan 294 F.0 probe (b), Windows: Job Object on top of libuv.
 *   probe_b            -> parent
 *   probe_b child      -> spawns a grandchild at once (`probe_b leaf`), then exits after printing its pid
 *   probe_b leaf       -> sleeps 20 s
 * Variant 1: uv_spawn(child), then AssignProcessToJobObject(child). Counts runs where the
 *            grandchild was ALREADY outside the job (the race the plan fears).
 * Variant 2: CreateProcess(CREATE_SUSPENDED) + assign + ResumeThread: must be 0 escapes.
 * Variant 3: nested-job question: uv_spawn children already live in libuv's own job
 *            (KILL_ON_JOB_CLOSE); does AssignProcessToJobObject still succeed (Win8+ nesting)?
 */
#include <uv.h>
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void kill_leaf_tree(HANDLE job) { TerminateJobObject(job, 1); }

static int in_job(DWORD pid, HANDLE job) {
    HANDLE h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
    if (!h) return -1;
    BOOL r = FALSE; IsProcessInJob(h, job, &r); CloseHandle(h); return r ? 1 : 0;
}

static HANDLE new_job(void) {
    HANDLE j = CreateJobObjectW(NULL, NULL);
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION li; memset(&li, 0, sizeof li);
    li.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    SetInformationJobObject(j, JobObjectExtendedLimitInformation, &li, sizeof li);
    return j;
}

/* exe path from uv_exepath is UTF-8: spawn through the wide API (a Cyrillic profile dir broke the ANSI call). */
static BOOL spawn_w(const char* exe, const char* arg, DWORD flags, PROCESS_INFORMATION* pi) {
    wchar_t cl[2200], w[1100], a[100];
    MultiByteToWideChar(CP_UTF8, 0, exe, -1, w, 1100);
    MultiByteToWideChar(CP_UTF8, 0, arg, -1, a, 100);
    swprintf(cl, 2200, L"\"%ls\" %ls", w, a);
    STARTUPINFOW si; memset(&si, 0, sizeof si); si.cb = sizeof si;
    return CreateProcessW(NULL, cl, NULL, NULL, FALSE, flags, NULL, NULL, &si, pi);
}

static void exit_cb(uv_process_t* p, int64_t s, int sg) { (void)s; (void)sg; uv_close((uv_handle_t*)p, NULL); }

/* enumerate pids in job -> count */
static int job_count(HANDLE job) {
    char buf[sizeof(JOBOBJECT_BASIC_PROCESS_ID_LIST) + 64 * sizeof(ULONG_PTR)] = {0};
    JOBOBJECT_BASIC_PROCESS_ID_LIST* l = (void*)buf;
    if (!QueryInformationJobObject(job, JobObjectBasicProcessIdList, l, sizeof buf, NULL)) return -1;
    return (int)l->NumberOfAssignedProcesses;
}

int main(int argc, char** argv) {
    char exe[1024]; size_t el = sizeof exe; uv_exepath(exe, &el);
    if (argc > 1 && !strcmp(argv[1], "leaf")) { Sleep(20000); return 0; }
    if (argc > 1 && !strcmp(argv[1], "child")) {
        PROCESS_INFORMATION pi;
        spawn_w(exe, "leaf", 0, &pi);
        Sleep(20000); return 0;
    }
    const int N = 30; int escaped1 = 0, assign_fail1 = 0, escaped2 = 0;
    uv_loop_t* loop = uv_default_loop();
    /* Variant 1 + 3 */
    int delay_ms = argc > 1 ? atoi(argv[1]) : 0; /* simulated descheduling between spawn and assign */
    for (int i = 0; i < N; i++) {
        uv_process_t p; char* a[] = { exe, "child", NULL };
        uv_process_options_t o; memset(&o, 0, sizeof o); o.file = exe; o.args = a; o.exit_cb = exit_cb;
        if (uv_spawn(loop, &p, &o)) { printf("spawn fail\n"); return 1; }
        if (delay_ms) Sleep(delay_ms);
        HANDLE job = new_job();
        HANDLE ph = OpenProcess(PROCESS_ALL_ACCESS, FALSE, (DWORD)p.pid);
        if (!AssignProcessToJobObject(job, ph)) { assign_fail1++; printf("  assign err=%lu\n", GetLastError()); }
        Sleep(300); /* let the grandchild appear */
        int cnt = job_count(job);
        if (i < 3) printf("  v1 run %d: job_count=%d err=%lu\n", i, cnt, GetLastError());
        if (cnt < 2) escaped1++;
        TerminateJobObject(job, 1); CloseHandle(ph); CloseHandle(job);
        uv_process_kill(&p, 9); uv_run(loop, UV_RUN_DEFAULT);
    }
    printf("V1 uv_spawn, delay %d ms, then Assign: runs=%d grandchild_escaped=%d assign_failed(nested job)=%d\n", delay_ms, N, escaped1, assign_fail1);
    /* Variant 2 */
    for (int i = 0; i < N; i++) {
        PROCESS_INFORMATION pi;
        HANDLE job = new_job();
        if (!spawn_w(exe, "child", CREATE_SUSPENDED, &pi)) { printf("cp fail err=%lu\n", GetLastError()); return 1; }
        if (!AssignProcessToJobObject(job, pi.hProcess)) printf("  v2 assign err=%lu\n", GetLastError());
        ResumeThread(pi.hThread);
        Sleep(300);
        if (job_count(job) < 2) escaped2++;
        TerminateJobObject(job, 1);
        WaitForSingleObject(pi.hProcess, 5000);
        CloseHandle(pi.hThread); CloseHandle(pi.hProcess); CloseHandle(job);
    }
    printf("V2 CREATE_SUSPENDED + Assign + Resume: runs=%d grandchild_escaped=%d\n", N, escaped2);
    (void)kill_leaf_tree; (void)in_job;
    return 0;
}
