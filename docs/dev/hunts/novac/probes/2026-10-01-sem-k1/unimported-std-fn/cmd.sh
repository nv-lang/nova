#!/bin/sh
# Probe unimported-std-fn: call of std is_high_surrogate WITHOUT an import: oracle undefined identifier, Carina accepts
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
