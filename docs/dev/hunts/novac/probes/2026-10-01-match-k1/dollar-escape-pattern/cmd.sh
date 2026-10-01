#!/bin/sh
# Probe dollar-escape-pattern: the dollar escape in a pattern literal against the same text in an expression (D467 s2)
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='_novac_strlit_[0-9]+_buf\[\] ='
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
