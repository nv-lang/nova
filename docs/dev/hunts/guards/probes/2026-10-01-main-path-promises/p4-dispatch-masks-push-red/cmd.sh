#!/usr/bin/env bash
# p4 -- check-push-proven-by-ci.py: a later workflow_dispatch run of nova-gate
# HIDES a red push run on the same sha. The guard asks gh for `event` and never
# reads it; "the newest is judged" whatever triggered it.
#
# Why that is not the same proof: nova-gate.yml line 339 --
#   NOVA_GATE_TIER: ${{ github.event_name == 'schedule' && 'full' || (inputs.tier || 'push') }}
# and the dispatch input `tier` offers [push, full, loop]. A dispatch with
# tier=loop runs the job still NAMED "gate.sh tier push (...)" on the text-only
# tier (no compiler) -- and its green replaces the red push-tier verdict.
# The guard's own header, lines 6-10: "the heavy tier (mega-CU, crate tests,
# conformance-full, novac-gate) is GitHub CI ... moves `main` ... only when CI
# on that commit is green".
#
# A: control -- only the red push run           -> expected FAIL.
# B: red push run, then a green dispatch run    -> ?
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-push-proven-by-ci.py"
PY=python3; "$PY" -c 'print(1)' >/dev/null 2>&1 || PY=python
SHA=1111111111111111111111111111111111111111
for f in "$G" "$HERE/runs-push-red-only.json" "$HERE/runs-push-red-then-dispatch-green.json" "$HERE/jobs.json" "$HERE/registry.md"; do
    [ -s "$f" ] || { echo "MISSING $f"; exit 2; }
done
[ -f "$HERE/acc-empty.list" ] || : > "$HERE/acc-empty.list"
for r in runs-push-red-only.json runs-push-red-then-dispatch-green.json; do
    echo "=== $r"
    "$PY" "$G" "$SHA" --runs-json "$HERE/$r" --jobs-json "$HERE/jobs.json" \
        --registry "$HERE/registry.md" --accepted "$HERE/acc-empty.list" 2>&1; echo "rc=$?"
done
echo "=== the guard requests 'event' from gh and never reads it:"
grep -n '"event"\|event' "$G" | grep -v '^\s*#'
echo "=== nova-gate dispatch input and the tier it drives:"
grep -n 'options: \[push, full, loop\]\|NOVA_GATE_TIER:' "$ROOT/.github/workflows/nova-gate.yml"
