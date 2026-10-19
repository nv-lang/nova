# Carina-only probes for D494

These are probe designs, **not runnable fixtures yet**. The current standard
conformance runner uses the frozen Rust Oracle, which has no `?T` type syntax;
adding the proposed sources to `spec_tests/conformance` now would fail for the
wrong compiler. Keep this directory out of fixture discovery until task 108
provides a running Carina path. Do not add Oracle fallback or suppress a red
result.

## Positive probe: `pos_1.nv` (after Carina build path exists)

Expected observable: `main` prints `option-prefix-ok` and exits 0. Compile both
the shorthand and explicit canonical type in the same program; use equivalent
values and consume them with the existing `Option` matching rules.

```nova
type PrefixHolder { item ?int }

fn as_prefix(x ?int) -> ?int => x
fn nested() -> ??int => Some(Some(7))
fn prefix_slice() -> ?[]int => Some([7])
fn slice_prefix() -> []?int => [Some(7)]
fn canonical(x Option[int]) -> Option[int] => x
fn expression_try(x Option[int]) -> Option[int] {
    ro value = x?
    Some(value)
}

fn main() Io -> () {
    ro a ?int = Some(7)
    ro b Option[int] = Some(7)
    ro nn ??int = Some(Some(7))
    ro ps ?[]int = Some([7])
    ro sp []?int = [Some(7)]
    match a {
        Some(x) => match b {
            Some(y) => match nn {
                Some(Some(z)) => if x == y && y == z { println("option-prefix-ok") } else { println("bad") }
                _ => println("bad")
            }
            _ => println("bad")
        }
        _ => println("bad")
    }
}
```

When enabled, additionally compare Carina check/emit output for equivalent
`?T` and `Option[T]` declarations after normalizing source locations; both must
resolve to the same canonical Option type and emitted ABI. The call to
`expression_try` is the regression control that `x?` remains the existing
expression operator, not a type parser rule.

## Negative probes (after diagnostic spelling is fixed)

Each expected refusal must be line-pinned with `nova:expect`; no probe runs in
CI before the Carina lane is available.

* `neg_postfix.nv`: `ro x int? = 1` — reject type-position postfix `T?`; the
  English diagnostic must contain `write ?T`.
* `neg_missing_operand.nv`: `ro x ? = None` — reject missing type after prefix
  `?` (do not recover it as expression propagation).
* `neg_bad_nesting.nv`: `ro x ??[] = None` — reject the incomplete list element
  type; no partial Option/list node may be accepted.

## Read-only implementation map (no source change before Carina build)

* `novac/src/lex/lex.nv`: lone `?` already has `TokenKind.Question`, distinct
  from `Question2`; expression parser consumes it in `parse/expr.nv`.
* `novac/src/parse/type_ref.nv`: the one type-reference door returns a flat
  token sequence. Add prefix-recursive handling there, preserving `[]?T` vs
  `?[]T`, nested `??T`, and recursively consumed function/tuple/generic types.
  Diagnose `T?` in type position at this door/caller with the D494 hint; do not
  touch expression parsing.
* `novac/src/sem/typeref.nv`: `ty_of_type_ref` maps source shapes into interned
  `TyId`s; slice and generic applications already recurse through this door.
  Canonicalize prefix sugar to the same `Option` application term used by
  `Option[T]`; no codegen or ABI branch.
* Existing `Question` expression handling in `parse/expr.nv` and D85 lowering
  are regression controls, not implementation targets.

Before promoting these designs to executable fixtures: prove pre-fix RED,
post-fix GREEN on the running Carina binary, sabotage the prefix parser branch
to get RED again, restore it to GREEN, and capture native output/exit code.
