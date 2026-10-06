#!/bin/sh
# scripts/tools/double-build.sh -- the S5 acceptance measure named in
# docs/dev/novac-bootstrap.md §5 (Carina 0.2, plan 274 "Лестница версий"):
#
#   A = novac, built by the oracle (as scripts/gate-novac.sh builds it)
#   B = novac, built by A
#   C = novac, built by B
#   accept: B and C byte-identical (and B.c, C.c -- the C each one was built from)
#
# "Built by X" means: `NOVAC_SELF_PATH=novac/src X emit novac/src/main.nv > Y.c`
# (the program mode of `novac emit`, task #23 / plan 274.11 E.10 step 2b), then
# clang with the argv and the runtime PCH that scripts/tools/novac-e1-smoke.sh
# captures from the oracle's build of the shell probe -- the SAME door the E1
# smoke and the differential link through, so a B that this script builds is
# the binary those tools would build, not a cousin of it.
#
# HISTORY. Until 2026-10-06 this file was only the check-half: it measured the
# precondition "does novac accept its own full source under `check`" and wrote
# "emit+build+compare not attempted". The precondition reached 0 rejects on
# 2026-10-06 (f2b6517c7; held since by scripts/guards/check-novac-self-accepted.py
# against novac-self-accepted.baseline), so the emit-half is written now, with
# the sabotage proof in both directions (task #24).
#
# STAGES and the ONE line that names where the chain stopped (verdict, line 1
# of target/double-build-verdict.txt -- the format rung-distance.sh reads):
#   1. A: oracle `build novac/src/main.nv`            -> "не собралась A"
#   2. precondition: A `check` on its own source      -> "S5 PRECONDITION NOT MET"
#   3. argv + PCH: novac-e1-smoke.sh --prepare        -> "нет argv/PCH"
#   4. B: emit by A -> clang -c -> link               -> "не собралась B (<step>)"
#   5. C: emit by B -> clang -c -> link               -> "не собралась C (<step>)"
#   6. link determinism: B.o linked twice must match  -> "линковка недетерминирована"
#   7. compare B.c/C.c and B/C byte for byte          -> "B ≠ C" or "B ≡ C"
# Every compare prints both file names, both sizes and the first differing
# byte (cmp's own words beside it) -- the proof plan 274 asks for is that
# output, not the word "сошлось".
#
# WHY STAGE 6. B and C are compared as files; if the linker stamps the time
# (lld-link does unless /Brepro) two links of ONE object differ and "B ≠ C"
# would be the linker's, not Carina's. The script adds /Brepro for the msvc
# target and then PROVES determinism by linking B.o twice: a comparison whose
# noise floor was never measured cannot redden "по делу".
# Measured 2026-10-06 on one object: without /Brepro two links differ at byte
# 129 (TimeDateStamp); with it they STILL differ, because lld-link writes the
# output's base name into the image (r1.exe vs r2.exe) and /Brepro makes the
# stamp a hash of the content. Hence every stage links to <stage>/novac$X --
# one base name, different directories -- and then two links are identical.
#
# SABOTAGE (the proof in both directions, docs/dev/test-conventions.md):
#   DOUBLE_BUILD_SABOTAGE=c-text  one byte of C.c is changed AFTER C is built;
#   DOUBLE_BUILD_SABOTAGE=c-exe   one byte of the C binary is changed after it
#                                 is linked (and after stage 6).
# Either must turn the verdict into "B ≠ C" naming the file and the position.
# `--compare X Y` runs the comparison alone on two existing files (the same
# function stage 7 uses) -- the cheap half of the proof, no build.
#
# Knobs: DOUBLE_BUILD_TREE -- the tree whose novac/src is built (default: this
# one; read only, everything is written under THIS tree's target/double-build).
# DOUBLE_BUILD_A -- an existing A binary to use instead of building one.
# DOUBLE_BUILD_DEADLINE -- seconds per heavy step (default 900).
#
# Usage: sh scripts/tools/double-build.sh            (heavy: ~3 novac builds;
#        run it through peer_watch {machine: true}, never beside a gate)
#        sh scripts/tools/double-build.sh --compare X Y
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT" || exit 2

