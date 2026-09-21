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
  final expression of the block).

  **THIS FOURTH POSITION IS NOT A ONE-LINE GAP -- IT IS A DESIGN BOUNDARY,
  AND THE CURRENT REFUSAL IS PROTECTIVE, NOT INCIDENTAL.** Read
  `check/match_arms.nv`'s `@type_arm` (lines 511-518, wave B9): a Block
  ARM BODY is typed via `@type_block` (a STATEMENT walk) and the fold
  ALWAYS receives `None` for it -- "the arm itself yields no value ...
  which is exactly what None says here", by explicit design, not an
  oversight. `@fold_arm_type`/`@agree_arm_type` then let the match's type
  come from whichever OTHER arms DO produce one; a Block arm contributes
  nothing and is not rejected, just excluded from the agreement. On the
  LOWERING side, `@lower_arm_body` (lower_match.nv) mirrors this exactly:
  `leaves = ... || arm_e.kind_of() == NodeKind.Block` is UNCONDITIONAL, so
  a Block arm NEVER reaches `@lower_place(arm_e, dest)` -- it always goes
  to `@lower_block_stmts_fn`, regardless of whether the match is a value
  or a statement. If such an arm is reached at runtime in a match used as
  a value, `dest` keeps the zero `@lower_match` pre-declares it with (the
  same convention already used for an incomplete match's uncovered arms).

  **Consequence: novac's checker and lowering already AGREE that a Block
  arm body never produces a value for the match** -- which means
  `@type_arm`'s refusal of `RecordCtor` in that block's tail (via the
  ordinary statement walk, `stmt_pos`) is not an accidental narrowness to
  patch with one more flag; it is the thing standing between "refuses a
  form" and "silently returns zero instead of the tail value the oracle
  actually produces" -- exactly the "check clean, wrong answer" class this
  project's own differential guard is built to catch. Widening `RecordCtor`
  to accept `stmt_pos` (or teaching `@type_arm` to thread a Block's tail
  value through) WITHOUT ALSO teaching `@lower_arm_body` to route that
  same tail through `@lower_place` when `dest != no_local()` would very
  likely MISCOMPILE this exact carrier, not just widen the subset.

  **CHECKED AGAINST THE REAL ORACLE, 2026-09-22 00:45 -- CONFIRMED, NOT
  HYPOTHETICAL.** `fn f(k int) -> Shape { match k { 0 => { if k > 100 {
  return Shape{n:-1} }  Shape{n:20} }  _ => Shape{n:20} } }`, built and run
  with the oracle (`nova-cli/target/release/nova.exe build`, isolated in
  its own directory so it is not swept into an unrelated compilation
  unit): builds clean, prints `20` -- the block's BARE TAIL expression (no
  `return`, no `;`) IS the arm's value there, exactly the function-body/
  if-branch tail convention. novac's current design (checker AND lowering
  agreeing a Block arm never produces a value) is a REAL, ORACLE-CONFIRMED
  divergence for this shape, not a guess.

  Not attempted this sitting -- three narrow, safe fixes plus this
  investigation is enough for one evening, and this specific gap needs its
  own scoped wave (checker AND lowering together, oracle-verified first),
  not a fourth quick patch mislabeled as "one more line".

  **CONCRETE NEXT STEP, FOR WHOEVER PICKS THIS UP (read, not guessed):**
  the lowering-side machinery for "a block's tail is a value" ALREADY
  EXISTS, generically, for a function body: `@lower_block_stmts_fn(b,
  tail)` (`lowering.nv:44`) -- when `tail=true`, it finds the block's last
  branch child and, if `@tail_places_value(c)` (tail_rules.nv) says that
  child is a value-producing form, calls `@lower_place(c,
  @ir.ret_place())`. This is EXACTLY the mechanism a match-arm's Block
  body needs -- except the destination is hardcoded to `@ir.ret_place()`
  (the function's own return slot), not an arbitrary local. `@lower_arm_body`
  currently calls `@lower_block_stmts_fn(arm_e, false)` (tail=FALSE,
  always) -- the fix likely generalizes `@lower_block_stmts_fn` (or adds
  a sibling) to take a destination `Local` instead of assuming
  `@ir.ret_place()`, then `@lower_arm_body` calls it with `tail = (dest !=
  no_local())` and that same `dest`. On the CHECKER side, `@type_arm`
  (match_arms.nv:511) needs the mirror change: when the body is a Block
  AND the match is in value position (the fold is being asked for a
  type), thread the block's tail value up through something like
  `@type_branch_value` (tail_rules.nv) instead of unconditionally
  returning `None` -- but ONLY when a value is actually wanted, since a
  Block arm in a STATEMENT match must keep behaving exactly as it does
  today (this is the same "who asked" distinction `@lower_arm_body`
  already makes via `dest != no_local()`, mirrored on the checker side).
  Two files, one door widened together -- not investigated further than
  this pointer.

## Carrier caveat

The fix closes the ARM-BODY position class, not the seven carriers named
above wholesale, and it does not close every illegal position a record
constructor can sit in -- only the two now-recognized value slots
(binding initializer, call argument) plus this third one (match arm
body). A record constructor as, say, a bare tail expression or a function
return value may still be its own, unclaimed class; not investigated here.
