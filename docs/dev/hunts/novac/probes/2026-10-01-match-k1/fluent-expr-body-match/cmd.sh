#!/bin/sh
# Probe fluent-expr-body-match: -> @ with a match expression body yielding a non-receiver value of the receiver type
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_pick'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
