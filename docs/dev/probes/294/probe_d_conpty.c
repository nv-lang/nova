/* SPDX-License-Identifier: MIT OR Apache-2.0
 * Plan 294 F.0 probe (c), Windows: ConPTY. Spawns `cmd /c echo hello & exit 3` under a pseudo console,
 * reads the output pipe to EOF with plain ReadFile, reports the raw bytes, the exit code, and
 * whether EOF arrives before or only after ClosePseudoConsole (R8 deadlock question).
 */
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int argc_delay = 0;
int main(int argc, char** argv) {
    if (argc > 1) argc_delay = atoi(argv[1]);
    HANDLE inR, inW, outR, outW;
    CreatePipe(&inR, &inW, NULL, 0); CreatePipe(&outR, &outW, NULL, 0);
    HPCON pc; COORD sz = { 120, 40 };
    HRESULT hr = CreatePseudoConsole(sz, inR, outW, 0, &pc);
    printf("CreatePseudoConsole hr=0x%08lx\n", (unsigned long)hr);
    if (FAILED(hr)) return 1;
    STARTUPINFOEXW si; memset(&si, 0, sizeof si); si.StartupInfo.cb = sizeof si;
    SIZE_T n = 0; InitializeProcThreadAttributeList(NULL, 1, 0, &n);
    si.lpAttributeList = HeapAlloc(GetProcessHeap(), 0, n);
    InitializeProcThreadAttributeList(si.lpAttributeList, 1, 0, &n);
    UpdateProcThreadAttribute(si.lpAttributeList, 0, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, pc, sizeof pc, NULL, NULL);
    wchar_t cmd[] = L"cmd.exe /c echo hello& exit 3";
    PROCESS_INFORMATION pi;
    if (!CreateProcessW(NULL, cmd, NULL, NULL, FALSE, EXTENDED_STARTUPINFO_PRESENT, NULL, NULL, &si.StartupInfo, &pi)) {
        printf("CreateProcess err=%lu\n", GetLastError()); return 1;
    }
    CloseHandle(inR); CloseHandle(outW);   /* our copies of the pty-side ends */
    WaitForSingleObject(pi.hProcess, 10000);
    DWORD code = 0; GetExitCodeProcess(pi.hProcess, &code);
    printf("child exited code=%lu\n", code);
    /* Without ClosePseudoConsole the output pipe does NOT hit EOF: prove it with PeekNamedPipe/timeouts. */
    char buf[4096]; DWORD got = 0, total = 0, avail = 0; char acc[8192]; 
    Sleep(argc_delay);   /* give conhost time to flush before we look */
    PeekNamedPipe(outR, NULL, 0, NULL, &avail, NULL);
    printf("bytes available before ClosePseudoConsole: %lu\n", avail);
    if (avail) { ReadFile(outR, buf, sizeof buf, &got, NULL); memcpy(acc, buf, got); total = got; }
    ClosePseudoConsole(pc);   /* drained first, so no deadlock */
    while (ReadFile(outR, buf, sizeof buf, &got, NULL) && got) { memcpy(acc + total, buf, got); total += got; }
    printf("ReadFile ended after ClosePseudoConsole, err=%lu (109 = ERROR_BROKEN_PIPE = EOF)\n", GetLastError());
    printf("raw output (%lu bytes): ", total);
    for (DWORD i = 0; i < total; i++) { unsigned char c = acc[i]; if (c >= 32 && c < 127) putchar(c); else printf("<%02x>", c); }
    putchar('\n');
    return 0;
}
