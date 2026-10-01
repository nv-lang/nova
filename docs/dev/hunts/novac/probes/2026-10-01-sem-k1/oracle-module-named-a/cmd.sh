#!/bin/sh
# Probe oracle-module-named-a: oracle observation: a program module named `a` breaks the oracle build in std access.nv (a.compare)
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
