# Nova — конверсии типов

Сводная страница всех правил конверсии в одном месте. Полные
D-decisions: [D54](decisions/03-syntax.md#d54),
[D73](decisions/08-runtime.md#d73), [D77](decisions/08-runtime.md#d77).

---

## Три механизма

| Механизм | Когда | Пример |
|---|---|---|
| `as` | infallible numeric/newtype/sum cast | `42 as f64`, `n as i16` |
| `T.from(v)` / `v.into()` | infallible struct/format конверсия | `str.from(42)`, `c.into()` |
| `T.try_from(v)?` | fallible parsing/validation | `int.try_from("42")?` |

**Auto-derive 4-way** (D73): пишешь одну форму — компилятор синтезирует три остальные.
- `T.from(v V)` ↔ `v.@into() -> T`
- `T.try_from(v V)` ↔ `v.@try_into() -> Result[T, E]`

---

## Numeric ↔ numeric

### Widening (no precision loss)

| From → To | Через | Семантика |
|---|---|---|
| `i8 → i16/i32/i64/int` | `as` | sign-extend |
| `u8 → u16/u32/u64/int` | `as` | zero-extend |
| `i8/u8 → f64` | `as` | exact (любой int64 representable как f64) |
| `f32 → f64` | `as` | exact |

### Narrowing (potential precision loss)

| From → To | Через | Семантика |
|---|---|---|
| `i64 → i32/i16/i8` | `as` | wraparound (modulo 2^N) |
| `u64 → u32/u16/u8/byte` | `as` | wraparound |
| `f64 → f32` | `as` | IEEE rounding (потеря точности) |
| **`f64/f32 → iN/uN`** | `as` | **saturation** + NaN→0 + ±∞→bounds |

**Float→int saturation** — defined behavior на любом входе (отличие
от C/C++ UB). Согласовано с Rust 1.45+.

```nova
let n = 1e20 as int             // saturates to INT64_MAX
let m = (-1.0) as u32           // saturates to 0
let nan = 0.0 / 0.0 as i16      // 0
```

---

## Numeric ↔ str

### str → numeric (parse, fallible)

| From → To | Через | Failure |
|---|---|---|
| `str → int/i64` | `int.try_from(s)?` | non-digit / overflow |
| `str → i8/i16/i32` | `i32.try_from(s)?` | + range out-of-bounds |
| `str → u8/u16/u32/u64` | `u32.try_from(s)?` | + negative / overflow |
| `str → f64/f32` | `f64.try_from(s)?` | invalid number format |

```nova
let n = int.try_from("42")?     // Ok(42)
let m = int.try_from("abc")     // Err
let f = f64.try_from("3.14")?   // Ok(3.14)
```

### numeric → str (format, infallible)

| From → To | Через |
|---|---|
| `int/iN/uN → str` | `str.from(n)` |
| `f64/f32 → str` | `str.from(f)` |
| `byte → str` | `str.from(b)` |

```nova
let s = str.from(42)            // "42"
let f = str.from(3.14)          // "3.14"
```

---

## Char / Byte / []byte / str

### char → str (UTF-8 encode)

| Через | Семантика |
|---|---|
| `str.from(c char)` | infallible UTF-8 encode (1-4 байта) |
| `c.into() -> str` | auto-derived из `str.from(char)` |

### str → char (single codepoint, fallible)

| Через | Failure |
|---|---|
| `char.try_from(s str)?` | empty / multi-char / invalid UTF-8 |

### int → char (codepoint range-check, fallible)

| Через | Failure |
|---|---|
| `char.try_from(n int)?` | `n < 0` / `n > 0x10FFFF` / surrogate |

### char → byte (only if codepoint < 256, fallible)

| Через | Failure |
|---|---|
| `byte.try_from(c char)?` | codepoint > 0xFF |

**Исключение:** `'A' as byte`, `'A' as int`, `'A' as u8` — разрешены
для char-литералов (compile-time-known codepoint).

### []byte ↔ str

| From → To | Через | Failure |
|---|---|---|
| `str → []byte` | `bytes()` метод | infallible (UTF-8 уже валиден) |
| `[]byte → str` | `str.try_from(bs []byte)?` | invalid UTF-8 |

---

## Bool ↔ всё

| From → To | Через | Семантика |
|---|---|---|
| `bool → int` | `as` | `true=1`, `false=0` |
| `bool → byte` / `bool → f64` | `as` | то же |
| `bool → str` | `str.from(b)` | `"true"` / `"false"` |
| `str → bool` | `bool.try_from(s)?` | match `"true"`/`"false"` strict |
| **`int/byte/f64/etc → bool`** | **запрещено** | use `n != 0` |

```nova
let s = str.from(true)          // "true"
let b = bool.try_from("true")?  // Ok(true)
let n = if x != 0 { ... }       // explicit
```

---

## Newtype ↔ underlying

| Через | Семантика |
|---|---|
| `n as MyNewtype` | identity (одинаковое C-представление) |
| `nt as int` | identity |

```nova
type UserId alias int
let u UserId = 42 as UserId
let n int = u as int            // 42
```

---

## Sum-variant ↔ int (discriminant)

Для sum'ов с числовыми discriminants:

```nova
type ErrorCode | NotFound = 404 | InternalError = 500
let code = NotFound as int      // 404
```

`int → Sum` через `as` **запрещён** (число может не попасть в варианты).
Используй pattern match.

---

## Strict if cond:bool / while cond:bool

`if cond`, `while cond`, `cond1 && cond2`, `cond1 || cond2` —
**cond обязан быть `bool`**. Truthy-int (`if a` где `a: int`)
запрещён.

```nova
let n int = 5
if n { ... }                    // ❌ compile error
if n != 0 { ... }               // ✅
```

**Прецеденты:** Rust, Swift, Kotlin — все требуют bool. Python/C/JS —
truthy, известный bug-class.

---

## Запрещённые конверсии — таблица

| Запрещено через `as` | Альтернатива |
|---|---|
| `int as char`, `iN/uN as char` | `char.try_from(n)?` |
| `char as byte` (кроме CharLit) | `byte.try_from(c)?` |
| `int/byte/f64 as bool` | `n != 0` |
| `str as int/i32/f64/bool` | `T.try_from(s)?` |
| `int/f64/bool/char as str` | `str.from(v)` |
| `T as U` для произвольных types | `U.from(v)` или `U.try_from(v)?` |
| `if int_value` | `if n != 0` |

---

## Auto-derive 4-way

[D73](decisions/08-runtime.md#d73) — программист пишет одну форму
конверсии, компилятор даёт три:

```nova
// Программист пишет одну:
fn Celsius.try_from(n int) -> Result[Celsius, str] => ...

// Компилятор синтезирует:
fn int @try_into() -> Result[Celsius, str] => Celsius.try_from(@)
```

Bootstrap-status: реализовано в codegen (Plan 08 Ф.3) для пары
`try_from` ↔ `try_into` и `from` ↔ `into`. Транзитивный auto-derive
(`A.from(B)` + `B.from(C)` ⇒ `A.from(C)`) **не делается** — каждая пара
регистрируется явно.

---

## Прецеденты по языкам

| Язык | Где близок к Nova |
|---|---|
| Rust | `as` semantics, From/Into pair, char::from_u32 |
| Swift | strict bool, no implicit coerce, Int(throwing:) |
| Kotlin | strict if-cond:bool, .toInt()/.toIntOrNull() |
| Go | `_ = strconv.ParseInt(s)` ≈ try_from |
| Python | `str(x)`/`int(s)` ≈ from/try_from но не type-safe |
| C/C++ | `(int)x` без проверок — UB-class, Nova не повторяет |

---

## Bootstrap status (2026-05-08)

Реализовано в bootstrap-codegen:

- ✅ Plan 05: `as`-cast как явный C-cast (narrowing wraparound для int)
- ✅ Plan 07: float→int saturation (defined на NaN/Inf/out-of-range)
- ✅ Plan 08 Ф.1+Ф.2: runtime helpers + bootstrap-table для int/f64/bool/char ↔ str
- ✅ Plan 08 Ф.3: 4-way auto-derive synthesis (try_from ↔ try_into, from ↔ into)
- ✅ Plan 08 Ф.4: strict `if cond: bool` (codegen check)
- ✅ Plan 08 Ф.5: as-cast restrictions для char/byte/bool

Не реализовано (отложено):

- ❌ Plan 08 Ф.6: generic-bound `[T Into[X]]` enforcement в type-checker
- ❌ Транзитивный auto-derive (consciously)
- ❌ Compile-error suggestions с file:line:col (TBD)
- ❌ char as raw-bytes / lossy unicode conversions

---

## Ссылки

- [03-syntax.md → D54](decisions/03-syntax.md#d54) — `as` оператор
- [03-syntax.md → D44](decisions/03-syntax.md#d44) — числовые литералы
- [03-syntax.md → D81](decisions/03-syntax.md#d81) — narrowing semantics
- [02-types.md → D52](decisions/02-types.md#d52) — newtype declarations
- [08-runtime.md → D73](decisions/08-runtime.md#d73) — From/Into protocol
- [08-runtime.md → D77](decisions/08-runtime.md#d77) — TryFrom/TryInto
