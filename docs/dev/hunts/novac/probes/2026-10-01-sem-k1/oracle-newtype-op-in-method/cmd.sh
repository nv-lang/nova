#!/bin/sh
# Probe oracle-newtype-op-in-method: oracle observation -- `@ + @` in a method of a newtype of another module: the oracle links against a missing Nova_Meters_method_plus
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
