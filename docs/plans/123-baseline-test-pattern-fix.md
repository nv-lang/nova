// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 123 — Baseline Test Pattern Fix (`..Default::default()` spread)

> **Создан 2026-06-02.** **Status:** ✅ CLOSED 2026-06-02 (Plan 123 V*.2
> followups infra fix). Branch: `plan-123-v2-followups`. Worktree:
> `nova-p123`. ~70 LOC delta (ast/mod.rs + 2 refactored fixtures).

---

## 1. Контекст

Plan 114 (cancel_safe_attr + fn_eval_max_depth) и Plan 124 (priv_field +
default_field_priv) последовательно добавляли поля в AST-structs FnDecl /
TypeDecl / RecordField. Каждый раз тесты в `sum_schema_registry.rs` и
`lints.rs:2167` ломались на стадии `cargo build --tests --lib` — потому
что fixture-конструкторы listed all fields explicitly.

**Pattern lesson** уже зафиксирован в `simplifications.md`: long-term fix
— Default impl + `..Default::default()` spread.

---

## 2. Изменения

### 2.1 AST Default impls (compiler-codegen/src/ast/mod.rs)

- `impl Default for FnBody` → `External` (no Block/Expr to construct)
- `impl Default for TypeDeclKind` → `Record(vec![])`
- `impl Default for TypeRef` → `Unit(Span::default())`
- `#[derive(Default)]` on `RealtimeAttr` → `None` (с `#[default]` attr)
- `#[derive(Default)]` on `FnDecl`, `TypeDecl`, `RecordField`

### 2.2 Refactored fixtures

- `compiler-codegen/src/codegen/sum_schema_registry.rs` — 7 fixture
  blocks (Option/Result inherit, Nova-body override, RuntimeError,
  drifted, ReadBufferError, ignored types). All переехали на
  `..Default::default()` spread.
- `compiler-codegen/src/lints.rs:2167` — `fake_prelude_peer` builder.

Удалены имена-импорты: `FnBody`, `RealtimeAttr`, `VerifyMode`, `Purity`
там, где раньше указывались только для дефолтных значений.

---

## 3. Acceptance

- **B1** `cargo build --lib` PASS (нет regression в production-paths)
- **B2** `cargo build --tests --lib` PASS — спекта только pre-existing
  `tests/spec_nova.rs` / `tests/common/mod.rs` errors (referencing old
  `nova` crate name, unrelated to baseline fix)
- **B3** Plan 123 field_cache lib tests — 14/14 PASS
- **B4** При добавлении новых AST полей (Plan 124+) тестовые fixtures
  переживают без правок — pattern фундаментально robust

---

## 4. Безопасность Default impls

Default values для AST-types — намеренно **инертны** (`Unit` тип,
`External` body, пустой `Record`). Production code никогда не строит AST
через `Default::default()` — все продакшен-пути explicit. Default
существует исключительно для test-fixtures и as defensive future-proof
для test infra.

---

## 5. Future-proofing protocol

Future plans, добавляющие поля в FnDecl / TypeDecl / RecordField,
теперь **не должны** ломать тестовые fixtures. Если ломают (например,
removed field → struct shrink) — единственное место для обновления
fixture тестов = единичная точка изменения semantics, а не bulk-update
20+ struct-literal callsites.

---

## 6. Closure status

✅ CLOSED 2026-06-02.
