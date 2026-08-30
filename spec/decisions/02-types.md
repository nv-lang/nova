# Types — record, sum-type, protocol, generic, поля

Решения этой группы задают систему типов Nova: четыре формы объявления
данных, структурные контракты-протоколы, семантику передачи параметров
и мутабельность полей, делегацию через `use`. Синтаксические детали
(методы через `@`, generic-применение `[T]`, литералы) — в
[03-syntax.md](03-syntax.md).

| # | Решение | Status |
|---|---|---|
| [D17](#d17-объявление-типов-единый-синтаксис-без-) | Объявление типов: единый синтаксис без `\|` | revised → D52 |
| [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-) | Объявление типов revised: newtype, `alias`, sum через leading `\|` | revised → D406 |
| [D406](#d406-sum-type-синтаксис-через-enum-маркер-2026-07-01) | Sum-type синтаксис: `enum` маркер вместо leading `\|`; inline enum в type-позиции | active |
| [D53](#d53-унификация-protocol-под-type-protocol-как-kind-токен) | Унификация: `protocol` под `type`, `protocol` как kind-токен | active |
| [D55](#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы) | Literal coercion в позиции с явным типом: sum-конструкторы и record-литералы | active |
| [D42](#d42-protocol-keyword-для-структурных-интерфейсов) | `protocol` keyword для структурных интерфейсов | revised → D53 |
| [D15](#d15-структурные-интерфейсы) | Структурные интерфейсы | revised → D42 → D53 |
| [D39](#d39-embed-и-delegation-use-name-type-alias-обязателен) | Embed и delegation: `use name Type` (alias обязателен) | active |
| [D32](#d32-семантика-передачи-параметров) | Семантика передачи параметров | revised для полей → D36 |
| [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindingа-ro-для-never-mut) | Поля типа: дефолт mutable у mut-binding'а, `ro` для never-mut | active |
| [D175](#d175-ro-field--полный-freeze-амендмент-d36) | `ro field` — полный freeze, транзитивность (амендмент D36) | active |
| [D176](#d176-ro-t--тип-модификатор) | `ro T` — тип-модификатор, coercion rules, zero overhead | active |
| [D66](#d66-self-universal--ссылка-на-обобщающий-тип-в-методах-effects-protocols) | `Self` universal: ссылка на обобщающий тип в методах, effects, protocols | active |
| [D72](#d72-generic-bounds-через-t-protocol--protocol-как-тип) | Generic bounds через `[T Protocol]` — protocol как тип | active |
| [D110](#d110-ghost-state--spec-only-bindings) | Ghost state — spec-only bindings | active |
| [D122](#d122-hybrid-dispatch-для-bound-k-methods) | Hybrid dispatch для bound-K methods | active |
| [D123](#d123-tuple-monomorphization) | Tuple monomorphization | active |
| [D215](#d215-named-tuple-fields--valuereference-allocation-contract) | Named tuple fields + value/reference allocation contract | active |
| [D119](#d119-method-level-type-parameters-в-generic-methods) | Method-level type parameters в generic methods | active |
| [D372](#d372-canonical-new-constructors-convention) | Canonical `.new()` constructors (convention) | active |
| [D181](#d181-array-methods----fluent-mut-chain--slice-syntax) | Array methods — `-> @` fluent mut chain + slice syntax | active |
| [D182](#d182-self-в-return-type-static-methods--required-form-для-parametric-types) | `Self` в return-type static methods — required form для parametric types | active |
| [D183](#d183-canonical-comparison-protocols--default-method-bodies-plan-918a) | Canonical comparison protocols + default method bodies (Plan 91.8a) | active |
| ~~flip-scan-draft~~ | Pointer mutability — running-current flip-scan (Plan 147; **RETRACTED 2026-06-12** → D246) | retracted |
| [D246](#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee) | Три оси мутабельности (L1 binding / L2 view / L3 pointee); restores `*T ≡ *ro T` universally; `E_REDUNDANT_POINTER_RO` (Plan 147) | active |
| [D281](#d281-module-level-field-privacy--type-x-priv---plan-160) | Module-level field privacy `type X priv { … }` — bare `priv` = module-private (Plan 160, D281) | active |
| [D355](#d355--blanket-protocol-receiver-methods-plan-161-2026-06-15) | Blanket protocol-receiver methods (ex-D282, renumber 2026-07-03) `fn[I Next[T]] I @m` — typevar-ресивер + bound-dispatch (Plan 161, G-F) | active |
| [D284](#d284-enumerateiter--zero-cost-enumerate-adapter-plan-162) | `EnumerateIter[I, T]` — zero-cost enumerate adapter; per-type `@zenumerate()` dispatch; tuple parametric return (Plan 162) | active |
| [D290](#d290--value-record-iterator-types-plan-165-2026-06-16) | Iterator value-records: `VecIter[T] value` (GC-pointer fields covered by fiber arena) + `Range`/`RangeIter`/`StepRangeIter`/`ReverseRangeIter value` (int-only, pure stack) — zero malloc in adapter chain (Plan 165) | active |
| [D307](#d307-file-private-visibility--privfile-plan-170) | File-private visibility `priv(file) fn`/`type`/`const` — лесенка `priv(file)` ⊂ module ⊂ export; `E_FILE_PRIV_LEAK`; file-discriminated codegen; dedup одноимённых в peer-файлах (Plan 170) | active |
| [D315](#d315-resolvedtype--единый-канонический-носитель-типа-plan-1721-2026-06-21) | `ResolvedType` — единый канонический носитель типа; проверки/совместимость/конверсии/перевод-в-C выводятся из него; `type_ref_to_c` ретайрится; сахар нормализуется; ABI выводится, не хранится (Plan 172.1, D315) | active |
| [D310](#d310-type-set-bounds-plan-1723) | Type-set bounds: `type Name set T1 \| T2 \| …` — именованное множество конкретных типов как generic-bound; `SignedInts`/`UnsignedInts`; Go-style type-constraint (Plan 172.3) | active |
| [D429](#d429-coerce--декларативные-неявные-zero-cost-конверсии-view--finalize-plan-214-2026-07-18) | `#coerce` — декларативные неявные zero-cost конверсии (view + finalize) | active |

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
- [02-types.md → D36](#d36-поля-типа-дефолт-mutable-у-mut-bindingа-ro-для-never-mut)
  — префиксы полей (`ro`, `mut`) и group-syntax внутри record.

---

## D52. Объявление типов revised: newtype, `alias`, sum через leading `|`

> ⚠️ **REVISED.** Синтаксис sum-type заменён [D406](#d406-sum-type-синтаксис-через-enum-маркер-2026-07-01): `enum Variant | ...` вместо leading `| Variant | ...`. Остальные формы (newtype, alias, record, tuple, unit) без изменений.

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
- **sum** — `type X enum A | B | C` (`enum` маркер обязателен; D406)

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

// 6. Sum — type X enum A | B (enum маркер обязателен; D406)
type Color enum Red | Green | Blue
type Direction enum North | East | South | West

// Sum многострочный — если варианты на новых строках, | обязателен у каждого
type Result[T, E] enum
    | Ok(T)
    | Err(E)

type Shape enum
    | Circle { radius f64 }
    | Square { side f64 }
    | Triangle { a f64, b f64, c f64 }
```

#### Парсер однозначен по первому токену после имени (с учётом дженериков)

| После `type X` (или `type X[params]`) идёт | Форма |
|---|---|
| `enum` | sum (D406) |
| `protocol` | protocol-тип (D53) |
| `effect` | effect-тип (D53) |
| `set` | type-set bound (D310) |
| `value` `{` | value-record — stack-allocated (D228/D277/D290) |
| `(` + ident + bare-type | named tuple (D215) — `(name1 T1, name2 T2)` |
| `(` + bare-type | positional tuple — `(T1, T2)` |
| `{` | record — heap-allocated (GC-managed) |
| `alias` | alias |
| `<base-type>` `enum` | sum с явным базовым типом для discriminants |
| идентификатор/тип, конец строки | newtype |
| конец строки сразу | unit |

Парсер видит первый токен — сразу знает форму. Для `(` — один
дополнительный lookahead: если `(IDENT type` → named tuple,
иначе → positional tuple. Никакого backtracking.

#### Модификаторы type-declaration

Помимо kind-токена, type-declaration может нести **модификаторы** — они не
меняют «форму» типа, а добавляют квалификаторы:

| Модификатор | Семантика | D-block |
|---|---|---|
| `export` | виден снаружи модуля | D47 |
| `value` | stack-allocated (по значению, не в GC-куче) | D228/D290 |
| `priv` | поля module-private по умолчанию | D281 |
| `priv(type)` | поля type-private по умолчанию | D281 |
| `priv(file)` | символ виден только в этом файле | D307 |
| `consume` | must-be-consumed affine type | D133 |

`priv(file)` — двойная позиция: prefix перед `type` (`priv(file) type X { … }`)
**или** modifier после имени (`type X priv(file) { … }`). Обе формы эквивалентны.
Для `fn` и `const` только prefix-форма: `priv(file) fn f()`, `priv(file) const K`.

Грамматика type-declaration:
`[export] type Name[T] [value] [priv|priv(type)|priv(file)] [consume] { … }`
эквивалентно:
`[export|priv(file)] type Name[T] [value] [priv|priv(type)] [consume] { … }`

Модификаторы **комбинируются**: `export type Job value priv consume { … }`.

Поля внутри `{…}` могут иметь **field-level** модификаторы:
`ro` (D175), `mut` (D36), `priv` / `priv(type)` / `priv(file)` (D281/D307).

#### Sum-варианты с числовыми discriminants

```nova
// Auto-increment без явных значений (от 0)
type ExitStatus enum Ok | Failure | Critical                  // 0, 1, 2

// Auto-increment от заданного
type FileMode enum Read = 1 | Write | Execute                 // 1, 2, 3

// Все явные
type ErrorCode enum
    | NotFound       = 404
    | Unauthorized   = 401
    | InternalError  = 500

// С отрицательными
type Sign enum Negative = -1 | Zero = 0 | Positive = 1

// Decreasing/non-monotonic — разрешено
type Code enum A = 10 | B = 5 | C                            // A=10, B=5, C=6

// Явный базовый тип
type Bit u8 enum Off = 0 | On = 1
type HttpCode i32 enum Ok = 200 | NotFound = 404
```

> ⚠ **Явный базовый тип пока не реализован** (parser drift, 2026-05-27).
> Формы с `u8`/`i32`/etc. между именем и `enum` парсер отвергает.
> Работает только дефолтная форма (без базового типа, implicit `int`). См.
> [Plan 105](../../docs/plans/105-sum-type-explicit-base.md).

**Правила discriminants:**

1. **Базовый тип** — дефолт `int`. Опционально явный (`type X i32 enum`,
   `type X u8 enum`).
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
   type Event enum
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
type Option[T] enum Some(T) | None
type Result[T, E] enum Ok(T) | Err(E)
type Tree[T] enum
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
- **`type X = A, B`** для sum — заменён на `type X enum A | B`.
- **`type X = { ... }`** для record — синтаксис никогда не был активным
  (D17 уже отвергал), `=` в этой позиции запрещён.
- **`,` для разделения вариантов sum** — заменено на `|`.
- **Sum без `enum` маркера** — запрещён (`type X Red | Green` ✗,
  `type X enum Red | Green` ✓). См. D406.
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
- **Kind-токен `enum` с фигурными скобками** (`type X enum { A, B }`). Заменено
  на `type X enum A | B` без скобок (D406).
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
- [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindingа-ro-для-never-mut)
  — префиксы полей (`ro`, `mut`) и group-syntax внутри record.
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
- ~~**Implicit cast литерала в newtype.**~~ **Закрыто** (Plan 200,
  2026-07-12) — [D55 §Obvious single-wrapper coercion](#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы):
  и литерал (`let u UserId = 42`), и binding (`let n = 42; let u
  UserId = n`) авто-коэрсятся одинаково (`... as UserId`), без разницы
  между литералом и переменной.

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

### Амендмент (ОКНО-5, 2026-07-23): newtype над fn-типом + call-through + alias-прозрачность

**Проблема.** `type Handler fn(ServerRequest) -> ServerResponse` (newtype,
underlying — fn-тип) **не парсился**: `expected identifier, got '('`.
Причина — parser-эвристика «пустой sum» (empty-sum body-end detection,
см. Plan 72 P1-B выше) трактовала ЛЮБОЙ голый `KwFn` сразу после `type X`
как начало СЛЕДУЮЩЕЙ top-level декларации (`type X` без тела + отдельная
`fn name(...)`), а не как начало fn-типа. Настоящий top-level `fn` ВСЕГДА
несёт identifier сразу после keyword (`fn name(`); fn-TYPE body, наоборот,
начинается `fn(` — сразу `(`, без имени. Однозначный lookahead одним
токеном: `fn` считается концом декларации (следующая `fn`-декларация)
ТОЛЬКО когда токен ПОСЛЕ него — НЕ `(`.

**Форма (закрывает синтаксический пробел, D52 §Полный синтаксис остаётся
без изменений — это РАСШИРЕНИЕ newtype-формы `type X Y`, где `Y` теперь
может быть fn-типом, не только именованным типом):**

```nova
type Handler fn(ServerRequest) -> ServerResponse   // newtype над fn-типом
type HandlerAlias alias fn(int) -> str             // alias fn-типа (тоже валиден)
```

**Call-through (единственный newtype-род, форвардящий операцию над
underlying).** Значение newtype-над-fn-типом вызывается НАПРЯМУЮ:

```nova
fn call_it(h Handler, req ServerRequest) -> ServerResponse => h(req)
```

Обоснование — асимметрия с `int`-newtype (`UserId`), который НЕ форвардит
`+`: вызов — ЕДИНСТВЕННАЯ осмысленная операция над значением функционального
типа (в отличие от арифметики, у которой для домена `UserId` попросту нет
единственно верной трактовки). Прецедент — Go `type HandlerFunc func(...)`,
который одновременно и вызывается (`h(w, r)`), и несёт методы. Newtype
как имя ОСТАЁТСЯ (методы, внятная диагностика «ожидался `Handler`», типы не
путаются) — call-through не отменяет nominal-типизацию, просто добавляет
ОДНУ разрешённую сквозную операцию.

**Alias fn-типа — прозрачность включает вызов.** До этого амендмента
`type X alias fn(A) -> B` объявлялся без ошибок, но значение типа `X` не
вызывалось (`[E_CALL_NOT_CALLABLE]`) — резолв вызова смотрел на ИМЯ типа
локала, не разворачивая alias. D52 alias уже требует полной прозрачности
(`X` и `Y` совместимы) — вызываемость теперь часть этой прозрачности,
разворачивается ТРАНЗИТИВНО через любую цепочку alias'ов.

**Авто-коэрсия `fn → Handler`.** См. [D55 §Obvious single-wrapper
coercion](#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)
— fn-типы добавлены в структурную «таблицу родов», используемую тем же
механизмом, что и `int`/`str`/`[]u8` (никакого нового кода
coercion-стороны не потребовалось — механизм уже был обобщён на
произвольный `Newtype(inner)`, не только скалярный `inner`).
**Однонаправленно**: `Handler → fn` НЕ коэрсится (nominal-типизация не
растворяется — два РАЗНЫХ newtype над ОДНИМ fn-типом взаимно
неподставимы, см. регресс `d52_newtype_fn_reverse_coerce_neg.nv`).

**Известный, задокументированный остаток (НЕ в объёме амендмента):** два
overload'а, различающихся ТОЛЬКО newtype-fn-параметром с ОДИНАКОВОЙ
underlying-сигнатурой, и вызов с голой fn-функцией — checker НЕ детектирует
двусмысленность на Nova-уровне (тот же класс, что и pre-existing
`[M-172.1-free-fn-multi-overload-ambiguous]`); компиляция падает ЧЕСТНО, но
на C-уровне (мангл-коллизия, оба erased-параметра — `void*`) — см. регресс
`d55_fn_newtype_ambiguous_lift_neg.nv` (`EXPECT_CC_ERROR`, НЕ
`EXPECT_COMPILE_ERROR`). Отдельно найден, ОРТОГОНАЛЬНЫЙ newtype-у,
pre-existing пробел: `assignable_direct` для fn-типизированного `expected`
коллапсирует категорию в `Any` (структурная проверка fn-сигнатур на
call-arg позиции отсутствует ВООБЩЕ, для любого fn-значения, newtype или
голого) — заведено как `[M-fn-type-expected-any-bypass]` в
`docs/plans/backlog-followups.md`, вне объёма этого окна.

**Реализация:** `compiler-codegen/src/parser/mod.rs` (`parse_type_decl`
empty-sum lookahead), `compiler-codegen/src/types/mod.rs`
(`BoundCtx::fn_type_names` pre-scan + `check_call_callee_not_local_shadow`),
`compiler-codegen/src/codegen/emit_c.rs` (`fn_newtype_sigs` pre-scan +
`resolve_fn_typeref` — call-dispatch через существующий `NovaClosBase`/
`NOVA_CLOS_CALL_*` механизм, ноль нового codegen). Регресс:
`spec_tests/conformance/d52_newtype_fn_type.nv` (4 куска) +
`spec_tests/conformance/neg/d52_newtype_fn_reverse_coerce_neg.nv` +
`spec_tests/conformance/neg/d55_fn_newtype_ambiguous_lift_neg.nv`.

---

## D406. Sum-type синтаксис: `enum` маркер (2026-07-01)

> Revises [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-) §«Sum».
> Остальные формы D52 (newtype, alias, record, tuple, unit) без изменений.

### Что

Sum-type теперь объявляется с обязательным ключевым словом `enum` вместо
leading `|`. Keyword `enum` — маркер в **грамматике типов**, поэтому он
работает везде где допустим тип: в named-type declaration, в позиции
параметра, возврата, поля, binding'а.

### Правило

#### Синтаксис

```nova
// Named sum inline — без | перед первым вариантом
type Color enum Red | Green | Blue
type Direction enum North | East | South | West
type Option[T] enum Some(T) | None

// Многострочный — | обязателен у каждого варианта (включая первый)
type Result[T, E] enum
    | Ok(T)
    | Err(E)

type Shape enum
    | Circle { radius f64 }
    | Square { side f64 }
    | Triangle { a f64, b f64, c f64 }

// С discriminants
type ExitCode enum Ok = 0 | Failure = 1 | Critical = 2

// С явным базовым типом (пока не реализован, Plan 105)
type Bit u8 enum Off = 0 | On = 1
```

#### Inline enum в type-позиции

`enum Variant1 | Variant2` — это **тип-выражение**, валидное в любой
позиции где допустим тип:

```nova
// Параметр функции
fn job(a enum A | B) { ... }

// Возвратный тип
fn parse() -> enum Ok(int) | Err(str) { ... }

// Поле записи
type Response {
    status enum Ok | NotFound | InternalError
}

// Let-binding
ro x: enum Some(int) | None = Some(42)
```

`type Foo enum A | B` — это объявление **имени** для типа-выражения
`enum A | B`. Named и inline — одна грамматика.

Минимум один вариант.

#### `|` в inline и многострочной формах

- **Inline** — `|` разделяет варианты, перед первым не нужен:
  `type Color enum Red | Green | Blue`
- **Многострочный** — если варианты на новых строках, `|` **обязателен**
  у каждого варианта (включая первый), аналогично type-set bounds:

```nova
type Color enum Red | Green | Blue       // inline — без | перед первым

type Result[T, E] enum                   // многострочный — | обязателен
    | Ok(T)
    | Err(E)
```

#### Парсер

| После `type X` (или `type X[params]`) | Форма |
|---|---|
| `enum` | sum |
| `<base-type>` `enum` | sum с явным базовым типом (Plan 105) |

В type-expression position `enum` — prefix, парсер строит `EnumTypeExpr`.

### Почему

1. **Симметрия с `alias`.** `type X alias Y` и `type X enum A | B` —
   одна структура: keyword даёт форму, далее описание. Единый паттерн.
2. **Явный grep-маркер.** `enum` в любой type-позиции мгновенно
   идентифицирует sum-type — в IDE, grep, LLM-prompt.
3. **Устраняет неоднозначность `|`.** В D52 leading `|` конфликтовало
   с оператором `@or` и вызывало удивление. `enum` — незвуковой маркер
   без операторных коннотаций.
4. **Inline enum.** Анонимные sum-типы в позициях параметра/возврата/поля
   становятся возможными — естественный extension грамматики.
5. **Named = `type` + inline.** `type Foo enum A | B` тавтологично
   читается: «тип Foo это enum A или B». Интуитивно.

### Что отвергнуто

- **Сохранить leading `|`** (D52). Конфликт с оператором; не grep-абелен;
  inline-позиция невозможна.
- **`enum { A, B }` со скобками** (Go/C стиль). Нарушает Nova-правило
  «`{` → record»; `,` как разделитель заменено на `|` ещё в D52.
- **`sum A | B`** (другой keyword). `enum` общепринятый термин в PL;
  `sum` слишком математичен, непривычен программистам.
- **`type X = enum A | B`** с `=`. Убрано ещё в D52; D406 следует тому
  же принципу «никаких `=` в type-declaration».

### Связь

- [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-) — заменяемый синтаксис
- [D53](#d53-унификация-protocol-под-type-protocol-как-kind-токен) — `protocol`/`effect` kind-токены в той же системе
- [D310](#d310-type-set-bounds-plan-1723) — `set` kind-токен в той же системе; `|` в type-set тоже разделитель, не оператор
- [D55](#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы) — literal coercion в позиции sum-type (inline `enum` тоже)
- [03-syntax.md → D46](03-syntax.md#d46) — `|` как `@or` оператор — разрешается по контексту (keyword `enum`/`set` или expr-контекст)
- [Plan 105](../../docs/plans/105-sum-type-explicit-base.md) — явный базовый тип discriminants

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
protocol Hash {
    hash() -> u64
    eq(other Self) -> bool
}

// Теперь (D53): kind-токен в системе D52
type Hash protocol {
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
| `effect` | effect-тип |
| `enum` | sum (D406) |
| `set` | type-set bound (D310) |
| `value` `{` | value-record — stack-allocated (D228/D277/D290) |
| `(` | tuple |
| `{` | record — heap-allocated |
| `alias` | alias |
| `<base-type>` `enum` | sum с явным базовым типом (Plan 105) |
| идентификатор/тип, конец строки | newtype |
| конец строки сразу | unit |

`protocol`, `effect`, `enum`, `set`, `alias`, `value` — kind-токены
после имени. Парсер однозначен по первому токену после имени (или
generic-параметров).

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

type Display protocol {
    show() -> str
}

fn User @show() -> str => "User(${@name})"

fn log_one(x Display) Log -> () =>
    Log.info(x.show())

log_one(my_user)                // ok, User совместим со Display
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
2. **На одно слово длиннее.** `type Hash protocol { ... }` против
   `protocol Hash { ... }` — лишний `type ` (5 символов).
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
type Hash protocol {
    hash() -> u64                    // ✅ голое имя
    eq(other Self) -> bool
}

type Hash protocol {
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

### D53 amend (2026-07-20, `[M-checker-protocol-typed-arg-any-bypass]`, worktree `nova-protoany`) — PLAIN protocol-typed параметр требует СТРУКТУРНОГО соответствия

**Status:** ACTIVE. Язык-меняющее по строгости (то, что раньше молча проходило,
теперь — compile error).

#### Что было

Строка 940 этой же секции (`log_one(my_user) // ok, User совместим со Display`)
документирует PLAIN (не-generic-bound) protocol-typed параметр как «type value /
existential» — заявленный, легальный способ использовать `protocol` (D53 сам называет
его прямо, наравне с generic-bound-формой). Но checker-реализация (`resolved_cat_of_depth`,
`compiler-codegen/src/types/mod.rs`) мапила **любой** `TypeDeclKind::Protocol`
expected-тип в `ResolvedType::Any` — permissive-коллапс, унаследованный от старого
`cat_of` («protocol/effect/opaque permissive»), появившийся ДО структурной
protocol-машинерии (D53/D72/D142) и никогда не пересмотренный. Из-за этого
`assignable_direct` пропускал **любой** аргумент в PLAIN protocol-typed позицию БЕЗ
проверки, что аргумент реально реализует протокол — структурная типизация (§«Структурная
совместимость») энфорсилась только для generic-bound-формы (`[T Protocol]`, отдельный
механизм — `BoundCtx::check_satisfaction`, D72/D142), никогда для голого `x Protocol`.

Симптом-прецедент: `Fmt = protocol { use Write, @width()..., ... }` (D422),
`StringBuilder` реализует только `Write` (embedded под-протокол), НЕ полный `Fmt` — но
`.debug(sb)`/`.display(sb)` с `sb: StringBuilder` компилировался БЕЗ ошибки (вместо
корректного «нужен `FmtCtx.bare(sb, ...)`», см. `d374_write_sink_decouple.nv`). На C
`Nova_StringBuilder*` шёл туда, где ожидался `Nova_FmtCtx*` (оба — указатель на offset 0)
→ тихая type confusion (пустая строка) вместо чистой compile-error.

#### Что теперь

`assignable_direct` СТРУКТУРНО проверяет PLAIN protocol-typed позицию — той же
проверкой «имя+арность метода присутствует (прямо, через `use`-embed (D145)
рекурсивно, или через `default_body`-фолбэк, D183)», что generic-bound-форма уже
применяет. Несоответствие → чистая compile error (переиспользует существующие коды
`[E7301]`/`[E_NO_MATCHING_OVERLOAD]` — новый код ошибки не заводился). Соответствующий
аргумент по-прежнему проходит — правило `log_one(my_user)` (строка 940) не изменилось,
изменилось только то, что НЕсоответствующий аргумент больше не проходит молча.

Не затронуто (намеренно, риск-контейнмент): `resolved_cat_of_depth` сам НЕ менялся
(Any-коллапс для protocol остаётся — используется другими consumer'ами); generic-bound
путь (`BoundCtx`) не менялся — уже был корректен; protocol-литералы
(expression-position, `protocol Name { ops }`, D142) не проверяются этим правилом —
их completeness уже проверяется на месте конструкции (missing-method там уже была
compile error, `neg_protocol_lit_missing_method.nv`).

**Гейт:** `spec_tests/conformance` standalone single-CU 508 PASS / 0 FAIL / 14 SKIP
(baseline 504/0/14 + 4 новых `neg/neg_protocol_param_*` фикстуры). Реализация —
`protocol_mismatch_found`/`protocol_required_missing`/`protocol_missing_methods`
(`compiler-codegen/src/types/mod.rs`, рядом с `assignable_direct`). Подробности —
[backlog-followups.md](../../docs/plans/backlog-followups.md) →
`[M-checker-protocol-typed-arg-any-bypass]`.

---

## D55. Literal coercion в позиции с явным типом: sum-конструкторы и record-литералы

### Что
В позиции, где компилятор **явно знает целевой тип** `T` (let с
аннотацией, аргумент функции, return-выражение), литерал
автоматически подгоняется под `T`. Четыре случая:

1. **Sum-coercion.** Значение типа `S` оборачивается в **единственный**
   unary-конструктор `C(S)` sum-типа `T`.
2. **Record-coercion.** Анонимный record-литерал `{ field: value, ... }`
   получает тип `T` без необходимости писать имя типа перед `{}`.
3. **Map-coercion.** Анонимный record-литерал `{ name: value, ... }` в
   позиции, ожидающей str-keyed map (тип помечен атрибутом `#from_fields`,
   как `HashMap[str, V]`), превращается в map: имена полей становятся
   **строковыми ключами**. Это **не** record-coercion (поля литерала ≠
   поля struct'а `HashMap`) — отдельное правило, см. ниже.
4. **Numeric literal coercion.** Целочисленный литерал в позиции
   numeric-типа (`u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64`,
   `int`) принимается без явного `as`-cast если значение **влезает в
   диапазон** целевого типа. Это распространяется на аргументы функций
   (включая generic после конкретизации: `Vec[u8].push(1)`), let с
   аннотацией (`ro a u8 = 42`), поля record-литерала.
   Выход за диапазон — **compile error `E_LIT_OUT_OF_RANGE`**:
   `ro a u8 = 300` → `300 > u8.MAX (255)`,
   `ro b u8 = -1` → `-1 < u8.MIN (0)`.

Без runtime-cost, без subtyping. После coercion тип значения — сам `T`.

```nova
// Sum-coercion
type StrOrInt enum S(str) | I(int)

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
| Generic-параметр после конкретизации — numeric (`Vec[u8].push(1)`) | да | ✅ numeric literals |
| Generic-параметр после конкретизации — record/sum | да | ⛔ ещё нет |
| Match-arm result (когда тип ветки фиксирован) | да | ⛔ ещё нет |
| Литерал коллекции с явным типом (`[]T`) — **record**-элементы | да для каждого элемента | ⛔ ещё нет |
| Литерал коллекции с явным типом (`[]T`) — **sum/newtype**-элементы (obvious single-wrapper, см. ниже) | да для каждого элемента | ✅ (Plan 200, 2026-07-12) |
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
> bootstrap'а.** Элемент-позиция литерала коллекции (`[]T`) для
> **record**-coercion помечена ⛔ — coercion анонимного `{...}` на
> элементах массива пока не работает. Пример станет валиден после
> расширения Ф.3a на element-positions (за scope Plan 52). Пока там нужен
> `[User{...}, ...]` с явным именем типа на каждом элементе.
>
> **sum/newtype element-position — реализовано (Plan 200, 2026-07-12),
> см. «Obvious single-wrapper coercion» ниже.** `ro args []SqlValue = [1,
> "alice", true]` работает без `SqlValue.I(1)`-обёрток — это отдельный,
> более узкий, механизм от record element-coercion выше (годится ровно
> для one-level sum-variant/newtype wrap, не для произвольных record-
> литералов).

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
type SqlValue enum I(i64) | F(f64) | S(str) | B(bool) | Bytes([]u8) | Null

ro args []SqlValue = [42, "alice", true]    // [I(42), S("alice"), B(true)]

// В sql`...` тэге интерполяции тоже coerce'ятся: i64 → I, str → S, bool → B
ro q = sql`SELECT * FROM users WHERE id = ${42}`   // args = [I(42)]

// D48 tagged-template интерполяция — та же coercion, включая ПЕРЕМЕННЫЕ
// (не только литералы синтаксически):
ro n = 1
ro id = 7
ro q2 = sql`UPDATE t SET name = ${"alice"} WHERE id = ${id} LIMIT ${n}`
// args = [S("alice"), I(7), I(1)] — без единой ручной обёртки
```

##### Obvious single-wrapper coercion (D55 amend, Plan 200, 2026-07-12)

> **АМЕНДМЕНТ 2026-08-21 (решение владельца) — NEWTYPE-ПОЛОВИНА СУЖЕНА ДО
> НЕТИПИЗИРОВАННЫХ КОНСТАНТ. Это ОТМЕНА части решения 2026-07-12, а не его
> уточнение.**
>
> Правило ниже читается с одной поправкой: пункт 1 (**newtype**) применяется
> ТОЛЬКО к нетипизированной константе — литералу, унарной операции над
> константой и арифметике констант (`40 + 60`). Типизированная переменная и
> любое выражение над ней требуют явной формы: `W(expr)` или `expr as W`.
> Пункт 2 (**sum**) действует БЕЗ ИЗМЕНЕНИЙ, включая переменные.
>
> **Почему половины разошлись.** В сумму коэрсия ВЫВОДИТ тег: подходящий
> унарный вариант ровно один, и автор написал бы ровно то же самое
> (`SqlValue.I(1)`). В newtype коэрсия ПРИДУМЫВАЕТ утверждение — «это число
> есть строка реестра», — а требовать именно этого утверждения newtype и
> заводят. Обёртка, которая надевается сама, обёрткой не является.
>
> **Граница взята у Go**, на который [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-)
> ссылается прямо («привычнее программистам с фоном Go»): неявно
> конвертируется нетипизированная константа, типизированная переменная — нет.
> Форма была заимствована в 2026-07-12, а граница сдвинута; замер 2026-08-21
> показал цену: `ro i int = 7; take_row(i)` и `take_row(i + 1)` принимались
> молча, то есть в доменную обёртку втекало любое целое.
>
> **Цена перехода измерена, а не оценена** (2026-08-21): скалярных newtype
> (`type X int`-форма) — НОЛЬ в `std/src` и НОЛЬ во всех пакетных репах
> (`nova-http`, `nova-tls`, `nova-polaris`, `nova-compress`, `nova-bignum`,
> `nova-socks`, `nova-comments`); в `examples` две декларации (`orm_demo`:
> `UserId`, `PostId`), и обе уже пишут `id as UserId` явно. Единственным
> носителем прежнего поведения был сам корпус — фикстура
> `spec_tests/conformance/d55_literal_coercion.nv`, перевёрнутая тем же
> слиянием, плюс новая негативная
> `spec_tests/conformance/neg/d55_newtype_var_coercion_neg.nv`.
>
> **Выход для автора, которому нужна прежняя мягкость, — [D429](#d429-coerce--декларативные-неявные-zero-cost-конверсии-view--finalize-plan-214-2026-07-18)
> `#coerce`**: пара `Y → W` объявляется в `.nv`, как объявляется `str → ro
> []u8`. Правило R11 (запрет дубля с встроенной обёрткой) с этим амендментом
> для newtype больше не срабатывает — иначе правило ужесточили бы, а дверь
> оставили запертой; для суммы R11 действует по-прежнему.
>
> **Честно о доктрине:** вводная D429 называет «single-wrapper вкл.
> переменные» частью доктрины «безопасных, очевидных, zero-cost конверсий».
> Этот амендмент ту доктрину для newtype-половины пересматривает: она
> написана для доменных обёрток в пользовательском коде и не рассматривала
> случай, ради которого newtype и понадобился в компиляторе, — РАЗВЕДЕНИЕ
> ПРОСТРАНСТВ ИНДЕКСОВ, где втекающий голый `int` и есть тот самый дефект.

Уточнение и расширение sum-coercion, закрывающее компилятор-gap (не
только «литералы», но и переменные/произвольные простые выражения; не
только sum, но и newtype — см. открытый вопрос в [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-)):

Значение выражения `expr` в позиции с явным ожидаемым типом `W`
авто-оборачивается, если `W` — **ровно одна** из двух форм «обёртки над
значением», и КАНДИДАТ **однозначен**:

1. **Newtype** (`type W Y`, [D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-)) — `expr` того же
   структурного «рода» (см. ниже), что и `Y` → `expr as W` (компилятор
   вставляет `as`-cast автоматически; `newtype` — та же C-repr, что и
   `Y`, поэтому это zero-cost, не runtime wrap).
2. **Sum** с **ровно одним** unary-вариантом `C(Y)`, чей `Y` совпадает
   по «роду» с типом `expr` → `W.C(expr)` (как ручная форма
   `SqlValue.I(1)`, которую авторы писали раньше вручную).

Совпадение по «роду» (не полная type-identity — иначе int-переменная
никогда не совпала бы с `i64`-payload'ом варианта) — это int-семья
(`int`/`i8..i64`/`u8..u64`, друг с другом взаимозаменяемы, ширина
доводится тем же `as`-cast'ом что и в п.1 — «прячет» `int`→`i64`
widening невидимо для автора), `f32`/`f64`, `bool`, `str`,
`[]u8`/`Vec[u8]`, именованный тип по имени. **Не** путать с
`cat_compatible_rt`'s permissive int↔float assignability — здесь int и
float — РАЗНЫЕ «роды», иначе `${1}` был бы неоднозначен между
`I(i64)`/`F(f64)` и ничего бы не обернулось.

**Амендмент (ОКНО-5, 2026-07-23): fn-типы в таблице родов.** Род `Fn`
добавлен для структурного совпадения `fn(A, …) -> R` с `fn(A, …) -> R`
(параметры и возврат совпадают структурно) — тот же механизм, что и для
`int`/`str`, БЕЗ отдельного нового кода: `single_wrap_candidates` уже был
обобщён на произвольный `Newtype(inner)` (не только скалярный `inner`), а
`WrapKind::Other`-fallback для `TypeRef::Func` УЖЕ различал fn-значения от
не-fn (единственный практический сценарий этого амендмента —
`type Handler fn(A) -> B` с ОДНИМ таким newtype в скоупе — уже корректно
разрешался существующим кодом; расширение зафиксировано here как явное
намерение, а не побочный эффект). Пример:

```nova
type Handler fn(ServerRequest) -> ServerResponse

fn make_response(req ServerRequest) -> ServerResponse => { body: "hi" }
fn accept(h Handler) -> str => "ok"

ro r = accept(make_response)   // fn -> Handler, авто-подъём
```

Как и для `int`/`str` — **однонаправленно**: `Handler → fn` (или
`Handler → Middleware`, другой newtype над ТЕМ ЖЕ fn-типом) НЕ коэрсится —
единственный существующий механизм (`single_wrap_candidates`) ключуется
ТОЛЬКО по `expected`-стороне и всегда ОБОРАЧИВАЕТ голое значение в
именованную обёртку, никогда не разворачивает названную обёртку обратно
(нет и не было обратной ветки). Регресс:
`spec_tests/conformance/neg/d52_newtype_fn_reverse_coerce_neg.nv`. См.
[D52 §Амендмент (ОКНО-5)](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-)
для call-through/alias-прозрачности этого же newtype-рода.

**Однозначность обязательна**: если ⩾2 кандидата совпадают по «роду»
(гипотетический sum с двумя int-payload'ами) или 0 совпадают —
auto-wrap НЕ срабатывает, экспрешн остаётся как есть (существующая
ошибка/поведение).

**Ровно один уровень** — кандидат-обёртка НЕ разворачивается рекурсивно:
`int → UserId → Wrapper` (обёртка над обёрткой) остаётся **отвергнутой**
(нужен явный промежуточный cast/конструктор). Аналогично — значение
**без** явной целевой типизации (`ro x = "test"` — тип `str`, никакой
обёртки) coercion не касается: правило работает только там, где
target-тип **явно известен** (см. таблицу позиций выше — `let`/`const`,
`return`, call-arg на РЕЗОЛВЛЕННОГО callee, element-позиция
`[]T`/`Vec[T]`).

Явный конструктор (`SqlValue.I(1)`, `n as UserId`) остаётся валиден без
изменений — значение УЖЕ целевого типа, `expr`-обёртка не запускается
(матчится только «голый» литерал/`Ident`, не произвольный `Call`/`As`).

```nova
type UserId int                              // newtype

ro id UserId = 100                            // 100 as UserId (было: E7301)
ro n = 100
ro id2 UserId = n                             // n as UserId — ПЕРЕМЕННАЯ, не только литерал
```

**Реализация:** `assignable`/`single_wrap_candidates`/`wrap_kind_of` в
`compiler-codegen/src/types/mod.rs` (accept-сторона, единое правило);
материализация (AST-rewrite `${1}` → `SqlValue.I(1 as i64)`) — в уже
существующем mutable-проходе `annotate_map_literals`/
`MapLitAnnotator::try_wrap_leaf` (тот же expected-type-propagated walker,
что и map-литерал coercion — без отдельного нового прохода).

**Генерики (амендмент, реестр 221.1 №320, окно p320, 2026-08-04):**
[M-generic-sumlift-mono-missing-variant-wrap] закрыт для варианта, чей
payload — ИМЕНОВАННЫЙ тип, зависящий от generic-параметров `expected`
(`type Node[K, V] enum Empty | Leaf(Wrap[K, V])`, `Wrap[K, V]` — своя
`value`-декларация): `w Wrap[K, V]` в `ro`-return-позиции с ожидаемым
`Node[K, V]` авто-оборачивается в `Node.Leaf(w)` — было ICE (`return w;` без
обёртки → C-типовая несовместимость на границе функции) до Plan p320.
До фикса `single_wrap_candidates` безусловно бейлилась на ЛЮБОЙ generic
`expected` (гейт `!generics.is_empty()`); теперь Sum-плечо этот гейт не
проверяет — `WrapKind`-дизамбигуация уже была name-only для `Named`-рода, так
что собственные generics `expected` ей были не нужны. Материализация ТАКЖЕ
сменила форму для generic-цели: голый `Leaf(w)` (как `Some(x)`/`Ok(x)` для
`Option`/`Result`), а не квалифицированный `Node.Leaf(w)` (как для
не-generic `SqlValue.I(1)`) — `emit_c.rs`'s `try_emit_explicit_variant_ctor`
намеренно отклоняет квалифицированный вызов на generic-сумме (её ctor
дорога — mono-aware bare-`Ident`-путь).

**НЕ закрыто** (пример ниже — ИСТОРИЧЕСКИЙ, был добавлен как желаемое
поведение и НЕ РАБОТАЛ уже на момент этого амендмента, независимо от
Plan p320's фикса выше): payload = ГОЛЫЙ generic-параметр сáмого sum'а
(`type Wrapper[T] enum W(T) | Empty`, `W(T)` — `T` тут не именованный тип, а
буквальное имя типового параметра). `wrap_kind_of` для такого payload'а даёт
`WrapKind::Named("T")` (T не резолвится ни в один известный тип), что
никогда не совпадёт с `WrapKind::IntFamily`/`Str`/etc. литерала/переменной
без подстановки `T` → конкретный тип из `expected.generics` — эта
подстановка НЕ реализована ни до, ни после Plan p320. `ro w Wrapper[int] =
42` сегодня даёт `[E7301] cannot assign value of type int to w declared as
Wrapper[int]`. Закрытие этого случая — отдельная, более инвазивная работа
(генерик-aware `WrapKind`-подстановка в `single_wrap_candidates`/
`wrap_kind_of`), не начатая.

##### Str-литерал → `[]u8` coercion (D55 amend, Plan 208 Ф.0, 2026-07-15)

> **RETRACTED → [D429](#d429-coerce--декларативные-неявные-zero-cost-конверсии-view--finalize-plan-214-2026-07-18) (2026-07-18, владелец):**
> отдельного литерал-правила не остаётся — литерал есть частный случай str-значения, общее
> D429-правило (`#coerce` на `str @bytes()`) покрывает литералы автоматически. Текст ниже —
> история; name-gated реализация (`synthesize_write_str_lit_bytes_coercion`) действует
> переходно до Plan 214 Ф.2, затем сносится.

Общее правило, того же семейства «литерал в типизированной позиции», что numeric-literal
coercion (case 4 выше) и obvious single-wrapper coercion (newtype/sum) — carve-out на
**литералах** (Nova в остальном без implicit-coercion, D9):
В позиции с явным ожидаемым типом `[]u8` **str-выражение любой формы** —
литерал, переменная, результат вызова — коэрсится в `[]u8` **напрямую**, без
обёртки и без `.bytes()`:

> **ПОПРАВКА 2026-08-09 (реестр 221.1 №492).** Прежняя редакция ограничивала
> коэрсию литералом: «str-литерал (не переменная, не str-выражение)».
> Реализация этого ограничения НЕ имеет — проверено прямой сборкой: `take("hi")`
> и `ro s = "hi"; take(s)` дают `PASS: 1 FAIL: 0` одинаково. Расхождение стоило
> `std` лишнего метода: `TcpStream.@write_str(s str)`, состоявший из
> `Net.write(@, s.bytes())`, был написан как обход ограничения, которого нет, и
> удалён вместе с этой поправкой (13 носителей переведены на `.write(...)`).
> Спека приведена к реализации, а не наоборот: коэрсия для переменных удобна,
> безопасна (str уже держит байты, копии не возникает) и уже работает —
> запрещать её значило бы ломать рабочий код ради текста.
str-выражение) коэрсится в `[]u8` **напрямую**, без обёртки/вызова:

```nova
fn Write mut @write(bytes []u8) -> ()
w.write("Point(")            // ✅ str-литерал → []u8 коэрсия (D55, zero-copy — str УЖЕ UTF-8-байты)
w.write("Point(" as []u8)    // эквивалент, явная форма тоже валидна

ro s = "Point("
w.write(s)                   // ❌ E_TYPE_MISMATCH — s НЕ литерал (переменная), coercion не применяется
w.write(s.bytes())           // ✅ явный .bytes() (D176) — ro []u8 zero-copy view переменной
```

**Позиции** — та же таблица «явно ожидаемого типа» выше (case-таблица): call-arg на
резолвленный `[]u8`-параметр, `let`/`const` с явной `[]u8`-аннотацией, return-позиция с
`[]u8`-типом, element-позиция коллекции `[][]u8`. **Не** ограничено `@write` — это ОБЩЕЕ
правило для ЛЮБОЙ `[]u8`-позиции (не отдельный `@write`-overload, который был отвергнут —
см. [D422](#d422-unified-formatter--единый-displaymut-f-fmt--debug-байтовый-write-zero-alloc-pad-plan-208-2026-07-15)
§1: `Write` протокол остаётся чисто `@write([]u8)`, без str-перегрузки; литеральная коэрсия —
компиляторный приём НА ВЫЗОВЕ, не второй метод в протоколе).

**Почему литерал, не любое str-значение:** str-переменная требует runtime-решения — тот же
буфер `{ptr,len}` (D139), но семантика владения/времени жизни неочевидна без явного жеста;
литерал же — compile-time известные байты, коэрсия эквивалентна тому, как `100` коэрсится в
`u8`/`UserId` (case 4 / single-wrapper выше) — компилятор видит целевой тип И буквальное
значение одновременно, вставляет `as`-cast без раздумий автора. str-значение → всегда явный
`.bytes()` ([D176](#d176-ro-t--тип-модификатор)) — `ro []u8` zero-copy view, тот же приём,
никакой копии, просто явный жест вместо неявного carve-out.

**Статус реализации (Plan 208 Ф.3, 2026-07-16 — сузилось после эмпирической проверки):**
формулировка выше — целевая/аспирационная модель (написана в Ф.0 ДО реализации). Реально
реализовано **ýже**: только call-arg на метод, названный буквально `write`, с ОДНИМ позиционным
str-литеральным аргументом, на приёмнике, УЖЕ имеющем зарегистрированный метод `write`
(`compiler-codegen/src/codegen/emit_c.rs::synthesize_write_str_lit_bytes_coercion`, вызывается
из `emit_call`'s pre-pass, тот же паттерн, что `synthesize_inout_refargs`/
`synthesize_record_lit_typed_call_args`) — АСТ-переписывание `w.write("Ok(")` →
`w.write("Ok(".bytes())` ДО остальной emit_call-логики (переиспользует существующий рабочий
`.bytes()`-путь, ноль нового C-форматирования). Что из аспирационного текста НЕ покрыто:
- **`let`/`const` с явной `[]u8`-аннотацией, return-позиция, element-позиция `[][]u8`** —
  НЕ реализовано (только call-arg к методу `write`);
- **любой ДРУГОЙ метод/функция с `[]u8`-параметром** (не названный `write`) — НЕ покрыто;
- **приёмник БЕЗ уже зарегистрированного `write`** — не триггерит (осознанный safety-gate,
  не баг: без него совпадение имени метода на неродственном типе рискует ложно коэрсить).

Эмпирически найдено при проверке (важное расхождение с текстом примера выше): чекер
**АСИММЕТРИЧЕН** по форме приёмника — `w.write("...")` на PROTOCOL-типизированном приёмнике
(`w Fmt`/`Write`, как в примере выше) раньше СИЛЕНТНО проходил type-check (permissive
`overload_applicability`, ready to skip category-check for protocol-erased receivers) и падал
только на CC-FAIL — это и была реальная дыра, которую чинит этот codegen-фикс. На КОНКРЕТНОМ
типе (`sb StringBuilder` напрямую, не через `Fmt`) чекер и ДО, и ПОСЛЕ этого фикса отвергает
str-литерал БЕЗ `.bytes()` диагностикой `[E_NO_MATCHING_OVERLOAD]` (не `E_TYPE_MISMATCH`, как
писал пример выше — исправлено ниже) — **этот путь не проходит эту codegen-коэрсию вообще**
(диагностика срабатывает раньше, в чекере, до emit_c.rs). str-ПЕРЕМЕННАЯ (не литерал) в
`Fmt.write(...)` — НЕ коэрсится (правильно — гейт по `ExprKind::StrLit` пропускает `Ident`), но
диагностика деградирует до голого CC-FAIL (`passing 'nova_str' to parameter of incompatible
type 'Nova_Vec____nova_byte *'`), не чистый `E_`-код — известный, не устранённый этой волной
пробел (чекер для protocol-приёмников вообще не гонит `overload_applicability`-проверку;
исправление диагностики для этого случая — отдельная задача, не блокирует литеральный кейс).
Пример кода (строка `w.write(s) // ❌ E_TYPE_MISMATCH`) читать как **иллюстрацию намерения**, не
буквальный текст ошибки — актуальный код на конкретном приёмнике: `E_NO_MATCHING_OVERLOAD`; на
protocol-приёмнике — CC-FAIL (пробел, не диагностика).

Обобщение до полного «ЛЮБАЯ `[]u8`-позиция» (let/return/collection-element/произвольный метод)
— осознанно НЕ сделано в этой волне (бюджет сессии); зафиксировано как follow-up.

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
type Shape enum Circle { radius f64 } | Square { side f64 }

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
**`#from_fields` атрибут** (TypeAttr, ставится перед `type`):

- Это **не** opt-in ради эргономики (которое D55 отвергает для
  sum/record) — marker здесь **load-bearing для дисамбигуации**:
  «трактовать `{...}` как поля этого struct'а» vs «как строковые
  ключи». Без него правило неоднозначно.
- Gating: `HashMap[str, V]` несёт marker; случайный struct — нет, и
  не начнёт принимать произвольные record-литералы.
- ~~Bootstrap: marker захардкожен для `HashMap`. Протокол `FromFields[V]`
  как точка расширения (`OrderedMap`, `BTreeMap[str, V]`) — позже.~~
  **АМЕНДМЕНТ 2026-08-30 (реестр №842): «позже» УЖЕ НАСТУПИЛО, и абзац
  описывал состояние до собственного расширения.** Оракул читает АТРИБУТ, а
  не имя типа: `compiler-codegen/src/ast/mod.rs:889` определяет `#from_fields`
  как маркер str-keyed map-типа ВООБЩЕ, а `:2529` прямо обрабатывает случай
  «`#from_fields` type OTHER than `HashMap`». Значит любой пользовательский
  тип, объявивший `#from_fields`/`#from_pairs` и давший протокольные
  `static with_capacity` и `mut @insert_new` (см. пункт ниже), получает ту же
  коерсию — точка расширения открыта, отдельный `FromFields[V]` не понадобился.
  Абзац оставлен зачёркнутым, а не удалён: он объясняет, почему в коде рядом
  живут упоминания «bootstrap».

  **Почему это важно читателю, а не только историку:** пока строка стояла как
  действующая, она говорила «свой map-тип написать нельзя, ждите». Нашло окно
  274 при подготовке волны коерсий — и нашло КОДОМ, а не аналогией, отдельно
  оговорив, что предыдущую свою находку (про `HashMap` без D-блока) оно само
  же отозвало как ложную аналогию.
- **Протокол `#from_fields`:** тип обязан иметь:
  - `static with_capacity(n int) -> Self` — предаллоцировать под `n` записей;
  - `mut @insert_new(key str, val V) -> ()` — вставить новую запись
    (без возврата `Option`; дублей нет по construction).
  Отсутствие любого из методов → CC-FAIL при десугаринге.

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
   `with_capacity` + `@insert_new`, никакой промежуточный record не
   материализуется (литерал — только синтаксис):
   ```nova
   { mut _m0 = HashMap[str, V].with_capacity(n)
     _m0.insert_new("debug", true)
     _m0.insert_new("verbose", false)
     _m0 }
   ```
   (`@insert_new` вместо `@insert` — мапа только что создана, дублей нет,
   нет нужды в `Option[V]`-возврате.)
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
type Ambiguous enum A(int) | B(int)

ro x Ambiguous = 42         // ОШИБКА: ambiguous, A(42) или B(42)?
ro x = A(42)                 // явный конструктор — ok
```

**Несоответствие — ни один конструктор не принимает тип значения:**

```nova
type Color enum Red | Green | Blue

ro c Color = "red"           // ОШИБКА: ни один конструктор не принимает str
ro c = Red                    // unit-конструктор
```

**Без аннотации — coercion отключён:**

```nova
type StrOrInt enum S(str) | I(int)

ro a = "test"                // a : str (не StrOrInt, аннотации нет)
ro b StrOrInt = "test"        // b : StrOrInt = S("test") (аннотация есть)

ro r = { id: 2, name: "Bob" }   // r : анонимный record { id int, name str }
ro u User = { id: 2, name: "Bob" }   // u : User (через record-coercion)
```

**Newtype через D52 — coercion следует типу значения, не возможным кастам:**

```nova
type UserId u64
type Wrapper enum W(UserId) | N(int)

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
type Event enum Click(int, int) | KeyPress(str)

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
type State enum Open | Closed
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
type Mixed enum A(int) | B(int)
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
type Hash protocol {        // D52/D53: kind-токен `protocol` под `type`
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

type Display protocol {
    show() -> str
}

fn User @show() -> str => "User(${@name})"

fn log_one(x Display) Log -> () =>
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

- **Bounds на дженерики** — `HashMap[K: Hash, V]` требует отдельного
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
type Display protocol {
    show() -> str
}

type User { id u64, name str }

fn User @show() -> str => "User(${@name})"

fn log_one(x Display) Log -> () => Log.info(x.show())

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

## D443. `use` — hard keyword → контекстный keyword (Plan 239, 2026-08-01)

### Что

`use` retracted из hard keyword (`TokenKind::KwUse`, безусловный, как `let`/
`const`) в **контекстный** keyword — тем же механизмом, что `bench`/`measure`
(D121) и `apply`/`null` (D278 §3): лексер выдаёт `Ident("use")` всегда, парсер
распознаёт `use` позиционно в трёх местах, где у него уже была синтаксическая
роль:

1. **import-synonym** — `use path.to.mod` (alias `import`, с bootstrap).
2. **record-field embed** ([D39](#d39-embed-и-delegation-use-name-type-alias-обязателен)) —
   `use alias Type` внутри `type { ... }`.
3. **protocol embed** (D145 §Protocol composition) — `use TypeName` в начале
   `protocol { ... }` тела.

Вне этих трёх позиций `use` — обычный идентификатор: имя поля, переменной,
функции, параметра, namespace-сегмента.

### Правило

Disambiguation — lookahead на 1-2 токена вперёд от `Ident("use")`, зеркало
`bench`-техники (D121 «parses как item только когда за `bench` идёт
string-literal»):

- **Top-level item position**: `Ident("use")` + следующий токен `Ident`/`.`/
  `..` (похоже на начало dotted-path или relative-anchor) ⇒ import-synonym.
  Иначе `use` не участвует в top-level item dispatch (падает в generic
  «expected fn/type/…» путь, как любой другой стрей-идентификатор на этой
  позиции).
- **Record-field list** (внутри `type { ... }`): `Ident("use")` + `Ident`
  (alias или `_`) + токен ПОСЛЕ него НЕ разделитель полей (`,`/newline/`;`/
  `}`) ⇒ embed (`use alias Type`/`use _ Type`). Иначе — обычное поле с именем
  `use` (`use SomeType` — 1 идентификатор перед типом, как у любого другого
  поля). Единственный известный remaining edge-case: поле `use` с generic-
  типом (`use Vec[T]`) синтаксически неотличимо от `use Vec [T]`-подобного
  embed по этому 2-токенному lookahead — задокументировано как компромисс
  (Plan 239 §2), не заблокировано отдельным механизмом (тот же класс trade-off,
  что у `bench`/`apply`).
- **Protocol body leading items**: `Ident("use")` + `Ident` (имя типа) ⇒
  embed. `use(` (метод по имени `use`, bare-ident effect-syntax) НЕ embed —
  проваливается в обычный method-parse путь. Раньше (hard keyword) метод с
  именем `use` был НЕВОЗМОЖЕН даже bare-ident'ом в effect-теле — теперь
  разрешено.

### Почему

Будучи hard keyword, `use` был **единственным** из этой группы контекстных
слов, полностью недоступным как идентификатор — при том что у `bench`/
`measure`/`apply`/`null` (D121, D278 §3) ровно такая многопозиционная
семантика уже жила как `Ident` с parser-side disambiguation. Асимметрия не
имела отдельного обоснования: все три существующие роли `use` синтаксически
однозначно определяются позицией + 1-2 токенами lookahead, без коллизий с
уже написанным кодом ( grep `std`/`examples` на `\buse\b`: 100% вхождений —
`use TypeName`-embeds, комментарии/строки; ни одного идентификатора `use` не
найдено — retraction расширяет допустимые программы, не сужает).

### Что отвергнуто

- **Оставить hard keyword** — статус-кво до этого плана; отклонено по
  просьбе владельца (симметрия с `bench`).
- **Диагностика `E_RESERVED_WORD` при misuse** (запасной вариант из брифа
  на случай «резерв без формы невозможен») — не понадобилась: у `use`, в
  отличие от гипотетического «зарезервировать под будущее», уже есть три
  РЕАЛЬНЫЕ синтаксические формы; misuse просто не парсится как `use`-форма и
  проваливается в generic diagnostic соответствующей позиции (тот же
  silent-fallthrough, что у `bench`).
- **Подсветка `use` в редакторах наравне с `bench`** (буквальная формулировка
  исходного брифа: «добавить use рядом с bench» в tmLanguage) — ПРОТИВОПОЛОЖНО
  верному направлению. D278 §3 нормативно требует ОБРАТНОГО: контекстные
  keyword'ы (`apply`/`bench`/`measure`/`null`, теперь `+use`) **намеренно НЕ**
  подсвечиваются — лексер держит их идентификаторами во избежание поломки
  пользовательских имён, и живой conformance-тест
  (`compiler-codegen/tests/syntax_highlight_conformance.rs`,
  `vscode_grammar_has_no_phantom_keywords` и парные vim/zed) фейлится, если
  контекстное слово остаётся в keyword-паттерне хайлайтера. `use` **снят** из
  `editors/vscode/syntaxes/nova.tmLanguage.json`, `editors/vim/syntax/nova.vim`,
  `editors/zed/languages/nova/highlights.scm` этим планом — брифовая
  формулировка была основана на неверной посылке «`use` пока не
  зарезервирован» (на деле был hard keyword).

### Связь

- [D39](#d39-embed-и-delegation-use-name-type-alias-обязателен) — record-field
  embed; семантика embed НЕ меняется, меняется только классификация токена.
- D145 §Protocol composition — protocol embed; аналогично, без изменений
  семантики.
- [D121](09-tooling.md#d121-benchmark-dsl-bench---measure----) — прецедент
  техники (`bench`/`measure` contextual lexing).
- [D278](09-tooling.md#d278-editor-syntax-highlighting-keyword-set-must-track-the-lexer) —
  §3 governance-правило «контекстные keyword'ы не подсвечиваются»; `use`
  добавлен в перечень.
- [Q-embed-syntax](../open-questions.md#q-embed-syntax-embed-keyword--use-vs-альтернативы) —
  вопрос выбора keyword'а (`use` vs `embed`) остаётся ОТКРЫТ; эта retraction
  снимает один аргумент «за» пересмотр (`use` больше не занимает identifier-
  пространство целиком), но не разрешает многопозиционную перегрузку
  семантики самого слова.
- [Plan 239](../../docs/plans/239-use-contextual-keyword.md) — план окна,
  полный список затронутых файлов.

---

## D32. Семантика передачи параметров

> Status: revised для полей. [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindingа-ro-для-never-mut)
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
> ⚠️ **D32 align (Plan 138.5; amend Plan 147 D246)** — мутируемость в указательном
> типе — это **L3 pointee** (target), из ТИПА позиционно-независимо: `*mut T` =
> writable target, `*T` = ro-pointee (`*T ≡ *ro T` УНИВЕРСАЛЬНО; pointee-mut НЕ
> наследуется от binding — flip-scan-draft отклонён). Перепривязываемость самого
> указателя — это **L1 binding** (`ro`/`mut`), как у любого параметра/переменной
> по D32/D36, **НЕ часть типа** и НЕ влияет на pointee. Prefix-модификатор перед
> `*` (`mut * T`) запрещён
> ([D216 §1](#d216-typed-pointer-family--unsafe-model--null-safety-через-npo)
> `E_POINTER_PREFIX_MODIFIER`). Семантика передачи параметров (этот D32) не
> меняется: `mut p *T` = mutable binding указателя (p reassignable), доступ к
> target — по pointee-модификатору типа.
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

**Явная таксономия value vs reference типов (D215 amend, Plan 120;
Receiver mut-ABI column added Plan 128 Ф.5, 2026-06-05):**

| Категория | Примеры | Размещение | Передача | Receiver mut-ABI (`fn T mut @...`) |
|---|---|---|---|---|
| Примитивы | `int`, `bool`, `f64`, `char`, `u8`, `()` | register/stack | by value (копия) | **forbidden** — `E_PRIMITIVE_MUT_METHOD` (Plan 128 Ф.3) |
| Tuples (positional или named) | `type X(T1, T2)`, `type Vec3(x f64, ...)` | **stack** | by value (копия) | `NovaTuple_<X>*` pointer (Plan 128 Ф.2) — `&v`/hoist+`&temp` call-site |
| Value records | `type X value { ... }` | **stack** | by value (копия) | `NovaValue_<X>*` pointer (D228) — `&v`/hoist+`&temp` call-site |
| Records | `type X { ... }` | **managed heap** | by reference (указатель) | `Nova_<X>*` pointer (unchanged — already by-reference) |
| Sum types | `type X \| A \| B` | managed heap | by reference | `Nova_<X>*` pointer |
| Arrays | `[]T` | managed heap (handle inline) | by reference | `Nova_<X>*` pointer |
| **`str`** (Plan 139) | `str` | **stack** (16-байт value `{ptr,len}`; буфер на heap/rodata) | **by value (копия)** | `nova_str` value — handle-copy |

> **`str` reclassified (Plan 139, 2026-06-11):** ранее `str` стоял в одной
> строке с `[]T` как «managed heap / by reference». Теперь `str` — **value
> type, несущий heap-backed буфер**: само значение — 16-байт stack-value
> `type str value priv { ptr *u8, len int }` с copy-семантикой (как
> примитив/tuple/value-record), а UTF-8 байты живут в heap (RawMem,
> GC-tracked) либо rodata (литералы). Поэтому передача str — by-value
> копия 16-байт handle'а (НЕ pointer-to-heap-object), а buffer разделяется
> immutably через `*u8` (ro-pointee, ≡ `*ro u8`; D246). См. [D26 MAJOR AMEND](08-runtime.md#d26-базовая-stdlib-и-prelude)
> + [D228](#d228) «str — канонический reference-field value-record».

Bracket choice **явно кодирует** size/lifetime semantics: `()` =
stack, `{}` = heap. Tuple value types (D123): zero GC pressure,
predictable lifetime — ideal для hot-path math types, FFI returns,
iterator state.

**Receiver mut-ABI rationale (Plan 128):** value categories (tuples,
value-records) — by-value normally, **но** `mut @` receiver требует
pointer чтобы мутации были видны caller'у. Reference categories
(records, sum-types, arrays, strings) уже passed by pointer — no extra
ABI flip. Primitives **никогда** mut-method (Nova-first idiom: int.add
returns new value, не mutates self) — Plan 128 Ф.3 `E_PRIMITIVE_MUT_METHOD`
diagnostic enforces. Threading: `MethodCallInfo::recv.mutable` flag
консолидирует решение, propagated через `emit_c.rs::prepare_method_recv`
(Plan 128 Ф.1).

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
[D36](#d36-поля-типа-дефолт-mutable-у-mut-bindingа-ro-для-never-mut)):

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
   разные. Согласовано с [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindingа-ro-для-never-mut)
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
- [02-types.md → D36](#d36-поля-типа-дефолт-mutable-у-mut-bindingа-ro-для-never-mut)
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
`ro` для never-mut, `mut` per-field — только для cache/lazy.
Семантика параметров не менялась.

---

## D36. Поля типа: дефолт mutable у `mut` binding'а, `ro` для never-mut

> Amended Plan 114 D184 (2026-05-31): `readonly` → `ro` keyword rename
> в полях. Sample обновлён. Error code `E_READONLY_FIELD` сохранён как
> stable API. Семантика per-field freeze не меняется.

### Что
Поле без префикса мутируется, **если binding mutable**. `ro`
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

// Структура с invariant'ами — ro для read-only полей
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
| `ro field T` | **никогда** | **никогда** | id, immutable invariants |
| `mut field T` | **да** | **да** | cache, lazy init, atomic counters |

### Почему

1. **Меньше шума для типичного случая.** Аккумулятор с 18 mutable
   полями писать без префиксов — все поля «обычные», никаких
   акцентов. Раньше 18 раз `mut` — визуальный мусор.
2. **Сигнатура показывает только важное.** Префикс ставится **только
   на исключения** (`ro` для invariants, `mut` для cache).
   LLM, читая тип, видит: `ro id` — «не трогай», обычное поле
   — «можно мутировать с mut-binding'ом».
3. **Прецедент Rust/Go/C++** — поля без префикса мутируются у
   mut-binding'а; `ro` для never-mut близко к C++ `const`
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
  `final class`, `final var`). `ro` прямо говорит «только для
  чтения».
- **`let` для never-mut полей.** Короче (3 символа), прецедент Swift,
  но `let` уже значит «binding имени со значением»
  ([03-syntax.md → D33](03-syntax.md#d33)). На поле без `=`
  необычно, не самообъясняемо. `ro` прямо говорит цель.
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
  binding; на поле — аналогия в роли `ro`.
- [03-syntax.md → D35](03-syntax.md#d35) — `fn Type mut @method`
  даёт mutable-binding self, поля затем по правилам D36.

### Эволюция
До D36 поле помечалось `mut field T`, мутируемое только у
`mut`-binding'а (D32). Для аккумуляторов это требовало 18 раз
повторить `mut` — шум без пользы. D36 инвертировал дефолт: «обычное
поле — мутируется у mut-binding'а», `ro` — для исключений.
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

| Контекст | Default = ro? | Opt-in mut |
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

### Указатели: перепривязываемость = binding (D216 cross-ref, Plan 138.5)

Перепривязываемость **указательной переменной** (можно ли `p = other_ptr`)
регулируется её **binding'ом** (`ro` = фиксирован, `mut` = reassignable) —
ровно как у любой другой переменной по правилам D36 выше. Это **НЕ часть
типа указателя**: указательный тип несёт только мутируемость **pointee**
(target) постфиксом (`*mut T` / `*ro T`). Никакого type-level «outer
pointer-mut» нет.

```nova
ro p *mut int = &x         // p фиксирован (binding ro); target writable (pointee mut)
mut q *ro int = &y         // q reassignable (binding mut); target read-only
q = &z                     // ✓ q — mut binding
p = &w                     // ✗ existing E_REBIND — p ro binding
```

Две роли чисто разделены: ведущий `ro`/`mut` **перед именем** = binding;
`*mut`/`*ro` **после `*`** = pointee. Prefix-модификатор перед `*`
(`mut * T`) запрещён ([D216 §1](#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) `E_POINTER_PREFIX_MODIFIER`).

### Связь
- [02-types.md → D175](#d175-ro-field--полный-freeze-амендмент-d36) — ro field полный freeze.
- [02-types.md → D176](#d176-ro-t--тип-модификатор) — ro T modifier + Plan 108.1 param default flip.
- [02-types.md → D216 §1/§V2.6](#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — указатель: pointee-mut в типе (postfix), reassignability = binding (Plan 138.5).
- [03-syntax.md → D33](03-syntax.md#d33) — `let` это immutable binding.

---

## D175. `ro field` — полный freeze (амендмент D36)

> 📌 **Полная модель мутабельности — [D246](#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee)** (3 оси: L1 binding / L2 content-view / L3 pointee). Этот D-блок описывает только **L2: freeze поля через `ro field`**. Для общей картины, дефолтов и error-кодов читай D246.

> ⚠️ **См. D216 V3 §V3.1** (Plan 118.5 V3, 2026-06-04/05) для storage-class-aware rules о `ro` + `mut` adjacency: type-form `ro mut T` запрещён на value-T (E_MUTABILITY_CONFLICT_VALUE_TYPE), binding-form `ro x mut T` allowed regardless of T storage class (Ф.6 relaxation).

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
| `mut field ro T` | ✅ всегда | ❌ никогда | swappable ro view |

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
- [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindingа-ro-для-never-mut) — расширяется
- [D176](#d176-ro-t--тип-модификатор) — `ro` как тип-позиция
- [D184](03-syntax.md#d184) — keyword refresh (readonly → ro rename)

---

## D176. `ro T` — тип-модификатор

> 📌 **Полная модель мутабельности — [D246](#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee)** (3 оси: L1 binding / L2 content-view / L3 pointee). Этот D-блок описывает только **L2: `ro T` как type-modifier + параметры ro по умолчанию**. Для общей картины, дефолтов и error-кодов читай D246.

> ⚠️ **См. D216 V3 §V3.1** (Plan 118.5 V3, 2026-06-04/05) для storage-class-aware rules о `ro` + `mut` adjacency: type-form `ro mut T` запрещён на value-T (E_MUTABILITY_CONFLICT_VALUE_TYPE), binding-form `ro x mut T` allowed regardless of T storage class (Ф.6 relaxation).

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

**Особый случай: pointer-returns (D216 amend, Plan 138.5; D246).** Pointee-mut
возвращаемого **указателя** — это **L3, из ТИПА** (`*T`=ro / `*mut T`=mut), а НЕ
return-mut-default: дефолтная mut-семантика возврата относится только к **L1
binding** результата у caller'а (reassign), не к pointee. У указательного типа
нет «outer pointer-mut» (retracted, [D216 §V2.6](#d216-v2-amend-2026-06-04--universal-right-binding-rule-для-type-level-modifiers--unsafe-t-first-class)).
`-> *ro T` — `E_REDUNDANT_POINTER_RO` (используй `-> *T`).

| Return type | Pointee (L3, из типа) | L1 binding результата |
|---|---|---|
| `-> *T` (≡ `-> *ro T`) | read-only | bind-site (`ro p`/`mut p`) |
| `-> *mut T` | writable | bind-site (`ro p`/`mut p`) |
| `-> *uninit T` | possibly-uninit (FFI) | bind-site (`ro p`/`mut p`) |

```nova
fn alloc_cell() -> *mut int                 // writable target
fn peek_head(buf []u8) -> *u8               // *u8 = ro target (D246; *ro u8 → E_REDUNDANT_POINTER_RO)
ro p *mut int = alloc_cell()                // binding ro: p фиксирован; target writable
mut q *u8 = peek_head(buf)                  // binding mut: q reassignable; target ro
```

Реассайнабельность результата (`p = other_ptr`) задаётся **bind-site**
(`ro`/`mut`, [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindingа-ro-для-never-mut)),
не возвращаемым типом. Это устраняет прежнюю двусмысленность «двух mut» в
return-позиции (pointee-mut в типе vs reassignability указателя).

> **Ревизия (Plan 184, 2026-07-06).** Наследование мутируемости ниже — исходная D326-R7/R8
> формулировка через оракул/decay. По ревизии D326 (Plan 184, Р7) `-> @` имеет конкретный
> тип: `-> ref Self` у стекового (value) типа, `-> Self` у кучевого — см.
> [Ревизия D326 (Plan 184)](#ревизия-d326-plan-184-ref-t--ограниченный-тип). Таблица ниже
> сохранена как исходная семантическая модель (поведение цепочек эквивалентно).

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
- Любой явный return type (`-> T`, `-> []u8`, `-> ro T`, `-> mut T`,
  `-> consume T`, `-> *mut T`, `-> *ro T`) — берётся как написан (для
  указателей модификатор всегда относится к pointee — postfix; prefix перед
  `*` = `E_POINTER_PREFIX_MODIFIER`, [D216 §1](#d216-typed-pointer-family--unsafe-model--null-safety-через-npo)).
  **`-> consume T` — префиксная форма, симметричная `-> ro T`/`-> mut T`**

> ⚠️ **ПЕРЕКРЫТО [D445](02-types.md#d445) §616** (пометка 2026-08-17 по аудиту самосогласованности). Форма `-> consume T` в позиции ВОЗВРАТА снята ЦЕЛИКОМ — «не переопределена, а удалена»: `E_RETURN_CONSUME_PREFIX_RETRACTED` для префиксной и `E_RETURN_CONSUME_POSTFIX_RETRACTED` для постфиксной (№301). Пять носителей в `std/src/runtime/sync.nv` переписаны на голый `-> MutexGuard` тем же слиянием. Класс держится стражем `check-retracted-param-form` (храповик осадка по зонам). Текст выше сохранён как история.
  ([D445](#d445),
  №301/221.1, окно p-lang, 2026-08-04); старая постфиксная `-> T consume`
  (Plan 103.9) РЕТРАКТИРОВАНА.
- `-> Self` (статический Self-тип, D182) — owned-by-caller; не наследует
  receiver-мут.
- `-> @` без receiver-method context (free fn) → **`E_AT_RETURN_OUTSIDE_METHOD`**.

### Escape hatch

Снять `ro` в Nova-коде нельзя. Кому нужен mutable доступ —
явно копирует: `let copy []u8 = view.to_owned()`. Если необходим
обход через FFI, это делается в `external fn` на C-стороне.

### Рантайм

Zero overhead — `ro` только compile-time проверка, не влияет
на codegen. ABI `ro []u8` = `NovaArray_uint8_t*` (идентично `[]u8`).

### Применение

`str.as_bytes() -> ro []u8` — zero-copy view в UTF-8 буфер строки
без memcpy. UTF-8 invariant защищён: записать в буфер нельзя.

### Параметры функций (Plan 108.1)

**Default = read-only.** Параметр без явного модификатора эквивалентен
`ro param T` — callee может только читать, не вызывать `mut`-методы,
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
| `param T` | ro (default) |
| `mut param T` | mutable view |
| `ro param T` | ro (явно) — synonym default |
| `consume param T` | owned move, mut by default |
| `mut consume param T` | ✗ parser-level `E_PARAM_MOD_CONFLICT` |
| `consume mut param T` | ✗ parser-level `E_PARAM_MOD_CONFLICT` |
| `mut ro param T` | ✗ parser-level `E_PARAM_MOD_CONFLICT` |
| `ro mut param T` | ✗ parser-level `E_PARAM_MOD_CONFLICT` |

**Coercion (передача аргумента в параметр).**

После Plan 108.1 `T` в позиции параметра **уже ro по умолчанию**.
Поэтому `ro T → T (param)` — это `ro → ro` (тождество),
а единственное реальное нарушение это `ro → mut`:

| caller-type → callee-param-type | OK? |
|---|---|
| `T → T` (param default ro) | ✓ (caller-T → callee-ro = сужение) |
| `T → ro T` (param explicit ro) | ✓ (synonym default) |
| `T → mut T` (param explicit mut) | ✓ (caller разрешает mut доступ) |
| `ro T → T` (param default ro) | ✓ — оба ro |
| `ro T → ro T` | ✓ |
| `ro T → mut T` (param explicit mut) | ✗ `E_READONLY_COERCE` — единственное нарушение |
| `mut T → T` (param default ro) | ✓ (сужение, mutable можно показать как ro) |
| `mut T → mut T` | ✓ |

**Closure-параметры** — аналогично функциональным.

### Закрытые маркеры (Plan 108.1)

- ✅ `[M-108-readonly-mut-method-check]` — вызов `mut`-метода на
  параметре без `mut` теперь даёт `E_PARAM_NOT_MUT`.
- ✅ `[M-108-readonly-coerce-on-param]` — closed **дефакто**:
  старая формулировка маркера предполагала, что param `T` mutable;
  после Plan 108.1 param `T` уже ro, поэтому coerce `ro T →
  T (param)` — это `ro → ro` (no violation).  Единственный
  остаточный case — `ro T → mut T (param explicit)` — отдельный
  followup `[M-108.1-readonly-to-explicit-mut-coerce]` (узкий нишевый
  сценарий, не блокирует).

### Связь
- [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindingа-ro-для-never-mut) — `ro field` предшественник
- [D175](#d175-ro-field--полный-freeze-амендмент-d36) — ro field enforcement
- [D144](#d144-sub-slice-views-для-t-и-str--arra-b--sa-b) — слайсы `arr[a..b]`
- [D157](#d157) — view-borrow для consume-типов (Plan 108.1 распространяет принцип на не-consume)
- Plan 108 — реализация D175/D176
- Plan 108.1 — params ro by default + закрытие 2 markers

---

## D445. Модификатор ВСЕГДА перед тем, что описывает (`ro`/`mut`/`consume`) {#d445}

> **Статус:** ✅ IMPLEMENTED. Решение владельца, 2026-08-03 (реестр 221.1
> №301); реализовано окном p-lang, 2026-08-04.
>
> **AMEND (2026-08-12, owner decision, реестр 221.1 №616/№611/№615, окно
> p616-mode-modifiers).** Три формы, у которых была ЗАПИСЬ, но не было
> ПРАВИЛА за ней, — сняты без периода deprecated:
>
> 1. **`-> consume T` (позиция возврата) снят ЦЕЛИКОМ** — не переопределён,
>    а ОТМЕНЁН. Пробой (`docs/plans/wip/repro-616-consume-return.nv.txt`)
>    доказано: модификатор не добавляет семантики. Тип, объявленный
>    `consume`, держит линейность САМ ПО СЕБЕ — `ro g = make_guard()` для
>    `fn make_guard() -> Guard` (без `-> consume` на функции) уже даёт
>    `E_CONSUME_KEYWORD_MISSING` (D180). А тип, НЕ объявленный `consume`,
>    возвращённый как `-> consume Box`, компилируется и пропускает
>    `ro b = make_box()` БЕЗ потребления — модификатор в этом случае
>    ОБЕЩАЕТ линейность, которой у типа нет. Ни в одном из двух случаев
>    модификатор не делает ничего полезного, во втором — вредит (ложное
>    обещание). Обе строки таблицы ниже (`-> consume T` и пример-код) —
>    удалены; return-позиция для `consume` больше не существует НИ В КАКОЙ
>    форме — `E_RETURN_CONSUME_PREFIX_RETRACTED` (новая, эта форма) рядом с
>    уже существовавшей `E_RETURN_CONSUME_POSTFIX_RETRACTED` (постфикс,
>    №301). Пять носителей в `std/src/runtime/sync.nv` (`Mutex@lock`,
>    `RwLock@read`/`@write`, `Once@make_guard`, `Semaphore@acquire`) —
>    переписаны на голый `-> MutexGuard` и родню в том же слиянии.
> 2. **Постфикс `имя mut Тип` в параметре — исключение отменяется по
>    имени.** Амендмент **2026-07-17** к [D374](#d374) (`AMEND ×3`, канон
>    mut-параметров) объявил голую постфиксную форму
>    ЗАПРЕЩЁННОЙ лint'ом (`W_PARAM_TYPE_POS_MUT`), но оставил ЖИВОЕ
>    исключение «легитимна ИСКЛЮЧИТЕЛЬНО за view-слайсами (`[]u8`) и
>    fixed-size массивами (`[N]u8`)» — именно то исключение, которое этот
>    D-блок отменял ПО СМЫСЛУ («без исключений и без постфиксной формы где
>    бы то ни было»), но не назвал по имени, из-за чего исключение и
>    дожило почти на девять дней. Теперь названо и закрыто: исключение для
>    view-слайсов/fixed-массивов **снято**, `W_PARAM_TYPE_POS_MUT` (лайт,
>    лишь предупреждал) заменён на жёсткую ошибку
>    `E_PARAM_TYPE_POS_MUT_RETRACTED`. Канон параметра — РОВНО три формы:
>    `buf Type` / `mut buf Type` / `consume buf Type`.
> 3. **R2-split `ro имя mut Тип` в параметре — санкционированность снята.**
>    Тот же амендмент 2026-07-17 называл `ro name mut Type` «санкционированным,
>    не каноническим, исключением», которое «самодокументирует „пишу в
>    содержимое, не подменяю биндинг“» (ссылки на [D246](#d246-3-axis-l1l2l3-универсальная-модель-мутабельности-plan-147)
>    P6, Plan 118.5 V3 amend). Проба показала: запись через такой параметр
>    даёт `E_READONLY_CONTENT` — форма, придуманная для записи в содержимое,
>    саму запись и запрещает. Санкционированность снята вместе с постфиксом
>    из пункта 2 (та же синтаксическая форма, просто с явным `ro` спереди);
>    `spec_tests/conformance/d246_param_ro_mut_view.nv` (позитивный
>    дискриминатор, доказывавший легальность формы) удалён — его роль
>    теперь играет негативная фикстура
>    `spec_tests/conformance/neg/param_type_pos_mut_r2_split_retracted_neg.nv`.
>    **Не путать** с R2-split ЛОКАЛЬНОГО биндинга (`ro r mut Point` —
>    ЛОКАЛЬНАЯ переменная, не параметр, [D246](#d246) §«R1 vs R2», `ro a
>    mut T` в таблице мутабельности) — тот механизм НЕ затронут, живёт
>    своей жизнью на другой оси (L1×L2 локального биндинга, а не позиция
>    параметра).
>
> Новый D-блок не заводится — амендмент правит существующий D445.

### Что

Единое правило размещения для всех трёх модификаторов режима (`ro`/`mut`/
`consume`) во ВСЕХ позициях грамматики: **модификатор всегда стоит перед
тем, что он описывает**, без исключений и без постфиксной формы где бы то
ни было.

| Позиция | Форма | Модификатор описывает |
|---|---|---|
| Имя привязки | `ro v` / `mut v` / `consume v` | саму привязку |
| Ресивер | `fn T @m()` / `fn T mut @m()` / `fn T consume @m()` | ресивер `@` |
| Тип возвращаемого значения | `-> ro T` / `-> mut T` | возвращаемое значение (`consume` в этой позиции СНЯТ ЦЕЛИКОМ — AMEND 2026-08-12 выше, №616) |
| Тело типа | `type X { … }` / `type X value { … }` / **`type X consume { … }`** / `type X enum A \| B` | сам тип (тот же слот, что `value`/`enum`) |

```nova
// Возврат: ro/mut — по-прежнему ПЕРЕД типом. `consume` в этой позиции
// СНЯТ (AMEND 2026-08-12, №616) — линейность несёт объявление типа
// (`type MutexGuard consume { … }`, D180), не return-аннотация.
fn Mutex @lock() -> MutexGuard
fn RwLock @read() -> ReadGuard
fn RwLock @write() -> WriteGuard
```

### Зачем

**До 2026-08-03 была асимметрия.** `ro`/`mut` в позиции возврата писались
ПЕРЕД типом (`-> ro Self`, `-> ro []u8` — так уже было в std), а `consume`
(Plan 103.9) — ПОСЛЕ (`-> MutexGuard consume`, 5 мест в
`std/src/runtime/sync.nv`: `Mutex@lock`, `RwLock@read`/`@write`,
`Once@make_guard`, `Semaphore@acquire`). В позиции возврата все три модификатора
отвечают на один и тот же вопрос («что можно делать с полученным
значением»), но стояли по разные стороны типа — единственный постфикс во
всей грамматике модификаторов.

**Постфикс ретрактирован, не переопределён.** Ранняя формулировка этого
правила («единственный постфикс — на объявлении типа», зафиксированная в
черновике решения владельца 2026-08-03) была **ошибочной** и в этот
D-блок не входит: `type X consume { … }` — это НЕ постфикс на имени типа
`X`, а модификатор ПЕРЕД телом `{ … }` — тем же слотом, который занимают
`value` (`type X value { … }`) и `enum` (`type X enum A | B`). Модификатор
здесь тоже стоит перед тем, что описывает (тело типа), просто «то, что
описывает» идёт не сразу за именем типа. Постфиксной позиции для
модификатора режима не существует нигде в грамматике.

### Как

**Текущее состояние (после AMEND 2026-08-12, №616).** Парсер: return-позиция
(`-> …`) видит `consume` СРАЗУ после `->` — `E_RETURN_CONSUME_PREFIX_RETRACTED`
(hard error, fix-it: убрать `consume`, `-> T`). `consume` СРАЗУ после типа
возврата (старый постфикс, №301) — `E_RETURN_CONSUME_POSTFIX_RETRACTED`, тот
же fix-it. Обе формы — parser/mod.rs, ветка после `->`, ДО и ПОСЛЕ
`parse_type()` соответственно. Параметр: постфикс `имя mut Тип` (голый И
`ro имя mut Тип`) — `E_PARAM_TYPE_POS_MUT_RETRACTED` (parser/mod.rs
`parse_param`, ветка после `parse_ident`), без исключений по типу (view-слайсы
и fixed-массивы включены). Лint `W_PARAM_TYPE_POS_MUT` (`lints.rs`) снят вместе
с формой, которую он предупреждал, а не запрещал.

### Связь
- [D176](#d176-ro-t--тип-модификатор) — `-> ro T`/`-> mut T` в позиции
  возврата (симметрия с `consume`, пока последний не сняли целиком, №616)
- [D133](#d133-type-x-consume--обязательная-consume-семантика-must-be-consumed) — `consume`-семантика типа/значения (несёт линейность и БЕЗ модификатора в return-позиции, №616)
- [D156](#d156-generic-t-consume-bound--collection-aware-iteration) — `[T consume]` generic bound (комбинация с протокольной цепочкой — см. правку раздела «Синтаксис bound» выше, №300/221.1)
- [10-overloading.md D84](10-overloading.md#d84) — режим `{ro,mut,consume}` как ось перегрузки (диспатч по форме привязки уточнён тем же окном — №309/221.1)
- [D374](#d374) AMEND ×3 (2026-07-17) — канон mut-параметров, чей carve-out
  для view-слайсов/fixed-массивов и R2-split этот амендмент отменяет (№611/№615)
- [D246](#d246) — три оси мутабельности; R2-split ЛОКАЛЬНОГО биндинга (не
  параметра) этим амендментом не затронут

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
type Hash protocol {
    hash() -> u64
    eq(other Self) -> bool       // Self = тот тип, что реализует
}

// effect — для transactional/recursive handler-операций
type Db effect {
    query(q Sql) -> []DbRow
    nested(body fn() Self -> ()) -> ()  // Self = Db
}

// sum-type method
type Tree enum Leaf | Node(int, Tree, Tree)
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
именно для type-safe equality (`Hash.eq(other Self)`). На
practice'е `Self` оказался полезен также в:
- static-методах для DRY возврата того же типа,
- instance-методах для builder pattern'а,
- effect-методах для self-referential операций (transactions),
- sum-вариантах для `@clone`/`@with_*` методов.

D66 убирает ограничение: `Self` валиден везде, где есть type-контекст.

### AMEND (Plan «self-nested-generic», 2026-06-15) — `Self` как вложенный generic type-arg

**Правило (расширение «generic-параметры наследуются», п.2 «Почему»).**
`Self` валиден не только как самостоятельный return/param-тип, но и как
**type-argument внутри другого Named generic** — в любой глубине вложения,
в return- и в param-позиции:

```nova
type MapIter[I, T, U] value { src I, f fn(T) -> U }

// Self как вложенный type-arg — receiver-mono наследуется в позицию I
fn MapIter[I, T, U] @zmap(g fn(U) -> V) -> MapIter[Self, U, V] => ...
//                                          ^^^^^^^^^^^^^^^^^^^^ Self ≡ MapIter[I,T,U]
fn MapIter[I, T, U] @zfilter(p fn(U) -> bool) -> FilterIter[Self, U] => ...

// и в param-позиции (симметрично)
fn FilterIter[I, T] @combine(other FilterIter[Self, T]) -> ... => ...
```

Семантически `MapIter[Self, U, V]` ≡ `MapIter[MapIter[I,T,U], U, V]` —
`Self` подставляется на тот **же mono-инстанс receiver'а**, что и в
return-`-> Self`. Это устраняет повтор имени receiver-типа в
adapter-on-adapter цепочках (zero-cost ленивые итераторы,
`std/collections/vec_iter_zc.nv`).

**Где было сломано (codegen, до фикса).** Call-site return-inference
биндил `Self` value-aware (без trailing-`*`), но эмиссия метода
(`register_mono_method_instance` fwd-decl + `emit_monomorphized_method`
body) строила `current_type_subst` **только** из receiver-generics — без
записи для `Self`. Вложенный `Self` промахивался мимо early-lookup
`current_type_subst["Self"]` и падал в общий `"Self"`-арм
`type_ref_to_c`, который даёт POINTER-форму (`Nova_X*`) → лишний trailing
`*` → `_p` в mangle → C-имя mono на call-site ≠ имя в fwd-decl/body →
forward-decl≠def. Это тот же класс рассинхрона, что
[D182](#d182-self-в-return-type-static-methods--required-form-для-parametric-types)
закрыл для голого `-> Self`.

**Фикс.** В обоих emit-местах после установки `current_receiver_type`
биндить `Self` → `value_aware_generic_c_type("Nova_{recv}*")` в
`current_type_subst` (через `.entry().or_insert()` — no-clobber), guard
`recv_type.contains("____")` (только mono-инстансы). `value_aware_*`
оставляет heap-generic / non-value формы без изменений, поэтому top-level
heap-generic `-> Self` (где `Self` — owned-by-caller heap-ref, не
value-record) **не затронут**. Подробности маркера —
[`docs/plans/backlog-followups.md` → `[M-138.2-self-in-param]`](../../docs/plans/backlog-followups.md).

**Известное ограничение (НЕ покрыто фиксом).** `Self`, равный
**single-param** generic-ресиверу (`VecIter[T]`), использованный как
type-arg внутри **multi-param** адаптера (`MapIter[Self, T, U]`),
по-прежнему мис-резолвит receiver (chain-ENTRY методы на `VecIter[T]`).
Фикс покрывает `Self`, где ресивер — тот же adapter-family, который
ре-вкладывается. См. ОСТАЁТСЯ в маркере.

---

## D72. Generic bounds через `[T Protocol]` — protocol как тип

### Что
Параметр-тип в generic-списке может иметь **bound** — protocol-тип,
которому должны удовлетворять конкретизации параметра. Синтаксис —
единое правило «name type» без двоеточия:

```nova
[T Hash]
[K Hash, V]
[K, T From[K]]
```

Без bound — `[T]` — параметр без ограничений (структурное соответствие
проверяется при использовании, как было до D72).

Bound — это **protocol-тип** (D53) **ИЛИ type-set** ([D310](#d310-type-set-bounds-plan-1723), Plan 172.3: именованное множество конкретных типов, `[T SignedInts]`). Тот же `Hash` стоит и в
позиции типа значения (`fn f(x Hash)` — existential), и в позиции
bound'а (`fn f[T Hash](x T)` — universal). Одна сущность —
тип со структурным контрактом — в трёх позициях:

1. Тип значения: `fn f(x Hash) -> u64`
2. Bound: `fn f[T Hash](x T) -> u64`
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

fn dedup[T Hash](xs []T) -> []T
//       ^^^^^^^^^^^ T должен реализовывать Hash

type HashMap[K Hash, V] {
//          ^^^^^^^^^^^ K — Hash, V — без bound
    ...
}

fn fold[T, Acc](xs Iter[T], init Acc, f fn(Acc, T) -> Acc) -> Acc
//      ^^^^^^ ни T, ни Acc bound'а не имеют
```

#### `fn[T] ReceiverType @method` префикс (Plan 101.1 partial, 2026-05-24)

Generic-параметры также декларируются через **`fn[T]` префикс** —
для receiver'ов без carrier-brackets (`[]T`, bare T, tuple). Параллель
[D145](#d145-fnt-префикс--receiver-generic-decl--bounds-plan-101).
Bound syntax из D72 применим в этой позиции — `fn[T Hash] []T @method`.

```nova
fn[T] []T @map[U](f fn(T) -> U) -> []U          // T через fn[T] (нет carrier)
fn[T Hash] []T @dedup() -> []T              // bound в fn[T] (D72 + Plan 101.2)
```

**Plan 101.1 status (2026-05-24):** parser + базовый codegen работают
для `[]int` element type. Codegen mono-per-T для других element-types
(`[]str`, `[]User`) — marker `[M-fn-prefix-int-only-mono]`
✅ RESOLVED (Plan 101 Group I, vec_map_int_str fix).

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

> ⚠️ **D72 §«Порядок объявления параметров» — ПРАВИЛО СНЯТО (решение владельца
> 2026-08-16; реестр №702).** Имена одного generic-списка видны друг другу
> ЦЕЛИКОМ: `fn f[T From[K], K](v K) -> T` — законно, как и `fn f[K, T From[K]]`.
> Прецедент — rustc (`fn f<T: From<K>, K>` законно). **Основание снятия — три
> факта, а не вкус:** (1) правило никогда не действовало — чекер порядок не
> проверял (`check_generic_bound_declarations`, `types/mod.rs:26054`, валидирует
> только имена бáундов); (2) std САМ пишет forward-order и компилируется
> (`std/src/testing/property.nv:224,304` — `[G Generator[T], T]`); (3) D145
> §«Bound syntax (через D72)» даёт эту форму как валидный пример. Собственное
> обоснование правила — «ради простоты type-checker'а» — истекло: чекер
> справляется без него. Снятие не меняет ни одной компилирующейся программы;
> реализация правила сломала бы std и D145. **Что НЕ затронуто:** D88
> (forward-ref в ДЕФОЛТАХ `[T = X]`) остаётся запретом — там порядок нужен для
> вычисления значения, здесь — нет. Пример-ошибка выше сохранён как ИСТОРИЯ и
> более не нормативен.

#### Bound — это protocol-тип

`Hash`, `From[T]`, `Into[T]` и т.д. — обычные protocol-типы (D53):

```nova
type Hash protocol {
    hash() -> u64
    eq(other Self) -> bool
}

// Bound в generic-объявлении:
fn map[K Hash, V](m HashMap[K, V]) -> ...

// Тот же Hash в позиции типа значения (existential):
fn dump_one(x Hash) -> u64 => x.hash()
```

**Existential vs universal — различение по позиции:**

| Форма | Семантика | Dispatch | Аналог Rust |
|---|---|---|---|
| `fn f(x Hash)` | existential («какое-то значение типа Hash») | dynamic (vtable) | `fn f(x: &dyn Hash)` |
| `fn f[T Hash](x T)` | universal («для любого T : Hash») | static (mono) | `fn f<T: Hash>(x: T)` |

В обоих случаях `Hash` — **тип**. Различие только в позиции:
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
type HashMap[K Hash, V] {
    ro buckets []Slot[K, V]
}

type Set[T Hash] {
    ro inner HashMap[T, ()]
}

type Sorted[T Ord] | Empty | Node(T, Sorted[T], Sorted[T])
```

Bound применяется при инстанциировании: `HashMap[User, int]` требует
чтобы `User` реализовывал `Hash`.

#### Проверка bound'а — структурная (D53)

Bound удовлетворён, если у конкретного типа есть **методы из
protocol'а** (структурно). Никаких явных `impl`/declaration не нужно:

```nova
type User { id u64 }

fn User @hash() -> u64 => @id
fn User @eq(other Self) -> bool => @id == other.id

// User автоматически удовлетворяет Hash, потому что есть @hash и @eq
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
   поле `id u64`, generic-параметр `T Hash` — единая грамматика.
   Двоеточие в Nova зарезервировано под key-value, использовать его
   для bound — нарушение D17.

3. **Protocol = тип (D53).** `Hash` уже тип в Nova. Использовать
   его как bound — естественное расширение, не новый механизм.
   Existential (`x Hash`) и universal (`[T Hash]`) различаются
   позицией.

4. **Прецедент Go.** Go 1.18+: `interface { M() }` используется и как
   тип значения, и как constraint в generics. Один синтаксис, два
   контекста, проверено в большом продакшне.

5. **Структурная проверка вместо impl.** Nova не имеет orphan rule
   (D42/D53) — нет `impl Trait for Type` блоков. Bound удовлетворяется
   автоматически, как и existential. Это последовательно.

6. **AI-friendly.** LLM пишет `[T Hash]` без специальных
   keyword'ов (`where`, `impl`, `:`). Грамматика читается как
   естественный язык: «параметр T типа Hash».

### Что отвергнуто

- **`[T: Hash]`** (Rust/Scala/Kotlin/Swift). Конфликтует с D17 —
  двоеточие в Nova только для key-value (record-литералы, dict).
  Делать исключение для generic-list — нарушение единства.
- **`[T is Hash]`.** `is` уже занят под runtime type-check (D54).
  Третий смысл (compile-time bound) перегружает keyword.
- **`where`-clauses после сигнатуры** (C# / Haskell-style). Многословно,
  раздваивает информацию между списком параметров и where-блоком.
  Bound у параметра — единое место.
- **`[T impl Hash]`** (Swift `some`-style). Нестандартно,
  `impl` не используется в Nova ни для чего ещё.
- **Bounds через контракты** (`requires implements(T, Hash)`).
  Контракты (D24) проверяются SMT на значениях, bound — type-checker'ом
  на типах. Разные уровни.
- **Sealed/closed bound'ы** («только эти типы»). Открытый вопрос,
  не входит в D72.

### Цена

1. **Type-checker сложнее.** Проверка structural-bound при
   мономорфизации — дополнительная работа.
2. **Сообщения об ошибках.** «`User` не реализует `Hash`: missing
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

- ~~**Множественные bounds**~~ — **ЗАКРЫТО** планом 101.3 (ред. 5,
  2026-05-25): форма композиции — `+`, `[T Hash + Equal]`. Реализовано и
  проверено пробой 2026-08-03. Кандидаты `&` и `(Hash, Eq)` из прежней
  редакции ОТКЛОНЕНЫ; запись оставалась в «открытых вопросах» по недосмотру
  (№299).
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

> ⚠️ **D72 AMENDED by Plan 108.4 (2026-06-09)** — When a type participates as
> bound `[T Iterable[U]]` or any protocol bound, the type-checker now verifies
> **receiver_mut consistency** for all protocol methods at the use-site. A type
> that declares `@method()` (ro) does not satisfy a protocol requiring
> `mut @method()`. Errors: `E_PROTO_IMPL_RO_FOR_MUT`, `E_PROTO_IMPL_MUT_FOR_RO`,
> `E_PROTO_IMPL_MUT_FOR_CONSUME`, `E_PROTO_IMPL_CONSUME_FOR_MUT`.
> See [D209](04-effects.md#d209--protocol-method--syntax--receiver-mutability-plan-1084-2026-06-09).

---

## D110. Ghost state — spec-only bindings

**Статус:** Принято (Plan 33.3 Ф.10, реализовано в AST и type-checker)

### Решение

`ghost let` / `ghost var` объявляют **spec-only переменные** — они видимы
в `requires`/`ensures`/`invariant` и других `ghost`-statements, но
**никогда не эмитируются в C-код** (ни в debug, ни в release).

```nova
fn fill(mut xs []int) -> ()
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
   `impl<T: Hash>`.

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
- ✅ Vtable runtime infrastructure готова (`NovaVtable_Hash`,
  `NovaVtable_Compare`, `NovaVtable_Display` + 4 primitive K
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
> [D354](#d354-generic-anonymous-tuple-monomorphization) для full spec.
>
> **Plan 148 Ф.4 amend (2026-06-12, `[M-codegen-unify-tuple-repr]`):**
> typed representation унифицирован, legacy all-int путь сжат до on-demand.
> Три изменения:
> 1. **Blanket pre-decl retired.** Раньше каждый C-файл получал
>    `typedef … _NovaTuple1; … _NovaTuple8;` (8 all-`nova_int` структур)
>    в преамбуле — вне зависимости от использования. Теперь legacy
>    `_NovaTupleN` эмитится **on demand** только для арностей, которые
>    erased-generic fallback реально запрашивает (на практике — только
>    arity 2, от erased `HashMap[K,V]`/`Set` `(K, V)` пар). Concrete
>    tuples всегда используют typed mono'd путь. Регистрируется через
>    `register_legacy_tuple(n)`, splice в `/*__LEGACY_TUPLE_TYPEDEFS__*/`.
> 2. **Self-describing field decode.** Field access (`t.0`, `t.0.1`) и
>    type inference больше не зависят исключительно от per-Ident side-table
>    `tuple_element_types` — элемент-тип декодируется напрямую из имени
>    mono'd структуры в `obj_ty` через `parse_mono_tuple_elements`. Это
>    чинит field-read на fn-параметрах, call-result кортежах и **вложенных**
>    `t.0.1` цепочках (раньше collapse'или в `nova_int` fallback → дроп
>    второго `.0` / неверный тип). Закрыло 5 pre-existing CC-FAIL в Plan 59
>    (f2/f10/f13/f15/f16).
> 3. **Arity diagnostic code.** Destructure-arity-mismatch diagnostic
>    (3 codegen-сайта: let / for-pattern / match-variant inner Tuple)
>    несёт код `[E_TUPLE_DESTRUCTURE_ARITY]` (раньше — bare message).

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
   Bootstrap-compat для truly generic contexts. **Plan 148 Ф.4:** этот
   typedef эмитится **on demand** (per requested arity, idempotent-guarded
   `#ifndef`) — не blanket `_NovaTuple1..8`. На практике достигается только
   arity 2 (erased `HashMap`/`Set` `(K, V)` пары).

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

- **Arity mismatch** (`[E_TUPLE_DESTRUCTURE_ARITY]`, code added Plan 148
  Ф.4) — destructure pattern имеющий разное число элементов чем actual
  tuple, reject'ится **Nova-level** clear error (file:line + hint) до
  C-emit'а. Покрывает 3 sites: let-destructure, for-pattern,
  match-variant inner Tuple. Раньше упирался в нечитаемый
  "no member named 'fN'" C error.

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

// Construction — ПОЗИЦИОННО (амендмент 2026-08-05: см. ниже «Конструирование
// и деструктуризация»; прежние примеры показывали именованные аргументы для
// полей БЕЗ дефолтов, что противоречит D102)
ro v = Vec3(1.0, 2.0, 3.0)
ro c = Color(255, 0, 128, 255)

// Field access — by name
v.x     // 1.0
v.y     // 2.0

// Methods — identical to records
fn Vec3 @add(other Vec3) -> Vec3 =>
    Vec3(@x + other.x, @y + other.y, @z + other.z)
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

### Method receiver passing (Plan 128 Ф.2, 2026-06-05)

Named tuple `@`-методы получают receiver через ABI-conditional форму:

| Receiver mode | Parameter type | Call-site form |
|---|---|---|
| `fn NamedTuple @method(...)` (ro receiver) | `NovaTuple_<Name>` (by value, copy) | `f(v)` — copy semantics |
| `fn NamedTuple mut @method(...)` (mut receiver) | `NovaTuple_<Name>*` (pointer) | `f(&v)` для identifier; для rvalue — hoist в temp + `&temp` |

**Mutation visibility:** `mut @method` мутирует caller's slot через
pointer; copies стека не делается. Это symmetric с D228 value-record
`mut @method` (`NovaValue_X*`).

**Call-site emission (`emit_c.rs::prepare_method_recv`):**

- **Identifier receiver:** emit `&local_var` directly. Var must be `mut`
  binding (D33 + D215 amend «binding-level mutability»); ro binding +
  mut @method = `E_BINDING_NOT_MUT` (caught в type-checker).
- **Lvalue projection receivers:** `b.v.method()`, `arr[i].method()`,
  `@field.method()`, multi-level `a.b.c.method()` — when each base в
  projection chain is an lvalue (Ident/SelfAccess/Member-of-lvalue/
  Index-of-lvalue), emit `&(b->v)` / `&(arr->data[i])` /
  `&(nova_self->field)` directly. Mutation flows к original slot,
  no temp hoist. Plan 128.1 Ф.1 implementation.
- **Rvalue receiver:** hoist в `NovaTuple_<Name> __tmp_recv_<id> = expr;`
  и pass `&__tmp_recv_<id>`. Мутации в temp видны только внутри
  expression chain — corresponds к D32 «mutate-by-copy для rvalue» spirit.
- **Chained `.method()`** на trailing receiver — recurse same rule.

**Symmetric правило с records (D32):** records передаются `Nova_<Name>*`
unconditionally; named tuples — by-value кроме `mut @` receiver path,
который промоутится к `NovaTuple_<Name>*`. Это codifies «no pointer in C
signature **кроме mut receiver**» refinement над D215 original wording.

**Wired через `recv.mutable` flag** (`MethodCallInfo::recv`) — Plan 128
Ф.1 thread'нул flag через `emit_c.rs` helpers; Ф.2 consume'нул для
NamedTuple codegen branch. См. также §D228 Ф.4 «Method receiver
passing» — параллельный pointer pattern для value-records.

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

## D354. Generic anonymous tuple monomorphization

> **Renumber 2026-07-03:** блок был **D216** — номер коллидировал с [D216 Typed pointer family](#d216-typed-pointer-family--unsafe-model--null-safety-через-npo); anon-tuple-mono перенумерован в D354 (решение владельца; приёмник из резерва Plan 174 §6, прецедент D109/D110/D111).
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

  > **Update 2026-06-08 (Plan 91 Ф.1, `[M-91.1-composite-array-storage]`):**
  > Для **pointer-элементов** (record/sum `Nova_<Name>*`) этот класс
  > РЕШЁН выбранным здесь подходом «align infer с body fallback» +
  > завершением side-channel `array_element_types`. Контракт хранения
  > composite-массивов: элементы record/sum — boxed-pointer в `nova_int`
  > слоте (`NovaArray_nova_int*`), а **реальный** elem C-тип проносится
  > через `array_element_types` (var→`Nova_<X>*`) и проставляется на
  > результат generic `map`/`filter` (`register_array_result_elem`),
  > так что `[i]`, `for-in` и `.get()` кастят слот назад к указателю.
  > **Tuple-by-value** (`[]((T,T))` — value-struct >8 байт) НЕ покрыт
  > erasure-подходом и остаётся открытым как `[M-91.1-value-struct-array-elem]`
  > (тот же класс, что и `[]Option[T]`). Подробности — plan-91 Ф.1 closure.

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
- [D215](#d215-named-tuple-fields--valuereference-allocation-contract) — named tuple types (Plan 120 D215, ortho к D354).
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

## D367. Удаление `byte`: каноническое имя — `u8`

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

## D368. Strict type propagation в codegen — no silent `nova_int` fallback

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
**хуже всех baseline** (silent default). D368 закрывает регрессию.

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
в [docs/dev/codegen-erasure-sites.md](../../docs/dev/codegen-erasure-sites.md).

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

**Internal lint guard (CI).** `scripts/guards/lint-no-silent-int-fallback.sh`
greps `compiler-codegen/src/` против baseline counts из
`docs/dev/codegen-erasure-sites.md`. Bumping baseline требует:
1. Inline comment с rationale «почему erasure безопасна»
2. Entry в `docs/dev/codegen-erasure-sites.md` со file:line + причина
3. PR review

CI gate fails если added counts превышают baseline без updates.

**Acceptance criteria (Plan 70 closure).**
- [x] Helper infra `err_no_int_fallback` + `record_strict_error` (Ф.1 / Ф.B0)
- [x] Cat A1/A2 migration: 90 → 8 (only Cat B holdovers remain)
- [x] Cat B documentation: 10 sites listed в codegen-erasure-sites.md
- [x] Internal lint guard `scripts/guards/lint-no-silent-int-fallback.sh`
- [x] Spec D368 (этот блок)
- [x] 796+ PASS / 0 FAIL nova test (0 regressions vs baseline 761)

**Реализовано:** [Plan 70](../../docs/plans/70-no-silent-nova-int-fallback.md)
  — sessions 1+2 (2026-05-18); 90+ Cat A1 sites migrated, infrastructure
  complete, lint guard active.

**Связь:**
- D118 — typed `Fail[E]` codegen (similar precision-by-construction pattern)
- Plan 67 — println overload fix (sibling: один из видимых частных случаев)
- Plan 48 — monomorphization (упрощает Cat B → меньше erasure)
- Plan 36 — diagnostic infra (R7 structured format)
- [docs/dev/codegen-erasure-sites.md](../../docs/dev/codegen-erasure-sites.md) — Cat B/D inventory

---

## D128. `char` distinct from `int` в codegen mono'd generics

**AMEND (Plan 152.8, 2026-06-16).** `nova_char` переведён с `int64_t` на
`uint32_t`. Codepoints fit in 21 bits (U+0000..U+10FFFF); `uint32_t` —
естественный unsigned type (как Rust `char` ABI). ABI cost минимален:
`nova_char`-поля в structs layout'ятся как 4-byte, Box-pointer смещается
с 8 на 8 (4-byte char + 4-byte padding → align 8). GC layout обновлён:
`char_size = (4, 4)` (было `(8, 8)`). Char-literal суффикс: `U` вместо
`LL`; is_typed_int_c_ty / emit_typed_int_literal включают `nova_char`.

**Решение (исходное, Plan 70.3).** Тип `char` имеет собственный C-typedef
`nova_char` (alias над `int64_t` → **`uint32_t` D128 AMEND**), distinct C
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

1. **Typedef:** `typedef int64_t nova_char;` → **`typedef uint32_t nova_char;`
   (D128 AMEND Plan 152.8)** в `compiler-codegen/nova_rt/nova_rt.h`.
2. **Codegen mapping:** `type_ref_to_c "char" => "nova_char"` (was
   `"nova_int"`) в `emit_c.rs` и `external_registry.rs` (двойная sync).
3. **Array element:** `[]char → NovaArray_nova_char*` (separate
   instantiation parallel `NovaArray_nova_int*`).
4. **Option element:** `NovaOpt_nova_char` typedef + constructors +
   `nova_opt_eq_nova_char` helper.
5. **CharLit emission:** `'x' → ((nova_char)<codepoint>U)` (was `LL`; D128
   AMEND Plan 152.8 — uint32_t requires U-suffix).
6. **infer_expr_c_type:** `CharLit => "nova_char"` (was `"nova_int"`).
7. **Runtime fn signatures:** `nova_str_char_at` updated return
   `NovaOpt_nova_char` (was `NovaOpt_nova_int`).

**GC layout (D128 AMEND Plan 152.8).** `char_size = (4, 4)` (was `(8, 8)`).
In a struct with a `char` field followed by a Box pointer: char occupies
4 bytes + 4-byte pad → Box at offset 8. `gc_layout.rs::prim_emit("char") =>
Some((4, 4))`.

**Backward compat.** В `emit_binary_op` special-case для
`Nova_StringBuilder* + char` accepts **обе** `nova_char` AND `nova_int`
для backward-compat — pre-fix existing code emitted char as `nova_int`,
existing test binaries reference legacy form. After full migration of
existing generated C (regen test fixtures), `nova_int` branch может
быть удалён.

**ABI cost.** Minimal. `nova_char` is `typedef uint32_t` — 4 bytes vs 8
bytes. Struct layout changes where `char` is followed by a pointer (padding
shrinks from 0 to 4 bytes). GC scanner updated. C type identifier remains
distinct from `nova_int`.

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
  **D128 AMEND (Plan 152.8)** — nova_char int64_t→uint32_t, 2026-06-16.

**Связь:**
- D26 — Q-string-indexing (char = codepoint convention)
- D54 — `as`-cast narrowing (explicit char↔int conversion)
- Plan 70 — parent family (silent type bugs от Nova↔C collapse)
- Plan 70.4 — sibling proposal (f32/f64 generic-container distinct mangling)

---

## D129. `int` как alias `i64` в bootstrap Nova

> ## ⚠️ AMEND (Plan 133) — ЧИТАЙ ПЕРВЫМ: `int` = `intptr_t`, **НЕ** `int64_t`
>
> ### **`int` = `nova_int` = `intptr_t` — ЗНАКОВОЕ ADDRESS-SIZED ЦЕЛОЕ (модель Go C-эры `intgo`), а НЕ `int64_t`.**
>
> - **`int`** → `nova_int` (`typedef intptr_t`) — ширина = ширине указателя платформы
>   (64 бита на x86_64/ARM64, 32 бита на 32-bit/WASM). См.
>   [`nova_rt.h`](../../compiler-codegen/nova_rt/nova_rt.h): `typedef intptr_t nova_int; /* int — signed address-sized (Go C-era intgo, Plan 133) */`.
> - **`i64`** → `int64_t` — **ВСЕГДА ровно 64 бита**, независимо от платформы.
> - На bootstrap-таргете (x86_64, 64-битный указатель) `int` и `i64` **СОВПАДАЮТ по ширине и
>   значению** — отсюда историческое слово «alias» в заголовке/теле ниже — **НО это РАЗНЫЕ C-типы**:
>   `primitive_name_to_c` даёт `int → nova_int` и `i64 → int64_t`, поэтому их **mangle-имена
>   РАЗЛИЧАЮТСЯ** (`NovaOpt_nova_int` ≠ `NovaOpt_int64_t`; `Map[int,V]` ≠ `Map[i64,V]` по C-имени).
> - **«int ≡ i64» — совпадение ШИРИНЫ на 64-бит, НЕ тождество типов.** Не считать `int` равным
>   `i64`/`int64`: аналогия — **Go `int` ≠ `int64`**, **Rust `isize` ≠ `i64`** (platform-pointer-width).
>   На 32-bit/WASM `int` станет 32-битным, `i64` останется 64-битным.
> - **Следствие для §0/named-priority:** числовая константа/выражение типа `i64` НЕ должна
>   схлопываться в `nova_int` (и наоборот) — это разные типы (см. Plan 172.1 P67 ФАЗА 2 STEP 1,
>   де-коллапс `i64.MAX→int64_t`). То же `char` (codepoint, `nova_char`) ≠ `int`.
>
> Текст «**Решение / Мотивация / Codegen**» НИЖЕ — **ИСТОРИЧЕСКИЙ** (Plan 70.4, до Plan 133):
> его утверждения «оба → `nova_int` (`typedef int64_t`)» и «mangle идентичен» **УСТАРЕЛИ** —
> читать как «совпадают по ширине на 64-bit», а C-тип `nova_int` теперь `intptr_t`, не `int64_t`.

**Решение.** Тип `int` в Nova bootstrap является **alias** для `i64`
(64-bit signed integer) **ПО ШИРИНЕ/ЗНАЧЕНИЮ на 64-bit таргете** (см. ⚠️ AMEND выше —
C-типы РАЗНЫЕ: `int`→`nova_int`=`intptr_t`, `i64`→`int64_t`). ~~Оба маппируются в C-тип
`nova_int` (`typedef int64_t`)~~ **[УСТАРЕЛО Plan 133]**. Отсутствие distinction в codegen
~~намеренно~~ относится к bootstrap-x86_64 (где ширины совпадают), но `int` и `i64` —
**различимые типы** (разный C-typedef, разный mangle): это не collapse-баг, а address-sized
vs fixed-width архитектурное различие (Plan 133).

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

**AMEND (Plan 133).** C-тип `uint` — **`nova_uint`** (`typedef uintptr_t`, address-sized
unsigned), а не сырой `uint64_t`. На x86_64-bootstrap (фикс. 64-бит указатель) `nova_uint`
≡ `uint64_t` по ширине/знаку; `u64` остаётся фиксированным `uint64_t`. Канонический словарь —
`primitive_name_to_c` (`uint → nova_uint`).

**AMEND (Plan 172.1-K1, 2026-06-28).** `uint` (и вся РАЗЛИЧНАЯ int-семья) лоуэрится в свой
точный C-тип во **ВСЕХ** позициях — включая **method-receiver**. Ранее `receiver_c_type`
**схлопывал** примитивный ресивер (`uint`/`u8`..`u64`/`i8`..`i32`) в `nova_int` (Plan 70.5
«64-бит слот») — нарушение §0/§10/D368 (второе окно правды) и **soundness-баг**:
`Nova_uint_method_compare(nova_int, nova_int)` давал **знаковое** сравнение беззнаковых (неверный
порядок для `uint` с установленным старшим битом). Теперь scalar-arm `receiver_c_type` делегирует
в единый `primitive_name_to_c` (тот же лист, что `resolved_type_to_c`): `uint`-ресивер = `nova_uint`
→ `@`-операции беззнаковые ПО ПОСТРОЕНИЮ. `int`≡`i64`→`nova_int` сохраняется (D129 — намеренный
alias, НЕ схлопывание различных типов). Acceptance: `spec_tests/conformance/d130_uint_method_compare.nv`.
`receiver_c_type` как отдельная функция ретайрится в receiver-aware `resolved_type_to_c` (U.4.5/FIN).

**Решение.** Тип `uint` является **alias** для `u64` (64-bit unsigned
integer) в Nova bootstrap. Маппируется в C-тип `uint64_t` **(AMEND Plan 133: → `nova_uint`)**. Отличие
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
| `T { field: tx, … }` — init consume-поля record-литерала голым binding'ом | `tx` → `Consumed` (move при конструировании) |
| `consume new_owner = tx` (transfer alias) | `tx` → `Consumed`, `new_owner` → `Live` |
| `f(tx)` где `f(tx Tx)` — view-param (no qualifier) | `tx` остаётся `Live` (callee — view-borrow) |
| `f(make_tx())` где `f(t Tx)` — rvalue → view-param | ❌ E (D133-consume-rvalue-in-view) |
| `f(tx)` где `f(mut tx Tx)` — mut-view-param | `tx` остаётся `Live` (callee — mut-borrow) |
| `f(make_tx())` где `f(mut t Tx)` — rvalue → mut-view-param | ❌ E (D133-consume-rvalue-in-mut-view) |
| `let alias = tx` — view-alias | оба в alias-class (Plan 73); consume любого инвалидирует |
| `let mut alias = tx` — mut-view-alias | то же + mut-методы через alias |
| `let _ = tx` (silent drop) | ❌ compile error D133-suppress-not-allowed |

> **Амендмент 2026-07-13 (fix `[M-178-consume-field-ctor-from-var]`):**
> строка «init consume-поля record-литерала» — НОРМАТИВНОЕ уточнение,
> закрывающее языковой обход. Инициализация **consume-поля** record-литерала
> **голой owned-переменной или consume-параметром** (`{ tcp: stream, session }`)
> — потребление этого биндинга, ровно как передача в consume-параметр вызова.
> Действует для всех форм литерала: типизированного (`T { … }` /
> record-вариант суммы `V { … }`), анонимного с типом из контекста
> (`=> { tcp: stream, session }` — best-effort структурный резолв;
> неоднозначность → консервативно без move, остаётся D133-not-consumed) и
> D52-punning (`{ tx }`). Использование биндинга ПОСЛЕ литерала —
> use-after-consume (D131); два consume-поля из одного биндинга — то же.
> До амендмента распознавалось только свежее inline-выражение/вызов в
> позиции поля, что вынуждало pass-through-обёртки вида
> `fn tcp_move(consume s TcpStream) -> TcpStream => s`.

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
  commit/rollback. D133 требует **явный** consume-метод. **Смягчено
  [D432](#d432) (Plan 217, 2026-07-20):** аргумент «размывает выбор»
  относился к безусловному drop БЕЗ доступа к исходу. После введения
  `ScopeOutcome` (D314) авто-`@cleanup(outcome)` ветвится по исходу
  (`Success`/`Failure(e)`/`Panic(m)`) так же явно, как ручной вызов в
  `consume X = e { body }` (D188) — размывания больше нет. D432 разрешает
  авто-вызов КАК OPT-IN: только для типов, объявивших эффект-чистый
  `@cleanup` (§8а п.1 плана 217). Тип БЕЗ `@cleanup` (например
  `StringBuilder` — потребление извлекает ценность, тихий drop = потеря
  работы) остаётся строго-линейным без изменений — «явный consume-метод»
  для НЕГО действует, как здесь и написано.
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
- [D432](#d432) Plan 217 — авто-`@cleanup` (гибрид C): смягчает «Что
  отвергнуто»/Drop-method пункт выше для эффект-чистых `@cleanup`-типов;
  оставляет строгую линейность (эта секция, без изменений) для типов без
  `@cleanup`.

---

## D432. Авто-`@cleanup` для непотреблённых consume-переменных (гибрид C)

**Статус:** ПРИНЯТО (Plan 217, владелец 2026-07-20, решения §8а).
**Амендирует:** D133 §«Что отвергнуто» (Drop-method auto-cleanup, смягчено
выше), D131/[D180](05-memory.md#d180) Rule 6 (не применяется к
`@cleanup`-типам, см. ниже), [D314](03-syntax.md#d314) (переиспользует
defer-kernel как транспорт, §3 ниже).

### Суть (гибрид C)

Для `type X consume`, объявившего `@cleanup`, — семантика сдвигается с
**линейной** (=1, D133 «ровно один раз») на **аффинную** (≤1, D131-стиль
«забыть можно»): непотребление к концу скоупа БОЛЬШЕ НЕ ошибка,
компилятор сам вставляет вызов `@cleanup(outcome)` на висячих exit-путях.
Для `type X consume` БЕЗ `@cleanup` — D133 действует без изменений
(строгая линейность, явное потребление обязательно).

Выбор аффинный/линейный делается САМИМ фактом объявления `@cleanup` —
видимый в исходнике признак, без скрытой магии и без нового keyword'а.
Канонический пример линейного класса — `StringBuilder` (`@into_str`
извлекает ценность; тихий авто-drop потерял бы построенную строку) —
такие типы `@cleanup` НЕ объявляют и остаются строго-линейными.

### §1. Ограждение 1 — эффект-чистота (ГЛАВНОЕ, обязательное)

Авто-cleanup применяется **ТОЛЬКО** к типам, чей `@cleanup(outcome
ScopeOutcome) -> ()` объявлен с **ПУСТЫМ** effect-row (эквивалент
`Cleanup[never]`). Прецедент [214](../../docs/plans/214-coerce-attribute.md)
R12 (эффект-свобода — `E_COERCE_EFFECTFUL`): скрытый эффект в неявной
позиции запрещён под `--strict-effects`. Fallible-`@cleanup` (непустой
effect-row, например `Fail[IoError]`) остаётся строго-линейным — D133
диагностика (`D133-not-consumed`) продолжает срабатывать для него
БЕЗ ИЗМЕНЕНИЙ, как если бы D432 не существовал. Включение
fallible-cleanup (эффект-пропагация в сигнатуру объемлющей функции ИЛИ
иная политика) — отдельное будущее решение, вне периметра D432.

**АМЕНДМЕНТ 2026-08-04 (решение владельца) — дверь, оставленная выше,
открыта: эффектная и падающая очистка РАЗРЕШЕНА с пропагацией.**

Ограничение «только пустой effect-row» снимается. Правило:

1. **Эффекты авто-вставленного вызова `@cleanup` — ПРЯМЫЕ эффекты
   объемлющей функции.** Не транзитивные: вызов физически генерируется в
   теле ЭТОЙ функции, значит эффект происходит здесь. Различие
   существенно — по [D62](04-effects.md#d62) транзитивные эффекты дают лишь
   предупреждение, и без этой фразы правило выродилось бы в необязательное.
2. **`Fail[E]` очистки обязан появиться в сигнатуре** объемлющей функции.
   Формулировка владельца 2026-08-04: «эффект `Fail` должен появиться в
   функции, если его требует процесс авто-очистки». **Новым правилом это НЕ
   является** — действующая норма уже говорит ровно это: `Fail` есть
   исключение из «только прямые», он **строго транзитивен и обязателен в
   сигнатуре везде, где может произойти**, и не ослабляется никаким флагом
   (обзор `spec/effects.ru.md`, раздел «Прямые эффекты, не транзитивные»;
   [D65](04-effects.md#d65)). Отсюда важный вывод для планирования: **№315 —
   не языковой пробел, а дефект реализации.** Проверка не видит
   синтезированный вызов очистки, потому что десугаринг блочной формы идёт
   ПОСЛЕ проверок эффектов. Правило менять не нужно, нужно чинить чекер.
3. **Кто пишет — по [D28](04-effects.md#d28)/[D62](04-effects.md#d62), без
   новой политики:** в private-функции компилятор выводит и добавляет сам;
   в `export fn` прямые эффекты обязательны явно, поэтому отсутствие
   `Fail[E]`/`Fs` — ошибка компиляции с указанием на биндинг, который её
   породил.
4. **Отказ очистки на уже падающем пути ПРИСОЕДИНЯЕТСЯ, а не замещает.**
   Решается по `outcome`, который в очистку уже передаётся: при
   `Success` отказ очистки распространяется как отказ функции; при
   `Failure`/`Panic` — присоединяется к исходной причине. Модель подавленных
   исключений Java. Альтернатива «замещать» теряет первопричину, а «молча
   глотать» — путь Go с `defer f.Close()`, где ошибка закрытия исчезает;
   оба отвергнуты.

Следствие: `File` и `TcpStream` перестают быть исключением из авто-очистки
(см. `std/src/fs/fs.nv:181` — комментарий об исключении подлежит снятию тем
же слиянием, что и реализация). **Контейнеров это НЕ касается** — см. §2
ниже: drop-glue не вводится, и `Vec[File]` по-прежнему не прибирается
поэлементно. Для контейнеров действует наследование линейности
(амендмент к [D156](#d156)), а не авто-очистка.

### §2. Ограждение 3 — НЕ рекурсивен (нет drop-glue)

Авто-cleanup применяется ТОЛЬКО к именованным bare consume-биндингам
(`consume X = e;`, одиночный `Ident`-паттерн). НЕТ drop-glue: record с
`@cleanup`-полем, `Vec[TlsStream]` и прочие агрегаты/контейнеры полем/
поэлементно НЕ прибираются — правила передач владения (Stored-in-
escaping-field = Transferred, D131) не меняются. Деструктурирующие
паттерны (`let (a, b) = e`, `let { x, y } = e`) — НЕ покрываются (остаются
под старым D133, если применимо).

> **Амендмент 2026-08-15 (реестр 221.1 №667): «паттерн» здесь — ЛЮБОЙ
> паттерн, включая арм match и `if let`.** Формулировка выше перечисляла
> только `let (a, b)` / `let { x, y }`, и это прочли как «про кортежи и
> записи», а не как правило. Биндинг, введённый ПАТТЕРНОМ — `match e {
> Ok(consume r) => … }`, `if let Ok(consume r) = e { … }` — авто-cleanup
> НЕ получает: кодоген армит `DeferEntry` только для bare-`Stmt::Let` с
> одиночным `Ident` (`auto_cleanup_qualifies(&LetDecl)`), у паттерн-биндинга
> точки постановки нет вовсе. Значит непотребление такого биндинга —
> **честная ошибка `D133-not-consumed`**, а не тихое принятие: чекер обязан
> снимать D432-исключение ровно там, где кодоген ничего не вставляет.
> **Что это стоило до амендмента:** `nova-polaris` брал слот семафора
> арм-биндингом (`Some(consume p) => …`) с комментарием «D432 вернёт слот на
> выходе» — слот не возвращался никогда, и после `max_inflight` соединений
> сервер вставал молча. Норму держат фикстуры
> `spec_tests/conformance/neg/m667_arm_binding_leak_neg.nv` (утечка = ошибка)
> и `standalone/m667_arm_binding_and_variant_ctor_ownership.nv` (все законные
> маршруты владения компилируются).

> **АМЕНДМЕНТ 2026-08-21 (решение владельца; реестр 221.1 №672):
> исключение §2 освобождает не «всё, кроме перечисленного», а РОВНО
> ОДНУ ФОРМУ — ту, что заведена bare `Stmt::Let` с `Pattern::Ident`.**
>
> Амендмент 2026-08-15 (№667) сказал правильную вещь про паттерны и
> оставил формулировку перечислительной. Перечисление и есть дефект: форм
> биндинга в языке больше, чем три, и каждая неназванная молча
> освобождалась исключением, которое кодоген не подкрепляет. Замер
> интегратора 2026-08-19/20 нашёл ПЯТЬ таких форм, и каждая была
> увидена НАБЛЮДЕНИЕМ — `println` внутри `@cleanup` не печатался ни разу.
>
> **НОРМА.** Авто-`@cleanup` снимает обязательство ТОЛЬКО с биндинга,
> заведённого как `consume X = e;` с одиночным `Ident`-паттерном. Правило
> формулируется через ПРОИСХОЖДЕНИЕ биндинга, а не через список
> исключений, именно чтобы следующая форма биндинга, которую добавят
> в язык, попадала под норму автоматически, а не текла до тех пор,
> пока её не впишут в список. Обязательство потребления заводится НА
> ВСЕХ формах; исключение — одна.
>
> **Пять форм, поимённо:**
> 1. consume-ПАРАМЕТР — `fn eat(consume r T)`. Вызывающий дизармит СВОЙ
>    флаг при передаче (§4 п.3), вызываемый не чистит никогда.
> 2. `while Pat = expr` — биндинг паттерна цикла, свежий на каждой
>    итерации и обязанный быть потреблённым в теле на ВСЕХ путях.
> 3. consume-РЕСИВЕР — `fn T consume @m()`. **ОСТАЁТСЯ ОТКРЫТОЙ**, см. ниже.
> 4. результат-биндинг consume-области — `consume out = consume s { … }`.
> 5. параметр ЗАМЫКАНИЯ и хендлер-опа. Типизированные формы
>    (closure-full `fn(consume v T) -> …`, типизированный параметр lambda,
>    хендлер-оп) — под нормой. Closure-light `|x| …` НЕ покрыта:
>    по [D22-rev](03-syntax.md) у её параметров типов нет в грамматике, и
>    происхождение биндинга там нечем заполнить.
>
> **ФОРМА (3) ОТКРЫТА, И ЭТО ВОПРОС К ЯЗЫКУ, А НЕ К ЧЕКЕРУ.**
> Попытка завести обязательство на consume-ресивер была сделана и снята
> ЗАМЕРОМ 2026-08-22. Вопрос, на который язык сегодня не отвечает:
> чем РАЗРЕШАЕТСЯ обязательство приёмника. Четыре признака
> распоряжения были найдены по живым носителям и каждый работал:
> делегат `@cleanup` (`File.close`), must-consume в типе возврата
> (`TcpStream.into_split`), вынос `@` хотя бы на одном пути
> (`TcpStream.close`), пустой `@cleanup`. Пятый носитель не берётся ниодним:
> `fn D188MvBoom consume @defuse() -> () { }` — НАМЕРЕННЫЙ сброс ресурса,
> чей cleanup паникует, и он СТРУКТУРНО НЕОТЛИЧИМ от пробы №672
> `fn R672 consume @swallow()`, которая намеренно течёт. Пока в языке
> нечем объявить «этот метод — терминальный освободитель» либо «этот
> сброс намерен», форма (3) закрыта быть не может без ложных отказов
> на законном коде.
>
> **Что НЕ стало нормой, хотя выглядело убедительно:** «пустой
> `@cleanup` — терять нечего». Применённое к исключению вообще, это
> правило делает `neg/m667_arm_binding_leak_neg` ЗЕЛЁНОЙ: в корпусе
> пустой cleanup — рабочий маркер ResourceTrace, а не признак отсутствия
> ресурса. Записано, чтобы не пробовать второй раз.
>
> **Норму держат фикстуры** `spec_tests/conformance/neg/m672_form{1,2,4,5}_*_neg.nv`
> (утечка = ошибка, каждая с построчным `nova:expect`) и их позитивные
> близнецы `spec_tests/conformance/m672_form{1,2,4,5}_*_ok.nv` (законные маршруты
> владения компилируются).

### §3. Ограждение 4 — отброшенный временный результат

Несвязанный результат `@cleanup`-типа как bare-statement (`acquire_lock()`
без биндинга) — авто-cleanup НЕ применяется (нет именованного биндинга,
на который можно было бы повесить flow-анализ). **Примечание к
реализации (2026-07-20):** на момент D432 общий D133 uncomitted-temporary
diagnostic для АНОНИМНОГО discard-результата любого consume-типа (не
только `@cleanup`-типа) эмпирически НЕ производит отдельную ошибку —
это ПРЕДСУЩЕСТВУЮЩИЙ (pre-D432) пробел в D133-диагностике, НЕ регрессия
от D432 (D432 расширяет обработку строго на именованные `Stmt::Let`,
анонимные значения не трогает вовсе — поведение до/после D432 идентично
для этого случая). Закрытие пробела — отдельный follow-up
(`[M-d133-discarded-temporary-diagnostic]`), вне периметра D432.

### §4. Flow-sensitive семантика (дизарм на передаче владения)

Авто-cleanup — **НЕ** безусловный `defer` (тот срабатывает на КАЖДОМ
выходе безусловно). Это flow-sensitive вставка: `@cleanup(outcome)`
эмитится ТОЛЬКО на exit-путях, где биндинг ещё `Live`/`MaybeConsumed`
(D131 `VarState`) в момент выхода. На путях, где биндинг уже `Consumed`
(явно потреблён: `return X` голым идентификатором, передача в
consume-параметр, вызов consume-метода на биндинге) — cleanup НЕ
запускается (семантика Rust `Drop`: перемещённое значение не дропается).

**Доказанно-безопасные дизарм-точки (реализация, Ф.2):**
1. `return X` (голый идентификатор) — D133 «Returned»-способ
   удовлетворения обязательства, безусловный (не зависит от режима
   параметра вызываемой функции).
2. Bare-statement (или любая позиция, где значение отбрасывается)
   receiver-вызов `X.method()`, где `method` — ЗАРЕГИСТРИРОВАННЫЙ
   consume-метод типа `X` (совпадает с checker's `is_consume_method`) —
   ЛЮБОЙ такой метод (не только `@cleanup`), напр. `g.unlock()` наряду с
   гипотетическим `g.cleanup()`.
3. Прямая передача биндинга аргументом вызова (`foo(g)`, free-fn ИЛИ
   `recv.method(g)`) на позиции, которая `consume`-режима хотя бы на ОДНОМ
   overload'е этого имени (`free_fn_consume_param_positions`/
   `method_consume_param_positions` — консервативный union по имени,
   зеркало checker's `consume_args`/`consume_idxs`). Найдено ОБЯЗАТЕЛЬНЫМ
   (не опциональным) folder-CU-регрессом: `guard_cross_scope_transfer.nv`
   (`consume g = mu.lock(); do_work_under_lock(g, counter)` — обычный
   std-паттерн «передать guard в хелпер») ломался двойным `unlock` без
   этого дизарма — то, что изначально казалось «редким гипотетическим
   пробелом», оказалось активным сценарием в существующем корпусе.
4. Вход в ЛЮБОЙ RE-CONSUME БЛОК (`consume X { body }`, D188/201-амендмент)
   на бывшем bare-биндинге `X` — блок берёт cleanup «на себя» целиком
   (exactly-once, свой tail/return-дизарм), поэтому OUTER auto-cleanup флаг
   дизармится БЕЗУСЛОВНО при входе в блок (до эмиссии тела), иначе cleanup
   срабатывает дважды. Тоже найдено folder-CU-регрессом
   (`d188_reconsume_block.nv`, D201Boom-сторож).

Все четыре точки реализованы; известных пробелов в disarm-механике на
момент принятия D432 нет (folder-CU регресс — `spec_tests/conformance`
одним CU — прогнан до зелёного после КАЖДОГО из вскрытых случаев).

### §5. Drop-флаг (§8а п.6, MaybeConsumed на общем exit-пути)

Владелец выбрал **(а) рантайм drop-флаг** (паритет с Rust drop-flags).
Реализация: скрытый `int _active` per биндинг (см. Ф.2), взводится ПОСЛЕ
захвата ресурса (partial-init safety — если инициализация throw'ит, флаг
остаётся невзведён), сбрасывается в 0 на каждой доказанно-безопасной
дизарм-точке (§4), читается ТОЛЬКО на exit (normal/return/throw/panic/
interrupt/cancel) — если `MaybeConsumed` (потреблён в одной ветке
if/match, не в другой), ветка, где дизарм произошёл, НЕ вызывает cleanup
повторно; ветка, где дизарма не было, вызывает его корректно. Это
буквально семантика Rust drop-flag (динамический, не статически
элиминированный — элиминация «эта переменная гарантированно Live на
ВСЕХ путях сюда, флаг не нужен» — оптимизация, НЕ требуется для
корректности, вне периметра D432).

### §6. Циклы

Consume-биндинг, объявленный ВНУТРИ тела цикла (`while`/`for`/`loop`), —
авто-cleanup срабатывает на КАЖДОЙ итерации (флаг/машинерия объявлены
внутри тела цикла — переисполняются с нуля на каждый проход, идентично
существующей семантике `defer` внутри цикла). `break`/`continue` —
exit-пути тела итерации, покрыты тем же early-exit-cleanup механизмом,
что и `return` (D314 §2 exit-таблица логически расширяется этими двумя
строками — маппинг на `ScopeOutcome::Success`, т.к. `break`/`continue`
не несут исход throw/panic).

### §3a (нумерация плана). Судьба блока `consume X = e { body }`

**Решение владельца: 3a — блок ОСТАЁТСЯ** (см. план §3а). После D432 обе
формы дают одинаковое авто-закрытие для `@cleanup`-типов: bare
`consume X = e;` закрывается в конце ОБЪЕМЛЮЩЕГО скоупа; блок-форма
`consume X = e { body }` закрывается в конце `body` — явное СУЖЕНИЕ
времени жизни (аналогия: Rust — авто-`drop` в конце блока по умолчанию,
вложенный `{ }`/явный `drop(x)` — для раннего освобождения). Блок-форма
technической реализации не меняется (D188/D314 remain as-is); bare-форма
получает НОВУЮ codegen-машинерию (per-block `DeferEntry.consume_policy`,
см. Ф.2 ниже).

### §9 (амендмент 2026-08-15, реестр 221.1 №671). Конструктор варианта — точка передачи владения

Передача consume-значения **аргументом конструктора варианта** —
`Ok(r)`, `Some(r)`, любой `Variant(r)` пользовательской суммы — есть
**потребление** ровно в том же смысле, что передача в consume-параметр
функции (D133): владение уходит в payload варианта и покидает область
вместе со значением. Канонический конструктор ресурса

```nova
fn open() -> Result[Res, IoError] {
    consume r = Res.acquire()
    Ok(r)                       // владение ушло вызывающему
}
```

обязан компилироваться, и `r` обязан считаться потреблённым.

**Исключение — биндинг `consume X { … }`-блока.** Внутри блока владением
безусловно распоряжается сам блок (D188-амендмент, Plan 201), а законный
вынос `return Ok(s)` разбирает его собственная машинерия; правило выше на
такой биндинг не распространяется.

**Что это стоило до амендмента:** обе стороны компилятора были слепы к этой
точке — кодоген чистил уже отданное значение (№666: `@cleanup` на
переданном наружу сокете, туннель умирал сразу после рукопожатия), а чекер
требовал потребления, которое уже произошло (№671: для строгого
consume-типа без `@cleanup` форма выше не компилировалась вовсе; для типа с
`@cleanup` это маскировалось исключением из §2).

### Реализация (сводка, Ф.1-Ф.2)

- **Чекер** (`compiler-codegen/src/types/mod.rs`): `LinearityRegistry.
  cleanup_pure_types` (тип → объявлен ли эффект-чистый `@cleanup`);
  `check_obligations_at_exit` не эмитит `D133-not-consumed`/
  `D156-strict-forget` для `Live`/`MaybeConsumed` состояний таких типов —
  **но только для bare-биндингов** (амендмент 2026-08-15, №667: на выходе
  арм-паттерна исключение снимается флагом `arm_pattern_exit_check`, и
  диагностика эмитится честно; см. §2).
- **Codegen** (`compiler-codegen/src/codegen/emit_c.rs`): bare-consume-let
  qualifying-типа получает СВОЙ `DeferEntry{consume_policy: Some(...)}`
  внутри УЖЕ СУЩЕСТВУЮЩЕГО per-block стека (`enter_defer_scope`/
  `leave_defer_scope`/`emit_early_exit_cleanup` — все три уже умели
  `consume_policy` генерически из D188/D314; ДВА inline run-сайта внутри
  `enter_defer_scope` самого (FAIL/INTERRUPT — тело блока throw'ит/
  interrupt'ится мидвей) потребовали нового кода, зеркалящего
  `enter_consume_defer_scope`'s собственные копии). Cancel-shield ЕСТЬ
  (`nv_consume_enter_shield`/`leave_shield`); watchdog-порог/ResourceTrace
  из D188 R1/R3 — ResourceTrace переиспользован (дёшево, наблюдаемость
  паритетна блок-форме), watchdog-таймаут НЕ подключён (`threshold_var =
  "0"` — secondary feature, вне keystone).

### §7. Раскатка на std-ресурсы (Plan 217.1)

Механизм §1-§6 не хардкодит список типов — `cleanup_pure_types` строится
структурно (любой `consume`-тип, объявивший `@cleanup(outcome ScopeOutcome)
-> ()` с пустым effect-row, автоматически становится аффинным). Plan 217.1
(2026-07-22) добавил такое объявление ещё четырём std-ресурсам сверх
исходных шести (4 lock-guard'а + `TcpStream` + `TlsStream`, nova-tls):
`TcpListener`, `TcpReadHalf`, `TcpWriteHalf` (std/src/net/tcp.nv),
`UdpSocket` (std/src/net/udp.nv) — каждый `@cleanup` = вызов уже существующего
`@close()` (эффект-чистый по сигнатуре сверху, как у `TcpStream`). Никакого
нового языкового механизма — чистое применение §1-§6 к дополнительным
типам, поэтому это НЕ отдельный D-блок.

**Осознанно НЕ включены** (явная причина, не забывчивость, владелец
Plan 217 §8а п.4):
- **`File`** (std/src/fs/fs.nv) / **`BufWriter[W]`** (std/src/io/buffered.nv)
  — `@close()` фаллибелен (`Fs`/`Fail`-несущий `Result`); §1 (эффект-
  чистота) уже отсекает их формально, а по духу — оба типа документируют
  «ошибка close/flush НИКОГДА не глотается на drop» как СВОЙ differentiator
  (см. комментарии у типов) — авто-cleanup, проглатывающий ошибку, был бы
  прямым регрессом design-intent. Остаются строго линейными; см. §8 п.1
  (fallible-`@cleanup` — отдельное будущее решение, если вообще будет).
- **`OnceGuard`** (std/src/runtime/sync.nv) — два discharge-метода
  (`.commit()`/`.abort()`) НЕ взаимозаменяемы (в отличие от единственного
  idempotent-release у lock-guard'ов): `@cleanup(outcome)` не может отличить
  «настоящий `Success`» от «код дошёл до конца скоупа, не завершив setup, по
  не-throw пути» — молчаливый auto-`commit()` в последнем случае
  необратимо помечает `Once` как `DONE` без реального выполнения (в отличие
  от `Transaction`, где спонтанный (spurious) commit стоит недорогого
  отменяемого rollback). Остаётся строго линейным.
- **`Body`** (nova-http, D359) и FFI `Sqlite Db`/`Stmt` (examples/ffi) — вне
  периметра std/**, не рассмотрены этой волной (соседние репозитории/non-std
  примеры).

### Связь

D131 · D133 (амендмент выше) · [D180](05-memory.md#d180) Rule 6 (не
применяется к `@cleanup`-типам — «consume obligation в-scope check»
пропускает типы из `cleanup_pure_types`) · [D314](03-syntax.md#d314)
defer-kernel (транспорт переиспользован без изменений интерфейса) ·
[Plan 217](../../docs/plans/217-auto-cleanup.md) (весь) ·
[Plan 217.1](../../docs/plans/217.1-cleanup-resource-rollout.md) (раскатка,
§7 выше) · [Plan 214](../../docs/plans/214-coerce-attribute.md) R12
(эффект-свобода, прецедент для §1) ·
[Plan 216](../../docs/plans/216-consume-enforce-a.md) (ортогонален —
периметр видимости передач владения не меняется, D432 ослабляет только
«непередачу», не «передачу»).

---

## D156. Generic `[T consume]` bound + collection-aware iteration

> **Plan 100.2.** Принято 2026-05-23 (proposed; implementation pending).
> Extends [D133](#d133) на generic-код. Closes silent-leak hole для
> consume-T в generic-функциях.
>
> **АМЕНДМЕНТ 2026-08-04 (решение владельца) — линейность НАСЛЕДУЕТСЯ
> контейнером; два семейства различаются бáундом.** Мотив — дефект реестра
> 221.1 **№325**: линейное значение клалось в коллекцию молча, оставалось
> живым и снаружи, обязательство исчезало вместе с контейнером. Формы
> проверены пробой на компиляторе 2026-08-04, не предположены:
>
> ```nova
> fn Vec[T consume] mut @push(consume v T) -> @              // семейство 1
> fn Vec[T consume Cleanup[E]] @m() -> Option[E] => None     // семейство 2
> ```
>
> 1. **`Vec[T consume]` — контейнер сам линеен.** Если `T` must-consume, то
>    и контейнер must-consume. Потребляется тремя уже существующими
>    способами: обход с изъятием (`for consume`, ниже), передача дальше
>    целиком, возврат. **Проверки «пуст ли контейнер» НЕ вводится**: пустота
>    — свойство времени выполнения и статически неразрешима, а потребление
>    ЗНАЧЕНИЯ компилятор уже отслеживает. Это и есть ответ на вопрос «что
>    происходит при выходе непустого линейного контейнера из области».
> 2. **`Vec[T consume Cleanup[E]]` — контейнер объявляет СВОЮ `@cleanup`**,
>    обходящую элементы, и по [D432](#d432) становится аффинным: забыть
>    можно, компилятор вставит вызов. `Vec[File]` закрывает файлы сам.
>    Параметр `E` вводится САМИМ бáундом — форма `fn[E] Vec[T consume
>    Cleanup[E]]` отвергается (`[E_UNUSED_PREFIX_TYPEVAR]`: переменная в
>    бáунде не считается использованием), канон — короткая форма без
>    префикса.
> 3. **Строка эффектов очистки контейнера в исходнике НЕ пишется** и
>    выводится на инстанциации из настоящей очистки `T`: для `Vec[File]` там
>    `Fs`, для `Vec[int]` пусто. Это не новая сущность, а перенос уже
>    действующего вывода эффектов ([D28](04-effects.md#d28): приватные —
>    выводятся) на generic-инстанс. Эффект-переменная в языке НЕ вводится.
> 4. **Drop-glue не появляется** — [D432 §2](#d432) цел: очистка контейнера
>    есть обычный библиотечный метод, а не магия компилятора.
>
> Почему это стало возможно только сейчас: условная доступность метода по
> бáунду до 2026-08-04 была декорацией — бáунд объявлялся, но на месте
> вызова не проверялся (реестр 221.1 **№303**, закрыт тем же днём). Без
> №303 вся конструкция держалась бы на честном слове.

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
из D72), **включая комбинацию с протокольной цепочкой** (№300/221.1
уточнение; окно p-lang, 2026-08-04): **`[T consume Hash + Equal]`** —
модификатор `consume` ВСЕГДА первым, затем `+`-цепочка протоколов, которым
тип обязан удовлетворять («модификатор перед тем, что описывает», см.
[D445](#d445)). `consume` в саму `+`-цепочку не входит — она перечисляет то,
что тип **реализует**, а линейность не реализуется, она объявляется. Обратный
порядок (`[T Hash + Equal consume]`) остаётся отвергнут — канон один.
Bootstrap-ограничение снято (было: `[T consume + Clone]` — parse error).

```nova
// №300/221.1: consume-bound + протокольная цепочка.
fn dedup_first[T consume Hash + Equal](consume v T) -> T => v
```

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
> - `FnDecl.needs_caps` AST field — удалён (Plan 91.15 Ф.5, `[M-91.10-remove-needs-caps-field]` ✅).
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
capability); (b) унифицировано с regular fn (одна mental model для
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

**Контекст.** [D368](#d368) закрыл silent-fallback в *кодогене* («no
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
- [D368](#d368) — sibling: «no silent fallback» для кодогена (Plan 70).
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

> **Amended (Plan 138 D238+D239, 2026-06-10):** `arr[i]` для user-типов
> (`Vec[T]`, `HashMap[K,V]` и т.д.) теперь через `@index` protocol (D238).
> `[]T` = `Vec[T]` (D239); typed-storage gap закрыт — `[]Option[int]`,
> `[]Record` и другие exotic-element типы получают правильное typed хранение.
> Range-slicing `v[2..5]` для `Vec[T]` — через `@index(Range)` overload.
> Предложение «future language version» из D232 Migration path снято: D239
> фиксирует `[]T` ≡ `Vec[T]` как текущую спецификацию.

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
(см. `[P-str-slice-clamp-vs-panic]` в `docs/dev/simplifications.md`).

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
> 101.5 partial). Plan 101.1 codegen для non-int mono-dispatch —
> marker [M-fn-prefix-int-only-mono] ✅ RESOLVED (Plan 101 Group I, vec_map_int_str fix).
>
> **AMEND (2026-06-14, Plan 153.5):** вложенные generic-ресиверы произвольной глубины
> (`fn[T] [][]T @m` / `fn[T] Vec[Vec[T]] @m` — structural typevar-bind в самый
> внутренний элемент, depth-agnostic) — см. секцию «AMEND … вложенные generic-ресиверы»
> ниже. Разблокировало `@flatten` (D263); закрыло `[M-153.5-flatten-nested-receiver]`.
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
> - vec_map_int_str — T=int U=str cross-type case
>   ✅ RESOLVED (Plan 101 Group I, M-fn-prefix-int-only-mono).
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
   - **С bound (D72):** `fn HashMap[K Hash, V] @from_pairs(...)`.
2. **`fn[T]` префикс** (новое, D145) — для receiver'ов **без carrier
   brackets**: bare T, `[]T`, tuple `(T, U)`, composite без carrier:
   - `fn[T] T @identity() -> T => @` — bare typevar.
   - `fn[T] []T @map[U](f fn(T) -> U) -> []U => ...` — array.
   - `fn[T, U] (T, U) @swap() -> (U, T) => (@1, @0)` — tuple.
   - `fn[T Hash] []T @dedup() -> []T => ...` — bounds через D72.
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
fn[K Hash, V] HashMap[K, V] @method   // ERROR E_DUPLICATE_GENERIC_DECL
// K, V уже декларированы через HashMap[K, V]; используй
// fn HashMap[K Hash, V] @method
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
fn[T Hash] []T @dedup() -> []T => ...
fn[T A + B] []T @method() => ...                    // multi-bound (Plan 101.3)
fn[K Hash, V] (K, V) @key_value() -> (K, V) => @
fn[T From[K], K] T @construct_from(v K) -> T => T.from(v)   // parametric protocol
```

**Bound = protocol-тип (D72) ИЛИ type-set ([D310](#d310-type-set-bounds-plan-1723), Plan 172.3).** Type-set — именованное
множество конкретных типов (`type SignedInts set i8 | i16 | …`), используемое как bound:
`fn[T SignedInts] T.parse(...)`. Композиция type-set ∧ protocol — через тот же `+`
(`[T SignedInts + Hash]`): T ∈ set И реализует protocol, проверки независимы per-member;
не более одного type-set в одном bound-листе (`E_MULTIPLE_TYPE_SETS`). Произвольные
**representation/underlying** bounds (`~int`, structural) — по-прежнему **open question**
[Q-representation-bound](../open-questions.md#q-representation-bound), Plan 102 (future);
D310 закрывает только explicit-member-set, не representation.

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

### AMEND (2026-06-14, Plan 153.5 commit `1c323d0e`): вложенные generic-ресиверы произвольной глубины

`fn[T]`-typevar в receiver-position теперь связывается **структурной унификацией на
ЛЮБОЙ глубине вложенности** — не только на верхнем уровне элемента. Это закрывает дыру,
из-за которой `[][]T @flatten()` (= carrier-форма `Vec[Vec[T]] @flatten()` под
[D239](#d239-t--синтаксический-псевдоним-vect)) не работал: тело должно назвать
**внутренний** `T`, а компилятор биндил его в *непосредственный* элемент.

**Корень (обе формы записи теряли вложенность до фикса):**
- **Carrier** `Vec[Vec[T]]` — ПАРСЕР отвергал вложенный тип в carrier-слоте
  (`parse_generic_decl_params` ждал `parse_ident` на каждый слот → «expected `]`, got
  identifier»).
- **Slice** `[][]T` — ПАРСИЛСЯ, но монорфизатор биндил receiver-typevar `T` в
  *непосредственный* элемент (`Vec[int]`), не во *внутренний* (`int`) — вложенность
  `[][]T` схлопывалась в один `"[]T"`-ресивер → тело строило `out []T == Vec[Vec[int]]`,
  возвращало неверный тип (verified probe RUN-FAIL, mono'd `out` =
  `Nova_Vec____Nova_Vec____nova_int_p`).

**Правило (после AMEND):**
- **Обе формы записи приняты и эквивалентны** под D239: `fn[T] Vec[Vec[T]] @m` ≡
  `fn[T] [][]T @m`. Парсер несёт полный структурированный тип ресивера в
  `Receiver.receiver_ty` (`type_name` его flatten'ит в `"[][]T"` и теряет глубину —
  поэтому нужен отдельный структурный слот).
- **Receiver-typevar биндится в самый внутренний элемент**, рекурсивно (depth-agnostic):
  для `Vec[Vec[T]]`/`[][]T` `T = element-of-element`; для `Vec[Vec[Vec[T]]]`/`[][][]T`
  `T` = element третьего уровня; и так далее. Унификация — структурная (по форме типа),
  не one-level-hardcoded.
- **Свободные typevar'ы collect'ятся рекурсивно** из вложенного carrier-слота для
  проверки `E_UNUSED_PREFIX_TYPEVAR` (typevar объявлен в `fn[T]`, но не упомянут в
  ресивере → ошибка) — собираются из `receiver_ty`, а не из flatten'енного имени.
- **`E_UNDECLARED_TYPEVAR_IN_RECEIVER` сохраняется** для `fn []T @m` / `fn [][]T @m`
  **без** `fn[T]`-префикса (scope-typevar НЕ сидится из `receiver_ty` — это намеренно,
  иначе ошибка маскировалась бы; см. checker-заметку).

```nova
fn[T] [][]T @flatten() -> []T => ...                // T = innermost (depth 2) — РАБОТАЕТ
fn[T] Vec[Vec[T]] @flatten() -> Vec[T] => ...       // carrier-форма, ≡ выше под D239
fn[T] [][][]T @deep_count() -> int => ...           // depth 3 — T = innermost
fn[T] [][]T @first_row() -> []T => ...              // вложенный-типизированный return
```

**Реализация (depth-agnostic, без one-level-hardcoding):**
- **AST** — `Receiver.receiver_ty: Option<TypeRef>` несёт полный структурированный тип
  вложенного ресивера (единственное место, где глубина переживает — `type_name`
  flatten'ит в `"[][]T"`).
- **Parser** — slice `[][]T`: счёт глубины `Array` + спуск до внутреннего `Named` →
  строит `Array(Array(Named T))`. Carrier `Vec[Vec[T]]`: новый разбор принимает
  ВЛОЖЕННЫЙ `parse_type` в слоте (детект `Ident[`) + рекурсивный сбор free-typevars;
  структурные слоты сворачиваются в `receiver_ty`. Free-fn `[T Bound=D]`-разбор не
  тронут.
- **Mono** — переиспользован существующий рекурсивный `infer_type_param_binding` для
  структурного бинда receiver-typevar (Array-арм также снимает mono-форму `Vec____`,
  восстанавливая элемент через `generic_type_instance_info`); override применён на ВСЕХ
  путях, биндящих receiver-typevar (emit-dispatch carrier + `[]T`-sentinel slice +
  call-site return-inference). Depth-aware sentinel-ключи `"[]"*N+"T"` заменили
  hardcoded `"[]T"`. **Flat `[]T` (depth 1) остался byte-identical** (legacy
  `NovaArray_`-путь); override гейтится `receiver_ty_is_nested` — только для реально
  вложенных ресиверов.
- **Checker** — вложенные typevar'ы из `receiver_ty` собираются в `referenced`-множество
  для `E_UNUSED_PREFIX_TYPEVAR`; scope `gs` **НЕ** сидится из `receiver_ty` (сохраняет
  `E_UNDECLARED_TYPEVAR_IN_RECEIVER` — verified, что seed был бы регрессией).

**Cross-cutting заметка.** Это путь, через который идут ВСЕ `[]T`-методы stdlib (slice-
dispatch). Изменение специально гейтнуто на genuinely-nested ресиверы → flat-случай
неизменен. См. [D263 AMEND](10-overloading.md#d263-vec-restructure-ops--оператор---plus--concat)
(`@flatten` использует этот фундамент).

**Известное ортогональное ограничение (pre-existing, вне scope):** slice-форма
`fn[T] [][]T -> []T`, чьё тело **строит** свежий `Vec[T].new()`, упирается в
pre-existing erased-base-body лимит, который ЛОМАЕТ и flat `fn[T] []T` с `Vec[T].new()`
на baseline (`expected struct 'Vec____Nova_T_p'`). Production-flatten — CARRIER-форма
`Vec[Vec[T]] @flatten` (как все stdlib), работает полностью; slice-form nested-receiver
binding доказан отдельно (`@count_all`/`@first_row`).

### AMEND (2026-07-25, Plan 221.1 №88 `[M-structured-receiver-generic-not-enforced]`): энфорс + юзер-carriers + `[]`-алиас в слотах + запрет затенения

Четыре дельты поверх предыдущего AMEND, закрывающие дыры карты владельца
(вопросы `Type[[]T]`/`Type[Vec[T]]`/двусмысленность T, 2026-07-24).

**(i) Call-site энфорс формы (главная звучность).** До этого AMEND метод со
структурным receiver'ом (`fn Vec[Vec[T]] @flatten`, юзер-carrier ниже) вызванный
на НЕ-унифицирующемся ресивере (`Vec[int].flatten()`) не диагностировался НИГДЕ
— либо случайный CC-FAIL глубоко в codegen, либо (для receiver'а без вообще
никаких typevar'ов) **молчание**: verified-проба — `OneBox[int].first()`, где
`@first` объявлен `fn OneBox[Vec[T]] @first() -> T`, проходил `nova check` И
`nova build` без единой диагностики; собранный бинарь на `print(b.first())`
печатал **ничего** (не панику, не мусор — тихий no-op где-то в codegen).
Теперь: на call-site метода со структурным receiver'ом (`Receiver.receiver_ty`
— Plan 153.5/D263 слот) чекер прогоняет **ту же** depth-agnostic структурную
унификацию, что мономорфизатор уже использует для биндинга (`build_recv_subst`
→ `const_fn_trampoline::unify_type`, НЕ shallow `unify_coerce_receiver`
Plan 214.1 — тот намеренно one-level, R4 «без рекурсии вглубь», непригоден для
многоуровневого mismatch). Bindable typevar-имена — объединение
`receiver.generics` (carrier-декларация, покрывает и flat, и (ii) nested-
harvest) И `fn.generics` (`fn[T]`-префикс/method-level — иначе ЛОЖНО отвергло
бы ВСЕ 7 `fn[T] []T @method` методов vec.nv, verified regression class).
Несовпадение → `E_RECV_SHAPE_MISMATCH` («method `X` requires receiver shape
`Y`, got `Z`»). **Единственный кандидат** (по `(receiver-база, имя-метода)`) —
best-effort posture, зеркалит `check_method_call_bounds` (Plan 101.2): ≥2
overload'ов по arity → пропуск (не второе гадание за резолвом overload'а, а
исключительно форма ОДНОЗНАЧНОГО кандидата).

**(ii) Юзер-carriers легальны.** 153.5-механика (harvest + depth-agnostic
структурная унификация) НЕ Vec-специфична — верифицировано: `fn
OneBox[Vec[T]] @first() -> T => @v[0]` (произвольный юзер-тип `OneBox[T]` как
внешний carrier, БЕЗ `fn[T]`-префикса — по доктрине carrier-скобки САМИ есть
декларация, `fn Vec[Vec[T]] @flatten`-идиома) работает без единой правки
codegen/mono — parser уже строил `receiver_ty` depth-agnostic для ЛЮБОГО
именованного carrier'а, не только `Vec`. `fn[T] OneBox[Vec[T]] @first()`
(С префиксом) корректно отвергается `E_DUPLICATE_GENERIC_DECL` — carrier уже
декларирует `T`, префикс дублирует; это ОЖИДАЕМОЕ поведение, не баг. Известное
ортогональное ограничение (verified, вне scope): мономорф юзер-carrier'а
(в отличие от std-шного `Vec[Vec[T]]`) CC-FAIL'ит на НЕ-`int` `T` (напр.
`OneBox[Vec[str]]` → `returning 'Nova_Vec____nova_str *' from a function with
incompatible result type 'nova_str'`) — codegen-гэп для generic-value-типов
с вложенным generic-аргументом, отдельный от 88-объёма (чекер/грамматика).
Также verified отдельно, вне 88-объёма: синтезированный `.new()`-конструктор
для generic value-типа с вложенным `Vec`-аргументом (`OneBox[Vec[int]].new(v:
...)`) резолвится в НЕСВЯЗАННЫЙ C-символ (`Nova_EmbeddedDir_static_new`) —
воспроизводится БЕЗ единого метода на типе; обход — anon-литерал + let-
аннотация (прецедент d277, `OneBox[Vec[int]] = { v: ... }`).

**(iii) `[]`-алиас в carrier-слотах (D239).** Грамматика раньше отвергала
`OneBox[[]T]`/`Vec[[]u8]` («expected identifier, got `[`») — carrier-слот
парсился ТОЛЬКО через `parse_ident`/`Ident[`-nested-detect, слот, начинающийся
с `[`, не имел ветки. Новая ветка в `parse_generic_decl_params_inner`
(`in_carrier_position`) парсит слот, начинающийся с `[`, через `parse_type`
(годится `[]T`, `[][]T`, …), затем **канонизует** результат в `Vec[...]`-форму
(общий хелпер `const_fn_trampoline::canonicalize_array_to_vec`, рекурсивный,
любая глубина) — так и flat, и nested carrier-слоты хранят ОДНУ каноническую
форму, независимо от того, какой синоним D239 автор написал. Тот же хелпер
применяется к call-site'овому инферренному типу receiver'а перед унификацией
(i) — без этого `ro v Vec[[]u8] = ...` (обычная type-annotation, идущая через
ОБЩИЙ парсер типов, не через carrier-слот-канонизацию) породила бы ложный
`E_RECV_SHAPE_MISMATCH` на чисто косметическом расхождении спеллинга.
`OneBox[[]T] @m()` теперь ≡ `OneBox[Vec[T]] @m()` byte-for-byte структурно.

**(iv) Запрет затенения (решение владельца).** Имя в receiver-carrier-скобках,
СОВПАДАЮЩЕЕ с уже объявленным реальным типом ИЛИ примитивом (`int`/`str`/…),
двусмысленно: это опечатка typevar'а, случайно попавшая на имя реального типа,
или намеренная **специализация** конкретным типом? Специализация НЕ
поддерживается (может стать фичей позже, явно) — раньше это молча принималось
(имя не typevar-формы — короткое uppercase, `ident_is_typevar` — не харвестится
как generic, остаётся литеральной ссылкой на тип) И **не проверялось вообще**
— метод регистрировался под `method_table[base]` и (до (i)) диспатчился на
ЛЮБОЙ ресивер той же базы. Проба владельца: `type Wid` + `fn Vec[Vec[Wid]]
@wsum` — раньше молча компилировался (мусор), теперь `E_RECV_GENERIC_SHADOWS_TYPE`
(«receiver generic param `Wid` затеняет объявленный тип `Wid` — переименуй
параметр»). Примитивы отвергаются той же нормой (`fn OneBox[Vec[int]] @m()` —
тоже `E_RECV_GENERIC_SHADOWS_TYPE`).

**Область (iv) — ТОЛЬКО nested-слоты (depth ≥ 2 от корня receiver'а),
намеренно НЕ «любая глубина».** Verified regression при первой (broader)
реализации: `std/src/encoding/serde/serde.nv` — `fn HashMap[str, V Serialize]
@serialize[...]` — **прямой (depth-1) carrier-слот** `str`, намеренная
частичная конкретная специализация ключа (K = str буквально, V остаётся
generic) — production-код, давно и молча так работающий (flat bare-ident
carrier-слот харвестится **безусловно**, без `ident_is_typevar`-гейта, парсер
`parse_generic_decl_params_inner`, non-nested ветка — отдельная, СТАРШАЯ,
известная permissive-механика, НЕ предмет этого окна). depth-1 remains
permissive БЕЗ проверки (out-of-scope находка, не тронуто; вероятно свой
маркер — см. отчёт окна). Аналогично `[]`-slice-sugar top-level ресивер
(`fn []u8 @to_str_unchecked`, std/runtime/string) исключён из (iv) целиком —
отдельная, уже звучная (свои `E_UNDECLARED_TYPEVAR_IN_RECEIVER`/
`E_BARE_TYPEVAR_NEEDS_PREFIX` гейты) механика, не carrier-форма.

**№91 `[M-nested-vec-concrete-extension-unresolved]` — честный вердикт, НЕ
закрыт полностью.** Slice-sugar top-level форма (`fn [][]u8 @m()`, конкретный
элемент `u8`) резолвится и работает end-to-end (build+run verified) — но эта
форма НЕ трогалась этим окном (отдельный, уже звучный путь), похоже была
рабочей и раньше. Carrier-форма (`fn Vec[[]u8] @m()` ≡ `Vec[Vec[u8]]`)
**теперь честно отвергается** правилом (iv) (`u8` — примитив, nested, depth
2) — специализация конкретным примитивом не поддерживается по явному решению
владельца («Примитивы… тоже отвергать»), так что №91's carrier-спеллинг НЕ
становится вызываемым, а получает громкую, осмысленную ошибку взамен прежнего
непрозрачного `E7320`. Если владелец хочет carrier-спеллинг ТОЖЕ рабочим —
это отдельная фича (явная поддержка concrete-специализации), не входит в
текущую доктрину «специализация НЕ поддерживается».

**`#coerce`-заглушка (`E_COERCE_GENERIC_PATTERN_UNSUPPORTED`, p2141f) —
СОХРАНЕНА, не снята.** Причина: `#coerce`'s собственный унификатор
(`unify_coerce_receiver`, Plan 214.1) остаётся ПРИНЦИПИАЛЬНО shallow (R4 «без
рекурсии вглубь» — design-инвариант того окна, не забытая недоделка) и НЕ
переиспользует (i)'s глубокий `unify_type` — эти два окна намеренно не
слиты. Снятие заглушки потребовало бы ОТДЕЛЬНОГО решения — либо углубить
`#coerce`'s унификатор (рискует R4-инвариантами того плана), либо провести
`#coerce`-путь через (i)'s механизм (архитектурная работа вне 88-объёма).
Regression-verified: `spec_tests/conformance/neg/
d429_1_generic_coerce_structured_receiver_neg.nv` по-прежнему падает тем же
кодом байт-в-байт.

**Гейт:** `nova check std/src` 142/27/1040 (байт-идентично, без
`NOVA_STD_PATH`); `nova test std/src/collections/vec` — зелёный (restructure/
flatten живут там, один агрегированный CU-отчёт per test-conventions.md);
новые фикстуры `spec_tests/conformance/standalone/p88_structured_receiver_pos.nv`
+ `spec_tests/conformance/neg/p88_recv_shape_mismatch_neg.nv` +
`spec_tests/conformance/neg/p88_recv_generic_shadows_type_neg.nv` +
`spec_tests/conformance/neg/p88_recv_generic_shadows_primitive_neg.nv` —
`nova test` дословно PASS на все четыре.

### Backward-compat

- **100% преserve** для existing `fn Option[T] @map[U]`, `fn HashMap[K, V] @keys`,
  `fn Result[T, E] @ok`, `fn HashMap[K Hash, V] @method` — D145
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
2. **Bound syntax без двоеточия** — `[T Hash]` (D72) — параллель
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
    integration `fn[T Hash]`.
  - [101.3](../../docs/plans/101.3-multi-bound.md) — multi-bound
    `[T A + B]`, closes Q-multi-bound.
  - [101.4](../../docs/plans/101.4-protocol-composition.md) — protocol
    embedding `use Foo`, closes D53 open question.
  - [101.5](../../docs/plans/101.5-stdlib-audit-close.md) — stdlib
    audit + LSP + close.
- [Q-representation-bound](../open-questions.md#q-representation-bound)
  — concrete-type bounds (newtype/embed-aware), Plan 102 future.

---

## D372. Canonical `.new()` constructors (convention)

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
(builtin). ~~Retracted~~ — см. амендмент ниже (2026-07-06): `with_capacity`
удалён, ёмкость теперь свойство `cap`.

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
mut buf = []u8.new(cap: 1024)     // pre-alloc: ровно 1024 слота (D372-amend2)

// User type — explicit:
type User { name str, email str, is_admin bool }
fn User.new(name str, email str) -> Self => { name, email, is_admin: false }
fn User.guest() -> Self => { name: "guest", email: "", is_admin: false }
```

### Связь

- [D26](#d26-базовая-stdlib-и-prelude) — prelude auto-availability.
- [D66](#d66-self-универсальный--ссылка-на-обобщающий-тип-в-методах-effects-protocols) — `Self` в return type.
- [D131](03-syntax.md#d131-consume-types-и-fluent-api) — consume / fluent.
- [D182](#d182-self-в-return-type-static-methods--required-form-для-parametric-types) — `Self` requirement.
- [Plan 91.7](../../docs/plans/91.7-array-methods-and-default-new.md).

> **Амендмент (vec-sweep, 2026-07-06): `with_capacity` удалён, ёмкость —
> свойство `cap`.** `with_capacity(n) -> Self` как отдельный static-конструктор
> УДАЛЁН для `Vec[T]`/`[]T`, `HashMap[K,V]`, `Set[T]`, `StringBuilder`,
> `WriteBuffer`, `Queue[T]` (везде, где он существовал). Ёмкость теперь
> read/write СВОЙСТВО по D117 (arity-overload): `X.cap() -> int` (getter) /
> `X.mut cap(n int) -> @` (setter). Конструкция с pre-allocated capacity —
> `X.new().cap(n)` вместо бывшего `X.with_capacity(n)`.
>
> Semantics по типу:
> - `Vec[T]`/`[]T`: `cap(n)` — ТОЧНАЯ ёмкость (`requires n >= @len()`, без
>   округления), полностью взаимозаменяема с бывшим `with_capacity(n)`.
> - `HashMap[K,V]`/`Set[T]`: `cap(n)` гарантирует МИНИМУМ n вставок без
>   rehash (entry-count → bucket-count, как раньше), но только РАСТЁТ на
>   непустой карте; на СВЕЖЕЙ (`@_count == 0`) карте допускает и уменьшение
>   до точного целевого bucket-count — так `new().cap(n)` воспроизводит
>   старое `with_capacity(n)` побайтово.
> - `WriteBuffer`/`StringBuilder`: `cap(n)` делегирует к внутреннему `[]u8`'s
>   собственному точному `cap(n)`.
> - `Queue[T]`: `cap(n)` резервирует ёмкость НА ОБОИХ backing-массивах
>   (`_inbox`/`_outbox`) одновременно.
>
> **from_raw_parts → перегрузка `new` (амендмент, vec-sweep 2026-07-06).**
> `Vec[T].from_raw_parts(ptr, len, cap) -> Self` переименован в
> `Vec[T].new(ptr *T, len int, cap int) -> Self` — арность-перегрузка
> статического `new` (0-арг = пустой Vec, 3-арг = raw components). Контракт
> unsafe-обязательства на call site — БЕЗ ИЗМЕНЕНИЙ (см. текст на месте
> декларации, `std/collections/vec/core.nv`). `@into_raw` (обратная
> операция) не переименован.
>
> **Известные компиляторные гэпы, обнаруженные при миграции:**
> - `[M-vec-spelling-array-value-position-cap-collision]` — цепочка
>   `[]T.new().cap(n)` В ОДНОМ выражении может мис-дispatch'иться по
>   NAME+ARITY на «cap»-метод НЕСВЯЗАННОГO ко-компилируемого типа (не только
>   erased-array случай D239-амендмента выше — воспроизведено и для
>   произвольных типов). Обход: биндинг в explicitly-typed локаль ПЕРЕД
>   вызовом `.cap(n)` отдельной инструкцией.
> - `[M-vec-spelling-consume-chain-cap-collision]` — `consume x = T.new().M(...)`
>   (ЛЮБОЙ 2-звенный chain, забинженный через `consume`, для `T consume`-типа)
>   ломает D133 consume-tracking для ВСЕХ ОСТАЛЬНЫХ `consume ... = T.new()`
>   site'ов в той же compile unit (воспроизведено вне зависимости от имени
>   второго метода — не специфично для `cap`). Обход: `consume x = T.new()`
>   (один вызов) + `x.M(...)` отдельной инструкцией.
> - `[M-vec-spelling-maplit-desugar-cap-ice]` — попытка добавить `.cap(n)`
>   pre-sizing statement в desugar `[k:v]`-map-литерала (после `HashMap.new()`,
>   до `insert_new`-цикла) роняла компилятор (ICE: "method call `.insert_new`
>   return type unknown") при компиляции нескольких файлов вместе в одной CU.
>   Обход (принят): pre-sizing убран из desugar'а полностью — map-литералы
>   строятся через голый `.new()` + amortized growth в insert_new-цикле
>   (perf-only регрессия, corretness не затронута).

> **Амендмент 2 — `Vec[T].new(cap int = 0)` точный pre-alloc конструктор (решение владельца 2026-07-12).**
> Каноничная предвыделяющая форма — **default-аргумент**, НЕ chain и НЕ arity-overload:
> ```nova
> mut v = Vec[T].new()           // cap = 0, пусто, без аллокации (default)
> mut v = Vec[T].new(cap: 1024)  // ТОЧНО 1024 слота, len = 0 (именованно — предпочтительно)
> mut v = Vec[T].new(1024)       // то же, позиционно (легально)
> ```
> - **`new(cap)` и `cap(n)` = ТОЧНАЯ ёмкость** (без округления; `new(cap)` ≡ `new().cap(cap)`, но одной
>   аллокацией и одним вызовом). Предпочтительное написание pre-alloc; chain `.new().cap(n)` остаётся легальным.
> - **Контраст с `reserve(n)`:** `@reserve(additional)` — АМОРТИЗИРОВАННЫЙ рост, округляет ёмкость ВВЕРХ до
>   степени 2 (8→16→32…) ради O(1)-amortized push; `new(cap)`/`cap(n)` — ровно `cap`, без округления. Три
>   намерения: `new(cap)` = «дай ровно столько с рождения», `cap(n)` = «переустанови ровно на n», `reserve(n)`
>   = «место ещё под n, можно с запасом».
> - **Почему default-arg, а НЕ overload:** одна функция `new(cap int = 0)` заменяет 0-арг `new()` — НЕ создаёт
>   набор перегрузок → не триггерит `[M-vec-new-static-arity-overload]` (см. поправку ниже). Применимо к типам
>   с точной ёмкостью (`Vec`/`[]T`; `HashMap`/`Set`/`WriteBuffer`/`StringBuilder`/`Queue` — по мере миграции,
>   семантика `cap(n)` того типа). План внедрения — **Plan 200 (зонтичный std-improvements)**. **КОД внедрён
> 2026-07-12** (`std/collections/vec/core.nv`, `Vec.new(cap int = 0)`). Компиляторный дефект
> `[M-vec-new-cap-default-arg-backfill]` (default-arg backfill на generic-static ctor не покрывал
> `Type[Args].method(...)`/`[]T.method(...)` call-сайты) — ЗАКРЫТ в `callnorm.rs` (`try_normalize_call`
> classify-match расширен на turbofish/`__array`-Path static-receiver формы) + 3 hand-formatted ctor-call
> сайта в `emit_c.rs`. **НЕ тот же класс**, что `[M-vec-new-static-arity-overload]` (arity-overload
> cross-wiring) — см. поправку ниже: тот дефект ЗАКРЫТ отдельно, вне зоны Plan 196.2.
>
> **ПОПРАВКА 2 к Амендменту 1 (from_raw_parts → new-overload): ЗАКРЫТО 2026-07-12 (форс-фикс, Plan 200 П5).**
> Складывание `from_raw_parts` в 3-арг перегрузку `Vec[T].new(ptr,len,cap)` **состоялось в коде**
> (`std/collections/vec/core.nv`): `fn Vec[T].new(ptr *mut T, len int, cap int) -> Self requires len >= 0 &&
> cap >= len => { data: ptr, len, cap }` — рядом с `new(cap int = 0)`. Дефект `[M-vec-new-static-arity-overload]`
> — **ЗАКРЫТ, вне 196-зоны** (не `infer_call_ret_c`/W2, как предполагалось ранее — тот план остаётся про
> ДРУГОЙ баг-класс, PRE-mono class-C резолв). Корень был в ДВУХ co-located name-only (arity-blind) overload-
> резолвах, оба «первый-по-имени», игнорирующие арность: (1) `compiler-codegen/src/callnorm.rs`
> (`Sigs::static_methods` — default-arg backfill раньше ФИЛЬТРОВАЛ прочь любой `(type,method)` с >1
> сигнатурой, т.е. просто НЕ бэкфиллил default для overloaded ctor; фикс — хранить ВСЕ overload'ы +
> `pick_static_params` дизамбигуирует по `bind_call_args`-совместимости на каждом call-site); (2)
> `compiler-codegen/src/codegen/emit_c.rs` — ветка «1b» (turbofish static-ctor call, `Type[Args].method(...)`,
> ~emit_call строка 32577) резолвила `generic_type_methods[base].find(name)` первым совпадением по имени,
> тогда как соседняя ветка «5b» (instance-method generic dispatch) уже имела арность/param-type дизамбигуацию
> (`[M-138.2-generic-method-overload-mono]`, 2026-06); фикс — та же схема (арность → param-C-type →
> `resolved_callees`-span чекера) + per-overload `__<paramtype>` суффикс у mono-имени, портированные в ветку
> «1b». Гейты: `vec_of_empty_panic` neg-тест зелёный, `nova test --full std/collections/vec` без cross-wiring,
> `nova test --full std/collections` (14/14) + `std/checksums`+`std/crypto` (используют `str.@bytes()` через
> folded `new`) зелёные, conformance single-CU 95/0. Целевая сигнатура (сложена): см. выше.

---

## D181. Array methods — `-> @` fluent mut chain + slice syntax

**Статус:** active (Plan 91.7, 2026-05-28).

> **Амендмент-упрощение (Plan 184, 2026-07-06).** Тип `-> @` больше не выводится эвристикой
> «heap-алиас / value-копия-с-распадом» (исходные D326-R7/R8: RETURN-оракул D246 +
> escape-decay D228). Теперь `-> @` имеет **конкретный тип** по категории `Self` (таблица Р7
> ревизии [D326-Plan184](#ревизия-d326-plan-184-ref-t--ограниченный-тип)): у стекового (value)
> типа `-> @` = `-> ref Self`; у кучевого (heap) — `-> Self`. Эвристики bind-site заменены
> типами; поведение fluent-цепочек (`a.push(1).push(2)`) не меняется.

### `-> @` для всех mut-методов `[]T`

Все мутирующие методы массива возвращают `@` (receiver pointer)
для fluent chain (D131):

| Метод | Сигнатура |
|---|---|
| `@push(v T)` | `-> @` |
| `@reserve(extra int)` | `-> @` |
| `@truncate(n int)` | `-> @` |
| `@fill(v T)` | `-> @` |
| `@copy_from(src ro []T)` | `-> @` |
| `@extend_from(src ro []T)` | `-> @` |
| `@insert_from(i int, src ro []T)` | `-> @` |
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
- [D372](#d372-canonical-new-constructors-convention) — `.new()` convention.
- [Plan 91.7](../../docs/plans/91.7-array-methods-and-default-new.md).

---

## D183. Canonical comparison protocols + default method bodies (Plan 91.8a)

**Статус:** active (Plan 91.8a, 2026-05-29).

### Канонические протоколы (renames)

| Было | Стало | Файл |
|---|---|---|
| `Iter[T]` | `Iterable[T]` → `Next[T]` + `Iter[I]` (Plan 138 D241+D242) | `std/prelude/collections.nv` |
| `Display` | `Display` | `std/prelude/protocols.nv` |
| `Equal.eq(other Self) -> bool` | `Equal.equals(other Self) -> bool` | `std/prelude/protocols.nv` |
| `Compare.cmp(other Self) -> Ordering` | `Compare.compare(other Self) -> int` | `std/prelude/protocols.nv` |
| `Hash.hash() -> u64` | unchanged | `std/prelude/protocols.nv` |

**Rationale renames:**
- **`-able` suffix convention** — unified naming (Iterable/Equal/Compare/Hash/Display).
- **`Compare.compare -> int`** — единый стиль с `str.compare()` (D178) и C `memcmp`/`strcmp`. `Ordering` sum-type удалён.
- **`Equal.equals`** — явнее чем `eq` (Java convention).
- **`Display` → `Display`** — действие через `-able`, не имя-noun.

### Compare embeds Equal

```nova
export type Equal protocol {
    equals(other Self) -> bool
}

export type Compare protocol {
    use Equal
    compare(other Self) -> int
    equals(other Self) -> bool => @compare(other) == 0    // default body
}
```

`use Equal` (D39 embed) делает каждый Compare также Equal.
Локальная декларация `equals` в Compare с default body **overrides**
embedded default — implementer пишет только `@compare`, `@equal`
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
type Compare protocol {
    use Equal
    compare(other Self) -> int                              // abstract
    equals(other Self) -> bool => @compare(other) == 0      // default
}

type MyDate { y int, m int, d int }
fn MyDate @compare(other MyDate) -> int { ... }
// @equal НЕ объявлен — используется default из Compare.

// Override для perf:
type FastHashed { hash_cache u64, ... }
fn FastHashed @compare(other FastHashed) -> int { ... }
fn FastHashed @equal(other FastHashed) -> bool {
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
- **`check_protocol_embeds`** (`compiler-codegen/src/types/mod.rs`): local override embedded methods разрешён — locally declared метод в protocol с тем же именем что embedded не считается duplicate. Используется для `Compare.equals` overrides embedded `Equal.equals` default.
- **Codegen synthesis для defaults**: followup `[M-91.8a.2-default-codegen]`. Сейчас implementer пишет default-method explicitly для compatibility (как boilerplate `equals(o) => @compare(o) == 0`).

### Известные ограничения / followups

- **Codegen synthesis (`[M-91.8a.2-default-codegen]`):** type T который имеет `@compare` но не `@equal` пока компилируется только если `@equal` объявлен явно. Eager synthesis из default body — отдельный codegen pass.
- **Operator dispatch (D363, Plan 91.8b):** `==` всё ещё dispatches к `@eq` (D46). Renaming `@eq` → `@equal` в operator dispatch — задача Plan 91.8b. До 91.8b implementer пишет оба: `@equal` (protocol) + `@eq` (operator).
- **Structural `==` для mono'd generic-sum + Result ✅ (Plan 153.3, commit `1cc82de5`):** дефолтное
  структурное `==` (tag + payload, без user `@equal`/`@compare`) теперь покрывает
  **мономорфизированные generic-sum** (`Foo[int].A(1) == A(1)`) и **Result** (`NovaRes_*`). Раньше
  оба тихо деградировали в pointer-identity: legacy `sum_schemas` keyed generic-именем (mono'd-ключ
  отсутствовал → `emit_field_eq` промахивался мимо schema), а Result-`NovaRes_*` (спец-ABI с
  typed-error-полями) не матчил `Nova_`-sum-тест. Фикс: `reconstruct_mono_sum_schema`
  (substituted-схема вариантов из generic-шаблона + recorded type-args; tag-префикс = полный
  `Nova_<mono>`) + `NovaRes_`-ветка в `emit_field_eq`/`==`-операторе через `novares_ok_err`.
  **`result == Ok(x)` / `== Err(x)` ✅** (`[M-153-result-eq-literal-expected-type]` RESOLVED): голый
  `Ok/Err`-литерал с non-default-E (напр. `binary_search`→`Result[int,int]`) дефолтил `E=str` и не
  совпадал по типу с LHS → CC-FAIL. Фикс codegen-local: в `==`-NovaRes_-ветке, если типы операндов
  расходятся и одна сторона — голый result-ctor, она переэмитится под concrete `NovaRes_<n>` другой
  (`reemit_result_variant_as`). General expected-type propagation для overload-резолва (`@into`)
  остаётся в `Q-overload-result-type`.
- **Generic sort/min/max (D373, ex-D185, Plan 91.8c):** generic `fn[T Compare]` array methods — реализовано, см. [D373](#d373--generic-array-api-sortminmaxbinary_search--_by-variants-plan-918c-2026-06-17).
- **D328 — Value-record `==` СТРУКТУРНОЕ (Plan 172.4 Ф.2, 2026-06-28):** value-record
  (`type P value {…}` — C-репрезентация `NovaValue_<Name>` by-value, D228/D277/D290) сравнивается
  **структурно** (field-by-field, как sum/heap-record), а **НЕ** сырым C-`==` на struct (была
  acceptance-CC-FAIL «invalid operands to binary expression» — C не имеет struct-`==`). Обоснование:
  value-record — **значение** (нет heap-идентичности); равенство = по значению полей. Маршрутизируется
  через **ЕДИНЫЙ** `emit_field_eq`-диспетчер (§0 «один источник per-type операций»): добавлен
  `NovaValue_`-арм (by-VALUE доступ `(l).field`, не `(*l)->field`) + top-level-`==`/`!=`-роутинг в тот же
  диспетчер; user-`@equal` (если объявлен) приоритетен, иначе структурная рекурсия по `record_schemas`.
  Вложенные value-record-поля рекурсируют тем же армом. **heap-record `==`** (`type P {…}`, `Nova_P*`)
  — **ОТДЕЛЬНАЯ ось**: сейчас reference-eq; структурить ли — open (Plan 172.4 Ф.2 дизайн-вопрос, НЕ
  решается этим блоком — value-records однозначно структурны, heap-records обсуждаемо). Арифметика на
  value-record (`@plus`/…) — отдельный value-ABI концерн (Plan 172.4 Ф.3), не этот блок.

### Связь

- [D26](#d26-базовая-stdlib-и-prelude) — prelude auto-availability.
- [D39](#d39-embed-и-delegation-use-name-type-alias-обязателен) — `use` embed.
- [D58](#d58-protocol-structural-typing) — structural typing.
- [D72](#d72-generic-bounds-через-t-protocol--protocol-как-тип) — bounds.
- [D109](#d109-Equal--Hash-split-policy) — split policy (Hash не embeds Equal; Compare embeds Equal в D183).
- [D178](08-runtime.md#d178-str-api-cleanup-и-расширения--plan-91-ф26) — `str.compare -> int`.
- [Plan 91.8a](../../docs/plans/91.8a-protocol-canon-renames.md) — implementation.

---

## D183 amendment — Plan 91.8a.2 part 1: protocols refactor (orthogonal) + Self в param

**Статус:** active (Plan 91.8a.2 part 1, 2026-05-29).

### Refactor: orthogonal protocols (canonical coercion form)

**Было (91.8a part 1):**
```nova
type Equal protocol {
    equals(other Self) -> bool
}
type Compare protocol {
    use Equal
    compare(other Self) -> int
    equals(other Self) -> bool => @compare(other) == 0   // override of embedded default
}
```

**Стало (91.8a.2 part 1) — canonical:**
```nova
type Equal protocol {
    equals(other Self) -> bool {
        ro cmp Compare = @                  // coercion-style (explicit dependency)
        cmp.compare(other) == 0
    }
}
type Compare protocol {
    compare(other Self) -> int
}
```

**Rationale:**
- **Orthogonal protocols** — каждый stand-alone, без embed-зависимости.
- **Coercion canonical (Q6 decision):** explicit cross-protocol dependency
  visible при чтении декларации; codegen devirtualizes к direct call когда
  тип known statically (zero runtime cost).
- **Conditional default:** T satisfies Equal если has @equal explicit
  ИЛИ satisfies Compare (default body synth via @compare). Type только
  Equal (Vector3, Complex, etc.) пишет @equal явно — coercion fails
  potential потому что @compare отсутствует.
- **Direct form `=> @compare(other) == 0` тоже валидна** — terser; same C
  output after devirtualization. Coercion form preferred в stdlib для
  documentation.

### Display.fmt default body

```nova
type Display protocol {
    fmt(sb StringBuilder) {
        sb.append(str.from(@))
    }
}
```

- Primitives — works via primitive `Nova_int_to_str` etc.
- User types — implementer пишет @display явно (perf) OR provides
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
   - Bound contexts (`[T Equal]` etc.) — synth default body для типов
     которые satisfy abstract methods
   - Protocol coercion (`let x Equal = m`)
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

> ⚠️ См. [D229](#d229-Debug-protocol--format-spec-expr) — Debug sibling protocol с distinct debug semantics (diagnostic vs user-facing display); `${expr:?}` syntax routes к Debug.@debug vs bare `${expr}` к Display.@display (Plan 91.14, 2026-06-05).

---

## D186 — `#impl(P1 + P2 + ...)` opt-in annotation для protocols

**Когда:** 2026-05-29 (Plan 91.9).
**Plan:** [91.9-impl-annotation.md](../../docs/plans/91.9-impl-annotation.md).
**Зависит от:** [D58](#d58-protocols-structural-typing) (structural protocols),
[D72](#d72-bounds) (generic bounds), [D183](#d183-canonical-protocols)
(canonical protocols Equal/Compare/Display + default body).

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
| Interpolation `"${u}"` | ✅ да | Ambient — Display.fmt synthesis |
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
#impl(Equal + Compare + Display)
type Coin { value int }

fn Coin @compare(other Self) -> int => ...
fn str.from(c Coin) -> str => ...
// equals auto-derived через Equal.equals default (uses @compare)
// fmt auto-derived через Display.fmt default (uses str.from)
```

`+` separator consistent с multi-bound `[T A + B + C]` ([D72](#d72), Plan 101.3).

Order arbitrary: `#impl(A + B)` ≡ `#impl(B + A)`.

Multiple `#impl` annotations не разрешены — single annotation with `+`.

#### Position

`#impl(...)` ставится **перед** `type T` (рядом с `#stable`, `#from_fields`):

```nova
#stable(since = "0.1")
#impl(Hash + Equal)
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

> ⚠️ **D186 AMENDED by Plan 108.4 (2026-06-09)** — `#impl(P)` annotation now
> checks **receiver_mut** in addition to method signature. If the protocol declares
> `mut @method()` and the implementing type declares `fn T @method()` (ro receiver),
> type-checker emits `E_PROTO_IMPL_RO_FOR_MUT` at the type-declaration site (where
> `#impl(P)` appears). The 4 new error codes are:
> `E_PROTO_IMPL_RO_FOR_MUT`, `E_PROTO_IMPL_MUT_FOR_RO`,
> `E_PROTO_IMPL_MUT_FOR_CONSUME`, `E_PROTO_IMPL_CONSUME_FOR_MUT`.
> See [D209](04-effects.md#d209--protocol-method--syntax--receiver-mutability-plan-1084-2026-06-09).

> ⚠️ **D186 AMENDED by Plan 221.1 (2026-07-21)
> [M-interp-numeric-fallback-silent-garbage]** — closes an ENFORCEMENT GAP in
> the "Gate semantics" table above: `Interpolation "${u}" | ✅ да` was already
> normative (interpolation requires `#impl(P)` same as bare call), but the
> codegen path for the built-in canonical `Display` protocol specifically
> (`emit_c.rs`'s bare-`${x}` lowering) had NO corresponding compile-time
> check — a user record/sum/namedtuple/newtype interpolated bare (`"${p}"`,
> not `"${p:?}"`) WITHOUT `#impl(Display)` compiled silently and reached a
> LAST-RESORT numeric-cast fallback (`nova_int_to_str((nova_int)(v))`,
> emit_c.rs ~42903) — printing the value's heap address as a decimal
> integer instead of erroring per the gate this section already mandates.
>
> **Fix:** new type-checker diagnostic `E_INTERP_NO_DISPLAY`
> (`types/mod.rs::check_interp_no_display`, called from the `f1_expr_inner`
> `ExprKind::InterpolatedStr` arm) fires when a bare-Display interpolated
> expression's static type is a **non-generic** `Record`/`Sum`/`NamedTuple`/
> `Newtype` declared type that has neither (a) an explicit `@display`
> method, (b) a gate-satisfied `#impl(Display)` auto-derive synthesis
> (already registered in the checker's `synth_methods` overlay before this
> pass runs — same predicate `find_method_decl(T, "display")` covers both),
> nor (c) the pre-existing D410 `str.from(T)` / `T.to_str()` fallback route.
> rustc precedent: `Display` is never auto-derived; a missing impl is a
> compile error, not a best-effort runtime fallback.
>
> **Scope — deliberately narrow** (avoids overreach into adjacent, already-
> handled or mono-time-only concerns):
> - generic type-parameters in scope (`fn f[T](x T)`) — an UNBOUNDED `T`
>   has no contract to check pre-monomorphization (D72: structural at use);
>   a BOUNDED `[T Display]` is decided by the checker from the bound itself —
>   see D464 (amended 2026-08-16: the earlier wording «bound-satisfiability
>   is a mono-time concern» over-generalised the unbounded case);
> - generic (parametrized) declared types — `Vec[T]`, builtin
>   `Option`/`Result`, a user `Box[T]` — routed via
>   `try_generic_mono_interp_dispatch` / the Option-Result `DeclaredBody`
>   special-case in `emit_c.rs`, neither visible to this pre-mono checker
>   pass;
> - typed pointers (`&v`, `*p`, `e as *T`) — separately covered by
>   `E_PTR_NO_DISPLAY_USE_DEBUG_STR` (Plan 91.14/118, D216 §15); `*T` Debug
>   auto-derive remains a distinct open item
>   (`[M-91.14-ptr-auto-derive]`), untouched by this amendment;
> - `${x:?}` (Debug format-spec) is handled by a **SIBLING** diagnostic,
>   `E_DEBUG_PRINTABLE_NOT_IMPLEMENTED` (`check_interp_no_debug`) — see the
>   D229 amendment below. ⚠️ **CORRECTION (same-day follow-up, coordinator
>   repro):** an earlier revision of this bullet claimed Debug synthesis is
>   unconditional (`gate_on_impl=false`) and therefore always available with
>   no `#impl` at all — that claim was **WRONG**, verified false by a
>   standalone runtime repro (`println("${d:?}")` on a plain record with
>   ZERO `#impl` annotations printed a raw heap address on both Windows and
>   Linux). See D229 amendment for the actual mechanism and why the
>   `gate_on_impl=false` flag never accomplishes what its comment claims for
>   Debug specifically.
>
> Fixture: `spec_tests/conformance/neg/d186_interp_no_display_neg.nv` (pin),
> `spec_tests/conformance/d186_interp_no_display_pos.nv` (both legitimate
> escapes: explicit `#impl(Display)`, and `${x:?}` with no `#impl` at all).
> One real pre-existing occurrence found + fixed in std during the
> pre-enforcement audit: `std/src/data/sql.nv`'s `SqlValue @expect_*`
> error-message interpolations (`"expected I, got ${@}"`) — `SqlValue` (a
> closed sum, no `#impl(Display)`) was silently printing its own heap
> address in `DbError.Constraint` messages; changed to `${@:?}` (Debug).

---

## D200. Associated constants — `const Type.NAME` (вне тела типа)

> **Plan 114.4 Ф.2** (extracted from Plan 114 Ф.10 safety hatch).
> **Status:** 🆕 draft (финализируется в Ф.4).

> **AMEND (решение владельца 2026-07-23): каноническая форма — ВНЕ тела типа,
> `const Type.NAME <Тип> = <значение>`.** In-body форма (`const` внутри
> `type X { … }` body — весь исходный текст блока ниже) **РЕТРАКТИРОВАНА** до
> финализации. Причина: in-body ставила статики В ТЕЛО, вперемешку с полями
> инстанса, тогда как ВСЁ остальное, привязанное к типу, в Nova объявляется ВНЕ
> тела через квалификатор `Type.` — конструкторы `fn Type.new(…)`, методы
> `fn Type @name(…)`. Out-of-body const восстанавливает симметрию (**тело типа =
> ТОЛЬКО layout инстанса**; всё с квалификатором `Type.` — снаружи) и ближе к
> rustc-эталону (`impl` держит консты+методы отдельно от полей структуры; у Nova
> нет `impl` → свободностоящая `const Type.X`, как свободностоящая `fn Type.x`).
> «Одна дверь»: остаётся ОДНА форма — out-of-body. Миграция **0 сайтов**
> (in-body формой никто не пользовался: греп std/nova-http/examples = 0).
>
> **Каноническая запись:**
> ```nova
> type Config value { name str, timeout Duration }   // тело = ТОЛЬКО поля инстанса
> const Config.VERSION int = 2                        // ← статик, ВНЕ тела
> const Config.MAX_PEERS int = 1024
> export const Config.PROTOCOL str = "v2"             // export — cross-module
>
> Config.VERSION                                      // ✓ 2 (namespace-доступ)
> ```
> Грамматика = обычный module-const `const <имя> [<Тип>] = <значение>`, где `<имя>`
> квалифицировано типом: `const Config.VERSION int = 2`.
>
> **УТОЧНЕНИЕ 2026-08-29 — `<Тип>` НЕОБЯЗАТЕЛЕН, и дом этого правила НЕ ЗДЕСЬ.**
> Раньше эта строка цитировала грамматику только в типизированном виде
> (`const <имя> <Тип> = <значение>`), потому что у АССОЦИИРОВАННЫХ констант тип
> в примерах стоит везде. Читалось это как «тип обязателен», и так его прочло
> окно 274, не найдя отдельного D-блока на голую форму и сочтя это дырой в спеке.
> Дыры нет: дом `const` — **[D184](03-syntax.md#d184)**, и голая форма записана
> там нормативно («`const X = expr` принимает только constexpr-eligible RHS»),
> с примерами `const MAX_PAYLOAD = 4096`, `const TIMEOUT_SEC = 60 * 5`,
> `const GREETING = "hello"`; обе формы стоят там рядом — `const ORIGIN Point =
> { … }` с типом и `const COMPUTED = make_point(7, 14)` без. Замер на день
> уточнения: в `std/src` голых `const` — 157, типизированных — 61, то есть
> основная форма именно голая. Компилятор принимает обе (проверено сборкой).
> Здесь исправлена ЦИТАТА, а не правило: правило не переезжает, иначе у `const`
> станет два дома и они разойдутся на первой правке. Sum-type и generic —
> так же (`const Status.VERSION int = 2`, `const Box.TAG int = 0`;
> T-dependent — `const Box[int].SIZE …` синтаксис уточнить на Ф.4).
>
> **Семантика — БЕЗ ИЗМЕНЕНИЙ** (меняется только место объявления): §Семантика
> ниже целиком в силе — zero-storage в инстансе, top-level C-symbol
> `Type_NAME` в `.rodata`, namespace-only доступ (`instance.NAME` →
> `E_CONST_INSTANCE_ACCESS`), strict-constexpr RHS, SCREAMING_SNAKE-lint.
> **Парсер:** `[M-assoc-const-out-of-body-syntax]` — сейчас `const Type.NAME` даёт
> `expected type, got '.'`. **Составные значения** (`{ code: 200 }` для
> `StatusCode.OK`) — ОРТОГОНАЛЬНЫЙ codegen-пробел `[M-d200-assoc-const-composite-value]`,
> с этой сменой поверхности не связан (нужен при любой форме).

> **AMEND (финализация парсера [M-assoc-const-out-of-body-syntax], окно №66,
> 2026-07-24):** реализовано целиком. Парсер (`parser::parse_const_decl`)
> принимает `Type.NAME` как qualified module-level const name; резолв
> (`imports::attach_out_of_body_assoc_consts`, после import-flatten, ДО
> type-check) переносит qualified decl в `TypeDecl.assoc_consts` — тот же
> const-table путь, что и in-body форма (namespace-доступ, `E_CONST_INSTANCE_ACCESS`,
> `.rodata` emission — БЕЗ изменений downstream). **In-body форма отвергнута
> целиком** (не deprecated) — `[E_CONST_IN_BODY_RETRACTED]` с migration-hint
> на любую попытку `const NAME = value` внутри `type X { … }` body: «одна
> дверь» реализована на парсер-уровне, не только текстом амендмента. Cross-file
> (тип в одном peer-файле folder-module, `const Type.NAME` — в другом) работает
> (merge — над плоским flatten'ённым item-списком). T-dependent
> `Box[int].SIZE` — синтаксис по-прежнему followup, парсер такую форму не
> распознаёт (падает раньше, на `[`). Фикстуры: `spec_tests/conformance/d200_associated_const.nv`
> мигрирован на out-of-body целиком (δ0 по PASS-результатам — 7 test-блоков,
> тот же набор assert'ов + новый namespace-only regression test); pos-комбо
> `StatusCode.OK`/`Rect.UNIT` (составное значение, №60) работает через
> out-of-body форму без изменений в codegen. Neg: `nova_tests/plan114/neg/d200_oob_instance_access_neg.nv`
> (`E_CONST_INSTANCE_ACCESS` на out-of-body decl), `d200_in_body_const_retracted_neg.nv`
> (`E_CONST_IN_BODY_RETRACTED`); мигрированы на out-of-body синтаксис (тот же
> код ошибки) `plan114_4_1_literal_includes_assoc_neg.nv`,
> `plan114_4_1_instance_access_neg.nv`, `d200_composite_ctor_call_neg.nv`,
> `d200_composite_str_field_neg.nv`.

> **AMEND (финализация [M-d200-assoc-const-composite-value], 2026-07-23):**
> codegen дореализован до составных (record-литерал) constexpr-значений —
> «only scalar» драфт-оговорка СНЯТА в границах ниже. **Пример переписан
> out-of-body формой** (окно №66, парсер финализирован — см. AMEND выше;
> исходно этот блок демонстрировал in-body форму, до её ретракции):
> ```nova
> type StatusCode value { priv code int }
> const StatusCode.OK StatusCode = { code: 200 }        // ✓ top-level struct-init в .rodata
> const StatusCode.NOT_FOUND StatusCode = { code: 404 }
> StatusCode.OK.code                              // ✓ 200 — namespace + field read
>
> type Rect value { origin Point, size int }
> const Rect.UNIT Rect = { origin: { x: 0, y: 0 }, size: 1 }  // ✓ вложенная запись
> Rect.UNIT.origin.x                              // ✓ 0 — рекурсивный field-chain
> ```
> **Границы (не расширено):**
> - **Только скаляры + вложенные записи-из-скаляров.** `str`-поле внутри
>   составного значения → честный отказ с диагностикой (`nova_str` на
>   file-scope нуждается в отдельном адресуемом `.rodata`-буфере байт,
>   которого этот путь не строит) — `[M-d200-assoc-const-composite-value]`
>   в тексте ошибки. Скалярный `str` (не внутри записи) — работает, как и
>   раньше (`const PROTOCOL str = "v2"` — отдельная ветка emit).
> - **Конструктор-вызов НЕ constexpr.** `const OK StatusCode = StatusCode.mk(200)`
>   → `E_CONST_NOT_CONSTEXPR` (runtime dispatch, не литерал) — не расширяется.
> - Grammar/место объявления на момент ЭТОГО амендмента (2026-07-23) было
>   БЕЗ ИЗМЕНЕНИЙ (in-body, как и скалярный случай) — синтаксис-поверхность
>   сменилась ПОЗЖЕ, окном №66 (AMEND выше); семантика/границы этого блока
>   переносятся дословно на out-of-body форму.
> **Codegen:** record-литерал эмитится designated-initializer'ом
> (`{ .field = <value>, ... }`) поверх уже зарегистрированной
> `record_schemas`/`type_aliases` схемы типа; рекурсия на вложенные поля
> идёт через тот же constexpr-emitter (`emit_const_expr_typed`), так что
> reference на другой top-level/assoc const в поле резолвится тем же путём,
> что и скалярный RHS. Self-referential составное значение (константа
> ссылается на СВОЙ ЖЕ тип) требует, чтобы struct-typedef типа был emit'нут
> ДО const-инициализатора — assoc-const emission-loop поэтому эмитится
> ПОСЛЕ struct/value-record body своего типа (было — до).

> **AMEND (implementation-note, [M-assoc-const-chained-method-call-p67],
> окно №73, 2026-07-24):** цепной method-вызов НАПРЯМУЮ на голом assoc-const —
> `Type.CONST.method()`, без промежуточной привязки в локаль
> (`StatusCode.NOT_FOUND.into_response()`, `StatusCode.OK.code()`) —
> резолвится КАК метод на значении const: 3-сегментный Path-call
> (`Type.CONST.method()`) эквивалентен bind-then-call (`ro x = Type.CONST;
> x.method()`). Уточнение реализации, не смена семантики D200: namespace-
> доступ и границы (скаляр/составное значение) выше — без изменений; это
> закрывает единственный оставшийся разрыв между «const как значение»
> (работало) и «метод на этом значении без привязки» (ранее — ICE
> `[P67-LEGACY] Path call return type unknown`, парсер сворачивает цепочку в
> один `ExprKind::Path([Type, CONST, method])`, симметрично уже закрытому
> 2-сегментному top-level-const кейсу `BUDGET_MS.to_millis()`). Фикстура:
> `spec_tests/conformance/assoc_const_chained_method_call.nv`.

> **Amend D200 ([Plan 157](../../docs/plans/221.1-bug-sweep.md), §157,
> 2026-07-31): `ro Type.NAME [Тип] = <выражение>` — associated **ro-value**
> (не constexpr).** Владелец лично предложил форму («через `ro` сделать
> константу», конкретно `ro BigInt.ZERO BigInt = { sign: Zero, limbs:
> []u32.new() }`) как ответ на измеренный факт: `const Type.NAME` **в
> принципе** не может держать поле с кучевой аллокацией (`Vec`-поле —
> `E_CONST_REFERS_NON_CONSTEXPR` / `E_CONST_NOT_CONSTEXPR`, strict-constexpr
> RHS — п. «Семантика» №1 выше, без изменений). До этого амендмента такая
> форма парсилась (дефолтная дот-нотация `parse_pattern` сворачивает
> `Type.NAME` в `Pattern::Variant`), но не была узнана НИГДЕ дальше по
> пайплайну — декларация тихо терялась (CC-FAIL «undeclared identifier» на
> use-site) либо (при call-подобном использовании) падала в ICE
> `[P67-LEGACY] Path call return type unknown`
> (`[M-associated-ro-const-path-call-ice]`, №157 в
> [221.1](../../docs/plans/221.1-bug-sweep.md)).
>
> **Каноническая запись:**
> ```nova
> type BigInt value { sign Sign, limbs []u32 }
> export ro BigInt.ZERO BigInt = { sign: Zero, limbs: []u32.new() }  // ✓ runtime-init
>
> BigInt.ZERO                                    // ✓ read — та же namespace-семантика, что у const
> ```
> Грамматика: `[export] ro Type.NAME [Тип] = <выражение>` на module-level —
> ровно тот же qualified-name синтаксис, что `const Type.NAME` (D200 AMEND
> окно №66), с `ro` вместо `const`.
>
> **Чем `ro Type.NAME` отличается от `const Type.NAME` (когда что выбирать):**
>
> | | `const Type.NAME` | `ro Type.NAME` |
> |---|---|---|
> | Initializer | **Strict constexpr** (literal / арифметика над literals / record-литерал из constexpr-полей / ссылка на другой `const` — п. «Семантика» №1) | **Любое выражение** — конструктор-вызов, кучевая аллокация (`Vec.new()`, `HashMap.new()`, …), вызов обычной (не `const`) fn |
> | Инициализация | Compile-time (`.rodata`-литерал, `static const T Type_NAME`) | **Runtime, ОДНОКРАТНО** — переиспользует существующую машину module-level `ro NAME = EXPR` («eager once-init», Plan 152.4/`emit_lazy_const`), КАЧЕСТВЕННО ТА ЖЕ, только keyed по квалифицированному `Type_NAME`, а не голому имени. Инициализация происходит ДО `main` (топологически упорядоченный `nova_consts_init()`), НЕ лениво по первому обращению — «once» ⇒ единожды за весь запуск, не per-access |
> | Instance access / namespace / export / record-literal-запрет | **Без изменений** — п. «Семантика» №2-6 целиком в силе для ОБЕИХ форм (обе живут в одном и том же `TypeDecl.assoc_consts`, отличаются только internal-флагом `is_lazy_ro`) | То же |
> | Переприсваивание `Type.NAME = …` | Ошибка (нет `mut Type.NAME` формы — п. «Modifier-conflicts» №6) | Ошибка, **тот же код `E_LOCAL_NOT_MUT`**, что у reassignment обычного `ro`-локала (симметрия: ассоциированное значение фиксировано СИЛЬНЕЕ локала — у локала есть escape-hatch `mut x = …`, у `Type.NAME` его нет вовсе) |
> | Constexpr-eligible RHS написан через `ro` | — | `E_RO_FOR_CONSTEXPR_PREFER_CONST` — **та же строгая partition-политика**, что у bare module-level `ro`/`const` (`[M-114.4-strict-partition]`, Plan 148 Ф.3): «одна дверь» — если RHS constexpr-eligible, он ОБЯЗАН быть `const Type.NAME`, `ro Type.NAME` в этом случае — ошибка, не тихий разрешённый дубль |
>
> **Выбор:** `const Type.NAME` — когда значение можно вычислить на этапе
> компиляции (числа, строки-скаляры, вложенные записи из скаляров).
> `ro Type.NAME` — когда значению НУЖЕН рантайм (конструктор, кучевая
> аллокация, вызов не-const функции) — типовой случай: value-record с полем
> `Vec`/`HashMap`/`Set`/`StringBuilder`/произвольным heap-record.
>
> **Codegen:** переиспользует машину module-level `ro` БЕЗ дублирования
> (`emit_lazy_const`, тот же путь, что уже эмитит eager-init global +
> topo-sort зависимостей для bare `ro NAME = expr`); квалифицированный
> C-symbol (`Type_NAME`, тот же формат, что у `const`-эмиссии) передаётся
> И как ключ Nova-уровня, И как C-qualifier — коллизий с реальными
> Nova-идентификаторами не бывает (голый Nova-идентификатор не содержит
> `_` на стыке type/const в этой позиции по построению квалификации).
> Эмиссия — В ТОЙ ЖЕ фазе пайплайна, что bare module-level `ro` (после
> generic-type-defs, чтобы кучевое generic-поле типа `[]u32`/`Vec[T]` уже
> имело typedef), НЕ в той же точке, что `const`-эмиссия (сразу после
> struct-тела типа) — иначе кучевой generic-C-тип мог бы быть недоступен.
>
> **Известный узкий разрыв (НЕ этим амендментом; см. отчёт волны Plan
> 157):** `emit_expr_c_type`'s финальный P67-LEGACY Path-fallback резолвит
> НЕАННОТИРОВАННЫЙ локал-биндинг (`ro z = Type.NAME`) через ГОЛЫЙ последний
> сегмент имени в глобальной (не type-qualified) таблице — если ДВА разных
> типа имеют associated `const`/`ro` с ОДИНАКОВЫМ последним сегментом имени
> где-то в одном compile unit (например, чужой `Type.ZERO` рядом с уже
> существующим bare top-level `export const ZERO Duration = {...}`,
> `std/src/time/duration/core.nv:82`, транзитивно тянущимся в КАЖДЫЙ CU) —
> инференс типа локала может ошибочно выбрать НЕ ТОТ тип. Это ПРЕДшествующий
> дефект (воспроизводится идентично и для уже отгруженного `const
> Type.NAME`), не специфичный для `ro`-формы; явная типовая аннотация на
> биндинге (`ro z Type = Type.NAME`) — обходной путь. Отдельно
> зарегистрирован разрыв в юнбаунд-4-сегментной цепочке
> `Type.NAME.field.method()` (унаследованный от того же класса, что уже
> известный 3-сегментный `[M-assoc-const-chained-method-call-p67]`, но
> сейчас покрывающий только `Type.CONST.method()`, НЕ
> `Type.CONST.field.method()`) — тот же ICE `[P67-LEGACY] Path call return
> type unknown`, воспроизводится идентично для уже отгруженного `const
> Type.NAME`, не специфичен для `ro`.
>
> Фикстуры: `spec_tests/conformance/plan157_ro_assoc_value.nv` (pos —
> namespace access / выражение / match / eager-once-init identity),
> `spec_tests/conformance/neg/plan157_ro_assoc_reassign_neg.nv` (neg —
> `E_LOCAL_NOT_MUT` на `Type.NAME = …`), `spec_tests/conformance/neg/
> plan157_ro_assoc_prefer_const_neg.nv` (neg —
> `E_RO_FOR_CONSTEXPR_PREFER_CONST` на constexpr-eligible RHS написанный
> через `ro`).

### Что

> ⚠️ Ниже — исходный in-body текст, РЕТРАКТИРОВАН амендментом выше (форма
> `const` в теле типа заменена на `const Type.NAME` вне тела). Семантика
> (storage/access/constexpr) переносится дословно; меняется только синтаксис
> объявления.

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

- [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindingа-ro-для-never-mut) — field-decl extended.
- [D184](03-syntax.md#d184) — Plan 114 master keyword refresh.
- [D199](03-syntax.md#d199-const-fn--comptime-evaluable-functions) — `const fn` (могут использоваться для assoc const RHS).
- [D27](03-syntax.md#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `[N]T` arrays.

### Acceptance

См. Plan 114.4 A5-A13 (T2 series).

## D214. `ptr` opaque pointer type + tuple FFI returns + opaque handle pattern

> **Plan 115** (foundational FFI). **Status:** ✅ V1 closed 2026-06-01.
>
> ⚠️ **SUPERSEDED by Plan 134 (2026-06-09)** — `ptr` built-in primitive type
> **removed**. Replace all occurrences with `*()` (pointer to unit type =
> `void*` in C). `*()` is the idiomatic expression of an opaque pointer in
> the `*T` type system (Plan 118 D216) — no compiler special-case required.
> Migration: `ptr` → `*()`; `0 as ptr` → `0 as *()`; `type X(ptr)` →
> `type X(*())`. `nova check` emits `E_TYPE_UNKNOWN` (`type `ptr` is removed —
> use `*()` …`) on `ptr`/`nova_ptr` in type position, with a migration hint
> (`type ptr = *()` user-level alias). A defensive codegen-time error mirrors
> the same message if a use ever bypasses the checker.
>
> **Pointer types after Plan 134:**
> - `*()` — opaque pointer (pointer to unit type = `void*` in C)
> - `*T` — typed pointer to T (read-only pointee by default, D216)
> - `*mut T` — typed pointer to mutable T (writable pointee)
> - `*uninit T` — pointer to possibly-uninit T (pointee contracts off; pointer
>   itself non-null — nullable = `Option[*uninit T]`, Plan 138.5 §1/§V2.4;
>   §10a rename Plan 174.5 2026-07-11: was `*unsafe T`)
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
>
> ⚠️ **AMENDED by Plan 118 Ф.5.7 (A23) 2026-06-02** — `null ptr` literal
> **hard-retracted** (E_NULL_PTR_RETRACTED_USE_OPTION). After Ф.5 NPO
> codegen (A19/A21), `Option[ptr]`/`Option[*T]` provide null-safety
> через type-system (single-pointer NPO layout, NULL=None convention).
> `null ptr` literal становится redundant и ambiguous (Some(null ptr)
> indistinguishable от None под NPO). Migration: `null ptr` →
> `(0 as ptr)` (mechanical, NULL=(void*)0 в C ABI) либо `Option[ptr]
> = None` для new code. Closes followup `[M-115-null-ptr-to-option-after-npo]`.

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

- **Size.** `int` / `intptr_t` (8 bytes на 64-bit). Bootstrap
  таргетит только 64-bit платформы (Linux x86_64, Windows x64, macOS
  ARM64/x86_64). Note: `usize` удалён (Plan 133) — pointer-sized = `int`.
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
> `[M-115-null-ptr-to-option-after-npo]` в `docs/dev/simplifications.md`
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

> **Коллизия номера разрешена 2026-07-03:** второй блок «D216 Generic anonymous tuple monomorphization» перенумерован в [D354](#d354-generic-anonymous-tuple-monomorphization); typed-pointers сохраняет D216 (несёт цепочку V2/V3-амендментов и сеть ссылок D246/118.x/174.5).
> **Plan 118** (typed pointers + unsafe model). **Status:** 🟢 ACTIVE 2026-06-02
> (Ф.0 + Ф.1.5 + Ф.2 scaffold + Ф.3 + Ф.3.2 + Ф.3.3 + Ф.3.5 + Ф.4 partial +
> Ф.5 partial + Ф.6 partial — 13 acceptance criteria closed).
>
> **D216 АМЕНДМЕНТ «всё через методы» (Plan 174.5, 2026-07-09, решения владельца
> 2026-07-06 — таблица §3 плана):** value-доступ и адресная арифметика указателей —
> ТОЛЬКО unsafe-методы; операторные формы РЕТРАКТИРОВАНЫ ошибкой
> `E_POINTER_OP_USE_METHOD` (§6/§8 этого блока в операторной части устарели):
>
> | Метод | Семантика |
> |---|---|
> | `p.read() -> T` / `p.write(v T)` | голый deref `*(p)` (D141; заменяют `*p`) |
> | `p.read_at(i) -> T` / `p.write_at(i, v)` | `*(p+i)` (заменяют `p[i]`); write_at — единая точка write-cap (`E_POINTER_RO_ASSIGN`) |
> | `p.offset(n) -> *T` | адресная арифметика element-units (заменяет `p±i`); тип НЕ деградирует |
> | `p.dist(q) -> int` | signed element count (заменяет `p-q`; порядок = знак dist, `p<q` ретрактирован) |
> | `p.read_unaligned()` / `p.write_unaligned(v)` | memcpy-семантика (невыровненный доступ) |
> | `p.read_volatile()` / `p.write_volatile(v)` | volatile |
> | `p.write(v *T) -> *mut T` | копия из указателя-источника (без value-копии) |
> | `p.copy_from/copy_to(src, n)` | memmove; `_nonoverlapping` — memcpy (обёртки RawMem byte-level) |
>
> ОСТАЮТСЯ операторами: `p == q`/`!=`, auto-deref `p.field`/`p.method()` (one-level),
> `p as *U` (cast; unsafe при U≠T). `[]`-индексация — ТОЛЬКО безопасные контейнеры
> (D138), у указателей её нет. `wrapping_offset` отложен ([M-174.5-wrapping-offset-deferred]).
> Ретрактированы: `*p`, `*p=v`, `p[i]`, `p[i]=v`, `p±i`, `p-q`, `p</<=/>/>=q`.
> Conformance: d174_5_* pos+neg (8 neg — по одному на форму). Эталон 79/0.
>
> **D216 §4 AMEND (Plan 118.6, 2026-06-17):** `&x` safe for all types (no
> `unsafe {}` required for promote path). `addr_of()` / `addr_of_mut()` retired
> → `E_ADDR_OF_REMOVED`. `mut` binding → `*mut T` auto; `ro` binding → `*T` auto —
> **норма 118.6 ВОССТАНОВЛЕНА решением владельца 2026-08-06** (в июле
> D246/Plan 147 временно заменял её на «вывод всегда `*T`»; обиход показал,
> что аннотационную церемонию систематически обходили кастом
> `*T as *mut T`, которому учил даже гайд). Сопровождение: каст
> `*T as *mut T` РЕТРАКТИРОВАН; от ro-биндинга `*mut T` не получить никак —
> ни выводом, ни аннотацией, ни кастом (№375); явная аннотация
> `ro p *mut T = &mut_x` остаётся легальной.** Escape
> analysis extended to primitives. 15/15 tests PASS.
>
> **D216 §4 AMEND 2 (Plan 118.7, 2026-06-18):** `raw &x` — новый унарный
> оператор для сырого стек-адреса без escape analysis / auto-promote.
> Требует `unsafe {}` контекст (`E_UNSAFE_REQUIRED`). `raw` — контекстное
> ключевое слово (не зарезервировано в lexer, аналог `bench`/`measure`).
>
> Инвариант после 118.7:
> - `&x` — **всегда** safe + escape analysis + auto-promote. Работает везде.
> - `raw &x` — **всегда** сырой стек-адрес, без промоута. Только в `unsafe {}`.
> - `unsafe { &x }` — эквивалентен `&x` (unsafe-контекст не влияет на `&`).
>
> Дополнительные диагностики:
> - `E_UNSAFE_REQUIRED` теперь также для `raw &x` вне unsafe (§4 amend 2).
> - `E_AMP_LITERAL` / `E_AMP_RECORD_LITERAL` / `E_ARRAY_INDEX_PTR_BANNED` —
>   применяются и к `raw &expr` (те же lvalue-ограничения).
>
> 4/4 plan118_7 tests PASS. Migration: 7 файлов `unsafe { &x }` → `unsafe { raw &x }`.
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
>   - `E_ADDR_OF_REMOVED` (D216 §4 amend, Plan 118.6) — `addr_of` / `addr_of_mut` retired
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

> **✅ FINAL — три оси мутабельности (Plan 147 D246, 2026-06-12):** flip-scan
> (flip-scan-draft) **ОТКЛОНЁН** adversarial-критикой (`*T` контекстно-зависим → тип не
> самодостаточен). FINAL = **L3 pointee-capability из ТИПА, позиционно-независимо**
> ([D246](#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee) ниже).
> **Восстановлено `*T ≡ *ro T` УНИВЕРСАЛЬНО** (во всех позициях:
> param/return/generic/alias/cast/field/local). Pointee-mut **НЕ наследуется** от
> binding (running-`current`/flip-scan убраны). `*mut T` — единственный опт-ин
> mut-pointee. Текст ниже про postfix-canonical / prefix-ban / Option-NPO
> **сохраняется**.

> **138.5 baseline (postfix pointee, prefix-ban) — KEPT под D246:** указательный
> ТИП несёт **pointee-мутабельность** постфиксом, сразу после `*` (`*mut T` /
> `*unsafe T`). **`*ro T` — HARD ERROR** `E_REDUNDANT_POINTER_RO` (избыточно: `*T`
> уже ro; fix-it `*T`). **Перепривязываемость самого указателя** (`p = other_ptr`)
> — это **L1 binding** (`ro` = фиксирован, `mut` = переприсваиваемый, D36), **НЕ
> часть типа** и НЕ влияет на pointee-capability. **Запрещены ВСЕ prefix-модификаторы
> перед `*`** (`mut * T` / `ro * T` / `unsafe * T`) — `E_POINTER_PREFIX_MODIFIER`
> (§1 ниже). Nullable = `Option[*T]` только (NPO §7). Это **ретрактит** D216 V2
> «outer pointer-mut как type-wrapper» (`mut * T = Mut(Pointer(T))`) и V3
> propagation/safe-stopper машинерию — см. §V2.2/§V2.6/§V3.2/§V3.3/§V3.4 ниже.

- `*T` — typed pointer; pointee **ro** (дефолт). **`*T ≡ *ro T`** универсально
  (D246: pointee-capability из типа, НЕ наследуется от binding).
- `*mut T` — explicit **mut** pointee (postfix only) — единственный опт-ин на
  запись `*p = …`. `*ro T` → **HARD ERROR** `E_REDUNDANT_POINTER_RO` (fix-it `*T`)

> **Flagship `*u8` (ro-pointee) use-case (Plan 139; D246, 2026-06-12):** `str` —
> Nova value-record lang-item `type str value priv { ptr *u8, len int }`. Поле
> `ptr *u8` — указатель на **иммутабельный** UTF-8 буфер (immutability строки
> выражена типом указателя: `*T` = ro-pointee, не отдельной меткой; `*ro u8`
> избыточен → `E_REDUNDANT_POINTER_RO`). `*u8` (ro) гарантирует: нет write-path
> сквозь `str.ptr`, поэтому `clone` = shallow 16-байт handle-copy
> с общим буфером безопасен, а compile-time interning литералов (один общий
> rodata-буфер на distinct content) семантически невидим. ABI поля — `T*` в C
> (`const uint8_t*`), layout-идентично старому `nova_str`. См.
> [D26 MAJOR AMEND](08-runtime.md#d26-базовая-stdlib-и-prelude) +
> [D228](#d228) content-eq override.

> **Amend (Plan 139.2 Ф.0+Ф.2, 2026-06-12): `str { ptr, len }` record-литерал +
> producer-миграция.** str — declared lang-item, поэтому str **type-методы**
> (receiver `str` ⇒ `current_recv_type == "str"`, privacy **type-based**, не
> module-based — D220) **конструируют** `str` value-record литералом `str {
> ptr: …, len: … }` в своём модуле. Codegen НЕ эмитит `NovaValue_str` (str ∈
> `RUNTIME_DEFINED_TYPES` skip-list), поэтому `str{…}` лоуэрится спец-кейсом в
> `emit_record_lit` напрямую в C compound-literal
> `(nova_str){.ptr=(const uint8_t*)(…), .len=(int64_t)(…)}` (без schema, без
> NovaValue-структуры). Внешние caller'ы по-прежнему ловят `E_PRIV_FIELD_INIT`
> (priv-поля). Это разблокировало миграцию **producer-форм** external-C →
> Nova-body:
>   - `@split(sep)` — byte-scan, каждый сегмент = zero-copy sub-view
>     `str{ptr:@ptr+off, len}` (raw-ptr арифметика под `unsafe`). **Амендмент
>     2026-07-11:** возврат `ro []str` → ленивый `SplitIter` (Rust-паритет; см.
>     03-syntax split-семейство); прежний массив — `.collect()`;
>   - `from_bytes_unchecked` / `from_bytes_lossy` — читают `(ptr,len)` источника
>     через публичные Vec-геттеры `@as_ptr()`/`@len()`, alloc(`len+1`)+memcpy+NUL
>     на `data[len]` (D26 §3); lossy валидирует UTF-8 и заменяет невалид на
>     U+FFFD;
>   - `from_bytes_unchecked_steal(consume bytes []u8)` — zero-copy reuse буфера
>     при `cap>len` (NUL in-place), иначе alloc+copy. consume-обязательство
>     закрыто новым `Vec[T] consume @into_raw() -> *mut T` (инверс
>     `Vec.from_raw_parts`: потребляет Vec-обёртку, отдаёт сырой writable-буфер).
>
> **Amend (Plan 139.2 Ф.3, 2026-06-12): `@concat` / `@compare` → Nova-body
> (operator-lowering ОСТАЁТСЯ на C — option (b)).** `@concat(other) -> str` и
> `@compare(other) -> int` мигрированы из external-C в Nova-body:
>   - `@concat`: alloc `[]u8` размера `@len()+other.len()`, копирует байты обоих
>     операндов через `@as_bytes()` (zero-copy view), затем
>     `str.from_bytes_unchecked` (owned + NUL-term D26 §3). Байт-в-байт идентично
>     C `nova_str_concat`.
>   - `@compare`: byte-loop над `@as_bytes()` обоих операндов (как C strcmp /
>     memcmp), length-aware tiebreak; возвращает `a_byte - b_byte` на первом
>     различии (u8 0..255 ⇒ тот же знак, что memcmp), иначе `sign(@len() -
>     other.len())`. Идентично C `nova_str_compare` (array.h:989).
> **РЕШЕНИЕ по operator-lowering — option (b) (оставить C-fn для операторов):**
> операторы `+` / `<` / `<=` / `>` / `>=` / `==` / `!=` над `nova_str`
> лоуэрятся ОТДЕЛЬНО, НАПРЯМУЮ в C `nova_str_concat` / `nova_str_lt` / … /
> `nova_str_eq` (emit_c.rs, BinOp-arm `lty == "nova_str"`), НЕ через
> method-dispatch. ПРЯМЫЕ method-вызовы (`s.concat(t)` / `s.compare(t)`,
> Compare-протокол `@compare(o)==0`-synthesis, `@plus`-body, `@replace` chained
> `.concat()`) маршрутизируются в Nova-body (убраны `"concat"`/`"compare"` из
> `str_method_to_rt`). **Почему option (b), не (a) (роутить операторы через
> методы + retire C-fn):** (1) **perf** — C `nova_str_concat` = один `nova_alloc`
> + два `memcpy`; Nova-body = `with_capacity` + два byte-push-loop'а (по байту,
> с bounds-check на каждом push). C `nova_str_cmp` = один `memcmp`; Nova-body =
> byte-loop с `as int`-конверсией на байт. Operator-формы — горячий путь (string
> building, sort-сравнения), C оптимальнее. (2) **ортогональность** — operator-
> lowering (BinOp codegen) и method-dispatch (`str_method_to_rt` / Nova-body) —
> независимые механизмы; миграция тела метода НЕ требует трогать operator-arm, и
> наоборот. Чистое retirement C-fn потребовало бы СОВМЕСТНОЙ миграции обоих +
> perf-харнесс для подтверждения отсутствия регрессии — orthogonal, низкий
> приоритет. Дубль (Nova-body метод + C-fn для оператора) — приемлемая цена:
> C-fn'ы малы (inline), и они единственные горячие; метод-форма редка. См.
> reframed `[M-139.1-operator-lowered-methods]`.
>
> Остаётся C **только** `@hash` (SipHash-1-3 + crypto-seed, DoS-resistance — см.
> `[M-139.1-hash-irreducible-crypto-seed]`). **9/10 str-методов — Nova-body**
> (`@concat`/`@compare` закрывают Ф.3); operator-lowering `+`/`<`/… —
> сознательно C (perf, option (b)).
>
> **Амендмент (владелец, 2026-07-21): оператор `+` для `str` РЕТРАКТИРОВАН —
> string-конкатенация через `+` больше не существует в языке.** До этого
> амендмента `nova_str + nova_str` был не документированной в D46 (общая
> operator-overloading таблица) фичей: codegen (`emit_c.rs`, BinOp-арм `lty ==
> "nova_str"`) молча лоуэрил его в `Nova_str_method_concat`, минуя `@plus`
> (см. блок option (b) выше) — де-факто рабочий, но НИГДЕ в спеке не
> санкционированный операторный `+` на строках. Закрытие дыры, не
> ретракция задокументированного поведения:
>
> - **Hard error `E_STR_CONCAT_PLUS`** — бинарный `+`, где хотя бы один
>   операнд определённо `str` (typed-checker, `types/mod.rs::walk_expr`,
>   `is_arith`-блок; permissive на unknown/generic-T, симметрично
>   `E_MIXED_WIDTH_ARITH` рядом). Сообщение — «string `+` is not part of
>   the language; use string interpolation (`"${a}${b}"`) instead — or,
>   inside a loop, a `StringBuilder` + `.append` (repeated `+` is O(n²))».
> - **Канон.** Вне цикла — string-интерполяция `"${a}${b}"`. Внутри
>   цикла — `StringBuilder.new()` + `.append(...)` + `.into_str()`
>   (не repeated `+`/`.concat()`: O(n²)).
> - **`@concat` НЕ ретрактирован** — `s.concat(t)` (std/src/runtime/
>   string/transform.nv) остаётся явным, вызываемым методом (perf-
>   идентичен старому lowering'у: alloc + 2×memcpy). Lint
>   `W_STR_CONCAT_METHOD` (perf-conventions реестр, Plan 185) рекомендует
>   интерполяцию вместо явного `.concat(...)` вне цикла;
>   `W_STR_CONCAT_LOOP` — то же самое внутри цикла (в т.ч. `.to_str()`-
>   RHS), рекомендуя `StringBuilder`.
> - **`@plus` (str, transform.nv: `str @plus(other str) -> str =>
>   @concat(other)`) НЕ удалён**, но операторный синтаксис `a + b` на
>   него больше не диспатчится — `E_STR_CONCAT_PLUS` срабатывает на
>   уровне AST (`ExprKind::Binary{Add}`) ДО любого метод-резолва,
>   независимо от того, определён ли `@plus`. D46 (`03-syntax.md`) —
>   таблица «оператор → метод» перестаёт применяться к `str`+`+`
>   конкретно (единственное исключение в языке; `@plus` для остальных
>   типов — без изменений).
> - **Операторы сравнения/равенства `str` — БЕЗ ИЗМЕНЕНИЙ.** `<` / `<=` /
>   `>` / `>=` / `==` / `!=` над `nova_str` продолжают лоуэриться как и
>   раньше (option (b) выше, `Nova_str_method_compare`/`_equal`) — этот
>   амендмент касается ИСКЛЮЧИТЕЛЬНО `+` (`BinOp::Add`).
> - **Негатив-фикстура:** `spec_tests/conformance/neg/str_concat_plus_neg.nv`
>   (`EXPECT_COMPILE_ERROR E_STR_CONCAT_PLUS`) пинует текст ошибки.
- `*uninit T` — pointer к possibly-uninit T (pointee init/layout contracts off);
  также degraded-форма после арифметики (alignment/bounds gone). **§10a rename
  (Plan 174.5, 2026-07-11):** was `*unsafe T` — see the amendment at the end
  of this D-block. (Distinct from the UNRENAMED `*unsafe fn(...)` fn-pointer-
  type composition, §10 — that keeps `unsafe`.)
- **Size:** pointer-width (8 bytes на 64-bit; bootstrap = 64-bit only)
- **ABI:** `T*` в C (compiler emits соответствующий C-type для FFI)
- **Validity:** **always non-null** (compile-time invariant); nullable
  variant — `Option[*T]` (NPO §7)

**Pointee (L3) vs binding (L1) — две ОРТОГОНАЛЬНЫЕ оси ([D246](#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee)):**

L1 binding (`ro`/`mut` перед именем) задаёт **reassignability** указателя (`p =
other_ptr`) — и больше **ничего** про pointee. L3 pointee-capability — **из типа**,
позиционно-независимо: `*T`=ro (нельзя `*p = …`), `*mut T`=mut. Оси НЕ влияют друг
на друга — pointee-mut НЕ наследуется от mut-binding (flip-scan убран).

| Запись | L1 binding (p =) | L3 pointee (`*p =`) | Вердикт |
|--------|------------------|---------------------|---------|
| `mut p *T`     | ✅ reassign | ❌ pointee ro | `*p = …` запрещён (`E_POINTER_RO_ASSIGN`) |
| `mut p *mut T` | ✅ reassign | ✅ pointee mut | оба ✅ |
| `mut p *ro T`  | — | — | ❌ `E_REDUNDANT_POINTER_RO` (use `mut p *T`) |
| `ro p *T`      | ❌ фиксирован | ❌ pointee ro | оба ❌ |
| `ro p *mut T`  | ❌ фиксирован | ✅ pointee mut | `p = …`❌, `*p = …`✅ |
| `ro p *ro T`   | — | — | ❌ `E_REDUNDANT_POINTER_RO` (use `ro p *T`) |

**`*T ≡ *ro T`** (восстановлено D216 §V2.6; flip-scan-draft отклонён): pointee-mut
задаётся **только** через `*mut T`; `*ro T` избыточен → `E_REDUNDANT_POINTER_RO`.
Reassignability указателя — L1 binding (`ro`/`mut`, D36), независима от L3.

**АМЕНДМЕНТ (2026-07-08, Plan 172.13, [M-redundant-param-ro-diagnostic]) —
избыточные модификаторы на границе fn, тот же принцип redundancy:**
(а) явный `ro` в позиции **параметра** избыточен (параметры — ro-вид по
умолчанию, D176) → hard error `E_REDUNDANT_PARAM_RO`, обе синтаксические
формы: префикс `(ro x T)` и тип-модификатор `x ro T` (fix-it: `x T`);
комбинация V3-амендмента `ro x mut T` (ro-binding + явный mut content-view,
ортогональные оси Plan 118.5 V3) — НЕ избыточна, остаётся легальной;
(б) явный `mut` в позиции **возврата** избыточен (возвращённое значение —
собственность вызывающего, мутабельность решает его биндинг) → hard error
`E_REDUNDANT_RETURN_MUT` (fix-it: `-> T`); НЕ задевает `-> *mut T`
(L3 pointee-capability) и `-> ro T` (осмысленный ro-view, oracle row D).
Тесты: conformance/neg/d246_redundant_param_ro_{prefix,type}_neg,
d246_redundant_return_mut_neg; позитив-граница d246_param_ro_mut_view.

**Запрет prefix-модификаторов (`E_POINTER_PREFIX_MODIFIER`):** токены
`ro`/`mut`/`uninit` (§10a rename, было `unsafe`) непосредственно **перед** `*`
в type-position запрещены. Расширяет `E_INVALID_POINTER_MODIFIER` (D216 §1,
commit 6d6a18a2ab7).

```nova
mut * T             // ❌ E_POINTER_PREFIX_MODIFIER — prefix перед *
ro * T              // ❌ E_POINTER_PREFIX_MODIFIER
uninit * T          // ❌ E_POINTER_PREFIX_MODIFIER
```

Сообщение: «модификаторы указателя — на pointee (после `*`: `*mut T`/`*ro T`/
`*uninit T`) или на binding (`mut x *T`); перед `*` не допускаются». Валидно:
`*mut T`/`*ro T`/`*uninit T`/`*T` (pointee, postfix), `mut name *T` (binding).

```nova
*T              // pointee ro (≡ *ro T, D246); pointee-mut из типа, не от binding
*ro T           // ❌ E_REDUNDANT_POINTER_RO (fix-it: *T) — избыточно
*mut T          // explicit mut pointee — единственный опт-ин на запись *p = …
*uninit T       // pointer к possibly-uninit T; deref требует unsafe layer
```

### §2. Binding (L1) vs pointee (L3) — ортогональны ([D246](#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee))

L1 binding (`ro`/`mut` перед именем) задаёт **только** reassignability указателя.
L3 pointee-capability — **из типа** (`*T`=ro / `*mut T`=mut), позиционно-независима,
**НЕ наследуется** от binding (flip-scan/`current` отклонены D246).

```nova
ro p *Acc           // p фиксирован; pointee ro (*p = … ❌)
mut p *Acc          // p reassignable; pointee ro (*p = … ❌ — L1 mut НЕ даёт mut-pointee)
mut p *mut Acc      // p reassignable; pointee mut (*p = … ✅)
ro p *mut Acc       // p фиксирован; pointee mut (*p = … ✅) — оси независимы
mut q = &acc        // от MUT-биндинга: сразу *mut Acc (решение 2026-08-06 — восстановленная 118.6)
```

**Восстановление D216 §V2.6 — ЧАСТИЧНО ОТМЕНЕНО 2026-08-06 (вариант Б,
решение владельца):** для АННОТИРОВАННОГО типа правило держится (`*T` в
аннотации всегда ro-pointee; `mut p *T` записи не даёт), но ВЫВОД `&x`
снова наследует от источника: от mut-биндинга — `*mut T`, от ro — `*T`
(восстановленная 118.6). Каст `*T as *mut T` ретрактирован. Гарантия: от
ro-источника писабельный указатель недостижим никаким путём (№375).

### §3. Chain order (multi-level pointers)

Pointee-модификатор пишется **постфиксом**, сразу после каждого `*`, и относится
к **target** этого `*`-уровня; читается left-to-right.

```nova
*mut *ro Acc        // mut pointer НА (ro pointer на Acc)
*ro *mut Acc        // ro pointer НА (mut pointer на Acc)
```

Prefix-форма (`mut * ro * Acc`) **запрещена** — `E_POINTER_PREFIX_MODIFIER` (§1).

Canonical Rust grammar.

### §4. `&value` operator + escape analysis с auto-promote

> ⚠️ **D216 §4 AMEND (Plan 118.6, 2026-06-16):** `&x` is now **safe** (no
> `unsafe {}` required) for all types including primitives. `addr_of()` /
> `addr_of_mut()` retired → `E_ADDR_OF_REMOVED`. Escape analysis extended to
> primitives (previously only records). See amendment block at end of §4.

**Safe pointer creation (no `unsafe {}` required, Plan 118.6+):**

```nova
ro p = &x       // *ro T  — safe; heap-promote if &x escapes function scope
ro p = &y       // *mut T if y is mut binding, otherwise *ro T
```

**Raw stack pointer (unsafe required):**

```nova
unsafe { ro p = &x }   // no heap-promote; raw stack address; programmer's responsibility
```

**Field address:**

```nova
ro p = &x.field  // chain root (x) is promoted to heap if pointer escapes scope;
                 // whole binding moves to heap, NOT individual field.
```

**Heap-promote semantics:**
- Compile-time static escape analysis decision (not runtime).
- `x` starts on stack; promoted to heap at its declaration point if address
  escapes function scope (return / closure / heap-field store / fn arg).
- Conservative V1: promote if ANY uncertainty. Precise inlining followup:
  `[M-118-escape-precise]`.
- Primitives (`int`, `bool`, `f64`, etc.) now subject to same escape analysis
  as records (Plan 118.6 extension).

**Records (heap references):** `&record` creates pointer to the reference.
Result C type: `Nova_Record**` (double-pointer because record is already
`Nova_Record*` in C ABI). Used primarily for FFI out-params:
`external fn try_init(out *Acc) -> i64` — C side fills `*out`.

**`&Record { ... }` literal без named binding forbidden** —
`E_AMP_RECORD_LITERAL`. Anonymous-local auto-promote from temporary
слишком implicit для production-grade reader clarity. Required pattern:
```nova
// ❌ implicit anonymous local
ro p = &Acc { name: "Piter" }
// ✓  explicit named local
ro acc = Acc { name: "Piter" }
ro p = &acc
```

**D32 amend rationale:** `&value` — typed pointer construction, не Rust
borrow. Safety через escape analysis + auto-promote (не lifetime checker).
До Plan 118.6 дополнительно требовал `unsafe {}` block; amend снимает это
требование для safe-promote path. Raw stack pointer остаётся `unsafe`.

> **D216 §4 AMEND (Plan 118.6, 2026-06-16):**
> - `&x` safe for all types (no `unsafe {}` for the promote path).
> - `addr_of(x)` / `addr_of_mut(x)` retired → `E_ADDR_OF_REMOVED`. Use `&x`
>   instead.
> - Escape analysis extended to primitives.
> - `E_UNSAFE_REQUIRED` is NOT triggered by AddrOf starting Plan 118.6
>   (only by Deref `*expr` and unsafe fn calls).

#### addr_of / addr_of_mut builtins (Plan 118.1 Ф.3 closeout, 2026-06-05) — RETIRED Plan 118.6

~~`addr_of(x)` / `addr_of_mut(x)`~~ — **RETIRED Plan 118.6 (2026-06-16).**
Use `&x` instead. Calling `addr_of(x)` or `addr_of_mut(x)` now emits
`E_ADDR_OF_REMOVED`. History preserved below for reference.

> **Historical (Plan 118.1 Ф.3, 2026-06-05 — Plan 118.5):** Zig-style builtin
> function aliases для `&x` (UnOp::AddrOf). Identical codegen path;
> rewriter-desugared в const_fn_eval pass. Used when explicit function-call
> syntax preferred over operator syntax (FFI patterns).
>
> Retired in Plan 118.6 — `&x` is now safe and universally preferred.
> `E_ADDR_OF_REMOVED` fires on any remaining call site.

**Enforcement (same as UnOp::AddrOf):**
- ~~E_UNSAFE_REQUIRED — outside unsafe {} block~~ (AddrOf no longer triggers
  this; only Deref and unsafe fn calls do, starting Plan 118.6)
- E_REALTIME_POINTER_OP — inside #realtime fn
- E_AMP_LITERAL / E_AMP_RECORD_LITERAL — invalid lvalues (literal / record literal)
- E_ARRAY_INDEX_PTR_BANNED — operand is, or its field-access chain passes through,
  an array index (`arr[i]` / `arr[i].field`) — unstable base (buffer resize / GC
  compaction), D216 §15
- E_ADDR_OF_NON_LVALUE — the operand's field-access chain roots in an rvalue
  (call result, arithmetic, …). **The lvalue check walks the WHOLE chain to its
  root** (Plan 118.1 [M-118.1-addr-of-chains], amended 2026-06-08): `a.b.c` /
  `(*p).f` / `self.x.y` rooted in a named local or `self` are accepted, while
  `make().f` / `arr[i].f` / `(x+1).f` are rejected (previously only the top operand
  node was inspected, so chains rooted in temporaries were wrongly accepted — a
  dangling-pointer gap). `addr_of(x)` and `&x` share one walker
  (`ast::addr_of_chain_root`), so the intrinsic and operator forms agree, and
  `addr_of(*p)` ≡ `&(*p)` (the walker descends an explicit deref to the pointer
  root, matching the `p.f` auto-deref sugar).
- E_ADDR_OF_MUT_REQUIRES_MUT_BINDING — `addr_of_mut` on a binding whose root is not
  `mut`; the mut-check also walks the field-chain root (`addr_of_mut(s.field)`
  requires `mut s`), not just a bare Ident (NEW).

**Known V1 gaps (2026-06-08 followups):** ~~(1) `addr_of_mut((*p).field)` is not yet
gated on `p` being `*mut`/mut-bound — the mut-check skips deref roots and the
desugar is a bare `UnOp::AddrOf` with no `*mut` cast~~ **CLOSED Plan 118.6** —
`addr_of_mut` retired; `&x` with mut binding auto-infers `*mut T`.
(2) ~~The `addr_of(...)` intrinsic chain-check runs in the const-fn rewrite pass, so
`nova check`/LSP does not surface it~~ **MOOT** — `addr_of` retired Plan 118.6;
`&x` chain-check runs at check-time.
([M-118.1-addr-of-chains-checktime] — closed by retirement).

Closes [M-118.1-addr-of-macros] (was: «add addr_of! macro» — macro framework not shipped, builtin-fn alternative landed).
Closes [M-118.1-addr-of-mut-deref-ptr-mut] (Plan 118.6 — `addr_of_mut` retired; `&x` mut-inference covers this).

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

> **`E_POINTER_RO_MUT_METHOD` ENFORCED (2026-08-08, Plan 221.1 №387, window
> pptr-ro-guard).** The row above (`p.method()` (mut recv) → ❌
> `E_POINTER_RO_MUT_METHOD`) documented the rule since Plan 118 but the
> checker never enforced it — a mut-receiver method (`fn T mut @m(...)`)
> called through a readonly-pointee pointer (`*T`) compiled clean and
> mutated the pointee at runtime (221.1 №387 registry). Same class as №375
> (`E_POINTER_MUT_FROM_RO_SOURCE`, which closed *materializing* a writable
> `*mut T` from a `ro`-bound SOURCE) but a distinct gap: this closes the
> call-site *use* of an already-existing readonly-typed pointer to reach a
> mut method — orthogonal to where or how that pointer's type was arrived
> at. Gated purely on the CALL-SITE receiver's own pointee-writability type
> (`*T` vs `*mut T`, `pointee_is_writable`) — irrelevant whether the
> pointer's SOURCE binding is `mut` or `ro`, or whether the pointer's OWN
> local binding is `mut` or `ro` (only the pointee axis matters, same as
> `E_POINTER_RO_ASSIGN`'s existing `.write()`/`*p = v` gate, whose call-site
> shape this enforcement mirrors exactly). Arity-scoped against a Plan 135
> ro/mut overload PAIR at the same method name (`@peek() -> int` ro-getter
> vs `mut @peek(v int) -> ()` mut-setter, D117 amend fluent idiom) — a
> readonly pointer may still call the RO-arity overload; only an
> arity-matched mut-receiver candidate (with no ro-arity sibling at THAT
> arity) is rejected. **Not a new rule** — closes an enforcement gap of an
> ALREADY-declared rule (same posture as №367/№368 in the same registry
> window: the diagnostic code and its semantics were already documented
> here and in Plan 118; no new D-number, no semantics change — programs
> that were always meant to be rejected are now actually rejected). Neg
> fixtures: `spec_tests/conformance/neg/d387_ptr_ro_mut_method_neg.nv`
> (explicit `*T` annotation), `neg/d387_ptr_ro_mut_method_ro_source_neg.nv`
> (unannotated, inferred from a `ro` source), `neg/d387_ptr_ro_mut_method_
> overload_arity_neg.nv` (arity-aware regression guard). POS boundary:
> `spec_tests/conformance/d387_ptr_mut_method_pos.nv`.

### §6. Pointer arithmetic + order comparison

```nova
unsafe {
    ro p1 = some_ptr + 1            // *unsafe T (degraded)
    ro p2 = some_ptr + offset
    ro diff = p2 - p1               // int (element count, signed)
    unsafe { *p1 }                   // *unsafe T deref требует ещё unsafe layer
    ro lt = p1 < p2                  // order-compare allowed inside unsafe
}

// Equality `==`/`!=` — safe anywhere (identity check, no ordering):
ro p = unsafe { &x }
ro q = unsafe { &x }
ro same = p == q                     // OK outside unsafe — identity check
```

- `+`/`-`/`+=`/`-=` only в `unsafe { }` block
- Result `*unsafe T` для `ptr ± int`; `int` для `ptr - ptr` (signed element count)
- Units: sizeof(T)-scaled (C/Rust convention)
- `*`/`/`/etc. — `E_PTR_ARITHMETIC_INVALID` (не математически осмыслено)
- **Order compare** `<`, `<=`, `>`, `>=` — require unsafe context —
  **`E_PTR_ORDER_COMPARE_REQUIRES_UNSAFE` ACTIVE 2026-06-02 (V1 syntactic,
  commit 601af30fc30)** — closes acceptance A17 partial. Rationale: pointer
  addresses не stable ordinals (GC-relocation invariant + OS ASLR random
  layout). V2 (Session 4+): full type-aware enforcement через
  `infer_expr_type`.
- **Equality** `==`/`!=` — safe everywhere (identity check; OK outside unsafe).

> **Nova codegen note (Plan 131, 2026-06-08):** For typed pointer `*mut T`,
> the expression `ptr + n` emits `(ptr + n)` in C — the C compiler scales by
> `sizeof(T)` automatically (standard C pointer arithmetic). `*(ptr + n) = v`
> emits an lvalue deref-write. `p as *mut T` emits `(T*)(p)` reinterpret cast.
> This is the foundation Vec[T] uses for its element buffer (see D232).

### §7. Null safety: `Option[*T]` + NPO codegen

`*T` — non-null guaranteed. `Option[*T]` — nullable через **NPO codegen**.

**Status: ACTIVE 2026-06-02** (Plan 118 Ф.5 V1 landed, commit 6b90e698437).
Closes acceptance **A19 ✅** (sizeof verification + struct layout).

- Layout: single pointer (8 bytes), не tagged struct (16 bytes)
  ```c
  // NPO-eligible: pointer-typed inner (c_ty ends_with('*'))
  typedef struct NovaOpt_const_Nova_X_p { const Nova_X* value; } ...;
  // Non-NPO: scalar/composite inner — tagged form retained
  typedef struct NovaOpt_nova_int { int tag; nova_int value; } ...;
  ```
- Construction: `Some(p)` → `{.value = p}`; `None` → `{.value = NULL}`
- Pattern match: `if (ptr == NULL) None_branch else Some_branch(ptr)`
- Direct C-FFI compatible (matches `malloc` / `fopen` / `dlopen` returns)

**V1 detection** (`c_ty.ends_with('*')`): covers `*T` family (`T*`,
`const T*`, `void*`), pre-existing stdlib pointer types (UdpSocket,
TcpStream, SocketAddr, TcpListener, File, etc — все benefit automatically).

**V2 detection** (Plan 118 Ф.5.4 ACTIVE 2026-06-02, commit cd168a4d53b):
- `Option[ptr]` — `nova_ptr` typedef (`= void*`) — now NPO-eligible
  (A21 partial closes). Plan 115 backward-compat preserved.

**V3 detection** (Plan 118 Ф.5.8 ACTIVE 2026-06-02, commit 9fe42f39c51):
- `Option[X]` где `type X(*T)` / `type X(ptr)` — newtype-over-pointer
  transparent typedef. Lookup `Nova_X` в type_aliases (registered
  emit_type_decl); if underlying alias_c ends '*' or == "nova_ptr",
  NPO-eligible. **Closes A20 ✅.** Note: для canonical Plan 115
  pattern Nova type system pre-collapses к underlying (nova_ptr) —
  V2 branch fires directly; V3 defensive coverage для future paths.

**V4 detection** (Plan 118 Ф.5.9 ACTIVE 2026-06-02, commit 3725af23fcd):
- `Option[Option[*T|ptr]]` nested — emits `W_OPTION_DOUBLE_NESTED`
  warning через lint framework (lints.rs lint_option_double_nested).
  Outer Option uses tagged fallback (correctly — inner c_ty = struct);
  semantically ambiguous (None vs Some(None)). **Closes A22 ✅.**

**V5 detection** (Plan 118 Ф.5.10 ACTIVE 2026-06-02):
- `Option[*fn(...)]` — function pointer types. После audit type_ref_to_c
  lowering: `TypeRef::Pointer(modif, TypeRef::Func{...}, _)` → `void**`
  (Func → `void*`, outer Pointer adds another `*`). c_ty ends with `*`
  → V1 detection (Ф.5 A19) ALREADY triggers NPO. **Closes A21 remainder ✅**
  через existing infrastructure без code changes. Test fixture
  t5_7_npo_option_fn_pointer_ok verifies.

**All Ф.5 NPO acceptance criteria CLOSED:** A19 ✅, A20 ✅, A21 ✅,
A22 ✅, A23 ✅.

**Other deferred** (Session 4+):
- ~~`null ptr` literal retraction~~ — **A23 ✅ CLOSED 2026-06-02**.
  D214 amended; parser emits `E_NULL_PTR_RETRACTED_USE_OPTION`;
  14 fixtures migrated к `(0 as ptr)`. Closes `[M-115-null-ptr-to-option-after-npo]`.

```nova
external fn malloc(sz int) -> Option[*u8]
// → C: uint8_t* malloc(size_t); (codegen casts intptr_t→size_t)

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

**Inside unsafe разрешено:** `&value`, `*p`, `p[i]` (pointer index),
`p.field`, `p.method()`, `p.field = v`, pointer arith, `int as *T`,
`<`/`>` compare, `&record.field`, calling `unsafe fn`, newtype construction
wrapping pointer.

**`ptr[i]` pointer index (D216 §8, [M-118-ptr-index-unsafe], 2026-06-09):**
`ptr[i]` ≡ `*(ptr + i)` — derefs the pointer without bounds guarantee.
Semantically identical к explicit `*ptr` deref, hence requires the same
unsafe context. `E_UNSAFE_REQUIRED` fired when `ptr[i]` is used outside
`unsafe { }` block or `unsafe fn` body. Detection: syntactic —
`expr_is_typed_pointer(obj)` (covers `*T`/`*mut T`/`*unsafe T` bindings via
`ptr_vars` frame OR explicit type-annotation `*T` on binding). Example
migration: `unsafe { *(@data + i) }` → `unsafe { @data[i] }` (more
ergonomic; enables C `(data)[i]` pointer-arithmetic emission which the C
compiler scales automatically by `sizeof(T)`).

**Outside unsafe safe:** type declarations `*T`, `external fn` declarations,
field read `acc.next` (where `next *T`), pattern match `Option[*T]`,
`==`/`!=` compare, newtype declarations, `p as int` (hash hazard warning).

### §9. `unsafe fn` keyword syntax (Plan 118.1.7 amend, 2026-06-09)

> **Plan 118.1.7 migrates from `#unsafe` attribute to `unsafe fn` keyword
> (type-consistent, per Plan 118.5 TypeRef::Unsafe + Plan 118.1.6 `*unsafe fn` ptr type).
> `#unsafe fn` → hard error `E_UNSAFE_ATTR_DEPRECATED`.**

- `unsafe fn foo(...)` — declares function of unsafe fn type
- `external unsafe fn foo(...)` — external fn of unsafe type
- Body of `unsafe fn` — implicit unsafe context (pointer ops без `unsafe { }` wrap)
- Call `unsafe fn` — requires `unsafe { ... }` wrap у caller (visual
  marker) — `E_UNSAFE_CALL_REQUIRES_WRAP` иначе
- Type: `unsafe fn(Args) -> Ret` (consistent with `*unsafe fn(...)` fn-ptr type, Plan 118.1.6)
- No propagation up — каждая fn decides encapsulate or propagate
- `#unsafe fn` / `#unsafe external fn` → `E_UNSAFE_ATTR_DEPRECATED` (hard error)

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

#### `unsafe fn` as part of fn-ptr type (Plan 118.1.6 closeout, 2026-06-08; amend Plan 118.1.7, 2026-06-09)

Function pointer тип encodes unsafe fn keyword:
- `*fn(...)` — safe function pointer
- `*unsafe fn(...)` — unsafe function pointer (postfix pointee; prefix
  `unsafe * fn(...)` retired — `E_POINTER_PREFIX_MODIFIER`, Plan 138.5)

Coercion rules:
- `*fn → *unsafe fn`: ✅ allowed (covariant — safe это «подмножество» unsafe)
- `*unsafe fn → *fn`: ❌ E_UNSAFE_FN_PTR_COERCION (нельзя «забыть» unsafe)

Call-site:
- Call через *unsafe fn ptr без unsafe { } → E_UNSAFE_CALL_REQUIRES_WRAP (mirrors direct `unsafe fn` call).

addr_of propagation:
- addr_of(safe_fn) → *fn(...)
- addr_of(unsafe fn) → *unsafe fn(...) (тип propagated из FnDecl.unsafe_attr)

Rust precedent: fn() ≠ unsafe fn() — same model.

Закрывает [M-118.1.5-unsafe-fn-pointer-type].

#### ABI-тег fn-ptr: `*extern "C" fn` (Plan 174.6 M0 cross-amend, 2026-07-04)

`*fn(...)` / `*unsafe fn(...)` (выше) — **Nova-ABI** captureless fn-ptr: Nova-типы в сигнатуре
допустимы (Nova ABI их передаёт; «captureless» — про отсутствие env, не про типы). Для передачи
Nova-функции как настоящего **C-callback** введён C-ABI-тегированный fn-ptr тип
**`*extern "C" fn(...)`** — параллель к объявлению `extern "C" fn` ([08-runtime.md#d282](08-runtime.md#d282)).
Типы его сигнатуры (параметры + возврат) обязаны быть **C-ABI-совместимы** (рекурсивный тип-лист,
[D282 rule 2](08-runtime.md#d282)); коэрция `fn → *extern "C" fn` проверяет C-ABI + captureless +
**effect-free/total** (callback не должен объявлять **никакого** эффекта — C зовёт его без Nova-handler-фрейма
на стеке, поэтому любая effect-операция unsound; это **обобщает** `Fail`-специфичный гейт §20 /
`E_CALLBACK_THROWS_OVER_C_ABI` на все эффекты). Полная спецификация ABI-тега и обоснование условия (3) —
[D353](08-runtime.md#d353). Реализация (парсер/чекер/тесты) — Plan 174.6 M1–M3; M0 = только спека.

Полная **cast/коэрция-матрица** `fn` / `*fn` / `*extern "C" fn` (какой источник во что коэрцится и с какой
диагностикой) + правило «`*fn` и `*extern "C" fn` — разные типы, нет неявной конверсии» + легальность тега
в non-`extern "C" fn` позициях (Nova-fn-параметр / поле value-record) — [D353 «Cast/коэрция-матрица»](08-runtime.md#d353)
(Plan 174.6 M2). Строка «Calling convention: default C ABI» выше (§10 список) относится к **эмиссии** fn-ptr
на платформе; ABI-**тег** на ТИПЕ (`*fn` = Nova-ABI против `*extern "C" fn` = C-ABI) — ортогональное
измерение, введённое здесь (полная ретракция формулировки §10 про «default C ABI» у bare `*fn` — остаток
`[M-174.6-ffi-abi]`, семантически связан с дефолтным ABI `*fn`).

### §11. `ptr` redefine (D214 amend cross-ref)

> ⚠️ **RETIRED by Plan 134 (2026-06-09)** — the `ptr` built-in name is fully
> removed; there is no `type ptr Option[*unsafe ()]` builtin anymore. The
> idiomatic opaque pointer is `*()` (pointer-to-unit = `void*`). `ptr`/`nova_ptr`
> in type position → `E_TYPE_UNKNOWN` (use `*()`). See the D214 SUPERSEDED
> banner. The note below is preserved for historical context only.

```nova
type ptr Option[*unsafe ()]
```

- ABI preserved (single `void*`)
- `null ptr` literal **retracted** (use `None`); closes
  `[M-115-null-ptr-to-option-after-npo]` ✅
- Backward-compatible для existing `ptr` usages (handle patterns, tuple
  FFI returns, etc.)

### §11a. Typed pointer instance methods (Ф.4 V1, amend 2026-06-03)

Primitive-`T` typed pointer instance methods landed (V1 scope —
primitive `T` only, struct-`T` deferred):

| Method                  | Receiver      | Returns    | C codegen                          |
|-------------------------|---------------|------------|------------------------------------|
| `(*ro T).read()`        | any `*T`/`*ro T`/`*mut T`/`*uninit T` | `T`        | `(*p)`                             |
| `(*mut T).write(v T)`   | `*mut T` (амендмент 2026-08-05, №358: голый `*uninit T` БОЛЬШЕ НЕ writable — нужен составной `*mut uninit T`, см. §V2.2) | `nova_unit`| `((*p) = v, NOVA_UNIT)`            |

Detection: `obj_ty` ends в `*` AND not a known Nova typedef
(`Nova_*`/`NovaArray_*`/`NovaOpt_*`/`NovaRes_*`/`NovaBox_*`/`NovaValue_*`)
AND not `void*` / `nova_ptr`. `is_const` derived от `const ` prefix on
`obj_ty`; controls write availability.

**Safety convention:** caller wraps в `unsafe { ... }` block. Enforcement
`[M-118.1-unsafe-attr-on-external-fn]` ✅ RESOLVED (Plan 118.1.5 — capability
ships; note: syntax later superseded by 118.1.7).

**Diagnostic:**
- `(*ro T).write(v)` — currently emits generic "method not found" via
  fall-through to default dispatcher; typed
  `E_PTR_WRITE_ON_RO_TARGET` deferred — followup `[M-118.4-typed-ro-write-error]`

**Limitations (V2 follow-up):**
- Struct `T` (`obj_ty` starts с `Nova_`) — `read`/`write` not dispatched
  (deep copy + ownership semantics required) — `[M-118.4-struct-ptr-read]`
- Pointer arithmetic (`p.add(n)`, `p.offset(n)`) — `[M-118-ptr-arithmetic]`
- Volatile variants (`read_volatile`/`write_volatile`) — `[M-118.1-volatile-ops]`

Closes followup `[M-118.1-typed-pointer-instance-methods]` для primitive
`T` scope.

### §12. Casts

| From | To | Safe? |
|---|---|---|
| `*T` | `int` | ✓ (см. hash hazard) |
| `int` | `*T` | unsafe |
| `*ro T` | `*mut T` | unsafe |
| `*mut T` | `*ro T` / `*T` | ✓ |
| `*T` | `*uninit T` | ✓ |
| `*uninit T` | `*T` | unsafe |
| `*T1` | `*T2` (T1≠T2) | unsafe |
| `fn → *fn` | ✓ если captureless | `E_CLOSURE_HAS_ENV` иначе |
| `*fn → fn` | unsafe | wraps |
| `*T` | `bool` / `f64` / etc. | ❌ `E_PTR_CAST_INVALID_TARGET` |

**Hash hazard:** `p as int` для GC-tracked objects + HashMap key →
`W_PTR_AS_INT_GC_HASH_HAZARD` (address can change via GC compaction).
Note: `usize`/`isize` removed (Plan 133) — use `int` for pointer-as-integer casts.

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

**Canonical form (Plan 91.14, D229 — 2026-06-05):**

- `*T` implements **Debug** (sibling protocol — см.
  [D229](#d229-Debug-protocol--format-spec-expr)) через built-in
  `@debug(sb StringBuilder)` который emits hex address + type name
  (`"0x7f... -> Account"`).
- Canonical interpolation: `"${p:?}"` — routes к `Debug.@debug`
  (debug semantics: diagnostic, machine-oriented).
- `*T` **НЕ** implements `Display` — bare `"${p}"` остаётся ошибкой
  (forces explicit decision; pointer debugging = deliberate; addresses
  non-deterministic, leak ASLR info).

**Legacy alias (backwards-compat):**

- `(*T).to_debug_str() -> str` — built-in method (in unsafe context only).
  Эквивалент `let sb = StringBuilder.new(); p.@debug(sb); sb.to_str()`.
  Сохраняется для пред-D229 кода; новый код пишет `"${p:?}"`.

**Bare `${p}` enforcement:**

- `"${p}"` interpolation → `E_PTR_NO_DISPLAY_USE_DEBUG_STR` —
  **ACTIVE 2026-06-02 (V1 syntactic, commit a9327c65d3f)** —
  closes acceptance A28 partial. V1 detects:
    - direct `${&x}` / `${*p}` (Unary AddrOf/Deref)
    - `${expr as *T}` (cast к pointer type)
    - `${var}` где var bound через `let var = AddrOf/Deref/As(*T)`
  V2 (Session 4+): full type-aware enforcement через `infer_expr_type` —
  fires на returned pointer values, field access, generic-bound `*T`.
- Hint в диагностике после Plan 91.14: «use `${p:?}` for pointer debug
  formatting (Debug, D229)».

**См. также:** [D229](#d229-Debug-protocol--format-spec-expr) —
Debug protocol + `${expr:?}` format-spec syntax.

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

### §21. Операции, требующие `unsafe { }` — авторитетная карта + `E_UNSAFE_UNUSED` (unsafe-cluster, Plan 174.5 followup, 2026-07-11)

> **Status:** ✅ DONE 2026-07-11 (sonnet, worktree `nova-nt` branch `unsafe-cluster`).
> Owner asked 3× for a precise answer on «does passing a raw `*T` through an
> `extern` call require `unsafe { }`» — this section is the single
> authoritative map (source: the actual checker, `types/mod.rs
> check_unsafe_context_in_module` / `UnsafeCtx::walk_expr`, NOT the older §4-
> §10 prose above, which predates several retractions — see the discrepancies
> noted inline below).

#### Карта: что ТРЕБУЕТ `unsafe { }` (checker-enforced, hard error if absent)

| # | Операция | Диагностика | Где в checker |
|---|---|---|---|
| 1 | `raw &x` — сырой stack-адрес без escape-анализа | `E_UNSAFE_REQUIRED` | `UnOp::RawAddrOf` arm |
| 2 | `*expr` pointer dereference (unary) | `E_UNSAFE_REQUIRED` | `UnOp::Deref` arm — **но см. примечание ниже: operator-форма мертва в валидных программах** |
| 3 | `ptr[i]` index на typed pointer (`*T`/`*mut T`/`*uninit T`) | `E_UNSAFE_REQUIRED` | `Index` arm, `expr_is_typed_pointer(obj)` — **тот же caveat** |
| 4 | Pointer-pointer order-compare `<`/`<=`/`>`/`>=` (оба операнда typed pointer) | `E_PTR_ORDER_COMPARE_REQUIRES_UNSAFE` | `Binary` arm — **тот же caveat** |
| 5 | Вызов `unsafe fn` / `external unsafe fn` (free-fn, instance-метод, static-метод `Type.m(...)`, или косвенный вызов через `*unsafe fn(...)`-биндинг) | `E_UNSAFE_CALL_REQUIRES_WRAP` | `Call` arm, `unsafe_fns`/`unsafe_static_methods`/`unsafe_fn_ptr_vars` |
| 6 | ЧТЕНИЕ (Ident/Member/Index access) локала/параметра типа `uninit T` (value-wrapper) | `E_UNSAFE_T_READ_REQUIRES_WRAP` | `Ident`/`Member`/`Index` arms, `unsafe_t_vars`. **Запись — safe** (переход к valid) |
| 7 | Narrow-cast `uninit_T_binding as T` (снятие `uninit`-обёртки) | `E_UNSAFE_T_NARROW_REQUIRES_UNSAFE` | `As` arm |

**Caveat к пп. 2-4 (operator-семья мертва, Plan 174.5 §Ф.0 «всё через методы», 2026-07-06):**
`*p`, `p[i]`, `p+i`/`p-i`, `p-q`, `p</<=/>/>=q` — все РЕТРАКТИРОВАНЫ на уровне
codegen: `emit_c.rs` безусловно (независимо от unsafe-wrap) отклоняет их с
`E_POINTER_OP_USE_METHOD`, требуя `.read()`/`.write()`/`.read_at()`/
`.write_at()`/`.offset()`/`.dist()` вместо них. Типоуровневая проверка
(пп. 2-4 выше) технически ещё существует в checker'е и всё ещё формально
входит в карту (нужна для «есть ли операция вообще», см. `E_UNSAFE_UNUSED`
ниже), но **ни одна валидная Nova-программа не может дойти до неё** — любое
использование `*p`/`p[i]`/`p<q` падает на codegen-стадии независимо от
исхода checker-стадии. Живые, реально-используемые эквиваленты — строка 8
следующей таблицы.

**Амендмент 2026-08-05 (№353, окно p-ptr):** та же ретракция enforced
ТАКЖЕ в checker'е (`nova check`, не только codegen/`nova test`) —
`check_target_readonly`'s Deref/Index arms, `types/mod.rs`; проверка
срабатывает ПЕРВОЙ и безусловно по writability (на ro-указателе больше не
выскакивает сбивающий `E_POINTER_RO_ASSIGN`). Codegen-проверка остаётся
defense-in-depth. Формы ЧТЕНИЯ (`x = *p`, `y = p[i]`) чекером пока не
покрыты — №367 реестра.

#### Карта продолжение: НЕ enforced checker'ом, но контрактно unsafe (известные gaps)

| # | Операция | Почему не enforced | Что делать |
|---|---|---|---|
| 8 | Вызов raw-pointer intrinsic-метода (`p.read()`/`p.write(v)`/`p.read_at(i)`/`p.write_at(i,v)`/`p.offset(n)`/`p.dist(q)`/`p.read_unaligned()`/`p.write_unaligned(v)`/`p.read_volatile()`/`p.write_volatile(v)`/`p.copy_from[_nonoverlapping](...)`/`p.copy_to[_nonoverlapping](...)`) | Эти методы — compiler intrinsics, хардкод-диспатч в `emit_c.rs` (поиск `method == "read"` и т.д.), НЕ `Item::Fn` — `check_unsafe_context_in_module` собирает `unsafe_fns`/`unsafe_static_methods` только из AST `Item::Fn` объявлений, так что для этих методов регистрировать нечего. Документировано ещё в комментарии `emit_c.rs` 2026-06 («Caller wraps in `unsafe { }` — parser не enforces»). | Оставлено как соглашение (caller ответственен). `E_UNSAFE_UNUSED` (ниже) распознаёт вызовы этих методов как «unsafe used» — чтобы существующий `unsafe { p.read() }` код по всему `std/` не считался мёртвым — но НЕ добавляет новый required-wrap gate. Закрытие этого gap (полноценный `E_UNSAFE_CALL_REQUIRES_WRAP`-стиль контроль вызовов) — отдельный, больший followup, вне рамок этой волны. |
| 9 | Вызов cross-module `unsafe fn` static-метода (напр. `RawMem.alloc(...)` из другого модуля) | `unsafe_fns`/`unsafe_static_methods` собираются ТОЛЬКО из `module.items`/`module.peer_files` — деклараций того же модуля/co-equal-файлов; `import`-нутые модули не просматриваются. Найдено этой волной (repro: `unsafe { RawMem.alloc(n) }` в `std/collections/vec/*.nv`, где `RawMem` объявлен в `runtime.raw_mem`, ложно флагуется `E_UNSAFE_UNUSED` — а до этой волны звонки `RawMem.alloc(...)` ВООБЩЕ БЕЗ unsafe-обёртки нигде не ловились `E_UNSAFE_CALL_REQUIRES_WRAP`). | `E_UNSAFE_UNUSED`-heuristic узко распознаёт `RawMem.*` (единственный известный кросс-модульный unsafe-namespace — весь его surface `unsafe fn`, см. `std/runtime/raw_mem.nv`) как unsafe-used независимо от модуля. Полноценный fix коллектора (собирать `unsafe_fns` по всем импортированным модулям) — отдельный, больший followup. |
| 10 | Передача raw-pointer-аргумента (`.ptr()`/`.as_ptr()`/`&x`/cast к `*T`) в НЕ-`unsafe`-помеченный `extern`/`external` fn | Ничего не требует этого — extern fn без `unsafe fn` keyword просто безопасен для вызова (см. п.5: gate только по `unsafe_attr`); передача `*T`-значения аргументом НЕ входит в карту вообще. | Наблюдаемое, но НЕ officially required соглашение по всему `std/net`+`std/tls` (см. находку ниже). `E_UNSAFE_UNUSED` распознаёт `extern_fn(...ptr-ish-arg...)` как unsafe-used, чтобы не флагать ~35 существующих сайтов, но НЕ добавляет новый required-wrap gate. |

#### D216 §21 AMENDMENT (2026-07-17, [M-d216-unsafe-map-single-file-gaps] закрыт)

Владельческое репро: `nova check std/src/runtime/string/core.nv` (single-file) был
ЛАТЕНТНО красным (`E_UNSAFE_UNUSED`) на `str @bytes() => unsafe { []u8.new(@ptr,
@byte_len()) }` — единственная строка, где реальный CU (импорты/тесты/CI) шёл
зелёным, а изолированный single-file check ловил ложный "unused unsafe". Два
независимых пробела карты, оба закрыты этой волной:

**(а) Cast к pointer-типу не входил в used-tracking карту.** `expr as *T`/`*mut
T`/`*uninit T` — канонический способ получить typed pointer из сырого источника
(напр. `@ptr as *mut u8` в исторической 3-арг форме VIEW-конструктора). §21-карта
уже узнаёт pointer-target cast для ДРУГИХ гейтов (`expr_is_typed_pointer`'s `As`
arm — используется deref/index/order-compare/interpolation проверками), но
used-tracker (`E_UNSAFE_UNUSED`) это распознавание не зеркалировал — `unsafe { p
as *mut T }`, не содержащий больше НИЧЕГО из карты, ложно флагуется. Заодно
backfilled пропущенную запись для `char`-target cast (строка была в коде с
2026-07-11, в карту не попала — тот же класс проблемы, drift между кодом и
спекой). Обе записи — used-tracking ONLY (см. `E_UNSAFE_UNUSED` ниже), новый
required-wrap gate НЕ добавляют (значение выражения было и остаётся safe вне
unsafe-контекста для этих кастов).

**(б) Generic-static/`[]T`-slice-sugar unsafe-fn оверлоад не резолвился ни для
used-tracking, ни для enforcement.** `Vec[T].new(ptr, len)` / `[]u8.new(ptr,
len)` парсятся `Member{ obj: TurboFish{ base: Ident(Type), .. } | Path(["__array",
elem]), name }` — форма, которую П.5-карта (`unsafe_callee_name` match в
`check_unsafe_context_in_module`) не распознавала вообще (знает только bare-`Ident`
ресивер и 2-сегментный `Path`, напр. `RawMem.alloc(...)`). Наивное расширение
матча на ЛЮБОЙ `Member`/`Path`-ресивер было бы arity-blind: у `Vec.new` ТРИ
арности под ОДНИМ `(тип, имя)` ключом (`new(cap: int = 0)` 0/1-арг, `new(ptr *T,
len int)` 2-арг **unsafe** VIEW-конструктор, `new(ptr *mut T, len int, cap int)`
3-арг owned) — потребовать `unsafe { }` у ВСЕХ них означало бы тысячи ложных
срабатываний на safe-арностях по всему `std/`.

Фикс — резолв КОНКРЕТНОГО оверлоада по ARG COUNT (`static_arities` в
`check_unsafe_context_in_module`, `types/mod.rs`): для static-ресивер `Item::Fn`
собирается `(min_required, max_accepted, unsafe_attr)` каждого оверлоада под тем
же `(тип, имя)` ключом; на call-сайте фактическое число позиционных аргументов
проверяется против всех оверлоадов — если РОВНО ОДИН диапазон включает это число,
его `unsafe_attr` резолвится однозначно (для `Vec.new` диапазоны `[0,1]`/`[2,2]`/
`[3,3]` не пересекаются — 2-арг вызов однозначен). Genuine same-arity ambiguity
(два оверлоада с ОДИНАКОВЫМ диапазоном) оставляется неразрешённой (консервативно,
как раньше). Попытка резолвить через checker-канал `resolved_callees` (Plan 172.1
U.3.4 / 196.7-семья) была отклонена ПОСЛЕ эмпирической проверки — канал остаётся
ПУСТЫМ для этой формы вызова: `check_call_argbind`'s `Member{obj,..}` arm явно
делает bail-out для static/type-ресивера (`resolve_instance_method` ожидает
VALUE-ресивер), так что arg-валидация generic-static ctor-вызовов идёт через
СОВСЕМ другой, не пишущий в канал путь (codegen-side arity+C-type dispatch,
`generic_type_methods[base].find(name)`, "1b" turbofish-ветка `emit_c.rs`) — канал
`resolved_callees` покрывает free-fn/instance-method/2-сегментный-`Path`
резолв, но НЕ generic-static ctor-форму; экземпляр (б) — из этой волны.

**Enforcement** (новая строка П.5-семьи, не отдельный номер — расширяет П.5):
резолв по arg-count применяется И к enforcement (`E_UNSAFE_CALL_REQUIRES_WRAP`
при depth==0), И к used-tracking (asymmetric fallback при неразрешённой арности:
used-tracking мягкий — засчитывает "used", если вызванное ИМЯ имеет ХОТЬ ОДИН
unsafe-оверлоад где-либо в scope; enforcement строгий — только при однозначном
arity-резолве). До фикса `unsafe fn Vec[T].new(ptr, len)` был семантически
заявлен владельцем, но вызов БЕЗ `unsafe { }` НЕ ловился вообще ни для одной
static-generic/slice-sugar формы в языке.

Фикстуры: `spec_tests/conformance/d216_unused_unsafe_pos.nv` (позитив: cast-only
used-tracking + оба call-shape'а с обёрткой), `spec_tests/conformance/neg/
d216_generic_static_unsafe_overload_neg.nv` + `neg/
d216_slice_sugar_unsafe_overload_neg.nv` (негатив: enforcement без обёртки, обе
формы). Реализация: `compiler-codegen/src/types/mod.rs`
(`check_unsafe_context_in_module`, `generic_static_receiver`,
`static_overload_arity_range`, `call_callee_name`).

#### Находка (владелец спрашивал 3×): `net_tcp_listen(addr.ptr(), ...)` — требует ли `unsafe { }`?

```nova
fn SocketAddr @ptr() -> *()                                        // сырой *(), НЕ *uninit
extern "C" fn net_tcp_listen(addr *(), backlog int, out_err *mut int) -> *()  // НЕ unsafe-typed

ro h = unsafe { net_tcp_listen(addr.ptr(), 128, &err) }
```

**Ответ: ПО ФАКТУ (текущие правила п.1-7) — НЕ требует.** Ни одно правило
карты не задевает этот вызов:
- `net_tcp_listen` не помечена `unsafe fn` → п.5 не применяется (calling
  a plain `extern "C" fn` is safe per se — checker gate'ит ТОЛЬКО по
  `FnDecl.unsafe_attr`, независимо от того, C-ABI это или нет; §8 текста
  выше буквально говорит «Outside unsafe safe: … external fn declarations»,
  и это распространяется на ВЫЗОВЫ, не только декларации — CALLS к non-
  `unsafe` extern fn тоже не gated).
- `addr.ptr()` — обычный (не `unsafe`) метод, возвращающий bare `*()` (не
  `uninit`-обёрнутый) → не Ident/Member/Index READ `uninit T`-биндинга → п.6
  не применяется.
- `&err` — `AddrOf` всегда safe (Plan 118.6 retraction, §4 amend) → не
  gated ни при каких условиях.
- Передача raw `*()` значения АРГУМЕНТОМ (в любую позицию) — не входит НИ
  в одно правило карты; `E_UNSAFE_ARG_REQUIRES_WRAP` (см. ниже) — про
  СОВСЕМ другое (передачу `uninit T`-биндинга).

Это **находка**: единственный способ реально потребовать `unsafe { }` на
таком вызове — объявить сам extern fn как `external unsafe fn` /
`extern "C" unsafe fn` (п.5). Пока decl не помечена — обёртка в
`unsafe { }` вокруг подобного вызова **ничего не гейтит и семантически
не требуется**; она держится ТОЛЬКО соглашением (см. п.9/п.10 выше — это
ИМЕННО та закономерность, воспроизведённая на ~35 реальных сайтах в
`std/net`+`std/tls` при тестировании `E_UNSAFE_UNUSED`, ниже). Действительно
ли extern C-функции, принимающие raw pointers, ДОЛЖНЫ становиться
`unsafe fn` по умолчанию (Rust-модель: «любой `extern "C"` вызов unsafe»)
— это language-policy решение владельца, вне рамок этой волны (см. `[M-
unsafe-cluster-extern-ptr-arg-policy]` followup ниже). **→ РЕШЕНО D424 (ниже).**

## D424 — raw-ptr `extern`/`external` fn ⇒ `unsafe fn` по инференсу + carve-out `E_UNSAFE_UNUSED` удаляется (Plan 174.6 M4, 2026-07-15, решение владельца)

**Закрывает** открытый вопрос находки выше (net_tcp_listen/tls_cfg_verify_pem) и followup
`[M-unsafe-cluster-extern-ptr-arg-policy]`. **Amends** D216 §9 (карта unsafe-триггеров, п.10) и D282
(классификация `extern "C" fn`). Выбран **вариант A** (vs B «externs всегда plain, снести ~35 обёрток»).

**Правило (нормативно):**
1. **Инференс unsafe.** `extern`/`external "…" fn`, чья сигнатура содержит сырой указатель (`*T` / `*mut T`
   / `*()` / `CStr`) в **параметре ИЛИ возврате**, классифицируется как **`unsafe fn` по инференсу** —
   **без keyword** (выводится из `*T` в сигнатуре; дифференциатор FFI «type-driven, без extra keyword»
   сохранён). Её вызов требует `unsafe { }` (п.5 карты, `E_UNSAFE_CALL_REQUIRES_WRAP`). Scalar/handle-only
   externs (`tls_client_cfg_new() -> CTlsCfgHandle`, `tls_cfg_verify_system(c)`) — **остаются обычными
   безопасными** (нет `*T` в сигнатуре → не `unsafe fn`).
2. **Carve-out удаляется.** Строка-gap п.10 карты (распознавание `extern_fn(...ptr-arg...)` как «unsafe-used»
   в `E_UNSAFE_UNUSED`) **снимается**: после п.1 такие вызовы требуют обёртки по-настоящему (п.5), поэтому
   обёртка genuinely-used честно, без спец-распознавания. `E_UNSAFE_UNUSED` (hard error, §выше) остаётся
   и теперь честен **без исключений**: `unsafe { }`, не покрывающий НИ ОДНОЙ реально-unsafe операции (в т.ч.
   обёртка над scalar-only extern, или ни над чем), → hard error. Жёсткий принцип владельца: «unused unsafe
   обязан флагаться, без carve-out».

**Обоснование A.** Сырой указатель в C разыменуется без проверки (валидность/время жизни/алиасинг
непроверяемы) — настоящий unsafe, честнее пометить, чем объявить безопасным (B недомаркировал бы реальный
риск, оставив опасные FFI-разыменования без визуального маркера). A даёт **0 churn** существующего кода
(`std/net`+`std/tls` ~35 сайтов становятся корректными-по-правилу, а не терпимыми по carve-out).

**Статус реализации: РЕАЛИЗОВАНО (Plan 174.6 M4, 2026-07-17, sonnet).** Checker-enforcement закрыт:
`check_unsafe_context_in_module`'s collect-фаза (`compiler-codegen/src/types/mod.rs`) фолдит
`extern`/`external` fn с `fn_sig_has_raw_ptr(fd)` (проверяет каждый параметр И возврат — рекурсивно в
`Tuple`/`FixedArray`, чтобы поймать указатель, вложенный на один уровень, напр. multi-value-return
конвенции `(*(), int)`; НЕ рекурсирует в `Array`/`Named`/generics — `[]T`/`Vec[T]` GC-управляем, не сам
по себе raw-ptr, а user-декларированный record/newtype требует резолва типа, недоступного на этом
синтаксическом проходе) прямо в `unsafe_fns` на месте сбора — без keyword, зеркалит существующую
`fd.unsafe_attr`-ветку. Бывший carve-out `E_UNSAFE_UNUSED` (D216 §21, `extern_fns`-сет used-tracking) снят
целиком (п.2) — обычный `E_UNSAFE_CALL_REQUIRES_WRAP`-гейт + used-tracking теперь покрывают такие вызовы
через тот же `unsafe_callee_name`-матч, отдельная эвристика не нужна.

Фикстуры: 2 pos-теста (`spec_tests/conformance/d424_rawptr_unsafe/d424_rawptr_unsafe_pos.nv`, свой пакет +
`d424_ffi_shim.h` — реальный линкуемый C-шим, т.к. позитивные conformance-тесты реально исполняются: (1)
wrapped-вызов raw-ptr extern принят, (2) unwrapped-вызов scalar-only extern принят) + 3 neg-теста
(`spec_tests/conformance/neg/d424_rawptr_extern_unwrapped_neg.nv` → `E_UNSAFE_CALL_REQUIRES_WRAP`;
`d424_scalar_extern_unused_unsafe_neg.nv` → `E_UNSAFE_UNUSED`, carve-out снят;
`d424_rawptr_extern_tuple_return_unwrapped_neg.nv` → тот же код на tuple-nested указателе, регресс-фиксация
`Tuple`-рекурсии).

Взрыв-оценка (`nova check std/` + `examples/ffi`): 12 сайтов на 3 файлах вскрылись и почтены добавлением
`unsafe { }` (смысл M4) — `std/src/net/mock.nv` (1: `net_addr_loopback` возвращает `*()`),
`std/src/runtime/fmt_buf.nv` (4: `f64_fmt_into`, `buf *mut u8` параметр), `examples/ffi/sqlite_mini.nv` (7:
все `mini_sqlite_*` externs; `mini_sqlite_open` — указатель вложен в tuple-возврат `(*(), int)`, живой
пример, подтвердивший необходимость `Tuple`-рекурсии). После фикса — дельта 0 (`nova check std` /
`nova check examples/ffi` чисты по `E_UNSAFE_*`).

#### `E_UNSAFE_ARG_REQUIRES_WRAP` — отдельно, НЕ про «забыл unsafe»

`E_UNSAFE_ARG_REQUIRES_WRAP` (Plan 118.5 V2, `check_unsafe_coerce_args`)
срабатывает, когда Ident-аргумент зарегистрирован в `unsafe_t_locals`
(т.е. его ТИП — `uninit T`, value-wrapper) передаётся в параметр, чей
задекларированный тип НЕ `uninit`-обёрнут — **безусловно**, вне
зависимости от `unsafe { }`-контекста самого call-сайта (эта проверка
живёт в отдельном `ConsumeCtx`-проходе, который не знает про depth/unsafe-
блоки). Единственный fix — явный narrow-cast (п.7 карты) внутри
`unsafe { }`: `unsafe { fn_name(x as T, ...) }`. Поэтому это НЕ входит
в «операции, для которых `unsafe { }`-обёртка сама по себе достаточна» —
обёртка БЕЗ narrow-cast'а всё равно ошибка.

#### `E_UNSAFE_UNUSED` — hard error на лишний `unsafe { }` (Rust `unused_unsafe`, но error не warning)

**Правило:** `unsafe { ... }` блок, внутри которого НИ ОДНА операция из
карты (пп. 1-10 выше, включая gap-строки 8-10 — они распознаются для
used-tracking, даже не будучи formally required) не была фактически
использована **на уровне этого блока** (не «съедена» вложенным `unsafe { }`
блоком глубже) → `E_UNSAFE_UNUSED` (hard error; владелец решил — не
warning, как в Rust). Nested-scope semantics — «ближайший enclosing unsafe
block» (аналог Rust): операция засчитывается ТОЛЬКО самому внутреннему
`unsafe { }` блоку, лексически её содержащему; `unsafe fn` body без
явного блока НЕ линтится (аналог Rust — атрибут функции не проверяется на
unused).

**Реализация:** `UnsafeCtx::unsafe_block_used: Vec<bool>` — стек флагов,
кадр пушится ТОЛЬКО для `Block.is_unsafe == true` (parallel к
`ptr_vars`/`unsafe_t_vars`); каждый существующий gate-сайт (пп. 1-7),
который раньше делал `if depth == 0 { error }`, теперь делает
`if depth == 0 { error } else { mark_unsafe_used() }` — так что карта
используется как ЕДИНЫЙ источник и для error-gating, и для used-tracking
(нет риска рассинхрона между ними). Gap-строки 8-10 добавляют СВОИ
used-tracking-условия (без нового error-gating).

**Побочные находки при тестировании** (`nova check std/` + `spec_tests/conformance`),
**обе устранены той же волной:**
- **Path vs Member parser bug** (не связано с этой сессией напрямую, но
  СДЕЛАЛО невозможным корректный used-tracking, поэтому исправлено здесь):
  `Type.lowercase_method(...)` (двусегментный static-call, напр.
  `RawMem.alloc(n)`) парсится в `ExprKind::Path(["RawMem","alloc"])`, а НЕ
  `ExprKind::Member` — комментарий в `parser/mod.rs` про «PascalCase.lowercase
  → stop for Member» описывает поведение, которого код НЕ реализует
  (`let _ = next_upper;` отбрасывает именно эту проверку). Из-за этого
  `unsafe_static_methods`-матчинг в `check_unsafe_context_in_module` (ветка
  `ExprKind::Member{ obj: Ident(tn), .. }`) была МЁРТВЫМ кодом для ЛЮБОГО
  static unsafe-метода — `E_UNSAFE_CALL_REQUIRES_WRAP` никогда не срабатывал
  на незавёрнутый `RawMem.alloc(n)` в ЛЮБОМ месте языка. Исправлено: новая
  `ExprKind::Path`-ветка в том же match'е.
  Второй repro — namespace-qualified static-call через `import mod as ns`
  / D289 last-segment (`ns.RawMem.method(...)`) парсится в ВЛОЖЕННЫЙ
  `Member{ obj: Member{ obj: Ident(ns), name: "RawMem" }, name: "method" }`
  (т.к. первый сегмент lowercase не входит в uppercase-Path-loop) — RawMem-
  used-tracking heuristic расширена, чтобы узнавать «receiver chain
  заканчивается сегментом RawMem» независимо от квалификации.
- **`expr_is_typed_pointer` blind spot**: не распознавал `x.ptr()`/
  `x.as_ptr()` zero-arg getter-вызов как typed-pointer expression (только
  `&x`/`*p`/`as *T`/Ident-в-ptr_vars). Из-за этого `let p = buf.ptr()`
  (БЕЗ явной type-аннотации) никогда не регистрировался в `ptr_vars`, что
  тихо отключало п.2/3/4 карты (и `E_PTR_NO_DISPLAY_USE_DEBUG_STR`) для
  ЭТОГО распространённого идиома — задолго до этой сессии (репро:
  `spec_tests/conformance/neg/d216_ptr_index_read_neg.nv`, которая
  полагалась ИСКЛЮЧИТЕЛЬНО на позднюю codegen-стадию
  `E_POINTER_OP_USE_METHOD`, а не на checker-стадию). Исправлено: расширен
  `expr_is_typed_pointer` — распознаёт этот идиом напрямую (единая точка,
  выгодополучатели — ptr_var-регистрация, Index-gate, order-compare-gate,
  interpolation-ban, И новый `E_UNSAFE_UNUSED`).

**Разбор std/-флагов этой волной (36 сайтов, все разрешены):**
- `std/tls/stream.nv` (5 сайтов, напр. `unsafe { tls_wants_write(session) }`)
  — блок реально лишний (аргументы — только `CTlsHandle`-хендл, ни одного
  raw pointer) → **unsafe-обёртка убрана** (fix the block).
- `std/net/mock.nv` (1 сайт, `unsafe { net_addr_loopback(0) }`) — тот же
  случай (нет pointer-аргумента) → **unsafe-обёртка убрана**.
- `std/net/addr.nv` (9), `dns.nv` (1), `error.nv` (1), `std/tls/client.nv`
  (5), `server.nv` (6) (23 сайта) — extern-fn-call с raw-pointer-аргументом
  (`.ptr()`-идиом) → **карта расширена** (п.10) — обёртки оставлены как есть.
- `std/collections/vec/{core,mutate,restructure}.nv` (7 сайтов) —
  cross-module `RawMem.*` вызов → **карта расширена** (п.9) — обёртки
  оставлены как есть.

**Neg/pos conformance:** `spec_tests/conformance/neg/d216_unused_unsafe_neg.nv`
(`unsafe { 1 + 1 }` → `E_UNSAFE_UNUSED`), `spec_tests/conformance/d216_unused_unsafe_pos.nv`
(5 позитивных случаев — `.write()` intrinsic, `raw &x`, `unsafe fn` call,
`.read_at()` intrinsic, `uninit T` narrow-cast — НЕ флагуются).

**Followups (вне рамок этой волны):**
- `[M-unsafe-cluster-extern-ptr-arg-policy]` — language-policy: должны ли
  `extern`/`external` fn, принимающие `*T`-аргумент, становиться `unsafe fn`
  по умолчанию (Rust-модель) или остаться opt-in (текущая модель)? Решение
  владельца; при «да» — retrofit ~35+ `std/net`+`std/tls` деклараций.
- `[M-unsafe-cluster-cross-module-collector]` — расширить `unsafe_fns`/
  `unsafe_static_methods` collector, чтобы просматривать `import`-нутые
  модули целиком (закрывает п.9 gap полностью, не только `RawMem`).
- `[M-unsafe-cluster-intrinsic-method-gate]` — полноценный call-site gate
  для raw-pointer intrinsic-методов (п.8), требующий type-inference на
  этом checker-проходе (сейчас чисто syntactic pass).

### §22. CStr type (Plan 118.1 Ф.4 closeout, 2026-06-05)

`type CStr(*u8)` newtype declared в std/ffi/cstr.nv — FFI-compatible C-string handle.
ABI: marshals к `const char*` / `uint8_t*` (single positional `*u8` field).

**Invariant**: instances must satisfy `ptr[strlen(ptr)] == '\0'`.

> **⚠ AMEND Plan 199 (2026-07-11) → [D418](08-runtime.md#d418-new--str-без-nul-терминатора-c-ffi-через-copy-based-cstr-as_cstr-plan-199-retracts-d26-nul-termination).**
> Nova `str` больше НЕ несёт trailing-NUL инвариант (retracts D26
> §«Nul-termination» rules 1-3). Conversion API переименован `as_cstr` →
> **`@to_cstr`** (`to_` correctly names a COPY — a str→CStr conversion под D418
> ВСЕГДА копирует; `as_` был misnomer): две D84-arity-перегрузки —
> GC-allocating `@to_cstr()` и zero-alloc caller-buffer `@to_cstr(buf, size)`.
> CStr's own invariant (satisfied by construction, not inherited from the source
> `str`) не меняется. «enabling zero-copy conversion» ниже — исторический текст,
> более не верно.

**Conversion methods (`@to_cstr`, copy-based; Plan 199 Ф.2/Ф.3, owner decision
2026-07-11):** str → CStr conversions реализованы как pure-Nova methods в
`std/ffi/cstr.nv`:

```nova
// GC-allocating copy: fresh byte_len()+1 buffer + '\0'. Scans for an embedded
// NUL (a classic silent-C-truncation footgun) and panics on the safe path.
export fn str @to_cstr() -> CStr {
    ro bytes = @bytes()
    for b in bytes {
        if b == 0 { panic("to_cstr: embedded NUL byte in str (would truncate C-string)") }
    }
    mut buf = Vec[u8].new().cap(@byte_len() + 1)
    buf.append(bytes)
    buf.push(0 as u8)
    unsafe { CStr(buf.ptr()) }
}
// Zero-alloc: copy into a caller-provided buffer, clamping to buf_size-1 +
// terminator (TRUNCATING, no scan — the explicit "I own the buffer" hot path;
// the `.min(buf_size-1)` clamp keeps the terminator write in-bounds).
export fn str @to_cstr(buf *mut u8, buf_size int) -> CStr
    requires buf_size > 0
{
    ro n = @byte_len().min(buf_size - 1)
    unsafe { RawMem.copy(@ptr(), buf, n); buf.write_at(n, 0 as u8) }
    unsafe { CStr(buf) }
}
```

Использует existing builtins: `str.@bytes()`/`@ptr()`/`@byte_len()` +
`Vec[u8]`/`RawMem`. C primitives НЕ требуются — Nova-side alloc+copy+terminate
достаточно (no source-`str` invariant to reuse под D418).

**V1 simplifications (explicit followups, not silent):**

- `[M-118.1-cstr-nul-check]` — ✅ CLOSED 2026-06-08. `@to_cstr()` scans the str
  bytes for an embedded NUL and panics (interior `0x00` would truncate the
  C-string at that byte); the caller-buffer `@to_cstr(buf, size)` overload
  truncates by design (no scan — the explicit hot path). `panic` is an
  always-available compiler intrinsic ([panic-assert-intrinsic] 2026-07-11) —
  needs no import regardless of cstr.nv's prelude status.
- `[M-118.1-cstr-to-cstr-distinct-copy]` — SUBSUMED by Plan 199 Ф.3: `@to_cstr()`
  IS the owning copy now (both overloads copy — the `as_`/`to_` naming tension
  that deferred a distinct `to_cstr` is resolved: `to_cstr` is the canonical,
  correctly-named copying conversion; `as_cstr`/`as_cstr_unchecked` retired).

Closes [M-118.1-cstr-literal] (was: «add c"hello" prefix-literal»; superseded by D26 invariant).
Closes [M-118.1-cstr-runtime-wiring] (was: «C primitive ABI wiring»; pure-Nova approach makes it unnecessary).

### Diagnostic codes (new)

**Errors:**
- `E_UNSAFE_REQUIRED` — pointer op (`*expr` Deref / unsafe fn call) outside
  unsafe context (block.is_unsafe = false AND not в `unsafe fn` body).
  Active enforcement через `check_unsafe_context_in_module` walker pass с
  depth counter — D216 §8 V1 ENFORCED 2026-06-02.
  **Plan 118.6 amend (2026-06-16):** `&x` AddrOf (safe promote path) no longer
  triggers `E_UNSAFE_REQUIRED`. Only raw stack `unsafe { &x }` and Deref/unsafe
  fn calls remain gated.
- `E_UNSAFE_CALL_REQUIRES_WRAP` — calling `unsafe fn` без `unsafe { }`
  wrap. Active enforcement через `check_unsafe_context_in_module` walker
  с pre-collected unsafe_fns: HashSet<String>. D216 §9 V1 ENFORCED
  2026-06-02 (commit abd4be4603b)
- `E_UNSAFE_ATTR_DEPRECATED` — `#unsafe fn` / `#unsafe external fn` syntax;
  use `unsafe fn` / `external unsafe fn` instead (Plan 118.1.7)
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
- `E_POINTER_RO_MUT_METHOD` — `p.mut_method()` где pointee readonly (`*T`).
  **ENFORCED 2026-08-08 (Plan 221.1 №387, window pptr-ro-guard)** — see §5
  amendment above (was declared-but-toothless since Plan 118).
- `E_PTR_CAST_INVALID_TARGET` — `p as bool / f64 / ...`
- `E_INVALID_POINTER_MODIFIER` — `*const T` и др.
- `E_POINTER_PREFIX_MODIFIER` — `ro`/`mut`/`unsafe` перед `*` в type-position
  (`mut * T` / `ro * T` / `unsafe * T`); use postfix `*mut T`/`*ro T`/`*unsafe T`
  или binding `mut x *T`. Extends `E_INVALID_POINTER_MODIFIER` (Plan 138.5 §1).
- `E_DUPLICATE_POINTER_MODIFIER` — `*ro mut T`
- `E_PARSE_POINTER_TYPE_INCOMPLETE` — `*` без type
- `E_REALTIME_POINTER_OP` — pointer op в `#realtime fn` body. Active
  enforcement — D216 §20 + Plan 113 D172 V1 ENFORCED 2026-06-02
  (commit 6752565f453)
- `E_UNSAFE_HANDLER_BUILTIN_ONLY` — user-defined unsafe_handler attempt
- `E_AMP_CONST_BINDING` — `&const_value`
- `E_AMP_LITERAL` — `&42`
- `E_AMP_RECORD_LITERAL` — `&Record { ... }` без named binding (Plan 118 §4 amend)
- `E_ADDR_OF_NON_LVALUE` — `addr_of` / `addr_of_mut` applied к non-Ident /
  Member / SelfAccess expression (rvalue / temporary). Mirrors Rust's
  «cannot take address of a temporary». Plan 118.1 closeout 2026-06-05.
- `E_ADDR_OF_MUT_REQUIRES_MUT_BINDING` — `addr_of_mut` applied к ro binding
  (let без `mut`, ro parameter, ro field). Mirrors `E_PARAM_NOT_MUT` /
  `E_LOCAL_NOT_MUT` pattern (D108.1 / D108.2). Plan 118.1 closeout 2026-06-05.
- `E_ADDR_OF_REMOVED` — `addr_of()` / `addr_of_mut()` called after retirement
  (Plan 118.6 D216 §4, 2026-06-16). Use `&x` instead. Both functions are
  removed from prelude; any surviving call site raises this error.
- `E_PTR_NO_DISPLAY_USE_DEBUG_STR` — `"${p}"`
- `E_VARARG_NOT_SUPPORTED` — vararg FFI call
- `E_CAST_RAW_FN_TO_CLOSURE` — `*fn → fn` cast outside unsafe
- `E_UNSAFE_UNUSED` — `unsafe { }` block with no operation from the §21 map
  inside it (unsafe-cluster, Plan 174.5 followup, 2026-07-11). Hard error
  (owner decision — Rust's `unused_unsafe` is a warning). See §21 for the
  full map + implementation + std/ triage.

**Warnings:**
- `W_UNSAFE_GC_TRIGGER` — GC trigger внутри unsafe с pointer in scope
- `W_PTR_AS_INT_GC_HASH_HAZARD` — `p as int` как HashMap key
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
| **Nova V2** (Plan 118) | **`*T` family** + `unsafe` | `unsafe { }` + `unsafe fn` (D2 amend) | `Option[*T]` + NPO | `p.field`/`p.method()` one-level | gated unsafe → `*unsafe T` |

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

## D216 V2 amend (2026-06-04) — universal right-binding rule для type-level modifiers + `unsafe T` first-class

> **⚠️ PARTIALLY SUPERSEDED — Plan 138.5 (2026-06-11):** часть V2, касающаяся
> **указателей** (outer pointer-mut как type-wrapper: `mut * T = Mut(Pointer(T))`,
> `unsafe * T = Unsafe(Pointer(T))`, NPO-таблица §V2.4 по outer-wrapper),
> **РЕТРАКТИРОВАНА**. FINAL pointer model = pointee-mut **постфикс** только
> (`*mut T`/`*ro T`/`*unsafe T`), reassignability = binding (`let`/`mut`, D36),
> nullable = `Option[*T]` только. Все prefix-формы перед `*` запрещены
> (`E_POINTER_PREFIX_MODIFIER`, §1). **СОХРАНЯЕТСЯ:** §V2.3 `unsafe T`
> value-wrapper (MaybeUninit-style) — про значение, ортогонально указателям; и
> универсальное right-binding для **value-T** модификаторов (`ro T`/`mut T`/
> `unsafe T` wrappers, codegen-transparent). См. retract-пометки в §V2.1/§V2.2/
> §V2.4/§V2.6 и §V3.2/§V3.3/§V3.4 ниже.

> **Status:** 🆕 SPEC LANDED 2026-06-04 (parser/codegen migration — Plan 118.5
> NEW sub-plan, см. follow-up markers ниже). Этот amend был **breaking change**
> для existing `*ro T` / `*mut T` / `*unsafe T` syntax — pointer-часть позже
> ретрактирована (Plan 138.5, см. banner выше).

### Motivation

Inconsistency discovered 2026-06-04: parser применяет «right-binding rule»
для `ro T` (`TokenKind::KwRo` → recursive `parse_type()` → `Readonly(inner)`),
но pointer-modifier syntax `*ro T` / `*mut T` / `*unsafe T` использует
**inline modifier-after-star** form. V2 попытался унифицировать через
prefix-wrappers; Plan 138.5 (2026-06-11) выбрал обратное — **postfix pointee
canonical, prefix запрещён** (один указатель-модификатор = pointee, постфикс).

| Token | Семантика (FINAL, Plan 138.5) |
|-------|-------------------------------|
| `ro T` | `Readonly(T)` — value-T wrapper, codegen-transparent (KEPT) |
| `mut T` | `Mut(T)` — value-T wrapper, codegen-transparent (KEPT, §V2.2b) |
| `uninit T` | `Uninit(T)` — value-T wrapper, MaybeUninit (KEPT, §V2.3; §10a rename Plan 174.5 2026-07-11, was `unsafe T`) |
| `consume T` | consume wrapper (receiver/field/decl, см. D162) |
| `*T` | `Pointer(T)` — pointee **ro** (≡ `*ro T`, D246; pointee-mut из типа, не от binding) |
| `*ro T` | ❌ `E_REDUNDANT_POINTER_RO` (избыточно: `*T` уже ro; fix-it `*T`) — D246 |
| `*mut T` | `Pointer(Mut(T))` — explicit mut pointee (единственный опт-ин на `*p = …`) |
| `*uninit T` | `Pointer(Uninit(T))` — **CANONICAL** pointer к possibly-uninit T (§10a rename, was `*unsafe T`) |
| `mut * T` | ❌ `E_POINTER_PREFIX_MODIFIER` (prefix перед `*` запрещён; use `mut x *T` binding) |
| `ro * T` | ❌ `E_POINTER_PREFIX_MODIFIER` |
| `uninit * T` | ❌ `E_POINTER_PREFIX_MODIFIER` (RETIRED `Unsafe(Pointer)`; nullable = `Option[*T]`, FFI nullable-uninit = `Option[*uninit T]`) |

### §V2.1 — universal right-binding rule (value-T modifiers)

**Правило (KEPT для value-T):** type-level modifier применяется к ВСЕМУ что
справа от него до конца type-expression, либо до следующего modifier. Это про
**value-T wrappers** (`ro`/`mut`/`unsafe`/`consume`), codegen-transparent.

> **⚠️ Pointer-часть RETRACTED (Plan 138.5):** `*` НЕ «pure constructor с
> prefix-модификатором снаружи». Указатель несёт **postfix pointee**-модификатор,
> сразу после `*` (`*mut T`/`*ro T`/`*unsafe T`). Prefix перед `*`
> (`mut * T`/`ro * T`/`unsafe * T`) — `E_POINTER_PREFIX_MODIFIER` (§1).

Modifiers (выровненная иерархия) — применимы к value-T и как postfix pointee:
- `ro` — readonly (compile-time immutability)
- `mut` — mutable (compile-time mutability marker)
- `unsafe` — unsafe (init/layout/aliasing contracts off)
- `consume` — consume (unique ownership, D162 follow rules)

Parser pattern (FINAL):
```
TYPE     := MODIFIER TYPE | BASE_TYPE | '*' POINTEE | '[' ']' TYPE | ...
POINTEE  := POINTEE_MOD POINTEE | TYPE        // постфикс после '*'
MODIFIER := 'ro' | 'mut' | 'unsafe' | 'consume'
POINTEE_MOD := 'ro' | 'mut' | 'unsafe'
```

Каждый value-modifier — `TypeRef::<Modifier>(Box<TypeRef>)` wrapper. `*` —
конструктор `Pointer(Box<TypeRef>)`, чей pointee может нести `ro`/`mut`/`unsafe`
**постфиксом**. Prefix-модификатор перед `*` запрещён (см. banner выше).

### §V2.2 — chain semantic (multi-modifier / multi-pointer)

> **⚠️ RETRACTED (Plan 138.5):** трактовка «`mut * T` = `Mut(Pointer(T))`
> (outer pointer-mut как type-wrapper)» **отозвана**. Outer-pointer-mut в типе
> больше не существует — reassignability указателя выражается **binding'ом**
> (`let`/`mut`, D36), а pointee-мутабельность — **postfix** после `*`.

FINAL chains читаются с postfix-pointee; reassignability — отдельно через binding:

```nova
*T                  // Pointer(T) ≡ Pointer(Readonly(T)) — pointee ro (D246; *T ≡ *ro T)
*mut T              // Pointer(Mut(T))       — explicit mut pointee (единственный опт-ин)
*uninit T           // Pointer(Uninit(T))    — valid (non-null) ptr к possibly-uninit T (§10a rename)
ro p *mut *T        // L3 из типа (D246): внешний *mut (writable), внутренний *T (ro pointee)
                    // позиционно-независимо; binding ro = только p не reassignable
ro p *T             // binding ro: p фиксирован; pointee ro
mut p *T            // binding mut: p reassignable; pointee ro (*p = … ❌ — L1 mut ≠ mut-pointee)
mut p *mut T        // binding mut: p reassignable; pointee mut, writable (*p = … ✅)
mut p *uninit u8    // binding mut: p reassignable; pointee possibly-uninit, НЕ writable (*p/… ❌; №358 2026-08-05 — запись только через *mut uninit u8)
```

RETRACTED (теперь parse error `E_POINTER_PREFIX_MODIFIER`):
```nova
mut * T             // ❌ — вместо: binding `mut p *T`
ro  * T             // ❌ — вместо: binding `let p *T` (или `ro p *T`)
uninit * T          // ❌ — RETIRED Unsafe(Pointer); вместо: Option[*T] / *uninit T (pointee)
mut * ro * T        // ❌ — вместо: `mut p *ro *... ` postfix-chain
ro p mut * uninit T // ❌ — вместо: `mut p *uninit T` (binding mut + pointee uninit)
```

Канонический пример FFI out-param (FINAL; §10a rename Plan 174.5 2026-07-11 — was `*unsafe u8`):
```nova
external fn os_read(fd int, buf *uninit u8, n int) -> int
//                              pointee uninit byte; buf non-null *; OS fills, returns count.
// Если sam buf переприсваивается в теле — `mut buf` на стороне caller's binding.
```

### §V2.3 — `unsafe T` semantic (MaybeUninit-style)

> **✅ KEPT (Plan 138.5):** `unsafe T` value-wrapper **сохраняется без изменений**
> — он про **значение** (maybe-uninit T-typed память), ортогонален указателям и
> prefix-запрету. `mut x unsafe T` = mut-binding к maybe-uninit value. Указательная
> форма «ptr к uninit T» = `*unsafe T` (postfix pointee, §1), а НЕ `* unsafe T` с
> пробелом и НЕ `unsafe *` (последнее retired).

`unsafe T` означает «T-typed memory с снятыми init/layout/aliasing contracts».
Caller asserts validity at use sites. Concretely:

- **Init:** значение **может быть uninitialized** — read без prior write — UB
- **Layout:** alignment / size — **same as T** (не bitwise opaque)
- **Identity:** bit-pattern **valid для T** at каждом read site
- **Aliasing:** Nova exclusivity rules off (но atomicity не гарантирована)

**Operations:**
- Read `unsafe T` value — **requires `unsafe { }` block** (caller asserts init)
- Write `unsafe T` slot — **safe** (transitions to valid)
- Cast `unsafe T → T` — **requires `unsafe { }` + value-level assertion**
  (e.g. `unsafe { x as T }` или dedicated `assume_init` builtin)

Соответствует Rust `MaybeUninit<T>` semantic, но как type modifier вместо
generic wrapper.

**Default-init для `unsafe T` bindings:** `mut x unsafe T` — slot выделена,
но не initialized. Compiler-emitted runtime check НЕ выполняется (это и
есть escape hatch).

### §V2.4 — Option B / niche optimization (FINAL, Plan 138.5)

> **⚠️ RETRACTED-and-simplified (Plan 138.5):** старая таблица зависела от
> «outermost pointer modifier» с `Unsafe(Pointer)` (16-байтные строки). После
> retire `unsafe *` (Unsafe(Pointer)) — указатель **ВСЕГДА** non-null, поэтому
> NPO применяется **универсально** (8 байт), без зависимости от модификатора.

Все указатели (`*T`/`*ro T`/`*mut T`/`*unsafe T`) — **guaranteed non-null**
(§1). Поэтому `Option[*…]` всегда NPO-eligible (8 байт, null = None):

| Тип | Может содержать null? | NPO размер |
|-----|----------------------|------------|
| `Option[*T]` (≡ `Option[*ro T]`) | ❌ | 8 байт ✅ |
| `Option[*mut T]` | ❌ | 8 байт ✅ |
| `Option[*unsafe T]` | ❌ pointer non-null; pointee uninit OK | 8 байт ✅ |
| `Option[*mut *ro T]` (chain) | ❌ | 8 байт ✅ |

**Nullable raw-uninit (FFI, C `T*` может быть NULL):** `Option[*unsafe T]` —
None = NULL, Some = non-null ptr к possibly-uninit T (validity асертится на
deref в `unsafe {}`). Это единственная nullable-форма; отдельного raw-nullable
`unsafe * T` больше нет. Nested `Option[Option[*T]]` — fallback к tagged repr +
`W_OPTION_DOUBLE_NESTED` (без изменений, см. §7 V4).

### §V2.5 — migration path

> **⚠️ REVISED (Plan 138.5):** шаги про prefix-pointer-форму отозваны. FINAL =
> postfix pointee canonical; prefix перед `*` — hard error
> `E_POINTER_PREFIX_MODIFIER`. Value-T wrappers (`Mut`/`Unsafe`/`Readonly`,
> codegen-transparent) сохраняются (AST/codegen/checker шаги ниже про них KEPT).

1. **AST changes (KEPT для value-T):**
   - `TypeRef::Mut(Box<TypeRef>)` / `TypeRef::Unsafe(Box<TypeRef>)` — value-T
     wrappers (codegen-transparent / MaybeUninit). Также используются как
     **pointee** содержимое внутри `Pointer(...)` (`*mut T` = `Pointer(Mut(T))`).
   - `TypeRef::Pointer(Box<TypeRef>)` — конструктор; pointee несёт `ro`/`mut`/
     `unsafe` **постфиксом**. Outer-pointer-mut wrapper (`Mut(Pointer)`) больше
     не строится из синтаксиса (prefix запрещён).

2. **Parser changes (FINAL):**
   - `mut T` / `unsafe T` parse arms (value-T) — recursive. KEPT.
   - `*` Star branch парсит postfix pointee modifier (`*mut`/`*ro`/`*unsafe`).
   - **Reject** `ro`/`mut`/`unsafe` token непосредственно перед `*` →
     `E_POINTER_PREFIX_MODIFIER` (НЕ warning — hard error в Plan 138.5 enforce-фазе).

3. **Codegen changes (KEPT):**
   - `Mut(T)` / `Unsafe(T)` value-wrapper — C-level **no-op** для primitive T.
   - `Pointer(Mut(T))` / `Pointer(Readonly(T))` / `Pointer(Unsafe(T))` — pointee
     модификаторы; все emit `T*` ABI (mut/ro различаются на assignment-check).

4. **Type-checker changes (KEPT):**
   - `Unsafe(T)` value read → `E_UNSAFE_T_READ_REQUIRES_WRAP` (§V2.3 value-wrapper).
   - `Unsafe(T)` value write — safe; `T → Unsafe(T)` implicit; `Unsafe(T) → T`
     explicit (unsafe cast).
   - NPO — универсальный (§V2.4, все `*…` non-null → 8 байт).

5. **Migration sweep (Plan 138.5 Ф.2 enforce):**
   - prefix usages `mut */ro */unsafe *` → postfix `*mut T`/`*ro T`/`*unsafe T`
     (или binding `mut x *T` / nullable `Option[*T]`). Основной call-site:
     `std/runtime/raw_mem.nv` (`dst mut * u8` → `dst *mut u8`).
   - Retire `Unsafe(Pointer)` (`unsafe * T`): → `Option[*T]` / `Option[*unsafe T]`.

6. **Spec amends downstream:**
   - D36 (binding ro default) — reassignability указателя = binding cross-ref.
   - D184 (return mut default) — pointer-return = **pointee**-mut.
   - D216 §1-3 — postfix pointee canonical (выполнено).
   - D217 (FFI intrinsics) — RawMem signatures postfix (Plan 138.5 Ф.2).

### §V2.6 — backward compatibility

> **✅ RESTORED (Plan 147 D246, 2026-06-12):** утверждение «`*T ≡ *ro T`»
> (always-ro pointee) **восстановлено** — flip-scan-draft (`*T` наследует
> binding-`current`) **ОТКЛОНЁН** (тип не самодостаточен). Под
> [D246](#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee) pointee-mut
> задаётся **только типом** (`*mut T`), позиционно-независимо. Поэтому `mut p *mut T`
> — **явная и единственная** форма writable-pointee при mut-binding; `mut p *T` даёт
> **ro**-pointee (L1 mut ≠ mut-pointee). `*ro T` → `E_REDUNDANT_POINTER_RO`
> (избыточно). Codegen `promote_pointer_pointee_mut` (flip-scan seed) **УДАЛЯЕТСЯ**
> в Ф.3 (наследование pointee-mut от binding запрещено D246). Постфикс-canonical
> и prefix-ban (ниже) — **сохраняются**.

> **Codegen-status UPDATE (D246, Plan 147 Ф.3, 2026-06-12):** legacy codegen
> inherit-current (commit `38360c30d80`, `field_type_with_binding_mut` /
> `promote_pointer_pointee_mut` — auto-promote `mut p *T`-поля в `Pointer(Mut(…))`)
> — **УДАЛЯЕТСЯ** под три-осевой моделью: наследование pointee-mut от binding
> запрещено (L1 ⊥ L3). После удаления writable buffer требует **явного** поля
> `mut data *mut T` (vec_owned.nv уже его несёт, line 104). `[M-138.2-v2-propagation-impl-gap]`
> остаётся закрытым (V2-propagation как механизм — moot под D246).

**FINAL (Plan 138.5, amend Plan 147 D246) — НЕТ grace-period для prefix:**
- `*mut T` / `*unsafe T` (postfix) — canonical. `Pointer(Mut(T))` /
  `Pointer(Unsafe(T))`. `*mut T` — единственный опт-ин на mut-pointee.
- `*T` — pointee **ro**, `*T ≡ *ro T` УНИВЕРСАЛЬНО (D246; pointee-mut из типа, не
  от binding).
- `*ro T` (postfix) — **hard error** `E_REDUNDANT_POINTER_RO` (избыточно: `*T` уже
  ro; fix-it `*T`). D246.
- `mut * T` / `ro * T` / `unsafe * T` (prefix перед `*`) — **hard error**
  `E_POINTER_PREFIX_MODIFIER` (НЕ warning). Migrate → postfix или binding-`mut`.
- `unsafe * T` (`Unsafe(Pointer)`, старый raw-nullable) — **RETIRED**. Nullable =
  `Option[*T]` (NPO); FFI nullable-uninit = `Option[*unsafe T]`.
- `*unsafe T` (postfix) сохраняет смысл «ptr к possibly-uninit T» (pointee unsafe).

> **Historical (V2 grace-period draft, отозвано Plan 138.5):** ранее
> планировался `W_DEPRECATED_POINTER_INLINE_MODIFIER` для postfix-формы и
> миграция на prefix `mut * T`. Это направление **обратно** финальной модели и
> не реализуется.

### §V2.7 — follow-up markers

- `[M-118.5-right-binding-migration]` — **SUPERSEDED by Plan 138.5** (prefix
  pointer-form retired; postfix pointee canonical + `E_POINTER_PREFIX_MODIFIER`
  enforce — Plan 138.5 Ф.2). Value-T wrapper parsing остаётся.
- `[M-118.5-unsafe-t-readwrite-semantics]` — type-checker E_UNSAFE_T_READ_REQUIRES_WRAP (KEPT, §V2.3)
- `[M-118.5-mut-t-vs-binding-distinction]` — ✅ CLOSED (§V2.2b: `mut T` value-wrapper
  transparent; pointer-mut = pointee postfix / binding, Plan 138.5)
- `[M-118.5-consume-as-type-modifier]` — generalize `consume` (currently
  receiver/field/decl) к универсальный type wrapper
- `[M-118.5-d218-maybeuninit-duplication]` — ✅ CLOSED (D218 RETRACTED — MaybeUninit
  subsumed `unsafe T`)
- `[M-118.5-npo-recalculation]` — NPO теперь универсальный (§V2.4 FINAL — все `*…`
  non-null → 8 байт); recalculation сводится к «всегда NPO для pointer-inner»
- **three-axis (Plan 147 D246, supersedes flip-scan-draft):** `*T ≡ *ro T`
  **восстановлено** УНИВЕРСАЛЬНО; pointee-mut из типа (`*mut T`), не от binding;
  `*ro T` → `E_REDUNDANT_POINTER_RO`. См.
  [D246](#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee).

### Cross-amend impact

- **D33** (binding propagation) — extend rule к value-T `mut` / `unsafe` / `consume`
- **D216 §1-3** — pointer modifier syntax = **postfix pointee canonical**
  (Plan 138.5; prefix перед `*` запрещён)
- **D36** (binding ro default) — reassignability указателя = binding (не тип)
- **D184** (return mut default) — pointer-return ставит **pointee**-mut
- **D217** (FFI intrinsics) — RawMem signatures postfix pointee
- **D218** (slice + MaybeUninit) — MaybeUninit subsumed `unsafe T` (см. D218 RETRACTED)
- **D162** (consume types) — extend для `consume T` type wrapper position

### Why now

Right-binding rule **уже** работает для `ro` (parser-verified) — для **value-T**
модификаторов это правило сохраняется. Для **указателей** Plan 138.5 выбрал
postfix-pointee canonical: один модификатор = pointee, постфикс после `*`;
reassignability = binding (`let`/`mut`). Это убирает путаницу «двух mut» (outer
pointer-mut в типе vs pointee-mut), особенно в return-позиции (D184×D216).

`unsafe T` first-class также unlock'ает MaybeUninit semantic без duplication
с Plan 118.2 D218 — это **simplification**, не addition.

---

## ~~flip-scan-draft~~. Указатели: running-current flip-scan модель (**RETRACTED**)

> **Status:** ❌ **RETRACTED 2026-06-12 (Plan 147 Ф.1).** Черновик flip-scan
> (commit `befe92c`, SPEC-ONLY — кода никогда не было) **ОТКЛОНЁН** adversarial-
> критикой: модель делала `*T` **контекстно-зависимым** (наследует binding-`current`),
> поэтому указательный тип переставал быть **самодостаточным** (4 BLOCKER: один и
> тот же `*T` означал ro в одной позиции и mut в другой; double-ptr flip-chain
> нечитаем; cast/generic-позиции требовали спец-исключений; «redundant»-проверка
> ломалась на параметрах). Заменён на
> [**D246 — три оси мутабельности**](#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee).
> `*T ≡ *ro T` восстановлено **универсально**. Ошибка `E_REDUNDANT_POINTER_MODIFIER`
> (была бы flip-scan-only) **никогда не реализовывалась**; под D246 заменена на
> `E_REDUNDANT_POINTER_RO` (`*ro T` → fix-it `*T`). Всё содержимое flip-scan-draft ниже
> удалено как недействительное; оставлен только этот retract-баннер для якоря ссылок.

---

## D246. Три оси мутабельности: L1 binding / L2 view / L3 pointee

> **Status:** ✅ FULLY IMPLEMENTED. Ф.1-Ф.6 LANDED 2026-06-12 (Plan 147).
> **AMENDED Ф.7** 2026-06-17 (Plan 147 Ф.7): checker enforcement gaps closed —
> ro-binding + param index-freeze (`E_READONLY_CONTENT`), redundant modifier oracle.
> **AMENDED 2026-07-23** ([M-ro-launder-via-mut-binding], [Plan 224](../../docs/plans/224-ro-launder-l1-coercion.md)):
> closes the L1×L2-cross-binding coercion gap — §«P8 AMEND (2026-07-23)» below
> **RETRACTS** the old P8 prose («coercion по оси content (L2), независимо от
> L1»); see the new §«Таблица конверсий между binding'ами (L1,L2)источник →
> (L1,L2)цель»** and ORACLE rows **G/H**.
> **AMENDED 2026-07-24 §72** ([M-ro-launder-fullstack-value-exemption],
> [Plan 226](../../docs/plans/226-ro-launder-l1-coercion.md), owner decision:
> "ДА — «полностью-стековый value-тип, проба G остаётся neg»"): РАСШИРЯЕТ
> исключение из P8/таблицы конверсий с «голый скаляр-примитив» на «ПОЛНОСТЬЮ-
> СТЕКОВЫЙ value-тип» — рекурсивный предикат `is_fully_stack_value`
> (types/mod.rs): тип И ВСЕ его поля транзитивно без кучевых (`Vec`/
> `HashMap`/`Set`/heap-record/`Array`). `str` — ОТДЕЛЬНОЕ исключение по
> **immutability** (D26), не по стековости (см. §«ORACLE G» ниже). **Граница
> НЕ сдвинута:** проба G (value-record С кучевым полем) остаётся `❌`; проба
> E (value-record БЕЗ кучевых полей, все скаляр-поля) RE-RECLASSIFIED
> neg→pos — теперь `✅` (квалифицируется как fully-stack).
> Реализация — [Plan 147](../../docs/plans/147-pointer-mut-flip-scan-model.md)
> (Ф.2 parser / Ф.3 checker / Ф.4 migration / Ф.5 tests / Ф.7 enforcement).
> **Supersedes** flip-scan-draft (отклонён). **Восстанавливает** D216 §V2.6
> «`*T ≡ *ro T`». **Источник:** 2 design-workflow (critique `wkx3dytr1`,
> value-side `wlqgc2nyk`, synthesis `w9nktq8x1`) + ~15 раундов ревью.
> **Гейтит** Plan 139 `[M-139-f0-lang-item-decl]` (str-поля `ptr *u8`).
> **AMENDED 2026-08-01 (решение владельца 2026-08-01, срочный пакет
> звучности mut/захватов):** дословная норма владельца — «Присваивание ro в
> mut возможно, ЕСЛИ все поля тоже значения на стеке РЕКУРСИВНО;
> MetricsRegistry содержит типы на куче, и мы начинаем их менять после
> присваивания — это неверно» — **ПОДТВЕРЖДАЕТ буквально** уже действующий
> §72 (`is_fully_stack_value`, 2026-07-24) без изменения границы: `Mutex`/
> `HashMap` — кучевые поля (не `AllocKind::Value`-record/scalar/`str`), так
> `MetricsRegistry{lock Mutex, counters HashMap[...], gauges HashMap[...]}`
> уже `❌` под §72 (проба-G-класс) на всех трёх энфорснутых каналах
> (let-init / call-argument mut-параметр / return, `is_fully_stack_value`
> at `compiler-codegen/src/types/mod.rs:10455,16179,35290`). **Живой разбор
> находки владельца:** `nova-polaris/src/metrics.nv`'s методы (`mut lock =
> @lock`, `mut counters = @counters`) сегодня компилируются НЕ потому, что
> этот L1-Ident-канал имеет дыру — источник там `@field` (self-field
> access), НЕ голый `Ident`, поэтому `check_readonly_source_coerce`'s
> `if let ExprKind::Ident(name) = &value.kind` вообще не матчит. Это ВТОРОЙ,
> независимый канал той же дыры — **field-launder** — закрыт отдельно ниже
> (см. «§73. Field-launder канал (`mut x = @field`/`obj.field`)»,
> `[M-router-handler-mut-capture-escape-soundness]` реестр, срочный пакет
> звучности 2026-08-01).

### Что

**Происхождение:** `ro` = **r**ead-**o**nly, `mut` = **mut**able.

Мутабельность в Nova задаётся **тремя ортогональными осями**; каждая
**самодостаточна** (C1 — тип/binding читается без контекста):

| Ось | Что задаёт | Синтаксис |
|---|---|---|
| **L1 — binding** | переприсваиваемость **имени** (`x = …`) + корень прав записи через имя | `ro`/`mut` **перед именем** (никогда в типе) |
| **L2 — view** | транзитивный ro/rw по **owned-графу** значения (`.field`/`[i]`); **СТЕНА на каждом `*`** | `ro`/`mut` **перед типом** value/record |
| **L3 — pointee-capability** | можно ли писать **за `*`** (`*p = …`); реально **В ТИПЕ**, позиционно-независимо | **постфикс**: `*T`(ro) / `*mut T`(mut) |

**Принцип (1 строка):** `ro` — дефолт везде; пишется только опт-ин (`mut x`,
`mut T`, `*mut T`). L2 транзитивно морозит owned-граф и **упирается в стену на
`*`**; за указателем — только L3 из типа. Soundness в GC (нет borrow-checker,
есть aliasing): `ro` = «**это имя/путь** не пишет», НЕ «объект заморожен».

**Отмена flip-scan (flip-scan-draft):** `*T` **НЕ наследует** binding-`current`. `*T ≡ *ro T`
**во всех позициях** (param/return/generic/alias/cast/field/local). Pointee-mut —
**только** через явный `*mut T`.

### Канон синтаксиса

- `*T` = ro-pointee (канон, дефолт, ≡ `*ro T`). `*mut T` = mut-pointee
  (единственный опт-ин на запись `*p = …`).
- **`*ro T` → HARD ERROR `E_REDUNDANT_POINTER_RO`** («избыточно → используй `*T`»;
  fix-it `*T`). Выбор (a): потребителей мало, std в формировании.
- **`mut *T` / `ro *T` (prefix перед `*`) → `E_POINTER_PREFIX_MODIFIER`**
  (модификатор на `*` запрещён; reassign = L1 binding).
- `ro T`/`mut T` (перед типом value/record) = L2 content-view. `ro x ro T` /
  `*ro ro T` → `E_REDUNDANT_TYPE_MODIFIER`. **`mut x mut T` → `E_REDUNDANT_TYPE_MODIFIER`**
  (тип без модификатора уже mutable по умолчанию; явный `mut T` при `mut`-binding избыточен).
  Аналогично для параметра: `func(mut a mut T)` → `E_REDUNDANT_TYPE_MODIFIER`.
- **Параметр: binding ro по умолчанию (D176) → явный `ro T` на типе избыточен.**
  `func(a ro T)` ≡ `func(ro a ro T)` → **`E_REDUNDANT_TYPE_MODIFIER`** (fix-it: убери `ro` с типа,
  используй `func(a T)`). Исключение: `func(mut a ro T)` — валидно
  (явный `mut` на binding снимает ro-default; `ro T` на типе = L2 freeze, не redundant).
- **Параметр `*T`: `func(a *T)` — pointee уже ro по умолчанию (L3 дефолт).** Явный
  `func(a *ro T)` → **`E_REDUNDANT_POINTER_RO`** (то же что в любой другой позиции).
- `**T ≡ *(*T)`, дефолт ro вниз; mut-уровни — явный `*mut` на нужном уровне
  (`*mut *T`, `**mut T`, `*mut *ro …` нельзя — `*ro` redundant).
- **`*T ≡ *ro T` УНИВЕРСАЛЬНО** — во ВСЕХ позициях. НЕТ наследования pointee-mut от
  binding. НЕТ cast-исключения (`x as *T`, не `as *ro T`).

### Дефолты

binding — пишешь явно `ro`/`mut`; **параметр** ro (D176); **return** mut-binding у
caller'а (D184 — свойство binding, не значения); **pointee** ro (`*T`); **поле**
mutable-у-mut-binding (D175).

**Асимметрия L2 vs L3:** тип value/record без модификатора (`T`) — **content mutable по умолчанию**
(L2; заморозка = явный `ro T`). Указатель без модификатора (`*T`) — **pointee ro по умолчанию**
(L3; запись = явный `*mut T`). Разные дефолты намеренны: value-тип — твой, владеешь, пишешь;
указатель — чужая/aliased память, по умолчанию не пишешь.

### Таблица мутабельности (binding × content)

Знаки: ✅ разрешено · ❌ запрещено · `E` = ошибка

**Локальные binding'и:**

| Форма | reassign | `.field`/`[i]` |
|---|---|---|
| `ro a T` | ❌ | ❌ (P7 freeze) |
| `mut a T` | ✅ | ✅ |
| `ro a mut T` | ❌ | ✅ (R2-split) |
| `mut a ro T` | ✅ | ❌ (R2-split) |

**Параметры (`ro` по умолчанию, D176):**

| Форма | `v = x` | `.field`/`[i]` write |
|---|---|---|
| `v T` ≡ `ro v T` | ❌ E_LOCAL_NOT_MUT | ❌ E_READONLY_CONTENT |
| `mut v T` | ✅ | ✅ |
| `mut v ro T` | ✅ | ❌ E_READONLY_CONTENT |

`v = x` внутри fn — reassign локальной копии binding'а (не виден снаружи).

**Указатели в параметре (L1 × L3):**

| Форма | `v = q` | `*v = x` |
|---|---|---|
| `v *T` ≡ `ro v *T` | ❌ | ❌ |
| `mut v *T` | ✅ | ❌ |
| `v *mut T` ≡ `ro v *mut T` | ❌ | ✅ |
| `mut v *mut T` | ✅ | ✅ |

**Локальные указатели (L1 × L3):**

| Форма | `p = q` | `*p = v` |
|---|---|---|
| `ro p *T` | ❌ | ❌ |
| `mut p *T` | ✅ | ❌ |
| `ro p *mut T` | ❌ | ✅ |
| `mut p *mut T` | ✅ | ✅ |

### 10 принципов (P1-P10)

1. **P1 — три оси ортогональны.** L1×L2×L3 не влияют друг на друга. mut-binding НЕ
   даёт mut-pointee; mut-pointee НЕ делает имя reassignable.
2. **P2 — тип самодостаточен (C1).** `*T` означает ro-pointee в ЛЮБОЙ позиции, без
   running-current/контекста.
3. **P3 — `ro` дефолт везде; опт-ин на запись явный** (`mut x` / `mut T` / `*mut T`).
4. **P4 — L2 freeze СТОИТ на каждом `*`.** Транзитивный ro-view не проникает за
   указатель; за `*` действует **только** L3 (из типа pointee).
5. **P5 — L3 из типа, позиционно-независимо.** `*mut T` = writable target где
   угодно; `*T` = ro где угодно.
6. **P6 — split (L1,L2) явны.** `ro r mut Point` (reassign❌/content✅),
   `mut r ro Point` (reassign✅/content❌). Разрешает `[M-138-binding-type-mut-conflict]`.
7. **P7 — голый `ro r` = freeze** (binding dominates, D175 §V2): и reassign, и весь
   owned-граф (до стены на `*`).
8. **P8 — coercion по оси content (L2) — ~~независимо от L1~~ RETRACTED
   2026-07-23 ([M-ro-launder-via-mut-binding]).** ro-источник → mut-content-цель =
   `E_READONLY_COERCE`; → ro-цель OK. `*mut T → *T` авто-сужение; `*T → *mut T` ❌.
   **AMEND:** это верно для L2 (тип-модификатор `ro T`), но старая формулировка
   «независимо от L1» была ДЫРОЙ, не намерением — она разрешала L1-ro источнику
   (голый `ro a = …` биндинг ЛИБО параметр по D176-дефолту, P7 freeze) свободно
   затекать в mut-цель при смене биндинга, потому что coercion проверялся
   ТОЛЬКО по типу источника (L2), никогда по биндингу источника (L1). P7
   декларирует заморозку («голый `ro r` = freeze: и reassign, и весь owned-граф»),
   но заморозка не переживала ре-биндинг/передачу аргументом — сама P7 не
   спорна, спорной была P8, читавшая её как «L1 источника не участвует в
   coercion». **Исправленная норма:** coercion учитывает L1 ОБЕИХ сторон —
   источника И цели, не только цели. Полная таблица — §«Таблица конверсий
   между binding'ами» ниже; норма СТРОГАЯ (владелец, 2026-07-23, ФИНАЛ —
   подтверждена доками+пробой): применяется ко ВСЕМ типам без исключения по
   классу хранения (см. эту секцию — проба G доказала «value-тип ⇒ безопасно»
   ЛОЖНО в общем случае: value-record С кучевым полем остаётся ❌) —
   **КРОМЕ одного явно ограниченного случая, РАСШИРЕННОГО 2026-07-24 §72**
   ([M-ro-launder-fullstack-value-exemption]): исходно (Plan 224) исключение
   было ограничено голыми скалярными примитивами
   (`int`/`i8`…`i64`/`u8`…`u64`/`f32`/`f64`/`bool`/`char`/`byte` — типы БЕЗ
   единого поля вообще), потому что классовый предикат «value-тип» пришлось
   бы делать рекурсивным по полям, чтобы оставаться безопасным (проба G —
   value-record С кучевым полем — доказала, что нерекурсивная классификация
   «value ⇒ безопасно» ложна), а скаляр эту рекурсивную хрупкость обходит
   ПОЛНОСТЬЮ, имея вообще ноль полей. **§72 строит именно этот рекурсивный
   предикат** (`is_fully_stack_value`, types/mod.rs) и на нём РАСШИРЯЕТ
   исключение с «скаляр-примитив» на «ПОЛНОСТЬЮ-СТЕКОВЫЙ value-тип»: тип и
   ВСЕ его поля транзитивно не содержат ни единого кучевого поля (`Vec`/
   `HashMap`/`Set`/heap-record/`Array`) — базовый случай рекурсии — скаляр
   (как раньше), плюс `str` по ОТДЕЛЬНОЙ причине (immutability, D26, не
   стековость — см. ORACLE G ниже). Единственное кучевое поле, на любой
   глубине вложенности, дисквалифицирует ВЕСЬ тип (проба G, без изменений —
   `ServerResponse{HeaderMap,[]u8}` остаётся ❌); проба E (`Point{x int,y int}`,
   value-record БЕЗ кучевых полей вообще) под §72 RE-RECLASSIFIED neg→pos —
   квалифицируется как fully-stack, копия целиком независима. Санкционированная
   дверь для независимой копии типа, НЕ прошедшего `is_fully_stack_value`, —
   явный `.clone()` (D230).
9. **P9 — deep-immutable НЕ навязывается снаружи сквозь `*mut`** (trade-off): `-> ro VR`
   морозит свои слоты, но `unsafe{*v.p=w}` проходит (L2 не лезет за `*`). Deep-ro →
   **производитель** объявляет поле `*T` (как `str { ptr *u8 }`).
10. **P10 — owned-vs-aliased heap статически неразличим** → граница рисуется
    **синтаксически на `*`** (L2 стоп на `*`), не по aliasing-статусу. `ro` =
    per-path write-ban, не object-freeze (GC, нет эксклюзивности).

### R1 vs R2 (обе живут — на разных осях)

- **R1 (transitive-ro)** = закон L2: `-> ro Value`/`-> ro HeapValue` морозят
  owned-граф (стена на `*`).
- **R2-split** = явный opt-in пары (L1,L2): `ro r mut Point` / `mut r ro Point`
  (см. P6). Голый `ro r` = freeze (P7).

### Нормативный ORACLE (тест-корпус; чтение всегда ✅, знаки = ЗАПИСЬ)

**A. VALUE-record `Point` (копия):** `mut r`: `r=X`✅ `r.x=5`✅ · `ro r`: ❌/❌ ·
`mut r ro Point`: ✅/❌ · `ro r mut Point`: ❌/✅

**B. HEAP-record `Acc` (handle):** те же знаки (семантика: запись видна
co-handle'ам; `ro` = это имя не пишет).

**C. POINTER (unsafe-ops):** `mut p *T`: `p=q`✅ `*p=v`❌ · `mut p *mut T`: ✅/✅ ·
`ro p *T`: ❌/❌ · `ro p *mut T`: ❌/✅ · `ro p **T`: `p`❌ `*p`❌ `**p`❌ ·
`ro p *mut *T`: ❌/`*p=q`✅/`**p`❌ · `ro p **mut T`: ❌/`*p`❌/`**p=v`✅

**D. RETURN:** `-> Value`: caller mut-default (`a=X`✅,`a.x=5`✅) · `-> ro Value`:
`mut a Value=f()`→`E_READONLY_COERCE`, `mut a ro Value=f()`✅, `ro a mut Value=f()`→
`E_READONLY_COERCE`, `ro a Value=f()`✅ · `-> *mut T`: `*a=v`✅(unsafe) · `-> *T`:
`*a=v`❌ · `-> *ro T`: ❌ `E_REDUNDANT_POINTER_RO`.

**E. Generic/Option/cast:** `Vec[*T]`: `*v[i]=x`❌ (L3 элемента) · `Vec[*mut T]`:
`*v[i]=x`✅¹ · `Option[*mut T]`: `Some(p)→*p=v`✅ · `x as *mut T; ro a=x`: `a=y`❌
`*a=v`✅ (из типа) · `mut a=x`: ✅/✅ · vr`{p *mut T}`→`ro v`: `v.p=q`❌
`unsafe{*v.p=w}`✅ · `str{ptr *u8,len int}`: `s.ptr=q`❌, буфер ro.

> ¹ `Vec[*mut T]: *v[i]=x` — семантически верно (тип элемента `*mut T`), но
> **codegen не реализован**: `Vec.new()` для pointer-element-type вызывает generic-заглушку
> `Nova_Vec_static_new()` → NULL → SEGFAULT. Граница `[M-138-vec-pointer-element-mono]` (Plan 138), P2.
> `Option[*mut T]: Some(p)→*p=v` работает (проверено e7_option_mut_ptr_deref_write).

**G. CROSS-BINDING re-init (AMEND 2026-07-23, [M-ro-launder-via-mut-binding];
letter `G` continues past the headed `#### ORACLE F` below to avoid a
letter-collision):**
`ro a = [1,2,3]; mut b = a; b[0]=99` → `E_READONLY_COERCE` на `mut b = a` (source
`a` — L1-ro bare binding, P7 freeze; target `b` — mut content-view; write через
`b` была бы видна `a`). Применяется независимо от storage-класса `T` — включая
value-record БЕЗ кучевых полей (`Point{x,y}`, проба E — по-прежнему `❌`,
классовое послабление по value-record отклонено) и value-record С кучевым
полем (`ServerResponse { headers HeaderMap, body []u8 }` → `mut resp = r;
resp.header(..)` пишет в `HeaderMap` вызывающего). **ИСКЛЮЧЕНИЕ (владелец,
ФИНАЛ 2026-07-23):** голые скалярные примитивы — `fn f(n int) { mut m = n }`
**✅ ЛЕГАЛЬНО** (копия `int` независима by construction, полей нет вообще —
скаляр не «value-тип» в смысле пробы G, у него нет owned-графа для утечки).
Дверь для НЕ-скалярных типов: `mut b = a.clone()` (D230) для независимой
копии, либо сделай источник `mut` с самого начала, если copy не нужна.

**AMEND 2026-07-25 №106 ([M-ro-launder-pattern-bind-not-enforced], реестр
221.1, Plan 224/226 зона):** §G/§H выше были прописаны и enforced ТОЛЬКО для
ДЕКЛАРИРОВАННЫХ `ro`-биндингов/D176-параметров — ПАТТЕРН-биндинги (match-arm
`Ok(b0) => …`, `if Some(x) = …`, D34 «bare pattern-bind = immutable») в L1
launder-таблицу (`ro_binding_names`) заведены НЕ были, так что то же самое
отмывание проходило МОЛЧА через паттерн: `Ok(b0) => { mut b = b0 }` на
кучевом payload (`b0` — bare pattern-bind, D34-immutable) НЕ давал
`E_READONLY_COERCE`, хотя семантически — тот же класс §G (`b` содержит тот
же handle, что `b0`/оригинальный источник). **Норма:** pattern-bind
получает L1-статус ИДЕНТИЧНО explicit-биндингу — bare (`Ok(b0)`) = ro-
freeze (P7), `mut` внутри паттерна (`Ok(mut b0)`) = mut, участвует в §G/§H
launder-проверке на общих основаниях (включая §72 fully-stack-исключение —
bare-bind скаляра → `mut` остаётся ✅). Канон-запись для решения — mut
ВНУТРИ паттерна (`Ok(mut b)`), не отдельный cross-binding.

**H. CROSS-BINDING call-argument (AMEND 2026-07-23):** тот же класс без
промежуточного re-init — аргумент напрямую: `fn outer(v []int) { fill(v) }`
при `fn fill(mut v []int)` → `E_READONLY_COERCE` на `fill(v)` (`v` — L1-ro
параметр `outer`-а по D176-дефолту, П7 freeze, передаётся в `mut`-параметр
`fill`). Тот же результат для `ro a = [..]; fill(a)` (ro-локал). Применяется
для методов симметрично (`recv.method(arg)` где `arg` L1-ro → `mut`-параметр
метода). **Исключение (checker precision, не норма):** имена, перегруженные
по оси режима {ro,mut,consume} (D326-ревизия, Р13/Р14 — `fn f(x T)` /
`fn f(mut x T)` / `fn f(consume x T)` под одним именем) НЕ проверяются этим
правилом — dispatch резолвит КОНКРЕТНЫЙ overload по режиму аргумента (ro-арг
→ ro-overload), так что L1-ro аргумент, дошедший до overloaded-имени с
mut-веткой где-то среди перегрузок, не обязательно течёт в НЕЁ; текущая
registry-инфраструктура (`fn_mut_params`/`method_mut_params`) хранит mut-idx
по ИМЕНИ (все перегрузки конфлированы), поэтому прецизионная проверка
per-overload — Ф.1-followup, не в этом амендменте.

### Таблица конверсий между binding'ами (L1,L2)источник → (L1,L2)цель

**НОВОЕ 2026-07-23** ([M-ro-launder-via-mut-binding]) — до этого амендмента
D246 описывал ТОЛЬКО права доступа ВНУТРИ одного биндинга (таблицы
binding×content/параметры/указатели выше); перенос значения МЕЖДУ
биндингами (инициализация нового binding'а · аргумент вызова · возврат) был
свёрнут в одну строку P8, покрывавшую только L2 (тип-модификатор) — L1
(биндинг источника) не проверялся ВООБЩЕ. Ниже — полная таблица; применяется
одинаково к ВСЕМ ТРЁМ позициям (инициализация · аргумент · возврат) и ко ВСЕМ
типам, **КРОМЕ голых скалярных примитивов** (владелец 2026-07-23, ФИНАЛ:
классовое послабление по value-record СНЯТО — проба G этой секции доказала
«value-тип ≠ безопасно» даже без кучевых полей, проба E остаётся neg — но
УЗКОЕ послабление для скаляров-примитивов ВОЗВРАЩЕНО: скаляр не имеет полей
вообще, рекурсивная хрупкость пробы G к нему неприменима, копия всегда
независима by construction).

| Источник (L1 биндинга) | → цель с mut content-view (L2) | → цель с ro content-view (L2) |
|---|---|---|
| `mut` (reassignable, любой L2 у источника) | ✅ | ✅ (сужение прав, D176) |
| **голый `ro`**, тип — **скаляр-примитив** (`int`/`i8`…`i64`/`u8`…`u64`/`f32`/`f64`/`bool`/`char`/`byte`, БЕЗ полей вообще) | ✅ **ИСКЛЮЧЕНИЕ** (копия независима by construction — нет owned-графа) | ✅ |
| **голый `ro`**, тип — value-запись/tuple/`[N]T`/heap (P7 freeze — явный `ro a = …` ЛИБО параметр по D176-дефолту, БЕЗ явного `mut T` split) | ❌ `E_READONLY_COERCE` | ✅ |
| split `ro a mut T` (L1 ro, L2 mut явно), любой тип | ❌ `E_READONLY_COERCE`¹ | ✅ |

> ¹ Split-источник (`ro a mut T`) уже разрешает запись НАПРЯМУЮ через `a`
> (`a.x=v` ✅, P6/R2-split) — поэтому перенос в mut-цель НЕ добавляет новых
> прав записи по сравнению с прямой записью через `a`, но норма всё равно
> отклоняет перенос МЕЖДУ биндингами единообразно (владелец: «параметр не
> может быть присвоен в mut-переменную», без оговорок на split-форму);
> прямая запись через исходный split-биндинг остаётся легальной и
> достаточной для тех, кому нужна mutation без re-binding. Скалярный
> exemption НЕ распространяется на split — split уже подразумевает
> составной тип с явным L2-mut content-view, скаляр в этой форме не
> встречается осмысленно (`ro a mut int` эквивалентно `ro a int` для
> примитива без owned-графа, но норма не вводит для этого отдельного
> правила — используй просто `mut a int`, если нужна мутация).

**Диагностика (порядок подсказок, владелец 2026-07-23):** сообщение
`E_READONLY_COERCE` в этом амендменте предлагает РЕШЕНИЯ в порядке (a) →
(b) → (c): **(a) первым — `mut`-параметр/локал** («если источник — параметр
текущей функции, сделай его `mut x T` — это ВСЕГДА in-out по D326-ревизии
§Р3, вызывающий увидит изменения после вызова; если локал — объяви `mut` с
самого начала»), **(b) вторым — явный `.clone()`** (D230, независимая
копия — использовать, когда (a) не подходит семантически, с одностроч-
комментом-обоснованием почему), (c) третьим — оставить цель `ro`. Порядок
намеренный: in-out через `mut`-параметр — идиоматичный канон Nova (D326),
`.clone()` — осознанный опт-ин с видимой стоимостью копии, не дефолт.

L2-ro источник (`ro T` тип-модификатор) в mut-цель уже был `E_READONLY_COERCE`
до этого амендмента (проба J/P8 старая) — без изменений, таблица выше просто
делает явным, что L1-ось источника учитывается СИММЕТРИЧНО L2-оси источника
(раньше проверялась только L2).

### АМЕНДМЕНТ 2026-08-21 — `ro` ЗАРАЖАЕТ ТОЛЬКО ПО АЛИАСУ

**Решение владельца 2026-08-21, реестр 221.1 №717.** Амендирует таблицу
конверсий (L1,L2) выше в части ЛОКАЛЕЙ.

**НОРМА.** Локаль, чьё значение есть ПСЕВДОНИМ ro-источника, сама
ro и подчиняется P7/P8 наравне с источником. Локаль, связанная со
СВЕЖИМ значением, — ОБЫЧНАЯ локаль, сколько бы немутабельной ни
была её привязка.

АЛИАС определяется по ФОРМЕ инициализатора — это МЕСТО (place),
укоренённое в ro-имени:

| инициализатор | `ro al = …` — алиас? |
|---|---|
| `v` (ro-параметр либо ro-алиас) | ДА |
| `v.field`, `v[i]` — проекция ro-источника | ДА |
| `f(…)` / `v.m(…)` — результат вызова | НЕТ (свежее) |
| литерал, конструктор, арифметика, `.clone()` | НЕТ (свежее) |

**Почему различитель обязателен, в числах.** Без него проверка
вырождается в «нельзя возвращать ни одну немутабельную локаль»: замер
2026-08-20 дал `nova check std/src` = `PASS: 3  FAIL: 177` и **773**
срабатывания `E_READONLY_COERCE` при каноне 26. Причина — `ro x = …`
обычная форма записи в Nova; `std/src/collections/vec/mutate.nv:133`
возвращает `removed` — СВЕЖЕЕ снятое значение, ничей не псевдоним,
и отвергалось бы только за немутабельность привязки.

**ПОЗИЦИЯ ХВОСТА ВЕТВИ.** Хвост ветви `if`/`match`, стоящей в позиции
значения тела функции, — ТОЖЕ возврат, и алиас-правило действует там
наравне с головным хвостом. Тела циклов в эту позицию НЕ входят.

**ГРАНИЦА С ВАРИАНТОМ (в), и она намеренная.** Решение владельца
2026-08-18 — выводить возврат как `ro`, когда внутрь попал ro-источник, —
НЕ ОТМЕНЯЕТСЯ и действует вместе с этим: (в) отвечает за ВОЗВРАТ (в
том числе через обёртку: кортеж, запись, `Some`/`Ok`, конструктор),
алиас-правило — за ЛОКАЛИ. Следствие для позиции хвоста ветви:
не-`mut` ПАРАМЕТР, возвращённый оттуда, сегодня НЕ судится — это
территория (в), а не алиас-правила. Цена обратного решения измерена
2026-08-22: 27 `E_READONLY_COERCE` в std и 3 в `nova-http`, и все до единого —
параметр в хвосте ветви (`@min`/`@max`/`@clamp`/`@or`/`@fold`/`@plus`), то
есть ровно то, что (в) велит ВЫВОДИТЬ как `ro`, а не отвергать.

**Что это закрывает в реализации** (реестр №717, дыры (2) и (3)):
происхождение биндинга запоминается ОДИН РАЗ, в момент заведения, и
поэтому восстановление ro-множества на выходе из блока (правильное
само по себе) больше не стирает его до проверки возврата. Это тот
же приём per-binding provenance, что и в амендменте к [D432](#d432) §2 выше,
и это не совпадение: оба дефекта о том, что свойство раздавалось по
ФОРМЕ ЗАПИСИ, а не по ПРОИСХОЖДЕНИЮ.

**Норму держат фикстуры:** `neg/m717_ro_alias_local_return_neg.nv` (алиас
через локаль = ошибка), `neg/m717_ro_alias_branch_tail_neg.nv` (то же через
хвост ветви), `m717_ro_fresh_local_return_ok.nv` (свежее значение законно),
`m717_ro_return_type_ok.nv` (явный `-> ro T` — граница варианта (в)).

### Error codes

- **`E_REDUNDANT_POINTER_RO`** (NEW, Plan 147 Ф.2) — postfix `*ro T` (избыточно:
  `*T` уже ro). Fix-it: «используй `*T`». Применяется во ВСЕХ позициях (тип
  самодостаточен).
- **`E_POINTER_RO_ASSIGN`** (NEW, Plan 147 Ф.3) — запись `*p = …` через `*T`/`*ro`
  pointee (ro-pointee read-only). Pointer-ops — в `unsafe {}`.
- **`E_POINTER_PREFIX_MODIFIER`** (существует, D216 §1) — модификатор перед `*`.
- **`E_REDUNDANT_TYPE_MODIFIER`** (существует) — `ro x ro T` / `*ro ro T` / `func(a ro T)` /
  `mut x mut T` / `func(mut a mut T)`. (Ф.7: oracle-test f7_neg3 подтверждает
  parser-level enforcement для `ro a ro T`; `func(a ro T)` — parser аналогично.)
- **`E_READONLY_CONTENT`** (существует, D176) — запись через ro-view L2: `view[i] = x`
  на `ro T`-типе ИЛИ (NEW Ф.7) через `ro`-binding (L1 dominates P7): `ro a = [...]`
  → `a[i] = x` → `E_READONLY_CONTENT`; `func(v []int)` → `v[i] = x` →
  `E_READONLY_CONTENT` (param ro-by-default D176, P7 freeze).
- **`E_READONLY_COERCE`** (существует) — ro-content-источник → mut-content-цель (P8);
  **AMENDED 2026-07-23** ([M-ro-launder-via-mut-binding]) — теперь ТАКЖЕ учитывает
  L1-ось источника (не только L2): голый `ro`-биндинг/параметр-по-D176-дефолту →
  mut-цель = ошибка во ВСЕХ трёх позициях (инициализация · аргумент · возврат),
  для ВСЕХ типов без исключения (ORACLE G/H, таблица конверсий выше). Дверь для
  независимой копии — явный `.clone()` (D230).

#### ORACLE F — Index-write through ro binding/param (Ф.7, 2026-06-17)

```nova
// F1: ro local binding → index write forbidden (P7 freeze)
ro a = [1, 2, 3]
a[0] = 99                  // ❌ E_READONLY_CONTENT (ro binding dominates, P7)
_ = a[0]                   // ✅ reads always ok

// F2: mut binding → index write allowed
mut a = [1, 2, 3]
a[0] = 99                  // ✅

// F3: param without mut → index write forbidden (ro by default D176, P7 freeze)
fn fill(v []int, val int) {
    v[0] = val             // ❌ E_READONLY_CONTENT
}

// F4: mut param → index write allowed
fn fill(mut v []int, val int) {
    v[0] = val             // ✅
}

// F5: ro local via fn return → index write forbidden
fn make_slice() -> []int { [1, 2, 3] }
ro a = make_slice()
a[1] = 99                  // ❌ E_READONLY_CONTENT (ro binding P7)
```

### Осознанные trade-off'ы (намеренно)

1. Deep-immutable сквозь `*mut` нельзя навязать снаружи (P9, C++ shallow-const):
   deep-ro → производитель объявляет поле `*T`.
2. Shared-mut heap-record под чужим `ro` возможен (GC, нет эксклюзивности): `ro` =
   per-path write-ban, не object-freeze (P10).
3. owned-vs-aliased heap статически неразличим → граница на `*` синтаксическая (P4).

### Cross-amend impact

- **D216 §V2.6** — «`*T ≡ *ro T`» (always-ro pointee) **RESTORED** (flip-scan-draft retract).
- **D33** (binding propagation) — L1 ось; не propagates в L3 (стоп на `*`, P4).
- **D36** (binding ro default) — L1 binding = reassignability, **только**;
  НЕ задаёт pointee-capability (L3 из типа).
- **D175 §V2** (binding dominates / access-time) = **L2 view-семантика — KEEP**;
  добавлено «freeze STOPS at every `*`» (P4) + пример vr-с-`*mut`-полем.
- **D176** (`ro T` тип-модификатор) — L2 content-view на параметре (ro дефолт).
- **D184** (return mut default) — свойство **binding** у caller'а (L1), не значения.
- **D26 / Plan 139** — str lang-item `type str value priv { ptr *u8, len int }`:
  `ptr *u8` (ro-pointee, ≡ `*ro u8`); `*ro u8` избыточен → `E_REDUNDANT_POINTER_RO`.
  Снимает гейт `[M-139-f0-lang-item-decl]`.
- **P7/P8 (AMEND 2026-07-23)** — P8's old «независимо от L1» retracted; coercion
  now checks the SOURCE's L1 binding too, closing the gap where P7's freeze
  ("голый `ro r` = freeze") did not survive a re-binding/argument-pass. See
  §«Таблица конверсий между binding'ами» + ORACLE G/H above.
- **D230** (`Clone` protocol) — the sanctioned door for an independent mutable
  copy of an L1-ro source (`.clone()`), now load-bearing for this amendment's
  migration (types without `Clone` need it added, or the site restructured).

### Связь

- `[M-138-binding-type-mut-conflict]` — **разрешён P6** (split на оси L1×L2).
- `[M-ptr-cast-reinterpret-unsafe]` — учитывается в L2-coercion (P8, авто-сужение
  `*mut → *T`).
- `[M-138-double-pointer-codegen-test]` — multi-level pointer (oracle C: `ro p *mut *T`).
- Гейтит Plan 139 `[M-139-f0-lang-item-decl]`.
- `[M-ro-launder-via-mut-binding]` (norm+checker landed 2026-07-23; migration
  IN PROGRESS, NOT closed — see [Plan 224](../../docs/plans/224-ro-launder-l1-coercion.md)) —
  L1-ro launder via re-binding/argument-pass.

### Acceptance

См. [Plan 147](../../docs/plans/147-pointer-mut-flip-scan-model.md) A1-A6: A1 —
oracle A-E (~20 форм): pos компилируются, neg дают `E_REDUNDANT_POINTER_RO` /
`E_POINTER_PREFIX_MODIFIER` / `E_READONLY_COERCE` / `E_POINTER_RO_ASSIGN`; A2 —
`*T ≡ *ro T` ВЕЗДЕ (позиционные фикстуры); A3 — L2 freeze транзитивен + СТЕНА на
`*` (vr с `*mut`-полем); A4 — split + return-coercion 4 случая; A5 — flip-scan-draft
retracted, pointer-таблица + str переписаны; A6 — 0 регрессий pointer-dirs.

---

## D220. Per-field visibility — `priv` keyword + type-level default flip

> **Status:** V1 ACTIVE (spec + parser/AST infrastructure landed, 2026-06-02). **AMENDED by [D281](#d281-module-level-field-privacy--type-x-priv---plan-160)** (2026-06-15): type-level `priv` теперь = module-private (не type-private); type-private type-level default = `priv(type)`. Field-level explicit `priv` остаётся type-private (без изменений). Реализация — [Plan 124](../../docs/plans/124-priv-field-visibility.md). Empirical validation — [docs/dev/research/06-field-visibility-go-kubernetes.md](../../docs/dev/research/06-field-visibility-go-kubernetes.md). Amends [D47](07-modules.md#d47) (replaces deprecated `_prefix` convention с compile-time enforcement).

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

priv field access РАЗРЕШЁН только из методов own type'а:
- Instance methods: `fn TypeX @method() { @priv_field }`
- Static methods: `fn TypeX.factory(...) { ... }`
- **Cross-instance:** `fn TypeX @eq(other TypeX) -> bool => @f == other.f` — доступ к `priv` полям **другого экземпляра того же типа** разрешён внутри метода этого типа. `other` — параметр типа `TypeX`, метод принадлежит `TypeX` → privacy scope совпадает.

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

#### §5 Семантика: организация, не security

`priv`/`priv(type)` — **организационный инструмент**, не security-барьер. Цель: защита от *случайного* обращения к деталям реализации, не от *намеренного*.

Nova не ограничивает добавление методов на тип из любого модуля. Следствие: пользователь *намеренно* может написать:

```nova
fn Test @id() -> int => @id   // в любом модуле — легально
```

Это не считается «вскрытием» — это осознанный выбор пользователя. Nova — «публичное по умолчанию» (D47, validated в docs/dev/research/06-field-visibility-go-kubernetes.md), и `priv(type)` означает «используй методы типа», а не «запрещено».

Настоящая граница инкапсуляции — **модуль** (`priv` = module-private, D281): другой модуль не может случайно прочитать поле — только намеренно добавив метод.

Аналог: Go `unexported` защищает от случайного обращения из другого пакета, но не является security-boundary.

Nova не имеет reflection API → `priv` enforcement compile-time, без reflection-bypass (в отличие от Java/Kotlin/C#).

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
- ✅ [M-124.2-pattern-sites-extension] CLOSED 2026-06-02 — Match/IfLet/WhileLet/For/ParallelFor + nested + spread (D221).
- [M-124.2-priv-embed] — priv use NAME Type.
- [M-124.4-tuple-priv] — named tuple priv (D215 ext).
- [M-124.4-protocol-impl-boundary].
- [M-124.5-doc-lsp].
- [M-124.6-test-access].

---

## D221. Pattern destructure + literal init edge cases (Plan 124.2)

> **Status:** ✅ ACTIVE since 2026-06-02 (Plan 124.2 closure).
> **Extends:** D220 §3-§4. Self-contained sub-decision: covers
> pattern-site и literal-spread edges not addressed в D220 §4.
> **Plan:** [Plan 124.2](../../docs/plans/124.2-pattern-literal-edges.md).
> **Cross-refs:** D220 (core priv semantics), D52 §1-§2 (record
> declaration / `@field` shorthand), D17 (pattern syntax).

### §1 Scope

D220 §4 описывает priv-access rules для базовых форм (Member access
+ Stmt::Let pattern + RecordLit named fields). D221 расширяет
coverage на:

1. Дополнительные pattern sites: match, if-let, while-let, for-in,
   parallel-for.
2. Nested Pattern::Record — recursive descent через sub-field types.
3. Record literal spread `Type { ...other }`.
4. Rest pattern `{ field, .. }` — non-binding semantics.

### §2 Pattern sites — complete enumeration

Following pattern-bearing forms ALL apply priv-pattern enforcement
(Plan 124.2 implementation hook each site):

| Site | AST node | Scrutinee source |
|---|---|---|
| `let PAT = EXPR` | `Stmt::Let { pattern, value }` | type of `value` |
| `if PAT = EXPR { ... }` | `ExprKind::IfLet { pattern, scrutinee }` | type of `scrutinee` |
| `while PAT = EXPR { ... }` | `ExprKind::WhileLet { pattern, scrutinee }` | type of `scrutinee` |
| `match EXPR { PAT => ... }` | `ExprKind::Match { scrutinee, arms[].pattern }` | type of `scrutinee` |
| `for PAT in EXPR { ... }` | `ExprKind::For { pattern, iter, elem_type }` | `elem_type` ∥ inferred element type |
| `parallel for PAT in EXPR { ... }` | `ExprKind::ParallelFor { pattern, iter, elem_type }` | same |

В каждой точке: for each Pattern::Record outside type-method scope,
each explicitly-named RecordPatternField corresponding к priv-field
→ `E_PRIV_FIELD_PATTERN`.

### §3 Rest pattern `..` — non-binding

```nova
ro Account { name, .. } = acc    // outside-of-Account ok if `name` public.
                                  // `..` does NOT bind priv `money`.
```

Pattern::Record.rest = true маркирует syntactic `..`. Семантика:
**игнорировать остальные поля, no bindings produced**. Priv-fields
не leak'аются через `..` потому что нет binding'а.

NB: explicit field names ARE checked даже если `..` присутствует —
`{ money, .. }` outside Account → E_PRIV_FIELD_PATTERN на `money`.

### §4 Nested Pattern::Record

```nova
type Address { priv mut zip str, ro city str }
type User { ro name str, ro addr Address }

// Outside any method scope:
ro User { addr: Address { zip }, name } = u   // ❌ E_PRIV_FIELD_PATTERN
                                               //   on `zip` (Address-internal)
```

Recursive descent: для каждой `RecordPatternField { name, pattern: Some(sub), .. }`
sub-pattern проверяется against sub-field's declared type (via outer
type's RecordField.ty). Outer field accessibility (User.addr public)
не освобождает inner check (Address.zip priv).

### §5 Record literal spread — `E_PRIV_FIELD_INIT_SPREAD`

```nova
type Account { priv mut money f64, ro name str }

// Outside Account-method scope:
Account { ...orig, name: "new" }   // ❌ E_PRIV_FIELD_INIT_SPREAD
```

Spread `...src` implicitly копирует все fields (включая priv).
Outside type-method scope, это нарушает encapsulation. Эмитим
**E_PRIV_FIELD_INIT_SPREAD** на spread field span с hint'ом
использовать factory method.

Inside type-method scope — allowed (recv = T → каноническая ситуация).

Note: type без priv fields → spread OK везде (нет encapsulation
boundary).

### §6 Diagnostic codes

| Code | Where | Plan |
|---|---|---|
| `E_PRIV_FIELD_PATTERN` | Pattern sites §2 + nested §4 | D220 §4 (reused) |
| `E_PRIV_FIELD_INIT_SPREAD` | RecordLit spread §5 | **D221 NEW** |

Format (Plan 50 D102):
```
[E_PRIV_FIELD_INIT_SPREAD] cannot use spread `...` in record literal
of `T` outside type-method scope: type has private fields which would
be implicitly initialized via copy (Plan 124 / D221 §5). Hint: use
factory method `T.new(...)` or list each public field explicitly.
```

### §7 Cross-refs

- D17 — pattern syntax.
- D52 §2 — record literal + field shorthand.
- D220 — core priv semantics (default vis, scope, access rules).
- D215 — named tuples; D221 covers ONLY record form, tuple-pattern
  priv в D222 (Plan 124.4).

### §G1 Generic types — uniform enforcement (Plan 124.3 amend)

> **Added 2026-06-02.** Plan 124.3 closure.

Per-field `priv` modifier applies uniformly к generic record types:

```nova
export type Stack[T] {
    priv mut len int
    ro capacity int
}
```

**Enforcement architecture:** check site reads `RecordField.priv_field`
из AST (pre-monomorphization). Mono'd instances (`Stack[int]`,
`Stack[str]`) inherit identical enforcement — T-substitution не
изменяет field metadata.

**Receiver-type tracking** (TypeCheckCtx.current_recv_type) uses
**type-name only**:
- `fn Stack[T] @push(...)` body sees recv = `Some("Stack")`.
- Generic parameters не factor в name comparison.
- Inside generic methods, accessing priv fields на ЛЮБОЙ T
  instantiation OWN type'а — allowed.
- Cross-type access (`Stack[int].@field` из `Queue[T].method`) —
  blocked (recv = "Queue", не "Stack").

**Bootstrap parser limitation:** explicit generic prefix в record
literal expression position (`Stack[int] { len: 5, capacity: 10 }`)
не парсится в bootstrap (parser-ambiguity с array-literal opening
`[`). Canonical form — **anonymous literal** `{ len: ..., capacity: ... }`
с return-type inference:

```nova
export fn Stack[T].with_len(initial int, cap int) -> Stack[T] =>
    { len: initial, capacity: cap }      // ✅ anonymous, inferred
```

Pattern destructure аналогично: `Stack { fields } = expr` (без
generic args) — resolved through scrutinee type.

INIT path (E_PRIV_FIELD_INIT) testing для generic types использует
non-generic specialized variant ИЛИ relies на anonymous literal
form's type inference (target type known via expected return type
of enclosing method).

### Acceptance — Plan 124.3

ALL closed 2026-06-02:

- A3.1 ✅ Generic type `Stack[T] { priv ... }` parser PASS.
- A3.2 ✅ Mono'd instance external access: write → E_PRIV_FIELD_WRITE,
  read → E_PRIV_FIELD_READ.
- A3.3 ✅ Inside `Stack[T].method` — @field access OK.
- A3.4 ✅ Generic method calling another OK.
- A3.5 ✅ Multiple instantiations (Stack[int] + Stack[str]) — same
  enforcement.
- A3.6 ✅ `Option[Account]` — outer Option public, inner Account rules
  unchanged.
- A3.7 ✅ plan124_3 10/10 fixtures PASS.
- A3.8 ✅ Regression plan124_1 9/9 + plan124_2 14/14 unchanged.

---

## D222. Named tuple priv + protocol impl boundary (Plan 124.4)

> **Status:** ✅ ACTIVE since 2026-06-02 (Plan 124.4 closure).
> **Extends:** D220 (per-field priv) + D215 (named tuples, Plan 120) +
> D221 (pattern check). Self-contained sub-decision: covers named tuple
> form + protocol impl boundary explicitly.
> **Plan:** [Plan 124.4](../../docs/plans/124.4-named-tuple-protocol.md).
> **Cross-refs:** D215 (named tuple form), D220 (core priv), D221
> (pattern), D52 §2 (record field syntax).

### §1 Named tuple priv syntax

```nova
type Vec3(priv x f64, priv y f64, priv z f64)
type Account(priv balance f64, name str)    // mixed
type Secret(pub key str, priv salt []u8)    // explicit pub override
```

Same modifier semantic as RecordField: `priv` before field name (or
`pub` for explicit public override; reserved для D220 type-level flip
extension Plan 124.7). Mutual exclusion enforced —
`priv pub x f64` / `pub priv x f64` → `E_PRIV_PUB_CONFLICT`.

### §2 Access rules — uniform with D220

**Read** (`v.x`):
- Inside named tuple's own methods (instance `@field` or static
  `T.method(...)`) — OK regardless of priv.
- Outside → `E_PRIV_FIELD_READ` если field marked priv.

**Init** via constructor (`Vec3(1.0, 2.0, 3.0)` — позиционно; именованная
форма допустима ТОЛЬКО для полей С дефолтом, D102):
- Inside type-method scope — OK (recv = T).
- Outside → `E_PRIV_FIELD_INIT` для каждого priv-named arg.

**Pattern destructure** (`Vec3 { x, y, z } = v` record-style):
- Same as record (D221 §2-§4): outside → `E_PRIV_FIELD_PATTERN`
  per priv field; `..` rest is non-binding; nested descent recursive.

**Write** — N/A: named tuple fields are immutable by D215 design

### Амендмент 2026-08-05 — конструирование и деструктуризация именованного кортежа

Правило не было записано ни в одном D-блоке (D221 §7 прямо признаёт, что
покрывает только форму записи, а D222 описывает лишь доступ к приватным
полям внутри уже существующей формы). Пробел вскрылся при закрытии
[№145](../../docs/plans/221.1-bug-sweep.md): энфорс появился в компиляторе
раньше, чем формулировка в спеке. Восполняется здесь.

**Конструирование — позиционное.** `Vec3(1.0, 2.0, 3.0)`. Именованная форма
допустима ТОЛЬКО для полей, у которых объявлен дефолт — по общему правилу
[D102](03-syntax.md): обязательный параметр передаётся позиционно,
опциональный — по имени. Прежние примеры в D215 и D222 показывали
именованные аргументы для полей без дефолтов и тем противоречили D102;
исправлены этим же амендментом.

**Деструктуризация — только фигурной формой, по имени.**

| Форма | На именованном кортеже |
|---|---|
| `{ x, y }` — фигурная, по имени | **разрешена** |
| `(a, b)` — круглая, по позиции | **ошибка компиляции** |

Круглая скобка означает разбор по позиции и остаётся формой позиционного
кортежа; именованный кортеж разбирается по именам полей, как запись. Смысл
различения тот же, что у конструирования: у именованного кортежа имена полей
— часть контракта, и разбор по порядку молча сломается при перестановке
полей, которая для именованной формы законна.

Частичный разбор — как у записей ([D411](#d411)): если перечислены не все
поля, обязателен явный `..`.

**Реализация:** проверка конструирования — в `f5_check_tuple_construct`,
запрет позиционного разбора — отдельной проверкой чекера
(`check_positional_destructure_on_named_tuple`), обе в чекер-канале
(волна p248tup-145, 2026-08-05). Проверка биндинг-агностична — применяется
к `let`/`ro`/`mut`/`consume` одинаково; `consume`-биндинг получил доступ к
деструктуризации вообще (обе формы) только окном p378 2026-08-06 (D180
амендмент «№378»), но правило круглая/фигурная было записано здесь раньше
и не менялось.
(no `mut` modifier на field). Все assignments к `v.x = ...` fail
`E_READONLY_FIELD` before priv check.

**Positional access `.0`/`.1`** — already blocked by Plan 120
`E_TUPLE_POSITIONAL_ACCESS_ON_NAMED` (D215 Q120 Option B); priv
не нужно добавлять отдельный код.

### §3 Protocol implementation boundary

Protocol satisfaction в Nova реализуется ДВУМЯ способами (D186 / Plan 91.9):

1. **Type-method impl** — `fn Vec3 @to_string() -> str`. Receiver = T;
   `current_recv_type = Some("Vec3")` → priv access OK канонически.
   ✅ Allowed.

2. **External free-fn** — `fn compute_sum(v Vec3) -> f64 => v.x + v.y + v.z`.
   No receiver tracking; `current_recv_type = None` →
   `E_PRIV_FIELD_READ` fires при touching priv. ✅ Blocked.

→ Encapsulation guarantee: protocol impls cannot **bypass** priv
boundary unless declared as type-method. Mirrors Rust trait impl
rules; stricter than Go/Kotlin (pkg-wide-allowed).

### §4 Diagnostic codes

| Code | Site | Plan |
|---|---|---|
| `E_PRIV_FIELD_READ` | Member access на priv named-tuple field | D220 §4 (reused) |
| `E_PRIV_FIELD_INIT` | `T(field: ...)` named-arg ctor priv field | D220 §4 (reused) |
| `E_PRIV_FIELD_PATTERN` | `T { field, ... } = v` priv field | D221 §2 (reused) |
| `E_PRIV_PUB_CONFLICT` | `priv pub` / `pub priv` mutual exclusion | D220 §6 (reused) |
| `E_TUPLE_POSITIONAL_ACCESS_ON_NAMED` | `.0` access | D215 / Plan 120 (preexisting) |

No new codes — D222 reuses D220/D221 codes uniformly. Spec mentions
named-tuple context в hint text.

### §5 Implementation hooks

| Layer | Site | Change |
|---|---|---|
| AST | `NamedTupleField` struct | Added `priv_field: bool` |
| Lexer | `KwPriv`, `KwPub` | Already declared (Plan 124.1) |
| Parser | `is_named_tuple_decl` | Recognize `priv`/`pub` as named-marker |
| Parser | `parse_named_tuple_fields` | Accept `priv`/`pub` modifier с conflict-check |
| Checker | `f3_check_member` NamedTuple arm | Added priv check (mirror Record) |
| Checker | `f5_check_tuple_construct` | INIT priv check on named-args |
| Checker | `check_priv_pattern_recursive` | Unified для Record + NamedTuple |

### §6 Cross-refs

- D215 — named tuple syntax + access rules.
- D220 — core priv semantics, error codes definition.
- D221 — pattern destructure + spread.
- D52 §2 — record field syntax (mirror form).
- D186 — protocol satisfaction (Plan 91.9): type-method primary,
  external-fn-impl secondary; D222 §3 formalizes boundary impact.

### Acceptance — Plan 124.4

ALL closed 2026-06-02:

- A4.1 ✅ Named tuple `type Vec3(priv x f64, ...)` parser PASS.
- A4.2 ✅ Positional `.0` access — handled by preexisting Plan 120
  E_TUPLE_POSITIONAL_ACCESS_ON_NAMED (priv-orthogonal).
- A4.3 ✅ Named `.x` access на priv field outside → E_PRIV_FIELD_READ.
- A4.4 ✅ Inside type-method scope — read + init + pattern + protocol
  method все allow.
- A4.5 ✅ Protocol impl (type-method-based, `fn Vec3 @method()`) —
  priv access OK.
- A4.6 ✅ Protocol impl external-fn-based (`fn compute(v Vec3)`) —
  priv access BLOCKED (E_PRIV_FIELD_READ).
- A4.7 ✅ plan124_4 10/10 fixtures PASS.
- A4.8 ✅ Regression Plan 120 (8/8) + plan124_1 (9/9) + plan124_2
  (14/14) unchanged.
- A4.9 ✅ D222 NEW + D215 cross-ref + D220/D221 code reuse.
- A4.10 ✅ plan120 backward compat — все existing named-tuple
  fixtures без `priv` modifier работают unchanged.

### §T1 nova doc + LSP integration (Plan 124.5 amend)

> **Added 2026-06-02.** Plan 124.5 closure. Cross-references D107
> (nova doc schema) + Plan 104.x (LSP infrastructure).

**nova doc behavior:**
- Default: priv fields **hidden** from rendered documentation
  (markdown / HTML / JSON).
- `--include-private` flag shows all fields с `priv` keyword
  preserved in signature rendering (`type X { priv mut f T }`).
- JSON output emits `"priv_field": true|false` per field
  regardless of `--include-private` — consumed by tooling.

**LSP integration (forward-ref):**
- AST `RecordField.priv_field` + `NamedTupleField.priv_field` flags
  available для LSP hover (Plan 104.2) и completion (Plan 104.3)
  integration once these ship.
- Expected behavior: priv-field filter в autocomplete outside
  type-method scope; 🔒 priv badge в hover popups; priv-field
  code-lens decoration.

**User-facing documentation:**
- `docs/guide/field-visibility-guide.md` — comprehensive guide:
  use cases, syntax, composition, diagnostics, tooling, comparison
  vs Go/Rust/TS/Java/Swift/C#, migration, common patterns.

### Acceptance — Plan 124.5

ALL closed 2026-06-02:

- A5.1 ✅ `nova doc` hides priv fields by default.
- A5.2 ✅ `nova doc --include-private` shows priv с keyword preserved.
- A5.3 🟡 LSP autocomplete filter — forward-ref Plan 104.3 (infra
  data source ready).
- A5.4 🟡 LSP hover badge — forward-ref Plan 104.2 (infra ready).
- A5.5 🟡 Quick-fix suggestion — forward-ref Plan 104.x (Plan 50 D102
  format hints уже в error messages).
- A5.6 ✅ plan124_5 fixtures 3/3 PASS (parser + smoke; doc behavior
  e2e verified manually).
- A5.7 ✅ `docs/guide/field-visibility-guide.md` created (~330 lines).
- A5.8 ✅ Regression: existing nova doc fixtures unchanged.

---

## D224. Escape hatches — `#test_access` + `#visible_to` (Plan 124.6)

> **Status:** ✅ ACTIVE since 2026-06-02 (Plan 124.6 closure).
> **Extends:** D220 §3 (scope rules) + D222 §3 (protocol boundary).
> **Plan:** [Plan 124.6](../../docs/plans/124.6-friend-attrs.md).
> **Cross-refs:** D220, D221, D222 (priv core + pattern + tuple).

### §1 Motivation

D220 устанавливает **strict type-method-only** scope для priv field
access — strictнее чем эталоны (Kotlin `internal` module-wide,
Rust `pub(crate)` crate-wide, Java package-private). В некоторых
production scenarios нужна controlled relaxation:

1. **Unit tests** должны verify internal state (balance, cache size,
   internal cursor pos) без публикации getter в public API.
2. **Sibling helper types** (`Account` + `Bank` audit utilities) —
   coordinated access без friend boilerplate.

D224 вводит **two explicit opt-in escape hatches** — каждый
syntactically marked, никаких неявных relaxation.

### §2 `#test_access(TypeX[, TypeY...])` — fn-level access grant

Attribute перед `fn` declaration: body fn получает priv-field access
ко всем listed types (READ + WRITE + INIT + PATTERN).

```nova
export type Account {
    ro name str
    priv mut balance f64
}

#test_access(Account)
fn assert_balance_eq(acc Account, expected f64) -> bool =>
    acc.balance == expected        // ✅ allowed by #test_access
```

Multi-type form:
```nova
#test_access(Account, Vault)
fn cross_audit(a Account, v Vault) -> bool =>
    a.balance == 0.0 && v.amount == 0.0
```

Scope: applies **only к body of marked fn**. Caller scope unchanged.
Composable: можно combine с `#realtime`, `#blocking`, `#verify`,
etc. — порядок attribute parsing уже supports multi-attribute.

### §3 `#visible_to(OtherType[, ...])` — field-level friend declaration

Attribute перед field declaration в `type X { ... }` или
`type X(...)`: methods listed types получают priv access **только
к этому field**.

```nova
export type Account {
    ro name str
    #visible_to(Bank) priv mut balance f64
}

export type Bank {
    ro id str
}

export fn Bank @audit_account(a Account) -> f64 =>
    a.balance        // ✅ allowed: Bank ∈ Account.balance.visible_to
```

Per-field granularity:
- Different fields могут have different friend lists.
- Other Account fields without `#visible_to` — strict type-only.
- Other types (НЕ Bank) — no access:
  ```nova
  export fn Auditor @check(a Account) -> f64 =>
      a.balance     // ❌ E_PRIV_FIELD_READ — Auditor not in visible_to
  ```

### §4 Combined access predicate

priv-field access allowed когда **любое** из:

1. `current_recv_type == tname` — canonical type-method scope (D220).
2. `tname ∈ current_fn.test_access_for` — `#test_access` grant.
3. `current_recv_type ∈ field.visible_to` — friend grant.

Implementation: `TypeCheckCtx::priv_field_access_allowed(tname, &visible_to)`
combines all three checks. `priv_access_allowed_base(tname)` covers
(1)+(2); per-field `visible_to` requires field-specific context
(handled at each callsite).

### §5 Diagnostic codes

D224 reuses Plan 124.1-124.4 codes (no new codes), but hints
now mention escape hatches:

```
[E_PRIV_FIELD_READ] cannot read private field `Account.balance` ...
Hint: add public getter method on `Account`, move accessing code
into a method of `Account`, or use `#test_access(Account)` on test
fn (escape hatch — D224).
```

Parser-level errors:
- `#test_access(...)` without `(` → "требует list: `#test_access(TypeX, ...)`".
- Empty list `#test_access()` → "требует хотя бы один Type".
- `#visible_to(...)` same shape.

### §6 Anti-patterns + lint guidance

Escape hatches — **opt-in, syntactically explicit**. Recommended
discipline:

- `#test_access` — only on test fns or dedicated assertion helpers.
  Production-code uses должны trigger code-review concern.
- `#visible_to` — explicit, named friend types only. Cross-module
  abuse — code-smell.
- Future lint (Plan 124.x): warn if `#test_access` used >N times
  per project (suggests missing public API).

### §7 Cross-refs

- D220 — core priv semantics, scope rules.
- D221 — pattern destructure / spread sites.
- D222 — named tuple + protocol impl boundary.
- D102 — diagnostic format (Plan 50).
- Plan 104.x — LSP hover/completion will display escape-hatch
  badges (forward-ref).

### Acceptance — Plan 124.6

ALL closed 2026-06-02:

- A6.1 ✅ `#test_access(TypeX)` attribute parser PASS.
- A6.2 ✅ Test fn с attribute получает priv access к TypeX.
- A6.3 ✅ `#visible_to(TypeY)` field attribute parser PASS.
- A6.4 ✅ TypeY's methods get access к marked priv field of TypeX.
- A6.5 ✅ Conservative: только marked fields, не whole type
  (per-field granular).
- A6.6 ✅ plan124_6 fixtures 7/7 PASS (4 positive + 3 negative).
- A6.7 ✅ Regression plan124_1 9/9 + plan124_2 14/14 + plan124_4 10/10
  unchanged.
- A6.8 ✅ D224 NEW + cross-refs к D220-D222.

---

## D225. Type-level priv flip для named tuples (Plan 124.7)

> **Status:** ✅ ACTIVE since 2026-06-02 (Plan 124.7 closure).
> **Extends:** D220 §3.3.1 (record-form type-level flip) + D222
> (named tuple priv per-field) + D215 (named tuple form, Plan 120).
> **Plan:** [Plan 124.7](../../docs/plans/124.7-tuple-type-level.md).
> **Cross-refs:** D220, D215, D222.

### §1 Syntax

Symmetric extension to record-form D220 §3.3.1:

```nova
// Record form (Plan 124.1 / D220 §3.3.1)
type Account priv {
    pub ro name str          // explicit pub override
    mut balance f64          // default = priv (inherits type-level)
}

// Named tuple form (Plan 124.7 / D225 — this section)
type Secret priv (key str, salt str)
//             ^^^^ priv ПОСЛЕ имени type'а, ДО `(`

type Credential priv (pub id str, secret str)
//                    ^^^ explicit pub override per field
```

`priv` keyword position между type name (+ optional generics) и
opening `(` — same position как для record form's `{`.

### §2 Effective priv_field resolution

Per-field `priv_field` для named-tuple field resolves в parser
identical к record form (D220 §3.3.1):

| field-level modifier | type-level flip | effective |
|---|---|---|
| explicit `pub` | flip or no-flip | `false` (overrides) |
| explicit `priv` | flip or no-flip | `true` |
| neither | flip = `false` | `false` (default public) |
| neither | flip = `true` | `true` (inherits) |

Bidirectional `priv pub` / `pub priv` → `E_PRIV_PUB_CONFLICT` (D220 §6).

### §3 Implementation hooks

- AST `TypeDecl.default_field_priv: bool` — пере-used (no extension
  needed; Plan 124.1 уже добавила).
- Parser `parse_type_decl`: KwPriv после type-name установится в
  `default_field_priv` (existing — Plan 124.1).
- Parser `parse_named_tuple_fields_with_default(default_priv)` — NEW
  wrapper around old `parse_named_tuple_fields`. Propagates default
  в effective `priv_field` resolution per field (mirror к
  `parse_record_fields_with_default` precedent).
- Backward-compat shim `parse_named_tuple_fields()` calls _with_default(false).

### §4 Access rules — unchanged (D220 §4 / D221 / D222 / D224)

Effective priv_field после resolution applied identically к explicit
per-field priv. All Plan 124.1-124.6 enforcement sites (READ /
WRITE / INIT / PATTERN / spread, + escape hatches `#test_access`,
`#visible_to`) work uniform.

### §5 Use cases

Invariant-heavy types где majority of fields should be private:

```nova
// Encapsulated handles (private impl detail)
type Mutex priv (state u32, owner_fid u64, pub kind MutexKind)

// Sensitive data + opaque session
type Session priv (token []u8, expires_at Instant, pub user_id u64)

// Tightly-coupled coordinate types
type Vec3 priv (x f64, y f64, z f64)
```

Bimodal coverage matches kubernetes empirical: `core/v1` API surface
92% public (use no flip), `pkg/internal` 53% private (use flip + few `pub`).

### §6 Cross-refs

- D215 — named tuple base syntax.
- D220 §3.3.1 — record-form type-level flip (D225 symmetric).
- D222 — named tuple per-field priv (D225 builds on this).
- D102 — diagnostic format.

### Acceptance — Plan 124.7

ALL closed 2026-06-02:

- A7.1 ✅ Parser принимает `type X priv { ... }` syntax (record form —
  Plan 124.1 preserved).
- A7.2 ✅ Parser принимает `type X priv (...)` syntax (named tuple
  form — D225 NEW).
- A7.3 ✅ `pub` modifier на field overrides type-level priv default.
- A7.4 ✅ Field без modifier inherits type-level default (priv).
- A7.5 ✅ Type-level + field-level combinations 4 cases verified:
  default-default ✅, default-explicit ✅, flip-default ✅, flip-explicit ✅.
- A7.6 ✅ plan124_7 fixtures 8/8 PASS (5 positive + 3 negative).
- A7.7 ✅ Regression Plan 120 8/8 + plan124_1 9/9 + plan124_4 10/10
  unchanged.
- A7.8 ✅ D225 NEW + cross-refs.

### Acceptance — Plan 124.2

ALL closed 2026-06-02:

- A2.1 ✅ Match arm pattern outside → E_PRIV_FIELD_PATTERN.
- A2.2 ✅ IfLet pattern outside → error.
- A2.3 ✅ WhileLet pattern outside → error.
- A2.4 ✅ For-loop pattern outside → error (positive case verifies
  no false-positive on public-only types).
- A2.5 ✅ Nested Pattern::Record с priv inner → error.
- A2.6 ✅ Spread outside → E_PRIV_FIELD_INIT_SPREAD.
- A2.7 ✅ Inside type-method scope — все hooks allow.
- A2.8 ✅ `..` rest pattern — no false-positive.
- A2.9 ✅ plan124_2 fixtures 14/14 PASS (8+ positive, 6 negative).
- A2.10 ✅ Regression plan124_1 9/9 unchanged.

---

## D226. Signed indexing convention — `int` для `len` / `capacity` / index

> **D226 RETIRED (Plan 133, 2026-06-09):** `usize` alias удалён. `int` = address-sized
> signed integer на 64-bit Nova target. Используй `int` для размеров, индексов, счётчиков байт.
> `usize`/`isize` больше не являются допустимыми Nova-типами — компилятор выдаёт ошибку
> с подсказкой «use `int`».

> **Принято 2026-06-03.** Формализует существующую практику: extracts из
> [D130](#d130) Q3 (2026-05-19, «Indexing → Keep `int`, no change»),
> поднимает в самостоятельный D-блок.

### Что

Все API длины, ёмкости и позиции в коллекциях Nova принимают и
возвращают **signed `int`** ([D129](#d129) alias `i64`), а не unsigned
`uint`/`u64`. Это касается `@len()`, `@capacity()`, `with_capacity(n)`,
`reserve(n)`, `truncate(n)`, индексных параметров (`arr[i]`,
`s.byte_at(i)`), позиций (`indexOf`/`find` возвращают `int` с `-1` или
`Option[int]`), и slice-границ (`arr[a..b]` где `a`, `b` — `int`).

### Правило

1. **Stdlib invariant.** Любая публичная функция в `std/`, принимающая
   или возвращающая «количество элементов» / «размер в байтах» /
   «индекс позиции», использует `int`.

2. **Защита от негатива через контракты.** Capacity-API
   (`with_capacity`/`reserve` и аналоги) добавляют
   `requires n >= 0` ([D24](../09-tooling.md#d24)). Это compile-time
   проверка при Z3 backend / runtime debug-assert при TrivialBackend.
   Compile-error на `with_capacity(-1)` без attempted конверсии типа.

3. **Negative-as-sentinel разрешён.** Поиск/позиция могут возвращать
   `-1` как «не найдено» (Java/Go convention) ИЛИ `Option[int]` —
   stdlib-конвенция в пользу `Option[int]` для type-safety, но `-1`
   sentinel допустим в low-level API (`str.find_byte`).

4. **Разности — естественно signed.** `a.len() - b.len()`,
   `xs.len() - 1`, обратные циклы `for i in (0..n).reverse()` работают
   без явных кастов или underflow-паник. На пустой коллекции
   `xs.len() - 1` даёт `-1`, что валидно как loop-guard вход
   (`for j in 0..-1` — пустой range).

5. **`uint`/`u64` — только для bit-twiddling, FFI и pointer bridge.** Hash-значения,
   битовые маски, raw memory addresses, sized-integer аргументы C-API — это `u64`/`uint`.
   **`usize`/`isize` удалены (Plan 133, 2026-06-09)** — используй `int` для address-sized
   операций и FFI size parameters. FFI-сигнатуры теперь пишут `n int`, C-codegen кастит
   `intptr_t`→`size_t` внутри.

6. **Future-arch path.** При миграции Nova на multi-arch (32-bit / WASM)
   `int` = platform-pointer-width signed (= `intptr_t`),
   `i64` остаётся fixed-64. Index API не меняется — auto-scale без
   breaking change. См. [D129](#d129) migration note.

7. **Pointer interactions.** Pointer arithmetic и pointer-integer
   bridges имеют свою numeric matrix, ортогональную stdlib index-API
   (Rule 1). Все signed для offset/diff (в духе Rule 4 «разности
   естественно signed»). `usize`/`isize` удалены — FFI-ABI через `int`.

   | Операция | Тип | Где |
   |---|---|---|
   | `coll.len()` / `coll[i]` | `int` | stdlib index-API (Rule 1) |
   | `arr[a..b]` slice bounds | `int` | sub-slice views ([D144](#d144)) |
   | `ptr + N` / `*T + N` offset | `int` | pointer arith ([D216](#d216) §6) — scaled by `sizeof(T)` |
   | `ptr - ptr` / `*T - *T` diff | `int` | element count ([D216](#d216) §6) — signed |
   | `external fn(..., sz int)` | `int` | FFI ABI ([D214](#d214), [D216](#d216) FFI) — codegen casts `intptr_t`→`size_t` |
   | `p as int` / `int as *T` | `int` | explicit address cast — opaque handle, hash key, GC-hazard ([D214](#d214) §casts) |
   | `ptr as u64` / `i64 as ptr` | `u64`/`i64` | opaque handle storage ([D214](#d214) §casts) |

   **Правило:** stdlib API никогда не использует `uint`/`u64` для
   index/len/capacity (Rule 1); FFI / pointer arithmetic / cast bridges
   — единственные легальные exemptions.

### Почему

**Industry baseline (2026-06).**

| Язык | Index/len тип | Знак | Hindsight |
|---|---|---|---|
| Go | `int` (platform-word) | **signed** | Сознательный выбор после C |
| Swift | `Int` (platform-word) | **signed** | Apple: «harder to make off-by-one errors» |
| Java | `int` (i32) | **signed** | Историческое; принято |
| Kotlin | `Int` (i32) | **signed** | Mirror Java |
| C# | `int` (i32) | **signed** | `LongLength` для >2B |
| Python | `int` (arbitrary) | **signed** | Negative-index slicing |
| TypeScript | `number` (f64) | signed (de facto) | Один тип |
| Rust | `usize` (platform) | unsigned | Community regrets vocal |
| C++ STL | `size_t` (platform) | unsigned | **Stroustrup: «I regret using unsigned for size in STL»** |
| Zig | `usize` (platform) | unsigned | Embedded-first рационал |

Счёт 7:3 в пользу signed. Двое из трёх unsigned-языков (C++ и Rust)
имеют публичные authorial regrets.

**Конкретные выгоды signed `int` для Nova:**

1. **Нет underflow-trap.** `xs.len() - 1` на пустом vec не паникует
   (даёт `-1`), в отличие от Rust `0_usize - 1` → overflow panic.
   Это — самая частая newbie-trap в Rust.

2. **Sentinel `-1`.** Эргономика find/indexOf без обязательной
   `Option`-аллокации.

3. **Разности и diff-логика.** `a.len() - b.len()` валидно signed;
   sorting comparators, position deltas, scroll offsets — все естественны.

4. **Mixed arithmetic без ceremony.** Никакого `(x as int) + i`,
   `(len as int) - 1`. AI-first killer-use ([D10](../01-philosophy.md#d10)):
   LLM пишет signed-индексацию правильно чаще, чем балансирует
   `uint`/`i64` касты.

5. **Bit-width аргумент мёртв на 64-bit.** Signed-`int` (= `i64`) даёт
   `2⁶³ − 1` ≈ 9.2 × 10¹⁸ элементов — никакая коллекция в адресном
   пространстве этого не достигнет.

6. **Совместимость с overflow-семантикой.** [Plan 33.8](../../docs/plans/33.8-verifier-soundness.md)
   Ф.1: `int` overflow → `nv_panic` (`__builtin_*_overflow`). Если
   ввести `uint` для len, мы заменим один trap (overflow on
   saturation) на другой (underflow on `0 - 1`) — без выигрыша.

7. **Effect/protocol симметрия.** Все примитивные методы
   (`hash`/`eq`/`lt`/etc., [D109](../08-runtime.md#d109)) уже работают
   с `int`. Введение второго numeric vocabulary для размеров
   удвоит type-checker complexity без semantic gain.

**Type-encoded invariant («n ≥ 0»)** — частично покрывается контрактами
(`requires n >= 0`), которые при Z3 backend ([D24](../09-tooling.md#d24))
дают **compile-time** гарантию того же уровня, что unsigned type.

### Что отвергнуто

1. **`uint`/`u64` для index/len** (как Rust `usize`, C++ `size_t`).
   Отвергнут [D130](#d130) Q3 (2026-05-19): breaking change для 100+
   APIs; underflow-trap хуже missing type-invariant; runtime
   contract-based check покрывает основной use-case.

2. **Mixed convention** (`uint` для capacity, `int` для index).
   Отвергнут: создаёт постоянные касты на границе API, удваивает
   protocol-method matrix.

3. **Refinement type `nat = {x int | x >= 0}` как параметр капасити.**
   Отвергнут на bootstrap: refinement-types — long-term Plan 33.x
   (после full SMT integration); сейчас `requires n >= 0` даёт ту же
   проверяемость без grammar-changes.

### Связь

- [D129](#d129) — `int = i64` alias decision (foundation).
- [D130](#d130) — `uint` symmetric pair + Q3 indexing decision (historical origin).
- [D24](../09-tooling.md#d24) — `requires`/`ensures` контракты для compile-time проверки.
- [D54](../03-syntax.md#d54) — `int as uint` saturation (cross-type bridge).
- [D109](../08-runtime.md#d109) — встроенные методы примитивов (включая `int`).
- [D141](../08-runtime.md#d141) — `byte_at`/bulk slice API использует `int` индексы.
- [D144](#d144) — sub-slice views `arr[a..b]` — границы `int`.
- [D214](#d214) — `ptr` opaque type + cast rules (usize removed, use int for ABI bridge).
- [D216](#d216) — `*T` typed pointer family + arithmetic (`int` offset + diff) + FFI.
- [Plan 33.8](../../docs/plans/33.8-verifier-soundness.md) — `int` overflow → panic (soundness).

### Эволюция

- **2026-05-19** ([D130](#d130) Q3): Решение «keep `int` for indexing,
  no change» принято внутри `uint` plan'а — внутри одного из четырёх
  Q-вопросов, не findable отдельно.
- **2026-06-03** (D226, этот блок): формализация в самостоятельное
  D-решение + правило `requires n >= 0` на capacity-API +
  cross-language baseline + future-arch migration path.
- **2026-06-03** (D226 amend, pointer-aware): §5 расширен для
  `usize` ABI bridge + pointer-integer casts; §7 «Pointer interactions»
  с numeric matrix для всех ptr ops; cross-refs на [D214](#d214) +
  [D216](#d216). Закрывает gap research'а §3 ([docs/dev/research/08](../../docs/dev/research/08-int-width-and-literal-inference.md)).
- **2026-06-09** (Plan 133): `usize`/`isize` удалены из Nova. `int` = address-sized
  signed integer (`intptr_t` на 64-bit). FFI-сигнатуры используют `int`, codegen кастит
  `intptr_t`→`size_t` внутри. D226 RETIRED в части `usize`/`isize` alias-semantics.

### Acceptance criteria

- [x] `std/collections/hashmap.nv` `with_capacity(min_capacity int) requires min_capacity >= 0`
- [x] `std/collections/set.nv` `with_capacity(cap int) requires cap >= 0`
- [x] `std/runtime/string_builder.nv` `with_capacity(n int) requires n >= 0`
- [x] `std/runtime/write_buffer.nv` `with_capacity(n int) requires n >= 0`
- [x] D226 spec block с industry baseline + rationale + rejected alternatives
- [ ] `[]T.with_capacity` / `[]T.reserve` built-in: requires-clause в
  `compiler-codegen` external_registry — followup `[M-D226-builtin-capacity-requires]`
- [ ] `nova check` lint W_D226_NEGATIVE_LITERAL — warn на `with_capacity(-N)`
  при literal-args (без Z3) — followup `[M-D226-negative-literal-lint]`
- [ ] `_experimental/` capacity APIs (`Queue.with_capacity`) — sweep после
  promotion в stable.
- [x] `isize` / `usize` удалены (Plan 133, 2026-06-09) — closes
  `[M-D226-isize-usize-alias-D-block]`.

### Amend 2026-06-03 — `usize` / `isize` formal alias D-block

> **RETIRED (Plan 133, 2026-06-09):** `usize` и `isize` удалены как Nova-типы.
> Используй `int` для размеров, индексов и address-sized операций. `uint` остаётся
> для беззнаковых битовых операций и FFI. Раздел оставлен как исторический контекст.

Closes followup `[M-D226-isize-usize-alias-D-block]`.

**Definition (HISTORICAL — типы удалены в Plan 133):**

| Alias | Bootstrap (64-bit) | Future arch |
|---|---|---|
| `usize` | `u64` (= `uint64_t`) | platform-pointer-width unsigned |
| `isize` | `i64` (= `nova_int`) | platform-pointer-width signed |

**Use cases:**

1. **FFI ABI bridge** (primary use) — C `size_t` / `ptrdiff_t` (HISTORICAL, до Plan 133):
   ```nova
   // БЫЛО (до Plan 133):
   external fn malloc(sz usize) -> Option[*u8]             // C: size_t
   external fn read(fd int, buf *mut u8, n usize) -> isize // C: size_t, ssize_t
   // СТАЛО (Plan 133):
   external fn malloc(sz int) -> Option[*u8]              // C: size_t — codegen casts intptr_t→size_t
   external fn read(fd int, buf *mut u8, n int) -> int    // C: size_t, ssize_t
   ```

2. **Pointer differences** (D216 §6) — `ptr - ptr → int`, signed semantically.

3. **Platform-pointer-width** — `int` = `intptr_t` на текущих 64-bit targets (Plan 133).

**НЕ для:**

- `len`/`capacity`/index APIs — используют `int` (per D226 Rule 1). Reason:
  signed convention; arithmetic safety (signed underflow detectable).
- General-purpose unsigned arithmetic — используют `uint` (= `u64` alias)
  per Plan 70.5.

**Casts (после Plan 133):**

- `ptr as int` / `int as *T` — explicit, allowed для opaque handles +
  address-as-integer (D216 §6, D214).

**Spec drift fix:** `isize`/`usize` использовались в D216/D214 examples и
Plan 118 FFI без formal D-block aliasing. Plan 133 удаляет эти типы целиком.

**Implementation:** `compiler-codegen/src/codegen/emit_c.rs` + `types/mod.rs`
type_ref_to_c + TyCat::Int + BUILTIN_TYPE_NAMES registry updated 2026-06-03.

**Cross-refs:**

- [D129](#d129) — `int` = `i64` aliasing
- [D216 §6](#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — pointer arithmetic
- [Plan 118.1](../../docs/plans/118.1-ffi-intrinsics-and-cstring.md) — FFI signatures use usize

---

## Plan 124.8 — Tuple+Value-Record design refinement (2026-06-02)

Sub-plan Plan 124 V2 refinement. Amends 6 D-blocks + introduces 1 NEW.
Status: ✅ ACTIVE since 2026-06-02 (Ф.0-Ф.5 closed).

### D33 amend §«binding propagation» (Plan 124.8 Ф.2)

> ⚠️ **AMENDED by D216 V3 §V3.1** (2026-06-04, refined 2026-06-05 Ф.6) — rows «`ro x mut T`» / «`mut x ro T`» в этой таблице storage-class qualified:
> - **Type-form** `ro mut T` / `mut ro T` (без имени между modifier'ами): forbidden когда T = value type (см. §V3.1 — primitives, value records, named/anonymous tuples, Unit). Error `E_MUTABILITY_CONFLICT_VALUE_TYPE`.
> - **Binding-form** `ro x mut T` / `mut x ro T` (с именем между): allowed regardless of T storage class (Ф.6 relaxation, 2026-06-05).

`ro`/`mut` на binding **по default распространяется** на тип справа.
Explicit повторение модификатора — redundant error.

| Декларация | Парсится | Семантика |
|---|---|---|
| `ro x T` | ✅ default | binding ro, type implicit ro |
| `mut x T` | ✅ default | binding mut, type implicit mut |
| `ro x ro T` | ❌ `E_REDUNDANT_TYPE_MODIFIER` | то же что `ro x T` |
| `mut x mut T` | ❌ `E_REDUNDANT_TYPE_MODIFIER` | то же что `mut x T` |
| `ro x mut T` | ✅ NEW | binding ro, content mut (cannot reassign, can mutate) |
| `mut x ro T` | ✅ existing | binding mut, content ro (can reassign, cannot mutate) |

Closes D176 §«type-modifier в любой позиции» partial parser implementation
gap — `mut T` теперь принимается в binding type annotation position.

### D33 amend §«consume binding-only — distinction rationale» (Plan 118.5 V2, 2026-06-04)

> **Closes [M-118.5-consume-as-type-modifier].**

`consume` НЕ становится type-level wrapper parallel к `ro` / `mut` / `unsafe`.
Stays **binding-only** modifier (per D131, D133, D162, D164).

**Rationale:**

`ro` / `mut` / `unsafe` — syntactic compile-time modifiers expressing
mutability / safety contract на the type. `consume` — fundamentally
different: it expresses **ownership transfer / linearity / drop
obligation** — semantic D-блоки D131 (linearity), D133 (consume types),
D162 (consume types implementation), D164 (D-block consume types).

Examples of the asymmetry:
- `ro T` value — ro view; multiple ro aliases allowed.
- `mut T` value — mutable; subject к binding-dominates rule (D175 amend).
- `unsafe T` value — MaybeUninit; read requires assertion (D216 V2 §V2.3).
- `consume T` value — **owned uniquely**; passing transfers ownership,
  drops invariants at scope exit. Not a syntactic property of T; a
  **structural property of the binding**.

Hypothetical `consume * T` («consume pointer») would mean a pointer that
the caller must consume — но это уже expressed via record-wrapped pointer
(`type Handle consume(* T)`). Plain `consume T` за `T` уже-not-consume
makes no semantic sense.

**Decision:** keep current consume design (D162, D164). Right-binding
rule applies only к ro/mut/unsafe. Reject Plan 118.5 V2 followup
`[M-118.5-consume-as-type-modifier]` as **NO ACTION** — consume already
fits its semantic-binding role correctly.

### D33 amend §«Projection-chain mutability check» (Plan 128.2, 2026-06-06)

> **Closes `[M-128.1-ro-binding-field-chain-not-mut]`** (P1 safety hole
> opened в Plan 128.1 Ф.3).

D33 locality-of-mutation invariant (Plan 108.2 D36 enforcement) is
extended: mut-method dispatch проверяет mutability **root binding**'а
lvalue projection chain, не только когда receiver — голый identifier.

**Rule:**

Для Call `obj.method(...)`, где `method` — mut-method (`fn T mut @method`),
type-checker walks Member/IndexAccess chain от `obj` к root:

```
walk_root(e) =
  | Ident(name)         → Some(name)
  | Member { obj, .. }  → walk_root(obj)
  | IndexAccess { obj, .. } → walk_root(obj)
  | _                   → None
```

Если `walk_root(obj) = Some(name)`:
- `local_mut[name] == Some(false)` (ro local) → `E_LOCAL_NOT_MUT`
  (или `E_RECEIVER_BINDING_NOT_MUT` — chain hint в note).
- `param_mut[name] == Some(false)` (ro param) → `E_PARAM_NOT_MUT`.
- otherwise — OK.

Если `walk_root(obj) = None` (chain начинается с rvalue base — Call
result, literal, …): no enforcement; mutation в hoisted temp семантически
no-op (D32 «mutate-by-copy для rvalue» — D215 amend «Method receiver
passing» Ф.2 §rvalue receiver).

**Receiver shapes table:**

| Receiver shape | binding | Result |
|---|---|---|
| `b.set_x()` | `ro b` | `E_LOCAL_NOT_MUT` (existing) |
| `b.v.set_x()` | `ro b` | `E_LOCAL_NOT_MUT` (NEW — chain root) |
| `arr[0].set_x()` | `ro arr` | `E_LOCAL_NOT_MUT` (NEW — chain root) |
| `b.parts[i].v.set_x()` | `ro b` | `E_LOCAL_NOT_MUT` (NEW — chain root) |
| `b.v.set_x()` | `mut b`, `mut v` field | OK |
| `make_body().v.set_x()` | rvalue base | OK (no-op semantically — temp) |

**Symmetry с D175 (ro field freeze):** projection-chain root check
независимая ось от per-field ro enforcement. `mut b; b.v.set_x()`
с `ro v` field остаётся `E_FIELD_NOT_MUT` (D175 invariant); orthogonal
к D33 root walk.

**Cross-ref:** D215 amend «Method receiver passing» (Plan 128.1 Ф.1)
implements call-site codegen (`&(b->v)`, `&(arr->data[i])`) для lvalue
projection — это codegen pair того же chain-walking; D33 root check —
type-checker pair. Symmetric infrastructure: оба обходят
Member/IndexAccess chain (codegen — для emit, type-checker — для
binding-mutability gate).

**Implementation:** helper `lvalue_root_ident` в `types/mod.rs`
вызывается из `consume_walk_expr` Call arm для receiver. Pure-read
methods (`x.abs()`) не подпадают под gate — `registered ||
builtin_mut_method` guard сохраняется.

### D215 cross-ref §«projection root binding mutability» (Plan 128.2, 2026-06-06)

> Pair note к D33 amend §«Projection-chain mutability check».

D215 lvalue-projection mut-method ABI (Plan 128.1 Ф.1) — `&(b->v)` для
`b.v.method()`, `&(arr->data[i])` для `arr[i].method()` — corresponds к
codegen pair того же invariant'а. Type-checker side (D33 projection-chain
root walk) проверяет, что root binding lvalue chain'а — `mut`; codegen
side (D215 §«lvalue projection receivers») emit'ит pointer на slot.

**Implication для users:** writing `b.v.set_x()` requires `mut b`
binding **AND** `mut v` field (D175 cross-ref) **AND** mut-receiver
target method (D32). All three axes independent; missing любой —
distinct error code (E_LOCAL_NOT_MUT / E_FIELD_NOT_MUT / receiver-mode
mismatch).

**Plan 128.2 §Markers closure:** `[M-128.1-ro-binding-field-chain-not-mut]`
(P1) — closed via root-walking enforcement.

### D33 amend §«Fluent `-> @` chain-receiver mutability gate» (ex-Plan 172.5 R6, fix 2026-07-10)

> **Closes `[M-172.5-chain-gating-ro-at]`** (the marker's own description of
> the fix was stale — it referenced `ParamRefMode`/`mut ref` param-form
> machinery that Plan 184 fully retracted — but the underlying soundness
> hole it named survived, sharpened by D326-Plan184 Р7).

The `walk_root(obj) = None` escape hatch above ("chain начинается с rvalue
base — Call result, literal, … → no enforcement; mutation в hoisted temp —
no-op") was sound **before** Plan 184: a value-record `-> @` was a copy
(D246 R7b), so mutating the tail of a chain rooted in a Call result was
truly a no-op on a temporary. **After** [D326-Plan184 Р7](#ревизия-d326-plan184-ref-t--ограниченный-тип),
`-> @` returns a genuine `ref Self` even for a **non-mut** method — the
no-op premise no longer holds for that specific shape.

Empirically: `fn T @peek() -> @ { }` / `fn T mut @bump() -> @ { @x += 1 }` —
`c.peek().bump()` on `mut c = T{x: 0}` compiled and left `c.x == 1`
(not `0`). `peek()` never declared `mut`, yet its `-> @` aliased the same
storage as a genuine mut method would — the ro/mut method-declaration axis
was silently bypassable through a chain.

**Rule (amendment):** `walk_root` above additionally recognizes a **Call**
receiver shape. When `obj` (the receiver of a mut-method Call) is itself a
Call `inner_obj.inner_method(...)`:

- If `inner_method` is a **confirmed registered ro-instance method** (Plan
  135 `ro_methods`, arity-matched — see below) that is **also declared with
  `-> @`** (D132 self-return, `recv_returning`) → chaining a mut method off
  it is rejected: **`E_RECEIVER_BINDING_NOT_MUT`**.
- If `inner_method` is itself mut (`X.inc().inc()`, the ordinary D132
  fluent-builder idiom) — legal, unchanged.
- If `inner_method` is **not** a self-return (`-> @`) method at all — e.g.
  `filter()`/`map()`/`chars()`/a static constructor (`T.new()`) — it builds
  a genuinely fresh value, not an alias; the original `walk_root = None`
  no-op-temp reasoning still applies unchanged. Legal.

**Arity-aware registries (implementation note):** the pre-existing
name-only `mut_methods`/`ro_methods` sets (Plan 108.1/135) collapse
arity-overloaded same-name pairs — e.g. `@cap() -> int` (0-arg ro getter)
vs `mut @cap(n) -> @` (1-arg setter), the canonical `T.new().cap(n)`
construction idiom (D117 amend) present throughout std. The gate above uses
new companion sets `mut_methods_arity`/`ro_methods_arity`
(`(receiver_type, method_name, arity)`) to disambiguate by argument count,
and additionally requires the SAME (name, arity) pair to never appear as
mut ANYWHERE in the module before firing — a conservative, false-positive-
averse trade-off (matches this section's existing "acceptable trade-off"
stance on name-only chain-root heuristics).

**Verified still legal (regression guard):** all-mut `-> @` chains
(`v.bump().bump()`), a fresh chain rooted in a static constructor
(`T.new().bump()`), and a mut method chained after a non-self-return
adapter (`v.doubled().bump()`) — see
`spec_tests/conformance/d326_chain_gating_ro_at.nv`. Rejected shape —
`spec_tests/conformance/neg/d326_chain_gating_ro_at_neg.nv`.

**Scope note:** the gate fires only when BOTH ends are positively confirmed
(inner = confirmed ro AND self-return; outer = confirmed mut) — it is a
narrow, additive check layered on top of the existing root-walk, not a
replacement. It does not attempt full receiver-type resolution (out of
scope for the linear `ConsumeCtx` pass); a hypothetical cross-type name
collision at the SAME arity (one type's mut `foo(n)` vs another's ro
self-return `foo(n)`) is a known, accepted miss (favors soundness of the
**existing** green tree over completeness of this new check).

### D216 V2 amend §V2.2b «mut T transparent» (Plan 118.5 V2, 2026-06-04)

> **Closes [M-118.5-mut-t-vs-binding-distinction].**

`TypeRef::Mut(T, span)` AST wrapper introduced in Plan 118.5 V1 (per V2
right-binding rule §V2.1) is **purely transparent** at C codegen level:

- AST representation: `Mut(inner, span)` carries no extra semantic vs `inner`
- C codegen: `mut T` emits same C type as `T` (через transparent recurse)
- Type-checker: `mut T` does NOT impose mutability requirement at the type
  level — the mutability semantic belongs к **binding-level mut** (Plan 108
  D176), не к type-level.

**Why mut T exists at all:**

Only purpose is **syntactic uniformity** under right-binding rule §V2.1.
Without `mut T` arm в parse_type, the user's natural extension of `ro T`
к `mut T` would fail к parse (no recursive arm). Adding the arm makes the
grammar regular and predictable. The wrapper has zero semantic at runtime.

**Practical implications:**

- `mut int` parses as `Mut(Named("int"))` — wrapper transparent for codegen
  (emits just `nova_int`). Type-checker doesn't validate Mut(non-Pointer)
  as anything special.
- `*mut T` parses as `Pointer(Mut(T))` — `Mut` is the **pointee** modifier
  (postfix, INNER of Pointer), означает writable target. **`mut * T` (Mut
  снаружи Pointer) НЕ строится из синтаксиса** — prefix перед `*` запрещён
  (`E_POINTER_PREFIX_MODIFIER`, §1, Plan 138.5). «Mut pointer» (reassignable
  binding) выражается binding'ом `mut p *T`, не type-wrapper'ом.
- `let mut x int = ...` — the **binding** mut (Plan 108) provides mutation
  rights. `let x mut int = ...` would parse but the `mut` wrapper on `int`
  doesn't grant mutation (binding `x` is implicit ro per Plan 108).

**Disambiguation reference:**

| Form | Meaning | Source |
|------|---------|--------|
| `let mut x T = ...` | Binding `x` is mutable | Plan 108 D176 |
| `let x mut T = ...` | Binding `x` is ro; type wrapper transparent (no mutation rights) | Plan 118.5 V2 §V2.2b |
| `let mut x mut T = ...` | Binding mut; type wrapper transparent (same as `let mut x T = ...` semantically) | combined |

User-visible recommendation: **prefer binding-level `mut`**; type-level
`mut T` is for syntactic uniformity only.

### D218 RETRACTED (Plan 118.5 V2, 2026-06-04)

> **Closes [M-118.5-d218-maybeuninit-duplication].**

D218 (Plan 118.2 — «Slice fat-pointer + MaybeUninit[T] + ManuallyDrop»)
**partially retracted** — the MaybeUninit[T] sub-design is subsumed by
Plan 118.5 V2 §V2.3 first-class `unsafe T` wrapper.

**What D218 proposed:**

`MaybeUninit[T]` generic wrapper providing «memory typed as T but may be
uninitialized» semantic. Caller asserts validity via `assume_init()`
method.

**Why subsumed:**

Plan 118.5 V2 §V2.3 promoted `unsafe T` to first-class type wrapper with
*exactly* the MaybeUninit semantic:
- init/layout/aliasing/identity contracts off (per §V2.3)
- read requires `unsafe { }` wrap (E_UNSAFE_T_READ_REQUIRES_WRAP)
- write safe (transitions к valid)
- narrow `unsafe T → T` requires explicit unsafe cast
  (E_UNSAFE_T_NARROW_REQUIRES_UNSAFE)

This first-class wrapper:
- Composes orthogonally с pointer modifier («ptr к uninit T» = `*unsafe T`,
  postfix pointee — Plan 138.5)
- Doesn't require generic instantiation
- Uses universal right-binding rule for grammar uniformity
- Provides finer-grained codegen control (NPO recalc per §V2.4)

**What survives in D218:**

The «slice fat-pointer» portion of D218 remains a valid sub-plan
(Plan 118.2 Ф.1+Ф.2). ManuallyDrop redesign is separate.

**Migration path для users:**

| D218 form (deprecated) | Plan 118.5 V2 form |
|------------------------|--------------------|
| `MaybeUninit[i32]`     | `unsafe i32`       |
| `MaybeUninit<T>::uninit()` | `mut x unsafe T = uninit_value` (write-safe init) |
| `m.assume_init()`      | `unsafe { m as T }` (narrow cast) |
| `*mut MaybeUninit<T>`  | `*unsafe T` (postfix pointee к uninit T) |

`MaybeUninit[T]` type itself **not added** в std. D218 spec section marked
RETRACTED для MaybeUninit subset; slice + ManuallyDrop subsets remain
unchanged pending Plan 118.2 implementation.

### D216 V2 amend §V2.3b «E_UNSAFE_ARG_REQUIRES_WRAP + E_UNSAFE_T_NARROW_REQUIRES_UNSAFE» (Plan 118.5 V2)

> Closes [M-118.5-narrow-cast] + [M-118.5-arg-coerce-unsafe] spec slots.

Two new error codes added для Plan 118.5 V2:

- `E_UNSAFE_T_NARROW_REQUIRES_UNSAFE` — explicit narrow cast `x as T` (where
  x: unsafe T binding and T is non-unsafe target) outside unsafe block.
  Caller must assert value validity via `unsafe { x as T }`.
- `E_UNSAFE_ARG_REQUIRES_WRAP` — passing unsafe-T binding as Ident argument
  к function/method parameter whose declared type is NOT `unsafe T`. Param-
  level mismatch detected via ConsumeRegistry `fn_non_unsafe_params` /
  `method_non_unsafe_params` registries.

Both errors are suppressible by:
- Wrapping the entire enclosing expression в `unsafe { ... }` block
  (depth > 0 disables the checks).
- Re-declaring the callee parameter as `unsafe T` (then arg-coerce matches).

---

## D216 V3 amend (Plan 118.5 V3, 2026-06-04) — 4 modifier composition rules

> **⚠️ LARGELY SUPERSEDED — Plan 138.5 (2026-06-11):** V3 строился вокруг
> **prefix-модификаторной пропагации** (outer modifier перед `*` распространяется
> через Pointer на pointee) + `safe`-стоппера. После запрета prefix перед `*`
> (`E_POINTER_PREFIX_MODIFIER`, §1) **пропагировать нечего**:
> - **§V3.3** (right-binding propagation через Pointer) — **SUPERSEDED** (нет
>   outer-модификатора над Pointer → ничего не пропагируется).
> - **§V3.4** `safe` стоппер + `Unsafe(Pointer)` пропагация — **RETIRED** (`safe`
>   останавливал outer-unsafe-пропагацию, которой больше нет). Type-level
>   E_REDUNDANT для `ro * ro T` / `unsafe * unsafe T` — moot (prefix запрещён).
> - **§V3.2** ordering — **flip на `ro unsafe T`** (safety-INNER, вплотную к базе,
>   как `external unsafe fn`); касается только value-T / pointee, редкий случай.
> - **§V3.1** (ro+mut adjacency на value-T) — **KEPT** (про value-T mutability
>   class, не про указатели); pointer-примеры в нём заменены на postfix/binding.
> **Builds on (residual):** D216 V1 (postfix pointee) + V2 §V2.3 (`unsafe T` value).

### §V3.1 — Storage-class-aware ban на `ro+mut` adjacency

`ro` и `mut` — modifiers same mutability class. Their combination at type
level OR binding+content level requires storage-class qualification.

**Rule:**

**Distinction binding-form vs type-form (V3 amend 2026-06-05):**

§V3.1 storage-class ban applies ONLY к **type-position** `ro mut T` /
`mut ro T` (where both modifiers appear consecutively на одном уровне
TypeRef). **Binding-position** `ro x mut T` / `mut x ro T` (modifiers
вокруг имени параметра/локала) — orthogonal binding modifiers, ALWAYS
allowed regardless of T's storage class. Closes
[M-118.5-V3-binding-context-relaxation] (was V4 deferred).

- For **value-type T** (storage IS value):
    - primitives (`int`, `bool`, `f64`, etc.)
    - value records (`type X value { ... }` per Plan 124.8 D228)
    - named tuples (`type Point(x f64, y f64)` per Plan 120 D215)
  
  Type-position conflicting combinations → **`E_MUTABILITY_CONFLICT_VALUE_TYPE`**:
    ```nova
    fn f(p *ro mut int)              // ❌ — pointee ro+mut on value T (postfix chain)
    fn f(p *mut ro Point)            // ❌ — same (postfix)
    fn f(p ro mut int)               // ❌ — type-form (name absent before modifiers)
    fn f() -> mut ro str             // ❌ — return type-form
    type X { field ro mut Acc }      // ❌ if Acc is value record
    ```
    (Note Plan 138.5: prefix `* ro mut int` сам по себе — `E_POINTER_PREFIX_MODIFIER`;
    pointee-adjacency `*ro mut int` postfix — это §V3.1 value-T conflict.)
  
  Binding-form ALLOWED для value-T:
    ```nova
    fn f(ro x mut int)               // ✅ — binding-form (ro pre-name, mut post-name)
                                     //   ro x: no rebind (binding-level)
                                     //   mut: mut-method access (binding-level)
    fn f(mut x ro int)               // ✅ — symmetric
    let ro x mut int = 5             // ✅ — local-binding form (parser may flag E_LOCAL_*
                                     //   for non-mut mutation attempts — orthogonal)
    ```

- For **reference-type T** (T-as-pointer-к-data semantically):
    - records (`type X { ... }` default — heap)
    - arrays `[]T`
    - heap-tracked types
  
  Both type-form AND binding-form VALID:
    ```nova
    fn f(ro mut Acc)                 // ✅ type-form, ref-T (Readonly(Mut(Acc)))
                                     //   semantically: ro binding to mut content
    fn f(ro acc mut Acc)             // ✅ binding-form
                                     //   ro acc: no rebind / mut access on binding
    fn f(mut acc ro Acc)             // ✅ symmetric binding-form
    ```

**For-loop exception:** `for y in iter` — loop variable `y` semantically
ro but reassigned per iteration. Plan 108.3 loop-var rule preserves this
behavior; V3 §V3.1 does NOT fire on loop-var-introduced rebindings.

**Value types per V3 (user-confirmed 2026-06-04):**

1. **Primitives** (full list):
   - Numeric: `int` (address-sized, = i64 on 64-bit; use for sizes, indices, counts), `uint`, `i8`/`i16`/`i32`/`i64`,
     `u8`/`u16`/`u32`/`u64`, `f32`, `f64`
   - Other: `bool`, `char`, `byte` (alias `u8`), `str`, `ptr`
   - Note: `usize`/`isize` **removed** (Plan 133, 2026-06-09) — use `int`.
2. **Value records**: `type X value { ... }` (Plan 124.8 D228)
3. **Named tuples**: `type Point(x f64, y f64)` (Plan 120 D215)
4. **Anonymous tuples**: `(A, B, C)` literal type syntax
5. **Unit**: `()` (zero-size value)
6. **Fixed arrays** `[N]T` ([M-fixed-array-value-semantics], 2026-07-10,
   D27-амендмент): inline value-класс (стек / поле-по-месту, C `T name[N];`),
   копирующее присваивание/передача. Элементы могут быть кучевыми
   (`[N]str`, `[N]*T`) — контейнер всё равно value, как и у Tuple.

**Reference types per V3:**

- Records (`type X { ... }` default — heap)
- Arrays `[]T` (≡ `Vec[T]`, D239; `[N]T` — value, см. п.6 выше)
- Pointer (any modifier wrapping)
- Func, Protocol

**Storage class detection** (compiler-codegen/src/types/mod.rs helper):
```rust
fn is_value_type_for_v3(ty: &TypeRef, type_decls: &TypeDeclRegistry) -> bool {
    use TypeRef::*;
    match ty {
        Named { path, .. } if path.len() == 1 => {
            let name = path[0].as_str();
            // Primitives (Plan 133: isize/usize removed; int = address-sized)
            if matches!(name,
                "int" | "uint"
                | "i8" | "i16" | "i32" | "i64"
                | "u8" | "u16" | "u32" | "u64"
                | "f32" | "f64"
                | "bool" | "char" | "byte" | "str" | "ptr") { return true; }
            // User type: value record OR named tuple
            if let Some(td) = type_decls.get(name) {
                return td.is_value_record() || td.is_named_tuple();
            }
            false
        }
        Tuple(..) => true,    // anonymous tuples — value
        FixedArray(..) => true,   // [N]T — inline value ([M-fixed-array-value-semantics], D27-амендмент 2026-07-10)
        Array(..) => false,       // []T — heap (Vec canon, D239)
        Pointer(..) => false,
        Func { .. } => false,
        Protocol { .. } => false,
        Unit(..) => true,
        // Modifier wrappers strip к inner
        Readonly(inner, _) | Mut(inner, _) | Unsafe(inner, _) => {
            is_value_type_for_v3(inner, type_decls)
        }
    }
}
```

**Note re int (Plan 133 amend, 2026-06-09):**

`int` = address-sized signed integer (= `i64` on 64-bit); `isize` и `usize` удалены.
`uint` = address-sized unsigned (= `u64` on 64-bit). V3 storage-class check
распознаёт `int`/`i64` как одно и то же value type.

**Conflict detection** — at check_decl_type (compiler-codegen/src/types/mod.rs):
```rust
fn check_v3_ro_mut_conflict(
    ty: &TypeRef,
    type_decls: &TypeDeclRegistry,
    errors: &mut Vec<Diagnostic>,
) {
    // Recursive walk:
    // For each Readonly(Mut(inner)) or Mut(Readonly(inner)) AST shape found,
    // determine inner's storage class. If value-type → error.
    //   Pure-type-level (in pointer/array interior) — also error for
    //   value-type T (no binding context to disambiguate).
    //   For reference-type T, allow at binding-context (let/param/field),
    //   disallow at nested-constructor position.
    ...
}
```

**Retracts D33 amend rows:** the previous unconditional «`ro x mut T` ✅»
and «`mut x ro T` ✅» entries get storage-class qualification per §V3.1.

### §V3.2 — Modifier ordering (FLIPPED to safety-inner: `ro unsafe T`)

> **⚠️ FLIPPED (Plan 138.5):** прежнее правило «safety-outer / mutability-inner»
> (`unsafe ro T`) **перевёрнуто** на **safety-inner / mutability-outer**
> (`ro unsafe T`) — `unsafe` вплотную к базе T, как `external unsafe fn` ставит
> `unsafe` вплотную к `fn`. Семантически свободно: оси ортогональны
> (`Readonly(Unsafe(T))` ≡ `Unsafe(Readonly(T))`). Касается только **value-T**
> и **pointee** (после forbid-prefix `*ro unsafe T` — редкий случай); для
> указателя-binding ordering не возникает.

**Rule (FINAL):** `ro`/`mut` (mutability class) — **outer**; `unsafe` (safety
class) — **inner** (вплотную к базе). Reverse → **`E_MODIFIER_ORDER`**.

**Rationale:** консистентность с keyword-формой `external unsafe fn` (unsafe
непосредственно перед тем, что оно квалифицирует). Оси независимы, выбор —
вопрос единообразия записи.

| Form | AST | Status |
|------|-----|--------|
| `unsafe T` | `Unsafe(T)` | ✅ |
| `ro unsafe T` | `Readonly(Unsafe(T))` | ✅ (safety-inner) |
| `mut unsafe T` | `Mut(Unsafe(T))` | ✅ |
| `unsafe ro T` | `Unsafe(Readonly(T))` | ❌ E_MODIFIER_ORDER |
| `unsafe mut T` | `Unsafe(Mut(T))` | ❌ E_MODIFIER_ORDER |
| `*ro unsafe T` | `Pointer(Readonly(Unsafe(T)))` | ✅ (pointee: ro outer, unsafe inner) |
| `*unsafe ro T` | `Pointer(Unsafe(Readonly(T)))` | ❌ E_MODIFIER_ORDER (pointee) |

> Note (Plan 138.5): prefix-формы (`ro * unsafe T` / `unsafe * ro T`) сами по себе
> теперь `E_POINTER_PREFIX_MODIFIER` (§1) — ordering-проверка применяется только к
> value-T и к **pointee** содержимому (постфикс после `*`).

**Detection:** parser KwRo/KwMut arms check `inner.contains_unsafe_in_chain()`
helper — recursive walk через Readonly/Mut wrappers (stopping at Pointer /
Named/Array/Tuple/Func boundaries). If found, emit E_MODIFIER_ORDER.

### §V3.3 — Right-binding propagation semantics (SUPERSEDED, Plan 138.5)

> **⚠️ SUPERSEDED (Plan 138.5):** §V3.3 описывал, как **outer** модификатор
> (перед `*`) семантически распространяется через Pointer на pointee. После
> запрета prefix перед `*` (`E_POINTER_PREFIX_MODIFIER`, §1) **outer-модификатора
> над Pointer не существует** → пропагировать нечего → правило **отозвано**.

> **⚠️ FINAL amend (Plan 147 D246, 2026-06-12; supersedes flip-scan-draft):**
> «никакого наследования нет» — **подтверждено**. Под три-осевой моделью (D246)
> bare `*T` = pointee **ro** (`*T ≡ *ro T`) во ВСЕХ позициях; pointee-mut НЕ
> наследуется от binding. Только `*mut T` пишется явно для mut-pointee; `*ro T` →
> `E_REDUNDANT_POINTER_RO`. Outer-prefix-пропагация остаётся отозванной (prefix-ban).

В FINAL-модели (D246) bare `*T` = pointee **ro** (`*T ≡ *ro T`); writable pointee —
**только** через явный `*mut T`:

```nova
mut p *T    // Pointer(T) ≡ Pointer(Readonly(T)) — pointee ro (D246; L1 mut ≠ mut-pointee)
ro p *T     // Pointer(T)                        — pointee ro; p фиксирован
*mut T      // Pointer(Mut(T)) — explicit mut pointee (единственный опт-ин на *p = …)
*ro T       // ❌ E_REDUNDANT_POINTER_RO (избыточно; fix-it *T)
*unsafe T   // Pointer(Unsafe(T)) — pointee possibly-uninit
```

Реассайнабельность указателя — binding (`let`/`mut`, D36), не пропагация типа.
Старые prefix-примеры (`ro * T`, `unsafe * T`, `unsafe * safe T`) — теперь
parse error (`E_POINTER_PREFIX_MODIFIER`). Helpers `contains_unsafe_in_chain` /
`contains_same_class_in_chain` остаются нужны лишь для **value-T** ordering
(§V3.2), не для pointer-пропагации.

### §V3.4 — `safe` keyword RETIRED + E_REDUNDANT_TYPE_MODIFIER (binding-level only)

> **⚠️ RETIRED (Plan 138.5):** `safe` модификатор был **propagation-stopper**
> для outer-`unsafe`, существовавшей только в prefix-форме `unsafe * safe T`.
> После запрета `unsafe *` (prefix, §1) outer-unsafe-пропагации нет → стопить
> нечего → `safe` **бесполезен** → **RETIRED**. usage в std = 0. Standalone
> `safe T` ≡ `T` и так был no-op. Lexer-токен `safe` в type-position — теперь
> ошибка (E_POINTER_PREFIX_MODIFIER family / E_SAFE_RETIRED, см. §V3.5);
> `safe_stoppers` / `is_safe_stopped_between` parser-машинерия — dead.

**E_REDUNDANT_TYPE_MODIFIER (FINAL scope):** V2 covered binding-level
(`ro x ro T`) — **KEPT**. V3 type-level prefix-chain extension
(`ro * ro T` / `unsafe * unsafe T`) опиралась на prefix-пропагацию — **moot**,
т.к. prefix перед `*` сам по себе `E_POINTER_PREFIX_MODIFIER` (§1).

| Form | Status |
|------|--------|
| `ro x ro T` — binding ro + duplicate type-level ro | ❌ E_REDUNDANT_TYPE_MODIFIER (binding-level, KEPT) |
| `ro T ro` — duplicate ro at same level | ❌ E_REDUNDANT_TYPE_MODIFIER |
| `*ro ro T` — duplicate pointee ro (postfix) | ❌ E_REDUNDANT_TYPE_MODIFIER (pointee chain) |
| `ro * ro T` (prefix) | ❌ E_POINTER_PREFIX_MODIFIER (prefix перед `*` — §1; не доходит до redundancy) |
| `unsafe * unsafe T` (prefix) | ❌ E_POINTER_PREFIX_MODIFIER (§1) |
| ~~`ro * safe ro T`~~ / ~~`unsafe * safe unsafe T`~~ | RETIRED — `safe` отозван |

**Detection** (parser/mod.rs, FINAL):
```rust
// Value-T / pointee chains only (no prefix-before-* allowed):
// In KwRo/KwMut/KwUnsafe arm after recursive parse_type:
//   if inner immediately repeats same modifier class → E_REDUNDANT_TYPE_MODIFIER
// `safe` stopper machinery removed (no propagation to stop).
```

### §V3.5 — New error codes registered

| Code | Description | Spec section |
|------|-------------|--------------|
| `E_MUTABILITY_CONFLICT_VALUE_TYPE` | ro+mut adjacency on value-type T | §V3.1 |
| `E_MODIFIER_ORDER` | `unsafe` wrapping `ro`/`mut` (safety-INNER rule, flipped Plan 138.5) | §V3.2 |
| `E_REDUNDANT_TYPE_MODIFIER` | same-class modifier repetition (binding-level + value-T/pointee chains) | §V3.4 |
| `E_POINTER_PREFIX_MODIFIER` | `ro`/`mut`/`unsafe` token перед `*` в type-position (Plan 138.5; extends `E_INVALID_POINTER_MODIFIER`) | §1 |

> **`safe` модификатор RETIRED (Plan 138.5):** `safe T` в type-position больше не
> валиден (был V3.4 propagation-stopper; пропагации нет). Parser трактует `safe`
> перед `*`/типом как `E_POINTER_PREFIX_MODIFIER` family (или dedicated
> `E_SAFE_RETIRED` — выбор enforce-фазы Ф.2). `safe_stoppers` machinery dead.

E_PARAM_MOD_CONFLICT preserved для дисциплинирующих случаев:
- `mut consume name T` / `consume mut name T` (D131 conflict)
- `mut readonly name T` (legacy form)

**§V3.1 amend (2026-06-05):** E_PARAM_MOD_CONFLICT **LIFTED** для
`ro x mut T` (pre-name ro + post-name mut) — orthogonal binding modifiers
per binding-context relaxation. Symmetric `mut x ro T` уже работал
(pre-name mut + post-name unhandled — falls to type-level Readonly(T)).

### §V3.6 — Migration impact (V2 → V3)

**Low breakage** per discovery audit:

- `nova_tests/plan108_1/readonly_mut_conflict_neg.nv` + `mut_readonly_conflict_neg.nv`
  — already NEG tests; keep expected error code OR migrate к new
  `E_MUTABILITY_CONFLICT_VALUE_TYPE` (binding-level distinction preserved
  per §V3.5)
- `nova_tests/plan118/t1_9_chain_modifiers_ok.nv:15` — `*ro mut Acc`
  (postfix pointee chain) — Acc context-dependent. Plan 138.5 Ф.2: rewrite
  к value-record для NEG demonstration, OR drop the redundant modifier.
- `nova_tests/plan118_5/*` + `plan118_5_v3/*` — **built on prefix forms +
  `safe`-stopper** (retired). Plan 138.5 Ф.2: convert к NEG fixtures
  (`mut * T` / `safe T` → `E_POINTER_PREFIX_MODIFIER` / `E_SAFE_RETIRED`)
  OR delete/rewrite к postfix-pointee equivalents.
- stdlib `std/runtime/raw_mem.nv` — uses `*u8` and prefix `mut * u8` →
  migrate prefix к postfix `*mut u8` (Plan 138.5 Ф.2). bare `*u8` unchanged.

### §V3.7 — Followup markers opened

- `[M-118.5-V3-safe-keyword-impl]` — **RETIRED by Plan 138.5** (`safe`
  модификатор отозван; пропагации нет — стоппер бесполезен).
- `[M-118.5-V3-ro-mut-storage-class]` — Ф.2 type-checker storage-class
  detection + check (§V3.1 value-T — KEPT)
- `[M-118.5-V3-modifier-order]` — §V3.2 ordering (FLIPPED к safety-inner,
  Plan 138.5) — value-T / pointee only
- `[M-118.5-V3-redundant-extension]` — E_REDUNDANT binding-level + pointee
  chains (prefix-chain part moot — `safe` escape retired)
- ✅ `[M-118.5-V3-binding-context-relaxation]` — **CLOSED 2026-06-05** —
  binding-form `ro x mut T` (и симметричное `mut x ro T`) allowed
  regardless of T storage class. Parser E_PARAM_MOD_CONFLICT lifted для
  pre-name ro + post-name mut combo. Type-form §V3.1 storage-class check
  unchanged.
- `[M-138.5-pointer-prefix-enforce]` — Ф.2 parser/checker enforce
  `E_POINTER_PREFIX_MODIFIER` + retire `safe`/`Unsafe(Pointer)` + migrate
  prefix usages (Plan 138.5).

### §10a rename — `unsafe` type-modifier → `uninit` (Plan 174.5, 2026-07-11)

> **Status:** ✅ **DONE 2026-07-11** (sonnet, worktree `nova-nt` branch
> `uninit-rename-d216`). Closes the `[M-174.5-pointer-ops-methods]` Ф.0
> rename-sweep followup («Rename `unsafe T`→`uninit T`»).

**Что переименовано.** Слово `unsafe` было перегружено: (1) **type-модификатор**
`*unsafe T` (pointer к possibly-uninit T) и value-wrapper `unsafe T`
(MaybeUninit-style, §V2.3/§V3 выше); (2) **блок** `unsafe { }` и **fn-атрибут**
`unsafe fn`/`external unsafe fn` (D2 amend, §8). Это амендирует **только (1)**:

- `*unsafe T` → **`*uninit T`** (pointer к possibly-uninit T, postfix pointee)
- `unsafe T` (value-wrapper) → **`uninit T`**
- Все производные формы выше в этом D-блоке (`mut x uninit T`,
  `Option[*uninit T]`, `*mut uninit T`, chain-примеры, таблицы §1/§11a/§12/
  §V2/§V3) — та же замена.

**Что НЕ переименовано (сохраняет `unsafe`):**

- `unsafe { ... }` блок (D2 amend, §8) — без изменений.
- `unsafe fn` / `external unsafe fn` declaration-атрибут (Plan 118.1.7,
  `FnDecl.unsafe_attr`) — без изменений.
- **`*unsafe fn(...)` / `*extern "C" unsafe fn(...)`** fn-pointer-type
  композиция (§10 «unsafe fn as part of fn-ptr type») — **сознательно НЕ
  переименована**. Хотя структурно это тот же AST-wrapper (`Pointer(Uninit(Func))`,
  внутреннее имя варианта переименовано в `Uninit` для единообразия), семантика
  ортогональна possibly-uninit data: это «указатель на fn, вызов которой требует
  unsafe» (mirrors `unsafe fn` call-site enforcement,
  `E_UNSAFE_CALL_REQUIRES_WRAP`/`E_UNSAFE_FN_PTR_COERCION`), а не «данные могут
  быть не инициализированы». Парсер различает по payload: `unsafe` легален в
  type-позиции ТОЛЬКО когда обёрнутый тип — `Func` (постфикс `*unsafe fn(...)`
  или bare `unsafe fn(...)`); для любого другого (data) типа — hard error.

**Цель.** Развязать «possibly-uninit pointee/value» (теперь `uninit`) от
«unsafe-операции» (блок/fn-атрибут/fn-ptr-композиция остаются `unsafe`) —
два независимых понятия больше не делят одно слово в type-позиции.

**Миграция.** `std/`, `spec_tests/conformance/` — **0 вхождений** type-модификатора
`*unsafe T`/`unsafe T` на момент рена́йма (грепом подтверждено); миграции кода
не потребовалось. `nova_tests/` (не гейт корректности) содержит старые
`*unsafe T`-фикстуры (plan118/plan118_5*) — не мигрированы этим заходом (вне
gate, см. `feedback-nova-tests-not-correctness-gate`).

**Hard error.** `unsafe` в type-позиции, обёртывающий НЕ-`Func` (т.е. старая
data-uninit форма) → **`E_UNSAFE_TYPE_MODIFIER_RENAMED`** с подсказкой
использовать `uninit`. Neg-тесты:
`spec_tests/conformance/neg/d216_unsafe_type_modifier_renamed_neg.nv` (bare
value-wrapper форма) и `d216_unsafe_ptr_modifier_renamed_neg.nv` (pointer
форма). Pos-тест (uninit T / *uninit T / legacy `*unsafe fn(...)` compose):
`spec_tests/conformance/d216_uninit_rename_174_5.nv`.

**Реализация (компилятор):** `TokenKind::KwUninit` (новый keyword `uninit`,
`lexer/mod.rs`); `TypeRef::Unsafe` → `TypeRef::Uninit` (AST variant rename,
`ast/mod.rs`) + `PointerModifier::Unsafe` → `PointerModifier::Uninit`
(`ast/mod.rs`, Ty-level tag, `types/mod.rs`); parser `parse_type` — новый
`KwUninit`-arm (generic, любой T) + сужение существующего `KwUnsafe`-arm до
ТОЛЬКО `Func`-payload (иначе `E_UNSAFE_TYPE_MODIFIER_RENAMED`), обе арки
production `TypeRef::Uninit` (`parser/mod.rs`); display/render-функции
(`types/mod.rs` ×4, `emit_c.rs`, `doc/collector.rs`, `doc/render_json.rs`,
`nova-lsp/src/symbol.rs`) — Func-conditional keyword при печати диагностик
(`unsafe` если payload = `Func`, иначе `uninit`). Editor-хайлайтеры (VSCode
tmLanguage, vim syntax, Zed scm-comment, `syntax_highlight_conformance.rs`
ACTIVE-список) — `uninit` добавлен рядом с `unsafe` (D278).

### D52 amend (Plan 124.8 Ф.2)

7-я форма declaration: **value record** — `type X value { ... }`. 
Stack-allocated reference type с copy-on-pass semantics (D32 amend). 
Composable с `consume`/`priv` модификаторами в каноническом порядке
`value consume priv` (см. amend ниже — order-independence RETIRED).

`value` — **contextual keyword** (recognized только в `type Name[Generics]
[modifiers] value [modifiers] {` position; identifier `value` остаётся
валидным во всех других позициях для backward compat).

Canonical modifier order: `type X value consume priv { ... }` —
allocation → ownership → visibility (outer → inner).

> **AMEND 2026-06-12 (Plan 148 Ф.1 / D241, [M-138-canonical-modifier-order]):**
> Parser больше **НЕ** order-independent — out-of-canon порядок модификаторов
> теперь hard error `E_MODIFIER_ORDER` (с machine-applicable fix-it),
> а не отложенный lint `W_NON_CANONICAL_TYPE_MODIFIER_ORDER`. Полное правило
> (canonical ranks, обобщение на новые модификаторы) — D241 в
> [03-syntax.md](03-syntax.md#d241-канонический-порядок-модификаторов-type-декларации-scope-adjacency).

### D175 amend §«binding dominates» (Plan 124.8 Ф.3)

`ro acc` binding теперь блокирует write к любому полю объекта, даже
если поле помечено `mut field T`. Rust-style правило.

| Field declaration | `ro acc` binding | `mut acc` binding |
|---|---|---|
| `field T` | ❌ переприсвоить, ❌ мутировать | ✅ / ✅ |
| `ro field T` | ❌ / ❌ | ❌ / ❌ (always frozen) |
| `field ro T` | ❌ / ❌ | ✅ / ❌ |
| `mut field T` | **❌ / ❌** (was ✅ / ❌ before amend) | ✅ / ✅ |
| `mut field ro T` | **❌ / ❌** (was ✅ / ❌ before amend) | ✅ / ❌ |

Changed: `mut field` теперь НЕ "always-mutable" — binding dominates.

### D175 amend §V2 «binding dominates — explanatory consolidation» (2026-06-04)

> **Rationale:** Clarifying amend без semantic change. Consolidates rules
> разбросанные между D33 amend, D36, D175 amend, D176 V1, Plan 108.1-108.3 в
> единое explanatory section. Никаких behavior changes — все existing rules
> stay valid.

> **✅ KEEP = L2-ось (Plan 147 D246, 2026-06-12):** этот §V2 «binding dominates /
> access-time enforcement» — **в точности L2 view-семантика** (транзитивный
> ro/rw freeze по owned-графу значения, access-time). Под три-осевой моделью
> (D246) **сохраняется без изменений** + добавлено уточнение **P4: L2 freeze
> ОСТАНАВЛИВАЕТСЯ на каждом `*`** (см. ниже §«L2 wall-at-*»). За указателем
> транзитивный freeze не действует — там работает только L3 (pointee-capability
> из типа). L1 (reassignability имени) и L3 (pointee-mut) — отдельные оси.

#### Принцип: **binding dominates → access-time enforcement**

Mutability при доступе к value (поля, индексы, mut-методы) определяется
**комбинацией двух факторов**, в порядке приоритета:

1. **Binding mutability** (call-site decision) — DOMINATES
2. **Type / field declaration** (definition-site intent) — refines what
   binding permits

Никакой **transitive type-modifier propagation** в spec НЕТ. Вместо неё —
runtime/check-time enforcement: каждое `acc.field`/`arr[i]` access валидируется
по обоим уровням. Это эквивалентно Rust `&T` vs `&mut T` philosophy — modifier
живёт на binding/reference, не propagates через type structure.

#### Полная таблица combination (5 axes × 2 binding modes)

| Field declaration | Access под `ro acc` | Access под `mut acc` | Reasoning |
|-------------------|---------------------|----------------------|-----------|
| `field T` (default) | ❌ read-only, ❌ mutate | ✅ read, ✅ mutate | binding dominates ro; mut binding allows default |
| `ro field T` | ❌ / ❌ | ❌ / ❌ (always frozen) | type-author intent enforced regardless |
| `mut field T` | ❌ / ❌ (dominates!) | ✅ / ✅ | binding ro blocks even «explicit mut» field |
| `field ro T` | ❌ / ❌ | ✅ reassign, ❌ content | content modifier independent of binding |
| `mut field ro T` | ❌ / ❌ | ✅ reassign, ❌ content | same: content ro is invariant |

**Ключевая asymmetry:** `ro acc` **dominates** ВСЕ field declarations
(everything frozen). `mut acc` **respects** field declarations (ro field stays
ro, mut/default field becomes mut).

#### L2 wall-at-`*` (Plan 147 D246 P4 — freeze STOPS at every pointer)

Транзитивный freeze L2 идёт **только по owned-графу значения** (`.field` /
`[i]` — поля value/heap-record, элементы массива). Он **упирается в стену на
КАЖДОМ `*`**: за указателем possibility-to-write определяется **исключительно
L3** (pointee-capability из типа `*T`=ro / `*mut T`=mut), а НЕ L2-binding.
Причина: owned-vs-aliased heap статически неразличим (нет borrow-checker), а GC
допускает shared-mut под чужим `ro` — поэтому `ro` = per-path write-ban, не
object-freeze (D246 P10). Deep-immutable сквозь `*mut` снаружи **не навязывается**
(D246 P9, C++ shallow-const trade-off); deep-ro → **производитель** объявляет
поле `*T` (как `str { ptr *u8 }`).

```nova
type Cell { mut v *mut int }      // поле — mut-pointee (L3)

// 1. owned-граф: L2 freeze работает транзитивно (стоп НЕ на *, а на каждом .field)
type Tags { mut items []str }
type Account { mut tags Tags }
ro acc Account = ...
acc.tags.items.push("x")          // ❌ E_READONLY_FIELD — L2 freeze транзитивен (нет *)

// 2. за указателем: L2 freeze ОСТАНАВЛИВАЕТСЯ, действует L3 из типа
ro c Cell = Cell{ v: p }          // ro-binding морозит owned-граф c
c.v = q                           // ❌ — reassign поля .v (owned-граф, до стены) заблокирован L2
unsafe { *c.v = 7 }               // ✅ — за `*` L2 не действует; pointee mut (L3 = *mut int)
                                  //     запись разрешена ИМЕННО потому, что поле объявлено *mut int

// 3. если производитель хочет deep-ro — объявляет поле *T (ro-pointee)
type RoCell { v *int }            // *int = ro-pointee (L3)
ro r RoCell = RoCell{ v: p }
unsafe { *r.v = 7 }               // ❌ E_POINTER_RO_ASSIGN — L3 pointee ro (из типа)
```

#### Почему НЕТ transitive type-modifier inflation

User's mental model «`fn f(b []str)` == `fn f(ro b ro [] ro str)` (полная
inflation всех ro on each nesting level)» — **НЕ работает** в Nova spec.
Reasons:

1. **D33 amend** explicit: `ro x ro T` → `E_REDUNDANT_TYPE_MODIFIER`. Single
   `ro` on binding **is enough** — propagation handled access-time.
2. **Implementation simplicity:** access-time enforcement requires single
   binding-modifier check; transitive inflation would require type-level
   modifier propagation through all nested wrappers.
3. **Rust precedent:** `&T` doesn't inflate to `&&T` for nested fields — same
   logic.

#### `let` keyword retracted — нет «neutral binding»

Plan 114 D184 retracted `let` keyword. **No third binding mode** — binary
`ro` / `mut` choice only. Rationale:

- 2-state model symmetric с Plan 108.x default-ro rule
- 3-state (`let` neutral / `ro` frozen / `mut` writable) ambiguous для
  default-prefix fields — would require defining «default field under let»
  semantic
- Call-site explicit choice (`ro` or `mut`) **dominates** type author's
  per-field intent — call-site has full safety knowledge

If user wants «trust type author's per-field declarations»:
- Use `mut acc` binding — ro fields stay ro, mut/default fields mut
- This is **already** what type author's intent maps к under `mut` binding

Hypothetical neutral `let acc = X{...}` mode adds no expressiveness —
either синоним `ro acc` (least permissive) or `mut acc` (respects field
intent). Rejected to keep binding-modifier landscape minimal.

#### Function return type interaction

Return type modifier (Plan 114 D184 default = mut, explicit ro allowed)
participates в same rules:

```nova
// 1. Return default mut Acc, ro binding dominates
fn make_acc() -> Acc => Acc{...}
ro b = make_acc()
b.name = "x"           // ❌ E_LOCAL_NOT_MUT — binding dominates

// 2. Return default mut Acc, mut binding allows full access
mut c = make_acc()
c.name = "x"           // ✅

// 3. Explicit ro return, mut binding without type annotation
fn make_ro_acc() -> ro Acc => Acc{...}
mut c = make_ro_acc()  // type inferred as `ro Acc`
                       // → D33 amend row «mut x ro T»: binding mut,
                       // content ro — split semantics
c = different_acc      // ✅ reassign OK (binding mut)
c.name = "x"           // ❌ E_READONLY_CONTENT (content ro)

// 4. Explicit ro return + explicit mut type annotation = coerce error
mut c Acc = make_ro_acc()    // ❌ E_READONLY_COERCE
                              // ro Acc → mut Acc forbidden (D176)
```

#### Cross-refs

- D33 amend (binding propagation) — [02-types.md:8927](#d33-amend-binding-propagation-plan-1248-ф2)
- D36 / Plan 108.2 enforcement — [02-types.md:2684](#enforcement-plan-1082-2026-05-30)
- D175 V1 amend (this section, just above)
- D176 (ro T modifier + Plan 108.1 param default flip) — [02-types.md:2763](#d176-ro-t--тип-модификатор)
- D184 (Plan 114 — `let` keyword retracted) — [03-syntax.md#d184](03-syntax.md#d184)
- D216 V2 (right-binding rule + universal type modifiers) — [02-types.md:7790](#d216-v2-amend-2026-06-04--universal-right-binding-rule-для-type-level-modifiers--unsafe-t-first-class)

#### Status

✅ **ACTIVE since 2026-06-04** — explanatory consolidation, no behavior change.
Implementation across D33 / D36 / D175 V1 / D176 / Plan 108.1-108.3 / Plan 114
unchanged. This amend documents existing rules в единую читаемую секцию.

### D176 amend §«mut T в binding position» (Plan 124.8 Ф.2)

`mut T` теперь принимается в binding type annotation после name.
Раньше parser принимал только в return-type и parameter positions.
Pre-amend rendered impossible the legitimate `ro view mut []u8 = arr`
form documented в D176 V1 §«type-modifier в любой позиции».

### D215 amend (Plan 124.8 Ф.1)

Named tuples (`type X(name1 T1, name2 T2)`) получают:
1. **Multi-line support** — newlines между fields после comma.
2. **Trailing comma support** — `type X(a int, b int,)`.
3. **Binding-level mutability** — Rust-style: `mut p = Vec3(...)`
   позволяет мутировать все поля; `ro p` блокирует все. Per-field
   `mut`/`ro` modifiers запрещены (`E_TUPLE_NO_PER_FIELD_MOD`).

Asymmetry с record `{}` form (which supports newline-as-separator)
preserved: tuples требуют comma + optional newline после. Это
intentional — tuples = compact pure-data form.

### D215 amend — record `{}` same-line comma enforcement (2026-06-15)

Record `{}` fields тоже поддерживали только «newline-as-separator» по D49, но при
fields на **одной строке** без запятой (`type P value { x int y int }`) парсер
молча принимал оба поля — баг (обе ветки if/else в `parse_record_fields_with_default`
делали одинаковый `skip_newlines()`). Уточнение:

- **Newline** — допустимый разделитель (multi-line record).
- **Comma** — допустимый разделитель (inline или multi-line).
- **Ни того ни другого** (next token — не Newline/Semicolon/RBrace) → **`E_RECORD_FIELD_MISSING_SEPARATOR`**.

Иначе говоря: на одной строке запятая **обязательна**, как и в named-tuple `()`.
Это унифицирует поведение обеих форм и закрывает парсер-баг.
Применяется к обоим видам record (heap `type X {}` и value `type X value {}`).

### D215 amend — named tuple field defaults (2026-06-17)

Поля named tuple могут иметь **значение по умолчанию** (default value):

```nova
type Complex(re f64 = 0.0, im f64 = 0.0)
type Rect(x f64, y f64, width f64 = 100.0, height f64 = 50.0)
const DEFAULT_SCALE f64 = 1.0
type Transform(tx f64 = 0.0, ty f64 = 0.0, scale f64 = DEFAULT_SCALE)
```

Конструктор может опускать любое подмножество полей с дефолтами:

```nova
ro z  = Complex()               // re=0.0, im=0.0
ro z2 = Complex(re: 3.0)        // im=0.0
ro r  = Rect(x: 1.0, y: 2.0)   // width=100.0, height=50.0
ro t  = Transform(scale: 2.5)   // tx=0.0, ty=0.0
```

#### Grammar (extends D215 original)

```ebnf
named_field  ::= IDENT type ("=" expr)?
```

До этого amend `named_field ::= IDENT type` — без optional default. Совместимость с существующим кодом: дефолты additive.

#### Семантика

- **Required field** (без `= expr`) — обязателен в каждом вызове конструктора.
- **Optional field** (с `= expr`) — можно опустить; absent → default expression
  инжектируется на call-site в declaration order.
- Порядок полей в declaration не ограничен: required и optional могут чередоваться
  (хотя рекомендуется ставить optional в конце).
- Default expression вычисляется **на call-site** (не хранится и не кэшируется);
  может ссылаться на module-level constants, literals, fn-calls.

#### Arity check (amend E_TUPLE_CONSTRUCT_ARITY_MISMATCH)

Проверка min/max arity вместо точного совпадения:

- `min_arity` = количество required-полей (без дефолта)
- `max_arity` = общее количество полей
- Provided-field count ∉ [min_arity, max_arity] → `E_TUPLE_CONSTRUCT_ARITY_MISMATCH`

#### AST (compiler-codegen/src/ast/mod.rs)

```rust
pub struct NamedTupleField {
    pub name: Ident,
    pub ty:   TypeRef,
    pub default: Option<Box<Expr>>,   // NEW
}
```

#### Checker (types/mod.rs)

`named_tuple_field_defaults: HashMap<String, Vec<(String, Expr)>>` —
ключ = bare type name, value = список `(field_name, default_expr)`.
Заполняется в step 1 (type-decl scan).
При вызове конструктора — missing optional полей → default expr инжектируется
в reconstructed arg list.

#### Codegen (emit_c.rs)

Default expressions инжектируются на call-site в `emit_tuple_construct`.
C-struct initializer включает все поля в declaration order.

#### Acceptance criteria

| # | Критерий | Status |
|---|---|---|
| AC-1 | `type X(f T = expr)` принимается парсером | ✅ |
| AC-2 | Mixed required+optional fields в одном типе | ✅ |
| AC-3 | `X()` when all fields have defaults | ✅ |
| AC-4 | Partial override `X(a: v)` — remaining defaults injected | ✅ |
| AC-5 | Missing required field → E_TUPLE_CONSTRUCT_ARITY_MISMATCH | ✅ |
| AC-6 | Default references module-level constant | ✅ |
| AC-7 | `std/_experimental/math/complex.nv` migrated to named tuple | ✅ |
| AC-8 | «без упрощений как для прода» — production-grade, no stubs | ✅ |
| AC-9 | plan120 12/12 PASS (t4_defaults×8, t5_defaults_methods×4, neg_t4×1, neg_t5×1) | ✅ |

Реализация: [Plan 120 D215-amend](../../docs/plans/120-named-tuples-and-allocation-contract.md#d215-amend--named-tuple-field-defaults-2026-06-17).

### D222 amend (Plan 124.8 Ф.1)

«Named tuple priv» portion **retract**: `priv`/`pub` на tuple field —
parser-error `E_TUPLE_NO_PRIV`. Tuples = pure data carriers (как Rust
tuples, C# ValueTuple), always all-public. Encapsulation на стеке —
через `type X value { priv field T }` form (D228 NEW).

«Protocol impl boundary» portion preserved для records (heap + value).

### D225 retract (Plan 124.8 Ф.1)

«Type-level priv flip для named tuples» — fully retracted. Tuples
всегда all-public; `type X priv (...)` syntax больше НЕ supported.
Records keep type-level priv flip (D220 §3.3.1 unaffected).

### D228 NEW — Value-record allocation contract (Plan 124.8 Ф.2/Ф.4)

> **Extended by [D277](#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2)** (Plan 153.2, 2026-06-15): by-value
> стек-codegen распространён с не-generic value-records на **generic**
> `type X[T] value {…}` — каждый mono-инстанс = inline `NovaValue_<short>`,
> passed/returned/copied by value, 0 `nova_alloc` для wrapper (зеркаля str-путь).
>
> Renumbered from D226 (2026-06-03) — D226 in main concurrently assigned
> to «signed indexing convention» commit `8827f8ec132`. D227 taken by
> «numeric literal inference» commit `41d4be096fa`. D228 next free.
>
> **Optimizer consumer (Plan 123 V7.6 V2 refactor, 2026-06-05):**
> field-cache `is_reference_type_ref` classifier consults `AllocKind`
> via `TypeKindRegistry`. `Record(AllocKind::Heap)` → ref-typed slot
> (pointer); `Record(AllocKind::Value)` → inline slot (D228 — mut
> methods write slot bits через `NovaValue_X*` pointer per §«Method
> receiver» above) → not slot-stable for V7.5/V7.7 own-field cache
> invalidation refinement. See `docs/plans/123-followups-2026-06-05.md`
> §2.1 для design + acceptance.

`type X value { ... }` — stack-allocated value type с copy-on-pass
semantics. Symmetric extension D52 §«record form» через `value` keyword.

**Semantic (V2 production-grade, landed 2026-06-03):**
- **Allocation:** stack (inline C struct `NovaValue_X` в callee frame).
  V2 codegen landed in Plan 124.8 V2.1-V2.4 — closes [M-124.8-value-codegen-stack].
- **Pass:** copy on parameter pass (D32 amend) — C handles natively
  для value types.
- **Method receiver `@`:** pointer на stack-slot (`NovaValue_X*`) —
  мутации видны caller'у. См. [D215 amend «Method receiver passing»](#d215-named-tuple-fields--valuereference-allocation-contract)
  (Plan 128 Ф.2) — NamedTuple uses the same pointer pattern (`NovaTuple_X*`
  для mut receiver), wired через `recv.mutable` flag в `emit_c.rs`.
  Same lvalue-projection rule applies to NovaValue_X mut-receivers (D228)
  and NovaTuple_X mut-receivers (D215) — Plan 128.1 Ф.1: `b.v.method()`,
  `arr[i].method()`, `@field.method()`, multi-level `a.b.c.method()`
  emit `&(b->v)` / `&(arr->data[i])` / `&(nova_self->field)` directly
  без temp hoist (mutation flows к original slot).
- **Reference fields:** handles inline (ptr+len+cap для `[]T`,
  ptr+len для `str`); data on heap (GC-tracked).
- **`str` — канонический reference-field value-record (Plan 139, 2026-06-11).**
  `type str value priv { ptr *u8, len int }` — 16-байт stack-значение,
  inline handle (`ptr+len`) над иммутабельным heap/rodata UTF-8 буфером.
  Это flagship-пример паттерна «value-record несёт shared-immutable
  reference-поле»: copy-семантика значения (16 байт), но буфер разделяется
  через `*u8` ro-pointee (`*T ≡ *ro T`, D246 — нет write-path → sharing
  безопасен → clone shallow,
  literal-interning невидим). Все остальные value-record-правила (D228) к
  `str` применяются единообразно; единственный opt-out — content-eq (ниже),
  потому что field-by-field над reference-полем сравнил бы pointer-identity.
  См. [D26 MAJOR AMEND](08-runtime.md#d26-базовая-stdlib-и-prelude).
- **Equality of reference-field value-records (Plan 139 Ф.3, content-eq
  override).** Default value-record `==` is **field-by-field** (Plan 141:
  `emit_field_eq` recurses each field). For a reference field whose pointee
  is **shared + immutable** (the `str.ptr *u8` ro-pointee buffer), naive field-by-field
  would compare **pointer identity** — WRONG: two distinct buffers with equal
  bytes must be equal. Therefore `str` **opts out** of field-by-field and uses
  **content-eq**: `emit_field_eq` special-cases `cty == "nova_str"` →
  `nova_str_eq` (`len && memcmp`) **before** any field-by-field path
  (emit_c.rs:11161), and direct `==`/`<`/… on `str` lower in BinOp codegen to
  `nova_str_eq`/`nova_str_lt`/… (emit_c.rs:16985). Consequence: str-in-tuple,
  str-in-record, str-in-sum eq, and str-keyed `HashMap` (hash via
  `nova_str_hash` SipHash-over-bytes) are **all content-keyed automatically**.
  `str.@clone` = 16-byte handle copy over the immutable shared buffer (no deep
  copy; `*u8` ro-pointee makes sharing safe). General rule: a value-record carrying a
  shared-immutable pointer field must register content-eq for that field rather
  than inherit pointer-identity from the field-by-field default.
- **Fixed array fields:** fully inline (`[32]u8` = 32 bytes inline).
- **Forward decls:** `typedef struct NovaValue_X NovaValue_X;` (no
  pointer alias unlike heap records).
- **Constructor:** `NovaValue_X tmp; tmp.f1 = v1; tmp.f2 = v2;` (stack
  init, no `nova_alloc`).
- **Field access:** `.field` (struct member), not `->field` (pointer).
- **Call-site receiver:** `&v` for identifier; hoisted temp + `&temp`
  для rvalue expressions (via `prepare_method_recv` helper).

**Codegen V2 helpers (compiler-codegen/src/codegen/emit_c.rs):**
- `emit_value_record_type(name, fields)` — emits inline C struct +
  registers in record_schemas + type_aliases + value_record_names.
- `prepare_method_recv(obj_c, obj_ty)` — wraps obj в `&` for value-record
  receivers (identifier-fast-path или temp-hoist для expressions).
- `is_value_type` recognizes `NovaValue_` prefix.
- `struct_name_from_c_type` recognizes `NovaValue_X` strip.

**Composability:**
- `value` + `priv` — composable (D220 §3.3.1 для type-level flip также применима).
- `value` + `consume` — composable; value-record содержащий consume
  field автоматически становится consume (user decision; orthogonal axes).
- `value` + `mut`/`ro` per-field — composable (D175 binding-dominates rule applies).

**Composition с эталонами:**
- Kotlin `value class` (1.5+, single-field) — Nova value-record более powerful (multi-field).
- Java Valhalla `value class` (incoming) — Nova alignment ahead-of-curve.
- Rust `struct` (default stack) — Nova explicit allocation marker (vs Rust implicit).
- C# `struct` vs `class` — Nova `value` modifier ≈ C# `struct`; reference record default ≈ C# `class`.
- Industry: Nova становится **первым языком с single declaration syntax
  + explicit allocation modifier**.

**V2 known limitations (defer-able):**
- `[]NovaValue_X` array storage — currently boxes elements (V3 followup
  for inline element storage).
- ~~Escape analysis для `&value` auto-heap-promote~~ — **CLOSED by
  Plan 127 (V1, 2026-06-05)** — см. §«escape & auto-promote» ниже.
- Auto-derive methods (Equal / Hash / Clone / Compare /
  Display) — Plan 126 (orthogonal feature).
- Generic value-record cross-module instantiation — works for simple
  cases; complex multi-T patterns may require V3 review.
- `#zero_on_move` opt-in — followup attribute для security-critical
  consume value-records.

### D228 amend — §«escape & auto-promote» (Plan 127, 2026-06-05)

> **Trigger:** closes Plan 124.8 V2 followup `[M-124.8-value-heap-promote]`.
> Extends Plan 118 Ф.2 escape walker на value-record locals.
> Plan 127 phases landed (branch `plan-127-value-record-escape`):
> Ф.1 AllocKind tri-state — `40815f7d960`;
> Ф.2 escape_analyze walker extension — `6ce9d2a4698`;
> Ф.3 codegen heap-allocation path — `6948d2ba9dc`;
> Ф.4 diagnostic codes — `0a0d7e2cf65`;
> Ф.5 fixtures (18 = 12 POS + 6 NEG) — `adb6850e7e0`.

`&v` на value-record local разрешён. Compiler выбирает stack vs heap
allocation для `v` based on escape analysis result (Go-style — без
Rust lifetimes, без borrow checker).

#### AllocKind tri-state

`AllocKind` enum расширен с binary `{Heap, Value}` до tri-state:

| Variant | C output | Когда |
|---|---|---|
| `AllocKind::Heap` | `Nova_X*` (heap, `nova_alloc`) | reference records (`type X { ... }`) |
| `AllocKind::Value` | `NovaValue_X` (stack-inline) | value records без escape |
| `AllocKind::ValueHeapPromoted` | `Nova_X*` (heap, `nova_alloc`) | value records с detected escape |

User-visible type — Vec3 (или `*Vec3` для address-of), та же declaration
`type Vec3 value { ... }`. Diff виден только в codegen (`emit_c.rs`).
`prepare_method_recv` helper расширен: `ValueHeapPromoted` → obj уже
указатель, не нужен `&` (vs `Value` где emit'ится `&v`).

#### 5 escape trigger conditions (V1 OVER-promote)

Local value-record `v` promote'ится на heap если **любое** из:

1. **Return:** `&v` возвращается из функции — `fn f() -> *Vec3 => &v`.
2. **Heap field store:** `&v` сохраняется в heap-аллоцированное поле —
   `acc.field = &v` (где `acc: Nova_X*`).
3. **Closure capture:** `&v` захватывается closure — `let cb = || &v`.
4. **Global / module binding:** `&v` сохраняется в module-level
   `let`/`const` — escape вне fn scope.
5. **Fn arg sink (conservative):** `&v` передаётся в fn arg —
   conservative assume sink escape transit (V1 OVER-promote: callee
   analysis не делается, любой `&v → fn` triggerит promote).

**Conservative fallback (V1):** если chain dataflow analysis cannot
prove «no escape», promote. Matches Plan 118 Ф.2 V1 OVER-promote stance —
любая uncertainty → heap. Precise mode = followup `[M-127-precise-escape]`
(gated на Plan 118 `[M-118-escape-precise]`).

Mixed branch (escape в одной ветке, no-escape в другой) — conservative
promote. Path-sensitive analysis = `[M-127-path-sensitive-escape]`.

#### Cross-ref D228 ↔ D216 §4 (Plan 118 escape machinery reuse)

D228 escape rules **используют ту же walker infrastructure**, что и
D216 §4 «`&value` operator + escape analysis с auto-promote» (Plan 118
primitives + named tuples). Контракт reuse:

- `escape_analyze` walker (`compiler-codegen/src/types/mod.rs`, Plan
  118 Ф.2) — единая dataflow walker, value-record locals добавлены как
  новая type category поверх существующих primitives/tuples.
- Trigger conditions унифицированы — same 5 conditions работают
  identically для primitives (`int`), tuples (`(a, b)`), и value-records
  (`Vec3 value`). Different type category, same analysis.
- AllocKind decision route — для primitives/tuples: stack vs heap-box;
  для value-records: `Value` vs `ValueHeapPromoted`. Decision point
  shared, codegen branches diverge.
- V1 OVER-promote stance shared — Plan 118 и Plan 127 promote on ANY
  uncertainty. Precise mode landings будут coordinated (V2 followups
  обоих планов могут landed independently).
- `E_AMP_RECORD_LITERAL` (D216 §4) применяется и к value-record
  literals — anonymous `&Vec3 { ... }` без named binding forbidden,
  требует pattern `ro v = Vec3 { ... }; ro p = &v`.

См. также [D216 §4](#4-value-operator--escape-analysis-с-auto-promote)
для primitive/tuple side контракта.

#### Diagnostic codes (Plan 127 Ф.4)

| Code | Kind | Trigger |
|---|---|---|
| `W_VALUE_RECORD_UNNECESSARY_PROMOTE` | Lint | Escape detected, но user мог return by-value; suggestion hint emit |
| `E_VALUE_RECORD_ESCAPE_AFTER_CONSUME` | Error | `&v` после `consume v` — D162 violation, escape после ownership transfer |

`W_VALUE_RECORD_UNNECESSARY_PROMOTE` **suppressed на synthesized
FnDecl bodies** (Plan 126 auto-derive coordination — `compiler_generated`
flag в FnDecl). Auto-derived `Clone::clone`/`Equal::equal`/
`Hash::hash` bodies могут эмитить `&self` без необходимости user
attention; lint бы создавал noise. Suppression channel прописан в
`compiler-check/src/lints/value_record_promote.rs` — skip emit, если
`fn.compiler_generated == true`.

#### Composability (D228 amend)

- `consume` value-record: `&v` после `consume v` → hard error
  `E_VALUE_RECORD_ESCAPE_AFTER_CONSUME`. До consume — escape analysis
  как обычно.
- `priv` field: на promoted value-record (`Nova_X*`) field privacy
  preserved (Nova_X имеет те же priv markers, что и NovaValue_X).
- `ro` binding: `ro` binding propagates через promote — `Nova_X*
  const` в C output для ro path.

#### Method receiver compatibility

`@`-методы работают идентично в обоих modes via `prepare_method_recv`:

```rust
fn prepare_method_recv(obj_expr, alloc_kind) -> CExpr {
    match alloc_kind {
        AllocKind::Value => emit_address_of(obj_expr),     // &v
        AllocKind::ValueHeapPromoted => obj_expr,           // already Nova_X*
        AllocKind::Heap => obj_expr,                        // already Nova_X*
    }
}
```

User-side syntax не меняется — `v.method()` works в обоих modes.

#### Field-access codegen для ValueHeapPromoted (Plan 127.1 Ф.1)

Member-access на value-record local выбирает `.` vs `->` operator based
on `AllocKind`:

| AllocKind | C output | Reason |
|---|---|---|
| `AllocKind::Value` | `obj_c.field` | stack struct member access |
| `AllocKind::ValueHeapPromoted` | `obj_c->field` | heap pointer dereference (Nova_X*) |
| `AllocKind::Heap` | `obj_c->field` | existing reference-record behavior |

До Plan 127.1: Member-access path в codegen всегда emit'ил `.field` для
value records, что давало invalid C code (`Nova_Vec3* obj; obj.x`)
после Plan 127 Ф.3 heap-promote — runtime miscompile (3 Plan 127
fixtures broke: t3/t8/t9).

Plan 127.1 Ф.1 (commit `a2b6f9c9518`) добавил `AllocKind::ValueHeapPromoted`
branch в Member-access emit path — symmetric с `prepare_method_recv`
helper из D228 Method receiver compatibility section. Closes
`[M-127-codegen-field-access-promoted-ptr]` P1 runtime bug.

Plan 127 regression: **12/6 → 15/3** (t3 + t8 + t9 now PASS).

#### Nested record-literal per-field type resolution (Plan 124.9 Ф.1)

> **Trigger:** discovered Plan 128.2 Ф.2 — nested record literal
> `A { b: B { c: C { d: D { value: 0 } } } }` (4-level) сообщённо
> аллоцировал `Nova_A` на каждом уровне вместо declared field types
> `B`/`C`/`D`; workaround был explicit `.new()` constructors. Plan 124.9
> closes `[M-124.9-nested-record-literal-codegen]`.

Record-literal field-value codegen (`emit_record_lit`,
`compiler-codegen/src/codegen/emit_c.rs`) резолвит тип каждого
field-value по **declared field type из record schema**
(`record_schemas[struct_name][field]`), а НЕ по outer/expected record
type родителя. Вложенный `RecordLit` с собственным `type_name`
(`B { ... }`) всегда аллоцирует свой declared field-type, независимо от
контекста окружающего литерала:

```c
/* mut a = A { b: B { c: C { d: D { value: 0 } } } } */
Nova_A* _t1 = nova_alloc(sizeof(Nova_A));   /* outer */
Nova_B* _t2 = nova_alloc(sizeof(Nova_B));   /* field b's declared type B */
Nova_C* _t3 = nova_alloc(sizeof(Nova_C));   /* field c's declared type C */
Nova_D* _t4 = nova_alloc(sizeof(Nova_D));   /* field d's declared type D */
```

Каждый уровень аллоцирует свой declared field-type — outer type НЕ
leak'ится во вложенные typed литералы. Правило применяется к reference
records, value records (D228), и generic nested literals; nested literal
в fn-arg / return-position резолвится идентично. Empty-context fallback
(top-level binding без expected type) не переопределяет inner
`type_name`. Lifts Plan 128.2 explicit-`.new()` workaround.

### Plan 124.8 Acceptance (A8.1-A8.20) — ALL ✅

- A8.1 ✅ Multi-line tuple `type X(\n a, \n b\n)` parses.
- A8.2 ✅ Trailing comma `type X(a, b,)` parses.
- A8.3 ✅ Multi-line + trailing comma parses.
- A8.4 ✅ `type X(priv f int)` → E_TUPLE_NO_PRIV.
- A8.5 ✅ `type X(pub f int)` → E_TUPLE_NO_PRIV.
- A8.6 ✅ `type X(mut f int)` → E_TUPLE_NO_PER_FIELD_MOD.
- A8.7 ✅ `mut p = Vec3(...)` + binding tuple works.
- A8.8 ✅ `ro p` blocking (D175 amend).
- A8.9 ✅ `type Vec3 value { ... }` parses.
- A8.10 ✅ **V2 LANDED 2026-06-03:** value-record real stack codegen
  через NovaValue_X inline struct (closes [M-124.8-value-codegen-stack]).
- A8.11 ✅ **V2 LANDED 2026-06-03:** method receiver = NovaValue_X*
  pointer to stack-slot через prepare_method_recv helper.
- A8.12 ⚠️ V2 partial: scalar/struct fields inline; `[]NovaValue_X` array
  elements currently boxed (V3 followup для inline element storage).
- A8.13 ✅ **V2 LANDED 2026-06-03:** param pass = value copy (C-native);
  return-by-value works через RVO.
- A8.14 ✅ `type Token value consume { ... }` works (composition).
- A8.15 ✅ `type X value priv { ... }` works (D220 §3.3.1 preserved).
- A8.16 ✅ `ro x ro T` → E_REDUNDANT_TYPE_MODIFIER.
- A8.17 ✅ `mut x mut T` → E_REDUNDANT_TYPE_MODIFIER.
- A8.18 ✅ `ro x mut T` parses + works (D176 gap closed).
- A8.19 ✅ `ro acc` blocks `acc.mut_field = X` (D175 amend).
- A8.20 ✅ Regression: plan120 8/8 + plan124_1 9/9 + plan124_2 14/14 +
  plan124_3 10/10 + plan124_6 7/7 + plan108_3 14/14 unchanged.

### Plan 124.8 V2.1 Acceptance — followup markers closed (A8.21-A8.30)

V2.1 closes 3 [M-124.8-*] markers landed 2026-06-03:

- A8.21 ✅ [M-124.8-ro-binding-scope]: `ro_binding_names` block-scoped.
  `f1_block` snapshots/restores на entry/exit. Inner `ro x = ...` не
  leak'ит в outer scope. Cross-module contamination (stdlib `ro v` →
  user `mut v`) fixed.
- A8.22 ✅ [M-124.8-ro-binding-scope] shadow: `Stmt::Let` shadow-aware.
  `ro x; { mut x; x.field = ... }` works через inner mut shadow remove
  prior ro entry; outer state restored на block exit.
- A8.23 ✅ [M-124.8-tuple-mut-field-write-codegen]: end-to-end positive
  (single field write, multi-field write, sequential overwrite,
  compound expressions, cross-field arithmetic, write-then-method-read).
- A8.24 ✅ [M-124.8-tuple-mut-field-write-codegen]: negative — D175
  binding-dominates на tuple field write через ro binding →
  E_READONLY_FIELD.
- A8.25 ✅ [M-124.8-zero-on-move] V1 parser: `#zero_on_move` attribute
  recognized наряду с #from_fields/#from_pairs/#impl. Duplicate detection.
  Только перед `type` декларацией валидно.
- A8.26 ✅ [M-124.8-zero-on-move] V1 AST: TypeDecl.zero_on_move: bool
  flag (default false; backward-compat preserved).
- A8.27 ✅ [M-124.8-zero-on-move] V1 checker validation: allowed kinds
  Record (heap + value), NamedTuple, Newtype. Reject Effect/Protocol/
  Sum/Alias/Opaque с E_ZERO_ON_MOVE_INVALID_KIND.
- A8.28 ✅ [M-124.8-zero-on-move] V1 codegen: per-type
  `static inline void Nova_T_zero_storage(<C_type>* p)` helper emit.
  Picks correct C type (Nova_T для heap+newtype, NovaValue_T для value,
  NovaTuple_T для named tuple).
- A8.29 ✅ [M-124.8-zero-on-move-auto-inject] V2.2 (221.1 №465,
  2026-08-09): auto memset at consume call sites — LANDED for the safe
  subset (consume-param-arg + bare consume-return, `AllocKind::Value`
  record + `NamedTuple` + ordinary `Newtype`). See the D-amendment below
  for the full analysis, the two new checker guards that keep the unsafe
  subset from silently compiling, and the named per-primitive risk
  breakdown for `std`'s sync/concurrency types that motivated the original
  DEFERRED note.
- A8.30 ✅ Regression V2.1: plan120 8/8 + plan124_1 9/9 + plan124_3 10/10
  + plan108_3 14/14 unchanged. plan124_8 27/27 → 40/40 PASS (+13 new
  fixtures для 3 markers).

#### D-amendment (221.1 №465, 2026-08-09) — A8.29 auto-inject landed for copy-semantics storage; aliased storage rejected at compile time

**Проблема, найденная владельцем 2026-08-08 по странице спеки:** A8.25-A8.28
были landed, но A8.29 (сам вызов `Nova_T_zero_storage` на consume-сайтах)
остался DEFERRED — значит `#zero_on_move` компилировался ЗЕЛЕНО и НИЧЕГО не
зануляло. Атрибут БЕЗОПАСНОСТИ с молчаливым no-op хуже отсутствия фичи
(реестр 221.1, запись №465).

**Почему это было отложено, а не сделано сразу (переоткрыто и подтверждено
здесь):** Nova's ownership-transfer для `consume` **не однородна** по видам
хранения:

- `AllocKind::Value` record / `NamedTuple` / ordinary (non-runtime-backed)
  `Newtype` — `consume`-передача (параметр, `return X`) — это **настоящий
  байтовый C-копирование** (A8.13: "param pass = value copy (C-native)").
  Подтверждено эмпирически чтением сгенерированного C
  (`docs/plans/repro/p465/probe1-3.nv.txt`): `nova_fn_helper(NovaValue_Secret s)` получает
  НЕЗАВИСИМУЮ копию; `return s;` копирует значение в return-слот ДО того,
  как исходный стек-слот освобождается. Зануление ИСТОЧНИКА после того, как
  копия сделана, безопасно — новый владелец не видит эффекта.
- `AllocKind::Heap` record (`type X { … }`, без `value`) — `consume`-передача
  — это **алиасинг указателя**: старая и новая переменная указывают на
  ОДИН И ТОТ ЖЕ heap-блок (Nova не делает deep-copy при move для
  heap-типов). Зануление pointee после такой передачи занулило бы значение
  и для НОВОГО владельца — та же память. Это не «занулить протухшую копию»,
  а «стереть единственный существующий экземпляр из-под живой ссылки» —
  порча данных, а не защита.
- Receiver-вызов consuming-метода (`x.method()`) — даже для `value`-типов
  `prepare_method_recv` передаёт `&x` (адрес-алиас в кадре вызывающего), НЕ
  копию (D228). `escape_analyze.rs` **намеренно** исключает receiver-позицию
  метод-вызова из своего множества escape-синков (иначе КАЖДЫЙ вызов метода
  форсировал бы heap-promote любого value-record — противоречило бы всей
  цели Plan 127). Значит для receiver-формы у компилятора НЕТ доказательства
  «callee не сохранил `&x` дольше своего вызова» — зануление здесь было бы
  недоказанным.

**Решение (реализовано в этом слиянии):**

1. **Auto-inject landed ТОЛЬКО для доказуемо-безопасного подмножества** —
   consume-param-arg (свободная функция ИЛИ метод, НЕ receiver-позиция) и
   bare `return X` (голый идентификатор), и ТОЛЬКО для типов с
   byte-copy storage (`AllocKind::Value` record / `NamedTuple` / обычный
   `Newtype`). Реализация — copy-out-then-zero-source: хвостовой
   промежуточный `__tmp = x;` эмитится ДО `Nova_T_zero_storage(&x)`, вызов/
   `return` используют `__tmp`, не `x` (иначе зануление ДО того, как копия
   сделана, отдало бы callee уже занулённые данные). Компилятор:
   `compiler-codegen/src/codegen/emit_c.rs`,
   `zero_on_move_rewrite_call`/`zero_on_move_hoist_and_zero` (consume-param-
   arg, hooked в central `emit_expr` choke-point, тот же, что уже
   использует `disarm_auto_cleanup_receiver_call`) и `Stmt::Return`'s
   bare-Ident path (return-site).
2. **`#zero_on_move` теперь требует `consume` на том же типе**
   (`E_ZERO_ON_MOVE_REQUIRES_CONSUME`, `compiler-codegen/src/types/mod.rs`)
   — auto-inject цепляется ИСКЛЮЧИТЕЛЬНО за уже существующую consume-
   трекинг машинерию (D432 §4: `consume_receiver_methods`/
   `*_consume_param_positions`), которая ключуется по `consume`. Без
   `consume` НЕТ ни одной отслеживаемой точки передачи владения — атрибут
   остался бы тем же самым молчаливым no-op, просто более узким. До этой
   поправки A8.27 разрешал `#zero_on_move` без `consume` (комбинация
   технически проходила чекер, но была бессмысленна) — **язык-меняющее
   сужение**, но ущерба нет: носителей атрибута в `std`/`examples` НОЛЬ
   (реестр 221.1 №465, зафиксировано владельцем 2026-08-08).
3. **`#zero_on_move` на heap-allocated record — HARD ERROR**
   (`E_ZERO_ON_MOVE_ALIASED_STORAGE`) — по причине «алиасинг указателя»
   выше. Единственный вид из допустимых A8.27 kinds, который теперь ДОПОЛНИТЕЛЬНО
   исключается: `TypeDeclKind::Record` с `AllocKind::Heap`.
   `NamedTuple`/`Newtype`/`value`-record остаются допустимы без изменений.
4. **Receiver-вызов (`x.method()`) НЕ покрыт** и остаётся тем же
   недоказанным случаем, каким был — компилятор не отвергает его отдельным
   диагностиком (это не структурная невозможность, как heap-alias, а
   отсутствие доказательства; полное покрытие требует нового
   escape-safety-анализа именно для receiver-позиции, вне периметра этого
   слияния). Задокументировано как известный, а не скрытый пробел.

**Именной разбор риска по `std`'s sync/concurrency-примитивам** (ровно то,
из-за чего A8.29 изначально отложили — "regression risk для sync
primitives"; проверено поимённо, ни один сегодня НЕ несёт
`#zero_on_move`):

| Тип | Объявление | `consume`? | Вид хранения | Судьба под новыми правилами |
|---|---|---|---|---|
| `AtomicI64`/`I32`/…/`AtomicInt`/`AtomicUint`/`AtomicBool` (`std/src/runtime/sync.nv`) | `type X value priv { v … }` | НЕТ | Value record | `E_ZERO_ON_MOVE_REQUIRES_CONSUME` отвергает — эти типы разделяемые/долгоживущие, не одноразово-переносимые, `consume` им семантически не подходит |
| `Mutex`, `RwLock`, `Condvar`, `ReentrantMutex`, `WaitGroup`, `Once`, `Barrier`, `CountDownLatch`, `Semaphore` (`std/src/runtime/sync.nv`) | `type X(*())` | НЕТ | Newtype, runtime-backed (`debt_is_runtime_backed_newtype`) | Двойная защита: `E_ZERO_ON_MOVE_REQUIRES_CONSUME` (не `consume`) на уровне чекера; ДАЖЕ если бы это правило обошли — codegen-пре-пасс НЕ регистрирует runtime-backed newtype в `zero_on_move_types` (helper для них вообще не эмитится, т.к. `emit_type_decl`'s Newtype-ветка возвращает раньше для этого списка) |
| `MutexGuard`, `ReadGuard`, `WriteGuard`, `Permit` (`std/src/runtime/sync.nv`) | `type X consume { ptr int }` | ДА | Heap record | `E_ZERO_ON_MOVE_ALIASED_STORAGE` отвергает — ровно случай «алиасинг указателя» выше; это и есть буквальный носитель исходного «sync primitives» риска в DEFERRED-заметке |
| `CancelToken` (`std/src/prelude/concurrency.nv`) | `type X(*())` | НЕТ | Newtype, ОБЫЧНЫЙ (не в списке runtime-backed) | `E_ZERO_ON_MOVE_REQUIRES_CONSUME` отвергает (не `consume` — токен разделяемый, не одноразовый). Будь он `consume`, был бы structurally безопасен (обычный newtype = typedef-копия, зануление трогает только локальный указатель, не то, на что он указывает) — но это гипотетический случай, сегодня неприменимо |
| `TcpStream`, `TcpListener`, `TcpReadHalf`, `TcpWriteHalf`, `UdpSocket` (`std/src/net/tcp.nv`, `udp.nv`) | `type X consume value priv { handle *(), [rc *mut AtomicInt] }` | ДА | **Value record** (не heap — уточнение к первоначальному предположению «половинки TCP — heap») | Проходит ОБА новых guard'а — если бы кто-то добавил `#zero_on_move`, auto-inject бы сработал на consume-param-arg/return сайтах. Structurally безопасно: зануление собственных полей wrapper'а (`handle`, `rc`) после независимой C-структурной копии НЕ трогает внешний OS-ресурс/разделяемый refcount-объект, на который эти поля указывают (та же логика, что для обычного Newtype, оборачивающего указатель) — но receiver-вызов (`stream.close()`) остался бы НЕ занулённым (см. п.4 выше), т.е. частичное, не полное покрытие, будь атрибут добавлен |

Вывод: каждый сегодняшний sync/concurrency-примитив либо структурно не
достигает нового кода (не `consume`, либо `consume`-но-heap → жёсткая
ошибка), либо (TCP/UDP-половинки) доказуемо безопасен под теми же
гарантиями, что и общий value-record случай. Полного покрытия
receiver-вызова НЕТ ни для одного вида — известный, документированный
предел объёма, не скрытая дыра.

**Регрессия:** `spec_tests/conformance/standalone` (140 файлов) 122/0/18,
`neg` (566 файлов, 6 партий) 559/4/3 — все 4 FAIL совпадают с
предсуществующим красным списком (`f5_propagation_trace_full`,
`f5_uncaught_trace_panic`, `f5_uncaught_trace_throw`, `neg_read_oob`).
`std` (весь `nova test std`) 66/7/106 — все 7 FAIL предсуществующие,
структурно недостижимы новым кодом (`grep -rl zero_on_move std/src` пуст).
`std/src/concurrency` чисто (7/0/6). `std/src/net` заблокирован
предсуществующим CC-FAIL в `addr.nv` (несвязанная ошибка типов
Result/IoError), не относящимся к #465. Проба «подсунь негодное»: снятие
инъекции (обе точки) красит новые фикстуры
(`spec_tests/conformance/standalone/m465_zero_on_move_autoinject_pos.nv`)
в RUN-FAIL; `git checkout --` восстанавливает — `git diff` пуст, фикстуры
снова PASS.

### Followups

- ✅ **[M-124.8-value-codegen-stack]** — V2 LANDED 2026-06-03 — proper
  stack codegen реализован (emit_value_record_type + prepare_method_recv).
- ✅ **[M-124.8-tuple-mut-field-write-codegen]** — CLOSED 2026-06-03.
  4 fixtures cover named-tuple field-write end-to-end через release
  nova-cli + clang: single field write, multi-field write, sequential
  overwrite, compound expression (`p.value = p.value * 2 + p.step`),
  cross-field arithmetic, write-then-method-call visibility,
  binding-dominates negative (`ro v` блокирует write).
- ✅ **[M-124.8-ro-binding-scope]** — CLOSED 2026-06-03. Root cause:
  `ro_binding_names` была monotonic per-ctx (никогда не очищалась).
  Stdlib `ro v = ...` (sha1.nv / semver.nv) leak'ало в пользовательские
  fixtures с `mut v = ...`. Fix: `f1_block` snapshots/restores
  `ro_binding_names` на entry/exit; `Stmt::Let` shadow-aware (всегда
  removes prior entry, добавляет назад только если ro). 3 fixtures
  positive + negative.
- ✅ **[M-124.8-zero-on-move]** V1 CLOSED 2026-06-03 — opt-in
  `#zero_on_move` attribute. V1 parser + AST + checker validation +
  per-type `Nova_T_zero_storage` helper emit. Allowed kinds: Record
  (heap + value), NamedTuple, Newtype. Reject Effect/Protocol/Sum/
  Alias/Opaque с E_ZERO_ON_MOVE_INVALID_KIND. Auto-injection
  отложено к V2 followup [M-124.8-zero-on-move-auto-inject].
- ✅ **[M-124.8-zero-on-move-auto-inject]** V2.2 CLOSED 2026-08-09 (221.1
  №465) — auto memset of source at consume-param-arg + bare consume-return
  sites, restricted to byte-copy storage (`AllocKind::Value` record /
  `NamedTuple` / ordinary newtype); heap-allocated records rejected at
  compile time (`E_ZERO_ON_MOVE_ALIASED_STORAGE` — pointer-aliasing move
  would corrupt the new owner's value); `#zero_on_move` now requires
  `consume` (`E_ZERO_ON_MOVE_REQUIRES_CONSUME`). Receiver-consuming-call
  sites remain uncovered (documented limit, not a silent gap — see the
  D-amendment above for the full analysis and the named per-primitive risk
  table for `std`'s sync/concurrency types).
- **[M-124.8-value-record-mut-literal-codegen]** — pre-existing bug
  (exposed at 2026-06-03 testing): `mut t = ValueT { ... }` direct
  literal binding emits `Nova_T*` вместо `NovaValue_T`. Workaround:
  `ro` + constructor pattern. Not zero_on_move specific.
- **[M-124.8-value-record-array-inline]** — `[]NovaValue_X` inline
  element storage (V3, deferred — currently boxed).
- **[M-124.8-value-heap-promote]** — `&value` escape analysis +
  auto-heap-promote. **Scope assigned to Plan 127** (2026-06-03) после
  consultation с Plan 118 owner: Plan 118 Ф.2 V1 покрывает primitives +
  named tuples, value-records остаются вне scope. Plan 127 extends Plan
  118 Ф.2 walker на value-records; reuse `escape_analyze` + extend
  trigger conditions.
- **Plan 126** — auto-derive Equal/Hash/Clone/Compare/Display.
- **Plan 127** — value-record escape & auto-promote (см. выше).

### D229 — Debug protocol + format spec `${expr:?}`

> **AMEND (Plan 208 Ф.0, 2026-07-15) — диспетч `${expr:?}` через `@debug(mut f Fmt)`, радикс
> через `f.kind()`.** [D422](#d422-unified-formatter--единый-displaymut-f-fmt--debug-байтовый-write-zero-alloc-pad-plan-208-2026-07-15)
> заменяет `@debug(mut w Write)` на `@debug(mut f Fmt)` (тот же sink-embed, что и `@display`,
> см. D374-аменд); радикс-спеки (`:x`/`:o`/`:b`) для user-типов, ранее `E_BAD_FORMAT_SPEC`,
> теперь читаются типом через ЕДИНУЮ ось `f.kind() -> FmtKind` внутри `@display`/`@debug` (не
> отдельные Rust-подобные трейты `LowerHex`/…). Default-body synthesis (`inject_synthesized_methods`)
> и derived-форма меняются на compact/named различие — см. D422 §5. Целевая модель,
> **Ф.1-4 pending**; текст ниже (сигнатура `@debug(mut w Write)`, memberwise-body с `w.write_str`)
> читать как ТЕКУЩЕЕ (до-D422) поведение.
>
**Plan 91.14** (2026-06-05). New protocol parallel к D183 Display — debug-specific representation.

#### Rationale

1. **Debug semantics distinct from Display.** Plan 91.8a.2 (D183) shipped Display (user-facing display) only. После Plan 118.5 V3 closure, pointer debugging via `(*T).to_debug_str()` proved inelegant — leaks unsafe context, no protocol-level extensibility, can't recursively debug struct holding pointers. Debug closes this gap.

2. **Diagnostic representation ≠ user-facing display.** Default Debug output is memberwise (`TypeName { field1: value, field2: value }`) — matches Rust `#[derive(Debug)]`. Display can be user-friendly (`Point(1, 2)` или `"hello"` без escapes).

3. **Pointer integration.** `*T` impls Debug (в unsafe context only) — emits `"0x7f8a..->Account"`. Bare `${p}` без `:?` остаётся E_PTR_NO_DISPLAY_USE_DEBUG_STR (per D216 §17). Only explicit `${p:?}` opt-in unlocks debug formatting.

#### Protocol declaration

```nova
#stable(since = "0.1")
export type Debug protocol {
    @debug(mut w Write) -> ()
    // D374 AMEND: parameter renamed sb→w, type StringBuilder→Write (decoupled sink).
    // NO default body в protocol decl. Compiler synthesizes per-type
    // via inject_synthesized_methods (auto_derive.rs) for #impl(Debug) types.
}
```

**Метод name = `@debug`** (НЕ `@display`) — avoid collision с D183 Display.@display. Distinct method names enable both protocols on same type simultaneously.

**Hybrid default body strategy** (Plan 91.14 design decision #1, user-confirmed 2026-06-05):
- Protocol decl ships БЕЗ default body — explicit synthesis через `inject_synthesized_methods` (auto_derive.rs) для `#impl(Debug)` типов.
- Primitives (int/f64/f32/bool/char/str): explicit `@debug` bodies в `std/prelude/protocols.nv`; routes через `@field.debug(w)` в synthesized method bodies.
- User types (records): `#impl(Debug)` → `inject_synthesized_methods` appends synthesized `Item::Fn` to AST → codegen sees it as ordinary method.
- `Option[T Debug]` и `Result[T Debug, E Debug]`: explicit Nova-body в `std/prelude/core.nv`; codegen dispatches via DeclaredBody в string interpolation path (Plan 91.15 Ф.2).
- Known limitation: checker does not validate field Debug bounds at `#impl(Debug)` synthesis time — missing bound produces CC-FAIL, not E_BOUND_MISSING.

#### Format spec syntax

Inside Nova interp-string `${expr:SPEC}`:
- `${expr}` — calls Display.@display (D183, unchanged)
- `${expr:?}` — calls Debug.@debug (NEW)
- `${expr:foo}` — E_FORMAT_SPEC_UNKNOWN (foundation; rich spec grammar
  extension shipped in D258/152.7-B, per-type spec dispatch via
  `@display_fmt` shipped in [D419](#d419-Fmt-protocol--format-spec-контекст-для-display_fmt-plan-173-2026-07-13);
  [M-91.14-format-dsl-extensions] CLOSED by D419)

#### Default body synthesis (#impl(Debug))

Когда user type X помечен `#impl(Debug)`:
1. `inject_synthesized_methods` (auto_derive.rs) синтезирует `fn X @debug(mut w Write) -> ()` и append'ит в `module.items` перед codegen.
2. Body: `w.write("X { "); w.write_str("field1: "); @field1.debug(w); w.write(", field2: "); @field2.debug(w); w.write(" }")`.
3. Все поля (primitive и record) — через `@field.debug(w)`. Primitives имеют @debug в `std/prelude/protocols.nv`.
4. Known limitation: checker не проверяет Debug bounds у полей при синтезе. Отсутствие @debug у поля даёт CC-FAIL, не checker error.

Primitives (int/f64/f32/bool/char/str) — explicit @debug в `std/prelude/protocols.nv`:
   - int → decimal string (`str.from(@)`)
   - f64/f32 → float string
   - bool → "true"/"false"
   - char → `"${@:?}"` (quoted with escapes)
   - str → `"${@:?}"` (quoted with escapes)

#### Error codes registered

| Code | Description | Trigger |
|------|-------------|---------|
| E_DEBUG_PRINTABLE_NOT_IMPLEMENTED | Type doesn't impl Debug, no auto-synthesis possible | `${x:?}` where typeof(x) lacks impl |
| E_FORMAT_SPEC_UNKNOWN | Unknown format spec | `${x:foo}` (only `:?` valid in V1) |
| E_PTR_NO_DISPLAY_USE_DEBUG_STR (preserved) | Bare `${p}` для pointer | unchanged from D216 §17 |

#### Cross-refs

- D183 — Display protocol (sibling, distinct semantics — Display equivalent)
- D216 §17 — Pointer Debug formatting (`(*T).to_debug_str()` method superseded by Debug.@debug + `${p:?}` syntax; E_PTR_NO_DISPLAY_USE_DEBUG_STR diagnostic preserved для bare `${p}`)
- D73 — From/Into protocol pair (orthogonal — conversion vs formatting)
- Plan 91.14 (this D-block's home plan)
- Plan 91.13 — JSON conformance (sibling, just landed)
- Plan 91.8a.2 — Display infrastructure (foundation, ~80% mechanism reused)

> ⚠️ **D229 AMENDED by Plan 221.1 (2026-07-21)
> [M-interp-numeric-fallback-silent-garbage] follow-up** — wires up the
> `E_DEBUG_PRINTABLE_NOT_IMPLEMENTED` code this section's §7 table already
> reserved, but which no code in the compiler ever actually raised.
>
> **Root cause (coordinator repro, confirmed both Windows and Linux):**
> `println("${d:?}")` for `type D { a int, b int }` with **NO** `#impl(...)`
> annotation at all printed a raw heap address (e.g. `2640040103904`), not
> `"D { a: 1, b: 2 }"`. `emit_c.rs`'s Debug branch (~42816) calls
> `try_synthesize_default_method_with_gate(t, c, "debug", gate_on_impl=false)`
> with a comment describing this as "zero-friction... no annotation needed"
> — but that call is DEAD for `Debug` specifically: its candidate search
> requires a protocol method with `default_body.is_some()`, and this
> section's own §2 protocol declaration explicitly ships `Debug` with **NO**
> default body ("Compiler synthesizes per-type via `inject_synthesized_
> methods`"). The candidate list is therefore always empty regardless of the
> gate flag — `gate_on_impl=false` never fires the intended bypass for
> Debug. The REAL synthesis mechanism actually used everywhere in the
> current implementation is `inject_synthesized_methods` (auto_derive.rs,
> hand-written memberwise-body generator per §4 above), and it gates on
> `td.impl_protocols` containing `"Debug"` literally — i.e. **`#impl(Debug)`
> IS required** in the shipping implementation, exactly as this section's
> §4 already states ("Когда user type X помечен `#impl(Debug)`") and
> exactly matching its own §7 error-code reservation — that diagnostic was
> simply never wired to fire; the type silently fell through to `emit_c.rs`'s
> generic non-primitive interpolation fallback (`nova_int_to_str`) instead,
> the SAME numeric-cast garbage class the sibling `E_INTERP_NO_DISPLAY`
> (D186 amendment above) closes for bare Display.
>
> ⚠️ **Retraction:** an earlier pass at this same fix (D186 amendment above,
> same-day) asserted "Debug synthesis already uses `gate_on_impl=false`
> (D229/D237, unconditional auto-derive)... available with no `#impl`
> regardless" — that assertion was **WRONG**, based on the emit_c comment's
> claimed intent rather than a verified runtime repro. §4/§7 of THIS section
> (D229, pre-existing, unchanged by this amendment) already correctly
> documented the `#impl(Debug)`-gated reality; the wrong assertion was an
> error in the sibling fix's own reasoning, now corrected.
>
> **Fix:** new type-checker diagnostic `E_DEBUG_PRINTABLE_NOT_IMPLEMENTED`
> (`types/mod.rs::check_interp_no_debug`, sibling of `check_interp_no_
> display`, called from the same `f1_expr_inner` `ExprKind::InterpolatedStr`
> arm) fires when a bare `${x:?}` (spec is exactly `FormatSpec::Debug`; a
> rich `Spec` with `Kind::Debug` goes through the separate
> `emit_format_spec_value` lowering, which already errors honestly via
> `E_BAD_FORMAT_SPEC` when no `@debug` resolves — no gap there) interpolated
> expression's static type is a **non-generic** `Record`/`Sum`/`NamedTuple`/
> `Newtype` declared type with neither (a) an explicit `@debug` method, (b)
> a gate-satisfied `#impl(Debug)` auto-derive synthesis (checker's own
> `synth_methods` overlay already mirrors `inject_synthesized_methods`'s
> gate — `find_method_decl(T, "debug")` covers both), nor (c) a `str.from_
> debug(T)` overload (the Debug-side D410 fallback; unlike Display there is
> no `to_str()`-instance equivalent for Debug).
>
> **Scope** — identical to `E_INTERP_NO_DISPLAY`'s (shared helper
> `resolve_interp_user_value_type`): primitives (which DO have unconditional
> `@debug` per §5 above — never gated), typed pointers (`E_PTR_NO_DISPLAY_
> USE_DEBUG_STR`, separate), generic type-params, and generic declared types
> are left alone.
>
> Fixture: `spec_tests/conformance/neg/d229_interp_no_debug_neg.nv` (pin —
> bare `${x:?}` on a type with zero `#impl` at all).
> `spec_tests/conformance/d186_interp_no_display_pos.nv` hardened to assert
> the EXACT expected string (`"D186InterpDebugOnly { a: 1, b: 2 }"`) via
> `#impl(Debug)`, replacing a prior `contains("1")`/`contains("2")` assertion
> that passed FALSELY (the printed heap address happened to contain both
> digit substrings locally, masking the bug — and on Linux CI the address
> didn't contain `"2"`, so the weak assertion caught the regression there).

### D230 NEW — `Clone` protocol (Plan 126 Ф.1)

**Plan 126** (2026-06-05). New built-in protocol для deep recursive copy
пользовательских типов. Аналог Rust `Clone` trait, completing the gap в
D109 family (Equal/Hash/Compare/Display + Clone).

#### Rationale

1. **Value-record + heap-record нуждаются в structural deep-copy.** До
   Plan 126 пользователь писал boilerplate `fn T.copy(other T) -> T => ...`
   per type. После Plan 124.8 (D228 value-record) этот pattern стал
   универсальным — auto-derive снимает boilerplate burden.

2. **Memberwise recursive semantics.** Compiler synthesize'ит body
   `Self { f1: @f1.clone(), f2: @f2.clone(), ... }` — каждое поле
   рекурсивно clone'ится. Primitives (int/f64/bool/char/byte/str/u*/i*)
   копируются по значению (`@field` без `.clone()`); `str` clone = новая
   аллокация с тем же содержимым.

3. **Distinct от built-in `Hash.@hash` / `Equal.@equal`** — clone
   возвращает Self (not bool / not u64), требует особого synthesizer'а
   `synthesize_clone` (Plan 126 Ф.3).

#### Protocol declaration

```nova
#stable(since = "0.1")
export type Clone protocol {
    clone() -> Self
}
```

Объявлен в `std/prelude/protocols.nv` (Plan 126 Ф.1, commit c7ff5a319ea).

#### Auto-derive семантика

**Record / NamedTuple** (Plan 124.8 D228 / Plan 120 D215):
- Synthesized body: `Self { field1: <clone_expr_1>, field2: <clone_expr_2>, ... }`.
- Per-field clone:
  - Primitive (`int`/`f64`/`bool`/`char`/`byte`/`str`/`u*`/`i*`) →
    shallow copy `@field` (compiler routes к built-in copy semantics).
  - User type (record/tuple) → recursive `.clone()` method call.
  - `[]T` → new array с recursive clone элементов (Plan 90.1
    deep-copy infrastructure).

**Sum-type** — V1 placeholder: returns `@` (self-reference). Rich
match-arms clone (per-variant payload recursion) — followup
[M-126-sum-clone-rich].

**Field eligibility check** (`check_field_eligibility`):
- Каждое поле либо primitive (always eligible),
- Либо имеет `#impl(Clone)` annotation,
- Либо имеет explicit `fn FieldType @clone() -> FieldType`.
- Иначе → `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL`.

**Cycle detection** (visited set `(type, "Clone")` pair):
- `type A { b B }; type B { a A }` + `#impl(Clone)` на обоих →
  `E_AUTO_DERIVE_CYCLE` (unbounded synthesis non-terminating).
- User должен переписать как explicit `fn A @clone() -> A => ...`.

#### Examples

```nova
#impl(Clone)
type Vec3 {
    x f64
    y f64
    z f64
}

ro a = Vec3 { x: 1.0, y: 2.0, z: 3.0 }
ro b = a.clone()    // synthesized: Vec3 { x: @x, y: @y, z: @z }
// (primitives → shallow copy, no .clone() recursion).

#impl(Clone)
type Container {
    name str       // primitive — shallow
    items []int    // array — recursive deep-copy
    inner Vec3     // user type — calls Vec3.@clone()
}

ro c = Container { name: "test", items: [1,2,3], inner: a }
ro d = c.clone()   // d.items != c.items (new allocation), но d.items[0] == 1
```

Manual impl (override default):

```nova
fn Vec3 @clone() -> Vec3 =>
    Vec3 { x: @x, y: @y, z: @z }   // explicit, same result
```

Explicit > auto-derive — user wins resolution в `verify_impl_protocols`.

#### Error codes registered

| Code | Description | Trigger |
|------|-------------|---------|
| E_AUTO_DERIVE_CYCLE | Cyclic recursion non-terminating | type A↔B + `#impl(Clone)` |
| E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL | Field type не impl Clone | `#impl(Clone) type X { y Y }` где Y не Clone |
| E_AUTO_DERIVE_UNSUPPORTED_KIND | Newtype/Alias/Effect/Protocol/Opaque | `#impl(Clone) type Foo = newtype Bar` |

#### Runtime codegen (Plan 126.2 Ф.1/Ф.2)

Plan 126 V1 (2026-06-05) синтезировал `@clone` body и проверял field
eligibility на type-check уровне, но synthesized FnDecl **не достигал**
runtime dispatch — method call `a.clone()` не находил эмитнутую функцию.

**Plan 126.2** (2026-06-06) закрывает этот gap:

- **Ф.1 — method_table registration.** Synthesized `@clone` FnDecl
  регистрируется в `method_table` наравне с user-declared методами, так что
  resolver видит его при разрешении `a.clone()` / operator dispatch.
- **Ф.2 — C codegen emit.** Synthesized body эмитится как C-функция
  `Nova_<T>_method_clone(<T> self) -> <T>` с deep-copy семантикой:
  primitive поля копируются по значению, user-type / array поля рекурсивно
  через `Nova_<FieldType>_method_clone`. Recursive `.clone()` вызовы
  резолвятся через ту же method_table запись.

Result: `a.clone()` теперь runtime-dispatched, completing Plan 126 V1
promise. То же codegen применяется к остальным auto-derived методам
(`@equal`/`@hash`/`@compare`/`@display`).

#### Cross-refs

- [D109 amend](../08-runtime.md#d109-amend-plan-126-2026-06-05---auto-derive-для-пользовательских-типов)
  — auto-derive rules для всех 5 built-in protocols.
- [D109 method_table dispatch note](../08-runtime.md#d109) — Plan 126.2
  runtime dispatch via method_table.
- [D186](#d186) — `#impl(P)` annotation foundation.
- [D183](#d183) — Display protocol (sibling, Display semantics).
- [D229](#d229) — Debug (sibling, Debug semantics).
- Plan 126 (this D-block's home plan).
- Plan 124.8 — value-record D228 (primary use-case).
- Plan 120 — NamedTuple D215 (secondary use-case).
- Plan 90.1 — `[]T` extend/copy_from family (used для array field clone).

---

## D237. Protocol naming convention: method-name capitalized (Plan 137, 2026-06-09)

> **AMEND (Plan 208 Ф.0, 2026-07-15) — сигнатуры `@display`/`@debug` → `(mut f Fmt)`.**
> Имена протоколов/методов (`Display`/`@display`, `Debug`/`@debug` — таблица переименований
> ниже) остаются БЕЗ ИЗМЕНЕНИЙ; меняется только сигнатура параметра под
> [D422](#d422-unified-formatter--единый-displaymut-f-fmt--debug-байтовый-write-zero-alloc-pad-plan-208-2026-07-15):
> `(mut w Write)` → `(mut f Fmt)` (единый sink+спек-контекст вместо голого `Write`). Целевая
> модель, **Ф.1-4 pending**.
>
**Status:** ACTIVE

Prelude-протоколы именуются по принципу **«имя протокола = заглавная форма имени метода»**.
Принцип: `[T Hash]` означает ровно один метод `@hash()`; `[T Display]` — `@display()`.
Conversion protocols (`From`/`Into`/`TryFrom`/`TryInto`) уже следовали принципу.
Domain-specific protocols (`Consumable`, `WithExitTimeout`) — исключения.

### Таблица переименований

| Старый протокол | Новый протокол | Старый метод | Новый метод |
|---|---|---|---|
| `Hash` | `Hash` | `@hash()` | `@hash()` (неизменён) |
| `Compare` | `Compare` | `@compare()` | `@compare()` (неизменён) |
| `Clone` | `Clone` | `@clone()` | `@clone()` (неизменён) |
| `Equal` | `Equal` | `@equal()` | `@equal()` |
| `Display` | `Display` | `@display()` | `@display()` |
| `Debug` | `Debug` | `@debug()` | `@debug()` |

### Диагностика (E_PROTOCOL_RENAMED)

Использование старого имени протокола в `#impl(...)` или bound `[T OldName]`
производит ошибку `E_PROTOCOL_RENAMED` с подсказкой использовать новое имя:

```
error[E_PROTOCOL_RENAMED]: protocol `Hash` was renamed to `Hash`
  --> file.nv:3:12
   |
 3 | #impl(Hash)
   |       ^^^^^^^^ use `Hash` instead
```

### D109 amend (Plan 137)

Протоколы `Hash`, `Equal`, `Compare`, `Display`, `Debug`, `Clone`
переименованы в `Hash`, `Equal`, `Compare`, `Display`, `Debug`, `Clone` соответственно.
Методы `@equal` → `@equal`, `@display` → `@display`, `@debug` → `@debug`.

### AMEND (Plan 175 Ф.3(d), 2026-07-10)

`Duration`/`Timestamp`/`Monotonic` (`std/time/duration.nv`) теперь реализуют
`Display`/`Debug` (D316-amend, детали и codegen-фикс — там). `@display`
byte-exact ASCII (нет байтов > 0x7F — фиксирует старую `μs` U+03BC
регрессию, снятую ещё в Ф.1c); `Monotonic.@display`/`@debug` — offset-форма
(`+1.234s`/`Monotonic(+1.234s)`), явно НЕ дата (D124).

### D183 amend (Plan 137)

`Equal.@equal` → `Equal.@equal`; `Compare.@compare` unchanged.
`Display.@display` → `Display.@display`; default-body synthesis обновлена.

### D229 amend (Plan 137)

`Debug.@debug` → `Debug.@debug`.
`${expr:?}` по-прежнему маршрутизируется к `Debug.@debug`.

### D230 amend (Plan 137)

`Clone` → `Clone`; метод `@clone` неизменён.
Все auto-derive references обновлены в `auto_derive.rs` и stdlib.

### Cross-refs

- [D109](../spec/decisions/08-runtime.md#d109) — built-in protocol family
- [D183](#d183) — Display protocol (was Display)
- [D229](#d229) — Debug protocol (was Debug)
- [D230](#d230-new--Clone-protocol-plan-126-ф1) — Clone protocol (was Clone)
- Plan 137 — home plan

### D230 amend (Plan 138.3, 2026-06-10) — `Clone` = deep/recursive; collections element-wise

**Status:** ACTIVE

Фиксирует семантический контракт `Clone` и отделяет его от shallow value-copy.
Триггер: ревью `Vec.clone` (Plan 138.2 design-cleanup) — текущая реализация была
shallow (bitwise `RawMem.copy`), что нарушает протокол-контракт `Clone`.

#### Контракт: `Clone` — DEEP / рекурсивный

`@clone()` возвращает **полностью независимую** копию. Мутация клона **никогда**
не должна затрагивать оригинал (и наоборот). Для композитных типов `@clone`
рекурсивно клонирует каждый член через его собственный `@clone()`:

```nova
// memberwise рекурсия (auto-derive, см. D230 §Auto-derive выше):
Self { f1: @f1.clone(), f2: @f2.clone(), ... }
```

Primitive-поля (`int`/`f64`/`bool`/`char`/`byte`/`u*`/`i*`) копируются по значению
(`@field` без `.clone()` — для них deep == shallow). `str.clone` = новая аллокация
с тем же содержимым. User-type / `[]T` поля — рекурсивный deep-clone.

#### Collections (Vec / HashMap / Set) — element-wise deep + conditional bound

Коллекции клонируются **поэлементно** через `@clone()` каждого элемента, с
conditional bound на параметре типа (зеркало Rust `impl<T: Clone> Clone for Vec<T>`):

| Тип | Сигнатура `@clone` | Семантика |
|---|---|---|
| `Vec[T]` | `Vec[T Clone] @clone() -> Self` | per-element `out.push(@data[i].clone())` |
| `HashMap[K,V]` | `HashMap[K Clone, V Clone] @clone() -> Self` | per-entry `copy.insert_new(k.clone(), v.clone())` |
| `Set[T]` | `Set[T Clone] @clone() -> Self` | делегирует `@map.clone()` (deep `HashMap[T, ()]`) |

Bound `[T Clone]` означает: `Vec[T].clone()` компилируется **только** если `T`
сам реализует `Clone` (примитивы — всегда; records — через `#impl(Clone)` или
manual `fn T @clone`; см. D230 §Field-eligibility). Это намеренно: коллекция
non-Clone-типа не может дать deep-копию.

#### SHALLOW value-copy — ОТДЕЛЬНАЯ операция (не `@clone`)

Следующие операции — **shallow** value-copy: bit-copy значений элементов, **любой
T** (без `Clone`-bound). Для ref-типовых элементов копия **разделяет** pointee с
источником (это корректно для move/переноса/построения буфера, **не** для clone):

| Операция | Bound | Назначение |
|---|---|---|
| `@extend` / `@push` (`Vec.from(items)` — **RETRACTED 2026-07-20**, Plan 200 П16, см. [D259 AMEND](#d259-конструктор-конвенция-vect--of-для-литерала-from-для-конверсии-plan-1531)) | любой T | построение буфера, move-перенос |
| `@copy_from` / `@copy_within` / `@insert` / `@remove` / `@append` | любой T | сдвиг/перенос значений в буфере |
| `@realloc_to` | любой T | рост capacity (перенос байт) |
| `HashMap.from_iter` / `Set.from_iter` | любой T | построение из итератора (per-element insert) |

`RawMem.copy` / `RawMem.copy_nonoverlapping` (bitwise) — **только** для этих
shallow-контекстов. **Никогда** не для `@clone` (bitwise-копия pointer'ов = aliasing-баг:
shallow-clone коллекции ref-типов тихо разделяет вложенные объекты, мутация «копии»
бьёт оригинал — ровно класс багов, который закрывает этот amend).

#### Различие deep vs shallow (резюме)

| | `@clone()` | `from`/`extend`/`push`/`copy_from`/`realloc` |
|---|---|---|
| Семантика | **deep** — рекурсивный клон via element `@clone()` | **shallow** value-copy (bit-copy) |
| Bound | `[T Clone]` (conditional) | любой T |
| ref-T элементы | независимый pointee (рекурсия) | **разделяет** pointee |
| Реализация | per-element `.clone()` loop | `RawMem.copy` / per-element push |
| Назначение | независимая копия | move / перенос / построение |

#### KNOWN GAP — ✅ ЗАКРЫТ (Plan 138.4 Ф.1, 2026-06-11)

> **UPDATE (2026-06-11):** этот gap **ЗАКРЫТ** — impl догнал prose. Deep element-wise clone
> для `Vec`/`HashMap`/`Set` теперь **РЕАЛИЗОВАН и GREEN** (Plan 138.4 Ф.1 G-C, commits
> `88432dd6f02` + `363f4b53788`). Блокер `[M-138.3-clone-bound-unsupported]` CLOSED. ROOT CAUSE
> отличался от гипотезы ниже: НЕ «монформизатор мис-диспатчит», а single-key last-wins
> `method_receivers["clone"]` instance-fallback (lookup ТОЛЬКО по имени метода, игнорируя
> receiver-тип) роутил unbound primitive-`T` `.clone()` в произвольный неродственный `@clone`.
> FIX: `PrimBuiltin::Identity` variant — `.clone()` на любом primitive-C-типе = bitwise self
> (вариант (a)/(c) рекомендации ниже) + record/heap identity-clone arm + зеркало в
> `infer_expr_c_type`. Реальный user/synthesized `@clone` сохраняет precedence. Текст ниже —
> историческое описание gap'а до фикса.

**Контракт выше — целевой.** Деталь auto-derive для **records** (memberwise
`field.clone()` recursion) **РЕАЛИЗОВАНА и работает** (`#impl(Clone)` —
plan126/plan126_2 `p3_cloneable_runtime_ok` + `p7_nested_record_clone_deep_ok`
PASS; record `@clone()` корректно эмитит рекурсию по полям).

**(ИСТОРИЧЕСКОЕ, до 138.4) deep element-wise clone для коллекций (`Vec`/`HashMap`/`Set`)
НЕ был реализован** — все три были **shallow** (`@clone()` с bound «любой T»,
без `[T Clone]`): bit-copy / per-(k,v) value-copy. Блокер —
**`[M-138.3-clone-bound-unsupported]`**: bootstrap-монформизатор мис-диспатчил
per-element generic `T.@clone()` / `K.@clone()` / `V.@clone()` для **примитивного**
`T`/`K`/`V` (нет `int.@clone()` / `str.@clone()` — примитивы copy-built-in per
этому D), резолвя unbound generic `.clone()` в произвольный неродственный
`@clone`. Эмпирика: deep `Vec[int].clone()` → runtime crash + регрессия
`plan131/vec_clone_pos`; deep `HashMap[str,int].clone()` → CC-FAIL
`passing 'nova_str' to parameter of incompatible type`. Bound `[T Clone]` сам по
себе **парсится и type-check'ается** (R1 Plan 138.3 — подтверждено), и для
**record** element-типов emit корректен (`Vec[Point].clone()` даёт верную
`Point.@clone()` рекурсию); сломан был только primitive-`T` dispatch.

**Следствие для bound-audit (G5):** так как collection-clone оставались shallow
(`@clone()` любой T, **без** `T: Clone`-требования), нового нарушения bound на
call-site'ах **не было** — deep-направление, требующее `T: Clone`, удерживалось. Для
**примитивных** элементов shallow == deep (нет разделяемого pointee), для record
элементов расхождение было gap'ом. **После 138.4 deep-форма с `[T Clone]` активна** —
collection-clone теперь требует `T: Clone`, recursion корректна для record-элементов.

#### Cross-refs

- [Q31](../open-questions.md#q31) — conditional `[T Clone]` bound на generic instance-методе (design clarification)
- Plan 138.3 — home plan (deep-clone collections)
- `[M-138.3-clone-bound-unsupported]` — primitive-`T` generic `.clone()` мис-диспатч; блокирует deep collection-clone
- `[M-138-rawmem-bulk-ops]` — clone→`RawMem.copy` был shallow-баг; этот amend фиксирует контракт
- Plan 90.1 — `[]T` deep-copy infrastructure (used для array-field clone)

---

## D231. RawMem allocator API — nova_alloc / nova_alloc_uncollectable / nova_free_uncollectable

**Status:** ACTIVE (Plan 131, 2026-06-08)

Low-level GC-tracked allocation exposed to Nova for implementing Nova-native
data structures (Vec[T], custom allocators, FFI).

### Что

Three extern C functions wrapped as `RawMem` static methods in
`std/runtime/raw_mem.nv`, all gated behind `unsafe fn` (E_UNSAFE_CALL_REQUIRES_WRAP
per D216 §9).

### API

```nova
// GC-tracked allocation. Memory zeroed. 8-byte aligned.
// Must be called inside unsafe {} block.
export external unsafe fn RawMem.alloc(n usize) -> *mut u8

// Not GC-tracked. Caller must call RawMem.free_uncollectable.
export external unsafe fn RawMem.alloc_uncollectable(n usize) -> *mut u8

// Free pointer from alloc_uncollectable. UB on GC-tracked pointer.
export external unsafe fn RawMem.free_uncollectable(ptr *mut u8) -> ()
```

### C mapping

| Nova | C (nova_rt/alloc.h) |
|------|---------------------|
| `RawMem.alloc(n)` | `nova_alloc(n)` |
| `RawMem.alloc_uncollectable(n)` | `nova_alloc_uncollectable(n)` |
| `RawMem.free_uncollectable(ptr)` | `nova_free_uncollectable(ptr)` |

### Safety rules

1. `nova_alloc` returns zeroed, GC-collectable, 8-byte aligned memory.
2. Do NOT call `nova_free` on GC-tracked pointers — Boehm GC handles collection.
3. `alloc_uncollectable` for long-lived buffers not visible to the conservative
   GC scanner (e.g. Windows fiber arena buffers where fiber stacks shadow heap).
4. Every call must be inside `unsafe {}` — `unsafe fn` keyword enforced
   (E_UNSAFE_CALL_REQUIRES_WRAP from D216 §9, ACTIVE 2026-06-02).
5. `n = 0` is implementation-defined; use `n > 0` in practice.

### Типичный use case

```nova
// Allocate a typed buffer of n elements of T:
fn alloc_buf[T](n int) -> *mut T {
    unsafe {
        RawMem.alloc((n as usize) * (size_of[T]() as usize)) as *mut T
    }
}
```

### Cross-refs

- [D216 §8](#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — unsafe gating model.
- [D216 §6](#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — pointer arithmetic (ptr + N scaled by sizeof(T)).
- [D232](#d232-vect--nova-native-generic-growable-array) — Vec[T] built on D231.
- `std/runtime/raw_mem.nv` — implementation.

---

## D232. Vec[T] — Nova-native generic growable array

**Status:** ACTIVE (Plan 131, 2026-06-08)

A production-grade growable array implemented *entirely in Nova* on top of
D231 (RawMem.alloc), D199 (size_of[T]()), and D216 pointer arithmetic.
Demonstrates that a collection with correct typed storage needs no compiler
magic beyond what Plans 118/114.4 provide.

### Layout

```nova
export type Vec[T] {
    priv mut data *mut T   // raw element buffer, cap slots wide (writable pointee — explicit *mut T, D246)
    priv mut len  int      // number of live (initialised) elements
    priv mut cap  int      // number of allocated element slots
}
```

> **Pointer model note (Plan 147 D246, 2026-06-12; supersedes flip-scan-draft):**
> `priv mut data *mut T` — writable buffer требует **явного** `*mut T` (L3
> pointee-mut из типа). `mut data` (L1) даёт лишь reassignability поля; pointee-mut
> НЕ наследуется от mut-binding. Прежний flip-scan-вариант `mut data *T` давал бы
> **ro**-pointee (`*T ≡ *ro T`) → запись `@data[i] = …` была бы запрещена. См.
> [D246 P5](#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee).

### Construction

| Call | Effect |
|------|--------|
| `Vec[T].new()` | empty, no allocation (cap = 0) |
| `Vec[T].with_capacity(n)` | empty, pre-allocated n slots |
| ~~`Vec[T].from(items []T)`~~ | **RETRACTED 2026-07-20** (Plan 200 П16, [D259 AMEND](#d259-конструктор-конвенция-vect--of-для-литерала-from-для-конверсии-plan-1531)) — same-T conversion is `existing.clone()`; literal is `of(...)`; width conversion is an explicit per-element loop |
| `Vec[T].from_raw_parts(ptr *T, len, cap)` | build from a raw `(ptr,len,cap)` triple (cross-type bridge, unsafe-obligated; D247) |

### Key methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `push` | `mut @push(v T) -> ()` | Append element, grow if needed |
| `pop` | `mut @pop() -> Option[T]` | Remove and return last |
| `get` | `@get(i int) -> Option[T]` | Element by index, bounds-checked |
| `get_mut` | `mut @get_mut(i int) -> Option[*mut T]` | Raw mutable pointer (unsafe) |
| `insert` | `mut @insert(i int, v T) -> ()` | Shift-insert at index |
| `remove` | `mut @remove(i int) -> T` | Shift-remove at index |
| `swap_remove` | `mut @swap_remove(i int) -> T` | O(1) order-disrupting remove |
| `len` | `@len() -> int` | Live element count |
| `cap` | `@cap() -> int` | Allocated slot count |
| `is_empty` | `@is_empty() -> bool` | True if len = 0 |
| `clear` | `mut @clear() -> ()` | Set len = 0 (retains buffer) |
| `truncate` | `mut @truncate(n int) -> ()` | Shorten to n elements |
| `reserve` | `mut @reserve(additional int) -> ()` | Ensure room for more elements |
| `shrink_to_fit` | `mut @shrink_to_fit() -> ()` | Cap → len |
| `shrink_to` | `mut @shrink_to(min_cap int) -> ()` | Cap → max(len, min_cap) |
| `reverse` | `mut @reverse() -> ()` | Reverse in place |
| `extend` | `mut @extend(items []T) -> ()` | Append all from slice |
| `append` | `mut @append(mut other Vec[T]) -> ()` | Move all from other |
| `retain` | `mut @retain(pred fn(T) -> bool) -> ()` | Keep matching elements |
| `first` | `@first() -> Option[T]` | First element |
| `last` | `@last() -> Option[T]` | Last element |
| `as_slice` | `@as_slice() -> []T` | Copy into built-in slice |
| `as_ptr` | `@as_ptr() -> *T` / `mut @as_ptr() -> *mut T` | Raw data-buffer pointer; recv-mut overload yields writable `*mut T` (cross-type bridge getter, D247) |
| `into_raw` | `consume @into_raw() -> *mut T` | Consume the Vec, surrender its buffer pointer (inverse of `from_raw_parts`; powers zero-copy `str.from_bytes_unchecked_steal`, D247) |
| `iter` | `@iter() -> VecIter[T]` | Index-cursor iterator |
| `clone` | `@clone() -> Vec[T]` | Deep copy (Clone) |
| `equals` | `@equal(other Vec[T]) -> bool` | Element-wise equality |
| `fmt` | `@display(mut sb StringBuilder) -> ()` | Display (format: `Vec[e0, e1, ...]`) |
| `debug_fmt` | `@debug(mut sb StringBuilder) -> ()` | Debug |

### Growth strategy

- Initial capacity: **8** slots on first push into an empty Vec.
- Doubling: `new_cap = current_cap * 2` until `new_cap >= needed`.
- Amortised O(1) push. Realloc copies live prefix via typed pointer loop.

### Typed storage (key property)

Elements are stored at the *real* C type of `T` in a `T*` buffer.
Pointer arithmetic `data + i` is C-scaled by `sizeof(T)` automatically by
the C backend. This means:

- `Vec[Option[int]]` stores each `NovaOpt_nova_int` struct inline (16 bytes/element).
- `Vec[MyRecord]` stores record pointers (`Nova_MyRecord*` 8 bytes/element).
- **No int64-slot erasure** — pre-D239 `[]T` built-in used `NOVA_ARRAY_DECL(T)`
  macro with int64 element slots; after D239 `[]T ≡ Vec[T]` so this gap is closed.

### `[]T` = `Vec[T]` — D239

`[]T` — **синтаксический псевдоним** `Vec[T]` (D239). Typed-storage gap
закрыт: элементы хранятся по реальному C-типу `T` в буфере `T*`. Никакого
int64-erasure.

| Criterion | `[]T` / `Vec[T]` (unified, D239) |
|-----------|----------------------------------|
| Default choice | ✅ yes |
| Primitives (int, f64, str, …) | ✅ typed |
| Records (heap pointer) | ✅ pointer-in-slot |
| Value-struct T (Option[U], tuple, >8-byte value-record) | ✅ typed (gap closed) |
| `for x in` loop | ✅ via VecIter |
| Compiler magic needed | no (pure Nova) |

`[]T` и `Vec[T]` взаимозаменяемы: `fn f(a []int)` принимает `Vec[int]`
и наоборот. Лексически предпочтительна краткая форма `[]T`; `Vec[T]`
используется там где явность важна (generic bounds, type params).

> **Note (Plan 138.2):** в единицах компиляции без явного `Vec`-импорта
> компилятор временно использует `NovaArray_T` C-backing для примитивных
> типов (`[]int`, `[]str`, `[]u8`). После того как `Vec` войдёт в prelude
> (Plan 138.2 [M-138.1-vec-in-prelude]), `NovaArray` будет удалён полностью.
> Поведение идентично — layout `{ data*, len, cap }` совпадает.

### Protocols implemented

- `Index[int, T]` (via `@index(i int) -> T`, panic OOB) — powers `v[i]` (D238)
- `Index[Range, Vec[T]]` (via `@index(r Range) -> Vec[T]`) — powers `v[a..b]` zero-copy (D238)
- `MutIndex[int, T]` (via `mut @index(i int, val T)`) — powers `v[i] = val` (D240)
- `Iter[VecIter[T]]` (via `@iter()`) + `Next[T]` на `VecIter[T]` (via `@next()`)
- `Equal` (element-wise via `@equal`)
- `Clone` (deep copy via `@clone`)
- `Display` (via `@display`)
- `Debug` (via `@debug`)

### Cross-refs

- [D239](#d239-t--синтаксический-псевдоним-vect) — `[]T` = `Vec[T]`; этот D232-тип является backing-типом `[]T`.
- [D238](03-syntax.md#d238-indexk-v-protocol--akey-magic) — `Index[K,V]` protocol; `v[i]` и `v[a..b]` через `@index`.
- [D240](03-syntax.md#d240-mutindexk-v-protocol--akey--val-magic) — `MutIndex[K,V]`; `v[i] = val` через `mut @index`.
- [D231](#d231-rawmem-allocator-api--nova_alloc--nova_alloc_uncollectable--nova_free_uncollectable) — allocator used by Vec[T].
- [D199](09-tooling.md) — `size_of[T]()` const fn used in buffer size calc.
- [D216 §6](#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — ptr arithmetic (`data + i` C-scaled).
- [D226](#d226-signed-indexing-convention) — `int` for len/cap/index.
- [D228](#d228-value-record-allocation-contract) — value-records are valid T (stored by value in buffer).
- [D247](08-runtime.md#d247-str-методы--миграция-external-c--nova-body--vec-cross-type-мост-plan-1392) — `from_raw_parts`/`as_ptr`/`into_raw` cross-type bridge (Plan 139.2 str-method migration).
- `std/collections/vec_owned.nv` — implementation.
- Plan 131 — home plan.

---

## D239. `[]T` — синтаксический псевдоним `Vec[T]`

**Status:** ACTIVE (Plan 138, 2026-06-10)

**Added:** Plan 138 Ф.1–Ф.4 (D239 NEW, 2026-06-10). **Closes:** D144 «future
language version» wording; amends D27 (arr[i] now through `@index`). **Depends
on:** [D232](#d232-vect--nova-native-generic-growable-array) (Vec[T]
implementation); [D238](03-syntax.md#d238-indexk-v-protocol--akey-magic)
(Index protocol).

### Что

`[]T` — **синтаксический псевдоним** `Vec[T]`. Компилятор разворачивает
любое использование `[]T` в `Vec[T]` на уровне type-resolution.

```nova
// Эти объявления полностью эквивалентны:
mut a []int = [1, 2, 3]
mut b Vec[int] = [1, 2, 3]

// Передача по типу:
fn process(xs []int) { ... }
mut v Vec[int] = [4, 5, 6]
process(v)                // OK — []int = Vec[int]

// Литерал строит Vec[T]:
[1, 2, 3]  →  Vec[int] { push 1, push 2, push 3 }
```

### Typed storage — gap закрыт

До D239 `[]T` использовал `NOVA_ARRAY_DECL(T)` C-макрос с int64-slot erasure.
`Vec[T]` хранит элементы по реальному C-типу `T` в буфере `*mut T`:

| Элемент T | `[]T` до D239 | `Vec[T]` / `[]T` после D239 |
|-----------|--------------|------------------------------|
| `int`, `f64`, `bool` | ✅ типизировано | ✅ типизировано |
| `str`, указатель на record | ✅ pointer-in-slot | ✅ pointer-in-slot |
| `Option[int]` (16 байт) | ❌ int64-erasure → UB | ✅ `NovaOpt_nova_int` inline |
| `(int, str)` tuple | ❌ int64-erasure | ✅ typed tuple inline |
| value-record > 8 байт | ❌ int64-erasure | ✅ typed by-pointer |

### Индексация через `@index` (D238)

После D239 `v[i]` и `v[a..b]` работают через `Index[K, V]` protocol (D238),
а не через compiler built-in magic:

```nova
mut v []int = [10, 20, 30]
v[1]       // → v.index(1)      → 20 (panic OOB)
v.get(1)   // → Some(20)        (safe)
v[0..2]    // → v.index(Range { start: 0, end: 2 })  // [10, 20] zero-copy view
v[1] = 99  // → v.@index(1, 99)   write-overload через MutIndex (D240)
```

### Статус реализации

> **Plan 138 Ф.1–Ф.4 (2026-06-10):** `[]T → Vec[T]` flip активен в единицах
> компиляции, которые импортируют `Vec` (явно или транзитивно). Примитивные
> единицы без `Vec`-импорта продолжают временно использовать `NovaArray_T`
> C-backing с идентичным layout.
>
> **Plan 138.2 Ф.0-final (2026-06-11):** `Vec`/`VecIter` вошли в prelude
> (`PRELUDE_VERSION` 13→14) → flip **УНИВЕРСАЛЕН**: `[]T` лоуэрится в
> `Nova_Vec____<T>*` в **КАЖДОМ** prelude-юните (Vec-free юнит `[]int` →
> `Nova_Vec____nova_int*`, typed storage). Юниты с `#no_prelude` **graceful-degrade**
> на legacy `NovaArray_T`-путь (4 gate-сайта `generic_type_templates.contains_key("Vec")`
> в `emit_c.rs` остаются — проходят для prelude-юнитов, fallback для `#no_prelude`).
>
> **Plan 153.0 CONFIRM (2026-06-13):** `Vec[T]` переехал в **folder-module**
> `std/collections/vec/` (co-equal `core`/`access`/`mutate`/`slice`/`iter`/
> `protocols` + `_module.nv`), модуль `collections.vec_owned` ретайрнут (имя
> исчезло; ~55 import-сайтов мигрированы `vec_owned`→`vec`; prelude re-export'ит
> `Vec`/`VecIter` из folder). Eager-комбинаторы (`map`/`filter`/`fold`/`any`/
> `all`) вынесены в отдельный explicit-import `collections.vec_seq` (НЕ
> prelude-global — иначе их generic/param-идентификаторы засоряют каждый юнит,
> [M-153-vec-combinators-prelude-global]). `[]T ≡ Vec[T]` подтверждён для
> инферированных значений; ОСТАЁТСЯ residual: ЯВНАЯ аннотация `v Vec[int]` не
> коэрсится в `[]int`-параметр (`E7301`, [M-153-d239-explicit-vec-to-slice-param]).
> См. [docs/dev/vec-internals.md](../../docs/dev/vec-internals.md).

### NovaArray retirement — частичный (BLOCKED на Plan 139 Ф.2)

> **Status (Plan 138.2 Ф.2-Ф.5, 2026-06-11): ЧАСТИЧНО — полный retire BLOCKED.**
> Универсальный flip (Ф.0-final) приземлён, но физическое удаление
> `NOVA_ARRAY_DECL/IMPL` из `array.h` **заблокировано** пятью load-bearing
> NovaArray-потребителями, которые переживают flip:

1. **Строковый/byte слой (главный блокер — Plan 139 Ф.2 scope-out).**
   `nova_str_as_bytes` → `NovaArray_nova_byte*`, `nova_str_split` →
   `NovaArray_nova_str*`, `from_bytes_lossy`/`from_bytes_unchecked`/`steal`
   остаются C-примитивами (требуют str `@ptr` field-access value-record'а —
   gated на `[M-139-f0-lang-item-decl]`). Эмпирически: `nova_byte`-NovaArray =
   ~35 700 вхождений по корпусу, `nova_str` = ~2 100, `nova_char` = ~1 200
   (`to_chars`). WriteBuffer/StringBuilder bulk-ops (`nova_array_append_nova_byte`
   ~3 300, `compare`/`append_zero`/`truncate`/`reserve` на `nova_byte`)
   маршрутят через C-builtin bridge. Удалить `NOVA_ARRAY_DECL(nova_byte/nova_str)`
   = сломать base64/json/encoding/text. **Risk RG.** → `[M-139-f2-ptr-field-producers]`.
2. **`#no_prelude` graceful-degrade.** `#no_prelude`-юниты (tested feature:
   plan107/plan62/plan110_9_np) лоуэрят `[]T` в `NovaArray_T` (Vec-template
   отсутствует). 4 gate-сайта `contains_key("Vec")` это и обеспечивают. **Risk RE.**
3. **Closure-array `[]fn`** → `NovaArray_void_p*` (намеренное исключение,
   `[M-138.1-closure-array]` / `[M-138.2-closure-array-vec]`).
4. **parfor (D71)** — internal result-collection буферы (`NovaArray_{nova_int,
   nova_bool,nova_f64,nova_str}`), layout-identical с Vec, никогда не escape'ят
   как user `[]T`; миграция = риск без семантического выигрыша → `[M-138.2-parfor-vec]`.
5. ~~**Literal-bridge `Vec[T].from(items []T)`**~~ — static-method param `[]T` всё
   ещё лоуэрится в `NovaArray_nova_int* items` (dead stub в каждом flipped-юните,
   класс `[M-138.2-self-in-param]` — generic-static-method param-type substitution).
   **MOOT 2026-07-20 (Plan 200 П16):** `Vec[T].from` ретрактирован целиком (см.
   [D259 AMEND](#d259-конструктор-конвенция-vect--of-для-литерала-from-для-конверсии-plan-1531)) — этот блокер физически исчез вместе с деклой; список сузился
   до четырёх пунктов. Полный retire NovaArray этим не закрыт (остальные 4
   блокера живы) — заметка только про исчезновение источника пункта 5.

После того как Plan 139 Ф.2 мигрирует string/byte слой на `Nova_Vec____nova_byte*`
(закрытие `[M-139-f2-ptr-field-producers]`), retirement можно завершить:

- `NovaArray_T` C-макрос и `array.h` NOVA_ARRAY_DECL/IMPL — удаляются
- `nova_array_*` runtime helpers для примитивов — заменяются `Vec`-методами
  (уже сделано для generic-Vec пути; остаётся `nova_byte` string-layer bridge)
- Строковый слой (`nova_str_to_bytes`, `nova_str_to_chars`, `split`,
  `string_builder.h`) — мигрирует на `Nova_Vec____nova_byte*`

### §2. Scalar min/max методы на числовых типах (D239 amend, 2026-06-16)

**Added:** [M-153-scalar-min-max] CLOSED (2026-06-16). Скалярные методы сравнения
нужны для shrink-to-min идиомы (Vec capacity management) и `@min`/`@max`-терминаторов
lazy-итератора (D260). Реализованы в [`std/runtime/defaults.nv`](../../std/runtime/defaults.nv).

```nova
// Все числовые типы: int / u8 u16 u32 u64 uint / i8 i16 i32 i64 / f32 f64
(5).max(3)    // → 5
(5).min(3)    // → 3
(10).min(20).min(5)  // → 5  (chaining)

// Сигнатура (int; аналогично для u8…f64 с Self):
fn int @min(other int) -> int
fn int @max(other int) -> int
```

**Семантика:** равенство → возвращается `@` (левый операнд). Для `f32`/`f64` не используются
C-макросы `fmin`/`fmax` (нет специальной NaN-семантики — `NaN` не меньше и не больше ничего,
результат детерминирован через `<`/`>`). **Не** протокол — встроенные методы-примитивы, как `@compare`.

### Связь

- [D232](#d232-vect--nova-native-generic-growable-array) — `Vec[T]` — backing тип для `[]T`
- [D238](03-syntax.md#d238-indexk-v-protocol--akey-magic) — `Index[K,V]`; `v[i]` и `v[a..b]` через `@index`
- [D240](03-syntax.md#d240-mutindexk-v-protocol--akey--val-magic) — `MutIndex[K,V]`; `v[i] = val` через `mut @index`
- [D144](#d144-sub-slice-views-для-t-и-str--arra-b--sa-b) — sub-slice views; D144 «future language version» снято D239
- [D27](03-syntax.md#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `[]T` синтаксис сохраняется; семантика меняется: `[]T ≡ Vec[T]`
- Plan 138 — полный план миграции; Plan 138.2 — NovaArray retirement


> **Amended (vec-sweep, 2026-07-06):** `[]T` — КАНОНИЧЕСКАЯ запись во всех
> контекстах: аннотации типов, возвраты, вложенные формы (`[][]u8`), кортежи
> (`[](str, str)`), статические вызовы-конструкторы (`[]u8.new()`,
> `[]int.of(1,2,3)`, `[]int.from(...)`). `Vec[...]` остаётся ТОЛЬКО
> definition-site — внутри самого модуля `std/collections/vec/` (реализация
> `Vec[T]`). Везде за пределами этого модуля предпочтителен `[]T`.
>
> Исключение — известный compiler-gap [M-153.x-array-new-not-vec]: bracket-
> spelling `[]T` в VALUE-позиции (конструктор-вызов, напр. `[]T.new()`)
> иногда лоуэрится через legacy int64-erased NovaArray-путь, а НЕ через
> типизированный Vec-путь, что при определённых сочетаниях (см.
> `std/collections/hashmap.nv::new_buckets`) даёт RUN-FAIL. В таких точечных
> местах `Vec[...]` остаётся исключением из канона с явным маркером в коде.

> **Amended (вместимость литерала, 2026-08-27):** литерал массива строится
> конструктором с ЗАДАННОЙ вместимостью, а не с умолчальной. Понижение, показанное
> выше (`[1, 2, 3]  →  Vec[int] { push 1, push 2, push 3 }`), описывало только
> пуши и о конструкторе молчало — здесь это молчание снято.
>
> **Правило (нормативно).** Вместимость литерала РАВНА числу его
> элементов, когда это число известно на компиляции, и НЕ МЕНЬШЕ числа
> известных элементов, когда неизвестно (форма со спредом, `[a, ...xs, b]`).
> Литерал без элементов даёт вместимость 0 — это точное число, а не оценка.
>
> Сформулировано над НАБЛЮДАЕМЫМ, а не над формой понижения: `[7, 8].cap() == 2`.
> Так правило не зависит от того, каким конструктором реализация строит литерал, и
> переживёт смену понижения — а смена уже была дважды: `Vec[T].from`
> ретрактирован целиком (MOOT 2026-07-20 выше), `with_capacity` удалён
> амендментом vec-sweep 2026-07-06.
>
> **Почему «не меньше», а не «примерно».** Нижняя граница обязательна: под
> «примерно» подходит и ноль, и тогда правило нельзя ни выполнить, ни нарушить.
> Верхняя граница НЕ задаётся: реализация вправе округлить вверх ради стратегии
> роста, но не вправе занизить.
>
> **Почему нормативно, а не совет.** Вместимость НАБЛЮДАЕМА: `@cap()` —
> публичный метод `Vec[T]` (`std/collections/vec/core.nv`). Молчание канона уже
> развело реализации: замер 2026-08-27 — нынешний компилятор отвечает `cap=8`
> и на `[7, 8, 9]`, и на `[7, 8]`. Это не произвол: без заданной вместимости литерал
> идёт по умолчальному росту [D232](#d232-vect--nova-native-generic-growable-array)
> («×2 growth, initial 8») — то есть следует канону так, как он был написан.
> Именно поэтому правило пишется сюда, а не заводится как дефект реализации.
> Реестр 221.1 №783.
>
> **Механизм есть и работает** (проверено тем же замером): `Vec[int].new(3)` с
> тремя `push` даёт `cap=3`, `Vec[int].new(cap: 2)` — `cap=2`. От реализации
> требуется передать число, а не завести новый конструктор.
>
> **Зеркало исправлено тем же амендментом, и это главный урок здесь.**
> Спека НЕ молчала — она себе ПРОТИВОРЕЧИЛА: зеркало D239 в `03-syntax.md`
> показывало ровно это правило (`Vec[int].with_capacity(3); push …`), но снятым
> именем, а канон конструктора не называл вовсе. Правило сгнило не оттого,
> что его не было, а оттого, что оно жило В КОПИИ и уехало вместе с
> ретрактированным API.

---

## D248. Запрет chained comparison + relational-операнды требуют ordered-категорию (Plan 150)

> **Status:** 🆕 SPEC LANDED 2026-06-13 (Plan 150 Ф.0). Реализация — [Plan 150](../../docs/plans/150-chained-comparison-relational-safety.md)
> Ф.1 (parser + checker). **Резолвит** [Q35](../open-questions.md). **Решение автора:** hard-error (как
> Rust); chained comparison **НЕ добавляем** (только Python чейнит). **Разблокирует** `[M-140-bounds-as-contract]`.

### Проблема (security-дефект)

`0 <= i < n` парсится как `(0 <= i) < n` = `bool < n`. Так как `bool ∈ {0,1}`, выражение **вакуумно-истинно**
для любого `n > 1` — предикат нейтрализуется молча. Следствие: канонический bounds-контракт
`requires 0 <= i < @len` **молча вакуумен** → проверка границ тихо обходится. Nova была **хуже всех peers**
(даже untyped JS коэрсит в детерминированно-неверный результат; Nova аннулирует предикат).

### Решение

1. **Chained comparison ОТКЛОНЁН (hard error).** Цепочка из ≥2 relational/equality операторов одного
   precedence-уровня (`a OP1 b OP2 c`, где `OP ∈ {< <= > >= == !=}`) → **`E_CMP_CHAIN_UNSUPPORTED`** с
   machine-applicable fix-it «`a OP1 b && b OP2 c`». Применяется **даже к ordered-типам** (`1 < 2 < 3` — тоже
   ошибка) и к equality-цепочкам (`a == b == c`). Rationale: только Python чейнит; Go/Rust/Kotlin/Java/Swift —
   hard-error. `&&` явно, без нового синтаксиса/парсер-сложности.

2. **Relational-операнды требуют ordered-категорию.** `<` / `<=` / `>` / `>=` требуют mutually-ordered тип
   (int/float/str/char или `@compare`-несущий тип). `bool` / `unit` как операнд → **`E_RELATIONAL_OPERAND_NOT_ORDERED`**.
   `ptr`-relational → `E_PTR_ARITHMETIC_BANNED` (existing, Plan 115 — см. ptr type-checker rules). `==` / `!=`
   на `bool`/`unit` — **остаются легальны** (под баном только relational). Консистентно с дизайном: bool
   ordering уже method-only через `@compare` ([D183](#d183-canonical-comparison-protocols--default-method-bodies-plan-918a),
   `std/runtime/defaults.nv`), а **не** через оператор `<`.

### Канон

- **Диапазон:** `a <= b && b < c`. `requires 0 <= i && i < @len` — **реальный** (не вакуумный) bounds-контракт.
- Permissive-на-Unknown/generic-категориях сохраняется (чекер фейлит только на definitively-known bool/unit
  + concrete cross-category mismatch — не ломать generics).

### Диагностические коды

- **`E_CMP_CHAIN_UNSUPPORTED`** — ≥2 сравнения в цепочке (`a < b < c`, `0 <= i < n`, `a == b == c`); сообщение
  «comparison operators cannot be chained» + fix-it «split into `a OP1 b && b OP2 c`». Эмитится в parser.
- **`E_RELATIONAL_OPERAND_NOT_ORDERED`** — `bool`/`unit` как операнд `<`/`<=`/`>`/`>=`. Эмитится в checker.

### Связь

- [D183](#d183-canonical-comparison-protocols--default-method-bodies-plan-918a) — comparison-протоколы
  (`@compare`/`@equal`); ordered-категория = `@compare`-несущие типы.
- Plan 115 / `E_PTR_ARITHMETIC_BANNED` — ptr relational ban (reuse, без дублирования).
- [Q35](../open-questions.md) — резолвит. `[M-140-bounds-as-contract]` — разблокирует (`requires 0 <= i && i < @len`).
- **Отложено (сознательно):** Python-style chaining (`a<b<c` ≡ `a<b && b<c`) — НЕ добавляем; если будет
  спрос — отдельное предложение в будущем.

---

## D374. Write-sink протокол — декаплинг Display/Debug от StringBuilder (Plan 152.7.1)

> **AMEND ×2 (Plan 208 Ф.0, 2026-07-15) — [D422](#d422-unified-formatter--единый-displaymut-f-fmt--debug-байтовый-write-zero-alloc-pad-plan-208-2026-07-15).**
> (1) Sink для `Display`/`Debug` меняется с `Write` на **`Fmt`** (embeds `Write` через `use`,
> D145) — `@display`/`@debug` получают `(mut f Fmt)`, не `(mut w Write)` напрямую (Fmt даёт
> доступ и к `@write`, и к осям спека). (2) `Write.@write_str(s str)` меняется на
> **`Write.@write(bytes []u8) -> ()`** — байтовый sink, не строковый; str-значение → явный
> `.bytes()` (D176), str-**литерал** → коэрсия в `[]u8` (D55-аменд). Целевая модель, **Ф.1-4
> pending** (см. D422 §Статус) — текст ниже (сигнатуры `@write_str(s str)`, `w Write` на
> `@display`/`@debug`) читать как ТЕКУЩЕЕ (до-D422) поведение.
>

> **AMEND ×3 (owner decision 2026-07-17, канон mut-параметров, обе позиции).**
> Канон записи sink/`mut`-параметров (`@display(mut f Fmt)`, `mut w Write`, и
> `mut`-параметров вообще) — **ПРЕФИКСНАЯ форма**: `mut <имя> <Тип>`. Есть ДВЕ
> позиции, куда исторически можно поставить `mut`, и они значат РАЗНОЕ:
> позиция ПЕРЕД именем — канон (D176/Plan 108.1: opt-in на мутацию, default —
> read-only); позиция ПОСЛЕ имени, ПЕРЕД типом (`<имя> mut <Тип>`, D6
> legacy-спеллинг) — парсер принимает её как ПОЛНЫЙ поведенческий синоним
> префиксной формы (эмпирика: `i mut int` реассайнится в теле идентично
> `mut i int`) — НЕ более узкая семантика, просто исторический альт-спеллинг.
> Голая постфиксная форма (без явного `ro` перед именем) для НЕ-slice/НЕ-fixed-
> array типов — под запретом lint'а `W_PARAM_TYPE_POS_MUT` (unconditional
> pipeline, `compiler-codegen/src/lints.rs`); позиция ПОСЛЕ имени остаётся
> легитимной ИСКЛЮЧИТЕЛЬНО за view-слайсами (`[]u8` и родня, io-канон,
> `buf mut []u8`) и fixed-size массивами (`[N]u8`, hash-digest out-буферы,
> `std/crypto/sha256.nv` и родня). Явный **R2-split** `ro <имя> mut <Тип>`
> (D246 P6, Plan 118.5 V3 amend) — санкционированное, НЕ каноническое,
> исключение (самодокументирует «пишу в содержимое, не подменяю биндинг»);
> lint её не флагует. Для sink-типов (`Fmt`/`Write`) семантика `mut` на
> кучевом handle — «в него ПИШУТ, видно вызывающему» (Plan 184 Р2/Р10: sink
> ВСЕГДА кучевой handle, `mut x T` не вводит in-out `T*` для handle-типов —
> `param_is_inout_ptr`=false в `emit_c.rs`; запись идёт через shared handle,
> переприсваивание биндинга локально/невидимо), НЕ «sink можно подменить» —
> см. также исследование 2026-07-16 (ветка `research-mut-canon`,
> `docs/dev/research/2026-07-16-mut-param-sink-canon.md`) для полного разбора
> R2-split-vs-канон trade-off'а и латентной лености чекера на protocol-typed
> handle-параметрах (закрыта тем же owner-decision: `[M-checker-protocol-param-mut-lenient]`
> + `[M-conformance-param-mode-check]`, `types/mod.rs`). Полный текст канона
> и примеры — `nv-coding-style.md §18б`. Bare `f Fmt` (без `mut` вовсе) для
> sink остаётся запрещён отдельно (§18 `E_PARAM_NOT_MUT`).
>
> **AMEND ×4 (owner decision 2026-08-12, реестр 221.1 №611/№615, окно
> p616-mode-modifiers, амендмент [D445](#d445)).** Абзац выше (AMEND ×3)
> называет постфиксную позицию `<имя> mut <Тип>` «легитимной ИСКЛЮЧИТЕЛЬНО
> за view-слайсами и fixed-size массивами» и явный R2-split `ro <имя> mut
> <Тип>` — «санкционированным, НЕ каноническим, исключением». ОБА эти
> исключения ОТМЕНЕНЫ: D445 (2026-08-03) уже говорит «без исключений и без
> постфиксной формы где бы то ни было», но не назвал ЭТО исключение по
> имени — теперь названо. `W_PARAM_TYPE_POS_MUT` (лайт-предупреждение)
> заменён на жёсткую ошибку `E_PARAM_TYPE_POS_MUT_RETRACTED` (parser-level,
> без exemption по типу параметра). Канон mut-параметров остаётся ПРЕФИКСНОЙ
> формой `mut <имя> <Тип>` — но теперь БЕЗ альтернативной постфиксной
> записи вообще, ни голой, ни с `ro`. `nv-coding-style.md §18б` переписан в
> том же слиянии — «полный текст канона» больше не показывает постфикс как
> легитимный.
>
**Status:** CLOSED 2026-06-16 (Plan 152.7.1, commits `a313926b` + `3d0e30fa`).
**Depends on:** [D183](#d183-canonical-comparison-protocols--default-method-bodies-plan-918a) (`Display`/`Debug` протоколы),
[D229](#d229) (`Debug` протокол), Plan 137 (protocol naming convention).
**Amends:** D183 (`@display` sig), D229 (`@debug` sig). **Breaking:** да.

### Решение

Вводится тонкий sink-протокол `Write`, от которого зависят `Display` и `Debug`:

```nova
protocol Write {
    mut @write_str(s str) -> ()
}

protocol Display {
    @display(mut w Write) -> ()
}

protocol Debug {
    @debug(mut w Write) -> ()
}
```

`StringBuilder` реализует `Write` через `@write_str`, делегируя в `@buf.append(s)`:

```nova
fn StringBuilder @write_str(s str) -> () {
    @buf.append(s)
}
```

### Кодогенерация (статическая мономорфизация)

`Write` в C-кодогене **статически мономорфизируется** в `Nova_StringBuilder*` — vtable
отсутствует, нулевая overhead. Интерполяция `"${x}"` всегда монтирует `StringBuilder`
как единственный sink-тип на данном этапе.

- `type_ref_to_c`: `"Write"` → `"Nova_StringBuilder*"`
- `extract_protocol_type_name`: bypass для `"Write"` → не эмитит `E7201`

### Мигрированные реализации

Все встроенные `@display`/`@debug` impl переведены с `sb StringBuilder` на `w Write`:
`int`, `f64`, `f32`, `bool`, `char`, `str` (в `std/prelude/protocols.nv`),
`Vec[T]` (в `std/collections/vec/protocols.nv`), auto-derive синтезируемые методы.

### Прецеденты

- **Rust:** `fmt::Write` (`write_str`), `Display::fmt(&self, f: &mut Formatter)` — не в `String`.
- **Go:** `io.Writer` (`Write([]byte)`) — `Fprintf`/`Fprintln` принимают `io.Writer`.
- **Java:** `Appendable` (`append(CharSequence)`) — базовый sink для `StringBuilder`/`Writer`.

### Связь

- [D183](#d183-canonical-comparison-protocols--default-method-bodies-plan-918a) — `Display` протокол (amended: `sb`→`w Write`).
- D229 — `Debug` протокол (amended: `sb`→`w Write`).
- Plan 152.7.1 — план; Plan 137 — источник `Display`/`Debug`; `StringBuilder` — единственный concrete-sink в codegen.
- `[M-152.7-write-sink]` — CLOSED 2026-06-16.

---

## D259. Конструктор-конвенция `Vec[T]` — `of` для литерала, `from` для конверсии (Plan 153.1)

**Status:** ACTIVE (Plan 153.1, формализована 2026-06-14). **Depends on:**
[D232](#d232-vect--nova-native-generic-growable-array) (`Vec[T]`),
[D239](#d239-t--синтаксический-псевдоним-vect) (`[]T ≡ Vec[T]`).

> **AMEND (2026-07-06, of-guard) — пустой `of()` запрещён контрактом.** Я закрываю
> footgun, который ниже сам же и называю «идиоматичнее `new()`, но можно»: пустой
> вызов вариадика `Vec[T].of()` (0 аргументов) теперь **запрещён контрактом**
> `requires args.len() > 0` на `Vec[T].of` (`std/collections/vec/core.nv`). Пустой
> вектор строится ТОЛЬКО через `Vec[T].new()` — двух равнозначных путей для одного
> и того же значения больше нет. Нарушение — runtime-panic (`requires failed:
> args.len() > 0`); фикстура — `std/collections/vec/neg/vec_of_empty_panic.nv`.
> Мигрированы все прежние вызовы `.of()` без аргументов на `.new()`
> (`std/http/server/server.nv`, `std/http/server/wire.nv`,
> `nova_tests/http_decompress/decompress_test.nv`); тесты, нарочно проверявшие
> легальность пустого `of()` (`spec_tests/conformance/d259_vec_of_vs_from.nv`,
> `nova_tests/plan153_0/variadic_of.nv`), приведены в соответствие. Правило
> «КАНОН» ниже устарело в части `Vec[int].of()` — актуальный текст только в этом
> амендменте.

> **AMEND (2026-07-20, Plan 200 П16) — `Vec[T].from(items []T)` ПОЛНОСТЬЮ
> РЕТРАКТИРОВАН.** Владелец: «это же просто `items.clone()`» (согласовано
> 2026-07-16, подтверждено 2026-07-20). Разделение ролей `of`/`from` ниже
> частично устарело: роль «литерал» (`of`) остаётся канон без изменений;
> роль «конверсия существующей коллекции» (`from`) закрывается напрямую через
> `Clone` (D230, deep/recursive) — `existing.clone()` — а НЕ отдельным
> статик-конструктором:
> - **same-`T` конверсия** (`Vec[int].from(other_vec)`) → **`other_vec.clone()`**
>   (Vec/HashMap/Set уже реализуют глубокий поэлементный `Clone`);
> - **литерал** (`Vec[f32].from([1.5, 2.5])`) → канон уже был `of` — теперь
>   единственный путь: **`Vec[f32].of(1.5, 2.5)`**;
> - **width/типо-конверсия** (`Vec[u8].from(int_vec)`, тихо сужающая элементы)
>   → явный поэлементный цикл (`for x in src { out.push(x as u8) }`) — сужение
>   больше не прячется за одним вызовом.
> Декларация удалена из `std/collections/vec/core.nv`; все живые вызовы
> мигрированы (`[M-lint-findings-static-conversion]`-часть про `Vec.from`
> закрыта). Текст «### Правило»/«### Почему» ниже — ИСТОРИЯ (объясняет, почему
> `of` вообще появился рядом с `from`), актуальное поведение — только в этом
> амендменте.

### Что

Два конструктора `Vec[T]` несут **разные роли**, и их нельзя путать:

- **`Vec[T].of(a, b, c)`** (вариадик) — построить вектор из **литерального списка
  элементов**. Аналог Rust `vec![a, b, c]`.
- **`Vec[T].from(coll)`** (`from(items []T)`) — **конвертировать существующую
  коллекцию / слайс** в новый `Vec`. Аналог Rust `Vec::from(iter)` — это `clone`-подобная
  копия.

### Правило

```nova
// КАНОН
Vec[int].of(1, 2, 3)          // литерал элементов → of (1 аллокация)
Vec[int].new()                // пустой вектор — ЕДИНСТВЕННЫЙ путь (см. AMEND выше)
Vec[int].from(existing_vec)   // конверсия существующей коллекции → from
Vec[u8].of(1, 2, 3)           // of сужает так же, как from (args []T)

// АНТИ-ПАТТЕРН
Vec[int].of()                 // ❌ contract violation (AMEND 2026-07-06): requires
                              //    args.len() > 0 — пустой of() запрещён, → new()
Vec[int].from([1, 2, 3])      // ❌ избыточно: под D239 литерал [1,2,3] УЖЕ Vec[int],
                              //    поэтому from его КОПИРУЕТ во второй буфер (2 аллокации).
                              //    `of(1,2,3)` берёт элементы напрямую (1 аллокация).
Vec[int].from([])             // ❌ → Vec[int].new()
```

- **`from([литерал])` — избыточный clone.** Под [D239](#d239-t--синтаксический-псевдоним-vect)
  массив-литерал `[1, 2, 3]` сам по себе **уже** `Vec[int]` (одна аллокация в точке
  литерала). `from` копирует его во второй буфер ⇒ две аллокации ради того же результата,
  что `of(1, 2, 3)` даёт за одну. Поэтому `from([литерал])` — анти-паттерн; doc-comment
  `from` (в `std/collections/vec/core.nv`) направляет на `of`.
- **`from` — ТОЛЬКО для конверсии существующей коллекции** (`from(переменная)` /
  `from(другой_слайс)`). Это легитимный `clone`-подобный путь и НЕ анти-паттерн.
- **Когда тип выводится из контекста — просто `[a, b, c]`** (литерал = `Vec[T]`),
  без `of`/`from`. `of`/`from` нужны лишь для inline-указания типа (return-position,
  generic-контекст, когда контекст не выводит элементный тип).

### Почему

Cost-transparency (D135): идиома, которая выглядит как «построить вектор из этих
элементов», должна стоить ровно одну аллокацию. `of(...)` это даёт; `from([...])` —
скрыто удваивает. Разделение ролей убирает footgun и читается как Rust (`vec![]` vs
`Vec::from(iter)`), Kotlin (`listOf` vs `toList`), Swift (`[a,b]` vs `Array(seq)`).

### Связь

- [D239](#d239-t--синтаксический-псевдоним-vect) — `[]T ≡ Vec[T]`; именно поэтому
  литерал уже вектор и `from([литерал])` избыточен.
- [D232](#d232-vect--nova-native-generic-growable-array) — `Vec[T]`-тип и его конструкторы.
- Миграция тестов/stdlib `from([литерал])` → `of(...)` — Plan 153.1 sweep
  (`[M-153.1-of-vs-from-sweep]`).

---

## D260. Ленивый итератор `Vec[T]` — boxed-fluent адаптеры (Plan 153.2)

> **AMEND (2026-07-06, решение владельца): терминатор `@nth(n)` РЕТРАКТИРОВАН** (вместе с
> `CharsIter @nth`) — тождествен `skip(n).next()` и провоцирует индексные привычки на
> итераторах (скрытый O(n)-«индекс», в циклах O(n²)). Канонический общий набор адаптеров:
> `skip`/`take`/`step_by`/`enumerate`/`filter`/`chain` + терминаторы `count`/`collect`/
> `fold`/`find`/... Миграция ~57 мест — волной `[M-d73-d77-retraction-migration]`
> (переписывание на целевую итерацию, НЕ механику `skip(n).next()`).

**Status:** ACTIVE (Plan 153.2 Phase A, 2026-06-14). **Amended by
[D277](#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2)** (2026-06-15): `BoxIter[T]` помечен `value` →
wrapper-рекорд лоуэрится by-value (0 heap-аллокаций обёртки, Stage 1); добавлен
allocation-free generic-over-source sibling `collections.vec_iter_zc` (Stage 2).
**Depends on:**
[D232](#d232-vect--nova-native-generic-growable-array) (`Vec[T]`),
[D239](#d239-t--синтаксический-псевдоним-vect) (`[]T ≡ Vec[T]`),
[D58](03-syntax.md) (`Iter`/`Next` — `VecIter`). **Закрывает:**
[Q-iterator-laziness](../open-questions.md), [Q-iter-mut](../open-questions.md) (Phase A).

### Решение

Ленивый итератор `Vec[T]` реализован по **boxed-fluent**-модели. Канон лени — этос
cost-transparency (D135): цепочка `v.lazy().map(f).filter(p).collect()` **не делает
промежуточных аллокаций**; каждый адаптер оборачивает upstream-`step`-замыкание и тянет
по одному элементу на запрос; цепочку приводит в движение только терминатор.

```nova
type BoxIter[T] { priv step fn() -> Option[T] }      // boxed-курсор
fn Vec[T] @lazy() -> BoxIter[T]                       // вход (мост VecIter→BoxIter)
fn BoxIter[T] @map[U](f fn(T) -> U) -> BoxIter[U]     // адаптер → новый BoxIter
fn BoxIter[T] mut @collect() -> Vec[T]               // терминатор драйвит цепочку
```

- **`BoxIter[T]`** держит единственный `step`-thunk: `Some(x)` (следующий элемент) /
  `None` (исчерпан). Адаптеры строят новые `BoxIter` обёрткой `step`; **ничего не
  выполняется**, пока терминатор не потянет.
- **Вход** `v.lazy()` мостит `VecIter[T]`→`BoxIter[T]` (free-fn `box_iter[T]`,
  захватывающий курсор). `[]T` тождественно `Vec[T]` (D239) → `lazy()` есть и на слайсе.
- **Адаптеры** (lazy, возвращают новый `BoxIter`, без аллокации): `map`/`filter`/
  `filter_map`/`enumerate`/`take`/`skip` (Phase A). Каждый копирует receiver
  (`mut src = @`) в свежее захватывающее замыкание → цепочка **реентерабельна** на
  терминатор-вызов и не мутирует BoxIter вызывающего, пока терминатор её не сдренит.
- **Терминаторы** (драйвят/коротят): `collect`/`fold`/`reduce`/`count`/`sum(zero T)`/
  `any`/`all`/`find`/`for_each`/`min`/`max`/`last` (Phase A; `nth` — ретрактирован, см.
  AMEND выше). `min`/`max` — на
  `[T Compare]`; `sum(zero T)` — аддитивная идентичность вместо числового протокола.

### Модульное размещение

`BoxIter`/адаптеры/терминаторы — в sibling **FILE-модуле**
[`std/collections/vec_lazy.nv`](../../std/collections/vec_lazy.nv) (`module
collections.vec_lazy`), доступном через `import std.collections.vec_lazy`, **НЕ** внутри
prelude folder-модуля `collections.vec`. Причина та же, что у eager `vec_seq` (D239
status-note): prelude-global generic-type-метод с CLOSURE-телом утекает свои method-level
generics (`[U]`/`[Acc]`) и callback-параметры (`f`/`pred`) в merged-body КАЖДОГО юнита →
коллизия с top-level `fn f`/`type Acc` ([M-codegen-var-types-fn-scope] + D145). Адаптеры
closure-dense → opt-in import.

### Eager `vec_seq` сосуществует

Eager `collections.vec_seq` (`v.map(f) -> new Vec`, материализует каждый шаг) **оставлен
без изменений** как переходный eager-surface. Lazy — канонический allocation-free путь
(Q-iterator-laziness); оба за раздельными explicit-import (eager НЕ переписан в сахар над
lazy, чтобы не навязывать lazy-import eager-пользователям).

### Codegen-инварианты (обязательны для мономорфизации closure-несущих методов)

Реализация потребовала фиксов в `compiler-codegen/src/codegen/emit_c.rs` (без них —
silent CC-FAIL / drain-0 / segfault), зафиксированных как контракт:

1. **mut-capture box-реестр (`var_boxed`) флашится per-test** (`emit_test`) — box
   `_box_<name>` не должен утекать между C-функциями тестов.
2. **`Stmt::Return` эмитит значение с типом возврата функции как target** — голый
   `return None` в mono-замыкании резолвится в `NovaOpt_<mono>`, не erased
   `NovaOpt_nova_int`.
3. **`infer_expr_c_type` регистрирует generic-инстанс типа-возврата**, когда generic
   free-fn ИЛИ метод generic-типа возвращает generic-инстанс (`box_vec[int](it) ->
   BoxIter[int]`, `Vec[T] @lazy() -> BoxIter[T]`) — иначе `.method()` на временном
   промахивается мимо generic-instance dispatch-path (block 5b) и попадает в erased
   NULL-stub.

(Лифт mono×closures — register_generic_instances_in_typeref + closure-capture в loop-arms,
commit `996ca01a`.)

### Phase B — ✅ ЗАКРЫТА (2026-06-16, амендмент D260)

**Критерии приёмки Phase B (все выполнены):**
- G-B1. `zip` возвращает правильные пары; останавливается на более коротком операнде.
- G-B2. `flat_map` корректно обрабатывает пустой внешний итератор, пустые внутренние
  итераторы и смешанные пустые/непустые внутренние.
- G-B3. Все адаптеры компилируются через C-codegen без CC-FAIL — без упрощений как для прода.
- G-B4. 0 новых регрессий по blast-radius (plan153_0/138/139/147/165/91_12).

**Реализовано:**
- ✅ **`step_by(n int)`** (BoxIter, `vec_lazy.nv`) + zero-cost `StepByIter[I,T]` (`vec_iter.nv`): yield каждый n-й элемент. Contract `n > 0` (requires). Тест: `plan153_2/phase_b_lazy` + `plan153_2_zc/step_by_zc`.
- ✅ **`chain(other BoxIter[T])`** (BoxIter): дренирует self, затем other. Тест: `plan153_2/phase_b_lazy`.
- ✅ **`zip(other BoxIter[B])`** (BoxIter): возвращает `BoxIter[(A,B)]`, останавливается когда
  любой из операндов исчерпан. Codegen-фикс: receiver typevar alias `A` (`fn BoxIter[A] @zip[B]`)
  теперь биндится в `type_subst` при dispatch — tuple return `(A,B)` резолвируется в mono
  `_NovaTuple_2_8_nova_int_8_nova_int` вместо erased `_NovaTuple2`.
  Тесты: `plan153_2/zip_basic` (9 pos), `plan153_2/zip_neg` (3 neg), `plan153_2/zip_min`.
- ✅ **`flat_map(f fn(T)->BoxIter[U])`** (BoxIter): дренирует каждый inner-итератор, возвращённый
  `f`. Codegen-фикс: `NovaOpt` typedef для `BoxIter[T]` payload (NovaValue_ by-value) теперь
  эмитируется ПОСЛЕ generic struct body через `novaopt_vr_typedefs_buf`.
  Тесты: `plan153_2/flat_map_basic` (7 pos), `plan153_2/flat_map_neg` (4 neg).

**Остаток Phase B (не реализован):**
`unzip`/`flatten`/`scan`/`inspect`/`take_while`/`skip_while`/`peekable`/
`min_by[_key]`/`max_by[_key]`/`partition`/`chunk_by`/`into_iter`;
мут-итерация `for mut x`/`mut @iter()` (Q-iter-mut write-through — отдельный путь).

**collect-target FromIterator — ✅ ЗАКРЫТ (Plan 153.6 / [D264](#d264-vec-протоколы-hash--fromiterator--collect-target-plan-1536)):**
`@collect()->Vec` (default) + `@collect_set()->Set` (терминаторы) + `from`/`from_iter`/`@extend`
(прочие таргеты/источники); статический generic-конструктор + tuple-`@collect_map` gated
(`[M-153.6-collect-static-generic]`/`[M-153.6-collect-map-tuple-receiver]`).
Zero-cost generic-over-source — **реализован Stage 2, см. [D277](#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2)** (`[M-153.2-generic-over-source-zerocost]` → 🟡 PARTIAL).
Tuple-PRESERVING-адаптер сразу после `enumerate` — `[M-153.2-tuple-elem-adapter]`
(residual `Option[<mono-tuple>]` closure-typing gap; схлопнуть tuple через `map`).

### Связь

- [D232](#d232-vect--nova-native-generic-growable-array) — `Vec[T]`; [D239](#d239-t--синтаксический-псевдоним-vect) — `[]T ≡ Vec[T]`.
- [D58](03-syntax.md) — `Iter`/`Next` (`VecIter` — upstream-источник для `lazy()`).
- [D135](01-philosophy.md) — cost-transparency (no hidden O(n)) — обоснование лени.
- [Q-iterator-laziness](../open-questions.md) — закрыта (lazy = канон).
- [Q-iter-mut](../open-questions.md) — Phase A закрывает терминаторами/адаптерами; мут-итерация — Phase B.
- Plan 153.2 — план; `vec_seq.nv` / `vec_lazy.nv` — реализация.

## D264. Vec-протоколы: `Hash` + FromIterator / collect-target (Plan 153.6)

**Статус:** ✅ IMPLEMENTED — `Hash` (2026-06-13) + FromIterator/collect-target (2026-06-14).

`Vec[T]` дополняет набор протоколов (`Equal`/`Compare`/`Clone`/`Display`/`Debug` —
[D230 amend](#collections-vec--hashmap--set--element-wise-deep--conditional-bound))
двумя возможностями: **content-`Hash`** и **FromIterator / collect-target** (мост к
ленивому слою D260). Оба — под conditional-bound на `T` (Rust `impl<T: Hash> Hash for
Vec<T>` / `impl<T> FromIterator<T> for Vec<T>`).

### `Vec[T Hash] @hash() -> u64`

Order- и length-sensitive content-hash (`protocols.nv`): FNV-1a (64-bit), сворачивает
длину + per-element `@hash()` (`h = (h ^ x) * prime`). Consistency с `@equal`: равные
Vec (равная длина + element-wise `==`) → равный hash (контракт `Hash`+`Equal`). u64-mul
**врапается** (Nova-семантика = FNV mixing-шаг); offset-basis — **hex**-литерал (десятичная
форма > `i64::MAX`). Делает `Vec[T: Hash]` сам `Hash` → элемент `HashSet`. (Вторая,
equality-половина ключ-контракта `HashMap` — `[M-153.6-vec-hashmap-key-eq]`, pre-existing
generic-key-dispatch gap; `@hash` готов.)

### FromIterator / collect-target

Nova **структурно**-типизирует итераторы ([D58](03-syntax.md): любой `mut @next() ->
Option[T]` итерируем; `Next[T]`/`Iter[I]` — протоколы), поэтому FromIterator — **НЕ**
отдельный enforced-протокол с одним методом, а **набор конструкторов/терминаторов**,
строящих коллекцию из любого итератора. Канон:

1. **Default collect-target → `Vec`** (ленивый слой D260): `BoxIter[T] mut @collect() ->
   Vec[T]` — материализует pipeline в один проход, без промежуточного `Vec` на стадию.
2. **`Set` collect-target:** `BoxIter[T Hash] mut @collect_set() -> Set[T]` (dedup; Rust
   `iter.collect::<HashSet<_>>()`). Allocation-free над pipeline (pull + insert на лету).
3. **Прочие таргеты — композицией над собранным `Vec`:** `Set[T].from_iter(it.collect())`,
   `HashMap[K, V].from(pairs.collect())`. `Set.from_iter([]T)` и `HashMap.from([](K,V))`
   уже принимают собранный `Vec` (под D239 `[]T ≡ Vec[T]`).
4. **FromIterator из произвольного `Iter`-источника (без ленивой стадии):**
   `Vec[T].new().extend(src)` — instance-метод `@extend[S Iter[T]]` (`mutate.nv`)
   мономорфизируется корректно для любого `S` (Range/VecIter/Vec). Прямой call-site
   идиом — НЕ требует обёртки.

### Gated (compiler-gaps, не упрощение)

- **`[M-153.6-collect-static-generic]`** — *статический* generic-конструктор
  `Vec[T].from_iter[S Iter[T]](src S)` с for-in по `S` в теле **не компилируется**: bound
  `S Iter[T]` не резолвится для for-in dispatch внутри **static** generic-метода (typevar
  остаётся `Nova_S`). Тот же класс, что generic-method-dispatch-collapse (`@cap`/`@splice`).
  Instance-`@extend` (#4) — рабочий обход. NEG-фикстура `collect_static_generic_neg`
  лочит границу (`EXPECT_COMPILE_ERROR for-in: type 'S'`).
- **`[M-153.6-collect-map-tuple-receiver]`** — прямой терминатор `BoxIter[(K, V)] mut
  @collect_map() -> HashMap[K, V]` **не парсится**: receiver type-аргумент кортежем
  (`BoxIter[(K, V)]`) отвергается парсером (`expected identifier, got '('`). HashMap
  collect-target остаётся `HashMap.from(pipeline.collect())` (#3).

### Зачем структурный набор, а не enforced-протокол

Один enforced `FromIterator[T]`-протокол с методом-конструктором потребовал бы
static-generic-method-dispatch (gated, см. выше). Структурный набор — это паритет Rust
(`collect`/`FromIterator`/`extend`) при существующей инфре: `@collect`/`@collect_set`
(терминаторы), `from`/`from_iter` (конструкторы из собранного), `@extend` (из источника).
Cost-transparency сохраняется: ленивый путь без промежуточных аллокаций (D260), материализация
именами `collect*`/`from*`/`extend`.

### Связь

- [D260](#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532) — ленивый слой;
  `@collect`/`@collect_set` — его терминаторы.
- [D58](03-syntax.md) — `Iter`/`Next` структурный duck-typing (основа FromIterator).
- [D239](#d239-t--синтаксический-псевдоним-vect) — `[]T ≡ Vec[T]` (собранный `Vec` = `[]T`-аргумент `from_iter`).
- [D230 amend](#collections-vec--hashmap--set--element-wise-deep--conditional-bound) — conditional-bound протоколы коллекций.
- Plan 153.6 — план; `vec_lazy.nv` (`@collect_set`) / `protocols.nv` (`@hash`) / `set.nv` (`from_iter`) — реализация.
---

## D277. By-value мономорфизация generic value-records + generic-over-source zero-cost адаптеры (Plan 153.2 Ф.2)

**Status:** ACTIVE (Plan 153.2 Stage 1 + Stage 2 + Stage 3 + Stage 4,
2026-06-14/15). **Amends:** [D228](#d228) (value-record allocation contract —
распространён на **generic** `type X[T] value {…}`),
[D260](#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532)
(lazy-итератор — добавлен allocation-free sibling-слой). **Зависит от:**
[D226](#d226) (always-pointer receiver ABI), [D123](#d123-tuple-monomorphization)/
[D354](#d354-generic-anonymous-tuple-monomorphization) (mono-инфраструктура).
**Маркеры:** `[M-153.2-generic-over-source-zerocost]` → 🟡 PARTIAL (Stage 2 done),
`[M-153.2-Z-closure-devirt]` → 🟡 PARTIAL (Stage 3 alloc-elimination done),
`[M-153.2-Z-noalloc-terminator]` → ✅ DONE (Stage 4),
`[M-153.2-closure-as-mono-type]` (P3 остаток — call-инлайн).

### Контекст

[D228](#d228) дал by-value стек-codegen для **не-generic** `value`-рекордов (6
языковых типов: `str` + 5). [D260](#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532)
зашипил lazy-итератор по **boxed-fluent**-модели: `BoxIter[T]` — единый erased
курсор, держащий `step`-thunk; адаптеры не аллоцируют **промежуточный Vec**, но
сам wrapper-рекорд `BoxIter[T]` (как любой generic-рекорд) лоуэрился в **heap**
(`nova_alloc(sizeof(Nova_BoxIter…))`, один на адаптер), плюс per-element fn-ptr
индирекция через `step()`. Решение закрывает обе статьи накладных расходов в две
композируемые стадии.

### Stage 1 — by-value мономорфизация generic value-records

`type X[T] value {…}` теперь лоуэрится **BY VALUE** для КАЖДОЙ конкретной
mono-инстанции, зеркаля не-generic value-record-путь ([D228](#d228)) и str-путь
([D26](08-runtime.md)). Mono-инстанс `X[int]` = inline-struct `NovaValue_<short>`
(`<short>` = sanitized mono-имя), **передаётся/возвращается/копируется по
значению**, **без `nova_alloc` для wrapper-рекорда**. Receiver-ABI — always-pointer
([D226](#d226)): `NovaValue_<short>*` на стек-слот (`&obj` / hoist+`&temp`
на call-site через `prepare_method_recv`), как у не-generic value-record.
`BoxIter[T]` помечен `value` ([`std/collections/vec_lazy.nv`](../../std/collections/vec_lazy.nv)) —
тот же fluent API D260, но теперь **0 wrapper-аллокаций**.

**Codegen-контракт (обязателен — gate на `AllocKind::Value`, heap-generics не
тронуты):**

1. **`receiver_c_type`** value-generic-mono receiver → `NovaValue_<short>*`,
   order-free относительно `type_aliases` (helpers `value_generic_mono_short` /
   `value_aware_generic_c_type`).
2. **generic-instance method-dispatch (block 5b)** маршрутит value-receiver
   через `prepare_method_recv` → передача по адресу.
3. **fn-typed-field вызов** `(@step)()` — accessor `.` vs `->` выбирается по
   value-ness receiver'а (`(*nova_self).step`).
4. **return-type inference** (`infer_mono_method_ret_with_args` + dispatch 5b +
   overload-pool rt-strip) снимает `NovaValue_` ПЕРЕД `Nova_`, чтобы
   `Nova_<rt>`-lookup в реестре попал (иначе method-level-generic-цепочка
   `.map[U]` коллапсировала в `int`).

### Stage 2 — generic-over-source zero-cost адаптеры

Новый **sibling FILE-модуль** [`std/collections/vec_iter_zc.nv`](../../std/collections/vec_iter_zc.nv)
(`module collections.vec_iter_zc`, opt-in import — тот же D145/leak-rationale, что
у `vec_lazy`/`vec_seq`). Rust-style `Map<I,F>`/`Filter<I,P>`: каждый адаптер —
**generic-over-source** `value`-рекорд (`MapIter[I,T,U]` / `FilterIter[I,T]` /
`FilterMapIter[I,T,U]`), держащий upstream-итератор **INLINE** полем `src I` (НЕ
boxed `step`-замыкание). `@next()` диспетчит `(@src).next()` **статически,
мономорфизованно**. Цепочка `v.ziter().zmap(f).zfilter(p).zcollect()`
мономорфизуется в ОДИН вложенный конкретный тип
`FilterIter[MapIter[VecIter[int],int,int],int]`; каждый `next()` инлайнится до
базового `VecIter.next()`. Вход — `Vec[T] @ziter()`; адаптеры zmap/zfilter/
zfilter_map; per-type терминаторы zcollect/zfold/zcount/zsum/zfor_each/zany/zall/
zfind.

**Дополнительные codegen-фиксы (все gate на `AllocKind::Value`):**

1. **`value_aware_subst_to_ref`** (новый `&self`-зеркало статического
   `apply_type_subst_to_ref`): nested-generic-арг несёт `NovaValue_`-префикс →
   worklist-enqueued mono-имя СОГЛАСУЕТСЯ с `type_ref_to_c`/field/type-decl именем
   (иначе две расходящиеся инстанции → undefined-struct CC-FAIL).
2. **`split_top_level_mono_args` + `mono_type_args_of`** (registry-backed,
   depth-aware): наивный `args_str.split("__")` рвал nested generic-over-source
   арг на 3 фрагмента, мис-биндя `I`/`T` → Vec[nova_int*] garbage. Применено в 3
   split-сайтах.
3. **`erased_type_ref_c`** — type-param-чек сделан РЕКУРСИВНЫМ
   (`uses_any_type_param`) → erased-stub возвращает erased-base-pointer, не
   placeholder-laden mono-имя.
4. **`drain_generic_type_worklist`** placeholder-guard (value-GATED
   `mangled_has_nested_placeholder`) — skip эмита value-mono, чьё by-value поле
   ссылалось бы на undefined inner-placeholder struct, БЕЗ подавления нужного heap
   `Vec[Slot[K,V]]` forward-typedef. **Регресс-урок:** over-eager ранняя версия
   guard'а (НЕ value-gated) сломала 15 HashMap/value-record файлов (plan139 t3/
   neg_t3, plan152_4 case/normalize/graphemes/sentences/words+conformance,
   plan152_5 collation) — пойман baseline-бинарём @`0da18125`, FIXED гейтингом на
   value-шаблон; все 15 восстановлены.

### Stage 3 — devirtualizация capture-free замыканий (alloc-elimination)

Замыкание БЕЗ свободных переменных (env = `{int _dummy}`) **stateless**: каждый
инстанс байт-идентичен и тело-функция никогда не читает env. Вместо ДВУХ
`nova_alloc` на каждый call-site (env-box + `NovaClos_xx`-box) эмитится ОДИН
**file-scope static singleton** (`nova_lambda_N_clos_singleton` +
`nova_lambda_N_env_singleton`) на closure-литерал, а call-site возвращает
`(void*)(&singleton)`. Хирургическая правка — `emit_lambda`
([`compiler-codegen/src/codegen/emit_c.rs`](../../compiler-codegen/src/codegen/emit_c.rs) ~31427),
capture-free fast-path. **Соундно безусловно:** static-адрес immortal — может
escape/store/outlive любой scope без dangling (Boehm видит его как root).
Захватывающие замыкания (`free_vars ≠ ∅`) — heap-путь БЕЗ изменений (immutable
by-value snapshot + mut by-ref box нужны per-instance, singleton нельзя шарить).
Это **alloc-elimination** половина closure-devirt'а: сам per-element ВЫЗОВ
`(@f)(x)` ещё идёт через `NOVA_CLOS_CALL` fn-ptr-макрос — true call-devirt =
закладка env как конкретного type-param (`MapIter[I,T,U,F]`,
`[M-153.2-closure-as-mono-type]`). Маркер `[M-153.2-Z-closure-devirt]` (P3,
PARTIAL).

### Stage 4 — alloc-free терминаторы + `collect_into`

В `vec_iter_zc` добавлен терминатор `mut @zcollect_into(out mut Vec[T]) -> ()` на
каждый адаптер (`MapIter`/`FilterIter`/`FilterMapIter`): тело = `zcollect`-drain
МИНУС `Vec[U].new()` header-аллокация — пушит в **переданный** буфер `out`.
**Семантика APPEND** (НЕ чистит `out`; для свежего sink caller делает
`out.clear()` — `len=0`, буфер сохранён → амортизированный 0 аллокаций при
переиспользовании). Возвращает `()` (буфер виден через caller-биндинг — `Vec[T]`
heap-ref). Стриминг-терминаторы (`zfold`/`zsum`/`zcount`/`zfor_each`/`zany`/
`zall`/`zfind`) уже alloc-free по конструкции (скаляр/bool/Option-аккумулятор, без
out-Vec). Маркер `[M-153.2-Z-noalloc-terminator]` (✅ DONE).

### Дизайн-решение: sibling, не замена

`vec_iter_zc` — **НОВЫЙ sibling-модуль**, boxed-fluent `vec_lazy`/`BoxIter`
**сохранён** как closure-fluent-альтернатива (единый erased курсор, единообразный
`BoxIter[T]` на каждой стадии). `vec_iter_zc` — allocation-free сиблинг (один
вложенный mono-тип на цепочку). Оба за раздельными explicit-import. Задокументировано
в шапке `vec_iter_zc.nv`.

### Измерено (канон `v.lazy/ziter().map().filter().collect()`)

| | boxed-fluent `vec_lazy` | zero-cost `vec_iter_zc` |
|---|---|---|
| wrapper-record heap allocs (адаптер-цепочка) | **6 → 0** (Stage 1: by-value `NovaValue_<short>`) | 0 |
| source-box (`_box_src`) | 9 | **0** (source inline полем `src I`) |
| per-element `step()` fn-ptr индирекция | есть | **убрана** (статический dispatch) |
| capture-free closure env/box (Stage 3) | **4→0** (collect) / **6→0** (fold) — static singleton | то же |
| терминатор-тело (Stage 4) | — | **0 `nova_alloc`** (`collect_into`/`fold`/`sum`/`count`/`for_each`/`any`/`all`/`find`) |
| остаточный heap | env ЗАХВАТЫВАЮЩИХ замыкания `f`/`pred` + `VecIter` source-курсор | то же — irreducible без closures-as-mono-types |

Stage 1 verify: `grep nova_alloc(sizeof(Nova_BoxIter` = **0** во всех
сгенерённых `plan153_2/*.c`; `NovaValue_BoxIter…` by-value struct — повсюду
(adapters 89 / chains 98 / laziness 64 / terminators 74 вхождений).

Stage 3 verify: `nova_alloc(sizeof(nova_lambda_N_env))` /
`nova_alloc(sizeof(NovaClos…))` для capture-free замыканий = **0** (заменены
file-scope static singleton); канон `zmap(f).zfilter(p).zcollect()` driver-тело
closure-allocs **4 → 0**, та же цепочка `.zfold(0,…)` **6 → 0**.

Stage 4 verify: все четыре мономорфизованных `…method_zcollect_into` тела = **0
`nova_alloc`** (vs `zcollect` с `…_static_new()`); `zfold`/`zsum`/`zcount`/
`zfor_each`/`zany`/`zall`/`zfind` мономорфизованные тела = **0 `nova_alloc`**
каждый.

### Остаток (честно)

- **per-element ВЫЗОВ `f`/`pred` ещё fn-ptr-индирекция** (`void*` +
  `NOVA_CLOS_CALL` на элемент) — Stage 3 убрал АЛЛОКАЦИЮ closure-env (capture-free
  → singleton), но не сам вызов. Rust-style инлайн мэппера требует
  **closures-as-mono-types** (env как конкретный type-param) — отдельный крупный
  лифт. `[M-153.2-closure-as-mono-type]` (P3).
- **захватывающие замыкания** всё ещё heap-env (per-instance; singleton нельзя
  шарить — by-value snapshot / by-ref box нужны свежими).
- **`VecIter` source-курсор** — heap-ref-type alloc на `.ziter()` (свойство
  `VecIter[T]`, не замыкание; вне scope ступеней 3–4).
- **`take`/`skip`/`enumerate`** (stateful / tuple-element) остаются на boxed
  `vec_lazy` — порт = wiring, не новая compiler-способность.

### Связь

- [D228](#d228) — value-record allocation contract (распространён на generic).
- [D260](#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532) — boxed-fluent lazy (0 wrapper-allocs via Stage 1; zero-cost sibling via Stage 2; capture-free closure devirt via Stage 3; alloc-free терминаторы + `collect_into` via Stage 4).
- [D226](#d226) — always-pointer receiver ABI (mono value-receiver).
- Plan 153.2 — план; `vec_lazy.nv` / `vec_iter_zc.nv` — реализация; `emit_c.rs::emit_lambda` — Stage 3 singleton.
- Cross-ref: [D355](#d355--blanket-protocol-receiver-methods-plan-161-2026-06-15) — blanket methods on `Next[T]` implementors (терминаторы blanket).

---

## D281. Module-level field privacy — `type X priv { … }` (Plan 160)

> **Status:** ACTIVE (2026-06-15, Plan 160). **Amends:** [D220](#d220-per-field-visibility--priv-keyword--type-level-default-flip) — расширяет type-level privacy-flip двумя уровнями. **Зависит от:** [D78](07-modules.md#d78) (module-path convention), [D29](07-modules.md#d29) (module model).

### Мотивация

D220 дал `priv` (field-level, только own-type) и `type X priv {...}` (type-level flip, тоже only-own-type). Недоставало среднего уровня: **module-private** — поле видно во всём модуле (паттерн Go unexported + Rust `pub(crate)`). Позволяет folder-module или multi-file-module иметь shared internal state без публичного поля и без экстра accessor-boilerplate.

### Синтаксис

```nova
// type-level modifier         → meaning (задаёт дефолт для полей без явного modifier'а)
// (bare)                      → fields default public      (D47, unchanged)
// priv                        → fields default module-private
// priv(type)                  → fields default type-private (only own methods)
// priv(file)                  → fields default file-private (only methods in same file)

export type Job value priv {      // module-private by default
    mut id   int                  // module-private (наследует type-level default)
    kind     int                  // module-private
    priv(type) secret int         // type-private (stronger: only Job methods)
    priv(file) internal int       // file-private (только методы в этом файле)
}
```

`priv` без квалификатора = **module-private** — на уровне типа (задаёт дефолт полей) и на уровне поля (задаёт видимость явно).  
`priv(type)` = **type-private** — аналогично: на уровне типа (дефолт) и на уровне поля (явно).  
`priv(file)` = **file-private** — аналогично: на уровне типа (дефолт) и на уровне поля (явно). Поле доступно только из методов, определённых в том же файле.  
Правило симметрично: все `priv`-квалификаторы ведут себя **одинаково** на type-уровне и field-уровне.  
`priv(module)` — **ОШИБКА** (`E_PRIV_QUALIFIER`); используй `priv` без квалификатора.

### Правило

#### §1 Effective visibility (четыре уровня)

| Контекст | Effective visibility |
|---|---|
| Explicit `pub` field | public |
| Explicit `priv` field | **module-private** |
| Explicit `priv(type)` field | type-private |
| Explicit `priv(file)` field | **file-private** |
| Type-level `priv(file)` default, no explicit field modifier | **file-private** |
| Type-level `priv(type)` default, no explicit field modifier | type-private |
| Type-level `priv` default, no explicit field modifier | **module-private** |
| (ничего — D47 default) | public |

Лесенка строгости: public ⊃ module-private ⊃ **file-private** ⊃ type-private.

#### §2 Module identity

Модуль = папка (Nova module model, D29/D78). Файл принадлежит модулю `P.Q` если его rev-3 path = `[P, Q]`. Все файлы в `project/src/foo/` декларируют `module project.foo` — они один модуль.

**module_priv_access_allowed(type T):** true iff `current_module == type_defining_module(T)`.

#### §3 Access rules

Module-private field (из type-level `priv` default, без explicit field `priv`):
- **Same module:** read / write / init / pattern — РАЗРЕШЕНЫ.
- **Other module:** read → `E_FIELD_MODULE_PRIVATE`, write → `E_FIELD_MODULE_PRIVATE`, init → `E_FIELD_MODULE_PRIVATE`, pattern → `E_FIELD_MODULE_PRIVATE`.

Type-private field (explicit `priv(type)` field, OR type-level `priv(type)` default):
- **Same module, non-method:** read → `E_PRIV_FIELD_READ` (D220 error codes unchanged).
- **Own-type method:** РАЗРЕШЁН — включая cross-instance: `fn T @eq(other T) -> bool => @f == other.f` читает `other.f` внутри метода `T` (см. D220 §3).

#### §4 Error codes

| Code | Context |
|---|---|
| `E_FIELD_MODULE_PRIVATE` | Read/write/init/pattern of module-private field from outside module |
| `E_PRIV_FIELD_READ` | Read of type-private field from non-method (includes same-module free fn) |
| `E_PRIV_FIELD_WRITE` | Write of type-private field (D220, reused) |
| `E_PRIV_FIELD_INIT` | Record-literal init of type-private field (D220, reused) |
| `E_PRIV_FIELD_PATTERN` | Pattern destructure of type-private field (D220, reused) |
| `E_PRIV_QUALIFIER` | `priv(module)` or unknown qualifier — use bare `priv` |

#### §5 Критерии приёмки (без упрощений, как для прода)

1. **Позитив:** `type T priv { f int }` в модуле M — свободные функции, методы и record-literal constructors в том же модуле читают/пишут/инициализируют `f` без ошибок.
2. **Негатив-read:** import T из другого модуля, `t.f` → `E_FIELD_MODULE_PRIVATE`.
3. **Негатив-write:** `t.f = x` из другого модуля → `E_FIELD_MODULE_PRIVATE`.
4. **Негатив-init:** `{ f: v }` конструктор из другого модуля → `E_FIELD_MODULE_PRIVATE`.
5. **Layering:** `priv(type)` field внутри `type T priv {...}` остаётся type-private — `E_PRIV_FIELD_READ` даже в том же модуле из свободной функции. Bare `priv` field внутри `type T priv {...}` = module-private (accessible in same module free fn).
6. **Public export:** type + его методы (возвращающие/принимающие T) публично экспортируются — клиентский модуль может использовать API без доступа к внутренним полям.
7. **Regression:** `nova test` core-suite без новых FAIL.
8. **Негатив-pattern:** `ro { f } = t` из другого модуля → `E_FIELD_MODULE_PRIVATE` (не `E_PRIV_FIELD_PATTERN`). **Позитив-pattern:** `ro { f } = t` в том же модуле — работает без ошибок.

### AST / Checker-реализация

- `ast::FieldDefaultVisibility::Module` — новый вариант enum (наряду с `Public`, `Private`).
- `RecordField.priv_module_field: bool` — true если поле получило module-private (из type-level default ИЛИ из explicit `priv` field modifier).
- `TypeCheckCtx.type_defining_modules: HashMap<String, Vec<String>>` — строится из `peer_files.items_here`.
- `TypeCheckCtx.current_module: RefCell<Vec<String>>` — RAII `CurrentModuleGuard` при входе в `check_module`.
- `module_priv_access_allowed(tname)` — compare maps.
- 5 check-сайтов: INIT (record-literal), READ (member expr), WRITE (assign), PATTERN (destructure) — каждый distinguishes `priv_module_field` → `E_FIELD_MODULE_PRIVATE` vs `priv_field` → old D220 codes.

### Связь

- [D220](#d220-per-field-visibility--priv-keyword--type-level-default-flip) — per-field `priv` + type-level flip (D281 amends: `priv` = module-private, `priv(type)` = type-private).
- [D78](07-modules.md#d78) — module-path enforcement (defines module identity).
- [D47](07-modules.md#d47) — default-public baseline (unchanged).
- Plan 160 — план реализации.

## D307. File-private visibility — `priv(file)` (Plan 170)

> **Status:** ACTIVE (2026-06-19, Plan 170). **Зависит от:** [D281](#d281-module-level-field-privacy--type-x-priv---plan-160) / [D220](#d220-per-field-visibility--priv-keyword--type-level-default-flip) (инфраструктура `priv`/`priv(type)`), [D29](07-modules.md#d29) (folder-module model), [D78](07-modules.md#d78) (module-path). **Нумерация:** план назвал блок «D304», но к моменту реализации D304 уже был занят (Test Category Selectors, 09-tooling.md, Plan 169.1.1, 2026-06-19); D305/D306 были временно зарезервированы за proposed-планом 104.10 (LSP), но при реализации 104.10 получил **D378-D380** (09-tooling.md) → **D305/D306 СВОБОДНЫ**; этому блоку присвоен **D307**.

### Мотивация

folder-module = один compile unit из co-equal peer-файлов (D29/D78): все `.nv` в папке с одинаковым `module X` делят одно top-level пространство имён — `fn`/`type`/`const` каждого файла видны всем peer-файлам. Текущая лесенка видимости top-level **бинарна**:

- `export` — виден снаружи модуля;
- (без модификатора) — module-private (виден всем peer-файлам модуля).

> ⚠️ **АМЕНДМЕНТ 2026-08-16 (решение владельца; реестр №699): ФАЙЛ `*_test.nv`
> НЕ ИМЕЕТ ЭКСПОРТНОЙ ПОВЕРХНОСТИ.** Его top-level декларации видны своим
> peer-файлам (Rule C не меняется — он остаётся co-equal членом модуля и
> компилируется вместе с ним), но НЕ входят в то, что видит импортёр модуля,
> даже будучи помеченными `export`. **Зачем:** без этого правила тип,
> написанный ДЛЯ ПРОВЕРКИ модуля, становится декларацией модуля для КАЖДОГО
> потребителя транзитивно. Живой случай: `type Node` из
> `std/src/encoding/serde/tagging_test.nv` сделал имя `Node` коллидирующим
> (D381) для любого CU, тянущего serde, и через квалификацию C-имени привёл к
> мискомпиляции без диагностики (реестр №696); то же `Token` ↔
> `encoding.json.Token`. **Почему правило, а не переименование:** переименовать
> `Node` — фикс носителя, класс вернётся на следующем совпадении имени
> (`Point`, `User`, `Span`…). Граница «поставляемое / проверочное» существует
> в конвенции («тесты рядом с модулем») и не была выражена в модели модуля —
> модель не знала слова «тест». **Прецедент:** rustc `#[cfg(test)] mod tests`
> — тестовый код не виден потребителю по построению. **Приёмка (№699):**
> pos-фикстура «потребитель объявляет свой `Node`, импортирует модуль с
> `*_test.nv`-пиром, объявившим `Node`, — компилируется без D381-квалификации»;
> плюс проверка, что сам `*_test.nv` по-прежнему видит peer-декларации.

Недоставало **самого узкого** уровня — file-private. Без него одноимённые helper-функции/типы в разных файлах одного folder-module **конфликтуют** (`E_DUP_DEFINITION`) и требуют некрасивого ordinal-rename (`helper1`/`helper2`). `priv(file)` закрывает дыру: символ виден **только в своём файле**, не утекает к peer-файлам. Аналог Rust `pub(self)`. Польза шире тестов — любой folder-module (`std/collections/vec/` и т.п.) получает file-local helper'ы без загрязнения общего namespace.

### Синтаксис

```nova
priv(file) type Acc { … }       // тип виден только в этом файле (prefix-форма)
type Job priv(file) { … }       // эквивалент — priv(file) как type-modifier после имени
priv(file) fn helper() -> int   // free fn не виден peer-файлам модуля
priv(file) const K = 42         // file-local константа
// (без модификатора)           // module-private (как сейчас, D281)
export     fn api() …           // публичный (как сейчас)
```

Для `type`: `priv(file)` допустим **и как prefix** (`priv(file) type X`), **и как modifier после имени** (`type X priv(file) { … }`) — обе формы эквивалентны, выбор стилевой.
Для `fn` и `const`: только prefix-форма.

Лесенка видимости top-level символов: **`priv(file)` ⊂ (module-default) ⊂ `export`**.

`priv(file)` применим на **двух уровнях**:
- **Top-level символ** (`priv(file) type X`, `priv(file) fn f`, `priv(file) const K`) — символ не виден peer-файлам модуля.
- **Поле типа** (`priv(file) secret int`) — поле доступно только из методов, определённых в том же файле. Симметрично `priv` (module) и `priv(type)` (D281).

`priv(file)` — это **visibility-hint, НЕ смена module-резолва**: модуль остаётся один (D29 не нарушается), символ лишь помечен «не виден peer-файлам». `file` — НЕ ключевое слово (распознаётся как `Ident("file")` внутри `priv(...)`), что исключает коллизию с идентификаторами.

**Нейминг-обоснование** (`priv(file)`, не `local`): единая ось видимости под `priv` + scope-квалификатор — симметрично `priv(type)` (D281). `local` двусмысленно (вложенные функции — тоже «локальные») и потребовало бы нового KW с риском коллизии идентификаторов.

**Применимость top-level.** `fn` / `type` / `const`. Не применим к `test`/`bench`/`lemma`/`let`/`ro` и к методам (`fn T @m` — receiver-qualified, вне scope file-private). Scope-local `const` внутри тела fn — уже block-scoped, `priv(file)` не нужен.

### Правило

#### §1 Резолв (checker)

При построении группового namespace folder-module из shared-набора **исключаются** `priv(file)`-имена каждого peer-файла. Для файла `F` видимый набор имён = `(shared group namespace без любых file-private) ∪ (собственные file-private имена F)`. Поэтому sibling-файл **никогда** не резолвит чужой `priv(file)` символ.

Если файл `F` ссылается на имя, которое `priv(file)` в ДРУГОМ peer-файле той же группы (и иначе не резолвится) → специфичная диагностика `E_FILE_PRIV_LEAK` (вместо родового «undefined identifier»), с подсказкой «remove priv(file) … or move the symbol».

#### §2 Дедупликация (no-conflict)

Два `priv(file)` символа с **одинаковым** именем в **разных** peer-файлах — **НЕ конфликт** (непересекающиеся file-scope). Проверка «duplicate top-level name» для такого имени снимается **тогда и только тогда**, когда **каждая** декларация этого имени file-private И они в разных файлах. Если хотя бы одна декларация module-private/export — имя живёт в общем namespace → коллизия как раньше (`E_DUP_DEFINITION`, D29).

#### §3 Codegen (mangling)

`priv(file)` free-fn получает **file-дискриминированное** C-имя — `nova_fn_<module>_f<file_id>_<name>` — где дискриминатор = стабильный `file_id` объявляющего файла. Два одноимённых `priv(file) fn helper` из разных файлов дают **разные** C-символы → нет коллизии линковки. Call-site внутри файла резолвит в свой вариант по тому же `(file_id, name)` ключу, что и checker §1. File-private free-fns НЕ регистрируются в shared overload-registry (file-local, не участвуют в cross-file overload resolution). Точный паттерн переиспользует существующий `private_const_c_names` (Plan 160, уже keyed по `file_id`).

#### §4 Error codes

| Code | Context |
|---|---|
| `E_FILE_PRIV_LEAK` | Ссылка из peer-файла на `priv(file)` символ другого файла группы |
| `E_PRIV_QUALIFIER` | `priv(file)` + `export` вместе; bare top-level `priv` без `(file)`; `priv(<other>)` на top-level; `priv(file)` перед не-`fn`/`type`/`const` |

#### §5 Критерии приёмки (без упрощений, как для прода)

1. **Парсинг:** `priv(file) fn`/`type`/`const` парсится; `priv(file)` + `export` → `E_PRIV_QUALIFIER`.
2. **Позитив own-file:** `priv(file) fn h()` вызывается в своём файле → компилируется и работает.
3. **Позитив co-exist:** два peer-файла, оба объявляют `priv(file) fn helper()` с разным телом, каждый вызывает свой → компилируются (file-discriminated codegen, нет линк-коллизии) и работают.
4. **Негатив leak:** ссылка из peer на чужой `priv(file)` → `E_FILE_PRIV_LEAK`.
5. **Регрессия:** module-private (`priv`/default) символ из одного peer ВИДЕН другому (D281 без изменений); `export` без изменений.
6. **Регресс:** plan160 / plan124* / modules / std — 0 новых FAIL.

### AST / Checker / Codegen-реализация

- `ast`: `FnDecl.file_private: bool` / `TypeDecl.file_private: bool` / `ConstDecl.file_private: bool` (default `false`). Минимальный путь — `bool` (без enum-рефактора `is_export`).
- `parser`: распознавание `priv` `(` `file` `)` в общем parse-item ДО `KwExport`-eat; пробрасывается в `parse_fn`/`parse_type_decl`/`parse_const_decl`. Взаимоисключение с `export`.
- `types`: file-private dedup в `check_module_impl` (снятие dup-проверки для disjoint file-scope); `NameResCtx.file_priv_leak: HashMap<FileId, HashMap<name, owner_path>>` — per-file leak-table; групповой namespace вычитает file-private каждого peer.
- `codegen`: `file_priv_fn_c_names: HashMap<(FileId, String), String>` + `current_emit_file_id` — file-дискриминированный C-символ, резолвится в `free_fn_c_name`/`mangle_fn`.

### Связь

- [D281](#d281-module-level-field-privacy--type-x-priv---plan-160) — module-private (соседний уровень лесенки); этот блок добавляет узкий file-private.
- [D29](07-modules.md#d29) — folder-module model (не нарушается: модуль остаётся один).
- [D78](07-modules.md#d78) — module-path / file identity (file_id источник дискриминатора).
- Plan 170 — план реализации.

---

## D355 — Blanket protocol-receiver methods (Plan 161, 2026-06-15)

> **Renumber 2026-07-03:** блок был **D282** — номер коллидировал с [D282 extern "nova"/"C" fn](08-runtime.md#d282) (двух-ABI FFI, Plan 91.12; тот сохраняет D282 — канон README-индекса, амендится Plan 174.6). Перенумерован в D355 (Plan 174 §6).

**Status:** ACTIVE (Plan 161 Ф.0-Ф.4, 2026-06-15). **Amends:** [D241](03-syntax.md#d241) (добавлен §3 «≤1 impl, cross-ref D355 (ex-D282)»). **Зависит от:** [D72](#d72-generic-bounds-через-t-protocol--protocol-как-тип) (generic bounds), [D119](#d119-method-level-type-parameters-в-generic-methods) (method-level type params), [D277](#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2) (D277/vec_iter_zc). **Маркеры:** `[M-161-blanket-receiver]` → ✅ Ф.0-Ф.4 CLOSED; `[M-161-parametric-return]` (V2 followup).

**§1 Синтаксис.** `fn[I Proto[T₁,…,Tₙ]] I @name[U₁,…](params) -> R { body }` — blanket-объявление: `I` — typevar-ресивер, `Proto[…]` — bound. `T₁,…,Tₙ` выводятся из bound (не нужно объявлять явно). Запись `fn[…]` (glued, без пробела) = prefixed-generic header (уже разобрана парсером).

**§2 Диспетч.** При вызове `expr.name(args)`, где тип `expr` — конкретный `C`, реализующий `Proto[…]`: blanket-метод виден на `C`, typevar `I` биндится в `C`. Конкретный метод (`method_table[C]`) всегда имеет приоритет над blanket.

**§3 Мономорфизация.** Mono-key = `(C, name, extra_type_args)`. Тело компилируется как обычный generic-метод с заполненным `I`. Внутри тела `I` = конкретный `C`, typevar'ы из bound (`T`) = конкретные типы из impl-записи `C`.

**§4 Инвариант (≤1 impl).** Тип не может реализовывать `Next[T]` для двух разных `T` одновременно. Нарушение = `E_DUPLICATE_PROTOCOL_IMPL`.

**§5 Область действия.** Blanket-метод виден в модуле где объявлен + его importers (те же правила видимости, что у обычных методов). Конфликт двух blanket-методов с одним именем на одном протоколе = `E_BLANKET_CONFLICT`.

**§6 Ограничения V1.** Работает для методов с конкретными или fully-resolved возвращаемыми типами. Параметрические возвращаемые типы `T`, `Option[T]`, `Vec[T]` — V2 (`[M-161-parametric-return]`). Один bound-уровень (`I Proto[…]`); цепные bounds — V2. Ресивер должен быть typevar, не конкретный тип.

**Реализовано в.** `std/collections/vec_iter_zc.nv`: `@zfold`, `@zcount`, `@zfor_each`, `@zany`, `@zall` — blanket на `Next[T]` (5 терминаторов, concrete return type). Перевод 12 per-adapter → 5 blanket деклараций (O(N²) → O(N)).

## D284 — EnumerateIter — zero-cost enumerate adapter (Plan 162)

**Status:** ACTIVE (Plan 162 Ф.0-Ф.5, 2026-06-16). **Зависит от:** [D277](#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2) (value-record mono / generic-over-source), [D355](#d355--blanket-protocol-receiver-methods-plan-161-2026-06-15) (blanket protocol-receiver для терминаторов, ex-D282), [D260](#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532) (lazy-iterator layer). **Маркер:** `[M-153.2-enumerate-zc]` → ✅ CLOSED Plan 162; `[M-153.2-tuple-elem-adapter]` OPEN (chained adapter сразу после enumerate гейтнут на closure-type propagation). **Было:** `[M-153.2-enumerate-zc]` (enumerate deferred из Plan 153.2, было в boxed `vec_lazy`).

**§1 Синтаксис.** `EnumerateIter[I, T]` — zero-cost value-record adapter:
```nova
export type EnumerateIter[I, T] value { mut src I, mut i int }
```
Поля: `src I` — источник (inline by-value, статически диспетчится), `i int` — текущий индекс (стартует с 0). Результат `@next()` — `Option[(int, T)]`; `Some((i, elem))` на каждый `Some` у источника, `None` транзитивно. Реализует `Next[(int, T)]` → совместим с blanket-терминаторами D355 (`@zcount`, `@zfold`, `@zfor_each`, `@zany`, `@zall`, `@zfind`, `@zsum`).

**§2 Диспетч (`@zenumerate`).** Метод `@zenumerate()` объявлен **per-type** (не blanket), потому что возвращаемый тип явно называет `EnumerateIter` (не параметрический конкретный тип в смысле D355 §6):
```nova
export fn VecIter[T]          @zenumerate() -> EnumerateIter[Self, T]         => { src: @, i: 0 }
export fn MapIter[I, T, U]    @zenumerate() -> EnumerateIter[Self, U]         => { src: @, i: 0 }
export fn FilterIter[I, T]    @zenumerate() -> EnumerateIter[Self, T]         => { src: @, i: 0 }
export fn FilterMapIter[I,T,U]@zenumerate() -> EnumerateIter[Self, U]         => { src: @, i: 0 }
export fn TakeIter[I, T]      @zenumerate() -> EnumerateIter[Self, T]         => { src: @, i: 0 }
export fn SkipIter[I, T]      @zenumerate() -> EnumerateIter[Self, T]         => { src: @, i: 0 }
```
`Self` — mono-ресивер (D66 §«Self как вложенный generic type-arg»). Каждый adapter-тип регистрирует собственный `@zenumerate`. Не может быть blanket, потому что тело конструирует `EnumerateIter[Self, _]` — return-тип содержит конкретное имя адаптера, не generic typevar, раскрываемый из протокола-bound.

**§3 Мономорфизация (tuple parametric return).** `@next()` возвращает `Option[(int, T)]`. Компилятор разрешает тип кортежа (plan162 fix, `emit_c.rs`): перед вызовом `type_ref_to_c` для `Option[(int, T)]` биндинги type-variable из protocol-bound (`I Next[T]` → T = elem-тип источника) устанавливаются в `type_subst_overrides`. Arm `Tuple` в `type_ref_to_c` получает `T` уже разрешённым → эмитит типизированный mono'd struct (`NovaTuple_2_8_nova_int_11__nova_int` для `int`), не erased `_NovaTuple2`. Без этого фикса T оставался нераспознанным → fallback на erased legacy-форму → CC-FAIL при использовании `.0`/`.1` полей.

**§4 Инвариант (счётчик i).** `i` стартует с 0. Инкрементируется ровно на 1 при каждом `Some`-ответе источника (пропуски на `None` не считаются — `EnumerateIter.@next()` прозрачно прокидывает `None`, не инкрементируя). Это соответствует `enumerate()` в Rust/Python: индекс = порядковый номер доставленного элемента, непрерывно с 0.

**§5 Ограничения V1.** (a) Нет blanket `@zenumerate` — return-тип называет `EnumerateIter` конкретно (contra D355 §6). Добавление нового adapter-типа требует явного `@zenumerate`. (b) Tuple-PRESERVING chained adapter сразу после `enumerate` (`enumerate().filter(..)` когда элемент остаётся `(int,T)`) гейтнут на closure-type-propagation codegen fix → `[M-153.2-tuple-elem-adapter]`; workaround: `enumerate().map(|p| ...)` (Map схлопывает кортеж). (c) `EnumerateIter` был deferred из Plan 153.2 (маркер `[M-153.2-enumerate-zc]` в boxed `vec_lazy`) — зашиплен в Plan 162.

**Кросс-ссылки:** [D355](#d355--blanket-protocol-receiver-methods-plan-161-2026-06-15) (blanket-receiver, гейтирует терминаторы на EnumerateIter), [D260](#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532) (boxed lazy layer — предшественник), [D277](#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2) (generic-over-source model — база для EnumerateIter).

**Реализовано в.** `std/collections/vec_iter_zc.nv`: `EnumerateIter[I,T]` value-record + `@next()` + chaining methods + 6 per-type `@zenumerate()` adapters. `compiler-codegen/src/codegen/emit_c.rs`: tuple parametric T-subst fix в blanket `infer_type_refs_for_blanket`. Тесты: `nova_tests/plan162/` (9 basic + 8 chain + neg). D284 NEW.

## D290 — Value-record iterator types (Plan 165, 2026-06-16)

**Status:** ACTIVE (Plan 165, 2026-06-16). **Зависит от:** [D277](#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2) (generic value-record mono), [D215](#d215-named-tuple-fields--valuereference-allocation-contract) (value/reference allocation contract), [D228](#d228) (fiber-arena GC root coverage для value-record с GC-pointer полями). **Маркеры:** `[M-codegen-value-type-generic-forward-decl]` ✅ CLOSED для `VecIter`/`Range*Iter` (Plan 165 Ф.1).

**§1 Правило.** Iterator-тип объявляется как `value` (stack-allocated), если выполняется **одно из** условий:

(a) Тип содержит только примитивные поля (`int`, `u8`, `bool`, `f64`, и т.д.) без GC-managed указателей → stack-аллокация безопасна без каких-либо оговорок.

(b) Тип содержит GC-pointer-поля, но является **cursor'ом** (итерационным состоянием над уже существующей структурой данных), не новым владельцем памяти → fiber arena автоматически корнирует value-record со ссылочными полями (D228 §escape), поэтому GC-безопасность сохраняется.

**§2 Типы (Plan 165).** Применено к:

- `VecIter[T] value` (`std/collections/vec/iter.nv`) — содержит `Vec[T]` (GC-pointer-ссылка); cursor-тип, правило (b). Stack-slot + fiber-arena root.
- `Range value`, `RangeIter value`, `StepRangeIter value`, `ReverseRangeIter value` (`std/collections/range.nv`) — содержат только `int`-поля; правило (a). Чистый stack, zero GC involvement.

**§3 Эффект на производительность.** `for x in v { }` компилируется в `VecIter[T]` на стеке — malloc отсутствует. Цепочка `0..n` (`Range` → `RangeIter`) — два `int64_t` на стеке. Adapter-цепочка zero-cost iterators (Plan 153.2 / D260 / D277) остаётся нулём аллокаций при нулевой escape (стековый инлайнинг адаптеров).

**§4 Codegen инварианты (Ф.1 fix, коммит `1f92f106`).** Generic value-тип `type X[T] value { … }` при мономорфизации:

- Forward declaration должна содержать полное mono-имя: `typedef struct NovaValue_X____nova_int NovaValue_X____nova_int;` (не `NovaValue_X`).
- Несоответствие forward-declaration и struct definition → CC-FAIL «incomplete return type». Исправлено: `emit_forward_decl_for_generic_value_type` передаёт mono-имя в полной форме.
- `field_cache.rs` предикат примитивного листа включает `"never"` (строчные).

**§5 Конвенция для новых iterator-типов.** При добавлении нового iterator-адаптера (`type FooIter[…] { … }`):

1. Если поля — только примитивы или ссылки на **уже живущие** GC-объекты (cursor-semantics) → объявить `value`.
2. Если адаптер **создаёт** новую GC-память (например, буферизующий адаптер) → оставить heap-record.
3. Проверить, что forward declaration генерируется с полным mono-именем (см. §4).

**Реализовано в.** `std/collections/vec/iter.nv` (VecIter value), `std/collections/range.nv` (Range/RangeIter/StepRangeIter/ReverseRangeIter value), `compiler-codegen/src/codegen/emit_c.rs` (generic value forward-decl fix), `compiler-codegen/src/field_cache.rs` (`"never"` лист).

---

## D364 — Net C FFI pattern: opaque handles + value-record wrapping (Plan 91.12)

**Source:** Plan 91.12 Ф.0–Ф.4, 2026-06-16. **Status:** ✅ ACTIVE.
**Связь:** [D214](02-types.md#d214), [D282](08-runtime.md#d282-new--extern-nova-fn--extern-c-fn--двух-abi-синтаксис-для-ffi-plan-9112-ф-1) (`extern "C" fn`), [D365](04-effects.md#d365).

### Паттерн

Стандартный способ обёртки C-библиотечного ресурса в Nova (используется в `std/net`, типичен для `std.ffi`-слоя):

**Шаг 1. Opaque handle** (D214 §newtype): `type CX(*())` — newtype над `*()` (void*). Даёт типобезопасность на FFI-границе без раскрытия внутренней C-структуры. Nova не может перепутать `CTcpListener` и `CUdpSocket`.

```nova
type CTcpListener(*())
type CTcpStream(*())
type CUdpSocket(*())
```

**Шаг 2. Приватный FFI-слой** (`ffi.nv`, без `export`): все `extern "C" fn` — module-private. Именование: `<resource>_<action>` (snake_case, без Nova-префиксов). C-side принимает typed handle pointer, возвращает новый handle (NULL = error) или числовой результат.

```nova
extern "C" fn tcp_listener_bind(addr CSocketAddr) -> CTcpListener  // NULL on error
extern "C" fn tcp_stream_write(s CTcpStream, data str) -> int       // bytes or -1
```

**Шаг 3. Публичный value-record**: `export type TcpListener value { priv handle CTcpListener }`. `priv` делает `handle` module-private (Plan 160 D281). Публичные методы делегируют в эффект (не в C напрямую).

```nova
export type TcpListener value { priv handle CTcpListener }
export fn TcpListener.bind(addr SocketAddr) TcpNet -> Result[TcpListener, NetError] {
    TcpNet.bind(addr)
}
```

**Шаг 4. Эффект-handler**: `real_tcp_net()` — конкретный `Effect[TcpNet]` (D365), содержит прямые вызовы C FFI. Тесты заменяют handler на mock без изменения кода пользователя.

### Инварианты

- FFI-функции НЕ экспортируются из модуля (`export` запрещён на `extern "C" fn` в `ffi.nv`-слое).
- Конструктор public-типа через `_from_raw(h CX)` — package-private factory.
- Опасный `close()` — `consume @close()`: потребляет тип, предотвращает double-close.
- Ошибки: null/отрицательный результат + TLS `net_last_error()` (cooperative-safe: нет yielding между C-вызовом и чтением ошибки).

### Реализовано в

`std/net/ffi.nv` (Ф.1), `std/net/tcp.nv` (Ф.3), `std/net/udp.nv` (Ф.4). Тесты: `nova_tests/plan91_12/` (19/19 PASS). D364 NEW (ex-D292 — renumber 2026-07-03, коллизия с ModuleSigTable-D292 07-modules).

---

## D299 — `AsSlice[T]` protocol: contiguous-buffer abstraction (Plan 153.1, 2026-06-17)

**Source:** Plan 153.1 `[M-153.1-append-extend-consolidation]`, 2026-06-17. **Status:** ✅ ACTIVE.
**Связь:** [D141](02-types.md#d141) (`@extend` / bulk-copy family), [D238](02-types.md#d238) (`@index` protocol).

### Мотивация

`Vec[T] @append(other Vec[T])` принимал только конкретный `Vec[T]`. Чтобы `@append` мог принять и slice-view (`[]T` = тот же тип `Vec[T]` с интерьерным указателем), и любой пользовательский contiguous-буфер, вводится protocol `AsSlice[T]` с двумя методами: `@as_ptr() -> *T` и `@len() -> int`.

### Спецификация

```nova
// std/prelude/protocols.nv
export type AsSlice[T] protocol {
    @as_ptr() -> *T
    @len() -> int
}
```

`Vec[T]` реализует `AsSlice[T]` через `#impl(AsSlice[T])` на `@as_ptr()`. Это покрывает и `[]T` (алиас `Vec[T]`), включая slice-views (`v[a..b]`).

`@append` переписан с конкретного аргумента на generic bound:

```nova
// std/collections/vec/mutate.nv
export fn Vec[T] mut @append[S AsSlice[T]](other S) -> @ {
    ro m = other.len()
    if m > 0 {
        @reserve(m)
        unsafe {
            RawMem.copy(other.as_ptr() as *u8, (@data + @len) as *mut u8, m * size_of[T]())
        }
        @len = @len + m
    }
    @
}
```

`RawMem.copy` (memmove) корректен для self-append: после `@reserve` регионы `[0, m)` и `[@len, @len+m)` не пересекаются.

### Правила

- Реализующий тип должен гарантировать, что `@as_ptr()` возвращает указатель на непрерывный буфер из не менее `@len()` живых элементов.
- Дереференс `@as_ptr()` вне `unsafe { }` запрещён (как у `Vec[T] @as_ptr`).
- `@extend` (итерация через `Iter`) остаётся для не-contiguous источников.

### Реализовано в

`std/prelude/protocols.nv` (protocol declaration), `std/collections/vec/access.nv` (`#impl(AsSlice[T])` на `Vec[T] @as_ptr`), `std/collections/vec/mutate.nv` (обновлённый `@append`). Тесты: `nova_tests/plan153_1/append_as_slice.nv` (6 кейсов: vec→vec, пустые случаи, self-append, slice view). D299 NEW.

## D300 — Vec generic forward-decl: body-site scan + tuple-elem fwd-decl (Plan 168, 2026-06-17)

**Проблема:** `Vec[u32]` в теле функции (локальная переменная, TurboFish-конструктор)
генерировал C-тип `Nova_Vec____Nova_u32_p` (через generic stub path),
тогда как в сигнатурах и полях тот же тип даёт `Nova_Vec____uint32_t`.
Pre-pass `collect_array_elem_typerefs` не заходил в тела функций →
`typedef struct Nova_Vec____Nova_u32_p Nova_Vec____Nova_u32_p;` отсутствовал в
глобальном preamble → CC-FAIL «unknown type name» в tuple typedefs.

**Два исправления:**

1. **Body scan** (`emit_c.rs`): добавлены `collect_array_elem_typerefs_in_fnbody`,
   `collect_array_elem_typerefs_in_block`, `collect_array_elem_typerefs_in_stmt`,
   `collect_array_elem_typerefs_in_expr`. `scan_item` для `Item::Fn` теперь вызывает
   `collect_array_elem_typerefs_in_fnbody(&f.body, acc)`.
   При нахождении `ExprKind::TurboFish { base: Ident("Vec"), type_args }` — напрямую
   добавляет type_args как Vec elem TypeRefs.

2. **Tuple-elem fwd-decl** (`emit_c.rs`, строки ~3915): перед splice `MONO_TUPLE_TYPEDEFS`
   проходит по всем mono'd tuple instances, и для каждого pointer-field вида `Nova_...__...*`
   (mono'd instance = содержит `__`) добавляет `typedef struct X X;` в начало `tuple_decls`.
   Это обеспечивает forward-decl для `Nova_Vec____Nova_u32_p` и любых аналогичных типов,
   которые появляются в tuple field-types до своего полного struct-определения.

**Результат:** `nova_tests/plan168` 2/2 PASS; `nova_tests/plan153_1` 8/9 PASS
(1 pre-existing CODEGEN-FAIL `resize_with_free_fn_shadow` — не связан с fix'ом).

**Инварианты:**
- `Nova_Vec____<elem>` — полная struct-definition эмитируется в `generic_type_defs_buf`
  (до fn-definitions, via marker splice)
- `typedef struct Nova_Vec____<elem> Nova_Vec____<elem>;` — в `user_type_fwd_decls`
  (до tuple typedefs, via marker splice)
- Tuple typedef может ссылаться на `Nova_Vec____<elem>*` как incomplete pointer — OK по C99

D300 NEW.

---

## D373 — Generic array API: sort/min/max/binary_search + _by variants (Plan 91.8c, 2026-06-17)

**Статус:** ACTIVE.

### Мотивация

Plan 91.3 давал `[]int @sort()` (concrete), Plan 91.8a закрепил `Compare.compare -> int`.
После 91.8a `str` и user-types с `@compare` должны быть sortable через generic dispatch.
D373 добавляет полный generic API поверх Compare.

### API surface

#### Bound-based (T Compare)

```nova
fn[T Compare] []T mut @sort_of() -> @
fn[T Compare] []T @min_of() -> Option[T]
fn[T Compare] []T @max_of() -> Option[T]
fn[T Compare] []T @binary_search_of(target T) -> Option[int]
```

Суффикс `_of` избегает name collision с concrete `[]int @sort()` / `@min()` / `@max()`
(те сохранены как fast-path для int; codegen выбирает concrete для `[]int` exact-receiver).

#### Callback-based (без bound на T)

```nova
fn[T] []T mut @sort_by_of(cmp fn(T, T) -> int) -> @
fn[T] []T @min_by_of(cmp fn(T, T) -> int) -> Option[T]
fn[T] []T @max_by_of(cmp fn(T, T) -> int) -> Option[T]
```

`_by_of` variants не требуют Compare — порядок задаётся callback'ом.

#### Utility (без bound на T)

```nova
fn[T] []T mut @reverse_of() -> @
fn[T] []T @position_of(pred fn(T) -> bool) -> Option[int]
fn[T] []T @count_of(pred fn(T) -> bool) -> int
fn[T] []T @find_of(pred fn(T) -> bool) -> Option[T]
```

### Алгоритмы

- **sort_of / sort_by_of:** stable insertion sort, O(n²). Подходит для массивов до ~1000
  элементов. Followup `[M-91.8c-pdq-sort]` для pdq/intro-sort на крупных данных.
- **binary_search_of:** classic O(log n) binary search; requires pre-sorted array.
  Returns `Option[int]` (index если найдено, None если нет).
  *Отличие от Vec canonical `@binary_search -> Result[int,int]` (D239 / vec/access.nv):
  тот возвращает `Ok(idx)` / `Err(insertion_point)` — более информативно; sort.nv
  вариант — legacy `Option` form для backward compat.*
- **min_of / max_of:** linear scan O(n), None для empty.
- **min_by_of / max_by_of:** linear scan с callback, None для empty.

### Backward compat

Concrete `[]int @sort()` / `@sort_by()` / `@min()` / `@max()` в `std/sort.nv` сохранены.
Generic `sort_of` / `min_of` / `max_of` работают на любом `T Compare` включая `int` и `str`.
Overload resolution: конкретный receiver (`[]int`) выбирает concrete method, generic — `_of`.

### Связь

- D183 (Plan 91.8a) — Compare.compare -> int convention.
- D178 — str.compare via native `nova_str_compare`.
- D72 — generic bounds `[T Protocol]`.
- D239 — `[]T ≡ Vec[T]` alias.
- Plan 91.3 — concrete `[]int @sort`.

D373 NEW (ex-D185, renumber 2026-07-03).

### D373 §amend-1 — direct @[i].method() dispatch in generic array methods

**Дата:** 2026-06-17. **Закрывает:** [M-91.8c-direct-index-method].

Codegen fix: при вызове `@[j].compare(key)` внутри generic `fn[T Compare] []T`-тела
компилятор не мог вывести тип элемента для `SelfAccess`-объекта и пропускал dispatch.

**Изменения в `compiler-codegen/src/codegen/emit_c.rs`:**
1. `compute_array_elem_type_for_obj` — добавлен arm для `ExprKind::SelfAccess`
   (ранее только `ExprKind::Ident` и другие; `SelfAccess` без arm → элемент-тип не выводился).
2. `emit_monomorphized_method` — при входе в метод заполняет `array_element_types["nova_self"]`
   из receiver-типа mono-инстанса, делая тип элемента доступным при обработке вложенных
   `@[i].method()`-вызовов до того, как в теле появится explicit присвоение.

**Результат:** `@[j].compare(key)` в `fn[T Compare] []T`-телах (sort_of, binary_search_of и т.д.)
теперь диспетчится корректно без промежуточного binding'а.

### D373 §amend-2 — @sort_unstable* переведены с heapsort на pdqsort (Plan 153.3.1, 2026-06-18)

**Дата:** 2026-06-18. **Закрывает:** [M-153.3-sort-pdqsort] + [M-91.8c-pdq-sort].

`@sort_unstable` / `@sort_unstable_by` / `@sort_unstable_by_key` теперь вызывают `@_pdqsort` вместо `@_heapsort`.

**Алгоритм `@_pdqsort` (итеративный, без рекурсии — Nova-safe):**
- n ≤ 1 → return immediately (no work)
- n ≤ 16 → `@_ins_sort_range` (insertion sort, cache-warm)
- `stack.len ≥ depth_limit` (depth_limit = 2·ilog2(n)+2) → heapsort fallback на диапазон через temp Vec
- иначе: median-of-3 pivot (→ `@_median3_to_end`) + Lomuto partition + Vec[int] work-stack (lo/hi pairs)

**O(n log n) worst case, O(log n) stack space.** Heapsort сохранён как depth-guard fallback и для `@select_nth_unstable` (не удалён). Stable `@sort*` (merge sort) не тронут.

## D315. `ResolvedType` — единый канонический носитель типа (Plan 172.1, 2026-06-21)

**Статус:** ACTIVE. *Single source of type truth.* Реализует [`compiler-conventions.md`](../../docs/dev/compiler-conventions.md) §0. Supersedes
врезку M2 Plan 172.1 («`ResolvedType` достаточен как носитель» — спайк 2026-06-21 доказал обратное).

### Что

Тип в компиляторе имеет **ОДНО** каноническое представление — `ResolvedType`. Из него выводится **ВСЁ**:
проверки, совместимость типов, конверсии **и перевод в C**. Legacy-путь «`TypeRef` → C»
(`type_ref_to_c`, который резолвит И переводит в одном проходе) — **ретайрится**.

### Правило

- **`TypeRef` — только синтаксис** (выхлоп парсера). Может нести `Self`, свёрнутые алиасы,
  неразрешённые имена, generic-параметры. Это сырая, **неразрешённая** форма → **НЕ вход для
  перевода в C** (сперва его надо разрешить).
- **Семантический анализ разрешает тип ОДИН раз** в канонический `ResolvedType`: `Self`→приёмник,
  алиасы развёрнуты, имена→конкретные объявления, дженерики. **Каноничность:** семантически
  равные типы → структурно равные `ResolvedType` (иначе совместимость = сравнение типов ломается).
- **`ResolvedType` несёт ПОЛНУЮ семантическую личность:** разрешённая идентичность (а не
  `path.last()`), generic-аргументы, ширина/знак (`Scalar{width,signed,wide_default}`), эффекты,
  **все оси изменяемости** (L1 binding / L2 view / L3 pointee, [D246](#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee)),
  верность модификаторов указателя.
- **Сахар нормализуется прочь** (намеренно): имя алиаса, написание `Self`. Это не потеря — это и
  делает окно *единым*. Исходное написание, нужное для текста ошибки, — **отдельный
  диагностический канал**, не часть канонического типа.
- **ABI/бэкенд-факты — НЕ хранятся в типе, а ВЫВОДЯТСЯ.** Мангл-имя, erasure
  (`Option[void*]`→`NovaOpt_nova_int`), `NovaValue_`-префикс (D228), int64-слоты — это **решения
  лоуэринга**, не свойства типа. Их выводит `resolved_type_to_c` (живёт в codegen: финальная
  mono-подстановка + побочки эмиссии typedef'ов, но **БЕЗ повторного резолва** — резолв уже сделан
  чекером).
- **Один вывод (чекер), один лоуэринг (`resolved_type_to_c`).** `type_ref_to_c` ретайрится:
  объявленные типы (сигнатуры, поля) тоже идут через «разрешить → опустить».

### Почему

- **§0 (единый источник истины).** `type_ref_to_c` сегодня делает ДВЕ вещи: (1) резолвит —
  разворачивает алиасы, превращает `Self` в приёмник, подставляет mono; (2) переводит в C. Первая
  половина **дублирует чекер** — ровно §0-анти-паттерн «codegen re-derive». Разделение «разрешить
  один раз (чекер) → опустить (codegen)» убирает дубль.
- **Два окна правды неизбежно дрейфуют.** `ResolvedType` для проверок + `TypeRef`-driven перевод
  в C = тот самый класс багов `Vec[u32]`-мис-манглинга (`simple_type_ref_to_c` дрейфнул, пропустил
  u32). Одно каноническое окно убивает класс дрейфа в корне.
- **Спайк (2026-06-21)** доказал: текущий `ResolvedType` лосси для C — берёт `path.last()` (теряет
  модуль), схлопывает `*mut T` (форма `Pointer(Mut)`) в `TypedPtr(Ro,…)`, разворачивает L2
  `ro`. Значит прежнее допущение плана (M2: «носитель достаточен») неверно; D315 ставит целью
  **обогащение до lossless-canonical** + ретайр `type_ref_to_c`.

### Что отвергнуто

- **«`TypeRef` → C — нормальный лоуэринг».** Отвергнуто: `TypeRef` неразрешён (`Self`/алиас) →
  не вход для перевода, его надо сперва разрешить.
- **«Ничего не обогащать; реконструировать `TypeRef` из `ResolvedType` на лоуэринге».** Отвергнуто:
  лосси round-trip (выбрасываемый костыль); возвращает резолв-в-codegen — тот самый анти-паттерн.
- **«Второй лоуэринг `resolved_type_to_c` рядом с `type_ref_to_c`».** Отвергнуто: два лоуэринга =
  фрагментация, которую §0 запрещает; цель — ОДИН.
- **«Хранить ABI-факты в `ResolvedType`».** Отвергнуто: засоряет семантический тип бэкенд-заботами;
  ABI выводится, не хранится.

### Связь

- [`compiler-conventions.md`](../../docs/dev/compiler-conventions.md) §0 — D315 это его конкретная
  формулировка про носитель типа (§10 анти-паттерн «два окна правды»).
- [D246](#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee) — три оси мутабельности;
  `ResolvedType` обязан нести все три.
- [D129](#d129-int-как-alias-i64-в-bootstrap-nova) / [D227](03-syntax.md#d227) — `int`=`i64`
  wide-default vs sized; несётся через `Scalar{width,signed,wide_default}`.
- D239 (`[]T≡Vec[T]`), D228 (value-record), D216 (typed pointers) — это **лоуэринг (ABI)** факты,
  выводятся не хранятся.
- [Plan 172.1](../../docs/plans/172.1-unified-type-engine.md) U.4/U.5/U.6.1 — реализация: U.5
  унифицировал внутреннее представление чекера; U.4 делает `ResolvedType` носителем для codegen;
  ретайрит `type_ref_to_c`.

### Эволюция

- **U.5 (2026-06-20):** `ResolvedType` введён как внутренний width/sign-тип чекера (заменил
  `Ty`/`TyCat`/`cat_of`).
- **2026-06-21 (D315):** поднят из «внутренний тип чекера» в **«единый канонический тип
  компилятора»**; спайк нашёл текущий носитель лосси → обогащение-до-lossless + ретайр
  `type_ref_to_c` поставлены целью. Supersedes врезку M2 Plan 172.1.
- **2026-06-21 (Plan 172.1 U.4.6→U.4.8):** цель реализована. U.5.5(a) сделал `ResolvedType`
  lossless для C (модуль-путь / `*mut` / L2 ro); U.4.6 построил единый `resolved_type_to_c`
  (ABI-лоуэринг ЧТЕНИЕМ полей `ResolvedType`, без повторного резолва) до byte-identical паритета;
  U.4.7 флипнул `type_ref_to_c` на делегирование; **U.4.8 (`e1f1d96a`) удалил дублирующий
  `type_ref_to_c_impl`** — `resolved_type_to_c` стал `Result<String,String>` (несёт причину отказа
  сам: usize/isize/ptr removed, Self-no-recv). Production type→C теперь ОДИН лоуэринг. Остаток —
  свернуть синтаксический адаптер-хоп `TypeRef→ResolvedType` на объявленных-тип сайтах (U.6.1).
- **2026-06-21 (Plan 172.1 U.5.5c, `f7511bda`):** носитель стал lossless и для ЭФФЕКТОВ —
  `ResolvedType::Func.effects` был `Vec<String>` (только имя, `from_type_ref`→`path.last()`, дропал
  generics → `Fail[E]` терял `E`, нарушение «несёт ПОЛНУЮ семантическую личность»). Обогащён до
  `Vec<ResolvedType>` (имя + module + type-args через lossless `Named`). Разблокирует typed-errors
  Plan 173 (`Fail[E]`-dispatch по `type_id`) + Plan 174.3 (`any`/`is`) — садятся на готовый носитель,
  не переделывая. Byte-identical конструкцией (effects write-only до consume).

---

## D326 — `ref` как режим передачи параметра (safe in-out / borrow); `@`/`-> @` формализация (Plan 172.5, 2026-06-26)

> ⚠️ **РЕВИЗИЯ (Plan 184, 2026-07-06, sign-off владельца).** Нижеследующая исходная
> формулировка D326 («`ref` — режим, НЕ тип»; явные формы `mut ref x T` / `ro ref x T`;
> call-site маркер `f(ref x)`) **ПЕРЕВЁРНУТА**. Актуальная нормативная модель — раздел
> **«## Ревизия D326 (Plan 184): `ref T` — ограниченный тип»** ниже (правила Р1–Р14).
> Читать исходные R1–R12 как **историю**: R1 (ref не тип) и R4 (call-site маркер) —
> **ретрактированы**; их места помечены «⛔ РЕТРАКТИРОВАНО Р-184». R2/R3-формы параметра
> (`mut ref`/`ro ref`) заменены на `mut x T` / `x T` (Р10).

**Source:** Plan 172.5 (in-out ref params), 2026-06-26 — owner переоткрыл Q29 в param-mode.
**Status:** 🔄 REVISED (Plan 184, 2026-07-06): исходное ядро (Plan 172.5) реализовано, но модель пересмотрена — `ref T` становится **ограниченным типом** (параметры-внутренне / возвраты / локалы), маркер вызова и формы `mut ref`/`ro ref` в сигнатуре **удалены**. Актуальные правила — раздел «Ревизия D326 (Plan 184)» ниже. Историческая справка о landed-ядре 172.5 (`mut ref` in-out params, эксклюзивность `E_REF_ALIAS_OVERLAP`, addressability/mut-place/escape-ban; 2 pos + 11 neg фикстуры) сохранена в исходном тексте.
**Amends:** Q29 (open-questions.md — снимает отвержение param-mode; `ref`-ТИП остаётся отвергнут), D132 (03-syntax.md — alias-гарантия `-> @` ↔ R7), D228 (value-record `@` escape-decay R8).
**Adopt verbatim:** D181/D184 (режим возврата `@`). **Bounds:** D157 (05-memory.md) + D246-P10 (эксклюзивность УЗКАЯ, не Rust/Swift).
**Cross-ref:** Plan 172.4 / Q-value-abi-auto-placement (авто-`ro ref` + heap↔stack — НЕ дублировать), D315 (ABI выводится), D246 (L3 pointee-cap / RETURN-оракул), D131/D133/D180 (consume — borrow≠move), D156 (consume-bound), Plan 174.5/174.6 (raw pointers / FFI).

---

## Ревизия D326 (Plan 184): `ref T` — ограниченный тип

**Source:** Plan 184 (ref-type-revision), обсуждение 2026-07-06, дизайн надиктован владельцем
(четыре раунда решений). **Status:** 🟢 РЕАЛИЗОВАНО ПОЛНОСТЬЮ (заходы 1-6, 2026-07-07).
Факт по правилам: Р1✅ Р2✅ Р3✅ Р4✅ Р5✅ Р6✅ Р7✅ Р8✅ Р9✅ Р10✅ Р11✅ Р12✅
**Р13✅ Р14✅** (заход-6: режим параметра {ro,mut,consume} — ось перегрузки, единая с
receiver-mut; раздельные C-символы через `MethodSig.param_modes` + collision-triggered
mode-tag; диспатч `narrow_by_param_mode` по изменяемости аргумента-биндинга / owned-
временному; см. D84-подраздел «Ось режима» в 10-overloading.md и docs/plans/184). Носитель — `TypeRef::Ref(T)`; коды
`E_REF_TYPE_POSITION` (Р1/Р6), `E_REF_ESCAPE` (Р8), `E_REF_ALIAS_OVERLAP` (Р9/Р12),
`E_MUT_ARG_NOT_MUTABLE` (Р10); ref-локалы — истинный указатель-алиас (write-through).
**Amends:** переворот исходного D326-R1 («режим, НЕ тип» → «ограниченный тип»); ретракция
D326-R4 (call-site маркер); замена форм D326-R2/R3 (`mut ref`/`ro ref` в сигнатуре) на
`mut x T` / `x T` (Р10). **Q29-история уточняется:** Q29 отвергал **НЕограниченный** `ref`-тип
(ссылки в полях/коллекциях/кучевые утечки → лайфтаймы); **ограниченный** `ref T` (только
параметры-внутренне, возвраты, локалы; запрет утечки в кучу) — НЕ то, что отвергал Q29.

### Суть

`ref T` — **валидный тип** (аналог C++ `T&`), но **ограниченный позициями**: параметр
(внутренне), возврат, локальная переменная. Мотивация-первопричина: легализовать тип
приёмника — `@` это всегда `ref Self` (для стековых value-типов), и `-> @` — тоже. Ссылка на
стек не должна утекать в долгоживущую кучу — отсюда позиционное ограничение.

### Правила (нормативно)

**(Р1) Позиции `ref T`.** Легален: возврат (`-> ref Self` — де-сахар `-> @` для
value-типов), локал-алиас (`ro y ref T = x.a.b` / `mut y ref T = …` — именованная ссылка на
место; истинный указатель-алиас в кодогене, `mut`-запись достигает цели), тип приёмника
(`@` = `ref Self`). **ЗАПРЕЩЁН** в: полях record/value, коллекциях (`Vec[ref T]`), суммах
(`enum`), `Option[ref T]`, **тип-аргументах дженериков** (turbofish `f[ref T]`,
`size_of[ref T]` и любой `f[...]`) И в параметрах (формы `ref` сняты Р3/Р10). Диагностика в
запрещённой позиции — `E_REF_TYPE_POSITION`. Причина запрета: ссылка на стек-сторадж не
должна попасть в долгоживущую кучу (без этого нужны лайфтаймы); `ref` нехраним и не имеет
«размера как тип» — только ABI-деталь передачи, поэтому в тип-аргументе он = код, зависящий от
режима передачи. **Взаимодействие с Р6:** проверка позиции идёт ПОСЛЕ нормализации `ref H ≡
H` — для КУЧЕВОГО `H` `ref H` легален в любой позиции (в т.ч. тип-аргументе: `f[ref H] ≡
f[H]`, `size_of[ref H] == size_of[H]`), реджектится только `ref <value/generic/unknown>`.
**Реализация:** `types/mod.rs walk_typeref` Ref-арм + `ref_target_confirmed_heap`;
легальные top-level позиции снимают ведущий `ref` через `walk_ref_return`.

**(Р2) `mut` при ссылке.** В `mut`-контексте ссылки `mut` = право ПИСАТЬ через ссылку в
цель. Сама ссылка **непереселяема** (как C++ `T&`: связал — навсегда; переприсваивания
ссылки на другой сторадж нет).

**(Р3) `ref` ИСЧЕЗАЕТ из сигнатур параметров (Р10-развёртка).** Формы `mut ref x T` /
`ro ref x T` / `ref x T` в списке параметров — **удалены**. Синтаксис параметра — тройная
ось режима БЕЗ слова `ref`:
- `ro`: `f(x T)` — представление (копия ≤~16 Б либо скрытая ссылка) выбирает компилятор по
  размеру; ненаблюдаемо (с оговоркой Р12). Явного `ro ... ref` в параметре нет.
- `mut`: `f(mut x T)` ≡ **in-out ссылка ВСЕГДА** (унификация стека/кучи: на куче приватной
  мутируемой копии и так нет). Локальная приватная копия — явным локалом `mut y = x`.
- `consume`: `f(consume x T)` — владение; представление считает компилятор.
- `ref` остаётся ТОЛЬКО как: тип приёмника (`@` = `ref Self` у value), `-> @` (= `-> ref Self`
  у value), локальные алиасы (Р1). Диагностики парсера при попытке старых форм:
  `E_REF_PARAM_FORM_REMOVED` (`mut ref`/`ro ref`/`ref` в параметре → hint «пишите `mut x T`
  (in-out)»), `E_REF_CALL_MARKER_REMOVED` (маркер вызова, ниже Р4).

**(Р4) МАРКЕР ВЫЗОВА `ref x` — УДАЛЁН.** Вызов везде `f(x)` — без `ref x`. Обоснование
владельца: кучевые объекты и так мутируются без маркера — маркер на стековых давал ложное
чувство «нет `ref` = нет мутации». `f(ref x)` → парс-ошибка `E_REF_CALL_MARKER_REMOVED`
(hint «маркер удалён (D326-ревизия): пишите `f(x)`»). ⛔ **РЕТРАКТИРУЕТ исходный D326-R4.**

**(Р5) Автоконверсия.** `ref T -> T` автоматически (чтение = разыменование).
`T -> ref T` на месте вызова автоматически для адресуемых аргументов (неадресуемый аргумент
к mut-ссылке — ошибка `E_REF_ARG_NOT_ADDRESSABLE`, как в исходном D326).

**(Р6) Нормализация над кучей.** Для кучевого `H`: `ref H` ≡ `H` (значение уже handle;
ссылка на handle не вводится). Важно для обобщённых `f[T](mut x T)` при `T = H` — mono не
плодит `ref`-обёрток над handle.

**(Р7) Типизация приёмника (таблица — заменяет эвристики D181/R7-R8).**

| Категория `Self` | `ro @` | `mut @` | `-> @` |
|---|---|---|---|
| Стековый (value) тип | `ref Self` (или копия по размеру, невидимо) | `mut ref Self` | `-> ref Self` |
| Кучевой (heap) тип | `Self` | `Self` | `-> Self` |

Магия D181-R7/R8 («heap алиас / value копия-с-распадом» через RETURN-оракул и escape-decay)
**ЗАМЕНЯЕТСЯ** этими типами: `-> @` теперь имеет конкретный тип (`-> ref Self` у value,
`-> Self` у heap), а не разрешается эвристикой на bind-site. `consume @ -> @` остаётся
парс-ошибкой `E_CONSUME_RECEIVER_RETURNS_AT`.

**(Р8) Безопасность без лайфтаймов.** Висячие ссылки исключаются связкой: запрет утечки в
кучу (Р1) + существующий escape-анализ с авто-промоутом (D216 §4: убегающий источник
поднимается в кучу) + запрет захвата ссылок замыканиями/spawn/parallel. Только синхронный
lifetime вызова.

**Уточнение Р8 (заход-5): `&<ref-цель>` и авто-промоут.** Для ССЫЛКИ-цели (приёмник `@`
value-типа = `ref Self`; ref-локал) авто-промоут D216 §4 НЕ применяется — промоутится только
СВОЁ значение-локал, а `ref` указывает на ЧУЖОЕ хранилище (промоут копии тихо меняет
семантику; промоут исходного места функции недоступен). Правило:
- `&<ref-цель>` эскейпящий **НАРУЖУ** (return / захват замыканием / запись в поле) → реджект
  `E_REF_ESCAPE` с подсказками (взять владение `consume @` + промоутить своё; либо явная
  локальная копия `ro v = @` и вернуть `&v`). Живой пример-негатив: `fn T @addr() -> *T => &@`.
- `&<ref-цель>` **ВНИЗ по стеку** (аргумент вызова, ffi out-параметр) → легален без промоута
  (заём короче исходного) — действующий канон `nova_str_parse_f64(s, &v)`.
- адрес **КУЧЕВОГО** содержимого через ресивер (`Vec @ptr => @data`) — легален (не путать: у
  heap-приёмника `@` = handle, промоут копии handle корректен).
**Реализация:** `types/mod.rs check_ref_addr_escape` — value-приёмник, `&@` в escaping-позиции
(возврат/trailing/return/тело замыкания/RHS-в-поле). Захват ref-локала замыканием — follow-up.

**(Р9) Эксклюзивность/адресуемость/FFI — без изменений** от исходного D326 (узкая
синтаксическая `E_REF_ALIAS_OVERLAP`; на `extern`-границе только сырые `*`/`*mut`, никогда
`ref`), но эксклюзивность расширяется — см. Р12.

**(Р10) `ref` исчезает из сигнатур параметров вовсе** (развёрнуто в Р3): единая тройная ось
`{ro, mut, consume}` без `ref`-слова; представление `ro` выбирает компилятор по размеру;
`mut` = in-out всегда; `consume` = владение. Неоднозначность перегрузок исчезает по
построению (форма параметра одна на режим).

**(Р11) Аудит-миграция mut-параметров.** Семантика `mut x T` меняется (была приватная копия →
станет in-out): перед реализацией Ф.2 — grep-аудит всех mut-параметров std/тестов; кто
полагался на приватность копии (мутирует параметр как рабочую копию, а вызывающий использует
исходное значение после) — переписать на явный локал `mut y = x`. Результат аудита — в отчёте
Ф.0/Ф.1 плана 184.

**(Р12) Расширение узкой эксклюзивности.** `ro`-автопредставление наблюдаемо при алиасинге
`ro×mut` в ОДНОМ вызове (`f(a, a)` c `f(x Big, mut y Big)`: ссылка vs копия дают разное
чтение `x` после записи `y`). `E_REF_ALIAS_OVERLAP` расширяется с пар `mut×mut` на пары
`mut×(любой параметр того же root-пути)` — тот же синтаксический критерий R9 (стирание
индексов, prefix-overlap). **Реализовано (заход-5):** `check_ref_arg_modes` собирает места
НЕ-mut value-параметров и проверяет пересечение с mut-местами; `f(a,a)` при
`f(x Big, mut y Big)` → `E_REF_ALIAS_OVERLAP`; `ro×ro` того же места и distinct — легальны.

**(Р13) Перегрузка по режиму параметра — ЛЕГАЛЬНА, по прецеденту ресивера.** Ось
«изменяемость ресивера» уже существует (Plan 135, [D84](10-overloading.md#d84) —
`fn T @m()` vs `fn T mut @m()`: разные символы, диспатч по изменяемости биндинга). Для
параметров — то же правило: `f(x T)` vs `f(mut x T)` различаются изменяемостью
аргумента-биндинга; mut-аргумент предпочитает mut-перегрузку, иначе ro (приоритет как у
ресивера).

**(Р14) `@` — форма параметра; единая тройная ось режима.** Ресивер = нулевой параметр. Ось
диспатча `{ro, mut, consume}` **едина** для ресивера и параметров: различимы
`@func` / `mut @func` / `consume @func` И `f(x T)` / `f(mut x T)` / `f(consume x T)`. Правило
выбора: ro-аргумент → только ro-версия; mut-биндинг → mut-версия приоритетнее ro (прецедент
ресивера, Plan 135); **consume-версия участвует в резолве ТОЛЬКО когда аргумент в последней
точке использования** (consume-чекер это уже вычисляет) — тогда она специфичнее mut; иначе
исключена. Детерминизм без молчаливого потребления живого биндинга. См. амендмент
[D84](10-overloading.md#d84) (тройная ось режима параметров).

> **✅ СТАТУС Р13/Р14 (заход-6, 2026-07-07): РЕАЛИЗОВАНО.**
> `f(x T)` / `f(mut x T)` / `f(consume x T)` — легальные перегрузки (ось режима параметра
> входит в overload-сигнатурный ключ: `types/mod.rs` dup-детект сравнивает `is_mut`+`consume`
> каждого параметра наряду с типом/арностью/возвратом/receiver-mut). Раздельные C-символы —
> `MethodSig.param_modes` + мангл-суффикс режима, добавляемый ТОЛЬКО при коллизии одинаковых
> `param_c_types` (кучевой случай Р6; существующие символы byte-identical). Диспатч —
> `emit_c.rs::narrow_by_param_mode`: ro-аргумент → ro-версия; mut-биндинг → mut-версия
> (приоритетнее ro, прецедент Plan 135); owned-временное (последняя точка) → consume-версия.
> Неоднозначность невозможна по построению (классы аргумента взаимоисключающи). Полные факты
> и правило выбора — D84-подраздел «Ось режима {ro, mut, consume}» в 10-overloading.md.
> Ограничение: value/прим `mut`-mode-overload меняет ABI на указатель — scoped follow-up
> `[M-184-value-mut-mode-overload-abi]`; кучевой T (канонический in-out) покрыт полностью.

### Что ретрактировано ревизией Р-184

- **D326-R1 (частично):** «`ref` — режим, НЕ тип; запрещены `ref T`-локалы/возвраты» —
  ретрактировано. `ref T` теперь **ограниченный тип** (Р1): легален в параметрах-внутренне,
  возвратах, локалах-алиасах. Запрет сохранён только для полей/коллекций/сумм/Option.
- **D326-R2/R3 (формы параметра):** `mut ref x T` / `ro ref x T` / `ref x T` в сигнатуре —
  удалены; заменены на `mut x T` / `x T` (Р10). Диагностика `E_REF_PARAM_FORM_REMOVED`.
- **D326-R4 (call-site маркер `ref x`):** удалён полностью (Р4). Диагностика
  `E_REF_CALL_MARKER_REMOVED`.
- **D181/R7-R8 эвристики** (RETURN-оракул + escape-decay для `-> @`): заменены типами Р7.

### Точки возобновления (для последующих заходов)

- **Ф.2 (чекер):** in-out семантика `mut x T` для стековых типов (тип `Ref(T)` в носителе,
  нормализация Р6, запрет позиций Р1, эскейп-правила Р8, расширение эксклюзивности Р12,
  перегрузка Р13/Р14). До Ф.2 парсер принимает `mut x T`, но чекер оставляет старую
  семантику (приватная копия) — зелёное дерево не ломается.
- **Локалы-ссылки** `ro y ref T = expr` (Р1): парсер/чекер — если не влезло в Ф.1, `ref` в
  тип-позиции пока остаётся `E_REF_NOT_A_TYPE`.

### Что

`ref` — **режим передачи параметра** (borrow), НЕ тип. Даёт безопасную in-place мутацию caller-значения (`mut ref`) и zero-copy чтение больших стек-значений (`ro ref`, авто) без сырых указателей и без лайфтаймов. Модель = Swift `inout` / C# `in`+`ref`. `@`-ресивер — частный случай: `mut @` ≡ `mut ref @`, `ro @` ≡ `ro ref @`, `-> @` ≡ `ref @`.

### Правило

**(R1) `ref` — режим параметра, НЕ тип.** ⛔ **РЕТРАКТИРОВАНО Р-184** (см. ревизию выше: `ref T` теперь ограниченный тип; запрет сохранён только для полей/коллекций/сумм/Option). Запрещены: `ref T`-локалы/биндинги, ref-поля, ref в Vec/коллекции/sum/Option, ref-возвраты. Единственное исключение — ресивер `@` и его `-> @`. Лайфтаймов нет. Это **НЕ** реинтродукция отвергнутого Q29 `ref`-ТИПА — это param-mode, консистентный с обоснованием самого Q29.

**(R2) Две формы.** `ro ref a T` — read-only borrow (авто). `mut ref a T` — mutable in-out borrow (единственный явный user-facing; callee пишет в caller-сторадж, видно после синхронного вызова).

**(R3) `ro ref` — авто/невидимо (это Plan 172.4 / Q-value-abi-auto-placement, не дублируем).** Компилятор передаёт value-параметр скрытым ro-указателем вместо копии, когда `sizeof > ~2*sizeof(ptr)` (≈16B) и копия ненаблюдаема; семантически тождественно by-value для `ro`. Маркеров нет.

**(R4) call-site маркер `ref`.** ⛔ **РЕТРАКТИРОВАНО Р-184** (маркер вызова удалён; вызов везде `f(x)`; `f(ref x)` → `E_REF_CALL_MARKER_REMOVED`). `mut ref`-аргумент на месте вызова помечается: `inc(ref x)` (короткий `ref`, не `mut ref`). Цель — читатель видит возможную мутацию `x`, не открывая сигнатуру (как C# `ref`, Swift `&`, Rust `&mut`; `&` занят `addr_of`, Plan 118.1). `ro ref` (авто) маркера НЕ имеет.

**(R5) ABI ресивера.** `mut @` ≡ `mut ref @` — **всегда by-pointer** (любой размер; нужно для видимости мутации; даже 1-байтный `type Flag value {b bool}`). `ro @` ≡ `ro ref @` — size-discretionary (мелкий by-value, большой hidden-ptr; невидимо — ro content-immutable). Это две РАЗНЫЕ `ref`-нормы (auto-size для params vs always-by-ptr для mut-ресивера), не сливать.

**(R6) `-> @` — режим = режим ресивера (D181 дословно, не расширяется).** `fn T @m() -> @` → `ro ref @`; `fn T mut @m() -> @` → `mut ref @`; `fn T consume @m() -> @` → parse-error `E_CONSUME_RECEIVER_RETURNS_AT`. В цепочке `x.a().b()` режим `@` от `a()` гейтит вызываемость `b()`: `ro ref @` → только `ro @`-методы; `mut @` на нём → `E_RECEIVER_BINDING_NOT_MUT` (`c.peek().bump()` = ошибка).

**(R7) `-> @` при биндинге — НЕ новое decay-правило, а D246 RETURN-оракул для типа ресивера:**
- **heap-record / consume / builder**-ресивер → биндинг (`ro y =` И `mut y =`, вкл. mid-chain `mut b = sb.append(); b.append()`) — **ВСЕГДА АЛИАС, не копия** (гарантия D131/D132, load-bearing для consume-чекера; копия раздвоила бы два хэндла на один буфер → потеря use-after-consume).
- **value-record**-ресивер → биндинг = D246 `-> Value`-оракул = **копия** (те же ro/mut-знаки коэрсии, ORACLE D).

**(R8) value-record `@` НЕ эскейпит указателем.** В пределах одного полного выражения (пока сторадж ресивера жив) `-> @` может оставаться ref; при биндинге/возврате/хранении — **decay by-value** (D246-оракул). Для rvalue-ресивера (`P{x:0}.inc()`) хвостовой `@` decay'ится в слот биндинга/возврата, НЕ указателем в `__tmp_recv` (умирает в конце выражения). D228-escape-walker `@` не покрывает (срабатывает на явный `&v`) → этот decay — замещающее escape-правило. `E_AT_RETURN_OUTSIDE_METHOD` (free-fn `-> @`) сохраняется.

**(R9) Эксклюзивность `mut ref` — УЗКАЯ синтаксическая, НЕ Rust/Swift.** `E_REF_ALIAS_OVERLAP` срабатывает ⟺ два `mut ref`-аргумента в ОДНОМ вызове — проекции одного root-локала и один путь — префикс другого (после стирания индексов, не доказуемо-различных int-литералов). `f(mut ref x, mut ref x)` reject; `f(mut ref x.a, mut ref x.b)` OK; `f(mut ref x.a, mut ref x)` reject; `arr[1]` vs `arr[2]` OK; любой неконстантный `arr[i]` vs `arr[j]` → консервативно overlap (нет SMT/i≠j-прувера в V1, over-reject sound). **Явно оговорить в спеке:** это анти-footgun, НЕ гарантия — aliased-мутация остаётся sound-под-GC везде ещё (D157, D246-P10 «нет эксклюзивности (GC)»). Через указатели/heap-хэндлы/два `[]T`-слайса над одним буфером эксклюзивность НЕ заявляется (undecidable).

**(R10) Эскейп `ref` запрещён** (кроме `@`, который сам decay'ится R8): не хранить в поле/heap, не захватывать closure/spawn/parallel/supervised/detach, не возвращать. Только синхронный lifetime вызова — это и делает no-lifetimes-модель звучной.

**(R11) Слоинг / FFI.** `ref` = safe non-null non-escaping pointer (повседневный инструмент); `*T`/`*mut T` = сырые (FFI/unsafe, D246 L3). `mut ref` ≈ pointee-mut от `*mut T` но safe+non-escape; `ro ref` ≈ `*T`. На `extern`-границе — только сырой `*`/`*mut`, никогда `ref` (у ref нет стабильного ABI — это lowering-выбор).

**(R12) Дженерики / consume.** `fn f[T](mut ref x T)` — ок (mode ортогонален type-param, не в type-arg-позиции → mono не затронут). `[T consume]`-bound (D156) несовместим с `ref` на том же месте (borrow ≠ move). `mut ref` НЕ консьюмит аргумент (D131/D133 без изменений). Overlap-отношение — НОВОЕ (per-pair prefix), строится РЯДОМ с consume place-анализом (делит только парсинг места Ident/Member/Index, не решётку MOVE/CONSUME) — НЕ «субсумировано D131».

### Почему

`mut @` + lvalue (как считал Q29 2026-06-21) НЕ покрывает: out-параметры, мутацию НЕ-ресивера, несколько mut-аргументов (`swap(mut ref a, mut ref b)`). Swift (`inout`) и C# (`in`/`ref`) оба имеют user-facing in-out РЯДОМ с авто-оптимизацией — не полагаются только на методы. Узкий `mut ref` закрывает дыру минимальной ценой (param-only, без лайфтаймов, локальная call-site-проверка). Формализация `@`/`-> @` делает reference явной сущностью лоуэринга. **Для одиночной мутации канон — `mut @`, НЕ `mut ref`** (`mut ref` — узкий инструмент для мульти-mut / control+мутация, не «способ менять любой параметр»).

### Что отвергнуто

- **`ref` как ТИП** (`ref T`, ref-локалы/поля/возвраты, ref в коллекциях) — Q29 (остаётся отвергнут); вернуло бы лайфтаймы.
- **Rust/Swift-уровень exclusive-borrow soundness** — Nova сознательно разрешает aliased-mut под GC (D157/D246-P10); берём лишь узкий анти-footgun.
- **codegen-субсумция `[M-177-ifexpr-value-materialize-codegen]`** — снята: тот баг = `infer_If`/`emit_if_expr` desync (R3-repair), закрыт отдельно (`836befcb`, 2026-06-26); ссылки его НЕ чинят. Его fixture (fluent `-> @`-хвост в if-цепочке) → лишь **acceptance-гейт** 172.5 (должна компилиться после ref-формализации).

### Связь

D181/D184 (режим `@`), D246 (L3 / RETURN-оракул / P10 no-exclusivity), D131/D132/D133/D180 (consume / alias-гарантия), D228 (escape), D315 (ABI выводится), D156 (consume-bound), D157 (multi-mut sound под GC), Q29 (amend), Plan 172.4 (авто-`ro ref`/`@`/heap↔stack — реализует часть), Plan 174.5/174.6 (raw pointers / FFI). **Новый код ошибки ровно один:** `E_REF_ALIAS_OVERLAP`; остальное переиспользует existing (`E_RECEIVER_BINDING_NOT_MUT`, `E_CONSUME_RECEIVER_RETURNS_AT`, `E_AT_RETURN_OUTSIDE_METHOD`).

### Амендменты по факту реализации (Plan 172.5 Ф.1-Ф.5, 2026-07-06)

- **`mut ref` = единственная user-facing реализованная форма.** `ro ref`
  синтаксис принят (R2), но его zero-copy lowering НЕ дублируется — это
  size-driven авто-механизм Plan 172.4 (R3). Explicit `ro ref` — семантическая
  аннотация, которая иначе передаётся как обычный value-параметр; **call-site
  маркер `ref` на `ro ref`-параметре — ошибка** (`E_REF_MARKER_NOT_ALLOWED`,
  R4: маркер сигнализирует ВОЗМОЖНУЮ мутацию, только для `mut ref`).
- **Lowering `mut ref` (Ф.4):** параметр → C-указатель `T*` (в `params_c`, единый
  для forward-decl и definition); body-использования имени авто-разыменовываются
  (`name` → `(*name)`, набор `ref_params` в эмиттере); call-site `ref x` → `&x`
  (узел `ExprKind::RefArg`). Форвардинг `ref`-параметра в другой `mut ref`-вызов
  (`&(*v) ≡ v`) работает. Скаляр + record проверены (pos-фикстуры зелёные).
- **AST:** `ref` — глобальное keyword (`TokenKind::KwRef`; `ref` не используется
  идентификатором нигде в std/examples/tests → non-breaking). `Param.ref_mode:
  ParamRefMode{None,RoRef,MutRef}`; call-site — `ExprKind::RefArg(place)`
  (не тип-узел, не UnOp — производится парсером ТОЛЬКО в arg-позиции).
- **Коды ошибок (checker/parser).** Новый headline-код — `E_REF_ALIAS_OVERLAP`
  (R9, per-pair prefix-overlap; поддержаны литерал-дизъюнктные индексы, dynamic
  → консервативный overlap). Механические диагностики (не заявленные в исходном
  дизайне как existing, объявлены как новые по факту): `E_REF_NOT_A_TYPE` (R1,
  `ref` в тип-позиции — parse), `E_REF_MODE_REQUIRES_RO_OR_MUT` (голый `ref`
  без `ro`/`mut` — parse), `E_REF_MARKER_REQUIRED` (пропущен `ref` на `mut ref`,
  R4), `E_REF_MARKER_NOT_ALLOWED` (`ref` на не-`mut ref`, R4),
  `E_REF_ARG_NOT_ADDRESSABLE` (не-lvalue / index-в-цепочке, R4),
  `E_REF_ARG_NOT_MUT` (borrow `ro`-места, R2), `E_REF_ESCAPE_CAPTURE` (захват
  `mut ref`-параметра closure/spawn, R10). `E_CONSUME_RECEIVER_RETURNS_AT`
  реюзнут на существующей parse-проверке `consume @ -> @` (R6).
- **R6 mid-chain gating (`E_RECEIVER_BINDING_NOT_MUT`) — ОТЛОЖЕНО (followup
  `[M-172.5-chain-gating-ro-at]`).** parse-часть R6 (`consume @ -> @`) сделана;
  но гейтинг «mut-метод на `ro -> @`-хвосте» (`c.peek().bump()`) требует
  моделирования режима `@`-возврата сквозь method-chain — глубокое
  взаимодействие с fluent-машинерией 172.4, вне soundness in-out `mut ref`.
  Сейчас такой вызов компилируется (value-record `-> @` = копия по R7b, мутация
  копии безвредна); диагностика — отдельная задача.
- **Generic `fn f[T](mut ref x T)` (R12) — codegen отложен** (`[M-172.5-generic-
  mut-ref-codegen]`): `params_c`/`ref_params` покрывают неген. путь (concrete);
  erased/mono-пути `mut ref` не лоуэрят указатель. Checker-часть R12 (mode
  ортогонален type-param) не блокирует.

---

## D327 — Codepoint = `u32` (а не `int`): тип кодпоинта в std.unicode (Plan 172.2, 2026-06-26)

**Source:** Plan 172.2 (scalar narrowing через method-arg), 2026-06-26. Миграция std.unicode под narrowing-чек (D54) вскрыла int↔u32-импеданс: кодпоинты типизированы `int`, но хранятся `Vec[u32]` — каждый push был неявным сужением. Owner предложил хранить кодпоинт как `u32`; обсуждение выявило, что [D226 «Signed indexing convention»](#d226) к кодпоинтам неприменим.
**Status:** ✅ ADOPTED (sign-off владельца 2026-06-26).
**Amends:** [nv-coding-style.md](../../docs/dev/nv-coding-style.md) §числовые-ширины (новый пункт «Codepoint = u32»). Снимает аномалию `is_alphabetic(cp int)` (был `int`-кодпоинт по инерции signed-правила).
**Cross-ref:** [D128](#d128) (`char` = `nova_char` = `uint32_t`), [D226](#d226) (signed indexing — ОТДЕЛЬНАЯ категория), [D54](#d54) (implicit narrowing — мотивация), D77 (fallible → `Option`).

### Что

**Unicode codepoint (scalar value, 0..0x10FFFF) — `u32`, НЕ `int`.** Кодпоинт — character-data интринсик-ширины 32 бит, как UTF-16 code unit — `u16` (nv-coding-style: «`u32` — когда значение само по себе этой ширины»). Это категория **значение-идентификатор**, ОТЛИЧНАЯ от D226-категории **index/len/offset/счётчик** (мера, где underflow/`-1`-сентинел/mixed-arith мотивируют signed `int`). D226 к кодпоинтам не относится — кодпоинт не индексируют и не вычитают как длину.

### Правило

1. **Хранилище** последовательностей кодпоинтов — `Vec[u32]` (4 байта; ср. Rust `Vec<char>`, Go `[]rune`=`[]int32`). НЕ `Vec[int]` (вдвое память/кэш на горячих путях коллации/нормализации — §2 perf).
2. **Поток и арифметика** внутри unicode-движков — `u32`. Это убирает int↔u32-границу by-construction (нет неявного narrowing, нет россыпи `as u32`).
3. **`char`** (= `u32`, D128) — семантический тип кодпоинта на границе `str`: `s.as_chars()` → `char`, `char.try_from(u32)` → валидный скаляр, char-методы (`'a'.is_alphabetic()`).
4. **Публичные cp-функции принимают `u32`** (`general_category(cp u32)`, `is_alphabetic(cp u32)`). Целочисленные литералы адаптируются к u32-контексту → `general_category(0x41)` остаётся валидным (контракт Plan 159 / plan152_3 сохранён). char-методы делегируют через `@ as u32`.
5. **Fallible-функции, выдающие кодпоинт → `Option[u32]`** (D77), НЕ `-1`-сентинел. (`-1` в `int`-функции — идиоматичен для find/мер D226, но кодпоинт — не мера.)
6. **Bit-packing** нескольких кодпоинтов в один ключ (`(a<<21)|b`, > 32 бит) → ключ `int`, явный `as int` на упаковке (packed key — не кодпоинт).

### Почему не оставить `int` + `Vec[u32]`-хранилище

Хранилище обязано быть 4-байтным (perf/идиома; 8-байтный `Vec[int]` кодпоинтов неидиоматичен). При `int`-потоке + `u32`-хранилище int↔u32-граница на каждом push неустранима — это и есть источник narrowing-боли 172.2. `u32`-поток её снимает.

### Звучность / footgun

`u32`-арифметика `cp - lo` молча заворачивается при `cp < lo` (D226 §«нет underflow-trap» — аргумент за signed). В unicode эти вычитания (hangul `cp - SBASE`) уже под range-guard'ами (`cp >= lo && cp <= hi`), underflow недостижим, footgun локализован и под охраной. Приемлемо для домена.

## D310. Type-set bounds (Plan 172.3)

**Статус:** дизайн закреплён 2026-06-28 (owner sign-off; Plan 172.3 Ф.0). Amends [D72](#d72-generic-bounds-через-t-protocol--protocol-как-тип) + [D145](#d145-fnt-префикс--receiver-generic-decl--bounds-plan-101): «bound = только protocol» → «protocol ИЛИ type-set».

### Что
Новая kind-форма объявления типа — **type-set** — задаёт **именованное множество конкретных типов**, используемое как generic-bound (`fn[T IntSet] …`). Это Go-style type-constraint: код, общий для семейства примитивов (`int.parse`/`u32.parse`/…), выражается одним generic вместо per-type обёрток. Частично закрывает [Q-representation-bound](../open-questions.md#q-representation-bound) — **только explicit-member-set**; `~underlying`/repr/structural — по-прежнему Plan 102.

```nova
// inline
type SignedInts   set i8 | i16 | i32 | i64 | int
type UnsignedInts set u8 | u16 | u32 | u64 | uint

> **АМЕНДМЕНТ 2026-08-17 (решение владельца; аудит самосогласованности,
> раздел 4, пункт 11).** Здесь стояло единственное число — `SignedInts` /
> `UnsignedInts`, — тогда как [D430](#d430) и весь `std` пишут множественное
> (`[T Ints]`, `SignedInts`). Имя одного и того же множества расходилось между
> двумя D-блоками и реализацией, то есть читатель D310 писал bound, которого
> нет. Верное — МНОЖЕСТВЕННОЕ: множество типов, а не один тип; так же
> называется семейство `Ints`/`Floats`.
>
> **ДОПОЛНЕНО В ТОТ ЖЕ ДЕНЬ, ПОСЛЕ ЗАМЕЧАНИЯ ВЛАДЕЛЬЦА.** Сначала было
> переименовано 16 вхождений В ЭТОМ ФАЙЛЕ — и на этом я остановился. Форма
> жила ещё в СЕМИ: `spec/syntax.md` (3), `spec/syntax.ru.md` (3),
> `spec/decisions/README.md` (4), `spec/decisions/04-effects.md` (8),
> `std/src/prelude.nv` (1), `std/src/math/overflow_policy_test.nv` (6),
> `std/src/runtime/defaults.nv` (1) — итого 28 вхождений в восьми файлах,
> из них три файла в `std`. Владелец заметил вопросом «`UnsignedInts` — ещё
> такое вроде есть».
>
> Записано здесь потому, что это ТОТ ЖЕ класс, про который часом раньше
> написан амендмент к D416 §5 («правку внесли в одно место из четырёх»), —
> и повторил его тот, кто этот амендмент писал. Правило из этого простое и
> механическое: **переименование считается сделанным не когда исправлен
> носитель, а когда греп по старой форме даёт ноль по всем зонам.**
> В `std` все 8 правок оказались в комментариях и названиях тестов —
> проверено: `nova check std/src` даёт прежние 154/26, регрессии нет.

// многострочный — | обязателен у каждого члена включая первый
type AnyNumber set
    | i8 | i16 | i32 | i64 | int
    | u8 | u16 | u32 | u64 | uint

fn[T UnsignedInts] T.parse(s str, radix int) -> Result[T, ParseUIntError] => ...
```

> **Амендмент (R3, 2026-07-07, я):** примеры этого блока были `T.try_parse(...)` (Option-контракт,
> до-R3 текст). `try_parse` без infallible-сиблинга нарушает R3 (D325); я ретрактировал
> компиляторный `f64.try_parse` builtin и заменил на `f64.parse(s) -> Result[f64, ParseFloatError]`
> (`[M-f64-try-parse-to-parse-f64]`). Примеры выше переписаны на `T.parse` — future generalization
> (Plan 174.1) читай как `T.parse`, не `T.try_parse`.

### Правило

- **Синтаксис.** Очередная kind-форма под `type` ([D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-)/D53/D406): `type Name set Member1 | Member2 | …`. Диспетчеризация по **первому токену после имени** — контекстный kind-токен `set` однозначно отличает type-set от sum-type (`type X enum A | B`, D406) и остальных форм. Backtracking нет (один токен lookahead). `set` — контекстное слово (только в позиции после `type Name`), НЕ глобально-зарезервированное. Члены — TypeRef через `|`. **Многострочная форма:** если первый член на новой строке — `|` обязателен у каждого члена включая первый (аналогично D406 `enum` и D310 `set`); несколько членов в одной строке допускаются.
- **Члены — по ИДЕНТИЧНОСТИ.** Примитивы и любые объявленные конкретные типы (newtype / named-tuple / record), каждый перечислен ЯВНО. Newtype `type MyI8 i8` **не** член set'а `{i8}` — нужен явный листинг. `~underlying` НЕТ (в Nova нет implicit-coercion; D52/D215).
- **Bound = membership-предикат.** В `[...]`-позиции type-set ведёт себя как protocol-bound (D72): `[T SignedInts]`. Композиция с протоколами через `+` (D145, conjunction): `[T SignedInts + Hash]` ⇒ T ∈ set И реализует Hash; проверки независимы, per-member. **Не более одного type-set** в bound-листе (`E_MULTIPLE_TYPE_SETS`); протоколов — сколько угодно.
- **Семантика тела.** Мономорфизация per член (как обычный `fn[T]`, Plan 48 worklist). `T.MAX`/`T.MIN`/`T.new`/литералы резолвятся per-instance через `numeric_type_constant_mapping` по **Nova-имени** подставленного члена (нужен Nova-name subst-канал T→"i8" ПЕРЕД lookup, отдельный от C-name subst T→"int8_t"). Операторы в теле — **пересечение** легальных для ВСЕХ членов; чекер материализует resolved-тип каждого T-выражения в per-ExprId канал (codegen лоуэрит, не ре-резолвит). Без `nova_int`-fallback (§1): неразрешённый член = диагностика чекера, не угадывание.
- **Знаковость.** Один set НЕ смешивает signed/unsigned целые (`u64.MAX = 2^64−1 ∉ i64` → несовместимые value-domains; единое тело несоундно для обеих групп). Чекер: `E_TYPE_SET_MIXED_SIGNEDNESS` на объявлении. Stdlib даёт два готовых: `SignedInts`, `UnsignedInts`. Без рантайм-ветки по `T.MIN==0` (§2: не платим рантаймом за статически известное).

### Проверки / диагностика (чекер, §1/§6; новые коды в [09-tooling](09-tooling.md))
- `E_TYPE_NOT_IN_SET` — конкретный T не член set'а (фиксируется на **инстанцировании**, не на use-site внутри тела; сообщение перечисляет членов + fix).
- `E_TYPE_SET_MEMBER_NOT_CONCRETE` — член set'а не конкретный тип (protocol / effect / другой type-set).
- `E_TYPE_SET_MIXED_SIGNEDNESS` — set смешивает знаковые/беззнаковые целые.
- `E_MULTIPLE_TYPE_SETS` — >1 type-set в одном bound-листе.

### Почему
- **Reuse через семейства примитивов** — один `fn[T SignedInts] T.parse` вместо ×10 обёрток (разблокирует Plan 174.1, вариант B).
- **Zero-ambiguity синтаксис** через существующий D52-диспетч (kind-токен, как `alias`/`protocol` под D53) — без нового top-level keyword, без backtracking, без конфликта с sum-`|`.
- **Звучность в чекере, лоуэринг в codegen** (§0/§1): membership и легальность операторов — чекер; `T.MAX` — лоуэринг подставленного имени, без `nova_int`-fallback.
- **Знаковость разрешена на уровне декларации** (§2/§5), не рантайм-веткой.

### Связь
[D52](#d52-объявление-типов-revised-newtype-alias-sum-через-leading-) (формы `type`, first-token dispatch) · D53 (kind-токен под `type`) · [D72](#d72-generic-bounds-через-t-protocol--protocol-как-тип) (bound = тип в [...]-позиции, **amended**) · [D145](#d145-fnt-префикс--receiver-generic-decl--bounds-plan-101) (`+` multi-bound conjunction, **amended**) · [D237](#d237) (capitalized naming) · [D315](#d315-resolvedtype--единый-канонический-носитель-типа-plan-1721-2026-06-21) (ResolvedType несёт ширину/знак). [Q-representation-bound](../open-questions.md#q-representation-bound) — частично (explicit-member-set); `~`/repr → Plan 102. Потребитель: Plan 174.1.

---

## D405 — Арифметика смешанных целых ширин: требовать явный cast (2026-06-30)

**Статус:** закреплён 2026-06-30 (owner sign-off).

### Что

Выражение `u8_val + u16_val` (или любые два целых операнда разных ширин/знаковостей) — **ошибка компиляции** `E_MIXED_WIDTH_ARITH`. Компилятор не делает implicit widening. Программист обязан явно указать целевой тип через `as`-cast:

```nova
ro a u8 = 200
ro b u16 = 300
// ro result = a + b      // E_MIXED_WIDTH_ARITH: u8 + u16 — implicit widening запрещён
ro result = a as u16 + b  // ok: a явно расширен до u16
```

### Правило

- **`E_MIXED_WIDTH_ARITH`** — оба операнда бинарного арифметического оператора (`+`, `-`, `*`, `/`, `%`, `>>`, `<<`, `&`, `|`, `^`) должны иметь **одинаковый** конкретный целочисленный тип. Разные ширины (`u8`+`u16`) или разные знаки (`i8`+`u8`) — compile error.
- Исключения: `int`-литералы без явной ширины инференсируются к типу другого операнда (D55 literal coercion). Только когда OBA операнда именованные — error.
- **Widening явен** — через `as`-cast (D54). Выбор ширины — на программисте. Нет автоматического «бери больший».

### Почему Rust/Go/Zig-стиль, не Java/Kotlin

| Язык | Поведение | Проблема |
|------|-----------|---------|
| Java/Kotlin | `byte + short → int` (implicit, always signed) | Результат signed даже для unsigned операндов; теряется знаковость |
| C | `u8 + u16 → uint` (integer promotion, C-зависимо) | Платформо-зависимо; signed promotion удивляет |
| **Rust/Go/Zig/Swift** | compile error → явный cast | Никаких сюрпризов; программист контролирует |
| Nova (D405) | compile error → явный `as` cast | Согласуется с «no implicit conversions» (D54) |

Nova не имеет implicit numeric coercion нигде (D54 — `as` для всех width-changes). D405 последовательно расширяет это на бинарные операторы: нет молчаливого widen или truncate.

### Связь

[D54](#d54-as-cast) (`as` явный cast) · [D55](#d55) (literal coercion — исключение для нетипизированных литералов) · [D315](#d315) (ResolvedType несёт ширину/знак — необходим для этой проверки) · [D310](#d310-type-set-bounds-plan-1723) (`SignedInts`/`UnsignedInts` type-sets — generic-альтернатива per-width обёрткам).

---

## D358 — HTTP message-model (`std/http`, Plan 178 Ф.1) {#d358}

**Статус:** ✅ landed (Ф.1, 2026-07-04) — message-model + URL + валидаторы. `Http`/`HttpServer`
effect-контракт (D357) и client/server-политики (D360/D361) — Ф.2+.

Pure value-типы поверх net byte-surface (Ф.0.5). Всё fallible → `Result[T, HttpError]`
(D325). Формы:

- **`Method`** — `| Get | Head | Post | Put | Delete | Connect | Options | Trace | Patch | Other(str)`;
  `parse` валидирует RFC 7230 tchar; сравнение case-sensitive; `@is_safe`/`@is_idempotent`/`@allows_body`.
- **`StatusCode`** — value-newtype над `u16` (100..599); `@class -> StatusClass`; `@reason` (RFC 9110);
  zero-arg-фабрики (`ok()`/`not_found()`/…, Q17). 4xx/5xx = валидный Response, НЕ ошибка (Q4).
- **`Version`** — `| Http10 | Http11 | Http2`; OPEN (forward-compat Http3 — wildcard рекомендуется;
  языкового `#open`-атрибута нет, свойство конвенциональное).
- **`HeaderName`/`HeaderValue`/`HeaderMap`** — case-insensitive по имени, ordered, multi-value.
  Имя = ASCII tchar (lowercase-канон); значение = `[]u8` (latin1 fast-path `from_str`, fallible
  `@to_str` на non-ASCII, Q18). **Безопасность (by construction):** `insert`/`append`/`from_*`
  ОТВЕРГАЮТ CR/LF/NUL → response-splitting невозможен; `@content_length` ловит CL+TE-конфликт
  (request-smuggling, RFC 7230 §3.3.3). `@insert` = replace, `@append` = add (§13.1).
- **`Url`** (промоут `_experimental/encoding/url.nv`) — `parse -> Result[Url, HttpError]` (было
  `from`/`Fail`, D325 R2), `@to_str`. **Строгий host/SSRF-валидатор:** bracket-IPv6, canonical
  dotted-quad IPv4, REJECT control/NUL/whitespace/non-ASCII, REJECT decimal/octal/hex IP-обфускации
  (`0x7f.1`/`0177.0.0.1`/`2130706433`/`127.1`); `@is_private_target` (loopback/link-local/RFC1918/
  metadata). `encode_query` percent-encodit КАЖДЫЙ UTF-8-байт (был баг: один байт для >127);
  `decode_query` — self-contained UTF-8-валидация. Байт-корректный парсер (byte-offsets, срезы по
  ASCII-делимитерам). Детали ошибок — `ParseUrlError` (tuple-варианты) через `ErrSource.UrlParse`.
- **`Mime`/`ContentType`** — `type/subtype` (lowercase-канон) + параметры (charset/boundary).
- **`Cookie`/`SetCookie`/`SameSite`** — RFC 6265bis SEND-инварианты enforce'ятся на `parse`:
  `Secure`-cookie не по `http://` (`@is_sendable`); `__Host-`/`__Secure-`-префиксы; `SameSite=None ⇒ Secure`.
- **`Request`/`Response`** — несут must-consume `Body` (D359) → сами `consume`; метаданные —
  borrow-методы; тело разряжается делегирующими consume-методами (`resp.text()`).

### Амендменты по факту реализации (Ф.1)

- **`SameSite.None` → `SameSite.Cross`** (wire-value «None» сохранён): вариант `None` в public-enum
  коллидирует с `Option.None` в namespace любого импортёра std.http → переименован.
- **`ErrSource.Url(ParseUrlError)` → `ErrSource.UrlParse(...)`**: имя-вариант == имя-тип `Url` ломает
  codegen (cast вместо wrap).
- **`ParseUrlError` — tuple-варианты** (`InvalidScheme(str)` …), НЕ record-варианты: auto-eq для
  record-вариантов внутри `Option[sum]` mis-lower'ит (`_0` на named-fields).
- **`HttpError` — non-`value` record**: `value` + `Option[Url]`/`Option[ErrSource]`-поля → codegen
  emit'ит Option-typedef ПОСЛЕ struct-а (forward-ref «unknown type»).

### Амендмент Ф.2 (auto-decompress landing, 2026-07-06)

- **`ErrSource` + `Compress(CompressError)`** (OPEN enum → non-breaking): типизированный source для
  провалившегося decode `Content-Encoding` (gzip/deflate). Разблокирован фиксом D381 (collision-aware
  module-qualified mangling) — `compress.ErrorKind` и `http.ErrorKind` теперь СОСУЩЕСТВУЮТ в одном CU
  (доказано `nova_tests/http_decompress`). Bomb (превышение `max_decompressed`, D334) НЕ несётся через
  `Compress`, а мапится в `HttpError{BodyTooLarge}` (DoS-guard). `br` (brotli) закрыт — нет кодека
  `[M-178-autodecompress-br]`. Клиент шлёт `Accept-Encoding: gzip, deflate` по умолчанию (opt-out через
  `HttpClientBuilder.@no_decompress()`); при декоде заголовок `Content-Encoding` снимается, а
  `Content-Length` переписывается на декодированную длину (headers описывают тело, которое видит вызыватель).

## D359 — must-consume `Body` (`std/http`, Plan 178 Ф.1) {#d359}

`Body` — линейный **must-consume** (D133): единственный способ «разрядить» — потребляющий метод
(`@bytes`/`@text`/`@drain`/`@into_reader`). Незакрытое тело = **compile-error** — чинит главный
Go-footgun (`resp.Body`-leak) на compile-time. `Response`/`Request` держат `Body` как `consume`-поле
(двойной D133-маркер), разряжают делегированием in-place.

Repr = `InMemory([]u8) | Stream(BodyReader)`; `BodyReader` — чистый Nova-декодер над byte-source
(Q19), НЕ C-handle. `@with_limit` → `BodyTooLarge` (DoS-guard). `@text` = строгий UTF-8 (Ф.1).

### Амендменты / гейты по факту (Ф.1)

- **`Body` — `consume` (НЕ `consume value`)**: value-копия оставила бы consume-поле владельца
  (Request/Response) неразряженным (`@body` копировался, не move'ился).
- **Конструктор из СЫРЬЯ, не из pre-built `Body`**: запись с consume-полем строится ТОЛЬКО с
  полем-значением как СВЕЖИМ inline-выражением (`Body.from_bytes(..)` внутри конструктора); move
  consume-переменной/параметра в поле НЕ распознаётся checker'ом → конструкторы принимают `[]u8`/
  `BodyReader`. `[M-178-consume-field-ctor-from-var]`
- **`BodyReader` — non-`consume` в Ф.1** (in-memory, ресурса нет; transport-backed reader держит
  socket → станет `consume` в Ф.2). `@next_chunk -> Result[[]u8]` + `@at_eof` (план-форма
  `Result[Option[[]u8]]` (None=EOF) упирается в codegen-ordering-баг eq `Option[Option[[]u8]]` →
  `[M-178-bodyreader-option-eof-eq-ordering]`).
- **ОТЛОЖЕНО в Ф.2 (гейты, НЕ упрощения):** `Http`-effect на потребляющих методах (park над
  транспортом) `[M-178-body-http-effect-surface]`; `@copy_to` (fs-gate 176) / `@json[T]` (serde-gate
  180 Ф.4) / `@trailers` (Ф.2) `[M-178-body-copy-json-trailers]`; charset-aware `@text`
  (latin1-fallback по Content-Type) `[M-178-body-text-charset]`; typed `expires Timestamp` в SetCookie
  (date→epoch, Plan 175) `[M-178-setcookie-expires-timestamp]`.
## D340 — serde data-model + protocols (Plan 180)

Format-agnostic typed serialization. Protocols `Serialize` / `Deserialize` (contract on a type) + `Serializer` / `Deserializer` (backend), generic-bound notation `[S Serializer]` (D72/D119, NOT `impl Trait` — Q12). Lean 12-case data-model (`bool int uint float str bytes option unit seq map struct enum`, Q2; `int`=i64-wide widen/range-check).

- **`Serialize` = push**: `@serialize[S Serializer](mut s S) -> Result[(), SerError]`. **`Deserialize` = static pull**: `.deserialize[D Deserializer](mut d D) -> Result[Self, DeError]` (D35 static).
- **Serializer = single mutable stack-machine** (realized form): composites framed by `begin_struct`/`struct_field`/`end_struct`, `begin_seq`/`end_seq`, `begin_map`/`map_key`/`end_map`; scalars terminal `serialize_X`. The backend owns an internal frame stack (format-agnostic). This REPLACES the aspirational "consume sub-serializer returned per composite" form: in value-semantics Nova a sub-serializer mutating shared parent state has no clean ownership; the stack machine is the sound realization (the synthesizer always emits matched begin/end pairs — same balance guarantee).
- **Deserializer = single cursor**: `enter_field` / `enter_field_or_null` / `enter_index` / `enter_key` return a sub-cursor (`Self`) positioned at the child; sub-deserializers only READ, so no write-back/ownership problem (keyed-access model = Swift `KeyedDecodingContainer`). No `Visitor` companion TYPE is synthesized — the keyed/indexed model makes it unnecessary.
- `SerError` / `DeError` = **`value`-record** (D215/D322 pattern), OPEN kind + `path` (D325 R5). Kinds use DISJOINT variant names (`SerErrorKind`: `NonFiniteFloat`/`SerDepthLimit`/`SerCustom`/`SerOther`; `DeErrorKind`: `UnexpectedType`/`MissingField`/`UnknownField`/`OutOfRange`/`LossyInteger`/`DepthLimitExceeded`/`Syntax`/`Custom`/`Other`) — Nova bare-variant construction does not disambiguate a shared variant name by expected-arg type, so overlaps are avoided by construction.
- PURE (no effect) — codec over values/bytes.

## D341 — record auto-derive contract (compiler synthesis)

`#impl(Serialize + Deserialize)` opt-in — the 7th/8th members of the auto-derive family (`Equal`/`Hash`/`Clone`/`Compare`/`Display`/`Debug`); `is_builtin_protocol` extended. **SUM is supported (Plan 180 Ф.2-sum, externally-tagged — see D345);** the record-path shapes below apply to record/named-tuple types. Emitted shapes:
- `@serialize`: `s.begin_struct(name, N)?`; per field `s.struct_field("k")?; @field.serialize(s)?`; `s.end_struct()` — UNIFORM memberwise push (like `@debug`).
- `.deserialize`: per field `mut sub = d.enter_field[_or_null]("k")?` then TYPE-DIRECTED read — scalar → `sub.deser_X()?` (instance); record/`Vec`/`HashMap` → `<T>.deserialize(sub)?` (static); `Option[T]` → inline `if sub.is_null()? { None } else { Some(<inner>) }` (built-in `Option` does not dispatch a user static method). Then `Ok(Type{ f1, f2, … })`.
- Field-eligibility: primitive / `Option`·`Vec`·`HashMap[str,_]` (recurse) / `#impl(P)` / provides-method — else `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL` (named field; no silent drop). `HashMap` key must be `str` (Q16). priv fields serialize (structural synth). User method wins (D77).
- **Injection ordering**: serde synth is injected BEFORE type-check (its bodies call other methods whose return types codegen's annotation-free `infer_expr_c_type` cannot always resolve; type-checking annotates them). Non-serde protocols inject AFTER check as before (some bodies, e.g. `@display`'s `w.write_str`, are intentionally not type-checkable). Bound satisfaction: a `#impl(P)` type satisfies `[T P]` for a built-in auto-derivable P even before the method is materialized.

## D342 — data-model ↔ synthesis mapping (Plan 180)

record→struct, `Vec`→seq, `HashMap[str,_]`→map, `Option`→option (None→null default; absent accepted on deser, Q7), scalars→prim (`int` i64-wide widen / exact-integer range-check, Q2/Q15), `[]u8`→bytes. **numeric-fidelity (Q15)**: on deser, `int`/`uint` require an f64 that is an exact integer in `[-2^53, 2^53]` — else `LossyInteger`; negative→uint → `OutOfRange`. `Option[Option[T]]` ambiguity documented (Some(None)==None on wire). `[]u8`→base64 and Timestamp/Duration mappings are followups ([M-180-bytes-base64], Plan 175 coordination).

## D344 — JSON backend over std/encoding/json (Plan 180)

`JsonSerializer` (stack-machine building `JsonValue` → `@into()`) / `JsonDeserializer` (cursor over `JsonValue`) layered on the existing `JsonValue`/`Json.parse` (Q11, reuse — not a new parser). Public API = **free functions** `json_encode[T]` / `json_decode[T]` / `json_encode_pretty` / `json_to_value` / `json_from_value` / `json_decode_bytes` / `json_decode_with` (NOT `Json.encode` namespace-static: turbofish on a namespace/type-static generic method does not monomorphize — Ф.0-verify empirics; free-fn turbofish does. Followup [M-180-namespace-static-generic-mono]). depth-guard (Q14 default 128 → `DepthLimitExceeded`). `ParseJsonError` → `DeError{Syntax(msg)}` with line/col preserved in the message. Map encode sorts keys (determinism). **Contract for Plan 178 record-DTO** (`json_decode[T]` / `json_encode`).

## D345 — sum auto-derive + tagging (Plan 180)

**Ф.1 — sum rich data-protocol synth (✅ landed 2026-07-06).** The six built-in
protocols (`Equal`/`Hash`/`Clone`/`Compare`/`Display`/`Debug`) now synthesize
`match @ { … }` with one arm per variant instead of the old placeholders
(equal=identity, hash=0, clone=self, compare=0, display/debug=typename). Per
`SumVariantKind`: `Unit` (no payload / bare-ctor reconstruction), `Tuple(tys)`
(positional binds, `V(a0,a1)`), `Record(fields)` (named binds, `V { f }`).
- `@equal`: `V(a..) => match other { V(b..) => a==b && …, _ => false }` — same
  variant + payload-wise `==`; different variant → false.
- `@hash`: variant-index seed (`idx+1`) combined with each payload's `.hash()`
  via the record-path rotate-XOR; distinct unit variants hash apart.
- `@clone`: match-arm reconstruction — primitives shallow-copied, composites
  `.clone()`d; `Unit`→bare ctor, `Tuple`→`V(clone…)`, `Record`→`V { f: clone }`.
- `@compare`: extract both variant indices, compare those first; on tie compare
  payloads lexicographically (`ro c = a.compare(b); if c != 0 { return c }`).
- `@display`/`@debug`: `"V"` / `"V(x, y)"` / `"V { f: x, g: y }"`; display routes
  primitives via `w.write_str(str.from(x))`, debug uniformly `x.debug(w)`.
These inject AFTER type-check (like the record-path), so the emitted `match` /
variant-patterns / variant-construction are lowered by codegen's annotation-free
inference (scrutinee `@` + `other: Self` types known). [M-126-sum-*-rich] CLOSED.

**Ф.2-sum — serde sum-derive, externally-tagged (✅ landed 2026-07-06).**
`#impl(Serialize + Deserialize)` on a sum synthesizes `match`-arm-per-variant
bodies over the Ф.1 pattern/ctor infra. **Externally-tagged (Q4, default):**
- unit variant `V`          → bare string `"V"`
- single-payload `V(x)`     → `{"V": <x>}`
- multi-tuple `V(a, b)`     → `{"V": [<a>, <b>]}`  (inner array)
- record variant `V{f, g}`  → `{"V": {"f": <f>, "g": <g>}}`  (inner struct)

Serialize emits over the existing `Serializer` primitives (`begin_struct`/
`struct_field`/`begin_seq`/`serialize_str`/…) — no new enum-specific serializer
methods. Deserialize reads the tag (`d.is_str()?` → bare string for unit; else
the single object key via `map_keys`/`enter_key`), then an `if/else-if` chain on
the tag name reconstructs the variant, reading payload from the tagged cursor
(tuple → `enter_index`, record → `enter_field`, single → direct). Unknown tag →
`DeError{UnknownVariant{name, expected}}` (new `DeErrorKind` variant); malformed
(non-single-key object) → `DeError{Syntax}`. Payload eligibility mirrors the
record-field check (typed `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL` by variant, never
a bad synth). Runtime additions: `Deserializer.@is_str()` + `DeErrorKind`
`UnknownVariant`/`NoVariantMatched`. NOT on Plan 178's critical path (record-DTO
suffices). Codegen: a static `T.deserialize(sub)?` whose return-type inference
degrades (mono-collection order perturbation once a sum ALSO derives Deserialize)
is pinned to `Result[T, DeError]` at the `?`-lowering site (`emit_c.rs` Try arm),
mirroring the `.serialize?` pin.

**Ф.5 — internal/adjacent tagging (✅ landed 2026-07-06, D382); untagged gated.**
Now that the `#serde(...)` declaration-attribute infra exists (D382), the
non-external tagging modes are synthesized from the type-level attributes:
- `#serde(tag="k")` → **internally-tagged** ✅: unit `V`→`{"k":"V"}`; record
  `V{f}`→`{"k":"V","f":…}` (fields inlined beside the tag). Tuple/positional
  payloads are rejected — `E_SERDE_INTERNAL_TAG_NON_STRUCT` (no object to inline
  the discriminator into; serde rule).
- `#serde(tag="t", content="c")` → **adjacently-tagged** ✅: unit→`{"t":"V"}`;
  single→`{"t":"V","c":x}`; tuple→`{"t":"V","c":[…]}`; record→`{"t":"V","c":{…}}`.
- `#serde(untagged)` → **untagged** 🔴 GATED (`[M-180-untagged-codegen-mono]`):
  synthesized correctly (unit→`null`; single→`x`; tuple→`[…]`; record→`{…}`;
  deserialize buffers the `JsonValue`, Q17, and tries each variant in declaration
  order — value-semantics cursor makes each attempt a non-destructive retry;
  `DeError{NoVariantMatched}` if all fail). BUT compiling an untagged-derive body
  perturbs `std/encoding/json` codegen in the same CU (mono-collection ordering →
  `Json.parse` mis-tags a number as a bool), so `#serde(untagged)` is rejected at
  compile time (`E_SERDE_UNTAGGED_GATED`) until that codegen-hardening prerequisite
  lands. A compiler bug, NOT a serde-logic defect.
Serialize/deserialize are synthesized over the SAME `Serializer`/`Deserializer`
primitives as external (`begin_struct`/`struct_field`/`enter_field`/`enter_index`
/`is_null`/…) — no new backend methods. The mode is computed from the type's
`serde_attrs` by `serde_tagging_mode` (auto_derive.rs) with static validation
(conflict / content-without-tag / on-non-sum / internal-on-tuple / untagged-gate).
Externally-tagged (no attribute) is unchanged. [M-180-serde-tagging-modes] CLOSED
for internal+adjacent; untagged → [M-180-untagged-codegen-mono].

**Codegen soundness note (Plan 180 Ф.6).** The synthesized deserialize bodies
exercise json.nv's `Deserializer` methods heavily; their `match m.get(k) { None
=> Err(..), Some(v) => Ok(..) }` shape exposed a latent match/if result-type bug
— `Ok(x)` alone infers a stub ERR side (`NovaRes_<ok>_nova_str`) and `Err(e)` a
stub OK side (`NovaRes_nova_int_<err>`), so neither arm yields the full
`Result[JsonDeserializer, DeError]` and the returned cursor was mis-laid-out
(decode returned spurious `UnexpectedType`). Fixed by reconciling the concrete OK
(from an `Ok(..)` arm) with the concrete ERR (from an `Err(..)` arm) across
`emit_match`/`emit_if_expr` + their `infer_expr_c_type` mirrors — splitting the
already-computed arm/branch Result-types via `novares_ok_err` (side-effect-free;
an earlier re-inference variant perturbed mono-collection order). Order-
independent; genuine `Result[int, E]` matches unchanged. Zero-regression verified
(~50 dirs, byte-identical to parent). This fixed internal+adjacent; untagged
needs a further, distinct mono-ordering fix (the json.nv corruption above).

## D346 — serde soundness invariants (Plan 180)

Q14 depth-guard (both sides, default 128); Q15 exact-integer-check (no silent-lossy); Q16 `str`-only map keys; Q18 dup-field reconciled with `Json.parse`'s strict `DuplicateKey`.

## D382 — declaration attributes `#serde(...)` (Plan 180 Ф.6) {#d382}

**Grammar.** A declaration attribute is `#name(arg (`,` arg)*)`, `arg := ident
[ `=` StrLit ]` — i.e. bare flags (`#serde(untagged)`), string-valued keys
(`#serde(tag="type")`), and comma-separated combinations
(`#serde(tag="t", content="c")`). Multiple `#serde(...)` annotations on one
declaration accumulate. The only recognized namespace in V1 is `#serde`. Parsed
by `parse_serde_attr` (shared across the three positions), extending the
`#visible_to`/`#impl` marker-parsing precedent.

**AST.** A new `serde_attrs: Vec<SerdeArg>` field on **`TypeDecl`**,
**`SumVariant`**, and **`RecordField`** (empty Vec = default, backward-compat).
`SerdeArg` = `Tag(String)` | `Content(String)` | `Untagged` — a structured
list, general enough that field-customization keys (rename/skip/…) drop in
without a grammar change. Type-level `#serde` is threaded through
`parse_type_attrs` → `parse_type_decl`; field-level alongside `#visible_to` in
the record-field loop; variant-level as a leading marker in
`parse_one_sum_variant`.

**Recognized keys (V1, AMENDED by D435 2026-07-22 — 180.1 Ф.1).** `tag`,
`content`, `untagged` — sum-type enum tagging (consumed by D345 Ф.5 via
`serde_tagging_mode`; `tag`/`content` land, `untagged` is parsed+validated but
its derive is gated `E_SERDE_UNTAGGED_GATED`, see D345). `rename`,
`rename_all`, `skip`, `skip_serializing_if`, `default`, `alias`,
`deny_unknown_fields`, `allow_unknown` — field/wire customization on RECORD
types, consumed by the record synthesizer (D435). `flatten` is parsed and
statically validated but its synthesis remains gated (D435,
[M-180-serde-flatten]). **Unknown-attribute policy** (convention, mirrors
`#impl`/`#from_fields`: unknown marker → hard error, never silent): any other
key inside `#serde(...)` → **`E_SERDE_BAD_ATTRIBUTE`** at parse time (beats
Go/Jackson silent tag-typo).

**Static validation** (`serde_tagging_mode`, surfaced as compile errors):
`E_SERDE_TAGGING_CONFLICT` (`untagged` with `tag`/`content`; or `tag`==`content`);
`E_SERDE_CONTENT_WITHOUT_TAG` (`content` without `tag`);
`E_SERDE_TAGGING_ON_NON_SUM` (tagging attr on a record/non-sum);
`E_SERDE_INTERNAL_TAG_NON_STRUCT` (internal `tag` on a type with a tuple
variant); `E_SERDE_UNTAGGED_GATED` (untagged derive gated on a codegen-mono fix,
[M-180-untagged-codegen-mono]). See D345 Ф.5 for the emitted wire per mode.
See D435 for the field-attribute-specific validations added on top
(`E_SERDE_ATTRIBUTE_MISPLACED`, `E_SERDE_WIRE_NAME_COLLISION`, etc.).

---

## D435 — field-attribute consumption + wire-contract validation (Plan 180.1 Ф.1/Ф.10) {#d435}

**What changed.** D382 defined the `#serde(...)` grammar/AST generally but
consumed only the sum-tagging keys; every other key parsed-but-rejected
(`[M-180-serde-field-attributes]`). This amendment lands **consumption** of
the field/wire-customization keys on **record types** (sum-type rich-attr
support remains a separate follow-up, gated the same way sum auto-derive
richness is — `[M-126-sum-*-rich]`) plus the compile-time wire-contract
validation that rename/alias introduce.

**`SerdeArg` grows** (`compiler-codegen/src/ast/mod.rs`): `Rename(String)`,
`RenameAll(RenameConvention)`, `Skip`, `SkipSerializingIf(String)`,
`Default(Option<String>)` (`None` = bare `default`, `Some(fn)` =
`default = "fn"`), `Alias(String)` (repeatable), `Flatten`,
`DenyUnknownFields`, `AllowUnknown`. `RenameConvention` (new enum): CamelCase /
SnakeCase / KebabCase / ScreamingSnakeCase / PascalCase, parsed from the
`rename_all` string value at PARSE time (unknown convention name →
`E_SERDE_BAD_ATTRIBUTE` naming the supported set, same policy as an unknown
key) and applied via `RenameConvention::apply` (splits the canonical
snake_case field name on `_`, recombines per convention).

**Semantics (per key):**
- **`rename = "wire_name"`** (field-level) — the field's effective wire name.
  **Overrides** a type-level `rename_all` (field-level wins — explicit beats
  derived).
- **`rename_all = "convention"`** (type-level, record only) — every field's
  wire name is `convention.apply(field_name)` unless that field has its own
  `rename`.
- **`skip`** (field-level) — the field is **never** serialized and **never**
  read from the wire on deserialize; its value on decode comes from the SAME
  fallback resolution as bare `default` (a computable zero value: numeric →
  `0`, `bool` → `false`, `str` → `""`, `Option[T]` → `None`, `Vec[T]`/`[]T` →
  `[]`, `HashMap`/`Map` → `.new()`) or, if given, `default = "fn"`'s result. A
  field type with **no** computable zero value and no `default` override →
  **`E_SERDE_SKIP_FIELD_NO_DEFAULT`** (actionable: names the field/type, tells
  the user to add `default = "fn_name"`) — never a silent bad value or an ICE.
- **`skip_serializing_if = "predicate"`** (field-level) — on serialize,
  `@field.<predicate>()` is called; if it returns `true` the
  `struct_field`+value pair is omitted from the wire for that value. General
  form (not `Option`-special-cased): any zero-arg bool-returning method on the
  field's type works (`is_none` on `Option`, but also e.g. `is_empty` on a
  collection). On deserialize the field is unaffected (absence is handled by
  the normal missing-field/`default`/alias machinery below) — this key is
  serialize-direction only, matching serde's own asymmetry.
- **`default` (bare)** — if the field (or all its `alias` candidates, see
  below) is absent from the wire, use the type's zero value instead of
  raising `MissingField`.
- **`default = "fn_name"`** — same trigger, but the fallback value is
  `fn_name()` (a zero-arg function returning the field's type) instead of a
  zero value.
- **`alias = "old_name"`** (field-level, repeatable) — an ADDITIONAL wire name
  accepted on READ ONLY (schema migration: old clients still send
  `old_name`). Resolution order: the field's own (rename/rename_all-resolved)
  wire name first, then each `alias` in declaration order — the first
  candidate PRESENT in the wire object wins (checked via the new
  `Deserializer.has_field`, not by catching a `MissingField`). If NONE of the
  candidates are present: an `Option` field (no explicit `default`) falls
  back to `None` (Q7 semantics, now alias-aware); a field WITH `default`
  (bare or `= fn`) uses that; otherwise (`Required`, no `default`) the
  primary wire name is re-entered so the natural `MissingField(primary)`
  diagnostic still fires, naming the CANONICAL name (not an alias) so the
  error message stays anchored to the type's own schema.
- **`deny_unknown_fields`** (type-level) — **AMENDED MEANING** by D436 below:
  now a no-op synonym of the (new) default; kept accepted for serde-muscle-
  memory rather than becoming a stale `E_SERDE_BAD_ATTRIBUTE` trap.
- **`allow_unknown`** (type-level) — see D436 (Ф.7 default reversal).
- **`flatten`** (field-level) — **parsed, statically validated, synthesis
  GATED.** `E_SERDE_FLATTEN_DENY_CONFLICT` when the type is (now-default)
  strict: a flattened field's inner keys arrive mixed into the parent wire
  object, which the parent's unknown-field scan cannot attribute without
  knowing the child type's field set. `E_SERDE_FLATTEN_UNSUPPORTED` once
  `allow_unknown` removes that specific conflict — actual flatten synthesis
  needs a companion "fields-only" synth variant (reads/writes the child's
  fields directly into the parent's `d`/`s` cursor, with NO
  `begin_struct`/`end_struct`/`enter_field` wrapper of its own) that the
  auto-derive machine does not yet emit. Honest scope-out, tracked
  `[M-180-serde-flatten]` — the hardest item in Ф.1, deliberately not forced.

**New protocol member** (`std/src/encoding/serde/serde.nv` `Deserializer`):
`mut @has_field(key str) -> Result[bool, DeError]` — an exact presence check,
distinct from the existing `enter_field_or_null` (which conflates "key
absent" with "value present and JSON `null`" — correct for `Option`'s
absence-is-`None` semantics, WRONG for `default`/`alias` resolution, which
must distinguish the two). JSON backend (`json.nv`): `@cur.object()?.get(key)
!= None`.

**Compile-time wire-contract validation (Ф.10)**, run once per record type
(`validate_wire_contract`, after rename/rename_all/alias resolution):
- **`E_SERDE_WIRE_NAME_COLLISION`** — two fields' effective wire names
  collide (after `rename`/`rename_all`); an `alias` collides with another
  field's wire name; an `alias` is declared on two different fields.
- **`E_SERDE_SKIP_RENAME_CONFLICT`** — `skip` + `rename` together (rename is
  meaningless on a field that is never on the wire).
- **`E_SERDE_ATTRIBUTE_MISPLACED`** — a field-only key on a type-level
  `#serde(...)` or vice-versa (e.g. `rename` on the type, `rename_all` on a
  field).
- **`E_SERDE_ATTRIBUTE_ON_SUM_UNSUPPORTED`** — `rename_all`/`allow_unknown`/
  `deny_unknown_fields` on a SUM type (Ф.1 v1 scope is record-only; silently
  ignoring would be a worse footgun than a clear gate).
- **`E_SERDE_DUPLICATE_ATTRIBUTE`** — the same key given more than once where
  that is ambiguous (`rename`, `rename_all`, `skip_serializing_if`, `default`
  — NOT `alias`, which is deliberately repeatable).

**Scope note (record vs sum).** All of the above targets `TypeDeclKind::
Record`/`NamedTuple` fields. `SumVariantKind::Record` payload fields (a
struct-shaped enum variant) do NOT yet consume field-attrs — sum rich synth
is a separate gate (`[M-126-sum-*-rich]`); using a field-customization key
inside a sum variant's payload is currently a silent no-op there (unlike the
type-level misplaced-key checks above, which DO cover sum declarations).
Tracked as a known v1 scope boundary, not a defect.

## D436 — unknown-field policy: strict by default (Plan 180.1 Ф.7, REVERSES Plan 180 Q5) {#d436}

**Reversal.** Plan 180's Q5 (`180-serde-derive.md` decision table) chose
serde parity: **ignore unknown wire fields by default**, `deny_unknown_fields`
opt-in strict. **Owner decision 2026-07-22 ("согласен"): REVERSED.** An
unknown field in the wire object is now **rejected by default** —
`Err(DeError{UnknownField(name)})` — with a NEW opt-out key,
`#serde(allow_unknown)`, restoring the old ignore-silently behaviour for
types that deliberately want forward-compatible wire evolution.

**Rationale (owner, verbatim intent).** serde/Jackson's ignore-by-default is
a well-known class of silent bug: a typo'd config key, or a field renamed on
one side of an API and not the other, is accepted and silently dropped —
the value the caller thought they set never takes effect, with no signal at
all. Nova already treats `#serde(...)` attribute typos as a hard compile
error (`E_SERDE_BAD_ATTRIBUTE`, D382) in the same no-magic spirit; treating a
wire-level "typo" (an unexpected key) as silently-fine while a
compile-level typo is a hard error was an inconsistency. AI-generated
client/server code in particular is prone to drifting field names — failing
loudly at the first decode is strictly better than a silently-incomplete
value discovered much later.

**Mechanism.** The record `.deserialize` synthesis (`auto_derive.rs`
`build_unknown_field_check`, called from `synthesize_deserialize`) emits, as
the FIRST statements of the body (unless the type carries
`#serde(allow_unknown)`):
```
ro __nv_wire_keys = d.map_keys()?
for __nv_uk in __nv_wire_keys {
    if <__nv_uk not in known-wire-names> {
        return Err(DeError.new(UnknownField(__nv_uk)))
    }
}
```
`known-wire-names` = every non-`skip` field's resolved wire name plus its
`alias`es (Ф.1). A `skip` field's name is deliberately NOT in that set — a
skipped field is functionally invisible, so a wire key matching its name is,
correctly, still "unknown" under strict policy.

**`deny_unknown_fields` (the OLD opt-in key, D382/Q5) is retained as an
ACCEPTED, explicit no-op** — it now merely re-states the already-active
default; kept rather than turned into a stale `E_SERDE_BAD_ATTRIBUTE` trap
for anyone carrying serde muscle memory. Combining it with the new
`allow_unknown` on the SAME type is a direct self-contradiction →
`E_SERDE_UNKNOWN_FIELD_POLICY_CONFLICT` (D435's validation list).

**`DeErrorKind::UnknownField(str)`** (`serde.nv`, D340) already existed in
the data model (originally documented "at `deny_unknown_fields`" — comment
updated) — this amendment is what makes the record synthesizer actually
raise it, closing that latent gap.

**Migration.** Audited every `#impl(... Deserialize ...)` type in the tree
at landing time (`std/`, `spec_tests/`, `nova_tests/`, `examples/` including
the flagship `aggregator`): **zero** existing record type decodes a
hand-written wire literal carrying a field outside its own schema — the only
DTOs consuming `Deserialize` at all live beside `std/encoding/serde` itself
(the flagship's JSON DTOs, `examples/flagship/aggregator/src/api/
report_json.nv` / `main.nv`, are `#impl(Serialize)`-only — snapshot/wire
output, never decoded back), so no pre-existing fixture needed a behavioural
migration to `allow_unknown` or a tightened DTO. New pos coverage:
`std/src/encoding/serde/field_attrs_test.nv` (strict-by-default → `Err
{UnknownField}`; `#serde(allow_unknown)` opt-out pos-case).

**Peer-table update.** `180-serde-derive.md` §2's `unknown-field-policy` row
("`= IGNORE by-default ... (serde-паритет)`") is superseded by this decision
— Nova is now **`🏆` strict-by-default** (stricter than serde/Swift/Kotlin's
ignore-default; matches kotlinx's protectiveness by DEFAULT rather than by
opt-in `@JsonClassDiscriminator`-adjacent configuration, while still offering
an explicit, opt-in escape hatch serde-style forward-compat APIs still need).

---

## D438 — `Reflect` protocol + `TypeShape` structural reflection (Plan 222.8 Ф.1, 2026-07-27) {#d438}

**What.** A 9th built-in auto-derive protocol, `Reflect` (`std/src/reflect.nv`,
`module std.reflect`), alongside Equal/Hash/Clone/Compare/Display/Debug/
Serialize/Deserialize (D237/D340/D341). `.reflect() -> TypeShape` — a
**STATIC** method (no receiver value: it describes a TYPE, not an instance)
returning a **format- and domain-independent** description of a type's
structural shape. Not a word about HTTP/OpenAPI/JSON anywhere in the
protocol or `TypeShape` itself — deliberately general enough for an
OpenAPI-schema emitter, a GraphQL SDL generator, a CLI-arg generator, or a
debug pretty-printer, all reading the SAME tree. The first consumer is
`nova-polaris`'s OpenAPI generator (Plan 222.8 Ф.2/Ф.3, `docs/plans/
222.8-openapi-gen.md`) — deliberately out of THIS decision's scope; `Reflect`
carries no knowledge of HTTP roles (`Path`/`Query`/`Json` wrappers), response
codes, or wire formats. That interpretation lives in the consuming library,
not the compiler or std.

**`TypeShape`** (`std/src/reflect.nv`):

```nova
export type SumRepr enum
    | External
    | Tagged(tag str)
    | TaggedContent(tag str, content str)
    | Untagged

export type TypeShape enum
    | Record(name str, fields [](str, TypeShape))
    | Sum(name str, repr SumRepr, variants [](str, TypeShape))
    | Ref(name str)
    | Str | Int | Float | Bool | Unit
    | Arr(items TypeShape)
    | Opt(inner TypeShape)
    | Opaque(name str)

export type Reflect protocol {
    .reflect() -> TypeShape
}
```

`Reflect` is a **structural** protocol (D42) like every other Nova protocol:
a type satisfies it by providing a matching `.reflect()` method, with or
without `#impl(Reflect)`. `#impl(Reflect)` on a type declaration REQUESTS
compiler synthesis of that method (opt-in, same as the other 8 auto-derive
protocols) — it is not a conformance marker. A type providing its own
hand-written `.reflect()` satisfies `Reflect` without ever writing
`#impl(Reflect)` (see `Opaque` below).

**Compiler synthesis (`compiler-codegen/src/protocols/auto_derive.rs`,
`synthesize_reflect`).** Same field-walk infrastructure as Serialize/
Deserialize (D340/D341/D435) — reused, not duplicated:
- **Record field wire-names** — `resolve_fields`/`wire_name_for` (D435):
  `TypeShape.Record`'s `fields` list carries names AFTER `rename`/
  `rename_all`/alias resolution — the WIRE contract, not the raw Nova field
  name. `#serde(skip)` fields are excluded (never on the wire, so absent
  from the schema too — a schema entry for a field that never serializes
  would be actively misleading to a consumer like an OpenAPI emitter).
- **Sum representation** — `serde_tagging_mode` (D382/D435), reused
  WHOLESALE (not re-parsed): `SumRepr.External`/`Tagged(tag)`/
  `TaggedContent(tag, content)` mirror `SerdeTagging::External`/`Internal`/
  `Adjacent`. **This means `Reflect` synthesis inherits the SAME
  `#serde(untagged)` gate as Serialize/Deserialize**
  (`E_SERDE_UNTAGGED_GATED`, `[M-180-untagged-codegen-mono]`) even though
  `Reflect` itself never touches `json.nv`'s mono-ordering — a deliberate,
  documented simplification (favor code reuse over a parallel permissive
  path for a single field-count of extra freedom); revisit if/when the
  untagged codegen gate lifts.
- **Sum variant shapes** (no existing precedent to reuse — new Ф.1 choice):
  unit variant → `Unit`; single-element tuple variant → the element's shape
  DIRECTLY (transparent, mirroring serde's own "single payload → bare
  content" treatment, e.g. adjacent-tagged `Val(int)` → `{"t":"Val","c":9}`,
  not `{"t":"Val","c":[9]}`); multi-element tuple variant → a synthetic
  `Record(variant_name, [("0", ..), ("1", ..)])` (positional fields named by
  index); record-payload variant → `Record(variant_name, [(field, ..), ..])`
  using RAW field names — sum-variant record fields do not yet consume
  `#serde(rename...)` (D435's own scope note, `[M-126-sum-*-rich]`), mirrors
  `synthesize_serialize`'s identical choice for the same reason.
- **Field/variant-payload eligibility** — a NEW `check_field_eligibility_
  reflect` (mirrors `check_field_eligibility_serde`'s shape: bespoke
  `Option`/`Vec` recursion, a named type must provide an explicit
  `.reflect()` or declare `#impl(Reflect)`) — narrower scalar set than
  serde's (no wire-precision concern: every integer width maps uniformly to
  `Int`, no widen/narrow split; `char`/`byte`/`i128`/`u128` excluded, same
  reasoning as serde — no faithful `TypeShape` leaf for them). A nested type
  without `Reflect` conformance is a typed `E_AUTO_DERIVE_FIELD_LACKS_
  PROTOCOL`, mirroring Serialize's identical diagnostic class (not an ICE,
  not a silent hole).

**Recursion (`Ref`) — the one genuinely NEW mechanism, no serde precedent.**
Unlike Equal/Hash/Serialize (per-INSTANCE protocols whose recursion always
terminates on finite RUNTIME data — a linked list's `@equal` bottoms out
because the VALUE is finite), `TypeShape` describes the TYPE GRAPH itself,
which can be genuinely cyclic (self-referential: `type Node { next
Option[Node] }`; or mutually recursive: `A` embeds `B`, `B` embeds
`Option[A]`). The synthesizer inlines nested named types' shapes directly
(builds `Record`/`Sum` literals recursively via `build_type_decl_shape`, NOT
by dispatching runtime calls to each type's own separately-synthesized
`.reflect()` — a dispatch chain would recurse unconditionally at runtime
with no data-driven base case, since `.reflect()` takes no `self` to pattern-
match on). An `in_progress` stack tracks the chain of type names currently
being inlined; a field naming one of THOSE types emits `TypeShape.Ref(name)`
instead of expanding further. This is the ONLY place a cycle is broken, so
the produced value is finite BY CONSTRUCTION (a compile-time graph-cycle
check), never by a depth limit, and handles indirect/mutual cycles too (by
the time `B`'s own inline expansion reaches its field back to `A`, `A` is
still on the SAME shared stack `B`'s expansion inherited). A type with an
EXPLICIT hand-written `.reflect()` (not `#impl(Reflect)`-derived) is
DISPATCHED (a plain static call), never inlined — the compiler does not
know what a hand-written body does, so it trusts it rather than expanding
it; this is also how `Opaque` composes safely with the recursion machinery
(see below).

**`Opaque(name)`** (owner addition, 2026-07-27, mid-Ф.1). A `TypeShape` leaf
meaning "this type's shape is intentionally not described" — for raw/
unshapeable types (e.g. a raw `ServerRequest` handle that appears as an
extractor-bundle field per 222.8 §1.3, but is never itself part of a JSON
schema). **The compiler NEVER synthesizes `Opaque` itself** — no "which
types are opaque" policy exists anywhere in `auto_derive.rs`; that decision
belongs entirely to whoever writes a manual `.reflect()` implementation
(typically a library wrapper type). Consumers (an OpenAPI emitter, etc.)
are expected to skip/omit `Opaque` fields from generated schemas.

**Blanket implementations** (`std/src/reflect.nv`, `.nv` code, not compiler
special-cases — same style as `Display`/`Debug`'s primitive blanket bodies
in `prelude/protocols.nv`): `int`/`i8..i64`/`uint`/`u8..u64` → `Int`;
`f32`/`f64` → `Float`; `bool` → `Bool`; `str` → `Str`; `fn[T Reflect] []T.
reflect() -> TypeShape => Arr(T.reflect())`; `fn Option[T Reflect].reflect()
-> TypeShape => Opt(T.reflect())`. These blankets are NOT invoked by the
compiler's own record/sum synthesis path (which builds `Arr(..)`/`Opt(..)`
directly inline for the same cycle-safety reason as above) — they exist for
general/library code that wants a container's shape without going through
auto-derive (e.g. `Vec[SomeType].reflect()` called directly).

**Scope boundary (explicit, Ф.1).** Tuple-typed FIELDS (`(A, B)` as a
record field's own type, not a sum-variant payload) are NOT supported —
`check_field_eligibility_reflect` has no `Tuple` arm, unlike the generic
`check_field_eligibility` used by Equal/Hash/Clone/Compare (a documented
narrower scope, `[M-222.8-reflect-tuple-field]` if ever needed). `resolve_
fields`'s wire-collision validation (`E_SERDE_WIRE_NAME_COLLISION` etc.) is
inherited for record types (a `Reflect`-only type without `#impl(Serialize)`
still gets that validation "for free" via the shared `resolve_fields` call)
— a deliberate, documented consequence of reuse, not a targeted feature.
`Result[T, E]` has no blanket (deferred — 222.8 §1.2 "см. 1.3-статусы").
Doc-comments → `description` is explicitly OUT of scope (222.8 §3, owner
decision: a docstring may carry internal notes, auto-publishing it is
unsafe without opt-in) — `TypeShape` carries no description/doc field at
all.

**Why not fold into `Serialize`** (per 222.8 §0, already-rejected designs
carried forward, not re-litigated here): `Serialize` is a general capability
not tied to JSON (§0.3 of the 222.8 plan); baking JSON-Schema-shaped output
into it would be a layering violation, and would cost binary size for every
`#impl(Serialize)` type that never touches HTTP. `TypeShape` is a SEPARATE,
opt-in protocol for exactly this reason.

---

## D417. Closure-литерал против скалярного return-типа — `E_CLOSURE_SCALAR_RETURN` (2026-07-10)

**Дыра.** Ни `assignable` (сверка типов только в позициях call-arg / annotated-`let`),
ни literal-coercion канал (`materialize_literal_coercion` — материализует ширину
INT-литерала, но НИЧЕГО не отвергает) не сверяли closure-литерал (`|| body` /
`|x| body` / typed `fn(...) ...`), попавший в return-позицию функции (implicit
tail — trailing блока/arrow-body, либо explicit `return X`), с объявленным
return-типом. Codegen лоуэрит closure в указатель на функцию; если объявленный
тип — скаляр (`bool`/int-family/`f32`/`f64`/`char`), указатель молча
бит-реинтерпретируется в скаляр (напр. `bool` — ВСЕГДА `true`) БЕЗ диагностики.

**Найдено** 2026-07-10 при расследовании `[M-toml-repeated-fail-call-run-fail]`
(std/encoding/toml.nv `is_bare_key_char`) — типовой триггер: многострочное `&&`/
`||`-выражение, где программист по Rust/C-привычке ставит продолжающий оператор
В НАЧАЛЕ следующей строки:

```nova
fn f(c char) -> bool {
    ro n = c as int
    (n >= 65 && n <= 90)
    || (n >= 97 && n <= 122)   // ведущий `||` — НЕ OR-продолжение!
}
```

Парсер (`parse_or`) **намеренно** не продолжает OR-цепочку через ведущий `||`
на новой строке — синтаксис зарезервирован за zero-arg closure-литералом
(`|| body`), и продолжение создало бы неоднозначность с реальным
closure-statement'ом. Следствие: вторая строка парсится как СВОЙ, ОТДЕЛЬНЫЙ
statement — closure-литерал `|| (n >= 97 && n <= 122)` — который, будучи
последним statement'ом блока, становится implicit-return значением. До этого
амендмента компиляция проходила молча; после — `nova check`/`nova test-build`
отвергают такое тело кодом `E_CLOSURE_SCALAR_RETURN`.

**Проверка** (`compiler-codegen/src/types/mod.rs`,
`TypeCheckCtx::check_closure_scalar_return` + `_in_block`/`_in_stmt`/`_in_expr`
walk-семейство, зеркалящее reachable-позиции существующего
`materialize_returns_in_block`/`_in_expr`): для каждой return-позиции fn'а
(implicit trailing блока/arrow-body И каждый explicit `return X`, в т.ч.
вложенный в `if`/`match`/`while`/`for`/`loop` того же execution-context —
НЕ внутрь `detach`/`spawn`/`parallel for`/вложенных closures, чей `return`
принадлежит другому execution-context) — если значение является closure-
литералом (`ExprKind::Lambda`/`ClosureLight`/`ClosureFull`) И объявленный
return-тип резолвится (`ResolvedType::from_type_ref`, сквозь
`ro`/`mut`/`unsafe`-модификаторы) в `Bool`/`Scalar`/`Float`/голый
`char` — ошибка. Возврат closure против **fn-типа** (`TypeRef::Func` —
легитимный HOF-возврат) НЕ флагуется — это единственный legal target для
closure-значения.

**Код:** `E_CLOSURE_SCALAR_RETURN` (новый, по образцу `E_RECORD_PATTERN_NEEDS_REST`/
`E_REFUTABLE_BINDING` — стабильный описательный `E_*`, не порядковый номер).
Диагностика указывает на closure-литерал (не на fn-декларацию) и explicitly
называет типовой триггер (ведущий `||`/`|x|` на continuation-строке) как
подсказку для читателя. Fail-fast fix для программиста: перенести
продолжающий оператор в КОНЕЦ предыдущей строки (`(...) ||` \ трейлинг —
легальное продолжение, в отличие от ведущего).

**Область.** Гейт — именно СКАЛЯР (`bool`/int-family/float/`char`); `str`/
`Any`/произвольный `Named`-тип в скоуп ЭТОГО амендмента намеренно не входят
(дыра шире, но подтверждённый репро и зафиксированный P2-маркер —
`[M-closure-trailing-scalar-coercion-no-typecheck]` — именно про скаляр;
расширение до общего closure-vs-non-fn-type mismatch — отдельный follow-up
при необходимости).

**Побочный улов.** Тот же класс бага (ведущий `||` на continuation-строке)
обнаружился этой проверкой ещё в двух местах std — `std/data/semver.nv`
`is_ascii_ident_char` и `std/encoding/csv.nv` `needs_quoting` — обе функции
ДО фикса возвращали `true` для ЛЮБОГО входа (closure-указатель, коэрснутый в
`bool`, always-truthy). Обе мигрированы на трейлинг-`||` (тот же канон, что
`toml.nv`'s `is_bare_key_char`) в той же волне.

## D419. `Fmt` protocol — format-spec context для `@display_fmt` (Plan 152.7.2, 2026-07-13)

> **AMEND (Plan 208 Ф.0, 2026-07-15) — RETRACT/SUPERSEDED целиком → [D422](#d422-unified-formatter--единый-displaymut-f-fmt--debug-байтовый-write-zero-alloc-pad-plan-208-2026-07-15).**
> Два опциональных метода (`@display(mut w Write)` + отдельный `@display_fmt(mut f Fmt)`)
> сворачиваются в ОДИН required-примитив `@display(mut f Fmt)`; `@display_fmt`-путь удаляется
> целиком (не остаётся вторым методом рядом). `Fmt` из D419 (`@write(s str)` + `alternate`/
> `precision` — «Fmt поверх Write», два метода) заменена на D422-`Fmt` (`use Write` protocol-embed,
> D145, + полный набор осей `width`/`align`/`fill`/`sign`/`alternate`/`kind`/`pad`). Диспетч
> «`@display_fmt` есть → зовём его, иначе прежний `@display`+внешний pad» (§«Правило диспетча»
> ниже) заменяется единым «`@display(f)` ВСЕГДА диспетчится, `pad_consumed` решает — внешний
> pad или нет» (D422 §«Инвариант»). **Статус: целевая модель (Ф.1-4 pending, см. D422 §Статус) —
> текст ниже читать как ИСТОРИЮ (что было ДО D422), не текущее/будущее поведение.**
>
**Закрывает** [Q-format-spec-to-display](../open-questions.md#q-format-spec-to-display--передача-формат-спека-в-displaydebug-pretty-и-др--движок-интерполяции-без-промежуточных-аллокаций--🟡-open-2026-07-13)
и обновляет протухший followup-маркер `[M-91.14-format-dsl-extensions]`
(перечислял уже реализованное — 152.7-B закрыл `:hex`/`:pad-N`/`:.3` до этого
амендмента; единственный реальный остаток был «типу не передаётся спек»,
что и закрывает это решение).

#### Проблема

До D419 встроенный rich format-spec (`${x:[[fill]align][sign][#][0][width]
[.precision][type]}`, D258/152.7-B) применялся СНАРУЖИ к выводу
`Display.@display` — тип не видел собственный спек. `#` (alternate)
существовал только для integer radix-префиксов (`0x`/`0o`/`0b`); для
user-типов флаг молча игнорировался. Pretty-печать (`JsonValue.to_str_pretty`)
жила отдельным методом, не связанным с интерполяцией.

#### Решение — три развилки (см. Q-блок), решены так:

**(а) Форма контекста.** Второй ОПЦИОНАЛЬНЫЙ метод, не расширение сигнатуры
`@display`: `@display_fmt(mut f Fmt)`. `Fmt` — protocol ПОВЕРХ `Write`
(структурно расширяет его: тот же метод `@write(s str)`, НЕ переименованный
`@write_str` — это сохраняет «Fmt поверх Write» буквально верным: любой `Fmt`
структурно удовлетворяет и `Write`) плюс две новые оси:

```nova
export type Fmt protocol {
    mut @write(s str) -> ()
    @alternate() -> bool
    @precision() -> Option[int]
}
```

Единственный production-implementor V1 — `FmtCtx` (конкретный record,
`std/prelude/protocols.nv`), который компилятор конструирует НА КАЖДОМ
call-site `${x:SPEC}`, диспетчащем в `@display_fmt`:

```nova
export type FmtCtx {
    sink Write
    alt bool
    has_prec bool
    prec int
}
export fn FmtCtx.new(sink Write, alt bool, has_prec bool, prec int) -> Self => …
export fn FmtCtx mut @write(s str) -> () { @sink.write(s) }
export fn FmtCtx @alternate() -> bool => @alt
export fn FmtCtx @precision() -> Option[int] =>
    if @has_prec { Some(@prec) } else { None }
```

ABI: `Fmt` эрайзится к конкретному C-типу `Nova_FmtCtx*` ТЕМ ЖЕ приёмом, что
`Write` уже эрайзится к `Nova_StringBuilder*` (152.7.1 D374 AMEND) — не
generic protocol-boxing (`NovaBox_<proto>`/vtable), явный V1-компромисс
(апгрейд до реального vtable — future followup, как и у `Write`).

**(б) Без ломки.** Сигнатуры `@display(mut w Write)` / `@debug(mut w Write)`
НЕ меняются — D374 не переламывается второй раз. `@display_fmt` — целиком
опциональный второй метод; тип без него получает ровно ПРЕЖНЕЕ поведение
(auto-делегация в `@display`/D410-`to_str`-fallback + внешняя пост-обработка
width/align/fill/precision).

**(в) Какие оси передаются типу.** Только `alternate` (`#` — канонический
pretty-флаг) и `precision` (`.N`). `width`/`fill`/`align` ОСТАЮТСЯ
пост-обработкой у вызывающего — тип не должен уметь паддить себя сам (тот же
`nova_fmt_pad` внешний проход, что и раньше). `radix` (`x`/`X`/`b`/`o`) —
без изменений: встроенный числовой путь, к user-типам неприменим
(`E_BAD_FORMAT_SPEC` как раньше).

#### Правило диспетча (`${expr:SPEC}`, ТОЛЬКО non-`?` rich-spec ветка)

1. Если `typeof(expr)` объявляет `@display_fmt` — вызывается ОН, с
   `FmtCtx`, сконструированным из разобранного спека (`alt`/`has_prec`/
   `prec` — из `FormatSpecParsed`, compiler-codegen/src/ast/format_spec.rs).
2. Иначе — прежний путь: `Display.@display` (или D410 `to_str`-fallback) в
   свежий `StringBuilder`, затем внешняя пост-обработка
   (precision-truncation + `nova_fmt_pad`) — байт-в-байт как до D419.
3. `${expr:?}` (Debug) — ВНЕ этого правила, не затронут; `@display_fmt`
   зеркалит только `Display`.
4. `E_FORMAT_SPEC_UNKNOWN` (парсер, syntax-level — неизвестный type-char
   типа `${x:foo}`) — БЕЗ изменений для всех типов (парсер типа не знает);
   «смягчение только для типов с `@display_fmt`» относится к СЕМАНТИКЕ
   `#`/`.N`, не к синтаксической диагностике: до D419 `#`/`.N` на
   user-типе БЕЗ ошибки, но МОЛЧА игнорировались (`#` не имел эффекта);
   после D419 — типы С `@display_fmt` получают РЕАЛЬНЫЙ эффект этих осей,
   типы БЕЗ — прежнее молчаливое игнорирование (не регрессия, задокументированный
   status quo).

#### Первый потребитель

`JsonValue @display_fmt` (`std/src/encoding/json.nv`): `f.alternate()` →
`@to_str_pretty()` (существующий `pretty_at`), иначе `@to_str()` (compact).
`"${v:#}" == v.to_str_pretty()`; `"${v}" == v.to_str()` (unaffected).

#### Codegen

`compiler-codegen/src/codegen/emit_c.rs::emit_format_spec_value` — user-type
branch: lookup `(arg_type, "display_fmt")` в `all_methods` (тот же реестр,
что и `@display`/`@debug`-lookup рядом); при hit — constructs `FmtCtx` inline
(`Nova_FmtCtx_static_new(sink, alt, has_prec, prec)`) и зовёт
`Nova_<T>_method_display_fmt(v, fmtctx)`; при miss — байт-в-байт прежний
`@display`/`@debug` путь. ABI-эрейзинг `Fmt`→`Nova_FmtCtx*` — три
hardcode-точки, зеркалящие существующие `Write`→`Nova_StringBuilder*`
(`resolved_named_to_c`, `debt_lowered_is_stub`, `extract_protocol_type_name`
E7201-exclusion).

#### Прямо-в-sink для примитивов (Q-блок п.4) — НЕ в этом амендменте

Q-блок поднимал отдельно движок интерполяции «прямо-в-sink» (`${x}` для
ЛЮБОГО типа лоуэрится в `@display`/`@debug` без промежуточной `str`,
`str.from` перестаёт быть движком интерполяции). D419 НЕ трогает эту часть —
value-record-путь (`${d}`/`${d:?}` для value-records) уже пишет напрямую в
sink (Plan 175 Ф.3(d)); примитивы/fallback-путь (`nova_int_to_str` и т.п.
внутри `emit_interpolated_str`) остаются как есть. Остаток —
[M-152.7.2-interp-direct-primitives].

#### Cross-refs

- [D374](#d374-write-sink-протокол--декаплинг-displaydebug-от-stringbuilder-plan-1527176)
  — `Write` sink protocol (Fmt расширяет структурно, тот же erasure-приём).
- [D229](#d229--Debug-protocol--format-spec-expr) — Debug, sibling, НЕ
  затронут этим амендментом.
- D258/152.7-B (`spec/decisions/03-syntax.md`) — rich format-spec grammar
  (parser, `FormatSpecParsed`), source of `alternate`/`precision` values.
- [M-91.14-format-dsl-extensions] — CLOSED by this decision (протухший
  followup обновлён; реальный остаток формализован здесь).
- Q-format-spec-to-display (`spec/open-questions.ru.md`) — RESOLVED → D419.
- Plan 152.7.2 (докладной подплан 152.7 — интерполяция и форматирование; this D-block's home plan).

---

## D422. Unified Formatter — единый `@display(mut f Fmt)` + `@debug`, байтовый `Write`, zero-alloc pad (Plan 208, 2026-07-15)

**Keystone.** Решение владельца 2026-07-15 (все развилки закрыты, полная карта —
[docs/plans/208-unified-formatter.md](../../docs/plans/208-unified-formatter.md)). Сворачивает
D419's двух-методную схему (`@display`/`@display_fmt`) в ОДИН required-примитив на тип, унифицирует
`Write` с байтовым `io.Write` (Plan 176), убирает C-поверхность форматирования до одного
float-extern.

### Мотивация

Одна поверхность форматирования вместо дублей: **один метод** `@display(mut f Fmt)` (не пара
`@display`+`@display_fmt`, как в D419); `Write` пишет `[]u8`, не `str` — фундаментальнее,
унифицирует с `io.Write`; буфер-примитивы (`int`/`bool`/`char`/радикс/pad) переезжают в `.nv`,
C остаётся ТОЛЬКО на float-body (dtoa/Ryu-класс непортируем). Максимизирует nv-sourcing (§3
compiler-conventions).

### Правило

#### 1. `Write` — байтовый sink, минимальный, инфаллибельный

```nova
export type Write protocol {
    mut @write(bytes []u8) -> ()      // параметры ro по умолчанию
}
```

Единственный метод. Никакого `@write(str)`-overload'а: str-значение → `w.write(s.bytes())`
(явный `.bytes()`, [D176](#d176-ro-t--тип-модификатор)); str-**литерал** `w.write("...")` →
**коэрсия литерала в `[]u8`** (амендмент [D55](#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы),
§2 ниже). `@reserve`/`@advance` **НЕ** в протоколе (были рассмотрены и отклонены — сырой
`*mut u8` не течёт в общий протокол, io-sink не может дать указатель на будущие байты) — эти
два метода остаются **конкретными** методами `StringBuilder` (см. §5); компилятор, зная что
sink конкретно `StringBuilder`, использует их напрямую для zero-**copy** top-level рендера;
generic-`@display`/`@debug` (sink видим только как `Fmt`/`Write`) рендерят примитив в
стек-буфер + один `@write(slice)` → zero-**alloc** (без кучи), без copy на sink-стороне, как
`fmt::Write` у Rust.

io-`Write` (Plan 176) — ОТДЕЛЬНЫЙ протокол-РОДСТВЕННИК: тот же байтовый шейп `@write([]u8)`,
но фаллибельная сигнатура `-> Result[(), IoError]` (std=Result, не `Fail`-эффект, конвенция
[D325](04-effects.md#d325)). Направления `Read`/`Write` НЕ сливаются. Мост fmt→io: собрать в
`StringBuilder` (инфаллибельно) → `file.write(sb.bytes())?`.

#### 2. `Fmt` — sink + оси спека, embeds `Write` через `use` (D145 protocol-embed)

```nova
export type Fmt protocol {
    use Write                        // компонует @write([]u8) из Write (D145 protocol composition)
    @width()     -> Option[int]
    @precision() -> Option[int]
    @align()     -> Option[Align]
    @fill()      -> char
    @sign()      -> Sign
    @alternate() -> bool
    @kind()      -> FmtKind
    mut @pad(bytes []u8) -> ()       // тип-управляемый паддинг → ставит pad_consumed
}
```

`use Write` — та же протокольная композиция, что [D145](#d145-fnt-префикс--receiver-generic-decl--bounds-plan-101)
§«Protocol composition» (`use A, B` внутри `protocol {}`, парсер `parse_protocol_body`,
type-check flatten): `Fmt` получает `@write([]u8)` из `Write` в свой method-set, а не
переопределяет его отдельным именем (`@write_str`, как в до-D422 D419-версии) — любой `Fmt`
структурно satisfies и `Write`.

`FmtCtx` — конкретный реализатор, компилятор строит его на каждом `${x:SPEC}`:

```nova
export type FmtCtx {
    sink Write            // главный StringBuilder (или под-регион при pad_in_place)
    mark int              // старт тела в sink (для pad_in_place)
    spec FormatSpec        // width/align/fill/sign/alternate/precision/kind
    mut pad_consumed  bool
    mut prec_consumed bool
}
```

#### 3. `Display`/`Debug` — REQUIRED-примитив, без to_str-дефолта, без цикла

```nova
export type Display protocol { @display(mut f Fmt) -> () }   // REQUIRED — нет дефолта
export type Debug   protocol { @debug(mut f Fmt) -> () }     // REQUIRED
```

**Инвариант «нет циклической ловушки»:**

1. `@display`/`@debug` — **required-примитив**; дефолта, который бы звал `@to_str`, НЕТ (это
   и была ловушка D419-эпохи: to_str-дефолт мог рекурсивно звать себя).
2. `@to_str` — бланкет **bare-T** (`fn[T] T @to_str() -> str`) — вызывается
   ТОЛЬКО на типе, уже реализующем `Display` (уже имеющем реальный `@display`) → зовёт
   настоящий примитив, не себя. Цикл структурно невозможен.
3. **Auto-derive** структурных типов (record/sum/tuple): компилятор синтезирует реальный
   `@display`/`@debug` по требованию → структурный тип Display-способен без ручного impl.
4. Тип без `@display` и не-деривируемый (опак без полей) → **не Display** → `${x}` =
   **compile-error** «type X не реализует Display», не бесконечная рекурсия.

> **АМЕНДМЕНТ 2026-08-17 (аудит самосогласованности, раздел 4, пункт 16).**
> Здесь стояло «бланкет **bounded `Display`** (`fn[T Display] T @to_str()`)»,
> что расходилось с амендментом D410 и с реализацией. Правда — bare-T:
> `std/src/runtime/string/core.nv:293` объявляет
> `export fn[T] T @to_str() -> str => "${@}"` без всякого bound'а, а
> комментарии `std/src/runtime/char.nv:26` и `char_test.nv:9` прямо говорят,
> что методы резолвятся ТОЛЬКО через bare-T blanket. Разница не косметическая:
> bounded-версия обещает, что `@to_str` доступен лишь типам, реализующим
> `Display`, — читатель на этом строит вывод «значит есть проверка», которой
> нет.

#### 4. Derived Display vs Debug — различаются формой

Auto-derive синтезирует ОБА метода, но с разной формой (владелец 2026-07-15, намеренный отход
от Rust — Rust деривит только `Debug`):

- **derived `@debug`** (`${x:?}`) — техдамп С ИМЕНАМИ полей: `Point { x: 1, y: 2 }`;
  sum: `Some(5)` / `Err(IoError { kind: NotFound, ... })`; pretty (`:#?`) — многострочно
  через `f.alternate()`.
- **derived `@display`** (`${x}`) — компактная «значенческая» форма БЕЗ имён полей:
  `Point(1, 2)`; sum: `Some(5)` (payload как значение).
- Примитивы: Display и Debug совпадают (`42`, `true`) — различие только для структурных типов.
  Кастомный `@display` перекрывает derived-форму.

#### 5. Буфер-примитивы — внутренние (`.nv`, не публичные), zero-alloc

Канон-сигнатуры ниже — value-first + type-first имена + БЕЗ суффикса `_into`
([§10R-Д1-Д3](../../docs/plans/208-unified-formatter.md#10r-д---три-нормы-дополнения-владелец-2026-07-21-переданы-исполнителю-брифом-здесь--source-of-truth),
владелец 2026-07-21, реализовано Ф.4R Ш4 — заменяет доредизайновый набросок этой подсекции):

```nova
export unsafe fn int_fmt(v int, buf *mut u8, cap int, spec FmtSpec = FmtSpec.new()) -> int  requires cap >= 0
export unsafe fn bool_fmt(v bool, buf *mut u8, cap int) -> int
export unsafe fn char_fmt(v char, buf *mut u8, cap int) -> int               // UTF-8 encode
// float — ЕДИНСТВЕННЫЙ C-extern (dtoa непортируем); "простой рендер" = дефолт-арги, НЕ отдельный
// `_into`-мост (Ф.4R Д3):
extern "C" fn nova_f64_fmt(v f64, buf *mut u8, cap int, kind int, prec int) -> int
export unsafe fn f64_fmt(v f64, buf *mut u8, cap int, kind FloatKind = FloatKind.Shortest, prec int = -1) -> int  requires cap >= 0
export unsafe fn f32_fmt(v f32, buf *mut u8, cap int) -> int  requires cap >= 0
```

`extern "C" fn` + **литеральное имя** (без `nova_`-префикса) — по
[D282](08-runtime.md#d282-new--extern-nova-fn--extern-c-fn--двух-abi-синтаксис-для-ffi-plan-9112-ф-1)
(`extern "nova" fn` добавляет `nova_fn_`-prefix; `extern "C" fn` — нет, литеральный C-symbol).
`FloatKind` пересекает C-ABI как int (`0=Shortest/1=Fixed/2=Sci`), `.nv`-wrapper конвертит
enum→int на границе.

#### 6. Энумы (D406 `enum`-маркер)

```nova
type Align     enum Left | Right | Center
type Sign      enum Minus | Plus
type FmtKind   enum Display | Debug | Hex | Oct | Bin | Exp
type FloatKind enum Shortest | Fixed | Sci        // для f64_fmt (C-ABI int, не пересекает границу как enum)
```

Все — `enum`-маркер синтаксис ([D406](#d406-sum-type-синтаксис-enum-маркер-2026-07-01)), не
leading-`|`.

#### 7. `StringBuilder` — аменд API (см. [D179](08-runtime.md#d179-stringbuilder--pure-nova-consume-type--plan-91-ф26))

```nova
fn StringBuilder mut @reserve(n int) -> *mut u8
fn StringBuilder mut @advance(n int) -> ()
fn StringBuilder @len() -> int
fn StringBuilder mut @pad_in_place(mark int, width int, fill char, align Align) -> ()
fn StringBuilder mut @write_padded(bytes []u8, width int, fill char, align Align) -> ()
fn StringBuilder consume @into_str() -> str                          // assume-valid UTF-8
fn StringBuilder consume @into_str_checked() -> Result[str, Utf8Error]
```

`@reserve`/`@advance` — компилятор-приватный zero-copy путь (§1); `@pad_in_place` — width-
композит без известной длины заранее (streaming-композит: record/tuple/Vec/sum), memmove
тела + fill-вставка (right/center), left — без сдвига; `@write_padded` — известная-длина
примитивы (int/float/bool/char/str), рендер в стек-буфер/расчёт, потом padded-запись без
сдвига. `pad_consumed`: если тип сам вызвал `f.pad(...)` внутри своего `@display`/`@debug`,
компилятор внешний pad НЕ навешивает (зеркало прежнего `precision_consumed`).

### Статус реализации

**Дизайн финализирован 2026-07-15** (все развилки закрыты — Ф.0-4 карта исполнения, гейты,
риски — см. [docs/plans/208-unified-formatter.md](../../docs/plans/208-unified-formatter.md)
§9-§11).

| Фаза | Что | Статус |
|---|---|---|
| Ф.0 | Эта спека (D422 keystone + amend-пометки D419/D374/D237/D229/D179 + D55-аменд) | ✅ 2026-07-15 |
| Ф.1 | Буфер-примитивы `.nv` (аддитивно, рядом с `conv.h`); `fmt_f64_into` C-extern; `StringBuilder`-аменд | ✅ 2026-07-15 (`std/src/runtime/fmt_buf.nv`, `string_builder.nv` — merge `b6ee6f40a`) |
| Ф.2 | Когерентная волна: `Write`/`Fmt`/`FmtCtx`/энумы в std; компилятор — переписка `emit_interpolated_str`/`emit_format_spec_value` на `@display(f)`/`@debug(f)`; удаление `@display_fmt`-пути; ретракт `str.from_debug` | ✅ 2026-07-16 (ветка `p208-impl`) — **с тремя V1-упрощениями, см. подсекцию ниже** |
| Ф.3 | Дженерики `.nv` (`[]T`/`Vec[T]`/`Option`/`Result` Display/Debug) + auto-derive record/sum/tuple (компактная `TypeName(a, b)` форма Display, отличная от именованной Debug-формы) | ✅ 2026-07-16 (ветка `p208-impl`, волна 2) — см. `docs/plans/wip/208-impl-progress.md` §"Ф.3 — генерики .nv + auto-derive" |
| Ф.4 | Зачистка: оставшийся `conv.h` → `.nv`; удаление мёртвого `nova_fmt_*` | ⏳ pending — **заблокирована** (разведка волны 2 подтвердила и УГЛУБИЛА блокер Ф.2's V1-упрощения #1, см. ниже): примитивный форматный путь (bare + rich-spec, `emit_interpolated_str`/`emit_format_spec_value`) сознательно НЕ перевязан на буфер-примитивы Ф.1 — `conv.h`'s `nova_fmt_*`/`nova_*_to_str`/`nova_*_to_debug_str` остаются ЖИВЫМИ (не мёртвыми), так что «удалить мёртвый nova_fmt_*» пока буквально нечего удалять. Волна 2 нашла ДОПОЛНИТЕЛЬНЫЙ блокер: буфер-примитивы Ф.1 не имеют quote/escape-логики для Debug str/char (нужна с нуля) — см. `wip/208-impl-progress.md` §"Ф.4 — статус: РАЗВЕДКА" |
| Ф.4R | Редизайн зачистки (owner 2026-07-20) + §10R-Д1-Д3 нормы-дополнения (owner 2026-07-21): value-first порядок аргументов везде (вкл. extern-границу), type-first имена (`fmt_f64`→`f64_fmt`), суффикс `_into` упразднён (`int_fmt_into`/`f64_fmt_shortest_into`/`f32_fmt_shortest_into` мосты retired — "простой рендер" = та же функция с default-аргами: `int_fmt(v,buf,cap,spec=FmtSpec.new())`, `f64_fmt(v,buf,cap,kind=FloatKind.Shortest,prec=-1)`, `f32_fmt(v,buf,cap)`; C-extern'ы → `nova_f64_fmt`/`nova_f32_fmt`, D282 литеральные-но-`nova_`-префиксные имена) | ✅ Ш0-Ш1/Ш3-Ш4 DONE, §10R-Д1-Д3 done (§5 code-примеры выше в ЭТОМ разделе переписаны на канон Ш4; норма семьи — [docs/plans/208-unified-formatter.md](../../docs/plans/208-unified-formatter.md) §10R-Д, source of truth); **Ш4 (снос `conv.h` `nova_fmt_*`/`nova_*_to_str`/`nova_*_to_debug_str` + kill-switch `NOVA_FMT_LEGACY` + str/char/bool rich-spec и int/float Debug rich переведены на `*_display_spec`) — ЗАКРЫТА**: для ВСЕХ шести примитивных видов (int/f64/f32/char/bool/str), И bare, И rich-spec, И Display, И Debug — единственный источник рендер-семантики теперь `std/src/runtime/{fmt_buf,string_builder}.nv` (`*_display_spec`-семейство); `conv.h` остаток = `nova_fmt_pad`+`nova_fmt_encode_fill`+`nova_fmt_char_count` (ТОЛЬКО композитный/user-type rich-spec pad — нет `*_display_spec`-аналога для произвольных типов) и `nova_ptr_to_debug_str` (pointer `${p:?}`, нет `.nv`-порта) — оба живые, не мёртвые. V1-упрощение #3 (см. подсекцию ниже) закрыто В ЧАСТИ «рендер хардкожен параллельно в Rust-эмиттере, `int_fmt` мёртв» (обе посылки теперь ложны). **Ш2 (2026-07-21, worktree `nova-sh2`, ветка `p208-sh2-bodies`, sonnet) — ЗАКРЫТА, закрывает V1-упрощение #3 ПОЛНОСТЬЮ:** блокер `[M-fmt-write-protocol-collision-cycle-adjacent]` снят (фикс влит в main, `compiler-codegen/src/types/mod.rs`); примитивные `@display`/`@debug`-тела (int/f64/f32/bool/char/str-`@debug`) в `prelude/protocols.nv` переписаны с циркулярной заглушки (`f.write("${@}".bytes())`) на прямые вызовы `*_display_spec`-семейства (`runtime.string_builder`) — физически тела ОСТАЛИСЬ в `protocols.nv` (не переехали в `fmt_buf.nv`, как исходно намечено картой — тот перенос потребовал бы `protocols.nv ↔ fmt_buf.nv` цикл; вместо этого — однонаправленный импорт `protocols.nv → string_builder.nv`, `string_builder.nv` не импортирует `prelude.protocols` обратно, поэтому нового цикла НЕТ), но теперь `f.kind()`/`f.width()` больше не остаются непрочитанными в теле «просто потому что» — тело зовёт РЕАЛЬНЫЙ рендер-движок (`*_display_spec`, тот же, что fast-path Ш3 девиртуализует), не самоссылающуюся интерполяцию. V1-упрощения #1 (композитный/user-type rich-spec не стримит в главный `sb`, рендерится во FRESH builder + внешний `nova_fmt_pad`) и #2 (precision для composite дропается) — ВНЕ scope Ш2/Ш4 (описывают ТОЛЬКО composite/user-type путь, который ни одна из этих волн не трогала — §10R предписывает закрыть ТОЛЬКО примитивную семью) — остаются как есть, НЕ регрессия. |

Ф.2 реализована на ветке `p208-impl` (3 шага: std-сигнатуры, `emit_c.rs`-диспатч,
миграция потребителей — json.nv + `spec_tests/conformance/d374_*`/`d229_*`/бывшие
`d419_*`→`d422_*`). Полный разбор по шагам, файлам, коммитам — см.
[docs/plans/wip/208-impl-progress.md](../../docs/plans/wip/208-impl-progress.md). Три
намеренных V1-упрощения относительно этого нормативного текста (НЕ противоречат
D422 — заполняют места, где D422 либо молчит, либо описывает алгоритм, для
которого V1 выбрал более простую/менее рискованную реализацию с идентичным
наблюдаемым поведением на всех текущих тестах):

1. **Композитный/user-type rich-spec путь НЕ стримит в главный interpolation
   `sb`.** §4 описывает "mark в главном sb + `@pad_in_place` после стрима" как
   целевой алгоритм для width-композитов. V1 (`emit_format_spec_value`) вместо
   этого сохраняет ДО-D422 архитектуру: рендер во FRESH `StringBuilder`,
   `nova_fmt_pad` пост-обработка снаружи (тот же C-хелпер, что примитивы) —
   тип получает настоящий `FmtCtx` (может читать любую ось), но
   `@pad`/`pad_consumed` НЕ читается обратно компилятором (внешний pad
   применяется БЕЗУСЛОВНО). Наблюдаемо идентично для любого типа, который сам
   не зовёт `@pad` (ни один существующий тип этого не делает) — но алгоритм
   §4 в буквальном смысле не реализован. Полная mark+`pad_in_place`-перевязка —
   явный follow-up. **Статус (Ф.4R Ш4, 2026-07-21): ВНЕ scope этой волны** —
   §10R предписывает закрыть ТОЛЬКО примитивную семью (int/f64/f32/char/bool/str);
   composite/user-type rich-spec путь Ш4 намеренно не трогала, стоит как было.
2. **Precision auto-truncate для composite/user-типов — ДРОПНУТ, не
   "пропущен через `prec_consumed`".** D422 §2 даёт `Fmt` только
   `@precision() -> Option[int]` (иммутабельный getter) — нет протокольного
   способа для типа сигнализировать «я учёл precision», в отличие от
   `pad_consumed`/`mut @pad`. V1 решение (owner-approved 2026-07-16): для
   composite-пути `precision_consumed` держится БЕЗУСЛОВНО `true` (внешняя
   обрезка никогда не применяется) — Rust-паритет (Debug/derive не обрезаются
   по precision), не молчаливое навязывание типу поведения, которое он не
   запрашивал. Это меняет один D419-эры assert (был:
   `${p:.3}` обрезает извне; стало: не обрезает) — задокументировано как
   легитимная миграция ретрактированной семантики, не ослабление теста
   (`spec_tests/conformance/d422_unified_display_dispatch.nv`). **Статус
   (Ф.4R Ш4, 2026-07-21): ВНЕ scope этой волны** — тот же composite-путь, что
   #1, не тронут.
3. **Примитивные `@display`/`@debug` (int/f64/f32/bool/char/str) не читают
   `f.kind()`/`f.width()`.** Тела — `f.write("${@}".bytes())` (интерп-стринг
   шорткат, byte-identical существующему `conv.h`-пути). Верно ТОЛЬКО потому,
   что компилятор НИКОГДА не диспетчит эти методы напрямую для примитива ни в
   голой, ни в rich-spec интерполяции (свой прямой `conv.h`-fast-path остаётся
   для ОБОИХ случаев, byte-parity подтверждён) — единственный реальный
   вызыватель этих тел — generic-диспетч (`Option[T Debug]`/`Result[...]`,
   будущий Ф.3 `Vec[T]`), где radix/width на элементах пока не тестируется.
   Полная перевязка на `int_fmt`/`bool_fmt`/`char_fmt` (Ф.1) — Ф.4 (и часть
   того, что делает Ф.4 сейчас "нечего удалять" — см. таблицу выше).
   **Волна 2 (2026-07-16) находка, углубляющая этот блокер:** буфер-примитивы
   Ф.1 (`fmt_buf.nv`) реализуют ТОЛЬКО display-форму `int`/`bool`/`char` — НЕТ
   quote/escape-логики для Debug `str`/`char` (`nova_char_to_debug_str`/
   `nova_str_to_debug_str` в `conv.h` дают `'c'`/`"a\nb"` с escaping; `fmt_buf.nv`
   ничего эквивалентного не содержит). Полная Ф.4-перевязка требует ЭТУ логику
   написать с нуля (не просто "подключить существующее") — см.
   `docs/plans/wip/208-impl-progress.md` §"Ф.4 — статус: РАЗВЕДКА" (волна 2) для
   полного разбора (там же — вторая находка: `int_fmt`/`bool_fmt`/`char_fmt`
   module-private по D422 §5 → единственный корректный способ их звать из
   hand-synth C — через method-dispatch на переписанных примитивных
   `@display(f)`/`@debug(f)` телах, не прямой C-вызов; это делает "перевязку"
   ОДНОЙ когерентной big-bang волной, не серией мелких безопасных шагов).
   **Статус (Ф.4R Ш4, 2026-07-21): ЧАСТИЧНО ЗАКРЫТО** (промежуточная запись,
   ниже — финал Ш2). Обе посылки этого пункта, КАК ДИАГНОСТИРОВАНО (компилятор
   хардкодит рендер параллельно `.nv`-движку; `int_fmt` мёртв), теперь ЛОЖНЫ:
   interp fast-path (bare И rich-spec, ОБА kind) девиртуализованно зовёт
   `*_display_spec` (`std/src/runtime/string_builder.nv`), который зовёт
   `int_fmt`/`f64_fmt`/`f32_fmt`/`bool_fmt`/`char_fmt`/`str_debug_fmt`/
   `char_debug_fmt` (`fmt_buf.nv`) — тот же движок, что использовал бы
   настоящий `@display`-body. `conv.h`'s `nova_fmt_*`/`nova_*_to_str`/
   `nova_*_to_debug_str` цепочка СНЕСЕНА (Ф.4R Ш4). Но САМИ примитивные
   `@display`/`@debug`-ТЕЛА (в `prelude/protocols.nv`) на тот момент оставались
   циркулярной заглушкой (`f.write("${@}".bytes())`) — компилятор их
   по-прежнему НЕ зовёт напрямую (обходит девиртуализацией), значит
   "примитивы не читают `f.kind()`/`f.width()` В СВОИХ ТЕЛАХ" оставалось
   буквально верным до Ш2, заблокированного `[M-fmt-write-protocol-collision-
   cycle-adjacent]`.

   **Статус (Ф.4R Ш2, 2026-07-21, worktree `nova-sh2`, ветка `p208-sh2-bodies`,
   sonnet): ПОЛНОСТЬЮ ЗАКРЫТО.** Блокер снят — `[M-fmt-write-protocol-
   collision-cycle-adjacent]` (Write-протокол name-коллизия по голому имени в
   `TypeCheckCtx::build`, `compiler-codegen/src/types/mod.rs`) исправлен и
   влит в main отдельной волной (per-file type overlay +
   `types_get_for_file`), равно как соседний `[M-imports-order-dependent-
   cycle]`. Примитивные `@display`/`@debug`-тела (int/f64/f32/bool/char/
   str's `@debug`; str's `@display` уже была некруговой identity-копией,
   не тронута) в `prelude/protocols.nv` переписаны на прямые вызовы
   `*_display_spec`-семейства:
   ```nova
   #impl(Display)
   fn int @display(mut f Fmt) -> () {
       consume sb = StringBuilder.new()
       int_display_spec(sb, @, 0, 10, false, false, false, false, Align.Left, ' ')
       f.write(sb.into_str().bytes())
   }
   ```
   (аналогично для f64/f32/bool/char displaying/debug-вариантов, зовущих
   `f64_display_spec`/`f32_display_spec`/`bool_display_spec`/
   `char_display_spec`/`char_debug_display_spec`/`str_debug_display_spec`
   соответственно — bare-параметры `width:0, align:Left, fill:' '` — no-op
   паддинг-оси, byte-identical старому bare-поведению).

   Тела физически ОСТАЛИСЬ в `protocols.nv` (НЕ переехали в `fmt_buf.nv`, как
   исходно намечено §10R-картой — тот перенос потребовал бы завести цикл
   `protocols.nv ↔ fmt_buf.nv` заново поверх уже разрешённого; вместо этого —
   ОДНОНАПРАВЛЕННЫЙ импорт `std.runtime.string_builder.{...}` в
   `protocols.nv`, где `*_display_spec`-семейство физически живёт с Ш1
   architecture-v2 — `string_builder.nv` `#no_prelude`, НЕ импортирует
   `prelude.protocols` обратно, значит нового цикла нет; тот же
   однонаправленный shape, что уже существующий `fmt_buf`-импорт строкой
   выше в том же файле). Каждое тело рендерит в свежий
   `consume sb = StringBuilder.new()` (т.к. `*_display_spec` требует
   КОНКРЕТНЫЙ `StringBuilder`, а `@display(mut f Fmt)` получает АБСТРАКТНЫЙ
   `Fmt`), затем ОДИН `f.write(sb.into_str().bytes())` копирует итог в
   реальный sink — не zero-alloc (в отличие от компиляторного fast-path,
   который пишет прямо в целевой `StringBuilder` без промежуточного), но
   единственный источник рендер-семантики: полиморфный путь
   (`v.display(f)`/`v.debug(f)` через абстрактный `Fmt` — `Option[T Debug]`/
   `Result[T Debug, E Debug]` и future generic `Vec[T]`/`[]T` Display) теперь
   зовёт ТОТ ЖЕ `*_display_spec`-движок, что компиляторный fast-path
   девиртуализует, а не отдельную циркулярную дорожку. V1-упрощение #3
   (эта секция целиком) ЗАКРЫТО ПОЛНОСТЬЮ — обе половины («рендер хардкожен
   параллельно» И «примитивные тела — циркулярная заглушка») теперь ложны.
   zero-alloc для ЭТОГО (generic-диспетч) пути — задокументированный
   follow-up вне scope Ш2 (примитив тут рендерится редко, только через
   абстрактный `Fmt`; горячий top-level путь остаётся zero-copy через Ш3
   fast-path, не задет).

   Верификация: `nova check` (checker-only — обходит НЕСВЯЗАННЫЙ
   pre-existing `[M-d216-write-at-return-type-unknown-cc-panic]`, который
   ловит ЛЮБОЙ full-pipeline прогон папки `spec_tests/conformance`) на трёх
   эталонах Ш0 (`d422_f4r_baseline_{int,float,strcharboolu64}.nv`) — PASS
   3/0 (216 WARN, все предсуществующие unused-import/W_FFI_CANCEL_UNSAFE);
   изолированный standalone-репро (`nova build`+run вне conformance-папки,
   обходит write_at ICE полностью) — bare Display/Debug на ВСЕХ шести
   примитивах PASS, ПЛЮС полиморфный путь через `Option[T Debug]`/
   `Option[T Display]`/`Result[T Debug, E Debug]` (единственный реальный
   вызыватель перенесённых тел до будущего generic-Display на `Vec`/`[]T`) —
   PASS; `fmt_buf/core_test` 1/0, `string_builder_test` 1/0, `checksums`
   3/0 (fnv/adler32/crc32); `d374_write_sink_decouple` (checker) PASS 1/0;
   флагман `examples/flagship/aggregator/src/main.nv --strict-effects`
   built чисто (70.34s, только предсущ. warnings). Полный мега-CU НЕ
   гонялся — заблокирован тем же НЕСВЯЗАННЫМ write_at ICE, за интегратором
   (`docs/plans/backlog-followups.md`).

**Координация с Plan 152.7.2** (`docs/plans/152.7.2-format-context.md`, отдельный
план, СВОЁ решение по статусу — не закрыт этим коммитом): его "interp-direct-
to-sink" наработка (`[M-152.7.2]`/`[M-d419-interp-direct-primitives]`) частично
реализована этой волной — bare-диспетч (`emit_interpolated_str`) действительно
пишет user-type `@display`/`@debug` ПРЯМО в главный interpolation sink (без
промежуточной строки); rich-spec композитный путь — ещё нет (V1-упрощение #1
выше). Маркер НЕ закрывается этим коммитом (only marking per coordination
note, 2026-07-16) — итоговое решение по статусу 152.7.2 (obsolete/superseded
vs remaining-tail) — за владельцем/интегратором.

### Почему

- **Один метод, не два** — Rust-прецедент `Display::fmt(&self, f: &mut Formatter)`; D419's
  `@display`+`@display_fmt` пара выглядела «некрасиво» (претензия владельца) и держала
  to_str-дефолт (ловушка цикла).
- **Байтовый `Write`** — фундаментальнее строкового; унифицирует с `io.Write` (Plan 176) без
  слияния самих типов направлений.
- **`use Write` protocol-embed, не переименованный метод** — сохраняет «`Fmt` поверх `Write`»
  буквально (тот же `@write`), убирая D419's дублирующее «структурное расширение без embed».
- **required-примитив + bounded-`to_str`** — устраняет структурную возможность цикла раз и
  навсегда (не полагается на runtime cycle-guard, как в D419-эпохе `str.from_debug`/default-body
  synthesis).
- **Derived Display ≠ Debug формой** — иначе `${x}` и `${x:?}` дают тождественный вывод, теряя
  смысл различия («значение» vs «для разработчика»).

### Связь

- [D419](#d419-Fmt-protocol--format-spec-context-для-display_fmt-plan-15272-2026-07-13) —
  RETRACT/SUPERSEDED целиком (двух-методная схема + узкая `Fmt` с двумя осями).
- [D374](#d374-write-sink-протокол--декаплинг-displaydebug-от-stringbuilder-plan-15271) —
  амендится ×2 (sink `Write`→`Fmt`, `@write_str(str)`→`@write([]u8)`).
- [D237](#d237-protocol-naming-convention-method-name-capitalized-plan-137-2026-06-09) —
  амендится (сигнатура `@display`/`@debug` → `(mut f Fmt)`, имена протоколов без изменений).
- [D229](#d229--Debug-protocol--format-spec-expr) — амендится (диспетч `${expr:?}` через
  `@debug(f)`, радикс через `f.kind()`).
- [D179](08-runtime.md#d179-stringbuilder--pure-nova-consume-type--plan-91-ф26) — амендится
  (байтовый append + `@reserve`/`@spare`/`@advance`/`@len`/`@pad_in_place`/`@write_padded`; 2026-07-18/20: `@reserve -> @` (fluent-канон D131; УКАЗАТЕЛЬ больше не возвращает), сырой хвост — отдельная дверь `@spare() -> *mut u8`; `@advance -> @`).
- [D55](#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы) —
  амендится (str-литерал→`[]u8` коэрсия, общее правило для любой `[]u8`-позиции).
- [D145](#d145-fnt-префикс--receiver-generic-decl--bounds-plan-101) — источник protocol-embed
  механизма (`use Write` внутри `Fmt`), без изменений.
- [D406](#d406-sum-type-синтаксис-enum-маркер-2026-07-01) — `enum`-маркер для
  `Align`/`Sign`/`FmtKind`/`FloatKind`.
- [D282](08-runtime.md#d282-new--extern-nova-fn--extern-c-fn--двух-abi-синтаксис-для-ffi-plan-9112-ф-1) —
  `extern "C" fn nova_f64_fmt`/`nova_f32_fmt` литеральные имена (§10R-Д3 канон; `fmt_f64_into` —
  доредизайновое имя, переименовано владельцем).
- [D176](#d176-ro-t--тип-модификатор) — `.bytes()` на str-переменной = `ro []u8` zero-copy view.
- str.from_debug/str.from ретракция (Plan 174.2, [D73](08-runtime.md#d73-from--into-protocol-пара-с-авто-выводом))
  — `str.from_debug(@)` в Debug-протокола default-body (`std/prelude/protocols.nv`) остаётся
  мёртвым/нереализованным символом ДО D422 (174.2 явно оставил его вне scope — см.
  `docs/plans/wip/174.2-scalar-to-str-notes.md`); D422 Ф.2 удаляет этот default-body ПОЛНОСТЬЮ
  вместе с переходом на compiler-synthesized `@debug(f)` — `str.from_debug` окончательно
  устраняется, не остаётся даже мёртвым текстом.
- Plan 208 (докладной план — [docs/plans/208-unified-formatter.md](../../docs/plans/208-unified-formatter.md), this D-block's home plan).
- Plan 176 (`io.Write` — координация байтового шейпа, направления раздельны).
- Plan 196 — НЕ пересекается (замороженная зона `infer_call_ret_c` вне scope форматирования).
## D429. `#coerce` — декларативные неявные zero-cost конверсии (view + finalize) (Plan 214, 2026-07-18)

**Контекст.** Nova прячет только безопасные, очевидные, zero-cost конверсии (доктрина D55:
single-wrapper вкл. переменные, int→i64 widening «невидимо для автора»). Пары вида
`str → []u8` (вид) и `StringBuilder → str` (финализация) в эту доктрину входят по стоимости,
но не имели механизма: реализованный компромисс (str-литерал в методе, названном `write`) был
name-keyed хардкодом — нарушение
[compiler-conventions §3](../../docs/dev/compiler-conventions.md). D429 вводит ОБЩИЙ
декларативный механизм: пара объявляется атрибутом в `.nv`-исходнике; в Rust-компиляторе нет
ни одной захардкоженной пары.

### Форма

**Один атрибут `#coerce`** на функции. Функция обязана быть **унарной по входному типу** и
объявляет неявную коэрсию `I → O`:

```nova
// V1: форма-ИСТОЧНИК — метод без параметров; I = тип-приёмник, O = возврат
#coerce fn str @bytes() -> ro []u8 => ...                  // str → ro []u8      (view)
#coerce fn StringBuilder consume @into_str() -> str { .. } // StringBuilder → str (finalize)
```

**Форма-ПРИЁМНИК (статик `.new` с одним параметром: `#coerce fn Matrix.new(v []f64) ->
ro Self`) — ОТЛОЖЕНА из V1** (решение владельца 2026-07-18, ревью-2 п.19): у неё ноль
носителей среди первых деклараций, а R2-проверка формы (`ro Self`) НЕ гарантирует
реальную zero-cost (конструктор может клонировать внутри — форм-инвариант без
стоимость-инварианта). Возврат — отдельным амендментом при первом реальном носителе
ВМЕСТЕ с правилом проверяемости (кандидат: ограничение newtype/однополевой обёрткой,
которую компилятор верифицирует). В V1 приёмник-форма — явная ошибка (см. R1), не тихое
игнорирование.

### Правила (нормативные)

- **R1 Унарность; V1 — только форма-источник.** Метод-без-параметров на конкретном типе;
  иначе `E_COERCE_NOT_UNARY`. Статик-с-одним-параметром (форма-приёмник) в V1 —
  `E_COERCE_RECEIVER_FORM_DEFERRED` с подсказкой «объявите пару методом на типе-источнике»
  (см. §Форма — отложена решением владельца 2026-07-18).
- **R2 Zero-cost, две полосы.** Неявная вставка разрешена только для форм без скрытой
  аллокации:
  - **view** — не-`consume` форма с `ro`-возвратом (zero-cost вид);
  - **finalize** — `consume`-метод с владеющим возвратом (zero-cost MOVE: передача владения,
    не копия). Ресивер разряжается в точке вставки; линейность (D133) отслеживается как при
    явном вызове — use-after = немедленный compile error (см. R7). Решение владельца
    2026-07-17: позиции finalize НЕ сужаются (return-only-срез отвергнут — рантайм-опасности
    нет ни в одной ветке, нецелевое использование громко ловится компиляцией).

  Форма вне полос (не-`consume` с владеющим возвратом = скрытая аллокация) —
  `E_COERCE_NOT_ZERO_COST`. Атрибут «без эффекта» не существует — либо работает, либо ошибка.
- **R3 Одна декларация на пару.** Не более одного `#coerce` на пару `(I, O)` во всей
  программе (обе формы записи считаются; дубль с двух сторон — тоже дубль):
  `E_COERCE_DUPLICATE_PAIR` с указанием обеих деклараций.

  > **R3' (Plan 214.1, «один образец на пару-форму»):** для GENERIC-образцов у пары нет
  > decl-time идентичности — конкретная пара кристаллизуется только когда `?T` связан на
  > конкретном сайте вызова, поэтому R3-дедуп по `seen_pairs` (decl-time) на образцы не
  > переносится буквально. Вместо этого: если ≥2 образца (при промахе конкретных пар)
  > унифицируются с ОДНОЙ и той же конкретной парой в данной позиции —
  > `E_COERCE_DUPLICATE_PAIR` В ТОЧКЕ ПРИМЕНЕНИЯ, перечисляя обе декларации. Смотри
  > «Generic-образцы» ниже за деталями матчера.
- **R4 Один уровень; механизмы НЕ компонуются (расширено ревью-5 2026-07-18).** Цепочки
  не разворачиваются: `I→O` и `O→P` объявлены — `I` в `P`-позиции НЕ коэрсится (паритет
  single-wrapper «ровно один уровень»). Это включает КРОСС-механизменную композицию: позиция
  ждёт `W` (newtype над `[]u8`), значение `str` — путь `str →(#coerce)→ []u8
  →(single-wrapper)→ W` ЗАПРЕЩЁН. Один шаг неявности ВСЕГО на позицию, каким бы механизмом
  он ни делался — иначе «один уровень» обходится смешиванием, а поиск становится
  транзитивным (непредсказуемость + перф).
- **R5 Exact > coercion; однозначность.** Перегрузка/позиция с точным типом всегда побеждает;
  коэрсия пробуется только когда позиция ждёт `O` и точного совпадения нет. Если к позиции
  применимы ≥2 пары — `E_COERCE_AMBIGUOUS` (перечислить кандидатов), никакого tie-break.

  > **R5' (Plan 214.1, генерализация на образцы):** сначала конкретные пары (`coerce_pairs`),
  > потом образцы (`generic_coerce_patterns`) — двухступенчатый lookup, генерик-матчер даже
  > не запускается при попадании в конкретный реестр (горячий str→[]u8-путь не замедляется).
  > ≥2 ПОДХОДЯЩИХ ОБРАЗЦА к ОДНОЙ и той же позиции с РАЗНЫМИ (I,O) (например `Json[T] -> T` и
  > `Json[T] -> str` при overload-перегрузке `dump(v User)` / `dump(v str)`) —
  > `E_COERCE_AMBIGUOUS`. **Честная пометка:** этот overload-уровневый сценарий разделяет
  > нереализованность с R5 для КОНКРЕТНЫХ пар — `E_COERCE_AMBIGUOUS` не встречается нигде в
  > текущем коде компилятора (ни для конкретных пар, ни для образцов); механизм, реально
  > реализованный и протестированный Plan 214.1, — это R3' (≥2 образца унифицируются к ОДНОЙ
  > и той же (I,O)-паре В ОДНОЙ позиции, `E_COERCE_DUPLICATE_PAIR`, см. R3'). Снятие этого
  > пробела для overload-уровневой R5/R5'-неоднозначности (обеих полос, конкретной и
  > generic) — отдельный, не начатый пункт; см. `[M-coerce-r5-ambiguous-overload-unimplemented]`.
- **R6 Позиции.** Таблица «явно ожидаемого типа» D55 (case-таблица выше, БЕЗ исключений):
  call-arg на резолвленный параметр, `let`/`const`/`ro`/`mut` с явной аннотацией типа,
  return-позиция (вкл. last-expr), element-позиция коллекции. `ro O` не матчит
  `mut`/`consume`-позицию (там — явный жест: `.bytes().clone()` и т.п.).

  > **Уточнение (ревью 2026-07-18, Plan 214 pre-Ф.1 ревью):** «без исключений» означает —
  > набор позиций #coerce НЕ ýже перечисленного списка выше (сверх явно оговорённой
  > mut/consume-оговорки), НЕ обещание достать позиции, которых сама D55-таблица ещё не
  > достигла. Перечисленный список (call-arg / `let`-`const`-`ro`-`mut` / return / element)
  > — это в точности ✅-строки D55-таблицы. Строки D55, помеченные ⛔ (generic-параметр
  > record/sum, match-arm, record-элемент коллекции), и позиция «значение record-литерала»
  > (не имеющая отдельной строки в D55-таблице вовсе, см. case 4) — **вне охвата Ф.1**:
  > #coerce переписывается ТОЙ ЖЕ машинерией expected-type-propagation
  > (`try_wrap_leaf`-семья), что и сам D55, и физически не может достать позицию, которую
  > не достаёт D55. Как только D55 доводит ⛔-строку до ✅ (или заводит строку для
  > record-литерал-поля), #coerce наследует её автоматически тем же проходом — отдельного
  > D429-амендмента не требуется.
- **R7 Вставка = именованный вызов + диагностика-контракт.** Компилятор АСТ-переписывает
  значение в вызов объявленной функции (`s` → `s.bytes()`); неявная форма байт-в-байт
  эквивалентна явной, нового лоуэринга нет. Для finalize-полосы диагностика use-after-consume
  ОБЯЗАНА указывать точку неявной вставки: «потреблён неявной #coerce-финализацией
  `into_str()` в вызове … (строка N); для чтения без потребления — явный view-метод».
- **R8 Охват — все типы, включая пользовательские, с первого дня** (решение владельца
  2026-07-18: без std-only-гейта). Orphan-развязка — extension-форма: автор `MyId` объявляет
  `#coerce fn str @to_myid() -> ro MyId` extension'ом в своём модуле. R3 глобально
  предотвращает конфликт деклараций. **Видимость (ревью 2026-07-18):** пара активна ТОЛЬКО
  там, где объявляющая функция ВИДИМА обычными правилами (импорт/prelude/extension-видимость)
  — вставленный вызов резолвится по построению; невидимая пара = коэрсии нет (обычный
  type mismatch), никакого action-at-a-distance и нерезолвящихся синтез-вызовов.
- **R9 Канон call-сайта (конвенция + линт).** В коэрсибельной позиции канон — ГОЛОЕ значение
  (`w.write(s)`, `return sb`), НЕ явный вызов. Линт `W_COERCE_EXPLICIT_REDUNDANT`: явный
  вызов `#coerce`-функции там, где голое значение скоэрсилось бы той же парой → «уберите
  явный вызов — действует #coerce (D429)». Линт читает ТОТ ЖЕ реестр пар, что и вставка
  (одно окно — расхождение линта и коэрсии невозможно по построению). Не флагаются: позиции
  без явного ожидаемого типа (`ro b = s.bytes()`), mut/clone-сайты, пары вне реестра.
  nv-coding-style амендится этим правилом в слиянии реализации (Plan 214 Ф.5).

  > **Уточнение (ревью 2026-07-18, Plan 214 pre-Ф.1 ревью):** «тот же реестр пар, что и
  > вставка» — про ОДНО ОКНО данных (не два независимых списка, которые могли бы
  > разойтись), не про то, что голого членства пары в реестре ДОСТАТОЧНО для флага.
  > Формальный критерий — предикат ВСТАВКИ ЦЕЛИКОМ (та же функция, что решает, вставлять
  > ли коэрсию на этом сайте), включая exact/generic-wins (R5/R16): при catch-all
  > `f[T](T)` пара `(I,O)` в реестре ЕСТЬ, но R16 её не вставляет здесь (успешно
  > инстанцируемый generic-кандидат = точное совпадение) — явный `.bytes()` на таком сайте
  > НЕ избыточен и флагаться не должен. Формула: варнинг ⇔ коэрсия РЕАЛЬНО вставилась бы на
  > этом сайте при голой форме значения.
- **R10 `as` НЕ вызывает `#coerce`** (вопрос владельца 2026-07-18, закреплено; rationale
  уточнён 2026-07-18 — поправка владельца: «кода не исполняет» было неверно). `as` —
  ЗАКРЫТОЕ спекой множество конверсий (численное сужение/расширение D54, newtype-relabel
  D52, ptr-каст в unsafe); часть пар исполняет вшитый спека-заданный код (насыщение
  `int as uint` neg→0 D54, см. §выше `nova_int_to_uint`) — но семантика КАЖДОЙ пары
  задана таблицей D-блоков и известна читателю из спеки, не из пользовательских деклараций
  (тот же контракт у Rust `as`: насыщающий float→int с 1.45 — и всё равно не зовёт
  From/Into). `#coerce` — ОТКРЫТЫЙ реестр произвольных пользовательских функций — другой
  класс; смешение сделало бы семантику `as` зависимой от атрибутов в чужих модулях
  (C++-ловушка conversion-operator у static_cast) и дало бы ТРЕТЬЮ дверь к паре
  (голое значение / именованный метод / as) — D9. Явная форма
  пары ровно одна — именованный вызов (`s.bytes()`, `sb.into_str()`); finalize через `as`
  немыслим (каст не потребляет операнд). Диагностика: `s as O` при объявленной паре `(I,O)`
  и невалидном repr-касте → обычная cast-ошибка + подсказка «пара #coerce объявлена: голое
  значение в типизированной позиции или явный `s.bytes()`». Форма `"lit" as []u8` из
  ретрактированной D55-подсекции — мертворожденная (0 использований в дереве), Ф.2 её не
  поддерживает; подсказка выше покрывает миграцию.
- **R11 Вшитая single-wrapper-неявность первична** (решение владельца 2026-07-18).
  Неявных механизмов ТРИ на одной таблице позиций: вшитый newtype-wrap (`expr as W`) и
  вшитый sum-lift (`W.C(expr)`, ровно один unary-вариант «рода» — §Obvious single-wrapper
  выше) + декларативный `#coerce`. Правило первенства: если пара (I, O) УЖЕ покрыта
  single-wrapper'ом (O — newtype над родом I, ЛИБО sum с ровно одним unary-вариантом
  I-рода) — `#coerce`-декларация на эту пару = ошибка `E_COERCE_DUPLICATE_PAIR` с
  указанием покрывающей newtype/sum-декларации (симметрично дублю двух `#coerce`, R3).
  Rationale: два механизма на одной паре = недетерминированная для читателя вставка и
  рассадник диффузных багов; вшитая форма дешевле (no-op repr / прямой конструктор) и
  первична по старшинству. Ф.4 Plan 214: две neg-фикстуры (newtype-цель, sum-цель).

- **R12 Эффект-свобода (ревью 2026-07-18).** `#coerce`-функция обязана иметь ПУСТОЙ
  effect-row — неявная вставка эффектного вызова была бы скрытым эффектом в голой позиции,
  что ломает контракт strict-effects («эффект виден в сигнатуре и на call-сайте»).
  Нарушение — `E_COERCE_EFFECTFUL`. Паника эффектом не является и не запрещается
  (doc-рекомендация: тело желательно total).
- **R13 Само-применение исключено (ревью 2026-07-18).** Внутри тела `#coerce`-функции ЕЁ
  СОБСТВЕННАЯ пара не применяется: `#coerce fn str @bytes() -> ro []u8 => @` иначе
  переписался бы в `@.bytes()` — бесконечная саморекурсия. С R13 голое `@` в такой позиции —
  честный type mismatch. Чужие пары в теле применяются как обычно.

  > **R13' (Plan 214.1, генерализация на образцы):** тот же запрет действует внутри тела
  > GENERIC `#coerce`-образца — `Json[T] @data() -> T => @` НЕ переписался бы в `@.data()`
  > (иначе бесконечная саморекурсия на ЛЮБОМ `T`). Реализовано публикацией span'а текущей
  > `#coerce`-декларации на время проверки её тела (`current_coerce_decl_span` в чекере,
  > `current_fn_span` в rewrite-проходе) и исключением образца с тем же `decl_span` из
  > кандидатов `generic_coerce_lookup`. Чужие образцы и чужие конкретные пары в теле
  > применяются как обычно — исключение строго self-only.

- **R14 (RETRACTED → см. «Generic-образцы» ниже, Plan 214.1, 2026-07-24).** Текст ниже —
  история (было нормативно до 214.1): ~~Generic-формы запрещены (V1, ревью-5 2026-07-18).
  `#coerce` на функции с method-level type-параметрами (`fn[T] …`) или на generic-инстансной
  паре — `E_COERCE_GENERIC_UNSUPPORTED` (явная ошибка, не тихое игнорирование): реестр пар —
  конкретный lookup, не унификация; параметрические пары взорвали бы R5-однозначность. Все
  текущие декларации (str/StringBuilder/WriteBuffer) конкретны. Снятие — отдельным
  амендментом при реальном кейсе.~~ Владелец 2026-07-23: «это было упущение, надо исправить
  сейчас» — `Json[T] @data() -> T` есть ИДЕАЛЬНЫЙ кандидат view-полосы (zero-cost: чтение
  поля; один уровень; exact-wins сохраняется), запрет упирался только в форму РЕЕСТРА
  (словарь-lookup по конкретному имени), не в семантику. `E_COERCE_GENERIC_UNSUPPORTED`
  ушёл из набора кодов; бланкет-реджект заменён узким матчером образцов — см. «Generic-
  образцы (Plan 214.1)» ниже. Родственный, НЕ путать: R16 (generic-ОВЕРЛОАДЫ на сайте
  вызова) остаётся в силе без изменений.
- **R15 На protocol-требовании запрещён (ревью-5 2026-07-18).** `#coerce` внутри
  `type P protocol { … }` — `E_COERCE_ON_PROTOCOL`: атрибут на требовании раздавал бы пару
  КАЖДОМУ имплементору (массовая генерация пар + гарантированные R5-конфликты). Легален
  только на конкретной fn-декларации: метод или extension-метод (V1; статик `.new` —
  вместе с приёмник-формой, см. R1).
- **R16 Generic-унификация = точное совпадение (решение владельца 2026-07-18, ревью-2
  п.18; НЕ путать с R14 — тот про generic-ДЕКЛАРАЦИИ #coerce, этот про generic-ОВЕРЛОАДЫ
  на сайте вызова).** При overload-резолве успешно инстанцируемый generic-кандидат
  (`f[T](T)` для `f(s)`) считается ТОЧНЫМ совпадением в смысле R5 — коэрсия НЕ пробуется.
  Коэрсия рассматривается только при НУЛЕ применимых кандидатов (конкретных и generic).
  Следствие (норма, не баг): в generic-насыщенных API (`f[T](T)` catch-all) пара #coerce
  в этой позиции не сработает никогда — предсказуемость диспатча дороже эргономики
  (урок семьи 196.7/196.8: неоднозначность «унификация vs спец-путь» = источник тихих
  мис-диспатчей). Фикстура Plan 214 Ф.4 пинует: при наличии `f[T](T)` и объявленной пары
  str→[]u8 вызов `f(s)` берёт generic-инстанс, НЕ коэрсию.

### Generic-образцы (Plan 214.1, снятие R14, 2026-07-24)

**Что снято.** R14 (было: любой `#coerce` на receiver'е с собственными generic'ами или на
функции с method-level type-параметрами — бланкет-`E_COERCE_GENERIC_UNSUPPORTED`) заменена
узким, но реально исполняемым матчером образцов:

```nova
#coerce
export fn Json[T] @data() -> T => @data
```
Значение `Json[User]` в позиции, ожидающей `User`, разворачивается автоматически (в
`j.data()`), симметрично тому, как `str` разворачивается в `[]u8` через `s.bytes()`.

**Дизайн — ДВА реестра, не один.** Конкретные пары (`coerce_pairs`, R1-R13/R15) остаются
байт-в-байт как есть — ноль регрессий, ноль замедления горячего str→[]u8-пути. Generic-формы
идут во ВТОРОЙ, отдельный реестр образцов (`generic_coerce_patterns`, ключ — БАЗОВОЕ имя типа
receiver'а, напр. `Json`), проверяемый ТОЛЬКО при промахе конкретного lookup'а:

```
lookup(I, O):
  1. coerce_pairs[(I,O)]           → hit? вернуть (текущий путь, байт-в-байт)
  2. generic_coerce_patterns       → унификация образца с (I,O); ноль/один/много (R3'/R5')
```

**Какие формы поддержаны.** Пара-переменные — ИСКЛЮЧИТЕЛЬНО собственные carrier-generic'и
receiver'а (`Json[T]`, `Pair[K, V]`) в ГОЛОМ виде (bare single-segment reference на позиции
generic-аргумента) — ЛЮБОЙ арности. Унификация ОДНОСТОРОННЯЯ и БЕЗ РЕКУРСИИ ВГЛУБЬ: пара
`Json[?T] → ?T` сопоставляется с конкретным `(Json[User], User)`, связывая `?T := User` —
слот receiver'а связывается ЦЕЛИКОМ (`Json[Vec[T]]` НЕ разбирается на `Vec`/`T` отдельно; это
уже нарушило бы форму «bare reference»). Подстановка связанных `?T` в `ret_shape` (не сам
матчинг, а применение уже НАЙДЕННЫХ связок) — рекурсивная, без ограничения глубины: это
чисто механическая замена, а не поиск, так что образец `Json[T] -> Vec[T]` корректно даёт
`Vec[User]`.

**Явно отклоняется (ошибка `E_COERCE_GENERIC_PATTERN_UNSUPPORTED`, не тихий пропуск —
та же поза «атрибут без эффекта не существует», что и у R2/R14):**
- метод-уровневый type-параметр (`fn[U] Type @method()`) — ничего в receiver'е не определяет
  `U` на сайте НЕЯВНОЙ вставки (нет turbofish на неявной вставке);
- generic-аргумент receiver'а, который НЕ является голой carrier-переменной (вложенная форма
  `Json[[]T]`, или конкретный аргумент `Json[int]`) — шаблон-матчер связывает только
  голые верхнеуровневые слоты.

**R4 (без изменений, критично для образцов).** Один уровень — `Json[Json[T]]` в позиции,
ожидающей `User`, НЕ разворачивается дважды: единственный проверяемый образец `Json[?T] →
?T` при унификации с `(Json[Json[User]], User)` связывает `?T := Json[User]`, ключ результата
— `Json[User]`, НЕ равен ожидаемому `User` → честный type mismatch (нет цепного повторного
поиска — `lookup` вызывается РОВНО один раз на позицию, как и для конкретных пар).

**R13' само-применение (детали в амендменте R13 выше).** Реализовано публикацией span'а
текущей `#coerce`-декларации на время проверки/переписывания ЕЁ ЖЕ тела
(`current_coerce_decl_span` в чекере — RAII-guard `CoerceSelfGuard`; `current_fn_span` в
mutable AST-rewrite проходе, независимая копия по той же R9-логике «одно окно данных») и
исключением образца с СОВПАДАЮЩИМ `decl_span` из кандидатов `generic_coerce_lookup` /
rewrite-поиска. Чужие образцы и конкретные пары в теле работают как обычно — исключение
строго self-only, не «не коэрситься вообще внутри `#coerce`-тел».

**R3'/R5' — см. амендменты у R3/R5 выше.** Реализовано: R3' (`E_COERCE_DUPLICATE_PAIR` в
точке применения при ≥2 образцах, унифицирующихся к одной и той же (I,O)-паре в ОДНОЙ
позиции — единственный сценарий, различающий образцы от конкретных пар: там decl-time
`seen_pairs` физически не может это поймать, потому что конкретная пара кристаллизуется
только после связывания `?T`). **НЕ реализовано** (честная пометка, разделяет пробел с
R5 конкретных пар): overload-уровневая R5'/`E_COERCE_AMBIGUOUS` (два образца с РАЗНЫМИ O,
оба применимые к разным перегрузкам одного call-arg) — маркер
`[M-coerce-r5-ambiguous-overload-unimplemented]`.

**Реализация:** `compiler-codegen/src/types/mod.rs` — `GenericCoercePattern` (структура
образца), `named_base_and_args`/`unify_coerce_receiver`/`substitute_coerce_shape` (шаблон-
матчер), `TypeCheckCtx::generic_coerce_lookup` (accept-путь, вызывается ПОСЛЕ конкретного
`coerce_pairs`-фоллбэка в `assignable`), `MapLitAnnotator::try_coerce_leaf` (AST-rewrite,
generic-ветка после конкретной). Один и тот же `collect_coerce_pairs` строит ОБА реестра за
один проход (R9 «одно окно» — расширено на образцы теми же гарантиями).

### Ретракт

Подсекция **D55 «Str-литерал → `[]u8` coercion» (2026-07-15) — RETRACTED → D429**: литерал —
частный случай str-значения, отдельного литерал-правила не остаётся; общее правило
распространяется на литералы автоматически. Name-gated реализация
(`synthesize_write_str_lit_bytes_coercion`, метод буквально `write`) сносится в Plan 214 Ф.2;
поведение сайтов `w.write("...")` сохраняется новым механизмом байт-в-байт.

**R14 «Generic-формы запрещены» (ревью-5 2026-07-18) — RETRACTED → см. «Generic-образцы»
выше (Plan 214.1, 2026-07-24):** бланкет-реджект был упущением (владелец), не осознанным
ограничением — заменён узким матчером образцов; `E_COERCE_GENERIC_UNSUPPORTED` ушёл из
набора кодов, его место занял `E_COERCE_GENERIC_PATTERN_UNSUPPORTED` (другая семантика —
не «generic вообще запрещён», а «эта КОНКРЕТНАЯ generic-форма не представима шаблон-
матчером»).

### Первые декларации (std, Plan 214 Ф.3)

`#coerce fn str @bytes() -> ro []u8` · `#coerce fn StringBuilder consume @into_str() -> str` ·
`#coerce fn WriteBuffer consume @into_bytes() -> []u8`.

### Отклонённые альтернативы (ревью 2026-07-17)

- Универсальная вшитая str→[]u8 (хардкод пары в Rust) — §3-нарушение.
- Безымянная call-форма `fn Vec[T](s str)` — новый синтаксис + реинкарнация ретрактированного
  `from` (§1а) + всё равно требует opt-in атрибута.
- Два атрибута `#view`/`#coerce` — унарность делает пару однозначной, атрибут один.
- return-only-срез finalize-полосы — перестраховка (линейность ловит всё компиляцией).

### Амендмент (№520, 2026-08-09, владелец): неявная finalize-коэрсия — сама потребление

**Контекст.** R2 уже нормативно требовал этого («ресивер разряжается в точке вставки;
линейность (D133) отслеживается как при явном вызове»), но реализация не соответствовала —
`spec_tests/conformance/lint/conv_clean.nv:46` (`sb.into_str()`, `sb consume =
StringBuilder.new()`) с голым `sb` вместо явного вызова давало `E_VIEW_BINDING_FORBIDDEN`/
`D133-not-consumed`/`E_CONSUME_KEYWORD_MISSING` в зависимости от позиции — линт
`W_COERCE_EXPLICIT_REDUNDANT` (R9) советовал код, который не собирался. Найдено интегратором
2026-08-09 (реестр 221.1 №519/№520). Разбор владельца отверг оба предложенных интегратором
варианта (не срабатывать на `#coerce`-функциях с `consume`-ресивером / завести
форму-выражение для потребления) в пользу третьего: **дефект в чекере, не в линте и не в
синтаксисе.**

**Решение владельца, дословно:** «автовызов через `#coerce` должен учитываться при расчёте
потребления автоматически во всех ситуациях, автодисарм, например».

**Нормативно (уточняет R2/R6, не меняет их):** неявная вставка finalize-вызова (consume-
ресивер) в ЛЮБОЙ позиции из R6-таблицы («без исключений») ЕСТЬ потребляющее использование
ресивера — гасит consume-обязательство/drop-флаг (D432 §4/§5 — тип с `@cleanup` не получает
повторный авто-вызов на выходе из scope) и запрещает повторное использование значения (D131),
байт-в-байт как явный вызов (R7 уже требовал этого от диагностики; теперь требует и от
самого расчёта потребления, а не только от текста сообщения).

**Реализация (`compiler-codegen/src/types/mod.rs`, `ConsumeCtx`):** до амендмента расчёт
потребления (отдельный проход по ДО-переписанному AST, до `MapLitAnnotator::try_coerce_leaf`)
корректно кредитовал только return-позицию (существовавшее ДО D429 типонезависимое правило
«голый Ident в хвосте блока — вынос владения») и call-arg свободной функции
(`fn_param_output_keys`/`coerce_finalize_output_keys`, тот же Plan 214). Остальные позиции
R6-таблицы падали в старые D180/D133-диагностики. Новый метод
`ConsumeCtx::credit_coerce_finalize(_by_key)` (общий с уже существующим call-arg кредитом)
подключён к: `let`/`ro`/`mut`-биндингу с явной аннотацией типа, аргументу МЕТОДА (новый
реестр `ConsumeRegistry::method_param_output_keys`, зеркало `fn_param_output_keys`), полю
record-литерала (`record_field_types`) и элементу array-литерала под annotated `let`.
Return/free-fn-call-arg/if-match-ветка (та же типонезависимая «голый Ident в хвосте блока»)
уже работали корректно — проверено пробой ДО правки, не принято на веру из формулировки
находки.

**Найденный побочный пробел (НЕ входит в этот амендмент, честная пометка):**
`resolve_call_params`'s резолв ожидаемого типа для АРГУМЕНТА МЕТОДА матчит callee по ГОЛОМУ
ИМЕНИ метода глобально («определён ровно на одном типе без overload»,
`unique_method_param_types`) — коллизия имени с ЛЮБЫМ другим методом в скомпилированном
корпусе (напр. `accept`, разделяемое с `TcpListener.accept()` из std) молча гасит
expected-type propagation, и `try_coerce_leaf` не переписывает AST, хотя новый чекер-кредит
(точный по (тип, метод) через `method_param_output_keys`) всё равно срабатывает — расхождение
ловится ГРОМКО на C-компиляции (несовпадение типов), не тихим неверным значением. Отдельный,
не начатый пункт — маркер `[M-520-method-arg-name-collision]`.

### Связь

- [D55](#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы) —
  сиблинг-механизм (wrap-in: структурно выведенная обёртка); D429 = view/finalize-out
  (объявленная функцией); таблица позиций общая; подсекция «Str-литерал → []u8» RETRACTED.
- [D176](#d176-ro-t--тип-модификатор) — `ro`-возврат = носитель view-полосы.
- D133 (01-basics.md) — линейность consume; finalize-полоса разряжает ресивер в точке вставки.
- [D372](#d372-canonical-new-constructors-convention) — форма-приёмник живёт на `.new`-перегрузке.
- [compiler-conventions §3](../../docs/dev/compiler-conventions.md) — механизм декларативен,
  пары в Rust не хардкодятся.

**Статус: спека нормативна с 2026-07-18 (прод-решение, без V1-упрощений — решение владельца).
Реализация конкретных пар — [Plan 214](../../docs/plans/214-coerce-attribute.md) (Ф.0-Ф.5
слиты; std-пары str→[]u8/StringBuilder→str/WriteBuffer→[]u8 действуют, write-костыль снесён).
R14 снят, generic-образцы реализованы — [Plan 214.1](../../docs/plans/214.1-generic-coerce.md)
(2026-07-24, см. «Generic-образцы» выше); Ф.3 (снятие ограничения у extractors, Plan 222.3) —
отдельно, не в этом амендменте.**

## D433. Match-арм int-width unify + `E_MATCH_ARM_WIDTH_MISMATCH` ([M-match-arm-mixed-int-width-sentinel-coerce], Plan 172.2 followup, 2026-07-21)

**Статус:** закреплён 2026-07-21 (bugfix + enforcement-gap closure, P1 — старейший живой пункт
backlog-followups.md, семья Plan 172.2). Behavior-changing (меняет наблюдаемое принятие/отклонение
части `match`-выражений) → D-амендмент в том же слиянии, что и код.

### Что

**Мотив.** `match o { Some(v) => v, None => -1 }` в контексте `Option[u32]` (значит `v` типа
`u32`) и `None`-арм с сентинелом-литералом `-1` (типа `int`) молча компилировался в C-код,
типизированный `uint32_t`, — `-1` тихо реинтерпретировался как `4294967295` (bit pattern),
КОГДА не было внешнего ожидаемого типа, форсирующего верную ширину (например,
неаннотированный `ro r = match {...}`; аннотированный `ro r int = match {...}` или прямой
`-> int`-возврат matcha уже работали верно — там ширина брался из аннотации/return-типа, а не
из самого match). Причина: `infer_match_common_primitive` (единственный производитель
согласованного типа арм matcha) бейлил в `None` при ЛЮБОМ расхождении типов арм, и codegen'ный
legacy-фоллбек (`infer_expr_c_type`'s Match-арм / `emit_match`'s собственный arm-type-цикл)
подбирал тип ПЕРВОГО non-`nova_int` арма — произвольно, безотносительно к типу ДРУГОГО арма.
Триггер исчез из std.unicode после миграции `compose_pair → Option[u32]` (D327), но остался
в компиляторе — тихая дыра (§1 + §4 compiler-conventions.md), P1 с 2026-06-26.

### Правило

**(R1) Safe-widening unify.** Если арм'ы matcha — int-family (`Scalar`) с РАЗНЫМИ типами, но
ОДНА сторона безопасно расширяется в другую (D54 `would_narrow_into` — тот же критерий, что
уже разрешает неявное присваивание `u32`-значения в `int`-локаль без `as`; ключевое правило:
`unsigned → строго более широкий signed` безопасно, `signed → unsigned` — НИКОГДА неявно),
общий тип matcha = БОЛЕЕ ШИРОКАЯ сторона — а не тип первого/произвольного арма. Закрывает
описанный выше баг: `u32`-payload-арм + `int`-сентинел-арм теперь детерминированно унифицируются
в `int`, `-1` остаётся `-1`. Итеративно (N>2 арм): при переходе к каждому следующему арму
common сравнивается с ТЕКУЩИМ (уже расширенным) common, не с первым исходным армом.

**(R2) Genuine mismatch → `E_MATCH_ARM_WIDTH_MISMATCH`.** Если НИ ОДНА сторона не расширяется
безопасно в другую (оба направления — narrowing; типовой случай: одинаковая ширина, разная
знаковость, `i32` vs `u32` — `signed → unsigned` запрещено правилом D54 категорически, ширина
тут не помогает) — раньше это ТОЖЕ тихо бейлилось в `None` и ловило тот же произвольный-arm-
codegen-баг. Теперь — hard compile error ДО кодогена, с точкой на несовместимом арме и
заметкой на первом установленном типе; требует явного `... as <T>` на одном из арм, ровно как
уже требуется для несовместимого присваивания (D54). Это НОВОЕ отклонение ранее молча
принимавшихся программ — источник D-амендмента.

**(R3) Область: int-family (`Scalar`) ТОЛЬКО.** Расхождение НЕ-числовых типов арм
(`Float` vs `Float`, `record` vs `record`, …) не тронуто — `infer_match_common_primitive`
по-прежнему бейлит в `None` МОЛЧА для этих категорий (unchanged pre-existing behavior, вне
периметра этого маркера; отдельный follow-up при реальном триггере).

**(R4) Literal-fit amend (2026-07-21, найдено мега-CU гейтом на `d407_enum_payload_width.nv`
— ложное срабатывание R2 на УЖЕ ЖИВОЙ фикстуре).** БЕЗ типового суффикса/каста литеральный арм
(`d407W(_) => 1`) не несёт СОБСТВЕННОГО фиксированного типа — в отличие от pattern-bound
переменной (чей тип ФИКСИРОВАН типом payload'а, из которого её извлекли), голый int-литерал
гибок до помещения в контекст (D54 literal-fit; rustc-эталон: unsuffixed integer literal
унифицируется с контекстом, а не навязывает свой дефолт). ПЕРЕД R1/R2-сравнением ResolvedType'ов
чекер теперь проверяет: если один из двух конфликтующих арм — голый литерал (`IntLit` или
`Unary{Neg, IntLit}`, `-1`), тот же самый critical shape, что уже матчит D227 Rule 6 в
`assignable`), впишется ли его СЫРОЕ значение в тип ДРУГОГО арма (те же правила, что литерал в
аннотированной позиции: D227 Rule 3 sized range-check, Rule 1 no-upper-check для wide-default
`int`/`uint`, Rule 6 negative-floor для unsigned) — если да, литеральный арм ПРОСТО ПРИНИМАЕТ
тип другого арма (никакого widen, никакой ошибки); НЕ имеет значения, какой арм физически
первый (порядко-независимо: `common`/`common_lit` отслеживают, был ли текущий «общий» тип сам
литералом, чтобы ПОЗЖЕ пришедший конкретный арм тоже мог быть усыновлён). **Floor НЕ ослаблен:**
отрицательный литерал (`-1`), который НЕ вписывается в unsigned-таргет (напр. `uint`, 64-бит
безнаковый — САМ `int` тоже не расширяется безопасно в `uint` той же ширины), падает через это
исключение и остаётся genuine-mismatch → `E_MATCH_ARM_WIDTH_MISMATCH`, как и раньше (R2
неизменён для этого случая — Rule 6 категоричен, ширина не спасает). Область — та же, что R1/R2
(int-family/`Scalar` only); нечисловые литералы (`str`/`char`/…) не тронуты.

### Реализация

`compiler-codegen/src/types/mod.rs`: `infer_match_common_primitive` (unify через
`would_narrow_into`, R1) + новая `check_match_arm_width_mismatch` (диагностика, R2), обе
построены над общим `match_arm_value_types` (per-arm `(Span, ResolvedType, Option<i128>)`
экстракция — третий элемент несёт RAW-значение, если арм — голый литерал; единый источник для
канала И диагностики — §0/§3). Литерал-fit (R4) — `bare_int_literal_value` (распознаёт
`IntLit`/`Unary{Neg,IntLit}`) + `literal_fits_scalar` (те же D227-правила, что `assignable`'s
`IntLit`-ветка уже применяет к аннотированной позиции), проверяется ПЕРВЫМ в обеих функциях,
до R1-widen/R2-mismatch-ветки. Вызывается из `f1_expr`'s `ExprKind::Match`-ветки ДО
материализации канала.

### Тесты

`detect172/u172_2_match_arm_width_pos.nv` (R1: широкий/узкий unify, three-arm progressive,
explicit-`as`-unify; R4: bare-литерал усыновляет `uint`-сиблинга, порядко-независимо) +
`detect172/neg/n_match_arm_width_mismatch.nv` (R2, `EXPECT_COMPILE_ERROR
E_MATCH_ARM_WIDTH_MISMATCH`) + `detect172/neg/n_match_arm_width_negative_literal_uint.nv` (R4
floor: отрицательный литерал НЕ усыновляет `uint`, mismatch остаётся) ·
`spec_tests/conformance/d129_match_arm_width_widen.nv` (R1+R4, конформанс-CU) +
`spec_tests/conformance/neg/n_match_arm_width_mismatch.nv` (R2) +
`spec_tests/conformance/neg/n_match_arm_width_negative_literal_uint.nv` (R4 floor). Полный
мега-CU `spec_tests/conformance` (1004+ файлов, `NOVA_CACHE=0`) — **PASS 519 / FAIL 0 / SKIP
19** (включает `d407_enum_payload_width.nv`, чей ложный R2-mismatch и был найден гейтом).

### Связь

Использует [D54](03-syntax.md#d54-операторы-as-и-is) (`would_narrow_into`/narrowing-критерий —
единственный источник, тот же, что метод-arg enforcement Plan 172.2) · [D129](#d129)
(`int`≡`i64`-alias — сентинел-литерал `-1` типизируется `int`, широкая сторона unify) ·
[D327](#d327) (Codepoint=`u32` — миграция, убравшая исходный триггер из std.unicode, не из
компилятора) · Backlog `[M-match-arm-mixed-int-width-sentinel-coerce]`
(`docs/plans/backlog-followups.md`, Plan 172.2 followup, P1).

## D447. `#no_copy` — аффинность на уровне ТИПА (нельзя копировать, забыть можно) (План 248, 2026-08-05)

> Принято владельцем 2026-08-05 по итогам разбора разделяемых ручек.
> Закрывает пустую клетку таблицы [D131](05-memory.md#d131) / [D133](#d133).

### Что

Атрибут `#no_copy` на объявлении типа. Значение такого типа **нельзя связать
вторым именем**; забыть его — можно, обязанности потребления НЕТ.

```nova
#no_copy type AtomicU32 value priv { v u32 }
```

### Зачем — пустая клетка, а не дубль `consume`

| | D131 `consume` | D133 `type X consume` | **D447 `#no_copy`** |
|---|---|---|---|
| потребить ≤1 раз (нельзя копировать) | да | да | **да** |
| потребить ≥1 раз (обязан израсходовать) | нет, забыть OK | да | **нет, забыть OK** |
| ставится на | получатель/параметр метода | тип | **тип** |

Аффинность в языке была, но только на получателе метода. На **типе** имелась
одна лишь must-consume-форма — и она для этого случая непригодна по двум
причинам сразу (проверено на release-компиляторе 2026-08-05):

```
[D133-empty-consume] type `AtomicU32` помечен `consume` но не имеет ни
  consume-полей, ни consume-методов — добавь хотя бы один consume-method
  либо убери `consume` с type-decl.
[E_CONSUME_KEYWORD_MISSING] binding `a` держит consume-обязательную инстансу
  типа `AtomicU32` — требуется keyword `consume` (D180).
```

Первая требует придумать метод-расход, которого у счётчика со значением
внутри нет и быть не может: память не выделяется, закрывать нечего. Вторая
требует писать `consume a = …` при каждом объявлении переменной. Обязанность
расхода существует, чтобы не забыли закрыть ресурс; там, где ресурса нет, она
вырождается в ритуал.

### Почему признак ОБЪЯВЛЯЕТСЯ, а не выводится

Структурная проверка «полностью стековое значение» (та же, что стоит за
запретом отмывания `ro`→`mut`) решает по составу полей. Она посмотрит на
`AtomicU32 { v u32 }`, увидит внутри обычное число и заключит «копировать
безопасно». **Семантика противоречит структуре** — атомарность ячейки не
следует из её состава, — поэтому вывести запрет нельзя, он объявляется.

Это ровно тот случай, который план 248 при закрытии ветки «признак не нужен»
называл возвращающим вопрос: тип не разделяемый, не стековый по смыслу, и не
копируется по причине, невыводимой из полей.

### Форма записи

`#no_copy`, с подчёркиванием — по строю соседних атрибутов типа
(`#from_fields`, `#from_pairs`, `#zero_on_move`, `#pub_to`, `#serde`).
Ближайший образец — `#zero_on_move`: тоже атрибут типа, влияющий на семантику
перемещения.

Отвергнутая альтернатива — `#copy` (пометка наоборот, «этому типу копирование
разрешено»). Она меняет умолчание всего языка: пометку пришлось бы
расставить примерно по 317 декларациям корпуса ради 21 случая. Измерено
окном p248-copyflip 2026-08-05.

### Механизм

Признак чисто чекерный, кодоген не затрагивается. Переиспользуется
существующий flow-sensitive проход (`check_consume`, состояния
`Live`/`Consumed`/`MaybeConsumed`) с одним изъятием: для `#no_copy`-типа НЕ
выполняется финальная проверка «на выходе из области не осталось живого» —
именно она и есть разница между «ровно один раз» и «не больше одного раза».

Реестр типов ведётся с **уровнем строгости**, а не двумя списками; обход
`type_is_consume_v`, тянущий свойство через поля записи, варианты суммы,
обёртки-дженерики и кортежи, тянет и уровень — не раздваиваясь.

Диагностики — **свои**, не переиспользовать тексты `consume`: там речь про
«обязан потребить», здесь про «нельзя завести второе имя».

### Энфорс — волна 2 (план 248, 2026-08-05)

**Правило второго имени.** Значение `Affine`-типа (`#no_copy`) не может
получить второе имя. Проверяются четыре формы:

1. Голое связывание: `ro b = a`.
2. Чтение поля в локальную: `x = obj.field`.
3. Передача аргументом получателю, который его НЕ заимствует (см. ниже).
4. Встраивание в литерал записи/кортежа: `Type { field: a }`, `(a, b)`.

Диагностика — `E_NO_COPY_SECOND_NAME`. Свежая конструкция
(`Type{field: Handle{...}}`, вызов функции, бинарная операция) НЕ
считается вторым именем — второе имя относится только к уже
СУЩЕСТВУЮЩЕМУ значению (bare identifier / `@self` / `.field`-путь).

**Заимствование.** Передача `Affine`-значения в параметр — не копия,
если параметр получателя `ro` ИЛИ `mut` (амендмент волны 3, 2026-08-06:
in-out mut-параметр по D246/Plan 184 P10 не создаёт второго имени —
значение возвращается владельцу; исключён только `consume`) И тело получателя
НЕ сохраняет его: не пишет в поле, не возвращает, не встраивает в
литерал, не захватывает в замыкание/`spawn`/`detach`/`blocking`/
`supervised`, не передаёт дальше аргументом. Такая передача — заём, не
перевязка, и остаётся законной.

**`consume` и `#no_copy` на одном типе — ошибка**
(`E_NO_COPY_CONSUME_CONFLICT`): два взаимоисключающих уровня строгости
(«обязан израсходовать» против «расходовать не обязан»).

**`#no_copy` применим к видам объявления с собственным хранилищем** —
record, sum, named tuple, newtype, external opaque (то же множество, что
у `#share`, D415 §1). На alias/type-set/effect/protocol —
`E_NO_COPY_INVALID_KIND`.

### Связь

- [D131](05-memory.md#d131) — аффинный `consume` на получателе/параметре,
  исходная половина таблицы.
- [D133](#d133) — must-consume на типе, вторая половина.
- [D415](06-concurrency.md) — `#share`: ортогонален. `#share` разрешает
  разделение между файберами, `#no_copy` запрещает второе имя; на одном типе
  не конфликтуют (проверено пробой 2026-08-05).
- [D246](#d246) — три оси изменяемости; запрет отмывания `ro`→`mut` для
  не-полностью-стековых типов остаётся в силе и работает независимо.
- План [248](../../docs/plans/248-shared-handles-linearity.md) — разбор
  разделяемых ручек, откуда решение выросло.

## D450. Контракт `#from_fields`/`#from_pairs` — один конструктор `new(cap:)`

> Решение владельца 2026-08-09: «по нашей текущей концепции мы вызываем
> `new(cap:)` одной функцией, предлагаю это перенести в
> `#from_fields`/`#from_pairs`». Уточняет [D55](#d55) и [D108](#d108).
> Записано ДО реализации (норма `dev-workflow.md`: спека первой).

### Что меняется

Тип, помеченный `#from_fields` либо `#from_pairs`, обязан предоставить **один**
статический конструктор с необязательным именованным параметром вместимости:

```nova
export fn T[K, V].new(cap int = <по умолчанию>) -> Self
```

Десугаринг литерала (`[k: v, …]` и `{field: val}`) вызывает **его одного**:
`T.new(cap: <число элементов литерала>)`, затем наполняет обычным `@insert`.

### Что отменяется, а что остаётся

Прежний контракт требовал **трёх** методов — `new`, `mut @cap(n)`, `insert_new`,
— и проверялся условием `has_new && has_cap && has_insert_new`. Обязательным
остаётся **один** конструктор; остальное переходит в разряд необязательного.

| было | стало |
|---|---|
| `new` + `mut @cap(n)` + `insert_new` — все обязательны | `new(cap:)` обязателен |
| — | `insert_new` — **необязательная** оптимизация: если тип её имеет, десугаринг использует её; иначе обычный `@insert` |

**`insert_new` может быть приватной** (решение владельца 2026-08-09: «её можно
кстати делать не публичным методом, но доступным для компилятора»). Компилятор
видит все методы типа, поэтому `priv` контракту не мешает — и тогда оптимизация
не попадает в публичный договор типа.

Её предусловия («ключа нет в карте», «вместимость не меньше итогового числа»)
выполнимы для литерала: дубликаты ключей запрещены линтом
(`lint_duplicate_str_key`/`lint_duplicate_int_key`), а вместимость задаёт сам
десугаринг через `cap:`.

> **Две поправки 2026-08-09, обе от владельца.** Первая редакция обосновывала
> отмену тем, что (а) «`cap` значит не то, что в коде» и (б) «`insert_new` —
> деталь реализации, требовать её нельзя». **Оба довода сняты:** сеттер
> `mut @cap(n int) -> @` существует и жив (`hash_map/core.nv:186`; ретрактирован
> был `with_capacity`), а приватность решает вопрос с деталью реализации.
> Решение держится на третьем, исходном доводе владельца — **единообразии с
> текущей концепцией конструктора**, — и он самодостаточен: `new(cap:)` уже
> канон (`HashMap[K, V].new(cap int = 16)`, вызов `Self.new(cap: pairs.len() * 2)`),
> а контракт атрибута от него отстал.
>
> Отдельно уцелело наблюдение, не зависящее от снятых доводов: проверка «есть
> метод по имени `cap`» **не отличает сеттер от геттера** — тип с одним лишь
> `@cap() -> int` проходил её так же. Контракт проверял имя, а не способность.

### Обязательная диагностика

Молчаливое игнорирование пометки запрещается (№503 (реестр 221.1)).
Если тип помечен, но конструктора нет — **ошибка компиляции** с указанием, чего
именно не хватает:

```
error: тип `IndexMap` помечен `#from_pairs`, но не имеет конструктора
       `IndexMap[K, V].new(cap int = …)`
```

Сегодня такой тип просто выпадает из множества `from_pairs_types`, литералы в
него не коэрсятся, и пользователь не отличает «атрибут работает» от «атрибут
проигнорирован» — тот же класс, что `#zero_on_move` (№465) и `#debug invariant`
(№466).

### Следствие для `std`

`IndexMap` получает `new(cap int = …)` и обе пометки — `insert_new` ему не
нужен, десугаринг обойдётся `@insert`. `HashMap` уже соответствует и правки не
требует: его `mut @cap(n)` и `insert_new` остаются как есть, просто перестают
быть ТРЕБОВАНИЯМИ атрибута — первый полезен сам по себе, второй продолжит
использоваться десугарингом как оптимизация.

---

## D457. Видимость уровня пакета — `priv(package)` (план 269, владелец 2026-08-11)

> **Status:** ACCEPTED (2026-08-11). **Продолжает:** D281 (module-private),
> [D307](#d307-file-private-visibility--privfile-plan-170) (`priv(file)`).
> **Реализация:** [план 269](../../docs/plans/269-package-visibility.md), ДО тега
> v0.1 — см. «Почему до тега» ниже.

### Мотивация — ступень, которой не хватало

Шкала видимости была такой: `priv(file)` — только этот файл; без модификатора
(или `priv`) — модуль, то есть все peer-файлы папки; `export` — снаружи модуля,
то есть ВСЕМ, включая пользователя пакета.

Все существующие уровни **уже модуля или ровно модуль**. Не хватало ровно одного
шага шире: «видно всем модулям МОЕГО ПАКЕТА, наружу — нет».

Нехватка не гипотетическая, у неё измеренная цена. `std/src/runtime/**`
объявляет интринсики рантайма (`nova_rt_*`), и зовут их `std.concurrency`,
`std.net`, `std.io` — другие модули ТОГО ЖЕ пакета. Дать соседнему модулю увидеть
объявление можно было только через `export`, а `export` означает «всем, включая
пользователя». Отсюда замер 2026-08-11: **425 `export extern`, из них 381 в
`std/src/runtime/**`** (`sync.nv` — 327, `math.nv` — 54). Это не расхлябанность
авторов: промежуточной ступени в языке не было, а писать 380 сквозных обёрток —
работа без выгоды.

### Что

```nova
priv(package) fn helper() -> int          // виден всем модулям своего пакета
priv(package) extern "C" fn nova_rt_x()   // но НЕ виден снаружи пакета
priv(package) type Slot { … }
type Job priv(package) { … }              // и как type-modifier, симметрично D307
```

Правило: объявление с `priv(package)` разрешается использовать из любого модуля
того пакета, которому принадлежит файл (пакет = каталог с `nova.toml`, с учётом
`[lib] src` и сброса корня по [D78 rev-6](07-modules.md)), и НЕ разрешается ни из
какого другого пакета — ни через `import`, ни через re-export фасада.

Шкала целиком, от узкого к широкому:
`priv(file)` → `priv` / без модификатора (модуль) → **`priv(package)`** →
`export`.

### Почему именно такое написание

Семья `priv(<область>)` уже существует и читается буквально: «приватно на уровне
области». `priv(package)` продолжает её без нового ключевого слова. Rust решает
ту же задачу через `pub(crate)` — «публично, но до границы крейта»; мы говорим с
другой стороны — «приватно, но до границы пакета». Слово `pub` в язык не
вводится: у нас его нет, а `export` уже занят и означает ровно «наружу».

### Почему ДО тега, а не после

Добавить уровень видимости после тега можно — это аддитивно. Но сегодняшние
425 `export extern` УЖЕ попадут в публичную поверхность v0.1, и пользователь
вправе на них опереться. Спрятать их потом — ЛОМАЮЩЕЕ изменение. Откладывание
меняет цену с «час работы» на «ждать v0.2».

### Что это упрощает немедленно

Страж FFI (реестр 221.1 №569, план 268 Ф.4) перестаёт нуждаться в именованном
исключении для `runtime/**`: правило становится «`export extern` — НОЛЬ, жёстко,
без исключений», а интринсики переезжают на `priv(package)`. Исключение, которого
нет, всегда лучше исключения, которое надо помнить.

### Приёмка

Позитивная фикстура: `priv(package)`-объявление видно из другого модуля своего
пакета. Негативная: то же объявление из другого ПАКЕТА — ошибка компиляции с
собственным кодом диагностики, а не «undefined identifier». Плюс проба «подсунь
негодное»: снять проверку — негативная фикстура краснеет.

---

## D458. Одна форма для методов и функций — `@` как переменная типа получателя (план 273, владелец 2026-08-12)

> **Status:** ACCEPTED (2026-08-12). **Родня:** [план 196](../../docs/plans/196-one-truth-closeout.md)
> («одна правда» — чекер пишет канал, кодоген лоуэрит), прецедент —
> 196.7 (method-dispatch через `resolved_callees`, закрыт 2026-07-15).
> **Реализация:** [план 273](../../docs/plans/273-one-form-for-methods-and-functions.md),
> фазы Ф.0–Ф.5, до тега v0.1 (Ф.0–Ф.1 обязательны, Ф.2 по факту критерия §4.2).

### Мотивация — не синтаксис, а место в компиляторе

Метод сегодня — функция, у которой первый параметр помечен как получатель, но
в компиляторе методы и функции живут в РАЗНЫХ местах: отдельная таблица имён,
отдельный обход достижимости, отдельная мономорфизация. Каждый проход поэтому
имеет про них ДВА случая, и расхождение между случаями — не гипотеза, а
источник конкретных дефектов реестра 221.1:

- **№576** — обход достижимости DCE (`compute_dead_decls_with`) ходит по
  ИМЕНАМ; голый конструктор варианта суммы (`Blue(7)`) упоминает только имя
  варианта, никогда имя владеющего типа — метод типа вычищался как мёртвый.
- **№613** — то же самое пятно кода: значение метода (`Type.@method`) не
  считалось использованием, метод вычищался, а переходник значения на него
  ссылался.
- **№536, №534, №514** — резолв по голому имени через границу модуля:
  таблица функций плоская и CU-широкая, метод-сет — другой механизм; чинили
  трижды, класс возвращался соседней дверью.
- **№600** — прямой путь `[]T.method()` не связывает метод-уровневые generic'и.

Единая внутренняя форма убирает не эти четыре записи, а МЕСТО, откуда они
берутся: один резолв, один обход достижимости, одна мономорфизация.

### Что

Метод — сахар над компиляторной формой, в которой получатель — обычный первый
параметр с явным маркером `@`:

```nova
fn Type mut @job(a int) -> @      // ИСХОДНАЯ форма (сахар, единственная в коде)
fn job(mut @Type, a int) -> @     // КОМПИЛЯТОРНАЯ форма — внутреннее представление
```

`@` — **переменная типа получателя**, аналог `Self`: связывается ПЕРВЫМ
параметром объявления (`@Type`/`mut @Type`/`consume @Type`, позиция
фиксирована), и эта же переменная доступна в позиции возврата (`-> @`) и внутри
generic-конструкторов (`-> Option[@]`). `@` — часть ТИПА, не пометка места
объявления: тип `fn(mut @Purse, int) -> @` **несовместим** с
`fn(mut Purse, int) -> Purse` намеренно (`@` может лечь в C иначе — метод
использует получателя как контекст диспетча, обычная функция нет).

`-> @` — НЕ новая форма: она уже используется в `std` 37 раз (`sort.nv`,
`queue.nv`, `fs.nv` и родня, паттерн `with_*`-цепочек:
`fn OpenOptions mut @read(v bool) -> @ { @rd = v }`). D458 не изобретает
`-> @`, а называет, чем оно всегда было — теперь общим механизмом, а не
точечным правилом только для позиции возврата.

**Получатель — первым параметром, без исключений на первом шаге.** Разрешить
получателя в середине списка — расширение, которое всегда можно сделать
позже; сузить обратно нельзя.

### Вызов: `@` принуждает точку, без вариантов

Раз `@` в типе — вызов возможен ТОЛЬКО через точку, и это касается и обычного
метода, и метод-ЗНАЧЕНИЯ:

```nova
x.job(5)                    // обычный вызов метода — как сегодня

ro f = Purse.@bump          // значение метода, тип: fn(mut @Purse, int) -> @
ro r = x.(f)(5)             // вызов ЗНАЧЕНИЯ: получатель слева, значение в скобках
```

Позиционная форма с получателем-аргументом (`job(x, 5)`, `f(x, 5)`) не
появляется в языке НИГДЕ — ни у имени, ни у значения. `x.(f)` БЕЗ вызова —
ошибка компиляции (не частичное применение): `@` принуждает вызов через точку,
и выражение, «связавшее получателя» и более ничего, было бы значением формы,
которую нельзя вызвать иначе, — тупик системы типов.

**Компиляторная форма запрещена как ОБЪЯВЛЕНИЕ, но легальна как ТИП.** Это
разные грамматические позиции: объявление создаёт callable, аннотация лишь
называет тип уже существующего значения.

```nova
fn job(mut @Type, a int) -> @ { … }        // ЗАПРЕЩЕНО: объявление в компиляторной форме
                                             // → E_D458_COMPILER_FORM_IN_SOURCE,
                                             // fix-it на `fn Type mut @job(a int) -> @`

ro f fn(mut @Type, a int) -> @ = Type.@job  // ЛЕГАЛЬНО: компиляторная форма как ТИП
```

**Явное преобразование метод-значения в обычную функцию — запрещено в обе
стороны.** Не заводится каста `as fn(...)`, стирающего `@`: для кода высшего
порядка, которому нужна функция без получателя, используется обычная лямбда
(`fn(mut q Type, k int) -> int => q.job(k)`) — она уже в языке, видна на месте
вызова и не создаёт второй способ делать то, что делает первый.

### Почему не разрешаем компиляторную форму нигде, кроме типа

Форма, легальная в одном контексте и запрещённая в другом БЕЗ названной
ошибки, становится источником вопросов «а почему там можно». Ошибка
`E_D458_COMPILER_FORM_IN_SOURCE` гейтится на грамматической позиции
ОБЪЯВЛЕНИЯ (`fn <возможно-с-получателем>(...) { … }` / `=> …`), не на встрече
компиляторной формы вообще — иначе она ложно сработала бы на каждой аннотации
типа метод-значения, которая обязана оставаться легальной.

### Родство с планом 196 — обязательное условие реализации

Единая форма — не переименование, а сведение К ОДНОМУ каналу: чекер резолвит
методы и функции ЕДИНЫМ резолвером в `resolved_callees`, кодоген лоуэрит
результат, а не переизобретает по имени. Реализация ОБЯЗАНА строиться на этом
канале (прецедент — 196.7, коллизия bare-T бланкета с фасадным методом уже
закрыта тем же приёмом); параллельный механизм резолва означал бы не одну
дверь, а третью — ровно то, что D458 существует закрыть.

### Смежное, но не часть этого D-блока

Реестр 221.1 №625: резолв неоднозначного значения метода по аннотации
let-binding (в отличие от `as fn(...)`) сегодня не работает — известный
дефект, не введённый D458, но который D458 делает заметнее (метод-значения
становятся центральным механизмом языка). Чинится отдельно, тем же принципом
«один резолв, оба синтаксиса передают в него целевой тип», до начала Ф.2.

### Приёмка

- Негативная фикстура: компиляторная форма как объявление вне типовой позиции
  → `E_D458_COMPILER_FORM_IN_SOURCE` с fix-it.
- Позитивная: та же форма в позиции типа (аннотация связывания) — легальна.
- Побайтово идентичный СИ-выход десугаринга на корпусе (мега-CU + `std`) до и
  после — критерий Ф.1, расхождение объясняется пофайлово, не списывается на
  «так тоже правильно».
- Все 37 существующих мест `-> @` в `std` — то же поведение, не проверяются
  саботажем заново (уже покрыты собственными тестами), а лишь остаются
  собранными.
- Каждая фаза удаления (Ф.2, Ф.3) называет СНЯТЫЙ метод-специфичный путь и
  показывает фикстуру, которая теперь идёт общим путём — приёмка
  формулируется как удаление, не как добавление.