# ---- the comparison: one function, used by stage 7 and by --compare ------
# Prints one line per pair; returns 0 when identical, 1 otherwise.
compare_pair() {
    _c_a="$1"; _c_b="$2"
    _c_sa=$(wc -c < "$_c_a" | tr -d '[:space:]')
    _c_sb=$(wc -c < "$_c_b" | tr -d '[:space:]')
    _c_out=$(cmp "$_c_a" "$_c_b" 2>&1)
    _c_rc=$?
    if [ "$_c_rc" -eq 0 ]; then
        echo "double-build: IDENTICAL $_c_a ($_c_sa bytes) == $_c_b ($_c_sb bytes)"
        return 0
    fi
    # first differing byte: cmp says "differ: byte N" or "EOF on X after byte N"
    _c_pos=$(printf '%s\n' "$_c_out" | sed -n 's/.*differ: \(char\|byte\) \([0-9]*\).*/\2/p; s/.*EOF on .* after byte \([0-9]*\).*/\1+1 (EOF)/p' | head -n 1)
    [ -n "$_c_pos" ] || _c_pos="?"
    echo "double-build: DIFFER $_c_a ($_c_sa bytes) != $_c_b ($_c_sb bytes): first difference at byte $_c_pos -- cmp: $_c_out"
    return 1
}

if [ "${1:-}" = "--compare" ]; then
    [ -f "${2:-}" ] && [ -f "${3:-}" ] || { echo "usage: double-build.sh --compare FILE1 FILE2" >&2; exit 2; }
    compare_pair "$2" "$3"
    exit $?
fi

. "$ROOT/scripts/guards/lib/novac.sh"
TREE="${DOUBLE_BUILD_TREE:-$ROOT}"
TREE="$(cd "$TREE" 2>/dev/null && pwd)" || { echo "double-build: FAIL -- DOUBLE_BUILD_TREE=$DOUBLE_BUILD_TREE is not a directory" >&2; exit 1; }
VERDICT="$ROOT/target/double-build-verdict.txt"
mkdir -p "$ROOT/target"
# The commit of the tree that is BUILT; uncommitted novac/src is named, so a
# verdict taken on a working copy never reads as the commit's.
HEAD_SHORT="$(git -C "$TREE" rev-parse --short HEAD 2>/dev/null || echo unknown)"
[ -n "$(git -C "$TREE" status --porcelain -- novac/src 2>/dev/null)" ] && HEAD_SHORT="$HEAD_SHORT+uncommitted-novac/src"
[ "$TREE" = "$ROOT" ] || HEAD_SHORT="$HEAD_SHORT (tree $TREE)"
cd "$TREE" || exit 2

# verdict LINE EXIT -- write the one-line verdict where rung-distance.sh reads
# it, print it, leave with EXIT.
verdict() {
    printf '%s\n%s\n' "$1" "$HEAD_SHORT" > "$VERDICT"
    echo "double-build: VERDICT: $1"
    echo "double-build: verdict recorded in $VERDICT"
    exit "$2"
}

fail() {
    echo "double-build: FAIL -- $1" >&2
    exit 1
}

[ -f "$TREE/novac/src/main.nv" ] || fail "no novac/src/main.nv under $TREE"
DEADLINE="${DOUBLE_BUILD_DEADLINE:-900}"
W="$ROOT/target/double-build"
rm -rf "$W"
mkdir -p "$W/cache"
case "$(uname -s 2>/dev/null)" in
    MINGW*|MSYS*|CYGWIN*) X=.exe ;;
    *) X= ;;
esac
DL="$ROOT/scripts/tools/with-deadline.sh"
ORACLE="$(novac_find_oracle "$TREE" || true)"
[ -n "$ORACLE" ] || fail "no oracle (nova-cli/target/release/nova) for $TREE"
# A task tree has no GC of its own; the same door the gate's novac-build uses.
novac_borrow_main_gc "$TREE" "double-build:"
echo "double-build: tree = $TREE"
echo "double-build: oracle = $ORACLE"
echo "double-build: work dir = $W (A$X, B.c, B/novac$X, C.c, C/novac$X and every step's log stay there)"

# ---- 1. A: novac built by the oracle --------------------------------------
# A is built HERE, from the tree's source, not taken from novac/target: the
# chain starts at the oracle by definition (§5), and a binary lying in the
# tree may be older than its source (the stale-binary class of #1607).
if [ -n "${DOUBLE_BUILD_A:-}" ]; then
    cp "$DOUBLE_BUILD_A" "$W/A$X" || fail "DOUBLE_BUILD_A=$DOUBLE_BUILD_A cannot be copied"
    echo "double-build: A = $DOUBLE_BUILD_A (given, not built)"
