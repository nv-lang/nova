#!/bin/sh
# Hunt 2026-10-06 strlit-k1 -- body shared by the JUDGE probes' cmd.sh (refuse or accept).
#
# A probe is ONE program, <probe>/prog.nv.txt (evidence, not a fixture). This body copies
# it to a temporary directory as prog.nv beside a one-line nova.toml and asks both sides
# whether it is legal Nova: Carina `NOVAC_UNIT=1 novac check prog.nv` (JSON diagnostics),
# the oracle `nova check prog.nv`. The run-and-compare probes use run-probe.sh instead.
#
# ROOT is found by walking up to AGENTS.md (or pass ROOT=<repo>). NOVAC and NOVA override
# the binaries (default: Carina through the door novac_bin, registry 1607; the oracle of
# this tree). Every artefact goes to TMPDIR.
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
NOVA_STD_PATH="${NOVA_STD_PATH:-$ROOT/std}"
export NOVA_STD_PATH
T=$(mktemp -d) || exit 1
cp "$PROBE/prog.nv.txt" "$T/prog.nv" || { echo "run-check.sh: copy failed"; rm -rf "$T"; exit 2; }
printf '[package]\nname = "prog"\nversion = "0.1.0"\n' > "$T/nova.toml"
echo "=== PROGRAM"
cat -n "$T/prog.nv"
cd "$T" || exit 1
echo "=== SUBJECT: novac check (NOVAC_UNIT=1)"
NOVAC_UNIT=1 "$NOVAC" check prog.nv > n.out 2>&1
echo "novac rc=$?"
sed "s|$T|<tmp>|g" n.out
echo "=== ORACLE: nova check"
"$NOVA" check prog.nv 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | grep -E "error|ok:" | sed "s|$T|<tmp>|g"
cd / && rm -rf "$T"
