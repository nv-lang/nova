#!/bin/sh
# Probe newtype-literal-arg (an integer LITERAL as the argument of a NEWTYPE parameter (D489: a newtype over a number takes a literal by its representation rule) -- `branch(0, NodeKind.File, []Node.new())` of sem/binding_test.nv and sem/mangle_test.nv with `id NodeId`; a single row (`g(0, 3)`) and an overload set (`f(0)` beside `f(s str)`). Refused "this argument is not the one this function declares" / "no overload accepts these argument types" since the sem module is typed past channel.nv (sweep 3-4, b77177e37))
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_g'
export PROBE GREP
. "$PROBE/../run-probe.sh"
