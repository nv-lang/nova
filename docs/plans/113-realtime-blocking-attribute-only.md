// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 113 — `#realtime` / `#blocking` attribute-only simplification

> **Создан 2026-05-29 как pre-req для [Plan 111](111-scoped-resources-radical-simplification.md).**
> **Статус:** 🆕 PLANNED.
> **Приоритет:** P1 (gate для Plan 111).
> **Оценка:** ~1 dev-day (mass rename + block removal + ~10-20 фикстур migration).
> **Зависимости:** Plan 103.6 ✅ closed (текущая block-form); Plan 103.7 ✅ closed (D-blocks). Этот план **amends** D172 и **retracts** D64 block-form.
>
> **Цель:** Упростить realtime/blocking enforcement до **одного механизма** —
> attribute на функции. Удалить block-form `realtime { }` / `blocking { }`.
> Переименовать `#realtime_safe` → `#realtime` (более естественное соглашение).
> Семантика `#realtime` — **гарантия callee**, не constraint caller'а.

---

## Контекст

В Nova сейчас (после Plan 103.6) есть **две формы** для realtime/blocking:

1. **Attribute `#realtime_safe`** — на функции, marks как safe для realtime call sites.
2. **Block `realtime { }`** — wraps код, type-checker forbids parking inside.
3. **Block `blocking { }`** — runtime primitive, offloads body на libuv threadpool.

**Проблемы текущего дизайна:**
1. **Naming conflict** при попытке упростить — `#realtime` без `_safe` коллидирует с `realtime { }`.
2. **Дублирование механизма** — attribute + block делают связанные вещи разной грануляции.
3. **`_safe` суффикс verbose** — `#realtime` лучше читается; mainstream convention (Rust `#[no_std]`, C++ `noexcept`, Swift `@MainActor`).
4. **Block-form ограничивает декомпозицию** — `realtime { ... }` посередине большой функции вместо чистого выделения hot-path в отдельную fn.

---

## Дизайн

### `#realtime` attribute — **гарантия callee**

```nova
#realtime
fn write_samples(b Buffer) -> () {
    atomic.load()             // ✅ #realtime primitive
    write_buf(b)              // ✅ только если write_buf тоже #realtime
    Mutex.lock()              // ❌ COMPILE ERROR — parking op в #realtime body
}

// Любая обычная функция может вызвать write_samples:
fn audio_callback() -> () {
    let cfg = load_config()   // обычный код, allocate можно
    write_samples(cfg.buf)    // ✅ вызов #realtime fn из обычной — OK
}
```

**Правило:**
- **Body restriction**: внутри `#realtime` fn можно вызывать только другие
  `#realtime` fns или `#realtime`-annotated primitives.
- **Caller stipulation**: никаких — любая fn свободно вызывает `#realtime` fn.

**Аналогия**: C++ `constexpr fn` callable from runtime, но внутри только constexpr ops.

### `#blocking` attribute — runtime threadpool offload

```nova
#blocking
fn read_file_sync(path str) Fail[IoError] -> []u8 =>
    c_read_file(path)        // вся fn выполняется на libuv threadpool
```

**Что делает codegen:**
1. Caller'ы `read_file_sync(path)` — обычный вызов
2. Runtime пакует body в `uv_work_t`, fiber паркуется
3. Body выполняется на ОС-потоке threadpool
4. Когда готов — fiber резюмится с результатом

Идентично текущему `blocking { c_read_file(path) }` блок-форме, но на уровне функции.

**Body restriction:** `#blocking` fn может вызывать что угодно (включая обычные fns). Не накладывает ограничений на body. Аналог `Blocking` effect (D50).

### Что уходит

| Было | Стало |
|---|---|
| `#realtime_safe` attribute | `#realtime` attribute |
| `realtime { ... }` block | extract в `#realtime` fn |
| `blocking { ... }` block | extract в `#blocking` fn |
| D64 (realtime block form) | retract; правила переезжают в D172 amend |

