#!/bin/sh
# scripts/tools/double-build.sh -- the S5 acceptance measure named in
# docs/dev/novac-bootstrap.md §5:
#
#   A = novac, built by the oracle (nova.exe)
#   B = novac, built by A
#   C = novac, built by B
#   accept: B and C byte-identical
#
# WHY THIS FILE DID NOT EXIST UNTIL 2026-09-21 EVEN THOUGH §5 HAS STOOD SINCE
# PLAN 274's EARLY DAYS: nobody could write the emit+link+run half honestly,
# because it cannot be exercised or verified before novac accepts its own
# FULL source under `check` -- and it does not yet (see PRECONDITION below).
# Writing untested emit/link/compare plumbing now would be exactly the
# "somehow it works" this project's own §6 forbids for open questions, and
# the same failure mode as any other half-finished implementation: code that
# looks like an answer to a question nobody has actually run.
#
# WHAT THIS SCRIPT DOES TODAY, HONESTLY: it measures and records the ONE
# precondition that gates every later step -- does novac accept its own full
# source under `check`? -- using the exact batch mechanism already proven in
# novac-diff-corpus.sh (one process over the whole tree, NOVAC_SELF_PATH,
# diagnostics grouped by their own "file" field; falls back to a per-file
# pass if the batch itself dies, same reasoning as there: a dead batch's
# tail reads as false-clean, not as untested). It writes
# target/double-build-verdict.txt in the format scripts/tools/rung-distance.sh
# already reads (line 1 = summary, line 2 = commit), so "double build: NEVER
# RUN in this tree" stops being the permanent answer.
#
# WHAT IT DELIBERATELY DOES NOT DO YET: attempt the actual emit -> clang
# compile -> link -> run -> byte-compare sequence for B and C. That half has
# a real, working single-file analogue (novac-e1-smoke.sh) whose argv/PCH
# machinery this script's next author should extend to the WHOLE self-source
# batch once the precondition below reads 0 rejects -- not before, because
# there would be nothing to run it against and no way to tell a real pass
# from a script that silently does nothing.
#
# KNOWN BROKEN, 2026-09-21 (Carina's window, registry 221.1 #TBD): the batch
# call this script and novac-diff-corpus.sh both rely on ICEs on this tree
# (`E_NOVAC_ICE types.nv:244: kind_of asked for a type id outside the
# interner`, isolated to `check/binds.nv`'s `@type_index`, unconfirmed
# mechanism -- possibly the self-declaration interner growing mid-typecheck).
# That means EVERY run so far has silently taken the per-file fallback below,
# and the fallback has its OWN distortion: each file is checked ALONE, so a
# type declared in a sibling file of the same novac module reads as
# "undeclared" -- a false rejection with nothing to do with real subset debt.
# CI's 83/99 and this script's own 11/99 are both numbers from a broken
# measure, not a subset-debt count. Do not trust either until the interner
# ICE is fixed (then the batch mode's whole-tree visibility becomes
# meaningful) or the fallback is changed to check by MODULE, not by file.
#
# Usage: sh scripts/tools/double-build.sh
# Cost: one novac process over ~99 files, a few seconds -- see
# novac-diff-corpus.sh's own comment on the same batch for the measured
# start-up-dominated cost (149ms cold start vs 50ms of real work per file).
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT" || exit 2
NOVAC="$ROOT/novac/target/novac.exe"
[ -x "$NOVAC" ] || NOVAC="$ROOT/novac/target/novac"
VERDICT="$ROOT/target/double-build-verdict.txt"
mkdir -p "$ROOT/target"

fail() {
    echo "double-build: FAIL -- $1" >&2
    exit 1
}

[ -x "$NOVAC" ] || fail "no novac binary (A) at $NOVAC -- build it first: nova build novac/src/main.nv -o novac/target/novac.exe"

echo "double-build: A = $NOVAC"

# ---- precondition: does A accept its OWN full source under `check`? -------
# Same batch approach as novac-diff-corpus.sh's self-build distance (credited
# there and here so the two never silently diverge in method): one process,
# NOVAC_SELF_PATH so relative diagnostics resolve, verdict per file taken
# from the "file" field each diagnostic carries. A dead/panicking batch is
# NOT trusted (its tail would read as false-clean for files after the death
# point -- the exact false-41/53 class novac-diff-corpus.sh's own comment
# names) and falls back to one process per file.
self_files=""
self_total=0
for f in "$ROOT"/novac/src/*/*.nv "$ROOT"/novac/src/*.nv; do
    [ -f "$f" ] || continue
    self_total=$((self_total + 1))
    self_files="$self_files \"$f\""
done
[ "$self_total" -gt 0 ] || fail "no .nv files found under novac/src -- wrong tree?"

T="${TMPDIR:-/tmp}/double-build.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

# NOVAC_SELF_PATH goes BEFORE `timeout`, not between it and the target
# command: `timeout` does not itself parse a `VAR=val` word among its own
# arguments as an environment assignment (that is a shell-parsing feature
# that only applies to words preceding the command name). The sibling
# `novac-diff-corpus.sh` carried exactly that bug from 2026-09-01 until
# registry 221.1 #1310 (2026-09-23): its one-process batch never ran, and its
# per-file fallback reported "ICE killed the batch" with no ICE at all. It is
# fixed there now and refuses rc=126/127 loudly; this note stays so the form
# is not copied back.
#
# NOVAC_UNIT=1 -- the module is checked as one text, the same measure the
# differential takes since 2026-09-23 (consensus of the Carina window and the
# integrator): self-build is a module build, and the per-file check produced
# harness artifacts (registry #1284, #1322). One rung, one measure -- the two
# tools must not disagree on what "Carina builds itself" means.
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
    for f in "$ROOT"/novac/src/*/*.nv "$ROOT"/novac/src/*.nv; do
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

echo "double-build: precondition -- novac check on its own source: accepted $self_acc/$self_total"

HEAD_SHORT="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"

if [ "${FELL_BACK:-0}" = 1 ]; then
    FALLBACK_NOTE=" -- UNTRUSTED NUMBER: batch mode ICEd (see this script's header), per-file fallback checks each file ALONE and false-rejects on cross-file references within the same novac module; this is not a real subset-debt count"
else
    FALLBACK_NOTE=""
fi

if [ "$self_rej" -gt 0 ]; then
    SUMMARY="S5 PRECONDITION NOT MET: novac check accepts only $self_acc/$self_total of its own source$FALLBACK_NOTE; emit+build+compare not attempted (would have nothing to run against)"
    printf '%s\n%s\n' "$SUMMARY" "$HEAD_SHORT" > "$VERDICT"
    echo "double-build: $SUMMARY"
    echo "double-build: verdict recorded in $VERDICT"
    exit 0
fi

# ---- precondition met: the actual A -> B -> C pipeline is NOT YET WRITTEN -
# See the file header: this is the extension point, not a silent no-op.
SUMMARY="PRECONDITION MET ($self_acc/$self_total self-check)$FALLBACK_NOTE BUT EMIT+BUILD+COMPARE IS NOT YET IMPLEMENTED -- see this script's header for what is missing and why"
printf '%s\n%s\n' "$SUMMARY" "$HEAD_SHORT" > "$VERDICT"
echo "double-build: $SUMMARY"
echo "double-build: verdict recorded in $VERDICT"
exit 0
