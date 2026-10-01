#!/bin/sh
# Probe ctl-priv-no-module: control: without the module line the instance compiles and behaves
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_is_empty'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
