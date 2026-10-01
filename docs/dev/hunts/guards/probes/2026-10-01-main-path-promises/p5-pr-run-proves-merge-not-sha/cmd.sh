#!/usr/bin/env bash
# p5 -- check-push-proven-by-ci.py accepts pull_request runs as proof of the sha.
#
# Header, lines 17-18: "every REQUIRED workflow has a completed run on exactly
# this sha (any event: the `integrate` push, a PR, a dispatch)".
# Header, line 3: "main receives only a commit GitHub CI has judged".
#
# For a pull_request event GitHub records the PR HEAD as the run's head_sha
# (that is what `gh run list --commit <sha>` filters on), but actions/checkout
# without `ref:` checks out refs/pull/N/merge -- the PR head MERGED with the base
# branch as it stood at run time. None of the seven required workflows sets a
# `ref:` on checkout (the grep at the end prints nothing). So a PR run judged a
# tree that is NOT <sha>; if main moved after the run, the judged tree is not even
# what main will get after a fast-forward to <sha>.
#
# A: all seven required workflows green, every run event=pull_request -> ?
# (This probe shows only what the guard accepts; the GitHub-side fact about the
# merge ref is stated, not reproduced offline.)
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-push-proven-by-ci.py"
PY=python3; "$PY" -c 'print(1)' >/dev/null 2>&1 || PY=python
SHA=1111111111111111111111111111111111111111
for f in "$G" "$HERE/runs-pr-only.json" "$HERE/jobs.json" "$HERE/registry.md"; do
    [ -s "$f" ] || { echo "MISSING $f"; exit 2; }
done
[ -f "$HERE/acc-empty.list" ] || : > "$HERE/acc-empty.list"
echo "=== A. all runs are pull_request runs"
"$PY" "$G" "$SHA" --runs-json "$HERE/runs-pr-only.json" --jobs-json "$HERE/jobs.json" \
    --registry "$HERE/registry.md" --accepted "$HERE/acc-empty.list" 2>&1; echo "rc=$?"
echo "=== checkout steps with ref: override in the seven required workflows (expect none):"
for w in nova-gate crate-tests nova-lint nova-test-regression contracts-crosscheck contracts-z3 nova-doc; do
    grep -n -A4 'actions/checkout' "$ROOT/.github/workflows/$w.yml" | grep 'ref:' | sed "s|^|$w: |"
done
echo "(end of list)"
