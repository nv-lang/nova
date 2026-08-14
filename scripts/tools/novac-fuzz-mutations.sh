#!/bin/sh
# scripts/tools/novac-fuzz-mutations.sh — the E1 zero-panic acceptance
# (plan 274 §9/Э1, CORE rank; bootstrap §8): mutate the basics corpus and
# demand that novac check never panics or crashes — honest diagnostics or
# clean exits only, on EVERY input.
#
# Deterministic by construction (no RNG): mutations are (a) truncation at
# every step-th byte, (b) a corruption byte written at every step-th offset,
# (c) doubled and reversed halves. Determinism means a red run is
# reproducible by its printed case id alone.
#
# Usage: sh scripts/tools/novac-fuzz-mutations.sh [step]   (default 40)
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NOVAC="$ROOT/novac/target/novac.exe"
STEP="${1:-40}"
T="${TMPDIR:-/tmp}/novac-fuzz.$$"
mkdir -p "$T"

[ -f "$NOVAC" ] || { echo "novac-fuzz: нет $NOVAC" >&2; exit 2; }

total=0; bad=0
run_case() {
    cid="$1"; f="$2"
    total=$((total+1))
    # 10s, not 5: a loaded machine stalls a normal ~50ms run for seconds
    # (measured 5.3s worst on 2026-08-14); real infinite loops still trip it.
    timeout 10 "$NOVAC" check "$f" > /dev/null 2> "$T/err"; rc=$?
    if [ "$rc" -ge 124 ] || grep -qi "panic" "$T/err"; then
        bad=$((bad+1))
        echo "novac-fuzz: PANIC/CRASH/HANG на случае $cid (exit=$rc)" >&2
        head -2 "$T/err" | sed 's/^/    /' >&2
        cp "$f" "$T/keep.$cid.nv"
    fi
}

for src in "$ROOT"/examples/basics/*.nv; do
    base=$(basename "$src" .nv)
    size=$(wc -c < "$src")
    # (a) truncations
    off=1
    while [ "$off" -lt "$size" ]; do
        head -c "$off" "$src" > "$T/m.nv"
        run_case "$base-trunc-$off" "$T/m.nv"
        off=$((off+STEP))
    done
    # (b) corruption byte at offsets
    off=0
    while [ "$off" -lt "$size" ]; do
        { head -c "$off" "$src"; printf '\001'; tail -c +$((off+2)) "$src"; } > "$T/m.nv"
        run_case "$base-corrupt-$off" "$T/m.nv"
        off=$((off+STEP))
    done
    # (c) structural: doubled file, first half + first half
    cat "$src" "$src" > "$T/m.nv";              run_case "$base-doubled" "$T/m.nv"
    half=$((size/2))
    { head -c "$half" "$src"; head -c "$half" "$src"; } > "$T/m.nv"
    run_case "$base-halves" "$T/m.nv"
done

if [ "$bad" -gt 0 ]; then
    echo "novac-fuzz: FAIL — паник $bad из $total (репро в $T/keep.*)" >&2
    exit 1
fi
rm -rf "$T"
echo "novac-fuzz ok: $total мутаций basics, паник 0"
exit 0
