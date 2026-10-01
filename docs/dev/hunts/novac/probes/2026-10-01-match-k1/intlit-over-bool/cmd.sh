#!/bin/sh
# Probe intlit-over-bool: int literal pattern over a non-integer scrutinee
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
