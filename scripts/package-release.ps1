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
