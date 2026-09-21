# `if` in call-argument position: three of four layers scoped, emission layer has an unresolved double-lowering bug

Found window Carina, 2026-09-21, reading the top cause of `docs/plans/274.5-read-own-source.md`
§5с's 84-file self-build map ("7 files: if in value position"). Not started from
`backlog-followups.md` -- surfaced directly while scoping the next self-build wave.

## The gap, before any fix

`fn Checker mut @report_first_leaf_of(kids []Node, if @substs.len() > 0 { "..." } else { "..." })`
-- an `if` used to pick between two string-literal arguments, passed directly as
a function-call argument. Six of novac's own files use exactly this idiom
(`calls.nv`, `literal_rules.nv`, `params.nv`, `tail_rules.nv`, `type_of.nv`,
`emit_c.nv`), independently. `check.nv`'s subset walk refuses `IfExpr` anywhere
but a binding initializer (wave B14) or a tail (the original wave): "outside the
subset: an `if` in value position is not compiled yet".

## What is ALREADY legal (established precedent, not a design question)

`RecordCtor` and `ArrayLit` both widened from "initializer only" to "initializer
or call argument" in earlier waves (B13, B8), by the identical shape of fix:
`check.nv`'s gate condition became `!init_pos && !arg_pos`. `arg_pos` is already
computed correctly for a `Call`'s children by `@walk` (`check.nv:883`,
`arg_pos: kind == NodeKind.Call || kind == NodeKind.MethodCall || ...`) -- no
question of NORM here, only of which doors still need the same widening `if`
has not had yet.

## Four layers, and where each one lives

A value form legal in one more position needs, by the RecordCtor/ArrayLit
precedent, changes at every layer that gates by node kind independently:

1. **`check.nv`'s shape gate** -- `IfExpr => { if !init_pos { refuse } }` needs
   `&& !arg_pos` added, mirroring RecordCtor/ArrayLit exactly. One line.
2. **`sem/node_questions.nv`'s `is_expr_kind`** -- the list that decides whether
   a `Call`'s child counts as an argument AT ALL (`is_arg_node` asks this,
   `typed_free_key`/`bind_slots` read the count from it). `IfExpr` was missing;
   its own header names the EXACT same bug for `RecordCtor` in wave B13 ("grab(P
   { n: 7 }) was refused with 'this call omits p' -- not because the constructor
   was rejected, but because the argument COUNT never saw it"). Confirmed live:
   without this addition, `pick(if c {10} else {20})` was refused with "this
   call omits `x`" even after fix 1 above.
3. **`check/exprs.nv`'s `type_expr` dispatcher** -- the generic per-expression
   typing door that `calls.nv`'s `@type_free_call` argument loop calls on every
   `is_expr_kind` child. Had no `IfExpr` arm; the tail and initializer positions
   never needed one because they call `@type_if_value` directly, bypassing this
   dispatcher entirely. Needs `else if k == NodeKind.IfExpr { @type_if_value(e) }`.
4. **`check/type_of.nv`'s `type_of` dispatcher** -- the READ-BACK half: once
   `@type_if_value` records a type via `@record`, something has to read it back
   through `chan.type_if_known(e.id_of())`, exactly as `Index`/`ArrayLit`/
   `TupleExpr` already do a few lines apart. Missing this gave the fallback ICE
   at `type_of.nv:577` ("expression kind without a type rule").

**Layers 1-4 above are VERIFIED CORRECT** -- with all four applied, `novac check`
on a minimal probe (`pick(if c {10} else {20})` as a call argument) returns
clean, exit 0, and `check_test.nv`'s existing suite stays green.

## Layer 5 (emission) -- NOT solved, reverted rather than shipped broken

`novac emit` on the same probe ICEs. Emission has its OWN two-part gate,
independent of the four above:

- `emit_c.nv`'s `@hoist_array_lits` -- the tree walk that hoists a
  multi-statement value form (`ArrayLit`, `TupleExpr`, `InterpStr`, `Coalesce`,
  `RecordCtor`) into a named temporary BEFORE the statement prints, recorded in
  `@lit_tmp`. `IfExpr` is not in this list, so it is never hoisted.