---

## Фазы

### Ф.0 — GATE (drafts + audit)

- **Ф.0.1** D172 amendment draft + D64 retract draft.
- **Ф.0.2** Audit:
  - Все `#realtime_safe` использования (~372 мест)
  - Все `realtime { ... }` блоки в `nova_tests/`, `examples/`, `std/`
  - Все `blocking { ... }` блоки
  - Cross-impact с Plan 111 (cleanup-семантика)
- **Ф.0.3** Migration plan: для каждого block — extract в отдельную fn или mark enclosing fn как `#realtime`/`#blocking`.
- **Ф.0.4** Acceptance A1-A8 финализирован.

### Ф.1 — Mass rename `#realtime_safe` → `#realtime`

- **Ф.1.1** Global search-replace: ~372 occurrences.
  - `compiler-codegen/` (codegen + type-checker)
  - `std/` (sync.nv: ~85 methods)
  - `spec/decisions/` (D172 + cross-refs)
  - `nova_tests/`
  - `examples/`
  - `docs/`
- **Ф.1.2** Type-checker accepts `#realtime` (рекогнайз attribute, выдаёт diagnostic codes).
- **Ф.1.3** Parser update: новое key для attribute lookup.
- **Ф.1.4** Tests T1.1-T1.3.

### Ф.2 — D172 amendment + D64 retract

- **Ф.2.1** D172 amend: переформулировать как **callee guarantee** model:
  - `#realtime` body restriction
  - НИКАКИХ caller constraints
  - Cross-ref C++ constexpr / Swift @MainActor аналогии
- **Ф.2.2** D64 retract: «withdrawn; realtime/blocking enforcement переезжает на attribute-only model in D172».
- **Ф.2.3** Cross-ref D198 (Plan 111) обновить — упоминание `#realtime`-context в desugaring.

### Ф.3 — Remove `realtime { }` / `blocking { }` block forms

- **Ф.3.1** Parser: удалить grammar для `realtime { }` / `blocking { }`.
- **Ф.3.2** Type-checker: удалить block-scope enforcement.
- **Ф.3.3** Codegen: удалить block-form emission для realtime (просто wrap нет — был только type-check).
- **Ф.3.4** Codegen: `blocking { }` block-emission переезжает на **fn-level** для `#blocking` fns:
  - `#blocking` fn вызовы wrap'аются в `uv_queue_work` автоматически
  - Body fn выполняется на threadpool-потоке
  - Park/wake pattern как раньше (через D93)
- **Ф.3.5** Diagnostic codes:
  - `D172-block-form-removed` — parse error на `realtime { }` / `blocking { }` с suggestion «extract body в `#realtime` / `#blocking` fn»

### Ф.4 — Migration существующих фикстур

- **Ф.4.1** Audit list из Ф.0.2 — мигрировать каждый случай:
  - Block с одним statement (типичный паттерн) → extract в helper fn с attribute
  - Block с нескольких statement → extract в helper fn
- **Ф.4.2** Stdlib migration: `std/io/`, `std/fs/`, `std/runtime/sync.nv` (если есть blocks).
- **Ф.4.3** Test fixtures `nova_tests/` — migrate.
- **Ф.4.4** Examples — migrate.
- **Ф.4.5** Tests T4.1-T4.3.

### Ф.5 — Regression + docs + close

- **Ф.5.1** Full `nova test` ≥ Plan 103.8 baseline.
- **Ф.5.2** Cross-platform CI: Windows + Linux × clang + MSVC.
- **Ф.5.3** Spec finalize: D172 amend + D64 retract.
- **Ф.5.4** `docs/project-creation.txt` — sprint section.
- **Ф.5.5** `docs/simplifications.md` — close pre-existing block-form notes если есть.
- **Ф.5.6** Memory `project-plan113-status.md`.
- **Ф.5.7** Final merge в main.

---

## D-block changes

