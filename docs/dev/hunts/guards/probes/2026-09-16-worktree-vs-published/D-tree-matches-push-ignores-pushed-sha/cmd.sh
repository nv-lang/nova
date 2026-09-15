#!/usr/bin/env bash
# Probe D -- check-tree-matches-push.sh answers "is MY checkout clean relative
# to MY HEAD?" and is read as "does the push carry what I gated?".
# The pre-push hook reads the pushed sha from stdin (<lref> <lsha> <rref> <rsha>)
# and throws <lsha> away: the guard is never told which object is being sent.
set -u
REPO="${NOVA_REPO:-$(git rev-parse --show-toplevel 2>/dev/null || echo "$PWD")}"
G="$REPO/scripts/guards/check-tree-matches-push.sh"
H="$REPO/scripts/githooks/pre-push"
[ -f "$G" ] || { echo "no guard at $G"; exit 2; }

T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
GC="git -c user.name=probe -c user.email=probe@example.com -C $T"

mkdir -p "$T/scripts/guards"
cp "$G" "$T/scripts/guards/"
$GC init -q -b main .
printf 'v1\n' > "$T/file.txt"
$GC add file.txt scripts/guards/check-tree-matches-push.sh; $GC commit -q -m c1
printf 'v2-UNGATED\n' > "$T/file.txt"
$GC add file.txt; $GC commit -q -m "c2 -- never gated by anyone"
MAIN=$($GC rev-parse main)
# the window works on its own branch, gated there
$GC checkout -q -b work HEAD~1
WORK=$($GC rev-parse work)
cd "$T" || exit 2
RUNHOOK() { printf 'refs/heads/main %s refs/heads/main %s\n' "$1" "0000000000000000000000000000000000000000" \
            | NOVA_SKIP_CI_CHECK=1 bash "$H"; echo "hook exit=$?"; }

echo "checked out: work = $WORK   |   being pushed: main = $MAIN"
echo "tree state: $($GC status --porcelain | wc -l) modified paths (clean)"

echo
echo "=== 1. the guard alone, clean checkout of 'work' ==="
bash "$G" "$T"; echo "exit=$?"

echo
echo "=== 2. full pre-push hook, pushing main (a commit this checkout never held) ==="
RUNHOOK "$MAIN"
echo "    ^ allowed: what lands on main is c2, which no gate ever saw."

echo
echo "=== 3. mirror: scratch edit in the checkout, pushing the SAME unchanged main ==="
printf 'scratch\n' >> "$T/file.txt"
RUNHOOK "$MAIN"
echo "    ^ refused, although the pushed object main=$MAIN is byte-identical to case 2."
$GC checkout -q -- file.txt

echo
echo "=== 4. the guard file itself missing from THIS checkout (case 635 re-enacted) ==="
$GC checkout -q main
$GC rm -q --cached scripts/guards/check-tree-matches-push.sh >/dev/null
rm -f "$T/scripts/guards/check-tree-matches-push.sh"
$GC commit -q -m "older checkout: guard not present here"
printf 'dirty\n' >> "$T/file.txt"
echo "tree is dirty: $($GC status --porcelain --untracked-files=no)"
RUNHOOK "$MAIN"
echo "    ^ hook line 40 is 'if [ -f \$TREE_GUARD ]' with no else:"
echo "      guard absent from the pushing worktree == check silently not performed."
