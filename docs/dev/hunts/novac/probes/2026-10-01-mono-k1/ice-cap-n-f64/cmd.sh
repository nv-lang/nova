#!/bin/sh
# Probe ice-cap-n-f64: observation: ICE in check on Vec[f64] cap(n)
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
