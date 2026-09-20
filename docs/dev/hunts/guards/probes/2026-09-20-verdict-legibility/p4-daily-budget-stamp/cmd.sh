#!/usr/bin/env bash
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-gate-daily-budget.py"
NOW=1758000000          # frozen "now"
FRESH=$((NOW - 60))     # heavy tier ran ONE MINUTE ago

run() { # $1 = case name, stamp already prepared at $HERE/stamp
  echo "### $1"
  out=$(NOVA_GATE_NOW=$NOW NOVA_GATE_STAMP="$HERE/stamp" python "$G" push "$ROOT" 2>&1); rc=$?
  echo "  rc=$rc"
  printf '  %s\n' "$out"
  echo "  stamp after: [$(cat "$HERE/stamp" 2>/dev/null | tr -d '\r\n')]"
}

printf '%s push\n' "$FRESH" > "$HERE/stamp"
run "A. honest stamp: heavy tier ran 1 minute ago"

printf '%s push\n' "$FRESH" > "$HERE/stamp"; : > "$HERE/stamp"        # crash mid-write -> empty file
run "B. same world, stamp file EMPTY (interrupted write)"

printf 'push %s\n' "$FRESH" > "$HERE/stamp"                            # fields swapped
run "C. same world, stamp field order changed"

rm -f "$HERE/stamp"
run "D. no stamp at all - heavy tier really never ran"
