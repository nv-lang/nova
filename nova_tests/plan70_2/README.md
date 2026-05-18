# Plan 70.2 — LinkedList sum-type mono fixtures

## Scope

[Plan 70.2](../../docs/plans/70.2-linkedlist-sum-type-mono.md):
Generic sum-type `LinkedList[T]` mono'd codegen — closed 2026-05-19.

## Fixes (compiler-codegen/src/codegen/emit_c.rs)

**Fix 1 — Drain timing (plan-70.2):** добавлен `self.drain_generic_type_worklist()?;`
после emit_test loop. Без этого generic-type instances enqueued via
`try_infer_variant_mono_args` (Cons-constructor type inference) никогда
не processed → struct decl `Nova_LinkedList____nova_int` missing →
`use of undeclared identifier` в C → CC-FAIL.

**Fix 2 — @method strip для mono'd sum dispatch (plan-70.2b):** method
name `@len` (с `@` префиксом из parser) lookup'илось в `generic_type_methods`
напрямую: `m.name == *method`. Но FnDecl `m.name = "len"` (без `@`) →
lookup всегда fail → fallback emit literal `l->@len()` в C → invalid
identifier. Fix: `method.trim_start_matches('@')` для lookup +
`base_method_name` construction. 4-line change в emit_call Member arm.

## Fixtures

| File | Coverage |
|---|---|
| `f1_linkedlist_int_pos.nv` | LinkedList[int] data construction (Nil + Cons), pattern matching (len/sum) via free fns ✅ |
| `f2_linkedlist_methods_pos.nv` | LinkedList[int] @method dispatch (`@len` instance method, mono'd receiver) ✅ |

## Acceptance

- ✅ `f1_linkedlist_int_pos` — LinkedList[int] data + free-fn pattern matching
- ✅ `f2_linkedlist_methods_pos` — @method dispatch на mono'd sum-type works
- ✅ 0 regressions vs Plan 70.1 baseline в полном nova test

## Remaining deferred (separate plan если возникнет)

1. **Unit-variant inference** (Plan 48 Ф.7.4 V2): `Nil` constructor
   currently falls back to erased `nova_make_LinkedList_Nil()`
   (returns base `Nova_LinkedList*`). Mixed with mono'd Cons —
   works because Cons constructor accepts erased Nil pointer. Если
   strict mono Nil ever needed, Plan 48 V2 refactor.
2. Other method-name collision scenarios (StringBuilder.append family)
   — не observed в тестах после fix; deferred.
