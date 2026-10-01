#!/bin/sh
# Probe fluent-expr-body: x
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_(noisy|swap|fresh)'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
