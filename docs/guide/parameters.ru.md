---
source_rev: 21dff1b37
source_date: 2026-08-02
---

# Параметры функций в Nova

[English](parameters.md) | **Русский**

> Руководство для пользователя по модификаторам параметров и их семантике.

## TL;DR

Параметры функций — **read-only по умолчанию**.  Хочешь менять — пиши `mut`.

```nova
fn append(mut b []int, v int) { b.push(v) }   // ✓ mutates
fn count(b []int) -> int => b.len()           // ✓ read-only (default)
fn count(ro b []int) -> int => b.len()  // ✓ ro (synonym default)
fn drain(consume b []int) { ... }             // ✓ ownership transfer
```

## Модификаторы

| Модификатор | Что разрешено вызываемому (callee) | Передача у вызывающего (caller) |
|---|---|---|
| (нет) — default | чтение, итерация, non-mut-методы | заимствование (владеет caller) |
| `mut` | + mut-методы (`.push`, `.append` и т.п.), присваивание по индексу | заимствование (владеет caller) |
| `ro` | то же, что и default — синоним | заимствование (владеет caller) |
| `consume` | всё (owned), включая mut-методы | перемещение (связывание caller'а мёртво) |

## Правила сочетания

- `mut` + `consume` — ✗ `E_PARAM_MOD_CONFLICT` (consume уже подразумевает mut)
- `mut` + `ro` — ✗ `E_PARAM_MOD_CONFLICT` (взаимоисключают)
- `ro` + `consume` — ✗ `E_PARAM_MOD_CONFLICT` (`ro` запрещает мутацию, consume требует владения)

## Когда использовать что

### `mut` — нужно изменить и вернуть вызывающему изменённое

```nova
fn append_world(mut sb StringBuilder) { sb.append(" world") }

ro sb = StringBuilder.from("hello")
append_world(sb)
ro s = sb.as_str()                  // "hello world" — мутация видна
```

### default или `ro` — только читать (с производством результата)

```nova
fn sum(b []int) -> int {
    mut total = 0
    for x in b { total = total + x }
    total
}
```

Используй `ro` явно, когда хочешь подчеркнуть гарантию в API
(особенно для FFI/документации):

```nova
export fn hash(ro bytes []u8) -> u64 => ...
```

### `consume` — забираешь владение

```nova
fn finalize(consume sb StringBuilder) -> str => sb.as_str()

consume sb = StringBuilder.from("x")
ro s = finalize(sb)                  // sb dead after this
```

## Диагностики

| Код | Когда |
|---|---|
| `E_PARAM_NOT_MUT` | вызов mut-метода на параметре без `mut` |
| `E_PARAM_MOD_CONFLICT` | взаимоисключающие модификаторы |
| `E_READONLY_COERCE` | передача `ro T` в `T` параметр (где `T` ожидает изменяемый) |

Все — с машинно-применимыми предложениями.

## Приведение (coercion / subtyping) для параметров

Поскольку `T` в позиции параметра **уже только для чтения** (Plan 108.1 default),
большинство комбинаций — тождество.  Единственное нарушение:
`ro → mut`.

| Тип у caller'а → параметр callee | OK? |
|---|---|
| `T` → `T` (параметр по умолчанию только для чтения) | ✓ (сужение) |
| `T` → `ro T` (параметр с явным `ro`) | ✓ (синоним default) |
| `T` → `mut T` (param explicit mut) | ✓ (caller разрешает mut) |
| `ro T` → `T` (параметр по умолчанию только для чтения) | ✓ — оба только для чтения |
| `ro T` → `ro T` | ✓ |
| `ro T` → `mut T` (param explicit mut) | ✗ `E_READONLY_COERCE` |
| `mut T` → `T` (параметр по умолчанию только для чтения) | ✓ (сужение) |
| `mut T` → `mut T` | ✓ |

## Мутабельность receiver в методах

Мутабельность receiver задаётся отдельно от обычных параметров:

```nova
fn StringBuilder @len() -> int               // read-only receiver
fn StringBuilder mut @append(s str) -> @     // mut receiver
fn StringBuilder consume @as_str() -> str    // consume receiver
```

## Локальные let-связывания (Plan 108.2)

Внутри тела функции локальные связывания подчиняются тому же правилу,
что и параметры: **без `mut` — read-only**.

```nova
ro arr = []
arr.push(1)                       // ✗ E_LOCAL_NOT_MUT
mut arr = []
arr.push(1)                       // ✓
```

`consume X = ...` неявно подразумевает `mut` (как и `consume`-параметр).

## Переменная цикла (loop-var) и pattern (Plan 108.3)

### `for mut x in iter`

Переменная цикла по умолчанию read-only.  Опциональный `mut`:

```nova
for x in arrs { x.push(1) }       // ✗ E_LOCAL_NOT_MUT
for mut x in arrs { x.push(1) }   // ✓
```

`for consume x in iter` — неявный mut (передача владения).

### Pattern: mut на каждое имя отдельно

При destructure `mut` ставится **на каждое имя отдельно** (в стиле Rust):

```nova
ro (a, b) = pair                  // оба immutable
ro (mut a, b) = pair              // a mutable, b immutable
ro (a, mut b) = pair              // a immutable, b mutable
ro (mut a, mut b) = pair          // оба mutable
```

**Запрет group-mut** — `let mut (a, b) = ...` отвергается на уровне парсера
(`E_PATTERN_GROUP_MUT`): ключевое слово `mut` относится к одному имени,
не к pattern целиком.

## Ссылки

- `spec/decisions/02-types.md` D176 — формальная спека параметров.
- `spec/decisions/02-types.md` D36 + amend Plan 108.2/108.3 — формальная спека локальных + переменной цикла + pattern.
- `docs/dev/migration/d176-param-readonly-default.md` — руководство по миграции параметров.
- `docs/dev/migration/d36-let-mut-enforcement.md` — руководство по миграции локальных.
- D131 (Plan 73) — аффинная семантика consume.
- D157 (Plan 100.3) — заём-представление для consume-типов.
