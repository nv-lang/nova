// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 108: `readonly` field enforcement (D175) + `readonly T` type modifier (D176)

> **Создан:** 2026-05-28 (из обсуждения Plan 91 Ф.2 — `str.as_bytes()` zero-copy + `split` на Nova).
> **Статус:** 📋 proposed
> **Приоритет:** P1 — разблокирует `str.as_bytes() -> readonly []u8` (zero-copy) и `split` на Nova (Plan 91).
> **Оценка:** ~1 dev-week (Ф.1 D175 + Ф.2 D176 + Ф.3 применение).
> **Зависимости:** Plan 91 (потребитель), D36 (расширяет), D144 (слайсы).

---

## 1. Проблема

**D36** (`readonly` поля) — семантика описана, **не реализована**. Флаг
`FieldDecl.readonly: bool` парсится и хранится в AST, но нигде не
читается в type-checker. `acc.id = 999` проходит без ошибки даже если
`id` объявлен `readonly`.

**`str.as_bytes()`** сейчас делает `memcpy` (`nova_str_bytes` в
`array.h`). Zero-copy view невозможен без `readonly []u8` типа —
иначе пользователь может записать в буфер строки и сломать UTF-8
invariant.

**`split` на Nova** (вместо C) требует `str.as_bytes() -> readonly []u8`
и byte-indexed `s[a..b]` (уже есть через D144).

## 2. Дизайн

### D175 — `readonly field` = полный freeze (амендмент D36)

**Уточнение семантики D36:** `readonly field T` запрещает **и** переприсвоение
поля, **и** мутацию содержимого — транзитивно.

```nova
type Account {
    readonly id u64           // нельзя: acc.id = 999
    readonly tags []str       // нельзя: acc.tags = other  И  acc.tags.push("x")
    balance money             // можно у mut binding
}
```

**Сводная таблица (полная, все комбинации):**

| Объявление | Переприсвоить поле | Мутировать содержимое | Use case |
|---|---|---|---|
| `field T` | у `mut` binding | у `mut` binding | большинство полей |
| `readonly field T` | ❌ никогда | ❌ никогда | id, invariants, frozen state |
| `field readonly T` | у `mut` binding | ❌ никогда | mutable ref, immutable content |
| `mut field T` | ✅ всегда | у `mut` binding | cache, lazy init |
| `mut field readonly T` | ✅ всегда | ❌ никогда | swappable readonly view |

**Транзитивность:** `readonly field Account` запрещает `acc.name = "x"` —
мутация через readonly поле транзитивно запрещена.

### D176 — `readonly T` как модификатор типа

`readonly` как prefix-модификатор типа в любой позиции:
- возвращаемый тип: `-> readonly []u8`
- параметр: `fn process(data readonly []u8)`
- поле: `field readonly []u8`
- локальная переменная: `let view readonly []u8 = arr`

**Семантика:**
- Запрещает вызов `mut`-методов на значении типа `readonly T`
- Запрещает запись через индекс: `view[i] = x` → ошибка
- `T` → `readonly T` coercion разрешён (сужение прав — автоматический)
- `readonly T` → `T` запрещён (расширение прав — нарушает invariant)
- `(readonly T) as mut T` — явный unsafe escape hatch (только в `unsafe` блоке)

**Рантайм:** zero overhead — `readonly` только compile-time проверка,
не влияет на codegen.

### Coercion rules

```nova
let arr []u8 = [1, 2, 3]
let view readonly []u8 = arr    // ✅ []u8 → readonly []u8 (автоматически)
let back []u8 = view            // ❌ readonly []u8 → []u8 — E_READONLY_COERCE

fn take_view(data readonly []u8) { ... }
take_view(arr)                  // ✅ автоматический coerce при вызове

// Явный escape — только в unsafe блоке:
unsafe {
    let mutable = view as mut []u8  // ✅ явный opt-in с пониманием последствий
}
```

**`as []u8` (без `mut`) невалидно** — нет неявного пути снятия readonly.
Только `as mut T` с явным ключевым словом подчёркивает намерение.

### Применение к `str`

```nova
// Zero-copy view в буфер строки — UTF-8 invariant защищён.
export external fn str @as_bytes() -> readonly []u8

// Старый @bytes() остаётся как копия (для случаев когда нужна owned копия).
export external fn str @bytes() -> []u8
```

## 3. Spec и документация

### Spec changes

