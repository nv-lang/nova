#!/bin/sh
# Probe fluent-clear-alias: a handed std '-> @' body (Vec @clear) is emitted with no return; its result is garbage
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_clear|_novac_self->len. = '
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
