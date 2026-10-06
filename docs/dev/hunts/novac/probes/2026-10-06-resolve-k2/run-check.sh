#!/bin/sh
# Hunt 2026-10-06 resolve x K2 -- body shared by every cmd.sh.
#
# A probe is ONE program, <probe>/prog.nv.txt (evidence is not a fixture: the
# suffix keeps it out of every corpus). This body copies it to a temporary
# directory as prog.nv, then from the repository root runs BOTH checkers on it:
#   1) ORACLE: `nova check prog.nv`
#   2) SUBJECT: `novac check prog.nv`
# and, when both accept, `nova build` + run of the oracle's binary (the
# oracle's stdout is the behaviour the norm is read against).
#
# ROOT is found by walking up to AGENTS.md (or pass ROOT=<repo>). NOVAC and
# NOVA override the binaries (default: the repository's door `novac_bin`, and
# nova-cli/target/release/nova.exe). Every artefact goes to TMPDIR.
[ -n "$PROBE" ] || { echo "run-check.sh: PROBE (the probe dir) is not set"; exit 2; }
[ -f "$PROBE/prog.nv.txt" ] || { echo "MISSING $PROBE/prog.nv.txt"; exit 2; }
[ -n "$TMPDIR" ] || TMPDIR=/tmp
export TMPDIR
if [ -z "$ROOT" ]; then
    d="$PROBE"
    while [ -n "$d" ] && [ "$d" != "/" ] && [ ! -f "$d/AGENTS.md" ]; do d=$(dirname "$d"); done
    if [ -f "$d/AGENTS.md" ]; then ROOT="$d"; else echo "run-check.sh: no repository root above the probe -- set ROOT=<repo>"; exit 1; fi
fi
[ -n "$NOVAC" ] || { . "$ROOT/scripts/guards/lib/novac.sh"; NOVAC=$(novac_bin "$ROOT"); }
[ -n "$NOVA" ]  || NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -x "$NOVAC" ] || { echo "run-check.sh: no novac at $NOVAC (pass NOVAC=)"; exit 1; }
[ -x "$NOVA" ]  || { echo "run-check.sh: no oracle at $NOVA (pass NOVA=)"; exit 1; }
cd "$ROOT" || exit 1
T=$(mktemp -d) || exit 1
cp "$PROBE/prog.nv.txt" "$T/prog.nv" || { echo "run-check.sh: copy failed"; rm -rf "$T"; exit 2; }
echo "=== PROGRAM"
cat -n "$T/prog.nv"
echo "=== ORACLE: nova check prog.nv"
"$NOVA" check "$T/prog.nv" > "$T/o.chk" 2>&1; O=$?
echo "oracle check rc=$O"
sed "s|$T|<tmp>|g" "$T/o.chk" | grep -E "error|E_|D13" | head -4
echo "=== SUBJECT: novac check prog.nv"
"$NOVAC" check "$T/prog.nv" > "$T/n.chk" 2>&1; N=$?
echo "novac check rc=$N"
sed "s|$T|<tmp>|g" "$T/n.chk" | head -4
if [ "$O" = "0" ] && [ "$N" = "0" ]; then
    echo "=== both accept: oracle build + run (the behaviour)"
    "$NOVA" build "$T/prog.nv" -o "$T/o.exe" > "$T/o.out" 2>&1; echo "oracle build rc=$?"
    if [ -f "$T/o.exe" ]; then "$T/o.exe" 2>&1 | tr -d '\r'; echo "oracle run rc=$?"; fi
fi
if [ "$O" = "0" ] && [ "$N" = "0" ]; then V="BOTH ACCEPT"
elif [ "$O" != "0" ] && [ "$N" != "0" ]; then V="BOTH REFUSE"
elif [ "$O" = "0" ]; then V="DIFFERENT: oracle accepts, Carina refuses"
else V="DIFFERENT: Carina accepts, oracle refuses"; fi
echo "=== VERDICT: $V"
rm -rf "$T"
