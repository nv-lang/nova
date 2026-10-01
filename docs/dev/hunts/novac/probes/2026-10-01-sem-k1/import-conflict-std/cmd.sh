#!/bin/sh
# Probe import-conflict-std: the same conflict with a std import: the oracle accepts
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
