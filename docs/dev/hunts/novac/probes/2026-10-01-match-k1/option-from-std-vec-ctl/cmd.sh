#!/bin/sh
# Probe option-from-std-vec-ctl: control: the same instance bound first, then matched
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_ff__'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
