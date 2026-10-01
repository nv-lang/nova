#!/usr/bin/env bash
# p6 -- the escape keys of the family ask for a registry row NUMBER and never
# look whether such a row exists. Inside ONE guard (check-push-proven-by-ci.py)
# the same number gets two answers: cited by ci-accepted-red.list it is judged
# against the registry ("does not exist" -> refusal), cited by the key it passes.
#
# Header promises:
#   check-push-proven-by-ci.py, line 43:  "ESCAPE: NOVA_PUSH_UNPROVEN="<reason
#     naming a registry row, e.g. #1234>" -- printed, and the reason must carry a
#     row number (a bare "1" is refused)."
#   check-main-no-direct-code.sh, lines 21-23: "КЛЮЧ: NOVA_MAIN_DIRECT_CODE=
#     "<причина #NNNN>" — номер строки реестра обязателен ... Голое «1» — отказ:
#     обход без причины неотличим от забывчивости."
#
# A: control, accepted list cites #00 -> "does not exist" refusal.
# B: same registry, key NOVA_PUSH_UNPROVEN="x #00" -> ?
# C: key NOVA_PUSH_UNPROVEN="#99999" (no reason text at all, no such row) -> ?
# D: check-main-no-direct-code on a git main tree with staged std/src code,
#    key NOVA_MAIN_DIRECT_CODE="#00" -> ?
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-push-proven-by-ci.py"
G2="$ROOT/scripts/guards/check-main-no-direct-code.sh"
PY=python3; "$PY" -c 'print(1)' >/dev/null 2>&1 || PY=python
SHA=1111111111111111111111111111111111111111
for f in "$G" "$G2" "$HERE/runs-red.json" "$HERE/jobs-red.json" "$HERE/registry.md" "$HERE/acc-00.list"; do
    [ -s "$f" ] || { echo "MISSING $f"; exit 2; }
done
: > "$HERE/acc-empty.list"
echo "=== A. control: accepted list cites #00"
"$PY" "$G" "$SHA" --runs-json "$HERE/runs-red.json" --jobs-json "$HERE/jobs-red.json" \
    --registry "$HERE/registry.md" --accepted "$HERE/acc-00.list" 2>&1; echo "rc=$?"
echo "=== B. key NOVA_PUSH_UNPROVEN='x #00'"
NOVA_PUSH_UNPROVEN='x #00' "$PY" "$G" "$SHA" --runs-json "$HERE/runs-red.json" --jobs-json "$HERE/jobs-red.json" \
    --registry "$HERE/registry.md" --accepted "$HERE/acc-empty.list" 2>&1; echo "rc=$?"
echo "=== C. key NOVA_PUSH_UNPROVEN='#99999'"
NOVA_PUSH_UNPROVEN='#99999' "$PY" "$G" "$SHA" --runs-json "$HERE/runs-red.json" --jobs-json "$HERE/jobs-red.json" \
    --registry "$HERE/registry.md" --accepted "$HERE/acc-empty.list" 2>&1; echo "rc=$?"
echo "=== D. check-main-no-direct-code, key NOVA_MAIN_DIRECT_CODE='#00'"
W="$HERE/work-repo"; rm -rf "$W"; mkdir -p "$W/std/src"
git -C "$W" init -q -b main; git -C "$W" config core.autocrlf false
echo base > "$W/std/src/a.nv"; git -C "$W" add -- std/src/a.nv
git -C "$W" -c user.name=probe -c user.email=probe@example.invalid -c core.hooksPath=/dev/null commit -q -m base
echo change >> "$W/std/src/a.nv"; git -C "$W" add -- std/src/a.nv
[ -n "$(git -C "$W" diff --cached --name-only)" ] || { echo "MISSING staged change"; exit 2; }
NOVA_MAIN_DIRECT_CODE='#00' bash "$G2" "$W" 2>&1; echo "rc=$?"
