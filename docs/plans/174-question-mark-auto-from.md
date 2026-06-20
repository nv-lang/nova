# Plan 174 — `?` авто-конверсия ошибки через `From` (Rust-паритет)

> **Top-level план.** Создан 2026-06-20. **Статус:** 📋 PLANNED (компиляторная фича +
> amend D85; реализация — зона 172/владельца). **Маркер:** `[M-174-question-from]`.
> **Решение (2026-06-20):** разворот D85-стойки «только явный `.map_err()`» — `?`
> авто-конвертит тип ошибки через `From`, как в Rust (`Err(e) => return Err(From::from(e))`).

## Зачем

Rust-эргономика: `?` авто-приводит тип ошибки источника к типу ошибки caller'а через
`From`, убирая ручные `.map_err(...)` в каждой точке проброса. Nova ранее
([D85](../../spec/decisions/04-effects.md#d85)) сознательно требовала явный `.map_err()`
(«не магия»). Решение 2026-06-20 — принять Rust-эргономику для `?`.

## Семантика: сейчас → станет

Обозначения: `E` — тип ошибки источника (`expr: Result[T,E]`), `E'` — тип ошибки caller'а.

### Сейчас (D85 + D165)

| Режим | Условие | Err-путь | При `E ≠ E'` |
|---|---|---|---|
| return ([D85](../../spec/decisions/04-effects.md#d85)) | fn → `Result[U,E']` | `return Err(e)` | **compile error** → нужен `.map_err` |
| throw (D165/Plan 100.7, Fail-context) | fn на `Fail[E']` | `throw e` | **compile error** → нужен `.map_err` |

### Станет (Rust-паритет)

При `E ≠ E'`, если на `E'` есть **не-identity** `From[E]`-impl (`fn E'.from(e E) -> E'`):

| Режим | Err-путь |
|---|---|
| return | `return Err(E'.from(e))` |
| throw | `throw E'.from(e)` |

- `E ≡ E'` → без конверсии (fast-path, как сейчас).
- `E ≠ E'` И НЕТ `From[E]` на `E'` → compile error `[E_TRY_NO_FROM]` с подсказкой
  «impl `From[E]` for `E'` или `expr.map_err(|e| …)?`».
- `Option[T]?` → **без изменений** (`return None`; None не несёт значения ошибки —
  как в Rust, From там не применяется).

## Изменения по слоям

1. **Checker** (`compiler-codegen/src/types/mod.rs`): в точке `?`, где сейчас при
   `E ≠ E'` выдаётся ошибка несовместимости, — проверить наличие не-identity
   `From[E]`-impl на `E'`; есть → разрешить + записать «нужна конверсия» для codegen;
   нет → `[E_TRY_NO_FROM]`. Sum-supertype-совместимость (D85) сохранить как fast-path
   ДО From-резолва.
2. **Codegen** (`compiler-codegen/src/codegen/emit_c.rs:21274-21334`, обе ветки
   return/throw): на Err-пути, когда нужна конверсия, эмитить `E'.from(e)` через
   существующий From-mono-механизм (`T.from(v)`), а не сырой `e`. `E'` — из
   `current_fn_return_ty` (return) или из Fail-эффект-типа (throw).
3. **Спека:** **amend [D85](../../spec/decisions/04-effects.md#d85)** — desugar Err-пути =
   `return/throw E'.from(e)` при несовпадении; снять «нужно явное `.map_err()`» (оставить
   `.map_err` для НЕ-From преобразований / смены значения). **Заодно починить stale
   `## D4` (04-effects.md:290) + дубль `####` (:950)** — они до-D85, говорят «`?` только
   в `Fail[E]` → throw», прямо противоречат D85.
4. **Тесты** `nova_tests/q_auto_from/`: (a) return E≠E' с From → PASS; (b) throw E≠E' с
   From → PASS; (c) E≡E' без конверсии; (d) neg E≠E' без From → `[E_TRY_NO_FROM]`;
   (e) identity-blanket НЕ оборачивает; (f) Option? без изменений.

## Открытые вопросы (на решение владельца)

- **`!!` тоже авто-From?** `?` в throw-режиме и `!!` оба throw'ят → для симметрии `!!`
  логично тоже авто-конвертить. Запрошено было только `?`. Реши scope: `?` или `?`+`!!`.
- **Identity-blanket — ключевая ловушка.** `From[T] for T` ([protocols.nv:109](../../std/prelude/protocols.nv#L109))
  существует для ВСЕХ `T` → формально «From есть всегда». Правило обязано быть: конверсия
  только если `E ≠ E'` И есть **не-identity** `From[E]` на `E'`; иначе любой mismatch
  ложно «разрешится» через несуществующий путь.
- **Транзитивность From.** Rust НЕ делает транзитивный From — держать один шаг.
- **D77 4-way auto-derive.** `From`-форма может быть синтезирована из `TryFrom`/Fail-формы
  ([protocols.nv:126-138](../../std/prelude/protocols.nv#L126)) — `?` должен видеть и
  синтезированные From-impl.

## Гейт / зависимости

- Реализация компиляторная (checker + codegen) → зона **172**/владельца.
- Пересекается с 172.1 U.4 (typed IR): после U.4 точка `?` знает оба типа ошибок чище —
  можно делать поверх U.4 либо независимо (точечно в текущем `?`-codegen).

## Followup-маркер

`[M-174-question-from]`.
