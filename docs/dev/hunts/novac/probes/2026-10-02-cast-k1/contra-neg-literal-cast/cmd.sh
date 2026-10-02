#!/bin/sh
# Contradiction probe neg-literal-cast: the two places, then what both
# compilers do with `-128 as i8`. Run from anywhere inside the repository.
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE
R="$PROBE"
while [ -n "$R" ] && [ "$R" != "/" ] && [ ! -f "$R/AGENTS.md" ]; do R=$(dirname "$R"); done
echo "=== place A (D489)"
grep -nF 'Литерал — и нетипизированное константное выражение из литералов (`40 + 60`, `-1`) — не имеет типа, пока' "$R/spec/decisions/02-types.md" || echo "QUOTE A NOT FOUND"
echo "=== place B (D54, amendment 2026-10-01 p.3)"
grep -nF '**3. Приоритет: `as` связывает сильнее префиксного минуса** — `-x as T` читается как `-(x as T)`' "$R/spec/decisions/03-syntax.md" || echo "QUOTE B NOT FOUND"
. "$PROBE/../run-probe.sh"
