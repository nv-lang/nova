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
- escape hatch — см. Q1 (не финализировано)

**Рантайм:** zero overhead — `readonly` только compile-time проверка,
не влияет на codegen.

### Coercion rules

```nova
let arr []u8 = [1, 2, 3]
let view readonly []u8 = arr    // ✅ []u8 → readonly []u8 (автоматически)
let back []u8 = view            // ❌ readonly []u8 → []u8 — E_READONLY_COERCE

fn take_view(data readonly []u8) { ... }
take_view(arr)                  // ✅ автоматический coerce при вызове

// Escape hatch: см. Q1 — не финализировано.
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

### D26 rev — `str.len()` = bytes O(1), `str.char_len()` = codepoints O(n)

**Решение принято в Plan 108 (2026-05-28):** Изменить семантику `str.len()`:

| Метод | Возвращает | Сложность | C-runtime |
|---|---|---|---|
| `str @len() -> int` | байты | O(1) | `nova_str_byte_len` |
| `str @char_len() -> int` | codepoints (UTF-8) | O(n) | `nova_str_char_len` |
| `str @byte_len() -> int` | ~~байты~~ (deprecated alias) | O(1) | `nova_str_byte_len` |

**Мотивация:** `len()` = bytes консистентно с Rust/Go/C. `str.len()` с O(n) scan ломает
допущение O(1) для размера. Старая семантика (len = codepoints) называлась «school B (D26)».

**Соглашение о receiver:** `fn Type @method()` == `fn readonly Type @method()` — implicit
readonly receiver. Ключевое слово `readonly` перед `@` в объявлении ЗАПРЕЩЕНО в целях
унификации (ничего в парсере не вводим).

**Известная проблема:** `str.len()` внутри generic-closures (erased `nova_int`) диспетчеризует
к `Nova_Range_method_len` вместо `nova_str_byte_len` из-за `method_receivers["len"] = "Range"`.
Deprecated alias `byte_len()` в generic-closures работает как coincidence. Исправление
отслеживается как **[M-str-len-closure-dispatch]** (follow-up).

### Новые предложения (записать в Q / follow-up)

**`copy_from` fluent (Plan 108 или план 90.x):**  
`fn[T] []T @clone() -> []T` хочет: `[]T.with_capacity(@len()).copy_from(@)`.  
Требует: сделать `copy_from` fluent — возвращать `self` вместо `unit`. Codegen change: emit
`(nova_array_copy_from_T(tmp, src), tmp)` + tmp var. Записать в Q.

**`chars()` → `to_chars()` rename:**  
`as_bytes()` = zero-copy, `to_chars()` = O(n) copy + decode. Конвенция `to_*` = copy.
`chars()` deprecated alias. Записать как amend D26 item.

**`str @into() -> []char` / `str @into() -> []u8` через D73:**  
Type-directed `into()` routing. `let bs: []u8 = s.into()` — D73 уже поддерживает.  
`[]char.from(s str) -> []char` зарегистрировать через D73/D77. Записать в Q.

**`str @compare(other str) -> int`:**  
```nova
fn str @compare(other str) -> int => @as_bytes().compare(other.as_bytes())
```
`[]u8 @compare()` уже существует через `nova_array_compare_nova_byte`.  
Возвращает -1/0/1 (как memcmp). NOT `Option[int]` — сравнение всегда валидно.  
Можно добавить в `std/runtime/string.nv` как Nova-body. Записать в Q.

**`[M-str-len-closure-dispatch]` fix:**  
В `method_receivers` fallback dispatch: если receiver `nova_int` и зарегистрированный
тип = `Range` (heap struct, pointer), не диспетчеризовать. OR: closure body emit должен
отслеживать Nova-type closure params (не только C-type). Записать как P2 follow-up.

## 3. Spec и документация

### Spec changes

- **`spec/decisions/02-types.md`** (или `spec/decisions/08-runtime.md`):
  - **Обновить D26**: `str.len()` = bytes O(1); `str.char_len()` = codepoints O(n).
    `str.byte_len()` — deprecated alias для `len()`. Убрать "school B" ссылку на codepoints.
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
- Escape hatch: см. Q1 (не финализировано)
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

### D26 rev — позитивные тесты (str len = bytes):

- ASCII: `"hello".len() == 5` (5 bytes == 5 codepoints) ✅ implemented
- Cyrillic: `"Привет".len() == 12`, `"Привет".char_len() == 6` ✅
- Emoji: `"ab😀cd".len() == 8`, `"ab😀cd".char_len() == 5` ✅
- Пустая: `"".len() == 0`, `"".char_len() == 0` ✅
- Slice end: `s[i..s.char_len()]` (используем `char_len()` для codepoint-indexed slice) ✅
- `str.as_bytes().len == s.len()` (zero-copy view, байты совпадают) ✅
- Deprecated `byte_len()` alias: `"hello".byte_len() == 5` (= `len()`, работает как alias)

### D26 rev — негативные тесты (семантика разница):

- `"Привет".len() != "Привет".char_len()` — для не-ASCII len ≠ char_len ✅
- `str.len()` внутри generic closure — KNOWN BUG [M-str-len-closure-dispatch]:
  диспетчеризует к `Nova_Range_method_len`. Использовать `byte_len()` как workaround.

### D175/D176 позитивные тесты:

- `readonly` поле не мутируется даже у `let mut` binding — значение читается корректно
- `readonly []u8` view от `str.as_bytes()` — элементы читаются нормально
- `[]u8` передаётся там, где ожидается `readonly []u8` — автоматический coerce
- `split` на Nova — те же результаты что и C-версия
- Транзитивность: `type Nested { val int }` + `readonly n Nested` — чтение `n.val` работает

### D175/D176 негативные тесты (compile errors):

- `acc.readonly_id = 999` → `E_READONLY_FIELD`
- `acc.readonly_tags.push("x")` → `E_READONLY_FIELD` (транзитивно через поле)
- `view[i] = x` на `readonly []u8` → `E_READONLY_CONTENT`
- `readonly []u8` → `[]u8` неявный coerce → `E_READONLY_COERCE`
- `view as []u8` → ошибка (нет пути снятия readonly — см. Q1)
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

## Q. Открытые вопросы

### Q1 — `as mut T`: escape hatch или запрет?

`unsafe` блока в Nova нет и не планируется. Как снять `readonly`?

**Варианты:**

1. **Запретить полностью** — `readonly T` → нельзя снять никак в Nova-коде.
   Обход только через `external fn` на C-стороне (FFI).
   Hard compile-time гарантия без исключений.

2. **`as mut T` везде** (без unsafe) — явный синтаксис как opt-in.
   Риск: слишком легко снять, теряется смысл readonly.

3. **Только через `unsafe fn`** — функция помечена `unsafe`, может принять
   `readonly T` и вернуть `T`. Соглашение, не enforcement.

4. **Не вводить escape совсем** — кому нужен mutable доступ, явно копирует:
   `let copy = view.to_owned()`. Наиболее чистый вариант для языка без
   unsafe-системы.

**Текущий план:** вариант 4 (нет escape), но решение не финализировано.
Удалить `as mut T` из §4/Ф.2 при старте реализации если Q1 не решён.

---

## 8. Связь

- [D36](../spec/decisions/02-types.md#d36) — расширяется D175
- [D144](../spec/decisions/02-types.md#d144) — слайсы `arr[a..b]`
- [Plan 91](91-stdlib-mvp-for-0.1.md) — потребитель: `str.as_bytes()` + `split` на Nova
- [Plan 90.1](90.1-array-extend-family.md) — `[]T` операции (смежно)
