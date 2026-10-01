#!/bin/sh
# Probe unimported-program-fn: call of f of hp.geo WITHOUT an import: oracle undefined identifier, Carina accepts (and, with geo_f, prints 1)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_'
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
