#!/bin/sh
# run from the nova worktree root. The seam probe of #992: a deliberately broken emitter under a
# seam must either go red or get a verdict that NAMES the unchecked stage. Before aca8e57d5 the
# single switch NOVAC_CORPUS=0 silenced the behaviour smoke of the fixtures -- the only stage where
# a fixture's C is compiled -- while the guard's docstring promised "the fixtures remain", and the
# differential said `ok: 34 fixtures` over C that clang refused. After the split, NOVAC_SMOKE=0 and
# NOVAC_CORPUS=0 each name the stage they remove. This probe prints what the seams say today; the
# red side (the broken emitter itself) is documented in note.txt with its verbatim verdicts.
NOVAC_SMOKE=0 NOVAC_CORPUS=0 sh scripts/guards/check-novac-differential.sh 2>&1 | grep -v '^$' | cut -c1-200
echo "--- the seams the gate labels a run with (must include every NOVAC_*=0 a guard reads):"
grep -n -E 'SEAMS="\$SEAMS' scripts/gate-novac.sh | cut -c1-120
python scripts/guards/check-novac-seams-listed.py . 2>&1 | tail -1 | cut -c1-200
