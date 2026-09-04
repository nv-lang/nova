#!/bin/sh
# grab-oracle-c.sh <file.nv> <out.c> -- build with the oracle through a NOVA_CLANG
# wrapper that copies the generated .c aside, so the ORACLE's own C can be read.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FILE="$1"; OUT="$2"
T="${TMPDIR:-/tmp}/grabc.$$"; mkdir -p "$T"; trap 'rm -rf "$T"' 0
cd "$ROOT" || exit 2
REAL_CLANG="${NOVA_CLANG_REAL:-C:/Program Files/LLVM/bin/clang.exe}"
WIN_OUT=$(cygpath -w "$(cd "$(dirname "$OUT")" && pwd)")"\\$(basename "$OUT")"
printf '@echo off\r\nsetlocal enabledelayedexpansion\r\n:loop\r\nif "%%~1"=="" goto run\r\nset A=%%~1\r\nif /I "!A:~-2!"==".c" copy /Y "!A!" "%s" >nul\r\nshift\r\ngoto loop\r\n:run\r\n"%s" %%*\r\n' "$WIN_OUT" "$(cygpath -w "$REAL_CLANG")" > "$T/w.cmd"
NOVA_CLANG="$(cygpath -w "$T")\\w.cmd" "$ROOT/nova-cli/target/release/nova.exe" build "$FILE" -o "$T/a.exe" 2>&1 | grep -v vcpkg | tail -3
[ -f "$OUT" ] && echo "grab-oracle-c: wrote $OUT ($(grep -c '' "$OUT") lines)" || echo "grab-oracle-c: NO C CAPTURED"
