#!/bin/sh
# Probe scrutinee-once: a scrutinee with a visible effect, evaluated once over string and Option arms
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_(word|opt)__'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
