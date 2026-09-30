#!/bin/sh
# Body shared by every cmd.sh: P must be set to the probe's absolute path.
[ -f "$P" ] || { echo "MISSING $P"; exit 2; }
. "$(dirname "$P")/../env.sh"
cd "$ROOT" || exit 1
echo "=== SUBJECT: novac check"
"$NOVAC" check "$P"; echo "novac rc=$?"
echo "=== ORACLE: nova check"
"$NOVA" check "$P"; echo "oracle check rc=$?"
if [ -z "$NO_BUILD" ]; then
  echo "=== ORACLE: nova build + run"
  T=$(mktemp -d) || exit 1
  "$NOVA" build "$P" -o "$T/p.exe" >"$T/b.log" 2>&1; echo "oracle build rc=$?"; head -5 "$T/b.log"
  if [ -f "$T/p.exe" ]; then "$T/p.exe"; echo "run rc=$?"; else echo "no binary built"; fi
  rm -rf "$T"
fi
