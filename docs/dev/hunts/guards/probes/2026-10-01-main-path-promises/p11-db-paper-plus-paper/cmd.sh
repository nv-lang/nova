#!/usr/bin/env bash
# p11 -- check-main-docs-batched.sh: a lone registry row stops being "одиночный"
# as soon as ANY other path rides along -- including another paper file.
#
# Header promise, line 2: "одиночный «бумажный» коммит в main запрещён."
# Header, lines 23-26: "Коммит, где рядом с бумагой есть хоть один другой путь
# (код, фикстура, спека), страж не судит ... строка реестра вместе со своей
# починкой — ровно то, чего от коммита и ждут."
# Code, line 62: `*) pass "в коммите есть путь вне бумажного набора ($p)"` --
# ANY path outside the four patterns, not "код, фикстура, спека".
#
# Built in work/ (throw-away repo, main tree on main, real guard).
# A: control -- registry + handoff (both in the set)          -> expect FAIL
# B: registry + docs/plans/README.md (a plan index, paper)      -> ?
# C: registry + docs/dev/hunts/x/LEDGER.md (paper)              -> ?
# D: --only check: code staged in the index, then
#    `git commit --only -- docs/plans/221.1-bug-sweep.md` through the REAL
#    pre-commit (AGENTS.md mandates --only)                     -> expect refusal
#    (control that the hook sees the temporary index, not the real one)
# E: read-only measure over the real history of main (first-parent, non-merge,
#    since 2026-09-29): commits the set judges vs docs-only commits it calls
#    "not ours" (classify_commits.py, here).
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-main-docs-batched.sh"
PC="$ROOT/scripts/githooks/pre-commit"
PY=python3; "$PY" -c 'print(1)' >/dev/null 2>&1 || PY=python
for f in "$G" "$PC" "$HERE/classify_commits.py"; do [ -s "$f" ] || { echo "MISSING $f"; exit 2; }; done
unset NOVA_DOCS_BATCH
W="$HERE/work"; rm -rf "$W"; mkdir -p "$W"
gitp() { git -c user.name=probe -c user.email=probe@example.invalid "$@"; }
git -C "$W" init -q -b main; git -C "$W" config core.autocrlf false
mkdir -p "$W/docs/plans" "$W/docs/dev/prompts" "$W/docs/dev/hunts/x" "$W/std/src" "$W/scripts/githooks" "$W/scripts/guards"
for f in docs/plans/221.1-bug-sweep.md docs/plans/README.md docs/dev/prompts/integrator-handoff.md docs/dev/hunts/x/LEDGER.md std/src/a.nv; do echo base > "$W/$f"; done
cp "$PC" "$W/scripts/githooks/pre-commit"; cp "$G" "$W/scripts/guards/"
git -C "$W" add -- docs std scripts
gitp -C "$W" -c core.hooksPath=/dev/null commit -q -m base
[ -n "$(git -C "$W" rev-parse -q --verify HEAD)" ] || { echo "MISSING base"; exit 2; }
st() { for f in "$@"; do echo x >> "$W/$f"; git -C "$W" add -- "$f"; done; }
reset() { git -C "$W" read-tree HEAD; git -C "$W" checkout -q -- .; }
echo "=== A. control: registry + integrator-handoff"
st docs/plans/221.1-bug-sweep.md docs/dev/prompts/integrator-handoff.md; bash "$G" "$W" 2>&1 | head -1; reset
echo "=== B. registry + docs/plans/README.md"
st docs/plans/221.1-bug-sweep.md docs/plans/README.md; bash "$G" "$W" 2>&1; echo "rc=$?"; reset
echo "=== C. registry + docs/dev/hunts/x/LEDGER.md"
st docs/plans/221.1-bug-sweep.md docs/dev/hunts/x/LEDGER.md; bash "$G" "$W" 2>&1; echo "rc=$?"; reset
echo "=== D. code staged; git commit --only -- registry (real pre-commit)"
git -C "$W" config core.hooksPath scripts/githooks
st std/src/a.nv; echo y >> "$W/docs/plans/221.1-bug-sweep.md"
gitp -C "$W" commit -q -m "registry row" --only -- docs/plans/221.1-bug-sweep.md 2>&1 | head -2
echo "   commits on main now: $(git -C "$W" rev-list --count main) (1 = refused)"
git -C "$W" config --unset core.hooksPath; reset
echo "=== E. real history of main since 2026-09-29 (read-only)"
"$PY" "$HERE/classify_commits.py" "$ROOT" 2026-09-29
