// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 127 — Value-record escape analysis + auto-heap-promote

> **Создан 2026-06-03.** Draft scaffold.
> **Status:** 🆕 PLANNED (черновик).
> **Trigger:** closes Plan 124.8 V2 followup `[M-124.8-value-heap-promote]` — `&value` operator + auto-promote.
> **Gated:** Plan 124.8 ✅ (value-record landed) + Plan 118 Ф.2 (escape analysis infrastructure) — required для reuse `escape_analyze` walker и `auto-promote` trigger machinery.
> **Эстимат:** ~2-3 dev-day.
> **Model:** Opus 4.7 + Thinking ON (codegen-synthesis, escape semantics).

---

## 1. Контекст

### 1.1 Plan 124.8 V2 — где остановились

После Plan 124.8 V2 (merge `bd63faefa1c`, 2026-06-03) value-records аллоцируются
на стеке:

```nova
type Vec3 value { x f64, y f64, z f64 }

fn make() -> Vec3 =>
    Vec3 { x: 1.0, y: 2.0, z: 3.0 }  // stack-init NovaValue_Vec3
```

`@`-методы получают receiver = `NovaValue_Vec3*` через helper
`prepare_method_recv` (`emit_c.rs`).

### 1.2 Чего нет

**Взятие адреса `&v`** на value-record local пока **запрещено** —
адрес может escape наружу функции, pointer станет dangling:

```nova
fn bad() -> *Vec3 =>
    ro v = Vec3 { x: 1, y: 2, z: 3 }
    &v                                  // ← address of stack slot escapes ❌
```

### 1.3 Что предлагается

**Escape analysis** для value-record locals + **auto-promote** на heap при
detected escape — Go-style. Без Rust lifetimes.

```nova
fn ok_local_only() -> f64 =>
    ro v = Vec3 { x: 1, y: 2, z: 3 }
    ro p = &v                            // ✅ no escape — stays NovaValue_Vec3* на stack
    p.x

fn ok_auto_promoted() -> *Vec3 =>
    ro v = Vec3 { x: 1, y: 2, z: 3 }
    &v                                   // ✅ escape detected → auto-promote v to heap
                                         //   v lives as Nova_Vec3* (heap)
                                         //   returned &v = Nova_Vec3* (valid)
```

User-visible типы те же — `*Vec3`. Compiler diff: stack-allocated
`NovaValue_Vec3` vs heap-allocated `Nova_Vec3*` под капотом.

---

## 2. Design

### 2.1 Escape conditions

Local value-record `v` promote'ится на heap если **любое** из:

1. `&v` возвращается из функции (`fn f() -> *Vec3 => &v`).
2. `&v` сохраняется в heap-аллоцированное поле (`obj.field = &v`).
3. `&v` передаётся в closure capture (`|| &v`).
4. `&v` сохраняется в global / module-level binding.
5. `&v` передаётся в fn arg где параметр sink'и (escape transit).
6. Conservative fallback: address escapes через chain dataflow analysis cannot prove.

V1: matches Plan 118 Ф.2 V1 OVER-promote — если **любая** uncertainty,
promote. Ф.6 «precise mode» — Plan 127 V2 followup
`[M-127-precise-escape]`.

### 2.2 Codegen — два режима

| Случай | C output |
|---|---|
| No escape | `NovaValue_Vec3 v; v.x = 1.0; ...` + `NovaValue_Vec3* p = &v;` (stack) |
| Escape detected | `Nova_Vec3* v = nova_alloc(sizeof(Nova_Vec3)); v->x = 1.0; ...` (heap) + `Nova_Vec3* p = v;` |

Type-level binding type unchanged (`Vec3` или `*Vec3` per Plan 118
binding rule). Diff виден только в C output.

### 2.3 Mixed escape / no-escape сценарии

