@echo off
rem build_c.bat <source.c> [output_exe] [gc=malloc|rc|boehm]
rem Compiles a generated Nova C file using MSVC cl.exe.
rem
rem   gc=malloc  (default) — plain malloc, no free
rem   gc=rc               — reference counting via nova_retain/nova_release
rem   gc=boehm            — Boehm tracing GC (collects cycles, requires bdwgc)

setlocal

set "INPUT=%~1"
set "OUTPUT=%~2"
set "GC=%~3"

if "%OUTPUT%"=="" set "OUTPUT=%~dpn1.exe"
if "%GC%"=="" set "GC=gc=malloc"

set "VCVARS=D:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvarsall.bat"
set "NOVA_RT_DIR=%~dp0nova_rt"
set "VCPKG_INC=%~dp0vcpkg_installed\x64-windows-static\include"
set "VCPKG_LIB=%~dp0vcpkg_installed\x64-windows-static\lib"

call "%VCVARS%" x64 > nul 2>&1

if "%GC%"=="gc=rc" (
    set "ALLOC_SRC=%NOVA_RT_DIR%\alloc_rc.c"
    set "EXTRA_LIBS="
) else if "%GC%"=="gc=boehm" (
    set "ALLOC_SRC=%NOVA_RT_DIR%\alloc_boehm.c"
    set "EXTRA_LIBS=%VCPKG_LIB%\gc.lib"
) else (
    set "ALLOC_SRC=%NOVA_RT_DIR%\alloc.c"
    set "EXTRA_LIBS="
)

set "FIBERS_SRC=%NOVA_RT_DIR%\fibers.c"
set "EFFECTS_SRC=%NOVA_RT_DIR%\effects.c"

rem Use a per-build temp dir; rt files get prefixed names to avoid
rem .obj collision when generated file has the same basename as a rt source.
set "OBJDIR=%TEMP%\nova_build_%RANDOM%"
mkdir "%OBJDIR%" > nul 2>&1

cl.exe /nologo /W3 /O1 /I"%~dp0" /Fo"%OBJDIR%\gen.obj" /c "%INPUT%"
if %ERRORLEVEL% NEQ 0 goto fail

cl.exe /nologo /W3 /O1 /I"%~dp0" /Fo"%OBJDIR%\rt_alloc.obj" /c "%ALLOC_SRC%"
if %ERRORLEVEL% NEQ 0 goto fail

cl.exe /nologo /W3 /O1 /I"%~dp0" /Fo"%OBJDIR%\rt_fibers.obj" /c "%FIBERS_SRC%"
if %ERRORLEVEL% NEQ 0 goto fail

cl.exe /nologo /W3 /O1 /I"%~dp0" /Fo"%OBJDIR%\rt_effects.obj" /c "%EFFECTS_SRC%"
if %ERRORLEVEL% NEQ 0 goto fail

if "%GC%"=="gc=boehm" (
    link.exe /nologo /SUBSYSTEM:CONSOLE /OUT:"%OUTPUT%" ^
        "%OBJDIR%\gen.obj" "%OBJDIR%\rt_alloc.obj" "%OBJDIR%\rt_fibers.obj" "%OBJDIR%\rt_effects.obj" ^
        "%VCPKG_LIB%\gc.lib" "%VCPKG_LIB%\atomic_ops.lib"
) else (
    link.exe /nologo /SUBSYSTEM:CONSOLE /OUT:"%OUTPUT%" ^
        "%OBJDIR%\gen.obj" "%OBJDIR%\rt_alloc.obj" "%OBJDIR%\rt_fibers.obj" "%OBJDIR%\rt_effects.obj"
)
if %ERRORLEVEL% NEQ 0 goto fail

rmdir /s /q "%OBJDIR%" > nul 2>&1
echo [OK] %OUTPUT% (%GC%)
endlocal
exit /b 0

:fail
rmdir /s /q "%OBJDIR%" > nul 2>&1
echo [FAIL] Compilation failed.
endlocal
exit /b 1
