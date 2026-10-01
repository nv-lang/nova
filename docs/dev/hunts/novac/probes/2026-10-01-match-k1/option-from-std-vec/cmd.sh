#!/bin/sh
# Probe option-from-std-vec: a Carina-made std instance as a match scrutinee: check clean, emit ICE
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_(ff|ls)__'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
