#!/bin/sh
# Probe tail-if-branch-tail (a tail `if` whose branch ends in a NESTED `if` (sem/sem.nv `generic_head_cnt`, lower/lower_match.nv) or in a bare literal against a `u8` branch (lex/lex.nv `byte_at`) -- refused "a branch of a tail `if` ends without a value" and "the branches of a tail `if` must agree on a type" (three in the 0.2 measure); `pair` holds a TUPLE branch tail, which the base accepted and lowered as a statement -- silently `0` (the lowering asked `is_expr_kind`))
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_deep'
export PROBE GREP
. "$PROBE/../run-probe.sh"
