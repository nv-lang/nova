#!/bin/sh
# Probe type-body-private-sum (#1567: a PRIVATE sum of a program module, a variant built and matched in its generic method)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_geo_(mode|pick)'
export PROBE GREP
. "$PROBE/../run-probe.sh"
