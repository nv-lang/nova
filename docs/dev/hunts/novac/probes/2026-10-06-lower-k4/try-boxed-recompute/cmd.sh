#!/bin/sh
# Architectural probe (hunt 2026-10-06 lower x K4): the two places that answer
# "how does the error of `x?` over a Result reach the body's Result -- same type,
# boxed into `any`, or wrapped into a sum (D55)?". Run from the repository root
# (or pass ROOT=<repo>). Prints both places with line numbers; read notes.txt.
[ -n "$ROOT" ] || ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || ROOT=.
cd "$ROOT" || exit 1
echo "=== A. CHECKER: the pair decided (check/try_rules.nv)"
grep -n "fn Checker mut @try_result_pair_ok\|ro have = @concrete(result_error\|ro want = @concrete(result_error\|ro fits = \|@sum_wraps_error(have, want)\|TRY_ERROR_SUM_WRAP_MSG)" novac/src/check/try_rules.nv
echo "=== B. LOWERING: the pair decided AGAIN (lower/lower_coalesce.nv)"
grep -n "ro rt = @ir.decl_of(@ir.ret_place()).ty\|ro boxed = is_any_ty\|@ir.set_err(@ir.ret_place()" novac/src/lower/lower_coalesce.nv
echo "=== C. EMITTER: a third answer for the cleanup's Failure(e) (emit_c/emit_flow.nv), and the ErrOf print (emit_c/emit_place.nv)"
grep -n "FailureExit(le) => {\|c_maker(@ctx, failure)}(\${@any_box_of" novac/src/emit_c/emit_flow.nv
grep -n "ro arg = if b.boxed" novac/src/emit_c/emit_place.nv
echo "=== D. what the IR carries between them: one bool"
grep -n "boxed bool" novac/src/lower/ir.nv
