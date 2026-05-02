# Types — record, sum-type, protocol, generic, поля

Решения этой группы задают систему типов Nova: четыре формы объявления
данных, структурные контракты-протоколы, семантику передачи параметров
и мутабельность полей, делегацию через `use`. Синтаксические детали
(методы через `@`, generic-применение `[T]`, литералы) — в
[03-syntax.md](03-syntax.md).

| # | Решение | Status |
|---|---|---|
| [D17](#d17-объявление-типов-единый-синтаксис-без-) | Объявление типов: единый синтаксис без `\|` | active |
| [D42](#d42-protocol-keyword-для-структурных-интерфейсов) | `protocol` keyword для структурных интерфейсов | active |
| [D15](#d15-структурные-интерфейсы) | Структурные интерфейсы | revised → D42 |
| [D39](#d39-embed-и-delegation-use-type-и-use-name-type) | Embed и delegation: `use Type` и `use name Type` | active |
| [D32](#d32-семантика-передачи-параметров) | Семантика передачи параметров | revised для полей → D36 |
| [D36](#d36-поля-типа-дефолт-mutable-у-mut-bindinga-readonly-для-never-mut) | Поля типа: дефолт mutable у mut-binding'а, `readonly` для never-mut | active |

---

## D17. Объявление типов: единый синтаксис без `|`

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
([04-effects.md → D18](04-effects.md#d18-эффекты-объявляются-через-protocol-не-type)).

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

## D42. `protocol` keyword для структурных интерфейсов

### Что
Структурные интерфейсы объявляются отдельным keyword `protocol`. `type`
— для **данных** (record, sum-type, alias), `protocol` — для
**поведения** (набор методов как контракт). Любой тип со структурно
совпадающими сигнатурами автоматически удовлетворяет protocol'у — без
явных `impl`-блоков.

**Эффекты — это тоже `protocol`**, использованный в позиции эффекта
(между `)` и `->`). Один и тот же `protocol` может играть роль эффекта
или роль структурного контракта-параметра — различение по контексту
использования ([04-effects.md → D18](04-effects.md#d18-эффекты-объявляются-через-protocol-не-type)).
`type` без полей с одними методами не допускается — нужен `protocol`.

### Правило

```nova
protocol Hashable {
    hash() -> u64
    eq(other Self) -> bool
}

protocol Iterator[T] {
    next() -> Option[T]
}

type Login {                    // record (данные) — keyword type
    username str
    password str
}
```

`Self` валиден только внутри `protocol`-блока.

Структурная совместимость — автоматическая. Метод определяется у типа
через `@`-синтаксис ([03-syntax.md → D35](03-syntax.md#d35)) и без
дополнительных деклараций удовлетворяет protocol'у:

```nova
type User { id u64, name str }

protocol Printable {
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
protocol Printable {
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

## D39. Embed и delegation: `use Type` и `use name Type`

### Что
Композиция типов через `use Type` внутри record-декларации. Имя
встроенного по умолчанию — имя типа (`use Account` →
доступ через `@Account`). Для алиаса — `use name Type` (как обычное
объявление поля: «имя тип»). Это **delegation**, не наследование:
обёртка не является подтипом встроенного.

### Правило

```nova
type AuditedAccount {
    use Account                     // имя = "Account"
    audit_log []AuditEntry
}

let acc AuditedAccount = ...

// Auto-proxy: прямой доступ к полям и методам Account
println(acc.balance)                // = acc.Account.balance
println(acc.owner)                  // = acc.Account.owner
acc.is_solvent()                    // = acc.Account.is_solvent()

// Доступ к встроенному объекту целиком
let just_account = acc.Account
```

**Auto-generated прокси-методы.** При `use Type` компилятор генерирует
прокси для каждого метода `Type`:

```nova
type Account { balance money }
fn Account @balance_pct(of money) -> f64 => @balance / of * 100.0

type AuditedAccount { use Account, audit_log []AuditEntry }

// Компилятор генерирует:
// fn AuditedAccount @balance_pct(of money) -> f64 =>
//     @Account.balance_pct(of)

let aa AuditedAccount = ...
aa.balance_pct(1000.0)              // через auto-proxy
```

Zero-cost — компилятор инлайнит вызов, никакой vtable.

**Алиас для имени.** Если имя типа неудобно или нужно несколько
встроенных одного типа — `use name Type`:

```nova
type Wrapper {
    use w HashMapIter[K, V]         // имя = "w"
    extra int
}

fn Wrapper mut @next() -> Option[K] => match @w.next() {
    Some((k, _)) => Some(k)
    None         => None
}
```

Грамматика согласована со всем языком: везде «имя тип» (параметры,
поля record, let-bindings, for-loop, embed).

**Override метода.** Если тип-обёртка определяет метод с тем же именем
— он затмевает делегированный:

```nova
type AuditedAccount {
    use Account
    audit_log []AuditEntry
}

fn AuditedAccount mut @deposit(amount money) {
    @Account.deposit(amount)        // явный вызов «родителя»
    @audit_log.push(AuditEntry.deposit(amount))
}

let mut acc AuditedAccount = ...
acc.deposit(100)                    // вызовет AuditedAccount.deposit
```

Без `@Account.` в теле — бесконечная рекурсия. Программист обязан
явно обращаться к встроенному.

**Конфликт имён — обязательный alias.** Если два `use` вводят
одинаковые имена методов и обёртка их не переопределяет —
compile error:

```nova
protocol Logger { log(msg str) -> () }
type Auditor { log(msg str) -> () }

type Combined {
    use Logger
    use Auditor
}

let c = Combined { ... }
c.log("...")                        // ОШИБКА: ambiguous
```

Решение — alias или явный вызов:

```nova
type Combined {
    use console Logger
    use audit Auditor
}

fn Combined @log_all(msg str) {
    @console.log(msg)
    @audit.log(msg)
}

// Или явный вызов через имя типа
let c = Combined { ... }
c.Logger.log("...")
c.Auditor.log("...")
```

### Что это НЕ

**Не наследование.** `AuditedAccount` не является `Account`:

```nova
fn process(a Account) -> () => ...

let aa AuditedAccount = ...
process(aa)                         // ОШИБКА
process(aa.Account)                 // ок: извлекли Account-часть
```

Если нужен полиморфизм — структурный protocol:

```nova
protocol HasBalance {
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
2. **Прецедент Go** — Go embed работает в продакшне годами; Nova
   расширяет его alias'ом и обязательным разрешением конфликтов.
3. **Согласованность с языком** — `use name Type` использует тот же
   порядок «имя тип», что параметры, поля, let-bindings.

### Что отвергнуто

- **`use Type as name`** (Rust import-style). `as` зафиксировано для
  импортов ([07-modules.md → D29](07-modules.md#d29)) — там «alias
  имени извне». В embed — «объявление поля». Разные случаи; единый
  порядок «имя тип» лучше.
- **Только `use Type` без alias** (чистый Go embed) — не покрывает
  конфликт имён без переименования встроенного типа.
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
- [03-syntax.md → D35](03-syntax.md#d35) — `@Type.method()` или
  `@alias.method()` для явного вызова из метода обёртки.
- [03-syntax.md → D38](03-syntax.md#d38) — generic-применение в
  embed: `use HashMapIter[K, V]`.

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
- [04-effects.md → D2](04-effects.md#d2) — мутация через эффект
  `Mut`; `mut` параметра + `Mut` в сигнатуре — два уровня контроля.
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

fn LazyConfig @get() -> str => {
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
