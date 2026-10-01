#!/usr/bin/env bash
# p12 -- check-push-proven-by-ci.py reads the jobs of a run ONLY when the run's
# own conclusion is not "success" (code lines 200-201: `if r.get("conclusion")
# == "success": continue`). Header item 2 (lines 20-21): "every job of those
# runs concluded success (or skipped)".
# A run can conclude success while a job in it concluded failure: a job with
# job-level `continue-on-error: true` (carrier: .github/workflows/nova-lint.yml
# line 128, the informational nova_tests debt job). Offline here: nova-lint run
# 103 is "success", its jobs file names one failed job -> does the guard see it?
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-push-proven-by-ci.py"
PY=python3; "$PY" -c 'print(1)' >/dev/null 2>&1 || PY=python
SHA=1111111111111111111111111111111111111111
for f in "$G" "$HERE/runs-green.json" "$HERE/jobs-103-has-failure.json" "$HERE/registry.md"; do
    [ -s "$f" ] || { echo "MISSING $f"; exit 2; }
done
: > "$HERE/acc-empty.list"
echo "=== nova-lint run 103 concluded success; its jobs list one failure"
"$PY" "$G" "$SHA" --runs-json "$HERE/runs-green.json" --jobs-json "$HERE/jobs-103-has-failure.json" \
    --registry "$HERE/registry.md" --accepted "$HERE/acc-empty.list" 2>&1; echo "rc=$?"
echo "=== continue-on-error carriers in the seven required workflows (job-level = 4 spaces):"
for w in nova-gate crate-tests nova-lint nova-test-regression contracts-crosscheck contracts-z3 nova-doc; do
    grep -n 'continue-on-error: true' "$ROOT/.github/workflows/$w.yml" | sed "s|^|$w.yml:|"
done
