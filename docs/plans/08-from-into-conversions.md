# План 08: `From`/`Into` framework + сводная конверсионная инфраструктура

**Статус:** ✅ почти выполнено (2026-05-08); Ф.6 отложен.
**Дата создания:** 2026-05-08.

**Результат:**
- Ф.1+Ф.2 — runtime helpers + bootstrap-table (commit `cb045108f`)
- Ф.3 — 4-way auto-derive synthesis (commit `c6e2c087c`)
- Ф.4 — strict `if cond: bool` (commit `515aa1406`)
- Ф.5 — as-cast restrictions (commit `27efacc35`)
- Ф.7 — spec D54 расширение + `spec/conversions.md` (этот коммит)
- ❌ Ф.6 — generic-bound enforcement (требует полного type-checker'а;
  отложен до полноценной фазы рефакторинга)

**Зависимости:**
- [D54](../../spec/decisions/03-syntax.md#d54) — `as` оператор.
- [D73](../../spec/decisions/08-runtime.md#d73) — `From`/`Into` protocol-pair.
- [D77](../../spec/decisions/08-runtime.md#d77) — `TryFrom`/`TryInto` 4-way auto-derive.
- [D81](../../spec/decisions/08-runtime.md#d81) — narrowing semantics.
- [Plan 05](05-as-cast-codegen.md) — `as` C-cast (closed).
- [Plan 07](07-as-cast-saturation.md) — float→int saturation (closed).

---

## Проблема

Spec D73/D77 описывают полноценный `From`/`Into` framework с
4-way auto-derive (программист пишет одну форму — компилятор
синтезирует остальные три). **В runtime/codegen этого нет**:

1. **Нет helper'ов парсинга** из str. `int.try_from("42")`,
   `f64.try_from("3.14")` объявлены в std, но runtime функций
   `nova_str_to_i64`/`nova_str_to_f64` не существует. Это блокирует
   `url.nv:173`, `toml.nv:306`, `bcrypt.nv:85` (их `int.try_from`
   never compiles).

2. **Нет 4-way auto-derive.** Каждая форма требует ручной
   реализации — программист пишет и `from`, и `into`, и `try_from`
   отдельно. D73 описывает синтез как обязательное поведение, но
   codegen не делает.

3. **Generic-bound `[T Into[X]]` парсится, не enforce'ится.**
   После раунда 6 (commit `4475e2c1d`) bounds парсятся, но в
   type-checker'е игнорируются. То есть программист может написать
   `fn greet[S Into[str]](s S)`, но проверка совместимости не
   делается.

4. **char/byte/str/[]byte конверсии не специфицированы.**
   Парсеры активно нуждаются в `str.from(c char)`, `char.try_from(s)`,
   `[]byte.from(s)`, `str.try_from(bs []byte)`. Сейчас всё это
   ad-hoc через эвристики codegen'а (раунд 4 `chars()`/`bytes()`/
   `split()` через специальный path).

5. **bool конверсии отсутствуют.** Нет `str.from(b bool)`, нет
   `bool.try_from(s str)` для парсинга TOML/INI. Нет обратной
   конверсии в `int`/`byte`/`f64` через `as` (запрещено? разрешено?
   spec не говорит).

6. **`if cond` не enforce'ит `cond: bool`.** Сейчас `if a` где
   `a: int` компилируется (codegen эмитит C-`if`, который truthy).
   Это нарушает strict-typing — `int as bool` запрещаем, но
   `if int_value` разрешено по недосмотру. **Inconsistency**.

7. **Нет сводной spec-страницы по конверсиям.** Программист/LLM
   читая spec должен пройти по 4 D-decisions (D54, D73, D77, D81)
   чтобы понять «как конвертировать X в Y». AI-unfriendly.

---

## Цель

Реализовать **базовый** `From`/`Into` framework + char/byte/bool
конверсии + строгий `if cond: bool` + сводную spec-страницу.

После плана 08:

- ✅ `int.try_from(s str)?` работает для всех 10 числовых типов.
- ✅ `n.into()` работает для widening в int / f64 (через runtime
  registrations).
- ✅ `str.from(c char)` / `char.try_from(s str)` / etc — char/byte
  пары полные.
- ✅ `bool.try_from(s)` / `str.from(b bool)` / `b as int` работает.
- ✅ `int as bool` / `int as char` / `if int_value` — compile errors
  с suggestions.
- ✅ Auto-derive 4-way: программист написал `try_from` — компилятор
  даёт `from`/`into`/`try_into`.
- ✅ Generic-bound `[T Into[X]]` enforce'ится при инстанциации.
- ✅ `spec/conversions.md` — сводная страница, single source of truth.

---

## Не цель

- **Полный D73 framework с registry для пользовательских типов.**
  Bootstrap-table в компиляторе достаточно — она знает встроенные
  пары. Регистрация пользовательских `from`/`into` через AST-walk
  module-level deklaration'ов уже работает (`method_receivers` и
  `into_targets` в `emit_c.rs`).
- **Orphan rule.** Структурная типизация Nova (D53) не требует
  coherence на уровне crates. Конфликты разрешаются на use-site.
- **Транзитивный auto-derive.** Если `int.from(i32)` и `f64.from(int)`,
  это **не даёт** `f64.from(i32)` автоматически. Каждая пара
  регистрируется явно. Если нужно — программист пишет.
- **Lossy `as`-conversions из float в narrowing-int.** Уже сделано
  в плане 07 (saturation).

---

## Что делаем

### Ф.1 — Runtime helpers

Новый файл `compiler-codegen/nova_rt/conv.h` (или дополнение
`nova_rt.h`):

```c
// String parsing
typedef struct { nova_int value; nova_bool ok; } nova_parse_int_result;
typedef struct { double value; nova_bool ok; } nova_parse_f64_result;

nova_parse_int_result nova_str_to_i64(nova_str s);
// uint версия — отдельная, потому что overflow-границы разные
typedef struct { uint64_t value; nova_bool ok; } nova_parse_u64_result;
nova_parse_u64_result nova_str_to_u64(nova_str s);
nova_parse_f64_result nova_str_to_f64(nova_str s);

// Char ↔ str (UTF-8)
nova_str   nova_char_to_str(nova_int codepoint);             // infallible UTF-8 encode
typedef struct { nova_int value; nova_bool ok; nova_int err_kind; } nova_char_decode_result;
nova_char_decode_result nova_str_to_char(nova_str s);        // 0=ok, 1=empty, 2=multi-char

// Char ↔ int (range check)
nova_char_decode_result nova_int_to_char(nova_int n);        // err: out-of-range / surrogate
typedef struct { nova_byte value; nova_bool ok; } nova_byte_check_result;
nova_byte_check_result nova_char_to_byte(nova_int c);        // ok if c < 256

// []byte ↔ str (UTF-8 validation)
typedef struct { nova_str value; nova_bool ok; nova_int err_pos; } nova_str_validate_result;
nova_str_validate_result nova_bytes_to_str(NovaArray_nova_byte* bs);
NovaArray_nova_byte* nova_str_to_bytes(nova_str s);          // infallible (UTF-8 already)

// Bool ↔ str
nova_str   nova_bool_to_str(nova_bool b);                    // "true" / "false"
typedef struct { nova_bool value; nova_bool ok; } nova_bool_parse_result;
nova_bool_parse_result nova_str_to_bool(nova_str s);         // matches "true"/"false"
```

~10 хелперов, ~200 строк C.

### Ф.2 — Bootstrap-table в компиляторе

В codegen'е завести таблицу `built_in_conversions: HashMap<(Source, Target), ConversionKind>`.

**str → numeric (10 try_from):**

```rust
register_try_from("int",  "str", "ParseIntError",   "nova_str_to_i64");
register_try_from("i8",   "str", "ParseIntError",   "nova_str_to_i64"); // + range check
register_try_from("i16",  "str", "ParseIntError",   "nova_str_to_i64"); // + range check
register_try_from("i32",  "str", "ParseIntError",   "nova_str_to_i64"); // + range check
register_try_from("i64",  "str", "ParseIntError",   "nova_str_to_i64");
register_try_from("u8",   "str", "ParseIntError",   "nova_str_to_u64"); // + range check
register_try_from("u16",  "str", "ParseIntError",   "nova_str_to_u64"); // + range check
register_try_from("u32",  "str", "ParseIntError",   "nova_str_to_u64"); // + range check
register_try_from("u64",  "str", "ParseIntError",   "nova_str_to_u64");
register_try_from("f64",  "str", "ParseFloatError", "nova_str_to_f64");
register_try_from("f32",  "str", "ParseFloatError", "nova_str_to_f64"); // + range check
```

**numeric → str (12 from):**

```rust
register_from("str", "int",  "nova_int_to_str");
// аналогично для i8/i16/i32/i64/u8/u16/u32/u64
register_from("str", "f64",  "nova_f64_to_str");
register_from("str", "f32",  "nova_f32_to_str");
register_from("str", "byte", "nova_byte_to_str");
```

**numeric widening (~20 from через as):**

```rust
register_from_via_cast("int",  "i8");   // i8 → int — widening
register_from_via_cast("int",  "i16");
register_from_via_cast("int",  "i32");
register_from_via_cast("int",  "u8");
register_from_via_cast("int",  "u16");
register_from_via_cast("int",  "u32");
register_from_via_cast("int",  "byte");
register_from_via_cast("f64",  "int");
register_from_via_cast("f64",  "i32");
register_from_via_cast("f64",  "f32");
register_from_via_cast("f32",  "int");  // через nova_int_to_f32
register_from_via_cast("f32",  "i32");
// ... и т.д.
```

**char/byte/str/[]byte:**

```rust
register_from("str",       "char",   "nova_char_to_str");        // infallible
register_try_from("char",  "str",    "NotSingleCharError",  "nova_str_to_char");
register_try_from("char",  "int",    "InvalidCodepointError", "nova_int_to_char");
register_try_from("byte",  "char",   "OutOfRangeError",     "nova_char_to_byte");
register_try_from("str",   "[]byte", "Utf8Error",           "nova_bytes_to_str");
register_from("[]byte",    "str",    "nova_str_to_bytes");        // infallible
```

**bool:**

```rust
register_from_via_cast("int",   "bool");   // bool → int: true=1, false=0
register_from_via_cast("byte",  "bool");
register_from_via_cast("f64",   "bool");
register_from("str",  "bool",  "nova_bool_to_str");
register_try_from("bool", "str", "ParseBoolError", "nova_str_to_bool");
```

Итого: ~50 регистраций.

### Ф.3 — Auto-derive 4-way в codegen

Когда видим вызов `T.from(v)` / `v.@into()` / `T.try_from(v)` /
`v.@try_into()`:

1. Если **прямая** реализация есть в bootstrap-table — эмитить её.
2. Иначе — попробовать **синтезировать** из обратной формы по
   правилам D73 (строки 1224-1227 spec'а):

   - `T.from(v)` из `T.try_from(v) -> Result[T, E]`:
     ```c
     // emit: nova_T_static_from(v) wrapper
     // {
     //     auto r = nova_T_static_try_from(v);
     //     if (!r.ok) { nova_throw(r.err); }
     //     return r.value;
     // }
     ```

   - `v.@into() -> T` из `T.from(v)`:
     ```c
     // emit: Nova_<source>_method_into(v) → Nova_T_static_from(v)
     ```

   - `v.@try_into() -> Result[T, E]` из `T.try_from(v)`:
     ```c
     // emit: Nova_<source>_method_try_into(v) → Nova_T_static_try_from(v)
     ```

   - И симметрично для случая когда программист написал `into`/`try_into`.

3. Если ни прямой, ни синтезируемой формы нет — compile error.

### Ф.4 — Strict `if cond: bool` в type-checker

Расширить type-checker (compiler-codegen/src/types/mod.rs) чтобы
**enforce'ить** `cond: bool` для:

- `if cond { ... }`
- `while cond { ... }`
- `cond1 && cond2`, `cond1 || cond2`

Несоответствие → **compile error** с suggestion:

```
error: if condition must be `bool`, got `int`
  in foo.nv:42:8
  │
  42 │ if a { ... }
     │    ^ found `int`
     │
  hint: use explicit comparison
        if a != 0 { ... }       (для truthy-проверки)
        if a > 0  { ... }       (для positive-only)
```

Это **новая** проверка — ранее type-checker был best-effort и
пропускал эту ошибку.

### Ф.5 — `as`-cast restrictions для char/byte/bool

Расширить `ExprKind::As` обработку в codegen + type-checker:

**Разрешено через `as` (прямой C-cast / no-op):**

| Cast | Реализация |
|---|---|
| `byte as char` | `((nova_int)(b))` — codepoint = byte value |
| `char as int` | identity (char is nova_int internally) |
| `char as u32` | `((uint32_t)(c))` |
| `bool as int` | `((nova_int)(b))` |
| `bool as byte` | `((nova_byte)(b))` |
| `bool as f64` | `((double)(b))` |

**Запрещено через `as` (compile error с suggestion):**

| Cast | Suggestion |
|---|---|
| `int as char` | `char.try_from(n)?` |
| `i32 as char`, `u32 as char` | то же |
| `char as byte` | `byte.try_from(c)?` или `(c as int) as byte` для wraparound |
| `int as bool`, `byte as bool`, `f64 as bool` | `n != 0` |
| `str as T`, `T as str` | `str.from(v)` / `T.try_from(s)?` |

### Ф.6 — Generic-bound `[T Into[X]]` enforcement

В type-checker'е при инстанциации generic-функции:

```nova
fn greet[S Into[str]](s S) Io -> ()
greet(42)         // S = int — проверить что int реализует Into[str]
```

Алгоритм:
1. Найти bound `S Into[str]` в generic-list.
2. Подставить `S = int`.
3. Проверить наличие `int.@into() -> str` (либо прямую реализацию,
   либо через bootstrap-table — `str.from(int)` ⟹ `int.@into() -> str`
   через auto-derive).
4. Если нет — compile error с suggestion «implement `Into[str]` for `int`».

### Ф.7 — Spec обновления

#### Ф.7.1 — D54 расширение

Добавить в раздел «Семантика narrowing» (план 07 Ф.4) подразделы:

**«char-конверсии»** — таблица 5 случаев (через `as` / через `try_from`).

**«bool-конверсии»** — таблица 8 случаев (через `as` / запрещено).

**«strict if cond: bool»** — упоминание что `if`/`while`/`&&`/`||`
требуют `bool`, не truthy/falsy. Прецеденты Rust/Swift/Kotlin/Go.

#### Ф.7.2 — Новый документ `spec/conversions.md`

Сводная страница (~280 строк). Структура:

```markdown
# Nova — конверсии типов

Сводка всех правил конверсии в одном месте. Полные D-decisions —
[D54](decisions/03-syntax.md#d54), [D73](decisions/08-runtime.md#d73),
[D77](decisions/08-runtime.md#d77), [D81](decisions/08-runtime.md#d81).

## Три механизма

| Механизм | Когда | Пример |
|---|---|---|
| `as` | infallible numeric/newtype/sum cast | `42 as f64`, `n as i16` |
| `T.from(v)` / `v.into()` | infallible struct/format конверсия | `str.from(42)`, `c.into()` |
| `T.try_from(v)?` | fallible parsing/validation | `int.try_from("42")?` |

Auto-derive 4-way (D73): пишешь одну форму — компилятор даёт три остальные.

## Полная таблица конверсий

### Numeric ↔ numeric

[Таблица 12 типов × 12 типов с пометками as / from / try_from]

### Numeric ↔ str

[Таблица: int.try_from(str), str.from(int), и т.д.]

### Char / Byte / []byte / str

[Таблица всех 6 пар]

### Bool ↔ всё

[Таблица: bool ↔ str, bool ↔ int, и т.д.]

### Newtype ↔ underlying

[Через `as`, идентичность]

### Sum-variant ↔ int (discriminant)

[Через `as` для sum'ов с числовыми discriminants]

## Семантика narrowing

[Из D81 + плана 07 + char/bool sections]

## Что **не происходит** автоматически

Nova **не делает** implicit конверсии:

- `if cond` требует `cond: bool` — не truthy/falsy для int/str/Option.
- `a + b` для разных численных типов — compile error.
- Тип параметра не coerce'ится автоматически.
- `int as bool`, `int as char` — запрещены через `as`.

Прецеденты: Rust, Swift, Kotlin — все запрещают. Python/C/JS —
разрешают, и это известный класс багов.

## Auto-derive 4-way

[Из D73 — кратко + ссылка]

## Запрещённые конверсии — таблица

| Запрещено через `as` | Альтернатива |
|---|---|
| `int as char` | `char.try_from(n)?` |
| `char as byte` | `byte.try_from(c)?` |
| `int as bool` | `n != 0` |
| `if int_value` | `if n != 0` |
| `T as U` для произвольных | `U.from(v)` или `U.try_from(v)?` |

## Прецеденты по языкам

| Язык | Где близок к Nova |
|---|---|
| Rust | `as` semantics, From/Into pair |
| Swift | strict bool, no implicit coerce |
| Kotlin | strict if-cond:bool |
| Go | `_ = strconv.ParseInt(s)` ≈ try_from |
| Python | `str(x)`/`int(s)` ≈ from/try_from но не type-safe |
```

#### Ф.7.3 — `simplifications.md` запись

`[P-from-into-bootstrap-table]` — bootstrap-table перечисляет
встроенные пары; пользовательские типы регистрируются через AST-walk.
Транзитивный auto-derive не делается.

### Ф.8 — Тесты

#### `nova_tests/syntax/from_into.nv`:

- str → int (для каждого из 10 числовых типов)
- int → str через `str.from(n)` / `n.into()`
- numeric widening: `i32` → `int`, `f32` → `f64`, и т.д.
- 4-way synthesis: программист пишет один `try_from` — все 4 формы вызова работают
- Generic-bound `[T Into[str]]` принимает int / str / custom-type
- Generic-bound enforce'ится — non-conforming type → compile error

#### `nova_tests/syntax/char_byte_str.nv`:

- `str.from(c char)` UTF-8 encode (ASCII + multi-byte)
- `char.try_from(s str)` strict 1-char (empty → Err, multi-char → Err)
- `char.try_from(n int)` valid / invalid / surrogate
- `byte.try_from(c char)` ASCII / non-ASCII
- `[]byte.from(s str)` round-trip
- `str.try_from(bs []byte)` valid / invalid UTF-8

#### `nova_tests/syntax/bool_conversions.nv`:

- `b as int` true=1 / false=0
- `b as byte` / `b as f64`
- `str.from(b)` / `bool.try_from(s)`
- `bool.try_from("yes")` → Err
- compile-error tests: `n as bool`, `if n` (где n int)

#### `nova_tests/syntax/as_restrictions.nv`:

- `int as char` → compile error с suggestion
- `char as byte` → compile error
- `int as bool` → compile error
- `if n` (int) → compile error

### Ф.9 — Sweep std

Проверить что после Ф.1-Ф.4 продвинулись:

- `std/encoding/url.nv` — был блок на `int.try_from(port_str)`.
- `std/encoding/toml.nv` — был блок на `f64.try_from(text)`.
- `std/crypto/bcrypt.nv` — был блок на `int.try_from(parts[2])`.

Также — проверить что `std/encoding/json.nv` тесты с
`JsonValue.from(s)` начинают работать (4-way synthesis).

---

## Acceptance criteria

- ✅ `int.try_from("42") == Ok(42)`, `int.try_from("abc")` → Err.
- ✅ `n.into()` работает для widening (i32 → int, f32 → f64).
- ✅ 4-way synthesis: пишешь только `T.try_from(v)`, остальные 3 формы работают.
- ✅ `Generic[S Into[str]]` enforce'ится — non-conforming type → error.
- ✅ char/byte/str/[]byte конверсии работают через try_from / from.
- ✅ bool ↔ str / int работает.
- ✅ `int as bool` → compile error с suggestion.
- ✅ `int as char` → compile error с suggestion.
- ✅ `if n` (int) → compile error с suggestion.
- ✅ `nova_tests/syntax/from_into.nv`, `char_byte_str.nv`,
  `bool_conversions.nv`, `as_restrictions.nv` — все PASS.
- ✅ `spec/conversions.md` создан.
- ✅ `spec/decisions/03-syntax.md` D54 расширен char/bool разделами.
- ✅ `simplifications.md` обновлён `[P-from-into-bootstrap-table]`.
- ✅ url/toml/bcrypt продвинулись через codegen.
- ✅ Существующие тесты — без регрессий.

---

## Trade-offs / упрощения

### Только встроенные пары — пользовательские через ручную регистрацию

Bootstrap-table содержит **только** numeric/char/byte/bool/str
конверсии. Пользовательские (`Celsius → Fahrenheit`) — программист
пишет вручную, codegen уже умеет их подхватывать через AST-walk
(`method_receivers`, `into_targets`). Полный D73-registry с
генериков-aware lookup — отложено до self-host'а.

### Транзитивный auto-derive не делается

`int.from(i32)` + `f64.from(int)` не даёт `f64.from(i32)` автоматически.
Каждая пара регистрируется явно. Это **сознательно** — транзитивный
поиск может выдавать неожиданные пути конверсии и медленнее.

### `if cond: bool` strict — может сломать существующий код

Сейчас type-checker best-effort пропускает `if int_value`. После
плана 08 этот код **сломается**. Sweep std — проверить, нет ли
зависимостей.

### Result[T, Never] для infallible через try_from — отложено

D73 говорит «программист может явно написать `T.try_from(v) ->
Result[T, Never]`» для случаев когда нужна generic-bound
`[U TryFrom[V]]`, но конверсия infallible. Это **edge case** —
в bootstrap не реализуем, программист вручную пишет if-нужно.

### `[]byte.from(s str)` без validation — корректно ли?

UTF-8 строка по определению содержит valid UTF-8 байты — поэтому
конверсия в `[]byte` infallible. Но если кто-то использует
`from_bytes_unchecked` с invalid bytes — round-trip может потерять
их. **Это документированный edge case**, не блокер.

---

## План работ

1. **Ф.1** — runtime helpers (`nova_rt/conv.h`, ~200 строк C).
2. **Ф.2** — bootstrap-table в компиляторе (~150 строк Rust).
3. **Ф.3** — auto-derive 4-way в codegen (~100 строк Rust).
4. **Ф.4** — strict `if cond: bool` в type-checker (~30 строк).
5. **Ф.5** — `as`-cast restrictions (~50 строк).
6. **Ф.6** — generic-bound `Into[X]` enforcement (~80 строк).
7. **Ф.7** — spec: D54 расширение + `conversions.md` + simplifications (~330 строк markdown).
8. **Ф.8** — тесты (~250 строк .nv).
9. **Ф.9** — sweep std + smoke check.

---

## Оценка

**~1200 строк** изменений (Rust + C + markdown + тесты).
**2 дня** компилятор-агента.

Самая сложная часть — Ф.3 (auto-derive 4-way) и Ф.6 (generic-bound
enforcement). Ф.1-Ф.2 — механическая регистрация. Ф.4-Ф.5 —
type-checker дополнения. Ф.7 — текст. Ф.8 — тесты.

---

## Что разблокирует

- **url/toml/bcrypt парсинг** (str → int/f64 через try_from).
- **AI-friendly numeric conversions:** `n.into()` для widening,
  `Generic[T Into[X]]` для accept-anything функций.
- **char/byte/str инфраструктура** для парсеров (regex, json,
  markdown, html).
- **bool ↔ str / numeric** для config-файлов.
- **4-way auto-derive (D73 implementation gap)** — закрывается.
- **Strict if-cond:bool** — закрывает silent-bug class.
- **Сводная spec-страница** — single source of truth для конверсий.

---

## Связь с другими планами

- [Plan 05](05-as-cast-codegen.md) — `as` C-cast (closed). План 08
  расширяет ограничения `as` для char/byte/bool.
- [Plan 06](06-iter-protocol-codegen.md) — Iter[T] protocol;
  не зависит, можно делать параллельно.
- [Plan 07](07-as-cast-saturation.md) — float→int saturation.
  План 08 дополняет его char/bool семантикой в D54 spec.
- [Plan 04](04-buffer-split-and-external.md) — Buffer split +
  `external` keyword. **Делается после плана 08** — план 04
  активно использует auto-derive D73 (`str.from(c char)` ⟹
  `c.into() -> str`), bootstrap-table для `external fn`,
  и 18 числовых типов в WriteBuffer API. Все эти механизмы
  реализуются в плане 08.
- **Будущий план 09** — пользовательский `From`/`Into` registry
  (полноценная D73 framework). Откладывается до self-host'а.

---

## Ссылки

- [spec/decisions/03-syntax.md → D54](../../spec/decisions/03-syntax.md#d54)
  — `as` operator. План 08 Ф.7.1 расширяет.
- [spec/decisions/08-runtime.md → D73](../../spec/decisions/08-runtime.md#d73)
  — `From`/`Into` protocol pair, 4-way auto-derive алгоритм.
- [spec/decisions/08-runtime.md → D77](../../spec/decisions/08-runtime.md#d77)
  — `TryFrom`/`TryInto`.
- [spec/decisions/08-runtime.md → D81](../../spec/decisions/08-runtime.md#d81)
  — narrowing semantics.
- `compiler-codegen/src/codegen/emit_c.rs` — `ExprKind::As`,
  `method_receivers`, `into_targets`.
- `compiler-codegen/src/types/mod.rs` — type checker (требует
  расширения для Ф.4 / Ф.6).
- `std/encoding/url.nv:173` — пример блокера на `int.try_from`.
- `std/encoding/toml.nv:306` — `f64.try_from`.
- `std/crypto/bcrypt.nv:85` — `int.try_from`.
