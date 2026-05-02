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
| **Примеры** | `Net`, `Db`, `Time`, `Log`, `Throws[BusinessError]`, `Mut` | деление на ноль, переполнение, выход за границы, OOM, переполнение стека |
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

fn server() Par Net Throws -> () {
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
`Throws[E]`, ловит обычным handler'ом.

#### Унификация двух уровней ошибок

- **`throw` + `Throws[E]`** — управляемая ошибка, видна в сигнатуре,
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

Если бы `Throws[DivByZero]` был обязателен, он бы появился в **каждой
второй сигнатуре** (любая функция со средним арифметическим,
дисперсией, делением). К нему присоединились бы `Throws[IntegerOverflow]`,
`Throws[ArrayBounds]`. Это **синдром Java checked exceptions** —
информативность сигнатуры исчезает, потому что эффекты везде.

Сознательный компромисс: **строгая теория эффектов уступает
читабельности** в зоне аппаратных сбоев.

#### Что НЕ Panic, а обычный эффект

- Бизнес-ошибки парсинга, валидации, аутентификации → `Throws[E]`.
- Network failure, DB connection refused → `Throws[NetError]`,
  `Throws[DbError]` внутри эффекта `Net` / `Db`.
- Любая ошибка, которую программа **намерена обрабатывать**, —
  это не Panic.

**Принцип:** «обработать никак нельзя, надо умереть» → Panic;
«обработать можно и нужно» → Throws.

### Что отвергнуто

- **`Throws[DivByZero]` для каждой функции** — спам в сигнатурах.
- **`try_panic`/`catch` в обычном коде** — путает с `Throws`,
  усложняет reasoning о потоке управления.
- **Panic как обычное Throwable** (Java RuntimeException) — приводит
  к ловле «всего» через `catch (Exception e)`, антипаттерн.

### Связь

- [04-effects.md → D25](04-effects.md#d25) — `throw` и `Throws[E]`.
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
type Error                                       // unit-тип-маркер для Throws
type Ordering | Less | Equal | Greater
type Never                                       // unit без значений (uninhabited)
```

**Базовые числовые и строковые типы** (`int`, `i8`-`i64`, `u8`-`u64`,
`f32`, `f64`, `str`, `bool`, `char`, `()`, `byte`) — встроены в язык,
не stdlib, но упомянуты для полноты.

**Стандартные эффекты** — `Throws[E]`, `Io`, `Net`, `Db`, `Fs`,
`Time`, `Random`, `Mut`, `Alloc[R]`, `Async`, `Par`, `Log`, `Trace`,
`Ask[T]` — также в prelude.

**Базовые функции:**

```nova
fn print(s str) Io -> ()
fn println(s str) Io -> ()
fn panic(msg str) -> Never
```

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

#### Эффекты как обычные типы — `Throws[E]` не магия

`Throws[E]` объявляется в prelude как любой другой эффект — через
`protocol` ([04-effects.md → D18](04-effects.md#d18-эффекты-объявляются-через-protocol-не-type)):

```nova
protocol Throws[E] {
    throw(value E) -> Never
}
```

`throw expr` — сахар для `Throws.throw(expr)` (вызов операции
активного handler'а), как `Db.query(...)`. Никакой специальной
обработки. См. [04-effects.md → D25](04-effects.md#d25).

#### Что НЕ в prelude

Коллекции (`String`, `Vec`, `HashMap`), I/O API (`File`, `Http`),
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
- [04-effects.md → D25](04-effects.md#d25) — `throw` и `Throws[Error]`.
- [04-effects.md → D18](04-effects.md#d18) — эффекты как обычные типы.
- [02-types.md → D17](02-types.md#d17) — sum-type, `Never` как пустой.
- [03-syntax.md → D20](03-syntax.md#d20) — `()` вместо `void`.
- [07-modules.md → D29](07-modules.md#d29) — prelude и явные импорты.

### Открытые вопросы

- Полный API `Option`/`Result` (`unwrap`, `map`, `and_then` и т.д.) —
  stdlib API, описывается отдельно.
- Семантика `?` для `Option` — `expr?` на `Option[T]` бросает что?
  Сейчас в коде это работает де-факто, правило не зафиксировано.
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
    Account { _balance: 0, owner: owner }

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
    Account { _balance: ACCOUNT_MIN_BALANCE, owner: owner }
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
// Эффект — protocol ([04-effects.md → D18](04-effects.md#d18-эффекты-объявляются-через-protocol-не-type))
protocol IdGen {
    fresh() -> u64
}

// Handler — обычная функция, возвращающая handler-литерал
fn counter_id_gen(c mut Counter) -> Handler[IdGen] =>
    IdGen {
        fresh() => {
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
