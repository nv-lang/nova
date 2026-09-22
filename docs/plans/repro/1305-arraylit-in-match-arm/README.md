# An array literal as a match arm's value body was refused as a wrong position

Found window Carina, 2026-09-22, sweeping the same class one node kind
further after fixing `RecordCtor` and `IfExpr` (registry #TBD each, same
evening): `ArrayLit`'s own gate has the identical shape.

## The gap

`check.nv`'s `ArrayLit` arm gated on `!init_pos && !arg_pos` (wave B8's own
threshold, set by a census of novac's own carriers at the time -- "12
field carriers and 9 argument carriers ... zero bare tails or
assignments"), never asking `arm_pos` -- the flag `@walk` computes
generically since 2026-09-20, after wave B8's census was taken. `match k {
0 => [1, 2, 3]  _ => [4, 5] }` was refused: "outside the subset: an array
literal is compiled only as an initializer, a call argument or a
constructor field -- other positions are not compiled yet".

Matches a self-build map bucket recomputed the same evening: "an array
literal is compiled only as..." (2 files, e.g. `novac/src/sem/coerce.nv`).

## Fix

One line, `check.nv`'s `ArrayLit` arm: gate widened from `!init_pos &&
!arg_pos` to `!init_pos && !arg_pos && !arm_pos`.

No emission change was needed, same reasoning as the `RecordCtor` sibling:
`@lower_arm_body` (lower_match.nv) places a value arm's body through
`@lower_place`, the same door an initializer's own array literal already
resolves through since wave B8 (`@lower_value_source` does not know
`ArrayLit` either, so `@lower_place` falls through to the same M1-bridge
assignment an initializer already uses).

## Proof

- `novac check` on the probe (`fn f(k int) -> []int { match k { 0 => [1,
  2, 3]  _ => [4, 5] } }`, called from `main`): both ways, on the real
  rebuilt binary, isolated to this one line (the sibling `RecordCtor`/
  `IfExpr` fixes were already committed/applied in the same build).
  - **RED**: two `E_NOVAC_SUBSET` "an array literal is compiled only as an
    initializer, a call argument or a constructor field", one per arm.
  - **GREEN**: message gone.
- Full-pipeline emit/compile/run smoke (`novac-e1-smoke.sh`): byte-for-byte
  identical stdout against the oracle, exit 0 both sides.
- `check_test.nv` carries a witness test.

## Carrier caveat

The fix closes the ARM-BODY position class for `ArrayLit`, the same way
its two siblings closed it for `RecordCtor` and `IfExpr` this same
evening. It does not claim every position an array literal can stand in
is now legal -- a bare tail (function body / `if`-branch tail, not a
match-arm body) and a plain assignment were explicitly measured OUT at
wave B8 by carrier count and are not reopened here; if the fourth,
deeper "block tail" position (named in the `RecordCtor`-in-match-arm
repro doc) is ever fixed for one node kind, `ArrayLit` likely needs the
identical extension, not assumed to come for free.