- **`spec/decisions/02-types.md`**:
  - Добавить **D175** как амендмент D36: `readonly field T` = full freeze (оба axis)
  - Добавить **D176**: `readonly T` тип-модификатор, coercion rules, `as mut T` unsafe escape
  - Обновить D36 с ссылкой на D175

- **`spec/decisions/03-memory.md`** (или соответствующий раздел):
  - Описать `as mut T` как unsafe операцию с safety invariant

### Документация

- **`docs/reference/types.md`** (или `language-guide/`): раздел «Readonly types»
  - Примеры readonly fields, readonly type modifier, coercion
  - Таблица всех комбинаций модификаторов поля
- **`std/STATUS.md`**: отметить когда D175+D176 реализованы

## 4. Декомпозиция

### Ф.1 — D175: `readonly` field enforcement в type-checker

- Читать `FieldDecl.readonly` в checker при assignment `obj.field = ...`
- Транзитивная проверка: если receiver — readonly поле, запрещать
  мутацию вложенных полей и `mut`-методов
- Тесты: позитивные + негативные (см. §5)
- Spec: добавить D175 в `spec/decisions/02-types.md` как амендмент D36

### Ф.2 — D176: `readonly T` тип-модификатор

- Parser: `readonly` перед типом → `TypeRef::Readonly(inner)`
- Type-checker: при вызове метода на `readonly T` — проверять `is_mut`
- Type-checker: запрет записи `view[i] = x` на `readonly []T`
- Coercion: `[]T` → `readonly []T` неявный; `readonly T` → `T` ошибка
- `as mut T` — только в `unsafe` блоке; снимает readonly статически
- Spec: добавить D176 в `spec/decisions/02-types.md`

### Ф.3 — Применение: `str.as_bytes()` + `split` на Nova

- `str @as_bytes() -> readonly []u8` — zero-copy (C: возвращает
  `{ s.ptr, s.len }` без `memcpy`)
- Переписать `str @split` на Nova через `as_bytes()` + `[]u8` операции
  + `s[a..b]` byte-indexed (D144)
- `nova_str_split` в `array.h` — оставить как fallback, пометить deprecated

### Ф.4 — Closure

- `std/STATUS.md` обновить (D175 + D176 реализованы)
- `nova test` — 0 новых FAIL
- Spec D175 + D176 финализировать
- Документация в `docs/reference/types.md` (раздел Readonly)

## 5. Тесты

### Позитивные (должны компилироваться и работать):

- `readonly` поле не мутируется даже у `let mut` binding — значение читается корректно
- `readonly []u8` view от `str.as_bytes()` — элементы читаются нормально
- `[]u8` передаётся там, где ожидается `readonly []u8` — автоматический coerce
- `split` на Nova — те же результаты что и C-версия
- `as mut []u8` в unsafe блоке — компилируется без ошибки
- Транзитивность: `type Nested { val int }` + `readonly n Nested` — чтение `n.val` работает

### Негативные (compile errors):

- `acc.readonly_id = 999` → `E_READONLY_FIELD`
- `acc.readonly_tags.push("x")` → `E_READONLY_FIELD` (транзитивно через поле)
- `view[i] = x` на `readonly []u8` → `E_READONLY_CONTENT`
- `readonly []u8` → `[]u8` неявный coerce → `E_READONLY_COERCE`
- `view as []u8` (без `mut`) → ошибка (нет такого пути — только `as mut T`)
- `as mut T` вне `unsafe` блока → `E_UNSAFE_REQUIRED`
- `let mut acc Account = ...; acc.readonly_id = 1` → `E_READONLY_FIELD` (mut binding не снимает)

## 6. Критерии приёмки (Definition of Done)

- [ ] `nova test` — 0 новых FAIL после каждой фазы
- [ ] Все позитивные тесты из §5 PASS
- [ ] Все негативные тесты из §5 дают ожидаемые коды ошибок
- [ ] Spec D175 + D176 записаны в `spec/decisions/02-types.md`
- [ ] `str.as_bytes()` возвращает zero-copy `readonly []u8` (нет memcpy)
- [ ] `str.split` переписан на Nova-body (не C)
- [ ] `std/STATUS.md` обновлён
- [ ] Раздел «Readonly types» в справочной документации

## 7. Связь

- [D36](../spec/decisions/02-types.md#d36) — расширяется D175
- [D144](../spec/decisions/02-types.md#d144) — слайсы `arr[a..b]`
- [Plan 91](91-stdlib-mvp-for-0.1.md) — потребитель: `str.as_bytes()` + `split` на Nova
- [Plan 90.1](90.1-array-extend-family.md) — `[]T` операции (смежно)
