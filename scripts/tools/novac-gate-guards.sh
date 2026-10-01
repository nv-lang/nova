#!/bin/sh
# scripts/tools/novac-gate-guards.sh -- every guard scripts/gate-novac.sh names,
# run one by one, minus the heavy ones (SKIP below); one line per guard, the
# first lines of each failure, and a tally.
#
# WHY IT EXISTS (2026-10-01, Carina window and the integrator, the same lesson
# twice in one night): a hand-picked list of "cheap guards" missed four that
# CI runs, and a merge checked without the full set reddened novac-gate on four
# points. The list is READ from gate-novac.sh itself, so it cannot drift from
# what the gate runs: a guard added there is run here the same day.
#
# It is NOT the gate: the heavy guards (differential, module tests, fuzz,
# fixed points, clean build, batch) are skipped and counted in the tally line,
# and a commit-message guard (check-novac-commit-no-simplification) judges the
# last commit, so its FAIL here is about that commit, not about the tree.
#
# Usage: sh scripts/tools/novac-gate-guards.sh [tree]   (default: .)
# Run time on 2026-10-01: about 13 minutes for 98 guards on a busy machine.
R=${1:-.}
cd "$R" || exit 2
# Run artefacts go to D: (owner's rule 2026-09-18); an exported TEMP wins.
: "${TMPDIR:=/d/Temp}"
export TEMP="${TEMP:-D:\\Temp}" TMP="${TMP:-D:\\Temp}" TMPDIR
SKIP="check-novac-differential.sh check-novac-module-tests.sh check-novac-fuzz-zero-panic.sh check-novac-selftest-proves-red.sh check-novac-iteration-cost.sh check-novac-mangle-fixed-point.sh check-novac-template-fixed-point.sh check-novac-clean-build.sh check-novac-batch.sh"
list=$(grep -oE '(par_add|guard( --deadline [0-9]+)?) "\$ROOT/scripts/guards/[A-Za-z0-9_.-]+' scripts/gate-novac.sh \
    | sed -E 's#.*scripts/guards/##' | sort -u)
[ -n "$list" ] || { echo "novac-gate-guards: no guard found in scripts/gate-novac.sh -- the pattern lost its target"; exit 3; }
n=0; bad=0; skipped=0
for g in $list; do
    case " $SKIP " in *" $g "*) skipped=$((skipped+1)); continue ;; esac
    [ -f "scripts/guards/$g" ] || { echo "MISSING $g"; bad=$((bad+1)); continue; }
    n=$((n+1))
    case $g in
        *.py) out=$(timeout 240 python "scripts/guards/$g" . 2>&1); rc=$? ;;
        *) out=$(timeout 240 bash "scripts/guards/$g" . 2>&1); rc=$? ;;
    esac
    if [ $rc -eq 124 ]; then
        bad=$((bad+1)); echo "СНЯТ ПРЕДЕЛОМ 240с: вердикта нет $g"
    elif [ $rc -ne 0 ]; then
        bad=$((bad+1)); echo "FAIL($rc) $g"; echo "$out" | head -4 | cut -c1-200
    else
        echo "ok $g"
    fi
done
echo "gate guards: $n run, $bad failed, $skipped heavy skipped"
[ $bad -eq 0 ]
