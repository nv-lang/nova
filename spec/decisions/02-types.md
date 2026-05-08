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
       | Data { payload []byte, crc u32 } = 10
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

---

## D55. Literal coercion в позиции с явным типом: sum-конструкторы и record-литералы

### Что
В позиции, где компилятор **явно знает целевой тип** `T` (let с
аннотацией, аргумент функции, return-выражение), литерал
автоматически подгоняется под `T`. Два случая:

1. **Sum-coercion.** Значение типа `S` оборачивается в **единственный**
   unary-конструктор `C(S)` sum-типа `T`.
2. **Record-coercion.** Анонимный record-литерал `{ field: value, ... }`
   получает тип `T` без необходимости писать имя типа перед `{}`.

Без runtime-cost, без subtyping. Тип значения после coercion — сам `T`.

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

| Позиция | Coercion применяется? |
|---|---|
| `let x T = value` (явная аннотация) | да |
| `fn f(x T)` — на caller-стороне (`f(value)`) | да |
| `fn f() -> T => value` (return-выражение) | да |
| `let x = value` (без аннотации) | **нет** — выводится тип значения |
| Generic-параметр после конкретизации (`Maybe[int]`) | да |
| Match-arm result (когда тип ветки фиксирован) | да |
| Литерал коллекции с явным типом (`[]T`) | да для каждого элемента |

В позициях без явного типа никакая coercion не применяется — литерал
имеет «свой» тип (`{ id: 2 }` — анонимный record, `42` — int, и т.д.).

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
type SqlValue | I(i64) | F(f64) | S(str) | B(bool) | Bytes([]byte) | Null

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
fn Set[T] @len() => @map.len                          // ✓
fn Set[T] @len() => @use.len                          // ✗ use — keyword
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

**Примитивы — by value.** Числа, `bool`, `char`, `byte`, `()` —
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

Если нужно «принимает Handler[Db]» — пишется явно: `fn run(h Handler[Db])`.

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
