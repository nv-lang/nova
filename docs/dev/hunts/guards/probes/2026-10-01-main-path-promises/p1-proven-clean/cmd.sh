#!/usr/bin/env bash
# p1 -- CLEAN control for check-push-proven-by-ci.py: all seven required workflows
# green on the sha, empty accepted list. Expected: ok 7/7, rc=0.
# Second step: the pure measure "how the guard answers today" -- same green runs,
# but the REAL registry and the REAL ci-accepted-red.list (defaults, read-only).
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-push-proven-by-ci.py"
PY=python3; "$PY" -c 'print(1)' >/dev/null 2>&1 || PY=python
SHA=1111111111111111111111111111111111111111
[ -f "$G" ] || { echo "no guard at $G (run from the repository root)"; exit 2; }
: > "$HERE/acc-empty.list"
echo '{}' > "$HERE/jobs-empty.json"
echo "=== A. mini tree: all green, empty accepted list"
"$PY" "$G" "$SHA" --runs-json "$HERE/runs-green.json" --jobs-json "$HERE/jobs-empty.json" \
    --registry "$HERE/registry.md" --accepted "$HERE/acc-empty.list" 2>&1; echo "rc=$?"
echo "=== B. same green runs, REAL registry + REAL accepted list (read-only measure)"
"$PY" "$G" "$SHA" --runs-json "$HERE/runs-green.json" --jobs-json "$HERE/jobs-empty.json" 2>&1; echo "rc=$?"
