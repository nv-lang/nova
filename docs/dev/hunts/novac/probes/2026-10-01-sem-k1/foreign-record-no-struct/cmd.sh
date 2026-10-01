#!/bin/sh
# Probe foreign-record-no-struct: a record of hp.geo: check/emit rc=0, no struct in the unit, C fails
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='Nova_Point'
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
