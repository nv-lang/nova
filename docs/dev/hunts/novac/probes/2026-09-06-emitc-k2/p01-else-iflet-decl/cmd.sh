#!/bin/sh
# run from the nova worktree root. Two questions, in order: does the subset accept the form
# (rc of `novac emit`), and what does the C after `else` look like -- a declaration bare after
# `else` is the defect (#991), a block `else {` is the fix. The smoke then compiles the C:
# the defect is visible ONLY at that stage (`novac emit` exits 0 on invalid C).
P=docs/dev/hunts/novac/probes/2026-09-06-emitc-k2/p01-else-iflet-decl
novac/target/novac.exe emit "$P/probe.nv" > "${TMPDIR:-/tmp}/p01.c" 2>&1; echo "novac emit rc=$?"
if grep -q -E '^\s*else\s+Nova[A-Za-z_]+ _novac_tmp_t[0-9]+ = ' "${TMPDIR:-/tmp}/p01.c"; then
    echo "DEFECT: a declaration printed bare after else (invalid C):"
    grep -n -E '^\s*else\s+Nova' "${TMPDIR:-/tmp}/p01.c" | head -2
else
    echo "fixed form: the chained if-let is braced --"
    grep -n -A1 -E '^\s*else \{' "${TMPDIR:-/tmp}/p01.c" | head -4
fi
bash scripts/tools/novac-e1-smoke.sh "$P/probe.nv" 2>&1 | tail -1
