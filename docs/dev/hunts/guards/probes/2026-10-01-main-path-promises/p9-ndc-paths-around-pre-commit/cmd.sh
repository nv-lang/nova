#!/usr/bin/env bash
# p9 -- check-main-no-direct-code.sh is wired ONLY into pre-commit, and judges
# "merge or not" ONLY by MERGE_HEAD. Which non-merge ways of putting code onto
# main in the MAIN tree does it see?
#
# Header promise, line 2: "код в main приезжает только слиянием."
# Header, lines 11, 15-16: "зовётся из scripts/githooks/pre-commit на КАЖДЫЙ
# коммит"; "слияние НЕ идёт (нет MERGE_HEAD: коммит, завершающий слияние, и есть
# законный путь кода в main)".
#
# Built in work/ (throw-away repo; REAL pre-commit + REAL guard copied in,
# core.hooksPath -> the copy). Branch `feat` carries a code commit on std/src.
# A: control -- plain `git commit` of code on main            -> expect refusal
# B: `git cherry-pick feat` on main                            -> ?
# C: `git revert --no-edit HEAD` of a code commit on main      -> ?
# D: `git merge --no-commit feat`, then the integrator ADDS his own code edit
#    to another std/src file and commits (MERGE_HEAD present) -> ?
# E: `git merge --squash feat` + `git commit` (a merge in name, no MERGE_HEAD) -> ?
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-main-no-direct-code.sh"
PC="$ROOT/scripts/githooks/pre-commit"
for f in "$G" "$PC"; do [ -s "$f" ] || { echo "MISSING $f"; exit 2; }; done
W="$HERE/work"; rm -rf "$W"; mkdir -p "$W"
R="$W/repo"; mkdir -p "$R"
gitp() { git -c user.name=probe -c user.email=probe@example.invalid "$@"; }
git -C "$R" init -q -b main; git -C "$R" config core.autocrlf false
mkdir -p "$R/std/src" "$R/scripts/githooks" "$R/scripts/guards"
echo base > "$R/std/src/a.nv"; echo base > "$R/std/src/b.nv"
cp "$PC" "$R/scripts/githooks/pre-commit"; cp "$G" "$R/scripts/guards/"
git -C "$R" add -- std/src/a.nv std/src/b.nv scripts/githooks/pre-commit scripts/guards/check-main-no-direct-code.sh
gitp -C "$R" -c core.hooksPath=/dev/null commit -q -m base
git -C "$R" config core.hooksPath scripts/githooks
gitp -C "$R" checkout -q -b feat
echo feat >> "$R/std/src/a.nv"; git -C "$R" add -- std/src/a.nv
gitp -C "$R" commit -q -m "feat: code on a branch" >/dev/null 2>&1
git -C "$R" checkout -q main
[ "$(git -C "$R" rev-list --count feat)" = 2 ] || { echo "MISSING feat commit"; exit 2; }
tipfiles() { echo "   main tip: $(git -C "$R" log -1 --format='%s' main) | files: $(git -C "$R" show --name-only --format= main | tr '\n' ' ')| parents: $(git -C "$R" log -1 --format='%p' main | wc -w)"; }

echo "=== A. control: plain git commit of std/src on main"
echo a >> "$R/std/src/b.nv"; git -C "$R" add -- std/src/b.nv
gitp -C "$R" commit -q -m "direct" 2>&1 | head -2; echo "rc=${PIPESTATUS[0]}"; tipfiles
git -C "$R" checkout -q -- . ; git -C "$R" read-tree HEAD; git -C "$R" checkout -q -- .

echo "=== B. git cherry-pick feat (on main, main tree)"
gitp -C "$R" cherry-pick feat >/dev/null 2>&1; echo "rc=$?"; tipfiles
gitp -C "$R" -c core.hooksPath=/dev/null reset -q --keep HEAD~1 2>/dev/null || git -C "$R" update-ref refs/heads/main HEAD~1
git -C "$R" checkout -q -f main

echo "=== C. git revert --no-edit of a code commit on main"
echo c >> "$R/std/src/b.nv"; git -C "$R" add -- std/src/b.nv
NOVA_MAIN_DIRECT_CODE="setup for revert #9001" gitp -C "$R" commit -q -m "keyed code" >/dev/null 2>&1
gitp -C "$R" revert --no-edit HEAD >/dev/null 2>&1; echo "rc=$?"; tipfiles

echo "=== D. merge --no-commit feat + integrator's OWN edit of std/src/b.nv, then commit"
gitp -C "$R" merge --no-commit --no-ff feat >/dev/null 2>&1
[ -f "$R/.git/MERGE_HEAD" ] && echo "   MERGE_HEAD present"
echo "integrator's own code" >> "$R/std/src/b.nv"; git -C "$R" add -- std/src/b.nv
gitp -C "$R" commit -q --no-edit 2>&1 | head -2; echo "rc=${PIPESTATUS[0]}"; tipfiles
echo "   b.nv changed by this merge relative to BOTH parents:"; git -C "$R" diff --name-only main^1 main -- std/src/b.nv; git -C "$R" diff --name-only main^2 main -- std/src/b.nv

echo "=== E. merge --squash feat + git commit"
gitp -C "$R" checkout -q -b feat2 main~1; echo e > "$R/std/src/c.nv"; git -C "$R" add -- std/src/c.nv
gitp -C "$R" -c core.hooksPath=/dev/null commit -q -m "feat2"; git -C "$R" checkout -q main
gitp -C "$R" merge --squash feat2 >/dev/null 2>&1
[ -f "$R/.git/MERGE_HEAD" ] && echo "   MERGE_HEAD present" || echo "   no MERGE_HEAD after --squash"
gitp -C "$R" commit -q -m "squash-merge feat2" 2>&1 | head -2; echo "rc=${PIPESTATUS[0]}"; tipfiles
