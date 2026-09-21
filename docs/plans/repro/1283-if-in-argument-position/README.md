# `if` in call-argument position: all five layers, closed

Found window Carina, 2026-09-21, reading the top cause of `docs/plans/274.5-read-own-source.md` section 5c's self-build map. This README supersedes the earlier WIP version (kept below as history) -- the fifth layer is now solved and proven.

## The gap

`fn Checker mut @report_first_leaf_of(kids []Node, if @substs.len() > 0 { "..." } else { "..." })` -- an `if` used to pick between two string-literal arguments, passed directly as a function-call argument. Six of novac's own files use exactly this idiom independently. `check.nv`'s subset walk refused `IfExpr` anywhere but a binding initializer (wave B14) or a tail (the original wave).

## Four checker/typing layers (unchanged from the WIP version, all independently verified)

1. **`check.nv`'s shape gate** -- `IfExpr => { if !init_pos { refuse } }` widened to `if !init_pos && !arg_pos`, the identical shape `RecordCtor`/`ArrayLit` already use (B13/B8).
2. **`sem/node_questions.nv`'s `is_expr_kind`** -- `IfExpr` was missing, so the argument COUNT (`is_arg_node` → `bind_slots`) never saw the node at all and refused "this call omits `x`" -- the exact class the file's own header names for `RecordCtor` in wave B13.
3. **`check/exprs.nv`'s `type_expr` dispatcher** -- had no `IfExpr` arm; tail and initializer call `@type_if_value` directly and never reach this generic dispatcher, but an argument does.
4. **`check/type_of.nv`'s `type_of` dispatcher** -- the read-back door, `chan.type_if_known`, the same pattern `Index`/`ArrayLit`/`TupleExpr` use.

## The fifth layer: emission -- found and closed this session

**First attempt (the WIP version): wrong door.** Mirroring `Coalesce`'s existing hoist call in `emit_c.nv`'s `@hoist_array_lits` (`@lit_tmp[...] = @lo.ir.decl_of(@lo.lower_if_value(e, ...)).name`) gave a deeper ICE: `"lower: a block sealed twice (it already returns)"`.

**Root cause, read precisely.** Novac carries TWO parallel emission systems. An older one walks the AST directly at print time (`emit_c.nv`'s `@hoist_array_lits`/`@emit_expr`, for `ArrayLit`/`RecordCtor`/`TupleExpr`/`InterpStr`/`Coalesce`). A newer one builds a sealed-block IR graph *before* printing (`emit_flow.nv`, "wave M2b-2, step 3b": `@lo.ir.finish()` builds the whole graph, *then* one pass prints it -- "the order is now the reverse: finish first, then ONE walk over what it returned"). `emit_expr.nv`'s own comment on `println` names the exact failure mode this session hit: "the printing this branch used to do went through the PRINT-TIME HOIST, and that hoist is exactly what refused with `lower: a block sealed twice` under the walk of step 3b" -- the identical ICE, already caught and retired once for `println`, for the same reason.

`@lower_if_value` opens/closes IR blocks and MUST run before `@lo.ir.finish()` seals the graph. `@hoist_array_lits` runs at print time, *after* the seal -- calling lowering from there tries to write into an already-sealed book. This is not double-lowering of one node; it is lowering in the wrong phase.

**A second wrong hypothesis, also read precisely and discarded.** `lowering.nv:726`'s argument-lowering loop (`match @lower_value_source(c, ty) { ... }`) looked like a pre-existing, generic "lower any call argument" pass. It is not: it lives inside `@lower_println` (line 694), specific to `println`. The nearest general mechanism, `@hoist_ordered_args` (line 647), only fires at two or more *call-containing* arguments (F65's concern is interleaving order, not existence) -- a lone `if`-value argument, containing no nested call, never reaches it. **No pre-finish lowering pass covered an ordinary call's `if`-value argument at all.**

**The fix: a new pre-finish pass, in the right phase.** `@hoist_sealed_args`, in `lower/lowering.nv`, called from the same two sites `@hoist_ordered_args` already is (`@lower_place`, a binding initializer's RHS, and `@lower_eval`, a statement-position call) -- *before* `@lo.ir.finish()`. For each argument whose own kind needs sealed blocks (`IfExpr`, `Coalesce` -- `@lower_value_source`'s own dispatch list), it calls `@lower_value_source` and records the result via `@ir.remember_hoist`, unconditionally (no "two or more" gate -- a sealed-block form needs pre-lowering regardless of count). **No printer-side change was needed at all**: `@emit_bound_args` (`emit_expr.nv`) already reads a pre-lowered argument back through `@lo.ir.local_of_node` -- the exact door `@hoist_ordered_args`'s own hoists already go through. The WIP version's `emit_c.nv`/`emit_expr.nv` changes were reverted; they were never needed.

## Proof

- `novac check` on the probe: exit 0.
- `novac emit` on the same probe: exit 0, zero diagnostics -- the exact step that ICEd before. The emitted C shows the expected shape directly: the `if`/`else` writes a temporary, the call reads it (`nova_int r = novac_fn_..._pick(...)` reading `_novac_l3`, set by the preceding `if`/`else`).
- `scripts/tools/novac-e1-smoke.sh` on the probe: **byte-for-byte identical to the oracle's stdout**, exit 0 both sides.
- Both ways, on the new layer alone: reverted `lower/lowering.nv` (kept the four checker layers and the new `check_test.nv` case) -- `novac emit` on the probe ICEs (`emit: expression kind outside the subset`, the *other* pre-existing ICE, confirming the checker layers alone are not sufficient); reapplied -- clean again.
- `novac/src/check/check_test.nv`, `novac/src/parse/parse_test.nv`, `novac/src/lower/lower_test.nv`: all green, no regression.
- Self-build batch: "an `if` in value position" dropped from 8 files (first cause) to 5 -- the five remaining are a *different* position this fix does not claim to cover (one read carrier: `ro (vfirst, vn) = if ... { ... } else { ... }` -- `if` as the initializer of a TUPLE-DESTRUCTURING bind, not a plain name -- a separate, narrower gap, pre-existing, not introduced by this fix). No regression: total self-build file count unchanged (72), consistent with these files carrying other, unrelated causes too.

## Carrier caveat

The fix closes the CLASS (five node-kind-keyed doors, not six file-keyed patches): any future `if`-value used as a call argument anywhere in novac's own source or a user's program goes through the same five doors.

## What this fix does NOT cover

`if` as the initializer of a tuple-destructuring bind (`ro (a, b) = if ... {...} else {...}`) is a separate, narrower gap this fix does not touch -- five files still carry it as their current first cause. Not investigated this session.

---

## History: the original WIP note (superseded above, kept for the record)

Three of four checker/typing layers were verified correct; the emission layer was not yet solved, and the whole six-file diff was reverted rather than shipped with `check` accepting a form `emit` could not yet produce -- the "worst kind of green" this registry names by name (D420, plan 274.5 section 5o). The WIP diff from that attempt (layers 1-4 correct; layer 5, the broken hoist+emit pair) was kept beside this file as `wip-three-of-four-layers.diff` for the next attempt; it has since been superseded by the working fix above and the emit_c.nv/emit_expr.nv half of it was never applied.
