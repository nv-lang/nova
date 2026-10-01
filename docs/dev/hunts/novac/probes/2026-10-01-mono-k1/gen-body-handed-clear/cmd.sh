#!/bin/sh
# Probe gen-body-handed-clear: same with a mut receiver
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_clear|novac_fn_wipe'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
