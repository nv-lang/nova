#!/bin/sh
# Probe ctl-direct-handed-call: control: the same handed call written in main is instantiated and behaves
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_clear'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
