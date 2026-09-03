#!/bin/sh
# novac-only.sh <file.nv> -- emit with novac, compile+link with the argv the
# smoke cache already captured from the oracle, then RUN. Used where the oracle
# REFUSES the program: the smoke cannot judge, but novac's own behaviour still
# has to be observable.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FILE="$1"
NOVAC="$ROOT/novac/target/novac.exe"
CACHE="${NOVAC_SMOKE_CACHE:-${TMPDIR:-/tmp}/novac-smoke-cache}"
T="${TMPDIR:-/tmp}/novac-only.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
cd "$ROOT" || exit 2
read -r ORACLE < "$CACHE/oracle.path"
ORACLE_STAMP=$(stat -c %Y "$ORACLE")
_RT=$(find "$ROOT/compiler-codegen/nova_rt" -name '*.h' -printf '%T@\n' 2>/dev/null | sort -rn | head -1 | cut -d. -f1)
if [ -n "$_RT" ] && [ "$_RT" -gt "$ORACLE_STAMP" ] 2>/dev/null; then ORACLE_STAMP="$_RT"; fi
LINKCMD="$CACHE/link-$ORACLE_STAMP.argv"
CFLAGS="$CACHE/cflags-$ORACLE_STAMP.argv"
PCH="$CACHE/prelude-$ORACLE_STAMP.pch"
[ -f "$LINKCMD" ] || { echo "novac-only: no cached argv ($LINKCMD) -- run the smoke on a buildable file first" >&2; exit 2; }
if command -v cygpath >/dev/null 2>&1; then
    REAL_CLANG="${NOVA_CLANG:-C:/Program Files/LLVM/bin/clang.exe}"
else
    REAL_CLANG="${NOVA_CLANG:-$(command -v clang || printf 'clang')}"
fi
"$NOVAC" emit "$FILE" > "$T/novac.c" 2>"$T/emit.err" || { echo "novac-only: EMIT FAILED"; cat "$T/emit.err"; exit 1; }
cp "$T/novac.c" "${NOVAC_ONLY_KEEP_C:-$T/keep.c}" 2>/dev/null
sed '0,/^#include "nova_rt\/nova_rt.h"$/{//d}' "$T/novac.c" > "$T/body.c"
eval "\"$REAL_CLANG\" $(tr '\n' ' ' < "$CFLAGS") -include-pch \"$PCH\" -c \"$T/body.c\" -o \"$T/body.o\"" > "$T/cc.out" 2>&1 \
    || { echo "novac-only: CLANG -c FAILED"; head -20 "$T/cc.out"; exit 1; }
eval "\"$REAL_CLANG\" $(tr '\n' ' ' < "$LINKCMD") -o \"$T/novac.exe\" \"$T/body.o\"" > "$T/link.out" 2>&1 \
    || { echo "novac-only: LINK FAILED"; head -10 "$T/link.out"; exit 1; }
echo "novac-only: novac built it. stdout:"
"$T/novac.exe"; echo "novac-only: exit=$?"
