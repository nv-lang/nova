#!/bin/sh
# Probe fluent-expr-body-other-recv: x
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_(swap|fresh)|return '
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
