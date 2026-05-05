@echo off
rem run_deep_tests.bat — build and run GC + fiber deep tests
rem Usage: run_deep_tests.bat
rem Requires MSVC (vcvarsall must be in PATH or call this from Developer Command Prompt)

setlocal
set "VCVARS=D:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvarsall.bat"
set "RT=%~dp0"
set "FAILED=0"

call "%VCVARS%" x64 > nul 2>&1

echo.
echo ============================================================
echo  nova_rt deep tests
echo ============================================================

rem ---- GC tests: malloc backend ----
echo.
echo [1/4] GC deep test (malloc backend)
cl.exe /nologo /W3 /O1 /I"%RT%\.." "%RT%test_gc_deep.c" "%RT%alloc.c" /Fe:"%RT%test_gc_malloc.exe" > nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo   BUILD FAILED
    set FAILED=1
) else (
    "%RT%test_gc_malloc.exe"
    if %ERRORLEVEL% NEQ 0 set FAILED=1
)

rem ---- GC tests: RC backend ----
echo.
echo [2/4] GC deep test (RC backend)
cl.exe /nologo /W3 /O1 /I"%RT%\.." "%RT%test_gc_deep.c" "%RT%alloc_rc.c" /Fe:"%RT%test_gc_rc.exe" > nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo   BUILD FAILED
    set FAILED=1
) else (
    "%RT%test_gc_rc.exe"
    if %ERRORLEVEL% NEQ 0 set FAILED=1
)

rem ---- Fiber tests: malloc backend ----
echo.
echo [3/4] Fiber deep test (malloc backend)
cl.exe /nologo /W3 /O1 /I"%RT%\.." "%RT%test_fibers_deep.c" "%RT%alloc.c" "%RT%fibers.c" /Fe:"%RT%test_fibers.exe" > nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo   BUILD FAILED
    set FAILED=1
) else (
    "%RT%test_fibers.exe"
    if %ERRORLEVEL% NEQ 0 set FAILED=1
)

rem ---- Fiber tests: RC backend ----
echo.
echo [4/4] Fiber deep test (RC backend)
cl.exe /nologo /W3 /O1 /I"%RT%\.." "%RT%test_fibers_deep.c" "%RT%alloc_rc.c" "%RT%fibers.c" /Fe:"%RT%test_fibers_rc.exe" > nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo   BUILD FAILED
    set FAILED=1
) else (
    "%RT%test_fibers_rc.exe"
    if %ERRORLEVEL% NEQ 0 set FAILED=1
)

echo.
echo ============================================================
if %FAILED% EQU 0 (
    echo  ALL DEEP TESTS PASSED
) else (
    echo  SOME TESTS FAILED
)
echo ============================================================

endlocal
exit /b %FAILED%
