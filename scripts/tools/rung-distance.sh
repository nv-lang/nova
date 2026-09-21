#!/usr/bin/env bash
# scripts/tools/rung-distance.sh -- how far Carina is from the rung she is on,
# in one output instead of four.
#
# WHY IT EXISTS. "Where are we" was assembled by hand from four different
# commands, and assembled again by the next window, and again the day after.
# On 2026-09-20 two windows spent about an hour between them reconstructing
# numbers that already existed. This prints them together.
#
# IT IS A MEASURE, NOT A GUARD, and it lives in scripts/tools for that reason:
# it reddens nothing and has no baseline. A guard says "you broke something";
# this says "here is where you stand". Mixing the two produces a guard nobody
# can keep green and a measure nobody trusts.
#
# THE ONE RULE IT KEEPS, and it is the lesson of the day it was written: a
# number is printed WITH the age and the provenance of the run that produced
# it, or it is not printed at all. The expensive measure (the differential
# corpus, ~5.5 minutes) is NOT run here, and it is NOT guessed from whatever
# log happens to be newest -- the first version of this script did exactly that
# and printed a fourteen-day-old number from another tree on its first run.
# Either a run recorded its verdict deliberately, or the line says NOT MEASURED
# and names the command. A measure that hides how old it is does not measure
# its subject; one that invents a provenance is worse, because it looks fresh.
#
# Usage: bash scripts/tools/rung-distance.sh [repo-root]
set -u
ROOT="${1:-.}"
cd "$ROOT" || exit 2

NOVAC="novac/target/novac.exe"
[ -x "$NOVAC" ] || NOVAC="novac/target/novac"

echo "=== RUNG DISTANCE -- $(date '+%Y-%m-%d %H:%M') ==="
# THE LADDER IS READ, NOT RESTATED. Its home is plan 274; a copy here would
# diverge on the first edit of either. Only the acceptance of each rung is
# quoted from there, and the quote names its source so a reader can check it.
LADDER=docs/plans/274-novac-self-hosted-compiler.md
if [ -f "$LADDER" ]; then
    echo "ladder: $LADDER (section on the version ladder)"
else
    echo "ladder: $LADDER NOT FOUND -- the rung names below are unanchored"
fi
echo

# --- rung 0.2: does Carina compile herself -----------------------------------
echo "--- 0.2  Carina compiles herself ---"

if [ -x "$NOVAC" ]; then
    newest=$(find novac/src -name '*.nv' -newer "$NOVAC" 2>/dev/null | head -1)
    if [ -n "$newest" ]; then
        echo "  binary:        STALE -- $newest is newer than $NOVAC"
        echo "                 (every number below about Carina is about the PREVIOUS compiler)"
    else
        echo "  binary:        fresh, $(ls -l "$NOVAC" | awk '{print $5}') bytes"
    fi
else
    echo "  binary:        ABSENT -- nothing below was measured against Carina"
fi

# NOT READ FROM A LOG. The first version of this script took "the newest file
# matching *differential*" and printed a fourteen-day-old number from another
# tree. The date line made it visible instead of believed, but a guessed
# provenance is not a measurement: either a run recorded its verdict here
# deliberately, or the number is absent and the command is named.
if [ -f target/remainder-verdict.txt ]; then
    echo "  remainder:     $(head -1 target/remainder-verdict.txt)"
    echo "                 recorded $(date -r target/remainder-verdict.txt '+%Y-%m-%d %H:%M'), commit $(sed -n 2p target/remainder-verdict.txt)"
    # CAVEAT ADDED 2026-09-21, after this line was trusted for a whole shift:
    # the self-distance count (the "N of 99" figure inside the verdict above)
    # comes from a BATCH check over all of novac/src
    # (NOVAC_SELF_PATH=novac/src), which ICEs (types.nv:244, an interner
    # bound; minimal repro: `NOVAC_SELF_PATH=novac/src novac check
    # novac/src/check/binds.nv`, registry TBD). The measure's own fallback on
    # that crash is a PER-FILE loop, and a file checked alone cannot see a
    # sibling file's declarations -- so most of what it counts as
    # "undeclared" is a type declared two files over, not real subset debt.
    # The number above is honest about ITS OWN age and source; it is not
    # honest about what the SOURCE run actually measured, because nobody
    # knew yet that the source run's measure was itself broken.
    echo "                 CAVEAT: if this verdict's self-distance count came from"
    echo "                 the per-file fallback (the batch ICEs -- see registry"
    echo "                 TBD, types.nv:244), most of what it counts is a name"
    echo "                 declared in a SIBLING file, not real subset debt."
else
    echo "  remainder:     NOT MEASURED -- and deliberately not measured from here:"
    echo "                 bash scripts/guards/check-novac-differential.sh .   (~5.5 min)"
    echo "                 To make it appear here, that run records its verdict line"
    echo "                 and HEAD into target/remainder-verdict.txt."
fi

# THE ACCEPTANCE OF 0.2, and the one number that must never read as a boolean.
# "Has it run" becomes permanently true an hour after the first run; what the
# reader needs is WHEN and ON WHAT.
if [ -f target/double-build-verdict.txt ]; then
    echo "  double build:  $(head -1 target/double-build-verdict.txt)"
    echo "                 age $(( ( $(date +%s) - $(date -r target/double-build-verdict.txt +%s) ) / 3600 ))h, commit $(sed -n 2p target/double-build-verdict.txt), HEAD now $(git rev-parse --short HEAD 2>/dev/null)"
else
    echo "  double build:  NEVER RUN in this tree."
    echo "                 It IS the acceptance of 0.2, so every other number on this"
    echo "                 line is about progress toward an acceptance nobody has taken."
fi
echo
# --- rung 0.3: does Carina compile everything else ---------------------------
echo "--- 0.3  Carina compiles the packages and the examples ---"
if [ ! -x "$NOVAC" ]; then
    echo "  examples:      skipped, no binary"
else
    files=$(ls examples/*/*.nv examples/*.nv 2>/dev/null)
    n=0; ok_c=0; ok_e=0
    for f in $files; do
        n=$((n + 1))
        "$NOVAC" check "$f" >/dev/null 2>&1 && ok_c=$((ok_c + 1))
        "$NOVAC" emit  "$f" >/dev/null 2>&1 && ok_e=$((ok_e + 1))
    done
    echo "  examples:      check $ok_c/$n   emit $ok_e/$n   refused $((n - ok_e))"
    echo "                 measured with $NOVAC, $(date -r "$NOVAC" '+%Y-%m-%d %H:%M')"
    if [ "$ok_c" != "$ok_e" ]; then
        echo "                 THE TWO DOORS DISAGREE -- that difference is itself the finding:"
        echo "                 a file the checker accepts and the emitter refuses is a gap"
        echo "                 between them, not a gap in the subset."
    fi
fi
echo "  packages:      NOT MEASURED -- no command exists yet. 0.3 says 'all packages'"
echo "                 and only the examples half is counted, so the number above is"
echo "                 an upper bound on how close we are, never a verdict."
echo

# --- the cheap debt numbers ---------------------------------------------------
echo "--- subset debt (cheap, measured now) ---"
python scripts/guards/check-novac-subset-debt-dated.py . 2>&1 | tail -1
echo
echo "NOTE: the self-build remainder counts OUR OWN source, so it says nothing"
echo "about 0.3 -- a legal form we never write contributes nothing to it, even at"
echo "zero. The examples line is the only number here that moves toward 0.3."
