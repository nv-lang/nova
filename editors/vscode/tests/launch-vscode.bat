@echo off
REM Wrapper that clears ELECTRON_RUN_AS_NODE before launching VSCode.
REM Needed because Claude Code (and other Electron-based shells) set
REM ELECTRON_RUN_AS_NODE=1, which makes Code.exe start as Node.js CLI
REM instead of the GUI app, breaking @vscode/test-electron.
REM
REM Usage: launch-vscode.bat path\to\Code.exe [args...]
REM The first argument is the real Code.exe path; remaining args are passed through.

set ELECTRON_RUN_AS_NODE=
set CODE_EXE=%1
shift
"%CODE_EXE%" %1 %2 %3 %4 %5 %6 %7 %8 %9
