# Types — record, sum-type, protocol, generic, поля

Решения этой группы задают систему типов Nova: четыре формы объявления
данных, структурные контракты-протоколы, семантику передачи параметров
и мутабельность полей, делегацию через `use`. Синтаксические детали
(методы через `@`, generic-применение `[T]`, литералы) — в
[03-syntax.md](03-syntax.md).

| # | Решение | Status |
|---|---|---|
| [D17](#d17-объявление-типов-единый-синтаксис-без-) | Объявление типов: единый синтаксис без `\|` | revised → D52 |
| [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-) | Объявление типов revised: newtype, `alias`, sum через leading `\|` | active |
| [D53](#d53-унификация-protocol-под-type-protocol-как-kind-токен) | Унификация: `protocol` под `type`, `protocol` как kind-токен | active |
| [D55](#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы) | Literal coercion в позиции с явным типом: sum-конструкторы и record-литералы | active |
| [D42](#d42-protocol-keyword-для-структурных-интерфейсов) | `protocol` keyword для структурных интерфейсов | revised → D53 |
| [D15](#d15-структурные-интерфейсы) | Структурные интерфейсы | revised → D42 → D53 |
| [D39](#d39-embed-и-delegation-use-name-type-alias-обязателен) | Embed и delegation: `use name Type` (alias обязателен) | active |
| [D32](#d32-семантика-передачи-параметров) | Семантика передачи параметров | revised для полей → D36 |
| [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindinga-readonly-для-never-mut) | Поля типа: дефолт mutable у mut-binding'а, `readonly` для never-mut | active |
| [D66](#d66-self-universal--ссылка-на-обобщающий-тип-в-методах-effects-protocols) | `Self` universal: ссылка на обобщающий тип в методах, effects, protocols | active |
| [D72](#d72-generic-bounds-через-t-protocol--protocol-как-тип) | Generic bounds через `[T Protocol]` — protocol как тип | active |
| [D110](#d110-ghost-state--spec-only-bindings) | Ghost state — spec-only bindings | active |
| [D122](#d122-hybrid-dispatch-для-bound-k-methods) | Hybrid dispatch для bound-K methods | active |
| [D123](#d123-tuple-monomorphization) | Tuple monomorphization | active |
| [D119](#d119-method-level-type-parameters-в-generic-methods) | Method-level type parameters в generic methods | active |

---

## D17. Объявление типов: единый синтаксис без `|`

> ⚠️ **REVISED.** Заменено [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-).
> Старый синтаксис (`type X = Y` для alias, `type X = A, B` для sum) —
> запрещён. Новый: `type X Y` (newtype), `type X alias Y` (alias),
> `type X | A | B` (sum). Текст ниже — для исторической справки.

### Что
Все формы объявления типа — record, позиционная структура, unit, alias,
sum-type — используют один разделитель списка (запятая) и
синхронизированы по `=`: `=` ставится **только** когда справа выражение
типа (alias или sum-type), не когда форма данных (`{...}` или `(...)`).

### Правило

Полный синтаксис:

```nova
// alias
type UserId = u64

// record (именованные поля)
type User { id u64, name str }

// позиционная структура
type Point(f64, f64)

// unit-тип (без полей)
type Empty

// sum-type
type Color = Red, Green, Blue

type Shape =
    Circle { radius f64 },
    Square { side f64 },
    Triangle { a f64, b f64, c f64 }

type Result[T, E] = Ok(T), Err(E)
```

Парсер однозначен по первому токену после имени типа:

| После `type X` идёт | Что это |
|---|---|
| `{ ... }` | record-структура |
| `( ... )` | позиционная структура |
| ничего | unit-тип |
| `=` потом тип | alias |
| `=` потом список вариантов через запятую | sum-type |

`type X { ... }` — это **record с полями**. Методы внутри `{...}`
запрещены: набор методов = поведение, для него используется `protocol`
([D42](#d42-protocol-keyword-для-структурных-интерфейсов)). Эффекты —
это `protocol`, использованный в позиции эффекта между `)` и `->`
([04-effects.md → D18](04-effects.md#d18-эффекты-объявляются-через-kind-токен-не-голый-type)).

Создание значений и pattern matching — обычные:

```nova
let p = Point(1.0, 2.0)
let u = User { id: 1, name: "alice" }
let c = Circle { radius: 5.0 }

match shape {
    Circle { radius }    => 3.14159 * radius * radius
    Square { side }      => side * side
    Triangle { a, b, c } => heron(a, b, c)
}
```

**Field punning** для record-литералов: если имя поля совпадает с
именем переменной в скоупе, можно писать имя один раз:

```nova
let key = "alice"
let value = 42

let entry = Entry { key, value }                    // shorthand
let entry = Entry { key, value, extra: "data" }     // можно смешивать
```

Парсер однозначен: `name:` → полная форма, `name,` или `name}` →
shorthand. Если переменной нет в scope — compile error.

**Partial pattern matching** — две эквивалентные формы:

```nova
// явная — с маркером ..
match @buckets[idx] {
    Occupied { value, .. } => Some(value)
    _                      => None
}

// неявная — без маркера, остальные поля игнорируются
match @buckets[idx] {
    Occupied { value } => Some(value)
    _                  => None
}
```

Явная форма — visual cue «здесь ещё поля». Неявная — краткость.

Переименование при деструктуризации остаётся явным: `Occupied { key: k, value }`.

Construction всегда требует все обязательные поля — частичное
заполнение типа Rust `..default` отдельным синтаксисом не зафиксировано.

### Почему

1. **Один разделитель списка на весь язык — запятая.** Параметры,
   элементы массивов, поля записи, варианты sum-type — везде `,`.
   Меньше правил, меньше ошибок LLM.
2. **`=` означает «справа выражение типа».** Когда справа форма данных
   — `=` лишний.
3. **Парсер по первому токену** — никакого backtracking, чистые
   сообщения об ошибках.

### Что отвергнуто

- **ML-style `| Variant`** (OCaml/Haskell/F#/Rust). Два разделителя
  подряд (`= |`), чужд языкам не из ML-семейства, дублирует роль
  запятой.
- **`type Point = | Point(f64, f64)`** для одно-вариантного sum-type —
  дубль. Sum-type с одним вариантом и структура — это одно и то же.
- **`type User = { id u64, name str }`** для record. `=` лишний, когда
  справа форма данных.

### Связь
- [03-syntax.md → D27](03-syntax.md#d27) — массивы (`[]T`, `[N]T`) как
  отдельные конструкции типов, не варианты `type`.
- [03-syntax.md → D38](03-syntax.md#d38) — generic-применение `Имя[T]`
  для параметризованных типов.
- [02-types.md → D42](#d42-protocol-keyword-для-структурных-интерфейсов)
  — почему `protocol` отдельный keyword, а не `type X = { методы }`.
- [02-types.md → D36](#d36-поля-типа-дефолт-mutable-у-mut-bindinga-readonly-для-never-mut)
  — префиксы полей (`readonly`, `mut`) и group-syntax внутри record.

---

## D52. Объявление типов revised: newtype, `alias`, sum через leading `|`

### Что
Полная пересборка [D17](#d17-объявление-типов-единый-синтаксис-без-).
**Один keyword `type` для всех data-форм**, никаких `=` в декларациях,
форма различается **первым токеном после имени**. Шесть форм:

- **newtype** — `type X Y` (X — новый тип, типизированно отличный от Y, Go-style)
- **alias** — `type X alias Y` (X и Y совместимы, для длинных дженериков)
- **record** — `type X { поля }`
- **tuple** — `type X(типы)`
- **unit** — `type X` (ничего после имени)
- **sum** — `type X | A | B | C` (leading `|` обязателен)

Sum-варианты могут иметь **числовые discriminants** с auto-increment.
`protocol` остаётся отдельным keyword'ом для поведения
([D42](#d42-protocol-keyword-для-структурных-интерфейсов)).

### Правило

#### Полный синтаксис

```nova
// 1. Newtype — type X Y, без =
type UserId u64
type Email str
type Score f64

// 2. Alias — type X alias Y, для сокращения длинных дженериков
type StringMap[V] alias HashMap[str, V]
type Cache[K, V] alias HashMap[K, (V, Time)]

// 3. Record — type X { поля }
type User { id u64, name str }
type Point3D { x, y, z f64 }                    // group-syntax (D36)
type Account {
    readonly id u64
    balance money
    mut last_access time
}

// 4. Tuple — type X(типы)
type Point(f32, f32)
type Pair[A, B](A, B)

// 5. Unit — type X
type Empty
type Sentinel

// 6. Sum — type X | A | B (leading | обязателен)
type Color | Red | Green | Blue
type Direction | North | East | South | West

// Sum многострочный
type Result[T, E]
    | Ok(T)
    | Err(E)

type Shape
    | Circle { radius f64 }
    | Square { side f64 }
    | Triangle { a f64, b f64, c f64 }
```

#### Парсер однозначен по первому токену после имени (с учётом дженериков)

| После `type X` (или `type X[params]`) идёт | Форма |
|---|---|
| `\|` | sum |
| `(` | tuple |
| `{` | record |
| `alias` | alias |
| `<base-type>` `\|` | sum с явным базовым типом для discriminants |
| идентификатор/тип, конец строки | newtype |
| конец строки сразу | unit |

Парсер видит первый токен — сразу знает форму. Никакого backtracking,
никакого lookahead за пределы одного-двух токенов.

#### Sum-варианты с числовыми discriminants

```nova
// Auto-increment без явных значений (от 0)
type ExitStatus | Ok | Failure | Critical                  // 0, 1, 2

// Auto-increment от заданного
type FileMode | Read = 1 | Write | Execute                 // 1, 2, 3

// Все явные
type ErrorCode
    | NotFound       = 404
    | Unauthorized   = 401
    | InternalError  = 500

// С отрицательными
type Sign | Negative = -1 | Zero = 0 | Positive = 1

// Decreasing/non-monotonic — разрешено
type Code | A = 10 | B = 5 | C                             // A=10, B=5, C=6

// Явный базовый тип
type Bit u8 | Off = 0 | On = 1
type HttpCode i32 | Ok = 200 | NotFound = 404
```

**Правила discriminants:**

1. **Базовый тип** — дефолт `int`. Опционально явный (`type X i32 |`,
   `type X u8 |`).
2. **Auto-increment** от первого варианта:
   - Первый без значения → 0.
   - Каждый следующий без значения → предыдущий + 1.
3. **Отрицательные значения** — разрешены.
4. **Decreasing/non-monotonic** последовательности — разрешены.
5. **Конфликт значений** (два варианта с одинаковым discriminant) —
   **запрещён** компилятором.
6. **Mixed** (некоторые с полями, некоторые без, у всех discriminants) —
   разрешено:
   ```nova
   type Event
       | Click(x int, y int)              = 1
       | KeyPress(key str)                 = 2
       | Idle                              = 3
       | Data { payload []u8, crc u32 } = 10
   ```

#### Cast между sum-типом и числом

**Sum → int** — безопасный, всегда работает:

```nova
let c = Red                 // Color
let n = c as int            // 0 (если auto-increment)

let e = NotFound            // ErrorCode
let n = e as i32            // 404
```

**int → Sum** — через **pattern match obligation**:

```nova
let n = read_from_db()
let c = match n {
    0 => Red
    1 => Green
    2 => Blue
    _ => throw InvalidColor
}
```

Никакого `n as Color` — программист сам обрабатывает «нет такого
варианта». Это согласовано с эффектом `Fail[E]`.

stdlib может предоставлять `Color.from_int(n)` для удобства:

```nova
fn Color.from_int(n int) Fail[InvalidVariant] -> Color =>
    match n {
        0 => Ok(Red)
        1 => Ok(Green)
        2 => Ok(Blue)
        _ => Err(InvalidVariant)
    }
```

#### Параметризованные sum

```nova
type Option[T] | Some(T) | None
type Result[T, E] | Ok(T) | Err(E)
type Tree[T]
    | Leaf
    | Node { value T, left Tree[T], right Tree[T] }
```

Параметры в `[...]` после имени работают везде, как и раньше.

#### Сравнение alias и newtype

```nova
type AliasUserId alias u64
type NewUserId u64

let a AliasUserId = 42        // ok
let b u64 = a                  // ok — alias совместим с u64
let c u64 = 42
let d AliasUserId = c          // ok — обратное тоже работает

let n NewUserId = 42           // ok (литерал подгоняется под целевой тип)
let e u64 = n                  // ОШИБКА: NewUserId не u64
let f u64 = n as u64           // ok через cast
```

**Альтернативу newtype через record-обёртку (`type X { value u64 }`)
никто не запрещает**, но `type X u64` — компактнее и привычнее
программистам с фоном Go.

#### Field punning — расширено и обязательно

D52 расширяет field punning из D17 двумя правилами:

**1. Shorthand для `@field`-доступов** (новое в D52):

```nova
type RangeIter { end int, inclusive bool, mut cur int }

fn Range @iter() -> RangeIter =>
    { @end, @inclusive, cur: @start }
//    ↑    ↑           ↑
//    @end shorthand   полная форма (имя поля cur ≠ start)
```

`{ @end }` означает «поле `end`, значение `@end` (то есть `self.end`)».
По симметрии с D17 (`{ name }` для переменной `name` в scope) —
теперь `{ @field }` для self-доступа.

**2. Shorthand обязателен, когда имя поля совпадает с источником:**

```nova
// Переменная в scope:
let key = "alice"
let value = 42
let entry = Entry { key, value }                  // ✓ обязательная форма
let entry = Entry { key: key, value: value }      // ✗ ОШИБКА: избыточная форма

// @field-доступ:
let r = { @end, @inclusive, cur: @start }         // ✓
let r = { end: @end, inclusive: @inclusive, ... } // ✗ ОШИБКА: избыточная

// Явная форма обязательна, когда имя источника отличается:
let entry = Entry { name: user_name }             // ✓ имя поля ≠ переменной
let r = { cur: @start }                            // ✓ имя поля cur ≠ start
let r = { end: other.end }                         // ✓ источник — выражение, не @field
```

**Парсер:** `{ name`/`{ @name`/`{ name,`/`{ name }` — shorthand;
`{ name: expr` — полная форма. После `:` ожидается выражение,
но если выражение — это **ровно тот же identifier или `@`+identifier**,
что и имя поля → ошибка компиляции «избыточная форма, используйте
shorthand».

**Status:** ✅ enforced (2026-05-17, commit 34666922c35). Реализация
в `compiler-codegen/src/types/mod.rs` RecordLit walker. AST flag
`RecordLitField.at_shorthand` различает parser-generated `@field`
shorthand от explicit `{ field: @field }` (одинаковая AST форма).
Test guards: `nova_tests/negative_capability/d52_redundant_field_literal_rejected.nv`
+ `d52_redundant_self_field_rejected.nv`.

**Mixed разрешён:**

```nova
{ @end, @inclusive, cur: @start, kind: "iter" }     // shorthand + полные
```

**Когда расширение работает:**

| Имя поля | Источник | Правило |
|---|---|---|
| `name` | переменная `name` в scope | shorthand `{ name }` обязателен |
| `name` | `@name` (self-поле) | shorthand `{ @name }` обязателен |
| `name` | переменная `other` (другое имя) | полная форма `{ name: other }` |
| `name` | `@other` или выражение | полная форма `{ name: @other }` |
| `name` | `obj.field` | полная форма `{ name: obj.field }` |
| `name` | литерал, вызов, любое выражение | полная форма |

#### Pattern matching и construction

```nova
match @buckets[idx] {
    Occupied { value, .. } => Some(value)            // partial с ..
    Occupied { value }     => Some(value)            // partial без ..
    _                      => None
}
```

**Construction всегда требует все обязательные поля.** Частичное
заполнение типа Rust `..default` отдельным синтаксисом не зафиксировано.

#### Что запрещено

- **`type X = Y`** для alias — старый D17 синтаксис, заменён на
  `type X alias Y`.
- **`type X = A, B`** для sum — заменён на `type X | A | B`.
- **`type X = { ... }`** для record — синтаксис никогда не был активным
  (D17 уже отвергал), `=` в этой позиции запрещён.
- **`,` для разделения вариантов sum** — заменено на leading `|`.
- **Sum без leading `|` у первого варианта** — обязателен (`type X
  Red | Green` ✗, `type X | Red | Green` ✓).
- **Single-variant sum** — запрещён (как в D17), используйте record.
- **Конфликт discriminants** — запрещён.
- **Избыточная форма `{ name: name }`** — обязателен shorthand
  `{ name }`. Аналогично `{ field: @field }` — обязателен `{ @field }`.
  Если имя источника совпадает с именем поля, программист **обязан**
  использовать shorthand. См. «Field punning» выше.

### Почему

1. **Системность.** В D17 правило «`=` для выражений типа, без `=` для
   форм данных» работало для alias, но **спотыкалось на sum-type**:
   `type Color = Red, Green, Blue` — справа не «выражение типа» в
   обычном смысле, а список конструкторов. С D52 sum обрабатывается
   как именованная форма (через `|`), как и record/tuple/unit.
2. **Никаких `=` в декларациях типов** — устраняется напряжение
   «иногда есть, иногда нет». `=` остаётся за binding'ом значений
   (`let x = ...`) и parameter defaults (если будут).
3. **Newtype как first-class.** Domain-modeling (`type Email str`,
   `type Score f64`) даёт реальную защиту типов без шумной
   record-обёртки. Прецедент Go (`type UserId int64`).
4. **Discriminants для wire-протоколов.** HTTP-коды, syscall-коды,
   serialization tags — программист может задать стабильные
   значения, как в C/TS/Swift enum.
5. **Парсер однозначен по первому токену** — никакого lookahead
   глубже одного-двух токенов. AI-friendly: LLM с одного взгляда
   понимает форму.
6. **Leading `|` для sum** — visual symmetry: все варианты
   выровнены, прецедент OCaml/F#/Scala 3.
7. **Согласованность с D1 «protocols + data, без классов»** — `type`
   только для данных, `protocol` отдельно для поведения.
8. **Field punning расширен и обязателен.** Один способ записать
   «поле = источник с тем же именем» — shorthand. Запрет избыточной
   формы `{ name: name }` устраняет «два пути к одному результату»,
   что AI-unfriendly (LLM генерирует случайно). Также покрывает
   `{ @field }` для self-доступов — частый паттерн в record-литералах
   методов-конструкторов. Прецедент: TS/Rust имеют shorthand, но не
   делают его обязательным; Nova идёт строже ради единого стиля
   (D40/D43-стилевая последовательность).

### Что отвергнуто

- **Сохранить `type X = Y` для alias.** Создаёт асимметрию: alias и
  sum с `=`, record/tuple/newtype без — нет единого правила.
- **Kind-токен `enum` для sum** (`type X enum { A, B }`). Длиннее,
  чем leading `|`, не даёт дополнительной информации.
- **Литералы как sum-варианты** (`type State | "open" | "closed"`,
  TS-style literal types). Полезно, но это **отдельная фича**
  (subtyping, runtime representation), отложена на следующую
  версию языка.
- **Итерация по вариантам** (`for c in Color`). Связано с
  reflection и stdlib, отложено до Q9.
- **`type X protocol { ... }`** под единым `type`. Семантически
  protocol — поведение, не данные; отдельный keyword чище.
- **`type X newtype Y`** с явным kind-токеном. `type X Y` без
  токена короче и согласовано с Go.
- **Implicit cast int → Sum.** Type-небезопасно (число может не
  попасть в варианты). Только через pattern match.

### Цена

1. **Большой breaking change.** Все существующие декларации в spec/,
   decisions/, examples/ переписать. Кода пока мало, миграция
   разовая.
2. **`alias` становится keyword'ом.** Раньше был обычным
   идентификатором.
3. **Программистам с фоном Rust/TypeScript:** `type X = Y` больше
   не alias, а ошибка. Адаптация через документацию.
4. **Парсинг `type X Y` (newtype) vs `type X` (unit)** — различие по
   следующему токену (тип vs конец строки). Просто, но требует
   внимательности.
5. **`|` имеет двойную роль** — разделитель в sum и `@or` в
   операторах ([D46](03-syntax.md#d46)). Парсер различает по
   контексту.

### Связь
- [D17](#d17-объявление-типов-единый-синтаксис-без-) — старая версия,
  помечена revised → D52.
- [D42](#d42-protocol-keyword-для-структурных-интерфейсов) —
  `protocol` остаётся отдельным keyword'ом для поведения.
- [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindinga-readonly-для-never-mut)
  — префиксы полей (`readonly`, `mut`) и group-syntax внутри record.
- [D39](#d39-embed-и-delegation-use-type-и-use-name-type) —
  delegation через `use Type`. Newtype с embed (`type X { use Y }`)
  — альтернатива alias для случаев, когда нужна обёртка с
  дополнительными полями.
- [03-syntax.md → D44](03-syntax.md#d44) — числовые литералы
  (`0xFF`, `1_000`, негативные) — используются для discriminants.
- [03-syntax.md → D46](03-syntax.md#d46) — `|` в operator
  overloading (`@or`) — разрешается компилятором по контексту.
  Полная семантика overloading — [D84](10-overloading.md#d84).

### Открытые вопросы
- **Литералы как sum-варианты** (TS-style `| "open" | "closed"`) —
  отложено до следующей версии.
- **Итерация по вариантам** (`for c in Color`, `Color.values()`) —
  связано с reflection, откладывается до Q9 (stdlib).
- **Implicit cast литерала в newtype.** Сейчас `let u UserId = 42`
  — допустим (литерал подгоняется), но `let n u64 = 42; let u
  UserId = n` — требует явного cast. Точную семантику зафиксировать
  в Q (литералы vs binding'и).

### Эволюция
[D17](#d17-объявление-типов-единый-синтаксис-без-) был первой
итерацией, основанной на правиле «`=` для выражений типа». Со
временем выяснилось, что:

1. Sum-type с `=` — натяжка («справа выражение типа» не точно
   описывает список вариантов).
2. Newtype отсутствовал как явная фича — программистам приходилось
   делать record-обёртки `type X { value u64 }`, что шумно.
3. Discriminants на sum-вариантах не были специфицированы — но
   реальные wire-протоколы их требуют.

D52 решает все три, ценой breaking change по syntax-site всех
type-объявлений. Подробно — [history/evolution.md](history/evolution.md).

---

## D53. Унификация: `protocol` под `type`, `protocol` как kind-токен

### Что
`protocol` перестаёт быть отдельным keyword'ом. Становится **kind-
токеном** в системе D52, наряду с `alias`. Все объявления типов
(включая структурные контракты-protocol'ы) идут через единый keyword
`type`. Анонимный protocol-тип в позиции параметра пишется через
`protocol { ... }` (с явным маркером, симметрично `[]T`, `(A, B)`,
`fn() -> T`).

`any` — пустой именованный protocol-тип в prelude:

```nova
type any protocol { }
```

### Правило

#### Объявление через `type X protocol { ... }`

```nova
// Раньше (D42): отдельный keyword
protocol Hashable {
    hash() -> u64
    eq(other Self) -> bool
}

// Теперь (D53): kind-токен в системе D52
type Hashable protocol {
    hash() -> u64
    eq(other Self) -> bool
}

type Logger effect {
    log(msg str) -> ()
}

type Iterator[T] protocol {
    next() -> Option[T]
}

type Db effect {
    query(q Sql) Fail[DbError] -> []DbRow
    exec(q Sql)  Fail[DbError] -> int
}
```

#### Парсер: `protocol` как kind-токен после имени

Расширение таблицы D52:

| После `type X` (или `type X[params]`) идёт | Форма |
|---|---|
| `protocol` | protocol-тип |
| `\|` | sum |
| `(` | tuple |
| `{` | record |
| `alias` | alias |
| `<base-type>` `\|` | sum с явным базовым типом |
| идентификатор/тип, конец строки | newtype |
| конец строки сразу | unit |

`protocol` встаёт в один ряд с `alias`. Парсер однозначен по первому
токену после имени (или generic-параметров).

#### Анонимный protocol-тип в позиции параметра

`protocol { ... }` в позиции типа — анонимный protocol-литерал,
симметрично `[]T`, `(A, B)`, `fn() -> T`:

```nova
fn log_one(x protocol { show() -> str }) Log -> () =>
    Log.info(x.show())

fn closer_call(c protocol { close() -> () }) Io -> () =>
    c.close()

fn process(x any) -> () =>      // any — именованный пустой protocol
    ...

fn process2(x protocol { }) -> () =>   // эквивалент через анонимный
    ...
```

Маркер `protocol` обязателен — `{ ... }` без префикса в позиции типа
запрещено. Это убирает двусмысленность с record-литералами и
выражениями-блоками.

#### `any` в prelude

```nova
// В prelude:
type any protocol { }
```

Любой тип удовлетворяет пустому контракту (структурная типизация),
поэтому `any` — top-type. Использование:

```nova
type Logger effect {
    log_event(level int, fields []any) -> ()
    //                          ^^^^^ массив значений любого типа
}

fn dump(x any) Io -> () =>
    println(x)
```

**Имя `any` lowercase** — исключение в [D30](03-syntax.md#d30) naming
convention, по аналогии с примитивами (`int`, `str`, `bool`, `f64`,
`()`). Top-type концептуально близок к примитивам — встроенный
универсальный тип.

#### Эффекты — без изменений

Эффект — это protocol-тип, использованный в позиции эффекта (между
`)` и `->`). Меняется только синтаксис **объявления**, не использования:

```nova
type Db effect {
    query(q Sql) Fail[DbError] -> []DbRow
    exec(q Sql)  Fail[DbError] -> int
}

fn list_users() Db -> []User =>      // Db в позиции эффекта — как раньше
    Db.query(sql`SELECT * FROM users`)
```

#### Generic-параметры — без изменений

[D42-уточнение](#d42-protocol-keyword-для-структурных-интерфейсов)
про две модели (на protocol-уровне и на методе) сохраняется. Меняется
только синтаксис объявления:

```nova
// Модель A — generic на protocol
type Container[T] protocol {
    add(item T) -> ()
    get(idx int) -> T
}

// Модель B — generic на методе
type Tracer effect {
    span[T](body fn() -> T) -> T
    measure[U](body fn() -> U) -> Duration
}
```

#### Структурная совместимость — без изменений

Любой тип со структурно совпадающими методами автоматически
удовлетворяет protocol'у:

```nova
type User { id u64, name str }

type Printable protocol {
    show() -> str
}

fn User @show() -> str => "User(${@name})"

fn log_one(x Printable) Log -> () =>
    Log.info(x.show())

log_one(my_user)                // ok, User совместим со Printable
```

`Self` внутри `protocol { ... }` блока — это «late-bound» тип,
определяется при удовлетворении (см. также [D66](#d66) — `Self`
universal во всех type-контекстах).

### Почему

1. **Унификация под одним keyword.** Все типы (data + behavior) идут
   через `type`. Один keyword для объявления, kind-токен различает
   форму. Согласовано с D52, который вводит `alias` как kind-токен —
   `protocol` встаёт в тот же ряд.
2. **Снимается асимметрия.** До D53: `protocol Foo` — отдельный
   keyword, но `Foo` использовался как тип (в позиции параметра).
   Программист спрашивал «если protocol — тип, почему не объявляется
   через type?». D53 отвечает: теперь объявляется.
3. **Анонимные protocol-типы становятся явными.** Раньше `fn f(x { ...
   })` без префикса — двусмысленно (record-литерал? record-тип?
   protocol-тип?). С `protocol { ... }` — намерение явно.
4. **`any` — пустой именованный protocol.** Простое и согласованное
   решение для top-type, через ту же систему. Прецедент Go (`type any
   = interface{}`), Swift (`protocol AnyObject { }`).
5. **Прецедент Go.** Go объявляет `type X struct { }` и `type X
   interface { }` через единый `type` с kind-токеном. D53 повторяет
   эту схему точно (только `interface` → `protocol`).
6. **AI-friendly.** Один keyword `type` в начале — LLM сразу видит
   «это объявление типа», kind показывает форму. Меньше keyword'ов
   для запоминания.

### Что отвергнуто

- **Сохранить `protocol Foo { ... }` как отдельный keyword** (текущий
  D42). Создаёт асимметрию: data объявляется через `type`, behavior —
  через `protocol`, оба используются как типы — два пути к одной
  концепции «тип». D53 устраняет.
- **`type any alias protocol { }` как форма для `any`.** Для protocol'ов
  alias-форма семантически тождественна newtype-форме (структурная
  типизация делает имена незначимыми). Дополнительный синтаксис без
  выигрыша. Прямая `type any protocol { }` короче и яснее.
- **`Any` (PascalCase).** Согласовано с D30 строже, но `any` lowercase
  привычнее (Go, TS) и согласовано с примитивами.
- **Анонимный protocol без префикса `{ ... }`.** Двусмысленно с
  record-литералами и блок-выражениями. `protocol { ... }` всегда
  явно.
- **Литеральные protocol'ы со значениями полей** (как `interface{}` в
  Go допускает методы и встраивание других interface'ов через
  composition). Composition protocol'ов (`Foo : Bar`) — открытый
  вопрос (см. D42 раздел «Открытые вопросы»), не входит в D53.

### Цена

1. **Большой breaking change.** Все `protocol Foo { ... }` в spec/,
   decisions/, examples/ переписать в `type Foo protocol { ... }`.
   Это — повторение масштаба D52 миграции.
2. **На одно слово длиннее.** `type Hashable protocol { ... }` против
   `protocol Hashable { ... }` — лишний `type ` (5 символов).
3. **`protocol` теперь kind-токен**, не keyword. Грамматически разные
   роли (kind-token ≠ leading keyword), хотя пишется одинаково.
4. **Анонимные protocol-типы в позиции параметра** — новая форма,
   старая (без префикса) запрещена. Все `fn f(x { method() })` →
   `fn f(x protocol { method() })`.
5. **Q22** закрывается этим решением — больше не открытый вопрос.

### Связь
- [D17](#d17-объявление-типов-единый-синтаксис-без-) — старая система
  объявлений, revised → D52.
- [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-)
  — D53 расширяет: `protocol` встаёт в ряд kind-токенов рядом с `alias`.
- [D42](#d42-protocol-keyword-для-структурных-интерфейсов) — D53
  заменяет `protocol` keyword на kind-токен. Семантика структурной
  типизации и generic-параметров сохраняется.
- [04-effects.md → D18](04-effects.md#d18) — эффект как использование
  protocol-типа в позиции эффекта. Меняется только объявление.
- [08-runtime.md → D26](08-runtime.md#d26) — `any` добавлен в prelude.
- [03-syntax.md → D30](03-syntax.md#d30) — naming: `any` lowercase
  как исключение, по аналогии с примитивами.

### Открытые вопросы
- **Type-pattern-match для значений `any`.** Извлечение конкретного
  типа из `any`-значения (`match x { int(n) => ..., str(s) => ... }`)
  требует runtime-tag и новой формы match. Не входит в D53.
- **Composition protocol'ов** (`Foo : Bar` или `Foo extends Bar`) —
  не входит, см. Q21 «proliferation эффектов» как родственный вопрос.

### Эволюция
[D42](#d42-protocol-keyword-для-структурных-интерфейсов) ввёл
`protocol` как отдельный keyword. После D52 (kind-токены `alias`)
выявилась асимметрия: protocol используется как тип, но объявляется
не через `type`. D53 снимает асимметрию — `protocol` становится
kind-токеном в системе D52, унифицируя объявление всех типов под
единым keyword'ом.

Q22 («унификация type/protocol») — закрыт принятием D53.

### Method-prefix в protocol-блоке (Plan 17 Ф.1)

В protocol-объявлении instance-методы можно писать в **обеих формах**
— и с префиксом `@`, и без. Они **эквивалентны**:

```nova
type Hashable protocol {
    hash() -> u64                    // ✅ голое имя
    eq(other Self) -> bool
}

type Hashable protocol {
    @hash() -> u64                   // ✅ с @, симметрия с реализацией
    @eq(other Self) -> bool
}
```

`@` факультативен потому что в protocol-блоке метод **всегда
instance** — без receiver-выражения, контекст однозначный. С `@`
форма читается как «копия декларации из реализации» (точно как `fn
User @hash() -> u64`); без `@` — короче. Структурная совместимость
работает одинаково.

**Когда писать что:**

- `@method()` — для **визуальной симметрии** с реализацией; для
  объявлений где соседние static-методы (если они появятся через
  Q-static-method-protocol) пишутся через `.method()`.
- `method()` — для **краткости** в простых protocol'ах.

**Mut-методы** — `mut @method()` обязательно с `@` (mut-modifier
требует receiver-маркера; голое `mut method()` отвергнуто как
двусмысленное с mut-binding'ом):

```nova
type Iter[T] protocol {
    mut @next() -> Option[T]         // ✅
    mut next() -> Option[T]          // ✅ (текущая prelude-форма, D26)
}
```

В bootstrap'е (2026-05-08) обе формы парсятся; std/testing/property.nv
и std/collections/* используют голую форму.

См. также [Q-protocol-method-prefix](../open-questions.md#q-protocol-method-prefix)
(closed этой секцией).

#### Реализация в bootstrap (2026-05-09)

Plan 15 D53 strict-mode (Plan 15 Ф.5) ввёл различие protocol/effect
на уровне AST. Раньше оба keyword'а маршрутизировались в один
`TypeDeclKind::Effect(Vec<EffectMethod>)`, что нарушало D72:
любой method-bag тип permissively принимался как generic-bound.

**Текущее состояние:**

- `TypeDeclKind::Protocol(Vec<EffectMethod>)` — для `type X protocol {…}`.
- `TypeDeclKind::Effect(Vec<EffectMethod>)` — для `type X effect {…}`.
- Парсер маршрутизирует по ключевому слову (отдельные match-arm).
- Codegen эмитит vtable **только** для Effect-kind. Protocol —
  compile-time-only; type_ref_to_c для protocol-методов не
  вызывается. Это попутно зафиксировало pre-existing bug: `Self` в
  protocol-методе раньше ломал codegen (искал несуществующий
  `Nova_Self*`).
- Type-checker (D72 enforcement) регистрирует **только**
  Protocol-kind в `protocol_specs`. Попытка использовать Effect
  как bound — compile error c hint'ом «`X` is an effect, not a
  protocol — declare as `type X protocol {…}`».
- Анонимные protocol-литералы в позиции типа (`fn close(c protocol {
  close() -> () })`, §628 этой секции) — ✅ **реализованы в Plan 97 Ф.2**
  через новый `TypeRef::Protocol(ProtocolSig)` variant.
- Protocol-литералы в expression-position (`let l = protocol Name { ops }`)
  с runtime vtable + dispatch — ✅ **реализованы в Plan 97.1**
  (codegen vtable struct + `emit_protocol_lit` + Plan 56 D122 box-pattern).
  См. также [D142](#d142).

---

## D55. Literal coercion в позиции с явным типом: sum-конструкторы и record-литералы

### Что
В позиции, где компилятор **явно знает целевой тип** `T` (let с
аннотацией, аргумент функции, return-выражение), литерал
автоматически подгоняется под `T`. Три случая:

1. **Sum-coercion.** Значение типа `S` оборачивается в **единственный**
   unary-конструктор `C(S)` sum-типа `T`.
2. **Record-coercion.** Анонимный record-литерал `{ field: value, ... }`
   получает тип `T` без необходимости писать имя типа перед `{}`.
3. **Map-coercion.** Анонимный record-литерал `{ name: value, ... }` в
   позиции, ожидающей str-keyed map (`HashMap[str, V]` — тип с
   compiler-recognized marker `FromFields[V]`), превращается в map:
   имена полей становятся **строковыми ключами**. Это **не**
   record-coercion (поля литерала ≠ поля struct'а `HashMap`) — отдельное
   правило, см. ниже.

Без runtime-cost, без subtyping. После coercion тип значения — сам `T`.

```nova
// Sum-coercion
type StrOrInt | S(str) | I(int)

let a StrOrInt = "test"          // компилятор: a = S("test")
let b StrOrInt = 25               // компилятор: b = I(25)

fn process(x StrOrInt) -> str => ...
process("alice")                   // компилятор: process(S("alice"))
process(42)                        // компилятор: process(I(42))

// Record-coercion
type User { id u64, name str }

let u User = { id: 2, name: "Bob" }    // компилятор: u = User { id: 2, name: "Bob" }

fn create_user() -> User =>
    { id: 3, name: "Carol" }            // компилятор подставляет User

fn save(u User) -> () => ...
save({ id: 4, name: "Dave" })           // компилятор: save(User { ... })
```

### Правило

#### Позиции с «явно ожидаемым типом»

Coercion (и sum-, и record-вариант) применяется только там, где
компилятор **точно знает** целевой тип:

| Позиция | Coercion применяется? | Реализовано (bootstrap)? |
|---|---|---|
| `let x T = value` (явная аннотация) | да | ✅ record (Plan 51 Ф.1) |
| `const X T = value` | да | ✅ record |
| `fn f() -> T => value` (return-выражение) | да | ✅ record |
| `fn f(x T)` — на caller-стороне (`f(value)`) | да | ✅ sum/record/map (Plan 52 Ф.3a) |
| Generic-параметр после конкретизации (`Maybe[int]`) | да | ⛔ ещё нет |
| Match-arm result (когда тип ветки фиксирован) | да | ⛔ ещё нет |
| Литерал коллекции с явным типом (`[]T`) | да для каждого элемента | ⛔ ещё нет |
| `let x = value` (без аннотации) | **нет** — выводится тип значения | — |

В позициях без явного типа никакая coercion не применяется — литерал
имеет «свой» тип (`{ id: 2 }` — анонимный record, `42` — int, и т.д.).

> **Статус реализации (2026-05-15).** В bootstrap-компиляторе
> sum-/record-/map-coercion для безымянного литерала реально работает в
> позициях, помеченных ✅ (включая **аргумент-позицию** после Plan 52
> Ф.3a — `f({...})`, `f([k:v])`, named-args). Для ⛔-позиций безымянный
> `{ ... }` пока даёт codegen-ошибку — там пиши `T { ... }`. Полная
> реализация D55 во всех позициях — отдельная задача (investigation в
> Plan 51 показал, что «~900 избыточных мест» — переоценка; основная
> масса — это перенос имени, а не устранение).
>
> ⚠️ **Пример `save_all([{id:1,name:"a"}, ...])` ниже некорректен для
> bootstrap'а.** Элемент-позиция литерала коллекции (`[]T`) помечена ⛔ —
> coercion на элементах массива пока не работает. Пример станет валиден
> после расширения Ф.3a на element-positions (за scope Plan 52). Пока
> там нужен `[User{...}, ...]` с явным именем типа на каждом элементе.

#### Запрет дублирования имени типа (Plan 51)

Там, где компилятор знает целевой тип, имя типа в record-литерале
**избыточно** и **запрещено** — тип объявляется ровно один раз.
Enforce'ится в двух позициях:

| Форма | Вердикт |
|---|---|
| `fn f() -> T => { ... }` | ✅ каноничная |
| `fn f() -> T => T { ... }` | ⛔ тип дважды |
| `fn f() => T { ... }` | ⛔ нет return-типа — тип «спрятан» в литерале |
| `let x T = { ... }` | ✅ каноничная |
| `let x = T { ... }` | ✅ (тип один раз — в литерале) |
| `let x T = T { ... }` | ⛔ тип дважды |

`-> Self` резолвится к типу receiver'а (`-> Self => Counter { ... }` в
методе `Counter` — тоже избыточно). Правило **не** срабатывает, когда
тип литерала ≠ целевой тип — это sum-coercion (`fn f() -> Result[U,E]
=> U { ... }`, `fn g() -> Shape => Circle { ... }`): имя варианта
обязательно. Применяется к `fn`, `@`-методам и closure-full с `=>`-телом.

#### Sum-coercion

В позиции с явным ожидаемым типом `T` (sum-тип) значение типа `S`
оборачивается, если:

1. У `T` **ровно один** unary-конструктор `C(S)`, принимающий тип `S`.
2. Значение точного типа `T` уже не подходит (нет exact match).

**Стандартные prelude-типы:**

```nova
let m Maybe[int] = 42                        // Just(42)
let r Result[User, str] = User { ... }       // Ok(User { ... })
let opt Option[str] = "alice"                // Some("alice")
```

**Коллекции:**

```nova
type SqlValue | I(i64) | F(f64) | S(str) | B(bool) | Bytes([]u8) | Null

let args []SqlValue = [42, "alice", true]    // [I(42), S("alice"), B(true)]

// В sql`...` тэге интерполяции тоже coerce'ятся: i64 → I, str → S, bool → B
let q = sql`SELECT * FROM users WHERE id = ${42}`   // args = [I(42)]
```

**Генерики:**

```nova
type Wrapper[T] | W(T) | Empty

let w Wrapper[int] = 42                      // W(42)
let w Wrapper[str] = "test"                   // W("test")
```

#### Record-coercion

В позиции с явным ожидаемым record-типом `T` анонимный record-литерал
`{ field: value, ... }` подгоняется под `T`. Имя типа перед `{}`
писать не нужно — компилятор подставляет.

```nova
type User { id u64, name str }

let u User = { id: 2, name: "Bob" }
// эквивалент:
let u User = User { id: 2, name: "Bob" }

fn save(u User) -> () => ...
save({ id: 4, name: "Dave" })             // эквивалент save(User { ... })

fn create() -> User =>
    { id: 5, name: "Eve" }                 // эквивалент User { id: 5, name: "Eve" }

fn make_default() -> Account =>
    { id: 1, balance: 0, closed: false }   // в return-позиции с типом Account
```

**Правила:**

1. **Все обязательные поля должны присутствовать** в литерале — как
   и для именованного record-литерала ([D17](#d17-объявление-типов-единый-синтаксис-без-)
   construction всегда требует все поля).
2. **Имена и типы полей** должны точно соответствовать `T`. Лишнее
   поле или несовпадение типа — ошибка компиляции.
3. **Field punning** ([D17](#d17-объявление-типов-единый-синтаксис-без-))
   работает: `let u User = { id, name }` если `id` и `name` —
   переменные в скоупе.
4. **Без явного целевого типа** литерал `{ id: 2, name: "Bob" }`
   остаётся анонимным record-значением. Тип параметра функции или
   аннотации `let` активирует coercion.

**Композиция с sum-coercion:**

```nova
let r Result[User, str] = { id: 2, name: "Bob" }
// шаг 1 (record-coercion): { id: 2, name: "Bob" } → User { id: 2, name: "Bob" }
// шаг 2 (sum-coercion): User → Ok(User { ... })
```

Записывается как одно действие компилятора в позиции с явным типом
`Result[User, str]`. Один-единственный record-литерал → User → Ok.

**Симметрия с массивами:**

То же type-driven поведение работает для массивов и других литералов в
позиции аргумента — это **та же модель**, которой Nova уже пользуется
для пустых массивов:

```nova
fn first[T](xs []T) -> Option[T] => ...
let r = first([])                   // [] : []T, T выводится из контекста

fn save(u User) -> () => ...
save({ id: 2, name: "Bob" })        // { ... } : User, тип параметра известен

fn save_all(us []User) -> () => ...
save_all([{ id: 1, name: "a" }, { id: 2, name: "b" }])
// каждый { ... } получает тип User из контекста []User
```

Аннотация типа параметра — единственный «локальный контекст», который
читается, и он рядом с вызовом.

**Sum-варианты с record-формой** не получают анонимной формы —
программист пишет конструктор:

```nova
type Shape | Circle { radius f64 } | Square { side f64 }

let s Shape = Circle { radius: 5.0 }   // явный конструктор обязателен
let s Shape = { radius: 5.0 }           // ОШИБКА: по полям невозможно
                                        // выбрать между Circle и Square
                                        // (даже если у них разные поля,
                                        // программист пишет имя варианта)
```

Это сознательное ограничение: sum-варианты с record-формой требуют
имени конструктора всегда. Иначе at parse-time нужно матчить
по структуре полей — type-driven parsing, антипаттерн.

#### Map-coercion

В позиции с явным ожидаемым типом `HashMap[str, V]` анонимный
record-литерал `{ name: value, ... }` превращается в str-keyed map:
**имена полей литерала становятся строковыми ключами**, значения —
значениями map.

```nova
let h HashMap[str, bool] = { debug: true, verbose: false }
// эквивалент: HashMap[str, bool] с ключами "debug", "verbose"

fn configure(opts HashMap[str, int]) -> () => ...
configure({ width: 80, height: 25 })          // ключи "width", "height"
```

**Почему отдельное правило, а не record-coercion.** `HashMap[K, V]` —
это struct (`type HashMap[K, V] { buckets, count, ... }`). Обычная
record-coercion матчила бы `{ debug: ... }` против **полей struct'а**
`HashMap` (`buckets`, `count`) и падала бы. Map-coercion трактует
имена полей литерала как **ключи**, а не как поля struct'а. Чтобы
компилятор знал, какое из двух правил применить, целевой тип несёт
**compiler-recognized marker** `FromFields[V]`:

- Это **не** opt-in ради эргономики (которое D55 отвергает для
  sum/record) — marker здесь **load-bearing для дисамбигуации**:
  «трактовать `{...}` как поля этого struct'а» vs «как строковые
  ключи». Без него правило неоднозначно.
- Gating: `HashMap[str, V]` несёт marker; случайный struct — нет, и
  не начнёт принимать произвольные record-литералы.
- Bootstrap: marker захардкожен для `HashMap`. Протокол `FromFields[V]`
  как точка расширения (`OrderedMap`, `BTreeMap[str, V]`) — позже.

**Правила:**

1. **Ключи** — только str (имена полей литерала). Нестроковые ключи,
   не-идентификаторные строки, вычисляемые ключи — это map-литерал
   `[k: v]` ([03-syntax.md → D108](03-syntax.md#d108)), не `{...}`.
2. **Значения гомогенны** — все поля одного типа `V` (после возможной
   sum-coercion на каждом значении).
3. **Композиция с sum-coercion:**
   ```nova
   let j HashMap[str, JsonValue] = { name: "alice", age: 30.0 }
   // "alice" → Str("alice"), 30.0 → Num(30.0); оба → JsonValue
   ```
4. **Десугаринг — без промежуточных объектов:** block-expression с
   `with_capacity` + `@insert`, никакой промежуточный record не
   материализуется (литерал — только синтаксис):
   ```nova
   { let mut _m0 = HashMap[str, V].with_capacity(n)
     let _ = _m0.@insert("debug", true)
     let _ = _m0.@insert("verbose", false)
     _m0 }
   ```
5. **Пустой `{}` — это НЕ пустая мапа.** `{}` всегда парсится как пустой
   block-expression с типом `unit` — даже в позиции, ожидающей
   `HashMap[str, V]`. Пустая мапа записывается как `[]` + ожидаемый тип
   ([03-syntax.md → D108](03-syntax.md#d108-map-литерал-k-v)):
   ```nova
   let h HashMap[str, bool] = []     // ✅ пустая мапа (тип из контекста)
   let h HashMap[str, bool] = {}     // ⛔ {} — пустой блок, тип unit ≠ HashMap
   ```
   > **Ревизия (Plan 52 Ф.0).** Прежняя формулировка §5 ошибочно
   > допускала `{}` в map-позиции → `HashMap[str, V].new()`. Это
   > требовало type-directed parsing блока — Nova этого не делает
   > ([D43](03-syntax.md#d43-trailing-block--без-params-fnp-body-с-params)).
   > Правило удалено; пустая мапа — только `[]`.
6. **Дубликаты ключей** невозможны — имена полей record-литерала
   уникальны by construction.

Граница с map-литералом `[k: v]`: `{...}` — когда ключи это
**статические имена-идентификаторы**; `[...]` — когда ключи это
**выражения** (см. D108).

#### Когда coercion НЕ применяется

**Ambiguity — несколько конструкторов с тем же типом** (sum-coercion):

```nova
type Ambiguous | A(int) | B(int)

let x Ambiguous = 42         // ОШИБКА: ambiguous, A(42) или B(42)?
let x = A(42)                 // явный конструктор — ok
```

**Несоответствие — ни один конструктор не принимает тип значения:**

```nova
type Color | Red | Green | Blue

let c Color = "red"           // ОШИБКА: ни один конструктор не принимает str
let c = Red                    // unit-конструктор
```

**Без аннотации — coercion отключён:**

```nova
type StrOrInt | S(str) | I(int)

let a = "test"                // a : str (не StrOrInt, аннотации нет)
let b StrOrInt = "test"        // b : StrOrInt = S("test") (аннотация есть)

let r = { id: 2, name: "Bob" }   // r : анонимный record { id int, name str }
let u User = { id: 2, name: "Bob" }   // u : User (через record-coercion)
```

**Newtype через D52 — coercion следует типу значения, не возможным кастам:**

```nova
type UserId u64
type Wrapper | W(UserId) | N(int)

let w Wrapper = 42            // 42 : int → N(42) (тип значения int)
let w Wrapper = 42 as UserId  // → W(42 as UserId) — явный as, потом coercion
let w Wrapper = UserId(42)    // явный конструктор UserId
```

**Несовпадение полей record:**

```nova
type User { id u64, name str }

let u User = { id: 2 }                    // ОШИБКА: missing field `name`
let u User = { id: 2, name: "Bob", age: 30 }   // ОШИБКА: unknown field `age`
let u User = { id: "two", name: "Bob" }   // ОШИБКА: id expects u64, got str
```

Coercion **не строит цепочку конверсий** — только одна обёртка вокруг
exact-type значения.

#### Multi-parameter и tuple-варианты

**Multi-parameter конструкторы — coercion не применяется в MVP:**

```nova
type Event | Click(int, int) | KeyPress(str)

let e Event = "enter"         // ok — KeyPress("enter"), unary с str
let e Event = (5, 10)          // ОШИБКА в MVP: tuple-coercion не вводится
let e = Click(5, 10)           // явный конструктор
```

Tuple-coercion `(5, 10) → Click(5, 10)` — отложено. Усложняет правила
(как различать «tuple как значение» vs «tuple-coercion в multi-param»),
не критично для use-case'ов.

#### Unit-конструкторы — coercion бессмыслен

Unit-варианты не принимают значение, coercion не нужен — программист
пишет конструктор напрямую:

```nova
type State | Open | Closed
let s State = Open              // unit, coercion не применяется
```

### Почему

1. **Огромный win в эргономике для prelude-типов.**
   `Option[T]` и `Result[T, E]` — самые частые sum'ы языка. Без coercion
   программист пишет `Some(42)`, `Ok(user)` каждый раз. С coercion —
   `42`, `user`. Убирает значительную часть boilerplate.
2. **Без subtyping.** Тип значения после coercion — **сам sum** или
   **сам record**, не подтип. На уровне типов всё чисто: pattern match
   exhaustive, variance не возникает. Anonymous unions (TS-style
   `string | number`) **не вводятся** — coercion не делает того же
   эффекта семантически.
3. **Без runtime-cost.** Sum-обёртка — обычный конструктор, runtime-tag
   уже есть в representation sum'а (D52). Record-coercion — это просто
   подстановка имени типа, никакого runtime-преобразования.
4. **Закрывает use-case'ы `any` (sum) и убирает шум именования
   (record).** `sql\`...${value}\`` теперь type-safe — `value`
   coerce'ится в `SqlValue` без `[]any` и без `is`-extract.
   `let u User = { id: 2, name: "Bob" }` — без повтора имени типа.
5. **AI-friendly.** LLM пишет `[42, "alice"]` для SQL-аргументов
   естественно, без думания о конструкторах. `{ id: 2, name: "Bob" }`
   в позиции с явным типом — естественный способ создать record.
   Имя типа из аннотации — единственный «локальный контекст», который
   нужно прочитать, и он уже рядом.
6. **Прецеденты:**
   - **Swift `ExpressibleByStringLiteral`/`ExpressibleByIntegerLiteral`** —
     opt-in protocol'ы для coercion. Nova делает это **автоматически**
     для unary-конструкторов sum'ов (без opt-in).
   - **Scala 3 `Conversion[A, B]`** — opt-in given-конверсии.
   - **TypeScript** — через subtyping для anonymous union, через
     structural typing для record (`const u: User = { id, name }`
     работает). Nova даёт похожую эргономику без subtyping.
   - **Rust struct expressions** требуют имени (`User { id, name }`) —
     прецедент против record-coercion. Nova выбирает TS-эргономику
     для record в позиции с явным типом, но **только** в этой позиции.

### Что отвергнуто

- **Subtyping (`int <: StrOrInt`)** — TS-style anonymous unions.
  Серьёзное расширение системы типов (variance, type inference,
  exhaustiveness), runtime-cost (boxing на каждой границе). Coercion
  даёт то же удобство **без** subtyping. Записан как
  Q-anonymous-union для возможного пересмотра.
- **Anonymous record-coercion вне позиций с явным типом.**
  `let x = { id: 2, name: "Bob" }` остаётся **анонимным record-типом**,
  не превращается в `User`. Только явный целевой тип активирует
  coercion. AI-locality сохраняется.
- **Record-coercion для sum-вариантов с record-формой**
  (`type Shape | Circle { radius f64 } | Square { side f64 }`,
  `let s Shape = { radius: 5.0 }`). Программист обязан писать имя
  варианта (`Circle { radius: 5.0 }`), даже если поля уникальны
  для одного варианта. Альтернатива — type-driven parsing по
  совпадению полей, антипаттерн в Nova.
- **Tuple-coercion** в MVP. Двусмысленность с tuple-литералами как
  значениями. Отложено до v1.0+.
- **Coercion на цепочках конверсий** (`int → UserId → Wrapper`).
  Только одна обёртка. Иначе правила усложняются, и легко получить
  неожиданный результат.
- **Coercion без явной аннотации типа** (`let x = "test"` →
  выводить `StrOrInt`?). Type inference не должен «угадывать» sum
  или record. Только явный target type активирует coercion.
- **Opt-in coercion через protocol** (Swift-style
  `ExpressibleBy*Literal`). Программист объявляет sum/record,
  **поведение работает автоматически** без дополнительного opt-in.
  Это менее гибко, но проще.
- **Coercion для multi-parameter конструкторов** через tuple
  (`(5, 10) → Click(5, 10)`). Отложено как tuple-coercion в MVP.

### Цена

1. **Implicit conversion — первая в Nova.** До D55 язык избегал
   неявного. Это **философский сдвиг**, обоснованный эргономикой
   prelude-типов и анонимных record. AI-friendly: LLM не должна
   угадывать конструктор или имя типа.
2. **Type-checker сложнее.** В позиции с явным типом нужно проверить
   exact match, потом coercion (sum или record). Стандартное
   расширение, но code path не нулевой.
3. **IDE-подсказки усложняются.** «Ожидается `StrOrInt`, передан
   `str` → coerce в `S`», «Ожидается `User`, передан анонимный record
   → подгонка под `User`» — IDE должна это показывать.
4. **Migration sum'а опасна:** добавление нового unary-конструктора
   с тем же типом параметра ломает существующий код (был exact match
   через coercion в `S(str)`, стал ambiguous из-за `S(str) | S2(str)`).
   Это **breaking change для sum'а** — программист должен учитывать.
5. **Migration record'а тоже:** добавление обязательного поля в record
   ломает все анонимные литералы без него. Это **известная
   проблема** record-типов вообще, не специфическая для D55.
6. **Закрывает большую часть use-case'ов `any`** — это плюс, но
   требует пересмотра примеров (`args []any` → `args []SqlValue`).
7. **Парсер — без type-driven decisions.** Coercion работает в
   позициях, где целевой тип **уже известен type-checker'у** —
   парсер по-прежнему чисто синтаксический. `{...}` парсится как
   record-литерал/block-выражение по обычным правилам D17/D49,
   а тип ему присваивает type-checker по аннотации.

### Связь
- [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-)
  — sum-типы и unary-конструкторы, на которых coercion работает.
- [D53](#d53-унификация-protocol-под-type-protocol-как-kind-токен)
  — `any` остаётся для подлинно открытых случаев (plugins, reflection),
  D55 закрывает большую часть use-case'ов через closed sum'ы.
- [03-syntax.md → D44](03-syntax.md#d44) — numeric literal coercion
  (`100` подгоняется под `u8`/`u32` в позиции типа) — D55 расширяет
  эту идею на sum'ы и record'ы.
- [03-syntax.md → D54](03-syntax.md#d54) — `as`/`is` остаются явными
  для конвертации/проверки. D55 не вводит implicit cast между
  обычными типами, только для sum-обёрток и record-литералов.
- [08-runtime.md → D26](08-runtime.md#d26) — `Option[T]`, `Result[T, E]`
  в prelude получают эргономичный синтаксис через D55.
- [#d17-объявление-типов-единый-синтаксис-без-](#d17-объявление-типов-единый-синтаксис-без-)
  (revised → D52) — record-литерал `User { id: 1, name: "alice" }` с
  именем типа — обязательный, когда тип не выводится из контекста.
  D55 разрешает опускать имя в позиции с явным целевым типом.
- [03-syntax.md → D108](03-syntax.md#d108) — map-литерал `[k: v]`;
  комплементарен map-coercion (`{...}` — ключи-имена, `[...]` —
  ключи-выражения). Реализация обоих — [Plan 52](../../docs/plans/52-hashmap-literals.md).

### Открытые вопросы
- **Tuple-coercion** для multi-parameter конструкторов. Отложено.
- **Anonymous unions** (`type StrOrInt | type str | type int`) —
  TS-style без обёрток. Записан как Q-anonymous-union (требует
  subtyping, серьёзное расширение системы типов). См.
  [open-questions.md](../open-questions.md).
- **Стандартные closed sum'ы в prelude** (`SqlValue`, `JsonValue`) —
  что именно положить, формат и набор операций. См. Q9 (stdlib).
- **Cross-type numeric coercion в D55** (`42` → `f64` для `Number(f64)`).
  Сейчас строгий exact match. См. Q-numeric-coercion.

### Style-guide: когда coerce, когда писать тип явно (Plan 17 Ф.1)

D55 разрешает обе формы — coerce и явный конструктор. Чтобы кодовая
база не превращалась в смесь стилей, ниже **рекомендации** для `nova
fmt`/линтера и code review (это **не правило компилятора**, оба
варианта остаются валидными).

**Coerce (короче, тип в аннотации) — предпочитать когда:**

```nova
// 1. let с явной аннотацией — тип сразу слева, имя справа лишнее
let u User = { id: 1, name: "alice" }                ✅
let m Maybe[int] = 42                                 ✅

// 2. return-position в expression-body, есть -> T
fn make_default() -> Account => { id: 0, balance: 0 } ✅

// 3. call-site с явным типом параметра — coercion даёт чистый литерал
serve({ ...SERVER_DEFAULTS, port: 9000 })             ✅

// 4. коллекции с разнородными элементами в позиции []SqlValue
let args []SqlValue = [42, "alice", true]             ✅
//                    [I(42), S("alice"), B(true)]    ❌ шумно
```

**Явный конструктор — предпочитать когда:**

```nova
// 1. let без аннотации — coercion не работает, имя обязательно
let r = if cond { Some(value) } else { None }         ✅
let r = if cond { value } else { None }               ❌ — нет аннотации

// 2. match-arms где хотя бы одна ветка — unit-вариант (None / Empty)
//    — для визуальной симметрии писать ВСЕ ветки с конструкторами
match @cache.get(key) {
    Some(v) => Some(v)            ✅ симметрично с None
    None    => fallback()
}
match @cache.get(key) {
    Some(v) => v                  ❌ value слева, None справа —
    None    => fallback()         //    асимметрично, читать сложнее
}

// 3. nested record-литерал внутри блока — { {...} } визуально шумно
fn compute() -> Money =>
    if special { Money { amount: 100, currency: usd } }   ✅
    else       { Money { amount: a + b, currency: c } }
fn compute() -> Money =>
    if special { { amount: 100, currency: usd } }          ❌ шум
    else       { { amount: a + b, currency: c } }

// 4. ambiguous unary-конструкторы (compile-error без явного имени)
type Mixed | A(int) | B(int)
let x Mixed = 42                  ❌ ambiguous — обязателен A(42) / B(42)
```

**Сводка:**

| Контекст | Рекомендация |
|---|---|
| `let x T = ...` (есть аннотация) | coerce |
| `let x = ...` (нет аннотации) | явный конструктор |
| `fn f() -> T => ...` (есть `-> T`) | coerce |
| `fn f(x T)` call-site `f(...)` | coerce |
| match с unit-веткой | явный (симметрия) |
| nested `{ ... }` в блоке после `if`/`else` | явный (избежать `{ {...} }`) |
| ambiguous unary-конструкторы | явный (обязательно) |

**Аргумент.** `nova fmt` не должен переписывать одну форму в другую —
выбор стилистический. Линтер может в будущем выдавать **подсказку**
для самых тяжёлых случаев (например, `{ {...} }` в block-context),
но без флага `--strict-style` — это рекомендация, не ошибка.

См. также [Q-style-coercion](../open-questions.md#q-style-coercion)
(закрыт этой секцией).

### Эволюция
До D55 sum-варианты требовали **явный конструктор** на каждом значении
(`Some(42)`, `Ok(user)`, `S("test")`), а record-литералы — **имя типа
перед `{}`** (`User { id: 1, name: "alice" }`).

После D55 в позиции с явным целевым типом:
- sum-значение оборачивается автоматически (`42` в позиции `Maybe[int]`
  → `Just(42)`),
- анонимный record-литерал получает имя из аннотации (`{ id: 1, name:
  "alice" }` в позиции `User` → `User { id: 1, name: "alice" }`).

Это **эргономический сдвиг** уровня D52, без слома типовой модели.

Альтернатива (anonymous unions через subtyping) рассмотрена и
отвергнута — слишком серьёзное расширение системы типов для
эргономического выигрыша. D55 даёт похожее удобство более узким и
контролируемым механизмом.

---

## D42. `protocol` keyword для структурных интерфейсов

> ⚠️ **REVISED.** Заменено [D53](#d53-унификация-protocol-под-type-protocol-как-kind-токен).
> `protocol` — теперь не отдельный keyword, а **kind-токен** в системе
> [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-):
> `type Foo protocol { ... }`. Семантика структурной типизации,
> generic-параметров и эффектов сохраняется. Текст ниже — для
> исторической справки.

### Что
Структурные интерфейсы объявляются отдельным keyword `protocol`. `type`
— для **данных** (record, sum-type, alias), `protocol` — для
**поведения** (набор методов как контракт). Любой тип со структурно
совпадающими сигнатурами автоматически удовлетворяет protocol'у — без
явных `impl`-блоков.

**Эффекты — это тоже `protocol`**, использованный в позиции эффекта
(между `)` и `->`). Один и тот же `protocol` может играть роль эффекта
или роль структурного контракта-параметра — различение по контексту
использования ([04-effects.md → D18](04-effects.md#d18-эффекты-объявляются-через-kind-токен-не-голый-type)).
`type` без полей с одними методами не допускается — нужен `protocol`.

### Правило

```nova
type Hashable protocol {        // D52/D53: kind-токен `protocol` под `type`
    hash() -> u64
    eq(other Self) -> bool
}

type Iterator[T] protocol {
    next() -> Option[T]
}

type Login {                    // record (данные) — голый type
    username str
    password str
}
```

`Self` внутри protocol-блока — late-bound. См. [D66](#d66) для других
контекстов где `Self` тоже валиден (static/instance методы, effects).

Структурная совместимость — автоматическая. Метод определяется у типа
через `@`-синтаксис ([03-syntax.md → D35](03-syntax.md#d35)) и без
дополнительных деклараций удовлетворяет protocol'у:

```nova
type User { id u64, name str }

type Printable protocol {
    show() -> str
}

fn User @show() -> str => "User(${@name})"

fn log_one(x Printable) Log -> () =>
    Log.info(x.show())

log_one(my_user)                // ok, User автоматически совместим
```

Параметр функции может декларировать требования прямо в типе, без
именованного protocol'а:

```nova
fn log_one(x { show() -> str }) Log -> () =>
    Log.info(x.show())
```

В `protocol` `fn`-префикс не нужен — там по определению все «члены»
это методы. В record-типе поле-функция объявляется явно с `fn`:

```nova
type Button {
    text str
    on_click fn() Io -> ()      // поле-функция в record, не protocol
}
```

#### Generic-параметры: на protocol-уровне vs на методе

В Nova есть **две явных модели** generic-параметров для protocol'а.
Программист выбирает по семантике.

**Модель A — generic на protocol** (`protocol P[T] { ... }`).
T фиксирован для всего protocol'а: один handler = один T. Все методы
видят один и тот же T. Разные T = разные сущности (`Iterator[Int]` и
`Iterator[String]` несовместимы).

```nova
type Iterator[T] protocol {
    next() -> Option[T]
    peek() -> Option[T]
}

type Container[T] protocol {
    add(item T) -> ()
    get(idx int) -> T
    size() -> int                    // методы без T тоже допустимы
}

type Channel[T] effect {            // effect — нужен with-substitution
    send(value T) -> ()
    recv() -> T
}

type Cache[K, V] effect {
    get(key K) -> Option[V]
    set(key K, value V) -> ()
}
```

Когда применять: когда T — фундаментальная характеристика protocol'а,
все или большинство методов работают с этим T, и **разные T = разные
handler'ы** имеют смысл.

**Модель B — generic на методе** (`method[T](...)`).
T живёт только в скоупе одного метода. Один и тот же handler protocol'а
вызывает метод с разными T для каждого вызова.

```nova
type Tracer effect {
    span[T](body fn() -> T) -> T          // T живёт только здесь
    measure[U](body fn() -> U) -> Duration  // U независим от T
    set_attr(key str, value Json) -> ()    // методы без generic тоже
}

type Db effect {
    query(q Sql) Fail[DbError] -> []DbRow
    in_transaction[T](body fn() Db Fail -> T) Fail -> T
    // ↑ один Db handler оборачивает любой T
}
```

Когда применять: когда метод принимает/возвращает любой тип, а **сам
protocol не привязан** к этому типу — один handler работает с любым
T для каждого вызова.

**Различие в семантике handler'а:**

| | Модель A | Модель B |
|---|---|---|
| Объявление T | `protocol P[T]` | `method[T]` в сигнатуре |
| Scope T | весь protocol | один метод |
| Один handler работает с | одним T | любым T (per-call) |
| Использование | `with P[Int] = ...` | `with P = ...; P.method[Int](...)` |
| Реализация | мономорфизация по T | rank-2 polymorphism в handler'е |

В одном protocol'е можно комбинировать оба механизма:

```nova
type Stream[T] protocol {
    next() -> Option[T]                       // T на protocol-уровне
    fold[Acc](init Acc, f fn(Acc, T) -> Acc) -> Acc   // Acc на методе
}
```

`T` фиксирован для stream (`Stream[int]`), `Acc` независим — fold
может собирать в разные accumulator-типы из одного и того же stream'а.

### Почему

1. **Намерение должно быть явным.** Старая форма `type X = { методы }`
   визуально совпадала с record-формой `type X { поля }`, различаясь
   только знаком `=`. LLM и человек различали намерение по
   единственному символу — хрупко. Отдельный keyword делает намерение
   явным с первого токена.
2. **Прецедент.** `protocol` как keyword для интерфейсов используется
   в Swift, Objective-C, Clojure, Elixir, Python (`typing.Protocol`).
   Семантически Nova ближе всего к Python `typing.Protocol` — чисто
   структурный subtyping.
3. **Эффекты в сигнатурах методов** делают protocol строже Go
   interface — реализация не может привнести эффект сверх
   объявленного. Это уникальное свойство Nova.

### Что отвергнуто

- **`type X = { методы }`** — слишком похоже на record, отличается
  одним знаком `=`. См. «Почему» выше.
- **`contract`** — занято под pre/post-условия ([09-tooling.md → D24](09-tooling.md#d24)).
- **`promise`** — массовая ассоциация с async (JS Promise).
- **`interface`** — слишком сильный nominal-bias (Java/C#).
- **`trait`** — обещает Rust-фичи (default impl, supertraits, blanket
  impl), которых в Nova нет.
- **`shape`** — короче, но менее знакомо как keyword.
- **`ability`** — образно, но без знакомства; навязывает `-able`
  суффикс именам.
- **Implicit shared scope для generic-параметров** (T в нескольких
  методах одного protocol'а автоматически означает один и тот же тип).
  Снижает локальность: чтобы понять `[T]` в одном методе, нужно
  прочитать весь protocol-блок и проверить остальные методы. Невозможно
  выразить «независимый T в разных методах» без смены convention
  (использования других букв). Прецедентов нет — Rust/Swift/Scala/Haskell
  все используют либо явный protocol-уровень, либо явный method-уровень.
  Альтернатива (`protocol P[T]`) уже даёт ту же семантику явно.

### Связь
- [02-types.md → D15](#d15-структурные-интерфейсы) — D15 ввёл
  структурные интерфейсы; D42 уточняет грамматику отдельным keyword.
- [02-types.md → D39](#d39-embed-и-delegation-use-type-и-use-name-type)
  — `use Type` для делегации между record-типами; `protocol` не
  embed'ится.
- [03-syntax.md → D35](03-syntax.md#d35) — методы через `@` как
  способ удовлетворить protocol.
- [01-philosophy.md → D1](01-philosophy.md#d1-парадигма-protocols--data-без-классов)
  — `protocols` + `data` как фундамент парадигмы.

### Открытые вопросы

- **Bounds на дженерики** — `HashMap[K: Hashable, V]` требует отдельного
  решения. Сейчас параметр без bound, компилятор полагается на
  структурное соответствие при использовании.
- **Default-методы в protocol** — пока запрещены.
- **Inheritance protocol'ов** — `protocol A : B` пока запрещено;
  эквивалент достигается явным включением методов `B` в `A`.

### Эволюция
Изначально структурные интерфейсы описывались через `type X = { методы }`
(см. [D15](#d15-структурные-интерфейсы)). D42 заменил эту форму на
отдельный keyword `protocol`. Детали — в `history/evolution.md`.

---

## D15. Структурные интерфейсы

> Status: revised. Роль перешла к `protocol` keyword
> ([D42](#d42-protocol-keyword-для-структурных-интерфейсов)).

### Что
Изначальный механизм структурных «интерфейсов» в Nova: отдельной
концепции `interface` или `trait` нет; контракт — это набор сигнатур,
любой тип со совпадающими методами автоматически совместим. Сейчас
этот механизм обогащён keyword `protocol` (D42), который делает
объявление контракта синтаксически явным.

### Правило

Структурная совместимость — автоматическая. Имя контракту даёт
`protocol`:

```nova
type Printable protocol {
    show() -> str
}

type User { id u64, name str }

fn User @show() -> str => "User(${@name})"

fn log_one(x Printable) Log -> () => Log.info(x.show())

log_one(my_user)                // ok, User автоматически совместим
```

Анонимный структурный тип прямо в сигнатуре параметра — без отдельного
имени:

```nova
fn log_one(x { show() -> str }) Log -> () =>
    Log.info(x.show())
```

**Что сохранено:**
- **Эффекты в полях-функциях** — часть сигнатуры, проверяются как
  обычно. Реализация не может привнести эффект сверх объявленного. Это
  ключевое отличие Nova от Go: контракт жёстче, потому что эффекты —
  часть сигнатуры.
- **Структурная совместимость** автоматическая, как в Go.
- **Дженерики** без bound'ов — требования описываются типом параметра.

### Почему

1. Следует из принципа «не добавлять фичи без оправдания центральной
   идеей или AI-first». Rust-style traits ни тому, ни другому не
   служат.
2. Унификация: одна концепция «структурный тип» вместо двух («record»
   + «interface»). Меньше синтаксиса — проще для LLM.
3. Эффекты в сигнатурах методов делают структурный тип строже, чем
   Go interface — это уникальное свойство Nova, которое нельзя
   получить простым заимствованием Go.

### Что отвергнуто

- **`trait` / `interface`** как отдельный keyword с nominal-семантикой
  (Java/C#/Rust).
- **`impl Trait for Type`** блоки.
- **`[T: Trait]`** bounds в дженериках.
- **`dyn Trait` vs `impl Trait`** разделение.
- **Ассоциированные типы.**
- **Дефолтные методы.**
- **Trait-наследование, specialization, HKT.**

### Цена

- **Нет имени для контракта** иначе как через `protocol`. В IDE нельзя
  «найти всех, кто реализует X» так же легко, как в Rust/Java —
  поиск идёт по совпадению методов.
- **Нет номинальности.** Если очень нужна — через newtype-обёртку
  (паттерн, не фича).

### Связь
- [02-types.md → D42](#d42-protocol-keyword-для-структурных-интерфейсов)
  — `protocol` как явное имя для контракта.
- [02-types.md → D39](#d39-embed-и-delegation-use-type-и-use-name-type)
  — embed/delegation как механизм композиции, не subtyping.
- [03-syntax.md → D35](03-syntax.md#d35) — `@`-методы как способ
  удовлетворить protocol.

### Эволюция
Ранние черновики описывали контракт через `type X = { методы }` —
визуально неотличимо от record. D42 ввёл отдельный keyword `protocol`,
сохранив структурную семантику D15. Подробно — в
`history/evolution.md`.

---

## D39. Embed и delegation: `use name Type` (alias обязателен)

### Что
Композиция типов через `use name Type` внутри record-декларации. Имя
поля **всегда явное** — программист пишет alias в snake_case по
[D30](03-syntax.md#d30). Default-имя по типу (Go-style `use Type` →
поле `Type`) **не вводится** — нарушает D30 (поля snake_case, типы
PascalCase).

Это **delegation**, не наследование: обёртка не является подтипом
встроенного.

### Правило

#### Базовое использование

```nova
type AuditedAccount {
    use account Account              // имя поля = "account" (snake_case)
    audit_log []AuditEntry
}

let acc AuditedAccount = ...

// Auto-proxy: прямой доступ к полям и методам Account
println(acc.balance)                 // = acc.account.balance
println(acc.owner)                   // = acc.account.owner
acc.is_solvent()                     // = acc.account.is_solvent()

// Доступ к встроенному объекту целиком — через имя поля
let just_account = acc.account
```

`use Account` без имени — **ошибка компиляции**: имя поля обязательно.

```nova
type AuditedAccount {
    use Account                      // ОШИБКА: имя поля обязательно
    audit_log []AuditEntry
}
```

#### Auto-generated прокси-методы

При `use name Type` компилятор генерирует прокси для каждого метода
`Type`:

```nova
type Account { balance money }
fn Account @balance_pct(of money) -> f64 => @balance / of * 100.0

type AuditedAccount {
    use account Account
    audit_log []AuditEntry
}

// Компилятор генерирует:
// fn AuditedAccount @balance_pct(of money) -> f64 =>
//     @account.balance_pct(of)

let aa AuditedAccount = ...
aa.balance_pct(1000.0)               // через auto-proxy
```

Zero-cost — компилятор инлайнит вызов, никакой vtable.

#### Грамматика согласована с record-полями

`use name Type` использует тот же порядок «имя тип», что и обычные
поля, параметры функций, let-bindings, for-loop:

```nova
type Wrapper {
    item       str                   // обычное поле: имя тип
    use iter   HashMapIter[K, V]     // embed: use + имя тип
    extra      int
}

fn deposit(mut acc Account) -> () => ...   // параметр: имя тип
let user User = ...                           // let: имя тип
for id u64 in ids { ... }                     // for: имя тип
```

Везде имя слева, тип справа — одно правило для всего языка.

#### `use` — keyword, не имя поля

`use` — зарезервированное слово ([D29](07-modules.md#d29) для импортов
+ embed-конструкция здесь). **Имя поля `use` запрещено.**

В декларации `{use name Type}` `use` — keyword embed-формы; **имя
поля — alias после `use`**:

```nova
type Set[T] {
    use map HashMap[T, ()]           // имя поля — "map"
}

// record-литерал — имя поля
let s Set[int] = { map: HashMap[int, ()].new() }      // ✓
let s Set[int] = { use: HashMap[int, ()].new() }       // ✗ use — keyword

// доступ — имя поля
fn Set[T] @len() => @map.len()                          // ✓
fn Set[T] @len() => @use.len()                          // ✗ use — keyword
```

#### Override метода

Если тип-обёртка определяет метод с тем же именем — он затмевает
делегированный:

```nova
type AuditedAccount {
    use account Account
    audit_log []AuditEntry
}

fn AuditedAccount mut @deposit(amount money) {
    @account.deposit(amount)         // явный вызов «родителя» через имя поля
    @audit_log.push(AuditEntry.deposit(amount))
}

let mut acc AuditedAccount = ...
acc.deposit(100)                     // вызовет AuditedAccount.deposit
```

Без `@account.` в теле — бесконечная рекурсия. Программист обязан
явно обращаться к встроенному через имя поля.

#### Конфликт имён — разные alias-имена

Если два `use` вводят одинаковые имена методов — программист даёт
разные alias-имена и явно решает, через какой:

```nova
type Logger effect { log(msg str) -> () }
type Auditor { log(msg str) -> () }

type Combined {
    use console Logger
    use audit Auditor
}

let c = Combined { ... }
c.log("...")                         // ОШИБКА: ambiguous (оба имеют log)
```

Решение — явный вызов через имя поля:

```nova
fn Combined @log_all(msg str) {
    @console.log(msg)
    @audit.log(msg)
}

let c = Combined { ... }
c.console.log("...")
c.audit.log("...")
```

#### Anonymous embed: `use _ Type` (без alias-имени)

Альтернатива явному alias — **anonymous embed** через `_`:

```nova
type Set[T] {
    use _ HashMap[T, ()]
}

let s = Set[int].new()
s.insert(item, ())          // ✓ через auto-proxy на HashMap.insert
s.contains(item)            // ✓ через auto-proxy
s.len()                     // ✓ через auto-proxy (D117 method-only)
```

`_` — это **wildcard**: программист **сознательно отказывается**
от имени поля, потому что не нуждается в прямом доступе к встроенному.

##### Когда использовать

`use _` подходит для **simple wrappers** где:
- Нет необходимости в **прямом доступе** к встроенному (`@base.method()`).
- Wrapper-методы не вызывают delegated в своём теле.

`Set[T]` — типичный case: вся семантика приходит из HashMap через
auto-proxy + override на одно поведение (`insert` возвращает `bool`
вместо `Option`).

##### Override через own-methods — работает

Программист может определить wrapper-метод того же имени что у
embedded:

```nova
type Set[T] {
    use _ HashMap[T, ()]
}

// Override @insert — заменяем семантику
fn Set[T] mut @insert(item T) -> bool {
    // Здесь нельзя обратиться к HashMap.insert напрямую — нет имени
    // поля для @<base>.insert(...). Override полностью заменяет
    // логику.
    Log.info("inserting...")
    // ... custom impl, не делегируя к HashMap
}
```

Resolution через **call-site overload resolution**
([D84](10-overloading.md#d84)) с **override-precedence**: own-method
(определённый напрямую на receiver) **wins** over delegated (через
`use`).

```nova
let s Set[int] = ...
s.insert(42)
// → resolve_overload("insert", "Set[int]", [int])
// → 2 candidates: Set.@insert (own), HashMap.@insert (delegated)
// → override-precedence: own wins → Set.@insert
// → no ambiguity error
```

##### Когда **не** использовать

Если wrapper-метод нуждается в `@base.method()` для делегирования —
**нужен named alias**:

```nova
// ✓ named alias — есть `@account` для явного call
type AuditedAccount {
    use account Account
    audit_log []AuditEntry
}

fn AuditedAccount mut @deposit(amount money) {
    @account.deposit(amount)        // explicit base call
    @audit_log.push(AuditEntry.deposit(amount))
}

// ✗ anonymous embed не подходит — нет имени для base call
type AuditedAccount {
    use _ Account
    audit_log []AuditEntry
}

fn AuditedAccount mut @deposit(amount money) {
    ???                             // как вызвать Account.deposit?
                                    // НИКАК — anonymous embed не даёт имени
}
```

Compile error в этом случае возникает **естественно** на call-site:
программист пишет `@deposit(amount)` (без имени поля), это **рекурсивный
вызов** Self — бесконечная рекурсия, которая, скорее всего, не то
что хотел программист.

**Lint-warning** (не error) предложит: «possible infinite recursion
in anonymous embed override; use named alias for base-call».

##### Что запрещено

**Два anonymous embed одного типа** — недопустимо:

```nova
// ✗ COMPILE ERROR
type Wallet {
    use _ Account
    use _ Account               // ambiguous — два anonymous Account
}
```

При вызове `w.balance` resolution даёт два candidates с одинаковым
priority — **ambiguity unresolvable**, потому что нет имени поля
для disambig'а. Compile error при declaration.

Решение — named alias:

```nova
type Wallet {
    use primary Account
    use backup Account
}
```

##### Резолвинг — общий механизм overload

Anonymous embed **не вводит** специальных правил в компилятор.
Resolution использует **тот же** `resolve_overload` ([D84](10-overloading.md#d84))
с двумя расширениями:

1. **Анонимные embed-методы** регистрируются в overload registry с
   `kind = MethodKind::Delegated(via_use_anonymous)` — флагом «delegated».
2. **Override-precedence**: own-methods (без флага) **wins** over
   delegated, при прочих равных (тот же receiver, та же arity, те
   же arg-types).

Это даёт желаемое поведение «own override затмевает delegated»
без отдельной declaration-time проверки collision'а.

##### Сводка `use _ Type` vs `use name Type`

| Аспект | `use name Type` | `use _ Type` |
|---|---|---|
| Имя поля | явное (`name`) | нет |
| Auto-proxy | да | да |
| Override через own-method | да | да |
| **Доступ к base через `@<name>.method()`** | **да** | **нет** |
| Multiple embed одного типа | да (разные имена) | нет (compile error) |
| Construction через literal | `T { name: ..., ... }` | через factory `T.new(...)` |
| Pattern destructure | возможен через имя | unsupported |

#### `use` для встроенных типов (`[]T`, tuples)

`use` поддерживает не только именованные record-типы, но и **встроенные
конструкции** — массивы (`[]T`), tuples (`(A, B)`), и т.п. Имя
поля **обязательно** (как и для именованных типов):

```nova
// VecBuf через embed []T — все методы массива доступны
type VecBuf[T] {
    use data []T
    extra str
}

let v = VecBuf[int] { data: [1, 2, 3], extra: "info" }
let n = v.len            // прокси-метод к data.len ([]T API)
v.push(42)               // прокси-метод к data.push
let x = v.get(0)         // прокси к data.get
```

Этим механизмом строятся «именованные обёртки над массивами» с
дополнительными полями/методами без переписывания базового API.

API расширяется обычными методами на типе ([D35](03-syntax.md#d35)):

```nova
fn VecBuf[T] @first_or_default(def T) -> T =>
    @data.get(0).unwrap_or(def)
```

API самих встроенных типов (`[]T.len`, `[]T.push`, etc.) — открытый
вопрос Q-array-api в `open-questions.md`, формализуется в Q9 stdlib.

### Что это НЕ

**Не наследование.** `AuditedAccount` не является `Account`:

```nova
fn process(a Account) -> () => ...

let aa AuditedAccount = ...
process(aa)                         // ОШИБКА
process(aa.account)                 // ок: извлекли Account-часть через имя поля
```

Если нужен полиморфизм — структурный protocol:

```nova
type HasBalance protocol {
    balance() -> money
}

fn process(a HasBalance) -> () => ...
process(aa)                         // ок: AuditedAccount имеет balance()
                                    //  через delegation auto-proxy
```

**Не множественное наследование.** Можно `use` несколько типов, но
конфликты решаются alias'ом или явным обращением. Diamond-problem не
возникает — нет иерархии.

### Почему

1. **Замена наследования** ([D1](01-philosophy.md#d1-парадигма-protocols--data-без-классов))
   — embed решает 80% задач композиции без сложности subtyping.
2. **Согласованность с D30 naming.** Поля Nova — snake_case
   ([D30](03-syntax.md#d30)). Default-имя по типу (Go-style) дало бы
   PascalCase-поле — нарушение D30. Явный alias обязывает программиста
   выбрать snake_case, всё единообразно.
3. **Согласованность с language-wide порядком.** `use name Type` —
   тот же порядок «имя тип», что параметры, поля, let-bindings,
   for-loop. Одно правило для всего языка.
4. **AI-friendly.** Никакой magic-conversion (`HashMap` → `hashmap`/
   `hash_map`?), программист **явно** выбирает имя поля. LLM не
   догадывается.

### Что отвергнуто

- **Default-имя поля по типу** (`use Account` → поле `Account`,
  Go-style). Создаёт исключение в [D30](03-syntax.md#d30) (поля
  PascalCase в одном record-блоке с snake_case полями). Auto-conversion
  PascalCase → snake_case (`HashMap` → `hash_map`?) — magic, не
  очевидное правило.
- **`use Type as name`** (Rust import-style). `as` зафиксировано для
  cast в выражениях ([D54](03-syntax.md#d54)) и импортов
  ([07-modules.md → D29](07-modules.md#d29)). В embed — «объявление
  поля», порядок «имя тип» согласован с остальным языком.
- **Subtyping** — противоречит [D1](01-philosophy.md#d1-парадигма-protocols--data-без-классов);
  полиморфизм через protocol.
- **Множественное наследование** — известный антипаттерн (diamond,
  fragile base).

### Связь
- [01-philosophy.md → D1](01-philosophy.md#d1-парадигма-protocols--data-без-классов)
  — `use` как замена наследования.
- [02-types.md → D17](#d17-объявление-типов-единый-синтаксис-без-)
  — `use` внутри record-блока.
- [02-types.md → D15](#d15-структурные-интерфейсы),
  [D42](#d42-protocol-keyword-для-структурных-интерфейсов) —
  полиморфизм для embed-типов идёт через protocol, не через subtyping.
- [03-syntax.md → D30](03-syntax.md#d30) — naming convention (поля
  snake_case, типы PascalCase). Обязательность alias следует из D30.
- [03-syntax.md → D35](03-syntax.md#d35) — `@field.method()` для
  явного вызова из метода обёртки.
- [03-syntax.md → D38](03-syntax.md#d38) — generic-применение в
  embed: `use iter HashMapIter[K, V]`.

### Эволюция

Первая редакция D39 разрешала **default-имя** = имя типа: `use
Account` → поле `Account` (PascalCase, Go-style). Это создавало
**нарушение D30** (поля должны быть snake_case) — в одном record-
блоке `audit_log` (snake) и `Account` (Pascal) выглядели несогласованно.

**Что стало:** alias обязателен. `use Account` без имени — ошибка
компиляции, программист пишет `use account Account`. Default-имя
отменено, никакой magic-conversion `HashMap` → `hash_map`.

Также поменялся синтаксис конфликтов: раньше предлагался «явный вызов
через имя типа» (`c.Logger.log(...)`), теперь только через alias-
имя поля (`c.console.log(...)`). Это согласовано с тем, что **все
поля имеют alias-имя**, и в коде используется оно.

Q-embed-syntax в open-questions всё ещё открыт — это отдельный
вопрос про *keyword* (`use` vs `embed` vs голый тип), а не про
обязательность имени.

**Anonymous embed (2026-05-08):** добавлена форма `use _ Type` для
simple wrappers где явное имя поля бессмысленно (`use _ HashMap[T, ()]`
в `Set[T]`). Программист не выбирает alias из bikeshedding `map`/`inner`/
`s`/`value` — `_` явно говорит «безымянный embed, прямой доступ
не нужен».

Resolution для anonymous через **lazy mechanism** — общий call-site
overload-resolution ([D84](10-overloading.md#d84))
с **override-precedence** (own-method wins over delegated). Никаких
declaration-time проверок collision'ов. Это упрощает компилятор —
один путь для named и anonymous.

Trade-off anonymous vs named: anonymous теряет `@<name>.method()`
(прямой base-call) и pattern-destructure через имя поля. Эти возможности
трактуются как «escape hatches» — для них программист пишет
`use name Type` явно.

Прецеденты:
- **Go** `embedded interface{}` — anonymous, прямой доступ через имя
  типа (`s.Account`). Nova не следует — D30 запрещает PascalCase
  поля.
- **D `alias this`** — anonymous embed с implicit conversion. Nova
  не следует — нет subtyping (D1).
- **Rust composition** — нет anonymous embed; программист пишет
  field + manual delegation. Nova `use _` экономит boilerplate.

### Bootstrap status (2026-05-08)

Реализовано в bootstrap-codegen ([Plan 11](../../docs/plans/11-method-values-and-overload.md) Ф.9):

- ✅ Parser: `use name Type` (named embed) и `use _ Type` (anonymous).
  Anonymous имя поля — синтетическое `__embed_<TypeName>`.
- ✅ AST: `RecordField.is_embed: bool`, `RecordField.embed_anonymous: bool`.
- ✅ Codegen auto-proxy generation: `embed_fields` registry per record-type;
  для каждого Own-метода embedded-типа эмитится Delegated MethodSig +
  C-функция, которая делегирует через `nova_self->field`.
- ✅ Override-precedence (Own > Delegated) в emit_call и infer paths
  (Plan 11 Ф.9.3). Strict-match candidates сначала, затем фильтр Own.
- ✅ Multi-anonymous detection: declaration-time error если ≥2
  anonymous embeds одного типа в одном record'е (Plan 11 Ф.9.4).
- ✅ Lint warning `possible infinite recursion`: при detect own-method
  override на anonymous embed — stderr-warning о невозможности
  base-call'а (Plan 11 Ф.9.5).

Bootstrap-ограничения:

- C-name mangling по param-types: для overloaded delegated proxy
  имена с suffix'ом `__<types>`, как для own overload.
- Generic embed (`use map HashMap[K, V]` в generic wrapper) — работает
  для конкретных type-параметров; full generic monomorphization —
  открытый вопрос.

---

## D32. Семантика передачи параметров

> Status: revised для полей. [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindinga-readonly-для-never-mut)
> переписал семантику `mut` **на поле типа**. Семантика `mut` **на
> параметре** (этот D32) — без изменений.

### Что
Параметры функций передаются by reference в managed heap (как Java/C#
для объектов, Go для maps/slices). Без `mut` — immutable view, с
`mut` — мутации видны вызывающему. Примитивы (`int`, `bool`, `f64`,
…) — by value в регистре. Borrow `&T` отсутствует как концепция.

### Правило

**Базовое поведение.**

```nova
type Account { balance money }

// без mut — функция только читает
fn show(acc Account) Io -> () =>
    println("balance: ${acc.balance}")

// с mut — функция меняет, изменения видны вызывающему
fn deposit(mut acc Account, amount money) {
    acc.balance += amount
}

let mut my_acc = Account { balance: 100 }
deposit(my_acc, 50)
// my_acc.balance == 150 — мутация видна
```

**Примитивы — by value.** Числа, `bool`, `char`, `u8`, `()` —
всегда копия в регистре. С `mut x int` это локальная переменная
функции, изменения не видны вызывающему:

```nova
fn weird(mut x int) {
    x = 999                         // меняет локально
}

let n = 5
weird(n)
// n == 5 — примитив всегда by value
```

**Объекты (record / sum-type / массивы) — managed reference.**
Указатель в managed heap, отслеживаемый GC. В синтаксисе программист
пишет просто `o Order` — никакого `&` или `*`:

```nova
type Order { items []Item, total money }

fn add_item(mut order Order, item Item) {
    order.items.push(item)
    order.total += item.price
}

let mut my_order = Order { items: [], total: 0 }
add_item(my_order, item1)
// my_order содержит item1 и обновлённый total
```

`&T` (borrow в Rust-стиле) **не существует в Nova**. Escape analysis
закрывает большинство perf-кейсов автоматически; для real-time —
`region { ... }` ([05-memory.md → D6](05-memory.md#d6)).

**Иммутабельный binding.** Без `mut` параметр нельзя мутировать ни
одно поле (кроме помеченных `mut` per-field — см.
[D36](#d36-поля-типа-дефолт-mutable-у-mut-bindinga-readonly-для-never-mut)):

```nova
type Account { balance money }

fn read_only(acc Account) {
    acc.balance += 50               // ОШИБКА: acc immutable
    println(acc.balance)            // ок, чтение
}
```

Семантика `mut` на параметре и `mut` на поле взаимодействуют через
правила D36 — для записи нужно соответствие на обоих уровнях.

**Производительность.** Когда нужна максимальная производительность
без GC overhead — escape analysis (автоматически) или
`region { ... }` ([05-memory.md → D6](05-memory.md#d6)):

```nova
fn process_audio(samples []f32) Realtime -> []f32 =>
    region {
        let buf = []f32.with_capacity(1024)
        // обработка, без GC pauses
        buf.to_owned()
    }
```

Никаких `&T` borrow, никаких lifetime-аннотаций в обычном коде.

### Сводка

| Форма параметра | Передача | Мутация видна снаружи |
|---|---|---|
| `x int` (примитив) | by value | нет (примитив всегда копия) |
| `mut x int` | by value | нет (локальная копия) |
| `o Order` (объект) | managed reference | нет (immutable view) |
| `mut o Order` | managed reference | да |

### Почему

1. **Согласовано с managed heap** ([05-memory.md → D6](05-memory.md#d6))
   — объекты уже в куче, передача указателя дешёвая, копировать
   бессмысленно.
2. **AI-first видимость в типах** ([01-philosophy.md → D10](01-philosophy.md#d10-революционная-ставка-всё--эффект--ai-first))
   — сигнатура `fn deposit(mut acc Account, …)` против
   `fn show(acc Account)` сразу показывает контракт. Java/C#: всё
   mutable references по умолчанию, программист помнит наизусть.
3. **`mut` — единый префикс для разных случаев** (let, поле,
   параметр). Везде «mut = разрешена мутация» — одно понятие, не
   разные. Согласовано с [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindinga-readonly-для-never-mut)
   и [03-syntax.md → D33](03-syntax.md#d33).

### Что отвергнуто

- **By-value для всех типов (Go-стиль).** Копирование больших structs
  дорого, несовместимо с managed heap, программист удивляется
  «изменил поле — не сохранилось».
- **By-reference с обязательным `&mut` (Rust-стиль).** Слишком много
  синтаксиса для прикладного кода; в Nova `mut` уже работает для
  let и полей.
- **Move-семантика (Rust для не-Copy).** Сложна для прикладного
  программиста, не нужна с GC.
- **Borrow `&T`.** Скопирован в раннем дизайне рефлекторно. Borrow
  существует в Rust, потому что нет GC; в Nova с GC передача =
  указатель. Escape analysis + `region` закрывают остальное.
  Lifetime checker — research-уровень, цена реализации высокая. Go
  показывает: без borrow инфраструктура интернета работает.

### Связь
- [02-types.md → D36](#d36-поля-типа-дефолт-mutable-у-mut-bindinga-readonly-для-never-mut)
  — пересмотр семантики `mut` для полей типа. Параметры — без
  изменений.
- [05-memory.md → D6](05-memory.md#d6) — managed heap делает
  by-reference дешёвым; `region` для real-time.
- [04-effects.md → D62](04-effects.md#d62) — `Mut[T]` как generic
  эффект удалён; мутация через `mut` поля/параметры (локально) или
  специализированные state-эффекты (Counter/Cache/IdGen).
- [01-philosophy.md → D10](01-philosophy.md#d10-революционная-ставка-всё--эффект--ai-first)
  — AI-first видимость мутации в типе.
- [03-syntax.md → D35](03-syntax.md#d35) — `fn Type mut @method`
  использует тот же `mut` для self-binding'а.

### Эволюция
В D32 поле типа `mut field` мутировалось только у `mut`-binding'а.
Для аккумуляторов (все поля mutable) приходилось писать `mut` 18 раз —
шум без пользы. D36 переписал это: дефолт mutable у `mut`-binding'а,
`readonly` для never-mut, `mut` per-field — только для cache/lazy.
Семантика параметров не менялась.

---

## D36. Поля типа: дефолт mutable у `mut` binding'а, `readonly` для never-mut

### Что
Поле без префикса мутируется, **если binding mutable**. `readonly`
запрещает мутацию даже у mutable binding'а (для id, foreign keys,
invariants). `mut` per-field разрешает мутацию даже у immutable
binding'а (для cache, lazy init, atomic counters — аналог C++
`mutable`). Group-syntax: несколько полей одного типа через запятую.

### Правило

**Базовое использование.**

```nova
// Аккумулятор — все поля мутируемые, никаких префиксов не нужно
type RunAcc {
    att_wins int, def_wins int, draws int
    total_rounds int
    total_moon_chance f64
    atk_lost_m int, atk_lost_s int, atk_lost_h int
}

let mut acc = RunAcc { att_wins: 0, def_wins: 0, ... }
acc.att_wins += 1                   // ок — binding mut, поле без readonly

// Структура с invariant'ами — readonly для read-only полей
type Account {
    readonly id u64                 // никогда не меняется
    readonly owner str              // тоже
    balance money                    // мутируется у mut binding'а
    closed bool
}

let acc = Account.new("alice")
acc.balance = 100                   // ОШИБКА: binding не mut

let mut acc2 = Account.new("alice")
acc2.balance = 100                  // ок
acc2.id = 999                       // ОШИБКА: id объявлено readonly

// Cache/lazy — mut для полей, мутируемых через immutable binding
type LazyConfig {
    path str
    mut cached_value Option[str]    // обновляется при первом read
}

fn LazyConfig @get() -> str {
    if let Some(v) = @cached_value { return v }
    let v = read_file(@path)
    @cached_value = Some(v)         // мутация через @-метод даже у let-binding
    v
}
```

**Group-syntax.** Несколько полей одного типа — через запятую:

```nova
type Point { x, y, z f64 }                          // три f64
type Color { r, g, b u8 }                           // три u8
type RunAcc {
    att_wins, def_wins, draws int
    atk_lost_m, atk_lost_s, atk_lost_h int
    atk_lost_pts, def_lost_pts f64
}
```

С префиксами:

```nova
type Account {
    readonly id, owner_id u64       // два immutable
    balance money                    // дефолт (mutable у mut-binding)
    mut last_access_time time        // mutable всегда
}
```

### Сводная таблица

| Объявление поля | Mutable у `let acc` | Mutable у `let mut acc` | Use case |
|---|---|---|---|
| `field T` (без префикса) | нет | **да** | большинство полей |
| `readonly field T` | **никогда** | **никогда** | id, immutable invariants |
| `mut field T` | **да** | **да** | cache, lazy init, atomic counters |

### Почему

1. **Меньше шума для типичного случая.** Аккумулятор с 18 mutable
   полями писать без префиксов — все поля «обычные», никаких
   акцентов. Раньше 18 раз `mut` — визуальный мусор.
2. **Сигнатура показывает только важное.** Префикс ставится **только
   на исключения** (`readonly` для invariants, `mut` для cache).
   LLM, читая тип, видит: `readonly id` — «не трогай», обычное поле
   — «можно мутировать с mut-binding'ом».
3. **Прецедент Rust/Go/C++** — поля без префикса мутируются у
   mut-binding'а; `readonly` для never-mut близко к C++ `const`
   member.

### Что отвергнуто

- **Старая семантика D32** (поле `mut` мутируется только у
  `mut`-binding). Заставляет писать `mut` перед каждым полем
  аккумулятора; если все поля mut — выделение теряет смысл.
- **Rust-полное** (поле всегда mutable у mut-binding, нет never-mut).
  Невозможно зафиксировать read-only invariant без приватного
  поля + getter.
- **`type X mut { … }`** (mut на тип). Один маркер вместо 18 — короче,
  но при 90% mut + 10% read-only нужен опт-аут per field.
  Усложнение. Конфликт с современным паттерном «struct + immutable
  defaults + явная мутация» из Swift/Rust.
- **`final` (Java-стиль)** для never-mut полей. Короче, прецедент
  Java/Dart/Kotlin, но семантически перегружен (`final method`,
  `final class`, `final var`). `readonly` прямо говорит «только для
  чтения».
- **`let` для never-mut полей.** Короче (3 символа), прецедент Swift,
  но `let` уже значит «binding имени со значением»
  ([03-syntax.md → D33](03-syntax.md#d33)). На поле без `=`
  необычно, не самообъясняемо. `readonly` прямо говорит цель.
- **`const` (C++-стиль).** Конфликт с
  [03-syntax.md → D33](03-syntax.md#d33) — там `const` =
  compile-time константа. Здесь — runtime-immutable. Перегрузка
  термина, AI-first против — невозможно.

### Связь
- [02-types.md → D32](#d32-семантика-передачи-параметров) —
  пересмотр семантики `mut` для полей. Передача параметров (`fn f(mut
  o Order)`) остаётся: `mut` на параметре = mutable binding,
  внутри — мутации полей по правилам D36.
- [02-types.md → D17](#d17-объявление-типов-единый-синтаксис-без-)
  — group-syntax для полей одного типа внутри record.
- [03-syntax.md → D33](03-syntax.md#d33) — `let` это immutable
  binding; на поле — аналогия в роли `readonly`.
- [03-syntax.md → D35](03-syntax.md#d35) — `fn Type mut @method`
  даёт mutable-binding self, поля затем по правилам D36.

### Эволюция
До D36 поле помечалось `mut field T`, мутируемое только у
`mut`-binding'а (D32). Для аккумуляторов это требовало 18 раз
повторить `mut` — шум без пользы. D36 инвертировал дефолт: «обычное
поле — мутируется у mut-binding'а», `readonly` — для исключений.
Семантика параметров (D32) не менялась. Подробно — в
`history/evolution.md`.

---

## D66. `Self` universal — ссылка на обобщающий тип в методах, effects, protocols

### Что
`Self` — keyword-ссылка на «тот тип, к которому принадлежит метод»,
валиден **в любом контексте, ассоциированном с конкретным типом**:

- Внутри `protocol { ... }` — `Self` = тип, удовлетворяющий контракту
  (как сейчас по [D42 (REVISED)](#d42)/[D53](#d53)).
- Внутри `effect { ... }` — `Self` = тип эффекта (`Db`, `Net`, ...).
- В static-методе `fn T.name(...)` — `Self` ≡ `T`.
- В instance-методе `fn T @method(...)` / `fn T mut @method(...)` —
  `Self` ≡ `T`.
- Для generic-типа `T[A, B]` — `Self` ≡ `T[A, B]` (с теми же параметрами).

### Правило

```nova
type Box[T] {
    value T
}

// static method — Self вместо повтора Box[T]
fn Box[T].of(v T) -> Self =>
    Self { value: v }

// instance method — Self в return type для builder pattern
fn Box[T] @with_value(v T) -> Self =>
    Self { value: v }

// protocol — для type-safe equality
type Hashable protocol {
    hash() -> u64
    eq(other Self) -> bool       // Self = тот тип, что реализует
}

// effect — для transactional/recursive handler-операций
type Db effect {
    query(q Sql) -> []DbRow
    nested(body fn() Self -> ()) -> ()  // Self = Db
}

// sum-type method
type Tree | Leaf | Node(int, Tree, Tree)
fn Tree @clone() -> Self => match @ {
    Leaf          => Leaf
    Node(v, l, r) => Node(v, l.clone(), r.clone())
}
```

### Семантика

- `Self` подставляется **в момент использования метода/протокола**,
  не в момент объявления.
- Для concrete-типа `T` (record, sum, newtype) `Self` ≡ `T`.
- Для generic `T[A, B]` `Self` ≡ `T[A, B]` (наследует ту же
  специализацию).
- Внутри protocol-объявления `Self` остаётся «late-bound» — конкретный
  тип определяется при удовлетворении.

### Static-методы знают свой тип через `Self`

Static-метод в Nova **связан с типом** на уровне компилятора — не
«просто функция в namespace» (как Go), а **полноценный метод типа**
с доступом к `Self`. Это влияет на три use-case'а:

#### 1. Self в return type (DRY-форма)

```nova
type Box[T] {
    value T
}

fn Box[T].of(v T) -> Self =>            // Self ≡ Box[T]
    Self { value: v }                    // generic-параметры наследуются

// Эквивалент без Self (verbose):
fn Box[T].of(v T) -> Box[T] =>
    Box[T] { value: v }
```

Без `Self` программист пишет `Box[T]` дважды; с `Self` — один раз
(в receiver). Compiler знает что `Self ≡ Box[T]` потому что метод
объявлен **на `Box[T]`**.

#### 2. Self в expression position — вызов другого статического

```nova
type Account { balance money }

fn Account.new() -> Self =>
    Self.with_initial(0)                 // другой static-метод того же типа

fn Account.with_initial(amount money) -> Self =>
    Self { balance: amount }              // Self { ... } literal
```

`Self.with_initial(0)` резолвится compiler'ом в `Account.with_initial(0)`.
То же для `Self { ... }` — это **`Account { ... }` literal**.

Это canonical pattern для **default-конструктор → parameterized-конструктор**:

```nova
fn HashMap[K, V].new() -> Self =>
    Self.with_capacity(16)              // default делегирует к parameterized

fn HashMap[K, V].with_capacity(n int) -> Self =>
    Self { buckets: new_buckets(n), count: 0, ... }
```

Refactoring-safe: переименование `HashMap → Map` меняет только
**заголовки методов**, не тела. Все `Self` авто-резолвятся.

#### 3. Self в полиморфных контекстах (через protocol bound)

```nova
type FromStr protocol {
    from_str(s str) -> Self              // late-bound
}

fn parse[T FromStr](s str) -> T => T.from_str(s)
//                                  ^^^^^^^^^^^^
// На каждой инстанциации parse[int](...) / parse[Money](...)
// T резолвится в конкретный тип. Compiler через monomorphization
// знает Self ≡ T для каждого вызова.
```

Это **post-monomorphization** — для каждого `parse[X]` генерится свой
код где `X.from_str(s)` это конкретный static-метод X. Static-метод
знает что он на X **в каждом инстанциации**.

#### Что это **не** значит

- **Нет runtime-рефлексии.** Static-метод не имеет `cls`-параметра
  (как Python `@classmethod`), не может узнать своё имя как строку,
  не может сравнить два типа в runtime. Знание чисто **compile-time**.
- **Self в expression — синтаксическая подстановка.** Compiler
  заменяет `Self` на имя receiver-типа в момент codegen'а; runtime
  никаких type-id не передаёт.
- **Нет inheritance / virtual dispatch.** Self ≠ виртуальный
  reference на subclass. У Nova нет наследования (D1) — только
  generic-bound через protocol.

#### Прецеденты

- **Rust:** `impl Foo { fn make() -> Self { Self::new(2) } }` —
  активно используется. `Self` доступен везде в impl-блоке.
- **Swift:** `static func make() -> Self`, `Self.method()`,
  `Self()` initializer.
- **Kotlin:** `companion object` с methods, доступ к `this::class`.
- **C#:** `static` метод имеет доступ к containing type.

Не следуем:
- **Go:** static-методов нет, только receiver-функции. Static в Nova =
  named function в namespace типа.
- **Python `@staticmethod`:** не получает `cls`, не знает свой тип.
  `@classmethod` получает `cls` runtime — мы делаем то же на
  compile-time через `Self`.

### Где запрещено

- На top-level (вне типа/protocol/effect) — compile error «Self не в
  type-контексте».
- Внутри лямбды, объявленной не в method-теле — compile error.
- В сигнатуре свободной (top-level) функции `fn name(...)` — compile
  error.

### Почему

1. **DRY.** До D66 в каждом методе `fn Box[T].of(v T) -> Box[T]` имя
   типа повторялось 2-3 раза. Refactoring (`Box` → `Container`) ломал
   копипастой. `Self` устраняет повтор.
2. **Generic-параметры наследуются автоматически.** `fn Box[T].of` с
   `Self` корректно подставит `Box[T]`, не `Box` без параметров —
   программисту не нужно указывать generics в методе.
3. **AI-friendly.** LLM генерирует `Self` для return type без знания
   точного имени — снижает количество ошибок при автогенерации
   builder-методов.
4. **Унификация.** До D66 `Self` работало только в protocol — это
   создавало впечатление, что для других контекстов нужен другой
   механизм. На самом деле семантика одинаковая — «текущий тип».
   Один keyword для всех контекстов = D40 «один способ».
5. **Прецеденты.** Swift, Rust используют `Self` универсально (везде
   где есть `impl T { ... }` блок). Nova следует тому же паттерну.

### Что отвергнуто

- **`@type`** — конструкция вида `@type` для ссылки на свой тип в
  методе. Отвергнуто: `@` уже занят под self-field, добавление
  второго смысла создаёт двусмысленность.
- **Имя типа повторять везде.** Отвергнуто: см. п.1 «DRY».
- **`Self` только в generic-методах** (как в Java `<T extends Self>`).
  Отвергнуто: семантика остаётся та же, ограничение лишнее.

### Связь

- [D42 (REVISED)](#d42) / [D53](#d53) — `Self` в protocol'ах
  (исходное правило, расширено D66).
- [03-syntax.md → D35](03-syntax.md#d35) — `@`-методы и `@field`.
- [04-effects.md → D61](04-effects.md#d61) — effect-типы и handler'ы.

### Эволюция
В D42 `Self` был валиден **только** внутри `protocol { ... }` блока —
это ограничение унаследовано от первой редакции, где Self вводился
именно для type-safe equality (`Hashable.eq(other Self)`). На
practice'е `Self` оказался полезен также в:
- static-методах для DRY возврата того же типа,
- instance-методах для builder pattern'а,
- effect-методах для self-referential операций (transactions),
- sum-вариантах для `@clone`/`@with_*` методов.

D66 убирает ограничение: `Self` валиден везде, где есть type-контекст.

---

## D72. Generic bounds через `[T Protocol]` — protocol как тип

### Что
Параметр-тип в generic-списке может иметь **bound** — protocol-тип,
которому должны удовлетворять конкретизации параметра. Синтаксис —
единое правило «name type» без двоеточия:

```nova
[T Hashable]
[K Hashable, V]
[K, T From[K]]
```

Без bound — `[T]` — параметр без ограничений (структурное соответствие
проверяется при использовании, как было до D72).

Bound — это **protocol-тип** (D53). Тот же `Hashable` стоит и в
позиции типа значения (`fn f(x Hashable)` — existential), и в позиции
bound'а (`fn f[T Hashable](x T)` — universal). Одна сущность —
тип со структурным контрактом — в трёх позициях:

1. Тип значения: `fn f(x Hashable) -> u64`
2. Bound: `fn f[T Hashable](x T) -> u64`
3. Эффект (между `)` и `->`): `fn f(...) Db -> ()` (D18)

Различение по позиции, не по keyword'у. Закрывает [Q-bounds](../open-questions.md#q-bounds).

### Правило

#### Синтаксис

```
generic-params = '[' generic-param { ',' generic-param } ']'
generic-param  = identifier [ type ]
```

`generic-param` следует общему правилу Nova «`name type`», как
параметры функции (`x int`), поля record (`id u64`), let-bindings
(`let x int = 5`), for-loops (`for x int in xs`), embed
(`use w HashMapIter[K, V]`).

```nova
fn sort[T](xs []T, less fn(T, T) -> bool) -> []T
//      ^ без bound — структурное соответствие при использовании

fn dedup[T Hashable](xs []T) -> []T
//       ^^^^^^^^^^^ T должен реализовывать Hashable

type HashMap[K Hashable, V] {
//          ^^^^^^^^^^^ K — Hashable, V — без bound
    ...
}

fn fold[T, Acc](xs Iter[T], init Acc, f fn(Acc, T) -> Acc) -> Acc
//      ^^^^^^ ни T, ни Acc bound'а не имеют
```

#### Порядок объявления параметров

Generic-параметры читаются **слева направо**. Имя в bound'е должно
быть **уже объявлено** — либо ранее в том же списке `[...]`, либо в
type-контексте (top-level type, окружающий тип для метода).

```nova
fn func[K, T From[K]](v K) -> T => T.from(v)
//      ^                          ^
//      объявлен раньше            используется в bound

fn func[T From[K], K](v K) -> T          // ОШИБКА: K используется до объявления
fn func[T Test[K]](v K) -> T             // ОШИБКА: K не объявлен вообще
```

Это согласовано с правилом параметров функции: `fn f(x int, y T)` —
имена читаются слева направо, ранее объявленные доступны позже.
Forward-references запрещены ради простоты type-checker'а и
читаемости (LLM не нужно держать «отложенный контекст»).

#### Bound — это protocol-тип

`Hashable`, `From[T]`, `Into[T]` и т.д. — обычные protocol-типы (D53):

```nova
type Hashable protocol {
    hash() -> u64
    eq(other Self) -> bool
}

// Bound в generic-объявлении:
fn map[K Hashable, V](m HashMap[K, V]) -> ...

// Тот же Hashable в позиции типа значения (existential):
fn dump_one(x Hashable) -> u64 => x.hash()
```

**Existential vs universal — различение по позиции:**

| Форма | Семантика | Dispatch | Аналог Rust |
|---|---|---|---|
| `fn f(x Hashable)` | existential («какое-то значение типа Hashable») | dynamic (vtable) | `fn f(x: &dyn Hashable)` |
| `fn f[T Hashable](x T)` | universal («для любого T : Hashable») | static (mono) | `fn f<T: Hashable>(x: T)` |

В обоих случаях `Hashable` — **тип**. Различие только в позиции:
внутри `[...]` — generic-параметр и его bound; в обычной позиции —
тип значения. Прецедент — Go (`interface { M() }` используется и как
тип, и как constraint).

#### Multiple bounds — анонимный protocol

Если параметру нужно несколько bounds, объединяются в анонимный
protocol-тип через `protocol { ... }` (D53):

```nova
fn min[T protocol { @lt(other Self) -> bool, @eq(other Self) -> bool }](xs []T) -> T
```

Долго, но без специального синтаксиса для intersection bound'ов.
Если паттерн повторяется — выносится в именованный protocol:

```nova
type Ord protocol {
    @lt(other Self) -> bool
    @eq(other Self) -> bool
}

fn min[T Ord](xs []T) -> T => ...
```

**Сокращённая форма `[T A & B]`** — открытый вопрос
([Q-multi-bound](../open-questions.md)).

#### `Self` в bounds

`Self` (D66) валиден внутри protocol/method-контекста. В bound'е
generic-параметра свободной функции — **запрещён**:

```nova
fn merge[T Eq](a T, b T) -> T => ...           // ok
fn merge[T Eq Self](a T, b T) -> T => ...      // ОШИБКА: Self вне type-контекста
```

В method-контексте (`fn Box[T] @method[U Self]`) — открытый вопрос,
пока запрещено.

#### Bound как effect — запрещено

Bound — это `protocol`-тип. Effect — тоже `protocol`, но используется
**в позиции эффекта** (между `)` и `->`). Использовать `Db` как bound
запрещено — это ошибка категории (D62: `effect` ≠ `protocol` для
generic-bound):

```nova
fn run[T Db](handler T) -> ()         // ОШИБКА: Db — effect, не bound-protocol
```

Если нужно «принимает Effect[Db]» — пишется явно: `fn run(h Effect[Db])`.

#### Bound на типах (не функциях)

Тот же синтаксис в declaration типов:

```nova
type HashMap[K Hashable, V] {
    readonly buckets []Slot[K, V]
}

type Set[T Hashable] {
    readonly inner HashMap[T, ()]
}

type Sorted[T Ord] | Empty | Node(T, Sorted[T], Sorted[T])
```

Bound применяется при инстанциировании: `HashMap[User, int]` требует
чтобы `User` реализовывал `Hashable`.

#### Проверка bound'а — структурная (D53)

Bound удовлетворён, если у конкретного типа есть **методы из
protocol'а** (структурно). Никаких явных `impl`/declaration не нужно:

```nova
type User { id u64 }

fn User @hash() -> u64 => @id
fn User @eq(other Self) -> bool => @id == other.id

// User автоматически удовлетворяет Hashable, потому что есть @hash и @eq
let m HashMap[User, str] = HashMap.new()       // ok
```

Если методов нет — compile error на месте использования (`HashMap[User, str]`
с инстанциированием), не на declaration `type User`.

### Почему

1. **Закрывает Q-bounds.** Generic-инфраструктура (HashMap, From/Into,
   collect, FromIter) требует bound'ов. Без них либо безопасности
   нет, либо ошибки откладываются до места использования с непонятным
   сообщением.

2. **Согласовано с правилом «name type».** Параметр функции `x int`,
   поле `id u64`, generic-параметр `T Hashable` — единая грамматика.
   Двоеточие в Nova зарезервировано под key-value, использовать его
   для bound — нарушение D17.

3. **Protocol = тип (D53).** `Hashable` уже тип в Nova. Использовать
   его как bound — естественное расширение, не новый механизм.
   Existential (`x Hashable`) и universal (`[T Hashable]`) различаются
   позицией.

4. **Прецедент Go.** Go 1.18+: `interface { M() }` используется и как
   тип значения, и как constraint в generics. Один синтаксис, два
   контекста, проверено в большом продакшне.

5. **Структурная проверка вместо impl.** Nova не имеет orphan rule
   (D42/D53) — нет `impl Trait for Type` блоков. Bound удовлетворяется
   автоматически, как и existential. Это последовательно.

6. **AI-friendly.** LLM пишет `[T Hashable]` без специальных
   keyword'ов (`where`, `impl`, `:`). Грамматика читается как
   естественный язык: «параметр T типа Hashable».

### Что отвергнуто

- **`[T: Hashable]`** (Rust/Scala/Kotlin/Swift). Конфликтует с D17 —
  двоеточие в Nova только для key-value (record-литералы, dict).
  Делать исключение для generic-list — нарушение единства.
- **`[T is Hashable]`.** `is` уже занят под runtime type-check (D54).
  Третий смысл (compile-time bound) перегружает keyword.
- **`where`-clauses после сигнатуры** (C# / Haskell-style). Многословно,
  раздваивает информацию между списком параметров и where-блоком.
  Bound у параметра — единое место.
- **`[T impl Hashable]`** (Swift `some`-style). Нестандартно,
  `impl` не используется в Nova ни для чего ещё.
- **Bounds через контракты** (`requires implements(T, Hashable)`).
  Контракты (D24) проверяются SMT на значениях, bound — type-checker'ом
  на типах. Разные уровни.
- **Sealed/closed bound'ы** («только эти типы»). Открытый вопрос,
  не входит в D72.

### Цена

1. **Type-checker сложнее.** Проверка structural-bound при
   мономорфизации — дополнительная работа.
2. **Сообщения об ошибках.** «`User` не реализует `Hashable`: missing
   method `@hash`» — нужно генерировать понятные диагностики.
3. **Множественные bounds через анонимный protocol** — многословно
   для частых пар (`Hash + Eq`). Сокращённая форма откладывается.

### Связь

- [02-types.md → D53](#d53-унификация-protocol-под-type-protocol-как-kind-токен)
  — protocol = тип, основа D72.
- [02-types.md → D42](#d42-protocol-keyword-для-структурных-интерфейсов)
  — структурная типизация, две модели generic-параметров.
- [02-types.md → D66](#d66-self-universal--ссылка-на-обобщающий-тип-в-методах-effects-protocols)
  — `Self` в protocol-контексте.
- [03-syntax.md → D16](03-syntax.md#d16-дженерики-через-t-не-t)
  — `[T]` синтаксис для generic'ов.
- [04-effects.md → D18](04-effects.md#d18-эффекты-объявляются-через-kind-токен-не-голый-type)
  — protocol в effect-position, отличается от bound-position.
- [08-runtime.md → D73](08-runtime.md#d73) — `From[T]`/`Into[T]`
  используют bound `[U From[T]]` для generic-функций конверсии.
- [Q-bounds](../open-questions.md#q-bounds) — closed by D72.
- [Q-collect-mechanism](../open-questions.md#q-collect-mechanism)
  — становится решаемой после D72.

### Открытые вопросы

- **Множественные bounds**: сокращённая форма (`[T Hash & Eq]`,
  `[T (Hash, Eq)]`) — Q-multi-bound.
- **Bound на эффект-параметре**: можно ли `[E SomeProtocolOnEffects]`
  — связано с Q-effect-params.
- **`Self` в bound** в method-контексте — отложено.
- **Conditional methods** через `where`-clause (`fn Vec[T] @sort()
  where T Ord`) — отложено вместе с conditional impls.

### Эволюция

В MVP bounds были **отвергнуты** ([D42 «Открытые вопросы»](#d42),
[history/rejected.md](history/rejected.md): «`[T: Bound]` отвергнут
в MVP»). Пользовались структурным соответствием при использовании —
ошибка вылезала на месте вызова, не объявления. С ростом stdlib
(HashMap, From/Into, collect) стало ясно что **без bound'ов нельзя**:
generic-функции не могут опираться на методы T без явного контракта.

Q-bounds зафиксировал синтаксис заранее (`[T Bound]` без двоеточия).
D72 принимает это как формальное решение, расширяет до полной семантики
(structural check, existential-vs-universal через позицию, multiple
bounds через анонимный protocol).

---

## D110. Ghost state — spec-only bindings

**Статус:** Принято (Plan 33.3 Ф.10, реализовано в AST и type-checker)

### Решение

`ghost let` / `ghost var` объявляют **spec-only переменные** — они видимы
в `requires`/`ensures`/`invariant` и других `ghost`-statements, но
**никогда не эмитируются в C-код** (ни в debug, ни в release).

```nova
fn fill(xs mut []int) -> ()
    ensures forall i in 0..xs.len() : xs[i] == 0
{
    ghost let n = xs.len()      // spec-only: виден в invariant
    for i in 0..xs.len()
        invariant forall j in 0..i : xs[j] == 0
    {
        xs[i] = 0
    }
}
```

**Правила видимости ghost:**
- Ghost-binding виден: в других `ghost`-stmts; в `requires`/`ensures`/`invariant`; в теле `#pure` функций.
- Использование ghost-binding в non-ghost emit-code → **compile error**.
- Codegen: ghost-stmts и ghost-bindings полностью стираются (паритет с Dafny).

**Следствие:** invariants, использующие ghost-данные, в debug **не проверяются
runtime** — только через SMT. Это задокументированное design-решение.

### Обоснование

Ghost state позволяет писать контракты в терминах вспомогательных
концепций (счётчики, логические флаги, промежуточные значения), не
засоряя runtime-код. Паритет с Dafny `ghost var`, F* `Ghost`.

### Реализация

- `compiler-codegen/src/ast/mod.rs` — поле `is_ghost: bool` в `LetDecl`;
  enum-вариант `Stmt::Ghost` для ghost-блоков (Ф.10 scope).
- `compiler-codegen/src/types/mod.rs` — type-check: reject ghost-ref
  в non-ghost context.
- `compiler-codegen/src/codegen/emit_c.rs` — ghost-stmts стираются
  (пустой emit).
- `compiler-codegen/src/verify/encode.rs` — ghost-vars участвуют
  в SMT-encoding как обычные fresh-vars.

---

## D122. Hybrid dispatch для bound-K methods

> **Status:** active (spec). Реализация — [Plan 56](../../docs/plans/56-vtable-dispatch-erased-generics.md).

### Что

Generic-bound method call'ы dispatch'аются по hybrid strategy:

1. **Mono path** — для concrete K на call-site (e.g. `HashMap[str, int]`):
   compiler instantiates generic method с substituted K, V. Bound
   methods (`key.hash()`, `key.eq()`) resolve в direct call к concrete
   K methods (`nova_str_hash(key)`). **Zero-cost** — паритет Rust
   `impl<T: Hashable>`.

2. **Erased path** — для generic body emit (когда compiler не может /
   не должен mono'd, e.g. recursive generic call на Self type внутри
   generic method body): generic body эмитится как **stub** (call'еры
   полагаются на mono path для concrete instances). Bootstrap не
   использует vtable — простая stub-fallback стратегия.

3. **Vtable path** (future, Plan 56 Ф.2 full): для truly erased
   contexts (cross-crate generic, `dyn Trait`-like), bound methods
   dispatch'аются через vtable structure. Vtable runtime defined в
   `compiler-codegen/nova_rt/vtables.h` (Plan 56 Ф.1).

### Bootstrap status (2026-05-16)

- ✅ Mono path для bound methods works (HashMap.@clone() пример).
- ✅ Vtable runtime infrastructure готова (`NovaVtable_Hashable`,
  `NovaVtable_Comparable`, `NovaVtable_Display` + 4 primitive K
  vtables: int/bool/u8/f64/str).
- ✅ Erased emit для bound-method-using generic methods stub'ится
  (`emit_generic_method_erased` — wider stub condition включает Array
  fields с generic inner type).
- ⏸️ Vtable codegen integration (truly erased dispatch) — deferred
  до cross-crate compilation (Plan 03).

### Acceptance criteria для bound methods

Type-checker (Plan 15 / D72) enforces:
- Bound должны быть protocol-типами (D53).
- Concrete K на call-site должен implement все bound methods (D72
  enforcement).

Codegen (Plan 56 Ф.1 + Ф.2):
- **Protocol-методы могут иметь эффекты** (`Fail` / `Io` / `Db`) —
  напр. `type TryFrom[T, E] protocol { try_from(t T) Fail[E] -> Self }`.
  Под **mono-dispatch** (текущий bootstrap) эффект protocol-метода
  пробрасывается как у обычной effectful-функции — без спец-кейса.
  *(D122 amended 2026-05-20: снят запрет Plan 56 Ф.2.7 на pure-only
  bound methods.)* **Ограничение**: true-vtable dispatch (Plan 03) не
  пробрасывает effect-handlers через vtable-ABI — в truly-erased
  контексте effectful-protocol bounds обязаны mono-dispatch'иться;
  чистая vtable-диспетчеризация effectful-метода — будущая работа
  Plan 03.
- Self type в bound method signature substitutes runtime receiver type.

### Связь

- [D72](#d72-generic-bounds-через-t-protocol--protocol-как-тип) —
  generic bounds enforcement (type-checker side).
- [D53](#d53-anonymous-protocol-literals) — protocol-типы.
- [D24](#d24-контракты) — vtable lookups compatible с proven-contracts
  skip (no-op).

## D123. Tuple monomorphization

> **Status:** active (spec, 2026-05-17 EOD+2 — Phase 7 production polish
> applied). Реализация — [Plan 59](../../docs/plans/59-tuple-monomorphization.md)
> (6 phases + Phase 7).

### Что

Tuple типы `(T1, T2, ..., TN)` monomorphized — для каждой concrete
комбинации element types compiler generate'ит отдельную struct
с **real** field types (не nova_int slot erasure).

### Mangle scheme (Plan 59 Phase 5, length-prefixed)

**Itanium ABI / Rust v0 mangle analog** — unambiguous для любой
глубины nesting:

```
_NovaTuple_<arity>_<L1>_<T1>_<L2>_<T2>_..._<LN>_<TN>
```

где `<Ln>` — десятичная byte length sanitized name `<Tn>`. Parser
читает length, берёт точно столько chars, переходит к следующему.
Самоописательный, никаких ambiguity даже для tuple-of-tuples.

**Примеры:**
- `(int, int)` → `_NovaTuple_2_8_nova_int_8_nova_int`
- `(str, int)` → `_NovaTuple_2_8_nova_str_8_nova_int`
- `((int, int), int)` outer →
  `_NovaTuple_2_34__NovaTuple_2_8_nova_int_8_nova_int_8_nova_int`
  (L1=34 — точно столько chars как T1)

Distinguishable от legacy `_NovaTupleN` (e.g. `_NovaTuple2`) по `_`
после `NovaTuple`.

### Правило

```nova
let p (str, int) = ("a", 1)
//                   ^^^^^^^ generates _NovaTuple_2_8_nova_str_8_nova_int
//                   { nova_str f0; nova_int f1; }

for (k, v) in hashmap {
//   ^^^^^^^^^^^^^^^^ implicit Iter (D58) + tuple destructure через
//                    mono'd struct (k: nova_str, v: nova_int direct
//                    field access)
}

match some_kv {
    Some((k, v)) => ...
//       ^^^^^^^ Plan 59 Phase 6 — variant payload mono'd tuple,
//               heterogeneous types работают (str + int)
}
```

**Параллель:** Rust `(T1, T2)` mono'd per concrete instantiation,
zero-cost. C++ `std::tuple<T1, T2>` template — то же. Nova bootstrap
паритет (vs предыдущий int-slot erasure breaking struct elements).

### Decision tree

При codegen tuple type:
1. **All elements concrete** (resolved via current_type_subst,
   no type-param placeholders) → use mono'd `_NovaTuple_<arity>_<L1>_<T1>...`
   struct. Zero erasure cost.
2. **Erased context** (one or more element types unresolved) →
   fallback legacy `_NovaTupleN` (nova_int slot) с runtime cast.
   Bootstrap-compat для truly generic contexts.

### Constraints

- **Tuple field access** (`p.0`, `p.1`) — direct C field access
  (`.f0`, `.f1`) на mono'd struct.
- **Tuple destructure** (`let (a, b) = ...`) — direct binding, no cast.
- **Nested tuples** (`((int, str), bool)`) — recursive mono'd (inner
  tuple registered first; length-prefix encoding handles нестинг
  любой глубины — validated 5-level tests).
- **Tuple в variant payload** (`Option[(K, V)]`, `Result[(K, V), E]`) —
  match destructure `Some((k, v))` / `Ok((k, v))` propagate mono'd
  element types через registry (Phase 6 + Plan 63 Fix F+).
- **Tuple in collections** (`HashMap[K, V]` returns `Option[(K, V)]` from
  `iter().next()`) — mono'd через template + subst at iter mono pass.

### Diagnostics (Plan 59 Phase 7.1)

- **Arity mismatch** — destructure pattern имеющий разное число
  элементов чем actual tuple, reject'ится **Nova-level** clear
  error (file:line + hint) до C-emit'а. Покрывает 3 sites:
  let-destructure, for-pattern, match-variant inner Tuple.
  Раньше упирался в нечитаемый "no member named 'fN'" C error.

### Lint warnings (Plan 59 Phase 7.3)

- **Large tuple warning** — mono'd tuple с >5 элементов **OR** >128
  bytes estimated size emit'ит W-warning suggesting record type
  (clarity + stable ABI). Estimate sums known element sizes:
  pointers=8, nova_str=16, scalars per type. Threshold выбран
  эмпирически — typical cache line 64 bytes, 2× giving safe margin.

### Stdlib idiom (Plan 59 Phase 7.2)

После Plan 63 Fix E (mono'd tuple iter в generic method body
работает) — stdlib коллекции используют **идиоматичный**
`for (k, v) in self` / `for (k, v) in @iter()` вместо
direct-field workaround'ов. HashMap.@clone/@merge_from/@filter все
idiomatic.

### Field literal style (related, D52 §2)

Record literal для tuple struct полей (`{ end, idx: 0 }` для
`{end int, idx int}` где `end` — variable в scope) — **shorthand
обязателен** при совпадении имени поля с источником (`{ end: end }`
запрещено, см. [D52 §2](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-)).

### Почему

1. **Correctness** — struct value types (nova_str, user records)
   не fit'ят в nova_int slot. Без mono `(str, int)` was broken.
2. **Zero-cost** — direct field access, no intptr_t cast, no heap
   alloc для tuple value.
3. **Параллель Rust/C++** — индустриальный standard для tuples.
4. **Diagnostics quality** — Plan 36 R7 bar (file:line + hint).
5. **Self-describing mangle** — length-prefix encoding debug'абельно,
   ABI-tools (debuggers) могут decode.

### Что отвергнуто (deferred с rationale)

- **Universal tuple type** (all elements `any`) — type-erased, runtime
  type-tag overhead, breaks AOT zero-cost goal.
- **Named tuple fields** (`(x: T1, y: T2)`) — **ОТКЛОНЕНО окончательно
  (Plan 59 Ф.7.4, 2026-05-21).** Именованные поля кортежа почти
  идентичны record'у; заводить два почти одинаковых синтаксиса для
  одной семантики в Nova нет причин. Нужен агрегат с именованными
  полями — это record (`type T { x int, y int }`). Tuple остаётся
  позиционным (`.0`/`.1`).
- **Tuple subtyping** (`(int, str) <: (any, any)`) — **ОТКЛОНЕНО
  окончательно (Plan 59 Ф.7.6, 2026-05-21).** Реализация дорогая
  (требует variance-системы covariance/contravariance в type-checker,
  которой в Nova нет — язык не использует structural typing); под
  фичу не нашлось ни одной реальной задачи. Не реализуется.
- ~~**Full mono'd Result** (`NovaRes_<T>_<E>` typedefs analogous Option)
  — Plan 63 Fix F+ targeted boxed-pointer tracking покрывает все
  observable cases без full sum-type mono refactor. Defer до Plan 65.~~
  **✅ РЕАЛИЗОВАНО (Plan 59 Ф.7.5 increment 2, 2026-05-21):** Result
  полностью мономорфизирован — per-(T,E) C-тип `NovaRes_<ok>_<err>*`
  (аналог `NovaOpt_<T>`). Legacy единый `Nova_Result` устранён;
  targeted Fix F+ boxed-tracking больше не нужен — Ok/Err payload
  типизируется реальным T/E inline.

### Связь

- [D27](03-syntax.md#d27-синтаксис-массивов-t-префикс-nt-фиксированные)
  — tuple литерал синтаксис.
- [D52 §2](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-)
  — field shorthand mandatory.
- [D58 Iter protocol] — `for (k, v) in coll` использует mono'd tuple
  через implicit `.iter()`.
- [Plan 48](../../docs/plans/48-closures-in-generics.md) —
  monomorphization infrastructure (mono pass).
- [Plan 63](../../docs/plans/63-cross-module-mono-dispatch-correctness.md)
  — Fix E (mono'd iter в generic method body) + Fix F/F+ (Result
  Ok payload tuple unboxing).

---

## D119. Method-level type parameters в generic methods

> **Status:** active (spec, 2026-05-17). Реализация — [Plan 48 Ф.9](../../docs/plans/48-closures-in-generics.md#-9--method-param-mono).
> Закрывает частично [Q-generic-receiver-method](../open-questions.md#q-generic-receiver-method)
> (для user-defined generic типов; built-in `[]T` остаётся V2).

### Что

Generic methods могут иметь **собственные type-параметры**, независимые
от type-параметров receiver'а. Метод `Wrapper[T] @map[U](f fn(T) -> U) -> Wrapper[U]`
имеет два уровня generics: receiver-level `T` и method-level `U`.
Compiler через monomorphization создаёт **отдельную mono-instance**
для каждой комбинации `(T, U)`.

### Правило

```nova
export type Wrapper[T] { inner T }

// Receiver-level T, method-level U.
export fn Wrapper[T] @map[U](f fn(T) -> U) -> Wrapper[U] {
    Wrapper[U].of(f(@inner))
}

// Call-site:
let w = Wrapper[int].of(5)
let a = w.map(|x| x * 2)              // (T=int, U=int) instance
let s = w.map(|x| str.from(x))        // (T=int, U=str) instance
let s2 = s.map(|x| x + "!")           // (T=str, U=str) instance
```

Compiler emits 3 distinct mono'd methods:
- `Wrapper____nova_int_method_map____nova_int`
- `Wrapper____nova_int_method_map____nova_str`
- `Wrapper____nova_str_method_map____nova_str`

**Параллель:** Rust `impl<T> Wrapper<T> { fn map<U>(self, f: impl Fn(T) -> U) -> Wrapper<U> }`
— то же monomorphization per `(T, U)`. C++ `template<T> class Wrapper {
template<U> Wrapper<U> map(...) }` — то же. Nova bootstrap теперь паритет.

### Decision tree

При codegen call'а `obj.method[U](args)`:

1. **Receiver T** — резолвится из obj C-type (`Nova_Wrapper____<T>*` →
   T = `<T>`). Существующая infrastructure (D72 + Plan 48 Ф.0).
2. **Method-level U** — резолвится через **bidirectional inference**
   из call args:
   - Non-closure args: `infer_expr_c_type(arg)` → bind U через
     `infer_type_param_binding`.
   - Closure-typed args (`|x| body`): pre-populate closure-param types
     с T-substituted C-types, recurse в body для return type → bind U.
3. **Method C-name** включает обa уровней: `<TypeBase>____<T>_method_<m>____<U>`.

### Constraints

- **Method-level generics declared в `@method[U]`** — synтаксис как у
  free-function generics (`fn name[U](...)`); receiver `[T]` parsed
  отдельно.
- **Closure args drive inference** — без explicit turbofish (`obj.map::<int>(...)`),
  U inferенtsя из closure return type. Если нет args или U не появляется
  в parameter types, compiler emit'ит clean diagnostic:

  ```
  cannot infer method-level type argument `U` for generic method
  `<TypeBase>____<T>.<method>` (only in return type — provide arg
  whose type binds it); provide a closure/arg whose type fixes `U`
  ```

  (См. реализацию в `compiler-codegen/src/codegen/emit_c.rs` path 5b.)
  Раньше unresolved method-level params silently dropped → `Nova_U_p`
  placeholder leak в emitted C → undefined-struct CC-FAIL.
- **Per-(T, U) instances** — каждая уникальная пара получает свою mono'd
  function. Worklist enrollment предотвращает дубликаты.
- **Return type substitution** — `Wrapper[U]` в return type корректно
  resolves в `Nova_Wrapper____<U>*` (не `Nova_U_p` placeholder).

### Почему

1. **Параллель Rust/C++** — индустриальный standard для generic methods.
2. **Zero-cost** — каждая mono-instance это direct call, инлайнится,
   no void* boxing/cast.
3. **Composability** — `w.map(f).map(g).filter(p)` typical functional
   chain работает без erasure penalty.
4. **Был CC-FAIL** — без method-param mono `let m = w.map(|x| str.from(x))`
   эмиттил `Nova_Wrapper____Nova_U_p* m = ...` (undefined struct, C-compile fail).

### Что отвергнуто

- **Method-level type-erasure (`void*` U)** — для bootstrap проще, но
  ломает первый-class closures + breaks struct-typed U (record-value
  не fit'ит в `void*` без heap-box). Equivalent проблема к Plan 48
  receiver-level erasure отвергнутой в V1.
- **Explicit-only U (`obj.map::<U>(...)` обязателен)** — verbose, не
  matches industry standard. Inference из args — first-class.

### Связь

- [D72](#d72-generic-bounds-через-t-protocol--protocol-как-тип) —
  generic bounds на type params; method-level U могут иметь bounds.
- [D122](#d122-hybrid-dispatch-для-bound-k-methods) — hybrid dispatch
  для protocol-bound type params; orthogonal к method-level vs receiver-level.
- [D123](#d123-tuple-monomorphization) — tuple mono пользуется тем же
  worklist infrastructure.
- [Plan 48 Ф.9](../../docs/plans/48-closures-in-generics.md#-9--method-param-mono)
  — реализация (emit_call path 5b + infer_mono_method_ret_with_args).
- [Plan 63 Fix C](../../docs/plans/63-cross-module-mono-dispatch-correctness.md#fix-c-mono-enrollment-для-anonymous-record-literal-в-generic-return)
  — remaining edge case Plan 63, закрытый этим D119.
- [Q-generic-receiver-method](../open-questions.md#q-generic-receiver-method)

---

## D125. Удаление `byte`: каноническое имя — `u8`

**Решение:** Тип `byte` удалён из языка. Единственное каноническое имя
для 8-битного беззнакового целого — `u8`. Срез байт пишется `[]u8`.

**Мотивация.** Наличие двух равнозначных имён (`byte` и `u8`) порождает
неоднозначность в коде, документации и стандартной библиотеке: один и тот же
тип можно было написать двумя способами, что усложняло чтение и тулинг.

**Миграция.** Все вхождения `byte` как типа заменяются на `u8`:
- `[]byte` → `[]u8`
- параметры/поля типа `byte` → `u8`
- в примитивном перечислении: `byte` убирается из списка

**Исключения (не меняются):**
- Тег шаблонных строк `` bytes`...` `` (D48) — это имя функции, не тип.
- Слово «byte» в английском/русском тексте комментариев (единицы памяти).

**Реализовано:** [Plan 69](../../docs/plans/69-byte-to-u8.md) — 2026-05-22.
`byte` удалён из builtin-типов компилятора (lexer/parser/type-checker/
codegen); все вхождения в `spec/` / `std/` / `nova_tests/` мигрированы
на `u8`. C-typedef `nova_byte` (= `uint8_t`) сохранён как внутреннее имя
codegen — не пользовательская поверхность.

---

## D126. Strict type propagation в codegen — no silent `nova_int` fallback

**Решение.** Codegen pass (`compiler-codegen/src/codegen/`) **обязан**
производить deterministic, явный C-type для каждого Nova expression и
type reference. **Silent fallback к `nova_int`** при failure type
resolution — **запрещён**. Любой site где `type_ref_to_c(...)`
возвращает `Err` без strict-error должен производить compile-time
diagnostic `[E7001]` и failing build, а не подставлять placeholder
type.

**Мотивация.** До Plan 70 паттерн `type_ref_to_c(&ty).unwrap_or_else(|_|
"nova_int".into())` встречался в codegen в 117 местах (audit 2026-05-18).
Семантика: «если type translation failed → silently emit nova_int
(`long long`) и продолжай». Результат — silent miscompilation:

- pointer cast to int → garbage address как число
- bool/char печатается как code-point (Plan 67 закрыл частный случай)
- record/sum-type memcpy с неправильным sizeof
- float → int truncation

Программа «работает», но возвращает мусор. Debug невозможен — компилятор
ничего не сигналит.

**Industry baseline.** Rust / Swift / Go (post-1.18) — все производят
compile error на любом unresolved type в codegen. Nova до Plan 70 был
**хуже всех baseline** (silent default). D126 закрывает регрессию.

**Категории erasure (Cat A/B/C/D).** Audit разделил 154 fallback sites
на четыре категории:

| Cat | Pattern | Семантика | Действие |
|---|---|---|---|
| **A1** | `type_ref_to_c(...).unwrap_or_else(\|_\| "nova_int")` | Silent fallback при resolution failure | **Strict error** |
| **A2** | `_ => "nova_int"` wildcard без комментария | Wildcard fallback unknown type | **Strict error или Cat D classification** |
| **B**  | `_ => "nova_int", // erased T` (commented) | Pre-mono generic body emit — type-param ещё unresolved | Documented intentional erasure |
| **C**  | `WithResultCategory::IntLike => "nova_int"` | Categorical mapping для int-family aliases | Legit, keep |
| **D**  | Dispatch wildcard на известный receiver | Known type, unknown method (type-checker уже rejected) | Legit, keep |

**Только Cat A** даёт silent miscompilation. После Plan 70 closure все
Cat A sites мигрированы к strict error path. Cat B/C/D documented
в [docs/codegen-erasure-sites.md](../../docs/codegen-erasure-sites.md).

**Strict-error architecture.** Две helper-функции в `emit_c.rs`:

1. `err_no_int_fallback(context, cause) → String` — для functions
   возвращающих `Result<_, String>`. Используется с `?` propagation:
   ```rust
   let ty = self.type_ref_to_c(&p.ty).map_err(|e|
       self.err_no_int_fallback("parameter `x`", &e)
   )?;
   ```

2. `record_strict_error(context, cause) → "nova_int"` — для
   **cascade-blocked** sites (functions whose signature нельзя менять
   без massive caller-chain refactor: `infer_expr_c_type` (135
   callers), `register_mono_instance`, etc). Pushes E7001 в
   `strict_errors: RefCell<Vec<String>>` field; finalization gate в
   `emit_module` проверяет non-empty и failit codegen pass с
   aggregated error message.

Оба helper'а используют unified diagnostic format `[E7001]` (range
E7001-E7099 reserved для Plan 70 family). Plan 36 R7 structured
diagnostic compatibility.

**Production-grade default.** Strict mode — **always on**, без opt-in
env var. ANY silent fallback = build failure (Rust/Swift baseline).
Это breaking change для user code который полагался на silent int
default (R20 в Plan 70). Bootstrap convention: clean break с
machine-applicable migration suggestions.

**Diagnostic format (E7001).**
```
[E7001] cannot infer C type for parameter `x`: <cause>. Silent
fallback к `nova_int` produced wrong runtime output для non-int
types (record/string/float/bool). Add explicit type annotation,
ensure generic is monomorphized, или register type в external_registry.
См. Plan 70 ([M-no-silent-nova-int-fallback]).
```

**Internal lint guard (CI).** `scripts/lint-no-silent-int-fallback.sh`
greps `compiler-codegen/src/` против baseline counts из
`docs/codegen-erasure-sites.md`. Bumping baseline требует:
1. Inline comment с rationale «почему erasure безопасна»
2. Entry в `docs/codegen-erasure-sites.md` со file:line + причина
3. PR review

CI gate fails если added counts превышают baseline без updates.

**Acceptance criteria (Plan 70 closure).**
- [x] Helper infra `err_no_int_fallback` + `record_strict_error` (Ф.1 / Ф.B0)
- [x] Cat A1/A2 migration: 90 → 8 (only Cat B holdovers remain)
- [x] Cat B documentation: 10 sites listed в codegen-erasure-sites.md
- [x] Internal lint guard `scripts/lint-no-silent-int-fallback.sh`
- [x] Spec D126 (этот блок)
- [x] 796+ PASS / 0 FAIL nova test (0 regressions vs baseline 761)

**Реализовано:** [Plan 70](../../docs/plans/70-no-silent-nova-int-fallback.md)
  — sessions 1+2 (2026-05-18); 90+ Cat A1 sites migrated, infrastructure
  complete, lint guard active.

**Связь:**
- D118 — typed `Fail[E]` codegen (similar precision-by-construction pattern)
- Plan 67 — println overload fix (sibling: один из видимых частных случаев)
- Plan 48 — monomorphization (упрощает Cat B → меньше erasure)
- Plan 36 — diagnostic infra (R7 structured format)
- [docs/codegen-erasure-sites.md](../../docs/codegen-erasure-sites.md) — Cat B/D inventory

---

## D128. `char` distinct from `int` в codegen mono'd generics

**Решение.** Тип `char` имеет собственный C-typedef `nova_char` (alias
над `int64_t`, same underlying storage как `nova_int`, но distinct C
identifier). Generic mono mangling использует `nova_char` separately от
`nova_int`, поэтому `Option[char]` и `Option[int]` производят разные
C-типы `NovaOpt_nova_char` vs `NovaOpt_nova_int` — структурно
неотличимы становятся **различимы**.

**Мотивация.** До Plan 70.3 оба `char` и `int` map'ились в один C-тип
`nova_int`. Результат — silent type collapse в generic mono:

- `Option[char]` и `Option[int]` mangle в идентичный `NovaOpt_nova_int`
- `[]char` и `[]int` обе → `NovaArray_nova_int*`
- `Map[char, V]` и `Map[int, V]` → одинаковая mangled name

Concrete observed bug (триггер плана): `str @char_at(idx int) -> Option[int]`
declared, returned `Option[char]` де-факто. Type-checker не ловил
поскольку C-level structural compatibility. ~50 callers использовали
char literals (`Some('/')`, `unwrap_or('.')`) в slot expecting
`Option[int]` — silent collapse через NovaOpt_nova_int. User pre-fix
2026-05-19 corrected signature, Plan 70.3 — архитектурное предотвращение.

**Industry baseline.** Rust/Swift `char` is distinct primitive (`char`
vs `u32`); Go has `rune` distinct from `int32`. Nova до Plan 70.3 был
unusual в C-level collapse. D128 закрывает регрессию.

**Implementation (Plan 70.3 Ф.1-Ф.2).**

1. **Typedef:** `typedef int64_t nova_char;` в `compiler-codegen/nova_rt/nova_rt.h`
   — zero ABI cost (same storage layout как `nova_int`).
2. **Codegen mapping:** `type_ref_to_c "char" => "nova_char"` (was
   `"nova_int"`) в `emit_c.rs` и `external_registry.rs` (двойная sync).
3. **Array element:** `[]char → NovaArray_nova_char*` (separate
   instantiation parallel `NovaArray_nova_int*`).
4. **Option element:** `NovaOpt_nova_char` typedef + constructors +
   `nova_opt_eq_nova_char` helper.
5. **CharLit emission:** `'x' → ((nova_char)<codepoint>LL)` (was `(nova_int)`).
6. **infer_expr_c_type:** `CharLit => "nova_char"` (was `"nova_int"`).
7. **Runtime fn signatures:** `nova_str_char_at` updated return
   `NovaOpt_nova_char` (was `NovaOpt_nova_int`).

**Backward compat.** В `emit_binary_op` special-case для
`Nova_StringBuilder* + char` accepts **обе** `nova_char` AND `nova_int`
для backward-compat — pre-fix existing code emitted char as `nova_int`,
existing test binaries reference legacy form. After full migration of
existing generated C (regen test fixtures), `nova_int` branch может
быть удалён.

**ABI cost.** Zero. `nova_char` is `typedef int64_t` — same size,
same alignment, same wire-format. Only difference — C type identifier
для compiler-level distinction.

**Acceptance criteria.**
- [x] Ф.1 codegen mapping switch (`emit_c.rs` + `external_registry.rs`)
- [x] Ф.2 runtime helpers parallel (`NovaArray_DECL(nova_char)`,
      `NovaOpt_nova_char` constructors + eq helper)
- [x] Ф.3 audit + fixtures (2 PASS в `nova_tests/plan70_3/`)
- [ ] Ф.4 type-checker tightening (reject `let x Option[int] = Some('a')`)
- [x] Ф.5 spec D128 (этот блок)
- [x] 0 regressions в `nova test` (801 PASS sustained)

**Реализовано:** [Plan 70.3](../../docs/plans/70.3-char-int-mono-distinction.md)
  — Ф.0-Ф.5 closed 2026-05-19.

**Связь:**
- D26 — Q-string-indexing (char = codepoint convention)
- D54 — `as`-cast narrowing (explicit char↔int conversion)
- Plan 70 — parent family (silent type bugs от Nova↔C collapse)
- Plan 70.4 — sibling proposal (f32/f64 generic-container distinct mangling)

---

## D129. `int` как alias `i64` в bootstrap Nova

**Решение.** Тип `int` в Nova bootstrap является **alias** для `i64`
(64-bit signed integer). Оба маппируются в C-тип `nova_int`
(`typedef int64_t`). Отсутствие distinction в codegen — **намеренно**:
это не collapse-баг (как в Plan 70.3 `char/int`), а архитектурный
bootstrap-invariant.

**Мотивация.** Audit Plan 70.4 выявил, что `int` и `i64` используют
один C-тип. Mangle для `Map[int, V]` и `Map[i64, V]` идентичен. В
отличие от других collapse-паттернов Ф.1/Ф.2 плана 70.4 (ABI-real
silent miscompilation) или Plan 70.3 char/int (semantically distinct
types), `int` ≡ `i64` является семантическим инвариантом — оба
означают 64-bit signed integer без разницы в значении или поведении.
Nova bootstrap targets x86_64 only (fixed 64-bit pointer width).

**Industry baseline.**
- Rust: `isize` distinct от `i64` (platform-pointer width varies на 32-bit)
- Go: `int` distinct от `int64` (platform-pointer width)
- C#: `int` = alias `System.Int32` (semantically identical)
- Python/Java: нет fixed-width integer aliases
- **Nova:** `int` = alias `i64` — правильная аналогия C# для fixed-width platform

**Future evolution path.** Если Nova добавит multi-arch targets
(32-bit, WASM), `int` может стать platform-pointer-width type аналогично
Rust's `isize`. На этот момент потребуется breaking change в codegen
mangling — `Map[int, V]` и `Map[i64, V]` станут distinct. D129
explicitly documents текущее bootstrap decision как **alias-based**,
чтобы будущий architect не принял отсутствие distinction за bug.
Migration path: introduce `nova_iptr` (platform-width) typedef, make
`int` resolve to it, maintain `nova_int` = `int64_t` for `i64`.

**Codegen.** Без изменений. `type_ref_to_c "int" => "nova_int"` и
`"i64" => "nova_int"` — оба корректны и эквивалентны по спецификации.
Distinct mangling не вводится, т.к. это создало бы необходимость явно
выбирать `int` vs `i64` для каждого generic instantiation — user-hostile
и ортогонально семантической разнице (которой нет).

**Acceptance criteria.**
- [x] Ф.3 spec D129 (этот блок) — формализует alias decision
- [x] Нет codegen изменений — intentional collapse документирован
- [ ] Future: multi-arch migration path зафиксирован (Migration note выше)

**Реализовано:** [Plan 70.4](../../docs/plans/70.4-primitive-type-distinction-complete.md)
  — Ф.3 closed 2026-05-19.

**Связь:**
- D54 — `as`-cast narrowing semantics
- D128 — Plan 70.3 char/int distinction (contrast: там distinction нужна)
- Plan 70.4 — parent plan (этот блок = Plan 70.4 Ф.3)
- Plan 70 — parent family (silent type bugs)

---

## D130. `uint` — unsigned 64-bit alias в bootstrap Nova

**Решение.** Тип `uint` является **alias** для `u64` (64-bit unsigned
integer) в Nova bootstrap. Маппируется в C-тип `uint64_t`. Отличие
от `int`/`i64` (alias pair, signed) — `uint`/`u64` является
симметричным unsigned pair. `int as uint` cast **saturates** (negative → 0);
`int as u64` — direct bit-cast (существующее поведение сохранено).

**Дизайн (Q1-Q4, подтверждены 2026-05-19).**

| Вопрос | Решение | Обоснование |
|---|---|---|
| **Q1: alias или distinct?** | Alias `u64` (= `uint64_t`) | Mirror `int` = `i64` alias pattern; нет multi-arch story в bootstrap |
| **Q2: int→uint cast** | `as uint` saturates (neg → 0) | D54 precedent (float→int); Rust bit-cast hostile; Swift trap verbose |
| **Q3: Indexing** | Keep `int` (no change) | Breaking change для 100+ APIs; Swift/Go/Kotlin используют signed indexing |
| **Q4: Literal default** | `int` (keep current) | Backward compat; `42 as uint` или `let x uint = 42` для opt-in |

**Saturation semantics (`int as uint`).**
```
 -1000 as uint → 0
    -1 as uint → 0
     0 as uint → 0
     1 as uint → 1
```
Реализован через `nova_int_to_uint(int64_t x)` helper в `nova_rt/cast.h`.
`u64 as uint` — direct cast (no-op; `uint64_t → uint64_t`).

**Codegen mapping.**
- `type_ref_to_c "uint" => "uint64_t"` (scalar)
- `[]uint → NovaArray_uint64_t*` (parallel с `u64`)
- `Option[uint] → NovaOpt_uint64_t` (parallel с `u64`)
- `uint.MAX` — **не поддержан** parser'ом (parser не распознаёт
  `uint` как type-path prefix; используй `u64.MAX` = эквивалент).

**Будущая эволюция.** Аналогично D129 (int/i64): если Nova добавит
multi-arch, `uint` может стать platform-pointer-width unsigned (как
Rust's `usize`). Bootstrap-grade alias.

**Acceptance criteria.**
- [x] `let x uint = 42 as uint` компилируется
- [x] `int as uint` saturates (neg → 0) — `nova_int_to_uint` helper
- [x] `int as u64` остаётся bit-cast (no saturation)
- [x] `[]uint` → `NovaArray_uint64_t*`
- [x] `Option[uint]` → `NovaOpt_uint64_t`
- [x] 3 fixtures `nova_tests/plan70_5/` PASS
- [x] 0 regressions
- [ ] `uint.MAX` — defer (parser keyword support)

**Реализовано:** [Plan 70.5](../../docs/plans/70.5-uint-primitive-symmetry.md)
  — Ф.1-Ф.3 closed 2026-05-19.

**Связь:**
- D54 — `as`-cast saturation precedent
- D129 — int/i64 alias (signed symmetric pair)
- Plan 07 — original float→int saturation
- Plan 70.5 — parent plan (этот блок)
- Plan 70.4 — sibling (codegen type distinction family)

---

## D133. `type X consume` — обязательная consume-семантика (must-be-consumed)

> **Plan 100.1.** Принято 2026-05-23 (proposed; implementation pending).
> Extends [D131](05-memory.md#d131) affine `consume` qualifier.

### Что

Квалификатор `consume` на **type-decl**. Помечает, что инстансы такого
типа **обязаны** быть потреблены до выхода из scope'а на каждом code-
path'е. Compile error если live consume-переменная остаётся на exit-
point'е.

```nova
type Transaction consume { id int }
type File consume { fd i32 }
type Lock consume { mutex *Mutex }
```

Расширяет [D131](05-memory.md#d131) с противоположной стороны:

| Свойство | D131 affine `consume` (Plan 73) | D133 type-level `consume` (Plan 100.1) |
|---|---|---|
| Потребить ≤1 раз | ✅ enforce | ✅ enforce (наследуется) |
| Потребить ≥1 раз (обязательно) | ❌ забыть OK | ✅ enforce — must-be-consumed |
| Помечается на | receiver / param метода | **type-decl** + поле + binding |

Канонический use-case — `Transaction.@commit() / .@rollback()`,
`File.@close()`, lock-guard `.@release()`.

### Синтаксис

`consume` стоит **после имени типа**, перед `{`:

```nova
type Transaction consume {                    // type-decl marker
    id int,
}

fn Transaction consume @commit() -> ()         // consume-method (D131)
fn Transaction consume @rollback() -> ()
```

`consume` на type-decl + хотя бы один consume-метод (D131) — обязательное
сочетание (compile error: «consume-type требует ≥1 consume-method»).

### Правило — must-consume на каждом exit-path'е

Compiler проводит **flow-sensitive** анализ (расширение Plan 73 D131
`check_consume` pass'а). Для каждой переменной consume-типа отслеживается
`VarState`:

- **`Live`** — значение доступно, обязательство активно.
- **`Consumed`** — значение потреблено (через consume-метод / consume-
  параметр / `return`).
- **`MaybeConsumed`** — потреблено лишь на части путей (branch join).

На каждой **точке выхода** scope'а проход по active consume-переменным:
- `Live` или `MaybeConsumed` → **compile error E (D133-not-consumed)**
  с указанием консьюм-методов.
- `Consumed` → OK.

Точки выхода:
- конец function body (последний statement);
- `return expr` — все live consume-vars (кроме возвращаемой) → error;
- `panic` / `expr!!` / `expr?` / unwinding-paths;
- `loop break`;
- branch join `if`/`match` — `Live ⊔ Consumed = MaybeConsumed`.

`defer` / `errdefer` могут покрывать обязательство (см. **D158+** Plan
100.4 family).

### Что считается consume

| Действие | Эффект на VarState |
|---|---|
| `tx.commit()` — вызов consume-метода | `tx` → `Consumed` |
| `f(tx)` где `f(consume tx Tx)` | `tx` → `Consumed` |
| `return tx` (тип consume) | `tx` → `Returned` (передача caller'у) |
| `record.field = tx` где field declared consume | `tx` → `Moved` (в record) |
| `f(tx)` где `f` без `consume` qualifier | ❌ compile error E (D133-move-to-non-consume) |
| `let _ = tx` (silent drop) | ❌ compile error (suppress not allowed) |

### Заразность через поля + explicit double-marker

Record/sum, имеющий поле consume-типа, **обязан** быть объявлен
`consume`:

```nova
type TxState consume {                         // ← ОБЯЗАТЕЛЬНО
    consume tx Transaction,                    // ← ОБЯЗАТЕЛЬНО (тип = consume)
    writes []Write,                            // обычное поле
}
```

Compiler enforces consistency:
- consume-поле без `consume`-маркера → error E (D133-field-marker-missing);
- consume-маркер на field без `consume` на type-decl → error
  E (D133-type-marker-missing);
- `consume` на type-decl без ≥1 consume-поля и без consume-методов →
  error E (D133-empty-consume).

### Field-aware flow внутри методов record'а

`@field` отслеживается как независимый VarState slot. На exit'е метода:

| Тип метода | consume-поля должны быть |
|---|---|
| `fn X consume @method(...)` | `Consumed` (record closes) |
| `fn X mut @method(...)` | **`Live`** (invariant preserved) |
| `fn X @method(...)` (regular) | **`Live`** (invariant preserved) |

Это позволяет реальные паттерны (rotate / reopen / replace):

```nova
type Service consume {
    consume file File,
}

fn Service mut @reopen() -> Result[(), OpenErr] {
    let new_file = File.open()?                // сначала добываем замену
    @file.close()                               // только теперь закрываем старое
    @file = new_file                            // rebind — @file опять Live
}                                               // mut exit: @file Live ✅
```

Compiler ловит реальные баги:
- забытый rebind на ветке → exit MaybeConsumed → error.
- early return без rebind → error.
- наивный close-then-open с error-path (`@file.close(); @file = open()?`)
  → error если open Err (@file Consumed, не rebinded).

### Assign в Live consume-поле / locals — запрещено

Прямое присваивание `@field = expr` разрешено **только** когда `@field`
уже `Consumed`. Иначе compile error E (D133-assign-live-field) с
suggestion «consume the existing value first via `<consume-method>()`».
Защита от silent overwrite старого consume-значения.

```nova
fn Service mut @overwrite_naive() {
    @file = File.open()?                       // ❌ @file Live, silent overwrite
}

fn Service mut @overwrite_correct() {
    @file.close()
    @file = File.open()?                       // ✅ @file Consumed → assign OK
}
```

То же для локальных consume-var: повторный `consume tx = ...` без
consume старой — error. Re-binding через shadow — отдельный случай
(Plan 73 alias-tracking).

### Nested field paths

Multi-level field tracking — `ConsumeCtx` хранит state по произвольно
глубокому пути `@.f1.f2.f3`:

```nova
type Inner consume { consume tx Transaction }
type Outer consume { consume inner Inner }

fn Outer mut @commit_inner() {
    @inner.tx.commit()                         // deep path consume; @.inner.tx → Consumed
    @inner = Inner.new()                       // rebind inner
}
```

Реализация — `ConsumeCtx::states: HashMap<FieldPath, VarState>` где
`FieldPath = Vec<String>`.

### Заразность через generic-args

`type_is_consume(TypeRef)` — рекурсивная функция (общая, не Option-
специфичная):

- тип в `LinearityRegistry` (объявлен `consume`)?
- record/sum с ≥1 consume-полем?
- generic-wrap `G[T1, ..., Tn]` — хотя бы один `Ti` consume?
- generic-param `T` (без bound) — false (bootstrap silent-ignore;
  закрывается **D156** Plan 100.2 через `[T consume]` bound).

`Option[Transaction]` / `Result[Transaction, E]` / `Box[Transaction]` /
user `Wrapper[Transaction]` — все автоматически consume через wrap.
**Никакого Option-специфичного хардкода** — общее правило для любого
generic-wrapper'а.

### Read-only access (non-consume параметр)

```nova
fn print_id(tx Transaction) {                  // без `consume` — read-only
    println(tx.id)                              // ✅ чтение поля
    tx.commit()                                 // ❌ consume-метод — error
    finish(tx)                                  // ❌ передача в consume-param — error
    storage.last = tx                           // ❌ store-в-поле — error
    return tx                                   // ❌ возврат — error (не владеешь)
    let f = || tx.commit()                      // ❌ closure capture с consume — error (D157)
    let local = tx                              // ✅ alias (Plan 73 alias-tracking)
}
```

Правило: read-only param = только чтение + alias. Identity-функция
требует явного `consume tx Transaction` параметра.

Глубокий peek (`match @file { Some(f) => f.id, ... }` для consume-
Option) — невозможен в D133; закрывается **D157** Plan 100.3 через
`view T`.

### `consume` + `-> @` несовместимы

`fn Tx consume @prepare() -> @ { ... }` → **parse error**. Противоречие
между «забираю целиком» и «возвращаю тот же объект» (D132 fluent-
return).

### Binding-level: `consume tx` vs `let tx`

Две формы привязки для consume-типов (strict, без or-or):

```nova
consume tx = begin()                           // strict: ОБЯЗАН закрыть здесь
                                               //          (return / в record-вверх — error)

let tx = begin()                               // strict: ОБЯЗАН передать наверх
                                               //          (return / consume-param / в record-вверх).
                                               //          Закрывать локально — error.
```

Декларация intent'а с compiler-checked гарантией.

### Runtime mental model (Option-projection, не ABI)

Концептуально consume-тип проецируется в `Option[T]`-space:
- `Live` ≡ `Some(t)`.
- `Consumed` ≡ `None`.
- `MaybeConsumed` ≡ branch-зависимо.

Это **mental model** для spec/docs. **Реализация остаётся pragmatic**
(D131-style):
- pointer-based consume: NULL = None (zero overhead);
- value consume: zero-out fields после consume;
- compile-time `check_consume` — основной механизм; runtime null-deref
  panic — defense-in-depth.

User-facing pattern-match `match tx { Some(t) => ... }` для runtime-
проверки **не вводится** — ослабит compile-time гарантии.

### Что отвергнуто

- **Universal affine/linear для всех `let`** — отвергнуто в [D75
  §«Compile-time token-scope enforcement»](06-concurrency.md#d75): «это
  Rust borrow checker ради одной фичи, несоразмерно для GC-языка».
  D133 — opt-in per-type, не default.
- **Suppress-механизм `let _ = v`** — anti-Rust `#[must_use]` gateway.
  Единственный канал — consume-метод. Если «иногда хочу забыть» — знак,
  что тип неправильно помечен `consume`.
- **Drop-method auto-cleanup** (Rust-style RAII) — размывает выбор
  commit/rollback. D133 требует **явный** consume-метод.
- **Pattern-match destructure consume-record** (`let { tx } = state`)
  — ломает encapsulation (consume-поле уходит в независимый linear-
  binding). Вынос через явный consume-метод record'а: `fn TxState
  consume @into_parts() -> (Transaction, []Write) => (@tx, @writes)`.

### Сравнение с другими языками

| Свойство | Rust | TS (ES2024) | Kotlin | Go | Nova D133 |
|---|---|---|---|---|---|
| Compile-time enforcement | ⚠️ `#[must_use]` warning, suppressable | ❌ runtime via dispose | ❌ runtime via `use{}` | ❌ | ✅ **error** |
| Suppress escape hatch | ✅ `mem::forget(v)` / `let _ = v` | n/a | n/a | n/a | ❌ **by design** |
| Distinct cleanup methods (commit/rollback) | ⚠️ enum-в-Drop, awkward | ⚠️ single `dispose` | ⚠️ `use{}` block | ⚠️ convention | ✅ **native** (consume-методы) |
| Lifetime / borrow-checker cost | ❌ есть | n/a | n/a | n/a | ✅ нет (поверх GC) |

D133 строже Rust на suppress (нет `mem::forget`), expressive Rust на
distinct cleanup methods. Не требует lifetime'ов / move-семантики.

### Связь

- [D131](05-memory.md#d131) — affine `consume` foundation. D133 —
  extension on type-decl level.
- [D132](03-syntax.md#d132) — `-> @` fluent-return; sound builder-chain
  alias через `-> @` нужен для consume-checker'а builder API.
- [D75](06-concurrency.md#d75) — почему universal consume отвергнут.
- [D90](03-syntax.md#d90) — `defer` / `errdefer` foundation; интеграция
  через Plan 100.4 family (D158-D162).
- [D85](04-effects.md#d85) — kinded throws, cancel-routing;
  взаимодействие через D162 Plan 100.4.5.
- D156 Plan 100.2 — generic `[T consume]` strict-mode bound.
- D157 Plan 100.3 — `view T` read-only borrow для deep peek.
- D158-D162 Plan 100.4.1-5 — defer/errdefer integration для cleanup-
  on-failure.
- D163 Plan 100.5 — FFI `external consume fn`.
- D164 Plan 100.6 — cross-module consume visibility + mangling.
- D165 Plan 100.7 — stdlib migration playbook.
- D166 Plan 100.8 — performance + IDE tooling.

---

## D156. Generic `[T consume]` bound + collection-aware iteration

> **Plan 100.2.** Принято 2026-05-23 (proposed; implementation pending).
> Extends [D133](#d133) на generic-код. Closes silent-leak hole для
> consume-T в generic-функциях.

### Что

Bound `[T consume]` на generic-параметр — opt-in **strict mode**: внутри
generic-body параметр `T` трактуется как possibly-consume; silent-forget
T-значения → compile error. Backward-compat: generic-функции **без**
bound сохраняют silent-ignore behavior (Plan 100.1 default), чтобы
existing stdlib generic-код продолжал работать.

```nova
// Strict mode — compiler enforces strict consume handling внутри:
fn box[T consume](consume x T) -> Box[T] => Box { val: x }

// Без bound — silent-ignore:
fn drop[T](x T) -> ()                          // silent forget если T consume
```

Плюс — **collection-aware iteration**: `for tx in vec { ... }` где
`vec []Transaction` consume'ит каждый element в arm-теле.

### Зачем

Без D156 generic-код имеет дыру:

```nova
type Transaction consume { id int }
fn Transaction consume @commit() -> ()

fn first[T](pair (T, T)) -> T => pair.0       // silent leak pair.1 если T=consume

consume tx1 = Transaction { id: 1 }
consume tx2 = Transaction { id: 2 }
consume chosen = first((tx1, tx2))             // tx2 уехала в first и потерялась
chosen.commit()
// tx2 LEAK — compiler молчит.
```

Это самый серьёзный hole D133 bootstrap'а — именно generic-helpers есть
в каждой stdlib. Rust решает через `Move` trait + ownership; D156 решает
через **`[T consume]` bound** + collection-aware iteration.

### Синтаксис bound

```nova
fn box[T consume](consume x T) -> Box[T]
fn map[T consume, U consume](items []T, f fn(consume T) -> U) -> []U
fn id[T consume](consume x T) -> T => x
```

`consume` — bound в generic-position, мирится с другими bounds (`[T Iter[U]]`
из D72) — но **bootstrap не поддерживает комбинации** (`[T consume +
Clone]` — parse error; будущее расширение).

### Strict mode внутри `[T consume]` body

Внутри функции с `[T consume]` bound параметр `T` трактуется как
possibly-consume; compiler обращается строго:

| Действие с T-значением | Без bound | С `[T consume]` |
|---|---|---|
| `let _ = x` (silent drop) | ✅ OK | ❌ error E (D156-strict-forget) |
| передача в non-consume fn | ⚠️ silently | ❌ error |
| destructure tuple, discard part | ⚠️ silently | ❌ error |
| `return x` | ✅ | ✅ (передача наверх) |
| передача в `consume` fn-param | ✅ | ✅ (consume) |

Force'ит honest API. Чтобы legitimately drop элемент — нужен явный
`consume`-параметр для drop:

```nova
fn first[T consume](consume a T, consume drop_b T) -> T => a
//                              ^^^^^^^^^^^^^^^^^^ — caller обязан передать
//                                                   drop_b как consume; внутри
//                                                   first drop_b силен забыть
//                                                   (это локальный binding).
```

### Backward-compat и migration policy

- **Default = silent-ignore** для generic-functions без bound (Plan
  100.1 behavior preserved). Иначе сломается весь stdlib generic-код.
- **Opt-in `[T consume]`** для функций, которые хотят strict mode.
- **Migration:** stdlib generic-functions (Plan 17/26/30/52/57
  collection API) — постепенно аннотируются `[T consume]` через `nova
  consume-migrate` CLI (Plan 100.7).

### Collection-aware iteration

`for x in []T` где `type_is_consume(T)` — каждый `x` в arm'е считается
Live linear, обязан Consumed/Returned в arm-теле:

```nova
consume tx1 = begin()
consume tx2 = begin()
consume tx3 = begin()
let txs = [tx1, tx2, tx3]                      // []Transaction — generic-заразность (D133 D6)

for tx in txs {
    tx.commit()                                // каждый element consume'ится ✅
}
// vec считается Consumed после for ✅
```

Loop-handling pragmatic: после `for`-block весь vec considered Consumed
(даже если break early). Каждый `tx` в теле проверяется стандартным
`check_consume`.

### Generic propagation для HOF (map/filter/fold)

```nova
fn map[T consume, U consume](items []T, f fn(consume T) -> U) -> []U
fn filter[T consume](items []T, f fn(view T) -> bool) -> []T
fn fold[T consume, U consume](items []T, init U, f fn(consume U, consume T) -> U) -> U
```

Все три требуют `[T consume]` (и `[U consume]` где нужно). Compiler
enforces consume-handling в `f` body через generic-bound propagation.
`filter` использует `view T` (D157 Plan 100.3) для read-only inspection.

### HashMap / user-generic propagation

`type_is_consume` рекурсивно (D133 D6): wrapper'ы с consume-arg сами
становятся consume:

```nova
let mut tx_map HashMap[str, Transaction] = HashMap.new()
                                               // ↑ Transaction consume → HashMap consume
                                               //   через generic-заразность
tx_map.insert("a", begin())                    // V value insert; HashMap инкапсулирует
// На scope-exit tx_map должен быть Consumed (через consume-метод HashMap).
```

HashMap (и другие collection API) — должны аннотировать `[V consume]`
на методах, манипулирующих consume-values (`insert`, `remove`, `get`).
Migration audit — часть Plan 100.7.

### Runtime cost

**Zero.** Все проверки compile-time. Runtime-представление generic'ов
не меняется. Bound `[T consume]` — type-level only, не влияет на
codegen mono'd functions.

### Сравнение

| Capability | Go | Rust | TS | Kotlin | Nova D156 |
|---|---|---|---|---|---|
| Generic linear bound | n/a | ✅ `T: Move` (default) | n/a | n/a | ✅ **`[T consume]`** opt-in |
| Detection «generic drops linear arg» | n/a | ✅ compile-error | n/a | n/a | ✅ |
| Backward-compat: generic без bound | n/a | n/a | n/a | n/a | ✅ **silent-ignore остаётся** |
| `Vec<T>` ownership iteration | n/a | ✅ | n/a | n/a | ✅ `for tx in vec` |

Nova **превосходит Rust** на одной оси — backward-compat: generic
без bound сохраняет existing behavior; opt-in strict — choice.

### Что отвергнуто

- **`[T consume + Clone]` combined bound** — bootstrap parse-error;
  будущее расширение (комбинация с другими D72 bounds).
- **`[T !consume]` anti-bound** — не вводится; нет use-case в
  bootstrap.
- **Variance** linear-typed wrappers — отдельный план (общая variance
  system).

### Связь

- [D133](#d133) — foundation type-level consume; D156 — generic-уровень.
- [D72](#d72) — generic bounds `[T Protocol]`; D156 идиоматически близок.
- [D157](05-memory.md#d157) — `view T` (Plan 100.3); `filter`-style HOF
  использует view для read-only inspection.
- D158-D162 (Plan 100.4 family) — defer/errdefer integration; orthogonal.

---

## D163. `external consume fn` — FFI consume через C-границу

> **Plan 100.5.** Принято 2026-05-23 (proposed). Extends [D82](08-runtime.md#d82)
> `external fn` + [D126](03-syntax.md#d126) `external type` + [D63](04-effects.md#d63)
> capability.

### Что

Маркер `consume` на `external fn` declaration — consume-семантика
пересекает C-границу. Поддерживает оба направления: **return-side**
(C-функция возвращает consume-obligation) и **param-side** (C-функция
consume'ит передаваемый ресурс).

```nova
// Return-side: caller-owns consume-obligation
external consume fn nova_file_open(path str) -> File
    needs Fs                                    // capability required (D163 D3)

// Param-side: callee-consumes
external fn nova_file_close(consume f File) -> ()

// Combined в opaque types
external type File consume                     // D126 opaque + D133 consume
external type Mutex consume
external type Socket consume
```

### Зачем

Без D163 существующие `external fn` не интегрированы с consume-
семантикой (Plan 100.1 D133). Plan 18 stdlib (File, Mutex, Socket,
Connection) не может быть consume-типами — leaks остаются runtime-only
checks.

### Capability requirement (D3)

`external consume fn` **обязан** declare capability:

```nova
external consume fn nova_file_open(path str) -> File
    needs Fs                                    // ОБЯЗАТЕЛЬНО

external consume fn nova_socket_accept(srv ServerSocket) -> ClientSocket
    needs Net
```

Marker `external consume` = «touch raw OS resource + carry consume
obligation» — combine'ит две responsibility'и в одной декларации.
Если не declare cap → compile error E (D163-missing-cap).

### C runtime defensive helpers

C-side `nova_file_close(consume f File)` обязан:
- `nv_consume_validate(f)` — assert `f != NULL` на entry.
- После работы — `memset` поля `File*` в zero / NULL (defense-in-depth
  per D131 Plan 73 pattern).

Это даёт двойную защиту: compile-time (D133 check_consume) + runtime
(NULL-deref panic на use-after-consume).

### Generic-заразность через FFI

`Result[File, IoErr]` / `Option[Mutex]` / etc. — generic-заразность из
D133 D6 работает уравномерно: если return-type wraps consume-type через
generic, wrapper consume.

```nova
external consume fn nova_open() -> Result[File, IoErr]
//                                ^^^^^^^^^^^^^^^^^^^ — Result consume
//                                                       через generic-arg.
// Caller обязан consume Result (через match Ok-arm с consume File).
```

### Vacuous marker — warning

```nova
external consume fn nova_get_pid() -> int       // ❌ W (D163-vacuous-consume)
//                                    ^^^ — int не consume, нет param consume
```

Force'ит honesty о contract'е.

### Cross-fiber FFI safety

FFI-call может суспендиться (libuv async I/O). Plan 47/22/49 fiber infra
preserves consume-state через migration; D163 verify через runtime tests
(Plan 100.5 Ф.6).

### Сравнение

| Capability | Rust | Kotlin/JNI | Go cgo | TS Node N-API | Nova D163 |
|---|---|---|---|---|---|
| Ownership через FFI | ✅ `unsafe fn` + manual | ⚠️ manual | ⚠️ manual | ⚠️ manual | ✅ **declaration native** |
| Auto-close на panic при FFI handle | ✅ через Drop wrapper | ⚠️ try-finally | ⚠️ defer | ⚠️ try-finally | ✅ **через D162** |
| Capability tracking | ⚠️ `unsafe fn` | ⚠️ manual | ⚠️ manual | n/a | ✅ **D63 native** |
| `unsafe` keyword нужен | ✅ да | n/a | n/a | n/a | ❌ **нет** (D6) |

Nova **превосходит Rust** — нет `unsafe` keyword (D6 «no unsafe» +
capability tracking через D63).

### Связь

- [D82](08-runtime.md#d82) — `external fn` foundation; D163 расширяет.
- [D126](03-syntax.md#d126) — `external type` opaque; combine'ится с
  `consume`.
- [D63](04-effects.md#d63), [D64](04-effects.md#d64) — capability
  enforcement.
- [D131](#d131-через-link), [D133](#d133) — consume foundation.
- [Plan 18](../../docs/plans/18-stdlib-roadmap.md) — основной consumer
  (File/Mutex/Socket migration).

---

## D164. Cross-module consume — visibility + mangling + package contracts

> **Plan 100.6.** Принято 2026-05-23 (proposed). Extends [D26](07-modules.md#d26)
> visibility + [D134](07-modules.md#d134) mangling v0 + Plan 03 package
> ecosystem.

### Что

consume-маркер (D133) — **part of exported type signature**. Visibility
(D26, D47 Plan 35 R26) propagates marker. Symbol mangling (extends
D134 Plan 81) включает **consume-bit** — ловит cross-version ABI break.
Plan 03 `nova audit` verifies cross-package consume-contracts.

### Cross-package visibility

```nova
// package A, module a/types.nv
export type Transaction consume {
    id int,
}
```

```nova
// package B, module b/main.nv
import a.types.Transaction

fn main() {
    consume tx = Transaction { id: 1 }          // ✅ consume-marker visible
    tx.commit()
}
```

`consume` propagates через `export` + `import`. Plan 35 R26 (visibility
enforcement) — без special-case'ов; consume — обычный type-attribute.

### Mangling extension (D134 amend)

Plan 81 D134 определил symbol-mangling v0:
```
nova_fn_<pkg>_<mod>_<name>_<param-types>_<return-type>
```

D164 amend:
```
nova_fn_<pkg>_<mod>_<name>_<consume-bit>_<param-types>_<return-type>
                          ^^^^^^^^^^^^^^^
                          `c` если consume-маркер на type-decl, `_` иначе
```

Это ловит ABI mismatch — package A v1.0 имеет `Transaction consume`,
v2.0 убрал marker; linker ловит cross-version mismatch на load.

### Re-export через `export import` (Plan 42.09)

```nova
// package B re-exports A.Transaction
export import a.types.{Transaction}
```

Re-export **preserves** consume-marker. Plan 42.09 уже работает; D164
verifies.

### Folder-modules (Plan 42) + relative imports (Plan 84)

consume-types работают идентично в folder-modules + relative imports:
не вводятся special-case rules. Plan 42 / Plan 84 уже работают; D164
verifies.

### Package version contracts (Plan 03)

`nova.toml` consume-contracts:

```toml
[package]
name = "my_lib"
version = "1.0.0"

[exports.consume_types]
Transaction = "1.0"                             // consume contract v1
File = "1.0"
```

Cross-version compat:
- v1.0 → v1.x — consume-status unchanged.
- v1.x → v2.0 — consume-status может change (major-bump required).

`nova audit` (Plan 03.4) verifies — ловит «v1 → v1.1 breaking change»
unauthorized.

### Cross-module diagnostic

```
error: consume value `tx` (type a::Transaction) not consumed
  note: type defined in package 'a' v1.0 at a/types.nv:5
  note: consume via .commit() or .rollback() (declared in 'a')
```

Includes package origin, version, consume-method hint.

### Private consume не leak

```nova
type InternalCache consume { ... }              // no `export`
// usable только в этом package; cross-package — invisible
```

Plan 35 R26 — без special-case'ов.

### Сравнение

| Capability | Rust | Kotlin/Java | Go | TS | Nova D164 |
|---|---|---|---|---|---|
| Pub visibility consume-маркера | ✅ pub Drop visible | ⚠️ AutoCloseable interface | ⚠️ exported method | ⚠️ TS types | ✅ **D164 propagation** |
| ABI mangling включает ownership-info | ✅ через type | ⚠️ via signature | ❌ | n/a | ✅ **consume-bit** |
| Cross-package consume contracts | ✅ Cargo + Rust types | ⚠️ Maven coordinates | ⚠️ go modules | ⚠️ npm types | ✅ **`nova.toml`** |
| Re-export preserves marker | ✅ через `pub use` | n/a | n/a | n/a | ✅ Plan 42.09 |

Nova **matches Rust** на всех осях; **превосходит** на consume-bit-in-
mangling (ловит silent ABI mismatch которого Rust не видит через
type-id alone).

### Связь

- [D26](07-modules.md#d26), [D47](07-modules.md#d47), Plan 35 R26 —
  visibility foundation.
- [D134](07-modules.md#d134) — mangling v0 (Plan 81); D164 extends.
- [D29](07-modules.md#d29) — modules + folder-modules.
- [D126](03-syntax.md#d126) — opaque types; cross-package consume может
  быть opaque.
- [D131](#d131-через-link), [D133](#d133) — consume foundation.
- Plan 03 / Plan 03.4 — package ecosystem, `nova audit`.
- Plan 42, Plan 42.09, Plan 84 — folder-modules, re-export, relative
  imports.

---

## D135. Type-checker completeness — «no silent fallback» на уровне типов

**Статус:** принято, реализовано ([Plan 79](../../docs/plans/79-typecheck-hardening-no-silent-fallback.md)).

**Контекст.** [D126](#d126) закрыл silent-fallback в *кодогене* («no
silent `nova_int`»). Но bootstrap type-checker (`types/mod.rs`) проверял
имена, структуру, эффекты, контракты — и **не** базовую совместимость
типов. Эмпирическая перепроверка 2026-05-21 показала: ряд элементарных
ошибок типов компилировался **молча** (silent miscompilation) либо
ловился только C-компилятором (CC-FAIL, поздняя нечитаемая диагностика):

| Случай | До Plan 79 | Severity |
|--------|-----------|----------|
| `let x int = true` | компилируется И выполняется неверно | 🔴 silent |
| `want_bool(42)` (int в bool-параметр) | то же | 🔴 silent |
| `fn g() -> Result[int]` (1 type-arg вместо 2) | компилируется тихо | 🔴 silent |
| `let c = Foo` (имя типа как значение) | CC-FAIL | 🟡 поздняя |
| `f.nonexistent` (нет поля) | CC-FAIL | 🟡 поздняя |

Go / Rust / TS ловят все пять на compile-time. По базовой проверке
типов Nova была позади всех трёх.

**Решение.** Type-checker обязан ловить базовые ошибки типов **на этапе
компиляции** собственной диагностикой (серия **E73xx**), а не молча и не
перекладывая на C-компилятор. Отдельный проход `TypeCheckCtx` (паттерн
`NameResCtx` / `MapLitCtx`):

- **E7310 — арность type-аргументов.** Использование generic-типа с
  явно указанным, но неверным числом аргументов (`Result[int]`,
  `Result[A,B,C]`, `Foo[int]` для не-generic `Foo`). Опущенные
  аргументы (`fn f() -> Result { Ok(1) }`) — легальны (выводятся из
  контекста), это **не** arity-ошибка.
- **E7301 — assignability.** `let`-аннотация ↔ RHS и аргумент ↔
  параметр. Целочисленный литерал полиморфен ([D44](03-syntax.md#d44)):
  `let x u8 = 200` валиден; `let x int = true`, `want_bool(42)` — нет.
  Сравнение по категориям типов; structural-конформность протоколов —
  забота [D72](#d72), не этой проверки.
- **E7320 — существование поля / метода.** `obj.name`, где `obj` —
  concrete record: `name` обязан быть полем либо методом (`into`/
  `try_into` синтезируются из [D73](08-runtime.md#d73)/[D77](08-runtime.md#d77)).
- **E7330 — type-vs-value.** Имя непустого record/sum-типа в
  value-позиции (`let c = Foo`, `Foo + 1`) — ошибка: тип не значение.

**Принцип «no any-hole» (строже TS).** Ни один путь проверки не
присваивает выражению результат «молча неверно». Там, где тип
выражения **не выводится** (bootstrap type-checker по дизайну не
типизирует каждое выражение — вывод завершается в кодогене), проверка
**пропускается локально** — это не silent miscompilation: программа не
становится неверной, недостающая проверка либо ловится дальше по
пайплайну, либо случай корректен. `any` — только из явной аннотации
(`[]any`), он не «заражает» и не отключает проверку соседних выражений.
Полная типизация каждого выражения на уровне type-checker'а — задача
пост-bootstrap full inference engine, вне scope Plan 79.

**Сравнение.** Go/Rust/TS ловят все пять случаев на compile-time;
Plan 79 выводит Nova на их уровень для перечисленных проверок. Строже
TS: у TS `any` молча гасит ошибки — в Nova такого пути нет.

**Связь:**
- [D126](#d126) — sibling: «no silent fallback» для кодогена (Plan 70).
- [D44](03-syntax.md#d44) — полиморфизм числовых литералов.
- [D72](#d72) — structural bounds (конформность протоколов — там).
- [D73](08-runtime.md#d73) / [D77](08-runtime.md#d77) — `into`/`try_into` синтез.
- Plan 79 — родительский план (этот блок).
- Plan 37 — newtype/alias `as`-cast строгость (смежная, отдельная).

---

## D142. protocol/effect declaration ↔ literal symmetry

> **Plan 97.** Принято 2026-05-23. Объединяет `Q-keyword-symmetry`
> (`open-questions.md`) с `Q-static-method-protocol` (D58).

### Что

Декларация и литерал и для **протоколов**, и для **эффектов** —
**симметричны** по ключевым словам:

```nova
// Declaration:
type Cron effect   { run() -> () }
type Fan  protocol { run() -> () }

// Literal (значение, реализующее контракт):
let h = effect   Cron { run() => spawn_cron() }   // value of type Effect[Cron]
let p = protocol Fan  { run() => spin_blades() }  // value реализующее Fan
```

Раньше литерал эффекта писался ключевым словом `handler`, а
литерала протокола **не было**. Теперь:

- литерал эффекта — `effect X { ... }` (тот же keyword, что в
  declaration);
- литерал протокола — `protocol X { ... }` (тот же keyword, что в
  declaration);
- встроенный тип `Handler[E, IRT]` → **`Effect[E, IRT]`**
  (`Effect[E]` ≡ `Effect[E, Never]` через [D88](03-syntax.md#d88)
  default).

**Clean break** — старое ключевое слово `handler` (литерал) **удалено**
без `deprecated`-алиаса; парсер при встрече выдаёт diagnostic
«`handler` keyword removed; use `effect` (D142)».

### Правило

#### Декларация (без изменений)

```nova
type Db   effect   { query(q str) -> [str] }
type Hash protocol { hash() -> u64 }
```

#### Литерал — symmetry

```nova
// effect-литерал (value)
let h = effect Db {
    query(q) => mock_rows()
}
with Db = h { ... }

// protocol-литерал (value реализующий контракт) — instance-only
let l = protocol Locker { lock() => state.lock() }
```

#### Анонимный protocol в type-position (D53 §628)

```nova
fn close_all(items []protocol { close() -> () }) {
    for it in items { it.close() }
}

fn min[T protocol { @lt(other Self) -> bool }](xs []T) -> Option[T] => ...
```

Body анонимного protocol — **тот же синтаксис**, что у named: bare-имена =
instance; leading-точка `.method` = static ([D143](03-syntax.md#d143)).

#### protocol-литерал: **instance-only**

Static-методы — это методы **типа** (`Type.method`, [D35](03-syntax.md#d35));
у литерала нет «своего типа» (анонимная impl). Попытка реализовать
static в protocol-литерале → diagnostic «static methods cannot be
implemented in protocol-literal; they belong to a type (D35) — use a
named type».

#### Capture-rules

Закрытие над окружающим scope'ом — **как обычное closure**
([D22](03-syntax.md#d22) / [D6](05-memory.md#d6) managed heap). Никаких
особых правил поверх closure не вводится.

### Почему

- **Симметрия снижает когнитивный налог.** Один keyword из declaration
  работает и в literal — нет «двух жаргонов» (`handler` vs `protocol`
  vs `effect`).
- **Анонимный protocol-литерал** разблокирует pattern «capability-split
  factory» — `Lock.new() -> (Locker, Unlocker)` без двух named-обёрток.
  Кандидаты в stdlib Plan 18: `Process.spawn`, `HttpServer.bind`,
  `Db.transaction`.
- **Symmetry побеждает локальную точность.** `let h = effect X { ... }`
  читается чуть точнее как «handler», но `protocol X { ... }`-литерал
  всё равно нужен — приходится либо ввести ещё keyword, либо
  унифицировать. Унификация чище.
- **Clean break без deprecated** — текущая база `.nv` маленькая (~30
  файлов); миграция атомарным sweep'ом дешевле двух-keyword'ового
  периода + последующей чистки.

### Что отвергнуто

- **`Protocol[P]` first-class тип** — отвергнут как избыточный. Для
  эффектов `Effect[E, IRT]` нужен, потому что **значение** эффекта
  передаётся в `with X = h` (нужна типизация значения). У протоколов
  «значение, реализующее контракт» — это **тип** реализации; обёртка
  не нужна. Тривиальный `alias` решит, если когда-нибудь понадобится
  (Q-protocol-type-wrapping).
- **`deprecated handler` alias** — отвергнут (clean break, ~30 файлов
  миграции).
- **Static в protocol-литерале** — отвергнут (нет «своего типа»; см.
  [D35](03-syntax.md#d35)).
- **Изменение семантики handler'ов** — нет, только rename keyword'ов.

### Связь

- [D53](#d53) — protocol declaration; D53 §628 (анон-protocol в
  type-position) ✅ реализовано (Plan 97 Ф.2).
- **Protocol-литерал codegen** — value `protocol Name { ops }`
  с runtime vtable + dispatch — ✅ реализовано в подплане Plan 97.1
  (`emit_protocol_lit` + расширенный Plan 56 D122 box-pattern).
  Capability-split factory pattern работает end-to-end.
- [D61](04-effects.md#d61) — handler-литерал; **rename** keyword
  `handler` → `effect` (Plan 97 Ф.3).
- [D87](04-effects.md#d87) — `Effect[E, IRT]`; **rename** в
  `Effect[E, IRT]` (Plan 97 Ф.3).
- [D88](03-syntax.md#d88) — default generics (`Effect[E]` ≡
  `Effect[E, Never]`).
- [D143](03-syntax.md#d143) — `.method`-префикс для static в
  protocol-body (закрывает Q-static-method-protocol).
- [D35](03-syntax.md#d35) — static vs instance методы.
- [D22](03-syntax.md#d22) — closure capture-rules.
- [Q-keyword-symmetry](../open-questions.md) — закрывается этим
  D-блоком.
- [Plan 97](../../docs/plans/97-protocol-effect-syntax-symmetry.md) —
  имплементация parser + AST + type-checker.
- [Plan 97.1](../../docs/plans/97.1-protocol-literal-codegen.md) —
  runtime codegen (vtable + dispatch) + followup-hardening
  (Nova-side enforcement, capture-mode by-value snapshot для factory,
  shadowing fix, scan_fwd recurse, GC stress, multi-method, nested).
- Ориентиры: Java/Kotlin (anonymous interface), TS (object-literal
  structurally), Koka/Eff (handler-literal).

### Canonical example — capability-split factory pattern

Use-case D142, разблокированный Plan 97.1 codegen'ом:

```nova
type Reader protocol { read() -> int }
type Writer protocol { write(v int) -> () }

type Cell { mut value int }

fn Cell.new(initial int) -> (Reader, Writer) {
    let state = Cell { value: initial }
    let r = protocol Reader { read() => state.value }
    let w = protocol Writer { write(v) { state.value = v } }
    (r, w)
}

// caller:
let (r, w) = Cell.new(10)
let initial = r.read()    // 10
w.write(99)
let after = r.read()      // 99 — shared state через protocol-литералы
```

Реализация (Plan 97.1 emit_protocol_lit, Approach A):
1. Литерал `protocol Reader { read() => state.value }` создаёт
   synthetic struct `Nova_ProtoLit_<N>` с capture-field `state`.
2. Free fn `Nova_ProtoLit_<N>_method_read(self, ...)` использует
   `self->state->value`.
3. Allocate `NovaVtable_Reader*` + ctx; patch vt->read = impl_fn.
4. Возврат `NovaBox_Reader { .data = ctx, .vtable = vt }`
   (fat-pointer pattern Plan 56 D122).

Method dispatch `r.read()` → `r.vtable->read(r.data)` — стандартный
vtable indirect call.

Capture-rules:
- Heap obj / `let mut` → by-pointer (alias, mutation visible).
- Immutable scalar / fn-param → by-value snapshot (factory-safe,
  survives fn exit).

---

## D144. Sub-slice views РґР»СЏ `[]T` Рё `str` вЂ” `arr[a..b]` / `s[a..b]`

> **Р�СЃС‚РѕС‡РЅРёРє:** Plan 96 (2026-05-23). Р—Р°РєСЂС‹РІР°РµС‚ Q-array-slicing,
> Q-array-api.5, D27 В§1663 drift (В«РЎР»Р°Р№СЃРёРЅРі РѕС‚Р»РѕР¶РµРЅВ»), D27 В§1632 drift
> (raw `arr[i]` Р±РµР· bounds-check). **Р—Р°РІРёСЃРёС‚ РѕС‚** [D6](05-memory.md#d6)
> non-moving GC; [D58](03-syntax.md#d58) Range; [D27](03-syntax.md#d27)
> `[]T` API; [Plan 90 / D141](08-runtime.md#d141) bulk-ops.

### РЎРµРјР°РЅС‚РёРєР° вЂ” sub-slice view

`arr[range]` РіРґРµ `range : Range` РІРѕР·РІСЂР°С‰Р°РµС‚ **view** вЂ” РЅРѕРІС‹Р№
24-Р±Р°Р№С‚РѕРІС‹Р№ header `NovaArray_T*` СЃ `data = orig->data + from`,
`len = cap = to - from`. **Р‘РµР· РєРѕРїРёРё РґР°РЅРЅС‹С… backing'Р°** (O(1) creation).

`str[range]` РІРѕР·РІСЂР°С‰Р°РµС‚ codepoint-indexed view (РґРІСѓС…РїСЂРѕС…РѕРґРЅС‹Р№ walk
UTF-8 в†’ byte offsets; structurally РёРґРµРЅС‚РёС‡РЅРѕ `nova_str_slice`, РЅРѕ СЃ
**panic РїСЂРё OOB** РІРјРµСЃС‚Рѕ clamp).

### 5 С„РѕСЂРј Range (Rust `RangeBounds` parity)

| Р¤РѕСЂРјР° | РЎРµРјР°РЅС‚РёРєР° | Open-ended? |
|---|---|---|
| `arr[a..b]` | exclusive: `[a, b)` | РЅРµС‚ |
| `arr[a..=b]` | inclusive: `[a, b]` | РЅРµС‚ |
| `arr[a..]` | РѕС‚ `a` РґРѕ РєРѕРЅС†Р° | РґР° (end = `len`) |
| `arr[..b]` | РѕС‚ РЅР°С‡Р°Р»Р° РґРѕ `b` | РґР° (start = 0) |
| `arr[..]` | РІРµСЃСЊ РјР°СЃСЃРёРІ | РґР° |

Open-ended С„РѕСЂРјС‹ вЂ” **С‚РѕР»СЊРєРѕ РІ slice-context** (`arr[range]`). Р’
materialize / for-loop / quantifier / parallel-for РѕРЅРё РѕС‚РІРµСЂРіР°СЋС‚СЃСЏ
СЃ compile-time diagnostic В«open-ended Range without bound (Plan 96)В».

### Single-type design

`[]T` вЂ” **РѕРґРёРЅ** С‚РёРї РґР»СЏ owner Рё view. РќРµС‚ `Slice[T]` (Rust-РјРѕРґРµР»СЊ
СЂР°Р·РґРµР»СЊРЅС‹С… С‚РёРїРѕРІ). View РїРµСЂРµРґР°С‘С‚СЃСЏ РІ С„СѓРЅРєС†РёСЋ Р¶РґСѓС‰СѓСЋ `[]T` Р±РµР·
РґРѕРїРѕР»РЅРёС‚РµР»СЊРЅРѕР№ РєРѕРЅРІРµСЂСЃРёРё.

### `cap == len` invariant

View РёРјРµРµС‚ `cap == len == to - from`. Push РЅР° view в†’ realloc (РєР°Рє
РѕР±С‹С‡РЅРѕ РїСЂРё exhausted cap) в†’ view **silent detach** РѕС‚ parent.
Parent backing **РЅРёРєРѕРіРґР°** РЅРµ РјРѕР»С‡Р° РїРµСЂРµР·Р°РїРёСЃС‹РІР°РµС‚СЃСЏ вЂ” СЌС‚Рѕ СѓСЃС‚СЂР°РЅСЏРµС‚
Go-`append`-footgun Р±РµР· borrow checker'Р°.

```nova
let mut parent = [1, 2, 3, 4, 5]
let mut view = parent[1..4]   \ view: [2, 3, 4]
view.push(99)                  \ realloc; view detached
\ parent == [1, 2, 3, 4, 5]   вЂ” РќР• Р·Р°С‚СЂРѕРЅСѓС‚
\ view == [2, 3, 4, 99]
```

### Mut-СЃРµРјР°РЅС‚РёРєР°

`mut`-view С‚РѕР»СЊРєРѕ РѕС‚ `mut`-РёСЃС‚РѕС‡РЅРёРєР°. Р§РµСЂРµР· `mut`-view write РёРґС‘С‚ РІ
**shared backing** вЂ” РёР·РјРµРЅРµРЅРёСЏ РІРёРґРЅС‹ parent. РќРµСЃРєРѕР»СЊРєРѕ `mut`-view
РѕРґРЅРѕРіРѕ backing'Р° **СЂР°Р·СЂРµС€РµРЅС‹** (РєР°Рє РІ Go); caller responsibility,
РЅРёРєР°РєРѕРіРѕ borrow checker'Р°.

### Iterator invalidation

`for x in view` вЂ” `len` Р±РµСЂС‘С‚СЃСЏ snapshot'РѕРј РІ РЅР°С‡Р°Р»Рµ С†РёРєР»Р° (Go-style).
Push РЅР° parent РІРѕ РІСЂРµРјСЏ РёС‚РµСЂР°С†РёРё view'Р° **РЅРµ РІРёРґРµРЅ** view'Сѓ: parent
СЂРµР°Р»Р»РѕС†РёСЂСѓРµС‚, view РїСЂРѕРґРѕР»Р¶Р°РµС‚ СѓРєР°Р·С‹РІР°С‚СЊ РЅР° СЃС‚Р°СЂС‹Р№ backing С‡РµСЂРµР·
interior-pointer.

### GC requirement вЂ” interior pointers stable

**РќРµРѕР±С…РѕРґРёРјРѕРµ СѓСЃР»РѕРІРёРµ:** runtime РіР°СЂР°РЅС‚РёСЂСѓРµС‚ stable interior pointers
(non-moving GC, D6). View С…СЂР°РЅРёС‚ `data = backing->data + from` вЂ” СЌС‚Рѕ
СѓРєР°Р·Р°С‚РµР»СЊ **РІРЅСѓС‚СЂСЊ** backing'Р°; Boehm (`GC_set_all_interior_pointers(1)`)
РґРµСЂР¶РёС‚ backing alive РїРѕ interior-ptr.

Р›СЋР±Р°СЏ Р±СѓРґСѓС‰Р°СЏ Р·Р°РјРµРЅР° GC-backend РЅР° moving GC С‚СЂРµР±СѓРµС‚ РѕРґРЅРѕРІСЂРµРјРµРЅРЅРѕР№
Р·Р°РјРµРЅС‹ slice-РїСЂРµРґСЃС‚Р°РІР»РµРЅРёСЏ (separate header struct + ptr-update on
move). Р­С‚Рѕ Р·Р°РєСЂРµРїР»СЏРµС‚СЃСЏ Р·РґРµСЃСЊ РєР°Рє РЅРѕСЂРјР°С‚РёРІРЅС‹Р№ invariant.

### Bounds-check

- `from < 0` в†’ panic
- `to < from` в†’ panic
- `to > len` в†’ panic (РґР»СЏ str вЂ” `to > total_codepoints`)
- Empty slice (`arr[a..a]`) в†’ РІР°Р»РёРґРµРЅ
- РћС‚СЂРёС†Р°С‚РµР»СЊРЅС‹Рµ РёРЅРґРµРєСЃС‹ в†’ panic, **РЅРµ** Python-style wrap

РЎРѕРѕР±С‰РµРЅРёРµ panic'Р°: `"array: slice [N..M] out of bounds for length L"`
(РїР°СЂРёС‚РµС‚ СЃ Go/Rust).

### РўР°РєР¶Рµ: raw `arr[i]` bounds-check (D27 В§1632 drift)

D144 РѕРґРЅРѕРІСЂРµРјРµРЅРЅРѕ С„РёРєСЃРёСЂСѓРµС‚ pre-existing drift: codegen `arr[i]`
**С‚РµРїРµСЂСЊ** СЌРјРёС‚РёС‚ runtime bounds-check (СЂР°РЅСЊС€Рµ СЌРјРёС‚РёР» РіРѕР»С‹Р№
`(arr)->data[i]` вЂ” controlled buffer overflow РЅР° Р·Р°РїРёСЃСЊ, UB РЅР° С‡С‚РµРЅРёРµ).
РЎРѕРѕР±С‰РµРЅРёРµ: `"array: index N out of bounds for length L"`.

### Concurrency / M:N

Slice-view = shared mut backing РјРµР¶РґСѓ fiber'Р°РјРё РІ M:N runtime =
**С„РѕСЂРјР°Р»СЊРЅРѕ UB РїРѕ [D79](06-concurrency.md#d79)**. Р’ D71 single-threaded
bootstrap вЂ” OK РїРѕ С„Р°РєС‚Сѓ. РџРµСЂРµРґР°С‡Р° view С‡РµСЂРµР· `Channel[]T]` РёР»Рё
spawn-capture РІ M:N вЂ” **inherits D79 disclaimer**.

### Header layout

24 Р±Р°Р№С‚Р° (`ptr + len + cap`) вЂ” С‚РѕС‚ Р¶Рµ С‡С‚Рѕ Сѓ owner. РќРµ РѕРїС‚РёРјРёР·РёСЂРѕРІР°РЅРѕ
РґРѕ 16 Р±Р°Р№С‚ (РєРѕС‚РѕСЂРѕРµ С‚СЂРµР±РѕРІР°Р»Рѕ Р±С‹ РѕС‚РґРµР»СЊРЅРѕРіРѕ С‚РёРїР° `Slice[T]` вЂ” РѕС‚РІРµСЂРіРЅСѓС‚Рѕ
single-type-design'РѕРј).

### `str[a..b]` вЂ” bracket syntax РґР»СЏ СЃС‚СЂРѕРє

Bracket-С„РѕСЂРјР° СѓРЅРёС„РёС†РёСЂСѓРµС‚ idiom: `arr[a..b]` в‰Ў `str[a..b]`.
Codepoint-indexed (РєР°Рє СЃСѓС‰РµСЃС‚РІСѓСЋС‰РёР№ `nova_str_slice` РјРµС‚РѕРґ).
**Panic РїСЂРё OOB** (consistent СЃ `arr[a..b]`).

РЎС‚Р°СЂС‹Р№ `s.slice(a, b)` РјРµС‚РѕРґ вЂ” **СЃРѕС…СЂР°РЅСЏРµС‚СЃСЏ** СЃ clamp-СЃРµРјР°РЅС‚РёРєРѕР№
РґР»СЏ backwards-compat; align РЅР° panic РѕС‚РєР»Р°РґС‹РІР°РµС‚СЃСЏ РІ Plan 94
(СЃРј. `[P-str-slice-clamp-vs-panic]` РІ `docs/simplifications.md`).

### Verified РїСЂРѕС‚РёРІ

- Go `s[a:b]` вЂ” РїР°СЂРёС‚РµС‚, **Р±РµР· append-footgun**.
- Rust `&[T]` вЂ” Р±Р»РёР·РєРѕ, **Р±РµР· borrow checker** (caller responsibility
  РґР»СЏ multi-mut).
- TypeScript `TypedArray.subarray` вЂ” РїР°СЂРёС‚РµС‚.
- Swift `ArraySlice<T>` вЂ” **Р±РµР· CoW-disconnect** (view СЃСЂР°Р·Сѓ РІРёРґРёС‚ mut).
- Python `memoryview` вЂ” РїР°СЂРёС‚РµС‚.

### РЎРІСЏР·СЊ

- [D6](05-memory.md#d6) вЂ” non-moving GC; interior-ptr invariant
  Р°РјРµРЅРґРёС‚СЃСЏ Р·РґРµСЃСЊ.
- [D27](03-syntax.md#d27) вЂ” `[]T` API; В§1632 bounds-check (D144 С‡РёРЅРёС‚
  drift); В§1663 В«РЎР»Р°Р№СЃРёРЅРі РѕС‚Р»РѕР¶РµРЅВ» (D144 Р·Р°РєСЂС‹РІР°РµС‚).
- [D58](03-syntax.md#d58) вЂ” Range-Р»РёС‚РµСЂР°Р»С‹; D144 СЂР°СЃС€РёСЂСЏРµС‚ РґРѕ 5 С„РѕСЂРј
  (open-ended).
- [D79](06-concurrency.md#d79) вЂ” shared mut РјРµР¶РґСѓ fiber'Р°РјРё = UB
  РІ M:N; slice inherits.
- [D141](08-runtime.md#d141) вЂ” Plan 90 bulk-ops; СЂР°Р±РѕС‚Р°СЋС‚ РЅР° view
  Р°РІС‚РѕРјР°С‚РёС‡РµСЃРєРё.
