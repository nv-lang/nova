# A plain string literal default parameter value was refused

Found window Carina, 2026-09-21, while picking the next self-build cause
from the freshly recomputed map (274.5 section 5c).

## The gap

`default_is_value` (`novac/src/check/rules.nv`) -- the door that decides
whether a default parameter expression "reads no scope" and is therefore
legal (D102) -- accepted only `IntLit`/`FloatLit` literals and the two bare
bool names. A plain string literal default (`fn greet(name str, greeting
str = "hello")`) was refused: "outside the subset: a default parameter
value is a number or a boolean here".

A string literal reads exactly as much scope as a number does -- its text
is fixed at the declaration and means the same thing at every call site --
so there was no reason for the boundary to exclude it, and the spec (D102
§"Правило — объявление", point 2) does not: "Default-выражение вычисляется
на месте вызова... может ссылаться на предшествующие параметры и
module-level const", with no restriction to numeric/bool types stated
anywhere.

Novac's own source used this form three times (`emit_c.nv`'s
`src_name str = ""`, `pipeline.nv`, `sem/callables.nv`), all in the current
self-build failure map's "default parameter value" cause.

## Fix

One function, `default_is_value` in `rules.nv`: a `StrLit` leaf is now
accepted, guarded by `interp_slices(leaf_text(t)).len() < 2` -- an
INTERPOLATED string default (`= "${x}"`) is NOT accepted, because
interpolation reads a slot's value, which is exactly the caller's scope
this door exists to keep out. A plain string is; an interpolated one
is not.

No emission change was needed. `@emit_slot_default` (`emit_expr.nv:284`)
already delegates generically to `@emit_expr`, whose `Lit` arm already
handles `StrLit` -- the only door in the way was the checker's own gate.

## Proof

- `novac check` on `fn greet(name str, greeting str = "hello") -> str`
  (two call sites, one omitting the default, one naming it): exit 0.
- `scripts/tools/novac-e1-smoke.sh` on the same probe: **byte-for-byte
  identical to the oracle's stdout**, exit 0 both sides -- full
  emit/compile/link/run pipeline confirmed, not just `check`.
- Control (interpolated string default `= "${x}"`): still refused, same
  message, unchanged -- the fix does not widen past plain literals.
- Control (existing numeric/bool defaults): unchanged, still accepted.
- Both ways: reverted `rules.nv` alone (kept the new `check_test.nv` case),
  rebuilt `novac.exe` -- the new test went RUN-FAIL red; reapplied,
  rebuilt -- green again.
- Self-build batch (`NOVAC_SELF_PATH=novac/src novac.exe check
  <all novac/src/**/*.nv except *_test.nv>`): zero remaining "default
  parameter value is a number or a boolean" occurrences (was 3+); total
  file count with any diagnostic unchanged (72) -- no regression, and the
  three carrier files still have other, unrelated causes left (this fix
  closes its own class, not those files wholesale).

## Carrier caveat

The fix closes the CLASS (`default_is_value`'s node-kind gate), not the
three carriers named above -- any future string-literal default anywhere
in novac's own source or a user's program is covered by the same check,
not by three individual patches.
