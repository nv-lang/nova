#!/bin/sh
# Probe ctl-instance-refusal-bound: control: the same instance bound to a name is refused by check, as by emit
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_last'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
