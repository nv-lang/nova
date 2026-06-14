# Auto-derive Guide (Plan 126, D109 amend + D230)

> **Status:** ✅ landed 2026-06-05.
> **D-blocks:** [D109 amend](../spec/decisions/08-runtime.md#d109-amend-plan-126-2026-06-05---auto-derive-для-пользовательских-типов) + [D230 NEW](../spec/decisions/02-types.md#d230-new--Clone-protocol-plan-126-ф1).

Nova поддерживает **auto-derive** для пяти built-in протоколов через
`#impl(P)` annotation на пользовательском типе. Аналог Rust `#[derive(...)]`
без отдельного keyword'а — переиспользуется единый mechanism `#impl(P)` (D186).

## TL;DR

```nova
#impl(Equal + Hash + Clone + Compare + Display)
type Vec3 {
    x f64
    y f64
    z f64
}

ro a = Vec3 { x: 1.0, y: 2.0, z: 3.0 }
ro b = Vec3 { x: 1.0, y: 2.0, z: 3.0 }
assert(a == b)             // auto-derived @equal
ro c = a.clone()           // auto-derived @clone
ro h = a.hash()            // auto-derived @hash
ro cmp = a.compare(b)      // auto-derived @compare
```

Компилятор синтезирует тела методов **memberwise рекурсивно** на основе полей
типа.

## Поддерживаемые протоколы

| Protocol     | Метод                          | Стратегия synth                            |
|--------------|--------------------------------|---------------------------------------------|
| `Equal`  | `@equal(other) -> bool`       | memberwise `&&` chain                       |
| `Hash`   | `@hash() -> u64`               | XOR + rotate FxHash-style combine           |
| `Clone`  | `@clone() -> Self` ([D230](../spec/decisions/02-types.md#d230-new--Clone-protocol-plan-126-ф1)) | record literal с `.clone()` per field |
| `Compare` | `@compare(other) -> int`       | lexicographic if-chain (memcmp-style)       |
| `Display`  | `@display(sb) -> ()`               | `sb.append("TypeName { f: v, ... }")` chain |

Все 5 — single-method built-in protocols, объявлены в `std/prelude/protocols.nv`.

## Когда compiler synthesize'ит

1. Type помечен `#impl(P)` где `P` — один из 5 built-in protocols.
2. Type **не** предоставляет explicit `fn T @method(...)` — иначе user wins.
3. Все поля type'а **eligible** — primitive ИЛИ имеют `#impl(P)` ИЛИ
   имеют explicit `fn FieldType @method`.

Если хотя бы одно условие нарушено — diagnostic из `E_AUTO_DERIVE_*` family
(см. ниже).

## Когда compiler НЕ synthesize'ит

- **Protocol не built-in** (user-defined protocol) — auto-derive только для
  5 known built-in. User-defined protocols → user пишет body вручную.
- **Type provides explicit method** — `fn T @equal(other) -> bool => ...`
  wins над auto-derive (manual override).
- **Field type не implement** требуемый protocol →
  `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL`.

## Field eligibility

Каждое поле type'а должно быть одним из:

| Категория поля | Что делает synthesizer |
|---|---|
| Primitive (`int`/`f64`/`bool`/`char`/`byte`/`str`/`u*`/`i*`) | Inline copy/compare/hash через built-in routines |
| `#impl(P)` annotated record/tuple | Recursive call `@field.method(...)` |
| Explicit `fn FieldType @method` | Direct dispatch к user-provided method |
| `[]T` array | Recursive по `T` |
| Tuple `(A, B, ...)` | Recursive по element types |

Что **не eligible** — `fn(...)` types, pointers `*T`, opaque types, protocol
types (требуют explicit user impl).

## Примеры

### Простой record

```nova
#impl(Equal)
type Money {
    cents int
}

ro a = Money { cents: 100 }
ro b = Money { cents: 100 }
assert(a == b)  // → @a.cents == b.cents → true
```

### Рекурсивный auto-derive

```nova
#impl(Clone)
type Inner {
    name str
    code int
}

#impl(Clone)
type Outer {
    inner Inner       // ← Inner has #impl(Clone) — eligible
    count int
}

ro o = Outer { inner: Inner { name: "x", code: 1 }, count: 5 }
ro p = o.clone()
// synthesized:
//   Outer { inner: @inner.clone(), count: @count }
// → Outer { inner: Inner { name: @name, code: @code }, count: 5 }
```

### Manual override (user wins)

```nova
#impl(Equal)
type CaseInsensitive {
    text str
}

// User implements @equal — wins над auto-derive.
fn CaseInsensitive @equal(other CaseInsensitive) -> bool =>
    @text.to_lower() == other.text.to_lower()

ro a = CaseInsensitive { text: "Hello" }
ro b = CaseInsensitive { text: "HELLO" }
assert(a == b)  // → user-defined logic
```

### Named tuple (Plan 120 D215)

```nova
#impl(Equal + Clone)
type Pair(left int, right int)

ro p = Pair(1, 2)
ro q = Pair(1, 2)
assert(p == q)
ro r = p.clone()
```

### Heap-record `==` override

До Plan 126 на heap-record `a == b` был **identity-eq** (pointer comparison).
После Plan 126:

```nova
// Без #impl(Equal) — identity-eq preserved (backward compat).
type Account {
    id int
    balance f64
}
ro a = Account { id: 1, balance: 100.0 }
ro b = Account { id: 1, balance: 100.0 }
assert(a != b)  // ← разные allocation'ы, identity не совпадает

// С #impl(Equal) — structural eq.
#impl(Equal)
type AccountStruct {
    id int
    balance f64
}
ro x = AccountStruct { id: 1, balance: 100.0 }
ro y = AccountStruct { id: 1, balance: 100.0 }
assert(x == y)  // ← memberwise structural eq
```

## Диагностики (Plan 126 Ф.4)

| Код                                  | Когда триггерится                                                              |
|---------------------------------------|--------------------------------------------------------------------------------|
| `E_AUTO_DERIVE_CYCLE`                 | Cyclic recursion через fields не терминируется                                 |
| `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL`  | Field type не implement требуемый protocol                                     |
| `E_AUTO_DERIVE_UNKNOWN_PROTOCOL`      | Protocol не в built-in list (`Equal`/`Hash`/`Clone`/`Compare`/`Display`) |
| `E_AUTO_DERIVE_UNSUPPORTED_KIND`      | Type kind (Newtype/Alias/Effect/Protocol/Opaque) не поддерживает derive        |

### Пример E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL

```nova
type Plain {
    n int
}

#impl(Equal)
type Wrapper {
    inner Plain    // ← Plain не #impl(Equal)
}
// ❌ E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL:
//   type `Wrapper` claims `#impl(Equal)` but field `inner`
//   (type `Plain`) does not implement `Equal`.
//   Either add `#impl(Equal)` to `Plain`, или provide explicit
//   `fn Wrapper @equal(...)`.
```

**Fix**: добавить `#impl(Equal)` на `Plain`:

```nova
#impl(Equal)   // ← Fix: now Plain eligible
type Plain {
    n int
}

#impl(Equal)
type Wrapper {
    inner Plain
}
```

## Cycle detection

Compiler ведёт **visited set** `(type, protocol)` во время synthesis. Если
synthesis для типа `T` уже идёт, и встречается рекурсивный путь обратно к
`T` — `E_AUTO_DERIVE_CYCLE`:

```nova
#impl(Clone)
type A { b B }

#impl(Clone)
type B { a A }
// ❌ E_AUTO_DERIVE_CYCLE: cyclic recursion через fields не терминируется.
//    Provide explicit `fn A @clone(...)` or `fn B @clone(...)`.
```

**Fix**: явный impl на одном из типов разрывает рекурсию:

```nova
#impl(Clone)
type A { b B }

fn A @clone() -> A => A { b: @b }   // ← manual; синтезатор для B продолжит работать
```

## Композиция с Plan 124.x семантикой

Auto-derive **совместим** с:

- **`priv` field modifier** ([Plan 124.1/D220 §3.3.1](../spec/decisions/02-types.md#d220)):
  synthesizer работает в type-method scope — имеет доступ к priv-полям.
- **`mut` field modifier** ([D33](../spec/decisions/02-types.md#d33)):
  `mut`-fields копируются как обычные fields, mutability preserve'ится в new value.
- **`ro` binding** ([D33](../spec/decisions/02-types.md#d33), [D175](../spec/decisions/02-types.md#d175)):
  synthesized methods receive `ro Self` receiver — only-read access.
- **Value-record `type X value { ... }`** ([Plan 124.8 D228](../spec/decisions/02-types.md#d228)):
  full support, synthesis работает идентично heap-record.
- **Named tuple `type X(a int, b str)`** ([Plan 120 D215](../spec/decisions/02-types.md#d215)):
  fields обрабатываются через `NamedTupleField` ровно как `RecordField`.

## Что НЕ supported V1 (followup)

| Marker                          | Описание                                                       |
|---------------------------------|----------------------------------------------------------------|
| `[M-126-sum-equal-rich]`        | Sum-type @equal — variant tag + payload recursion             |
| `[M-126-sum-hash-rich]`         | Sum-type @hash — discriminant + payload combine                |
| `[M-126-sum-clone-rich]`        | Sum-type @clone — match-arms с payload recursion               |
| `[M-126-sum-compare-rich]`      | Sum-type @compare — variant ordering                           |
| `[M-126-sum-fmt-rich]`          | Sum-type @display — variant-aware output                           |
| `[M-126-codegen-method-table]`  | V1: synthesized FnDecl не register'ится в method_table. Codegen wiring для full `a == b` runtime semantics — V2 expansion |

V1 fokuses на type-check level — auto-derive **suppresses** `E_IMPL_MISSING_METHODS` корректно, что разблокирует pattern usage в downstream type-checked code. Полное `==` wiring через method_table — Plan 126 V2 (когда понадобится в production stdlib).

## Метод-уровень `#impl(P)` — opt-in конформность (D268, Plan 154.1)

`#impl(P)` как ведущий атрибут работает не только на **типе** (auto-derive выше),
но и на отдельной **метод-декларации** — это **необязательная** пометка «этот метод
реализует метод протокола `P`»:

```nova
#impl(Display)
fn int @display(mut sb StringBuilder) -> () { sb.append(@) }
```

- **Opt-in, не required.** Конформность остаётся **структурной** — тип с подходящим
  методом удовлетворяет бонд `[T Display]` и без `#impl`. `#impl` лишь **добавляет**
  проверку подписи против `P` + явно привязывает `P` к receiver-типу
  (`type_impl_protocols`), как если бы `P` был перечислен на `type`-декларации.
- **Три кода ошибок** (checker): `E_IMPL_UNKNOWN_PROTOCOL` (P не протокол),
  `E_IMPL_NOT_A_PROTOCOL_METHOD` (`@m` не объявлен в `P`),
  `E_IMPL_SIGNATURE_MISMATCH` (подпись/receiver-mut не совпадает).
- **Где применяется в stdlib:** все 6 примитивов (`int/f64/bool/char/str/f32`) получили
  конкретные `#impl(Display)` + `#impl(Debug)` в [protocols.nv](../std/prelude/protocols.nv) —
  это чинит мис-диспатч `Vec[T].debug(sb)` на примитивном элементе (Plan 154.1 / D269).

Подробности — [D268](../spec/decisions/10-overloading.md#d268-opt-in-конформность-протоколов-impl-на-метод-декларации)
и [Plan 154.1](plans/154.1-impl-conformance-primitive-format.md).

## См. также

- [Plan 126 — Auto-derive протоколов](plans/126-auto-derive-protocols.md) —
  весь roadmap, design rationale, AC list.
- [D268 / D269 — метод-уровень `#impl` + конкретные Display/Debug примитивов](../spec/decisions/10-overloading.md#d268-opt-in-конформность-протоколов-impl-на-метод-декларации)
  (Plan 154.1).
- [D109 amend](../spec/decisions/08-runtime.md#d109-amend-plan-126-2026-06-05---auto-derive-для-пользовательских-типов)
  — auto-derive rules.
- [D230 NEW](../spec/decisions/02-types.md#d230-new--Clone-protocol-plan-126-ф1) —
  Clone protocol semantics.
- [D186 — `#impl(P)` annotation](../spec/decisions/02-types.md#d186) —
  foundation infrastructure.
- [std/prelude/protocols.nv](../std/prelude/protocols.nv) — protocol
  declarations source-of-truth.
