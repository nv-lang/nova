# План 11: Method values + overload resolution

**Статус:** ✅ **ЗАКРЫТ (2026-05-08, вечер).** Ф.1-Ф.3 + Ф.4 + Ф.4.5 +
Ф.5 + Ф.6 + Ф.7 + Ф.9 готовы. Ф.8 (sweep std для overload-rename) —
optional, не блокер. Method values как first-class работают (bound /
unbound / static), overloaded method values disambig'ятся через
`as fn(P...) -> R`.
**Дата создания:** 2026-05-08.
**Зависимости:**
- [D35](../../spec/decisions/03-syntax.md#d35) — методы через `@`/`.`.
- [D22](../../spec/decisions/03-syntax.md#d22) — функции/лямбды.
- [D20](../../spec/decisions/03-syntax.md#d20) — function type syntax.
- [D46](../../spec/decisions/03-syntax.md#d46) — operator overloading
  (specific case).
- [D73](../../spec/decisions/08-runtime.md#d73) — `From`/`Into`
  multi-defines (specific case).
- [Q-overloading](../../spec/open-questions.md#q-overloading) —
  закрывается частично.

---

## Проблема

**Две связанные spec-feature не реализованы в bootstrap-codegen:**

### 1. Method values (как first-class values)

Spec ([syntax.md:762-770](../../spec/syntax.md), D35) описывает:

```nova
acc.balance()              // вызов
acc.balance                // bound method value, тип fn() -> money
Account.@balance           // unbound, тип fn(Account) -> money
Account.new                // static, тип fn(str) -> Account
```

**Реальность:** `acc.balance` без скобок в bootstrap-codegen
**не работает** — нет специальной обработки для «метод как value».
Программист может только **вызвать** метод, не сохранить.

```nova
let f = acc.balance        // ❌ скорее всего compile error или
                            //    некорректный codegen
let m = nums.map(double)   // ❌ если double это метод/named fn,
                            //    не работает как value
```

### 2. Overload по типу аргумента

[Q-overloading](../../spec/open-questions.md#q-overloading) описывает
текущее состояние:

| Ось перегрузки | Bootstrap | Прецеденты |
|---|---|---|
| **По receiver-типу** (`fn int @m()` vs `fn str @m()`) | ✅ Работает | Rust impl |
| **По типу результата** (D73/D77) | ✅ Работает | Haskell |
| **По типу аргумента** (`fn T @m(s str)` vs `fn T @m(b []byte)`) | ❌ Last-wins | Java/Swift |
| **По arity** (разное число аргументов) | ❌ Last-wins | C# |

Причина: `method_receivers: HashMap<name, (recv_ty, is_instance)>`
имеет ключ **только из имени метода**. Re-define переписывает.

Это блокирует:

```nova
fn Buffer mut @write(s str) -> () => ...
fn Buffer mut @write(b []byte) -> () => ...   // ← перезаписывает первое в bootstrap

let buf = Buffer.new()
buf.write("hello")           // ✓ или ✗ — зависит от порядка
buf.write([0xDE, 0xAD])      // ✗ если первое было write(str)
```

D73 имеет специальное исключение — `T.from(V1)` и `T.from(V2)`
**различаются** по arg-type через специальный dispatch. Но **только
для `from`/`into`/`try_from`/`try_into`** (D73 4-way auto-derive).
Для произвольных методов — не работает.

### Связанность задач

Method values и overload — **связаны** потому что:

```nova
type T {}
fn T @m(s str) -> int => ...
fn T @m(b []byte) -> int => ...

let f = t.@m            // ← КАКОЙ метод? str-версия или []byte?
```

Если **только один** метод с именем `m` — `t.@m` это однозначное
method value. Если **два** перегруженных — нужен **disambiguation**:

```nova
let f1 = t.@m as fn(str) -> int        // явная aннотация
let f2 = t.@m as fn([]byte) -> int     // другая
```

Поэтому method values **должны** учитывать overload-resolution. Один
план покрывает обе темы.

---

## Цель

После плана 11:

- ✅ `acc.balance` (без скобок) работает как bound method value,
  тип `fn() -> money`.
- ✅ `Account.@balance` работает как unbound method value,
  тип `fn(Account) -> money`.
- ✅ `Account.new` работает как static method value,
  тип `fn(str) -> Account`.
- ✅ Overload по типу аргумента: `Buffer @write(str)` vs
  `Buffer @write([]byte)` различаются на call-site по типу
  аргумента.
- ✅ Overload по arity: `Logger @log(msg)` vs `Logger @log(level, msg)`.
- ✅ Disambiguation через type annotation: `let f = t.@m as fn(str) -> int`.
- ✅ Method values и overload работают совместно.
- ✅ `Self.method(...)` в expression position работает (D66
  расширение): `fn Account.new() -> Self => Self.with_initial(0)`.
- ✅ `Self { ... }` literal работает (D66): `fn Box[T].of(v T) -> Self
  => Self { value: v }`.

---

## Не цель

- **Generic-bound based dispatch** (`fn @m[T Encodable](v T)`).
  Это план для после реализации generic bounds enforcement
  (план 08 Ф.6).
- **Variance / subtyping** в overload-resolution. Nova не имеет
  subtyping (D1 — no inheritance), это упрощает.
- **Implicit conversions** при overload-resolution (как в C++).
  Мы не применяем `int → f64` чтобы matching'нуть `f(f64)` вместо
  `f(int)`. Strict matching типов.
- **Полное Q-overloading закрытие.** Q описывает 4 варианта (1 ad-hoc,
  2 D73-style, 3 разные имена, 4 protocol-based). План 11 закрывает
  только **вариант 1** (ad-hoc). Вариант 4 (protocol-based) — будущее.

---

## Что делаем

### Ф.1 — Расширить method registry в codegen

В `compiler-codegen/src/codegen/emit_c.rs`:

```rust
// Старое:
method_receivers: HashMap<String, (String, bool)>;
// key = method_name, value = (receiver_type, is_instance)
// → last-wins при overload

// Новое:
method_receivers: HashMap<String, Vec<MethodSig>>;
struct MethodSig {
    receiver_type: String,
    is_instance: bool,
    param_types: Vec<String>,
    return_type: String,
    is_mut: bool,
    effects: Vec<String>,
}
```

При regis'тре нового метода — добавляем в `Vec`, не replace'им.

### Ф.2 — Overload resolution на call-site

При `obj.method(args)` или `Type.method(args)` codegen:

1. Найти все signatures для `method` в registry.
2. Filter по receiver type (matches `obj`'s type).
3. Filter по arity (matches `args.len()`).
4. Filter по argument types (strict match с `infer_expr_c_type(arg)` для каждого).
5. Если **ровно один** matches — эмитить вызов.
6. Если **>1** matches — **ambiguity error** с suggestion'ом disambiguate
   через `as fn(...)` annotation.
7. Если **0** matches — error «no matching overload, available: <list>».

```rust
fn resolve_overload(
    method_name: &str,
    receiver_ty: &str,
    arg_types: &[String],
) -> Result<&MethodSig, ResolveError> {
    let candidates: Vec<&MethodSig> = self.method_receivers
        .get(method_name)
        .into_iter()
        .flatten()
        .filter(|sig| sig.receiver_type == receiver_ty)
        .filter(|sig| sig.param_types.len() == arg_types.len())
        .filter(|sig| sig.param_types.iter().zip(arg_types).all(|(a, b)| a == b))
        .collect();

    match candidates.len() {
        0 => Err(no_match(method_name, receiver_ty, arg_types, available)),
        1 => Ok(candidates[0]),
        _ => Err(ambiguous(method_name, candidates)),
    }
}
```

### Ф.3 — C-side mangling для перегруженных методов

Сейчас codegen эмитит `Nova_T_method_name(...)`. С overload —
несколько функций с одним именем не уживутся в C. Нужен **name
mangling** по сигнатуре:

```c
// Nova:
//   fn Buffer mut @write(s str) -> ()
//   fn Buffer mut @write(b []byte) -> ()

// C output:
void Nova_Buffer_method_write_str(Nova_Buffer* self, nova_str s);
void Nova_Buffer_method_write_NovaArray_nova_byte(Nova_Buffer* self, NovaArray_nova_byte* b);
```

Mangling: `<original>_<param_type_1>_<param_type_2>_...`. Для unique
signatures — unique C names.

При вызове на call-site — codegen знает выбранный `MethodSig`,
эмитит mangled name.

### Ф.4 — Method values как first-class

Когда видим `obj.method` без скобок (или `Type.@method`,
`Type.method`):

1. Определить kind:
   - `obj.method` → **bound method value**.
   - `Type.@method` → **unbound** (явный `@` после точки).
   - `Type.method` → **static method value**.

2. Получить overloads (как Ф.2).

3. Если **один** match (по контексту — typed binding или передача
   в параметр известного типа) — эмитить function pointer.

4. Если **несколько** — ambiguity, требовать `as fn(...)` annotation.

Эмиссия в C (для bound):

```c
// Nova: let f = acc.balance
// C output:
typedef struct {
    nova_money (*fn_ptr)(Account*);
    Account* self;
} BoundMethod_Account_balance;

BoundMethod_Account_balance f = {
    .fn_ptr = Nova_Account_method_balance,
    .self = acc
};

// Вызов f():
nova_money m = f.fn_ptr(f.self);
```

Это **closure-as-struct** — pointer + captured self. Стандартный pattern для closures в C.

Для **unbound** и **static** — просто function pointer:

```c
// let g = Account.@balance
nova_money (*g)(Account*) = Nova_Account_method_balance;

// let h = Account.new
Account* (*h)(nova_str) = Nova_Account_static_new;
```

### Ф.4.5 — `Self.method(...)` в expression position

D66 описывает `Self` как «текущий тип» в любом методе. В **type
position** (return type, parameter type) — уже работает в bootstrap'е.
В **expression position** (call-site) — **не работает**:

```nova
type Account { balance money }

fn Account.new() -> Self => Self.with_initial(0)
//                          ^^^^^^^^^^^^^^^^^^^^
//                          ↑ Self.method(...) — call current type's static
//                            Сейчас в codegen: ошибка либо incorrect emission

fn Account.with_initial(amount money) -> Self =>
    Self { balance: amount }
//  ^^^^^^^^^^^^^^^^^^^^^^^^
//  ↑ Self { ... } literal — тоже expression position
```

Нужен **resolver** который при встрече `Self` в:
- **Call expression** (`Self.make(...)`): резолвит в `<current_type>.make(...)`.
- **Literal** (`Self { ... }`): резолвит в `<current_type> { ... }`.
- **Static method ref** (`Self.@balance`): резолвит как unbound method
  value (см. Ф.4).

Алгоритм: как `current_receiver_type` уже используется для type-position
(`-> Self` в emit_c.rs:659-666), но расширить на expression-position.

**Use-cases (мотивирующие):**

1. **Default → parameterized constructor:**
   ```nova
   fn HashMap[K, V].new() -> Self => Self.with_capacity(16)
   ```

2. **Generic constructor DRY:**
   ```nova
   fn Box[T].of(v T) -> Self => Self { value: v }
   ```

3. **Builder pattern с self.method'ами:**
   ```nova
   fn Builder.empty() -> Self => Self.with_default()
   fn Builder.with_default() -> Self => Self { ... }
   ```

Это **прецедент Rust** (`impl Foo { fn make() -> Self { Self::new(2) } }`)
и Swift (`Self.method()`). Spec D66 разрешает; реализация — расширение
этого плана.

### Ф.5 — Type annotation для disambiguation

```nova
let f = t.@m                                  // ambiguous (два overload'а)
let f = t.@m as fn(str) -> int                // disambiguated
let f fn(str) -> int = t.@m                   // через let-annotation, тоже работает
```

Codegen использует **target type** из контекста — let-annotation,
type cast, parameter type — чтобы выбрать конкретный overload.

### Ф.6 — Update spec

#### Ф.6.1 — D35 расширение

Добавить раздел «Перегрузка методов»:

```markdown
### Overload по типу аргумента и arity

Несколько определений одного метода на одном receiver-типе различаются
по сигнатуре (тип параметров и/или arity):

\`\`\`nova
fn Buffer mut @write(s str) -> ()
fn Buffer mut @write(b []byte) -> ()
fn Buffer mut @write(c char) -> ()
\`\`\`

Resolution на call-site по статическим типам аргументов:

\`\`\`nova
buf.write("hello")        // → @write(str)
buf.write([0xDE, 0xAD])   // → @write([]byte)
buf.write('A')             // → @write(char)
\`\`\`

При ambiguity — compile error с suggestion'ом disambiguate через
`as fn(...)` annotation.

Strict matching типов: no implicit conversions. `buf.write(42)` где
`42 int` — error если нет `@write(int)`. Программист пишет
`buf.write(42 as char)` или `buf.write(int.to_str(42))`.
```

#### Ф.6.2 — Q-overloading закрытие

Помечен ✅ CLOSED by D35 extension (Variant 1 ad-hoc overload по
типу аргумента). Variant 4 (protocol-based) остаётся отдельным
будущим Q.

#### Ф.6.3 — D22 / D20 — method values

D22 уже описывает (строки 762-770 syntax.md). Добавить cross-link
на D35-extension.

### Ф.7 — Тесты

`nova_tests/syntax/method_values.nv`:
- `acc.balance` (bound) — сохранение, повторный вызов.
- `Account.@balance` (unbound) — вызов с явным self.
- `Account.new` (static) — вызов как функция.
- Method value передан в `nums.map(int.@to_str)`.
- Bound method переживает scope-exit (если runtime поддерживает).

`nova_tests/syntax/overload.nv`:
- `@write(str)` vs `@write([]byte)` — call-site dispatch.
- Arity overload: `@log(msg)` vs `@log(level, msg)`.
- Ambiguity error при `t.@m` без context'а.
- Disambiguation через `as fn(...) -> ...`.
- Strict matching: `@m(int)` не подбирается для `m(42 as f64)`.

`nova_tests/syntax/self_in_expr.nv`:
- `Self.method(args)` в static-методе резолвится в `<type>.method(args)`.
- `Self { fields }` literal резолвится в `<type> { fields }`.
- `Self.@method` в static-method — unbound method value.
- Generic-types: `Box[T].of` использует `Self { value: v }` корректно
  (Self ≡ `Box[T]`, не `Box`).
- Self в overload context: `Self.from(2)` выбирает overload по
  `int`-параметру.

### Ф.8 — Sweep std

Проверить кандидаты на использование overload вместо `_str`/`_bytes`
суффиксов:
- `std/encoding/base64.nv` — может быть `encode(str)` / `encode([]byte)`.
- `std/crypto/*.nv` — `update(str)` vs `update([]byte)`.

Sweep — **отдельный коммит после плана 11**, не блокер. Существующие
имена с суффиксами продолжат работать.

### Ф.9 — Anonymous embed `use _ Type` + override-precedence

Расширение D39 — anonymous embed (без alias-имени) для simple
wrappers. Полностью описано в [02-types.md → D39 «Anonymous embed»](../../spec/decisions/02-types.md#d39).

#### Подзадачи

**Ф.9.1 — Parser:** добавить `use _ Type` форму. Сейчас
`use name Type` парсится как `KwUse Identifier Type`.
`use _ Type` — `KwUse Underscore Type` (или single identifier `_`
игнорируется как поле, особый case в pattern_binding).

**Ф.9.2 — Method registry kind:** добавить флаг
`MethodKind::Delegated` в `MethodSig` (план 11 Ф.1). При AST-walk'е:
- Own-методы (declared on receiver) → `MethodKind::Own`.
- Anonymous embed methods (auto-proxy from `use _ Type`) →
  `MethodKind::Delegated`.

**Ф.9.3 — Override-precedence в `resolve_overload`:**

```rust
fn resolve_overload(...) -> Result<&MethodSig, ResolveError> {
    let candidates = self.method_overloads.get(&(recv, name)).into_iter().flatten();

    // First pass: filter by arity + arg-types (как уже было в Ф.2)
    let matching: Vec<&MethodSig> = candidates
        .filter(|sig| sig.param_types.len() == arg_types.len())
        .filter(|sig| sig.param_types.iter().zip(arg_types).all(|(a, b)| a == b))
        .collect();

    // NEW: Override-precedence — Own > Delegated
    let own: Vec<&MethodSig> = matching.iter()
        .filter(|s| s.kind == MethodKind::Own)
        .copied().collect();
    let pool = if !own.is_empty() { own } else { matching };

    match pool.len() {
        0 => Err(no_match(...)),
        1 => Ok(pool[0]),
        _ => Err(ambiguous(...)),
    }
}
```

Если хотя бы один candidate с `MethodKind::Own` matches — выбираем
его, игнорируя Delegated. Это даёт **«override wins»** без
declaration-time check'а.

**Ф.9.4 — Multi-anonymous detection:**

```nova
type Wallet {
    use _ Account
    use _ Account              // ✗ COMPILE ERROR
}
```

При AST-walk'е: подсчитать `use _ T` per type. Если `count > 1`
для любого T → compile error «multiple anonymous embeds of `T`,
use named alias for disambig».

Это **declaration-time** проверка, но **только на ambiguity
unresolvable** (когда оба candidate имеют тот же priority). Не
проверяет collision с own-methods (это lazy).

**Ф.9.5 — Lint warning «possible infinite recursion»:**

Когда программист определяет own-method который имеет тот же name
что delegated в anonymous embed:

```nova
type Set[T] { use _ HashMap[T, ()] }

fn Set[T] mut @insert(item T) -> bool {
    @insert(item, ())   // ← @insert это рекурсивный self-call,
                         //   а не call к delegated HashMap.insert
}
```

`@insert(item, ())` это own-method recursive call → infinite recursion.
Программист, скорее всего, хотел `@<base>.insert(item, ())` —
но **anonymous embed не даёт имени**.

**Lint warning** (не error) при detection:
```
warning: possible infinite recursion in @insert
  → method `insert` is also delegated through `use _ HashMap[T, ()]`,
    but anonymous embed has no name for explicit base-call
  → consider `use map HashMap[T, ()]` to enable `@map.insert(...)`
```

#### Тесты

`nova_tests/syntax/anonymous_embed.nv`:
- `Set[T]` с `use _ HashMap[T, ()]` — auto-proxy работает.
- Override own-method (own wins).
- Multiple anonymous of same type → compile error.
- Lint warning на potential recursion.

#### Sweep std

Текущий `std/collections/set.nv` использует `use map HashMap[T, ()]`
с `@map.X` calls в теле — это правильно, **не мигрируем** на
anonymous. Когда появятся **новые** simple wrappers — могут
использовать `use _`.

---

## Acceptance criteria

- ✅ `acc.balance` без скобок — bound method value, тип `fn() -> money`.
- ✅ Method value передаётся в higher-order функцию (`map`, `filter`).
- ✅ `Buffer @write(str)` и `Buffer @write([]byte)` сосуществуют,
  call-site dispatch работает.
- ✅ Arity overload: `@log(msg)` vs `@log(level, msg)`.
- ✅ Ambiguity error с suggestion'ом disambiguate.
- ✅ Strict argument-type matching, no implicit conversions.
- ✅ Все существующие тесты PASS (нет регрессий — `method_receivers`
  переход на `Vec<MethodSig>` не должен ломать single-overload код).
- ✅ Spec обновлён: D35 раздел «Перегрузка методов»,
  Q-overloading ✅ CLOSED.
- ✅ Anonymous embed `use _ Type` (Ф.9): auto-proxy работает,
  override-precedence (own wins), multiple anonymous of same type →
  compile error, lint warning на potential recursion.

---

## Trade-offs / упрощения

### No implicit conversions в overload resolution

C++ применяет implicit conversions (`int → double`) при resolve.
Это вводит **subtle behavior** — `f(42)` может выбрать `f(double)`
вместо `f(int)` если программист добавит overload позже.

Nova **не делает этого**. Strict argument-type match. Если программист
хочет конверсию — пишет `f(42 as f64)`. AI-friendly: один путь, нет
скрытых dispatch правил.

### Bound method value через struct (а не closure)

Bound method = pointer + self. Можно реализовать через **closure**
(C-функция с captured environment), но это потребует heap-allocated
closure structs и call-site indirection через interface. **Дешевле:**
struct из 2 полей, call-site вызывает `f.fn_ptr(f.self)` напрямую.

Trade-off: bound method не можно передать в interface ожидающий
plain `fn() -> T`. Требует **closure adapter** — отдельная задача
если понадобится.

### Mangling по полным C-type именам

Альтернатива — короткие hashes (`Nova_Buffer_method_write_a3f2`).
Plus читаемость в C-output. Минус — длинные имена для array/generic
типов (`NovaArray_NovaTuple2_nova_int_nova_str`). Решаем в пользу
читаемости.

### Не делаем generic-bound dispatch (Q-overloading вариант 4)

Protocol-based dispatch (`fn @m[T Encodable](v T)`) — это **другой
механизм**. Требует:
- Generic-bound enforcement в type-checker (план 08 Ф.6).
- Protocol method-table generation в codegen.

Это **отдельный план 12** (или включить в план 08 расширение).
План 11 — только ad-hoc overload.

---

## План работ

1. **Ф.1** — `method_receivers` переход на `Vec<MethodSig>` (~80 строк Rust).
2. **Ф.2** — `resolve_overload` функция (~100 строк Rust).
3. **Ф.3** — name mangling для overloaded methods (~50 строк).
4. **Ф.4** — method values как first-class (bound/unbound/static)
   (~150 строк codegen, +runtime struct definitions).
5. **Ф.4.5** — `Self.method(...)` и `Self { ... }` в expression
   position (~50 строк codegen).
6. **Ф.5** — type annotation для disambiguation (~30 строк, integrate
   с existing type inference).
7. **Ф.6** — spec D35 раздел + Q-overloading закрыть (~100 строк
   markdown).
8. **Ф.7** — тесты (~150 строк .nv).
9. **Ф.8** — sweep std (отдельный коммит).
10. **Ф.9** — anonymous embed `use _ Type` + override-precedence
    (~120 строк codegen + ~80 строк markdown spec):
    - Ф.9.1: parser `use _ Type`.
    - Ф.9.2: `MethodKind::Delegated` флаг в registry.
    - Ф.9.3: override-precedence в `resolve_overload` (Own > Delegated).
    - Ф.9.4: multi-anonymous detection (declaration-time error).
    - Ф.9.5: lint warning «possible infinite recursion».

---

## Оценка

**~660 строк изменений** (Rust + markdown + тесты).
**1.5-2 дня** компилятор-агента.

Самая сложная часть — **Ф.4 method values** (bound = pointer + self
struct, lifetimes для self в C). Может потребовать GC integration
(self должен outlive bound method value).

---

## Связь с другими планами

- [Plan 04](04-buffer-split-and-external.md) — **главный motivating
  consumer.** План 04 (Buffer split + `external` keyword) **прямо
  зависит** от плана 11 для overload static/instance методов:
  ```nova
  export external fn StringBuilder.from(s str)  -> Self
  export external fn StringBuilder.from(c char) -> Self
  // ↑ нужен план 11 Ф.1-Ф.2 (Vec<MethodSig> + overload resolution)

  export external fn StringBuilder mut @append(s str)  -> ()
  export external fn StringBuilder mut @append(c char) -> ()
  // ↑ то же
  ```
  Без плана 11 — `method_receivers` last-wins, второе объявление
  переписывает первое. Plan 04 также добавляет `external fn` как
  новую форму — взаимодействие с overload-resolution описано в plan 04.
  **Делать план 04 после плана 11.**
- [Plan 06](06-iter-protocol-codegen.md) — `Iter[T]` protocol;
  не зависит, можно делать параллельно. Iterator methods (например
  `m.values()`) — в общем path с method values.
- [Plan 08](08-from-into-conversions.md) — `From`/`Into` 4-way
  auto-derive. Это **специальный случай overload** (по результирующему
  типу через D77 dispatch). План 11 расширяет на любые методы.
  Большая часть Plan 08 закрыта (Ф.1-Ф.5), 4-way auto-derive
  работает.
- [Plan 12 (будущий)](12-protocol-dispatch.md) — protocol-based
  dispatch (Q-overloading вариант 4). После Ф.6 плана 08
  (generic-bound enforcement) + плана 11 (ad-hoc overload).

---

## Что разблокирует

- **Method values как first-class** — необходимо для функционального
  стиля (`nums.map(int.@to_str)`, callback'и, higher-order).
- **Overload по типу аргумента** — `Buffer @write(str/[]byte/char)`
  без `_str`/`_bytes` суффиксов. Чище API.
- **Closure-style usage** методов — `let counter = obj.next` сохраняет
  bound method для повторных вызовов.
- **AI-first method-as-value** — LLM пишет естественный код без
  workaround'ов.

---

## Ссылки

- [spec/syntax.md строки 762-770](../../spec/syntax.md) — описание
  bound/unbound/static method values.
- [spec/decisions/03-syntax.md → D35](../../spec/decisions/03-syntax.md#d35)
  — методы, расширяется в Ф.6.
- [spec/decisions/03-syntax.md → D46](../../spec/decisions/03-syntax.md#d46)
  — operator overloading (specific case, остаётся как есть).
- [spec/open-questions.md → Q-overloading](../../spec/open-questions.md#q-overloading)
  — закрывается этим планом частично (Variant 1).
- `compiler-codegen/src/codegen/emit_c.rs` — `method_receivers`,
  `into_targets`, метод-resolution paths.

---

## Follow-up: Self в return type generic-методов (2026-05-17)

**Bug:** `-> Self` в signature методов **generic** типа не резолвится
в codegen — emit'ит литеральный `Nova_Self` как C type name, что даёт
linker error `unknown type name 'Nova_Self'`.

### Воспроизведение

```nova
export type LinkedList[T]
    | Empty
    | Cons(T, LinkedList[T])

export fn LinkedList[T] @reverse() -> Self { ... }    // ← Self → Nova_Self в C
```

Generated C:
```c
static Nova_Self Nova_LinkedList_method_reverse(...) { ... }   // unknown type
```

### Где работает

- **Static method `.new() -> Self`** на generic type — работает (compiler
  знает Self из enclosing-fn signature).
- **Не-generic types** (Snowflake, BloomFilter) — работает.
- **Lru.new()** (generic, static, return literal `{...}`) — работает,
  потому что Self в return type **statically** заменяется на enclosing
  type instance, и литерал `{...}` инфер'ится.

### Где ломалось (исправлено 2026-05-17)

| Контекст | Pre-fix | Post-fix |
|---|---|---|
| Generic instance method `-> Self` | ❌ `Nova_Self` | ✅ resolves в `Nova_<recv>*` |
| Generic instance method `Self.method()` (call site) | ❌ linker undefined | ✅ через self_method_decls + mono enrollment |
| Generic static method `Self.method()` (call site) | ❌ linker undefined | ✅ same path |
| Generic instance param `other Self` | ❌ `Nova_Self*` | ✅ resolves в `Nova_<recv>*` |
| Generic instance method `-> Self { ... }` (Self literal) | ❌ undefined | ✅ literal `{...}` infers concrete type |
| Generic sum-type instance method `-> Self` (returning variant) | ❌ pre-existing variant emit issue | 🟡 partial — see «Open issues» |

### Fix applied (commits 2026-05-17)

В `compiler-codegen/src/codegen/emit_c.rs`:

1. **`emit_fn_forward_decl`** (line ~3893): `current_receiver_type` set
   на recv.type_name **до param iteration** (не только до ret_c), чтобы
   `Self` в param-position резолвился. Restore — после whole emit.

2. **`emit_method_body` erased generic path** (line ~5037): `current_receiver_type`
   set остаётся через весь body emit (var_types для params, match-infers).
   Restore только в самом конце функции (line ~5165).

3. **`emit_monomorphized_method`** (line ~5945): same — set BEFORE param_c_tys
   compute, restore только в конце (через new `current_receiver_type = None`
   add'нутый после fn_body emit).

4. **`emit_call` для `ExprKind::Path` и `ExprKind::Member`** (lines ~11270, ~9870):
   Self.method(args) fast-path — после substitution Self → recv.type_name,
   эмитим direct `Nova_<safe_recv>_static_<method>(args)` + enroll mono'd
   version через `register_mono_method_instance` (когда recv это mono'd
   `Base____types` имя).

5. **Новая карта `self_method_decls`** (HashMap[(String, String), FnDecl]):
   регистрирует все receiver-generic methods для fast-path lookup'а. Отдельная
   от `mono_method_decls` (которая observable другими code paths и регрессит
   если расширить).

### Workaround в stdlib (применён 2026-05-17, потом revert'нут после fix)

Раньше (до fix'а) все методы generic-типов использовали **explicit type**:
- `HashMap[K, V].new() -> HashMap[K, V] => HashMap[K, V].with_capacity(16)`
- `LinkedList[T] @reverse() -> LinkedList[T]`

После fix'а — `-> Self` восстановлен в LinkedList. HashMap.new() оставлен
с explicit type (история показала что Self.X() ломал mono dispatch ещё до
моего fix'а, и осторожнее оставить пока).

### Open issues

Изначально считал что есть sub-bug «sum-type `-> Self` body resolution» (на
основе CC-FAIL repro_self_a.nv). При повторном analysis оказалось — это
**не Self related**. Минимальный repro без assert-in-match-arm passing:

```nova
export fn Box[T] @swap_empty() -> Self => Empty
let e = b.swap_empty()              // Nova_Box____nova_int* (✓)
let is_empty = match e {            // OK
    Empty   => true
    Full(_) => false
}
assert(is_empty)                    // ОК
```

Падает только если `assert()` стоит **внутри** match arm body:

```nova
match e {
    Empty   => assert(true)         // ❌ codegen emit _nv_match = nova_assert(...)
    Full(_) => assert(false)        //    nova_assert C-func returns void, не nova_unit
}
```

**Это отдельный codegen bug** про assert-in-expression-position (match arm
infers unit type, emit `_nv = nova_assert(...)`, но nova_assert returns void).
Не Self, не trackится в этом plan'е. Может быть отдельным follow-up'ом.

Status: **Self bug полностью закрыт**. Все 5 known scenarios pass.
