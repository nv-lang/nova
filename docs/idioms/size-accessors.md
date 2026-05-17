# Идиомы: size-accessors (Plan 60 / D117)

> Конвенция Nova по доступу к размерам / cardinality / capacity
> коллекций.

## Правило

**Всегда** `.method()` со скобками. **Никогда** field-style.

```nova
let v = [1, 2, 3]

let n = v.len()              // ✓
let n = v.len                // ✗ error E_SIZE_ACCESSOR_FIELD

if v.is_empty() { ... }      // ✓
if v.is_empty { ... }        // ✗

let c = v.capacity()         // ✓
let c = v.cap                // ✗ rename + parens: .capacity()

let s = "hello"
let b = s.byte_len()         // ✓
let b = s.is_empty()         // ✓
```

## API per type

### `[]T` (built-in array)

| Метод | Возвращает | Cost |
|---|---|---|
| `arr.len()` | `int` — количество элементов | O(1) |
| `arr.capacity()` | `int` — выделенный storage | O(1) |
| `arr.is_empty()` | `bool` — `len() == 0` | O(1) |

### `str` (built-in string)

| Метод | Возвращает | Cost |
|---|---|---|
| `s.len()` | `int` — количество codepoint'ов | O(n) — UTF-8 walk |
| `s.byte_len()` | `int` — количество байтов | O(1) |
| `s.is_empty()` | `bool` — `byte_len() == 0` | O(1) |

### `HashMap`, `Set`, `Lru`, `Deque`, `Queue`, `Range`, `Vec` (user types)

| Метод | Возвращает | Cost |
|---|---|---|
| `c.len()` | `int` | O(1) (типично; зависит от типа) |
| `c.is_empty()` | `bool` | O(1) |
| `c.capacity()` | `int` (только для тех, кто имеет capacity) | O(1) |

## Method-value form

`let f = x.@len` — это **bound method value** типа `fn() -> int`.
Это легитимный паттерн (D-block method-values, [Plan 11](../plans/11-method-values-and-overload.md)),
но требует **явного `@`-prefix** для disambiguation:

```nova
let f = arr.@len    // ✓ bound method value, тип fn() -> int
let f = arr.len     // ✗ error E_SIZE_ACCESSOR_FIELD
let n = arr.len()   // ✓ method call, тип int
```

## Loop с индексом

```nova
// ✓ предпочтительно — for-in (D58 Iter[T])
for x in arr {
    process(x)
}

// ✓ если нужен индекс
for i in 0..arr.len() {
    process(i, arr[i])
}

// ✗ field-style
for i in 0..arr.len { ... }
```

## Почему

См. [D117 в spec](../../spec/decisions/03-syntax.md#d117).

Краткий ответ:
1. **Predictable cost** — скобки = вычисление; без скобок было бы
   странно для `s.len` (O(n) UTF-8 walk на str).
2. **Consistency** — нет смешивания built-in и user-defined APIs.
3. **AI-friendly** — LLM не должен запоминать «для какого типа какая
   форма» (Java-патология).

## Перенесённые legacy паттерны

| Было | Стало |
|---|---|
| `arr.len` | `arr.len()` |
| `arr.cap` | `arr.capacity()` (rename + parens) |
| `arr.is_empty` | `arr.is_empty()` |
| `s.len` | `s.len()` |
| `s.byte_len` | `s.byte_len()` |
| `s.is_empty` | `s.is_empty()` |

Migration см. [docs/migration/plan-60.md](../migration/plan-60.md).

## Сравнение со state-of-the-art

| Language | Подход | Inconsistency? |
|---|---|---|
| Rust | `.len()` method везде | none |
| Go | `len(x)` builtin везде | none (но top-level fn) |
| TypeScript | `.length` property | none (но property) |
| Swift | `.count` property | none (но property) |
| Java | `arr.length` field vs `list.size()` method | **inconsistent** |
| **Nova** | `.len()` method, **с explicit D117 spec** | **none, enforced** |

Nova = Rust паритет + явная D-block enforce (Rust полагается на
convention).
