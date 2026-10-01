#!/bin/sh
# Probe coalesce-instance-check-blind: d.first() ?? 0.0 over Vec[f64]: check rc=0, emit ICE
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_first'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
