#!/bin/sh
# Probe diag-wrong-file-module: same for a program module body: main.nv named, offset is geo.nv's
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_'
export PROBE GREP
. "$PROBE/../run-probe.sh"
