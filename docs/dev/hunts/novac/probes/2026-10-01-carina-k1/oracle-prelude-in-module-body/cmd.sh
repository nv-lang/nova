#!/bin/sh
# Probe oracle-prelude-in-module-body (oracle, possible defect): `println` -- the prelude, visible without import (07-modules.md) -- in a file of program module `hp.geo` is refused by the ORACLE as "undefined identifier `println`" -- both in a generic extension method (geo.nv:4:5) and in a plain exported function (geo.nv:8:5); the same call in main.nv (module `hp`) is accepted. Carina accepts all three (its link fails only because the plain `show` is the owner unit's to emit)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_geo_(show|mark)'
export PROBE GREP
. "$PROBE/../../2026-10-01-mono-k1/run-probe.sh"
