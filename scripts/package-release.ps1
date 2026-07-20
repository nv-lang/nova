# SPDX-License-Identifier: MIT OR Apache-2.0
#
# package-release.ps1 — Plan 221 Ф.2 (A-V2): собирает Windows-x64 zip-релиз
# Nova (nova.exe + nova-lsp.exe + std/ + минимальный C-рантайм для дистрибуции
# вне монорепы).
#
# PowerShell 5.1-совместимость: без `&&`, без ternary (`?:`), без
# null-coalescing (`??`). Только if/else и явные проверки.
#
# std-discovery (см. docs/plans/wip/221-version-notes.md для полной разведки):
# `nova.exe` ищет std/nova_rt НЕ относительно себя, а относительно
# пользовательского проекта (`nova.toml` вверх от CWD) — если не переопределено
# env-переменными `NOVA_STD_PATH` / `NOVA_CG_INCLUDE` / `NOVA_RT_DIR` /
# `NOVA_GC_LIB_DIR` / `NOVA_GC_INCLUDE_DIR` (штатная config-поверхность,
# compiler-codegen/src/manifest.rs resolve_std_path + nova-cli/src/main.rs
# resolve_paths + test_runner.rs detect_boehm). Поэтому дистрибутив = (а)
# кладём std/ + урезанный nova_rt/ (headers + нужный подсет libuv) + урезанный
# gc/ (Boehm GC lib+headers, БЕЗ полного vcpkg_installed) рядом с exe, плюс
# `setup-env.ps1`, который выставляет эти 5 env vars относительно СВОЕГО
# расположения (dot-source после распаковки).
#
# Usage:
#   powershell -File scripts/package-release.ps1 [-SkipBuild] [-Version 0.1.0]
#     [-OutDir dist] [-SmokeTest]
#
#   -SkipBuild   не собирать cargo — взять уже собранные
#                nova-cli/target/release/nova.exe и
#                nova-lsp/target/release/nova-lsp.exe.
#   -SmokeTest   после сборки zip: распаковать в чистую temp-папку, dot-source
#                setup-env.ps1, собрать+прогнать hello.nv (реальная проверка
#                std-discovery вне монорепы).

param(
    [switch]$SkipBuild,
    [string]$Version = "0.1.0",
    [string]$OutDir = "dist",
    [switch]$SmokeTest
)

$ErrorActionPreference = "Stop"

# Корень репо — родитель scripts/.
$RepoRoot = Split-Path -Parent $PSScriptRoot
Write-Host "RepoRoot: $RepoRoot"

$ZipName = "nova-v$Version-windows-x64"
$OutDirFull = Join-Path $RepoRoot $OutDir
$StageRoot = Join-Path $OutDirFull "stage"
$StageDir = Join-Path $StageRoot $ZipName

# ---------- 1. Сборка (если не -SkipBuild) ----------

if (-not $SkipBuild) {
    Write-Host "=== cargo build --release (nova-cli) ==="
    Push-Location (Join-Path $RepoRoot "nova-cli")
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build --release (nova-cli) failed, exit=$LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }

    Write-Host "=== cargo build --release (nova-lsp) ==="
    Push-Location (Join-Path $RepoRoot "nova-lsp")
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build --release (nova-lsp) failed, exit=$LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
} else {
    Write-Host "=== -SkipBuild: беру уже собранные бинари ==="
}

$NovaExe = Join-Path $RepoRoot "nova-cli\target\release\nova.exe"
$NovaLspExe = Join-Path $RepoRoot "nova-lsp\target\release\nova-lsp.exe"

if (-not (Test-Path $NovaExe)) {
    throw "nova.exe не найден: $NovaExe (собери без -SkipBuild, либо cargo build --release --manifest-path nova-cli\Cargo.toml)"
}
if (-not (Test-Path $NovaLspExe)) {
    throw "nova-lsp.exe не найден: $NovaLspExe (собери без -SkipBuild, либо cargo build --release --manifest-path nova-lsp\Cargo.toml)"
}

Write-Host "nova.exe:     $NovaExe"
Write-Host "nova-lsp.exe: $NovaLspExe"

# ---------- 2. Чистая stage-папка ----------

if (Test-Path $StageRoot) {
    Remove-Item -Recurse -Force $StageRoot
}
New-Item -ItemType Directory -Force -Path $StageDir | Out-Null
New-Item -ItemType Directory -Force -Path $OutDirFull | Out-Null

Write-Host "=== Копирую бинари ==="
Copy-Item $NovaExe (Join-Path $StageDir "nova.exe")
Copy-Item $NovaLspExe (Join-Path $StageDir "nova-lsp.exe")

Write-Host "=== Копирую std/ (стандартная библиотека, исходники) ==="
Copy-Item (Join-Path $RepoRoot "std") (Join-Path $StageDir "std") -Recurse

# ---------- 3. nova_rt/ — минимальный C-рантайм для дистрибуции ----------
#
# Полный compiler-codegen/nova_rt/libuv (git submodule) — ~468M рабочего
# дерева (docs/tests/tools/img/.github/cmake-toolchains не нужны сборке).
# build_libuv_lib (test_runner.rs ~4533) компилирует ТОЛЬКО:
#   - libuv/include/**            (заголовки, нужны всегда)
#   - libuv/src/*.c   (НЕ рекурсивно — только верхний уровень)
#   - libuv/src/win/*.c           (Windows-специфика)
# Это резолвится через NOVA_RT_DIR=<install>/nova_rt (detect_or_build_libuv
# ищет `<rt_dir>/libuv/include/uv.h` + `<rt_dir>/eventloop.c`).

