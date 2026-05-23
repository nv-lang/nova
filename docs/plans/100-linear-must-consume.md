// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 100 (umbrella): consume-типы — production-grade «must-be-consumed»

> **Создан 2026-05-23. Ред. 2 (2026-05-23) — production-grade rewrite
> с декомпозицией.** Исходный «bootstrap» Plan 100 (D1–D10 + D5.1) — это
> необходимый минимум, но не production: он имел 5 honest defer'ов
> (generic silent-leak, deep peek в Option, closure capture, defer/
> errdefer tracking, async/cancel). Каждый из них — реальная дыра, по
> которой Nova уступает Rust / Kotlin / Rust-async-Drop. Эта редакция
> закрывает их **полностью** через декомпозицию на 4 sub-plan'а.
>
> **Статус umbrella:** 📋 roadmap, не начат. 4 sub-plan'а (100.1
> foundation, 100.2–100.4 параллелизуемы после 100.1); 100.4 — sub-
> umbrella с 5 sub-sub-plan'ами. **~27 dev-day total** (5 + 4 + 4 + 14
> на 100.1/2/3/4 соответственно; vs ~4–6 у bootstrap — цена production-
> grade).
>
> **Зависимости:** [Plan 73](73-consume-qualifier.md) ✅ (D131 affine
> foundation), [Plan 77](77-fluent-return.md) ✅ (D132 `-> @` sound
> alias), [Plan 95](95-builtin-sum-method-mono.md) ✅ (Option/Result
> generic-method-able), [Plan 20](20-defer-implementation.md) ✅ (D90
> defer/errdefer), [Plan 49](49-cancel-throw-routing.md) ✅ (D65/D85
> cancel-throw routing — для 100.4).

---

## Vision — production-grade resource management для Nova

Цель — закрыть для Nova весь класс bug'ов «забыл закрыть ресурс»
(transaction без commit/rollback; file без close; lock без release;
connection без disconnect; pending-error без handler) — на уровне
compile-time, без runtime cost'а, без borrow-checker'а.

Метрика — **не хуже Rust / Kotlin / TS (ES2024 `using`) / Go**
(который имеет почти ничего) по каждому из ~12 capabilities resource-
management'а. На большинстве — **строже Rust** (нет `mem::forget`
escape hatch; явные commit/rollback вместо single-method Drop;
effect-typed cleanup через Plan 49).

## Сравнение с Go / Rust / TS / Kotlin

