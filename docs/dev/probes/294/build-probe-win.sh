#!/bin/sh
# Build a plan-294 probe with clang against the repo's libuv (Windows, MSVC ABI).
# usage: build-probe-win.sh <probe.c> <out.exe> [extra libs]
UV=${UV_CACHE:-/d/Sources/nv-lang/nova/target/libuv-cache/d28184508b18249e/libuv.lib}
INC=${UV_INC:-$(dirname "$0")/../../../../compiler-codegen/nova_rt/libuv/include}
src=$1; out=$2; shift 2
"/c/Program Files/LLVM/bin/clang.exe" -O1 -I"$INC" "$src" "$UV" -lws2_32 -liphlpapi -luserenv -lole32 -ladvapi32 -lshell32 -luser32 -ldbghelp -lsecur32 -lntdll "$@" -o "$out"
