#!/bin/sh
# Probe std-name-record-field-type: program CallerLoc { line f64 } typed as std CallerLoc { line int }: false E_NOVAC_LANG
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
