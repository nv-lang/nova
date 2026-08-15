#!/bin/sh
# scripts/tools/novac-e1-smoke.sh — the E1/E2 vertical smoke: a file compiled
# by novac must BEHAVE identically to the oracle's binary (plan 274 §9 core).
#
# Fast path (owner 2026-08-15: «секунды, а не минута»). Profiling showed the
# oracle's `nova build` spends ~20s OUTSIDE the compiler (dep-lock of the
# examples package ~12s, vcvars capture ~7s per call) and ~1s in cc. So:
#   1. the ORACLE binary of a file is built once and CACHED by the file's
#      content hash (+ oracle mtime) — the second smoke of the same file
#      never calls `nova build`;
#   2. novac's C is linked DIRECTLY by clang with the exact argv the oracle
#      uses, captured ONCE through the NOVA_CLANG interception door
#      (compiler-codegen/src/test_runner.rs: NOVA_CLANG overrides the clang
#      path) and cached next to the oracle binary; re-captured when the
#      oracle binary changes. No hand-written link flags: the flags ARE the
#      oracle's, byte for byte.
# Result: ~1.5s per file after the first run instead of ~40s.
#
# Usage: sh scripts/tools/novac-e1-smoke.sh [file.nv]   (default: hello)
# Проверялся: Windows (Git Bash), 2026-08-15.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FILE="${1:-examples/basics/hello.nv}"
NOVAC="$ROOT/novac/target/novac.exe"
ORACLE_MAIN=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
ORACLE="$ORACLE_MAIN/../nova-cli/target/release/nova.exe"
[ -f "$ORACLE" ] || ORACLE="$ROOT/nova-cli/target/release/nova.exe"
CACHE="${NOVAC_SMOKE_CACHE:-${TMPDIR:-/tmp}/novac-smoke-cache}"
mkdir -p "$CACHE"
T="${TMPDIR:-/tmp}/novac-smoke.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

fail() { echo "novac-e1-smoke: FAIL — $1" >&2; exit 1; }
[ -f "$NOVAC" ] || fail "нет $NOVAC (собери novac)"
[ -f "$ORACLE" ] || fail "нет оракула"
cd "$ROOT" || exit 2

# ---- 1. oracle binary, cached by content hash + oracle mtime -------------
ORACLE_STAMP=$(stat -c %Y "$ORACLE" 2>/dev/null || echo 0)
KEY=$( { cat "$FILE"; echo "$ORACLE_STAMP"; } | md5sum | cut -c1-16)
ORACLE_EXE="$CACHE/oracle-$KEY.exe"
LINKCMD="$CACHE/link-$ORACLE_STAMP.argv"
if [ ! -f "$ORACLE_EXE" ] || [ ! -f "$LINKCMD" ]; then
    # Build OUTSIDE the examples package (module renamed to the file stem:
    # D78 root-peer rule) — skips the package's dep-lock entirely.
    STEM=$(basename "$FILE" .nv)
    mkdir -p "$T/pkgless"
    sed "s/^module [a-zA-Z_.]*${STEM}\$/module ${STEM}/" "$FILE" > "$T/pkgless/$STEM.nv"
    # Capture the oracle's clang argv through the NOVA_CLANG door on the
    # SAME build that produces the reference binary — one build, two facts.
    WIN_T=$(cygpath -w "$T" 2>/dev/null || echo "$T")
    LOG="$T/cc.log"; : > "$LOG"
    WIN_LOG=$(cygpath -w "$LOG" 2>/dev/null || echo "$LOG")
    REAL_CLANG="${NOVA_CLANG:-C:\Program Files\LLVM\bin\clang.exe}"
    printf '@echo off\r\nsetlocal\r\n:loop\r\nif "%%~1"=="" goto run\r\necho %%1>> "%s"\r\nshift\r\ngoto loop\r\n:run\r\necho __END__>> "%s"\r\n"%s" %%*\r\n' "$WIN_LOG" "$WIN_LOG" "$REAL_CLANG" > "$T/clang-log.cmd"
    NOVA_CLANG="$WIN_T\clang-log.cmd" "$ORACLE" build "$T/pkgless/$STEM.nv" -o "$T/oracle.exe" >"$T/oracle.out" 2>&1 \
        || fail "оракул не собрал $FILE: $(tail -3 "$T/oracle.out")"
    cp "$T/oracle.exe" "$ORACLE_EXE"
    # Keep every arg except -o/<exe> and the .c input (they vary per file).
    # Backslashes -> forward slashes: clang on Windows takes both, and the
    # shell eval below would eat backslashes.
    awk 'BEGIN{skip=0} /^__END__/{exit} skip{skip=0; next} /^-o$/{skip=1; next} /\.c"?$/{next} {print}' "$LOG" | tr -d '\r' | sed 's|\\|/|g' > "$LINKCMD"
    grep -q "libnova_rt" "$LINKCMD" || fail "перехват clang-argv не сработал (нет rt-архива в $LINKCMD)"
fi

# ---- 2. novac emit + direct clang link with the oracle's own argv --------
"$NOVAC" emit "$FILE" > "$T/novac.c" 2>"$T/emit.err" || fail "novac emit упал: $(cat "$T/emit.err")"
REAL_CLANG="${NOVA_CLANG:-C:/Program Files/LLVM/bin/clang.exe}"
# argv file -> command line: args are already shell-safe tokens (paths without
# spaces except the quoted ones, which the log kept quoted).
ARGS=$(tr '\n' ' ' < "$LINKCMD")
NOVAC_EXE_W=$(cygpath -m "$T/novac.exe" 2>/dev/null || echo "$T/novac.exe")
NOVAC_C_W=$(cygpath -m "$T/novac.c" 2>/dev/null || echo "$T/novac.c")
eval "\"$REAL_CLANG\" $ARGS -o \"$NOVAC_EXE_W\" \"$NOVAC_C_W\"" > "$T/link.out" 2>&1 \
    || fail "clang не слинковал C от novac: $(head -5 "$T/link.out")"

# ---- 3. behavior diff ---------------------------------------------------
"$ORACLE_EXE" > "$T/out.oracle" 2>&1; e_o=$?
"$T/novac.exe"  > "$T/out.novac"  2>&1; e_n=$?
diff "$T/out.oracle" "$T/out.novac" > "$T/out.diff" 2>&1 \
    || fail "stdout расходится: $(head -3 "$T/out.diff")"
[ "$e_o" -eq "$e_n" ] || fail "exit-коды расходятся: oracle=$e_o novac=$e_n"

echo "novac-e1-smoke ok: $FILE — поведение идентично оракулу (stdout байт-в-байт, exit $e_o)"
exit 0
