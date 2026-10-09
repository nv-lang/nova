#!/bin/sh
# Build a plan-294 probe with clang against the repo's libuv (Windows, MSVC ABI).
# usage: build-probe-win.sh <probe.c> <out.exe> [extra libs]
# Paths are DERIVED from the repository, never written down (registry #698):
# libuv.lib is the one `nova test` builds into <repo>/target/libuv-cache/<hash>/;
# override with UV_CACHE / UV_INC, and CLANG if clang is not on PATH.
ROOT=$(cd "$(dirname "$0")/../../../.." && pwd)
UV=${UV_CACHE:-$(ls -t "$ROOT"/target/libuv-cache/*/libuv.lib 2>/dev/null | head -1)}
INC=${UV_INC:-$ROOT/compiler-codegen/nova_rt/libuv/include}
[ -n "$UV" ] && [ -f "$UV" ] || { echo "build-probe-win: no libuv.lib under $ROOT/target/libuv-cache (run any nova test once, or set UV_CACHE)" >&2; exit 1; }
src=$1; out=$2; shift 2
"${CLANG:-clang}" -O1 -I"$INC" "$src" "$UV" -lws2_32 -liphlpapi -luserenv -lole32 -ladvapi32 -lshell32 -luser32 -ldbghelp -lsecur32 -lntdll "$@" -o "$out"
