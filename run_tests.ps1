# Plan 24: thin wrapper над `nova-codegen test-all`.
#
# Логика runner'а (EXPECT-маркеры, toolchain detection, build flags,
# subprocess invocation) живёт в `compiler-codegen/src/test_runner.rs`.
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
    [switch]$KeepArtifacts
)

$ErrorActionPreference = "Stop"
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

# vcvars detection — Windows-specific, передаём в test-all явным флагом.
# На Linux/macOS vcvars не нужен; этот блок просто пропустится.
$vcvars = $env:NOVA_VCVARS
if (-not $vcvars) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vcvars = & $vswhere -latest -products * `
            -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
            -find "VC\Auxiliary\Build\vcvars64.bat" | Select-Object -First 1
    }
}

$cli_args = @(
    "test-all",
    "--tests-dir", (Join-Path $repo_root "nova_tests"),
    "--stdlib-dir", (Join-Path $repo_root "std"),
    "--cg-include", (Join-Path $repo_root "compiler-codegen"),
    "--rt-dir", (Join-Path $repo_root "compiler-codegen\nova_rt"),
    "--mode", $Mode,
    "--toolchain", $Toolchain
)
if ($Filter)         { $cli_args += @("--filter", $Filter) }
if ($IncludeStdlib)  { $cli_args += "--include-stdlib" }
if ($KeepArtifacts)  { $cli_args += "--keep-artifacts" }
if ($vcvars -and (Test-Path $vcvars)) {
    $cli_args += @("--vcvars", $vcvars)
}

# PowerShell trap: native exe пишущий в stderr триггерит NativeCommandError
# в pipe'ах ($ErrorActionPreference='Stop' эскалирует это до termination'а
# процесса с exit 255). Workaround: явно установить Continue вокруг вызова.
# Stderr-output остаётся видимым через автоматический PS merge в stdout-host
# при $ErrorActionPreference='Continue'.
$ErrorActionPreference = "Continue"
& $codegen @cli_args
exit $LASTEXITCODE
