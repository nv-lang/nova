# Runtime — режимы запуска, panic, prelude, статическое состояние

Решения этой группы определяют, как программа Nova **исполняется**:
поддерживаемые режимы компиляции, что считается panic'ом и как он
обрабатывается, что предоставляет prelude и почему в языке нет
static-состояния.

| # | Решение |
|---|---|
| [D7](#d7-один-язык--три-режима-компиляции) | Один язык — три режима компиляции |
| [D13](#d13-panic-vs-эффекты-что-не-является-эффектом) | Panic vs эффекты: что НЕ является эффектом |
| [D26](#d26-базовая-stdlib-и-prelude) | Базовая stdlib и prelude |
| [D41](#d41-static-функции-есть-static-состояния-нет) | Static-функции есть, static-состояния нет |
| [D70](#d70-tostr-protocol--to_str-метод--free-function-tostrv) | `ToStr` protocol + `@to_str()` метод + free function `to_str(v)` |

---

## D7. Один язык — три режима компиляции

### Что
Один и тот же исходник Nova поддерживает три режима исполнения:
**AOT** (бинарь, как Go), **JIT** (как .NET) и **интерпретатор**
(как Python). Скрипт за 1 строку и сервер на 100k строк — это
разные режимы запуска одного языка, а не разные языки.

### Правило

```bash
nova run script.nv          # интерпретатор / JIT (быстрый старт)
nova build app.nv           # AOT-бинарь, как `go build`
nova jit-server             # долгоиграющий процесс с JIT-компиляцией
```

Один и тот же `script.nv` без модификации работает во всех трёх
режимах. Эффекты, типы, контракты, handler'ы — везде ведут себя
одинаково.

### Почему

- **Скрипт vs сервер — это режимы запуска.** Не разные языки.
  Программисту не нужно «переписывать» под другой режим.
- **Прецедент Julia** — тот же подход (JIT по умолчанию + AOT через
  `PackageCompiler.jl`) работает на масштабе data-science.
- **AI-first** — LLM может генерировать код и запускать через
  интерпретатор для быстрой проверки, а тот же код собирать в бинарь
  для production.
- **Эффекты ортогональны runtime'у** — handler'ы перехватываются и в
  JIT, и в AOT, и в интерпретаторе одинаково.

### Что отвергнуто

- **Только AOT** (Rust/Go-стиль) — медленный feedback loop, плохо
  для скриптов и REPL.
- **Только интерпретатор** (Python) — производительность недостаточна
  для backend.
- **Транспиляция в чужой язык** (TypeScript → JS) — теряется
  возможность контроля runtime, привязка к чужой экосистеме.

### Связь

- [01-philosophy.md → D9](01-philosophy.md#d9-честная-оценка-новизны) —
  «три режима компиляции в строго типизированном языке» — одна из двух
  потенциальных уникальных заявок Nova.
- [01-philosophy.md → D10](01-philosophy.md#d10) — три режима следуют
  из «всё — эффект»: handler'ы абстрагируют runtime.

### Открытые вопросы

- Конкретные технологии: LLVM для AOT? Cranelift для JIT? Tree-walking
  для интерпретатора? — выбор реализации.
- Совместимость артефактов между режимами — пока считаем, что один
  исходник, разные бинарные форматы.

---

## D13. Panic vs эффекты: что НЕ является эффектом

### Что
**Не каждое прерывание вычисления — эффект.** Аппаратные/математические
сбои (деление на ноль, выход за границы массива, переполнение, OOM,
переполнение стека) **не указываются в сигнатуре** функции. Они
образуют общую категорию `Panic` — runtime-сбоев, перехватываемых
runtime'ом на границе fiber'а, не программистом в коде.

### Правило

#### Граница

| | Видимое (в сигнатуре) | Универсальное (не в сигнатуре) |
|---|---|---|
| **Что** | эффекты, описывающие **намерение** | сбои, описывающие **невозможность вычисления** |
| **Примеры** | `Net`, `Db`, `Time`, `Log`, `Fail[BusinessError]` | деление на ноль, переполнение, выход за границы, OOM, переполнение стека |
| **Где ловится** | handler'ом в коде | runtime'ом на границе fiber'а |
| **Как создаётся** | `throw` | `panic(msg)` или сам runtime |

#### Перехват — на границе fiber'а runtime'ом

`panic` концептуально означает **смерть текущего fiber'а**, не
процесса. В синхронной программе без fiber'ов (CLI, скрипт) fiber один
= процесс, поэтому panic = exit. В серверной программе с fiber-runtime
([06-concurrency.md → D14](06-concurrency.md#d14)):

- **HTTP-handler** — fiber на запрос. Panic = смерть fiber'а, runtime
  возвращает 500, остальные запросы продолжают.
- **Worker очереди** — fiber. Panic = задача упала, scheduler берёт
  следующую.
- **Supervised group** — supervisor видит «fiber завершился panic'ом»,
  рестартует по своей стратегии.

```nova
fn handle_request(r Request) Db Log -> Response =>
    process(r)             // если panic — fiber умирает, runtime вернёт 500
                            // если throw — handler выше ловит обычно

fn server() Net Fail -> () {
    supervised {
        spawn() { handle_requests() }
        spawn() { periodic_cleanup() }
    } strategy = one_for_one, max_restarts = 3
    // supervisor рестартует упавшие fiber'ы
}
```

**Никакого `try_panic`/`catch` в коде.** Программист **не ловит**
panic в обычной функции — это работа runtime'а на границе fiber'а.
Если программист хочет управляемую ошибку — пишет `throw` +
`Fail[E]`, ловит обычным handler'ом.

#### Унификация двух уровней ошибок

- **`throw` + `Fail[E]`** — управляемая ошибка, видна в сигнатуре,
  перехватывается handler'ом в коде ([04-effects.md → D25](04-effects.md#d25)).
- **`panic`** — сбой fiber'а, перехват только runtime'ом на границе
  fiber'а. В сигнатуре не виден.

Третьего уровня нет. Никаких `try_panic { ... } catch p { ... }` или
`panic_boundary { ... } recover (p) => { ... }` в языке.

#### Опция: строгий режим `@strict_total`

Для критичного кода (медицина, финансы, авионика):

```nova
@strict_total
fn critical(...) -> Result =>
    // деление на ноль здесь — compile error
    // обязаны checked-операции: safe_div(a, b)?, arr.get(i)?
```

Превращает функцию в тотальную (всегда завершается). Цена — больше
кода, но для 1% случаев это окупается.

### Почему

Если бы `Fail[DivByZero]` был обязателен, он бы появился в **каждой
второй сигнатуре** (любая функция со средним арифметическим,
дисперсией, делением). К нему присоединились бы `Fail[IntegerOverflow]`,
`Fail[ArrayBounds]`. Это **синдром Java checked exceptions** —
информативность сигнатуры исчезает, потому что эффекты везде.

Сознательный компромисс: **строгая теория эффектов уступает
читабельности** в зоне аппаратных сбоев.

#### Что НЕ Panic, а обычный эффект

- Бизнес-ошибки парсинга, валидации, аутентификации → `Fail[E]`.
- Network failure, DB connection refused → `Fail[NetError]`,
  `Fail[DbError]` внутри эффекта `Net` / `Db`.
- Любая ошибка, которую программа **намерена обрабатывать**, —
  это не Panic.

**Принцип:** «обработать никак нельзя, надо умереть» → Panic;
«обработать можно и нужно» → Fail.

### Что отвергнуто

- **`Fail[DivByZero]` для каждой функции** — спам в сигнатурах.
- **`try_panic`/`catch` в обычном коде** — путает с `Fail`,
  усложняет reasoning о потоке управления.
- **Panic как обычное Throwable** (Java RuntimeException) — приводит
  к ловле «всего» через `catch (Exception e)`, антипаттерн.

### Связь

- [04-effects.md → D25](04-effects.md#d25) — `throw` и `Fail[E]`.
- [06-concurrency.md → D14](06-concurrency.md#d14) — supervisor, fiber'ы.
- [01-philosophy.md → D10](01-philosophy.md#d10) — «всё — эффект» с
  оговоркой про runtime panics.

---

## D26. Базовая stdlib и prelude

### Что
Базовые типы (`Option[T]`, `Result[T, E]`, `Error`, `Never`,
`Ordering`) и их конструкторы (`Some`, `None`, `Ok`, `Err`) живут в
**prelude** — автоматически в скоупе любого модуля, без `import`.
Список prelude **явно зафиксирован** в одном месте, не «магия».

### Правило

#### Что в prelude (v1.0)

**Типы:**

```nova
type Option[T] | Some(T) | None
type Result[T, E] | Ok(T) | Err(E)
type Ordering | Less | Equal | Greater
type Never                                       // unit без значений (uninhabited)
type any protocol { }                            // top-type через пустой protocol (D53)

// Error — record для quick-and-dirty ошибок с сообщением (D65)
type Error {
    readonly msg str
}
fn Error.new(msg str) -> Error => { msg }

// RuntimeError — sum-тип встроенных runtime-сбоев (D65)
// Бросается встроенными операциями: a/b на 0, arr[i] на out-of-bounds, etc.
// StackOverflow и OutOfMemory не входят — они panic, не Fail (D13).
type RuntimeError
    | DivByZero
    | Overflow
    | IndexOutOfBounds { index int, length int }
    | TypeMismatch(str)
    | AssertFailed(str)
    | NoHandler(str)

// Iterator protocol (D58)
type Iter[T] protocol {
    mut next() -> Option[T]
}

// Range — литерал `a..b` / `a..=b` (D58)
type Range {
    readonly start int
    readonly end int
    readonly inclusive bool
}
type RangeIter {
    end       int
    inclusive bool
    mut cur   int
}
```

**Базовые числовые и строковые типы** (`int`, `i8`-`i64`, `u8`-`u64`,
`f32`, `f64`, `str`, `bool`, `char`, `()`, `byte`) — встроены в язык,
не stdlib, но упомянуты для полноты.

**`any`** — пустой protocol-тип (D53). Любой тип удовлетворяет
пустому контракту, поэтому `any` — top-type (универсальный супертип).
Имя lowercase — исключение в [03-syntax.md → D30](03-syntax.md#d30)
naming convention, по аналогии с примитивами. Использование:
`fn dump(x any) Io -> ()`, `Logger.log_event(level, fields []any)`
для гетерогенных структурных логов.

**`Iter[T]`** — структурный protocol для итераторов (D58). Любой
тип с методом `mut next() -> Option[T]` автоматически удовлетворяет.
`for x in collection`-синтаксис вызывает `collection.iter().next()` в
цикле; коллекции реализуют `iter()` возвращая собственный iterator-тип.

**`Range`** — runtime-представление range-литерала `a..b` (exclusive)
и `a..=b` (inclusive) (D58). Range — обычное значение, можно
передавать как аргумент, хранить в переменной, использовать в `for`.

**Стандартные эффекты** (после [D62](04-effects.md#d62)) — `Fail[E]`,
`Io`, `Net`, `Db`, `Fs`, `Time`, `Random`, `Alloc[R]`, `Log`, `Trace`,
`Ask[T]` — также в prelude. `Async`/`Par` — runtime-инфраструктура,
не type-system эффекты ([D14 (REVISED)](06-concurrency.md#d14)).
`Mut` удалён ([D62](04-effects.md#d62)) — изменяемое состояние через
`mut` поля и параметры.

**Базовые функции:**

```nova
fn print(...items []any) Io -> ()           // variadic, см. D69
fn println(...items []any) Io -> ()         // variadic + newline
fn panic(msg str) -> Never
```

`print`/`println` — **variadic** ([D69](03-syntax.md#d69)),
принимают любое число аргументов любого типа (`any` —
[D54](03-syntax.md#d54)). Каждый аргумент конвертируется в строку
через `to_str()` ([D70](#d70-tostr-protocol--to_str-метод--free-function-tostrv)).
Spread разрешён: `print(...parts)`.

#### `Never` — обычный тип без значений

`Never` объявлен как **sum-type с нулём вариантов** — синтаксически
`type Never =` (после `=` пусто). Это легальная конструкция в системе
[02-types.md → D17](02-types.md#d17): пустой список вариантов —
корректный частный случай.

**Свойства следуют из пустоты, не из специального правила:**

- **Нельзя создать значение типа `Never`** — нет ни одного варианта.
- **`Never` — подтип любого типа** (bottom type ⊥). Любой контекст,
  ожидающий `T`, может принять `Never`-выражение.
- **Используется в типах не-возвращающих выражений** — `throw expr`,
  `return expr`, `panic(...)`, бесконечный `loop`. Все имеют тип
  `Never`, поэтому совместимы с любым контекстом.

Аналоги: Rust `!`, Haskell `Void`, Kotlin/Scala `Nothing`,
TypeScript `never`. Не уникальная фича Nova.

#### Эффекты как обычные типы — `Fail[E]` не магия

`Fail[E]` объявляется в prelude как любой другой эффект — через
kind-токен `effect` ([04-effects.md → D18 (REVISED)](04-effects.md#d18-эффекты-объявляются-через-kind-токен-не-голый-type),
[D61](04-effects.md#d61)):

```nova
type Fail[E] effect {
    fail(value E) -> Never
}
```

`throw expr` — сахар для `Fail[E].fail(expr)` (вызов операции
активного handler'а), как `Db.query(...)`. Никакой специальной
обработки. См. [04-effects.md → D25](04-effects.md#d25),
[04-effects.md → D61](04-effects.md#d61).

#### Что НЕ в prelude

Коллекции (`String`, `HashMap`, `HashSet`, `LinkedList`), I/O API (`File`, `Http`),
JSON, SQL, время как библиотека — **обычные модули**, требующие
явного импорта:

```nova
import std.io.{File, read_all}
import std.collections.HashMap
```

### Почему

#### Зачем нужен prelude

Без prelude каждый файл начинается с:

```nova
import std.option.{Option, Some, None}
import std.result.{Result, Ok, Err}
```

Это шум на 90% файлов. Прецедент — Rust, Haskell, Swift, Kotlin: все
имеют prelude. AI-first: LLM не должен генерировать boilerplate-импорты
базовых типов.

#### Не противоречит «локальности контекста»

Prelude **документирован**, его содержимое — фиксированный список,
не магия. LLM знает, что доступно везде. Всё остальное — явный импорт
([07-modules.md → D29](07-modules.md#d29)).

### Что отвергнуто

- **Никакого prelude, всё через явный import** — шум, не выигрыш.
- **Prelude определяется компилятором, без документации** — магия,
  ломает AI-first тезис.
- **Prelude настраивается per-project** — усложнение без выгоды; LLM
  должен знать фиксированный набор.
- **`Void`** — отвергнут, тип «без значения» это `()` (unit). См.
  [03-syntax.md → D20](03-syntax.md#d20).

### Связь

- [01-philosophy.md → D10](01-philosophy.md#d10) — AI-first,
  локальность через документированный prelude.
- [04-effects.md → D25](04-effects.md#d25) — `throw` и `Fail[Error]`.
- [04-effects.md → D18](04-effects.md#d18) — эффекты как обычные типы.
- [02-types.md → D17](02-types.md#d17) — sum-type, `Never` как пустой.
- [03-syntax.md → D20](03-syntax.md#d20) — `()` вместо `void`.
- [07-modules.md → D29](07-modules.md#d29) — prelude и явные импорты.

### Открытые вопросы

- Полный API `Option`/`Result` (`unwrap`, `map`, `and_then` и т.д.) —
  stdlib API, описывается отдельно.
- ~~Семантика `?` для `Option`~~ — закрыто
  [D67](04-effects.md#d67): ранний `return None` из текущей функции.
- `Error` как универсальный тип — что в нём (`to_str()`, цепочка
  причин)? Похоже на Rust `std::error::Error`.

### Цена

1. **Список prelude нужно поддерживать.** Любое добавление в prelude —
   breaking change после v1.0 (имя становится «зарезервированным» в
   модулях). Поэтому prelude **минимален**.
2. **Импорт-конфликты.** Если программист объявит свой `type Option`,
   будет конфликт с prelude — компилятор предупредит.

---

## D41. Static-функции есть, static-состояния нет

### Что
У типа есть **static-функции** (`fn Type.name(...)`), но **нет
static-полей**, **нет static-переменных**, **нет static initializer'ов**.
Если нужны константы, ассоциированные с типом, — это `const` в том же
модуле. Если нужно «глобальное» изменяемое состояние — это **handler**
(эффект-capability), не static.

### Правило

#### Static-функции — обычные функции в namespace типа

Внутри одной static-функции другие static-функции того же типа
вызываются **через полное имя**, без сокращений:

```nova
fn Account.new(owner str) -> Account =>
    Account { _balance: 0, owner }

fn Account.from_balance(owner str, initial money) -> Account {
    let acc = Account.new(owner)             // явное Account.new, не self.new
    Account.deposit_static(acc, initial)     // тоже явно
    acc
}
```

Никакого `Self::new` (Rust) или просто `new` (Java/C#). Один способ
вызова static-функции — через имя типа, что внутри типа, что снаружи.

#### Константы рядом с типом — `const` в модуле

```nova
const ACCOUNT_MIN_BALANCE money = 0
const ACCOUNT_MAX_OVERDRAFT money = 1000

fn Account.new(owner str) -> Account =>
    Account { _balance: ACCOUNT_MIN_BALANCE, owner }
```

Если нужна группировка — отдельный модуль:

```nova
module account_limits

export const MIN_BALANCE money = 0
export const MAX_OVERDRAFT money = 1000

// использование:
import account_limits
let acc = Account.new_with(account_limits.MIN_BALANCE)
```

#### Глобальное изменяемое — через handler

Вместо static counter / static config — handler, передаваемый через
`with`-блок:

```nova
// Эффект ([04-effects.md → D61](04-effects.md#d61))
type IdGen effect {
    fresh() -> u64
}

// Handler — обычная функция, возвращающая handler-литерал
fn counter_id_gen(c mut Counter) -> Handler[IdGen] =>
    handler IdGen {
        fresh() {
            c.count += 1
            c.count
        }
    }

// в main:
fn main() {
    let mut counter = Counter { count: 0 }
    with IdGen = counter_id_gen(counter) {
        run_app()
    }
}
```

> Это пример **closure-capture** паттерна по [D68](04-effects.md#d68).
> Альтернатива — `@as_handler` метод на record'е `Counter` —
> рассмотрена в D68 для случаев, когда state нужно проинспектировать
> снаружи. Выбор между паттернами детерминирован сценарием
> (нужен ли state наружу), не вкусом.

Тестируется тривиально — другой handler в `with`-блоке.

### Почему

- **Static state — главный источник скрытых багов.** Глобальный
  изменяемый стейт не виден в сигнатурах, ломает параллельность,
  невозможно тестировать без хаков.
- **Тесты.** Static-поле = разделяемое состояние между тестами.
  Каждый тест должен либо ресетить его (хрупко), либо запускаться
  изолированно (медленно). Handler — `with`-блок изолирует
  автоматически.
- **Параллелизм.** Несколько fiber'ов на одном static-поле = data race
  по умолчанию. Handler-state живёт в scope и не делится случайно.
- **DI is the language.** Передача зависимостей — это handler. Не
  нужен отдельный фреймворк для DI, не нужны static-singleton'ы как
  замена.
- **Единственный путь.** Нет «иногда static, иногда handler» —
  всегда handler. Меньше способов сделать неправильно.

### Что отвергнуто

- **Static mutable поля** (Java `static int counter`, Python class
  variable) — мешают тестам и параллелизму.
- **Static immutable поля как `const`** на типе (`const Account.MIN`)
  — технически безопасно, но добавляет второй способ объявить
  константу. Один способ — `const` в модуле.
- **Companion-object** (Kotlin) — то же что и static, просто в
  обёртке. Не нужен.
- **Lazy static** (Rust `lazy_static!`) — скрытое глобальное состояние
  с инициализацией. Если нужна ленивость — handler с lazy полем.

### Связь

- [05-memory.md → D6](05-memory.md#d6) — глобального mutable state не
  предусмотрено в модели памяти; всё живёт в fiber-scope или
  handler-scope.
- [04-effects.md → D11](04-effects.md#d11),
  [04-effects.md → D31](04-effects.md#d31) — handler-механизм для
  «глобальных» состояний.
- [04-effects.md → D18](04-effects.md#d18) — эффекты это обычные `type`,
  не keyword `effect`.
- [03-syntax.md → D33](03-syntax.md#d33) — `const` — единственный
  способ объявить immutable «глобальную» константу.

### Цена

1. **Привычка из Java/C#/Python ломается.** Нет `Account.MAX_BALANCE`
   как поля, есть `MAX_BALANCE` как `const` в модуле. Чуть длиннее,
   но единообразнее.
2. **Singleton'ы переписываются как handler.** Это не цена, а фича —
   но мигрирующий код придётся переделать.
3. **Counter / cache / pool** требуют явного создания и проброса в
   `with`-блок. Не «само работает», а явный жизненный цикл.

### Эволюция

В исходной формулировке D41 пример использовал устаревшие keyword'ы
`effect IdGen { ... }` и `handler counter_id_gen(...) IdGen { ... }` —
оба отменены ([04-effects.md → D18](04-effects.md#d18) — эффект это
обычный `type`; слово `handler` не зарезервировано).
В текущем тексте пример переписан как `type IdGen { ... }` +
обычная функция, возвращающая handler-литерал.

---

## D70. `ToStr` protocol + `@to_str()` метод + free function `to_str(v)`

### Что
Универсальный механизм конверсии значения в строку:

1. **`ToStr`** — protocol с одним методом `@to_str() -> str`.
2. **`@to_str()`** — метод на типе, реализует представление в строку.
3. **`to_str(v)`** — свободная функция в prelude, sugar над `v.to_str()`.

Все встроенные типы (`int`, `str`, `bool`, `float`, `()`,
record/sum-комбинации) реализуют `ToStr` автоматически (auto-derive
по структуре). Программист может override на своих типах через обычный
`@`-метод.

### Правило

#### Декларация protocol'а в prelude

```nova
type ToStr protocol {
    to_str() -> str
}
```

#### Builtin реализации (auto-derive)

Все базовые типы реализуют `ToStr` автоматически — программист **не
пишет** `@to_str()` для:

| Тип | Формат |
|---|---|
| `int` (любой size) | десятичное число: `42`, `-100` |
| `float` (f32/f64) | как Rust `Display`: `3.14`, `-0.5` |
| `bool` | `true` / `false` |
| `str` | сама строка (без кавычек) |
| `()` (unit) | `()` |
| `[]T` (где T: ToStr) | `[a, b, c]` (элементы через `to_str`) |
| `(A, B, ...)` tuple | `(a, b, ...)` |
| record `T { f1, f2 }` | `T { f1: ..., f2: ... }` |
| sum-variant `Foo(x)` | `Foo(x)` |
| sum-variant `Bar` (unit) | `Bar` |

Auto-derive работает рекурсивно — записи и sum-варианты
форматируются через `to_str()` своих полей/аргументов.

#### Override на пользовательском типе

```nova
type UserId u64

fn UserId @to_str() -> str => "user#${@}"

let id = UserId(42)
to_str(id)              // "user#42" (через override)
"id is ${id}"           // "id is user#42" (string interpolation также через ToStr)
```

#### Free function `to_str`

```nova
fn to_str[T: ToStr](v T) -> str => v.to_str()
```

Это единственная универсальная точка для получения строкового
представления. Внутри `print`/`println` и string interpolation
используется именно `to_str(v)`.

#### Compile-time enforcement

`ToStr`-bound — обычный generic-bound:

```nova
fn debug_log[T: ToStr](label str, v T) Log -> () =>
    Log.info("${label} = ${to_str(v)}")
```

Если программист объявил `type MyType { ... }` и НЕ реализовал
`@to_str()`, и тип не подпадает под auto-derive — `to_str(my)`
вызовет compile error «`MyType` does not implement `ToStr`».

В практике auto-derive покрывает большинство случаев, поэтому
явное объявление `@to_str()` нужно только для **кастомного формата**
(как `UserId` выше).

#### Связь со string interpolation

Любой `${expr}` в string-литерале — sugar над `to_str(expr)`:

```nova
"id=${user_id}"          // ≡ "id=" + to_str(user_id)
"point=(${x}, ${y})"     // → "point=(3, 4)"
```

Тип `expr` должен реализовывать `ToStr` (обычно auto-derive).

### Семантика auto-derive

Компилятор генерирует **default `@to_str()`** для:

- **Record**: `T { f1: v1, f2: v2 }` → `"T { f1: ${to_str(v1)}, f2: ${to_str(v2)} }"`
  - Поля выводятся в порядке объявления (D52).
- **Sum-variant**: `Foo(x, y)` → `"Foo(${to_str(x)}, ${to_str(y)})"`
- **Sum-unit-variant**: `Red` → `"Red"`
- **Tuple**: `(a, b, c)` → `"(${to_str(a)}, ${to_str(b)}, ${to_str(c)})"`
- **Array**: `[a, b, c]` → `"[${to_str(a)}, ${to_str(b)}, ${to_str(c)}]"`
- **Newtype**: тот же что и underlying — `type UserId u64` без override
  → `to_str(UserId(42))` = `"42"`. Override меняет.

Все элементы рекурсивно требуют `ToStr`. Если хоть один не реализует —
compile error на месте использования.

### Почему

1. **AI-friendly default** — программист пишет `to_str(v)` или `"${v}"`
   и получает работу для любого типа. Не нужно реализовывать `Show`-
   trait вручную.

2. **Compile-time enforcement** — `ToStr`-bound в функциях
   (`fn f[T: ToStr]`) даёт явный контракт. LLM/compiler ловит
   несоответствие до runtime'а.

3. **Override через стандартный `@`-метод** — не новый синтаксис.
   Если auto-derive формат не подходит — пишешь `fn T @to_str()` как
   обычный метод.

4. **Один protocol, не два** (как Rust `Display`/`Debug`) — D40
   «один способ». Если когда-то понадобится debug-формат — отдельный
   D-блок (`Debug` protocol с `@to_debug()`), но не сейчас.

5. **Имя `ToStr` буквальное** — описывает что делает (converts to
   `str`). Не путается с UI-кодом (как `Display`/`Show`).

6. **Symmetric с возможным расширением:**
   - `ToStr` → `to_str() -> str`
   - `ToJson` (если понадобится) → `to_json() -> Json`
   - `ToBytes` → `to_bytes() -> []u8`

   Единое naming convention.

### Что отвергнуто

- **`Display` имя** (как Rust). Слишком общее, конфликтует с UI/HTML
  кодом (`fn Slide @display()`). `ToStr` описательнее.
- **`Show` имя** (Haskell/OCaml). Конфликтует с UI (`popup.show()`).
- **`Stringer` имя** (Go). Метод в Go называется `String()`; у нас
  метод `to_str()` — несоответствие.
- **Без protocol'а, только free function `to_str(any)`**. Без bound'а
  нет compile-time enforcement; программист может забыть реализовать
  override и получит auto-derive вместо ожидаемого формата.
- **Два protocol'а `ToStr` + `Debug`** (как Rust). У Nova нет
  отдельной debug-семантики на уровне prelude. Если понадобится —
  отдельный D-блок.
- **Универсальный `@cast[X]` метод** (был рассмотрен и отвергнут):
  - `[X]` синтаксически объявляет generic-параметр (D16), не target —
    конфликт грамматики.
  - Return-type dispatch требует typeclass-механизма, которого в Nova
    пока нет.
  - Каждая конверсия — отдельный protocol с уникальным именем
    (`ToStr`, `ToJson`) — D46 overloading по имени работает естественно.

### Связь

- [D26](#d26-базовая-stdlib-и-prelude) — `to_str(v)` в prelude,
  `print`/`println` через variadic ([D69](03-syntax.md#d69)).
- [D35](03-syntax.md#d35) — `@`-методы.
- [D40](01-philosophy.md#d40) — «один способ» (один protocol, не два).
- [D42 (REVISED)](02-types.md#d42) / [D53](02-types.md#d53) /
  [D62](04-effects.md#d62) — `protocol` для структурных контрактов.
- [D46](03-syntax.md#d46) — overloading методов по имени.
- [D69](03-syntax.md#d69) — variadic `print(...items []any)` использует
  `to_str` для каждого элемента.

### Эволюция

В bootstrap-stdlib функция `to_str(v)` существовала как Native-функция,
работающая на любом значении через Rust-side `format!("{}", v)` (то
есть auto-derive прямо на runtime-уровне). Но **формальной декларации
`ToStr` protocol'а в спеке не было** — это был implementation-факт.

D70 формализует:
1. `ToStr` protocol с методом `@to_str()` — стандартная декларация.
2. Auto-derive для всех встроенных + record/sum типов.
3. Override через обычный `@to_str()` метод.
4. Free function `to_str[T: ToStr](v T) -> str` — публичный API.
5. String interpolation `"${expr}"` — sugar над `to_str(expr)`.

Альтернативы рассмотрены и отвергнуты:
- `Display`/`Show`/`Stringer` имена — конфликты с UI-кодом или
  inconsistency с именем метода.
- Универсальный `@cast[X]` — синтаксический конфликт с generic-
  параметрами и нет return-type dispatch'а в Nova.
- Без protocol'а — нет compile-time enforcement.
