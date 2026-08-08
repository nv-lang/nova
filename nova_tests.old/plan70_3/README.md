# Plan 70.3 — char↔int distinction fixtures

## Scope

[Plan 70.3](../../docs/plans/70.3-char-int-mono-distinction.md) +
[Spec D128](../../spec/decisions/02-types.md#d128) — `char` distinct
from `int` в codegen mono'd generics.

## Fixes (compiler-codegen)

**Runtime headers** (`nova_rt/`):
- `nova_rt.h:18` — `typedef int64_t nova_char;` (zero-cost alias)
- `array.h:103+` — `NOVA_ARRAY_DECL(nova_char)` + `NOVA_ARRAY_IMPL`
- `array.h:215+` — `NovaOpt_nova_char` constructors + `nova_opt_eq_nova_char`
- `array.h:432` — `nova_str_char_at` returns `NovaOpt_nova_char` (was nova_int)

**Codegen** (`src/codegen/`):
- `emit_c.rs:2680` — `type_ref_to_c "char" => "nova_char"` (was nova_int)
- `emit_c.rs:9834` — `CharLit` emits `((nova_char)<cp>LL)` cast
- `emit_c.rs:18275` — `infer_expr_c_type CharLit => "nova_char"`
- `emit_c.rs:687-693` — pre-populate `novaopt_decls_seen` с `nova_char`
  (skip lazy emit — runtime already provides typedef)
- `emit_c.rs:10086-10094` — `Nova_StringBuilder* + char` accepts `nova_char|nova_int`
  (backward compat)
- `external_registry.rs:228` — parallel `"char" => "nova_char"`
- `external_registry.rs:250` — `[]char → NovaArray_nova_char*`

## Fixtures

| File | Coverage |
|---|---|
| `f1_option_char_vs_int_pos.nv` | Option[char] / Option[int] independent C types, basic match, None cases ✅ |
| `f2_array_char_vs_int_pos.nv` | []char / []int distinct mangling, index access, char literals в array ✅ |

## Acceptance

- ✅ Distinct C types: `NovaOpt_nova_char` vs `NovaOpt_nova_int`,
  `NovaArray_nova_char` vs `NovaArray_nova_int`
- ✅ 2 fixtures PASS (basic data construction + access)
- ✅ 0 regressions: full nova test 801+ PASS sustained
- ✅ Spec D128 published
- ✅ Backward-compat для `sb + char` (accepts both types)

## Remaining (Ф.4 type-checker tightening — deferred)

Currently type-checker rejects обычное type mismatch (Option[char] vs
Option[int] в slot assignment), но **некоторые** edge cases могут
проскользнуть через C-level structural compatibility (оба `int64_t`
underlying). Examples:
- Implicit coercion in turbofish positions
- Generic instance shared mangling до full mono pass

These edge cases deferred — current fix solves dominant collapse vector
(generic mono mangling identity). Type-checker hardening — отдельный
follow-up если когда-нибудь нужен.
