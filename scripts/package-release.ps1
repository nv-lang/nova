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

# ---------- 5. setup-env.ps1 (dot-source после распаковки) ----------
#
# nova.exe НЕ ищет std/nova_rt относительно себя — только через env vars
# (NOVA_STD_PATH/NOVA_CG_INCLUDE/NOVA_RT_DIR/NOVA_GC_LIB_DIR/NOVA_GC_INCLUDE_DIR)
# или репо-относительно от CWD-проекта (find_repo_root). Для дистрибутива вне
# монорепы — только env vars. Скрипт вычисляет путь от СВОЕГО расположения
# ($PSScriptRoot), поэтому работает из любой распакованной папки.

$SetupEnvContent = @'
# setup-env.ps1 — выставляет env vars, чтобы nova.exe/nova-lsp.exe находили
# std/ и C-рантайм из ЭТОЙ распакованной папки (а не из монорепы разработки).
#
# Использование (ОБЯЗАТЕЛЬНО dot-source — иначе env vars не попадут в текущую
# сессию, а исчезнут вместе с дочерним процессом):
#
#   . .\setup-env.ps1
#
# После этого `nova.exe build/test <file>.nv` работает из ЛЮБОЙ рабочей
# директории (при условии, что у твоего проекта есть свой nova.toml —
# `nova init` создаёт его).

$root = $PSScriptRoot

$env:NOVA_STD_PATH = Join-Path $root "std"
$env:NOVA_CG_INCLUDE = $root
$env:NOVA_RT_DIR = Join-Path $root "nova_rt"
$env:NOVA_GC_LIB_DIR = Join-Path $root "gc\lib"
$env:NOVA_GC_INCLUDE_DIR = Join-Path $root "gc\include"

if ($env:PATH -notlike "*$root*") {
    $env:PATH = "$root;$env:PATH"
}

Write-Host "Nova env настроен:"
Write-Host "  NOVA_STD_PATH        = $env:NOVA_STD_PATH"
Write-Host "  NOVA_CG_INCLUDE      = $env:NOVA_CG_INCLUDE"
Write-Host "  NOVA_RT_DIR          = $env:NOVA_RT_DIR"
Write-Host "  NOVA_GC_LIB_DIR      = $env:NOVA_GC_LIB_DIR"
Write-Host "  NOVA_GC_INCLUDE_DIR  = $env:NOVA_GC_INCLUDE_DIR"
Write-Host "  PATH += $root"
'@

Set-Content -Path (Join-Path $StageDir "setup-env.ps1") -Value $SetupEnvContent -Encoding utf8

# ---------- 6. README-INSTALL.md ----------

$ReadmeContent = @"
# Nova v$Version — установка (Windows x64)

## Установка

1. Распакуй `$ZipName.zip` в любую папку (например `C:\nova`).
2. В **той же** PowerShell-сессии, из папки установки, выполни (обязательно
   через точку — dot-source, иначе env-переменные не сохранятся):

   ``````powershell
   . .\setup-env.ps1
   ``````

   Это выставляет ``NOVA_STD_PATH`` / ``NOVA_CG_INCLUDE`` / ``NOVA_RT_DIR`` /
   ``NOVA_GC_LIB_DIR`` / ``NOVA_GC_INCLUDE_DIR`` (нужны, чтобы ``nova.exe``
   находил стандартную библиотеку и C-рантайм вне монорепы разработки) и
   добавляет папку в ``PATH`` текущей сессии.

   Чтобы не повторять это в каждой новой сессии — добавь папку установки в
   ``PATH`` через «Параметры → Переменные среды» и пропиши те же 5 env vars
   постоянно (Панель управления или ``setx``).

3. Проверь: ``nova --version`` должен вывести ``nova $Version``.

## Требования

Nova компилирует программы в C, поэтому на машине нужен C-компилятор:
MSVC (Visual Studio Build Tools, ``vcvars64.bat``) — определяется
автоматически, либо clang/gcc через ``--toolchain``.

## Быстрый старт

У твоего проекта должен быть свой ``nova.toml`` (минимум ``[package]
name = "..."``) — ``nova build``/``nova test`` ищут его вверх от текущей
директории. Дальше — hello world:

``````
module hello

fn main() {
    println("Hello, Nova!")
}
``````

``````powershell
nova build hello.nv
.\hello.exe
``````

Более полный тур — mini_aggregator во флагман-примерах монорепозитория
(``examples/flagship/aggregator``) и quickstart в docs репозитория.

## VSCode-расширение

Пока отдельно от этого архива — см. ``editors/vscode`` в исходном
репозитории (сборка vsix — отдельный атом релиза).

## Лицензия

MIT OR Apache-2.0 — см. ``LICENSE`` / ``LICENSE-MIT`` / ``LICENSE-APACHE``.
Сторонние компоненты (libuv, Boehm GC) — см. ``THIRD_PARTY/`` и
``nova_rt/libuv/LICENSE``.
"@

Set-Content -Path (Join-Path $StageDir "README-INSTALL.md") -Value $ReadmeContent -Encoding utf8

# ---------- 7. Лицензии ----------

