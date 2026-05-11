# Build libuv.lib for Nova tests (Plan 22 Ф.1).
#
# Output: target/libuv-cache/libuv.lib + target/libuv-cache/version.txt
#
# Idempotent: skips rebuild if version.txt matches current libuv submodule
# commit. Force rebuild via -Force flag.
#
# Usage:
#   .\build_libuv.ps1            # build if needed
#   .\build_libuv.ps1 -Force      # always rebuild
#
# Requires:
#   - $env:NOVA_VCVARS set OR vswhere finds MSVC (same as run_tests.ps1).
#   - libuv submodule initialized (git submodule update --init).

param(
    [switch]$Force,
    [switch]$Verbose
)

$ErrorActionPreference = "Stop"
$script_dir = Split-Path -Parent $MyInvocation.MyCommand.Path

$libuv_dir   = Join-Path $script_dir "compiler-codegen\nova_rt\libuv"
$cache_dir   = Join-Path $script_dir "target\libuv-cache"
$lib_file    = Join-Path $cache_dir "libuv.lib"
$version_file = Join-Path $cache_dir "version.txt"

# ─── 1. Sanity: libuv submodule initialized ────────────────────────────
if (-not (Test-Path "$libuv_dir\include\uv.h")) {
    Write-Host "ERROR: libuv submodule not initialized at $libuv_dir" -ForegroundColor Red
    Write-Host "Run: git submodule update --init --recursive" -ForegroundColor Yellow
    exit 1
}

# ─── 2. Determine libuv version (commit SHA + tag if any) ──────────────
Push-Location $libuv_dir
$libuv_sha = git rev-parse HEAD
$libuv_tag = git describe --tags --exact-match HEAD 2>$null
if (-not $libuv_tag) { $libuv_tag = "(no-tag)" }
Pop-Location
$current_version = "$libuv_sha $libuv_tag"

# ─── 3. Check cache validity ───────────────────────────────────────────
if (-not $Force -and (Test-Path $lib_file) -and (Test-Path $version_file)) {
    $cached_version = Get-Content $version_file -Raw -ErrorAction SilentlyContinue
    if ($cached_version -and $cached_version.Trim() -eq $current_version) {
        Write-Host "libuv cache OK: $libuv_tag ($libuv_sha)" -ForegroundColor Green
        Write-Host "  Lib: $lib_file"
        exit 0
    }
    Write-Host "libuv cache stale, rebuilding..." -ForegroundColor Yellow
}

# ─── 4. Locate vcvars64 (same logic as run_tests.ps1) ──────────────────
if ($env:NOVA_VCVARS) {
    $vcvars = $env:NOVA_VCVARS
} else {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vcvars = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find "VC\Auxiliary\Build\vcvars64.bat" | Select-Object -First 1
    }
    if (-not $vcvars -or -not (Test-Path $vcvars)) {
        Write-Host "ERROR: vcvars64.bat not found. Set NOVA_VCVARS env-var or install Visual Studio Build Tools." -ForegroundColor Red
        exit 1
    }
}

# ─── 5. Prepare cache dir + obj subdir ─────────────────────────────────
$obj_dir = Join-Path $cache_dir "obj"
New-Item -ItemType Directory -Force -Path $cache_dir | Out-Null
New-Item -ItemType Directory -Force -Path $obj_dir | Out-Null

# Clean obj dir (avoid stale objects from previous version)
Remove-Item -Path "$obj_dir\*" -Force -Recurse -ErrorAction SilentlyContinue

# ─── 6. Collect source files (cross + win) ─────────────────────────────
$src_common = Get-ChildItem -Path "$libuv_dir\src\*.c" -File | Select-Object -ExpandProperty FullName
$src_win    = Get-ChildItem -Path "$libuv_dir\src\win\*.c" -File | Select-Object -ExpandProperty FullName
$src_all    = @($src_common) + @($src_win)
Write-Host "libuv: $($src_all.Count) source files" -ForegroundColor Cyan

