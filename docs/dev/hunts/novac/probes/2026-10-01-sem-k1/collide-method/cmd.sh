#!/bin/sh
# Probe collide-method: own fn geo_double(m Meters) and method Meters @double of hp.geo get ONE C name
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_'
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
