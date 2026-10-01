#!/bin/sh
# Probe fluent-return-value-ctl: control: the return 5 branch never taken
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
