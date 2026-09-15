#!/usr/bin/env bash
# Probe F -- check-ratchet-moves.sh counts ADDED LINES THAT LOOK LIKE
# "name=number", not ratchet VALUES that moved. Appending the trailing newline
# that check-baseline-newline.sh demands re-adds the last line verbatim, and
# the guard reports "ratchet shifted without a reason" although no number
# changed. Live right now in the nova tree on registry-routes.baseline.
set -u
REPO="${NOVA_REPO:-$(git rev-parse --show-toplevel 2>/dev/null || echo "$PWD")}"
G="$REPO/scripts/guards/check-ratchet-moves.sh"
B="$REPO/scripts/guards/check-baseline-newline.sh"
[ -f "$G" ] || { echo "no guard at $G"; exit 2; }

T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
GC="git -c user.name=probe -c user.email=probe@example.com -C $T"

mkdir -p "$T/scripts/guards"
$GC init -q .
# committed WITHOUT a trailing newline -- exactly the state check-baseline-newline
# calls a defect and orders you to fix.
printf '# reason: measured 2026-01-01\nblockers=73' > "$T/scripts/guards/demo.baseline"
$GC add scripts/guards/demo.baseline
$GC commit -q -m init

echo "=== before the fix: baseline-newline says the file is broken ==="
bash "$B" "$T" "$T"; echo "exit=$?"
echo "=== before the fix: ratchet-moves is green ==="
bash "$G" "$T" "$T"; echo "exit=$?"

echo
echo "=== the ONLY edit: append the missing newline. No number changes. ==="
printf '# reason: measured 2026-01-01\nblockers=73\n' > "$T/scripts/guards/demo.baseline"
echo "--- the diff git sees:"
$GC diff -- scripts/guards/demo.baseline | tail -5 | sed 's/^/    /'
echo "--- value before: $($GC show HEAD:scripts/guards/demo.baseline | sed -n 's/^blockers=//p')"
echo "--- value now:    $(sed -n 's/^blockers=//p' "$T/scripts/guards/demo.baseline")"
echo
bash "$B" "$T" "$T"; echo "baseline-newline exit=$?"
bash "$G" "$T" "$T"; echo "ratchet-moves exit=$?"
echo "    ^ FAIL 'ratchet shifted without a reason' with the value unchanged at 73."
