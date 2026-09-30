#!/bin/sh
# Hunt 2026-09-30 emit_c x K2 -- shared environment for every probe.
# ROOT: the repository whose novac is judged. Derived by walking up to
# AGENTS.md; outside a checkout pass ROOT=<repo> explicitly.
# NOVAC defaults to the repository's novac; the hunt measured a private build
# (pass NOVAC=<path>). TMPDIR keeps every artefact out of the repository.
[ -n "$TMPDIR" ] || TMPDIR=/tmp
export TMPDIR
if [ -z "$ROOT" ]; then
    d=$(cd "$(dirname "$P")" 2>/dev/null && pwd)
    while [ -n "$d" ] && [ "$d" != "/" ] && [ ! -f "$d/AGENTS.md" ]; do d=$(dirname "$d"); done
    if [ -n "$d" ] && [ -f "$d/AGENTS.md" ]; then ROOT="$d"; else echo "env.sh: no repository root above the probe -- set ROOT=<repo>"; exit 1; fi
fi
[ -n "$NOVAC" ] || NOVAC="$ROOT/novac/target/novac.exe"
# Oracle: a private COPY -- the gate holds the repository binary (registry 1098).
[ -n "$NOVA" ]  || NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -x "$NOVAC" ] || { echo "env.sh: no novac at $NOVAC"; exit 1; }
[ -x "$NOVA" ]  || { echo "env.sh: no oracle at $NOVA"; exit 1; }
# A copied oracle does not find std beside itself: name it.
[ -n "$NOVA_STD_PATH" ] || NOVA_STD_PATH="$ROOT/std"
export ROOT NOVAC NOVA NOVA_STD_PATH
