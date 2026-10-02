#!/bin/sh
# Probe handed-sum-pattern (payload count: a `match` on a sum declared in ANOTHER module of the program whose payload is that module's NEWTYPE -- `TFn(r)` on `TFn(Row)`, `type Row int` -- refused "the pattern's payload count disagrees" (the 0.2 measure's four in mono/mono.nv on sem's `CalleeTarget`, whose payloads `FnRow`/`VariantRow` are newtypes))
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_pick'
export PROBE GREP
. "$PROBE/../run-probe.sh"