- `emit_expr.nv`'s `@emit_expr` -- the printer that reads `@lit_tmp` back for
  each of those same kinds. `IfExpr` is not in this list either, so it falls to
  the generic `Bin`-arm's `else { ice("emit: expression kind outside the
  subset") }`.

**First attempt (both these gaps closed, mirroring `Coalesce`'s existing
pattern exactly -- `@lit_tmp[...] = @lo.ir.decl_of(@lo.lower_if_value(e,
@out.type_of(e.id_of()))).name`): a DIFFERENT ICE**, deeper in the pipeline:
`"lower: a block sealed twice (it already returns)"`. This is the exact
failure mode `lowering.nv`'s own comment on `@lower_coalesce` names as
something already fixed once, for `??`, by routing lowering through
`@lower_value_source` (the generic, position-agnostic dispatcher at
`lowering.nv:754`, which the call-argument LOWERING loop already calls
correctly at `lowering.nv:726` -- confirmed by reading: that loop is
independent of `emit_c.nv`'s `@hoist_array_lits` and already handles `IfExpr`
generically, since `@lower_value_source`'s own dispatch has carried an
`IfExpr` arm since wave B14).

**The unresolved question, named honestly rather than guessed at:** two
apparently-independent code paths reach for `@lower_if_value` on the SAME
node -- `emit_c.nv`'s `@hoist_array_lits` (which I added the call to) and
whatever already calls `@lower_value_source` for this call's arguments during
the statement-level walk (`lowering.nv:726`'s loop, or its caller). If both
run on the same `IfExpr` node, the IR builder's block gets sealed twice.
Coalesce's own hoist call (`emit_c.nv`, the line immediately above where I
added mine) looks identical in shape and is proven working (`println(x ?? y)`
compiles), so either Coalesce's lowering does not open/seal a block the same
way an if/else pair does, or there is a sequencing rule between
`@hoist_array_lits` and the statement-level lowering walk that I have not
found -- **not investigated further this session.**

## Both-ways proof on what WAS kept (layers 1-4, then reverted together with 5)

Applied all five layers, watched `novac check` on the probe go from refused
("if in value position") to clean; watched `novac emit` go from the ORIGINAL
named refusal (not reached, since check now accepts) to an ICE. Reverted the
whole six-file diff as one unit (`git checkout --`) rather than leaving
`check` accepting a form `emit` cannot yet produce -- shipping that split
would be exactly the "check clean, emit ICE" class this codebase treats as
the worst kind of green (measured precedent: `docs/plans/274.5-read-own-source.md`
§5о, and D420's own naming of the class). `novac.exe` rebuilt on the reverted
tree; `check_test.nv` still green.

The WIP diff (layers 1-4, correct; layer 5, the broken hoist+emit pair) is
kept as `wip-three-of-four-layers.diff` beside this file, for whoever picks
this back up -- layers 1-4 can very likely be reapplied verbatim once layer 5
is solved.

## Carrier caveat

Fixing one carrier (the `pick(if c {...} else {...})` probe, or any single one
of the six self-build files) is not the acceptance criterion. All four of the
non-emission layers are already GENERAL (node-kind-keyed, not carrier-keyed),
so once emission is solved the fix should close all six files' occurrences of
this cause at once -- but that is a prediction, not yet measured.

## What would unblock this

Reading `lowering.nv`'s block/seal state machine (`begin_else`, wherever
"already jumps"/"already returns" are tracked) closely enough to answer: does
`@hoist_array_lits` run BEFORE or AFTER the statement-level walk that already
lowers call arguments through `@lower_value_source`? If after, `IfExpr`
should NOT be added to `@hoist_array_lits` at all -- the existing
`lowering.nv:726` loop already covers it, and `emit_expr.nv`'s printer needs
to read whatever place THAT loop already produced instead of expecting a
fresh `@lit_tmp` entry. If before, the double-lowering has a different cause
and needs different evidence.
