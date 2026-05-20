# Plan 75: `never` — bottom-тип как строчный встроенный keyword

> **Создан 2026-05-20.** Выделен из обсуждения Plan 72 P1-B.
>
> **Цель:** привести bottom-тип к единой конвенции примитивов Nova —
> переименовать `Never` → `never` (строчный), сделать встроенным
> keyword'ом, убрать любую идею «объявить его в prelude».

---

## Контекст: что не так

Все примитивные типы Nova — **строчные** и встроены в язык, в prelude
**не объявляются**: `int, i8…i64, u8…u64, uint, f32, f64, byte, bool,
char, str` — список `is_primitive_type` в `compiler-codegen/src/parser/mod.rs`.

Bottom-тип выпадает из конвенции:

- называется `Never` (с **заглавной**) — выглядит как номинальный/user-тип;
- **НЕ** входит в `is_primitive_type` — парсится как капитализированный тип;
- Plan 72 P1-B doc предлагал «type Never формально в prelude» — неверно
  вдвойне: встроенный bottom-тип не должен быть ни капитализированным,
  ни в prelude (`int` ведь не в prelude).

Это inconsistency: `int`/`bool`/`u32` — строчные built-in, а `Never` — нет.

**Объём** (`\bNever\b`): ~38 в `.rs` (`types/mod.rs` 24 — type-checker,
`emit_c.rs` 6, `parser/mod.rs` 3, `ast/mod.rs` 1, `interp/stdlib.rs` 1,
`nova-cli/main.rs` 3) + ~41 в `.nv` (`std/prelude/*`, `std/time/duration.nv`,
`nova_tests/*`).

---

## Не путать с `RuntimeNoneError`

`never` (bottom-тип) и `RuntimeNoneError` (std error-тип) — разное:

- **`never`** — bottom-тип, языковой примитив → строчный keyword, НЕ в prelude.
- **`RuntimeNoneError`** — std error-тип → капитализирован, в
  `std/prelude/errors.nv` (как `Error`, `RuntimeError`, `ReadBufferError`).
  Закрывается отдельно (Plan 62.C.bis; empty-sum syntax P1-B уже готов и
  проверен — `type RuntimeNoneError` парсится+компилируется).

**Plan 75 — только про `never`. `RuntimeNoneError` вне scope.**

---

## Задачи по фазам

### Ф.1 — Parser: `never` как примитив-keyword

- Добавить `"never"` в `is_primitive_type` (`parser/mod.rs`).
- `never` не имеет вариантов/полей — это keyword-тип, не `type`-decl.
- Убедиться, что `type Never` / `type never` как user-декларация больше
  не требуется и не ожидается парсером.

### Ф.2 — Rename во всех Rust-сайтах (~38)

- `types/mod.rs` (24) — type-checker: bottom-тип, subtyping (`never` <: T
  для любого T), результат `throw`, default exit-row для `Handler`.
- `emit_c.rs` (6) — codegen: C-тип bottom (ABI placeholder — оставить как
  сейчас у `Never`).
- `parser/mod.rs` (3), `ast/mod.rs` (1), `interp/stdlib.rs` (1),
  `nova-cli/main.rs` (3).
- `throw expr` имеет тип `never` (spec D25/D65); `Handler[E]` ≡
  `Handler[E, never]` (spec D88 default).

### Ф.3 — Prelude / std миграция (~41 `.nv`)

- Проверить: есть ли где-то `type Never` **декларация** в prelude/std —
  если да, удалить (built-in не объявляется в prelude).
- Rename всех **использований** `Never` → `never`: `std/prelude/*`
  (core, runtime, errors, effects, `prelude.nv`), `std/time/duration.nv`,
  `nova_tests/*` (panic_exit, throw_in_expression, fail_handler, …).
- Рекомендуется one-shot migration tool (как `migrate_plan60` /
  `migrate_plan65`) — token-aware rename через lexer, skip строк/комментариев.

### Ф.4 — Spec

- D25/D65 (`throw` → bottom-тип): `Never` → `never`.
- D88 (`Handler[E]` default exit row): `Never` → `never`.
- Новый/обновлённый D-блок: `never` — bottom-тип, строчный примитив,
  uninhabited (0 значений), subtype любого `T`, не объявляется в prelude.

### Ф.5 — Тесты (позитивные + негативные)

- **Позитивные:** `throw` в expression-position, `Handler[E, never]`,
  `fn f() -> never` — компилируются (обновить имена в существующих
  фикстурах: `throw_in_expression.nv`, `panic_exit.nv`, `fail_handler.nv`).
- **Негативный:** нельзя сконструировать значение `never` — `let x never =
  …` невозможно (нет конструктора), ожидается CE.

---

## Порядок выполнения

```
Ф.1 (parser keyword)        — первым,            ~1h
Ф.2 (Rust rename)           — после Ф.1,         ~2-3h (types/mod.rs основной)
Ф.3 (.nv миграция)          — параллельно Ф.2,   ~2h (migration tool)
Ф.4 (spec D-блоки)          — после Ф.2,         ~1h
Ф.5 (тесты pos + neg)       — последним,         ~1h
```

Общий объём ≈ 1 dev-день.

---

## Риски

- **Breaking change:** после Ф.1 `Never` (заглавная) → parse error. Все
  сайты (Ф.2 + Ф.3) должны мигрироваться в одном атомарном PR. Приемлемо
  для bootstrap 0.1 — прецедент: `nova-codegen test` → `test-interp`
  hard-rename без deprecation-периода.
- **`Never` как user-имя:** если в чужом коде есть user-тип `Never` —
  конфликт. Маловероятно (де-факто зарезервированное имя).
- **Edition-gate не требуется** — 0.1 ещё не стабилизирована.

---

## Ссылки на источники

- Plan 72 §P1-B — обсуждение empty-sum syntax, откуда выделен этот план.
- Spec D25/D65 (`throw` как expression, bottom-тип).
- Spec D88 (`Handler[E]` default exit row `Never`).
