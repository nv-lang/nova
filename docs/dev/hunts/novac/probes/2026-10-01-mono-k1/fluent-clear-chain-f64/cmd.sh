#!/bin/sh
# Probe fluent-clear-chain-f64: same body over Vec[f64], chained: crash where the oracle prints true
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_clear|_novac_self->len. = '
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
