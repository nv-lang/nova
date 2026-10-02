#!/bin/sh
# Probe type-body-sig (#1567: the body module's private type in a SIGNATURE of its private helper, called from its generic method)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_geo_(boxed|open)'
export PROBE GREP
. "$PROBE/../run-probe.sh"
