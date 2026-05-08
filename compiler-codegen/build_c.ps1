# build_c.ps1 — wrapper for full Nova build pipeline (.nv -> .c -> .exe)
#
# Usage:
#     .\build_c.ps1 hello.nv                 # produces hello.exe in same dir
#     .\build_c.ps1 hello.nv -Run             # build and run
#     .\build_c.ps1 hello.nv -Output out.exe  # custom output path
#     .\build_c.ps1 hello.nv -KeepC           # don't delete intermediate .c
#
# Pipeline:
#     1. nova-codegen.exe compile <file>.nv  -> <file>.c
#     2. cl.exe via vcvars64.bat              -> <file>.exe
#     3. (optional) run <file>.exe
#
# Requirements:
#     - nova-codegen built: cargo build (in compiler-codegen/)
#     - MSVC Build Tools installed (vcvars64.bat)
#     - PowerShell 5+
#
# Cross-platform note:
#     For GCC/Clang on Linux/Mac, use build_c.sh (TODO) or invoke gcc manually:
#         nova-codegen compile hello.nv
#         gcc hello.c nova_rt/alloc.c nova_rt/effects.c nova_rt/fibers.c \
#             -I/path/to/compiler-codegen -o hello && ./hello

param(
    [Parameter(Mandatory=$true, Position=0)]
    [string]$File,

    [string]$Output = "",
    [switch]$Run,
    [switch]$KeepC,
    [string]$VCVarsPath = "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
)

# Не "Stop" — нативные exe пишут в stderr и без error-exit-code,
# что PowerShell в strict mode трактует как ошибку. Проверяем
# $LASTEXITCODE явно после каждого внешнего вызова.
$ErrorActionPreference = "Continue"

# Resolve paths
$compiler_root = $PSScriptRoot
$codegen = Join-Path $compiler_root "target\debug\nova-codegen.exe"
$rt_dir  = Join-Path $compiler_root "nova_rt"

if (-not (Test-Path $codegen)) {
    Write-Host "error: nova-codegen.exe not found at $codegen" -ForegroundColor Red
    Write-Host "       run 'cargo build' in $compiler_root first" -ForegroundColor Yellow
    exit 1
}

if (-not (Test-Path $VCVarsPath)) {
    Write-Host "error: vcvars64.bat not found at $VCVarsPath" -ForegroundColor Red
    Write-Host "       install Visual Studio Build Tools or pass -VCVarsPath <path>" -ForegroundColor Yellow
    exit 1
}

if (-not (Test-Path $File)) {
    Write-Host "error: input file not found: $File" -ForegroundColor Red
    exit 1
}

$nv_file  = (Resolve-Path $File).Path
$nv_name  = [System.IO.Path]::GetFileNameWithoutExtension($nv_file)
$nv_dir   = [System.IO.Path]::GetDirectoryName($nv_file)
$c_file   = Join-Path $nv_dir "$nv_name.c"
$exe_file = if ($Output -ne "") { $Output } else { Join-Path $nv_dir "$nv_name.exe" }
$obj_dir  = Join-Path $env:TEMP "nova_build_$nv_name"

New-Item -ItemType Directory -Force -Path $obj_dir | Out-Null

# Step 1: .nv -> .c
Write-Host "[1/3] codegen: $nv_file -> $c_file" -ForegroundColor Cyan
# nova-codegen пишет "ok: ..." в stderr — глушим, проверяем по exit code.
$cg_out = & $codegen compile $nv_file 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "codegen failed:" -ForegroundColor Red
    $cg_out | Where-Object { $_ -match "error" } | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    exit 1
}
if (-not (Test-Path $c_file)) {
    Write-Host "codegen produced no .c file at $c_file" -ForegroundColor Red
    exit 1
}

# Step 2: .c -> .exe via MSVC
Write-Host "[2/3] cl.exe: $c_file -> $exe_file" -ForegroundColor Cyan
$cl_cmd = "cl.exe /nologo /W0 " +
          "/I `"$compiler_root`" " +
          "/Fo`"$obj_dir\\`" " +
          "/Fe`"$exe_file`" " +
          "`"$c_file`" " +
          "`"$rt_dir\alloc.c`" " +
          "`"$rt_dir\effects.c`" " +
          "`"$rt_dir\fibers.c`""
$cl_out = cmd /c "`"$VCVarsPath`" && $cl_cmd" 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "cl.exe failed:" -ForegroundColor Red
    $cl_out | Where-Object { $_ -match "error" } | Select-Object -First 5 | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    exit 1
}

# Cleanup intermediate .c
if (-not $KeepC) {
    Remove-Item -Force $c_file -ErrorAction SilentlyContinue
}

Write-Host "[3/3] built: $exe_file" -ForegroundColor Green

# Optional: run
if ($Run) {
    Write-Host ""
    Write-Host "running $exe_file ..." -ForegroundColor Cyan
    & $exe_file
    Write-Host ""
    Write-Host "exit code: $LASTEXITCODE" -ForegroundColor Cyan
}
