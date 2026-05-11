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
    # Plan 26 Ф.4: output format.
    [ValidateSet("text", "json", "tap")]
    [string]$Format = "text",
    # Plan 26 Ф.9: verbose/quiet.
    [switch]$Verbose,
    [switch]$Quiet,
    # Plan 26 Ф.10: results-file path + --rerun-failed.
    [string]$ResultsFile = "",
    [switch]$RerunFailed
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
if ($vcvars -and (Test-Path $vcvars)) {
    $cli_args += @("--vcvars", $vcvars)
}

# Plan 26 Ф.11: вызов через cmd.exe вместо `& exe`, чтобы избежать PS
# stderr-trap (PowerShell оборачивает native-exe stderr lines в
# NativeCommandError, при $ErrorActionPreference='Stop' это эскалирует
# до termination'а самого скрипта). cmd.exe не имеет этого trap'а.
# Quoting через -ArgumentList: PowerShell сам escape'ит каждый arg.
$proc = Start-Process -FilePath $codegen -ArgumentList $cli_args `
    -NoNewWindow -Wait -PassThru
exit $proc.ExitCode
