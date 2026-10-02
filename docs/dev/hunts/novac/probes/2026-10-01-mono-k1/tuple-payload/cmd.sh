#!/bin/sh
# Probe tuple-payload (a TUPLE literal as a variant payload -- `Some((a, 2))`, the form of sem/callables.nv `VariantRows(first, cnt) => Some((first, cnt))` -- refused "this variant takes a different number of payload values than the call gives" (two in the 0.2 measure))
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_span'
export PROBE GREP
. "$PROBE/../run-probe.sh"
