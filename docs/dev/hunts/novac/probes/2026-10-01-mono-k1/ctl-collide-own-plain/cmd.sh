#!/bin/sh
# Probe ctl-collide-own-plain: control: a NON-generic own is_empty keeps its own name
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_is_empty'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
