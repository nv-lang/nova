#!/bin/sh
# Probe diag-body-file-module (#1521): a refusal INSIDE the instance of a program module's body names the body's file (the oracle: geo.nv:4); Carina names main.nv at 0..0 and puts the body's file into the message text
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_geo_bad'
export PROBE GREP
. "$PROBE/../../2026-10-01-mono-k1/run-probe.sh"
