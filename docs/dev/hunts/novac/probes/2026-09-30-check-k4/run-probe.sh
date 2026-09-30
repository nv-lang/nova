#!/bin/sh
# Body shared by every cmd.sh: P must be set to the probe's absolute path.
# Prints BOTH verdicts: novac check, oracle check. With RUN=1 also builds and
# runs the probe on both compilers through scripts/tools/novac-e1-smoke.sh
# (novac's C linked with the oracle's own argv; behaviour diffed).
[ -f "$P" ] || { echo "MISSING $P"; exit 2; }
. "$(dirname "$P")/../env.sh"
cd "$ROOT" || exit 1
echo "=== SUBJECT: novac check"
"$NOVAC" check "$P"; echo "novac rc=$?"
echo "=== ORACLE: nova check"
"$NOVA" check "$P"; echo "oracle check rc=$?"
if [ -n "$RUN" ]; then
  echo "=== BEHAVIOUR: novac-e1-smoke (oracle binary vs novac binary)"
  NOVAC_BIN="$NOVAC" sh "$ROOT/scripts/tools/novac-e1-smoke.sh" "$P"; echo "smoke rc=$?"
fi
if [ -n "$EMIT" ]; then
  echo "=== SUBJECT: novac emit (lines matching \$EMIT)"
  "$NOVAC" emit "$P" 2>&1 | grep -n -E "$EMIT" | head -40
fi
