#!/bin/sh
# Probe user-effect-prelude-control: CONTROL: the same three forms over the prelude effect Random; both compilers accept.
# Run: sh cmd.sh from THIS directory (it cds to the repo root itself).
. ../env.sh || exit 1
cd "$ROOT" || exit 1
P="$HUNT/user-effect-prelude-control/probe.nv"
[ -f "$P" ] || { echo "MISSING $P"; exit 2; }
echo "=== SUBJECT: novac check"
"$NOVAC" check "$P"; echo "novac rc=$?"
echo "=== ORACLE: nova check"
"$NOVA" check "$P"; echo "oracle check rc=$?"
echo "=== ORACLE: nova build + run"
T=$(mktemp -d) || exit 1
"$NOVA" build "$P" -o "$T/p.exe"; echo "oracle build rc=$?"
if [ -f "$T/p.exe" ]; then "$T/p.exe"; echo "run rc=$?"; else echo "no binary built"; fi
rm -rf "$T"
