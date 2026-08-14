#!/bin/sh
# scripts/tools/novac-e1-smoke.sh — the E1 vertical smoke: a file compiled by
# novac must BEHAVE identically to the oracle's binary (plan 274 §9/Э1 core).
#
# Seam (proven 2026-08-14): novac emits a full C translation unit (fixed
# runtime shell + generated body); the EXISTING driver links it by the
# build-cache seam — after an oracle build the cache holds the generated C
# under the file's key, we overwrite that .c with novac's emission and rebuild:
# the driver reuses it and does cc+link with its own flags. Rust untouched.
#
# Usage: sh scripts/tools/novac-e1-smoke.sh [file.nv]   (default: hello)
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FILE="${1:-examples/basics/hello.nv}"
NOVAC="$ROOT/novac/target/novac.exe"
ORACLE_MAIN=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
ORACLE="$ORACLE_MAIN/../nova-cli/target/release/nova.exe"
[ -f "$ORACLE" ] || ORACLE="$ROOT/nova-cli/target/release/nova.exe"
T="${TMPDIR:-/tmp}/novac-smoke.$$"
mkdir -p "$T"

fail() { echo "novac-e1-smoke: FAIL — $1" >&2; exit 1; }
[ -f "$NOVAC" ] || fail "нет $NOVAC (собери novac)"
[ -f "$ORACLE" ] || fail "нет оракула"

cd "$ROOT" || exit 2
# 1. Oracle build: binary + its generated C lands in the cache.
"$ORACLE" build "$FILE" -o "$T/oracle.exe" >/dev/null 2>&1 || fail "оракул не собрал $FILE"
KEY=$(ls -t "$ROOT/target/.nova-cache/"*.c 2>/dev/null | head -1)
[ -n "$KEY" ] || fail "кэш C не найден после сборки оракула"
cp "$KEY" "$T/oracle.c"

# 2. novac emit -> same cache key -> driver relink.
"$NOVAC" emit "$FILE" > "$T/novac.c" 2>"$T/emit.err" || fail "novac emit упал: $(cat "$T/emit.err")"
cp "$T/novac.c" "$KEY"
"$ORACLE" build "$FILE" -o "$T/novac.exe" >/dev/null 2>&1
rc=$?
cp "$T/oracle.c" "$KEY"   # всегда вернуть кэш оракула на место
[ "$rc" -eq 0 ] || fail "драйвер не слинковал C от novac"

# 3. Behavior diff.
"$T/oracle.exe" > "$T/out.oracle" 2>&1; e_o=$?
"$T/novac.exe"  > "$T/out.novac"  2>&1; e_n=$?
diff "$T/out.oracle" "$T/out.novac" > "$T/out.diff" 2>&1 \
    || fail "stdout расходится: $(head -3 "$T/out.diff")"
[ "$e_o" -eq "$e_n" ] || fail "exit-коды расходятся: oracle=$e_o novac=$e_n"

echo "novac-e1-smoke ok: $FILE — поведение идентично оракулу (stdout байт-в-байт, exit $e_o)"
rm -rf "$T"
exit 0
