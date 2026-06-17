<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 167 — E_TYPE_NAME_TOO_SHORT: запрет однобуквенных имён типов

> **Создан:** 2026-06-17. **Статус:** ✅ CLOSED 2026-06-17. **Приоритет:** M.
> **Эстимат:** ~0.5 dev-day. **Model:** Sonnet 4.6.
> **Маркер:** [M-single-letter-type-ban].

## Идея

Запретить `type X { ... }` где имя типа длиной 1 символ.

Мотивация: однобуквенные имена конфликтуют с generic-параметрами функций
(`fn[S Iter[T]]` vs `type S`), вызывая E_PREFIX_SHADOWS_NAMED_TYPE.
Haskell решает регистром (type vars строчные), Nova решает запретом
однобуквенных типов — generic-параметры остаются однобуквенными по конвенции.

Принцип D30: типы = PascalCase, полные слова. Однобуквенный тип — нарушение.

## Что добавить

- E_TYPE_NAME_TOO_SHORT: `type X` → error, suggest `type Xx` (descriptive name)
- Проверка в `check_type_decl()` в types/mod.rs: `if td.name.len() == 1`
- D-block в spec/decisions/02-types.md или 03-syntax.md (D30 amend)
- Sweep nova_tests/: 37 файлов с однобуквенными типами

## Фазы

### Ф.1 — Реализация E_TYPE_NAME_TOO_SHORT
- types/mod.rs: в check_type_decl() — добавить проверку td.name.len() == 1
- Диагностика с fix-it подсказкой

### Ф.2 — Миграция тестов
- 37 файлов nova_tests/ с однобуквенными типами — переименовать
- Стратегия: type S → type Sv (v = value), type A → type Av, etc.
  ИЛИ по контексту: type S { v i64 } → type Sv { v i64 }

### Ф.3 — Spec D30 amend
- spec/decisions/03-syntax.md или 02-types.md: добавить правило
- E_TYPE_NAME_TOO_SHORT в таблицу error codes

### Ф.4 — Тесты plan167/
- POS: `type Foo { x int }` — OK
- NEG: `type S { x int }` → E_TYPE_NAME_TOO_SHORT
- NEG: `type A` (newtype) → E_TYPE_NAME_TOO_SHORT

### Ф.5 — Критерии приёмки
- A1. `type X` (1 символ) → E_TYPE_NAME_TOO_SHORT
- A2. `type Xy` (2+ символов) → OK
- A3. Все 37 мигрированных тестов PASS
- A4. nova test plan167: все PASS
- A5. plan118_1_addr_chains: 12/12 PASS (было 9/12 из-за type S)
- A6. Нет новых FAIL в регрессии

## Статус

✅ CLOSED 2026-06-17

## ИТОГ

Реализован запрет однобуквенных имён типов (E_TYPE_NAME_TOO_SHORT).

- Проверка в `check_type_decl()` (`types/mod.rs`): `td.name.chars().count() == 1` → error.
- D30 §naming amend: spec/decisions/02-types.md.
- Мигрировано 37 тестовых файлов `nova_tests/`.
- `plan118_1_addr_chains`: 12/12 PASS (было 9/12 из-за `type S`).
- `plan167`: все PASS.
- `[M-single-letter-type-ban]` CLOSED.
