#!/bin/sh
# Probe match-payload-rest-leading: variant payload with leading rest: which payload z reads
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
echo "=== SUBJECT: novac emit -- which payload slot does z read? (Pr(20, 30); last is _1)"
cd "$ROOT" && "$NOVAC" emit "$P" 2>&1 | grep -n "nova_int z = "
