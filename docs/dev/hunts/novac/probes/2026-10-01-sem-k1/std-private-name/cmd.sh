#!/bin/sh
# Probe std-private-name: program Token record hidden by PRIVATE std json Token enum: false #812 refusal
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
