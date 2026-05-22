# Plan 82 F.0 -- build standalone re-diagnosis harnesses.
# Run:  powershell -ExecutionPolicy Bypass -File build.ps1
#
# ASCII-only by design: PowerShell 5.1 reads .ps1 in the system ANSI
# codepage when there is no BOM -- non-ASCII text breaks the parser.
#
# Builds:
#   f0_probe.c   -- Windows memory-behavior probes (MSVC + clang-cl)
#   f0_gc_link.c -- Boehm GC API link test against gc.lib (clang-cl)
#   f0_test_a.c  -- decision point path A vs B (MSVC + clang-cl)
#   f0_test_d.c  -- attempts 3-4 anomaly re-diagnosis (MSVC + clang-cl)
#   f0_test_e.c  -- SEH across fiber-stack boundary (MSVC + clang-cl)
#
# clang-cl MUST carry /EHa: without it __try/__except does not catch
# hardware SEH (see f0-rediagnosis.md section 3).

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$vc  = 'C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
$cl  = 'C:\Program Files\LLVM\bin\clang-cl.exe'
$gc  = 'd:\Sources\nv-lang\nova\compiler-codegen\vcpkg_installed\x64-windows-static'

Write-Host '=== f0_probe.c -- MSVC cl.exe ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && cl /nologo /O1 /W3 f0_probe.c /Fe:f0_probe_msvc.exe /Fo:f0_probe_msvc.obj"

Write-Host '=== f0_probe.c -- clang-cl (/EHa) ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && `"$cl`" /O1 /EHa /W3 f0_probe.c /Fe:f0_probe_clangcl.exe /Fo:f0_probe_clangcl.obj"

Write-Host '=== f0_gc_link.c -- clang-cl + gc.lib ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && `"$cl`" /O1 /EHa /W3 /I`"$gc\include`" f0_gc_link.c /Fe:f0_gc_link.exe /Fo:f0_gc_link.obj /link /libpath:`"$gc\lib`" gc.lib"

Write-Host '=== f0_test_a.c -- MSVC ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && cl /nologo /O1 /W3 f0_test_a.c /Fe:f0_test_a_msvc.exe /Fo:f0_test_a_msvc.obj"

Write-Host '=== f0_test_a.c -- clang-cl ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && `"$cl`" /O1 /EHa /W3 f0_test_a.c /Fe:f0_test_a_clangcl.exe /Fo:f0_test_a_clangcl.obj"

Write-Host '=== f0_test_d.c -- MSVC ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && cl /nologo /O1 /W3 f0_test_d.c /Fe:f0_test_d_msvc.exe /Fo:f0_test_d_msvc.obj"

Write-Host '=== f0_test_d.c -- clang-cl ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && `"$cl`" /O1 /EHa /W3 f0_test_d.c /Fe:f0_test_d_clangcl.exe /Fo:f0_test_d_clangcl.obj"

Write-Host '=== f0_test_e.c -- MSVC ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && cl /nologo /O1 /W3 f0_test_e.c /Fe:f0_test_e_msvc.exe /Fo:f0_test_e_msvc.obj"

Write-Host '=== f0_test_e.c -- clang-cl ===' -ForegroundColor Cyan
cmd /c "call `"$vc`" >nul 2>&1 && `"$cl`" /O1 /EHa /W3 f0_test_e.c /Fe:f0_test_e_clangcl.exe /Fo:f0_test_e_clangcl.obj"

Remove-Item *.obj -ErrorAction SilentlyContinue
Write-Host '=== built. probe/gc_link/test_a/test_d/test_e ===' -ForegroundColor Green
