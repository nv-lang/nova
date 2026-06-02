/* segv_diag.c — Plan 83.11 §12.31: in-process SEGV crash localizer for Windows.
 *
 * Replaces the §12.9 attempt that used llvm-symbolizer (which requires DIA to
 * read MSVC PDB and our llvm build lacks it). Uses dbghelp's SymFromAddr +
 * StackWalk64 instead — works with any PDB next to the exe.
 *
 * Gated by NOVA_DIAG_SEGV env var; no overhead when unset.
 * Activate: NOVA_DIAG_SEGV=1 ./_min.exe
 *
 * Output: registers + faulting addr + 30-frame stack on EXCEPTION_ACCESS_VIOLATION,
 * then ExitProcess(3) to prevent recursion. Linux path is no-op (Linux bug
 * confirmed absent per §12.21).
 *
 * Linked via dbghelp.lib (already in test_runner.rs windows_libs). */

#include "alloc.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <dbghelp.h>

static volatile LONG _nova_segv_in_handler = 0;
static volatile LONG _nova_segv_installed  = 0;

static LONG WINAPI _nova_segv_veh(EXCEPTION_POINTERS* ep) {
    if (ep->ExceptionRecord->ExceptionCode != EXCEPTION_ACCESS_VIOLATION) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    /* Prevent recursion if our handler itself triggers AV. */
    if (InterlockedExchange(&_nova_segv_in_handler, 1) != 0) {
        return EXCEPTION_CONTINUE_SEARCH;
    }

    HANDLE proc   = GetCurrentProcess();
    HANDLE thread = GetCurrentThread();

    SymSetOptions(SYMOPT_LOAD_LINES | SYMOPT_DEFERRED_LOADS | SYMOPT_UNDNAME);
    if (!SymInitialize(proc, NULL, TRUE)) {
        fprintf(stderr, "[segv-diag] SymInitialize failed: %lu\n",
                (unsigned long)GetLastError());
    }

    CONTEXT ctx = *ep->ContextRecord;

    fprintf(stderr, "\n=== [SEGV-DIAG] EXCEPTION_ACCESS_VIOLATION ===\n");
    fprintf(stderr, "PID=%lu TID=%lu\n",
            (unsigned long)GetCurrentProcessId(),
            (unsigned long)GetCurrentThreadId());
    fprintf(stderr, "ExceptionCode:    0x%08lX\n",
            (unsigned long)ep->ExceptionRecord->ExceptionCode);
    fprintf(stderr, "ExceptionAddress: %p (RIP at fault)\n",
            ep->ExceptionRecord->ExceptionAddress);
    if (ep->ExceptionRecord->NumberParameters >= 2) {
        ULONG_PTR op = ep->ExceptionRecord->ExceptionInformation[0];
        const char* op_str = (op == 0) ? "READ"
                           : (op == 1) ? "WRITE"
                           : (op == 8) ? "DEP/EXEC"
                           : "?";
        ULONG_PTR addr = ep->ExceptionRecord->ExceptionInformation[1];
        fprintf(stderr, "AccessMode:       %s (info[0]=%llu)\n",
                op_str, (unsigned long long)op);
        fprintf(stderr, "FaultAddress:     0x%016llX (target of the bad op)\n",
                (unsigned long long)addr);
    }

    /* Find exe module base for RVA computation. */
    HMODULE exe = GetModuleHandleW(NULL);
    fprintf(stderr, "ExeBase:          %p\n", (void*)exe);
    fprintf(stderr, "RIP-RVA:          0x%llX (= RIP - ExeBase, if RIP in exe)\n",
            (unsigned long long)((uintptr_t)ep->ExceptionRecord->ExceptionAddress
                                 - (uintptr_t)exe));

    fprintf(stderr, "RIP=%016llX RSP=%016llX RBP=%016llX\n",
            (unsigned long long)ctx.Rip, (unsigned long long)ctx.Rsp,
            (unsigned long long)ctx.Rbp);
    fprintf(stderr, "RAX=%016llX RBX=%016llX RCX=%016llX RDX=%016llX\n",
            (unsigned long long)ctx.Rax, (unsigned long long)ctx.Rbx,
            (unsigned long long)ctx.Rcx, (unsigned long long)ctx.Rdx);
    fprintf(stderr, "RSI=%016llX RDI=%016llX R8 =%016llX R9 =%016llX\n",
            (unsigned long long)ctx.Rsi, (unsigned long long)ctx.Rdi,
            (unsigned long long)ctx.R8,  (unsigned long long)ctx.R9);
    fprintf(stderr, "R10=%016llX R11=%016llX R12=%016llX R13=%016llX\n",
            (unsigned long long)ctx.R10, (unsigned long long)ctx.R11,
            (unsigned long long)ctx.R12, (unsigned long long)ctx.R13);
    fprintf(stderr, "R14=%016llX R15=%016llX\n",
            (unsigned long long)ctx.R14, (unsigned long long)ctx.R15);

    /* StackWalk64 setup. */
    STACKFRAME64 sf;
    memset(&sf, 0, sizeof(sf));
    sf.AddrPC.Offset    = ctx.Rip;
    sf.AddrPC.Mode      = AddrModeFlat;
    sf.AddrFrame.Offset = ctx.Rbp;
    sf.AddrFrame.Mode   = AddrModeFlat;
    sf.AddrStack.Offset = ctx.Rsp;
    sf.AddrStack.Mode   = AddrModeFlat;

    fprintf(stderr, "\n=== Stack trace (frame[1] = caller of crash site = KEYSTONE) ===\n");
    enum { kSymBufBytes = sizeof(SYMBOL_INFO) + 1024 };
    char symbuf[kSymBufBytes];
    SYMBOL_INFO* sym = (SYMBOL_INFO*)symbuf;
    sym->SizeOfStruct = sizeof(SYMBOL_INFO);
    sym->MaxNameLen   = kSymBufBytes - sizeof(SYMBOL_INFO) - 1;

    IMAGEHLP_MODULE64 mod_info;
    memset(&mod_info, 0, sizeof(mod_info));
    mod_info.SizeOfStruct = sizeof(mod_info);

    for (int frame = 0; frame < 50; frame++) {
        BOOL ok = StackWalk64(IMAGE_FILE_MACHINE_AMD64, proc, thread,
                              &sf, &ctx, NULL,
                              SymFunctionTableAccess64, SymGetModuleBase64, NULL);
        if (!ok || sf.AddrPC.Offset == 0) break;

        DWORD64 displacement = 0;
        const char* name   = "?";
        const char* module = "?";
        if (SymFromAddr(proc, sf.AddrPC.Offset, &displacement, sym)) {
            name = sym->Name;
        }
        if (SymGetModuleInfo64(proc, sf.AddrPC.Offset, &mod_info)) {
            module = mod_info.ModuleName;
        }

        IMAGEHLP_LINE64 line;
        memset(&line, 0, sizeof(line));
        line.SizeOfStruct = sizeof(line);
        DWORD line_disp = 0;
        if (SymGetLineFromAddr64(proc, sf.AddrPC.Offset, &line_disp, &line)) {
            fprintf(stderr, "  #%02d %016llX  %s!%s+0x%llX  (%s:%lu)\n",
                    frame,
                    (unsigned long long)sf.AddrPC.Offset,
                    module, name,
                    (unsigned long long)displacement,
                    line.FileName ? line.FileName : "?",
                    (unsigned long)line.LineNumber);
        } else {
            fprintf(stderr, "  #%02d %016llX  %s!%s+0x%llX\n",
                    frame,
                    (unsigned long long)sf.AddrPC.Offset,
                    module, name,
                    (unsigned long long)displacement);
        }
    }

    fprintf(stderr, "=== [SEGV-DIAG END] ===\n");
    fflush(stderr);

    SymCleanup(proc);
    /* Exit code 3 = SEGV by convention; distinct from normal failures. */
    ExitProcess(3);
    /* Unreachable. */
    return EXCEPTION_EXECUTE_HANDLER;
}

void _nova_install_segv_handler(void) {
    if (InterlockedCompareExchange(&_nova_segv_installed, 1, 0) != 0) {
        return;
    }
    const char* env = getenv("NOVA_DIAG_SEGV");
    if (!env || env[0] == '\0' || env[0] == '0') {
        return;
    }
    /* First arg = 1 → "call me FIRST", before any C++ SEH or CRT handlers. */
    AddVectoredExceptionHandler(1, _nova_segv_veh);
    fprintf(stderr, "[segv-diag] handler installed (NOVA_DIAG_SEGV=%s)\n", env);
    fflush(stderr);
}

#else  /* !_WIN32 */

void _nova_install_segv_handler(void) {
    /* Linux path: §12.21 confirms bug is Windows-only; no handler needed. */
}

#endif
