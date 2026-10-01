#!/bin/sh
# Probe option-payload-grid: Option over seven payload kinds, Some/None arms
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
