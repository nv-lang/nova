#!/bin/sh
# Architectural probe (hunt 2026-10-06 lower x K4): the places that answer
# "is this ? / ?? over an Option or over a Result, and which container does
# the body return?". Run from the repository root (or pass ROOT=<repo>).
[ -n "$ROOT" ] || ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || ROOT=.
cd "$ROOT" || exit 1
echo "=== A. CHECKER x? (check/try_rules.nv): container and return container decided"
grep -n "ro payload = if is_ty(ot) { coalesce_payload\|ro rt = @ret_expect\|ro ret_is_opt = \|ro ret_is_res = \|ro op_is_opt = \|@record(tk\[0\], ot)" novac/src/check/try_rules.nv
echo "=== B. CHECKER a ?? b (check/exprs.nv)"
grep -n "ro payload = coalesce_payload(@ctx, lt)" novac/src/check/exprs.nv
echo "=== C. LOWERING a ?? b and x? (lower/lower_coalesce.nv): the same questions asked again"
grep -n "ro is_opt = is_ty(option_payload(@ctx, ot))\|ro ok = coalesce_ok_variant(@ctx, ot)\|ro rt = @ir.decl_of(@ir.ret_place()).ty\|@ir.set_none(@ir.ret_place())" novac/src/lower/lower_coalesce.nv
echo "=== D. the shared door both sides call (sem/type_shape.nv)"
grep -n "export fn option_payload\|export fn coalesce_ok_variant\|export fn coalesce_payload\|export fn result_err_variant" novac/src/sem/type_shape.nv
echo "=== E. the return type: checker's @ret_expect is NOT reset for a spawn body; a handler op resets it"
grep -n "@type_block(sbody)" novac/src/check/with_typing.nv
grep -n "@ret_expect = sig.ret_id" novac/src/check/handler.nv
grep -n "export fn FnBuilder @ret_place() -> Local => Local(0)" novac/src/lower/ir.nv
