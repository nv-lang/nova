#!/bin/sh
# Probe tail-if-else-chain (an `else if` CHAIN as a value -- `ro shell = if .. else if .. else ..` (sem/mangle.nv) and a body tail -- refused "an `else if` chain is not compiled as a value yet" (one in the 0.2 measure); the rest as tail-if-branch-tail: a tail `if` whose branch ends in a NESTED `if` (sem/sem.nv `generic_head_cnt`, lower/lower_match.nv) or in a bare literal against a `u8` branch (lex/lex.nv `byte_at`) -- refused "a branch of a tail `if` ends without a value" and "the branches of a tail `if` must agree on a type" (three in the 0.2 measure); `pair` holds a TUPLE branch tail, which the base accepted and lowered as a statement -- silently `0` (the lowering asked `is_expr_kind`))
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_grade'
export PROBE GREP
. "$PROBE/../run-probe.sh"