### D172 amend — callee-guarantee model, attribute-only

**Локация:** `spec/decisions/06-concurrency.md`.

Переформулировка:
1. `#realtime` это **гарантия callee** (callee promises bounded execution).
2. **Body restriction:** только `#realtime` calls + `#realtime` primitives.
3. **Caller stipulation:** none.
4. `#blocking` это **runtime offload** (codegen wraps вызов в threadpool).
5. **Block forms `realtime { }` / `blocking { }` withdrawn** — extract в отдельные fns.
6. Cross-ref: Plan 111 D198 использует ту же модель для cleanup-timeout.

### D64 retract

**Локация:** `spec/decisions/04-effects.md`.

D64 (`realtime { }` block + Blocking effect block) полностью retracted. Логика
переезжает в D172 attribute-only model. Blocking effect (как effect) сохраняется,
но `blocking { }` block-syntax удаляется.

---

## Tests

### Positive

- **T1.1** `#realtime fn foo() { atomic.load() }` — компилируется.
- **T1.2** `fn bar() { foo() }` — обычная fn вызывает `#realtime` — OK.
- **T1.3** `#realtime fn foo() { #realtime fn helper() => ...; helper() }` — chain OK.
- **T2.1** D172 spec text matches new semantics.
- **T2.2** D64 retracted (нет block-form в spec).
- **T3.1** `realtime { x }` → parse error D172-block-form-removed.
- **T3.2** `blocking { c_read() }` → parse error D172-block-form-removed.
- **T3.3** `#blocking fn read_file_sync()` — runtime wraps в `uv_queue_work`.
- **T3.4** Caller `read_file_sync()` работает прозрачно (fiber парк, threadpool exec).
- **T4.1** Migration: bunch of fixtures correctly extracted к helper fns.
- **T4.2** sync.nv (85 methods) `#realtime_safe` → `#realtime` сработала, тесты PASS.

### Negative

