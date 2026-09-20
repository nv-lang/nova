#!/bin/sh
# Probe 5 -- same shape as probe 4, but the innocent peer is a ZERO-BYTE file:
# the diagnostic is printed at offset 1 of a file that has no byte 1.
# NOVAC = <repo>/novac/target/novac.exe ; NOVA_STD_PATH = <repo>/std/src
echo "z_empty.nv byte count: $(wc -c < z_empty.nv)"
echo "=== SUBJECT: unit of a_broken.nv + the empty peer ==="
NOVAC_UNIT=1 "$NOVAC" check a_broken.nv z_empty.nv
echo "=== CONTROL 1: the broken file alone ==="
"$NOVAC" check a_broken.nv
echo "=== CONTROL 2: the empty file alone (no JSON = clean) ==="
"$NOVAC" check z_empty.nv; echo "rc=$?"
