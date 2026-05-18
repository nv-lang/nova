# Plan 68: `print`/`println` как Nova-функции через protocol-as-value

> **Создан:** 2026-05-18. **Статус:** proposed — открытые вопросы не решены.
> **Приоритет:** P1 (архитектурное улучшение, не hotfix).
> **Зависимости:** protocol-as-value в codegen (Ф.0 этого плана).

---

## Зачем

Сейчас `println` — magic identifier, хардкоден в type-checker и кодогене.
Это создаёт дублирование (`infer_print_helper` — Plan 67 hotfix),
непрозрачность для пользователя и невозможность передавать `println`
как значение.

Цель: сделать `print`/`println` обычными Nova-функциями с явной
сигнатурой, видимой в prelude:

```nova
external fn print(...v []Into[str]) Io -> ()
fn println(...v []Into[str]) Io -> () => print(...v, "\n")
```

Это соответствует спеке: D73 (`Into[str]`), D53 (protocol-as-value),
D69 (variadic), строка 782–786 `spec/decisions/08-runtime.md`.

---

## Архитектура

### Ф.0 — protocol-as-value в codegen

`[]Into[str]` — массив erased значений. Каждый элемент хранится как
пара `(vtable*, data*)`. Сейчас codegen не умеет это генерировать —
вместо vtable подставляет `nova_int` (см. проверку 2026-05-18:
`NovaArray_nova_int*` вместо vtable-array, CC-FAIL при mixed типах).

Нужно реализовать:
- Представление `Into[str]` как C-структуры с vtable
- `NovaArray_Into_str*` — массив таких структур
- При передаче `42` в `[]Into[str]` — авто-боксинг: `{ .vtable = &nova_int_into_str_vtable, .data = &v }`
- Вызов `elem.into()` внутри `print` — dispatch через vtable

### Ф.1 — сигнатура в prelude

```nova
external fn print(...v []Into[str]) Io -> ()
fn println(...v []Into[str]) Io -> () => print(...v, "\n")
```

`print` — `external`, реализация в кодогене (специальный emit для
`print` по имени). Сигнатура видна type-checker'у — убирает хардкод
из `types/mod.rs:1992`.

`println` — обычная Nova-функция поверх `print`. Spread `...v` +
append `"\n"` как последний элемент.

### Ф.2 — Io effect + migration

`println` получает `Io` в сигнатуре. Это **breaking change** — весь
существующий код где `println` вызывается без `Io` перестаёт
компилироваться.

Migration strategy: пока не решены открытые вопросы — не делать.

### Ф.3 — убрать magic из кодогена

После Ф.0–Ф.2: удалить `infer_print_helper` (Plan 67 уже сократил
его до ~15 LOC — финальный шаг). `emit_println` заменяется обычным
`emit_fn_call` для `print`.

---

## Открытые вопросы

> Эти вопросы нужно решить перед началом реализации.

### Q1. Представление protocol-as-value в C

Как хранить erased `Into[str]` значение в C?

Варианты:
- **fat pointer** `{ void* vtable; void* data; }` — стандартный подход (Rust `dyn Trait`)
- **tagged union** — менее гибко
- **всегда heap-alloc** — проще, но медленнее

Влияет на весь codegen для erased protocol types, не только `print`.

### Q2. Авто-боксинг при передаче примитивов

`print(42)` — `42` это `nova_int` (стек). При передаче в `[]Into[str]`
нужно создать `{ vtable, &data }`. Но `&42` — адрес временного.

Как: копировать на heap? Или передавать по значению через union?
Выбор влияет на ABI всех protocol-as-value вызовов.

### Q3. Spread `...v` + append в одном вызове

```nova
fn println(...v []Into[str]) Io -> () => print(...v, "\n")
```

`...v` — это spread существующего массива, `"\n"` — новый элемент.
Сейчас spread variadic не реализован в кодогене. Нужно ли реализовать
конкатенацию `[]T + elem` как примитив? Или `println` тоже `external`?

### Q4. Io effect — breaking change стратегия

Сейчас `println` без `Io` работает везде. После изменения:

```nova
fn greet(name str) -> () {
    println("hello ", name)  // ← FAIL: Io не объявлен
}
```

Варианты:
- **Migrate all callers** — большой объём изменений во всём коде
- **Gradual**: сначала без Io (как сейчас), Io добавить отдельным планом
- **Io implicit для main** — main всегда имеет Io, propagation вверх

Нужно решить стратегию до реализации Ф.2.

### Q5. `print` — external или magic?

По ответу на вопрос при создании плана: сигнатура в Nova, реализация
в кодогене. Но тогда кодоген всё равно special-case'ит `print` по
имени функции. Это полное избавление от magic или только перенос
magic на уровень ниже?

Альтернатива: `print` полностью реализован в C runtime как настоящая
external fn (принимает vtable-array, итерирует, вызывает `into()`).
Тогда кодоген не special-case'ит вообще — но требует Ф.0 полностью.

### Q6. Совместимость с `eprintln` / `eprint`

Те же функции для stderr. Делать одновременно или отдельно?

---

## Связь

- **[Plan 67](67-println-overload-return-type.md)** — hotfix `infer_print_helper`;
  Plan 68 делает его obsolete (Ф.3).
- **[Plan 62](62-prelude-hardcode-migration.md)** — migration hardcoded
  builtins в prelude; `print`/`println` — часть этой работы.
- **[spec D53](../../spec/decisions/02-types.md#d53)** — protocol-as-value.
- **[spec D73](../../spec/decisions/08-runtime.md#d73)** — `Into[str]`.
- **[spec D69](../../spec/decisions/03-syntax.md#d69)** — variadic.
