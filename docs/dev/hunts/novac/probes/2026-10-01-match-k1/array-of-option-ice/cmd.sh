#!/bin/sh
# Probe array-of-option-ice: an array literal of Options: check ICE in the mangler
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
