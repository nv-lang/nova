#!/usr/bin/env bash
# Probe A -- check-ratchet-moves.sh judges the UNCOMMITTED diff while its
# verdict speaks of "the same merge".
# Self-contained: builds its own throwaway repo, needs only NOVA_REPO env
# (path to the nova repo that holds the guard) or defaults to $PWD.
set -u
REPO="${NOVA_REPO:-$(git rev-parse --show-toplevel 2>/dev/null || echo "$PWD")}"
G="$REPO/scripts/guards/check-ratchet-moves.sh"
[ -f "$G" ] || { echo "no guard at $G"; exit 2; }

T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
GC="git -c user.name=probe -c user.email=probe@example.com -C $T"

mkdir -p "$T/scripts/guards"
printf 'rows=10\n' > "$T/scripts/guards/registry-rows.baseline"
$GC init -q .
$GC add scripts/guards/registry-rows.baseline
$GC commit -q -m init

echo "=== state 1: clean tree, nothing shifted ==="
bash "$G" "$T" "$T"; echo "exit=$?"

echo
echo "=== state 2: shift STAGED without a reason (index = what will be committed) ==="
printf 'rows=13\n' > "$T/scripts/guards/registry-rows.baseline"
$GC add scripts/guards/registry-rows.baseline
bash "$G" "$T" "$T"; echo "exit=$?"

echo
echo "=== state 3: THE SAME shift now COMMITTED, no reason anywhere ==="
$GC commit -q -m "shift the ratchet, no reason given"
$GC show --stat --oneline HEAD | head -3
bash "$G" "$T" "$T"; echo "exit=$?"

echo
echo "=== state 4: shift lives ONLY in the working copy, will never be committed ==="
printf 'rows=99\n# reason: probe\n' > "$T/scripts/guards/registry-rows.baseline"
echo "--- git diff --cached (what the commit would carry):"
$GC diff --cached --stat | sed 's/^/    /'
echo "    (empty above == the commit carries NO ratchet shift)"
bash "$G" "$T" "$T"; echo "exit=$?"
