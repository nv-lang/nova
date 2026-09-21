# A record constructor as a match arm's value body was refused as a wrong position

Found window Carina, 2026-09-22, while recomputing the self-build map fresh
(batch `NOVAC_SELF_PATH=novac/src novac.exe check <all non-test files>`)
right after closing the plain-array-literal sibling of this same class
(registry #TBD, RecordCtor-in-array-literal). 7 carriers, e.g.
`novac/src/emit_c/emit_interp.nv`'s `match row.family { PfStr =>
DisplayCall {head: ..., cast: ..., tail: ...} ... }`.

## The gap

`check.nv`'s `RecordCtor` arm gates on `!init_pos && !arg_pos` -- it never
asked `arm_pos`, the flag `@walk` has computed generically since
2026-09-20 ("ARM-BODY POSITION": `arm_pos: at == body_at`, added so that a
`match`/`if` standing as a match arm's body is recognized as a value
position). A record constructor sitting in that exact slot -- `k => Point
{x: 1, y: 2}` -- was refused with the same message as any other illegal
position, even though the slot itself was already legal for two other
value kinds.

## Fix

One line, `check.nv`'s `RecordCtor` arm: gate widened from `!init_pos &&
!arg_pos` to `!init_pos && !arg_pos && !arm_pos`.

No emission change was needed, and the reason is a chain of two existing
doors, not a new one. `lower_match.nv`'s `@lower_arm_body` places a value
arm's body through `@lower_place(arm_e, dest)` -- the SAME door a binding
initializer's own RHS goes through, which is exactly the call site a
record constructor has been legal at since wave B8. Read `@lower_place`
itself (`lowering.nv:603`) to confirm it does not special-case which
caller reached it: it tries `@lower_value_source` (which does not know
`RecordCtor` -- MatchExpr/Coalesce/IfExpr/IfLet only) and falls through to
`@ir.place(e, dest)`, the same M1-bridge assignment an initializer's
RecordCtor already resolves through. The match-arm body and the binding
initializer are, at this door, the identical operation.

## Proof

- `novac check` on the probe (`type Shape value {n int}`, `fn f(k int) ->
  Shape { match k { 0 => Shape {n: 10}  _ => Shape {n: 20} } }`, called
  from `main`): both ways, on the real rebuilt binary.
  - **RED** (patch reverted, `novac.exe` rebuilt from the unpatched
    source): two `E_NOVAC_SUBSET` "a record constructor is compiled only
    as a binding initializer or a call argument", one per arm.
  - **GREEN** (patch reapplied, rebuilt): message gone.
- Full-pipeline emit/compile/run smoke (`novac-e1-smoke.sh`) against the
  oracle, on the same probe wrapped in a real `main` (`println`, not the
  refused-elsewhere `print`-as-expression form): **byte-for-byte identical
  stdout, exit 0 both sides** -- confirms the full emit/compile/link/run
  chain, not just `check`.
- While probing this fix's sibling shape, found a THIRD instance of the
  same class: `if` as a match arm's body (`match k { 0 => if c {1} else
  {2}  _ => 3 }`) is ALSO refused today ("an `if` in value position is not
  compiled yet") -- `IfExpr`'s own gate (`check.nv`, wave B14/if-in-argument)
  checks `!init_pos && !arg_pos`, the identical shape `RecordCtor` had
  before this fix, also missing `arm_pos`. Filed and fixed separately
  (registry #TBD, repro `docs/plans/repro/TBD-if-in-match-arm/README.md`)
  rather than folded in here -- a different node kind, its own gate line.
- Self-build batch, before/after: the "record constructor is compiled only
  as..." FIRST-diagnostic bucket did NOT move (5 files, both before and
  after this fix -- checked with the real carrier `emit_c/emit_interp.nv`
  in isolation, single-file, against the freshly rebuilt binary). Read
  directly why: this fix's OWN target offset in that file no longer
  refuses (confirmed absent), but the file carries a FOURTH, different
  position -- a record constructor as the TAIL EXPRESSION of a `{ ... }`
  block, where that block is itself a match arm's body (`PfFloat => { if
  ... { return DisplayCall{...} } DisplayCall{...} }` -- the un-parenthesised
  final expression of the block). `check.nv`'s own comment on `MatchExpr`
  names why this is a SEPARATE mechanism, not another case for `arm_pos`:
  "the `{...}` tail is a statement position plus the typing route of wave
  B4's tail machinery" -- a block's tail child is walked as `stmt_pos`,
  and whether a VALUE is legal there is decided by tail-specific typing
  (`tail_rules.nv`), not by `@walk`'s `init_pos`/`arg_pos`/`arm_pos` flags
  at all. Fixing THIS position needs reading that mechanism fresh -- not
  attempted this sitting (three narrow fixes plus this investigation is
  enough for one evening); named honestly as an open follow-up rather than
  forced into a fourth quick patch that would not actually be one line.

## Carrier caveat

The fix closes the ARM-BODY position class, not the seven carriers named
above wholesale, and it does not close every illegal position a record
constructor can sit in -- only the two now-recognized value slots
(binding initializer, call argument) plus this third one (match arm
body). A record constructor as, say, a bare tail expression or a function
return value may still be its own, unclaimed class; not investigated here.
