#!/bin/sh
# Probe strarm-in-loop-flow: string and char arms in a loop with continue/break/return in the arms
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='_novac_matched_t[0-9]+ = '
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