Write-Host "=== Копирую лицензии ==="
foreach ($lic in @("LICENSE", "LICENSE-APACHE", "LICENSE-MIT")) {
    $srcLic = Join-Path $RepoRoot $lic
    if (Test-Path $srcLic) {
        Copy-Item $srcLic (Join-Path $StageDir $lic)
    }
}
$ThirdPartyDir = Join-Path $RepoRoot "THIRD_PARTY"
if (Test-Path $ThirdPartyDir) {
    Copy-Item $ThirdPartyDir (Join-Path $StageDir "THIRD_PARTY") -Recurse
}

# ---------- 8. Zip + sha256 ----------

Write-Host "=== Собираю zip ==="
$ZipPath = Join-Path $OutDirFull "$ZipName.zip"
if (Test-Path $ZipPath) {
    Remove-Item -Force $ZipPath
}
Compress-Archive -Path $StageDir -DestinationPath $ZipPath -CompressionLevel Optimal

$Hash = Get-FileHash -Path $ZipPath -Algorithm SHA256
$ShaLine = "$($Hash.Hash.ToLower())  $ZipName.zip"
Set-Content -Path "$ZipPath.sha256" -Value $ShaLine -Encoding ascii

$ZipSizeMb = [math]::Round((Get-Item $ZipPath).Length / 1MB, 1)
Write-Host ""
Write-Host "=== ГОТОВО ==="
Write-Host "Zip:    $ZipPath ($ZipSizeMb MB)"
Write-Host "SHA256: $($Hash.Hash.ToLower())"
Write-Host "(записан рядом: $ZipPath.sha256)"

# ---------- 9. -SmokeTest: реальная проверка std-discovery вне монорепы ----------
#
# Распаковывает zip в ЧИСТУЮ temp-папку (никакого доступа к монорепе),
# dot-source setup-env.ps1, создаёт отдельный "пользовательский проект" в
# ДРУГОЙ temp-папке (свой nova.toml, hello.nv) и гоняет nova.exe build оттуда.
# Это единственный способ реально подтвердить вердикт (а) — что std/nova_rt,
# положенные рядом с exe, действительно работают вне монорепы.

if ($SmokeTest) {
    Write-Host ""
    Write-Host "=== SmokeTest: распаковка в чистую папку + hello-smoke ==="

    $TmpBase = Join-Path ([System.IO.Path]::GetTempPath()) ("nova-release-smoke-" + [System.Guid]::NewGuid().ToString("N"))
    $ExtractDir = Join-Path $TmpBase "extracted"
    $ProjectDir = Join-Path $TmpBase "hello-project"
    New-Item -ItemType Directory -Force -Path $ExtractDir | Out-Null
    New-Item -ItemType Directory -Force -Path $ProjectDir | Out-Null

    try {
        Write-Host "Extract -> $ExtractDir"
        Expand-Archive -Path $ZipPath -DestinationPath $ExtractDir -Force
        $InstallDir = Join-Path $ExtractDir $ZipName

        if (-not (Test-Path (Join-Path $InstallDir "nova.exe"))) {
            throw "SmokeTest FAIL: nova.exe не найден в распакованном архиве ($InstallDir)"
        }

        # dot-source setup-env.ps1 из распакованной папки — задаёт env vars в
        # ЭТОМ процессе powershell.exe (сохраняются до конца скрипта).
        . (Join-Path $InstallDir "setup-env.ps1")

        # "Пользовательский проект" — своя nova.toml (совсем не монорепа).
        Set-Content -Path (Join-Path $ProjectDir "nova.toml") -Value @'
[package]
name = "hello-smoke"
version = "0.1.0"
'@ -Encoding utf8

        Set-Content -Path (Join-Path $ProjectDir "hello.nv") -Value @'
module hello

fn main() {
    println("Hello, Nova!")
}
'@ -Encoding utf8

        Push-Location $ProjectDir
        try {
            Write-Host "--- nova --version (распакованный бинарь) ---"
            & (Join-Path $InstallDir "nova.exe") --version
            if ($LASTEXITCODE -ne 0) {
                throw "SmokeTest FAIL: nova --version, exit=$LASTEXITCODE"
            }

            Write-Host "--- nova build hello.nv (из чистого проекта, env указывает на распакованный std/nova_rt/gc) ---"
            & (Join-Path $InstallDir "nova.exe") build hello.nv
            if ($LASTEXITCODE -ne 0) {
                throw "SmokeTest FAIL: nova build hello.nv, exit=$LASTEXITCODE"
            }

            $HelloExe = Join-Path $ProjectDir "hello.exe"
            if (-not (Test-Path $HelloExe)) {
                throw "SmokeTest FAIL: hello.exe не создан после build"
            }

            Write-Host "--- ./hello.exe ---"
            $out = & $HelloExe
            Write-Host $out
            if ($out -notmatch "Hello, Nova!") {
                throw "SmokeTest FAIL: неожиданный вывод hello.exe: $out"
            }

            Write-Host ""
            Write-Host "=== SMOKE TEST PASSED === (std-discovery вердикт (а) подтверждён: работает из чистой папки вне монорепы)"
        } finally {
            Pop-Location
        }
    } finally {
        Remove-Item -Recurse -Force $TmpBase -ErrorAction SilentlyContinue
    }
}
