@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
if "%~1"=="" goto :eof
if "%~1"=="NOOP" (
  "C:\Program Files\LLVM\bin\clang.exe" --version >nul 2>&1
  goto :eof
)
"C:\Program Files\LLVM\bin\clang.exe" --target=x86_64-pc-windows-msvc -O0 -g -Wno-everything ^
  -ffunction-sections -fdata-sections -DNOVA_GC_BOEHM -DGC_THREADS -DNOVA_USE_LIBUV=1 ^
  -DNOVA_MAX_EFFECT_STORAGES=%~3 ^
  -I "D:\Sources\nv-lang\nova\compiler-codegen" ^
  -I "D:\Sources\nv-lang\nova\compiler-codegen\vcpkg_installed\x64-windows-static\include" ^
  -I "D:\Sources\nv-lang\nova\compiler-codegen\nova_rt\libuv\include" ^
  -c "%~1" -o "%~2"
