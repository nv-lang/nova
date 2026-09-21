# An index-expression call argument was silently uncounted, not refused

Found window Carina, 2026-09-22, while checking whether a single-carrier
self-build bucket ("this call omits `n`, a parameter with no default
value", `novac/src/check/operators.nv`) was a real bug or a harness
artifact -- read the exact call site (`bin_op_of(leaf_kind(kids[1]))`)
and confirmed with a minimal, isolated probe that this reproduces on ANY
call whose argument is an index expression, in any file.

## The gap

`is_arg_node` (`sem/node_questions.nv`) decides whether a call's child
node counts as an argument at all; it asks `is_expr_kind`. `NodeKind.Index`
was never on that list. A call like `takes_one(xs[0])` -- one argument,
plainly written -- had that argument silently SKIPPED by the counting
walk (`unknown_args`/`typed_args`, check/typing.nv and check/type_of.nv),
so the call looked like it had zero arguments and was refused: "this call
omits `n`, a parameter with no default value" -- naming the wrong cause
entirely. The file's own comments on this exact list already document the
IDENTICAL class happening twice before: `RecordCtor` (wave B13, `grab(P {
n: 7 })` refused "this call omits `p`") and `IfExpr` (registry #TBD,
if-in-argument, `pick(if c {10} else {20})` refused "this call omits
`x`"). `NodeKind.Index` was simply never added when those two were.

## Fix

One line, `is_expr_kind` (`sem/node_questions.nv`): `k == NodeKind.Index`
joins the disjunction.

This does NOT make indexing itself compile -- `binds.nv`'s own gate still
refuses `xs[i]` used as a value ("indexing is read but not compiled yet
(E2-b) -- the interop shell carries no `index` for this instance"), and
that refusal is correct and pre-existing (a real, separate, already-named
subset gap; see registry history around #1239/binds.nv). The fix only
lets a call with an index-expression argument reach THAT honest refusal
instead of stopping earlier with a misleading "omits a parameter"
message that names the wrong axis of the problem.

## Proof

- Isolated probe (`fn takes_one(n int) -> int => n + 1  fn f(xs []int) ->
  int { takes_one(xs[0]) }`), oracle: builds clean, exit 0 -- confirms
  this is legal Nova, refused by novac for the wrong reason.
- `novac check` on the same probe, both ways, on the real rebuilt binary:
  **RED** (fix reverted, `novac.exe` rebuilt): "this call omits `n`, a
  parameter with no default value" (the misleading message). **GREEN**
  (fix reapplied, rebuilt): "outside the subset: indexing is read but not
  compiled yet (E2-b) -- the interop shell carries no `index` for this
  instance, so widen novac/probe/shell_probe.nv" -- the honest,
  pre-existing refusal, confirming the fix corrects the DIAGNOSED cause
  without making indexing itself compile.
- `check_test.nv` carries a witness test asserting the misleading message
  is gone (substring "this call omits" absent), not asserting a clean
  compile -- the indexing gap itself is untouched and still legitimately
  refused.

## Self-build map, before/after

Fresh batch check (`NOVAC_SELF_PATH=novac/src novac.exe check <all
non-test files>`), same total (72 files with a diagnostic, unchanged --
this fix corrects a diagnosis, it does not complete a feature): the
"this call omits `n`" first-diagnostic bucket disappeared entirely (was
present before), and "indexing is read but not compiled yet (E2-b)" grew
from 1 file to 3 -- exactly the carriers whose first diagnostic used to
be the misleading message now correctly show the honest, pre-existing
indexing gap instead.

## Carrier caveat

The fix closes the COUNTING class for `Index` specifically -- any other
node kind still missing from `is_expr_kind` would have the identical
symptom and is not swept here. `operators.nv`'s own carrier (5
occurrences of this message, 3 for `leaf_kind(kids[N])`-shaped calls, 2
for `@lit_fits_or_refuse(kids[N], ...)`-shaped ones) is not itself fixed
by this change -- widening the diagnostic's honesty does not compile
indexing, so `operators.nv` will still show diagnostics after this fix,
just the CORRECT ones (the pre-existing indexing gap), not this
misleading one.
