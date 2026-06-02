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
| [D175](#d175-readonly-field--полный-freeze-амендмент-d36) | `readonly field` — полный freeze, транзитивность (амендмент D36) | active |
| [D176](#d176-readonly-t--тип-модификатор) | `readonly T` — тип-модификатор, coercion rules, zero overhead | active |
| [D66](#d66-self-universal--ссылка-на-обобщающий-тип-в-методах-effects-protocols) | `Self` universal: ссылка на обобщающий тип в методах, effects, protocols | active |
| [D72](#d72-generic-bounds-через-t-protocol--protocol-как-тип) | Generic bounds через `[T Protocol]` — protocol как тип | active |
| [D110](#d110-ghost-state--spec-only-bindings) | Ghost state — spec-only bindings | active |
| [D122](#d122-hybrid-dispatch-для-bound-k-methods) | Hybrid dispatch для bound-K methods | active |
| [D123](#d123-tuple-monomorphization) | Tuple monomorphization | active |
| [D215](#d215-named-tuple-fields--valuereference-allocation-contract) | Named tuple fields + value/reference allocation contract | active |
| [D119](#d119-method-level-type-parameters-в-generic-methods) | Method-level type parameters в generic methods | active |
| [D180](#d180-canonical-new-constructors-convention) | Canonical `.new()` constructors (convention) | active |
| [D181](#d181-array-methods----fluent-mut-chain--slice-syntax) | Array methods — `-> @` fluent mut chain + slice syntax | active |
| [D182](#d182-self-в-return-type-static-methods--required-form-для-parametric-types) | `Self` в return-type static methods — required form для parametric types | active |
| [D183](#d183-canonical-comparison-protocols--default-method-bodies-plan-918a) | Canonical comparison protocols + default method bodies (Plan 91.8a) | active |

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
ro p = Point(1.0, 2.0)
ro u = User { id: 1, name: "alice" }
ro c = Circle { radius: 5.0 }

match shape {
    Circle { radius }    => 3.14159 * radius * radius
    Square { side }      => side * side
    Triangle { a, b, c } => heron(a, b, c)
}
```

**Field punning** для record-литералов: если имя поля совпадает с
именем переменной в скоупе, можно писать имя один раз:

```nova
ro key = "alice"
ro value = 42

ro entry = Entry { key, value }                    // shorthand
ro entry = Entry { key, value, extra: "data" }     // можно смешивать
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
- **record** — `type X { поля }` — **heap-allocated** reference type (GC-managed)
- **tuple** — `type X(типы)` — **stack-allocated** value type (позиционные поля `.0`/`.1`)
- **named tuple** — `type X(name1 T1, name2 T2)` — **stack-allocated** value type (именованные поля `.name`) (D215, Plan 120)
- **unit** — `type X` (ничего после имени)
- **sum** — `type X | A | B | C` (leading `|` обязателен)

> **Allocation contract (D215, Plan 120):** скобки кодируют семантику
> размещения: `()` = **stack-allocated** value type, copy-семантика при
> передаче; `{}` = **heap-allocated** reference type, GC-tracked. Выбор
> формы явно документирует производительность и lifetime ожидания.

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
    ro id u64
    balance money
    mut last_access time
}

// 4a. Positional tuple — type X(типы)
type Point(f32, f32)          // .0 / .1 access
type Pair[A, B](A, B)

// 4b. Named tuple — type X(name type, ...) (D215, Plan 120)
type Vec3(x f64, y f64, z f64)       // .x / .y / .z access; stack-allocated
type Color(r u8, g u8, b u8, a u8)
type Generic[T](value T, count int)

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
| `(` + ident + bare-type | named tuple (D215) — `(name1 T1, name2 T2)` |
| `(` + bare-type | positional tuple — `(T1, T2)` |
| `{` | record |
| `alias` | alias |
| `<base-type>` `\|` | sum с явным базовым типом для discriminants |
| идентификатор/тип, конец строки | newtype |
| конец строки сразу | unit |

Парсер видит первый токен — сразу знает форму. Для `(` — один
дополнительный lookahead: если `(IDENT type` → named tuple,
иначе → positional tuple. Никакого backtracking.

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

> ⚠ **Явный базовый тип пока не реализован** (parser drift, 2026-05-27).
> Формы с `u8`/`i32`/etc. между именем и `|` парсер отвергает с
> `expected fn / type / let / const / test, got '|'`. Работает только
> дефолтная форма (без базового типа, implicit `int`). См.
> [Plan 105](../../docs/plans/105-sum-type-explicit-base.md).

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
ro c = Red                 // Color
ro n = c as int            // 0 (если auto-increment)

ro e = NotFound            // ErrorCode
ro n = e as i32            // 404
```

**int → Sum** — через **pattern match obligation**:

```nova
ro n = read_from_db()
ro c = match n {
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

ro a AliasUserId = 42        // ok
ro b u64 = a                  // ok — alias совместим с u64
ro c u64 = 42
ro d AliasUserId = c          // ok — обратное тоже работает

ro n NewUserId = 42           // ok (литерал подгоняется под целевой тип)
ro e u64 = n                  // ОШИБКА: NewUserId не u64
ro f u64 = n as u64           // ok через cast
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
ro key = "alice"
ro value = 42
ro entry = Entry { key, value }                  // ✓ обязательная форма
ro entry = Entry { key: key, value: value }      // ✗ ОШИБКА: избыточная форма

// @field-доступ:
ro r = { @end, @inclusive, cur: @start }         // ✓
ro r = { end: @end, inclusive: @inclusive, ... } // ✗ ОШИБКА: избыточная

// Явная форма обязательна, когда имя источника отличается:
ro entry = Entry { name: user_name }             // ✓ имя поля ≠ переменной
ro r = { cur: @start }                            // ✓ имя поля cur ≠ start
ro r = { end: other.end }                         // ✓ источник — выражение, не @field
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

ro a StrOrInt = "test"          // компилятор: a = S("test")
ro b StrOrInt = 25               // компилятор: b = I(25)

fn process(x StrOrInt) -> str => ...
process("alice")                   // компилятор: process(S("alice"))
process(42)                        // компилятор: process(I(42))

// Record-coercion
type User { id u64, name str }

ro u User = { id: 2, name: "Bob" }    // компилятор: u = User { id: 2, name: "Bob" }

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
ro m Maybe[int] = 42                        // Just(42)
ro r Result[User, str] = User { ... }       // Ok(User { ... })
ro opt Option[str] = "alice"                // Some("alice")
```

**Коллекции:**

```nova
type SqlValue | I(i64) | F(f64) | S(str) | B(bool) | Bytes([]u8) | Null

ro args []SqlValue = [42, "alice", true]    // [I(42), S("alice"), B(true)]

// В sql`...` тэге интерполяции тоже coerce'ятся: i64 → I, str → S, bool → B
ro q = sql`SELECT * FROM users WHERE id = ${42}`   // args = [I(42)]
```

**Генерики:**

```nova
type Wrapper[T] | W(T) | Empty

ro w Wrapper[int] = 42                      // W(42)
ro w Wrapper[str] = "test"                   // W("test")
```

#### Record-coercion

В позиции с явным ожидаемым record-типом `T` анонимный record-литерал
`{ field: value, ... }` подгоняется под `T`. Имя типа перед `{}`
писать не нужно — компилятор подставляет.

```nova
type User { id u64, name str }

ro u User = { id: 2, name: "Bob" }
// эквивалент:
ro u User = User { id: 2, name: "Bob" }

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
ro r Result[User, str] = { id: 2, name: "Bob" }
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
ro r = first([])                   // [] : []T, T выводится из контекста

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

ro s Shape = Circle { radius: 5.0 }   // явный конструктор обязателен
ro s Shape = { radius: 5.0 }           // ОШИБКА: по полям невозможно
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
ro h HashMap[str, bool] = { debug: true, verbose: false }
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
   ro j HashMap[str, JsonValue] = { name: "alice", age: 30.0 }
   // "alice" → Str("alice"), 30.0 → Num(30.0); оба → JsonValue
   ```
4. **Десугаринг — без промежуточных объектов:** block-expression с
   `with_capacity` + `@insert`, никакой промежуточный record не
   материализуется (литерал — только синтаксис):
   ```nova
   { mut _m0 = HashMap[str, V].with_capacity(n)
     ro _ = _m0.insert("debug", true)
     ro _ = _m0.insert("verbose", false)
     _m0 }
   ```
5. **Пустой `{}` — это НЕ пустая мапа.** `{}` всегда парсится как пустой
   block-expression с типом `unit` — даже в позиции, ожидающей
   `HashMap[str, V]`. Пустая мапа записывается как `[]` + ожидаемый тип
   ([03-syntax.md → D108](03-syntax.md#d108-map-литерал-k-v)):
   ```nova
   ro h HashMap[str, bool] = []     // ✅ пустая мапа (тип из контекста)
   ro h HashMap[str, bool] = {}     // ⛔ {} — пустой блок, тип unit ≠ HashMap
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

ro x Ambiguous = 42         // ОШИБКА: ambiguous, A(42) или B(42)?
ro x = A(42)                 // явный конструктор — ok
```

**Несоответствие — ни один конструктор не принимает тип значения:**

```nova
type Color | Red | Green | Blue

ro c Color = "red"           // ОШИБКА: ни один конструктор не принимает str
ro c = Red                    // unit-конструктор
```

**Без аннотации — coercion отключён:**

```nova
type StrOrInt | S(str) | I(int)

ro a = "test"                // a : str (не StrOrInt, аннотации нет)
ro b StrOrInt = "test"        // b : StrOrInt = S("test") (аннотация есть)

ro r = { id: 2, name: "Bob" }   // r : анонимный record { id int, name str }
ro u User = { id: 2, name: "Bob" }   // u : User (через record-coercion)
```

**Newtype через D52 — coercion следует типу значения, не возможным кастам:**

```nova
type UserId u64
type Wrapper | W(UserId) | N(int)

ro w Wrapper = 42            // 42 : int → N(42) (тип значения int)
ro w Wrapper = 42 as UserId  // → W(42 as UserId) — явный as, потом coercion
ro w Wrapper = UserId(42)    // явный конструктор UserId
```

**Несовпадение полей record:**

```nova
type User { id u64, name str }

ro u User = { id: 2 }                    // ОШИБКА: missing field `name`
ro u User = { id: 2, name: "Bob", age: 30 }   // ОШИБКА: unknown field `age`
ro u User = { id: "two", name: "Bob" }   // ОШИБКА: id expects u64, got str
```

Coercion **не строит цепочку конверсий** — только одна обёртка вокруг
exact-type значения.

#### Multi-parameter и tuple-варианты

**Multi-parameter конструкторы — coercion не применяется в MVP:**

```nova
type Event | Click(int, int) | KeyPress(str)

ro e Event = "enter"         // ok — KeyPress("enter"), unary с str
ro e Event = (5, 10)          // ОШИБКА в MVP: tuple-coercion не вводится
ro e = Click(5, 10)           // явный конструктор
```

Tuple-coercion `(5, 10) → Click(5, 10)` — отложено. Усложняет правила
(как различать «tuple как значение» vs «tuple-coercion в multi-param»),
не критично для use-case'ов.

#### Unit-конструкторы — coercion бессмыслен

Unit-варианты не принимают значение, coercion не нужен — программист
пишет конструктор напрямую:

```nova
type State | Open | Closed
ro s State = Open              // unit, coercion не применяется
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
ro u User = { id: 1, name: "alice" }                ✅
ro m Maybe[int] = 42                                 ✅

// 2. return-position в expression-body, есть -> T
fn make_default() -> Account => { id: 0, balance: 0 } ✅

// 3. call-site с явным типом параметра — coercion даёт чистый литерал
serve({ ...SERVER_DEFAULTS, port: 9000 })             ✅

// 4. коллекции с разнородными элементами в позиции []SqlValue
ro args []SqlValue = [42, "alice", true]             ✅
//                    [I(42), S("alice"), B(true)]    ❌ шумно
```

**Явный конструктор — предпочитать когда:**

```nova
// 1. let без аннотации — coercion не работает, имя обязательно
ro r = if cond { Some(value) } else { None }         ✅
ro r = if cond { value } else { None }               ❌ — нет аннотации

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
ro x Mixed = 42                  ❌ ambiguous — обязателен A(42) / B(42)
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

ro acc AuditedAccount = ...

// Auto-proxy: прямой доступ к полям и методам Account
println(acc.balance)                 // = acc.account.balance
println(acc.owner)                   // = acc.account.owner
acc.is_solvent()                     // = acc.account.is_solvent()

// Доступ к встроенному объекту целиком — через имя поля
ro just_account = acc.account
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

ro aa AuditedAccount = ...
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
ro user User = ...                           // ro: имя тип
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
ro s Set[int] = { map: HashMap[int, ()].new() }      // ✓
ro s Set[int] = { use: HashMap[int, ()].new() }       // ✗ use — keyword

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

mut acc AuditedAccount = ...
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

ro c = Combined { ... }
c.log("...")                         // ОШИБКА: ambiguous (оба имеют log)
```

Решение — явный вызов через имя поля:

```nova
fn Combined @log_all(msg str) {
    @console.log(msg)
    @audit.log(msg)
}

ro c = Combined { ... }
c.console.log("...")
c.audit.log("...")
```

#### Anonymous embed: `use _ Type` (без alias-имени)

Альтернатива явному alias — **anonymous embed** через `_`:

```nova
type Set[T] {
    use _ HashMap[T, ()]
}

ro s = Set[int].new()
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
ro s Set[int] = ...
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

ro v = VecBuf[int] { data: [1, 2, 3], extra: "info" }
ro n = v.len            // прокси-метод к data.len ([]T API)
v.push(42)               // прокси-метод к data.push
ro x = v.get(0)         // прокси к data.get
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

ro aa AuditedAccount = ...
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
>
> ⚠️ **AMENDED by Plan 118 (D216)** — `&value` operator restored для
> создания typed pointer (`*T` / `*mut T`). **Это НЕ Rust borrow** (нет
> lifetime checker, нет XOR aliasing); safety обеспечивается через escape
> analysis + auto-promote (Go-style) + unsafe gating. D32 spirit «no
> borrow» preserved — `*T` это explicit unsafe-gated raw pointer с
> safety net через GC, не lifetime-checked reference. See
> [Plan 118](../../docs/plans/118-typed-pointers-and-unsafe.md) §«&value
> operator + escape analysis с auto-promote» и [D216 §4](#d216-typed-pointer-family--unsafe-model--null-safety-через-npo).
>
> Amended Plan 114 D184 (2026-05-31): default immutable binding теперь
> выражается через `ro X = …` (immutable) и `mut X = …` (mutable); `let`
> retracted. Семантика default-immutable не меняется — только keyword.
> См. [D184](03-syntax.md#d184).

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

mut my_acc = Account { balance: 100 }
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

ro n = 5
weird(n)
// n == 5 — примитив всегда by value
```

**Явная таксономия value vs reference типов (D215 amend, Plan 120):**

| Категория | Примеры | Размещение | Передача |
|---|---|---|---|
| Примитивы | `int`, `bool`, `f64`, `char`, `u8`, `()` | register/stack | by value (копия) |
| Tuples (positional или named) | `type X(T1, T2)`, `type Vec3(x f64, ...)` | **stack** | by value (копия) |
| Records | `type X { ... }` | **managed heap** | by reference (указатель) |
| Sum types | `type X \| A \| B` | managed heap | by reference |
| Arrays, strings | `[]T`, `str` | managed heap | by reference |

Bracket choice **явно кодирует** size/lifetime semantics: `()` =
stack, `{}` = heap. Tuple value types (D123): zero GC pressure,
predictable lifetime — ideal для hot-path math types, FFI returns,
iterator state.

**Объекты (record / sum-type / массивы) — managed reference.**
Указатель в managed heap, отслеживаемый GC. В синтаксисе программист
пишет просто `o Order` — никакого `&` или `*`:

```nova
type Order { items []Item, total money }

fn add_item(mut order Order, item Item) {
    order.items.push(item)
    order.total += item.price
}

mut my_order = Order { items: [], total: 0 }
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
        ro buf = []f32.with_capacity(1024)
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

## D36. Поля типа: дефолт mutable у `mut` binding'а, `ro` для never-mut

> Amended Plan 114 D184 (2026-05-31): `readonly` → `ro` keyword rename
> в полях. Sample обновлён. Error code `E_READONLY_FIELD` сохранён как
> stable API. Семантика per-field freeze не меняется.

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

mut acc = RunAcc { att_wins: 0, def_wins: 0, ... }
acc.att_wins += 1                   // ок — binding mut, поле без ro

// Структура с invariant'ами — readonly для read-only полей
type Account {
    ro id u64                 // никогда не меняется
    ro owner str              // тоже
    balance money                    // мутируется у mut binding'а
    closed bool
}

ro acc = Account.new("alice")
acc.balance = 100                   // ОШИБКА: binding не mut

mut acc2 = Account.new("alice")
acc2.balance = 100                  // ок
acc2.id = 999                       // ОШИБКА: id объявлено ro

// Cache/lazy — mut для полей, мутируемых через immutable binding
type LazyConfig {
    path str
    mut cached_value Option[str]    // обновляется при первом read
}

fn LazyConfig @get() -> str {
    if Some(v) = @cached_value { return v }
    ro v = read_file(@path)
    @cached_value = Some(v)         // мутация через @-метод даже у ro-binding
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
    ro id, owner_id u64       // два immutable
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

### Enforcement (Plan 108.2, 2026-05-30)

Плановое поведение D36 («`let` без `mut` — immutable») de-facto
существовало с самого начала, но компилятор не enforce'ил его
строго: на binding'е без `mut` можно было вызывать mut-методы
(`.push`, `.append`, `.insert` и т.п.) и присваивать поля
(`b.field = ...`).  Plan 108.2 закрывает этот gap:

```nova
ro b = Box.new(1)
b.value = 99                  // ✗ E_LOCAL_NOT_MUT
b.push(2)                     // ✗ E_LOCAL_NOT_MUT

mut b2 = Box.new(1)
b2.value = 99                 // ✓
b2.push(2)                    // ✓
```

**Правила (Plan 108.2):**

| Операция | `let x = ...` | `let mut x = ...` | `consume x = ...` |
|---|---|---|---|
| read field | ✓ | ✓ | ✓ |
| non-mut method | ✓ | ✓ | ✓ |
| `x.field = ...` | ✗ `E_LOCAL_NOT_MUT` | ✓ | ✓ |
| `x.mut_method()` | ✗ `E_LOCAL_NOT_MUT` | ✓ | ✓ |
| `x[i] = ...` | ✗ `E_LOCAL_NOT_MUT` | ✓ | ✓ |
| rebind `x = newval` | ✗ existing E_REBIND | ✓ | n/a (move) |

`consume X = ...` неявно подразумевает mut (как `consume` param в D176
amend Plan 108.1) — ownership transfer → владелец может мутировать.

**Symmetry с D176 (Plan 108.1):**

| Контекст | Default = readonly? | Opt-in mut |
|---|---|---|
| Param | ✓ (Plan 108.1) | `mut name T` |
| Local binding | ✓ (Plan 108.2) | `let mut x = ...` |
| Loop variable | ✓ (Plan 108.3) | `for mut x in iter` |
| Pattern element | ✓ (Plan 108.3) | `let (mut a, b) = pair` (per-name) |
| Field | ✓ (D36 default = mutable у mut-binding) | n/a |

### Loop-var и pattern-binding (Plan 108.3, 2026-05-30)

**Loop-var mutability:** в `for`-цикле переменная итерации по умолчанию
read-only.  Opt-in mut через `for mut x in iter`:

```nova
for x in arrs { x.push(1) }           // ✗ E_LOCAL_NOT_MUT — x immutable
for mut x in arrs { x.push(1) }       // ✓ — x mutable
```

**Pattern-binding per-name mut:** при destructure (tuple, record) `mut`
ставится **на каждое имя отдельно**, parallel Rust pattern semantics:

```nova
ro (a, b) = pair                     // оба immutable
ro (mut a, b) = pair                 // a mutable, b immutable
ro (a, mut b) = pair                 // a immutable, b mutable
ro (mut a, mut b) = pair             // оба mutable
```

**Запрет group-mut:** `let mut (a, b) = ...` отвергается parser-level
(`E_PATTERN_GROUP_MUT`) — keyword `mut` относится к одному имени,
не к pattern целиком (consistent с Rust):

```nova
mut (a, b) = pair                 // ✗ E_PATTERN_GROUP_MUT
```

Использование `mut` внутри pattern — единственно правильная форма.

### Связь
- [02-types.md → D175](#d175-readonly-field--полный-freeze-амендмент-d36) — readonly field полный freeze.
- [02-types.md → D176](#d176-readonly-t--тип-модификатор) — readonly T modifier + Plan 108.1 param default flip.
- [03-syntax.md → D33](03-syntax.md#d33) — `let` это immutable binding.

---

## D175. `ro field` — полный freeze (амендмент D36)

> Status: active (Plan 108, 2026-05-28); amended Plan 114 D184 (2026-05-31):
> `readonly` → `ro` keyword rename. Error code `E_READONLY_FIELD` сохранён
> как stable API. Семантика freeze + транзитивность не меняется.

### Что

Уточнение D36: `ro field T` запрещает **и** переприсвоение поля,
**и** мутацию содержимого — транзитивно.

| Объявление | Переприсвоить | Мутировать содержимое | Use case |
|---|---|---|---|
| `field T` | у `mut` binding | у `mut` binding | большинство полей |
| `ro field T` | ❌ никогда | ❌ никогда | id, invariants, frozen state |
| `field ro T` | у `mut` binding | ❌ никогда | mutable ref, immutable content |
| `mut field T` | ✅ всегда | у `mut` binding | cache, lazy init |
| `mut field ro T` | ✅ всегда | ❌ никогда | swappable readonly view |

**Транзитивность:** если поле объявлено `ro`, доступ через него
также запрещает мутацию вложенных полей и вызов `mut`-методов:

```nova
type Tags { mut items []str }
type Account {
    ro id u64
    ro tags Tags              // нельзя acc.tags.items.push("x")
}
mut acc = ...
acc.id = 999                  // E_READONLY_FIELD
acc.tags = Tags{}             // E_READONLY_FIELD
acc.tags.items.push("x")      // E_READONLY_FIELD (транзитивно)
```

### Связь
- [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindinga-readonly-для-never-mut) — расширяется
- [D176](#d176-ro-t-тип-модификатор) — `ro` как тип-позиция
- [D184](03-syntax.md#d184) — keyword refresh (readonly → ro rename)

---

## D176. `ro T` — тип-модификатор

> Status: active (Plan 108, 2026-05-28); amended (Plan 108.1, 2026-05-30);
> amended Plan 114 D184 (2026-05-31): `readonly` → `ro` keyword rename;
> return-type defaults + `@`-inheritance section added.
> Error codes `E_READONLY_CONTENT` / `E_READONLY_COERCE` / `E_PARAM_NOT_MUT`
> сохранены как stable API.

### Что

`ro` как prefix-модификатор типа в любой позиции:

```nova
fn str @as_bytes() -> ro []u8                 // возвращаемый тип
fn process(data ro []u8) { ... }              // параметр
type Wrapper { field ro []u8 }                // поле
ro view ro []u8 = s.as_bytes()                // binding с ro-content
```

> Двойное `ro` в последней строке — не tautology: первое `ro` — binding
> mutability (нельзя `view = …`), второе — type-modifier (нельзя `view[0] = …`).
> См. [D184](03-syntax.md#d184) для полного дизайна.

### Семантика

- Запрещает вызов `mut`-методов на значении типа `ro T`
- Запрещает запись через индекс: `view[i] = x` → `E_READONLY_CONTENT`
- `T` → `ro T` coercion разрешён автоматически (сужение прав)
- `ro T` → `T` запрещён: `E_READONLY_COERCE`

```nova
ro arr []u8 = [1, 2, 3]
ro view ro []u8 = arr                 // ✅ []u8 → ro []u8
mut back []u8 = view                  // ❌ E_READONLY_COERCE
view[0] = 99                          // ❌ E_READONLY_CONTENT
take_ro(arr)                          // ✅ auto-coerce при вызове
```

### Return-type defaults + `@`-inheritance (Plan 114 D184)

**Асимметрия с параметрами — намеренная.** Plan 108.1 сделал параметры
default `ro` (callee не может мутировать без opt-in). Для возвращаемых
значений правило **противоположное**: default = **mutable** (caller
получает значение, делает с ним что хочет).

```nova
fn make_buf(n int) -> []u8                  // -> mutable []u8 by default
fn read_view(s str) -> ro []u8              // explicit ro в возврате
```

**Обоснование.** Param `ro` default — defensive (callee не имеет права).
Return mut default — permissive (caller владеет результатом). Это совпадает
с Rust/Swift/Kotlin: `fn foo() -> Vec<T>` отдаёт owned mutable; чтобы
вернуть read-only view — explicit `-> ro T`.

**Особый случай: `-> @` (self-return для fluent chains, D181).**
Возвращаемая `@` **наследует мутируемость от receiver**:

| Receiver | Return `-> @` | Пример |
|---|---|---|
| `fn T @method() -> @` (implicit/ro receiver) | `ro @` (read-only self-view) | `ro r = obj.method()` |
| `fn T mut @method() -> @` | mut `@` (mutable self-view) | `obj.push(1).push(2)` — fluent mut chain |
| `fn T consume @method() -> @` | **parse error `E_CONSUME_RECEIVER_RETURNS_AT`** | consume already moves ownership; return `@` создал бы dangling-view |

**Почему такое правило для `@`.** `@` это **тот же экземпляр** что
receiver — его access-mutability не может быть строже, чем у receiver'а:

- `ro @` receiver → `@` уже view; return view'а — view; consistent.
- `mut @` receiver → `@` mutable handle; return mutable handle; consistent —
  именно так работают fluent chains `xs.push(1).push(2)`.
- `consume @` receiver → ownership уже перемещён внутрь method'а; вернуть
  `@` = alias на consumed value = use-after-move; **запрещено**. Если
  нужно fluent после consume — возвращайте новый owned (`fn T consume
  @transform() -> T`), не `@`.

**Что НЕ меняется** в return-семантике:
- Любой явный return type (`-> T`, `-> []u8`, `-> ro T`, `-> mut T`) —
  берётся как написан.
- `-> Self` (статический Self-тип, D182) — owned-by-caller; не наследует
  receiver-мут.
- `-> @` без receiver-method context (free fn) → **`E_AT_RETURN_OUTSIDE_METHOD`**.

### Escape hatch

Снять `readonly` в Nova-коде нельзя. Кому нужен mutable доступ —
явно копирует: `let copy []u8 = view.to_owned()`. Если необходим
обход через FFI, это делается в `external fn` на C-стороне.

### Рантайм

Zero overhead — `readonly` только compile-time проверка, не влияет
на codegen. ABI `readonly []u8` = `NovaArray_uint8_t*` (идентично `[]u8`).

### Применение

`str.as_bytes() -> readonly []u8` — zero-copy view в UTF-8 буфер строки
без memcpy. UTF-8 invariant защищён: записать в буфер нельзя.

### Параметры функций (Plan 108.1)

**Default = read-only.** Параметр без явного модификатора эквивалентен
`readonly param T` — callee может только читать, не вызывать `mut`-методы,
не присваивать через индекс.

```nova
fn f(b []int) { b.push(1) }       // ✗ E_PARAM_NOT_MUT — нет `mut`
fn f(mut b []int) { b.push(1) }   // ✓ explicit mut
fn f(ro b []int) { ... }    // ✓ synonym default (для документации)
fn f(consume b []int) { ... }     // ✓ owned move — mut по умолчанию
```

**Правила сочетания модификаторов:**

| Сочетание | Результат |
|---|---|
| `param T` | readonly (default) |
| `mut param T` | mutable view |
| `readonly param T` | readonly (явно) — synonym default |
| `consume param T` | owned move, mut by default |
| `mut consume param T` | ✗ parser-level `E_PARAM_MOD_CONFLICT` |
| `consume mut param T` | ✗ parser-level `E_PARAM_MOD_CONFLICT` |
| `mut readonly param T` | ✗ parser-level `E_PARAM_MOD_CONFLICT` |
| `readonly mut param T` | ✗ parser-level `E_PARAM_MOD_CONFLICT` |

**Coercion (передача аргумента в параметр).**

После Plan 108.1 `T` в позиции параметра **уже readonly по умолчанию**.
Поэтому `readonly T → T (param)` — это `readonly → readonly` (тождество),
а единственное реальное нарушение это `readonly → mut`:

| caller-type → callee-param-type | OK? |
|---|---|
| `T → T` (param default readonly) | ✓ (caller-T → callee-readonly = сужение) |
| `T → readonly T` (param explicit readonly) | ✓ (synonym default) |
| `T → mut T` (param explicit mut) | ✓ (caller разрешает mut доступ) |
| `readonly T → T` (param default readonly) | ✓ — оба readonly |
| `readonly T → readonly T` | ✓ |
| `readonly T → mut T` (param explicit mut) | ✗ `E_READONLY_COERCE` — единственное нарушение |
| `mut T → T` (param default readonly) | ✓ (сужение, mutable можно показать как readonly) |
| `mut T → mut T` | ✓ |

**Closure-параметры** — аналогично функциональным.

### Закрытые маркеры (Plan 108.1)

- ✅ `[M-108-readonly-mut-method-check]` — вызов `mut`-метода на
  параметре без `mut` теперь даёт `E_PARAM_NOT_MUT`.
- ✅ `[M-108-readonly-coerce-on-param]` — closed **дефакто**:
  старая формулировка маркера предполагала, что param `T` mutable;
  после Plan 108.1 param `T` уже readonly, поэтому coerce `readonly T →
  T (param)` — это `readonly → readonly` (no violation).  Единственный
  остаточный case — `readonly T → mut T (param explicit)` — отдельный
  followup `[M-108.1-readonly-to-explicit-mut-coerce]` (узкий нишевый
  сценарий, не блокирует).

### Связь
- [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindinga-readonly-для-never-mut) — `readonly field` предшественник
- [D175](#d175-readonly-field--полный-freeze-амендмент-d36) — readonly field enforcement
- [D144](#d144-sub-slice-views-для-t-и-str--arra-b--sa-b) — слайсы `arr[a..b]`
- [D157](#d157) — view-borrow для consume-типов (Plan 108.1 распространяет принцип на не-consume)
- Plan 108 — реализация D175/D176
- Plan 108.1 — params readonly by default + закрытие 2 markers

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

#### `fn[T] ReceiverType @method` префикс (Plan 101.1 partial, 2026-05-24)

Generic-параметры также декларируются через **`fn[T]` префикс** —
для receiver'ов без carrier-brackets (`[]T`, bare T, tuple). Параллель
[D145](#d145-fnt-префикс--receiver-generic-decl--bounds-plan-101).
Bound syntax из D72 применим в этой позиции — `fn[T Hashable] []T @method`.

```nova
fn[T] []T @map[U](f fn(T) -> U) -> []U          // T через fn[T] (нет carrier)
fn[T Hashable] []T @dedup() -> []T              // bound в fn[T] (D72 + Plan 101.2)
```

**Plan 101.1 status (2026-05-24):** parser + базовый codegen работают
для `[]int` element type. Codegen mono-per-T для других element-types
(`[]str`, `[]User`) — известная limitation, marker
`[M-fn-prefix-int-only-mono]`, deferred ~4-6h follow-up.

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
    ro buckets []Slot[K, V]
}

type Set[T Hashable] {
    ro inner HashMap[T, ()]
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
ro m HashMap[User, str] = HashMap.new()       // ok
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
    ghost ro n = xs.len()      // spec-only: виден в invariant
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

- ✅ Mono path для bound methods works (HashMap.clone() пример).
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
>
> **Plan 59.1 amend (2026-06-01):** general generic anonymous tuple
> monomorphization — `fn[T] f() -> (A[T], B[T])` — закрывает gap в
> Plan 59 Ф.7.5. Schema `_NovaTuple_<arity>_<L1>_<T1>_..._<LN>_<TN>`
> (length-prefixed) теперь применяется не только к Result, но к любому
> generic anonymous tuple в return position. См.
> [D216](#d216-generic-anonymous-tuple-monomorphization) для full spec.

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
ro p (str, int) = ("a", 1)
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
- ~~**Named tuple fields** (`(x: T1, y: T2)`) — **ОТКЛОНЕНО окончательно
  (Plan 59 Ф.7.4, 2026-05-21).** Именованные поля кортежа почти
  идентичны record'у; заводить два почти одинаковых синтаксиса для
  одной семантики в Nova нет причин. Нужен агрегат с именованными
  полями — это record (`type T { x int, y int }`). Tuple остаётся
  позиционным (`.0`/`.1`).~~
  **✅ REOPENED (Plan 120, 2026-05-31).** Отклонение основывалось на
  неполном reasoning: tuple и record имеют _fundamentally different_
  allocation semantics (D32: stack vs heap). Named tuple fields не
  эквивалентны record — они value types с именованным доступом,
  zero GC overhead. See [D215](#d215-named-tuple-fields--valuereference-allocation-contract).
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

## D215. Named tuple fields + value/reference allocation contract

> **Status:** active (spec, 2026-05-31). Реализация — [Plan 120](../../docs/plans/120-named-tuples-and-allocation-contract.md).
> Extends [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-) tuple form;
> amends [D32](#d32-семантика-передачи-параметров) с explicit value/reference taxonomy;
> amends [D123](#d123-tuple-monomorphization) с named field codegen.
> Withdraws Plan 59 Ф.7.4 rejection (corrected reasoning).

### Что

Extension D52 tuple form: поля кортежа могут быть **именованными**
(parallel с positional). Named tuple — **stack-allocated value type**
с именованным доступом (`.x`, `.y`), identical performance к
positional tuple (D123).

**Allocation contract — bracket choice кодирует semantics:**

| Синтаксис | Тип | Размещение | Семантика |
|---|---|---|---|
| `type X(T1, T2)` | positional tuple | **stack** | value (копия при передаче) |
| `type X(name1 T1, name2 T2)` | named tuple | **stack** | value (копия при передаче) |
| `type X { name T }` | record | **heap** (GC) | reference (pointer при передаче) |

### Синтаксис

```nova
// Named tuple declarations
type Point(x f64, y f64)
type Vec3(x f64, y f64, z f64)
type Color(r u8, g u8, b u8, a u8)
type Generic[T](value T, count int)

// Construction — named args
ro v = Vec3(x: 1.0, y: 2.0, z: 3.0)
ro c = Color(r: 255, g: 0, b: 128, a: 255)

// Field access — by name
v.x     // 1.0
v.y     // 2.0

// Methods — identical to records
fn Vec3 @add(other Vec3) -> Vec3 =>
    Vec3(x: @.x + other.x, y: @.y + other.y, z: @.z + other.z)
```

### Грамматика (extends D52)

```ebnf
tuple_fields  ::= positional_list | named_list
positional_list ::= type ("," type)*
named_list      ::= named_field ("," named_field)*
named_field     ::= IDENT type

// Mixed positional+named в одном декларации — forbidden (E_TUPLE_MIXED_FIELDS)
```

Parser disambiguation: если после `(` стоит `IDENT type-start` →
named tuple; иначе → positional. Один lookahead, никакого backtracking.

### Type errors

| Ситуация | Ошибка |
|---|---|
| `.0` на named tuple | `E_TUPLE_POSITIONAL_ACCESS_ON_NAMED` |
| `.name` на positional tuple type | `E_TUPLE_NAMED_ACCESS_ON_POSITIONAL` |
| mixed named+positional в declaration | `E_TUPLE_MIXED_FIELDS` |

### Codegen (extends D123)

Named tuple → C named struct (not anonymous):

```c
typedef struct NovaTuple_Vec3 NovaTuple_Vec3;
struct NovaTuple_Vec3 {
    double x;
    double y;
    double z;
};
```

Symbol prefix `NovaTuple_<Name>` distinguishes от positional
`_NovaTuple_<arity>_...` и от records `Nova_<Name>*`. Named tuple
= **value type** (no pointer in C signature); всегда stack-allocated.

### Use cases (recommended patterns)

| Паттерн | Тип | Почему |
|---|---|---|
| Hot-path math (Vec3, Matrix, Quaternion) | named tuple | zero GC, predictable |
| Pixel formats (Color, Pixel) | named tuple | small, copy-cheap |
| FFI multi-value returns | named tuple | stack return, fit в registers |
| Iterator state | named tuple | local-lifetime, no heap |
| Domain entities (User, Order, Account) | record | identity, sharing |
| Large aggregates | record | copy expensive |

### Почему: Plan 59 Ф.7.4 rejection был неполным

Plan 59 rejection (2026-05-21) argued: «named tuples ≈ records,
нет причин иметь два похожих синтаксиса». Reasoning flaw: tuple
и record имеют **fundamentally different** allocation semantics:
- Tuple → **stack**, zero GC pressure, copy semantics
- Record → **heap** (D32, D123), GC-tracked, reference semantics

Разные allocation characteristics = разные performance + lifetime
characteristics = different syntactic forms **justified**. Plan 120
(2026-05-31) reopens с corrected reasoning.

### Out of scope (followups)

- `[M-120-positional-fallback]`: allow `.0`/`.1` на named tuples
  (Rust-style fallback). V1 = Option B: forbid (Q120 decision).
- `[M-120-named-positional-mix]`: mixed positional+named в одном decl.
- `[M-120-stack-arrays]`: stack-allocated fixed-size arrays `[3]Vec3`.

### Связь

- [D32](#d32-семантика-передачи-параметров) — value vs reference taxonomy (amended)
- [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-) — tuple syntax (amended + named form)
- [D123](#d123-tuple-monomorphization) — positional tuple codegen (named form extends)
- [Plan 120](../../docs/plans/120-named-tuples-and-allocation-contract.md) — реализация

---

## D216. Generic anonymous tuple monomorphization

> **Status:** active (spec, 2026-06-01). Реализация — [Plan 59.1](../../docs/plans/59.1-generic-anon-tuple-mono.md).
> Extends [D123](#d123-tuple-monomorphization) с generic-aware substitution path.
> Closes gap в Plan 59 Ф.7.5 (Result mono landed; general generic anonymous
> tuple оставался под V1 erasure fallback до 2026-06-01).

### Что

Generic anonymous tuple в return position функции с type-параметрами —
`fn[T] f() -> (A[T], B[T])` или `fn[T, U] g() -> (T, U)` — мономорфизируется
per instantiation. Element types конкретизируются через
`current_type_subst`, получают C-name через `type_ref_to_c`, регистрируются
через `register_mono_tuple`, и emit'ятся как unique typedef'ы per element
combination.

```nova
fn[T] dup(v T) -> (T, T) => (v, v)

test {
    ro (a, b) = dup[int](42)           // → _NovaTuple_2_8_nova_int_8_nova_int
    ro (s, t) = dup[str]("hi")          // → _NovaTuple_2_8_nova_str_8_nova_str
    // Два разных typedef'а в одной compilation unit, каждый с real types.
}

fn[T, U] pair(a T, b U) -> (T, U) => (a, b)
ro (i, s) = pair[int, str](7, "x")      // → _NovaTuple_2_8_nova_int_8_nova_str
```

### Правило

#### Mangling schema

Length-prefixed mangling: `_NovaTuple_<arity>_<L1>_<T1>_<L2>_<T2>...`

- `<arity>` — количество элементов
- `<Li>` — длина sanitized C-name i-го элемента
- `<Ti>` — sanitized C-name (точки/звёздочки заменены на `_`,
  pointer suffix retained как `_p`)

Примеры:
- `(int, int)` → `_NovaTuple_2_8_nova_int_8_nova_int`
- `(str, bool)` → `_NovaTuple_2_8_nova_str_9_nova_bool`
- `(ChanWriter[T], ChanReader[T])` после mono[T=int] →
  `_NovaTuple_2_18_Nova_ChanWriter_p_18_Nova_ChanReader_p`

**Length prefix обязателен** — без него parsing неоднозначен для nested
tuples (tuple of tuples) и user types с underscores в имени.

#### Per-instantiation deduplication

`mono_tuple_instances` (HashSet) хранит set element-type vectors.
`register_mono_tuple([elem1, elem2, ...])` идемпотентен — повторные
вызовы с same elements не emit'ят дубликаты typedef'а.

#### Finalize emit (typedef ordering)

В module finalize все registered tuples emit'ятся с topological sort'ом
(внутренний tuple раньше outer'а):
- Tuple A depends on tuple B если B's mangled name появляется как element
  type в A → emit B first.
- Cycle detection: impossible для value-tuple struct'ов; если обнаружен —
  emit anyway без depth-check (no hang).

#### Codegen в emit_call

1. Call-site `f[T1, T2, ...](args)` lookups `mono_fn_decls[f.name]`.
2. `resolve_mono_type_args` строит type_subst из turbofish + arg-inference.
3. `compute_mono_name(base, subst)` → unique mono fn name.
4. `register_mono_instance` enqueue в worklist.
5. Args emit без erasure boxing (concrete types).
6. Variable type at call site = mono'd tuple via `type_ref_to_c(return_type)`
   с активным `current_type_subst`.

#### Body emission (emit_monomorphized_fn)

`current_type_subst` устанавливается перед body emit; `type_ref_to_c(TypeRef::Tuple)`
возвращает mono'd name; tuple-литералы emit'ятся как value-struct
compound literals (no heap-box).

#### Destructure

`emit_tuple_destructure` использует actual mono'd return type для temp
variable (получает через `infer_expr_c_type`). Element types парсятся
через `parse_mono_tuple_elements` (length-prefixed inverse). Arity
mismatch → Nova-level diagnostic с pattern/scrutinee arity (Plan 59 Ф.7.1).

#### Value semantics, no heap-box

Mono'd tuple — **value type** (C struct), passed by value, returned by
value. No heap allocation для anonymous tuple wrapper'а (Result mono Ф.7.5
parity). Element pointers (если elements — pointer types) остаются
heap-allocated независимо.

### Edge cases (covered V1)

- ✅ **Multi-instantiation:** same fn → разные T → unique typedef'ы per
  instantiation.
- ✅ **Multi-param tuple:** `fn[T, U] pair(a T, b U) -> (T, U)`.
- ✅ **Nested generic tuple:** `fn[T] nest() -> (T, (T, T))` — recursive
  subst через `register_tuples_in_typeref`.
- ✅ **Tuple-in-Option:** `fn[T] f() -> Option[(T, T)]` — Option mono +
  inner tuple mono.
- ✅ **Tuple-in-Result:** уже работает (Plan 59 Ф.7.5).
- ✅ **Non-generic tuple:** `fn make() -> (int, str)` — без T,
  substitution тривиален, мономорфизация single instance.
- ✅ **Arity 3+:** `fn[T] triple() -> (T, T, T)` — generic mangling
  параметризован по arity.
- ✅ **Positional field access:** `pair.0` / `pair.1` после mono.

### Edge cases (V1 limitations — followups)

- 🟡 **`[M-59.1-array-of-mono-tuple]`:** `fn[T] f() -> []((T, T))` —
  array-of-mono-tuple. Body falls back на `NovaArray_nova_int*` (boxed
  pointer storage, как records/sums в bootstrap), call-site infer
  выдаёт `NovaArray_<mono_tuple>*` (typedef которого не существует).
  Mismatch → CC-FAIL. Fix: align infer с body fallback ИЛИ packed
  `NovaArray_<mono_tuple>` typedef + element retrieval cast. Низкий
  приоритет — workaround через explicit Nova_<Pair> record type.

- 🟡 **`[M-59.1-tuple-field-oob-nova-diag]`:** `pair.5` на arity-2 tuple
  leaks к C-level error «no member named 'f5'». Should be Nova-level
  diagnostic в type-checker. Cosmetic — error caught, но not optimal UX.

- 🟡 **`[M-59.1-channel-new-cleanup]`:** Channel.new продолжает использовать
  3 ad-hoc special-case branches в emit_c.rs:18435/20159/22694 +
  Nova_ChannelPair runtime struct. После Plan 59.1 generic mono path
  **способен** обработать Channel.new если добавить Nova-side declaration
  `fn[T] Channel[T].new(cap int) -> (ChanWriter[T], ChanReader[T])` через
  external fn (Plan 115 Pattern B). Cleanup deferred to отдельный план
  (runtime + std API surgery). Spec D91 signature остаётся
  буквальной реальностью после cleanup'а; до того — implementation
  detail, aspirational notation.

### Backward compatibility

- Все existing non-generic anonymous tuple usages (`(int, str)` returns,
  destructures) — продолжают работать unchanged. Plan 59 Ф.7.5 mono'd
  path был активен только для Result; теперь активен для всех anonymous
  tuples.
- Plan 59 Ф.7 legacy `_NovaTuple<arity>` schema (без underscore — nova_int
  placeholders) технически остаётся как fallback в `type_ref_to_c` для
  cases где type_subst не доступен (degenerate case — non-generic context
  с unresolved tuple). На практике не наблюдается после fix.

### Cross-refs

- [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-) — anonymous tuple type syntax.
- [D123](#d123-tuple-monomorphization) — positional tuple codegen baseline (Plan 59 Ф.7.5).
- [D215](#d215-named-tuple-fields--valuereference-allocation-contract) — named tuple types (Plan 120 D215, ortho к D216).
- [D91](06-concurrency.md#d91-channel-revision--capability-split-на-chanwriter--chanreader) — Channel.new signature now буквально implementable; cleanup ad-hoc paths — [M-59.1-channel-new-cleanup].
- [D141](08-runtime.md#d141-примитивы-доступа-к-памяти--byte_at--bulk-slice-операции) — bulk slice-операции (orthogonal к tuple mono).
- [Plan 59.1](../../docs/plans/59.1-generic-anon-tuple-mono.md) — implementation plan.
- [Plan 59 Ф.7.5](../../docs/plans/59-tuple-monomorphization.md) — Result mono prior art.

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
ro w = Wrapper[int].of(5)
ro a = w.map(|x| x * 2)              // (T=int, U=int) instance
ro s = w.map(|x| str.from(x))        // (T=int, U=str) instance
ro s2 = s.map(|x| x + "!")           // (T=str, U=str) instance
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

Канонический use-case — `Transaction.commit() / .rollback()`,
`File.close()`, lock-guard `.release()`.

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
| `f(tx)` где `f(consume tx Tx)` — consume-param | `tx` → `Consumed` |
| `f(make_tx())` где `f(consume t Tx)` — rvalue → consume-param | rvalue ownership передаётся напрямую (без binding) ✅ |
| `return tx` (тип consume) | `tx` → `Returned` (передача caller'у) |
| `record.field = tx` где field declared consume | `tx` → `Moved` (в record) |
| `consume new_owner = tx` (transfer alias) | `tx` → `Consumed`, `new_owner` → `Live` |
| `f(tx)` где `f(tx Tx)` — view-param (no qualifier) | `tx` остаётся `Live` (callee — view-borrow) |
| `f(make_tx())` где `f(t Tx)` — rvalue → view-param | ❌ E (D133-consume-rvalue-in-view) |
| `f(tx)` где `f(mut tx Tx)` — mut-view-param | `tx` остаётся `Live` (callee — mut-borrow) |
| `f(make_tx())` где `f(mut t Tx)` — rvalue → mut-view-param | ❌ E (D133-consume-rvalue-in-mut-view) |
| `let alias = tx` — view-alias | оба в alias-class (Plan 73); consume любого инвалидирует |
| `let mut alias = tx` — mut-view-alias | то же + mut-методы через alias |
| `let _ = tx` (silent drop) | ❌ compile error D133-suppress-not-allowed |

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
- `consume f int` (тип поля **не** consume) → error E (D133-marker-on-
  non-consume) — keyword использован но не нужен.

**`consume`-type БЕЗ consume-полей разрешён** — каноничный паттерн
для opaque-resource типов (`StringBuilder consume` с runtime backing
через `external type`; consume-method `@into()` потребляет; никаких
consume-полей в декларации). Достаточно хотя бы одного declared
consume-метода.

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
    consume new_file = File.open()?            // сначала добываем замену
    @file.close()                               // только теперь закрываем старое
    @file = new_file                            // rebind — @file опять Live;
                                                //  new_file → Consumed (transfer в @file)
}                                               // mut exit: @file Live ✅
```

Compiler ловит реальные баги:
- забытый rebind на ветке → exit MaybeConsumed → error.
- early return без rebind → error.
- наивный close-then-open с error-path (`@file.close(); @file = open()?`)
  → error если open Err (@file Consumed, не rebinded).

### Assign в Live consume-поле / locals — запрещено

Прямое присваивание `@field = expr` разрешено **только** когда `@field`
уже `Consumed` (для simple-typed consume-поля) либо **все consume-sub-
fields внутри `@field`** уже `Consumed` (для nested-consume-record-поля).
Иначе compile error E (D133-assign-live-field).

```nova
fn Service mut @overwrite_naive() {
    @file = File.open()?                       // ❌ @file Live, silent overwrite
}

fn Service mut @overwrite_correct() {
    @file.close()                              // @file → Consumed
    consume new = File.open()?
    @file = new                                // ✅ @file Consumed → assign OK
}
```

**Nested case** — `@inner` содержит `consume tx`; assign в `@inner`
разрешён когда внутренний `@inner.tx` уже Consumed (recursively для
deep nesting):

```nova
fn Outer mut @reset() {
    @inner.tx.commit()                         // @inner.tx → Consumed;
                                               //  @inner effectively «empty container»
    consume new = Inner.new()
    @inner = new                               // ✅ all consume-sub-fields Consumed
                                               //  → @inner replace OK
}
```

То же для локальных consume-var: повторный `consume tx = ...` без
consume старой — error.

### Nested field paths

Multi-level field tracking — `ConsumeCtx` хранит state по произвольно
глубокому пути `@f1.f2.f3`:

```nova
type Inner consume { consume tx Transaction }
type Outer consume { consume inner Inner }

fn Outer mut @commit_inner() {
    @inner.tx.commit()                         // deep path consume; @inner.tx → Consumed
                                               //  @inner — «empty container» (consume-sub-field Consumed)
    consume new = Inner.new()
    @inner = new                               // rebind inner — assign OK
                                               //  (внутренний tx был Consumed)
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

### Три mode'а binding-position: view / mut-view / consume

**Единое правило везде** (param / for / match / if-let / let-binding):
**`consume` keyword маркирует ownership**. Без него — **view** (read-
only borrow). `mut` — view + mutation.

```nova
fn read(tx Transaction) -> int                 // view (default; callee читает)
fn modify(mut tx Transaction)                  // mut-view (+ mut методы)
fn close(consume tx Transaction)               // consume (transfer; tx → Consumed)
```

#### View (default — без qualifier'а)

| Действие | OK? |
|---|---|
| `tx.field` (read) | ✅ |
| `tx.regular_method()` | ✅ |
| `t.mut_method()` | ❌ (нужен `mut tx`) |
| `t.consume_method()` | ❌ E (D133-consume-via-view) |
| передача в view-param другой fn | ✅ |
| передача в `consume`-param | ❌ E (D133-move-via-view) |
| передача в `mut`-param | ❌ (нужен `mut tx`) |
| `return tx` (escape) | ❌ E (D133-view-escape-return) |
| store в record-field | ❌ E (D133-view-escape-store) |
| capture в closure, returned | ❌ E (D133-view-escape-closure) |
| `let alias = tx` (alias) | ✅ view-alias (Plan 73) |

#### Mut-view (`mut tx` qualifier)

То же что view, но **mut-методы разрешены**. Не consume, не escape.

#### Consume (`consume tx` qualifier)

Полный ownership-transfer. Callee/binding обязан consumed до scope-
exit'а через один из 5 механизмов (см. §«Когда consume binding
считается удовлетворённым»).

#### Consume-rvalue в arg-position (без binding)

Прямой call `f(make_tx())`, где `make_tx() -> Tx consume` возвращает
fresh consume-owner, **без сохранения через `consume name = …`** —
правила по qualifier'у callee-param:

| Callee param | OK? |
|---|---|
| `f(consume t Tx)` — consume-param | ✅ ownership передаётся напрямую; callee обязан consumed внутри |
| `f(t Tx)` — view-param (default) | ❌ E (D133-consume-rvalue-in-view) |
| `f(mut t Tx)` — mut-view-param | ❌ E (D133-consume-rvalue-in-mut-view) |

**Почему запрет на view / mut-view:** view/mut-view-param **не
consume'нят** callee-стороной. После возврата из `f` rvalue остаётся
не consumed и не bound к локальной переменной → flow-checker не имеет
slot'а в `ConsumeCtx` для tracking'а → must-consume gate его не
увидит → ресурс утечёт молча. Запрет — единственное безопасное
правило: consume-value требует именованного owner'а либо немедленной
передачи ownership через consume-param.

**Hint в diagnostic:** «привяжи через `consume name = make_tx()`,
затем `f(name)`; после consume-method/consume-param/return name
будет Consumed». Альтернатива — заменить sig `f` на consume-param,
если callee действительно должен потребить.

**Цепочки** (`g(f(make_tx()))`) — рекурсивно: rvalue-результат `f`
анализируется по тому же правилу для соответствующего param'а `g`.
Если `f` возвращает consume-value, а `g`-param это view → error на
внешнем вызове.

#### Глубокий peek без consume

```nova
match @file {                                  // view-match (default)
    Some(f) => f.fd,                           // f: view File, read-only
    None => 0,
}
// @file остаётся Live ✅
```

См. **D157** (Plan 100.3) — match-pattern в view-mode + closure capture
analysis.

### `consume` + `-> @` несовместимы

`fn Tx consume @prepare() -> @ { ... }` → **parse error**. Противоречие
между «забираю целиком» и «возвращаю тот же объект» (D132 fluent-
return).

### Binding: `consume` keyword обязателен для ownership

Для consume-типов **`consume` keyword обязателен** в LHS, когда binding
становится Live-linear-owner:

```nova
ro tx = begin()                               // ❌ ERROR D133-consume-needs-keyword:
                                               //    consume-type требует `consume` keyword

consume tx = begin()                           // ✅ initial binding — owns

ro alias = tx                                 // ✅ view-alias (no ownership; Plan 73)
mut alias = tx                             // ✅ mut-view-alias
consume new_owner = tx                         // ✅ transfer: tx → Consumed
```

**Без `consume` keyword'а LHS = view-alias** (alias-class Plan 73,
read-only borrow). Это симметрично param/for/match — везде «no qualifier
= view, consume = transfer».

#### Когда consume binding считается удовлетворённым

Live consume-binding обязан к scope-exit'у оказаться в одном из 5
состояний:

1. **Closed locally** — `tx.commit()` (consume-метод).
2. **Returned** — `return tx`.
3. **Transferred** — `f(tx)` где `f(consume tx T)`.
4. **Stored in record-field, который сам уходит наверх:**
   ```nova
   consume tx = begin()
   return Wrapper { tx: tx }                  // tx → record-field, record returns
   ```
5. **Covered by defer/errdefer/okdefer** (D158-D162 Plan 100.4 family).

Иначе error E (D133-not-consumed).

### AI-first explicit-ness — почему mandatory

`consume` keyword обязателен **специально** — для loud visibility:
- 🟢 Каждое появление ownership видно с первого взгляда.
- 🟢 Refactor-safety — добавил `consume` к типу → compiler ловит все
  существующие `let x = T.new()` sites, force review.
- 🟢 Единое правило симметрии с param / for / match.

Verbose-ness bounded — только для consume-типов (rare; resource-
management).

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
- **Strict-mode binding-form (`let tx =` «обязан передать наверх» vs
  `consume tx =` «обязан закрыть здесь»)** — отвергнуто (overspec,
  refactor friction). Финальная модель: `consume` keyword mandatory
  для ownership; `let` для consume-types = error либо view-alias (в
  alias-position).
- **`view T` keyword как explicit qualifier** — отвергнуто (default-
  view достаточно). `view` mode = absence of `consume`/`mut`
  qualifier (см. D157 Plan 100.3).
- **Implicit `_ = tx` discard** — суррогат suppress; force compile-
  error.

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

Плюс — **collection-aware iteration с 3 mode'ами** (unified с D133):
`for tx in vec` (view default) / `for mut tx in vec` (mut-view) /
`for consume tx in vec` (consume, vec → Consumed).

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

### Collection-aware iteration — 3 mode'а

Симметрично D133 param/match mode'ам:

```nova
consume tx1 = begin()
consume tx2 = begin()
consume txs = [tx1, tx2]                       // []Transaction — generic-заразность (D133 D6)
                                               // txs владеет (consume keyword обязателен)

// View (default) — read-only, vec stays Live:
for tx in txs {
    println(tx.id)                             // ✅ read field
    // tx.commit()                             // ❌ view → не consume-метод
}
// txs Live после for; нужно consume другим способом.

// Mut-view — vec stays Live, элементы mutated in-place:
for mut tx in txs {
    tx.update()                                // ✅ mut method
}
// txs Live, элементы updated.

// Consume — consume каждое, vec → Consumed:
for consume tx in txs {
    tx.commit()                                // ✅ consume-метод
}
// txs → Consumed после for ✅
```

Loop-handling pragmatic: `for consume tx in vec` помечает vec Consumed
после loop (даже если break early — D161 multi-defer LIFO error
accumulation gracefully handles partial-consumed state).

Каждый `tx` в arm-теле проверяется стандартным `check_consume`
правилом для соответствующего mode'а (view / mut-view / consume).

### Alternative consume-methods для collection

Чтобы consume collection без iteration:
- `vec.pop() -> Option[T]` — single-element consume (Option auto-
  consume через D133 D6 generic-заразность).
- `vec.drain() -> Iter[T]` — consume через iterator.
- `vec.into_first() -> T` consume-method record'а возвращает один
  элемент (consume rest internally).

stdlib audit (Plan 100.7) аннотирует эти методы с `[T consume]` bound.

### Generic propagation для HOF (map/filter/fold)

Closure-параметры HOF используют те же 3 mode'а через qualifier:

```nova
fn map[T consume, U consume](consume items []T, f fn(consume T) -> U) -> []U
fn filter[T consume](consume items []T, f fn(t T) -> bool) -> []T
//                                          ^^^ — view (default; read-only)
fn for_each[T consume](consume items []T, f fn(consume T) -> ())
fn modify[T consume](mut items []T, f fn(mut T) -> ())
//                                       ^^^^ — mut-view (in-place modify)
```

`filter` использует view-closure (default) — predicate читает T без
consume. `map` consume'ит каждое T → producer'ит U. `modify` mut-view
для in-place.

Compiler enforces consume-handling в closure-body через generic-bound
propagation + view-default rules.

### HashMap / user-generic propagation

`type_is_consume` рекурсивно (D133 D6): wrapper'ы с consume-arg сами
становятся consume:

```nova
consume tx_map = HashMap[str, Transaction].new()
                                               // ↑ Transaction consume → HashMap consume
                                               //   через generic-заразность
                                               //   consume keyword обязателен (D133)
tx_map.insert("a", consume begin())            // insert требует consume value (transfer)
// На scope-exit tx_map должен быть Consumed (через consume-метод HashMap).
for consume (_, tx) in tx_map.drain() {        // consume через drain-iteration
    tx.commit()
}
```

HashMap (и другие collection API) — должны аннотировать `[V consume]`
на методах, манипулирующих consume-values (`insert(k K, consume v V)`,
`remove() -> Option[V]`, `drain() -> Iter[(K, V)]`, etc.). Migration
audit — часть Plan 100.7.

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

## D163. FFI consume integration — type-driven, без отдельного keyword'а

> **🔴 RETRACTED 2026-05-30 (Plan 91.10).** `needs <Cap>` syntax удалён.
> Capability tracking via отдельный mechanism — **redundant** с effect system
> (Plan 33). Structurally `needs Cap` ≡ effect-без-операций: same propagation,
> same static tracking, different syntax. Если в будущем понадобится
> capability gating — вводить как formal effect declarations
> (`type Fs effect { ... }`) с handler'ами. Конкретный pain: `consume`
> (ownership/linearity) vs capability (authority) — orthogonal concerns,
> D163 их жёстко связал. См. [docs/plans/91.10-d163-retract-capability-syntax.md].
>
> **Что осталось от D163:**
> - `external type X consume` в любом module — продолжает работать (D126 +
>   опаковая FFI-семантика).
> - `consume` keyword на параметрах external fn — продолжает работать (D131
>   ownership). Но external fn остаётся stdlib-only (D82) — user-module
>   external fn через D163 capability path больше не валидны.
>
> **Plan 100.5 historical original record:** Принято 2026-05-23. Ред. 2
> (2026-05-24): drop `external consume fn` keyword. Ред. 3 (2026-05-27):
> **РЕАЛИЗОВАНО** — parser `needs` clause, type-checker D163-missing-cap,
> C codegen стабы. Extends [D82](08-runtime.md#d82) `external fn` +
> [D126](03-syntax.md#d126) `external type` + [D63](04-effects.md#d63)
> capability.
>
> **Удалено (Plan 91.10):**
> - Parser `needs` clause (hard error w/ migration hint).
> - `check_external_fn_needs_caps` (D163-missing-cap diagnostic).
> - `emit_d163_external_stub` (C codegen стаб generator).
> - `FnDecl.needs_caps` AST field — сохранён как always-empty, удаление
>   followup `[M-91.10-remove-needs-caps-field]`.
> - Test fixtures `nova_tests/plan100_5/external_*` (6 files) и
>   `nova_tests/plan100_7/{file_open_read_close,mutex_lock_release,
>   socket_listen_accept}.nv` (3 files).
>
> Текст ниже — historical reference для контекста.

### Что

Никакого нового keyword'а для external fn — **унифицировано с regular fn**:
return-type carrying consume-ness (через D133 type-decl `consume`)
автоматически передаёт ownership caller'у. `consume` keyword
используется только на параметрах/receiver'ах (D131 semantic).

```nova
// Opaque consume-type (D126 + D133):
external type File consume
external type Mutex consume
external type Socket consume

// Return consume-type → caller получает ownership (через type, не keyword):
external fn nova_file_open(path str) -> File
    needs Fs                                    // capability required (D63)

// Param-side consume — D131 semantic, тот же keyword `consume` на param:
external fn nova_file_close(consume f File)
    needs Fs

// Result wraps consume — generic-заразность из D133 D6:
external fn nova_open(path str) -> Result[File, IoErr]
    needs Fs
// Caller обязан consume Result через match-Ok-arm.
```

### Зачем drop keyword

Параллель с regular fn:

```nova
fn factory() -> Transaction => Transaction.new()
//              ^^^^^^^^^^^ — return type carries consume-ness. NO `consume`
//                            keyword on fn declaration.

fn finish(consume tx Transaction) -> () { ... }
//        ^^^^^^^ — consume on PARAM (D131).
```

Применяем то же к external — symmetry без нового keyword'а.

### Capability requirement (D63)

`external fn` касающийся OS resource обязан declare capability —
это **независимо** от consume-семантики (общее правило D63):

```nova
external fn nova_file_open(path str) -> File
    needs Fs                                    // OS access → cap required

external fn nova_socket_accept(consume srv ServerSocket) -> ClientSocket
    needs Net
```

Capability и consume — две ortogонные concern. Capability для OS
privilege; consume для ownership. Combined через type-decl + needs-clause.

### C runtime defensive helpers

C-side `nova_file_close(consume f File)` обязан:
- `nv_consume_validate(f)` — assert `f != NULL` на entry.
- После работы — `memset` поля `File*` в zero / NULL (defense-in-depth
  per D131 Plan 73 pattern).

Это даёт двойную защиту: compile-time (D133 check_consume) + runtime
(NULL-deref panic на use-after-consume).

### Generic-заразность через FFI — uniform

```nova
external fn nova_open() -> Result[File, IoErr] needs Fs
//                         ^^^^^^^^^^^^^^^^^^^ — Result consume через generic-arg
// Caller обязан consume Result (через match Ok-arm с consume File).
```

Никакого FFI-специфичного правила — общее D133 D6 generic-заразность.

### Cross-fiber FFI safety

FFI-call может суспендиться (libuv async I/O). Plan 47/22/49 fiber infra
preserves consume-state через migration; D163 verify через runtime tests
(Plan 100.5 Ф.6).

### Сравнение

| Capability | Rust | Kotlin/JNI | Go cgo | TS Node N-API | Nova D163 |
|---|---|---|---|---|---|
| Ownership через FFI | ✅ `unsafe fn` + manual contract | ⚠️ manual | ⚠️ manual | ⚠️ manual | ✅ **type-driven, без extra keyword** |
| Auto-close на panic при FFI handle | ✅ через Drop wrapper | ⚠️ try-finally | ⚠️ defer | ⚠️ try-finally | ✅ **через D162** |
| Capability tracking | ⚠️ `unsafe fn` | ⚠️ manual | ⚠️ manual | n/a | ✅ **D63 needs-clause** |
| `unsafe` keyword нужен | ✅ да | n/a | n/a | n/a | ❌ **нет** (D6) |
| Уникальный FFI-syntax | ⚠️ unsafe fn | ⚠️ JNI prefix | ⚠️ cgo annotation | ⚠️ napi macro | ✅ **унифицировано с regular fn** |

Nova **превосходит Rust** — (a) нет `unsafe` keyword (D6 + D63
capability); (b) уни­фи­цировано с regular fn (одна mental model для
FFI и Nova-side functions).

### Что отвергнуто

- **`external consume fn` keyword** (Ред. 1) — избыточный, return-type
  уже carries consume-ness. Drop в Ред. 2.
- **Vacuous-marker warning** (Ред. 1 W D163-vacuous-consume) —
  отпадает вместе с keyword.

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
ro h = effect   Cron { run() => spawn_cron() }   // value of type Effect[Cron]
ro p = protocol Fan  { run() => spin_blades() }  // value реализующее Fan
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
ro h = effect Db {
    query(q) => mock_rows()
}
with Db = h { ... }

// protocol-литерал (value реализующий контракт) — instance-only
ro l = protocol Locker { lock() => state.lock() }
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
    ro state = Cell { value: initial }
    ro r = protocol Reader { read() => state.value }
    ro w = protocol Writer { write(v) { state.value = v } }
    (r, w)
}

// caller:
ro (r, w) = Cell.new(10)
ro initial = r.read()    // 10
w.write(99)
ro after = r.read()      // 99 — shared state через protocol-литералы
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

## D144. Sub-slice views для `[]T` и `str` — `arr[a..b]` / `s[a..b]`

> **Источник:** Plan 96 (2026-05-23). Закрывает Q-array-slicing,
> Q-array-api.5, D27 §1663 drift («Слайсинг отложен»), D27 §1632 drift
> (raw `arr[i]` без bounds-check). **Зависит от** [D6](05-memory.md#d6)
> non-moving GC; [D58](03-syntax.md#d58) Range; [D27](03-syntax.md#d27)
> `[]T` API; [Plan 90 / D141](08-runtime.md#d141) bulk-ops.

### Семантика — sub-slice view

`arr[range]` где `range : Range` возвращает **view** — новый
24-байтовый header `NovaArray_T*` с `data = orig->data + from`,
`len = cap = to - from`. **Без копии данных backing'а** (O(1) creation).

`str[range]` возвращает codepoint-indexed view (двухпроходный walk
UTF-8 → byte offsets; structurally идентично `nova_str_slice`, но с
**panic при OOB** вместо clamp).

### 5 форм Range (Rust `RangeBounds` parity)

| Форма | Семантика | Open-ended? |
|---|---|---|
| `arr[a..b]` | exclusive: `[a, b)` | нет |
| `arr[a..=b]` | inclusive: `[a, b]` | нет |
| `arr[a..]` | от `a` до конца | да (end = `len`) |
| `arr[..b]` | от начала до `b` | да (start = 0) |
| `arr[..]` | весь массив | да |

Open-ended формы — **только в slice-context** (`arr[range]`). В
materialize / for-loop / quantifier / parallel-for они отвергаются
с compile-time diagnostic «open-ended Range without bound (Plan 96)».

### Single-type design

`[]T` — **один** тип для owner и view. Нет `Slice[T]` (Rust-модель
раздельных типов). View передаётся в функцию ждущую `[]T` без
дополнительной конверсии.

### `cap == len` invariant

View имеет `cap == len == to - from`. Push на view → realloc (как
обычно при exhausted cap) → view **silent detach** от parent.
Parent backing **никогда** не молча перезаписывается — это устраняет
Go-`append`-footgun без borrow checker'а.

```nova
mut parent = [1, 2, 3, 4, 5]
mut view = parent[1..4]   \ view: [2, 3, 4]
view.push(99)                  \ realloc; view detached
\ parent == [1, 2, 3, 4, 5]   — НЕ затронут
\ view == [2, 3, 4, 99]
```

### Mut-семантика

`mut`-view только от `mut`-источника. Через `mut`-view write идёт в
**shared backing** — изменения видны parent. Несколько `mut`-view
одного backing'а **разрешены** (как в Go); caller responsibility,
никакого borrow checker'а.

### Iterator invalidation

`for x in view` — `len` берётся snapshot'ом в начале цикла (Go-style).
Push на parent во время итерации view'а **не виден** view'у: parent
реаллоцирует, view продолжает указывать на старый backing через
interior-pointer.

### GC requirement — interior pointers stable

**Необходимое условие:** runtime гарантирует stable interior pointers
(non-moving GC, D6). View хранит `data = backing->data + from` — это
указатель **внутрь** backing'а; Boehm (`GC_set_all_interior_pointers(1)`)
держит backing alive по interior-ptr.

Любая будущая замена GC-backend на moving GC требует одновременной
замены slice-представления (separate header struct + ptr-update on
move). Это закрепляется здесь как нормативный invariant.

### Bounds-check

- `from < 0` → panic
- `to < from` → panic
- `to > len` → panic (для str — `to > total_codepoints`)
- Empty slice (`arr[a..a]`) → валиден
- Отрицательные индексы → panic, **не** Python-style wrap

Сообщение panic'а: `"array: slice [N..M] out of bounds for length L"`
(паритет с Go/Rust).

### Также: raw `arr[i]` bounds-check (D27 §1632 drift)

D144 одновременно фиксирует pre-existing drift: codegen `arr[i]`
**теперь** эмитит runtime bounds-check (раньше эмитил голый
`(arr)->data[i]` — controlled buffer overflow на запись, UB на чтение).
Сообщение: `"array: index N out of bounds for length L"`.

### Concurrency / M:N

Slice-view = shared mut backing между fiber'ами в M:N runtime =
**формально UB по [D79](06-concurrency.md#d79)**. В D71 single-threaded
bootstrap — OK по факту. Передача view через `Channel[]T]` или
spawn-capture в M:N — **inherits D79 disclaimer**.

### Header layout

24 байта (`ptr + len + cap`) — тот же что у owner. Не оптимизировано
до 16 байт (которое требовало бы отдельного типа `Slice[T]` — отвергнуто
single-type-design'ом).

### `str[a..b]` — bracket syntax для строк

Bracket-форма унифицирует idiom: `arr[a..b]` ≡ `str[a..b]`.
Codepoint-indexed (как существующий `nova_str_slice` метод).
**Panic при OOB** (consistent с `arr[a..b]`).

Старый `s.slice(a, b)` метод — **сохраняется** с clamp-семантикой
для backwards-compat; align на panic откладывается в Plan 94
(см. `[P-str-slice-clamp-vs-panic]` в `docs/simplifications.md`).

### Verified против

- Go `s[a:b]` — паритет, **без append-footgun**.
- Rust `&[T]` — близко, **без borrow checker** (caller responsibility
  для multi-mut).
- TypeScript `TypedArray.subarray` — паритет.
- Swift `ArraySlice<T>` — **без CoW-disconnect** (view сразу видит mut).
- Python `memoryview` — паритет.

### Связь

- [D6](05-memory.md#d6) — non-moving GC; interior-ptr invariant
  амендится здесь.
- [D27](03-syntax.md#d27) — `[]T` API; §1632 bounds-check (D144 чинит
  drift); §1663 «Слайсинг отложен» (D144 закрывает).
- [D58](03-syntax.md#d58) — Range-литералы; D144 расширяет до 5 форм
  (open-ended).
- [D79](06-concurrency.md#d79) — shared mut между fiber'ами = UB
  в M:N; slice inherits.
- [D141](08-runtime.md#d141) — Plan 90 bulk-ops; работают на view
  автоматически.


## D145. `fn[T]` префикс — receiver-generic decl + bounds (Plan 101)

> **Status:** MOSTLY CLOSED (2026-05-25, ред. 6 — Plan 101.1/2/3/4 ✅,
> 101.5 partial). Plan 101.1 codegen для non-int mono-dispatch — единственная
> deferred edge case (marker [M-fn-prefix-int-only-mono] в simplifications.md).
>
> **Реализовано (Plan 101.1–101.4 + 101.2):**
> - **101.1** ✅ — Parser `fn[T] ReceiverType @method` + 5 disambiguation
>   error codes (E_UNDECLARED_TYPEVAR_IN_RECEIVER, E_BARE_TYPEVAR_NEEDS_PREFIX,
>   E_DUPLICATE_GENERIC_DECL, E_PREFIX_SHADOWS_NAMED_TYPE, E_UNUSED_PREFIX_TYPEVAR).
>   Codegen mono `[]int` element + bare-T + non-int element (через Plan 95
>   array-ext infrastructure). vec.nv migration: 7 методов.
> - **101.2** ✅ — Bound integration: method-call bound enforcement
>   (check_method_call_bounds в types/mod.rs); receiver-generic `fn[T Bound] []T @m`
>   ловит violation на call-site `xs.m()`.
> - **101.3** ✅ — Multi-bound `[T A + B]`: GenericParam.bound → bounds Vec,
>   parser `+ Type` chain, type-check iterate all bounds (conjunction),
>   strict check_generic_bound_declarations (E_BOUND_UNKNOWN /
>   E_BOUND_NOT_PROTOCOL).
> - **101.4** ✅ — Protocol composition `use TypeName` в protocol body:
>   AST TypeDeclKind::Protocol { methods, embeds }, parser parse_protocol_body,
>   type-check flatten DFS + 5 диагностик (E_PROTOCOL_EMBED_{UNKNOWN,
>   NOT_PROTOCOL, CYCLE, DUPLICATE, AFTER_METHOD, NOT_NAMED}).
> - **101.5 partial** — stdlib audit: только vec.nv использует fn[T] prefix
>   (7 методов работают; non-int — deferred). HashMap/PQ/Lru используют
>   carrier-brackets (Plan 15 D72 path, unchanged).
>
> **Deferred (followup):**
> - vec_map_int_str — T=int U=str cross-type case (M-fn-prefix-int-only-mono).
> - LSP quick-fixes (Plan 101.5 V2).
>
> **Ред. 3 (2026-05-24):** complete rewrite после critical review.
> Ред. 1 описывала narrow `fn[T]` only. Ред. 2 ошибочно ввела
> implicit-T (моя misinterpretation D35). Ред. 3 — finalized design:
> **никакого implicit T**, `fn[T]` префикс обязателен везде где
> receiver не имеет carrier-brackets, + bounds через existing D72,
> + multi-bound `+`, + protocol composition `use Foo`.
>
> **Ред. 5 (2026-05-25):** Plan 101.3 (multi-bound `[T A + B]`)
> и Plan 101.4 (protocol composition `use TypeName` — pivot от
> earlier discussion A1 `use A, B` к более читаемому line-per-use)
> финализированы и реализованы.
>
> **Ред. 3 (2026-05-24):** complete rewrite после critical review.
> Ред. 1 описывала narrow `fn[T]` only. Ред. 2 ошибочно ввела
> implicit-T (моя misinterpretation D35). Ред. 3 — finalized design:
> **никакого implicit T**, `fn[T]` префикс обязателен везде где
> receiver не имеет carrier-brackets, + bounds через existing D72,
> + multi-bound `+`, + protocol composition `use Foo`.

### Что

Generic-параметры функции в receiver-position декларируются по
**одному из двух механизмов**, в зависимости от формы receiver'а:

1. **Carrier-brackets на named generic-типе** — existing
   [D119](#d119-method-level-type-parameters-в-generic-methods):
   - `fn Option[T] @map[U]` — T в `Option[T]` декларирует T.
   - `fn HashMap[K, V] @keys()` — K, V в `HashMap[K, V]`.
   - `fn Result[T, E] @ok()` — T, E.
   - **С bound (D72):** `fn HashMap[K Hashable, V] @from_pairs(...)`.
2. **`fn[T]` префикс** (новое, D145) — для receiver'ов **без carrier
   brackets**: bare T, `[]T`, tuple `(T, U)`, composite без carrier:
   - `fn[T] T @identity() -> T => @` — bare typevar.
   - `fn[T] []T @map[U](f fn(T) -> U) -> []U => ...` — array.
   - `fn[T, U] (T, U) @swap() -> (U, T) => (@.1, @.0)` — tuple.
   - `fn[T Hashable] []T @dedup() -> []T => ...` — bounds через D72.
   - `fn[T A + B] []T @method() => ...` — multi-bound через `+` (Plan 101.3).

### Правило

#### Когда `fn[T]` обязателен

`fn[T1, ..., Tn]` префикс **обязателен** для каждого typevar в
receiver-position, который **не декларируется через carrier-brackets**
именованного generic-типа. Конкретно:

| Receiver-shape | Carrier? | `fn[T]` нужен? |
|---|---|---|
| `Option[T]`, `HashMap[K, V]` | да named-brackets | нет |
| `[]T` | нет — `[]` not bracket-decl | да `fn[T] []T` |
| `T` bare | нет | да `fn[T] T` |
| `(T, U)` tuple | нет — tuple-parens not bracket-decl | да `fn[T, U] (T, U)` |
| `(T, Option[U])` mix | T нет, U через Option | да `fn[T] (T, Option[U])` |
| `[]Option[T]` composite | T через Option[T] | нет |

#### Запрет дублирования

`fn[T]` **запрещён** для typevar, который ТАКЖЕ декларируется через
carrier-brackets:

```nova
fn[K Hashable, V] HashMap[K, V] @method   // ERROR E_DUPLICATE_GENERIC_DECL
// K, V уже декларированы через HashMap[K, V]; используй
// fn HashMap[K Hashable, V] @method
```

#### Disambiguation: bare T vs named type

| `fn`-prefix | Receiver | `type T` в scope? | Result |
|---|---|---|---|
| — | `T` | да | OK — метод на named T (D35 status quo) |
| — | `T` | нет | error `E_BARE_TYPEVAR_NEEDS_PREFIX` |
| `[T]` | `T` | нет | OK — generic, T = typevar |
| `[T]` | `T` | да | error `E_PREFIX_SHADOWS_NAMED_TYPE` |
| — | `[]T` | да или нет | parse OK — но если есть named T, T = named (silent miscompile risk; см. ниже) |
| `[T]` | `[]T` | да или нет | OK — explicit prefix wins, T = fn-generic |

**Critical:** `fn []T @method` без `fn[T]` префикса и без `type T в scope` —
**type-check error**: «`T` не объявлен ни через carrier-brackets, ни через
`fn[T]` префикс, ни как named type». Закрывает silent-miscompile gap
(vec.nv pre-Plan-101 поведение).

#### Bound syntax (через D72)

```nova
fn[T Hashable] []T @dedup() -> []T => ...
fn[T A + B] []T @method() => ...                    // multi-bound (Plan 101.3)
fn[K Hashable, V] (K, V) @key_value() -> (K, V) => @
fn[T From[K], K] T @construct_from(v K) -> T => T.from(v)   // parametric protocol
```

**Bound = только protocol-тип** (D72). Concrete-type bounds (`fn[T int]`,
`fn[T User]`) — **отдельный open question**
[Q-representation-bound](../open-questions.md#q-representation-bound),
Plan 102 (future).

#### Protocol composition (Plan 101.4 — закрывает D53 open question)

Protocols composed через `use A, B` keyword **внутри protocol body**.
Параллель D39 record-embed (same keyword, разная семантика). Composition
валиден в **type-decl** и **anonymous type-position**.
**Literal-position — composition ОТВЕРГНУТА** (см. ниже).

```nova
type Reader protocol { read(buf []u8) -> int }
type Writer protocol { write(buf []u8) -> int }

// 1. Multi-composition в type-decl:
type ReadWriter protocol {
    use Reader, Writer       // embed
    close() -> ()            // own method
}

// 2. Single-composition (естественно, без ambiguity):
type ReadExt protocol {
    use Reader
    job() -> ()
}

// 3. Pure composition без own methods:
type Streamable protocol {
    use Reader, Writer, Closeable
}

// 4. Mix anywhere в block — order independent:
type Complex protocol {
    init() -> ()
    use Reader
    helper() -> int
    use Writer
}

// 5. Anonymous-composition в type-position (extension D53):
fn process(rw protocol { use Reader, Writer }) { ... }

// 6. Использование как bound — composed protocol работает как named:
fn[T ReadWriter] []T @process() => ...
// эквивалентно fn[T Reader + Writer] []T @process() (101.3 multi-bound)
```

**Семантика:**
- `use A, B, C` — flatten method-signatures из A, B, C в этот protocol.
- Resulting method-set = union(A, B, C, own_methods).
- Multiple `use`-statements аккумулируются: `use A, B; use C` ≡ `use A, B, C`.
- T satisfies composed-protocol ⟺ T has все methods из union.

**Реализация ред. 5 (2026-05-25, Plan 101.4):**
- Парсер поддерживает обе формы: `use A, B` (comma-list, как в spec)
  и `use A\n  use B` (line-per-use, более читаемо в большом protocol'е).
- Все `use`-items должны идти В НАЧАЛЕ protocol body — interleaving
  с методами запрещён (E_PROTOCOL_EMBED_AFTER_METHOD). Это упрощает
  чтение: сначала видишь "состав", потом "новое".
- Type-check ловит:
  * E_PROTOCOL_EMBED_UNKNOWN — embed target не объявлен.
  * E_PROTOCOL_EMBED_NOT_PROTOCOL — target существует, но не protocol.
  * E_PROTOCOL_EMBED_CYCLE — `A use B` ↔ `B use A` (или self-embed).
  * E_PROTOCOL_EMBED_DUPLICATE — после flatten'а ≥2 method из разных
    embed-источников с тем же (name, arity). Override-механизм отложен.
  * E_PROTOCOL_EMBED_NOT_NAMED — `use <complex type>` запрещено.

**Literal-composition — отвергнута:**

```nova
// ❌ ОТВЕРГНУТО:
ro v = protocol Foo {
    use Reader               // error: E_LITERAL_COMPOSITION_NOT_ALLOWED
    read(buf) => impl1
    close() => impl2
}

// Workflow: extract в named type:
type MyRW protocol { use Reader, Writer }
ro v = protocol MyRW {
    read(buf)  => impl1
    write(buf) => impl2
}
```

**Почему literal-composition отвергнута:** literal — value-construction
(impls), composition — type-level operation. Смешивать слои когнитивно
нагружено. Industry-aligned — Rust/Go/Java/Kotlin/Scala не разрешают
anonymous-composition в literals.

**Asymmetry с multi-bound (101.3) `[T A + B]` оправдана:** разные
contexts — multi-bound = use-site intersection при satisfaction-check;
protocol composition = decl-time method-set union. Разные scopes,
разные операторы.

**Differences vs D39 (record-embed):**
- D39 record `use name Type` (field-form, runtime delegation+field).
- D53+ protocol `use Type[, Type]*` (нет field, compile-time method-set union).
- Same keyword `use` — same intuition «include this stuff». Parser
  распознаёт по контексту (record-body vs protocol-body).

### Многократное использование одного имени

Одно имя — один generic во всей сигнатуре (existing D119 / D72 convention):

```nova
fn[T] (T, T) @duplicate(a T) -> (T, T) => (a, a)   // T дважды → один T
fn[T] [][]T @flatten() -> []T => ...                // T в receiver и return — один T
```

### Backward-compat

- **100% преserve** для existing `fn Option[T] @map[U]`, `fn HashMap[K, V] @keys`,
  `fn Result[T, E] @ok`, `fn HashMap[K Hashable, V] @method` — D145
  строго аддитивно.
- **`std/collections/vec.nv`** содержит 7 методов pattern `fn []T @method[U]`
  (написан как-если-бы T дженерик). Это **bug** — T silently трактуется
  как named type, codegen падает. **Plan 101.1 включает migration**
  vec.nv → `fn[T] []T @method[U]`.

### Параллель индустрии — таблица

| Lang | Synтакс для array-method | Bound syntax |
|---|---|---|
| Rust | `impl<T> Vec<T> { fn map<U> }` | `<T: A + B>` |
| Go | `func (v Vec[T]) Map[U]` | `[T A \| B]` (union, не intersection!) |
| TypeScript | `function map<T, U>(arr: T[], f)` | `T extends A & B` |
| Kotlin | `fun <T, U> Array<T>.map(f)` | `<T : A>` + `where T : B` |
| Scala 3 | `extension [T](arr: Array[T]) def map[U]` | `T <: A & B` |
| Java | `<T, U> U[] map(T[] arr, ...)` | `<T extends A & B>` |
| **Nova D145** | `fn[T] []T @map[U]` | `[T A + B]` (Rust-style `+`) |

**Nova edge:**

1. **Cleanest receiver syntax** — `fn[T] []T @map` короче Rust
   `impl<T> Vec<T> { fn map<U> }` (2 nested blocks → 1 line).
2. **Bound syntax без двоеточия** — `[T Hashable]` (D72) — параллель
   Nova `name type` convention (params, fields, let).
3. **Multi-bound `+` familiar** — Rust audience узнаёт.
4. **Protocol composition через `use`** — параллель D39 record-embed,
   единое правило.
5. **Loud disambiguation** — `E_BARE_TYPEVAR_NEEDS_PREFIX` /
   `E_PREFIX_SHADOWS_NAMED_TYPE` явные, не silent miscompile.
6. **Future-proof** — `Q-representation-bound` открыт для extension на
   concrete-type bounds (Plan 102).

### Lineage

- Plan 48 / D119 — method-level + receiver-via-carrier generics.
- Plan 72 / D72 — bound syntax `[T Bound]` (free fn + type-decl). D145
  переиспользует в новой позиции (`fn[T Bound]` prefix).
- Plan 88 — static-method-on-typevar.
- Plan 99 — Option/Result closure-applying на Nova-body (paritет).
- D39 — `use Type` embed для records. D145 переиспользует pattern для
  protocol composition (Plan 101.4).
- D53 — `type X protocol { ... }`. D145 закрывает open question
  «Composition protocol'ов» через 101.4.

### См. также

- [D72](#d72-generic-bounds-через-t-protocol--protocol-как-тип) — bound syntax.
- [D119](#d119-method-level-type-parameters-в-generic-methods).
- [D39](#d39-embed-и-delegation-use-name-type-alias-обязателен) — `use` для embed.
- [D53](#d53-унификация-protocol-под-type-protocol-как-kind-токен) — protocol decl.
- [Plan 101 master](../../docs/plans/101-receiver-generic-prefix.md)
  + 5 sub-plan'ов:
  - [101.1](../../docs/plans/101.1-fn-prefix-core.md) — core `fn[T]`
    grammar + codegen + vec.nv migration (P1, blocker Plan 91).
  - [101.2](../../docs/plans/101.2-bound-integration.md) — bound
    integration `fn[T Hashable]`.
  - [101.3](../../docs/plans/101.3-multi-bound.md) — multi-bound
    `[T A + B]`, closes Q-multi-bound.
  - [101.4](../../docs/plans/101.4-protocol-composition.md) — protocol
    embedding `use Foo`, closes D53 open question.
  - [101.5](../../docs/plans/101.5-stdlib-audit-close.md) — stdlib
    audit + LSP + close.
- [Q-representation-bound](../open-questions.md#q-representation-bound)
  — concrete-type bounds (newtype/embed-aware), Plan 102 future.

---

## D180. Canonical `.new()` constructors (convention)

**Статус:** convention (stdlib provides, compiler does NOT auto-generate).

stdlib предоставляет `.new()` для типов с **единственным очевидным**
default-значением:

| Тип | `.new()` возвращает | Файл декларации |
|---|---|---|
| `int`, `u8`–`u64`, `i8`–`i64` | `0` | `std/runtime/defaults.nv` |
| `f32`, `f64` | `0.0` | `std/runtime/defaults.nv` |
| `bool` | `false` | `std/runtime/defaults.nv` |
| `str` | `""` | `std/runtime/string.nv` |
| `[]T` (для любого T) | `[]` (empty array) | builtin (emit_c.rs) |

Также `[]T.with_capacity(n int) -> Self` — empty с pre-allocated capacity
(builtin).

**Для своих типов** разработчик пишет `.new()` явно. Компилятор НЕ
автогенерирует для user records / sum types / consume types.
Это design discipline:

1. Явный конструктор виден в `nova doc` и IDE.
2. Имена кодируют намерение (`User.new(name, email)` vs `User.guest()`).
3. Валидация инвариантов в момент создания.
4. Эволюция типа: добавление поля заставляет обновить конструктор —
   good failure (компилятор поймает breaking change).

**НЕ имеют canonical `.new()`** (convention — не использовать;
enforcement diagnostic — followup `[M-91.7-default-new-enforcement]`):

- `char` (`'\0'` сомнителен как «default»)
- `Result[T, E]` (`Ok` или `Err`? ambiguous)
- `Option[T]` — каноничен, но codegen ограничение для generic builtin
  sum static methods откладывает Nova-side декларацию (followup
  `[M-91.7-option-new-static]`). До закрытия — использовать `None` напрямую.
- tuples (`(int, str)` etc.)
- user-defined records / sum / consume types — по конвенции этого блока
- protocols, fn types, external/opaque

### Пример

```nova
// stdlib provides:
ro x = int.new()      // 0
ro s = str.new()      // ""
ro a = []int.new()    // []
ro buf = []u8.with_capacity(1024)

// User type — explicit:
type User { name str, email str, is_admin bool }
fn User.new(name str, email str) -> Self => { name, email, is_admin: false }
fn User.guest() -> Self => { name: "guest", email: "", is_admin: false }
```

### Связь

- [D26](#d26-базовая-stdlib-и-prelude) — prelude auto-availability.
- [D66](#d66-self-universal--ссылка-на-обобщающий-тип-в-методах-effects-protocols) — `Self` в return type.
- [D131](03-syntax.md#d131-consume-types-и-fluent-api) — consume / fluent.
- [D182](#d182-self-в-return-type-static-methods--required-form-для-parametric-types) — `Self` requirement.
- [Plan 91.7](../../docs/plans/91.7-array-methods-and-default-new.md).

---

## D181. Array methods — `-> @` fluent mut chain + slice syntax

**Статус:** active (Plan 91.7, 2026-05-28).

### `-> @` для всех mut-методов `[]T`

Все мутирующие методы массива возвращают `@` (receiver pointer)
для fluent chain (D131):

| Метод | Сигнатура |
|---|---|
| `@push(v T)` | `-> @` |
| `@reserve(extra int)` | `-> @` |
| `@truncate(n int)` | `-> @` |
| `@fill(v T)` | `-> @` |
| `@copy_from(src readonly []T)` | `-> @` |
| `@extend_from(src readonly []T)` | `-> @` |
| `@insert_from(i int, src readonly []T)` | `-> @` |
| `@copy_within(src_from, dst_from, len)` | `-> @` |
| `@sort()` (Nova-side) | `-> @` |
| `@sort_by(cmp)` | `-> @` |

Non-mut методы (`@get(i)`, `@pop()`) возвращают `Option[T]` —
unchanged.

### Пример

```nova
mut a = []int.new()
a.push(1).push(2).push(3).reserve(10)
a.sort()                       // direct call
ro r = a.sort_by(|x,y| ...)   // can also return into binding
```

### Slice — только bracket syntax (Plan 96)

Метод `@slice(from, to) -> []T` удалён. Используйте `arr[a..b]`
(zero-copy view, см. Plan 96 / D-str-slice). Один очевидный путь.

### Известные ограничения

- **Mixed Nova-method + builtin chain:** `a.sort().push(99)` — codegen
  пока эмитит `a->sort()` (struct field access) вместо function call.
  Followup `[M-91.7-mixed-method-chain]`. Workaround: разнесите вызовы.
- **Generic sort/min/max для `[T Ord]`** — followup `[M-91.7-sort-generic]`.
  Текущий MVP — concrete `[]int @sort()` (Plan 91.3).

### Связь

- [D131](03-syntax.md#d131-consume-types-и-fluent-api) — fluent API
  семантика `-> @`.
- [D177](08-runtime.md#d177-str-nova-body-dispatch--plan-54-ф2-extension)
  — Nova-body dispatch механизм.
- [Plan 90.1](../../docs/plans/90.1-array-extend-family.md) — extend-family
  (extend_from, insert_from, reserve).
- [Plan 96](../../docs/plans/96-array-slices.md) — `arr[a..b]` slice
  syntax.

---

## D182. `Self` в return-type static methods — required form для parametric types

**Статус:** active (Plan 91.7, 2026-05-28).

### Правило

Для **static-методов на параметризованных типах** (`fn Option[T].new()`,
`fn HashMap[K, V].new()`, etc.) return-type должен использовать `Self`,
а не explicit-form `-> Option[T]` / `-> HashMap[K, V]`.

**Rationale:**
1. Explicit-form дублирует тип-параметры — redundant.
2. `Self` устойчив к переименованию типа (rename-safe).
3. `Self` явно говорит «возврат того же receiver-типа» — semantic clarity.
4. Single canonical form — D9 «один очевидный путь».

### Примеры

```nova
// ✅ Correct (canonical):
export fn Option[T].new() -> Self => None
export fn HashMap[K, V].new() -> Self => { ... }
export fn StringBuilder.new() -> Self => { ... }

// ❌ Wrong (explicit redundant form):
export fn Option[T].new() -> Option[T] => None
export fn HashMap[K, V].new() -> HashMap[K, V] => { ... }
```

### Для primitive receiver types

`Self` тоже **рекомендуется** для consistency:

```nova
export fn int.new() -> Self => 0          // канонично
export fn int.new() -> int => 0           // допустимо, но не canonical
```

### Codegen requirement

`Self` в return-type корректно resolved через `current_receiver_type` ⇒
правильный C type:
- primitive receiver → primitive value type (`nova_int`, `nova_bool`, ...)
- Option/Result → sum repr (`NovaOpt_<T>`, `NovaRes_<ok>_<err>*`)
- user record → `Nova_<TypeName>*`

См. `emit_c.rs::type_ref_to_c "Self"` case — делегирует в `receiver_c_type`.

### Enforcement

Validation rule — followup `[M-91.7-self-required-parametric]`. Текущий
compiler принимает обе формы; canonical форма документирована здесь.

### Связь

- [D66](#d66-self-universal--ссылка-на-обобщающий-тип-в-методах-effects-protocols)
  — `Self` универсальный.
- [D180](#d180-canonical-new-constructors-convention) — `.new()` convention.
- [Plan 91.7](../../docs/plans/91.7-array-methods-and-default-new.md).

---

## D183. Canonical comparison protocols + default method bodies (Plan 91.8a)

**Статус:** active (Plan 91.8a, 2026-05-29).

### Канонические протоколы (renames)

| Было | Стало | Файл |
|---|---|---|
| `Iter[T]` | `Iterable[T]` | `std/prelude/collections.nv` |
| `Display` | `Printable` | `std/prelude/protocols.nv` |
| `Equatable.eq(other Self) -> bool` | `Equatable.equals(other Self) -> bool` | `std/prelude/protocols.nv` |
| `Comparable.cmp(other Self) -> Ordering` | `Comparable.compare(other Self) -> int` | `std/prelude/protocols.nv` |
| `Hashable.hash() -> u64` | unchanged | `std/prelude/protocols.nv` |

**Rationale renames:**
- **`-able` suffix convention** — unified naming (Iterable/Equatable/Comparable/Hashable/Printable).
- **`Comparable.compare -> int`** — единый стиль с `str.compare()` (D178) и C `memcmp`/`strcmp`. `Ordering` sum-type удалён.
- **`Equatable.equals`** — явнее чем `eq` (Java convention).
- **`Display` → `Printable`** — действие через `-able`, не имя-noun.

### Comparable embeds Equatable

```nova
export type Equatable protocol {
    equals(other Self) -> bool
}

export type Comparable protocol {
    use Equatable
    compare(other Self) -> int
    equals(other Self) -> bool => @compare(other) == 0    // default body
}
```

`use Equatable` (D39 embed) делает каждый Comparable также Equatable.
Локальная декларация `equals` в Comparable с default body **overrides**
embedded default — implementer пишет только `@compare`, `@equals`
auto-synthesized из default body как `@compare(other) == 0`.

### Default method bodies в protocols

**Правило (новое в D183):**

> Метод в protocol-декларации **может иметь тело** (`=> expr` или `{ ... }`).
> Тело используется как **default-реализация**: если тип-implementer не задаёт
> свой `@method`, компилятор использует body из протокола, подставляя `Self`
> = receiver type. Если implementer задал `@method` явно — explicit version
> используется (override).

**Семантика:**

- **Метод без тела** = abstract — implementer ОБЯЗАН реализовать.
- **Метод с телом** = default — implementer МОЖЕТ override.

**Пример:**

```nova
type Comparable protocol {
    use Equatable
    compare(other Self) -> int                              // abstract
    equals(other Self) -> bool => @compare(other) == 0      // default
}

type MyDate { y int, m int, d int }
fn MyDate @compare(other MyDate) -> int { ... }
// @equals НЕ объявлен — используется default из Comparable.

// Override для perf:
type FastHashed { hash_cache u64, ... }
fn FastHashed @compare(other FastHashed) -> int { ... }
fn FastHashed @equals(other FastHashed) -> bool {
    @hash_cache == other.hash_cache && @compare(other) == 0
}
```

### Cleanup

- `Ordering` sum-type удалён из `std/prelude/core.nv`.
- `Less` / `Equal` / `Greater` exports удалены из `std/prelude.nv`.
- `std/sort.nv` `sort_by(cmp fn(int, int) -> int)` — memcmp-style convention.
- `PRELUDE_VERSION` bumped 12 → 13.

### Memcmp-compatible int return

`compare(other) -> int` returns:
- **negative** if `@ < other`
- **zero** if `@ == other`
- **positive** if `@ > other`

Caller должен использовать только sign (`< 0`, `== 0`, `> 0`), НЕ magnitude.
Совместимо с C `memcmp`/`strcmp` convention. Implementer для primitive numerics
рекомендуется использовать safe signum form:

```nova
fn int @compare(other int) -> int =>
    if @ < other { -1 } else if @ > other { 1 } else { 0 }
```

Не использовать `=> @ - other` — overflow risk для больших int.

### Реализация (части)

- **Парсер** (`compiler-codegen/src/parser/mod.rs::parse_effect_methods`): добавлен parser default body после return_type/contracts. Body = `=> expr` или `{ ... }`. Поле `EffectMethod.default_body: Option<Block>` в AST.
- **`check_protocol_embeds`** (`compiler-codegen/src/types/mod.rs`): local override embedded methods разрешён — locally declared метод в protocol с тем же именем что embedded не считается duplicate. Используется для `Comparable.equals` overrides embedded `Equatable.equals` default.
- **Codegen synthesis для defaults**: followup `[M-91.8a.2-default-codegen]`. Сейчас implementer пишет default-method explicitly для compatibility (как boilerplate `equals(o) => @compare(o) == 0`).

### Известные ограничения / followups

- **Codegen synthesis (`[M-91.8a.2-default-codegen]`):** type T который имеет `@compare` но не `@equals` пока компилируется только если `@equals` объявлен явно. Eager synthesis из default body — отдельный codegen pass.
- **Operator dispatch (D184, Plan 91.8b):** `==` всё ещё dispatches к `@eq` (D46). Renaming `@eq` → `@equals` в operator dispatch — задача Plan 91.8b. До 91.8b implementer пишет оба: `@equals` (protocol) + `@eq` (operator).
- **Generic sort/min/max (D185, Plan 91.8c):** generic `fn[T Comparable]` array methods — отдельный subplan.

### Связь

- [D26](#d26-базовая-stdlib-и-prelude) — prelude auto-availability.
- [D39](#d39-embed-и-delegation-use-name-type-alias-обязателен) — `use` embed.
- [D58](#d58-protocol-structural-typing) — structural typing.
- [D72](#d72-generic-bounds-через-t-protocol--protocol-как-тип) — bounds.
- [D109](#d109-equatable--hashable-split-policy) — split policy (Hashable не embeds Equatable; Comparable embeds Equatable в D183).
- [D178](08-runtime.md#d178-str-api-cleanup-и-расширения--plan-91-ф26) — `str.compare -> int`.
- [Plan 91.8a](../../docs/plans/91.8a-protocol-canon-renames.md) — implementation.

---

## D183 amendment — Plan 91.8a.2 part 1: protocols refactor (orthogonal) + Self в param

**Статус:** active (Plan 91.8a.2 part 1, 2026-05-29).

### Refactor: orthogonal protocols (canonical coercion form)

**Было (91.8a part 1):**
```nova
type Equatable protocol {
    equals(other Self) -> bool
}
type Comparable protocol {
    use Equatable
    compare(other Self) -> int
    equals(other Self) -> bool => @compare(other) == 0   // override of embedded default
}
```

**Стало (91.8a.2 part 1) — canonical:**
```nova
type Equatable protocol {
    equals(other Self) -> bool {
        ro cmp Comparable = @                  // coercion-style (explicit dependency)
        cmp.compare(other) == 0
    }
}
type Comparable protocol {
    compare(other Self) -> int
}
```

**Rationale:**
- **Orthogonal protocols** — каждый stand-alone, без embed-зависимости.
- **Coercion canonical (Q6 decision):** explicit cross-protocol dependency
  visible при чтении декларации; codegen devirtualizes к direct call когда
  тип known statically (zero runtime cost).
- **Conditional default:** T satisfies Equatable если has @equals explicit
  ИЛИ satisfies Comparable (default body synth via @compare). Type только
  Equatable (Vector3, Complex, etc.) пишет @equals явно — coercion fails
  potential потому что @compare отсутствует.
- **Direct form `=> @compare(other) == 0` тоже валидна** — terser; same C
  output after devirtualization. Coercion form preferred в stdlib для
  documentation.

### Printable.fmt default body

```nova
type Printable protocol {
    fmt(sb StringBuilder) {
        sb.append(str.from(@))
    }
}
```

- Primitives — works via primitive `Nova_int_to_str` etc.
- User types — implementer пишет @fmt явно (perf) OR provides
  `fn str.from(MyType) -> str` overload.

### From identity blanket (D183 amendment)

```nova
export fn[T] T.from(t T) -> T => t
```

- Аналог Rust `impl<T> From<T> for T`.
- **Override запрещён (Q4 strict decision):** попытка `fn Money.from(m Money) -> Money`
  даёт `E_BLANKET_IDENTITY_OVERRIDE`. Identity is identity (D9 single canonical path).
- **Resolution order для `T.from(value)`:**
  1. Explicit `fn T.from(value_type)` → win
  2. Blanket identity — match только если `value_type == T`
  3. D77 auto-derive из From[value_type] chain
  4. Error E_NO_FROM_IMPL
- Identity Into auto-derived через D77.
- Coexistence: blanket additive с existing `From[T]` protocol decl
  (`std/prelude/protocols.nv:81-83`) + `emit_c.rs::from_targets`/`into_targets`
  registries (D77 4-way derive).

### `Self` в param-type position (М-91.8a-self-in-param closed)

Раньше `fn T @method(other Self) -> R` давал E7001 «Self type used outside
receiver context». Fix: `emit_c.rs::emit_module` method overload registration
устанавливает `current_receiver_type` перед param_c_types calculation
(mirror return-type path). Закрыто Plan 91.8a.2 part 1.

### Codegen lazy synthesis + devirtualization — followup (Plan 91.8a.2 part 2)

**Часть 1 (текущая) ограничена** структурным refactor + Self fix. **Часть 2**
(отдельный sub-session) реализует:

1. **Lazy synthesis at use-site:**
   - Bound contexts (`[T Equatable]` etc.) — synth default body для типов
     которые satisfy abstract methods
   - Protocol coercion (`let x Equatable = m`)
   - Operator dispatch (Plan 91.8b)
   - String interpolation (Plan 91.10)
   - NOT triggered: bare method call (`m.equals(other)` — direct lookup only)
2. **Devirtualization pass** — coercion form `let cmp Protocol = @` становится
   type ascription + direct call при synthesis для concrete T. Result: same
   C output что direct form.
3. **Cache** per compilation unit: `HashMap<(TypeId, MethodName), SynthFnDecl>`.
4. **From blanket mono** — extension Plan 101 mono pass на `fn[T] T.method`
   static на generic T.
5. **Error diagnostics:** E_SYNTH_CYCLE, E_SYNTH_AMBIGUOUS, W_DEVIRT_FAILED,
   E_BLANKET_IDENTITY_OVERRIDE.

До части 2 — implementer пишет default body methods явно (boilerplate
compatibility). Это работает но дублирует код.

### Связь

- [D183 (part 1)](#d183-canonical-comparison-protocols--default-method-bodies-plan-918a) — base D183.
- [D26](#d26-базовая-stdlib-и-prelude) — prelude.
- [D58](#d58-protocol-structural-typing) — structural typing.
- [D77](08-runtime.md#d77-fromtryfrom-auto-derive) — From/Into 4-way auto-derive.
- [Plan 91.8a.2](../../docs/plans/91.8a.2-default-body-codegen-and-from-blanket.md).

---

## D186 — `#impl(P1 + P2 + ...)` opt-in annotation для protocols

**Когда:** 2026-05-29 (Plan 91.9).
**Plan:** [91.9-impl-annotation.md](../../docs/plans/91.9-impl-annotation.md).
**Зависит от:** [D58](#d58-protocols-structural-typing) (structural protocols),
[D72](#d72-bounds) (generic bounds), [D183](#d183-canonical-protocols)
(canonical protocols Equatable/Comparable/Printable + default body).

### Проблема

Nova protocols — structural ([D58](#d58)). Compiler разрешает `obj.method()`
если у типа есть соответствующий метод, без явного opt-in. С добавлением
default body synthesis (D183) ситуация ухудшилась:

```nova
type Greetable protocol {
    greet() -> str { "Hello, " + @name() }
}
type User { display_name str }
fn User @name() -> str => @display_name

u.greet()  // ??? — без D186 это работало structurally (TypeScript-style)
```

Проблемы:
1. **Невидимая мутация behavior:** добавление протокола в одном модуле
   тихо добавляет методы всем типам подходящей сигнатуры.
2. **Reader-hostile:** глядя на `type User`, нельзя понять что у него
   есть метод `greet` (он синтезирован).
3. **Ambiguity:** два протокола с methods одинакового имени и default
   bodies — порядок resolution не детерминирован.
4. **Verification:** type-author не получает feedback что type соответствует
   intended protocol.

### Решение

`#impl(P1 + P2 + ...)` annotation **перед** type declaration. Меняет
**два** аспекта:

#### 1. Gate semantics (bare-call / interpolation требуют opt-in)

Контексты, где synthesis fires:

| Context | Требует `#impl(P)`? | Почему |
|---|---|---|
| Bare call `u.method()` | ✅ да | Ambient — type-author opt-in нужен |
| Interpolation `"${u}"` | ✅ да | Ambient — Printable.fmt synthesis |
| Generic bound `[T P]` | ❌ нет | Caller opted in через bound |
| Coercion `let x P = u` | ❌ нет | Caller opted in через annotation |
| Cast `(u as P).method()` | ❌ нет | Caller opted in через cast |
| Param `func(...args []P)` | ❌ нет | Caller opted in (signature) |

**Принцип симметрии:** хотя бы один из (type-author, use-site) должен
opt'нуться явно. Структура `#impl` — type-author side; bound/coercion/cast/
param — use-site side.

#### 2. Verification (auto-check соответствия)

При декларации `#impl(P)` compiler проверяет:

1. **E_UNKNOWN_PROTOCOL** — `P` не найдено как type name.
2. **E_IMPL_NOT_PROTOCOL** — `P` найдено, но не protocol kind.
3. **E_IMPL_MISSING_METHODS** — T не provides метод P:
   - не имеет explicit `fn T @method(...)`,
   - и default body P.method не synthesizable для T (зависит от другого
     метода которого T не имеет).

Verification работает **at type-declaration site** — error появляется
сразу, не при первом использовании.

### Синтаксис

```nova
#impl(Equatable + Comparable + Printable)
type Coin { value int }

fn Coin @compare(other Self) -> int => ...
fn str.from(c Coin) -> str => ...
// equals auto-derived через Equatable.equals default (uses @compare)
// fmt auto-derived через Printable.fmt default (uses str.from)
```

`+` separator consistent с multi-bound `[T A + B + C]` ([D72](#d72), Plan 101.3).

Order arbitrary: `#impl(A + B)` ≡ `#impl(B + A)`.

Multiple `#impl` annotations не разрешены — single annotation with `+`.

#### Position

`#impl(...)` ставится **перед** `type T` (рядом с `#stable`, `#from_fields`):

```nova
#stable(since = "0.1")
#impl(Hashable + Equatable)
type UserId { value u64 }
```

### Семантика

**Use-site остаётся structural** (D58 preserved). `#impl` не делает тип
nominal. Он добавляет:
- **Gate** на ambient synthesis (bare call / interpolation).
- **Verification** в точке декларации.

Через bound / coercion / cast / param-coercion использование любого
structurally-подходящего типа всё ещё работает — `#impl` не требуется.

### Что НЕ делает

- НЕ создаёт nominal typing (use-site structural preserved).
- НЕ обязателен — opt-in, existing types работают через use-site coercion.
- НЕ меняет runtime — `#impl` только compile-time проверка/gate.

### Codegen

`emit_c.rs::try_synthesize_default_method_with_gate(t, c, m, gate_on_impl)`:
- `gate_on_impl = true` — bare call / interpolation; restricts candidates
  к protocols в `type_impl_protocols[t]`.
- `gate_on_impl = false` — vtable thunk (coercion), bound mono; structural.

`type_impl_protocols: HashMap<String, HashSet<String>>` populated в
forward-decl pass из `TypeDecl.impl_protocols`.

### Type-checker verification

`types/mod.rs::verify_impl_protocols` walks каждый `Item::Type` с
non-empty `impl_protocols`:

1. Each `P` lookup в `self.types`. None → E_UNKNOWN_PROTOCOL.
2. Kind check — must be `TypeDeclKind::Protocol`. Иначе → E_IMPL_NOT_PROTOCOL.
3. Each required method `m` в `P.methods`:
   - `t_provides_method(T, m.name)` → ok (explicit).
   - `m.default_body.is_some() && default_body_calls_satisfy_for(body, T)`
     → ok (synthesizable).
   - Else → list в missing, emit E_IMPL_MISSING_METHODS с hint.

`default_body_calls_satisfy_for` — AST walker проверяет body's referenced
calls resolve for T (через `t_provides_method` + `t_satisfies_str_from` для
auto-derive `str.from(@)` pattern).

### Compatibility

- Existing structural use-sites (bound `[T P]`, coercion `let x P = u`,
  cast `(u as P)`, parameter coercion) continue работать без `#impl`.
- Existing types **без** `#impl` могут потерять bare-call:
  `fn User @name() -> str => ...; u.greet()` (Greetable.greet default) —
  раньше работало, теперь error (без `#impl(Greetable)`).
- Migration trivial: добавить `#impl(Protocol)` перед type decl.

### Связь

- [D58](#d58-protocols-structural-typing) — structural protocols (use-site preserved).
- [D72](#d72-bounds) — generic bounds (use-site opt-in alternative).
- [D183](#d183-canonical-protocols) — canonical protocols + default body
  synthesis (что gate'ится).
- [D109 split policy](#d109-split-policy).
- Plan 101.3 — multi-bound `+` syntax.

---

## D200. Associated constants — `const` field в `type X`

> **Plan 114.4 Ф.2** (extracted from Plan 114 Ф.10 safety hatch).
> **Status:** 🆕 draft (финализируется в Ф.4).

### Что

`const` declaration внутри `type X { … }` body — **associated constant**
типа. Не часть instance layout; accessible через namespace
`Type.CONST_NAME`.

```nova
type Config {
    const VERSION int = 2                  // associated const
    const PROTOCOL str = "v2"
    const MAX_PEERS int = 1024
    name str                                // instance field
    timeout Duration                        // instance field
}

// Access — только namespace
Config.VERSION                              // ✓ 2
Config.MAX_PEERS                            // ✓ 1024

// Instance access — error
ro c = Config { name: "alice", timeout: SECOND }
c.VERSION                                   // ✗ E_CONST_INSTANCE_ACCESS

// Layout
sizeof(Config) == sizeof(name) + sizeof(timeout)  // const fields НЕ в layout
```

### Семантика

1. **Strict constexpr** — RHS должен быть literal-eligible.
2. **Zero storage в instance.** Codegen не emit'ит const-field в struct
   layout. Каждый const-field живёт как top-level C-symbol
   `Type_FieldName` в .rodata.
3. **Namespace access only.** `Type.NAME` resolution через type's
   const-table. `instance.NAME` → `E_CONST_INSTANCE_ACCESS`.
4. **Не указывается в record literal.** Указание → `E_CONST_FIELD_IN_LITERAL`.
5. **`export const` field** — publicly accessible cross-module.
6. **Modifier-conflicts:**
   - `mut const` / `const mut` → `E_CONST_MUT_CONFLICT`.
   - `ro const` / `const ro` → `E_CONST_RO_REDUNDANT`.
   - `consume const` → `E_CONST_CONSUME_CONFLICT`.
7. **SCREAMING_SNAKE_CASE convention** — lint warning (D30 carry-over).

### Sum-type associated constants

`const` decl внутри sum-type body — associated на sum-type-level:

```nova
type Status = Active | Inactive | Pending {
    const VERSION int = 2
    const MAX_TRANSITIONS int = 100
}

Status.VERSION                              // ✓ 2
```

Per-variant const'ы (`Active { const X = 1 }`) — out-of-scope V1, followup
`[M-115-per-variant-const]`.

### Generic-type associated constants

**T-independent** — RHS не reference'ит generic params:

```nova
type Box[T] {
    const TAG int = 0
    value T
}
Box.TAG                                     // ✓ emit single Box_TAG
```

**T-dependent** — RHS reference'ит generic param:

```nova
type Box[T] {
    const SIZE int = sizeof(T)
    value T
}
Box[int].SIZE                               // ✓ 8 — per-mono Box_int_SIZE
Box[str].SIZE                               // ✓ 16 — per-mono Box_str_SIZE
Box.SIZE                                    // ✗ E_GENERIC_CONST_REQUIRES_INSTANTIATION
```

**Allowed в T-dependent RHS (V1):**
- `sizeof(T)` где `T` — generic param.
- Арифметика над `sizeof(T_i)` и literals.
- Ссылки на T-independent `const` через `Type.CONST`.

**НЕ allowed в V1**:
- `T.METHOD()` calls — `[M-115-t-method-in-const]`.
- `const fn` calls с generic args — `[M-115-generic-const-fn]`.
- Recursive type refs (`Tree[T] { const X = sizeof(Tree[T]) }`) →
  `E_GENERIC_CONST_CYCLE`.

### Codegen

- **Non-generic + T-independent:** top-level `static const T Type_FieldName
  = …;` в .rodata. Resolution `Type.FieldName` → C-symbol `Type_FieldName`.
- **Generic T-dependent:** per-mono symbol naming coherent с existing
  generic-fn mono (Plan 70.5). Emit при каждой monomorphization.
- **`export const` field:** public C-symbol visibility.

### Сравнение с mainstream

| Язык | Синтаксис | Storage |
|---|---|---|
| Java | `static final int VERSION = 2;` (внутри class) | top-level C-static |
| Rust | `impl Config { const VERSION: i32 = 2; }` | top-level |
| Kotlin | `companion object { const val VERSION = 2 }` | companion slot |
| Swift | `struct Config { static let version = 2 }` | type-metadata |
| TS | `class Config { static readonly VERSION = 2 }` | class-static |
| **Nova** | `type Config { const VERSION int = 2; … }` | top-level .rodata |

### Use cases

- Version / protocol identifiers: `Config.VERSION`, `Protocol.MAGIC_BYTES`.
- Capacity / size limits: `Buffer.DEFAULT_CAPACITY`.
- Math constants: `Circle.PI`, `Complex.UNIT_IMAGINARY`.
- Per-mono sizes: `Box[int].SIZE`, `Pair[T,U].TOTAL`.

### Cross-ref

- [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindinga-readonly-для-never-mut) — field-decl extended.
- [D184](03-syntax.md#d184) — Plan 114 master keyword refresh.
- [D199](03-syntax.md#d199-const-fn--comptime-evaluable-functions) — `const fn` (могут использоваться для assoc const RHS).
- [D27](03-syntax.md#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `[N]T` arrays.

### Acceptance

См. Plan 114.4 A5-A13 (T2 series).

## D214. `ptr` opaque pointer type + tuple FFI returns + opaque handle pattern

> **Plan 115** (foundational FFI). **Status:** ✅ V1 closed 2026-06-01.
>
> ⚠️ **AMENDED by Plan 118 (D216)** — `ptr` redefined as
> `type ptr Option[*unsafe ()]` newtype над nullable unsafe void pointer
> (D216 §11). ABI preserved (single `void*`); semantics formalized as
> nullable Option (NPO emits NULL). `null ptr` literal **retracted**
> (`E_NULL_PTR_RETRACTED_USE_OPTION`); migrate к `None`.
>
> Closes followup [M-115-null-ptr-to-option-after-npo] от Plan 115. See
> [Plan 118](../../docs/plans/118-typed-pointers-and-unsafe.md) §«ptr
> redefine» и [D216 §11](#d216-typed-pointer-family--unsafe-model--null-safety-через-npo).
> Existing usages (handle pattern, tuple FFI returns) — **no migration
> required** (semantically equivalent post-amend).
>
> **Plan 91.12 V2 amend (2026-06-02):** generic tuple-newtype `type X[T](ptr)`
> now supported (was V1-limited to non-generic `type X(ptr)`). См. §«Generic
> opaque handle» ниже. Closes `[M-115-newtype-constructor-generic]`.

### Что

Foundational FFI infrastructure для bindings к произвольным C libraries
(`libsqlite3`, `libpng`, `libcurl`, etc.) без участия compiler-team.

Три компонента:

1. **`ptr` built-in primitive type** — opaque pointer-sized integer,
   ABI-эквивалентен `void*` в C.
2. **Tuple-by-value returns в `external fn`** — multi-value через
   struct-return calling convention.
3. **Opaque handle pattern** через `type X(ptr)` (D52 tuple newtype) —
   compile-time-distinct typed wrappers.

### 1. `ptr` built-in primitive type

```nova
ro p ptr = null ptr                          // NULL pointer literal
ro q ptr = some_external_fn()                // получили ptr из FFI
if p == null ptr { /* handle NULL */ }       // null check
ro as_int = q as u64                         // explicit cast → integer
ro back_to_ptr = (0x1000 as ptr)             // explicit cast int → ptr
```

#### Семантика

- **Size.** `usize` (8 bytes на 64-bit, 4 bytes на 32-bit). Bootstrap
  таргетит только 64-bit платформы (Linux x86_64, Windows x64, macOS
  ARM64/x86_64).
- **ABI.** `void*` в C. Передаётся в registers по платформенному ABI;
  identity при passing через external fn boundary.
- **Opaque.** Nova не имеет `*p` deref-операции (никогда не было), нет
  field/method access на `ptr`. Только comparison + cast + pass-through.
- **Default value.** `null ptr` (bitwise 0). Zero-init valid и обозначает
  «нет указателя».
- **Equality.** `==` / `!=` — bitwise pointer comparison (стандартная C
  semantics).
- **Casts.**
  - `ptr as u64` / `ptr as i64` — извлечь integer representation.
  - `u64 as ptr` / `i64 as ptr` — собрать ptr из integer (для opaque
    handle storage).
  - `ptr as ptr` — no-op (identity).
- **Arithmetic banned.** `ptr + N`, `ptr - ptr`, `ptr * 2` —
  `E_PTR_ARITHMETIC_BANNED`. Pointer arithmetic — unsafe operation,
  отложен на followup (`[M-115-ptr-arithmetic]`).
- **GC.** Conservative GC сканирует pointer-sized слоты как potential
  references — `ptr` слот с GC-allocated адресом будет pin'ить allocation
  (defensive, correct). `ptr` слот с non-GC адресом (e.g. sqlite3 handle)
  не tracs ничего (адрес вне GC arena). Зачем это работает: Boehm-style
  conservative collector реагирует только на адреса внутри tracked heap.
- **Memory ownership.** FFI domain — **user responsibility**. `ptr`,
  returned'ый из C library, должен быть освобождён matching C-side call
  (`sqlite3_close`, `png_destroy_read_struct`, etc.). Pattern:
  типизированный handle + `consume close()` метод на Nova-wrapper.

#### `null ptr` литерал

```nova
ro p = null ptr                              // valid expression
if p == null ptr { ... }                     // null check
```

Синтаксис: keyword `null` + type-name `ptr`. Two-token literal, parser
expects `null` followed by `ptr` ident. Распространение синтаксиса на
другие pointer types (Plan 118 `*T` family) — `null *T` — спроектировано
forward-compatible, но не реализуется в V1.

V1 ограничение: только `null ptr` valid. `null int`, `null str`, `null
SomeRecord` — `E_NULL_LITERAL_REQUIRES_PTR`.

> ⚠ **INTERIM construct (Plan 115 V1 only).** `null ptr` дублирует
> функциональность `None` из `Option[T]` (sum-type из D-блока Option/
> Result). Идиоматический Nova-путь — `Option[ptr]` с явной `None` /
> `Some(p)` диспозицией и compiler-enforced null check'ом.
>
> **Почему `null ptr` существует в V1.** `Option[ptr]` в bootstrap
> представлен как `NovaOpt_nova_ptr` struct (tag + value) — НЕ
> ABI-совместим с raw `void*` из C library. FFI shim пришлось бы
> оборачивать pointer'ы в Option struct'у — лишний overhead + struct
> return convention вместо register return. `null ptr` = bitwise 0 =
> идентично C `NULL` → zero-cost FFI.
>
> **Plan 118 NPO (Null Pointer Optimization).** После Plan 118 V2
> добавит `Option[*T]` с NPO codegen — `None` представляется как
> bitwise 0, `Some(p)` как `p`. Zero-cost + type-safe + ABI-compatible
> одновременно. См. `[[project-plan118-status]]` §«Option[*T] NPO
> codegen».
>
> **После Plan 118 landed: `null ptr` полностью удаляется** —
> retract из spec, parser emit'ит `E_NULL_LITERAL_REPLACED_BY_OPTION`
> с migration hint к `Option[ptr] / None`. См. marker
> `[M-115-null-ptr-to-option-after-npo]` в `docs/simplifications.md`
> для migration tracking.

#### Type-checker rules

| Операция | Результат | Diagnostic |
|---|---|---|
| `null ptr` | `Ty::Ptr` | — |
| `ptr == ptr` / `ptr != ptr` | `bool` | — |
| `ptr == null ptr` | `bool` | — |
| `ptr as u64` / `ptr as i64` | integer | — |
| `u64 as ptr` / `i64 as ptr` | `ptr` | — |
| `ptr as ptr` | `ptr` | no-op |
| `ptr + N` / `ptr - ptr` / etc. | error | `E_PTR_ARITHMETIC_BANNED` |
| `ptr.field` / `ptr.method()` | error | `E_PTR_NO_MEMBER` (нет деf членов на opaque) |
| `int as ptr` (для `int = i64`-style) | `ptr` | — (transparent через i64 path) |
| `ptr as int` | `int` | — |
| `ptr` в record-field | OK | — (storage в struct slot) |

`ptr` distinct от `i64`/`u64`/`int` на type-check уровне (нельзя смешать
без cast'а). Distinction enforced через отдельный `Ty::Ptr` variant.

### 2. Tuple-by-value returns в `external fn`

```nova
external fn nova_sqlite3_open(path str) -> (Sqlite3Handle, i64)
//                                          ↑              ↑
//                                          handle         error code
```

Соответствующий C shim:

```c
typedef struct {
    void*   _0;   // handle slot
    int64_t _1;   // error code slot
} Nova_Sqlite3OpenResult;

Nova_Sqlite3OpenResult nova_sqlite3_open(nova_str path) {
    sqlite3* db;
    int rc = sqlite3_open(path.data, &db);
    return (Nova_Sqlite3OpenResult){ db, (int64_t)rc };
}
```

#### ABI rules

- **Layout** Nova tuple type `(T1, T2, ..., Tn)` ↔ C struct `{ T1 _0; T2
  _1; ...; Tn _{n-1}; }`. **Element order preserved**, no padding inserted
  beyond what C compiler emits по target ABI.
- **Mangling.** Compiler emits `_NovaTuple_<arity>_<elem_mangles>` typedef
  (Plan 59 mechanism, существующий — переиспользуется). C-side shim
  должен иметь struct с тем же layout (struct typedef name произвольное —
  ABI layout совпадает).
- **Calling convention** — определяется C компилятором на target платформе:
  - **Sys V AMD64 (Linux, macOS x86_64):** structs ≤ 16 bytes (2 GPR) →
    return через `%rax:%rdx` registers. Bigger → caller passes hidden
    out-pointer в `%rdi`.
  - **AArch64 (macOS ARM64, Linux ARM64):** structs ≤ 16 bytes → `X0:X1`
    registers. Bigger → hidden out-pointer.
  - **Win x64 MSVC:** structs ≤ 8 bytes → `RAX`. Bigger → hidden
    out-pointer в `RCX`, shifting all other args.
- **Compiler responsibility.** Codegen эмитит struct return-type
  declaration; платформенный C compiler делает rest. Nova не пытается
  override calling convention — соответствие platform ABI делегировано
  toolchain.
- **Element type compatibility.** Tuple elements должны быть:
  - Primitives (`int`/`i32`/etc., `f64`, `bool`, `u8`-`u64`, `ptr`),
  - Newtype handles (`type X(ptr)`),
  - Pointer-like types (`str` — actually `{ data ptr; len u64 }`
    layout-equivalent struct),
  - Other tuples (nested struct return) — supported, transitive.
- **Прохибиции (V1).** Elements типа `[]T` (NovaArray pointer), Option,
  Result, sum-types — **не рекомендуется**, т.к. GC-tracked layouts. Pass
  them отдельно через out-params (если действительно нужно) или
  переупаковывайте в opaque handle. Followup `[M-115-tuple-gc-types]` —
  formal V2 support.

#### Layered FFI pattern

```
LAYER 1  Public Nova API (Database.open)
   ↓
LAYER 2  Nova wrapper (construct typed handle from raw)
   ↓
LAYER 3  external fn declaration (typed handle + tuple return)
            external fn nova_sqlite3_open(path str) -> (Sqlite3Handle, i64)
   ↓
LAYER 4  C shim (~5-10 lines per fn — adapts out-param convention → struct)
            Nova_Sqlite3OpenResult nova_sqlite3_open(nova_str path) { ... }
   ↓
LAYER 5  Actual C library (libsqlite3.so / sqlite3.dll)
            int sqlite3_open(const char* path, sqlite3** db_out);
```

Layer 4 (shim) — единственное место «где Nova ABI встречается с C
library ABI». User пишет один раз per fn. ~5-10 строк per shim.

### 3. Opaque handle pattern через `type X(ptr)` (D52 tuple newtype)

```nova
type Sqlite3Handle(ptr)                       // typed wrapper
type PngImageHandle(ptr)
type CurlEasyHandle(ptr)

// Construct
ro h = Sqlite3Handle(some_raw_ptr)

// Destructure inner ptr (used rarely; usually pass-through)
ro raw_ptr = h.0

// Type safety: distinct types prevent mixing
fn close_sqlite(h Sqlite3Handle) -> i64 { ... }

ro png = PngImageHandle(other_raw_ptr)
close_sqlite(png)                             // ✗ E_TYPE_MISMATCH — PngHandle ≠ Sqlite3Handle
```

#### Семантика

- **D52 tuple newtype** (`type X(Y)`) — existing mechanism, leveraged
  как-есть. Никаких новых parser/checker rules для handle pattern — он
  buisness layer convention, не language feature.
- **ABI.** Newtype = transparent wrapping. C-level Sqlite3Handle ≡ ptr ≡
  `void*`. Zero runtime overhead.
- **Distinct type.** Compile-time check `Sqlite3Handle ≠ PngHandle ≠
  ptr` — нельзя передать без явного wrap/unwrap.
- **Construct:** `Sqlite3Handle(ptr_value)` — standard tuple constructor.
- **Destructure:** `handle.0` — D52 tuple field access.

#### Generic opaque handle — `type X[T](ptr)` (Plan 91.12 V2, 2026-06-02)

Generic newtype над `ptr` поддерживается для type-parameterized FFI
handles (phantom T для compile-time discrimination):

```nova
type Region[T](ptr)             // generic phantom T
type RegionKind = Persistent
type RegionKind = Transient

// Distinct types at compile-time, identical ABI at runtime
ro p = Region[Persistent](some_ptr)
ro t = Region[Transient](other_ptr)
// fn drop_persistent(r Region[Persistent]) — нельзя передать Region[Transient]

// Multi-param OK
type DualHandle[T, U](ptr)
ro h = DualHandle[int, str](raw)
```

**Семантика.** `T` параметр — type-system fiction; C-level ABI identical
(`Nova_Region` ≡ `nova_ptr`). All monomorphizations share typedef.
Codegen emit'ает single `typedef nova_ptr Nova_X;` (не per-T), `.0` access
+ constructor — identity cast same как non-generic case.

**Use case:** phantom type discrimination для same-runtime-shape handles
(prepared statement kinds, region/arena ownership classes, FFI buffer
mutability flags, и т.д.).

**Inner non-ptr types** (Plan 91.12 V2 followup, 2026-06-02) — generic
newtype над любым primitive типом supported: `type Counter[T](int)`,
`type Tag[T](str)`, `type Flag[T](bool)`, `type Measure[T](f64)`.
Семантика идентична ptr-case: phantom T для compile-time discrimination,
single shared typedef над inner C type, zero runtime overhead. Use cases:
typed int counters, tagged strings (Email/UserId), tagged booleans
(Visible/Hidden), tagged floats (measurement units).

**Inner uses generic param** (`type Wrap[T](T)`) — **REJECTED** type-checker'ом
с `[E_GENERIC_NEWTYPE_INNER_USES_PARAM]`. Tuple newtype = transparent
typedef (shared C ABI across T's); per-T storage variance — record-semantics:

```nova
// ✗ E_GENERIC_NEWTYPE_INNER_USES_PARAM
type Wrap[T](T)                  // inner depends on T → not newtype

// ✓ Correct migration to record form (per-T mono)
type Wrap[T] { value T }         // properly mono'd по T
```

Closes `[M-91.12-generic-newtype-non-ptr-inner]`.

#### `consume close()` cleanup convention

Recommended pattern для handle types с resource ownership:

```nova
type Database { ro handle Sqlite3Handle }

fn Database.open(path str) Fail[DbError] -> Database {
    ro (h, rc) = nova_sqlite3_open(path)
    if rc != 0 { Fail.throw(DbError.OpenFailed(rc)) }
    Database { handle: h }
}

fn Database consume @close() -> () {
    nova_sqlite3_close(self.handle)
    // Plan 100.4 defer machinery интегрируется автоматически:
    // failable cleanup body допустим, ошибки propagate'ятся caller'у.
}
```

Combined с D90 `defer` / `errdefer` для automatic cleanup — leak-resistant
без runtime cost.

### 4. Coexistence с D126 `external type`

Plan 115 **не retracts** D126. Оба паттерна остаются valid:

| Pattern | Use case | Trade-offs |
|---|---|---|
| **D126** `external type X` | stdlib internals (Nova-team владеет C struct) | Tighter integration; C-side knows Nova types; no `.0` boilerplate |
| **D214** `type X(ptr)` | user FFI к third-party libs ИЛИ stdlib opting in | Universal; C-side не знает Nova internal layouts; `.0` для inner access |

**Recommendation.** Stdlib мигрирует на Plan 115 pattern для consistency
с user-FFI conventions (Plan 91.12 amend в Pattern B). D126 deprecation —
followup `[M-115-d126-deprecation]` после migration audit.

### Diagnostic codes

- `E_PTR_ARITHMETIC_BANNED` — попытка арифметики на `ptr` (V1 banned).
- `E_PTR_NO_MEMBER` — попытка `ptr.field` / `ptr.method()` — `ptr` opaque.
- `E_NULL_LITERAL_REQUIRES_PTR` — `null T` где T ≠ ptr (V1 ограничение;
  Plan 118 expand для `*T`).
- `E_PTR_CAST_INVALID_TARGET` — `ptr as T` где T ≠ {i64, u64, int, ptr} —
  string/float/bool casts не имеют semantic meaning для opaque pointer.

### Implementation notes

- **Parser** добавляет `"ptr"` в `is_primitive_type` allowlist (для
  `ptr.method` / static-dispatch namespace). `null ptr` literal — special
  case в `parse_atom` / `parse_primary`.
- **Type-checker** добавляет `Ty::Ptr` variant; `ty_of_ref` mapping `"ptr"
  => Ty::Ptr`; arithmetic / member access reject hooks.
- **Codegen** добавляет `"ptr" => "void*"` mapping в `type_ref_to_c`;
  `null ptr` → `((void*)0)`; cast emissions `((void*)(uint64_t)(...))`
  для int→ptr; `((uint64_t)(...))` для ptr→int.
- **GC** — no changes. Conservative GC handles `void*` слоты by-default.
- **Tuple FFI** — leveraging existing `_NovaTuple_*` mono'd struct
  pipeline (Plan 59 mechanism). C-side shim author writes matching struct
  typedef с теми же elements.

### Mainstream comparison

| Язык | Opaque pointer type | Typed wrappers |
|---|---|---|
| Rust | `*mut c_void` / `*const c_void` | `struct H(*mut c_void)` |
| Zig | `*anyopaque` / `?*anyopaque` | `const H = opaque {}; *H` |
| Go | `unsafe.Pointer` | `type H = unsafe.Pointer` |
| Haskell FFI | `Ptr ()` | `newtype H = H (Ptr ())` |
| OCaml ctypes | `unit ptr` | `type h = unit ptr` |
| Python ctypes | `c_void_p` | subclass `c_void_p` |
| Java JNI | `jlong` | (just `long`) |
| .NET P/Invoke | `IntPtr` / `nint` | `struct H { IntPtr h; }` |
| **Nova V1** | (нет) — нужны compiler hacks | — |
| **Nova V2 (Plan 115)** | `ptr` (built-in) | `type H(ptr)` (D52 tuple newtype) |

Nova V2 = Rust/Zig tier (typed wrappers без runtime overhead, opaque
deref, arithmetic banned by default).

### Use cases

- libsqlite3 binding (`type Sqlite3Handle(ptr)`, `type
  Sqlite3StmtHandle(ptr)`).
- libpng / libjpeg / libwebp image processing.
- libcurl HTTP client (Plan 117/118 prerequisite).
- rustls / OpenSSL TLS handles (Plan 116 prerequisite).
- Plan 91.12 std/net Pattern B migration (replaces D126 для TcpListener /
  TcpStream / UdpSocket если migration deemed worthwhile).
- Any third-party C library без Nova-team coordination.

### Cross-ref

- [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-) — tuple newtype `type X(Y)` (leveraged).
- [D82](03-syntax.md#d82) — external fn syntax (extended для tuple returns).
- [D126](03-syntax.md#d126) — external type (coexists; alternative
  pattern для stdlib internals).
- [D54](03-syntax.md#d54-explicit-cast-as-only) — `as`-cast operator
  (added ptr↔integer casts).

### Acceptance

См. Plan 115 A1-A10 (T1, T2, T3 series).

---

## D216. Typed pointer family + unsafe model + null-safety через NPO

> **Plan 118** (typed pointers + unsafe model). **Status:** 🟢 ACTIVE 2026-06-02
> (Ф.0 + Ф.1.5 + Ф.2 scaffold + Ф.3 + Ф.3.2 + Ф.3.3 + Ф.3.5 + Ф.4 partial +
> Ф.5 partial + Ф.6 partial — 13 acceptance criteria closed).
>
> Enforced diagnostics (V1):
>   - `E_UNSAFE_REQUIRED` (D216 §8) — A8 ✅ commit 5c0d2c975ce
>   - `E_UNSAFE_CALL_REQUIRES_WRAP` (D216 §9) — A11 ✅ commit abd4be4603b
>   - `E_CALLBACK_THROWS_OVER_C_ABI` (D216 §10/§20) — A25 ✅ commit e4cff57142e
>   - `E_EXTERNAL_FN_FAIL_EFFECT` (D216 §20) — A26 ✅ commit 7ff3007f3af
>   - `E_REALTIME_POINTER_OP` (D216 §20 + D172 cross-ref) — A33 ✅ commit 6752565f453
>   - `E_INVALID_POINTER_MODIFIER` (D216 §1) — commit 6d6a18a2ab7
>   - `E_AMP_LITERAL` / `E_AMP_RECORD_LITERAL` / `E_ARRAY_INDEX_PTR_BANNED`
>     (D216 §4 amend + §15) — commits d9d3084ed69 + 986fdb04c0d + 7d61617bcf8
>
> Remaining Session 4+ work (V1.1):
>   - Ф.4 full auto-deref codegen integration (A12-A17)
>   - Ф.5 NPO codegen (A19-A23 + closes [M-115-null-ptr-to-option-after-npo])
>   - Ф.6 full *fn cast checks (A24 — E_CLOSURE_HAS_ENV)
>   - Ф.7 W_UNSAFE_GC_TRIGGER + Debug fmt (A27, A28)
>   - Ф.8 cross-platform CI + ABI snapshot + perf bench (A31, A32)
>   - Plan 118.1/118.2/118.3 sub-plans
>
> **Cross-amend:** [D2](04-effects.md#d2) (unsafe keyword restored as
> effect-handler sugar), [D214](#d214-ptr-opaque-pointer-type--tuple-ffi-returns--opaque-handle-pattern)
> (ptr redefined как newtype), [D32](#d32-семантика-передачи-параметров)
> (`&value` is typed pointer construction, NOT Rust borrow),
> [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-)
> (tuple newtype `type Handle(*T)` canonical для FFI handles).

### Что

Foundational language addition: typed pointer family `*T` + unsafe gating
model + NPO null-safety. Replaces `ptr` opaque-only model из Plan 115 V1
с typed alternative; backward-compatible через D214 amend.

Plan 118 family scope:
- **Plan 118 core** (этот D216): `*T` family + unsafe + NPO + escape + `*fn` +
  GC honor-system
- **Plan 118.1** (D217): FFI memory intrinsics + C-string convention
- **Plan 118.2** (D218): slice fat-pointer + MaybeUninit + ManuallyDrop
- **Plan 118.3** (D219): pointer concurrency safety + AtomicPtr[T]

### §1. `*T` family типов

- `*T` (≡ `*ro T`) — readonly typed pointer (default)
- `*ro T` / `*mut T` — explicit mutability
- `*unsafe T` — pointer после арифметики (alignment/bounds gone)
- **Size:** pointer-width (8 bytes на 64-bit; bootstrap = 64-bit only)
- **ABI:** `T*` в C (compiler emits соответствующий C-type для FFI)
- **Validity:** **always non-null** (compile-time invariant); nullable
  variant — `Option[*T]` (NPO §7)

```nova
*T              // ro pointer (default)
*ro T           // explicit readonly
*mut T          // mutable (can *p = ...)
*unsafe T       // unsafe (после арифметики; deref требует ещё unsafe layer)
```

### §2. Binding mut rule

`mut p *T` ≡ `mut p *mut T` (pointer mut по default при mut binding).
Explicit `ro p *mut T` valid (edge case: cannot reassign p, BUT can `*p = ...`).

```nova
ro p *Acc           // binding ro; pointer ro
mut p *Acc          // binding mut; pointer mut auto
ro p *mut Acc       // valid edge case
mut q = &acc        // pointer mut auto (no &mut acc needed)
```

Consistency с Plan 114 binding semantics; reduces noise в hot-path FFI.

### §3. Chain order (multi-level pointers)

Modifier перед `*` относится к этому pointer'у; read left-to-right.

```nova
*mut *ro Acc        // mut pointer НА (ro pointer на Acc)
*ro *mut Acc        // ro pointer НА (mut pointer на Acc)
```

Canonical Rust grammar.

### §4. `&value` operator + escape analysis с auto-promote

- `&value` creates `*ro T` or `*mut T` (по контексту binding) — **в unsafe context**
- Stack values (primitives, tuples) auto-promoted в heap если pointer escapes
  scope (return / closure / heap-field store / fn arg)
- Records (heap references) — `&record` creates pointer на reference.
  Result C type: `Nova_Record**` (double-pointer because record уже
  Nova_Record* в C ABI). Used primarily для FFI out-params:
  `external fn try_init(out *Acc) -> i64` — C side fills `*out`.
- **`&Record { ... }` literal без named binding forbidden** —
  `E_AMP_RECORD_LITERAL`. Anonymous-local auto-promote from temporary
  слишком implicit для production-grade reader clarity. Required pattern:
  ```nova
  // ❌ implicit anonymous local
  ro p = &Acc { name: "Piter" }
  // ✓  explicit named local
  ro acc = Acc { name: "Piter" }
  ro p = &acc
  ```
- GC-friendly семантика (vs Rust lifetimes — у нас GC + auto-promote)
- Conservative V1: promote если ANY uncertainty; precise inlining followup
  `[M-118-escape-precise]`

**D32 amend rationale:** `&value` это **typed pointer construction**, не Rust
borrow. Safety через escape analysis + auto-promote + unsafe gating (не
lifetime checker). См. D32 amend note.

### §5. Auto-deref

```nova
unsafe {
    p.field             // ✓ auto-deref one-level read
    p.method()          // ✓ auto-deref method call (one-level)
    p.field = v         // ✓ auto-deref assignment (requires *mut T)
    *p                  // ✓ explicit deref read
    *p = v              // ✓ explicit assignment (requires *mut T)
    (*p).field          // ✓ multi-level chain через explicit *
}
```

**Rules:**

| Op | `*ro T` | `*mut T` | Notes |
|---|---|---|---|
| `p.field` read | ✓ | ✓ | auto-deref one-level |
| `p.field = v` | ❌ E_POINTER_RO_ASSIGN | ✓ | requires `*mut` |
| `p.method()` (ro recv) | ✓ | ✓ | auto-deref |
| `p.method()` (mut recv) | ❌ E_POINTER_RO_MUT_METHOD | ✓ | requires `*mut` |
| `*p` read | ✓ | ✓ | yields T |
| `*p = v` | ❌ E_POINTER_RO_ASSIGN | ✓ | requires `*mut` |

**One-level only** для auto-deref (Go-style); multi-level requires explicit
`(*p).field` chain. **Только в unsafe context** — все pointer ops gated.
Pattern match `Option[*T]` — safe outside unsafe (inspection, не deref).

### §6. Pointer arithmetic

```nova
unsafe {
    ro p1 = some_ptr + 1            // *unsafe T (degraded)
    ro p2 = some_ptr + offset
    ro diff = p2 - p1               // isize (element count)
    unsafe { *p1 }                   // *unsafe T deref требует ещё unsafe layer
}
```

- `+`/`-`/`+=`/`-=` only в `unsafe { }` block
- Result `*unsafe T` для `ptr ± int`; `isize` для `ptr - ptr`
- Units: sizeof(T)-scaled (C/Rust convention)
- `*`/`/`/etc. — `E_PTR_ARITHMETIC_INVALID` (не математически осмыслено)

### §7. Null safety: `Option[*T]` + NPO codegen

`*T` — non-null guaranteed. `Option[*T]` — nullable через **NPO codegen**:

- Layout: single pointer (8 bytes), не tagged struct (16 bytes)
- Pattern match: `if (ptr == NULL) None_branch else Some_branch(ptr)`
- Direct C-FFI compatible (matches `malloc` / `fopen` / `dlopen` returns)

```nova
external fn malloc(sz usize) -> Option[*u8]
// → C: uint8_t* malloc(size_t);

unsafe {
    match malloc(1024) {
        Some(buf) => use(buf),       // buf: *u8 non-null
        None      => Fail.throw(OutOfMemory),
    }
}
```

**NPO applies к:**
- `Option[*T]` всех вариантов
- `Option[*fn(...) -> ...]`
- `Option[ptr]` (D214 amend)
- `Option[NewtypeOверPtr]` где `type X(*T)` / `type X(ptr)`

**Excluded:** nested `Option[Option[*T]]` — fallback к tagged repr +
`W_OPTION_DOUBLE_NESTED` warning.

### §8. `unsafe { }` block

- Pointer ops require unsafe context (compile-time gating через
  `E_UNSAFE_REQUIRED`)
- Implementation: sugar над `with unsafe_handler { perform UnsafeOps.* }`
  (D2-consistent; см. [D2 amend](04-effects.md#d2))
- `unsafe_handler` — built-in, не user-overridable
  (`E_UNSAFE_HANDLER_BUILTIN_ONLY`)
- Effect не propagates up (encapsulates per fn — canonical Rust pattern)

**Inside unsafe разрешено:** `&value`, `*p`, `p.field`, `p.method()`,
`p.field = v`, pointer arith, `usize as *T`, `<`/`>` compare, `&record.field`,
calling `#unsafe` fn, newtype construction wrapping pointer.

**Outside unsafe safe:** type declarations `*T`, `external fn` declarations,
field read `acc.next` (where `next *T`), pattern match `Option[*T]`,
`==`/`!=` compare, newtype declarations, `p as usize` (hash hazard warning).

### §9. `#unsafe` attribute

- `#unsafe fn` body — implicit unsafe context (pointer ops без `unsafe { }`
  wrap)
- Call `#unsafe` fn — requires `unsafe { ... }` wrap у caller (visual
  marker) — `E_UNSAFE_CALL_REQUIRES_WRAP` иначе
- No propagation up — каждая fn decides encapsulate or propagate

### §10. `*fn(...)` function pointers

- `*fn(Args) -> Ret` distinct от `fn(Args) -> Ret` closure
- Cast `fn → *fn` — captureless required (`E_CLOSURE_HAS_ENV` иначе)
- Cast `*fn → fn` — unsafe (wraps в captureless closure;
  `E_CAST_RAW_FN_TO_CLOSURE` без unsafe)
- **Callback no-throw:** Fn-with-Fail effect cast → *fn —
  `E_CALLBACK_THROWS_OVER_C_ABI` (C ABI не propagates Nova exceptions)
- **External fn no-Fail:** `external fn ... Fail -> ...` —
  `E_EXTERNAL_FN_FAIL_EFFECT`
- Calling convention: default C ABI текущей платформы (single ABI V1;
  stdcall/vectorcall — `[M-118-stdcall-fn-ptr]` followup)
- Vararg — `E_VARARG_NOT_SUPPORTED` (`[M-118-vararg-ffi]` followup)

### §11. `ptr` redefine (D214 amend cross-ref)

```nova
type ptr Option[*unsafe ()]
```

- ABI preserved (single `void*`)
- `null ptr` literal **retracted** (use `None`); closes
  `[M-115-null-ptr-to-option-after-npo]` ✅
- Backward-compatible для existing `ptr` usages (handle patterns, tuple
  FFI returns, etc.)

### §12. Casts

| From | To | Safe? |
|---|---|---|
| `*T` | `usize` | ✓ (см. hash hazard) |
| `usize` | `*T` | unsafe |
| `*ro T` | `*mut T` | unsafe |
| `*mut T` | `*ro T` / `*T` | ✓ |
| `*T` | `*unsafe T` | ✓ |
| `*unsafe T` | `*T` | unsafe |
| `*T1` | `*T2` (T1≠T2) | unsafe |
| `fn → *fn` | ✓ если captureless | `E_CLOSURE_HAS_ENV` иначе |
| `*fn → fn` | unsafe | wraps |
| `*T` | `bool` / `f64` / etc. | ❌ `E_PTR_CAST_INVALID_TARGET` |

**Hash hazard:** `p as usize` для GC-tracked objects + HashMap key →
`W_PTR_AS_USIZE_GC_HASH_HAZARD` (address can change via GC compaction).

### §13. Comparison

- `==`/`!=` safe (identity check)
- `<`/`>`/`<=`/`>=` unsafe (cross-allocation UB + moving GC concern)

### §14. `&record.field` only в unsafe

GC compaction concern: address меняется при collection. Inside unsafe —
user обещает no GC trigger (honor-system §16).

### §15. Forbidden ops

- `&arr[i]` всегда — `E_ARRAY_INDEX_PTR_BANNED` (array buffer может
  relocate via realloc / GC compaction)
- `null` literal — `E_NULL_LITERAL_USE_NONE` (use `None`; one-way-to-do)
- `undefined` — `E_UNDEFINED_USE_NONE_INIT_PATTERN` (use `Option[*T] =
  None + init`; полноценный `MaybeUninit[T]` — Plan 118.2)
- Vararg calls — `E_VARARG_NOT_SUPPORTED`

### §16. GC honor-system

**Контракт unsafe-блока:** внутри `unsafe { ... }` user **обещает** no GC
trigger между pointer creation и use. GC trigger = heap allocation,
yield-point (await/spawn/supervised{}), string formatting which allocates,
`#parks`/`#wakes` fn calls.

**Compiler warns:** `W_UNSAFE_GC_TRIGGER` per violation site.
**Silence:** `// noqa: W_UNSAFE_GC_TRIGGER` comment marker.

**Rationale V1:**
- Boehm-style conservative GC не двигает объекты → V1 безопасно (warning =
  awareness, not error)
- Future moving GC → potрebует formal pin API (`[M-118-pin-api]` followup)
- Honor-system + warning = pragmatic trade-off (no runtime cost, spec
  contract clear, future-compatible)

### §17. Pointer Debug formatting

- `(*T).to_debug_str() -> str` — built-in method (in unsafe context only)
- Emits hex address + type name (`"0x7f... -> Account"`)
- НЕ implements Display — forces explicit decision (pointer debugging =
  deliberate; addresses non-deterministic, leak ASLR info)
- `"${p}"` interpolation → `E_PTR_NO_DISPLAY_USE_DEBUG_STR` —
  **ACTIVE 2026-06-02 (V1 syntactic, commit a9327c65d3f)** —
  closes acceptance A28 partial. V1 detects:
    - direct `${&x}` / `${*p}` (Unary AddrOf/Deref)
    - `${expr as *T}` (cast к pointer type)
    - `${var}` где var bound через `let var = AddrOf/Deref/As(*T)`
  V2 (Session 4+): full type-aware enforcement через `infer_expr_type` —
  fires на returned pointer values, field access, generic-bound `*T`.

### §18. FFI handle allocation contract

**Production-grade guidance:**

| Form | Allocation | ABI | When |
|---|---|---|---|
| `type Handle(*T)` tuple newtype | **stack** | single pointer | opaque handles, no extra state |
| `type Handle(ptr)` tuple newtype | **stack** | single pointer | untyped opaque handles |
| `type Handle { p *T, extra State }` record | **heap** | pointer-to-struct | handle с extra state |

**Canonical (zero-overhead):**
```nova
type Sqlite3Handle(*sqlite3)
external fn open(path str) -> (Option[Sqlite3Handle], i64)
```

Plan 115 V1 cookbook examples (record form `type Db { ro value ptr }`) —
migrated к tuple newtype в Plan 118 Ф.9 (`[M-118-handle-migration]`).

### §19. Function call argument passing

- `*T` parameters — pass by value (single pointer-word; standard C ABI)
- `&value` at call site creates `*T` argument
- Auto-promote applies к escape-via-fn-arg (conservative: ESCAPE always
  for fn args; precise inlining `[M-118-escape-precise]` followup)

### §20. `extern "C-unwind"` story (NEGATIVE — not V1)

V1: external fn + `*fn` callbacks **must not** have Fail effect on Nova→C
boundary. Diagnostics: `E_EXTERNAL_FN_FAIL_EFFECT`,
`E_CALLBACK_THROWS_OVER_C_ABI`. Workaround: catch внутри callback, return
sentinel.

V2 — research `extern "C-unwind"` (Rust 2024 model);
`[M-118-extern-c-unwind]` followup.

### Diagnostic codes (new)

**Errors:**
- `E_UNSAFE_REQUIRED` — pointer op (`&value` AddrOf / `*expr` Deref) outside
  unsafe context (block.is_unsafe = false AND not в `#unsafe fn` body).
  Active enforcement через `check_unsafe_context_in_module` walker pass с
  depth counter — D216 §8 V1 ENFORCED 2026-06-02
- `E_UNSAFE_CALL_REQUIRES_WRAP` — calling `#unsafe` fn без `unsafe { }`
  wrap. Active enforcement через `check_unsafe_context_in_module` walker
  с pre-collected unsafe_fns: HashSet<String>. D216 §9 V1 ENFORCED
  2026-06-02 (commit abd4be4603b)
- `E_ARRAY_INDEX_PTR_BANNED` — `&arr[i]`
- `E_NULL_LITERAL_USE_NONE` — `null` literal (general)
- `E_NULL_PTR_RETRACTED_USE_OPTION` — `null ptr` (Plan 115 V1) retracted
- `E_UNDEFINED_USE_NONE_INIT_PATTERN` — `undefined` used
- `E_CLOSURE_HAS_ENV` — fn → *fn cast attempted с closure env
- `E_CALLBACK_THROWS_OVER_C_ABI` — Fn-with-Fail → *fn cast. Active
  enforcement — D216 §10/§20 V1 ENFORCED 2026-06-02 (commit e4cff57142e)
- `E_EXTERNAL_FN_FAIL_EFFECT` — external fn declaration с Fail
- `E_PTR_ARITHMETIC_INVALID` — `p * 2`, `p / 4`, etc.
- `E_POINTER_RO_ASSIGN` — `*p = v` / `p.field = v` где p ro
- `E_POINTER_RO_MUT_METHOD` — `p.mut_method()` где p ro
- `E_PTR_CAST_INVALID_TARGET` — `p as bool / f64 / ...`
- `E_INVALID_POINTER_MODIFIER` — `*const T` и др.
- `E_DUPLICATE_POINTER_MODIFIER` — `*ro mut T`
- `E_PARSE_POINTER_TYPE_INCOMPLETE` — `*` без type
- `E_REALTIME_POINTER_OP` — pointer op в `#realtime fn` body. Active
  enforcement — D216 §20 + Plan 113 D172 V1 ENFORCED 2026-06-02
  (commit 6752565f453)
- `E_UNSAFE_HANDLER_BUILTIN_ONLY` — user-defined unsafe_handler attempt
- `E_AMP_CONST_BINDING` — `&const_value`
- `E_AMP_LITERAL` — `&42`
- `E_AMP_RECORD_LITERAL` — `&Record { ... }` без named binding (Plan 118 §4 amend)
- `E_PTR_NO_DISPLAY_USE_DEBUG_STR` — `"${p}"`
- `E_VARARG_NOT_SUPPORTED` — vararg FFI call
- `E_CAST_RAW_FN_TO_CLOSURE` — `*fn → fn` cast outside unsafe

**Warnings:**
- `W_UNSAFE_GC_TRIGGER` — GC trigger внутри unsafe с pointer in scope
- `W_PTR_AS_USIZE_GC_HASH_HAZARD` — `p as usize` как HashMap key
- `W_OPTION_DOUBLE_NESTED` — `Option[Option[*T]]` NPO fallback

### Mainstream comparison

| Язык | Typed ptr | Unsafe model | Null safety | Auto-deref | Arithmetic |
|---|---|---|---|---|---|
| Rust | `*const T`/`*mut T`/`&T`/`&mut T` | `unsafe { }` + `unsafe fn` | `Option<&T>` + NPO | через ref | unsafe only |
| Zig | `*T`/`*const T`/`[*]T` | (нет keyword; intrinsics) | `?*T` + NPO | `.*` postfix + `.` | `+` для `[*]T` only |
| C# | `T*` (unmanaged) / `ref T` / `in T` / `out T` | `unsafe` modifier | `T?` | `p->field` | unsafe only |
| Swift | `UnsafePointer<T>` / `UnsafeMutablePointer<T>` | Type-based (Unsafe* prefix) | Optional + NPO | `.pointee` | only через `advanced(by:)` |
| D | `T*` / `ref T` / `scope T*` | `@safe`/`@trusted`/`@system` | `Nullable!T` | `p.field` auto | `@system` only |
| Go | `*T` (managed); `unsafe.Pointer` | `unsafe` package | Nil runtime | `p.field` auto | `unsafe.Pointer` only |
| **Nova V1** (Plan 115) | `ptr` only | (нет) | `null ptr` | (нет) | banned |
| **Nova V2** (Plan 118) | **`*T` family** + `unsafe` | `unsafe { }` + `#unsafe` (D2 amend) | `Option[*T]` + NPO | `p.field`/`p.method()` one-level | gated unsafe → `*unsafe T` |

### Use cases

- Typed FFI buffers (libpng image data, libcurl headers, sqlite blobs) —
  full impl Plan 118.1 (memory primitives) + 118.2 (slice fat-pointer)
- Memory-mapped I/O (registers, framebuffers) — Plan 118.1 volatile RW
- Manual linked structures (intrusive lists, lock-free queues, custom
  allocators) — Plan 118.3 AtomicPtr
- Performance-critical hot loops (escape analysis + GC-pressure reduction)
- Out-params для FFI (`int func(out int* result)`) — Plan 118.1 addr_of_mut!

### Cross-ref

- [D2 (amend)](04-effects.md#d2) — `unsafe { }` keyword restored
- [D32 (amend)](#d32-семантика-передачи-параметров) — `&value` not Rust borrow
- [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-) — type forms (tuple newtype canonical для FFI handles)
- [D214 (amend)](#d214-ptr-opaque-pointer-type--tuple-ffi-returns--opaque-handle-pattern) — ptr redefine
- [D215](#d215-named-tuple-fields--valuereference-allocation-contract) — Plan 120 stack tuples (escape interaction)
- [D172](06-concurrency.md#d172) — `#realtime` ban для pointer ops
- [D217](#d217-ffi-memory-primitives--c-string-convention) — Plan 118.1 (FFI intrinsics)
- [D218](#d218-slice-fat-pointer--maybeuninit--manuallydrop) — Plan 118.2 (slice + uninit)
- [D219](#d219-pointer-concurrency-safety--atomicptr) — Plan 118.3 (concurrency)
- [Plan 118](../../docs/plans/118-typed-pointers-and-unsafe.md) — implementation

### Acceptance

См. Plan 118 A1-A35 (T1-T8 + R1-R5 series).

---

## D220. Per-field visibility — `priv` keyword + type-level default flip

> **Status:** V1 ACTIVE (spec + parser/AST infrastructure landed, 2026-06-02). Реализация — [Plan 124](../../docs/plans/124-priv-field-visibility.md). Empirical validation — [docs/research/06-field-visibility-go-kubernetes.md](../../docs/research/06-field-visibility-go-kubernetes.md). Amends [D47](07-modules.md#d47) (replaces deprecated `_prefix` convention с compile-time enforcement).

### Что

Per-field visibility modifier `priv` для records + named tuples (D215). По умолчанию все поля **публичны** (D47 unchanged, validated: kubernetes 92% public в API surface). Explicit `priv` — field accessible **только из методов own type'а** (instance + static).

Type-level default flip syntax `type X priv { ... }` — для invariant-heavy types где majority of fields private; explicit `pub` modifier overrides priv default.

### Правило

#### §1 Syntax

```nova
// Per-field priv modifier (field-level).
export type Account {
    priv mut money f64
    ro name str
    priv id u64
}

// Type-level default flip — fields default = priv.
export type Secret priv {
    pub ro tag str
    mut salt u64
    key u64
}
```

Modifier ordering в field decl: priv/pub → ro/mut/consume → name TYPE. priv и pub mutually exclusive (E_PRIV_PUB_CONFLICT).

#### §2 Effective visibility

Field's effective priv_field = first matching:
1. Explicit `pub` field modifier → priv_field = false (public).
2. Explicit `priv` field modifier → priv_field = true (private).
3. Type-level default (`type X priv {...}` → priv_field = true).
4. Otherwise (D47 default) → priv_field = false (public).

#### §3 Access rules

priv field access РАЗРЕШЁН только из:
- Instance methods own type'а: `fn TypeX @method() { @priv_field }`
- Static methods own type'а: `fn TypeX.factory(...) { ... }`

priv field access ЗАПРЕЩЁН во всех других контекстах:
- Read: `outside.priv_field` → E_PRIV_FIELD_READ
- Write: `outside.priv_field = X` → E_PRIV_FIELD_WRITE
- Init via record literal: `Foo { priv_f: X }` → E_PRIV_FIELD_INIT
- Pattern destructure: `Foo { priv_f }` → E_PRIV_FIELD_PATTERN

#### §4 Diagnostic codes

- E_PRIV_FIELD_READ — read priv field outside type-method scope.
- E_PRIV_FIELD_WRITE — write priv field outside type-method scope.
- E_PRIV_FIELD_INIT — init priv field via literal outside.
- E_PRIV_FIELD_PATTERN — destructure priv field в pattern outside.
- E_PRIV_PUB_CONFLICT — both priv и pub modifiers на одном field.
- E_PRIV_FIELD_PROTOCOL (V4 deferred).
- E_PRIV_TUPLE_POSITIONAL_ACCESS (V4 deferred).

#### §5 No reflection backdoor

Nova не имеет reflection API → priv enforcement compile-time hard guarantee. Vs Java/Kotlin/C#/Swift которые имеют reflection bypass.

#### §6 Composition

priv composes orthogonally с:
- ro/mut/consume mutability modifiers
- use NAME Type (D39 embed) — V2 deferred [M-124.2-priv-embed]
- const NAME T = expr — reserved future use

#### §7 Backward compatibility

Existing Nova code = all-public fields → migration purely additive. priv opt-in keyword — старый код не ломается. _prefix convention deprecated 2026-06-02.

### Почему

Empirical validation: kubernetes audit 35239 fields — 92.4% public в API surface. Public-default minimum boilerplate. Bimodal distribution → bimodal syntax (field-level priv + type-level priv {} flip).

Compile-time enforcement vs convention: prior _prefix hint-only privacy — false safety. priv keyword вводит compile-time guarantee → refactoring safety + invariant enforcement + API clarity.

### Что отвергнуто

- Private-by-default — отклонено после kubernetes data.
- Edition default flip — отклонено (per-type granular лучше).
- #strict_visibility per-module attribute — отклонено (fragmentation).

### Cross-refs

- D5 (07-modules.md) — module-level visibility.
- D29 (07-modules.md) — modules.
- D35 (03-syntax.md) — method declaration.
- D47 (07-modules.md) — export keyword; _prefix deprecated.
- D52 (this file) — record/sum/alias syntax.
- D131 (05-memory.md) — consume types.
- D215 (this file) — named tuples.

### Acceptance

V1 (Plan 124.1) — ALL closed 2026-06-02:
- A1.1-A1.3 ✅ Parser/AST infrastructure (Ф.1 + Ф.4 commits).
- A1.4 ✅ E_PRIV_FIELD_READ enforcement (Ф.2 — f3_check_member hook).
- A1.5 ✅ E_PRIV_FIELD_WRITE enforcement (Ф.2.2 — check_target_readonly hook).
- A1.6 ✅ E_PRIV_FIELD_INIT enforcement (Ф.2.3 — RecordLit walk_expr hook).
- A1.7 ✅ E_PRIV_FIELD_PATTERN enforcement (Ф.2.4 — Pattern::Record f1_block hook).
- A1.8 ✅ Regression 0 new FAIL.
- A1.9 ✅ plan124_1 fixtures 9/9 PASS (4 positive + 5 negative).
- A1.10 ✅ Spec D220 NEW (this section).

### Followup markers

- ✅ [M-124.1-checker-enforcement] CLOSED 2026-06-02 — all 4 codes via TypeCheckCtx current_recv_type RAII tracking.
- [M-124.2-priv-embed] — priv use NAME Type.
- [M-124.4-tuple-priv] — named tuple priv (D215 ext).
- [M-124.4-protocol-impl-boundary].
- [M-124.5-doc-lsp].
- [M-124.6-test-access].
