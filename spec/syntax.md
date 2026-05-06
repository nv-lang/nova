# Nova — синтаксис

## Минимальные примеры

```nova
// Hello world — никаких main, package, import для stdlib
print("hello")

// Чистая функция: нет эффектов, нет ошибок, детерминирована
fn double(x int) -> int => x * 2
```

## Tagged template literals — `tag\`...\``

Литерал с префиксом-тегом обрабатывается функцией `tag`. Возвращает
тип, который выбирает функция (не обязательно `str`):

```nova
let j = json`{"name": "alice"}`              // -> Json
let q = sql`SELECT * FROM users WHERE id = ${user_id}`   // -> Sql, безопасно
let r = regex`\d+\.\d+`                       // -> Regex, raw
let b = bytes`deadbeef`                       // -> Bytes
```

**Интерполяция через `${expr}`** — tag-функция получает части и
аргументы **раздельно**, что обеспечивает безопасность (защита от SQL
injection):

```nova
sql`SELECT * FROM users WHERE name = ${name}`
// → sql(["SELECT * FROM users WHERE name = ", ""], [name])
// функция передаёт name как параметр, не склеивает в строку
```

**Multiline** работает естественно. Escape: `` \` ``, `\\`, `\${` —
буквальные. Остальные символы — raw (удобно для regex и SQL).

**Стандартные теги в stdlib:** `json`, `sql`, `regex`, `bytes`.

**Свой тег** — обычная функция:

```nova
export fn url(parts []str, args []str) -> Url => ...

let u = url`https://api.example.com/users/${user_id}`
```

Подробно — [D48](decisions/03-syntax.md#d48).

## Statement separator: newline или `;`

**Перенос строки** разделяет statement'ы. **`;` опционален** —
нужен только для нескольких statement'ов на одной строке:

```nova
let x = 1                        // newline разделяет
let y = 2
foo(x, y)

let a = 1; let b = 2; foo(a, b)  // ; для одной строки
```

Newline **игнорируется** в позициях, где statement продолжается:

```nova
// 1. После висящего бинарного оператора
let total = a +
            b +
            c

// 2. Внутри открытых () [] {}
let user = User {
    name: "alice",
    age: 30,
}

// 3. Перед .method() (chain)
let result = list
    .filter() { x => x > 0 }
    .sum()

// 4. Перед ? (error propagation)
let user = find_user(id)
    ?
```

**Бинарные операторы — в конец строки** (Go-стиль), не в начало:

```nova
let total = a +              ✅
            b
let total = a
          + b                 ❌ парсится как унарный +b
```

Подробно — [D49](decisions/03-syntax.md#d49).

## Числовые литералы

```nova
// Целые
1
1_000_000_000              // разделитель `_` между цифрами
0xFF_FF_FF_FF              // hex (любой регистр)
0b1010_0001                // binary
0o755                      // octal

// Float
1.5
1_234.567_89
1e10                       // научная нотация
1.5e-3
```

**Default-типы** без контекста: `int` для целых, `f64` для float. С
аннотацией/контекстом — берётся тип контекста:

```nova
let x u8 = 200             // 200 это u8
let arr []f32 = [1.0, 2.0]
```

**Type-suffixes (`100u32`, `1.5f32`) не вводятся.** Для редких случаев
дисамбигуации — `as`-cast: `100 as u32`, `0xFF as u8`.

**Разделитель `_` разрешён только между цифрами**, не подряд, не в
начале/конце, не сразу после префикса (`0x_FF` ❌), не вокруг точки
или `e`. Подробно — [D44](decisions/03-syntax.md#d44).

## Аннотации типа — без двоеточия

В позициях, где компилятор однозначно знает «дальше идёт тип»,
двоеточие опускается:

```nova
fn save(u User, amount money) Fail Db -> ()    // параметры
let users []User = []                            // let
type User { id u64, name str }                   // поля типа
for id u64 in ids { ... }                        // for-loop
```

`:` остаётся там, где это **разделитель ключ-значение**:

```nova
let alice = User { id: 1, name: "alice" }       // record-литерал
let cfg = { "host": "localhost", "port": 8080 } // dict-литерал
```

## Возврат: `->` обязателен, `()` опционален

```nova
fn compute(x int) -> int => x * 2    // явный тип возврата
fn log_event(e Event) Log            // -> () можно опускать
fn save(u User) Fail Db            // эффекты + dropped -> ()
```

## Trailing-block — блок-аргумент за скобками вызова

Если последний параметр функции — `fn(...) -> T`, блок-аргумент можно
вынести за `(...)`. Это **не лямбда** (лямбда строго `(params) => expr`,
без блок-формы; см. [D22](decisions/03-syntax.md#d22)) — это
**trailing-block** ([D43](decisions/03-syntax.md#d43)). **`()` обязательны**
даже без других аргументов:

```nova
with_timeout(2.seconds) {
    Db.exec(sql`UPDATE counters SET v = v + 1`)
}

list.filter() { x => x > 0 }
list.fold(0) { (acc, x) => acc + x }
```

**Правила:**
- `{` на той же строке, что `)`. Перенос между ними запрещён.
- Параметры через `=>`: `{ x => stmts; expr }`, `{ (a, b) => stmts; expr }`,
  `{ stmts; expr }` (без параметров).
- Тело — block-body: множество statement'ов плюс опциональное финальное
  выражение, как у блок-формы `fn`.
- Тип последнего параметра должен быть функциональным.
- Один trailing на вызов.

`spawn` — keyword-конструкция, не функция, поэтому не подчиняется правилу D43.
Его синтаксис описан отдельно ниже.

Короткие лямбды (одно выражение) — в скобках, через `=> expr`:
```nova
list.filter((x) => x > 0)            // короткая inline-лямбда
m.get_or_insert("k", () => 0)        // короткая inline-лямбда
```

Когда нужен блок с `let`'ами и несколькими statement'ами — выносить
через trailing-block, не лямбду (лямбда строго `=> expr`,
[D22](decisions/03-syntax.md#d22)). Подробно — [D43](decisions/03-syntax.md#d43).

## Тело функции: `=>` для выражения, `{}` для блока

Два **взаимоисключающих** способа:

```nova
// expression-body — ровно одно выражение
fn double(x int) => x * 2                    // -> int выведен (D45)
fn classify(n int) -> str => match n {       // -> str для ясности
    0 => "zero",
    n if n > 0 => "positive",
    _ => "negative",
}

// block-body — несколько шагов; последнее выражение = значение блока
fn next_pow2(n int) -> int {                 // -> int обязателен
    if n <= 1 { return 1 }
    let mut p = 1
    while p < n { p *= 2 }
    p
}
```

**В expression-body `-> T` опционален** — тип выводится из тела
([D45](decisions/03-syntax.md#d45)). В block-body `-> T` обязателен (если не unit).

**Indentation не значим.** `fn f() => stmt1; stmt2` или multiline без
`{}` — ошибка. Если шагов больше одного — `{}` обязательны.

Style: для `export`-функций (public API) рекомендуется писать `-> T`
явно — это документация и стабильность. Для приватных и tiny helpers
можно опускать.

Подробно — [D40](decisions/03-syntax.md#d40), [D45](decisions/03-syntax.md#d45).

## Перегрузка операторов

Стандартные операторы автоматически вызывают методы с фиксированными
именами:

```nova
fn Duration @plus(other Duration) => Duration { nanos: @nanos + other.nanos }
fn Duration @times(n i64) => Duration { nanos: @nanos * n }

let total = 1.hour() + 30.minutes()       // вызывает @plus
let triple = 5.seconds() * 3              // вызывает @times
if elapsed > 1.second() { ... }           // вызывает @gt
```

| Оператор | Метод | | Оператор | Метод |
|---|---|---|---|---|
| `+` | `@plus(o)` | | `==` | `@eq(o) -> bool` |
| `-` (binary) | `@minus(o)` | | `<` | `@lt(o) -> bool` |
| `-` (unary) | `@neg()` | | `<=` | `@le(o) -> bool` |
| `*` | `@times(o)` | | `>` | `@gt(o) -> bool` |
| `/` | `@div(o)` | | `>=` | `@ge(o) -> bool` |
| `%` | `@rem(o)` | | `!` | `@not()` |
| `\|` | `@or(o)` | | `<<` | `@shl(n)` |
| `&` | `@and(o)` | | `>>` | `@shr(n)` |
| `^` | `@xor(o)` | | | |
| `a[i]` | `@get(i)` | | `a[i]=v` | `@set(i, v)` |

`!=` выводится из `@eq`. `&&`/`||` **не перегружаются** (short-circuit
семантика). Custom-операторы (`:+`, `<>`) не разрешены. Подробно —
[D46](decisions/03-syntax.md#d46).

## Конвенции именования

| Что | Стиль | Пример |
|---|---|---|
| Типы, эффекты, протоколы, варианты sum | **PascalCase** | `User`, `HashMap`, `Db`, `Hashable`, `Some` |
| Generic-параметры | **PascalCase, односимвольные** | `T`, `K`, `V`, `E` |
| Функции, методы (`@name`), параметры, поля | **snake_case** | `parse_url`, `@deposit`, `user_id`, `created_at` |
| Константы (`const`) | **SCREAMING_SNAKE_CASE** | `MAX_PAYLOAD`, `DEFAULT_TIMEOUT` |
| Модули | **snake_case** через точки | `module admin.audit`, `module std.duration` |

**Акронимы — PascalCase, не UPPERCASE.** `Db`, не `DB`. `Http`, не `HTTP`.
`Json`, не `JSON`. `Url`, не `URL`. Правило: акроним = обычное слово.

**Зарезервированные имена методов** (operator overloading, [D46](decisions/03-syntax.md#d46)):
`@plus`, `@minus`, `@times`, `@div`, `@rem`, `@neg`, `@or`, `@and`,
`@xor`, `@shl`, `@shr`, `@eq`, `@lt`, `@le`, `@gt`, `@ge`, `@not`,
`@get`, `@set`. Не использовать для других целей.

**Договорные конвенции:**
- `T.new(...)` — стандартный конструктор; `T.from_X(...)` — из значения.
- `@to_str()` — конверсия в строку через `ToStr` ([D70](decisions/08-runtime.md#d70)),
  `@hash()` — хеш, `@clone()` — копия, `@iter()`/`@next()` — iterator.
- `@is_X()` — bool-предикат; `@as_X()` — дешёвая конверсия; `@to_X()` —
  возможно дорогая.
- `_prefix` — **только для полей** (используй методы вместо прямого
  доступа). Для функций/методов **не используется**.
- Test-имена — строки естественного языка: `test "insert and get"`,
  не `"test_insert_and_get"`.

### Зарезервированные identifier'ы

Помимо grammar-keyword'ов, Nova имеет identifier'ы со специальной
семантикой, известной компилятору. Их можно переопределить локально,
но это анти-паттерн (линтер предупреждает).

**Special types:**
- `Self` — referential type, refers к receiver-типу метода или типу,
  удовлетворяющему protocol'у ([D66](decisions/02-types.md#d66)).
  Валиден в любом type-контексте.
- `any` — top-type для runtime type-check ([D54](decisions/03-syntax.md#d54)).
- `Never` — bottom-type для не-возвращающих функций.

**Prelude types:**
- `Option[T]`, `Some(v)`, `None` — sum-тип
- `Result[T, E]`, `Ok(v)`, `Err(e)` — sum-тип
- `Error` — record `{ msg str }` для `throw err`
- `RuntimeError` — sum bottom-уровневых runtime-ошибок
- `Handler[E]` — first-class тип handler'а эффекта
- `ToStr` — protocol с методом `@to_str() -> str` ([D70](decisions/08-runtime.md#d70))

**Стандартные эффекты:**
- `Fail[E]`, `Fail` — failable-эффект
- `Io`, `Net`, `Db`, `Fs`, `Time`, `Random`, `Log`, `Trace` — основные
- `Ask[T]` — Reader-style контекст
- `Alloc[R]` — аллокация в region
- `Detach`, `Blocking` — ([D50](decisions/06-concurrency.md#d50))

**Примитивные типы (lowercase, исключение из PascalCase-правила):**
- `int`, `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`
- `f32`, `f64`
- `str`, `bool`, `byte`

Подробно — [D30](decisions/03-syntax.md#d30), [D46](decisions/03-syntax.md#d46), [D47](decisions/07-modules.md#d47).

## Видимость: `export` для публичных деклараций

`export` перед декларацией = публичная (видна снаружи модуля).
Без `export` = приватная (видна только внутри модуля).

Применяется единообразно к **типам**, **функциям**, **методам**,
**константам** и **протоколам**:

```nova
module account

export type Account {                    // публичный тип
    readonly owner str
    balance money
    _internal_id u64                     // convention: `_` = приватное-по-договору
}

type InternalState { ... }               // приватный тип

export const ACCOUNT_MIN_BALANCE money = 0
const _INTERNAL_TIMEOUT_MS int = 5_000

export fn Account.new(owner str) -> Account => ...      // публичный конструктор
export fn Account @balance() => @balance                // публичный метод
fn Account @validate(amount money) => amount > 0       // приватный helper

export type Hashable protocol {
    hash() -> u64
    eq(other Self) -> bool
}
```

**Поля record:** в MVP все поля `export`-типа публичны. Convention
`_prefix` для приватных-по-договору, не enforced компилятором —
обычно инкапсуляция делается через **методы** (геттеры/сеттеры).

Подробно — [D47](decisions/07-modules.md#d47), [D29](decisions/07-modules.md#d29) (модули).

## Объявление типов

| После `type Имя` идёт | Что это |
|---|---|
| `\|` | sum-type |
| `(` | tuple-структура |
| `{` | record-структура |
| `alias` | alias |
| идентификатор/тип | newtype |
| ничего | unit-тип |

```nova
// newtype — type X Y, новый тип, типизированно отличный от Y
type UserId u64
type Email str

// alias — type X alias Y, для длинных дженериков
type StringMap[V] alias HashMap[str, V]

// record (форма сразу после имени, без `=`)
type User { id u64, name str }

// позиционная структура
type Point(f64, f64)

// unit-тип
type Marker

// sum-type — варианты через leading |
type Color | Red | Green | Blue

type Shape
    | Circle { radius f64 }
    | Square { side f64 }
    | Triangle { a f64, b f64, c f64 }

type Result[T, E] | Ok(T) | Err(E)
type Option[T] | Some(T) | None
```

Sum-варианты могут иметь числовые discriminants с auto-increment:

```nova
type ExitStatus | Ok | Failure | Critical              // 0, 1, 2 (auto)
type ErrorCode
    | NotFound       = 404
    | Unauthorized   = 401
    | InternalError  = 500
type Bit u8 | Off = 0 | On = 1                         // явный базовый тип
```

Подробно — [decisions/02-types.md → D52](decisions/02-types.md#d52).

### Варианты sum-type — те же три формы, что top-level type

Каждый вариант sum-type объявляется по тем же правилам, что top-level
объявление:

| После имени варианта | Что это | Пример |
|---|---|---|
| `( ... )` | позиционный вариант | `Some(T)`, `Ok(T)`, `Point(f64, f64)` |
| `{ ... }` | record-вариант | `Circle { radius f64 }` |
| ничего | unit-вариант | `None`, `Red`, `Origin` |

```nova
type Option[T]
    | Some(T)                 // позиционный — несёт значение T
    | None                    // unit — без полей, само по себе значение

type Shape
    | Circle { radius f64 }   // record-вариант
    | Point(f64, f64)         // позиционный
    | Origin                  // unit
```

`None` — это значение типа `Option[T]`, **не функция и не конструктор**.
Используется без скобок:

```nova
let x = Some(42)              // позиционный — нужен аргумент
let y = None                  // unit — без скобок
```

Подробно — [D17](decisions/02-types.md#d17).

## Создание значений и pattern matching

```nova
let p = Point(1.0, 2.0)
let u = User { id: 1, name: "alice" }
let c = Circle { radius: 5.0 }
let s = Active

// доступ к полям (D37)
println(u.name)              // record — по имени
println(p.0, p.1)            // позиционная — по индексу
let pair = (1, "alice")
println(pair.0, pair.1)      // кортеж — то же

// создание массивов (D38)
let xs []int = []                          // пустой, тип из annotation
let ys = []int.new()                       // через static-метод
let buf = []u8.with_capacity(1024)         // с pre-allocation
let zeros = []u8.filled(0, 16)             // заполненный

// turbofish для дженериков (D38)
let n = parse[int]("42")?                  // явный T = int
let m = HashMap[str, int].new()            // явные K, V

match shape {
    Circle { radius }    => 3.14159 * radius * radius
    Square { side }      => side * side
    Triangle { a, b, c } => heron(a, b, c)
}

match result {
    Ok(value)  => value
    Err(error) => default
}
```

## Pattern matching

```nova
fn classify(x) => match x {
    0          => "zero"
    1..=9      => "digit"
    n if n < 0 => "negative"
    _          => "big"
}
```

Каждая arm имеет форму `pattern => result`, опционально с **guard'ом**
`pattern if condition => result`. Компилятор пробует arm'ы сверху вниз,
берёт первую, где паттерн совпал И guard истинный.

**Виды паттернов:**

| Форма | Пример | Что делает |
|---|---|---|
| Литерал | `0`, `"hello"`, `true` | сравнение по значению |
| Range | `1..=9`, `0..100` | попадание в диапазон |
| Имя (binding) | `n`, `x` | ловит любое значение, привязывает к имени |
| Wildcard | `_` | ловит любое значение, не привязывает |
| Конструктор | `Some(v)`, `Ok(value)`, `None` | разбор варианта sum-type |
| Record | `User { id, name }` | разбор record-полей |
| Tuple | `(a, b)`, `(_, value)` | разбор кортежа |
| Guard | `n if n < 0` | паттерн + дополнительное условие |

**Exhaustiveness check.** Компилятор проверяет, что match покрывает
все возможные случаи. Если нет — ошибка с указанием непокрытого
варианта. Это работает для sum-type, range'ей, bool. Для общих типов
(`int`, `str`) нужен либо `_`-wildcard, либо явная проверка всех
рассматриваемых значений.

```nova
type Color | Red | Green | Blue

fn name(c Color) -> str => match c {
    Red   => "red"
    Green => "green"
    // ОШИБКА: missing variant `Blue`
}
```

`match` — это **выражение**, возвращает значение. Все ветви должны
иметь совместимый тип (или общий supertype, либо обёрнутые в sum-type).

### Record-литералы и patterns

**Shorthand** — когда имя поля совпадает с именем переменной в scope:

```nova
let key = "alice"
let value = 42

let entry = Entry { key, value }                 // shorthand обязателен (D52)
let entry = Entry { key, value, extra: "data" }  // можно смешивать
// `Entry { key: key }` — ОШИБКА: используйте shorthand `{ key }`.
```

**Partial pattern matching** — указывать только нужные поля:

```nova
match @buckets[idx] {
    Occupied { value }     => Some(value)        // partial: key игнорируется
    Occupied { value, .. } => Some(value)        // явный .. — то же самое
    _                      => None
}
```

Обе формы валидны (`..` или без) — выбор по контексту. `..` —
сигнал «у типа есть ещё поля». Без — короче.

**Переименование при деструктуризации:**

```nova
Occupied { key: k, value }      // key переименовано в k, value совпадает
```

Подробно — [D17](decisions/02-types.md#d17).

### Циклы `for` / `while` / `loop`

```nova
for x in list { ... }            // x — immutable binding на каждой итерации
for mut x in list { ... }         // x можно мутировать в теле
for x int in nums { ... }         // явный тип элемента
for (i, x) in list.enumerate() { ... }   // индекс через метод

while cond { ... }                // условный цикл
loop { ... }                      // бесконечный, выход через break/return
```

Переменная в `for x in iter` — **immutable binding** (как `let` без
`mut`), на каждой итерации получает **новое значение**. В теле блока
переприсвоить нельзя:

```nova
for x in list {
    x = 5                         // ОШИБКА: x immutable
}

for mut x in list {
    x = transform(x)              // ок
}
```

Это согласовано с правилом D32/D33 — все binding'и иммутабельны по
умолчанию, мутация явно через `mut`. Никакого `const` или `final`
маркера в Nova нет — иммутабельность и так дефолт.

`break` / `continue` — стандартные. `break value` выходит из `loop`
со значением (loop — выражение).

### `if let` и `while let`

Паттерн-матч прямо в условии — короткая альтернатива `match` для
одного варианта:

```nova
// если в кеше есть — вернуть
if let Some(data) = cache.get(key) {
    return data
}

// извлечение из Result
if let Ok(user) = Db.find(id) {
    process(user)
} else {
    Log.warn("user not found")
}

// while let — итерация пока паттерн совпадает
while let Some(line) = reader.read_line()? {
    process(line)
}

// несколько условий через запятую
if let Some(user) = lookup(id), user.is_active {
    process(user)
}
```

Локальные binding'и (`data`, `user`, `line`) доступны **только в теле
блока**. После закрывающей `}` — недоступны.

Подробно — [D34](decisions/03-syntax.md#d34).

## Методы инстанса и static-функции

В Nova — **два вида функций ассоциированных с типом**, различимых по
синтаксису декларации:

```nova
// конструктор / static — через точку, без @
fn Account.new(owner str) -> Account =>
    Account { _balance: 0, owner }

// метод инстанса — через пробел и @, неявный self
fn Account @balance() -> money => @_balance

fn Account @is_solvent() -> bool => @_balance > 0

// мутирующий метод — mut перед @name
fn Account mut @deposit(amount money) {
    @_balance += amount
}
```

**Использование:**

```nova
let acc = Account.new("alice")    // вызов constructor через точку
acc.deposit(100)                   // вызов метода — точка + скобки
let bal = acc.balance()            // getter, обязательные скобки
```

### `@field` для доступа к полям

Внутри метода (`@method` или `mut @method`) поля self доступны через
**`@field`** — единственная форма:

```nova
fn Account @summary() -> str =>
    "${@owner}: ${@_balance}"      // = self.owner, self._balance
```

`@.field` **невалидно** — точка не используется. `@field` — единственно
верно.

`@` без поля — это **значение текущего инстанса**:

```nova
fn Account @copy() -> Account => @
fn Account @send_to(ch Channel[Account]) => ch.send(@)
```

### Скобки обязательны для вызова

```nova
acc.balance()              // вызов метода
acc.balance                // bound method value (не вызов!), тип: fn() -> money
Account.@balance           // unbound method value, тип: fn(Account) -> money
Account.new                // static-функция как значение, тип: fn(str) -> Account
```

Программист и LLM мгновенно различают: вызов = со скобками, значение
= без скобок. Никаких property с побочками.

### Generic'и

```nova
fn HashMap[K, V].new() -> HashMap[K, V] => ...        // generic на типе
fn HashMap[K, V] @get(key K) -> Option[V] => ...      // тоже
fn []T @map[U](f fn(T) -> U) -> []U => ...            // generic на методе [U]
```

Подробно — [D35](decisions/03-syntax.md#d35).

## Embed и delegation: `use Type` и `use name Type`

Композиция вместо наследования. `use` — это **поле + автопрокси методов**:

```nova
type Account {
    owner str
    balance money
}

fn Account mut @deposit(amount money) => @balance += amount

// embed: имя поля обязательно (D39 — alias всегда явный)
type AuditedAccount {
    use account Account
    audit_log []AuditEntry
}

fn AuditedAccount mut @withdraw(amount money) Fail[AuditError] {
    @account.deposit(-amount)               // явный вызов "родителя" через имя поля
    @audit_log.push(AuditEntry.new(amount))
}

let aa = AuditedAccount { ... }
aa.deposit(100)                              // авто-прокси: account.deposit
aa.balance                                   // авто-прокси: account.balance
```

Имя поля **обязательно** при `use` ([D39](decisions/02-types.md#d39))
— согласовано с [D30](decisions/03-syntax.md#d30) (поля snake_case):

```nova
type Wrapper[K, V] {
    use w HashMapIter[K, V]      // имя поля = "w"
    extra int
}

fn Wrapper[K, V] @next() -> Option[Pair[K, V]] => @w.next()

// конфликт двух embed — псевдонимы обязательны
type Composite {
    use a TimerA
    use b TimerB                  // оба определяют tick() — нужны имена
}
```

**Override.** Метод того же имени на внешнем типе перекрывает прокси.
Доступ к «родительскому» — через имя поля:

```nova
fn AuditedAccount mut @deposit(amount money) {
    @account.deposit(amount)                // вызов оригинала через имя поля
    @audit_log.push(AuditEntry.new(amount))
}
```

**`use` — это не наследование.** `AuditedAccount` не подтип `Account`.
Функции `fn(Account)` принимают `Account`, не `AuditedAccount`. Структурные
интерфейсы — отдельный механизм (см. ниже).

Подробно — [D39](decisions/02-types.md#d39).

## Передача параметров

Объекты (record, sum-type, массивы) передаются **по ссылке** в managed
heap. Примитивы (`int`, `bool`, `f64`, ...) — **по значению**.
Префикс `mut` разрешает мутацию.

```nova
type Account { balance money }    // обычное поле — мутируется у mut binding'а

// без mut — иммутабельный view, мутация запрещена
fn show(acc Account) Io => println("${acc.balance}")

// с mut — мутации видны вызывающему
fn deposit(mut acc Account, amount money) {
    acc.balance += amount
}

let mut my_acc = Account { balance: 100 }
deposit(my_acc, 50)
// my_acc.balance == 150  ← мутация видна

show(my_acc)
// показывает 150, my_acc не изменён
```

### Поля типа: `let` для never-mut, `mut` для cache

```nova
type Account {
    readonly id u64                // никогда не меняется (D36)
    readonly owner str             // тоже
    balance money                  // мутируется у mut-binding
    closed bool                    // тоже
    mut last_cached_total money    // мутируется ВСЕГДА (для cache/lazy)
}

// group-syntax — несколько полей одного типа через запятую
type Point { x, y, z f64 }
type Color { r, g, b u8 }
```

Подробно про правила мутации полей — [D36](decisions/02-types.md#d36).

| Форма | Передача | Мутация снаружи |
|---|---|---|
| `x int` | by value | нет |
| `o Order` | managed reference | нет (immutable) |
| `mut o Order` | managed reference | да |

Для perf-критичного кода компилятор использует **escape analysis**:
не утекающие значения остаются на стеке, без аллокаций в managed
heap. Программист не пишет ничего особого. Для real-time — блок
`realtime nogc { }` ([D64](decisions/04-effects.md#d64)), внутри
`region { }` для arena-allocations ([D6](decisions/05-memory.md#d6)).

Подробно — [D32](decisions/02-types.md#d32).

## Эффекты в сигнатуре

Любое нечистое действие — эффект, объявляется между `)` и `->`:

```nova
fn double(x int) -> int                          // чистая
fn parse(s str) Fail -> int                    // может бросить
fn save(u User) Fail Db Log -> ()              // три эффекта
fn fetch(url str) Net Fail -> Response   // сеть + async + ошибки
```

`?` — пробрасывание ошибки, работает в функциях с `Fail`:

```nova
fn pipeline(s str) Fail -> int {
    let n = parse(s)?
    let doubled = n * 2
    validate(doubled)?
    doubled
}
```

Подробнее — [effects.md](effects.md), [revolutionary.md](revolutionary.md).

## Контракты (опциональны)

```nova
fn withdraw(mut acc Account, amount money) Fail -> ()
    requires amount > 0
    requires acc.balance >= amount
    ensures acc.balance == old(acc.balance) - amount
=>
    acc.balance -= amount
```

Без контрактов код работает как обычно. С ними компилятор пытается
доказать статически, что не может — превращает в runtime-проверку
в debug-режиме.

## Handler'ы — литералы у `protocol`-эффектов

```nova
type Logger effect {
    log(msg str) -> ()
}

fn process(x int) Logger -> int {
    Logger.log("processing ${x}")
    x * 2
}

// handler — обычное значение через keyword `handler` (D61)
let console = handler Logger {
    log(msg) => println("[LOG] ${msg}")
}

// применение через with
fn main() Io -> () {
    with Logger = console {
        process(42)
    }
}
```

`return value` или финальное выражение в handler-method'е продолжает
вычисление с возвращённым значением. Для досрочного выхода из всего
with-блока — `interrupt v` (D61). `resume` в Nova не существует.

## Имя эффекта в коде — три позиции

```nova
fn process() Db -> ()                // 1. позиция типа
Db.query(sql`...`)                   // 2. операция активного handler'а
let captured = Db                    // 3. сам активный handler как значение
```

Парсер различает по позиции.

## With-блок — несколько подмен в одном

```nova
test "complex flow" {
    with Logger = collect_into(buf),
         Db = in_memory,
         Time = fixed(t0) {
        process_order(o)?
    }
    assert buf.contains("processed")
}
```

После `with` — список «эффект = handler-выражение» через запятую,
потом **один** блок тела.

## Параллелизм — без `async/await`

```nova
fn fetch_all(ids []u64) Net Fail -> []User =>
    parallel for id in ids {
        fetch_user(id)
    }
```

Suspension в Nova — ambient runtime-инфраструктура, не эффект и не
специальная конструкция (D62). Тип возврата `[]User`, не
`Future<[]User>`. Подробно — [revolutionary.md R7](revolutionary.md).

`parallel for` — structured concurrency: ждёт всех, отменяет хвост
при ошибке.

## Capability-режим

```nova
fn run_user_script(code str) Fail -> Result =>
    forbid Net, Fs, Db {
        eval(code)
    }
```

Внутри `forbid` компилятор не пропустит вызов функции с запрещёнными
эффектами. Sandbox в типах, не в рантайме.

## Производительность — escape analysis и regions

Программист пишет обычный код:

```nova
fn hot_loop(data []f64) -> f64 =>
    data.iter().sum()  // SIMD-авто, zero-alloc через escape analysis
```

Компилятор сам решает: примитивы — в регистрах, не утекающие
объекты — на стеке, остальное — в managed heap. Никаких ссылок
вручную.

Для real-time hot path — блок `realtime nogc { body }`
([D64](decisions/04-effects.md#d64)). Внутри блока запрещены
suspend-операции и аллокации в managed heap; `region { ... }`
используется для arena-allocations ([D6](decisions/05-memory.md#d6)).

## Структурные «интерфейсы» — `protocol`

Никаких `interface`/`trait`. Структурный контракт — отдельным keyword
**`protocol`**:

```nova
// именованный
type Printable protocol {
    show() -> str
}

fn log_one(x Printable) Log -> () => Log.info(x.show())

// или прямо в сигнатуре, без имени — анонимный структурный тип
fn log_one(x { show() -> str }) Log -> () => Log.info(x.show())
```

Совместимость **автоматическая** по структуре — любой тип с
подходящими методами автоматически удовлетворяет protocol'у, никаких
`impl`-блоков не нужно. `Self` валиден внутри любого type-контекста
(protocol-блок, effect-блок, instance-метод, static-метод, sum-вариант)
по [D66](decisions/02-types.md#d66):

```nova
type Hashable protocol {
    hash() -> u64
    eq(other Self) -> bool
}

type Iterator[T] protocol {
    next() -> Option[T]
}
```

`type` — для **данных** (record, sum-type, alias). `protocol` — для
**поведения** (методы как контракт). Подробно — [D42](decisions/02-types.md#d42),
[D9](decisions/01-philosophy.md#d9) / [D15](decisions/02-types.md#d15).

## Дженерики

```nova
fn map[T, U](xs []T, f T -> U) -> []U =>
    [f(x) for x in xs]

// дженерик по эффектам — функция наследует эффекты `f`
fn map_eff[T, U, E](xs []T, f (T) E -> U) E -> []U =>
    [f(x) for x in xs]
```

Параметры типа — после имени в квадратных скобках `Имя[T]`, не `<T>`.
Подробно — [D16](decisions/03-syntax.md#d16).
Массивы — `[]T` (динамический), `[N]T` (фиксированный), [D27](decisions/03-syntax.md#d27).

## spawn / supervised / parallel for / detach

См. [D14](decisions/06-concurrency.md#d14), [D50](decisions/06-concurrency.md#d50),
[D71](decisions/06-concurrency.md#d71).

### `spawn expr`

`spawn` — keyword-конструкция (не функция). По спеке D50 — разрешён только внутри
structured-scope (`supervised`, `parallel for`, `race`, `cancel_scope`,
`with_timeout`); вне scope — compile error. В bootstrap-реализации `spawn` вне
scope временно разрешён в eager-blocking семантике (D71 legacy).

Внутри scope `spawn` кладёт fiber в очередь и возвращает unit; результат
работы — через захваченные `mut`-переменные или каналы. `spawn() { body }`
с пустыми скобками **запрещён** (нет смысла; `spawn` — не функция).

```nova
supervised {
    spawn fetch_users()           // spawn + вызов функции
    spawn { compute(x) }          // spawn + inline-блок
}
```

### `supervised { body }`

Structured-concurrency scope. Все `spawn` внутри ждут scope-exit перед запуском;
scheduler крутит resume по очереди (round-robin) пока все не завершатся. См.
D71 для bootstrap-семантики.

```nova
supervised {
    spawn handle_requests()
    spawn periodic_cleanup()
}                                  // ← ждёт пока обе fiber'ы не завершатся
```

`Time.sleep(0)` внутри `supervised` body (на main-уровне) даёт main-flow yield
к queued fibers'ам — один full pass scheduler'а очереди.

### `parallel for x in iter { body }`

Fan-out: для каждого элемента `iter` запускается fiber с `body`. Десугарится в
`supervised { for x in iter { spawn { body } } }`. Loop-переменная захватывается
**по value** (snapshot на момент spawn'а).

```nova
fn fetch_all(urls []str) Net Fail -> []Response =>
    parallel for url in urls {
        fetch(url)
    }
```

### `detach { body }`

Fire-and-forget: тело живёт после возврата вызывающей функции, привязано к
глобальному supervisor'у. Требует эффекта `Detach` в сигнатуре (D50). В bootstrap-
default'е — `SyncDetach` исполняет тело inline.

```nova
fn handle_request(req Request) Net Db Detach -> Response {
    let resp = process(req)
    detach { write_audit(req, resp) }
    resp
}
```

### `Time.sleep(ms)`

Yield-point. По D62 — обычная функция, callable откуда угодно (Async ambient).
В bootstrap'е (D71) — context-sensitive:

| Контекст | Эффект |
|---|---|
| Внутри fiber-body (spawn) | suspend — scheduler крутит других |
| Вне fiber, внутри `supervised` body | один pass очереди (main-yield) |
| Полностью вне scope | no-op |

В bootstrap'е `ms` игнорируется (timer-wheel'а нет). Любое `Time.sleep(N)` =
один cooperative yield.

## Тестирование без моков

`test "name" { body }` — тест-блок верхнего уровня. Имя — строковый
литерал (любые символы, обычно человеческое описание поведения).
Тело — обычный блок выражений; `assert` — встроенный оператор.

```nova
test "withdraw decreases balance" {
    with Db = in_memory_db([acc1, acc2]) {
        let acc = Account.new("alice")
        acc.deposit(100)?
        acc.withdraw(30)?
        assert acc.balance == 70
    }
}

test "insert and get" {
    let mut m = HashMap[str, int].new()
    m.insert("a", 1)
    assert m.get("a") == Some(1)
    assert m.get("b") == None
}
```

Тесты собираются и запускаются только под `nova test`. В обычной сборке
тело пропускается — никаких `#[cfg(test)]`-обвязок. Эффекты подменяются
теми же `with`-блоками что и в проде, никакого mock-фреймворка.

## Panic — не эффект, ловится только runtime'ом

Деление на ноль, выход за границы массива, переполнение — это
**не эффект**, это `Panic`. Программист **не ловит panic в коде** —
panic означает смерть текущего fiber'а, runtime обрабатывает на границе:

```nova
fn mean(xs []int) -> int =>
    xs.sum() / xs.len()                  // никакого Fail[DivByZero]

fn handle(r Request) Db Log -> Response =>
    process(r)             // если panic — fiber умирает, runtime вернёт 500
```

В синхронной программе без fiber'ов (CLI/скрипт) panic = exit процесса.
В серверной — смерть только текущего fiber'а.

Подробно — [revolutionary.md R11](revolutionary.md), [D13](decisions/08-runtime.md#d13).
