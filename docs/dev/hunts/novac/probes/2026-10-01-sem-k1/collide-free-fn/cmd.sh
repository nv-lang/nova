#!/bin/sh
# Probe collide-free-fn: own fn geo_f of module hp and imported f of module hp.geo get ONE C name
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_'
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
