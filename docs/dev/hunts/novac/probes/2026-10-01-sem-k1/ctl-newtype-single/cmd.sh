#!/bin/sh
# Probe ctl-newtype-single: control: the same program without the second module -- accepted, 300
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
