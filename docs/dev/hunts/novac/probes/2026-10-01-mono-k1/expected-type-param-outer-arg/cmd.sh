#!/bin/sh
# Probe expected-type-param-outer-arg (the same rule with the expectation from an OUTER call's parameter: `take(idf(if true { 5 } else { 0 }))` and `take(idf(8))` with `fn take(r Row)`. D489 names an argument a position that knows its type; the ORACLE refuses both (E7301 "cannot pass `int` as argument `r` of type `Row`") -- an oracle row, Carina accepts and prints 5 8)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_take'
export PROBE GREP
. "$PROBE/../run-probe.sh"