else
    echo "double-build: building A: oracle build novac/src/main.nv (deadline ${DEADLINE}s)"
    _t0=$(date +%s)
    if ! (cd "$TREE" && bash "$DL" "$DEADLINE" "$ORACLE" build novac/src/main.nv -o "$W/A$X") > "$W/A.build.log" 2>&1 \
       || [ ! -f "$W/A$X" ]; then
        { grep -i -m 5 'fatal\|error' "$W/A.build.log"; tail -n 3 "$W/A.build.log"; } | sed 's/^/double-build:   /'
        verdict "не собралась A: the oracle did not build novac/src/main.nv (log $W/A.build.log)" 1
    fi
    echo "double-build:   built in $(( $(date +%s) - _t0 ))s"
fi
echo "double-build: A = $W/A$X ($(wc -c < "$W/A$X" | tr -d '[:space:]') bytes)"
NOVAC="$W/A$X"

# ---- 2. precondition: does A accept its OWN full source under `check`? ----
# Judged by A itself (the compiler about to emit), not by whatever binary lies
# in the tree. Same batch approach as novac-diff-corpus.sh and
# check-novac-self-accepted.py (one rung, one measure): one process,
# NOVAC_UNIT=1 (the module is checked as one text), NOVAC_SELF_PATH so
# relative diagnostics resolve, verdict per file from the "file" field each
# diagnostic carries. A dead/panicking batch is NOT trusted (its tail would
# read as false-clean) and falls back to one process per file -- a number from
# that fallback is marked UNTRUSTED (it false-rejects cross-file references
# inside a module).
#
# NOVAC_SELF_PATH goes BEFORE `timeout`, not between it and the target command:
# `timeout` does not parse `VAR=val` as an assignment (registry #1310; guard
# check-no-env-after-timeout.sh holds the form).
self_files=""
self_total=0
for f in "$TREE"/novac/src/*/*.nv "$TREE"/novac/src/*.nv; do
    [ -f "$f" ] || continue
    self_total=$((self_total + 1))
    self_files="$self_files \"$f\""
done
[ "$self_total" -gt 0 ] || fail "no .nv files found under novac/src -- wrong tree?"

T="${TMPDIR:-/tmp}/double-build.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

eval "NOVAC_UNIT=1 NOVAC_SELF_PATH=novac/src timeout 60 \"$NOVAC\" check $self_files" > "$T/self.out" 2> "$T/self.err" </dev/null
rc=$?
if [ "$rc" -eq 124 ]; then
    echo "double-build: батч СНЯТ ПРЕДЕЛОМ 60с (rc=124) -- вердикта нет, это не «все файлы плохи»" >&2
fi
if grep -q "E_NOVAC_ICE" "$T/self.out" "$T/self.err" 2>/dev/null; then
    rc=99
fi
if [ "$rc" -le 2 ]; then
    self_rej=$(cat "$T/self.out" "$T/self.err" 2>/dev/null \
        | grep -o '"file":"[^"]*"' | sort -u | wc -l | tr -d '[:space:]')
else
    echo "double-build: batch died (rc=$rc) -- falling back to one process per file" >&2
    FELL_BACK=1
    self_rej=0
    timed_out=0
    for f in "$TREE"/novac/src/*/*.nv "$TREE"/novac/src/*.nv; do
        [ -f "$f" ] || continue
        timeout 10 "$NOVAC" check "$f" >/dev/null 2>&1 </dev/null
        frc=$?
        if [ "$frc" -eq 124 ]; then
            timed_out=$((timed_out + 1))
            echo "double-build: SNYAT PREDELOM 10s (rc=124), не отказ проверки: $f" >&2
        fi
        [ "$frc" -eq 0 ] || self_rej=$((self_rej + 1))
    done
    if [ "$timed_out" -gt 0 ]; then
        echo "double-build: $timed_out файл(ов) снято пределом, а не отвергнуто -- считаются отвергнутыми за неимением вердикта, но это не одно и то же" >&2
    fi
fi
self_acc=$((self_total - self_rej))

echo "double-build: precondition -- A check on its own source: accepted $self_acc/$self_total"

if [ "${FELL_BACK:-0}" = 1 ]; then
    FALLBACK_NOTE=" -- UNTRUSTED NUMBER: batch mode died, per-file fallback false-rejects cross-file references within one module"
else
    FALLBACK_NOTE=""
fi
if [ "$self_rej" -gt 0 ]; then
    verdict "S5 PRECONDITION NOT MET: A (novac by the oracle) check accepts only $self_acc/$self_total of its own source$FALLBACK_NOTE; emit+build+compare not attempted" 1
fi

# ---- 3. clang argv and the runtime PCH, through the smoke's own door -------
# A private, fresh cache: the shared smoke cache is keyed by the oracle's stamp
# and carries paths of whichever tree filled it first (seen 2026-10-06: argv
# pointing into another task's worktree). Fresh means exactly one argv/PCH set.
echo "double-build: capturing clang argv + PCH (novac-e1-smoke.sh --prepare)"
if ! (cd "$TREE" && NOVAC_BIN="$W/A$X" NOVAC_SMOKE_CACHE="$W/cache" \
        bash "$DL" "$DEADLINE" sh "$TREE/scripts/tools/novac-e1-smoke.sh" --prepare) > "$W/prepare.log" 2>&1; then
    tail -n 5 "$W/prepare.log" | sed 's/^/double-build:   /'
    verdict "нет argv/PCH: novac-e1-smoke.sh --prepare failed (log $W/prepare.log)" 1
fi
LINKCMD=$(ls "$W"/cache/link-*.argv 2>/dev/null | head -n 1)
CFLAGS=$(ls "$W"/cache/cflags-*.argv 2>/dev/null | head -n 1)
PCH=$(ls "$W"/cache/prelude-*.pch 2>/dev/null | head -n 1)
[ -f "$LINKCMD" ] && [ -f "$CFLAGS" ] && [ -f "$PCH" ] \
    || verdict "нет argv/PCH: --prepare left no link/cflags/pch in $W/cache" 1
if grep -q 'windows-msvc' "$LINKCMD"; then
    # lld-link stamps the link time into the PE header unless /Brepro; see
    # "WHY STAGE 6" in the header. Measured 2026-10-06: one object linked twice
    # without it differs at byte 129 (the PE TimeDateStamp). Spelled with a
    # DASH: MSYS rewrites the argument `/Brepro` into the path
    # `C:/Program Files/Git/Brepro` and lld-link fails "could not open".
    printf '%s\n' "-Wl,-Brepro" >> "$LINKCMD"
fi
if command -v cygpath >/dev/null 2>&1; then
    REAL_CLANG="${NOVA_CLANG:-C:/Program Files/LLVM/bin/clang.exe}"
else
    REAL_CLANG="${NOVA_CLANG:-$(command -v clang || printf 'clang')}"
fi

# link_obj OBJ EXE -- the oracle's link argv over one object.
link_obj() {
    eval "\"$REAL_CLANG\" $(tr '\n' ' ' < "$LINKCMD") -o \"$2\" \"$1\""
}

# build_stage NAME BY -- NAME = novac built by compiler BY:
# emit the whole program, drop the prelude include (it IS the PCH), clang -c
# against the PCH, link. Any failure ends the run with "не собралась NAME".
build_stage() {
    _n="$1"; _by="$2"
    echo "double-build: building $_n: NOVAC_SELF_PATH=novac/src $_by emit novac/src/main.nv"
    _t0=$(date +%s)
    (cd "$TREE" && NOVAC_SELF_PATH=novac/src bash "$DL" "$DEADLINE" "$_by" emit novac/src/main.nv) \
        > "$W/$_n.c" 2> "$W/$_n.emit.err" </dev/null
    _rc=$?
    if [ "$_rc" -ne 0 ]; then
        _nd=$(grep -c '"code"' "$W/$_n.c")
        echo "double-build:   emit rc=$_rc; stderr: $(head -c 400 "$W/$_n.emit.err")"
        echo "double-build:   diagnostics in stdout: $_nd, by code and by message:"
        grep -o '"code":"[A-Z_]*"' "$W/$_n.c" | sort | uniq -c | sort -rn | head -n 10 | sed 's/^/double-build:   /'
        grep -o '"message":"[^"]*"' "$W/$_n.c" | cut -c1-240 | sort | uniq -c | sort -rn | head -n 15 | sed 's/^/double-build:   /'
        verdict "не собралась $_n (emit by $(basename "$_by"), rc=$_rc, $_nd diagnostics; $W/$_n.c, $W/$_n.emit.err)" 1
    fi
    echo "double-build:   emitted $W/$_n.c ($(wc -c < "$W/$_n.c" | tr -d '[:space:]') bytes) in $(( $(date +%s) - _t0 ))s"
    grep -q '^#include "nova_rt/nova_rt.h"$' "$W/$_n.c" \
        || verdict "не собралась $_n (emit by $(basename "$_by") printed no prelude include: not a C program; $W/$_n.c)" 1
    sed '0,/^#include "nova_rt\/nova_rt.h"$/{//d}' "$W/$_n.c" > "$W/$_n.body.c"
    # -ferror-limit=0: the first run's job is to NAME every refusal (one
    # registry row per class), and clang stops at 20 errors by default.
    eval "\"$REAL_CLANG\" $(tr '\n' ' ' < "$CFLAGS") -ferror-limit=0 -include-pch \"$PCH\" -c \"$W/$_n.body.c\" -o \"$W/$_n.o\"" \
        > "$W/$_n.cc.log" 2>&1 \
        || { echo "double-build:   clang errors by class (quoted names folded to 'X'):"
             grep ' error: ' "$W/$_n.cc.log" | sed "s/^.*error: //; s/'[^']*'/'X'/g" | sort | uniq -c | sort -rn | head -n 15 | sed 's/^/double-build:   /'
             grep ' error: ' "$W/$_n.cc.log" | head -n 8 | sed 's/^/double-build:   /'
             verdict "не собралась $_n (clang -c failed: $(grep -c 'error:' "$W/$_n.cc.log") errors; $W/$_n.cc.log)" 1; }
    mkdir -p "$W/$_n"
    link_obj "$W/$_n.o" "$W/$_n/novac$X" > "$W/$_n.link.log" 2>&1 \
        || { head -n 8 "$W/$_n.link.log" | sed 's/^/double-build:   /'
             verdict "не собралась $_n (link failed; $W/$_n.link.log)" 1; }
    echo "double-build:   $_n = $W/$_n/novac$X ($(wc -c < "$W/$_n/novac$X" | tr -d '[:space:]') bytes)"
}

# flip_byte FILE -- change the byte in the middle of FILE (sabotage only).
flip_byte() {
    _f_size=$(wc -c < "$1" | tr -d '[:space:]')
    _f_off=$((_f_size / 2))
    _f_old=$(dd if="$1" bs=1 skip="$_f_off" count=1 2>/dev/null)
    _f_new=X; [ "$_f_old" = X ] && _f_new=Y
    printf '%s' "$_f_new" | dd of="$1" bs=1 seek="$_f_off" count=1 conv=notrunc 2>/dev/null
    echo "double-build: SABOTAGE: byte $((_f_off + 1)) of $1 changed to '$_f_new'"
}

build_stage B "$W/A$X"
build_stage C "$W/B/novac$X"

# ---- 6. the comparison's noise floor -------------------------------------
mkdir -p "$W/B.relink"
link_obj "$W/B.o" "$W/B.relink/novac$X" > "$W/B.relink.log" 2>&1 \
    || verdict "линковка недетерминирована: relinking B.o failed ($W/B.relink.log)" 1
compare_pair "$W/B/novac$X" "$W/B.relink/novac$X" \
    || verdict "линковка недетерминирована: B.o linked twice gives two different files -- B/C comparison would measure the linker" 1

case "${DOUBLE_BUILD_SABOTAGE:-}" in
    "") ;;
    c-text) flip_byte "$W/C.c" ;;
    c-exe) flip_byte "$W/C/novac$X" ;;
    *) fail "DOUBLE_BUILD_SABOTAGE=$DOUBLE_BUILD_SABOTAGE: expected c-text or c-exe" ;;
esac

# ---- 7. B against C -------------------------------------------------------
same=1
compare_pair "$W/B.c" "$W/C.c" || same=0
compare_pair "$W/B/novac$X" "$W/C/novac$X" || same=0
if [ "$same" -eq 1 ]; then
    verdict "B ≡ C: B/novac$X and C/novac$X byte-identical ($(wc -c < "$W/B/novac$X" | tr -d '[:space:]') bytes), B.c and C.c byte-identical ($(wc -c < "$W/B.c" | tr -d '[:space:]') bytes)" 0
fi
verdict "B ≠ C: see the DIFFER lines above (files in $W)" 1
