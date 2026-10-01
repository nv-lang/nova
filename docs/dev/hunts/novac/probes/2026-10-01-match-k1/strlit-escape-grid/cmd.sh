#!/bin/sh
# Probe strlit-escape-grid: string-literal arms with escapes, the dollar escape among them
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='_novac_strlit_[0-9]+ =|nova_str_eq'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
