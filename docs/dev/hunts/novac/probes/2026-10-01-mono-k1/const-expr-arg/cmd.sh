#!/bin/sh
# Probe const-expr-arg (an untyped integer CONSTANT EXPRESSION in a narrow position (D489, registry 1679): `+ - * /`, nested parentheses, a unary minus, into `u8`, `i16`, newtypes over `int` and `u8`, a return, a constant. Oracle 42 42 42 42 3 42 2 -30000 42 42 42 42. Refused at the base: "this argument is not the one this function declares" / "cannot return value of type `int`")
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_k'
export PROBE GREP
. "$PROBE/../run-probe.sh"
