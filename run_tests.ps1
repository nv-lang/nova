# Plan 24 + Plan 26: thin wrapper над `nova-codegen test-all`.
#
# Логика runner'а (EXPECT-маркеры, toolchain detection, build flags,
# subprocess invocation, parallel scheduling, timeout enforcement, JSON
# output, --rerun-failed) живёт в `compiler-codegen/src/test_runner.rs`.
# Этот скрипт только:
#   1. Локализует пути относительно репо.
#   2. Детектит vcvars64.bat через vswhere.exe (Windows-specific).
#   3. Прокидывает аргументы в `nova-codegen test-all`.
#
# Cross-platform аналог — `run_tests.sh`.

param(
    [string]$Filter = "",
    [switch]$IncludeStdlib,
    [ValidateSet("dev", "release")]
    [string]$Mode = "dev",
    [ValidateSet("auto", "clang", "msvc", "gcc")]
    [string]$Toolchain = "auto",
    [switch]$KeepArtifacts,
    # Plan 26 Ф.1: timeout per test (секунды).
    [int]$Timeout = 60,
    # Plan 26 Ф.3: количество параллельных worker'ов. 0 = num_cpus.
    [int]$Jobs = 0,
    # Plan 26 Ф.4 + Ф.14: output format.
    [ValidateSet("text", "json", "tap", "junit")]
    [string]$Format = "text",
    # Plan 26 Ф.9: verbose/quiet.
    [switch]$Verbose,
    [switch]$Quiet,
    # Plan 26 Ф.10: results-file path + --rerun-failed.
    [string]$ResultsFile = "",
    [switch]$RerunFailed,
    # Plan 26 Ф.12: retry transient AV/race fails. 0 = no retry, 2 = CI default.
    [int]$Retries = 0
)

$ErrorActionPreference = "Continue"
$repo_root = $PSScriptRoot
$codegen = if ($env:NOVA_CODEGEN) {
    $env:NOVA_CODEGEN
} else {
    Join-Path $repo_root "compiler-codegen\target\debug\nova-codegen.exe"
}

if (-not (Test-Path $codegen)) {
    Write-Host "ERROR: nova-codegen not found at $codegen" -ForegroundColor Red
    Write-Host "Run: cd compiler-codegen; cargo build" -ForegroundColor Yellow
    exit 1
}

# vcvars detection — Windows-specific. На Linux/macOS блок просто пропустится.
$vcvars = $env:NOVA_VCVARS
if (-not $vcvars) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vcvars = & $vswhere -latest -products * `
            -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
            -find "VC\Auxiliary\Build\vcvars64.bat" | Select-Object -First 1
    }
}

# Default ResultsFile если RerunFailed запрошен — стандартный путь в target/.
if (-not $ResultsFile -and $RerunFailed) {
    $ResultsFile = Join-Path $repo_root "target\last-test-results.json"
}

$cli_args = @(
    "test-all",
    "--tests-dir", (Join-Path $repo_root "nova_tests"),
    "--stdlib-dir", (Join-Path $repo_root "std"),
    "--cg-include", (Join-Path $repo_root "compiler-codegen"),
    "--rt-dir", (Join-Path $repo_root "compiler-codegen\nova_rt"),
    "--mode", $Mode,
    "--toolchain", $Toolchain,
    "--timeout", $Timeout,
    "--jobs", $Jobs,
    "--format", $Format
)
if ($Filter)         { $cli_args += @("--filter", $Filter) }
if ($IncludeStdlib)  { $cli_args += "--include-stdlib" }
if ($KeepArtifacts)  { $cli_args += "--keep-artifacts" }
if ($Verbose)        { $cli_args += "--verbose" }
if ($Quiet)          { $cli_args += "--quiet" }
if ($ResultsFile)    { $cli_args += @("--results-file", $ResultsFile) }
if ($RerunFailed)    { $cli_args += "--rerun-failed" }
if ($Retries -gt 0)  { $cli_args += @("--retries", $Retries) }
if ($vcvars -and (Test-Path $vcvars)) {
    $cli_args += @("--vcvars", $vcvars)
}

# Plan 26 Ф.11: snake-pit с PowerShell native-exe stderr-trap'ом:
#   - `& $codegen @cli_args` — массив expand'ится корректно, но при
#     $ErrorActionPreference='Stop' native stderr lines становятся
#     NativeCommandError и обрушивают скрипт с exit 255.
#   - `Start-Process -ArgumentList $cli_args` — массив join'ится через
#     space без quoting, поэтому пути с пробелами (vcvars) ломаются.
#
# Решение: `Continue` + явный stderr-merge через Out-String. Stderr идёт
# в stdout normally, exit code нативно сохраняется в $LASTEXITCODE.
$ErrorActionPreference = "Continue"
& $codegen @cli_args
exit $LASTEXITCODE
