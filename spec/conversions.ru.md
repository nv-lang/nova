# Nova — конверсии типов

Сводная страница всех правил конверсии в одном месте. Полные
D-decisions: [D54](decisions/03-syntax.md#d54) (`as`),
[D52](decisions/02-types.md#d52) (newtype/alias/sum),
[D325](decisions/04-effects.md#d325) (единый fallible-контракт std),
[D410](decisions/03-syntax.md#d410) (`to_str`/`bytes`-семейство),
[D429](decisions/02-types.md#d429) (`#coerce` — zero-cost implicit),
[D430](decisions/04-effects.md#d430) (checked narrowing `try_to_*`).
`From`/`Into`/`TryFrom`/`TryInto` как **протоколы** ретрактированы
2026-07-06 ([D73](decisions/08-runtime.md#d73)/[D77](decisions/08-runtime.md#d77)) —
подробности в разделе «Именование `from`/`try_from`» ниже.

---

## Три механизма

| Механизм | Когда | Пример |
|---|---|---|
| `as` | infallible numeric/newtype/sum cast, compile-time, без runtime-кода | `42 as f64`, `n as i16` |
| `.to_str()` | универсальная конверсия значения **в строку** (bare-`T` blanket + специализации) | `42.to_str()`, `bs.to_str()` |
| `T.from(v)` / `T.try_from(v)` | конкретный статик-конструктор — **имя-конвенция**, НЕ протокол/auto-derive | `Fahrenheit.from(c)`, `u32.try_from(port_str)` |
| `consume @into_ЦЕЛЬ()` | потребляющая передача владения (конкретное имя на источнике) | `sb.into_str()`, `wb.into_bytes()` |
| `#coerce` | декларативная **неявная** zero-cost конверсия в позиции с известным типом (view/finalize) | `w.write(s)` — `str` неявно `.bytes()` |

**Важно (2026-07-06 ретракция, см. ниже):** `.from(v)` / `.try_from(v)` —
это ПАРА конкретных статик-методов на конкретном типе, не generic-протокол
`From[T]`/`TryFrom[T,E]`. Компилятор **не синтезирует** обратную форму
(`.into()`/`.try_into()`) автоматически — программист пишет ровно то, что
объявил. «Универсального» `.into()` в языке больше нет.

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
ro n = 1e20 as int             // saturates to INT64_MAX
ro m = (-1.0) as u32           // saturates to 0
ro nan = 0.0 / 0.0 as i16      // 0
```

### Checked narrowing — `try_to_*` ([D430](decisions/04-effects.md#d430), 2026-07-20)

`as` между целочисленными ширинами всегда wraparound (тихая потеря
старших бит). Если нужна **проверка**, а не тихий wrap — bounded-бланкет
`@try_to_<T>()` на любом типе из `Ints`-набора, симметрично для всех
целевых ширин (`i8`/`i16`/`i32`/`i64`/`int`/`u8`/`u16`/`u32`/`u64`/`uint`):

```nova
ro ok = (100 as u32).try_to_u8()       // Ok(100 as u8)
ro err = (300 as u32).try_to_u8()      // Err(RangeError) — не влезло
ro neg = (-1 as i32).try_to_u8()       // Err(RangeError) — отрицательное → unsigned
```

`RangeError` — unit-тип («не влезло», без payload — сам факт исчерпывающий).
`as` остаётся быстрым обрезающим кастом без изменений — `try_to_*` не
заменяет его, а добавляет проверяемую альтернативу рядом.

---

## Numeric ↔ str

### str → numeric (parse, fallible) — метод НА ИСТОЧНИКЕ, не статик на цели

**Канон (Plan 174.1, 2026-07-08, owner decision — superseded ранний
static-constructor дизайн `T.parse(s)`/`T.try_from(s)`):** конверсия строки
в число — это метод **на `str`** (`s.to_int()`), а не статик-конструктор на
целевом типе. Зеркалит `s.to_str()`-семью в обратную сторону.

| From → To | Через | Failure |
|---|---|---|
| `str → int` | `s.to_int(radix: int = 10)` | non-digit / overflow / (custom radix) invalid radix |
| `str → i64/u64` | `s.to_i64()` / `s.to_u64()` | без доп. range-check (та же ширина, что движок) |
| `str → i8/i16/i32/u8/u16/u32` | `s.to_i8()` / `s.to_i16()` / `s.to_i32()` / `s.to_u8()` / `s.to_u16()` / `s.to_u32()` | + range-check в целевую ширину |
| `str → f64` | `s.to_f64()` | invalid number format |

```nova
fn parse_decimal(s str) -> Result[int, ParseIntError] =>
    Ok(s.to_int()?)             // radix 10 по умолчанию, Ok(42)

fn parse_hex(s str) -> Result[u32, ParseIntError] =>
    Ok(s.to_u32(radix: 16)?)    // hex-парсинг

fn parse_decimal_f64(s str) -> Result[f64, ParseFloatError] =>
    Ok(s.to_f64()?)             // Ok(3.14)
```

Ошибки — структурные enum'ы: `type ParseIntError enum Empty | InvalidDigit
| Overflow | InvalidRadix` и `type ParseFloatError enum Empty | Invalid`
(`std/runtime/string/parse.nv`).

### str → bool (parse, fallible)

**Канон (Plan 232.1 Т1, owner decision «добавить», 2026-07-26):**
`s.to_bool()` — строго `"true"`/`"false"`, lowercase-only (Rust
`str::parse::<bool>`-канон; нет case-insensitive/`"1"`/`"0"`/`"yes"`-алиасов).

| From → To | Через | Failure |
|---|---|---|
| `str → bool` | `s.to_bool()` | пусто → `Err(Empty)`; что угодно кроме точно `"true"`/`"false"` → `Err(Invalid)` |

```nova
fn parse_flag(s str) -> Result[bool, ParseBoolError] => s.to_bool()

assert("true".to_bool() == Ok(true))
assert("TRUE".to_bool().is_err())      // регистр не lowercase → Err(Invalid)
```

`type ParseBoolError enum Empty | Invalid` (`std/runtime/string/parse.nv`)
— тот же двухвариантный паттерн, что `ParseFloatError`.

### numeric → str (format, infallible) — единый вход `.to_str()`

**Канон (Plan 174.2, 2026-07-14):** `str.from(scalar)` **ретрактирован**.
Единственный публичный вход «значение → строка» — bare-`T` blanket
`fn[T] T @to_str() -> str => "${@}"` ([D410](decisions/03-syntax.md#d410)
amend), специализируемый конкретными перегрузками там, где нужна другая
arity/семантика (например decode для `[]u8`, см. ниже).

| From → To | Через |
|---|---|
| `int/iN/uN → str` | `n.to_str()` |
| `f64/f32 → str` | `f.to_str()` |
| `bool → str` | `b.to_str()` |
| `char → str` | `c.to_str()` |

```nova
ro s = 42.to_str()             // "42"
ro f = 3.14.to_str()           // "3.14"
```

Интерполяция (`"${n}"`) лоуэрится в тот же путь напрямую (для примитивов —
в Display-хелпер C-уровня, без повторного вызова `.to_str()` — рекурсии
нет).

---

## Char / Byte / []byte / str

### char → str (UTF-8 encode)

| Через | Семантика |
|---|---|
| `c.to_str()` | infallible UTF-8 encode (1-4 байта) — специализация `to_str()`-blanket'а, byte-identical бывшему `str.from(char)` |

### str → char (single codepoint, fallible)

**Канон (Plan 232.1 Т1, owner decision «добавить», 2026-07-26):**
`s.to_char()` парсит РОВНО один Unicode codepoint (не байт — `"é".to_char()`
успешен, хотя `é` — 2 UTF-8 байта). Ресивер-форма на источнике, тот же
принцип, что `str @to_int()`.

| Через | Failure |
|---|---|
| `s.to_char() -> Result[char, ParseCharError]` | пусто → `Err(Empty)`; >1 codepoint → `Err(TooManyChars)` |

```nova
assert("a".to_char() == Ok('a'))
assert("ab".to_char() == Err(TooManyChars))    // строгий отказ, не first-char silently
```

`type ParseCharError enum Empty | TooManyChars` (`std/runtime/string/parse.nv`)
— **НЕ** переиспользует `CharFromError` (см. раздел «int → char» ниже): тот
домен — codepoint вне диапазона Unicode scalar value/surrogate, недостижим
для str→char (байты `str` уже валидный UTF-8, R-UTF8).

### int → char (codepoint range-check, fallible)

**Канон (владелец, 2026-07-09):** ресивер-форма на **источнике**
(`(cp int).to_char()`), не статик `char.try_from(n)` — тот же принцип
цепочечности, что у `str @to_int()`: `(32 + off).to_char()?`.

| Через | Failure |
|---|---|
| `(cp int).to_char() -> Result[char, CharFromError]` | `cp < 0` / `cp > 0x10FFFF` / surrogate `[0xD800, 0xDFFF]` |

```nova
fn describe(cp int) -> str =>
    match cp.to_char() {
        Ok(c)              => "codepoint ${cp} = '${c}'"
        Err(CharFromError) => "codepoint ${cp} вне диапазона"
    }
```

### char → byte (only if codepoint < 256, fallible)

Эта пара **осталась статик-формой** (не мигрировала на ресивер) — единственный
случай, где `try_` остался на целевом типе:

| Через | Failure |
|---|---|
| `u8.try_from(c char) -> Result[u8, TryFromCharError]` | codepoint > 0xFF (не Latin-1) |

**Исключение:** `'A' as byte`, `'A' as int`, `'A' as u8` — разрешены
для char-литералов (compile-time-known codepoint), см. D54.

### []byte ↔ str — единая `to_str`-семья (D325/174.1)

**Канон:** `[]u8`-decode тоже идёт через `to_str()` — конкретная
перегрузка (arity/семантика decode, не format) побеждает bare-`T` blanket
по правилу «конкретное побеждает generic» ([D84](decisions/10-overloading.md#d84)).
`str.try_from([]u8)` / отдельный `str.from_bytes(...)` — исторические
имена, **отозваны**, актуальны только формы ниже:

| Форма | Тип | Семантика |
|---|---|---|
| `bs.to_str()` | `-> Result[str, Utf8Error]` | checked decode; `Utf8Error{byte_offset}` указывает первый невалидный байт |
| `bs.to_str_lossy()` | `-> str` | infallible, невалидные последовательности заменяются replacement-символом |
| `unsafe { bs.to_str_unchecked() }` | `-> str` | без проверки, вызывающий гарантирует валидный UTF-8 |
| `unsafe { bs.consume.into_str_unchecked() }` | `-> str` | как выше, но потребляющий zero-copy move буфера |

```nova
fn decode(bytes []u8) -> str =>
    match bytes.to_str() {
        Ok(s)                        => s
        Err(Utf8Error{byte_offset})  => "invalid UTF-8 at ${byte_offset}"
    }
```

**str → []byte** (view, infallible, zero-copy) — голый вид, не
трансформация: `s.bytes() -> ro []u8` ([D410](decisions/03-syntax.md#d410) —
`as_bytes` переименован в `bytes`; это же имя — первая объявленная
`#coerce`-пара, см. раздел «Zero-cost неявные конверсии» ниже).

---

## Bool ↔ всё

| From → To | Через | Семантика |
|---|---|---|
| `bool → int` | `as` | `true=1`, `false=0` |
| `bool → byte` / `bool → f64` | `as` | то же |
| `bool → str` | `b.to_str()` | `"true"` / `"false"` |
| **`int/byte/f64/etc → bool`** | **запрещено** | use `n != 0` |

```nova
ro s = true.to_str()           // "true"
ro n = 5
ro ok = if n != 0 { true } else { false }   // explicit != 0, не truthy-int
```

str → bool — см. TODO выше (не найдено в std на момент этой ревизии).

---

## Newtype ↔ underlying

Newtype (`type X Y`, без `alias`, [D52](decisions/02-types.md#d52)) —
**отдельный** от источника тип; конверсия — явный `as` (identity, тот же
C-repr). Это отличается от `alias` (`type X alias Y`) — там `X` и `Y`
взаимозаменяемы **без всякого cast'а** (не отдельный тип).

| Через | Семантика |
|---|---|
| `n as MyNewtype` | identity (одинаковое C-представление) |
| `nt as int` | identity |

```nova
type UserId int
ro u UserId = 42 as UserId
ro n int = u as int            // 42
```

---

## Sum-variant ↔ int (discriminant)

Sum-тип требует маркер `enum` после имени ([D406](decisions/02-types.md#d406),
2026-07-01 — старый синтаксис с ведущим `|` без `enum` отменён):

```nova
type ErrorCode enum NotFound = 404 | InternalError = 500
ro code = NotFound as int      // 404
```

`int → Sum` через `as` **запрещён** (число может не попасть в варианты).
Используй pattern match.

---

## Strict if cond:bool / while cond:bool

`if cond`, `while cond`, `cond1 && cond2`, `cond1 || cond2` —
**cond обязан быть `bool`**. Truthy-int (`if a` где `a: int`)
запрещён.

```nova
ro n int = 5
if n { ... }                    // ❌ compile error
if n != 0 { ... }               // ✅
```

**Прецеденты:** Rust, Swift, Kotlin — все требуют bool. Python/C/JS —
truthy, известный bug-class.

---

## Zero-cost неявные конверсии — `#coerce` ([D429](decisions/02-types.md#d429), Plan 214/214.1)

Отдельно от явных механизмов выше — декларативный атрибут `#coerce` на
**унарной** функции объявляет **неявную** конверсию `I → O`, вставляемую
компилятором в позиции с известным ожидаемым типом (call-arg, `ro`/`mut`
с аннотацией, return, элемент коллекции) — БЕЗ явного вызова на месте:

Форма показана на свежем примере (`str @bytes()`/`StringBuilder @into_str()` —
уже объявленные в std пары, показывать их повторно здесь означало бы
конфликт деклараций):

```nova
type Meters { ro raw f64 }
type Boxed consume { ro payload int }

#coerce
fn Meters @value() -> ro f64 => @raw            // view — Meters → ro f64

#coerce
fn Boxed consume @unbox() -> int => @payload    // finalize — потребляющий move
```

Канон call-сайта — **голое значение**, не явный вызов. Реальная std-пара
`str @bytes() -> ro []u8` включается автоматически там, где позиция ждёт
`[]u8`, а на руках `str`:

```nova
import std.runtime.write_buffer.{WriteBuffer}

fn write_greeting(mut wb WriteBuffer, s str) -> () =>
    wb.write_bytes(s)   // s неявно .bytes() — не пишем это руками
```

Две «полосы», обе гарантированно zero-cost:
- **view** — не-`consume` метод с `ro`-возвратом (заём, без аллокации);
- **finalize** — `consume`-метод с владеющим возвратом (move, ресивер
  разряжается в точке вставки; use-after — обычная compile-error линейности).

Правила (см. D429 полностью): ровно одна декларация на пару `(I, O)`;
один уровень (цепочки НЕ разворачиваются, коэрсии не компонуются друг с
другом и с single-wrapper — конфликт = ошибка, не тихий выбор); exact-match
всегда побеждает коэрсию; `#coerce`-функция обязана быть без эффектов.
Первые декларации в std: `str @bytes() -> ro []u8`, `StringBuilder consume
@into_str() -> str`, `WriteBuffer consume @into_bytes() -> []u8`. Механизм
работает и для generic-образцов (`Json[T] @data() -> T`, снятие ограничения
Plan 214.1, 2026-07-24).

`as` **не** задействует `#coerce` (D429 R10) — `as` остаётся закрытым,
задокументированным в спеке множеством конверсий; `#coerce` — открытый
пользовательский реестр, смешение двух дало бы третью дверь к одной паре.

**Амендмент (№520, 2026-08-09):** finalize-полоса — потребление ВЕЗДЕ, не только
в явном вызове. `ro s str = sb` (аннотированный `let`), `Rec { s: sb }` (поле
record-литерала), `h.accept(sb)` (аргумент метода) и `[sb]` под annotated `let`
гасят обязательство `sb` ровно как явный `sb.into_str()` — использование `sb`
после ЛЮБОЙ из этих форм ловится тем же use-after-consume (D131), а тип с
`@cleanup` не получает повторный авто-вызов на выходе из scope. Возврат и
call-arg свободной функции работали так уже до амендмента; деталь — [D429
амендмент](decisions/02-types.md#d429).

---

## Именование `from`/`try_from` — конвенция, не протокол (⛔ ретракция 2026-07-06)

**До 2026-07-06** `From[T]`/`Into[U]`/`TryFrom[T,E]`/`TryInto[U,E]` были
generic-протоколами с авто-выводом обратной формы («4-way auto-derive»):
написал `T.from(v)` — компилятор сам синтезировал `v.into()`. **Решением
владельца эти четыре протокола упразднены целиком:**

1. В Rust conversion-bounds — костыль отсутствия перегрузок; в Nova
   перегрузки есть ([D84](decisions/10-overloading.md#d84)), `From`/`Into`
   как generic-bound в живом std не использовались НИ РАЗУ.
2. `?` не делает auto-`From`-конверсию ошибки ([D325](decisions/04-effects.md#d325):
   один `XError` на домен, конверсия — явный `.map_err(...)`).
3. Все реальные вызовы `.into()` в дереве играли роль «представление в
   строку» — это ось `to_str()`, а не передача владения.
4. Уходит компиляторная магия синтеза (§3 compiler-conventions):
   blanket identity `From`, auto-derive `From→Into`, 4-шаговый resolution.

**Что остаётся** (три независимых конвенции ИМЁН, каждая — обычная
Nova-функция без протокола за спиной):

- **(а) `.from(x)` / `.try_from(x)`** — конкретные статик-методы,
  конструктор-конверсия по конвенции имени (не generic-bound-able).
  `try_` — **только** когда есть infallible-сиблинг с тем же именем без
  префикса (R3, [D325](decisions/04-effects.md#d325)); одиночная
  фаллибельная операция без сиблинга — bare-имя без `try_` (пример —
  `s.to_int()`, не `s.try_int()`).
- **(б) `consume @into_ЦЕЛЬ()`** — конкретное имя для потребляющей
  передачи владения (`into_str`, `into_raw`, `into_bytes`,
  `into_str_unchecked`). Не общая операция `.into()` — генерик-версии
  больше нет, каждое имя объявляется на своём типе явно.
- **(в) `.to_str()` / семейство `to_*`** — представление и трансформация
  (см. [D410](decisions/03-syntax.md#d410)).

**Компилятор НИЧЕГО не синтезирует между этими тремя** — ни обратную
форму, ни цепочку. Если тип хочет оба направления — программист пишет оба
явно, разными именами.

```nova
type Celsius f64
type Fahrenheit f64

fn Fahrenheit.from(c Celsius) -> Self =>
    Self((c as f64) * 9.0 / 5.0 + 32.0)

// Компилятор НЕ синтезирует c.into() — Into больше нет. Если нужна
// обратная форма — пишем отдельную функцию явно:
fn Celsius.from(f Fahrenheit) -> Self =>
    Self(((f as f64) - 32.0) * 5.0 / 9.0)
```

Fallible-версия — то же самое, но статик возвращает `Result`:

```nova
fn Port.try_from(n u16) -> Result[Self, str] =>
    if n == 0 { Err("port 0 reserved") } else { Ok(Port(n)) }

ro p = Port.try_from(8080)?
```

---

## Прецеденты по языкам

| Язык | Где близок к Nova |
|---|---|
| Rust | `as` semantics, `from`/`try_from` naming, char::from_u32 |
| Swift | strict bool, no implicit coerce, Int(throwing:) |
| Kotlin | strict if-cond:bool, .toInt()/.toIntOrNull() |
| Go | `_ = strconv.ParseInt(s)` ≈ try_from |
| Python | `str(x)`/`int(s)` ≈ from/try_from но не type-safe |
| C/C++ | `(int)x` без проверок — UB-class, Nova не повторяет |

---

## Текущий статус (актуализировано после ревизии 2026-07-26)

Реализовано и стабильно:

- ✅ `as`-cast (numeric/newtype/sum), narrowing wraparound, float→int saturation
- ✅ `str @to_*` parse-семья (`to_int`/`to_i64`/`to_u64`/`to_i8`/`to_i16`/`to_i32`/
  `to_u8`/`to_u16`/`to_u32`/`to_f64`) — Plan 174.1, полный `SignedInts`/`UnsignedInts`-набор
- ✅ `str @to_bool()`/`str @to_char()` — Plan 232.1 Т1 (2026-07-26)
- ✅ bare-`T @to_str()` blanket + специализации (`char`, `[]u8`) — Plan 174.2
- ✅ `[]u8 @to_str()`/`@to_str_lossy()`/`@to_str_unchecked()`/`@into_str_unchecked()` — D325
- ✅ `(cp int).to_char()`, `u8.try_from(c char)` — D54/D77-naming
- ✅ Checked narrowing `@try_to_i8()`..`@try_to_uint()` — D430 (2026-07-20)
- ✅ `#coerce` (view/finalize) — D429/214.1, три std-пары + generic-образцы

Ретрактировано (не воскрешать без нового sign-off):

- ⛔ Протоколы `From`/`Into`/`TryFrom`/`TryInto` и их auto-derive синтез — 2026-07-06
- ⛔ `str.from(scalar)` static-конструктор — 2026-07-14 (заменён `.to_str()`)
- ⛔ `str.try_from([]u8)` / `str.from_bytes(...)` — заменены `[]u8 @to_str()`-семьёй
- ⛔ Методы `.unwrap()`/`.unwrap_or()`/`.unwrap_or_else()` на `Option`/`Result` — 2026-07-07
- ⛔ Старый sum-синтаксис без `enum`-маркера — D406 (2026-07-01)

---

## Ссылки

- [03-syntax.md → D54](decisions/03-syntax.md#d54) — `as` оператор
- [03-syntax.md → D44](decisions/03-syntax.md#d44) — числовые литералы
- [03-syntax.md → D410](decisions/03-syntax.md#d410) — `to_str`/`bytes`/`into_*`-семейство имён
- [02-types.md → D52](decisions/02-types.md#d52) — newtype/alias/sum-декларации
- [02-types.md → D406](decisions/02-types.md#d406) — `enum`-маркер sum-типа
- [02-types.md → D429](decisions/02-types.md#d429) — `#coerce` (zero-cost implicit view/finalize)
- [04-effects.md → D430](decisions/04-effects.md#d430) — checked narrowing `try_to_*`
- [04-effects.md → D325](decisions/04-effects.md#d325) — единый fallible-контракт std (Result-everywhere)
- [08-runtime.md → D73](decisions/08-runtime.md#d73) — `From`/`Into` (⛔ протокол ретрактирован 2026-07-06)
- [08-runtime.md → D77](decisions/08-runtime.md#d77) — `TryFrom`/`TryInto` (⛔ протокол ретрактирован 2026-07-06)
