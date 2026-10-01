#!/bin/sh
# Probe import-conflict: imported label of hp.geo beside own label(int): oracle E_IMPORT_NAME_CONFLICT, Carina accepts
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_'
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
