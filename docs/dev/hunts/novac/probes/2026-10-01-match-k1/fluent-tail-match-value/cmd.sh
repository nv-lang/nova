#!/bin/sh
# Probe fluent-tail-match-value: a -> @ body whose tail is a value match / value if: discarded, then return @ (D409)
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