| Capability | Go | Rust | TS (ES2024) | Kotlin | Nova (Plan 100) |
|---|---|---|---|---|---|
| Compile-time enforcement «must close» | ❌ нет | ⚠️ warning (`#[must_use]`, suppressable) | ❌ нет (runtime via `Symbol.dispose`) | ❌ нет (runtime via `use{}`) | ✅ **error** (sub-plan **100.1**) |
| Suppress escape hatch | n/a | ✅ `mem::forget(v)` / `let _ = v` | n/a | n/a | ❌ **by design** (anti-pattern, 100.1) |
| Multiple distinct close methods (commit/rollback) | ⚠️ конвенция | ⚠️ enum-в-Drop, awkward | ⚠️ один `dispose`, выбор runtime | ⚠️ `use{}` block, runtime-ветвление | ✅ **native** (consume-методы, compile-time-выбор, 100.1) |
| Effect-typed cleanup (commit может fail) | ⚠️ возврат error | ✅ через `Drop` impl Result | ⚠️ через try-catch внутри dispose | ⚠️ throws | ✅ **Fail[E] effect** (Plan 49, 100.4) |
| Generic linear bound | n/a | ✅ `Move` marker | n/a | n/a | ✅ **`[T consume]`** (100.2) |
| Collection of linear elements | n/a | ✅ `Vec<T>` ownership tracking | n/a | n/a | ✅ **`[]T` consume-aware iteration** (100.2) |
| Read-only borrow без consume | n/a | ✅ `&T` (lifetime cost) | n/a | n/a | ✅ **`view T`** (100.3, **без lifetime!**) |
| Closure with linear capture (`FnOnce`) | n/a | ✅ `FnOnce`/`FnMut`/`Fn` система | ❌ нет статической проверки | ❌ нет | ✅ **consume-closure analysis** (100.3) |
| Auto-cleanup на panic / unwind | ❌ | ✅ `Drop` гарантирован | ⚠️ try-finally | ✅ `use{}` блок | ✅ **`errdefer`** + check_consume (100.4) |
| Auto-cleanup на async-cancel | ❌ | ✅ `Drop` в async (с caveat'ами) | ⚠️ AbortController + cleanup-manually | ✅ structured concurrency | ✅ **Plan 49 cancel-aware consume** (100.4) |
| Cleanup на early-return / `?` / `!!` | ⚠️ `defer` manual | ✅ автоматический | ⚠️ ручной | ✅ `use{}` block-scoped | ✅ **errdefer + check_consume** (100.4) |
| Field-aware tracking в record'ах | n/a | ✅ через `&mut` | ❌ | ❌ | ✅ **D5 field-aware flow** (100.1) |
| Nested field paths (`@.state.tx.commit()`) | n/a | ✅ | ❌ | ❌ | ✅ **многоуровневый tracker** (100.1) |
| Pattern destructure consume-типа | n/a | ✅ через move | n/a | n/a | ✅ **через consume-метод record'а** (100.1) |
| Deep peek в `Option[ConsumeType]` без consume | n/a | ✅ `match &opt { Some(f) => f }` | n/a | n/a | ✅ **`match view @file`** (100.3) |
| Lifetime / borrow-checker cognitive cost | ✅ нет | ❌ есть | ✅ нет | ✅ нет | ✅ **нет (GC + scope-only view)** |

**Свод:** Nova matches Rust по всем 16 capabilities; **превосходит** на
4 (non-suppressable, distinct-methods, effect-typed cleanup, no
lifetime) — и матчит Go/Kotlin/TS на capabilities где они вообще
доступны. Точка дизайна: «гарантии Rust ownership без borrow-checker'а
поверх GC».

## Что было в bootstrap (Ред. 1) и почему этого недостаточно

Bootstrap Plan 100 (D1–D10 + D5.1) — это **100.1 core** из текущей
декомпозиции. Имел 5 honest defer'ов:

| # | Hole | Уступка кому | Закрывает |
|---|---|---|---|
| 1 | Generic silent-leak (`fn first[T](pair (T,T)) -> T => pair.0` теряет pair.1) | Rust `Move` trait propagation | **100.2** |
| 2 | Deep peek в `Option[ConsumeType]` невозможен | Rust `&match` | **100.3** |
| 3 | Closure capture consume-var — permissive false-negative | Rust `FnOnce` analysis | **100.3** |
| 4 | `defer`/`errdefer` conditional consume не tracked | Rust `Drop` гарантия + Kotlin `use{}` | **100.4** |
| 5 | Async-cancel propagation через consume-var | Rust async `Drop` + Plan 49 | **100.4** |

Плюс ещё **2 не-defer'а bootstrap'а**, которые в production должны
работать чище:

| # | Изъян | Закрывает |
|---|---|---|
| 6 | Collection `[]T` consume-elements не iterated | **100.2** |
| 7 | Nested field paths (`@.state.tx.commit()`) — только direct в bootstrap | **100.1** (расширяется) |

## Декомпозиция

```
Plan 100 (umbrella, this doc)
├── 100.1 Core static analysis (foundation; ~5 dev-day)
│      ↓
│   ├──────────────────────────────────┬─────────────────────────────┐
│   ↓                                  ↓                             ↓
├── 100.2 Generic + Collections   ├── 100.3 Borrow/view          ├── 100.4 Cleanup-on-failure
│   propagation (~4 dev-day)       │   (~4 dev-day)               │   (defer/errdefer/async/cancel)
│                                  │                              │   (~4 dev-day)
└────────────────────────────────────────────────────────────────────┘
                                   ↓
                              Production-ready
```

Граф зависимостей:
- **100.1** — foundation, blocks все остальные.
- **100.2 / 100.3 / 100.4** — параллелизуемы после 100.1, не зависят
  друг от друга (но best-practice — реализовывать в порядке 100.2 →
  100.3 → 100.4 для итеративной полноты).

### Sub-plan'ы

| # | Файл | Скоп | Owner deps |
|---|---|---|---|
| **100.1** | [100.1-core-must-consume.md](100.1-core-must-consume.md) | type-level `consume`, field-aware flow (с nested-paths), binding-form, D1–D10 + D5.1, 17 фикстур, pilot mock Transaction | Plan 73/77/95 |
| **100.2** | [100.2-generic-propagation.md](100.2-generic-propagation.md) | `[T consume]` generic bound, `[]T consume`-aware iteration (`for tx in vec` consume каждое), HashMap/Option/Result propagation, stdlib migration, 15 фикстур | 100.1 |
| **100.3** | [100.3-borrow-and-view.md](100.3-borrow-and-view.md) | `view T` (read-only borrow без lifetime; scope-only), `match view @file` deep peek, closure capture analysis (`consume` / `view` qualifiers), 12 фикстур | 100.1 |
| **100.4** | [100.4-cleanup-on-failure.md](100.4-cleanup-on-failure.md) (**umbrella**) | Production-grade defer/errdefer rework — amend D90 системно через **5 sub-sub-plan'ов**: 100.4.1 failable body (D136 + Plan 49 composition), 100.4.2 async/suspend body (D137 cancel-safe), 100.4.3 okdefer + reason-aware (D138), 100.4.4 multi-defer LIFO error accumulation + panic composition (D139), 100.4.5 consume-integration final (D140). ~14 dev-day | 100.1, Plan 20/49 |

## Acceptance (umbrella, across все 4 sub-plan'а)

После закрытия всех 4 sub-plan'ов:

- [ ] **Compile-time guarantee:** ни одна consume-переменная не может
      утечь незакрытой ни на одном code-path'е (включая return / panic /
      `?` / `!!` / loop break / async-cancel / panic-в-defer-body).
