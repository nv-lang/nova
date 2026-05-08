@echo off
rem Plan 13: regenerate std/runtime/*.nv from runtime_registry.rs.
rem
rem Usage:
rem   regen_runtime.bat            — write std/runtime/string.nv + math.nv.
rem   regen_runtime.bat --check    — drift detection (CI/pre-commit guard).
rem
rem См. docs/plans/13-runtime-stdlib-and-autogen.md.

setlocal
rem %~dp0 always ends with `\`. Strip it to avoid quoting bugs when
rem path is passed as argument (`"D:\path\"` confuses CLI parsers).
set "ROOT=%~dp0"
if "%ROOT:~-1%"=="\" set "ROOT=%ROOT:~0,-1%"
set "CODEGEN=%ROOT%\compiler-codegen\target\debug\nova-codegen.exe"

if not exist "%CODEGEN%" (
    echo error: nova-codegen.exe not found at %CODEGEN%
    echo Build it first: cargo build --manifest-path compiler-codegen\Cargo.toml
    exit /b 1
)

"%CODEGEN%" emit-runtime-stubs --root "%ROOT%" %*
exit /b %errorlevel%
