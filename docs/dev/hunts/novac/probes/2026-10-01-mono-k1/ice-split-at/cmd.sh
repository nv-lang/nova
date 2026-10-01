#!/bin/sh
# Probe ice-split-at: observation: ICE in sem on Vec[int] split_at
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
