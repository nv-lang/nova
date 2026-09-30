#!/bin/sh
# Probe arch-second-door-op-schema: second door: the emitter re-derives the op schema and the literal's effect by text (#1427)
# Architectural finding: the reproduction is the places side by side.
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
if [ -z "$ROOT" ]; then d=$(cd "$(dirname "$0")" && pwd); while [ "$d" != "/" ] && [ ! -f "$d/AGENTS.md" ]; do d=$(dirname "$d"); done; ROOT="$d"; fi
cd "$ROOT" || exit 1
echo "=== novac/src/emit_c/emit_handler.nv:60-110"; awk 'NR>=60 && NR<=110' novac/src/emit_c/emit_handler.nv | grep -nE 'ops\[.*\]\.name ==|\.name == opname|lit_effect|effect_of|leaf'
echo "=== novac/src/check/handler.nv:100-185"; awk 'NR>=100 && NR<=185' novac/src/check/handler.nv | grep -nE 'ops\[.*\]\.name ==|\.name == opname|lit_effect|effect_of|leaf'
