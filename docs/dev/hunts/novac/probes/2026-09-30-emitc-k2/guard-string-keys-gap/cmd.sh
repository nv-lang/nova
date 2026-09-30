#!/bin/sh
# Probe guard-string-keys-gap: guard gap: check-novac-no-string-keys.py does not see a `.name ==` scan nor a string name used as a C identifier (#1428)
# Architectural finding: the reproduction is the places side by side.
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
if [ -z "$ROOT" ]; then d=$(cd "$(dirname "$0")" && pwd); while [ "$d" != "/" ] && [ ! -f "$d/AGENTS.md" ]; do d=$(dirname "$d"); done; ROOT="$d"; fi
cd "$ROOT" || exit 1
echo "=== novac/src/emit_c/emit_handler.nv:85-240"; awk 'NR>=85 && NR<=240' novac/src/emit_c/emit_handler.nv | grep -nE '\.name ==|c\.name|CaptureRow'
echo "=== novac/src/check/handler.nv:170-180"; awk 'NR>=170 && NR<=180' novac/src/check/handler.nv | grep -nE '\.name ==|c\.name|CaptureRow'
echo "=== novac/src/sem/channel.nv:255-265"; awk 'NR>=255 && NR<=265' novac/src/sem/channel.nv | grep -nE '\.name ==|c\.name|CaptureRow'
echo "=== guard verdict on the tree (green = the gap)"; "${PYTHON:-python}" scripts/guards/check-novac-no-string-keys.py . 2>&1 | tail -2
