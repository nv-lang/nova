#!/bin/sh
# Probe ctl-own-newtype-vs-foreign: control: own newtype Id int beside a foreign Id u8 -- own wins, 300
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
