#!/bin/sh
# Measure emitted self-build C, without linking/running stage B.
# Usage: sh measure.sh NOVAC_BIN SMOKE_CACHE SCRATCH_OUTPUT
# All three paths must be absolute; output is a session scratchpad directory.
set -eu
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
NOVAC="${1:?absolute novac binary path}"
CACHE="${2:?absolute smoke cache path}"
OUT="${3:?absolute session scratch output path}"
cd "$ROOT"
mkdir -p "$OUT"
# Pass a cache with exactly one prepared flag/PCH pair, rather than silently
# choosing a stale pair when the oracle/header stamp changes.
set -- "$CACHE"/cflags-*.argv
[ "$#" = 1 ] && [ -f "$1" ] || { echo 'expected one cflags file' >&2; exit 2; }
CFLAGS="$1"
PCH="$CACHE/prelude-$(basename "$CFLAGS" | sed 's/^cflags-//; s/\.argv$//').pch"
[ -f "$PCH" ] || { echo 'missing matching PCH' >&2; exit 2; }
CLANG="${NOVA_CLANG:-C:/Program Files/LLVM/bin/clang.exe}"
set -x
NOVAC_SELF_PATH=novac/src "$NOVAC" emit novac/src/main.nv > "$OUT/self.c" 2> "$OUT/emit.log"
grep -q '^#include "nova_rt/nova_rt.h"$' "$OUT/self.c"
sed '0,/^#include "nova_rt\/nova_rt.h"$/{//d}' "$OUT/self.c" > "$OUT/body.c"
set +e
eval "\"$CLANG\" $(tr '\n' ' ' < "$CFLAGS") -ferror-limit=0 -include-pch \"$PCH\" -c \"$OUT/body.c\" -o \"$OUT/self.o\"" > "$OUT/clang.log" 2>&1
RC=$?
set +x
printf 'CLANG_RC=%s\n' "$RC"
printf 'CLANG_ERRORS_TOTAL=%s\n' "$(grep -c 'error:' "$OUT/clang.log")"
echo 'RAW_SORT_UNIQ_BEGIN'
grep ' error: ' "$OUT/clang.log" | sed 's/^.*error: //' | sort | uniq -c
echo 'RAW_SORT_UNIQ_END'
grep 'errors\? generated\.' "$OUT/clang.log" || true
exit "$RC"
