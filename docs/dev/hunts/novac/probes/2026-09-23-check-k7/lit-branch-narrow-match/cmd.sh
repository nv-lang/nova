#!/bin/sh
# Probe lit-branch-narrow-match: SIBLING: integer literals in the arms of a tail `match` of a `-> u64` fn; oracle builds and prints 4.
# Run: sh cmd.sh from THIS directory (it cds to the repo root itself).
. ../env.sh || exit 1
cd "$ROOT" || exit 1
P="$HUNT/lit-branch-narrow-match/probe.nv"
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
