# Plan 60: standardize `.len()` access (POD vs encapsulated unification)

> **Status:** proposed (2026-05-17). Не блокер MVP — quality-of-API issue.

---

## Problem

Сейчас в Nova **inconsistent** доступ к размеру коллекций:

| Тип | Сейчас | Семантика |
|---|---|---|
| `[]T` (built-in array) | `arr.len` (field) | D32: struct `(ptr, len, cap)`, layout — compile-time |
| `str` (built-in string) | `s.len` (field) | Аналогично через `nova_str_char_len` |
| `HashMap`, `Lru`, `Set`, `Range`, `Deque`, `Queue` | `m.len()` (method) | Обёртка над internal `_count` field |

Counts в stdlib: **~280 occurrences `.len`** (field-style) vs **~33 `.len()`** (method-style).

**Bug-симптом** для пользователя: пишет `vec.len` и `hashmap.len()`, не зная заранее который из них работает. Errors появляются on compile.

В spec ([rejected.md:464](../../spec/decisions/history/rejected.md#L464)): per-field `export` отвергнуто, все поля публичны → built-in field-access работает технически, но **не consistent с user types**.

## Что НЕ делаем

- **Field-style для всех** — невозможно (user types скрывают internal state).
- **Магический uniform access** (auto property↔method) — противоречит spec
  ([syntax.md:820](../../spec/syntax.md#L820)): «Скобки обязательны для вызова.
  acc.balance() — вызов, acc.balance — bound method value. Никаких property
  с побочками».

## Решение: метод-only для всех

`.len` без скобок → **error** (или method value `fn() -> int` за пределами
arg-position). Везде `.len()` со скобками.

Аналогично для `cap`, `byte_len`, и других size-like accessors.

### Compiler changes

1. **Built-in `[]T`**:
   - Удалить direct field access `.len` / `.cap`.
   - Добавить runtime-registered methods `[]T.@len() -> int` / `[]T.@cap() -> int`.
   - Внутри codegen: `arr.len()` lowers в `(arr->len)` (zero-cost wrapper, O(1)).

2. **Built-in `str`**:
   - Удалить direct field access `.len` / `.byte_len`.
   - Добавить `str.@len() -> int` / `str.@byte_len() -> int`.
   - Existing `nova_str_char_len(s)` остаётся как implementation.

3. **Other built-ins** (`Buffer`, `StringBuilder`, etc.) уже method-based ✓.

4. **User types** (HashMap/Lru/...) уже method-based ✓.

### Spec changes

- Новый D-block (D-XXX) в `spec/decisions/03-syntax.md`:
  > **Size accessors требуют скобок.** `arr.len()` — единственная корректная
  > форма. `arr.len` (без скобок) — это **bound method value** (`fn() -> int`),
  > что обычно ошибка пользователя.
- Update `spec/decisions/02-types.md` D32: упомянуть что `len`/`cap`
  не exposed как fields, доступ — через methods.

### Migration

- **Stdlib**: ~280 occurrences переписать `.len` → `.len()`. Mechanical replace
  (но осторожно с false-positives типа `result.len == 0` где `len` — local var
  не field). Скрипт + manual review.
- **User code**: breaking change для всех `.len`/`.cap` без скобок на `[]T`/`str`.
  Diagnostic: compiler emit'ит `error: size access via field — use method call .len()` с
  fix-it hint.
- **GrowthBook-style flag** не нужен — это compile-time error, instant migration.

## Scope / Estimate

- Compiler: ~150-300 LOC (registry add, codegen disable field-path для `[]T/str.len`).
- Stdlib migration: ~280 lines update.
- Spec: 1 new D-block + D32 amend.
- Tests: ~50 regression tests (positive + negative).
- Migration tooling: nova fix-it hint в diagnostics.

**Total estimate:** 2-4 dev-days.

## Open questions

- `.first()` / `.last()` для `[]T` — тоже надо method-only? (Сейчас часть из них уже method'ы из stdlib_vec.nv.) Скорее всего — да.
- `.empty()` / `.is_empty()` — `.is_empty()` уже convention; `empty()` ambiguous (factory? predicate?). Plan 60 → standardize на `.is_empty()`.
- Backwards compat для `arr.len` в loops `for i in 0..arr.len` — после migration `0..arr.len()`. Не более verbose, just consistent.

## Why not done as part of Plan 11

Plan 11 уже закрыт по scope (method values, overload resolution). Эта работа —
**отдельная архитектурная унификация**, требует spec D-block и migration. Не
вписывается в Plan 11 follow-up.

## Acceptance criteria

- ✅ `arr.len` (без скобок) — compile error с fix-it suggestion.
- ✅ `arr.len()` lowers в эквивалентный по скорости C-код (zero-cost).
- ✅ Все stdlib используют `.len()` consistently.
- ✅ Все sample code в `docs/`, `examples/`, `spec/` обновлены.
- ✅ Migration FAQ в docs/migration/plan-60.md.

## Связь с другими планами

- **Plan 45** (`nova doc`): после Plan 60 — обновить stdlib doc-comments с consistent `.len()` references.
- **Plan 11** (method values): закрыт; Plan 60 продолжает spirit «scope rules → predictable behavior».
- **Plan 56** (mono): mono dispatch для `[]T.@len()` нужен — будет through method_overloads registration.

## Ссылки

- [spec/syntax.md:820](../../spec/syntax.md#L820) — «Скобки обязательны для вызова».
- [spec/decisions/02-types.md → D32](../../spec/decisions/02-types.md#d32) — array layout.
- [spec/decisions/history/rejected.md:464](../../spec/decisions/history/rejected.md#L464) — per-field export.
- [docs/plans/11-method-values-and-overload.md](11-method-values-and-overload.md) — context.
