# Plan 174 — `?` в точности как в Rust: return-only + авто-`From`

> **Top-level план.** Создан 2026-06-20. **Статус:** 📋 PLANNED (компиляторная фича +
> amend D85; реализация — зона 172/владельца). **Маркер:** `[M-174-question-from]`.
> **Решение (2026-06-20):** `?` приводится в точности к Rust — (A) **return-only**
> (только на `Result`/`Option`, проброс значением), (B) **авто-`From`** конверсия ошибки.

## Зачем

Rust-эргономика `?`: пробрасывает ошибку **значением** (`return Err(From::from(e))`),
авто-конвертя тип через `From` — убирает ручные `.map_err(...)`. Nova ранее
([D85](../../spec/decisions/04-effects.md#d85)) требовала явный `.map_err()` («не магия»),
а D165 добавил мутный throw-режим `?` в Fail-функциях. Решение 2026-06-20 — чистый Rust.

## Часть A — `?` строго return-only (убрать throw-режим D165)

`?` работает **только** в функциях, возвращающих `Result[T,E]` / `Option[T]`, и пробрасывает
**значением** (`return Err/None`). В Fail-эффект-функциях `?` **запрещён** — там ошибка
пробрасывается прямым `throw` или `!!`.

- **Обоснование:** `never` — это тип операции эффекта `fail(value E) -> never`
  ([04-effects.md:922](../../spec/decisions/04-effects.md#L922)), а не функции (у неё свой
  `-> T`); смешивать value-propagation (`?`) с effect-throw (`fail`/`!!`) незачем. Возвращает
  каноническую [D85](../../spec/decisions/04-effects.md#d85) («`?` не задействует Fail»).
- **Fallout минимальный.** В компилируемом корпусе `?` уже используется только на
  Result/Option-функциях (`nova_tests/effects/throws.nv`) — throw-режим D165 в корпусе НЕ
  задействован, «нечего откатывать». Мигрировать лишь: aspirational examples
  (`http.nv`, `oxsar_port.nv`) + doc-comment-примеры on_exit в spec/stdlib
  (`@commit()?` → `@commit()!!`).

## Часть B — `?` авто-конвертит ошибку через `From` (как Rust)

`E` — тип ошибки источника (`expr: Result[T,E]`), `E'` — тип ошибки caller'а.

| Случай | Сейчас | Станет (Rust) |
|---|---|---|
| `E ≡ E'` | `return Err(e)` | `return Err(e)` (без конверсии, fast-path) |
| `E ≠ E'`, есть не-identity `From[E]` на `E'` | **compile error** → `.map_err` | `return Err(E'.from(e))` (авто) |
| `E ≠ E'`, нет `From` | compile error | compile error `[E_TRY_NO_FROM]` + подсказка |

`Option[T]?` → без изменений (`return None`; None не несёт значения ошибки, как в Rust).

## Изменения по слоям

1. **Checker** (`compiler-codegen/src/types/mod.rs`):
   - (A) `?` в функции на Fail-эффекте (return-тип не Result/Option) → `[E_TRY_IN_FAIL_FN]`
     с подсказкой «используй `!!` или `throw`».
   - (B) в return-точке `?` при `E ≠ E'`: проверить не-identity `From[E]` на `E'`; есть →
     разрешить + пометить конверсию; нет → `[E_TRY_NO_FROM]`. Sum-supertype (D85) — fast-path.
2. **Codegen** (`compiler-codegen/src/codegen/emit_c.rs:21236-21340`):
   - (A) убрать ветку Fail-context throw для `?` (`in_fail_ctx`) — становится недостижимой
     (checker отверг раньше); оставить только return-Err/None.
   - (B) на Err-пути при конверсии эмитить `E'.from(e)` через From-mono (`T.from(v)`).
3. **Спека:** amend [D85](../../spec/decisions/04-effects.md#d85): (A) `?` return-only, запрещён
   в Fail-fn; (B) Err-путь авто-`From`. Починить stale `## D4` (04-effects.md:290) + дубль
   `####` (:950) (до-D85, противоречат). Поправить doc-comment-примеры on_exit (`?` → `!!`)
   в spec D188 + `std/prelude/{core,protocols,errors}.nv`.
4. **Тесты** `nova_tests/q_auto_from/`: (a) return `E≠E'` с From → PASS; (b) `E≡E'` без
   конверсии; (c) neg `E≠E'` без From → `[E_TRY_NO_FROM]`; (d) neg `?` в Fail-fn →
   `[E_TRY_IN_FAIL_FN]`; (e) identity-blanket НЕ оборачивает; (f) `Option?` без изменений.

## Открытые вопросы

- **`!!` тоже авто-From?** `!!` — оператор effect-throw на Result/Option-значении (Fail).
  Для симметрии с `?` логично тоже авто-конвертить (`throw E'.from(e)`); Rust-аналога у `!!`
  нет. Реши: авто-From только `?` или `?`+`!!`.
- **Identity-blanket — ловушка.** `From[T] for T`
  ([protocols.nv:109](../../std/prelude/protocols.nv#L109)) есть для всех `T` → правило:
  конверсия только если `E ≠ E'` И есть **не-identity** `From[E]` на `E'`.
- **Транзитивность From** — Rust не делает; держать один шаг.
- **D77 4-way auto-derive** — `?` должен видеть синтезированные From-impl
  ([protocols.nv:126-138](../../std/prelude/protocols.nv#L126)).

## Гейт / зависимости

- Реализация компиляторная (checker + codegen) → зона **172**/владельца.
- Пересекается с 172.1 U.4 (typed IR): после U.4 типы ошибок в точке `?` известны чище.

## Followup-маркер

`[M-174-question-from]`.