Write-Host "=== Копирую nova_rt/ (C headers/sources, БЕЗ полного libuv submodule) ==="
$SrcNovaRt = Join-Path $RepoRoot "compiler-codegen\nova_rt"
$DstNovaRt = Join-Path $StageDir "nova_rt"
New-Item -ItemType Directory -Force -Path $DstNovaRt | Out-Null

# 3a. Верхний уровень nova_rt/*.c, *.h, *.md (без подпапки libuv/).
Get-ChildItem -Path $SrcNovaRt -File | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $DstNovaRt $_.Name)
}

# 3b. libuv/include — целиком, рекурсивно (заголовки нужны компилятору всегда).
$SrcLibuvInclude = Join-Path $SrcNovaRt "libuv\include"
if (-not (Test-Path $SrcLibuvInclude)) {
    throw "libuv/include не найден: $SrcLibuvInclude — libuv submodule не инициализирован в этом checkout? Запусти `git submodule update --init compiler-codegen/nova_rt/libuv` в РЕПО, из которого пакуешь (не в этом worktree — см. docs/plans/wip/221-version-notes.md)."
}
New-Item -ItemType Directory -Force -Path (Join-Path $DstNovaRt "libuv") | Out-Null
Copy-Item $SrcLibuvInclude (Join-Path $DstNovaRt "libuv\include") -Recurse

# 3c. libuv/src/*.c (top-level, не рекурсивно) — общие source-файлы.
$SrcLibuvSrc = Join-Path $SrcNovaRt "libuv\src"
$DstLibuvSrc = Join-Path $DstNovaRt "libuv\src"
New-Item -ItemType Directory -Force -Path $DstLibuvSrc | Out-Null
Get-ChildItem -Path $SrcLibuvSrc -Filter "*.c" -File | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $DstLibuvSrc $_.Name)
}

# 3d. libuv/src/win/*.c — Windows platform-specific.
$SrcLibuvWin = Join-Path $SrcLibuvSrc "win"
$DstLibuvWin = Join-Path $DstLibuvSrc "win"
New-Item -ItemType Directory -Force -Path $DstLibuvWin | Out-Null
Get-ChildItem -Path $SrcLibuvWin -Filter "*.c" -File | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $DstLibuvWin $_.Name)
}

# 3e. libuv LICENSE (compliance — используем C-исходники libuv).
$LibuvLicense = Join-Path $SrcNovaRt "libuv\LICENSE"
if (Test-Path $LibuvLicense) {
    Copy-Item $LibuvLicense (Join-Path $DstNovaRt "libuv\LICENSE")
}

Write-Host "nova_rt/ staged: $DstNovaRt"

# ---------- 4. gc/ — Boehm GC (нужен detect_boehm's NOVA_GC_LIB_DIR/INCLUDE_DIR) ----------
#
# ПОЛНЫЙ vcpkg_installed/x64-windows-static — ~4.3G (там ещё z3 и debug-либы).
# Дистрибуции нужны только gc.lib+atomic_ops.lib + их заголовки.

Write-Host "=== Копирую gc/ (Boehm GC lib+headers, подмножество vcpkg_installed) ==="
$VcpkgBase = Join-Path $RepoRoot "compiler-codegen\vcpkg_installed\x64-windows-static"
$DstGc = Join-Path $StageDir "gc"
$DstGcLib = Join-Path $DstGc "lib"
$DstGcInclude = Join-Path $DstGc "include"
New-Item -ItemType Directory -Force -Path $DstGcLib | Out-Null
New-Item -ItemType Directory -Force -Path $DstGcInclude | Out-Null

$GcOk = $true
$GcLibFiles = @("gc.lib", "atomic_ops.lib")
foreach ($f in $GcLibFiles) {
    $srcF = Join-Path $VcpkgBase "lib\$f"
    if (Test-Path $srcF) {
        Copy-Item $srcF (Join-Path $DstGcLib $f)
    } else {
        Write-Warning "GC lib отсутствует: $srcF"
        $GcOk = $false
    }
}

$GcIncludeTop = @("gc.h", "gc_cpp.h", "atomic_ops.h", "atomic_ops_malloc.h", "atomic_ops_stack.h")
foreach ($f in $GcIncludeTop) {
    $srcF = Join-Path $VcpkgBase "include\$f"
    if (Test-Path $srcF) {
        Copy-Item $srcF (Join-Path $DstGcInclude $f)
    } else {
        Write-Warning "GC include отсутствует: $srcF"
        $GcOk = $false
    }
}

$GcIncludeDirs = @("gc", "atomic_ops")
foreach ($d in $GcIncludeDirs) {
    $srcD = Join-Path $VcpkgBase "include\$d"
    if (Test-Path $srcD) {
        Copy-Item $srcD (Join-Path $DstGcInclude $d) -Recurse
    } else {
        Write-Warning "GC include-подпапка отсутствует: $srcD"
        $GcOk = $false
    }
}

if (-not $GcOk) {
    Write-Warning "GC-бандл неполный — дистрибутив не будет собирать программы без системного vcpkg/Boehm GC на машине пользователя. См. [M-release-std-discovery] в отчёте."
}

Write-Host "gc/ staged: $DstGc (ok=$GcOk)"
