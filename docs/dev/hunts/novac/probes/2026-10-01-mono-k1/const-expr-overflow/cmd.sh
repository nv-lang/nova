#!/bin/sh
# Probe const-expr-overflow (an untyped integer CONSTANT EXPRESSION OUT OF its position's range (D489: refused by the exact value, E_LIT_OUT_OF_RANGE). THE ORACLE WRAPS IN SILENCE: `k(200 + 100, 0)` prints 44 (registry: oracle, filed with part 9). Carina refuses at check)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_k'
export PROBE GREP
. "$PROBE/../run-probe.sh"
