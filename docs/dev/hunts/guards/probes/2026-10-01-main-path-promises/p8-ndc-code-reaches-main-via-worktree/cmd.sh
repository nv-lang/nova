#!/usr/bin/env bash
# p8 -- check-main-no-direct-code.sh: code reaches main WITHOUT a merge when
# main is checked out in a worktree (main tree parked on another branch), and
# when the code path is not in the six-prefix list.
#
# Header promise, line 2: "код в main приезжает только слиянием."
# Header condition, line 13: "дерево ГЛАВНОЕ, не worktree (`--git-dir` ==
# `--git-common-dir`)". The rule is about the BRANCH main; the code judges it
# only in the main working copy. The selftest pins the worktree case as ok
# ("код на main в worktree -> ok"), so this is the header's own two lines
# disagreeing, not a regression.
#
# Everything is built in work/ here (throw-away git repo; real pre-commit and the
# real guard are COPIED in, core.hooksPath points at the copy).
# A: clean   -- main tree on main, only docs staged         -> expect ok
# B: canon   -- main tree on main, std/src staged           -> expect FAIL w/ path
# C: other   -- main tree on `park`, worktree on main, std/src committed there
#               through the real pre-commit -> ? and where did the commit land?
# D: other   -- main tree on main, compiler-codegen/build.rs staged -> ?
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-main-no-direct-code.sh"
PC="$ROOT/scripts/githooks/pre-commit"
for f in "$G" "$PC"; do [ -s "$f" ] || { echo "MISSING $f"; exit 2; }; done
W="$HERE/work"; rm -rf "$W"; mkdir -p "$W/main"
R="$W/main"
gitp() { git -c user.name=probe -c user.email=probe@example.invalid "$@"; }
git -C "$R" init -q -b main; git -C "$R" config core.autocrlf false
mkdir -p "$R/std/src" "$R/docs/plans" "$R/compiler-codegen" "$R/scripts/githooks" "$R/scripts/guards"
echo base > "$R/std/src/a.nv"; echo base > "$R/docs/plans/x.md"; echo 'fn main() {}' > "$R/compiler-codegen/build.rs"
cp "$PC" "$R/scripts/githooks/pre-commit"; cp "$G" "$R/scripts/guards/"
git -C "$R" add -- std/src/a.nv docs/plans/x.md compiler-codegen/build.rs scripts/githooks/pre-commit scripts/guards/check-main-no-direct-code.sh
gitp -C "$R" -c core.hooksPath=/dev/null commit -q -m base
git -C "$R" config core.hooksPath scripts/githooks
[ -n "$(git -C "$R" rev-parse -q --verify HEAD)" ] || { echo "MISSING base commit"; exit 2; }

echo "=== A. clean: docs only on main"
echo a >> "$R/docs/plans/x.md"; git -C "$R" add -- docs/plans/x.md
bash "$G" "$R" 2>&1; echo "rc=$?"; git -C "$R" read-tree HEAD

echo "=== B. canonical violation: std/src on main"
echo b >> "$R/std/src/a.nv"; git -C "$R" add -- std/src/a.nv
bash "$G" "$R" 2>&1; echo "rc=$?"; git -C "$R" read-tree HEAD; git -C "$R" checkout -q -- .

echo "=== D. build.rs (Rust code compiled into the compiler) on main"
echo '// d' >> "$R/compiler-codegen/build.rs"; git -C "$R" add -- compiler-codegen/build.rs
bash "$G" "$R" 2>&1; echo "rc=$?"; git -C "$R" read-tree HEAD; git -C "$R" checkout -q -- .

echo "=== C. main tree parked on 'park', main checked out in a worktree"
git -C "$R" checkout -q -b park
git -C "$R" worktree add -q "$W/wt" main
[ -f "$W/wt/std/src/a.nv" ] || { echo "MISSING worktree"; exit 2; }
echo c >> "$W/wt/std/src/a.nv"; git -C "$W/wt" add -- std/src/a.nv
echo "-- guard called directly on the worktree:"
bash "$G" "$W/wt" 2>&1; echo "rc=$?"
echo "-- real 'git commit' in the worktree (pre-commit runs):"
gitp -C "$W/wt" commit -q -m "code straight onto main" 2>&1; echo "commit rc=$?"
echo "-- files changed by the tip of main:"
git -C "$R" log -1 --format='%s' main; git -C "$R" show --name-only --format= main
