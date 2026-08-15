#!/bin/sh
# scripts/tools/novac-fuzz-mutations.sh — the E1 zero-panic acceptance
# (plan 274 §9/Э1, CORE rank; bootstrap §8): mutate the basics corpus and
# demand that novac check never panics or crashes — honest diagnostics or
# clean exits only, on EVERY input.
#
# Deterministic by construction (no RNG): mutations are (a) truncation at
# every step-th byte, (b) a corruption byte written at every step-th offset,
# (c) doubled file and doubled first half. A red run is reproducible by its
# printed case id alone.
#
# Cost (plan 274.2): all mutations are GENERATED first, then judged in ONE
# batch `novac check <all files>` (one process; a single check is ~120ms of
# which most is process start; the old per-case run forked 5+ processes and
# cost 336ms/case = 65s for 192 cases). Verdict per case = its own
# diagnostic lines; a panic kills the batch, so a batch that ends before
# the last case is itself the red signal, and the killer is found by
# bisection over the remaining list (rare path, only on a red run).
#
# Usage: sh scripts/tools/novac-fuzz-mutations.sh [step]   (default 40)
# Проверялся: Windows (Git Bash), 2026-08-15.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
. "$ROOT/scripts/guards/lib/novac.sh"   # novac_is_panic_rc: контракт §7 — только 0/1
NOVAC="$ROOT/novac/target/novac.exe"
STEP="${1:-40}"
T="${TMPDIR:-/tmp}/novac-fuzz.$$"
mkdir -p "$T/cases"
[ -f "$NOVAC" ] || { echo "novac-fuzz: нет $NOVAC" >&2; exit 2; }

# ---- 1. generate all cases (pure file ops, no compiler) ------------------
n=0
for src in "$ROOT"/examples/basics/*.nv; do
    base=$(basename "$src" .nv)
    size=$(wc -c < "$src")
    off=1
    while [ "$off" -lt "$size" ]; do
        head -c "$off" "$src" > "$T/cases/$base-trunc-$off.nv"; n=$((n+1))
        off=$((off+STEP))
    done
    off=0
    while [ "$off" -lt "$size" ]; do
        { head -c "$off" "$src"; printf '\001'; tail -c +$((off+2)) "$src"; } > "$T/cases/$base-corrupt-$off.nv"; n=$((n+1))
        off=$((off+STEP))
    done
    cat "$src" "$src" > "$T/cases/$base-doubled.nv"; n=$((n+1))
    half=$((size/2))
    { head -c "$half" "$src"; head -c "$half" "$src"; } > "$T/cases/$base-halves.nv"; n=$((n+1))
done
ls "$T/cases" | sort > "$T/list"

# ---- 2. one batch check; a panic/hang/crash of the process is red ---------
judge() {   # $1 = list file; prints nothing on green, case ids on red
    # shellcheck disable=SC2046
    ( cd "$T/cases" && timeout 120 "$NOVAC" check $(cat "$1") ) > "$T/out" 2> "$T/err"
    rc=$?
    if novac_is_panic_rc "$rc" || grep -qi "panic" "$T/err"; then
        return 1
    fi
    return 0
}
if judge "$T/list"; then
    rm -rf "$T"
    echo "novac-fuzz ok: $n мутаций basics, паник 0"
    exit 0
fi

# ---- 3. red: bisect the list to name the killers (rare path) -------------
echo "novac-fuzz: батч красный (rc/паника) — ищу виновников бисекцией" >&2
bad=0
bisect() {   # $1 = list file
    cnt=$(wc -l < "$1")
    if [ "$cnt" -le 1 ]; then
        cid=$(cat "$1")
        bad=$((bad+1))
        echo "novac-fuzz: PANIC/CRASH/HANG на случае ${cid%.nv}" >&2
        head -2 "$T/err" | sed 's/^/    /' >&2
        cp "$T/cases/$cid" "$T/keep.$cid"
        return
    fi
    half=$((cnt/2))
    head -n "$half" "$1" > "$1.a"; tail -n +$((half+1)) "$1" > "$1.b"
    judge "$1.a" || bisect "$1.a"
    judge "$1.b" || bisect "$1.b"
}
bisect "$T/list"
echo "novac-fuzz: FAIL — паник $bad из $n (репро в $T/keep.*)" >&2
exit 1
