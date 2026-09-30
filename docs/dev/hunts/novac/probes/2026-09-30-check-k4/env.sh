#!/bin/sh
# Hunt 2026-09-30 check x K4 -- shared environment for every probe.
# ROOT: the repository whose novac is judged. Derived by walking up to
# AGENTS.md; outside a checkout pass ROOT=<repo> explicitly (no machine
# path is baked in). NOVA: the oracle; defaults to ROOT's own build.
export TEMP='D:\Temp' TMP='D:\Temp' TMPDIR=/d/Temp
if [ -z "$ROOT" ]; then
    d=$(cd "$(dirname "$P")" 2>/dev/null && pwd)
    while [ -n "$d" ] && [ "$d" != "/" ] && [ ! -f "$d/AGENTS.md" ]; do d=$(dirname "$d"); done
    if [ -n "$d" ] && [ -f "$d/AGENTS.md" ]; then ROOT="$d"; else echo "env.sh: no repository root above the probe -- set ROOT=<repo>"; exit 1; fi
fi
[ -n "$NOVAC" ] || NOVAC="$ROOT/novac/target/novac.exe"
[ -n "$NOVA" ]  || NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -x "$NOVAC" ] || { echo "env.sh: no novac at $NOVAC"; exit 1; }
[ -x "$NOVA" ]  || { echo "env.sh: no oracle at $NOVA"; exit 1; }
export ROOT NOVAC NOVA
