#!/bin/sh
# Probe 4 -- the mistake is in a_broken.nv (a `fn` body that is never closed);
# in unit mode novac blames b_fine.nv, which CONTROL 2 shows is clean alone.
# NOVAC = <repo>/novac/target/novac.exe ; NOVA_STD_PATH = <repo>/std/src
# Run from the directory that holds this file.
echo "=== SUBJECT: unit of the two peers ==="
NOVAC_UNIT=1 "$NOVAC" check a_broken.nv b_fine.nv
echo "=== CONTROL 1: the broken file alone ==="
"$NOVAC" check a_broken.nv
echo "=== CONTROL 2: the fine file alone (no JSON = clean) ==="
"$NOVAC" check b_fine.nv; echo "rc=$?"
