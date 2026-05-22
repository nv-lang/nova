# Plan 82 Ф.0 — сборка standalone re-diagnosis харнесса.
# Запуск:  powershell -ExecutionPolicy Bypass -File build.ps1
#
# Собирает f0_probe.c обеими toolchain'ами (MSVC + clang-cl) и
# f0_gc_link.c (линковочный тест Boehm GC API против gc.lib).

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$vc  = 'C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
$cl  = 'C:\Program Files\LLVM\bin\clang-cl.exe'
$gc  = 'd:\Sources\nv-lang\nova\compiler-codegen\vcpkg_installed\x64-windows-static'

Write-Host '=== f0_probe.c — MSVC cl.exe ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && cl /nologo /O1 /W3 f0_probe.c /Fe:f0_probe_msvc.exe /Fo:f0_probe_msvc.obj"

Write-Host '=== f0_probe.c — clang-cl (/EHa обязателен для hardware-SEH) ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && `"$cl`" /O1 /EHa /W3 f0_probe.c /Fe:f0_probe_clangcl.exe /Fo:f0_probe_clangcl.obj"

Write-Host '=== f0_gc_link.c — clang-cl + gc.lib (§1.6 линковка) ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && `"$cl`" /O1 /EHa /W3 /I`"$gc\include`" f0_gc_link.c /Fe:f0_gc_link.exe /Fo:f0_gc_link.obj /link /libpath:`"$gc\lib`" gc.lib"

Write-Host '=== f0_test_a.c — MSVC (test a: decision point путь A/B) ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && cl /nologo /O1 /W3 f0_test_a.c /Fe:f0_test_a_msvc.exe /Fo:f0_test_a_msvc.obj"

Write-Host '=== f0_test_a.c — clang-cl ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && `"$cl`" /O1 /EHa /W3 f0_test_a.c /Fe:f0_test_a_clangcl.exe /Fo:f0_test_a_clangcl.obj"

Remove-Item *.obj -ErrorAction SilentlyContinue
Write-Host '=== built. Запуск test (a): f0_test_a_msvc.exe  (default —' -ForegroundColor Green
Write-Host '    самооркеструемый: спавнит __chkstk-под-прогоны). ===' -ForegroundColor Green