# ─── 7. Compile all .c → .obj ──────────────────────────────────────────
# Flags:
#   /c          — compile only, no link.
#   /nologo     — quiet.
#   /W0         — suppress all warnings (libuv source has many; we don't patch).
#   /MT         — multi-thread static CRT (matches cl.exe default для Nova tests).
#   /O2         — optimize for speed (release perf).
#   /D_WIN32_WINNT=0x0602 — Windows 8 baseline (libuv requires).
#   /DWIN32_LEAN_AND_MEAN — minimal windows.h.
#   /DBUILDING_UV_SHARED=0 — static linkage.
#   /I include  — public uv.h.
#   /I src      — internal headers (uv-common.h etc).
#   /I src/win  — win-internal headers (internal.h, winapi.h).
$inc_pub  = Join-Path $libuv_dir "include"
$inc_src  = Join-Path $libuv_dir "src"
$inc_win  = Join-Path $libuv_dir "src\win"

$compile_flags = "/c /nologo /W0 /MT /O2 /D_WIN32_WINNT=0x0602 /DWIN32_LEAN_AND_MEAN /DBUILDING_UV_SHARED=0 /I `"$inc_pub`" /I `"$inc_src`" /I `"$inc_win`""
$obj_flag = "/Fo`"$obj_dir\\`""

# Write response file (cl.exe @file.rsp) — bypass cmd.exe 8191-char limit.
$rsp_file = Join-Path $cache_dir "compile.rsp"
$rsp_lines = @($compile_flags, $obj_flag) + ($src_all | ForEach-Object { "`"$_`"" })
Set-Content -Path $rsp_file -Value $rsp_lines -Encoding ascii
$cl_cmd = "cl.exe @`"$rsp_file`""

Write-Host "Compiling libuv source files (this takes ~30 seconds)..." -ForegroundColor Cyan
# Полный wrapper через cmd: vcvars + cl + redirect stderr→stdout уже внутри cmd,
# результат — в log файл. Это обходит NativeCommandError'ы PowerShell на
# stderr-output от vcvars64.bat (vswhere warning'и).
$log_file = Join-Path $cache_dir "compile.log"
$wrapper = "`"$vcvars`" >nul 2>&1 && $cl_cmd > `"$log_file`" 2>&1"
cmd /c $wrapper
$compile_exit = $LASTEXITCODE
if ($compile_exit -ne 0) {
    Write-Host "ERROR: libuv compilation failed (exit=$compile_exit, see $log_file)" -ForegroundColor Red
    Write-Host "--- Last 30 lines of output: ---" -ForegroundColor Gray
    Get-Content $log_file -Tail 30 | ForEach-Object { Write-Host $_ }
    exit 1
}

# ─── 8. Archive all .obj → libuv.lib ───────────────────────────────────
$obj_files = Get-ChildItem -Path "$obj_dir\*.obj" -File | Select-Object -ExpandProperty FullName
if ($obj_files.Count -eq 0) {
    Write-Host "ERROR: no .obj files produced" -ForegroundColor Red
    exit 1
}
# lib.exe тоже имеет ~8191 char limit, используем response file.
$lib_rsp = Join-Path $cache_dir "lib.rsp"
$lib_rsp_lines = @("/nologo", "/OUT:`"$lib_file`"") + ($obj_files | ForEach-Object { "`"$_`"" })
Set-Content -Path $lib_rsp -Value $lib_rsp_lines -Encoding ascii
$lib_cmd = "lib.exe @`"$lib_rsp`""
Write-Host "Archiving $($obj_files.Count) objects into libuv.lib..." -ForegroundColor Cyan
$lib_log = Join-Path $cache_dir "lib.log"
$lib_wrapper = "`"$vcvars`" >nul 2>&1 && $lib_cmd > `"$lib_log`" 2>&1"
cmd /c $lib_wrapper
$lib_exit = $LASTEXITCODE
if ($lib_exit -ne 0) {
    Write-Host "ERROR: lib.exe failed (exit=$lib_exit, see $lib_log)" -ForegroundColor Red
    Get-Content $lib_log -Tail 20 | ForEach-Object { Write-Host $_ }
    exit 1
}

# ─── 9. Write version stamp ────────────────────────────────────────────
Set-Content -Path $version_file -Value $current_version -Encoding utf8

$lib_size = (Get-Item $lib_file).Length / 1MB
Write-Host "libuv.lib built: $($lib_size.ToString('0.0')) MB" -ForegroundColor Green
Write-Host "  Version: $libuv_tag ($libuv_sha)"
Write-Host "  Path: $lib_file"