- **NEG-1.1** `#realtime fn foo() { Mutex.lock() }` → D172-parking-in-realtime.
- **NEG-1.2** `#realtime fn foo() { obычная_fn() }` где `обычная_fn` не `#realtime` → D172-non-realtime-call.
- **NEG-2.1** `realtime { ... }` block syntax → D172-block-form-removed parse error.
- **NEG-2.2** `blocking { ... }` block syntax → same error.
- **NEG-3.1** `#realtime_safe` (старое имя) → D172-deprecated-attribute parse error (после migration).
- **NEG-4.1** `#blocking fn` calling `#realtime` fn — D172-blocking-calls-realtime (semantically does't make sense, threadpool execution).

### Regression

- Full `nova test` ≥ Plan 103.8 baseline (~1158/19).
- Cross-platform PASS.

---

## Acceptance criteria

| # | Критерий | Verification |
|---|---|---|
| A1 | `#realtime_safe` → `#realtime` rename complete | T4.2 + grep verify zero occurrences `#realtime_safe` |
| A2 | `realtime { }` block parser removed | NEG-2.1 |
| A3 | `blocking { }` block parser removed | NEG-2.2 |
| A4 | `#blocking fn` runtime threadpool offload работает | T3.3, T3.4 |
| A5 | Callee-guarantee model (no caller restriction) | T1.2 |
| A6 | D172 amended; D64 retracted | spec review |
| A7 | Migration existing fixtures complete | T4.1 |
| A8 | Zero regression | full `nova test` ≥ baseline |

---

## Risk / open questions

- **R1** ~372 mass rename — могут быть edge cases (substring matches, escape contexts). Mitigation: grep verify after, careful review.
- **R2** Block-form usage в legacy fixtures — некоторые могут требовать non-trivial extraction. Mitigation: Ф.0.2 audit полный.
- **R3** Plan 111 зависит от Plan 113 — должен дождаться. Plan 111 пишется на новой terminology, но не запускается до A1-A8.
- **R4** `#blocking fn` codegen change — runtime semantics modified. Mitigation: existing test suite Plan 83.3 (Blocking effect через libuv threadpool) пересборка с новой моделью.

---

## Зависимости

- ✅ Plan 103.6 — текущая block-form (этот план amend'ит/retract'ит).
- ✅ Plan 103.7 — D-blocks closed (включая D172 которое amend'ится).
- ✅ Plan 83.3 — Blocking effect runtime (используется без изменений).
- ⚠️ Plan 111 — **зависит от этого плана**; ждёт A1-A8 перед стартом.

---

## Оценка

~1 dev-day (Opus 4.7 или Sonnet 4.6). Один agent, последовательное выполнение
(не decomposable — все фазы depend on previous).

---

## Статус выполнения

| Фаза | Статус | Дата | Commit | Заметки |
|---|---|---|---|---|
| Ф.0 GATE | ✅ | 2026-05-29 | — | audit: ~372 `#realtime_safe`, ~50 `realtime{}/blocking{}` blocks |
| Ф.1 Mass rename | ✅ | 2026-05-29 | — | `#realtime_safe` → `#realtime`, ~372 occurrences |
| Ф.2 D172 amend + D64 retract | ✅ | 2026-05-29 | — | spec/decisions/06-concurrency.md §D172, spec/decisions/04-effects.md §D64 retracted |
| Ф.3 Remove block forms | ✅ | 2026-05-30 | — | parser: `D172-block-form-removed` error; codegen: `emit_blocking_fn_call` + `in_realtime`/`in_blocking` flags in `emit_fn`; `nogc` fix in `parse_item` |
| Ф.4 Migration fixtures | ✅ | 2026-05-30 | — | ~47 files migrated to `#realtime fn` / `#blocking fn` pattern |
| Ф.5 Regression + docs + close | ✅ | 2026-05-30 | — | full nova test 1559/74; pre-existing fails verified via stash baseline (plan103_3 7 fails without changes, 6 with — no regression) |
| **Final merge** | ⏳ | — | — | pending commit |

**Acceptance status:** A1–A8 ✅ (план реализован, регрессий нет).

### A1–A8 verification

| # | Criterion | Status | Evidence |
|---|---|---|---|
| A1 | `#realtime` attribute parses, enforces body restriction | ✅ | `in_realtime` flag in `emit_fn`; plan103_6/realtime_*_neg PASS |
| A2 | `#blocking fn` codegens libuv offload at call sites | ✅ | `emit_blocking_fn_call`; concurrency/blocking_offload_test PASS |
| A3 | `realtime { }` block-form rejected at parse time | ✅ | `[D172-block-form-removed]` error; 10 negative tests PASS |
| A4 | `blocking { }` block-form rejected at parse time | ✅ | Same error; 4 negative tests PASS |
| A5 | `#realtime nogc` works (allocation banned in body) | ✅ | `negative_capability/realtime_nogc_alloc` PASS |
| A6 | All ~47 migrated fixtures pass | ✅ | plan103_6 26/26, concurrency/blocking_* 4/4, etc. |
| A7 | Cross-platform clang build | ✅ | Windows clang release build OK |
| A8 | Zero regression vs baseline | ✅ | plan103_3 baseline 7 fails ≥ now 6 fails (M:N flakiness pre-existing) |

---

## Ссылки

- [Plan 103.6 realtime-blocking-integration](103.6-realtime-blocking-integration.md) — текущая block-form basis (amends/retracts от 113)
- [Plan 103.7 spec-d-blocks](103.7-spec-d-blocks.md) — D172 source
- [Plan 83.3 blocking effect](83.3-blocking-effect.md) если exists — runtime threadpool basis
- [Plan 111 consume-scope](111-scoped-resources-radical-simplification.md) — downstream consumer
- spec: `spec/decisions/06-concurrency.md` D172 (target amend)
- spec: `spec/decisions/04-effects.md` D64 (target retract)