- [ ] **No suppress escape:** нет syntax / library / FFI способа
      обойти consume-обязательство кроме consume-метода. (`let _ = v`
      для consume-типа → error; `mem::forget`-аналог отсутствует.)
- [ ] **Generic correctness:** `fn first[T consume](pair (T,T)) -> T`
      ловит silent-leak `pair.1`; `[T consume]` bound work'аcт на всех
      generic-functions и user-generic types.
- [ ] **Collection correctness:** `for tx in vec { tx.commit() }` где
      `vec: []Transaction` consume'ит каждый element; `vec.push(consume
      tx)` integration с D131; `vec.map`, `vec.filter` через `[T
      consume]` bound работают.
- [ ] **Read-only borrow:** `match view @file { Some(f) => println(f.id),
      None => () }` peek'ает без consume; `view` propagates scope-only
      (без lifetime); compile-error если `view`-binding outlives source.
- [ ] **Closure capture:** `let f = || tx.commit()` — compiler знает,
      что f должен быть вызван (FnOnce-семантика); `let f = || println(tx.id)`
      — view-capture, tx остаётся Live; cargocult «забыл вызвать
      closure» → error.
- [ ] **Cleanup-on-failure:** `consume tx = begin(); errdefer { tx.rollback() };
      tx.commit()` — на success path commit, на error path errdefer
      rollback. check_consume видит обе ветки.
- [ ] **Async/cancel:** `supervised { spawn { consume tx = begin(); ...
      } }` — на cancel автоматически rollback через errdefer-в-supervised-
      scope; tx гарантированно не утекает.
- [ ] **Cross-language parity matrix** (см. выше) — каждая строка
      «✅ Nova» подтверждена ≥1 фикстурой.
- [ ] **Pilot stdlib migration** — `Transaction`, `File`, `Connection`,
      `Lock-guard` мигрированы на consume-семантику (планируется в
      рамках Plan 18 stdlib MVP; Plan 100 umbrella готовит инфру).
- [ ] **Полный `nova test`** → 0 регрессий.
- [ ] **Spec sweep**: D131 (Plan 73), D132 (Plan 77), D90 (Plan 20),
      D85 (Plan 49), D133 (Plan 100.1), D134 (Plan 100.2), D135 (Plan
      100.3), D136 (Plan 100.4) — все cross-ref'ы согласованы.

## Risks (cross-cutting)

1. **Borrow-checker creep.** `view T` (100.3) — это шаг в сторону Rust
   `&T`. Соблазн добавить lifetime'ы. **Митигация:** жёсткое
   ограничение — `view` живёт только в рамках вызова функции / arm'а
   pattern-match'а / closure-body, никогда не сохраняется в record /
   возвращается / эскейпит scope. Это «borrow-via-scope», не «borrow-
   via-lifetime».

2. **Migration cost.** Stdlib generic-функции (Plan 17/26/30/52
   collection API) могут потребовать `[T consume]` annotation для
   правильной обработки consume-elements. **Митигация:** 100.2 Ф.4 —
   audit + migration; default — silent-ignore сохраняется для
   non-annotated funcs (backward-compat); annotated дают strict mode.

3. **Async / supervised interaction (Plan 49 lineage)** — самая тонкая
   часть. **Митигация:** 100.4 переиспользует Plan 49 cancel-routing
   инфру; не вводит новые механизмы.

4. **Spec D-block коллизии.** Резервируем D133 (100.1) / D134 (100.2)
   / D135 (100.3) / D136 (100.4). **Митигация:** проверка перед
   каждым sub-plan'ом start'ом.

5. **Performance overhead.** Field-aware tracking + nested paths +
   collection-aware iteration — расширение `check_consume` pass'а.
   **Митигация:** все проверки compile-time, runtime overhead = 0
   (defense-in-depth zero-out полей — копеечный).

## Связь

- [Plan 73 / D131](73-consume-qualifier.md) ✅ — affine `consume`
  foundation. Plan 100.1 расширяет на type-decl.
- [Plan 77 / D132](77-fluent-return.md) ✅ — `-> @` fluent-return.
  Outline-источник Plan 100 (§«Потенциал лучше Rust»).
- [Plan 95](95-builtin-sum-method-mono.md) ✅ — Option/Result
  generic-method-able; основа для Plan 100.3 borrow-via-pattern.
- [Plan 20 / D90](20-defer-implementation.md) ✅ — `defer`/`errdefer`;
  основа Plan 100.4.
- [Plan 49 / D65 D85](49-cancel-throw-routing.md) ✅ — cancel-routing,
  kinded throws; интегрируется в Plan 100.4 async/cancel.
- [Plan 47 / D75](47-supervised-cancel.md) ✅ — supervised-scope; цикл
  cancel-propagation в Plan 100.4.
- [Plan 18 stdlib](18-stdlib-roadmap.md) — реальные кандидаты на
  consume-миграцию (`File`, `Connection`, `Lock`, `Transaction`).
- [Plan 91](91-stdlib-mvp-for-0.1.md) — stdlib MVP, после которого
  можно делать массовую миграцию.
