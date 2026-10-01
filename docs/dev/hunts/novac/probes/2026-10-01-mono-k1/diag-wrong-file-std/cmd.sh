#!/bin/sh
# Probe diag-wrong-file-std: a refusal inside the handed std body names the PROGRAM file with offsets of the std file
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_equal'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
