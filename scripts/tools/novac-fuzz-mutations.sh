#!/bin/sh
# scripts/tools/novac-fuzz-mutations.sh — the E1 zero-panic acceptance
# (plan 274 §9/Э1, CORE rank; bootstrap §8): mutate a corpus and demand that
# novac check never panics or crashes — honest diagnostics or clean exits
# only, on EVERY input.
#
# Deterministic by construction (no RNG): mutations are (a) truncation at
# every step-th byte, (b) a corruption byte written at every step-th offset,
# (c) a deleted run of step bytes, (d) two adjacent bytes swapped, (e) an
# unbalanced closer injected, (f) doubled file and doubled first half. A red
# run is reproducible by its printed case id alone.
#
# Cost (plan 274.2): all mutations are GENERATED first, then judged in
# BATCHES of CHUNK files (one process per chunk; a single check is ~120ms of
# which most is process start). Chunking is not a nicety: the whole list on
# one command line blows the Windows 32k argument limit the moment the run
# gets dense, and the failure mode is a truncated list judged as green —
# a fuzzer that silently stops fuzzing. Verdict per case = its own
# diagnostic lines; a panic kills its chunk, so a chunk that ends before its
# last case is itself the red signal, and the killer is found by bisection
# over that chunk (rare path, only on a red run).
#
# Usage: sh scripts/tools/novac-fuzz-mutations.sh [step] [corpus-dir] [chunk]
#   step        byte stride of the mutations, default 40 (smaller = denser)
#   corpus-dir  directory scanned for *.nv, default examples/basics
#   chunk       files per novac process, default 150
# Проверялся: Windows (Git Bash), 2026-08-17.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
. "$ROOT/scripts/guards/lib/novac.sh"   # novac_is_panic_rc: контракт §7 — только 0/1
NOVAC="$ROOT/novac/target/novac.exe"
STEP="${1:-40}"
CORPUS="${2:-$ROOT/examples/basics}"
CHUNK="${3:-150}"
T="${TMPDIR:-/tmp}/novac-fuzz.$$"
mkdir -p "$T/cases"
trap 'rm -rf "$T"' 2 15
[ -f "$NOVAC" ] || { echo "novac-fuzz: нет $NOVAC" >&2; exit 2; }
[ -d "$CORPUS" ] || { echo "novac-fuzz: нет корпуса $CORPUS" >&2; exit 2; }

# ---- 1. generate all cases (pure file ops, no compiler) ------------------
n=0
srcn=0
find "$CORPUS" -type f -name '*.nv' | sort > "$T/srcs"
[ -s "$T/srcs" ] || { echo "novac-fuzz: в $CORPUS нет ни одного .nv" >&2; exit 2; }
while IFS= read -r src; do
    srcn=$((srcn+1))
    base=$(basename "$src" .nv)-$srcn
    size=$(wc -c < "$src")
    [ "$size" -gt 2 ] || continue
    off=1
    while [ "$off" -lt "$size" ]; do
        head -c "$off" "$src" > "$T/cases/$base-trunc-$off.nv"; n=$((n+1))
        off=$((off+STEP))
    done
    off=0
    while [ "$off" -lt "$size" ]; do
        { head -c "$off" "$src"; printf '\001'; tail -c +$((off+2)) "$src"; } > "$T/cases/$base-corrupt-$off.nv"
        n=$((n+1))
        off=$((off+STEP))
    done
    # (c) a deleted run: the shape a truncation never makes — a HOLE with
    # valid text on both sides, which is what a half-typed edit looks like.
    off=1
    while [ $((off+STEP)) -lt "$size" ]; do
        { head -c "$off" "$src"; tail -c +$((off+STEP+1)) "$src"; } > "$T/cases/$base-hole-$off.nv"
        n=$((n+1))
        off=$((off+STEP))
    done
    # (d) two adjacent bytes swapped: keeps the length and the alphabet, so
    # the lexer stays happy and the PARSER gets the surprise.
    off=1
    while [ $((off+1)) -lt "$size" ]; do
        { head -c $((off-1)) "$src"
          tail -c +$((off+1)) "$src" | head -c 1
          tail -c +$off "$src" | head -c 1
          tail -c +$((off+2)) "$src"; } > "$T/cases/$base-swap-$off.nv"
        n=$((n+1))
        off=$((off+STEP))
    done
    # (e) an unbalanced closer: the recovery paths (TERMINATORS, the junk
    # buckets) are exactly what a stray ')' or '}' exercises, and they are
    # the youngest code in the parser.
    for cl in ')' '}' ']'; do
        off=1
        while [ "$off" -lt "$size" ]; do
            { head -c "$off" "$src"; printf '%s' "$cl"; tail -c +$((off+1)) "$src"; } \
                > "$T/cases/$base-closer$(printf '%s' "$cl" | od -An -tx1 | tr -d ' ')-$off.nv"
            n=$((n+1))
            off=$((off+STEP*3))
        done
    done
    cat "$src" "$src" > "$T/cases/$base-doubled.nv"; n=$((n+1))
    half=$((size/2))
    { head -c "$half" "$src"; head -c "$half" "$src"; } > "$T/cases/$base-halves.nv"; n=$((n+1))
done < "$T/srcs"
ls "$T/cases" | sort > "$T/list"

# ---- 2. batched check; a panic/hang/crash of a process is red ------------
judge() {   # $1 = list file; 0 = green, 1 = something died
    # shellcheck disable=SC2046
    ( cd "$T/cases" && NOVA_STD_PATH="$ROOT/std/src" timeout 300 "$NOVAC" check $(cat "$1") ) \
        > "$T/out" 2> "$T/err"
    rc=$?
    if novac_is_panic_rc "$rc" || grep -qi "panic" "$T/err"; then
        return 1
    fi
    return 0
}
split -l "$CHUNK" "$T/list" "$T/chunk." 2>/dev/null || {
    awk -v C="$CHUNK" -v P="$T/chunk." 'NR%C==1{f=sprintf("%s%03d",P,int(NR/C))} {print > f}' "$T/list"
}
red=""
for c in "$T"/chunk.*; do
    judge "$c" || red="$red $c"
done
if [ -z "$red" ]; then
    rm -rf "$T"
    echo "novac-fuzz ok: $n мутаций ($srcn файлов корпуса, шаг $STEP), паник 0"
    exit 0
fi

# ---- 3. red: bisect the guilty chunks to name the killers ----------------
echo "novac-fuzz: чанк(и) красные (rc/паника) — ищу виновников бисекцией" >&2
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
for c in $red; do bisect "$c"; done
echo "novac-fuzz: FAIL — паник $bad из $n (репро в $T/keep.*)" >&2
exit 1
