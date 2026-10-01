#!/bin/sh
# Probe instance-name-collision: the std instance and the file's own generic of the same name and shape get ONE C name
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_is_empty'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
