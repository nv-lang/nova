# A record constructor inside a plain array literal was refused as a wrong position

Found window Carina, 2026-09-21, while picking the next self-build cause from
the freshly recomputed map (274.5 section 5щ), carrier `novac/src/check/
operators.nv` (`fn equality_key(...) -> []ArgSpec => [ArgSpec {...}, ArgSpec
{...}]`).

## The gap

`check.nv`'s `@walk` computes `arg_pos` for an `ArrayLit`'s children, but the
condition was true only for the `.of(...)` constructor form (registry
#1239's own carrier): `kind == NodeKind.ArrayLit && of_lparen_at >= 0 && at >
of_lparen_at`. A PLAIN bracket literal, `[Point {x:1,y:2}, Point {x:3,y:4}]`
(no `.of(` at all), left every element neither `init_pos` nor `arg_pos`, so
each `RecordCtor` element was refused: "outside the subset: a record
constructor is compiled only as a binding initializer or a call argument --
other positions are not compiled yet (E2, the records/sums horizon)" --
correctly describing the gate's OWN rule, not a bug in the rule's premise.
The premise was just too narrow: a plain array literal's elements are call
arguments too, of the (synthetic) vector constructor.

## Fix

One line, `check.nv`'s `arg_pos` computation for `ArrayLit`: widened from
`of_lparen_at >= 0 && at > of_lparen_at` to `of_lparen_at < 0 || at >
of_lparen_at` -- true for every element when there is no `.of(` at all
(bracket form), and unchanged (still gated on position past the paren) when
there is one.

No emission change was needed. `@emit_array_lit` (`emit_c.nv:427`) already
calls `@hoist_array_lits(e)` on each of its own elements before printing
them (line 442, "the element may itself be a construction... its OWN
emitter hoists what its elements need, the same line the record constructor
has carried since B8") -- and `@hoist_array_lits` already has a `RecordCtor`
arm (falls through to `@emit_record_ctor` when `kind == RecordCtor`, line
493-499). The emitter was already correct for this shape; only the checker
gate disagreed with it. (An earlier reading, 274.5 section 5щ, believed the
`ArrayLit` arm "stops recursion" into its own elements and would need a
matching emitter change -- re-reading `@emit_array_lit` itself, not just
`@hoist_array_lits`'s own doc comment, showed the recursion already
happens one level up, at the array-literal builder, not inside the hoist
dispatcher. Correction recorded here since the plan section still states
the old, wrong belief.)

## Proof

- `novac check` on the plain-literal probe
  (`type Point value {x int, y int}`, `fn f() -> int { ro pts = [Point {x:
  1, y: 2}, Point {x: 3, y: 4}]; 0 }`): both ways.
  - **RED** (patch reverted, `novac.exe` rebuilt from the unpatched
    source): two `E_NOVAC_SUBSET` diagnostics, both "a record constructor
    is compiled only as a binding initializer or a call argument", one per
    element.
  - **GREEN** (patch reapplied, rebuilt): that message is gone entirely --
    the only diagnostic left is the separate, pre-existing "the shell
    novac links into carries no `Vec` instance for Point" (see Carrier
    caveat below).
- `check_test.nv`: new test "record constructor: an ordinary array-literal
  element is a legal position, not just .of(...) (registry #TBD)" --
  asserts the position-refusal message is absent for the plain-literal
  form, and (control) equally absent for the pre-existing `.of(...)` form
  under the same function-bodied shape. PASS.
- Full-pipeline emit/compile/run smoke (`novac-e1-smoke.sh`) could NOT be
  run for this specific fix with a record element type -- see Carrier
  caveat. Checked and confirmed the block is the pre-existing shell gap,
  not this fix, by isolating it: `[]int.of(1, 2)` inside a function body
  compiles clean (`novac check` exit 0), `[]Point.of(...)` inside a
  function body still hits the same shell-instance refusal this fix does
  not touch -- so the position class is provably closed and the remaining
  refusal is provably the other, named gap, not a regression hiding behind
  it.

## Carrier caveat

**The interop-shell gap is real, not a test-harness artefact.** Checked
directly against the real CLI (`novac/target/novac.exe`, not just
`check_test.nv`'s isolated empty `InteropTable.new()`): no probe program
has ever exercised a record-typed `Vec[T]` instance (`grep
Vec____.*_method_ctor novac/src/emit_c/shell.tpl.c` finds only the
primitive instances, `Vec____nova_int`/`Vec____nova_byte`), so
`[]AnyRecordType.of(...)` or a plain `[AnyRecordType{...}, ...]` refuses on
that door regardless of the RecordCtor-position class this fix closes.
Filed separately below.

A MODULE-LEVEL binding of the identical form (`ro pts = []Point.of(...)`,
no enclosing function -- the shape registry #1239's own precedent test
uses) does not hit the interop-instance door at all; a function-bodied
binding does. That asymmetry is a second, independent, unexplored
observation about `check`'s own top-level-vs-function-body path -- noted
here, not chased: out of scope for this fix, which is about element
POSITION, not about which scopes reach the interop check.

The fix closes the POSITION class (`arg_pos`'s gate for `ArrayLit`
children), not the carrier (`operators.nv`) wholesale and not the separate
shell-instance gap -- a record-typed vector literal anywhere in novac's own
source or a user's program still needs `novac/probe/shell_probe.nv` widened
before it can emit, independent of which array-literal form it is written
in.
