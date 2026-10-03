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
#
# THE LIMIT IS THE GATE'S, NOT ONE NUMBER (2026-10-02, integrator): a flat 240s
# cut the fixture-running guards (no-panic, diag-schema, no-cascade,
# fixture-expect) on every merge of the day -- the gate runs them in PARALLEL
# (`par_add`) with no 240s cap, and emitted-unique has `--deadline 300`. Each
# guard now takes its limit from gate-novac.sh: a `par_add` one 900s, a
# `guard --deadline N` one 2*N (the gate calibrates N upward under load), the
# rest 240s. Read from the gate, so the list cannot drift (the header above).
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
limit_of() {
    if grep -qF "par_add \"\$ROOT/scripts/guards/$1\"" scripts/gate-novac.sh; then echo 900; return; fi
    d=$(grep -oE "guard --deadline [0-9]+ .\\\$ROOT/scripts/guards/$1" scripts/gate-novac.sh | head -1 | awk '{print $3}')
    if [ -n "$d" ]; then echo $((d * 2)); else echo 240; fi
}
for g in $list; do
    case " $SKIP " in *" $g "*) skipped=$((skipped+1)); continue ;; esac
    [ -f "scripts/guards/$g" ] || { echo "MISSING $g"; bad=$((bad+1)); continue; }
    n=$((n+1))
    lim=$(limit_of "$g")
    case $g in
        # The commit-message guard takes <message-file> <root>, not a tree:
        # handed "." it died on Errno 13 (Carina window, 2026-10-01). It gets
        # the last commit's message, as the header above promises.
        check-novac-commit-no-simplification.py)
            msg="$TMPDIR/novac-gate-guards-msg.$$"
            git log -1 --format=%B > "$msg"
            out=$(timeout "$lim" python "scripts/guards/$g" "$msg" . 2>&1); rc=$?
            rm -f "$msg" ;;
        *.py) out=$(timeout "$lim" python "scripts/guards/$g" . 2>&1); rc=$? ;;
        *) out=$(timeout "$lim" bash "scripts/guards/$g" . 2>&1); rc=$? ;;
    esac
    if [ $rc -eq 124 ]; then
        bad=$((bad+1)); echo "СНЯТ ПРЕДЕЛОМ ${lim}с: вердикта нет $g"
    elif [ $rc -ne 0 ]; then
        bad=$((bad+1)); echo "FAIL($rc) $g"; echo "$out" | head -4 | cut -c1-200
    else
        echo "ok $g"
    fi
done
echo "gate guards: $n run, $bad failed, $skipped heavy skipped"
[ $bad -eq 0 ]
