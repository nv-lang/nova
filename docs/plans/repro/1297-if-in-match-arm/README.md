# An `if` value as a match arm's body was refused as a wrong position

Found window Carina, 2026-09-22, immediately after closing the sibling
class for `RecordCtor` (registry #TBD, RecordCtor-in-match-arm) -- checked
whether `IfExpr`'s own gate (`check.nv`, wave B14 / the earlier
if-in-argument fix, same evening) had the identical shape, since both node
kinds' gates were written by the same hand at the same time and one had
just been proven to have this exact gap.

## The gap

`check.nv`'s `IfExpr` arm gated on `!init_pos && !arg_pos` -- the same
shape `RecordCtor`'s gate had before its own fix, and missing the same
thing: `arm_pos`, computed generically by `@walk` since 2026-09-20 for a
value standing as a match arm's body. `match k { 0 => if c {1} else {2}
_ => 3 }` was refused: "outside the subset: an `if` in value position is
not compiled yet" -- this is very likely the root of one of the self-build
map's own buckets ("an `if` in value position is not compiled yet", 3
files as of the map recomputed 2026-09-22).

## Fix

One line, `check.nv`'s `IfExpr` arm: gate widened from `!init_pos &&
!arg_pos` to `!init_pos && !arg_pos && !arm_pos`.

No emission or typing change was needed. `exprs.nv`'s `type_expr`
dispatcher's `IfExpr` arm is already unconditional (calls
`@type_if_value(e)` with no position gate of its own -- read directly,
not assumed), and lowering already places an `IfExpr` value through the
position-agnostic `@lower_value_source` dispatch (the same door the
if-in-argument fix, same evening, proved reachable from `@lower_place`,
which `@lower_arm_body` also calls). The shape walk in `check.nv` was the
only door left closed.

## Proof

- `novac check` on the probe (`fn f(k int, c bool) -> int { match k { 0 =>
  if c {1} else {2}  _ => 3 } }`, called from `main`): both ways, isolated
  to this one line specifically (the sibling `RecordCtor` fix was already
  committed/applied in the same build; only this line was toggled).
  - **RED** (just this condition reverted to `!init_pos && !arg_pos`,
    `novac.exe` rebuilt): "outside the subset: an `if` in value position
    is not compiled yet".
  - **GREEN** (restored, rebuilt): clean, exit 0.
- Full-pipeline emit/compile/run smoke (`novac-e1-smoke.sh`): byte-for-byte
  identical stdout against the oracle, exit 0 both sides.

## Carrier caveat

The fix closes the ARM-BODY position class for `IfExpr` specifically, the
same way the sibling fix closed it for `RecordCtor` -- it does not claim
every carrier in the self-build map's "if in value position" bucket is
this exact shape; the bucket needs re-measuring after this fix lands to
confirm how much of it this closes versus a still-open, different `if`
position (the map noted a tuple-destructuring bind's initializer as a
separate, unclaimed `if`-position gap earlier this evening).
