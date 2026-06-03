// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 124.8 Ф.0 — Investigation artifacts

> **Дата:** 2026-06-02.
> **Worktree:** `d:/Sources/nv-lang/nova-p124-8` от main HEAD `7adc3d1a2c0`.

## 1. `value` keyword reservation audit

### Status: NOT reserved в lexer

```
grep "KwValue\|\"value\"" compiler-codegen/src/lexer/token.rs → НЕТ совпадений
```

### Usage as identifier (codebase-wide)

`value` широко используется как **обычный identifier** в stdlib:

```
std/_experimental/collections/deque.nv:77:    ro value = @_items[last_idx]
std/_experimental/collections/deque.nv:91:    ro value = @_items[0]
std/_experimental/crypto/bcrypt.nv:73:        throw InvalidCost { value: cost }
std/_experimental/data/semver.nv:202:        throw InvalidComponent { component: "core", value: core }
... [10+ occurrences as field names и binding names]
```

### Decision: **Contextual keyword**

`value` recognized **только** в позиции `type Name[Generics] [modifiers] value [modifiers] {`. В прочих позициях остаётся обычным identifier'ом.

**Реализация:** в `parse_type_decl` — match на `TokenKind::Ident(s) if s == "value"` вместо нового `TokenKind::KwValue`. Аналогично контекстуальной обработке `alias` в D52.

→ **Backward compat preserved** — существующие `value` identifier'ы продолжают работать.

---

## 2. `()` form usage audit

### Stdlib

```
std/runtime/sync.nv:2169: export type Condvar(ptr)
```

Только 1 occurrence — **positional newtype для FFI**. Не named tuple, не затронут Plan 124.8.

### nova_tests/ (outside Plan 120/124)

```
nova_tests/plan115/ (FFI handles): SqHandle(ptr), PngHandle(ptr), CurlHandle(ptr), Db(ptr), Stmt(ptr)
nova_tests/plan118/ (typed pointers): BufferHandle(*Buffer), LegacyHandle(ptr), TypedHandle(*TypedBuffer)
```

Все — **positional tuple newtype** (bare types, без field names). Plan 124.8 НЕ ломает эту форму.

### nova_tests/plan120 (D215 named tuples — Plan 120)

8 fixtures (`t1_basic_named_tuple`, `t2_types`, `t3_methods`, 5 negative). **ОСТАЮТСЯ** — Plan 124.8 расширяет Plan 120 (multi-line + binding-mut), не ломает.

### nova_tests/plan124_4 + plan124_7

**See §3 delete list.**

---

## 3. Fixtures delete list

### `nova_tests/plan124_4/` — 9 delete + 1 keep

**DELETE (priv-dependent — 9 файлов):**
- `named_tuple_priv_parse_ok.nv` — uses `Secret(priv key, priv salt)`
- `named_tuple_inside_method_ok.nv` — uses `Vec3(priv x, priv y, priv z)`
- `named_tuple_mixed_priv_pub_ok.nv` — uses `Account(priv balance, name)`
- `named_tuple_priv_read_outside_neg.nv` — priv-tuple test
- `named_tuple_priv_init_outside_neg.nv` — priv-tuple test
- `named_tuple_priv_pub_conflict_neg.nv` — `priv pub` conflict (impossible after retract)
- `named_tuple_priv_pattern_outside_neg.nv` — priv-tuple destructure
- `named_tuple_protocol_method_inside_ok.nv` — uses `Vec3(priv x, ...)` для protocol boundary test
- `named_tuple_protocol_external_fn_neg.nv` — uses `Vec3(priv x, ...)` для boundary test

**KEEP (Plan 120 backward compat — 1 файл):**
- `named_tuple_no_priv_ok.nv` — `Point(x f64, y f64)`, no priv used. Verifies Plan 120 backward compat unchanged.

### `nova_tests/plan124_7/` — 8 delete (целиком)

