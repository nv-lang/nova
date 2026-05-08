# Plan 13: regenerate std/runtime/*.nv from runtime_registry.rs.
#
# Usage:
#   .\regen_runtime.ps1            — write std/runtime/string.nv + math.nv.
#   .\regen_runtime.ps1 --check    — drift detection (CI/pre-commit guard).
#
# См. docs/plans/13-runtime-stdlib-and-autogen.md.

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$codegen = Join-Path $root "compiler-codegen\target\debug\nova-codegen.exe"

if (-not (Test-Path $codegen)) {
    Write-Error "nova-codegen.exe not found at $codegen.`nBuild it: cargo build --manifest-path compiler-codegen\Cargo.toml"
    exit 1
}

& $codegen emit-runtime-stubs --root $root @args
exit $LASTEXITCODE
