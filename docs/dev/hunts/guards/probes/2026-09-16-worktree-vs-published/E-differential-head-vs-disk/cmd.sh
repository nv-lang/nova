#!/usr/bin/env bash
# Probe E -- READ-ONLY measurement on the real tree. Nothing is created,
# edited, staged or removed.
#
# check-novac-differential.sh:198-207 guards "the corpus ratchet may only
# grow": it compares `git show HEAD:scripts/guards/novac-corpus.baseline`
# with the SAME file read off the disk. The two objects differ only while a
# lowering sits uncommitted -- and the sanctioned way to run the gate
# (scripts/tools/push-after-gate.sh) REFUSES to start on a dirty tree, so at
# the moment the check runs the two sides are the same bytes by construction.
set -u
REPO="${NOVA_REPO:-$(git rev-parse --show-toplevel 2>/dev/null || echo "$PWD")}"
cd "$REPO" || exit 2

echo "== 1. the two objects the monotonicity block compares =="
B=scripts/guards/novac-corpus.baseline
git show "HEAD:$B" > "$TMPDIR/hb.$$" 2>/dev/null || { echo "no HEAD copy"; exit 2; }
if cmp -s "$TMPDIR/hb.$$" "$B"; then
    echo "   HEAD:$B  ==  worktree $B   (identical bytes)"
    echo "   -> the 'may only grow' comparison has no input at all right now."
else
    echo "   they differ -- the tree carries an uncommitted baseline edit:"
    diff "$TMPDIR/hb.$$" "$B" | head -10 | sed 's/^/      /'
fi
rm -f "$TMPDIR/hb.$$"

echo
echo "== 2. why that is the steady state, not an accident =="
grep -n 'git diff --name-only HEAD' scripts/tools/push-after-gate.sh | sed 's/^/   /'
grep -n 'push-after-gate: derevo\|ne chistoe' scripts/tools/push-after-gate.sh >/dev/null 2>&1
sed -n '/^DIRTY=/,/^fi$/p' scripts/tools/push-after-gate.sh | sed 's/^/   /'
echo
echo "   The gate only ever runs with HEAD == worktree; a lowering that is"
echo "   already in a commit compares equal to itself and passes forever."
echo
echo "== 3. the two addresses that answer the same question differently =="
grep -n 'HEAD_B=\|BFILE=\|now\" -lt \"\$was' scripts/guards/check-novac-differential.sh | sed 's/^/   /'
