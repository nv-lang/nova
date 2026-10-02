#!/bin/sh
# Probe type-body-private-method (#1567: a method of a PRIVATE type of a program module, called from its generic method)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_geo_(total|twice)'
export PROBE GREP
. "$PROBE/../run-probe.sh"