Если value-record `v` участвует в escape AND no-escape paths
(branch'и), conservative — heap-allocate. V1 simple. V2 path-sensitive
analysis = `[M-127-path-sensitive-escape]`.

### 2.4 Method receiver compatibility

`@`-методы value-record получают `NovaValue_Vec3*` в stack-mode
(Plan 124.8 V2) и `Nova_Vec3*` в heap-mode. Helper `prepare_method_recv`
расширяется на auto-promote case:

```rust
fn prepare_method_recv(obj_expr, alloc_kind) -> CExpr {
    match alloc_kind {
        AllocKind::Stack => emit_address_of(obj_expr),     // &v
        AllocKind::HeapPromoted => obj_expr,                // already Nova_X*
    }
}
```

### 2.5 Spec changes

- **D228 amend §«escape & auto-promote»** — value-record specific rules.
- **Cross-ref D228 ↔ D216 §4** — Plan 118 escape machinery reuse.
- **Q127.1** — array element `&arr[i]` — heap-promote whole array или
  per-element? **Recommend:** per-element promote — array of `Nova_Vec3*`
  вместо array of `NovaValue_Vec3`. Coordinate с
  `[M-124.8-value-record-array-inline]` если будет landed.

### 2.6 Error / lint conditions

- `W_VALUE_RECORD_UNNECESSARY_PROMOTE` — escape detected но user мог
  бы избежать (e.g., вернул by-value вместо `&`). Lint, not error.
- `E_VALUE_RECORD_ESCAPE_AFTER_CONSUME` — `&v` after `consume v`. Hard error.

### 2.7 Composability с consume / priv / readonly

- `consume` value-record: escape after consume = hard error (D162 violation).
- `priv` field на promoted value-record: остаётся priv через
  `Nova_Vec3*` access.
- `readonly` binding на promoted value-record: `Nova_Vec3* const` в C.

---

## 3. Phases

### Ф.0 — Investigation + audit Plan 118 Ф.2 infrastructure

- Read `compiler-codegen/src/types/mod.rs:walk_typeref + escape_analyze`.
- Map current trigger conditions (primitives + tuples) → identify extension
  points для value-record.
- Probe-test current `&v` behavior на value-record (vs primitive vs tuple).
- **Эстимат:** 0.3 dev-day.

### Ф.1 — AllocKind tri-state + value-record stack/heap routing

- Extend `AllocKind` enum: `{ Heap, Value, ValueHeapPromoted }`.
- Type-resolver: на момент binding allocation решает stack/heap based on
  escape analysis result.
- Codegen branch: `ValueHeapPromoted` → `nova_alloc(sizeof(Nova_X))`
  + heap-init pattern.
- **Эстимат:** 0.5 dev-day.

### Ф.2 — escape_analyze walker extension

- В Plan 118's `escape_analyze` walker — добавить value-record locals в
  trigger conditions.
- Reuse существующих primitives/tuples logic — same dataflow analysis,
  different type category.
- 5 escape conditions (см. 2.1) implemented.
- **Эстимат:** 0.7 dev-day.

### Ф.3 — Codegen heap-allocation path

- `emit_value_record_lit` extended: `match alloc_kind { Value → stack, ValueHeapPromoted → heap }`.
- `prepare_method_recv` extended for `ValueHeapPromoted` (см. 2.4).
- Field access `.x` — `&` for stack, `->` for heap-promoted.
- **Эстимат:** 0.5 dev-day.

### Ф.4 — Lint + diagnostic

- `W_VALUE_RECORD_UNNECESSARY_PROMOTE` — emit when escape detected but
  could be by-value return.
- `E_VALUE_RECORD_ESCAPE_AFTER_CONSUME` — hard error.
- Diagnostic message includes both escape origin point и promotion site.
- **Эстимат:** 0.3 dev-day.

### Ф.5 — Tests

- `nova_tests/plan127/` — ≥12 positive + ≥6 negative.
- Positive:
  - Local-only `&v` — stays stack.
  - Return `&v` — auto-promote heap.
  - Closure capture `|| &v` — auto-promote.
  - Heap field store — promote.
  - Method receiver works in both modes.
  - Mixed branch — conservative promote.
- Negative:
  - `&v` after consume — E_VALUE_RECORD_ESCAPE_AFTER_CONSUME.
  - Cycle detection edge cases.
- Lint:
  - Unnecessary promote — W_VALUE_RECORD_UNNECESSARY_PROMOTE emitted.
- **Эстимат:** 0.4 dev-day.

### Ф.6 — Spec + docs

- D228 amend §«escape & auto-promote» (~80 lines).
- Cross-ref D228 ↔ D216 §4 (Plan 118 escape machinery).
- `docs/value-records.md` — обновить раздел `&value` semantics.
- README D-block index updated.
- **Эстимат:** 0.2 dev-day.

### Ф.7 — Closure

- 3 logs (project-creation + simplifications + discussion-log).
- Plan status flip + commit per phase + push + merge.
- Memory update.
- **Эстимат:** 0.2 dev-day.

**Total:** ~2.5-3 dev-day.

---

## 4. Acceptance criteria (A127.1-A127.15)

- **A127.1** ✅ `&v` на value-record local без escape → stays
  `NovaValue_X` на стеке + `NovaValue_X*` valid в local scope.
- **A127.2** ✅ `&v` returned from fn → auto-promote `v` to heap as `Nova_X*`.
- **A127.3** ✅ `&v` captured by closure → auto-promote.
- **A127.4** ✅ `&v` stored в heap field → auto-promote.
- **A127.5** ✅ `&v` saved в global → auto-promote.
- **A127.6** ✅ Mixed branch (escape in one arm) → conservative promote.
- **A127.7** ✅ Method receiver works в обоих modes (stack `&v`, heap `v`).
- **A127.8** ✅ Field access `.x` correct в обоих modes (auto-deref).
- **A127.9** ✅ `&v` after consume → E_VALUE_RECORD_ESCAPE_AFTER_CONSUME.
- **A127.10** ✅ Unnecessary promote → W_VALUE_RECORD_UNNECESSARY_PROMOTE lint.
- **A127.11** ✅ Composability priv: promoted value-record preserves field
  privacy через `Nova_X*` access.
- **A127.12** ✅ Composability readonly: `ro` binding propagates через
  promote.
- **A127.13** ✅ plan127 fixtures ≥18 PASS.
- **A127.14** ✅ Regression: plan124_8 27/27 unchanged + plan120 + plan118
  baselines unchanged.
- **A127.15** ✅ V2 known limitations документированы:
  `[M-127-precise-escape]`, `[M-127-path-sensitive-escape]`,
  `[M-127-array-element-promote]`.

---

## 5. D-blocks impact

- **D228 amend** — value-record escape & auto-promote semantics.
- Cross-ref **D216 §4** (Plan 118) — escape machinery reuse contract.

---

## 6. Cross-references

- **Plan 124.8** — value-record core (D228 NEW).
- **Plan 118 Ф.2** — escape_analyze walker (D216 §4).
- **Plan 118 [M-118-escape-precise]** — V1 OVER-promote → V2 precise mode
  (parallel followup, можем landed independently).
- **`[M-124.8-value-heap-promote]`** — closes by Plan 127.
- **`[M-124.8-value-record-array-inline]`** — coordination point (array
  promotion granularity).
- Industry: Go escape analysis, Java escape analysis (HotSpot),
  C# stackalloc + ref locals.

---

## 7. Open questions

- **Q127.1** — `&arr[i]` где `arr: []Vec3` — per-element promote или
  whole-array promote? **Recommend:** per-element. Coordinate с
  `[M-124.8-value-record-array-inline]`.
- **Q127.2** — `&v` где `v: Vec3` параметр fn (по-value) — escape escape
  caller? **Recommend:** No — параметр уже COPY, его адрес не escape'ит
  caller scope.
- **Q127.3** — Trait-bound generic `T value` + `&T` — escape analysis
  per-monomorphization? **Recommend:** Yes — monomorphic walker уже
  per-instantiation у Plan 118.
- **Q127.4** — Diagnostic hint показывать suggestion «return by value»
  при W_VALUE_RECORD_UNNECESSARY_PROMOTE? **Recommend:** Yes — DX value.

---

## 8. Followup markers (V2/V3)

- **`[M-127-precise-escape]`** — V2 precise mode (no OVER-promote).
  Gated на Plan 118 [M-118-escape-precise].
- **`[M-127-path-sensitive-escape]`** — V2 path-sensitive analysis для
  mixed branch scenarios.
- **`[M-127-array-element-promote]`** — V3 per-element array auto-promote.
  Coordinate с `[M-124.8-value-record-array-inline]`.

---

## 9. Out of scope

- **Lifetimes** (Rust-style) — explicitly NOT included. Nova = GC-based
  escape analysis, не lifetime tracking.
- **Borrow checker** — same reason.
- **Unsafe escape hatch** — `unsafe { &v }` bypass escape check. Defer to
  Plan 118 `unsafe { }` block semantics — no separate handling нужно.
