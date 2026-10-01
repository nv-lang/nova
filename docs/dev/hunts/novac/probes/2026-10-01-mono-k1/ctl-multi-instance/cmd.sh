#!/bin/sh
# Probe ctl-multi-instance: control: one handed body instantiated over f64 and str in one program
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_index'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
