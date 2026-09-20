#!/usr/bin/env bash
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
H="$ROOT/scripts/claude-hooks/guard-memory.py"
for c in a-compliant b-violating c-malformed d-schema-shift; do
  echo "### $c"
  out=$(python "$H" < "$HERE/$c.json" 2>&1); rc=$?
  echo "  rc=$rc  out=[$out]"
done
