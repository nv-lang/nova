#!/bin/sh
# Probe ambiguity-linearity: observation: [T]/[T consume] pair refused as ambiguous (D464 amendment rule 1 picks [T]); novac has no linearity (#1099)
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_contains'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