ALL 8 files — type-level priv flip для tuples. Все retract'ed:
- `tuple_priv_flip_inside_ok.nv`
- `tuple_priv_flip_pub_override_ok.nv`
- `tuple_priv_flip_no_flip_ok.nv` (даже без flip — это просто Plan 120 backward compat — может быть KEEP'нут? **DELETE** — функционально дублирует plan120/t1)
- `tuple_priv_flip_record_form_ok.nv` (record-form preserved — purpose of test = ensure record `priv {}` works after tuple priv retract. Может оставить? Plan 124.7 spec — retracted. Этот тест проверяет record-form D220 §3.3.1 которая остаётся. **KEEP**? Renamed → `nova_tests/plan124_8/record_priv_flip_preserved_ok.nv`)
- `tuple_priv_flip_explicit_priv_redundant_ok.nv`
- `tuple_priv_flip_read_outside_neg.nv`
- `tuple_priv_flip_init_outside_neg.nv`
- `tuple_priv_flip_pattern_outside_neg.nv`

**Decision:** DELETE all 8. Record-form priv flip переcoverается в новых plan124_8 фикстурах (см. Ф.5 plan).

### Cumulative delete

- plan124_4: 9 файлов
- plan124_7: 8 файлов
- **Total: 17 deletions** (+ 1 keep в plan124_4 — `named_tuple_no_priv_ok.nv`)

---

## 4. New fixture targets (Ф.5 sketch)

### Positive (≥20)

**Tuple syntax (4):**
- `tuple_single_line_ok.nv` (baseline)
- `tuple_multiline_commas_ok.nv` (NEW)
- `tuple_trailing_comma_ok.nv` (NEW)
- `tuple_multiline_trailing_ok.nv` (NEW)

**Tuple binding mutability (2):**
- `tuple_mut_binding_field_write_ok.nv` — `mut p; p.x = 5.0` works
- `tuple_ro_binding_field_read_ok.nv` — `ro p` only read

**Value-record (5):**
- `value_record_basic_ok.nv` — `type Vec3 value { mut x f64 }` works
- `value_record_mut_method_ok.nv` — `@rotate()` mutates оригинал
- `value_record_array_storage_ok.nv` — `[]Vec3` inline + index mut
- `value_record_pass_by_value_ok.nv` — `mut v Vec3` param = local copy
- `value_record_priv_compose_ok.nv` — `type X value priv { ... }` works
- `value_record_consume_compose_ok.nv` — consume field → type implicit consume

**Binding rules (2 valid pos):**
- `binding_ro_type_mut_ok.nv` — `ro x mut T = ...` парсится
- `binding_mut_type_ro_ok.nv` — `mut x ro T = ...` парсится

**D175 binding-dominates (3 valid pos cases):**
- `binding_mut_acc_can_write_ok.nv`
- `binding_ro_acc_blocks_mut_field_neg.nv` — `ro acc; acc.mut_field = X` → E_READONLY_FIELD
- `binding_ro_acc_blocks_field_method_neg.nv` — `ro acc; acc.mut_method()` → E_READONLY_FIELD

**Record preserved (2):**
- `record_heap_unchanged_ok.nv` — backward compat sanity
- `record_priv_flip_preserved_ok.nv` — Plan 124.7 D220 §3.3.1 для records survives

**Plan 120 backward compat (2):**
- `plan120_no_priv_preserved_ok.nv` — basic tuple still works
- `plan115_positional_tuple_ok.nv` — `type Db(ptr)` form unchanged

### Negative (≥10)

**Tuple priv ban (3):**
- `tuple_priv_field_neg.nv` — `type X(priv f int)` → E_TUPLE_NO_PRIV
- `tuple_pub_field_neg.nv` — `type X(pub f int)` → E_TUPLE_NO_PRIV
- `tuple_per_field_mut_neg.nv` — `type X(mut f int)` → E_TUPLE_NO_PER_FIELD_MOD

**Binding redundancy (2):**
- `binding_ro_type_ro_redundant_neg.nv` — `ro x ro T` → E_REDUNDANT_TYPE_MODIFIER
- `binding_mut_type_mut_redundant_neg.nv` — `mut x mut T` → E_REDUNDANT_TYPE_MODIFIER

**D175 binding-dominates (1 — extends positive coverage):**
- `binding_ro_blocks_mut_field_reassign_neg.nv` — explicit test

**Value-record errors (3):**
- `value_record_typo_neg.nv` — typo detection (sanity)
- `value_record_method_value_param_neg.nv` — passing value record to mut param → copy, mutation not visible (this is positive actually — but expectation matters)
- `value_record_consume_use_after_move_neg.nv` — consume composition still enforces linearity

**Cumulative: 12 positive coverage + actual 10 negative.**

---

## 5. Parser changes summary (Ф.1-Ф.2)

### Ф.1 — Tuple parser

1. `is_named_tuple_decl()` — current check.
2. `parse_named_tuple_fields_with_default(default_priv)` — **modify:**
   - **`skip_newlines()`** после `(` (before first field).
   - **`skip_newlines()`** после comma between fields.
   - **Trailing comma** support (skip newlines after comma, check for RParen).
   - **Ban priv/pub modifier** — emit `E_TUPLE_NO_PRIV` если KwPriv/KwPub.
   - **Ban per-field mut/ro modifier** — emit `E_TUPLE_NO_PER_FIELD_MOD` если KwMut/KwRo.

### Ф.2 — Value-record parser + binding rules

1. `parse_type_decl` — **modify:**
   - Accept `value` Ident-based modifier in position before `{`.
   - Composable с consume/priv (order-independent).
   - `value` + `(` form → reject `E_VALUE_RECORD_PARENS_FORBIDDEN` (value only для `{}` form).

2. `parse_let_stmt` (or wherever binding type annotation parsed) — **modify:**
   - Accept `mut TYPE` после binding name (previously only `ro TYPE` worked).
   - Reject redundant: `ro x ro T` / `mut x mut T` → `E_REDUNDANT_TYPE_MODIFIER`.
   - Allow useful: `ro x mut T` / `mut x ro T`.

---

## 6. AST changes summary (Ф.3)

### `TypeDecl`

Add:
```rust
pub allocation: AllocKind, // Heap | Value
```

Default = Heap (preserved для existing records).

### `NamedTupleField`

Remove:
```rust
// REMOVE:
pub priv_field: bool,
pub visible_to: Vec<String>,
```

### `AllocKind` enum (NEW)

```rust
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum AllocKind {
    Heap,    // type X { ... } — default
    Value,   // type X value { ... } — NEW, stack-allocated
}
```

---

## 7. Risks identified

1. **R0.1 `value` contextual keyword recognition** — must work правильно с
   `consume`/`priv` ordering: `type X value consume priv { ... }` vs
   `type X consume value priv { ... }` etc. Test all 6 permutations of
   3 modifiers in Ф.5.

2. **R0.2 D175 binding-dominates impact на stdlib** — нужно audit `mut field`
   usage в std/ + nova_tests. Could break:
   - `cache.mut_field = X` where `ro cache` binding.
   
   **Mitigation:** перед Ф.3 grep audit + manual review.

3. **R0.3 Plan 115/118 positional tuple unaffected** — verified. No action.

4. **R0.4 Codegen value-record sizeof / layout** — need C struct без pointer
   wrapping. Existing positional tuple codegen pattern (Plan 120) — наследие
   stack-struct emission infrastructure exists.

---

## 8. Decision summary для Ф.1

✅ **Start Ф.1** with parser changes для tuple multi-line + ban priv.

Estimated time Ф.0 actual: ~30 min (audit + artifact write). Within budget.
