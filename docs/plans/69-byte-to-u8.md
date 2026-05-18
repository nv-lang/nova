# Plan 68 — Remove `byte` type alias, canonicalise `u8`

## Status: in-progress

## Problem

Nova has both `byte` and `u8` as names for the same 8-bit unsigned integer type.
Having two names creates ambiguity: identical values can be written two ways, style
inconsistency creeps into the stdlib and tests, and documentation is unclear about
which form is idiomatic.

## Decision

* Remove `byte` as a named primitive type.
* `u8` is the sole canonical name for the 8-bit unsigned integer.
* `[]u8` is the canonical type for binary data (byte slices).
* The tagged-template tag `bytes` (D48: `` bytes`deadbeef` ``) is a **function name**,
  not a type — it is **unaffected** by this change.
* Word "byte" appearing in prose comments describing byte counts / bit widths
  is English text — also **unaffected**.

## Spec change

Add D125 in `spec/decisions/02-types.md`:

> **D125** — `byte` is removed as a built-in type alias. Use `u8` everywhere.
> Binary data slices use `[]u8`. Existing code using `byte` must migrate to `u8`.

## Scope

| Location | Occurrences |
|---|---|
| `spec/decisions/` | ~65 |
| `std/` | ~212 |
| `nova_tests/` | ~92 |
| `examples/` | ~1 |

## Steps

1. [x] Create this plan document.
2. [ ] Add D125 to `spec/decisions/02-types.md`.
3. [ ] Mass-replace `byte` → `u8` in `spec/decisions/` (skip `bytes` tag and prose).
4. [ ] Mass-replace `byte` → `u8` in `std/`.
5. [ ] Mass-replace `byte` → `u8` in `nova_tests/`.
6. [ ] Mass-replace `byte` → `u8` in `examples/`.
7. [ ] Build + test.
8. [ ] Commit.
