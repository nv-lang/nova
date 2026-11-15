<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Nova Oracle 0.1.5 — release notes (alpha)

**Nova Oracle 0.1.5** is the fifth alpha of the Oracle line, a quality
patch over 0.1.4. Oracle compiles to C and then to a native binary —
there is no interpreter. This is an early alpha: the language surface
and APIs may change, and compatibility is not guaranteed.

## Fixed (quality patch)

Type safety and memory soundness (registry `docs/plans/221.1-bug-sweep.md`):

- **Fixed-array/slice call boundary** (TBD-1, `fixarr_to_vec_arg`): a
  typed `[N]T` value passed into a `[]T` parameter — and the reverse —
  is refused by name at the checker (`E7301` with the nova-file
  coordinate). Previously this built C that failed at the clang step
  (`_NovaFixArr_N_T` struct passed where `Nova_Vec__T*` was expected),
  or, for some carriers, silently reinterpreted memory. The implicit
  `[N]T -> []T` conversion itself remains an open language question.
- **Generic instancing judged by the compatibility door** (№1015,
  hunt 2026-09-07): a typed slice or spread never lands in a
  differently-instantiated generic slot — the same base type with a
  different concrete type-argument is a distinct monomorphization and
  is refused (`E7301`). Previously this reinterpreted the source's
  bytes (reads past the allocation on a wider element). Untyped literal
  containers keep their D55 narrowing.
- **The last silent value positions** (№1014 residual): the `??`
  right-hand side, an effect-handler operation body (against its
  declared return type), a parameter's default value, variadic pack
  elements, an explicit turbofish argument, and a bare value tail of an
  arrow-function body are all judged by the same compatibility door as
  every other assignment position.
- **std fixes on the same doors**: the `Time` handler's `now()`/
  `now_monotonic()` builds carry explicit widening casts (previously
  invisible D491 narrowing inside std itself).
- **Bare `Ok`/`Err`/`Some` resolution** (№1966, in 0.1.4's line):
  resolves by the position's expected `Option`/`Result`, not the
  file-local sum.
- Configurable file-read buffers in `std.fs`; simplified grapheme
  counting in `std.unicode`.

Also carried by this release: the Five Roads fixes (four
checker/emitter unifications from the package sweep, the
`NovaValue_Output` C-name collision road, the reg-1967 pair),
consistent C type ownership in the emitter, runtime allocation fixes,
Windows packaging of the Rust CLI as `nova.exe` (defect 1968), and
CI fixes (pinned Boehm GC on hosted runners; HTTPS apt archive;
mbedTLS removed from the regression CI).

## Known limitations

- The implicit fixed-array→slice conversion is an open language
  question; copy the elements into a fresh slice (the checker's
  diagnostic names this fix).
- Protocol-typed parameters inside explicit fn-type slots are compared
  permissively (№1915, open; a call that relies on it is undefined).
- `Time.now()` without `import std.time.duration` is refused late
  (at codegen) with a hint, not at the checker (№1918, narrowed).
- One-line `match` arms whose body is an assignment refuse with a
  separator diagnostic even across lines (№TBD-3, narrowed; use `=`
  with braces or commas).
- Pre-existing soundness-lane diagnostic-coordinate defects (D78
  module-path mismatches) are unchanged in this patch.
