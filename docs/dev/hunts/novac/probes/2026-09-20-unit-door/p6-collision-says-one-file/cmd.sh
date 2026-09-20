#!/bin/sh
# Probe 6 -- two DIFFERENT peer files each declare `type T` once. The
# diagnostic says the name "is declared twice in one file".
# NOVAC = <repo>/novac/target/novac.exe ; NOVA_STD_PATH = <repo>/std/src
echo "=== SUBJECT: unit of ta.nv + tb.nv ==="
NOVAC_UNIT=1 "$NOVAC" check ta.nv tb.nv
echo "=== CONTROL: ta.nv alone (no JSON = clean) ==="
"$NOVAC" check ta.nv; echo "rc=$?"
