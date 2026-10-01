#!/bin/sh
# Probe ctl-foreign-call: control: a call into hp.geo without a colliding own name -- check/emit rc=0, link fails (the named E.10 simplification)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_'
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
