#!/bin/sh
# Probe bind-tuple-rest-middle-int: D486 s2 tuple rest in the middle, all-int: which element z reads
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
echo "=== SUBJECT: novac emit -- which tuple field does z read? (t = (10, 20, 30); last is f2)"
cd "$ROOT" && "$NOVAC" emit "$P" 2>&1 | grep -n "nova_int a = \|nova_int z = \|_novac_tmp_t1.f[0-9] = "
