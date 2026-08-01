# Effects — `Fail`, `Io`, `Db`, handlers, with-блоки

Решения этой группы определяют центральную абстракцию Nova: **алгебраические
эффекты**. Любое взаимодействие с внешним миром — эффект; у эффекта есть handler;
handler перехватывается в `with`-скоупе. Из этой идеи следуют замены
ключевых слов `async`/`throws`/`unsafe` на типы и единый механизм
для тестов, транзакций, undo/redo, capability-режима.

| # | Решение |
|---|---|
| [D2](#d2-эффекты-вместо-ключевых-слов-asyncthrowsunsafe) | Эффекты вместо ключевых слов `async`/`throws`/`unsafe` |
| [D3](#d3-синтаксис-эффектов-типы-между--и--) | Синтаксис эффектов: типы между `)` и `->` |
| [D4](#d4--для-пробрасывания-ошибки) | `?` для пробрасывания ошибки |
| [D11](#d11-имена-эффектов-и-синтаксис-with) | Имена эффектов и синтаксис `with` |
| [D12](#d12-effect-erasure-и-dynamic-effects) | Effect erasure и dynamic effects |
| [D18](#d18-эффекты-объявляются-через-kind-токен-не-голый-type) | Эффекты объявляются через `protocol`, не `type` |
| [D25](#d25-throw-и-параметризация-throwse) | `throw` и параметризация `Fail[E]` |
| [D28](#d28-вывод-эффектов-private--выводится-public--обязательно-явно) | Вывод эффектов: private — выводится, public — явно |
| [D31](#d31-handler-лямбда-для-эффектов-с-одной-операцией) | Handler-лямбда для эффектов с одной операцией |
| [D61](#d61-полная-семантика-эффектов-effect-keyword-handler-литерал-handlere-interrupt) | Полная семантика эффектов: `effect` keyword, handler-литерал, `Effect[E]`, `interrupt` |
| [D62](#d62-прагматичная-семантика-эффектов-прямые-в-сигнатуре-fail-strict-async-ambient-правило-effectprotocol) | Прагматичная семантика эффектов: прямые в сигнатуре, Fail strict, Async ambient, правило effect/protocol |
| [D63](#d63-forbid-x--body---capability-sandbox) | `forbid X { body }` — capability sandbox |
| [D64](#d64-realtime--block--гарантия-не-приостановки) | `realtime { body }` — гарантия не-приостановки |
| [D65](#d65-полная-семантика-fail-гибрид-faile--fail-lookup-prelude-runtimeerror-и-error) | Полная семантика `Fail`: гибрид `Fail[E]` / `Fail`, lookup, prelude `RuntimeError` и `Error` |
| [D67](#d67--оператор-семантика-для-result-через-fail-для-option-через-ранний-return) | ⚠️ ОТМЕНЕНО → D85: `?` оператор (две семантики) |
| [D68](#d68-stateful-handlers-через-closure-capture-или-as_handler-метод-record) | Stateful handlers: через closure capture или `@as_handler` метод record |
| [D85](#d85-операторы--и--унифицированное-поведение-для-result-и-option-throw-стиль-через-) | Операторы `?` и `!!` — унифицированное поведение для `Result` и `Option`, throw-стиль через `!!` |
| [D86](#d86--coalesce-оператор-fallback-для-resultoption) | `??` coalesce-оператор — fallback для `Result`/`Option` без `Fail` |
| [D87](#d87-handlere-irt--параметризация-handler-типом-interruptа) | `Effect[E, IRT]` — параметризация `Handler` типом interrupt'а |
| [D120](#d120-pure-views--axioms--verifytrusted-handlers) | `#pure` views + axioms + `#verify`/`#trusted` handlers |
| [D115](#d115-axiom-binder--bindertype-enum-вместо-optiontyperef) | Axiom binder — `BinderType` enum вместо `Option<TypeRef>` |

Полное введение в концепцию — [../effects.md](../effects.md). AI-first
обоснование — [01-philosophy.md → D10](01-philosophy.md#d10-революционная-ставка-всё--эффект--ai-first).

---

## D2. Эффекты вместо ключевых слов `async`/`throws`/`unsafe`

> ⚠️ **REVISED → [D62](#d62).** `Async` / `Mut` / `Par` убраны из
> стандартного набора. `Async` стал ambient (невидимая инфраструктура
> fiber-runtime'а, см. [D14](06-concurrency.md#d14)), `Par` тоже не
> эффект — параллелизм через `spawn`/`parallel` без эффект-метки
> ([D50](#d50)). `Mut` удалён целиком (изменяемое состояние через
> `mut`-поля и `mut`-параметры, не эффект). Для no-suspend гарантии
> используется `realtime { }` block ([D64](#d64)) как inverse-маркер.
>
> ⚠️ **AMENDED by Plan 118 (D216)** — keyword `unsafe { }` **restored**
> как **syntactic sugar** для built-in effect handler. Под капотом:
> `unsafe { expr }` ≡ `with unsafe_handler { perform UnsafeOps.<op>(expr) }`.
> D2 spirit (всё — эффекты) **preserved** — `unsafe_handler` is built-in
> effect handler internally. User-facing syntax ergonomic (Rust-familiar
> `unsafe { }` block). `unsafe fn` — declares function of unsafe type
> (caller must `unsafe { ... }` wrap call).
> **No effect propagation** up the call stack — `unsafe` encapsulates
> per fn (canonical Rust pattern). Affected ops: pointer creation/deref/
> auto-deref/arith/reverse-cast/ordering-compare/`&record.field`/calling
> `unsafe fn`. See [Plan 118](../../docs/plans/118-typed-pointers-and-unsafe.md)
> §«unsafe { } block model» и
> [D216 §8-9](02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo).
>
> ⚠️ **D2 amend (Plan 118.1.5 closeout, 2026-06-06)**
>
> `#unsafe` attribute scope extended от Nova fn declarations к external fn
> declarations. Same enforcement: call site без `unsafe { }` block →
> `E_UNSAFE_CALL_REQUIRES_WRAP`. Use case: FFI bindings wrapping C-side
> unsafe operations (`dlopen`, `memcpy`, `dlerror`, `RawMem.copy` etc.)
> обязывают caller к explicit unsafe context. Closes
> [M-118.1-unsafe-attr-on-external-fn].
>
> ⚠️ **D2 amend (Plan 118.1.7 closeout, 2026-06-09)**
>
> Plan 118.1.7 migrates from `#unsafe` attribute to `unsafe fn` keyword
> (type-consistent, per Plan 118.5 TypeRef::Unsafe + Plan 118.1.6 `*unsafe fn` ptr type).
> `unsafe fn` — declares function of unsafe fn type; `external unsafe fn` — external fn
> of unsafe type. Declaration syntax mirrors fn-ptr type `*unsafe fn(...)` (Plan 118.1.6).
> `#unsafe fn` → hard error `E_UNSAFE_ATTR_DEPRECATED`.
>
> ⚠️ **D2 align (Plan 138.5)** — fn-ptr type следует тому же no-prefix
> правилу, что и data-указатели ([D216 §1](02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo)):
> `unsafe` пишется **постфиксом pointee** — `*unsafe fn(...)` (unsafe fn ptr),
> а не prefix `unsafe * fn(...)`. Канонично: `*fn(...)` (safe) / `*unsafe fn(...)`
> (unsafe). Prefix-модификатор перед `*` → `E_POINTER_PREFIX_MODIFIER`. См.
> [D216 §9-10](02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo).

### Что
Единая система эффектов заменяет два разнородных языковых механизма
(`throws`, `unsafe`). Эффекты — обычные **типы в PascalCase**
(`Fail[E]`, `Io`, `Db`, `Net`, `Log`, `Alloc[R]`), выводятся
компилятором в private, объявляются явно в public между списком
параметров и `->`.

### Правило

Стандартный набор эффектов в stdlib (после [D62](#d62)):

| Эффект | Что описывает |
|---|---|
| `Fail[E]` | Контракт для перехвата и обработки ошибки типа `E` (D25/D65) |
| `Io` | Файлы, stdout/stderr |
| `Net` | Сетевые запросы |
| `Db` | База данных |
| `Fs` | Чтение/запись файлов |
| `Time` | Часы, таймеры, задержки |
| `Random` | RNG |
| `Alloc[R]` | Аллокация в регионе `R` |
| `Log` | Структурированный лог |
| `Trace` | Распределённая трассировка |
| `Ask[T]` | Чтение из контекста (как Reader) |

Все имена в **PascalCase** — это типы, не keyword'ы. Никаких
специальных правил «эффекты с маленькой».

```nova
fn parse(s str) Fail -> int
fn fetch(url str) Net Fail -> Response
fn save(u User) Fail Db Log -> ()
```

`Fail` без параметров ≡ `Fail[any]` ([D65](#d65)) — catch-all для
quick-and-dirty. Для production рекомендуется явный `Fail[E]`.

Программист может объявлять собственные эффекты через keyword
`effect` ([D18 (REVISED)](#d18-эффекты-объявляются-через-kind-токен-не-голый-type), [D61](#d61)):

```nova
type Logger effect {
    log(msg str) -> ()
}

fn process(o Order) Logger Db -> Receipt {
    Logger.log("processing")
    Db.query(sql`SELECT receipt FROM orders WHERE id = ${o.id}`)
}
```

### Почему

1. **Невидимое поведение в Java/Python/JS.** Любая функция может бросить
   что угодно — это не видно по сигнатуре. Checked exceptions Java
   получились плохо: не комбинируются с дженериками и лямбдами.
   Go-стиль `if err != nil` — много шума, легко забыть.
2. **Async-вирус.** В Rust/JS/C# `async` отравляет всю цепочку вызовов
   через `Future<T>` и обязательный `await`. В Nova suspension —
   ambient runtime-инфраструктура ([D62](#d62), [D14](06-concurrency.md#d14)),
   без цвета функции и без `await`.
3. **AI-first.** LLM, читая сигнатуру, **знает все побочные действия**.
   В Python/Java/Go этой информации в типе нет — для AI это
   восстанавливается чтением десятка вызываемых функций.
4. **Один механизм для всего.** Тестирование без моков, транзакции,
   undo/redo, детерминированный запуск, трассировка, capability
   security — всё это handler'ы одного и того же механизма.

### Что отвергнуто

- **`async`/`throws`/`unsafe` как отдельные keyword'ы.** Три разных
  механизма для трёх случаев — каждый с собственными правилами
  композиции, перехвата и пропагации.
- **Lowercase имена эффектов** (`throws io async`, как было в первых
  черновиках). Эффекты — типы, к ним применяется единое
  PascalCase-правило ([03-syntax.md → D30](03-syntax.md#d30)).
- **Effects как ещё одна фича рядом с trait'ами.** В Nova это центр
  языка ([01-philosophy.md → D10](01-philosophy.md#d10-революционная-ставка-всё--эффект--ai-first)).

### Связь

- [D3](#d3-синтаксис-эффектов-типы-между--и--) — позиция между `)` и `->`.
- [D11](#d11-имена-эффектов-и-синтаксис-with) — three positions имени
  эффекта, синтаксис `with`.
- [D18](#d18-эффекты-объявляются-через-kind-токен-не-голый-type) — эффект
  объявляется через `protocol`, не `type` и не специальный keyword.
- [D25](#d25-throw-и-параметризация-throwse) — `Fail[E]` параметризация.
- [D28](#d28-вывод-эффектов-private--выводится-public--обязательно-явно) —
  правило вывода private vs public.
- [01-philosophy.md → D10](01-philosophy.md#d10-революционная-ставка-всё--эффект--ai-first) —
  «всё эффект» как центральная абстракция языка.
- [06-concurrency.md → D14](06-concurrency.md#d14) — fiber runtime
  как ambient инфраструктура (suspension не в типах).

### Эволюция

В первых черновиках имена эффектов были lowercase (`throws io async`) —
их пытались выделить визуально из имён типов. Пересмотрено: эффекты —
обычные типы, к ним применяется PascalCase-правило ([D11](#d11-имена-эффектов-и-синтаксис-with),
[03-syntax.md → D30](03-syntax.md#d30)). `Fail` без параметра теперь
читается как сахар над `Fail[Error]`. Подробно —
[history/evolution.md](history/evolution.md).

---

## D3. Синтаксис эффектов: типы между `)` и `->`

### Что
Эффекты в сигнатуре функции перечисляются через пробел между закрывающей
скобкой параметров `)` и стрелкой возврата `->`. Граница задана
структурой, парсер однозначен, никаких маркеров и ограничителей.

### Правило

```nova
fn save(u User) Fail Io -> ()
fn fetch(url str) Net Fail -> Response
fn process(o Order) Db Log -> Receipt
fn double(x int) -> int                          // нет эффектов — чистая
```

Параметры — без двоеточия (`u User`, не `u: User`) — единое
правило для всех типов в Nova ([02-types.md → D17](02-types.md#d17),
[03-syntax.md → D33](03-syntax.md#d33)).

Эффекты с параметрами читаются так же:

```nova
fn parse(s str) Fail[ParseError] -> int
fn alloc_in(buf []u8) Alloc[r] -> Buffer
fn read_ctx(key str) Ask[Config] -> str
```

Если эффектов нет — между `)` и `->` пусто:

```nova
fn add(a int, b int) -> int =>
    a + b
```

Эффекты в сигнатуре методов через `@` — после параметров, перед `->`:

```nova
fn Account mut @deposit(amount money) Fail Log -> () => ...
```

### Почему

1. **Граница задана структурой.** `)` слева, `->` справа — парсер
   однозначен без маркеров.
2. **Эффекты — это типы** ([D2](#d2-эффекты-вместо-ключевых-слов-asyncthrowsunsafe),
   [D11](#d11-имена-эффектов-и-синтаксис-with)). Применяется единое
   PascalCase-правило ([03-syntax.md → D30](03-syntax.md#d30)).
3. **Читается слева направо как фраза:**
   «функция `save` от `User` бросает, делает `Io`, асинхронна, возвращает
   `()`».

### Что отвергнуто

- **`!throws io async`** (маркер `!` слева). Глаз читает `!throws` как
  «не throws» — противоположный смысл. К тому же `!` стоит только
  перед первым эффектом, дальше идут «голые» — границы списка не видно.
- **`!throws !io !async`** (маркер на каждом). Шумно, проблема «`!`
  как not» остаётся.
- **`!{throws, io, async}`** (явный блок). Фигурные скобки заняты телом
  функции — путается.
- **`<throws, io, async>`** (Koka-style). Угловые скобки нужны для
  дженериков (хотя в Nova используется `[T]`, см.
  [03-syntax.md → D16](03-syntax.md#d16)), читается тяжелее.
- **Атрибуты `@throws @io @async`.** Четыре лишних символа, и `@`
  ассоциируется с метаданными, а не с типом. К тому же `@` уже занят
  методами инстанса ([03-syntax.md → D35](03-syntax.md#d35)).
- **Без маркера, всё выводить молча.** Опасно — эффекты должны быть
  **видны на глаз** в публичном API.
- **Lowercase имена эффектов** (`throws io async`). Отвергнуто в
  [D11](#d11-имена-эффектов-и-синтаксис-with) — эффекты обычные типы.
- **`:` в параметрах** (`u: User`). Заменено на `u User` —
  единый стиль ([02-types.md → D17](02-types.md#d17)).

### Связь

- [D2](#d2-эффекты-вместо-ключевых-слов-asyncthrowsunsafe) — эффекты как
  альтернатива keyword'ам.
- [D11](#d11-имена-эффектов-и-синтаксис-with) — имена эффектов как обычные
  типы, three positions.
- [02-types.md → D17](02-types.md#d17) — параметры без `:`.
- [03-syntax.md → D30](03-syntax.md#d30) — PascalCase правило для типов.

### Эволюция

В первых черновиках синтаксис был `fn save(u: User) !throws io async` —
маркер `!` + lowercase эффекты + параметры с `:`. Каждая из трёх
особенностей пересмотрена отдельно:

- `!` отброшен (визуальный конфликт с «not») — этот D3.
- Lowercase → PascalCase в [D11](#d11-имена-эффектов-и-синтаксис-with).
- `:` в параметрах → `u User` в [02-types.md → D17](02-types.md#d17).

**Главный урок.** Символьная пунктуация дёшева на одном месте и
дорожает экспоненциально с количеством мест. Слова и структурные
границы (`)` ... `->`) масштабируются линейно.

---

## D4. `?` для пробрасывания ошибки

> **🚫 RETRACTED / SUPERSEDED by [D85](#d85-операторы--и--унифицированное-поведение-для-result-и-option-throw-стиль-через-)** (2026-05-10, enforcement Plan 173 Ф.1 #3 2026-07-04).
> Тело ниже описывает УСТАРЕВШУЮ throw-семантику `?` («работает только в функциях с `Fail[E]`»).
> **Актуальный канон:** `?` — **return-only** (только на `Result`/`Option`, проброс значением);
> в Fail-эффект-функциях `?` запрещён → `[E_TRY_IN_FAIL_FN]`, там `!!`/`throw`. Единственные
> исключения — consume-init `?` (D196 form 2) и `?` в defer-body (D158). Оставлено как historical.

### Что
Постфиксный оператор `?` после выражения — «если ошибка, верни её
выше». Работает только в функциях с эффектом `Fail[E]` в сигнатуре.

### Правило

```nova
fn pipeline(s str) Fail[ParseError] -> int {
    ro n = parse(s)?         // если parse бросил — pipeline бросает то же
    validate(n)?               // если validate бросил — pipeline бросает то же
    n
}
```

Без `Fail` в сигнатуре `?` — ошибка компиляции:

```nova
fn pipeline(s str) -> int {
    ro n = parse(s)?           // ОШИБКА: ? requires effect Fail[E]
    n
}
```

### Семантика — сахар над `match` + `throw`

`expr?` компилятор разворачивает в:

```nova
match expr {
    Ok(v)  => v
    Err(e) => throw e         // обычный throw, требует Fail[E]
}
```

Поэтому `?` работает только в функциях с `Fail[E]` — не специальное
правило компилятора, а следствие того, что `throw` сам требует эффект
([D25](#d25-throw-и-параметризация-throwse)).

### Совместимость с `Fail[E]`

`!!` пробрасывает ошибку **наверх** через `Fail` — тип ошибки в
сигнатуре вызывающего должен быть совместим. Если совпадают — проходит
напрямую; если разные — нужно явное преобразование через `.map_err()`:

```nova
fn pipeline(s str) Fail[PipelineError] -> int {
    ro n = parse(s).map_err(|e| PipelineError.Parse(e))!!
    validate(n).map_err(|e| PipelineError.Validate(e))!!
    n
}
```

### Почему

1. **Заимствовано из Rust/Swift**, проверено годами использования.
2. **Дешевле** `try { ... } catch { ... }`. **Безопаснее** `if err != nil` —
   нельзя забыть проверку.
3. **Не магия.** Полностью разворачивается в существующие конструкции
   языка (`match`, `throw`) — никаких специальных правил.

### Что отвергнуто

- **`try expr`** (Swift-style). Слово длиннее, а `?` уже знаком всем,
  кто видел Rust/Swift.
- **`expr!`** для force-unwrap. Конфликтует с логическим «не», и
  panic-семантика противоречит [08-runtime.md → D13](08-runtime.md#d13)
  (panic не ловится в коде).
- **`?` без `Fail` в сигнатуре** (с автоматическим выводом). Нарушает
  правило «public-API явный» ([D28](#d28-вывод-эффектов-private--выводится-public--обязательно-явно)).
  В private может работать через вывод, но даже там удобнее видеть
  `Fail` явно.

### Связь

- [D25](#d25-throw-и-параметризация-throwse) — `throw` как операция
  эффекта `Fail[E]`, `?` разворачивается в `throw`.
- [D2](#d2-эффекты-вместо-ключевых-слов-asyncthrowsunsafe),
  [D11](#d11-имена-эффектов-и-синтаксис-with) — `Fail` как обычный
  эффект.
- [03-syntax.md → D19](03-syntax.md#d19) — `match` со стрелкой `=>`
  (используется в desugaring `?`).

> **Coalesce `??` вынесен в [D86](#d86--coalesce-оператор-fallback-для-resultoption).** Раньше
> описывался подразделом D4; в 2026-05-10 выделен в самостоятельное
> решение для возможности независимой эволюции и явных ссылок.

### Эволюция

В первой формулировке D4 в исходниках указано «работает в функциях
с эффектом `throws`» (lowercase). Отметка устарела: эффекты в Nova —
PascalCase, правильное имя — `Fail[E]` (раньше `Throws[E]` —
переименование в [D61](#d61-полная-семантика-эффектов-effect-keyword-handler-литерал-handlere-interrupt)
ради согласованности convention «имя эффекта — существительное в
единственном числе», `Throws` был глаголом, остальные эффекты —
существительные).

---

## D11. Имена эффектов и синтаксис `with`

> ⚠️ **REVISED → [D61](#d61)**. Эффект объявляется через keyword `effect`
> (`type X effect { ... }`), не через `protocol`. Handler-литерал —
> через keyword `handler` (`effect X { ops }`), а не через `X { ops }`.
> Раздел оставлен для семантики `with`-блока (без изменений). Старая
> формулировка про «protocol-форму» устарела.

### Что
Эффект объявляется через keyword `effect` (см. [D61](#d61)), а handler-литерал —
через keyword `handler`. Имена в PascalCase. Синтаксис `with` принимает
либо имя handler-переменной, либо подмену вида `EffectName = expr`
через запятую, и ровно **один** блок тела.

### Правило

#### Объявление эффекта

```nova
type Logger effect {
    log(msg str) -> ()
}

type Db effect {
    query(q Sql) -> []DbRow
    exec(q Sql)  -> ()
}
```

Имя эффекта — обычный идентификатор в PascalCase. Объявление через
keyword `effect` (см. [D61](#d61), [D18 (REVISED)](#d18-эффекты-объявляются-через-kind-токен-не-голый-type)).

#### Имя эффекта в коде — three positions

Имя эффекта в коде может появляться в **трёх позициях**, каждая
разрешается контекстом:

```nova
// 1. ПОЗИЦИЯ ТИПА — между ) и -> (или в generic-параметре)
fn process(o Order) Db -> Receipt => ...
//                  ^^ Db — имя типа эффекта

// 2. ПОЗИЦИЯ ОПЕРАЦИИ — Db.X(...) — обращение к операции активного handler'а
Db.query(sql`select * from users`)

// 3. ПОЗИЦИЯ ВЫРАЖЕНИЯ — одиночное Db в выражении
ro captured_db = Db          // активный handler как значение Effect[Db]
some_function(Db)
return Db
```

Парсер различает по позиции, никакой неоднозначности нет. Никакого
`Db.current()` или подобного геттера не существует — просто `Db` в
выражении. Это симметрично тому, как `User` в выражении не нуждается
в `User.current()`.

#### Форма 1: подмена через `EffectName = expr`

Основной случай — тесты, переключение реализации:

```nova
with Logger = console_logger, Db = in_memory, Time = fixed(t0) {
    process_order(o)
}
```

После `with` — список «эффект = handler-выражение» через запятую,
потом **один** `{ body }`. Парсер однозначен: запятые разделяют
подмены, `{` открывает тело.

#### Форма 2: handler как обычное значение

Для сложных или переиспользуемых handler'ов:

```nova
ro audit = effect Logger {
    log(msg) { audit_db.write(msg); return () }
}

with Logger = audit {
    critical_operation()
}
```

`EffectName { ... }` — выражение-литерал, дающее значение типа
`Effect[EffectName]`. Параллель с record-литералами: разные keyword'ы,
разные формы литералов:

```nova
type User { id u64, name str }                              // record-тип (data)
ro alice = User { id: 1, name: "alice" }                   // record-литерал

type Logger effect { log(msg str) -> () }                  // эффект (behavior)
ro console = effect Logger { log(msg) => println(msg) }  // handler-литерал
```

Handler-литерал начинается с keyword'а `handler` (по [D61](#d61)) —
однозначно отличает от record-литерала. Стрелка в handler-операциях —
именно `=>`, как в match-arms ([03-syntax.md → D19](03-syntax.md#d19))
и теле лямбды ([03-syntax.md → D22](03-syntax.md#d22)).

#### Слово `handler` — keyword (D61)

В первой редакции D11 использовался синтаксис без префикса —
`Logger { log(msg) => ... }`, парсер различал record vs handler по
содержимому `{...}`. После [D61](#d61) `handler` стало keyword'ом,
а handler-литерал требует явного префикса. Это улучшает локальную
читаемость: `effect X {...}` сразу читается как «литерал handler'а».

### Почему

1. **Один блок тела `with`** — нет визуальной путаницы между телом
   handler'а и телом `with`-блока.
2. **Несколько эффектов в одном `with`** — естественно и компактно
   для тестов:
   ```nova
   with Logger = test_log, Time = fixed_clock, Random = seeded(42) {
       run_simulation()
   }
   ```
3. **Handler — обычное значение**, не специальная синтаксическая
   форма, привязанная к `with`. Это упрощает композицию — handler'ы
   можно хранить в переменных, передавать функциям, держать в map.
4. **Симметрия с record-литералами** — `Имя { ... }` для значений
   любых типов, без специальных префиксов.
5. **`with` остаётся примитивом языка**, а не сахаром над функцией —
   потому что он структурно влияет на стек handler'ов (continuation
   capture).

### Что отвергнуто

- **`with effect Logger { log(msg) => ... } { body }`.** Два `{...}`
  блока подряд читаются плохо: непонятно, где кончается тело
  handler'а и начинается тело `with`.
- **`handler EffectName = ...`** keyword. Префикс лишний — содержимое
  блока (`name(args) => body`) однозначно говорит, что это handler.
- **Lowercase имена эффектов** (`throws`, `io`). Эффекты — обычные типы,
  применяется единое PascalCase-правило ([03-syntax.md → D30](03-syntax.md#d30)).
- **`Db.current()` геттер** для активного handler'а. Лишний синтаксис —
  имя эффекта в выражении и так даёт активный handler.

### Связь

- [D2](#d2-эффекты-вместо-ключевых-слов-asyncthrowsunsafe),
  [D3](#d3-синтаксис-эффектов-типы-между--и--) — эффекты как типы.
- [D18](#d18-эффекты-объявляются-через-kind-токен-не-голый-type) — эффект
  объявляется через `protocol`; литералы различаются по содержимому.
- [D25](#d25-throw-и-параметризация-throwse) — `Fail[E]` — частный
  случай этой схемы.
- [D31](#d31-handler-лямбда-для-эффектов-с-одной-операцией) —
  handler-лямбда (третья форма для эффектов с одной операцией).
- [02-types.md → D42](02-types.md#d42) — `protocol` как структурный
  контракт; эффекты — это `protocol`, использованный в позиции эффекта.
- [03-syntax.md → D19](03-syntax.md#d19),
  [03-syntax.md → D22](03-syntax.md#d22) — стрелка `=>` в match-arms
  и теле лямбды (та же стрелка в handler-операциях).
- [03-syntax.md → D30](03-syntax.md#d30) — PascalCase для типов.

### Эволюция

Ранние черновики содержали `with effect EffectName { ... } { body }` —
два `{...}` подряд и обязательный префикс `handler`. Пересмотрено на
форму без префикса с handler-литералом `EffectName { op() => ... }` и
явную форму подмены `EffectName = expr`. Lowercase имена эффектов
(`throws`, `io`) отброшены в пользу PascalCase.

### Q-note (Plan 174.4) — размер effect-handler-registry вычисляется на компиляции

Наследование handler'ов через фиберы (per-fiber snapshot save/restore, Plan
83.10.4 Ф.3) держится на таблице зарегистрированных handler-storage адресов
(`NovaEffectRegistry` / `NovaEffectSnapshot`, `nova_rt/effects.h`). Её размер
**`NOVA_MAX_EFFECT_STORAGES`** больше не хардкод-32, а **compile-time N** — точное
число distinct-эффектов в программе (built-in `Fail`/`Time`/`Mem` + user-defined),
которое компилятор знает из реестра `effect_schemas`. Проброс N (ФАКТИЧЕСКИЙ
механизм, НЕ `#define` в теле `.c`): codegen эмитит на строке 1 сгенерированного
`.c` comment-МАРКЕР `/* nova-effect-count: N */`; build-слой
(`test_runner.rs::effect_count_define_arg`) читает N из маркера и передаёт
`-DNOVA_MAX_EFFECT_STORAGES=N` (`/D` для MSVC) на **весь** cc-вызов — во все TU
разом. Почему НЕ `#define` внутри самого `.c`: генерируемый TU и рантайм-TU
(`effects.c`/`runtime.c`/`fibers.c`) компилируются как **отдельные** translation
units в одном cc-вызове — `#define` только в `.c` дал бы `NovaEffectRegistry`/
`NovaEffectSnapshot` разного размера в разных TU → OOB-запись в TLS-registry →
segfault (тот самый ABI-раскол, что реализация **сознательно отвергла**). `-D` на
весь вызов держит размер массива идентичным во всех TU (ABI-uniformity); хедерный
`#ifndef`-fallback 32 остаётся только для hand-written bootstrap-кода без маркера.
Следствия: (1) прежний тихий дроп 33-го эффекта — при котором
handler молча не наследовался через фибер — устранён по построению (размер = точный
N, переполнение теперь — hard-fail с диагностикой, т.е. индикатор бага codegen'а);
(2) per-fiber snapshot занимает ровно N указателей, без фиксированных 256 байт на
каждый фибер. Это внутренняя codegen/runtime-деталь (не влияет на язык), поэтому
отдельного D-блока нет — только эта заметка. Follow-up `[M-174.4-effect-registry-size]`
Ф.2 (статические индексы эффектов, удаление рантайм-registry) — отдельным заходом.

---

## D12. Effect erasure и dynamic effects

### Что
Статическая типизация эффектов — **дефолт**: очереди, каналы, планировщики
типизированы по эффектам функций, которые они принимают. Для разнородных
задач, плагинов и сериализации есть **явные** инструменты стирания
эффектов и динамики.

### Правило

#### Уровень 1 — статически типизированный планировщик (дефолт)

```nova
ro order_queue Queue[fn(OrderId) Db Log Fail -> ()]

order_queue.enqueue(send_order_confirmation)        // ок
order_queue.enqueue(cleanup_db_task)                 // ОШИБКА: лишний эффект Net
```

Воркер этой очереди статически проверен. Лишний эффект не пройдёт.
Это правильный дефолт для типизированных пайплайнов.

#### Уровень 2 — явное стирание через `erase[E]`

```nova
fn erase[E](task fn() E -> ()) E -> fn() -> () =>
    ro captured = capture_handlers[E]()
    || with captured { task() }

universal_queue.enqueue(erase(send_email_task))
universal_queue.enqueue(erase(cleanup_db_task))
```

Эффекты захвачены в момент `erase`, тип задачи становится `fn() -> ()`,
очередь принимает разнородные задачи. Цена: handler'ы зашиты, если
они стали невалидными к моменту исполнения — это проблема программиста,
не компилятора.

#### Уровень 3 — динамические эффекты через `EffectSet` + `DynFn`

Runtime-структура `EffectSet`, тип `DynFn` для случаев, когда
эффекты задачи известны только в рантайме (плагины, сериализация
в БД). Используется редко, помечается явно.

### Что НЕ делается

- **Стирание не автоматическое** — иначе строгая типизация превращается
  в видимость (как Java generic erasure). Программист должен явно
  попросить `erase[E]`.
- **Все очереди не делаются динамическими по умолчанию** — потеряется
  главное свойство Nova (видимость эффектов в типе).
- **Через границу процесса handler'ы не передаются.** Эффекты на этой
  границе становятся протоколом (имена сервисов, типов сообщений) —
  это паттерн «commands + dispatcher», не часть системы эффектов.

### Почему

1. **Правильный дефолт.** 95% случаев — типизированные пайплайны, для
   них эффекты в типе очереди — гарантия безопасности.
2. **Эскейп-хатч есть, но виден.** `erase[E]` или `DynFn` — явные
   маркеры в коде, понятные при ревью. Компилятор не трогает остальные
   места.
3. **AI-first.** LLM, генерируя код, видит явный `erase` — понимает,
   что в этой точке статическая безопасность кончается.

### Что отвергнуто

- **Автоматическое стирание (Java-style generic erasure).** Превращает
  типизацию в видимость — лишает Nova главного свойства.
- **Все очереди динамические.** Каждый `enqueue` тогда требует runtime-
  проверки эффектов; типизация в сигнатуре теряет смысл.
- **Эффекты как часть protocol-message** через сеть. Handler'ы — это
  closures с capture, по проводу не передаются. Через границу процесса —
  обычный паттерн «команды + диспатчер».

### Связь

- [D2](#d2-эффекты-вместо-ключевых-слов-asyncthrowsunsafe) —
  типизация эффектов в сигнатуре.
- [D11](#d11-имена-эффектов-и-синтаксис-with) — handler как обычное
  значение, что позволяет capture через `erase`.
- [06-concurrency.md → D14](06-concurrency.md#d14) — fiber runtime,
  планировщик задач.

### Открытые вопросы

- Конкретный синтаксис `capture_handlers[E]()` (имя, форма параметра).
- Семантика `EffectSet` в рантайме (теги типов? vtable?).
- Граничные случаи: эффект выходит за scope, handler уже невалиден к
  моменту исполнения — как сигналить ошибку.

---

## D18. Эффекты объявляются через kind-токен, не голый `type`

> ⚠️ **REVISED → [D53](02-types.md#d53), [D61](#d61), [D62](#d62).**
> Финальный синтаксис для эффектов: `type X effect { ... }`. `effect` —
> kind-токен (по [D53](02-types.md#d53)) И keyword (по [D61](#d61)).
> Структурные контракты остаются как `type X protocol { ... }` (см.
> [D62](#d62) правило `effect`/`protocol`: program-based выбор по двум
> sniff-вопросам). Различие: effects поддерживают `with`-substitution
> и continuation-capture, protocols — нет.

### Правило

#### Чёткое разделение `type` vs `type X effect` vs `type X protocol`

```nova
// data — голый type (см. D52)
type User { id u64, name str }
type Color enum Red | Green | Blue
type UserId u64

// эффекты (with-substitution + continuation-capture) — kind-токен effect
type Db effect {
    query(q Sql) -> []DbRow
    exec(q Sql)  -> ()
}

type Logger effect {
    log(msg str) -> ()
}

// структурные контракты (без with-substitution) — kind-токен protocol
type Hash protocol {
    hash() -> u64
    eq(other Self) -> bool
}
```

Выбор `effect` vs `protocol` — программистский (D62 правило 4):
- with-substitution нужна (mock в тестах)? — `effect`
- continuation-capture нужен (throw, interrupt)? — `effect`
- Оба «нет» — `protocol`

`type X { методы без полей }` запрещено — нужно `type X effect { ... }`
или `type X protocol { ... }`. Слова `effect` и `handler` зарезервированы
как keyword'ы; `protocol` — kind-токен (не зарезервирован как keyword
вне type-decl).

#### Один `protocol` — две роли по контексту использования

Тот же protocol может работать и как эффект, и как структурный
параметр. Различение идёт **по позиции в сигнатуре**:

```nova
type Logger effect { log(msg str) -> () }

// А: позиция эффекта — между ) и ->. Активный handler берётся из скоупа.
fn process_a(o Order) Logger -> () =>
    Logger.log("processing")

// Б: позиция типа значения — обычный параметр, передаётся явно.
fn process_b(o Order, logger Logger) -> () =>
    logger.log("processing")
```

Программист выбирает стиль:

- **Эффект** (А) — для пронизывающих контекстов: БД, лог, аутентификация,
  трассировка. Не таскается через 10 функций.
- **Параметр** (Б) — для явных зависимостей одной функции, когда
  хочется локальной видимости.

#### Что осталось без изменений

- **Имя protocol'а в позиции эффекта** (между `)` и `->`) — требование
  активного handler'а в скоупе.
- **`Db.operation(args)`** — вызов операции активного handler'а.
- **`Db` в позиции выражения** — активный handler как значение
  ([D11](#d11-имена-эффектов-и-синтаксис-with)).
- **`with Db = expr { body }`** — подмена handler'а в скоупе.
- **Литерал handler'а** — `effect Db { query(s, a) => ..., exec(s, a) => ... }`
  (через keyword `handler`, см. [D61](#d61)).

Handler-литерал начинается с keyword `handler` — это однозначно
отличает его от record-литерала. До [D61](#d61) парсер различал по
содержимому `{...}` (двоеточие vs стрелка); теперь — по prefix'у
keyword'а.

#### Различение литералов

- `Type { name: value }` → record-литерал у `type`
  (`User { id: 1 }`)
- `effect Type { name(args) => body }` → handler-литерал у effect'а
  (`effect Db { query(s, a) => ... }`)

Стрелка handler-операций — `=>`, та же что в match-arms
([03-syntax.md → D19](03-syntax.md#d19)) и теле лямбды
([03-syntax.md → D22](03-syntax.md#d22)). Не `->`.

### Почему

1. **`type` для данных, `protocol` для поведения** — единое правило
   языка ([02-types.md → D42](02-types.md#d42)). Эффект — это поведение
   (набор операций без полей), и логично, чтобы он использовал тот же
   keyword, что и обычные структурные контракты.
2. **Намерение явно по первому токену.** Раньше требовалось смотреть
   на содержимое `{...}` (поля или методы), чтобы понять, что
   объявлено. Теперь с keyword видно сразу.
3. **Меньше двусмысленности у LLM.** В предыдущей редакции D18 LLM
   нужно было запоминать «type с одними методами — это контракт/эффект».
   Сейчас правило прямее: «методы → `protocol`».
4. **Согласованность с D42.** D42 разделил данные и поведение, но
   эффекты выпадали из правила (объявлялись через `type`). Этот
   разворот D18 убирает противоречие.

### Что отвергнуто

- **`effect X { ... }` keyword** (как в первоначальной редакции). Не
  возвращаем — третий keyword рядом с `type`/`protocol` плодит сущности
  без выгоды. `protocol` уже описывает «именованный набор операций»;
  эффект — это `protocol`, использованный в позиции эффекта.
- **`handler X = ...` keyword.** Префикс лишний — содержимое
  `X { op(args) => body }` однозначно говорит, что это handler.
- **Сохранить `type` для эффектов** (как в предыдущей редакции D18).
  Конфликтует с D42: D42 говорит «`type` — данные, `protocol` —
  поведение», а эффект — это поведение. Оставлять эффекты под `type`
  — это ровно то противоречие, которое этот разворот D18 устраняет.

### Цена

1. **Breaking change для всех ранее написанных примеров эффектов.**
   Все `type Db { query, exec }` → `protocol Db { query, exec }`.
   Поскольку реализации компилятора нет, цена — обновление спецификации
   и примеров.
2. **Семантическая зависимость в парсинге литералов сохраняется.**
   Парсер всё ещё смотрит на содержимое `{...}` (двоеточие vs стрелка),
   чтобы различить record-литерал и handler-литерал. Но keyword
   `protocol` явно говорит, что у этого имени литерал — handler-форма.
3. **Anonymous structural type в позиции эффекта** —
   `fn f(x { show() -> str })` сейчас валиден как анонимный protocol
   в позиции параметра ([D42:200-203](02-types.md#d42)).
   Допустим ли он в **позиции эффекта** между `)` и `->`? — open
   question, см. [open-questions.md](../open-questions.md).

### Связь

- [D2](#d2-эффекты-вместо-ключевых-слов-asyncthrowsunsafe) — эффекты как
  protocol'ы, не keyword'ы `async`/`throws`/`unsafe`.
- [D11](#d11-имена-эффектов-и-синтаксис-with) — three positions имени
  эффекта; `with`-синтаксис для подмены.
- [02-types.md → D17](02-types.md#d17) — единый синтаксис объявления
  `type` (для данных).
- [02-types.md → D42](02-types.md#d42) — `protocol` keyword;
  эффекты — частный случай `protocol`, использованного в позиции эффекта.
- [03-syntax.md → D19](03-syntax.md#d19) — стрелка `=>` в match-arms,
  та же что в handler-литералах.

### Эволюция

История развода в три шага:

1. **Первая редакция** — два keyword'а: `effect X { ... }` для эффектов,
   `type X { ... }` для всего остального.
2. **Вторая редакция** — `effect` отменён, эффекты объявляются через
   `type`. Различение по контексту использования. Этот шаг убрал лишний
   keyword, но оставил `type` перегруженным (и данные, и поведение).
3. **Текущая редакция** — после D42, который разделил `type` (данные)
   и `protocol` (поведение), эффекты переведены на `protocol`.
   `type` теперь только для данных. Это устраняет противоречие
   между D18 и D42.

В одном из ранних черновиков D18 пример handler-литерала был записан
со стрелкой `->` (`Db { query(s, a) -> return ... }`) — устарело.
Стрелка `=>` — единое правило для всех мест, где «образец/параметры →
тело» ([03-syntax.md → D19](03-syntax.md#d19),
[03-syntax.md → D22](03-syntax.md#d22)).

---

## D25. `throw` и параметризация `Fail[E]`

> **Уточнено [D65](#d65-полная-семантика-fail-гибрид-faile--fail-lookup-prelude-runtimeerror-и-error):**
> `Fail` без параметра — сахар над `Fail[any]` (universal), не
> `Fail[Error]`. Lookup-правило, re-throw, prelude-типы `RuntimeError`
> и `Error` (record) формально определены в D65. Этот блок (D25)
> сохраняется как описание **базового механизма** `throw` и `Fail[E]`;
> полная семантика — в D65.

### Что
Бросать ошибку — выражение `throw expr`, прерывающее функцию через
эффект `Fail[E]`. Параметр `E` — тип бросаемого значения, обычно
sum-type. `Fail` без параметра — сахар над `Fail[any]` (universal,
catch-all). Convention: для public API использовать `Fail[E]` с
конкретным типом; для quick-and-dirty/scripts/internal helper'ов —
`Fail` (any) допустим. См. [D65](#d65-полная-семантика-fail-гибрид-faile--fail-lookup-prelude-runtimeerror-и-error).

### Правило

#### Базовое использование

```nova
type DepositError enum Closed | NotPositive | OverLimit

fn deposit(mut acc Account, amount money) Fail[DepositError] -> () =>
    if acc.closed   { throw Closed }
    if amount <= 0  { throw NotPositive }
    acc.balance += amount
```

`throw expr` — выражение типа `never` (никогда не возвращает), прерывает
функцию и передаёт `expr` через эффект `Fail[E]`. Тип `expr` должен
совпадать с `E` в сигнатуре.

**Bootstrap (2026-05-06):** `throw` парсится и как statement, и как
expression. В expression-position (match-arm body, ternary, аргумент
функции) codegen эмитирует `Nova_Fail_fail(msg)` + dummy `((nova_int)0LL)`
— dummy после fail() недостижим. Тесты — `nova_tests/effects/throws.nv` (stmt),
`nova_tests/syntax/throw_in_expression.nv` (expr).

**Plan 125 (2026-06-05) — divergence-aware result-type inference:**
паттерн `if cond { throw } else { val }` теперь полноценно поддержан.
Codegen эмитирует `_nv_if : type-of-else`, divergent then-ветка не
участвует в join-типе. Whitelist (Ф.1-Ф.4):
- `ExprKind::Throw` (Ф.1) — direct `throw expr`
- `ExprKind::Interrupt` (Ф.3) — `interrupt val` внутри handler-literal
- `Call(panic, ...)` / `Call(exit, ...)` (Ф.2) — prelude builtin'ы
- `Call(f, ...)` где `f` объявлена `-> never` (Ф.3) — direct call only
- Recursive composition (Ф.4): if/if-let/match/block, у которых все
  ветви diverge

Реализация **codegen-local**, trailing-only — последняя позиция в
блоке (`b.trailing` или `b.stmts.last()` если trailing отсутствует и
last-stmt = `Stmt::Throw`/`Stmt::Return`/`Stmt::Expr(...)`). Helper
**НЕ** переиспользует `block_diverges` из type-checker'а (root cause
прошлой попытки 2026-06-03 — он walked stmts и flip'ил легитимный
idiom `if early-cond { return X } else { compute() }`).

Type-checker side (`Ty::Never` first-class subtype) — отдельный
follow-up `[M-125-type-checker-never-first-class]`; codegen V1
production-ready без него.

**Plan 125.1 (2026-06-05) — type-checker `Ty::Never` first-class:**
`[M-125-type-checker-never-first-class]` ✅ CLOSED. Дополнено
codegen-fix настоящим type-side first-class subtype rule
(`compiler-codegen/src/types/mod.rs`):
- Ф.1 — `assignable()` hookpoint: `if matches!(ty_of_ref(&found_tr),
  Ty::Never) { return Compat::Ok }` — pure additive,
  `TyCat::Other` safety-net preserved
- Ф.2 — `infer_expr_type` propagates `never` для `ExprKind::Throw` /
  `ExprKind::Interrupt` / `Call(panic|exit|abort|unreachable, ...)` +
  user fn'ов с return type `Ty::Never` (all-overloads-divergent guard)
- Ф.3 — `infer_block_trailing_typeref` возвращает `Some(prim_ref("never"))`
  когда trailing diverges (top-level shape: Throw/Interrupt/never-call);
  conservative — не walks preceding stmts
- Ф.4 — `detect_divergent_consumable` (D196 form 3) использует
  `block_diverges` для early-skip обеих веток вместо `?`-propagation
  abort; ЛЮБОЙ divergent путь → SKIP (None)

Test coverage: `nova_tests/plan125_1/` — 12 positive + 3 negative
фикстуры; full plan125 (22) + plan125_followups (9) baseline preserved.

#### `throw` — операция эффекта `Fail[E]`, не магия

Связь между `throw` и `Fail[E]` — **не специальная проверка
компилятора**, а прямое применение модели алгебраических эффектов.
`throw expr` — это **операция эффекта**, точно так же как `Db.query(...)`
или `Logger.log(...)`.

Концептуально prelude объявляет:

```nova
type Fail[E] effect {
    fail(value E) -> never        // операция, никогда не возвращает
}
```

`throw expr` — сахар для `Fail[E].fail(expr)`. Компилятор разворачивает
синтаксический `throw` в обычный вызов операции эффекта. Дальше работает
**общее правило для всех эффектов**: использовал операцию — задекларируй
эффект в сигнатуре.

```nova
fn lookup(id u64) Db -> User =>           // Db в сигнатуре — ок
    Db.query(sql`SELECT * FROM users WHERE id = ${id}`)

fn lookup(id u64) -> User =>               // Db отсутствует — ошибка
    Db.query(sql`SELECT * FROM users WHERE id = ${id}`)
//  ^^^^^^^^^^^^^^^^^^^^ operation Db.query requires effect Db

fn parse(s str) Fail[ParseError] -> int =>    // Fail в сигнатуре — ок
    throw ParseError.BadFormat

fn parse(s str) -> int =>                       // Fail отсутствует — ошибка
    throw ParseError.BadFormat
//  ^^^^^^^^^^^^^^^^^^^^^^^^^^ throw requires effect Fail[ParseError]
```

Никакой отдельной логики для `throw` нет. Та же проверка, что для
`Db.query`, `Net.get`, `Time.now` и любой другой операции эффекта.

#### `?` — сахар над `match` + `throw`

> **🚫 SUPERSEDED by [D85](#d85-операторы--и--унифицированное-поведение-для-result-и-option-throw-стиль-через-)** (throw-семантика `?` устарела; enforcement Plan 173 Ф.1 #3). Актуально: `?` = return-only (`match { Ok(v)=>v, Err(e)=>return Err(e) }`), throw-стиль — через `!!`. Ниже — historical.

`?` тоже не магия. `expr?` разворачивается в:

```nova
match expr {
    Ok(v)  => v
    Err(e) => throw e            // обычный throw, требует Fail[E]
}
```

Поэтому `?` работает только в функциях с `Fail[E]` в сигнатуре —
потому что **раскрывается в `throw`**, а `throw` требует эффект.
Отдельного правила «`?` требует `Fail`» нет, оно вытекает из
обычной проверки эффектов ([D4](#d4--для-пробрасывания-ошибки)).

#### `never` — почему `throw` совместим с любым типом

`throw expr` имеет тип `never` — тип, означающий «не возвращает
значение в обычном смысле». `never` — подтип любого типа (как `Nothing`
в Kotlin/Scala), поэтому `throw` можно использовать как выражение в
любой позиции:

```nova
ro x int = if condition { 42 } else { throw NotReady }
//                                     ^^^^^^^^^^^^^^^
//                                     тип never, совместим с int
```

Это работа `never`, а не специальное правило для `throw`. То же
поведение у `return` и `panic` — все три имеют тип `never`. Поэтому
работают и такие выражения:

```nova
ro user = lookup(id) ?? return Response.error(404)
//                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^
//                       тип never, совместим с типом user
```

#### Поймать через `?` (проброс) или handler (обработка)

**Проброс через `?`** — сахар «если ошибка, верни её выше». Работает
в функциях с совместимым `Fail[E]` в сигнатуре:

```nova
fn pipeline(s str) Fail[ParseError] -> int =>
    ro n = parse(s)?              // если parse бросил — pipeline тоже бросает
    n
```

**Обработка через handler** — обычный handler-блок. Для `Fail[E]`
основная форма — handler-лямбда ([D31](#d31-handler-лямбда-для-эффектов-с-одной-операцией)),
потому что у `Fail[E]` ровно одна операция (`throw`):

```nova
fn try_deposit(acc Account, amount money) Log -> bool {
    // handler-лямбда (D31) — это лямбда (`=> expr`), без блок-формы
    // (D22). Когда нужен блок с side-effect'ами — используем полный
    // handler-литерал в блок-форме `op(p) { block }`.
    with Fail[DepositError] = effect Fail[DepositError] {
        fail(err) {
            Log.error("deposit failed: ${err}")
            interrupt false
        }
    } {
        deposit(acc, amount)
        true
    }
}
```

Тип результата `with`-блока — общий тип всех веток (тело и handler'ы).
Здесь оба возвращают `bool`. Если разнотипные — обернуть в sum-type/
`Result`/`Option`.

#### Две формы handler'а для `Fail[E]`

Операция `Fail[E].fail` имеет тип возврата `never` — она по
определению **не возвращает** значение в точку вызова. Из этого
следует, что у handler'а `Fail[E]` всего **два** допустимых
исхода:

1. **`interrupt v`** → прерывание (`with`-блок возвращает `v`).
   Аналог `try/catch` в Java. Continuation отбрасывается.
2. **Новый `throw`** → проброс наверх (как другой тип или с
   обогащением контекста). Управление ищет следующий handler в
   стеке.

Третья форма — `return value` / финальное выражение, которая для
других эффектов даёт «продолжение с подменой» (управление
возвращается в точку вызова операции с подменённым значением), —
для `Fail` **запрещена**. Тип операции `never` означает, что в
точке `throw` некуда возвращать значение; type-checker должен это
запрещать (см. [D61](#d61) «never-операции», [Q-resume](../open-questions.md#q-resume)).

#### Один параметр в `Fail[E]`

Параметр **один**. Если функция бросает несколько типов — программист
делает sum-type:

```nova
type TransferError enum InsufficientFunds | InvalidAccount | AccountClosed

fn transfer(from Account, to Account, amount money) Fail[TransferError] Db -> Receipt => ...
```

#### `Fail` без параметра — catch-all (D65)

```nova
fn process(o Order) Fail Db {                    // Fail ≡ Fail[any]
    validate(o)
    save(o)
}

export fn process_pub(o Order) Fail[OrderError] Db -> () => ...
```

Правило (по [D65](#d65)):

- `Fail` без `[E]` ≡ **`Fail[any]`** (top-type, ловит любую ошибку).
- В **приватных функциях** допустим как quick-and-dirty.
- В **публичных функциях** допустим, но **рекомендуется** явный
  `Fail[E]` — это часть контракта. Линтер может предупреждать.

Это **AI-first компромисс**: внутри модуля программист может писать
быстро, не придумывая имя ошибки. На границе модуля LLM (и человек)
видят конкретный тип в сигнатуре.

> ⚠️ **Изменение в D65.** Раньше `Fail` ≡ `Fail[Error]` (конкретный
> record-тип). После [D65](#d65) `Fail` ≡ `Fail[any]` (top-type).
> Семантика catch-all сохранилась, тип-обёртка `Error` остался в
> prelude как удобный record для `throw` (см. D26).

#### Связь с `Result[T, E]`

`Fail[E]` и `Result[T, E]` — **разные инструменты с пересечением
сценариев**.

```nova
fn parse_a(s str) Fail[ParseError] -> int => ...    // эффект-стиль
fn parse_b(s str) -> Result[int, ParseError] => ...   // value-стиль
```

#### Когда `Fail[E]`

- Прикладной код, где ошибка пробрасывается до handler'а (через `?`).
- Effect-композиция: handler `Fail[E]` ловится через `with`,
  retry/log/централизованная обработка.
- Несколько функций цепочкой выбрасывают одну и ту же ошибку —
  чтение становится линейным (`x()? .map(f)?` без обёрток).

#### Когда `Result[T, E]`

- Значение, которое **нужно проинспектировать** прямо у вызова
  (`match result { Ok(v) => ..., Err(e) => ... }`).
- API, где обработка ошибки ВСЕГДА происходит локально (не пробрасывается).
- Возвращаемое значение функции, которая **сама по себе не ошибка**
  (например, `try_parse` возвращает `Result` намеренно).

#### Конвертация

Из `Fail[E]` в `Result[T, E]`:

```nova
ro r = with Fail[ParseError] = |e| interrupt Err(e) {
    Ok(parse(s)?)
}
// r: Result[int, ParseError]
```

Из `Result[T, E]` в `Fail[E]`:

```nova
ro v = parse(s)?              // если Result, ? = match Ok(v) => v / Err(e) => throw e
```

Оператор `?` работает на обоих типах ([D26](08-runtime.md#d26)).

#### Дефолт и AI-first

**Дефолт — `Fail[E]`.** Эффект-стиль читается линейно, лучше для
LLM-генерации (нет вложенных match'ей). `Result` — когда сценарий
«ошибка как значение, всегда обработать локально».

«Два пути для одного» — кажущееся: пути решают разные задачи. Это
не нарушает D40, потому что выбор детерминирован сценарием, не
вкусом.

#### `throw` ≠ `panic`

`throw expr` — обычная ошибка через эффект, **видна в сигнатуре** через
`Fail[E]`. Перехватывается handler'ом в коде.

`panic` ([08-runtime.md → D13](08-runtime.md#d13)) — аппаратные/
математические сбои (деление на 0, переполнение, OOM, выход за границы
массива) или вызов `panic(msg)` программистом. **Не виден в сигнатуре**.
**Не ловится в коде** — означает смерть текущего fiber'а, ловится
только runtime'ом на границе fiber'а.

Это разные миры:

- «обработать можно и нужно» → `throw` + `Fail[E]`
- «обработать никак нельзя, fiber умирает» → `panic`

### Почему

1. **`throw` — обычная операция эффекта**, не специальная конструкция.
   Минус один концепт — `throw` объясняется через тот же механизм,
   что `Db.query` и `Logger.log`.
2. **Тип ошибки в сигнатуре** — AI-first: LLM видит конкретный класс
   ошибок, не общий «может бросить что-то».
3. **`throw` известно из Java/JS/C#/Swift** — AI-friendly без
   переучивания.
4. **Sum-type для нескольких ошибок** — простая композиция handler'ов:
   один handler ловит весь sum-type, дальше `match` по вариантам.

### Что отвергнуто

- **`raise` или `error()` вместо `throw`.** `throw` известно по умолчанию
  из мейнстримных языков.
- **`Fail` всегда без параметра** (как Java unchecked exceptions
  или Swift `throws`). Теряется видимость типа ошибки в сигнатуре,
  ломает AI-first тезис.
- **`Fail[E1, E2, E3]` (множественные параметры).** Усложняет
  композицию handler'ов (handler ловит «один из E1/E2/E3»? все три?
  один с union-pattern'ом?). Семантически избыточно — sum-type выражает
  то же чище. Нарушает простое правило «один эффект — один параметр»,
  как `Alloc[R]`, `Ask[T]`.
- **`throw` без эффекта в сигнатуре** (как Java RuntimeException).
  Невидимое control flow — главная проблема Java unchecked exceptions.

### Связь

- [D2](#d2-эффекты-вместо-ключевых-слов-asyncthrowsunsafe) — эффекты
  вместо keyword'ов; `Fail[E]` — один из эффектов.
- [D4](#d4--для-пробрасывания-ошибки) — `?` как сахар над `match` +
  `throw`.
- [D11](#d11-имена-эффектов-и-синтаксис-with) — handler-литералы для
  `Fail[E]`.
- [D31](#d31-handler-лямбда-для-эффектов-с-одной-операцией) —
  handler-лямбда для `Fail[E]` (главный case сахара).
- [02-types.md → D15](02-types.md#d15) — sum-types для нескольких
  типов ошибок.
- [08-runtime.md → D13](08-runtime.md#d13) — `throw` ≠ `panic`.

### Цена

1. Программист обязан явно описывать тип ошибки в публичных API —
   дополнительная работа, оправданная видимостью контракта.
2. Sum-type для нескольких типов ошибок — небольшой синтаксический
   налог, оправданный простотой композиции handler'ов.
3. Граница `throw` vs `panic` требует понимания — лечится документацией.

### Performance: насколько дорогой `throw`

Bootstrap-runtime реализация `throw msg`:

1. **Vtable indirect call**: `_nova_handler_Fail->fail(ctx, msg)` —
   один pointer-load + indirect-call. ~1ns на современном CPU.
2. **Handler-method body** — пользовательский Nova-код. Зависит.
3. **`longjmp`** на nearest fail-frame: restore callee-saved regs,
   sp, pc. ~10-20ns. Без RAII-unwind (D6 GC — нет destructor'ов).
4. **Cross-mco-boundary** (если throw в fiber, handler снаружи):
   запись pending в scope-state, longjmp на fiber-local fail-frame,
   потом scope-runner re-issue на main. Дополнительно ~10-20ns.

Итого: **~50-200ns** на throw без stack-trace. Дёшево.

Сравнение:

| Язык | Cost throw |
|---|---|
| Java exceptions | 10000-50000ns (stack-trace fill-in + class lookup) |
| C++ exceptions | 1000-10000ns (zero-cost happy path, expensive throw) |
| Rust panic | 1000-10000ns (similar to C++) |
| Go panic | 100-500ns (similar approach to Nova) |
| **Nova throw** | **~50-200ns** (без stack-trace, без RAII) |

**Когда throw становится узким местом:**

Hot loop с throw на каждой итерации (парсер где throw для каждого
invalid char) — даже 100ns × 10⁶ итераций = 100ms. В таком случае
**использовать Result-стиль через D77** `try_from`/`try_into`:
match на Result в hot path вообще не использует longjmp.

Throw — для **business-level errors**, где он редок и acceptable.
Result — для **парсинга / валидации / hot path**. Это рекомендация
из D73 (`from`/`into` для use-cases, `try_from`/`try_into` для
implementation хотя оба доступны вызывающему).
   и сообщениями компилятора.

### Эволюция

В первых черновиках допускалось `Throws[E1, E2]` (множественные
параметры) — пересмотрено в пользу sum-type. Также раньше `Throws` без
параметра был всегда допустим, теперь — только в приватных функциях
([D28](#d28-вывод-эффектов-private--выводится-public--обязательно-явно)).
Эффект переименован `Throws` → `Fail` в [D61](#d61-полная-семантика-эффектов-effect-keyword-handler-литерал-handlere-interrupt).

---

## D28. Вывод эффектов: private — выводится, public — обязательно явно

> ⚠️ **REVISED → [D62](#d62).** Изначально D28 объявлял
> «полный транзитивный вывод эффектов = compile error при missing
> в public». После D62 вывод **прямых** эффектов остался обязательным
> (compile error если не объявлены), но транзитивные эффекты теперь
> дают **warning** (suppressable через `#allow_transit(...)` или
> `Nova.toml`). «Чистая функция = проверенный факт» теперь работает
> как «**прямой** эффект отсутствует» — функция может транзитивно
> делать `Db.exec`, но если она сама не вызывает `Db.X`, она
> формально без `Db` в сигнатуре. Гарантия чистоты ослаблена; для
> жёсткой санитизации использовать [`forbid X { }`](#d63).

### Что
Эффекты в сигнатуре private-функций (без `export`) **выводятся
компилятором** для прямых вызовов; транзитивные — warning. Программист
может опустить прямые эффекты в private, компилятор проанализирует
тело и добавит. В public API (`export fn`) **прямые** эффекты
**обязательны явно** — это контракт.

Функция без прямых эффектов — это **проверенный факт об отсутствии
прямых обращений к эффект-операциям**. Транзитивные обращения через
вложенные вызовы возможны (но виден warning по [D62](#d62)).

### Правило

#### Базовое использование

```nova
// private — эффекты выводятся
fn helper(x int) =>
    Logger.log("processing ${x}")     // компилятор добавит Logger в сигнатуру
    x * 2

// то же явно — программист тоже может писать
fn helper(x int) Logger -> int =>
    Logger.log("processing ${x}")
    x * 2

// public — должно быть явно
export fn process(x int) Logger -> int =>
    Logger.log("...")
    x * 2

// public БЕЗ эффектов — компилятор проверяет, что их и правда нет
export fn double(x int) -> int =>
    x * 2                              // ок, чистая

export fn bad(x int) -> int =>
    Logger.log("...")                  // ОШИБКА: эффект Logger не объявлен
    x * 2
```

#### Гарантия отсутствия прямых эффектов

Функция **без эффектов в сигнатуре** (после D62) = компилятор
доказал, что она **сама** не использует эффект-операции:

- Не вызывает `Io.read/print` (но может вызывать функцию, которая внутри это делает — warning)
- Не делает прямых вызовов на сеть/БД/файлы
- Не делает `throw` или `?` (Fail strict — транзитивный, всегда виден)
- Не аллоцирует в region'ах с эффектом

Это **слабее «полной чистоты»** (которая была в D28 до D62). Для
жёсткой гарантии «вызовы суда не доходят» — использовать
[`forbid X { ... }`](#d63) capability sandbox.

Что осталось strict:
- **`Fail[E]`** — всегда транзитивный, обязан быть в сигнатуре
  если callee может бросить ([D65](#d65)).
- Прямые вызовы — compile error если эффект не объявлен.

#### Правило вывода (после D62)

Компилятор анализирует тело функции:

1. Использование операции эффекта (`Db.query`, `Logger.log`) **прямо
   в теле** → этот эффект добавляется (обязательный для public).
2. Каждый `throw` или `expr?` → `Fail[E]` добавляется (всегда
   транзитивный, см. [D65](#d65)).
3. Каждый вызов функции с эффектами **в чужой сигнатуре** →
   - `Fail` транзитивно добавляется (strict).
   - Другие эффекты — warning «не объявленный транзитивный X»,
     suppressable через `#allow_transit(X)`.
4. Мутация `@field` в `mut @method` ([03-syntax.md → D35](03-syntax.md#d35))
   — это `mut`-метод, не эффект (D62 убрал `Mut`).

#### Public API — почему обязательно явно

1. **Контракт модуля.** Сигнатура — это интерфейс, который другие
   модули видят. Изменение эффектов = breaking change. Должно быть
   видно в коде, не выводиться невидимо.
2. **AI-first.** LLM, читая сигнатуру публичной функции, должна
   видеть все побочные действия. Public API — точка, где «сигнатура =
   полное описание» работает.
3. **Документация.** Public — это то, что попадает в `nova doc`.
   Эффекты — часть документации, не runtime-деталь.
4. **Случайное расширение.** Если private-функция получила лишний
   эффект (программист добавил `Logger.log` в утилиту), это **не
   должно** автоматически попадать в public — public видит ошибку
   компиляции, программист принимает осознанное решение.

#### Случайное расширение в private — после D62

Программист добавил вызов `Logger.log(...)` в утилиту → у функции
автоматически появился `Logger` (прямой) → вызывающие private-функции
получают **warning** «транзитивный Logger не объявлен», но
**компилируются**. До public API доходит warning, не ошибка.

Это **ослабление D28** в пользу удобства. Если нужна жёсткая
проверка «функция не должна косвенно делать X» — использовать
[`forbid X { ... }`](#d63):

```nova
fn pure_view(u User) -> str =>
    forbid Db, Net, Io {
        format_user(u)         // compile error если внутри есть Db/Net/Io
    }
```

Тулинг:

- **`nova check --show-effects`** — режим, показывающий выведенные
  эффекты для всех private-функций.
- **`@no_effects` атрибут** на private-функцию — компилятор обязан
  подтвердить, что функция чистая. Если нет — ошибка.
- **`@effects(Logger, Db)` атрибут** — закрепить ожидаемые эффекты для
  private. Расширение → ошибка.

В release-сборке тулинг не нужен, в dev — стандартный механизм
проверки.

#### Историческая заметка про Async

В первой редакции D28 здесь был раздел «Async — особенно важно»,
обсуждавший «сделать ли Async дефолтным эффектом». После
[D62](#d62) `Async` вообще не эффект (ambient runtime-инфраструктура,
см. [D14](06-concurrency.md#d14)), поэтому дилемма не актуальна.

Возникал вопрос «сделать `Async` дефолтным для всех функций, чтобы
не писать его в каждой backend-сигнатуре». **Отвергнуто** в пользу
полного удаления `Async` из системы типов:

Чистая функция `double(x int) -> int` гарантированно **не
приостанавливается**. Можно использовать в hot loop без yield-pauses.
Если бы `Async` был дефолтом — этой гарантии бы не было.

D28 решает «шум `Async`» иначе: в private он **выводится**, программист
не пишет. В public — пишет один раз. Гарантии чистоты сохраняются.

### Почему

1. **AI-first компромисс.** Внутри модуля программист пишет быстро,
   на границе модуля LLM (и человек) видит явный контракт.
2. **Гарантия чистоты сохраняется.** Public-функция без эффектов —
   проверенный факт, можно мемоизировать.
3. **Шум `Async` уходит.** В private его не пишут, в public — один
   раз для каждой границы.

### Что отвергнуто

- **Везде явно (как Java checked exceptions).** Шум в private-утилитах
  без выгоды.
- **Везде выведено (как Haskell для типов).** Public API теряет явный
  контракт.
- **`Async` как эффект** (любой формы — дефолт или явно). Отвергнуто
  в [D62](#d62): suspension — runtime-факт, не type-fact.
- **Опт-ин для вывода (`@infer_effects` или подобный).** Программист
  выбирает каждый раз — лишний шум.

### Связь

- [D2](#d2-эффекты-вместо-ключевых-слов-asyncthrowsunsafe) — эффекты
  вместо keyword'ов; D28 уточняет правило вывода.
- [D25](#d25-throw-и-параметризация-throwse) — то же правило для
  `Fail`: выводится в private (можно опустить параметр), обязателен
  в public.
- [01-philosophy.md → D10](01-philosophy.md#d10-революционная-ставка-всё--эффект--ai-first) —
  AI-first: видимость в public API сохраняется, шум в private убирается.
- [07-modules.md → D5](07-modules.md#d5) — два уровня видимости (`export`
  / приватно), эффект-видимость следует видимости функции.

### Цена

1. **Качество сообщений компилятора** при ошибке «private-функция
   приобрела эффект, public-вызывающий не объявлен» — критично.
   Программист должен видеть **где** эффект пришёл, **через какую
   цепочку вызовов**.
2. **Цепные изменения в private** — диф не показывает явно, что
   эффекты расширились. Тулинг (`--show-effects`, `@no_effects`)
   компенсирует.
3. **Compile-time стоимость** — анализ эффектов транзитивный, увеличивает
   время компиляции на несколько процентов. Приемлемо.

---

## D31. Handler-лямбда для эффектов с одной операцией

> **Обновлено [D61](#d61-полная-семантика-эффектов-effect-keyword-handler-литерал-handlere-interrupt)**
> и [D22-rev](../03-syntax.md#d22-closure-light--и-full-fn) (2026-05-10):
> синтаксис `Fail[E]` ушёл в `Fail[E]`, `protocol` для эффектов ушёл
> в `effect`, handler-литерал получил keyword `handler`. Тело
> handler-method'а завершается через `return v` / финальное выражение
> или `interrupt v` (см. D61). Handler-лямбда мигрирована с
> `(args) => expr` на `|args| expr` симметрично closure-rev — единый
> «pipe-маркер» для всех безымянных функций.

### Что
Если эффект имеет **ровно одну операцию**, handler можно записать как
**handler-лямбду** в форме `|args| body` — параметры соответствуют
параметрам единственной операции эффекта. Для эффектов с двумя и
более операциями — handler-литерал `effect EffectName { ... }`
обязателен.

Handler-литерал `effect EffectName { op(p) ... }` содержит **handler-методы**,
у которых **две взаимоисключающие формы тела**, как у `fn` ([D40](03-syntax.md#d40)):
`op(p) => expr` (одно выражение) или `op(p) { block }` (блок-форма
**без `=>`**). Сочетание `=>` и `{}` в handler-method **запрещено** —
правило симметрично D40.

### Правило

#### Базовое использование

```nova
type Fail[E] effect {
    fail(value E) -> never
}

// сокращённо — handler-лямбда (одна операция → |args| body)
// Тело должно содержать `interrupt v` (поскольку fail возвращает
// never, нормальное завершение через return невозможно):
with Fail[Error] = |err| interrupt log_and_default(err) {
    Db.exec(sql`UPDATE accounts SET balance = balance - 1`)
}

// полная форма — эквивалентна
with Fail[Error] = effect Fail[Error] {
    fail(err) => interrupt log_and_default(err)
} {
    Db.exec(sql`UPDATE accounts SET balance = balance - 1`)
}
```

Тело handler-лямбды — **bare expression или block** (как у
closure-light, [D22](../03-syntax.md#d22-closure-light--и-full-fn)):

```nova
// expression-body — типичный случай
with Fail[Error] = |err| interrupt default_value { ... }

// block-body — несколько statement'ов
with Fail[Error] = |err| {
    Log.error("got error: ${err}")
    interrupt default_value
} { ... }
```

**`|err| -1` без `interrupt`** — **невалидно** для `Fail[E]`-handler'а:
операция `fail(value E) -> never` запрещает return/финальное выражение
(нет значения типа never), требуется явный `interrupt`. Старая форма
без `interrupt` соответствовала pre-D61 implicit-interrupt семантике —
она отвергнута в D61 как AI-unfriendly.

Для эффекта с несколькими операциями — handler-литерал обязателен.
Handler-method может быть как `=> expr`, так и блок-формы `{ block }`:

```nova
type Db effect {
    query(q Sql) -> []DbRow
    exec(q Sql)  -> ()
}

with Db = effect Db {
    // короткая форма — одно выражение
    query(q) => real.query(q)

    // блок-форма — несколько statement'ов
    exec(q) {
        staged.push(q)
        return ()
    }
} {
    transfer(alice, bob, 100)
}
```

**Запрещено** (нарушает D40 «`=>` и `{}` не сочетаются»):

```nova
with Db = effect Db {
    exec(q) => {                          // ← запрет: => { block }
        staged.push(q)
        return ()
    }
}
```

#### Какие эффекты попадают под сахар

Из стандартного набора:

| Эффект | Операции | Сахар работает |
|---|---|---|
| `Fail[E]` | `fail(value)` | да — главный win |
| `Random` | `next()` | да (если одна операция) |
| `Logger` (минимальный) | `log(msg)` | да |
| `Time` | `now()`, `sleep(d)` | нет |
| `Db` | `query`, `exec` | нет |
| `Net` | `get`, `post`, ... | нет |
| `Fs` | `read`, `write`, ... | нет |
| Пользовательские | зависит | если одна |

`Fail[E]` — самый частый случай, ради него сахар главным образом
вводится. В backend-коде `with Fail[E] = |err| ... { ... }` будет
основной формой обработки ошибок через handler.

#### Грамматика

В позиции значения после `with EffectName =`:

- **Handler-лямбда** `|params| body` (где body — expression или block,
  по [D22-rev](../03-syntax.md#d22-closure-light--и-full-fn))
  → сахар, разворачивается в handler-литерал с одной операцией.
  Компилятор проверяет, что у эффекта **ровно одна операция**, и
  параметры лямбды совместимы с её сигнатурой.
- **No-arg handler-лямбда** `|| body` — для операций без параметров
  (например `Random.next() -> int`).
- **Handler-литерал** `effect EffectName { op(p) => expr, op(p) { block }, ... }`
  → используется как есть. Работает для любого числа операций. Каждый
  handler-method — `=> expr` или `{ block }`, никогда не вместе ([D40](03-syntax.md#d40)).
- **Переменная или выражение** типа `Effect[EffectName]` или
  `Effect[EffectName, IRT]` ([D87](#d87-handlere-irt--параметризация-handler-типом-interruptа))
  → используется как есть.

Парсер однозначен по первому токену после `=`:

- `|` (pipe) → handler-лямбда (по closure-light grammar [D22](../03-syntax.md#d22-closure-light--и-full-fn))
- `||` → handler-лямбда без параметров
- `handler` (keyword) → handler-литерал
- идентификатор → переменная/выражение

В отличие от обычной closure-light, **закрытие в этой позиции
интерпретируется как handler-лямбда** — компилятор смотрит на
ожидаемый тип `Effect[EffectName]` и:
- проверяет что эффект имеет ровно одну операцию,
- сопоставляет параметры лямбды с параметрами этой операции,
- разворачивает в полный handler-литерал.

#### Что компилятор проверяет

```nova
// ОК — эффект с одной операцией
type Logger effect { log(msg str) -> () }
with Logger = |msg| println(msg) { ... }

// ОШИБКА — у Db две операции, лямбда неоднозначна
with Db = |sql| ... { ... }
//        ^^^^^^^^^^^
//        error: handler-lambda requires effect with exactly one operation
//               (Db has 2: query, exec)
//        suggestion: use handler literal — effect Db { query(...) => ..., exec(...) => ... }

// ОШИБКА — параметры лямбды не совпадают с операцией
with Fail[Error] = || { ... } { ... }
//                  ^^^^^^^^^^^^
//                  error: handler-lambda parameter count mismatch
//                         expected one parameter (value Error), got zero
```

### Почему

1. **Главный win — `Fail[E]` обработка.** В backend-коде
   `with Fail[E] = |err| ... { ... }` повторяется в каждой
   обработке ошибок. Сахар сокращает в 2-3 раза без потери семантики.
2. **«Минимум строк на выходе»** — один из центральных принципов Nova
   ([01-philosophy.md → D10](01-philosophy.md#d10-революционная-ставка-всё--эффект--ai-first)).
3. **Граница сахара чёткая** — только в позиции `with EffectName =`,
   только для эффектов с одной операцией. Не превращается в общую
   SAM-conversion.
4. **Симметрия с closure-rev.** После [D22-rev](../03-syntax.md#d22-closure-light--и-full-fn)
   `|x|` — единый «pipe-маркер» для всех безымянных функций (closure
   как value, closure как arg, handler-лямбда). Программист учит одну
   грамматику.

### Что отвергнуто

**Полная SAM-conversion (любой type с одной операцией → лямбда).**
Это разрешало бы лямбды и для не-эффектных типов:

```nova
type Comparator { compare(a int, b int) -> int }
ro c Comparator = |a, b| a - b      // ← отвергнуто, не делаем
```

Причина: для **эффектов** сахар сильно окупается (Fail частый,
минимизация строк критична). Для **обычных типов** дублирует
функциональный тип `fn(int, int) -> int` без выгоды. Граница чёткая —
**сахар работает только в позиции `with EffectName =`**.

**Handler-лямбда через `(params) =>`** (форма до 2026-05-10) —
заменена на `|params| body` ради симметрии с closure-rev. `=>`
освобождён от роли «лямбда-стрелки» и остаётся маркером тела
named fn / handler-method / match-arm.

### Связь

- [D11](#d11-имена-эффектов-и-синтаксис-with) — добавляет третью форму
  handler'а (помимо литерала и переменной).
- [D25](#d25-throw-и-параметризация-throwse) — `throw` как операция
  эффекта `Fail[E]`, лямбда естественно её перехватывает.
- [03-syntax.md → D22](../03-syntax.md#d22-closure-light--и-full-fn) —
  closure-light `|x| body`. Handler-лямбда — специализация в позиции
  `with EffectName =` для эффектов с одной операцией.
- [03-syntax.md → D40](03-syntax.md#d40) — handler-method подчиняется
  тому же правилу `=>` ↔ `{}`, что и `fn`: или `=> expr`, или
  `{ block }`, никогда не вместе.
- [03-syntax.md → D43](03-syntax.md#d43) — trailing-block с
  обязательными `()` (сюда не применяется — здесь handler-выражение
  после `=`, не trailing-block).

### Цена

1. **Парсер чуть сложнее** — после `with X =` нужно различить
   handler-лямбду (`|...|`), handler-литерал (`handler`-keyword) и
   переменную. Каждый случай распознаётся по первому токену.
2. **Breaking change при добавлении операции** — если эффект расширили,
   все handler-лямбды для него ломаются с compile error. Это
   **корректное** поведение (видимое нарушение контракта), но
   программисту нужно обновить код в нескольких местах.
3. **Два способа делать одно и то же.** Сахар (`|params| body`) и
   полная форма (`effect EffectName { op() => ... }`). Линт может
   предлагать сахар где он короче.

### Эволюция

Ранее в `open-questions.md` была отрицательная запись «SAM-conversion
отвергнут». Сейчас пересмотрена: SAM **принят с ограничением** — только
для эффектов в `with`, только при одной операции. Главный аргумент
пересмотра: «минимум строк на выходе» — `with Fail[E] = effect Fail[E] { fail(err) =>
... }` повторяется в каждой обработке ошибок.

Ревизия (2026-05-10): handler-лямбда мигрирована с `(params) => expr`
на `|params| body` симметрично [closure-rev D22](../03-syntax.md#d22-closure-light--и-full-fn).
Тело теперь может быть expression ИЛИ block (раньше — только expression).
Семантика не изменилась. Migration: ~15 примеров в spec.

---

## D61. Полная семантика эффектов: `effect` keyword, handler-литерал, `Effect[E]`, `interrupt`

### Что

Закрывающий блок системы эффектов. Фиксирует:

1. **`type Foo effect { ... }`** — отдельный keyword для объявления типа
   эффекта (вместо ранее использовавшегося `protocol`).
2. **`effect Foo { ... }`** — keyword для handler-литерала (значения,
   реализующего эффект).
3. **`Effect[E]`** — тип значения handler-литерала, first-class.
4. **Effect-row** — неупорядоченное множество, дубликаты запрещены.
5. **`return v` / финальное выражение** в handler-method — нормальное
   завершение, значение идёт в caller операции (continuation
   возобновляется).
6. **`interrupt v`** — досрочное завершение всего `with`-блока, новый
   keyword.
7. **tail-position для `return` / `interrupt`** — код после запрещён.
8. **`Effect[E].op(args)`** — прямой вызов операции на handler-значении,
   минуя with-стек.
9. **Тип `with`-блока** — единый тип `T`, который дают и финальное
   выражение body, и все handler-method'ы (когда они не делают `interrupt`).
10. **Алгоритм компиляции/интерпретации** — пошаговое тех-задание для
    имплементатора (раздел ниже).

Этот блок закрывает [Q-resume-semantics](../open-questions.md#q-resume-semantics)
и [Q-handler-method-param-inference](../open-questions.md#q-handler-method-param-inference).

### Правило

#### 1. `type Foo effect { ops }` — объявление эффекта

```nova
type Db effect {
    query(q Sql) Fail[DbError] -> []DbRow
    exec(q Sql)  Fail[DbError] -> int
    in_transaction[T](body fn() Db Fail -> T) Fail -> T
}
```

**Generic-методы в effect-объявлении** (например, `in_transaction[T]`)
требуют rank-2 polymorphism — один handler работает с любым `T` для
каждого вызова. Точная семантика type-checker'а для rank-2 в effect-
методах — открытый вопрос ([Q6](../open-questions.md#q6)). Bootstrap-
интерпретатор поддерживает через runtime erasure (T мономорфизуется
как `any` на уровне dispatch'а); production-компилятор должен дать
формальное правило.

```nova
type Logger effect {
    log(msg str) -> ()
}

type Fail[E] effect {
    fail(value E) -> never
}
```

Раньше эффекты объявлялись через `type X protocol { ... }` ([D18](#d18-эффекты-объявляются-через-kind-токен-не-голый-type),
[D53](02-types.md#d53)). Теперь — отдельный keyword `effect`. Причина:
**эффект и protocol — семантически разные** контракты:

- `protocol` — структурный интерфейс, проверяется на типе значения
  параметра (`fn sort[T Hash](xs []T)`, [D72](02-types.md#d72)).
  Статический dispatch.
- `effect` — контракт на наличие активного handler'а в скоупе
  (`fn save() Db -> ()`). Lookup в with-стеке, динамический dispatch.

Смешение запрещено:

- `fn f[T Db](x T)` — compile error: `Db` это эффект, не protocol.
- `fn f() Hash -> ()` — compile error: `Hash` это protocol, не effect.

#### 2. `effect Foo { ops }` — handler-литерал

Значение, реализующее эффект `Foo`. Появляется в `let`-биндинге, в
`with X = ...`, в return-position функций, в аргументах:

```nova
// Место 1 — let-биндинг
ro postgres_db = effect Db {
    query(q) => real_query(q)
    exec(q)  => real_exec(q)
    in_transaction(body) => real_transaction(body)
}

// Место 2 — внутри with
with Db = effect Db {
    query(q) => []
    exec(q)  => 0
    in_transaction(body) => body()
} {
    process()
}

// Место 3 — return из функции (декоратор)
fn with_audit(real Effect[Db]) -> Effect[Db] => effect Db {
    query(q) => real.query(q)
    exec(q) {
        spawn write_audit(q)
        real.exec(q)
    }
    in_transaction(b) => real.in_transaction(b)
}

// Место 4 — аргумент функции
fn run_with(h Effect[Db], body fn() Db -> ()) -> () {
    with Db = h { body() }
}
```

Handler-литерал содержит **handler-method'ы** — по одному на каждую
операцию эффекта. Тело handler-method'а — `=> expr` или `{ block }`,
как у `fn` ([D40](03-syntax.md#d40)).

#### 3. `Effect[E, IRT]` — тип значения

`Effect[E, IRT]` — встроенный generic-тип, не объявляется в
пользовательском коде. Параметризован эффектом `E` и типом
**interrupt'а** (IRT — interrupt-return type), полностью описан в
[D87](#d87-handlere-irt--параметризация-handler-типом-interruptа).

`Effect[E]` ≡ `Effect[E, never]` — sugar (через
[D88 default generic](03-syntax.md#d88-default-значения-generic-параметров))
для handler'а, который **не делает** `interrupt`.

Источники значений:

- handler-литерал `effect EffectName { ops }` — выражение типа `Effect[E, IRT]`
  (IRT inferred из interrupt'ов в теле; если их нет — `never`)
- handler-лямбда `|args| body` для одно-операционных эффектов
  ([D31](#d31-handler-лямбда-для-эффектов-с-одной-операцией))

`Effect[E, IRT]` — first-class:

```nova
ro h = effect Db { ... }                  // h: Effect[Db, never] (нет interrupt)
ro arr = [h, h2, h3]                       // в массив
ro pair = (h, "label")                     // в кортеж
fn make() -> Effect[Db] => h               // вернуть из fn (never по default)
fn use(h Effect[Db]) { ... }               // принять как параметр

// Handler с interrupt типа int:
fn make_fatal() -> Effect[Logger, int] => effect Logger {
    log(msg) {
        if msg.starts_with("FATAL") { interrupt -1 }
        println(msg)
    }
}
```

##### Effect-type vs Effect[E] — где какой использовать

Это **два разных типа**, и компилятор различает их по позиции:

| Тип | Где допустим | Что значит |
|---|---|---|
| `Foo` (effect-тип сам по себе) | effect-position сигнатуры (между `)` и `->`), позиция эффекта в `with X = ...` | контракт «нужен handler в скоупе» |
| `Effect[Foo]` (тип значения) | позиция типа значения: тип переменной, тип параметра, тип return | конкретное handler-значение, которое можно передавать |

Конкретные правила:

- `let h Fail[Error] = ...` — **compile error**: `Fail[Error]` не тип
  значения. Должно быть `let h Effect[Fail[Error]] = ...`.
- `fn f() Fail[Error] -> ()` — OK: `Fail[Error]` в effect-position.
- `fn make() -> Effect[Fail[Error]] => effect Fail[Error] { ... }` —
  OK: возвращаемый тип = `Effect[Fail[Error]]`, литерал даёт это
  значение.
- `fn run(h Effect[Fail[Error]]) -> () { with Fail[Error] = h { ... } }` —
  OK: параметр-handler в позиции типа значения, в `with` — effect-тип.

Эта строгая разделённость позиций — не ради «чистоты», а ради
disambiguation при чтении. Один и тот же синтаксический токен `Foo`
парсится в effect-row или в обычной type-position, и эти позиции
грамматически различимы. Правило «compile error при попытке
смешать» — gatekeeper, чтобы случайные ошибки ловились на type-check.

#### 4. Effect-row неупорядочен, дубликаты запрещены

```nova
fn process(o Order) Db Logger Fail[E] -> ()
fn process(o Order) Logger Db Fail[E] -> ()       // та же сигнатура
```

Effect-row — **множество**, не список. Порядок не определяет
сигнатуру. Lookup в with-стеке индексирует по имени типа эффекта.

Дубликаты **одного и того же эффекта** — compile error:

```nova
fn bad() Db Db -> ()                         // ОШИБКА: duplicate effect `Db`
```

Разные параметры одного generic-эффекта — **разрешены** (D65):

```nova
fn process(s str) Fail[ParseError] Fail[RuntimeError] -> int { ... }
                                             // ОК: multi-Fail в row,
                                             // см. D65
```

Это применимо ТОЛЬКО к параметризованным эффектам, у которых разные
type-аргументы дают разные effect-роли. Для `Fail[E]` — это canonical
паттерн composition'а (см. [D65](#d65)).

Convention для записи: алфавитный порядок или по «частоте использования»
(программистский выбор), но **это convention, не grammar**.

#### 5. Завершение handler-method'а — `return` / финальное выражение

Handler-method ведёт себя как обычная функция. Возвращает значение
**в caller операции** (continuation возобновляется с этим значением):

```nova
effect Db {
    query(q)  => real_query(q)               // финальное выражение = return
    exec(q)  {
        ro r = real_exec(q)
        return r                              // явный return
    }
}
```

С точки зрения caller'а операции — это обычный возврат:

```nova
ro rows = Db.query(q)                       // получает результат query
println(rows.len())                           // программа продолжается
```

#### 6. `interrupt v` — досрочное завершение with-блока

Когда handler-method хочет прервать continuation и **сделать так, чтобы
вместо вызова операции из `Db.query(...)` весь `with`-блок сразу
вернул `v`** — используется `interrupt v`:

```nova
effect Fail[E] {
    fail(err) => interrupt -1               // throw перехвачен; with-блок отдаёт -1
}

effect Db {
    query(q) => real_query(q)                // обычное завершение
    exec(q) {
        if dangerous(q) {
            interrupt 0                       // прервать с 0, не выполнять SQL
        }
        real_exec(q)
    }
}
```

Семантика:
- `interrupt v` валиден **только** внутри handler-method'а. Вне — compile error.
- После `interrupt v` continuation **не возобновляется**. Значение `v`
  становится результатом всего `with`-блока.
- Handler-method, в котором сработал `interrupt`, считается завершённым.
  Code после `interrupt` в той же ветке — compile error (мёртвый код).

**Тип аргумента `interrupt v`** — это **тип `with`-блока** (`W`), не
return-тип операции. Компилятор знает `W` через type inference сверху
вниз для всего with-блока (см. раздел «Тип `with`-блока» ниже).
Для каждого handler-method'а:

| Путь завершения | Тип `v` должен быть |
|---|---|
| `return v` или финальное выражение | return-тип операции (`R` из декларации) |
| `interrupt v` | тип `with`-блока (`W`) |

Это **разные типы**: `R` определяется effect-декларацией статически,
`W` — контекстом where the with appears. Один handler-method может
смешивать оба завершения в разных ветвях:

```nova
type Db effect {
    query(q Sql) -> []DbRow      // R = []DbRow
}

ro result = with Db = effect Db {
    query(q) {
        if q.template == "" {
            interrupt 42          // здесь v: int (W = int — см. body ниже)
        }
        real_query(q)             // здесь финальное выражение: []DbRow (R)
    }
} {
    ro rows = Db.query(some_q)
    rows.len()                       // body даёт int → W = int
}
// result: int
```

Чтобы это валидно проходило type-check:
- В ветке `interrupt 42` — `42: int`, совместимо с `W = int`. ✅
- В ветке `real_query(q)` — `[]DbRow`, совместимо с `R = []DbRow`. ✅
- Body даёт `rows.len: int`, совместимо с `W = int`. ✅

#### 7. Tail-position для `return` и `interrupt`

После `return v` или `interrupt v` в той же ветке — **код запрещён**
(аналогично [D23](03-syntax.md#d23) для `return` в обычной функции):

```nova
exec(q) {
    return real_exec(q)
    println("dead")                           // ОШИБКА: код после return недостижим
}

exec(q) {
    if dangerous(q) {
        interrupt 0                            // OK — последняя инструкция в ветке
    } else {
        return real_exec(q)                   // OK — последняя инструкция в ветке
    }
    // OK — код после if/else возможен, если хотя бы одна ветка не выходит
}
```

В `match` каждая arm — отдельная tail-position. То же что для обычных
функций.

#### 8. never-операции и `interrupt`

Операция типа `never` (классический пример — `Fail.throw`) **не имеет
валидных значений возврата**. Поэтому в её handler-method'е:

- `return v` запрещён (нет значения типа `never`).
- Финальное выражение запрещено.
- Единственный валидный путь — `interrupt v`, где `v` имеет тип
  результата with-блока.

```nova
effect Fail[Error] {
    fail(err) => interrupt log_and_default(err)     // OK
}

effect Fail[Error] {
    fail(err) => err.message                        // ОШИБКА: return запрещён для never
}
```

##### `throw expr` — keyword-сахар над `Fail[E].fail(expr)`

Keyword `throw expr` — синтаксический сахар над операцией `fail`
эффекта `Fail[E]`:

```nova
throw expr
// разворачивается в
Fail[E].fail(expr)
```

Семантика:
- Тип `throw expr` — `never` (как и операция `fail`).
- Требует **активный handler** для `Fail[E]` где-то выше по стеку.
  Без него runtime panic «no handler for effect Fail[E]» (либо
  compile error, если static-анализ доказал что handler никогда
  не активен).
- Type checker проверяет, что в эффект-row enclosing-функции есть
  `Fail[E]` (или эффект может быть выведен через [D28](#d28-вывод-эффектов-private--выводится-public--обязательно-явно)
  для private-функций).
- Тип `E` для `throw expr` определяется типом `expr` (или явной
  параметризацией если `Fail[E]` указан в сигнатуре с конкретным `E`).

Связь с `?` ([D4](#d4--для-пробрасывания-ошибки)):

```nova
expr?
// разворачивается в
match expr {
    Ok(v)  => v
    Err(e) => throw e        // throw e ≡ Fail[E].fail(e)
}
```

То есть `?` это сахар над `match` + `throw`, а `throw` — сахар
над `Fail[E].fail(...)`. Никаких специальных правил компилятора
для `throw` или `?` — всё через стандартный effect-механизм.

##### `never` как тип значения, совместимый с любым

`never` — bottom-тип ([02-types.md → D26](02-types.md#d26)). Значений
типа `never` не существует, но **позиция типа `never` совместима с
любым другим типом** при type-check. Это нужно потому что:

- `throw expr` (тип `never`) может стоять в любой позиции: `let x int = throw e`,
  `if cond { throw e } else { 42 }`, и т.д.
- Body `with`-блока, который **всегда** заканчивается throw'ом, имеет
  тип `never`. Тогда тип `with`-блока = тип `interrupt`-веток handler'а
  (потому что body не возвращает значение нормально).

Пример:

```nova
ro i = with Fail[Error] = effect Fail[Error] {
    fail(err) => interrupt -1
} {
    throw Error.new("bad")           // тип throw — never
}
// type of i = int
//   body's type = never (всегда throw)
//   handler interrupt's type = int
//   объединение: int (never совместим с int)
```

Алгоритм типизации `with`-блока:
- `T_body` — тип финального выражения тела (может быть `never`).
- `T_handler[i]` — тип каждого `interrupt v` пути в каждом handler-method'е.
- `W` (тип всего with-блока) = наименьший общий тип всех `T_body` и
  всех `T_handler[i]`. `never` поглощается любым типом (то есть
  `lub(never, T) = T` для любого `T`).

#### 9. Прямой вызов `h.op(args)` на handler-значении

Handler-значение поддерживает прямой member-call к своим операциям:

```nova
ro real = effect Db { query(q) => real_query(q), ... }
ro rows = real.query(sql`SELECT 1`)         // прямой вызов на handler-значении
```

Семантика:
- `h.op(args)` исполняет handler-method `op` на значении `h` напрямую.
- **Минует with-стек** — runtime не ищет handler по имени, использует
  именно `h`.
- Handler-method'ы внутри `h` могут использовать **другие эффекты**
  (через свой собственный with-скоуп, или через прямой вызов на
  ещё одном handler-значении).
- `interrupt v` внутри `h.op(args)` прерывает **этот вызов**, не
  enclosing-with-блок. Значение `v` становится результатом `h.op(args)`.

Это нужно для **handler-декораторов**:

```nova
fn with_audit(real Effect[Db]) -> Effect[Db] => effect Db {
    query(q) => real.query(q)                // вызываем real напрямую
    exec(q) {
        spawn write_audit(q)
        real.exec(q)                          // вызываем real напрямую
    }
}
```

**`h.op(args)` — не сахар для `with E = h { E.op(args) }`**, это разные
механизмы с разной семантикой при вложенных вызовах:

- `real.exec(q)` — `real` **не попадает в with-стек**. Если внутри `real`
  есть `Db.exec(...)`, он найдёт handler из внешнего скоупа.
- `with Db = real { Db.exec(q) }` — `real` кладётся в стек как активный
  `Db`. Вложенный `Db.exec(...)` внутри `real` рекурсивно снова попадёт
  в `real`.

Для handler-декораторов это критично: если бы `with_soft_delete` использовал
`with Db = real { ... }` вместо `real.exec(q)`, любой вложенный вызов
`Db.exec(...)` внутри `real` снова проходил бы через `with_soft_delete` —
бесконечная рекурсия. Прямой вызов явно говорит «вызови именно этот
handler-объект, не ищи в стеке».

Без прямого вызова декоратору пришлось бы оборачивать в `with`:

```nova
exec(q) {
    spawn write_audit(q)
    with Db = real { Db.exec(q) }            // длиннее, и семантика другая
}
```

##### Канонический пример: handler через переменную

Тип-разграничение «`Foo` в effect-position vs `Effect[Foo]` в
value-position» лучше всего видно на примере, где handler сначала
кладётся в переменную, а потом передаётся в `with`:

```nova
fn make_recovery() -> Effect[Fail[Error]] => effect Fail[Error] {
    fail(err) => interrupt -1
}

ro h = make_recovery()                    // тип h: Effect[Fail[Error]]

ro i = with Fail[Error] = h {              // Fail[Error] здесь — effect-position
    throw Error.new("not good")
}
// тип i = int (через never-совместимость + interrupt -1)
```

По строкам:
- `Effect[Fail[Error]]` — return-тип фабрики (позиция типа значения).
- `effect Fail[Error] { ... }` — handler-литерал, выражение типа
  `Effect[Fail[Error]]`.
- `let h = make_recovery()` — биндинг handler-значения. Тип переменной
  выводится: `Effect[Fail[Error]]`.
- `with Fail[Error] = h { ... }` — `Fail[Error]` в effect-position
  (контракт), `h` — конкретное handler-значение.
- `throw Error.new("not good")` — keyword `throw` раскрывается в
  `Fail[Error].fail(Error.new("not good"))`. Тип throw-выражения = `never`.
- `interrupt -1` в handler-method'е — даёт `int` как результат всего
  with-блока.
- `i` имеет тип `int` (never из body совместим с int из interrupt).

Невалидные альтернативы:
- `let h Fail[Error] = ...` — compile error: `Fail[Error]` не type-position.
  Нужно `let h Effect[Fail[Error]] = ...`.
- `with Effect[Fail[Error]] = h { ... }` — compile error: `Effect[Fail[Error]]`
  не effect-position. Нужно `with Fail[Error] = h { ... }`.

#### 10. Тип `with`-блока

```nova
ro r = with Db = h { body }
```

Тип `r` определяется так:

- `T_body` — тип финального выражения body.
- Для каждого handler-method'а, который **может завершиться без `interrupt`**
  (т.е. через `return v` или финальное выражение): тип `v` должен быть
  совместим с типом, ожидаемым caller'ом операции (т.е. с return-типом
  операции в decl).
- Для каждого handler-method'а, который **может завершиться с `interrupt v`**:
  тип `v` должен быть совместим с `T_body`.
- Тип `r` = `T_body`.

Несовпадения — compile error:

```nova
ro r = with Fail[E] = effect Fail[E] {
    fail(err) => interrupt "fail"           // handler даёт str
} {
    fetch_user_id()?                          // body даёт int
}
// COMPILE ERROR: handler interrupt type str != body type int
```

#### 11. Параметры handler-method'а

Имена параметров handler-method'а биндят аргументы операции по позиции.
**Типы выводятся** из effect-декларации — писать их в handler-литерале
не обязательно (закрывает [Q-handler-method-param-inference](../open-questions.md#q-handler-method-param-inference)):

```nova
type Db effect {
    query(q Sql) Fail[DbError] -> []DbRow
}

effect Db {
    query(q) => real_query(q)                // q: Sql выводится из decl
}

// Явные типы тоже разрешены (для документации):
effect Db {
    query(q Sql) => real_query(q)            // OK, но избыточно
}
```

### Алгоритм компиляции/интерпретации эффектов

Это **тех-задание для имплементатора**. Пошагово описывает что делает
компилятор и runtime для каждой конструкции эффекта. Без этого раздела
любая независимая имплементация выберет своё поведение и сломает
совместимость.

#### При парсинге `type Foo effect { ops }`

1. Парсер регистрирует тип `Foo` как effect-тип.
2. Каждая `op(params) effects? -> R` в теле — **сигнатура операции**.
   Сохраняется в symbol-table эффекта `Foo`: имя, типы параметров,
   row эффектов внутри (опц.), return-тип.

#### При парсинге `effect Foo { handler-methods }`

1. Парсер ищет `Foo` в symbol-table — должен быть effect-тип. Если
   protocol или другой тип — compile error.
2. Каждый handler-method `name(params) body` сопоставляется с
   операцией `Foo.name`. Имена операций должны **точно совпадать**.
3. Каждая операция эффекта **обязана** иметь handler-method
   (full coverage). Иначе compile error «handler missing operation `name`».
4. Параметры handler-method'а биндятся по позиции к параметрам
   декларации операции; типы инферируются.
5. Возвращается значение типа `Effect[Foo]`.

#### При парсинге `with EffectName = handler-expr { body }`

1. `EffectName` ищется в symbol-table — должен быть effect-тип.
2. `handler-expr` должен иметь тип `Effect[EffectName]`. Иначе compile error.
3. Тип `body` определяется по правилам выше (раздел «Тип with-блока»).
4. `with X = h1, Y = h2 { body }` равно вложенным with'ам:
   `with X = h1 { with Y = h2 { body } }`.

#### При вызове операции `EffectName.op(args)`

1. Type checker:
   - `EffectName` существует и это effect.
   - `EffectName` присутствует в effect-row enclosing-функции (или
     активен в текущем with-скоупе через inference, [D28](#d28-вывод-эффектов-private-выводится-public-обязательно-явно)).
   - Типы `args` совместимы с декларацией операции.
2. Runtime (interpreter / codegen):
   - Ищет в handler-стеке handler с тегом `EffectName`. Стек
     просматривается **сверху вниз**, берётся первый найденный.
   - Если не найден — runtime panic «no handler for effect `EffectName`».
   - Найденный handler — значение типа `Effect[EffectName]`. Из него
     извлекается handler-method `op`.
   - Управление передаётся в handler-method с биндингом параметров.
   - Continuation сохраняется (или, в (II) tail-only, не сохраняется
     — см. ниже).

#### При завершении handler-method'а

В **семантике (II) tail-only** — текущая принятая семантика Nova:

- Handler-method это обычный блок. Finalize:
  - **`return v` или финальное выражение** — handler-method заканчивается
    нормально. Значение `v` передаётся **в caller операции** через
    стандартный return-механизм (тот же что для обычных функций).
    Continuation = «остаток caller'а после операции» — продолжается
    обычным flow-of-execution.
  - **`interrupt v`** — handler-method заканчивается аномально.
    Значение `v` становится **результатом всего `with`-блока**.
    Continuation **не запускается** (никакой возврат в caller операции).

Технически в (II):
- Continuation **не нужно сохранять как объект** — она это просто
  «продолжение текущего call-stack'а после операции».
- `return v` ⇒ обычный возврат из handler-method-fn, значение
  становится результатом операции для caller'а.
- `interrupt v` ⇒ исключение-подобный escape: runtime разворачивает
  стек до границы текущего `with`-блока, делает `v` его результатом.

Это позволяет реализовать эффекты **без специального fiber-runtime'а**:
обычный stack, обычные вызовы функций, исключение-подобное `interrupt`.
Цена — нельзя писать код **после** возврата continuation в handler-method'е
(нет `resume` в полной семантике).

В **полной семантике (Koka, OCaml 5)** continuation сохраняется как
first-class объект, может вызываться явно. Это требует stack-снимка
(corosensei в OCaml 5) или CPS-преобразования (Koka). Nova **не идёт
по этому пути** — выбираем (II) ради простоты понимания и реализации.
Если когда-нибудь потребуется multi-step или multi-shot resumption —
это будет отдельный D-блок, отложен под Q-multishot-resume.

#### При прямом вызове `h.op(args)`

1. `h` — значение типа `Effect[E]`.
2. `op` ищется среди handler-method'ов `h`. Если нет — compile error.
3. Handler-method вызывается **без** push'а handler'а в with-стек.
4. Continuation для этого вызова — обычный return (не возобновляет
   что-то снаружи `h.op`). `interrupt v` внутри прерывает `h.op`,
   возвращая `v` как результат именно этого вызова.

#### Lifetime handler-стека

- При входе в `with X = h { body }` — push `(X, h)` на стек.
- При выходе из body (любым способом — нормально, через interrupt,
  через panic) — pop стека.
- Handler-стек локален текущему fiber'у/потоку. В bootstrap'е fiber
  один — стек глобальный.

### Почему

1. **Закрытие зияющего пробела в спеке.** До D61 семантика resume,
   тип `Effect[E]`, поведение «без resume», запрет для never-операций
   — фактически использовались в коде, но не были формализованы.
   Любой имплементатор должен был догадываться. Теперь — пошаговый
   алгоритм, не требующий гипотез.

2. **Семантика «как обычный return» снижает порог входа.** Программист,
   видящий handler-литерал впервые, должен понимать его за 30 секунд.
   `query(q) => real_query(q)` — «возвращает значение для query»,
   как обычная функция. `interrupt` — единственный новый keyword,
   используется редко, его легко выучить отдельно.

3. **(II) tail-only достаточна для backend-кода.** Реальные handler'ы
   (Fail, Db, Logger, Time, Random, Cache) укладываются в две
   формы — `return v` или `interrupt v` в tail-position. Полная
   resume-семантика с кодом-после-resume используется в backtracking
   и sampling-задачах, которые в Nova-целевой нише редкость.

4. **Раздельные `effect` / `protocol`** — семантически разные контракты
   (статический dispatch vs lookup в with-стеке). Один keyword для
   обоих создавал ложное ощущение взаимозаменяемости.

5. **`Effect[E]` first-class** — нужен для handler-декораторов
   ([orm_decorators.nv](../../examples/orm_decorators.nv)), которые
   выражают audit / soft-delete / replica-routing как обычные
   функции. Без first-class handler'ов это невозможно сделать без
   AOP/reflection.

6. **Прямой `h.op(args)`** — sugar для частого паттерна, без него
   декораторы пишутся в 2 раза длиннее через вложенный `with`.

7. **`interrupt` отдельный keyword** — однозначно сигнализирует
   «прервать continuation», не требует понимания что финальное
   выражение делает в зависимости от типа операции.

### Что отвергнуто

- **Слово `resume` для нормального завершения** — литературное, но
  пользователь без опыта алгебраических эффектов не поймёт.
  В (II) tail-only это **обычное возвращение значения**, поэтому
  слово `return` (или финальное выражение, как у обычной функции)
  передаёт смысл точнее.

- **`return` в handler-method перегружен** (значит «вернуть в caller
  операции», а в обычной функции «вернуть из самой функции»). Это
  технически правда, но семантика идентична для пользователя:
  «handler возвращает значение». Перегрузка минимальна.

- **Полная continuation-семантика (multi-step resume)** — отложено.
  Цена реализации высока (stack-снимки или CPS), польза в backend-коде
  низка. Если потребуется — отдельный D-блок и keyword.

- **Multi-shot resume** — отложено как Q-multishot-resume. Backend Nova
  не нуждается в backtracking-эффектах.

- **`Effect[E]` как `Handler[E]` или `Impl[E]`** — `Effect[E]` это
  стандарт литературы (Eff, Koka, Effekt) и наш choice после
  Plan 97 Ф.3 / D142 (см. amendment ниже + D87 amendment).
  Раньше использовался `Handler[E]` — снят clean-break'ом для
  симметрии с keyword'ом литерала.

- **Сохранение `protocol` для эффектов** — раздельный keyword `effect`
  снимает двусмысленность со structural-protocol'ами.

- **Финальное выражение без keyword'а как «implicit interrupt» для
  never-операций** — implicit поведение зависит от типа операции,
  AI-unfriendly. Явный `interrupt` для never и `return`/финальное
  выражение для остальных — однозначно.

### Связь

- [D2](#d2-эффекты-вместо-ключевых-слов-asyncthrowsunsafe) — концепция
  эффектов вместо keyword'ов.
- [D10](01-philosophy.md#d10) — «всё — эффект» как центральная ставка.
- [D11](#d11-имена-эффектов-и-синтаксис-with) — синтаксис `with X = h { body }`.
- [D18](#d18-эффекты-объявляются-через-kind-токен-не-голый-type) — отменено в части
  «через protocol»; эффекты теперь через `effect`.
- [D25](#d25-throw-и-параметризация-throwse) — `Fail[E]` как эффект.
  D61 формализует, что Fail-handler использует `interrupt` (не resume).
- [D31](#d31-handler-лямбда-для-эффектов-с-одной-операцией) — handler-лямбда
  для одно-операционных эффектов. Сохраняется как сахар над `effect X { ... }`.
- [D40](03-syntax.md#d40) — handler-method body имеет две формы (`=> expr`
  или `{ block }`), как `fn`.
- [D53](02-types.md#d53) — `protocol` остаётся для structural-интерфейсов.
  D61 расщепляет: `protocol` для типов значений, `effect` для эффектов.
- [02-types.md → D55](02-types.md#d55) — literal coercion применяется к
  параметрам операций как обычно.
- [03-syntax.md → D23](03-syntax.md#d23) — tail-position для `return`.
  D61 расширяет правило на `return`/`interrupt` в handler-method'ах.
- [06-concurrency.md → D80](06-concurrency.md#d80) — handler scoping
  per-fiber. Семантика `with X = h { body }` локальна для текущего
  fiber'а; через `spawn` наследуется snapshot. D80 — runtime invariant
  поверх D61.

### Цена

1. **Sweep по spec и examples.** ~30+ файлов содержат `protocol` для
   эффектов (`type Db effect { ... }`) — переписать на `effect`.
   Handler-литералы (`Db { query(q) => ... }`) → `effect Db { ... }`.
   Fail-handler'ы и другие, которые не делали resume — добавить
   `interrupt` явно.

2. **Bootstrap-компилятор требует доработки.** Сейчас (на момент D61):
   - `effect` keyword не парсится — пока используется `protocol`.
   - `handler` keyword не парсится — handler-литерал распознаётся
     эвристикой по `Ident (` после `{`.
   - `interrupt` keyword не парсится — нет в lexer'е.
   - `Effect[E]` тип не понимается type checker'ом — это просто
     dynamic-typed value.
   - Прямой `h.op(args)` не реализован.

3. **Линтер `interrupt` для Fail-handler'ов** — нужен, иначе старые
   `(err) => -1` без interrupt'а будут проходить парсер, но
   семантически ломаются.

### Эволюция

D61 — закрывающий блок системы эффектов. Закрывает Q-resume-semantics
(в варианте (II) tail-only) и Q-handler-method-param-inference (в
варианте (A) inference из protocol-сигнатуры). Также явно фиксирует
расщепление `protocol`/`effect` (раньше намеренно объединённое в D53,
но опыт показал — разные контракты, нужны разные keyword'ы).

Альтернативы которые рассматривались:
- `resume v` (Koka-стандарт) — отвергнут, перегружает понятие для
  пользователя без опыта алгебраических эффектов.
- `effect Db { ... }` для handler-литерала (двойное использование
  `effect`) — отвергнуто, путаница «тип/значение» через одно слово.
  **REVERTED 2026-05-22, см. [D142](02-types.md#d142)** — в Plan 97
  принято обратное решение: keyword `handler` отменён, литерал
  записывается через `effect X { ops }` (clean break). Симметрия с
  объявлением `type X effect { sigs }` оказалась важнее изоляции
  «тип/значение» через отдельное слово; декларация vs литерал теперь
  различаются позицией (`type ...` префикс / выражение или
  let-инициализатор).
- `Handler[E]` → `Effect[E]` — отвергнуто, тавтология.
  **REVERTED 2026-05-22, см. [D142](02-types.md#d142)** — переименован
  в `Effect[E, IRT]` для симметрии с keyword'ом литерала `effect`.
  Тавтология не подтвердилась практикой: `Effect[Db]` читается как
  «значение-effect для эффекта Db» — то же отношение «тип/контекст»,
  что `[]T` (массив элементов типа T) или `Option[T]`. См. [D87](#d87)
  для обновлённого определения.
- (I) полная resume-семантика — отложена до Q-multishot-resume,
  backend-фокус Nova не требует.

### Plan 97 amendment (2026-05-22) — handler keyword retired

**Pre-D142 status:** keyword `handler` парсился для handler-литерала
(`handler Db { query(q) => ... }`), тип значения `Handler[E, IRT]`.

**D142 (post-Plan 97 Ф.3):** keyword `handler` снят. Литерал
записывается через **тот же keyword `effect`**, что и объявление,
с дисамбигуацией по позиции:

```nova
// Объявление (как было)
type Db effect {
    query(q Sql) Fail[DbError] -> []DbRow
}

// Литерал (изменилось: handler → effect)
ro pg = effect Db {
    query(q) => real_query(q)
}

// Тип значения (переименован Handler → Effect)
fn run(h Effect[Db]) -> () => with Db = h { ... }
```

Парсер различает декларацию и литерал по leading-keyword'у `type`:
`type X effect { ... }` — declaration, `effect X { ... }` (без
`type`) — literal. То же правило, что для `protocol` (см. D53 + D142
+ [D143](03-syntax.md#d143)).

Clean break — миграция через sweep одной CL'ой (`nova_tests/**`,
`std/**`, `examples/**`, `spec/**`). Backwards-compat не сохраняется.

---

## D62. Прагматичная семантика эффектов: прямые в сигнатуре, Fail strict, Async ambient, правило effect/protocol

### Что

Финальная ревизия философии эффектов после большой дискуссии о
транзитивности, Async, Mut, и правиле выбора `effect`/`protocol`.
Закрывающий блок этой темы.

Четыре связанных решения:

1. **Прямые эффекты в сигнатуре**, не транзитивные. Функция объявляет
   только те эффекты, чьи операции она использует **сама**, не через
   вложенные вызовы.
2. **`Fail` strict**. Эффект `Fail[E]` обязателен в сигнатуре везде,
   где может произойти throw — прямой `throw e` или `expr?` (который
   desugar'ится в throw). Транзитивный throw через границы вызовов
   тоже требует `Fail` в сигнатуре caller'а. Это **исключение** из
   правила «прямые эффекты».
3. **`Async` — ambient capability**. Не пишется в сигнатурах, не
   является частью type system'ы. Fiber-runtime — реализационный
   механизм под капотом.
4. **Правило выбора `effect`/`protocol`** для программиста — два
   вопроса. Сознательный выбор; compile-time enforcement = последствие.
5. **`Mut[T]` убран** из стандартного набора эффектов. Реальные
   use-case'ы покрываются специализированными эффектами или
   локальными `let mut`.

Это **большая ревизия философии**. Ослабляется R5.2 «сигнатура =
полное описание»: теперь сигнатура показывает только прямые эффекты
+ Fail транзитивно. Транзитивные эффекты других типов — лишь
warning'ом подсвечиваются. R6 capability-режим ослабляется
аналогично.

### Правило 1. Прямые эффекты в сигнатуре

#### Что считается «прямым» использованием

Функция использует эффект **прямо** (и обязана его декларировать),
если в её собственном теле:

- Вызывается операция эффекта: `Db.exec(...)`, `Log.info(...)`.
- Используется keyword-сахар, разворачивающийся в операцию эффекта:
  `throw e` ⇒ `Fail[E].fail(e)`, `expr?` ⇒ throw на ошибке.

Функция использует эффект **транзитивно** (НЕ обязана декларировать,
но есть warning), если:

- Вызывает другую функцию, в чьей сигнатуре эффект объявлен.
- Транзитивный throw через границы — **исключение**, см. правило 2.

```nova
fn save(u User) Db -> () {
    Db.exec(...)              // прямое использование Db — Db в сигнатуре
}

fn helper(u User) -> () {
    save(u)                    // транзитивное Db — warning, можно подавить
}
```

#### Семантика проверки

Type checker:
- Прямой эффект **не объявлен** в сигнатуре → **compile error**
- Транзитивный эффект **не объявлен** → **warning** (suppressable)
- Активный handler в runtime отсутствует на момент операции →
  **runtime fail** (panic)

#### Подавление warning'а

Программист может явно подавить warning:

```nova
#allow_transit(Db, Log)
fn helper(u User) -> () {
    save(u)         // save имеет Db Log, но helper не объявляет — без warning
}
```

Или через настройку `Nova.toml` для проекта:

```toml
[lints]
transit_effects = "off"        # disable warnings for whole project
# или
transit_effects = "error"      # treat as compile error (strict mode)
```

Программист контролирует уровень дисциплины для своего кода.

### Правило 2. `Fail` strict — исключение из «прямых»

`Fail[E]` обязателен в сигнатуре функции **всегда**, когда внутри
неё может произойти throw — прямой или транзитивный:

- Прямой `throw e` или `expr?` в теле → `Fail[E]` в сигнатуре. Иначе
  compile error.
- Транзитивный throw через вызов функции с `Fail[E']` в её сигнатуре
  → `Fail[E']` (или совместимая) в сигнатуре caller'а. Иначе
  **compile error**, не warning.

```nova
fn parse(s str) Fail[ParseError] -> int {
    if invalid(s) { throw ParseError.Bad }     // прямой throw — Fail обязан
    ...
}

fn pipeline(s str) Fail[ParseError] -> int {
    ro n = parse(s)?                          // ? = throw — Fail обязан
    n
}

fn caller(s str) Fail[ParseError] -> int {     // ОБЯЗАН Fail (transit)
    pipeline(s) + 1
}

fn caller(s str) -> int {                       // COMPILE ERROR
    pipeline(s) + 1                             // pipeline может throw, не объявлено
}
```

#### Почему Fail — исключение

Throw это **изменение control-flow**, не side-effect. Программист
обязан знать что вызов может «не вернуться нормально», иначе
происходят bugs типа Java RuntimeException — невидимые crash'и.
Это центральный аргумент checked exceptions из Java и `Result<T,E>`
из Rust.

В Nova `Fail[E]` — типизированная версия checked-throw. Транзитивность
сохраняется, чтобы caller всегда знал «может бросить — обработай
или объяви».

Остальные эффекты (Db, Log, Time, ...) — не control-flow, а
side-effects. Они меняют мир, но не ломают возврат значения.
Программист может позволить себе их не отслеживать транзитивно.

#### Альтернатива через with

Если caller хочет обработать throw локально и **не** объявлять Fail:

```nova
fn caller(s str) -> int {
    with Fail[ParseError] = |e| interrupt 0 {
        pipeline(s) + 1
    }
}
```

Handler ловит throw, дает дефолт. caller возвращает int, без Fail
в сигнатуре.

### Правило 3. `Async` — ambient capability

`Async` **не является эффектом** в Nova. Не пишется в сигнатурах, не
является частью type system'ы.

```nova
fn fetch(url str) Net -> Response {        // НЕТ Async
    Net.get(url)
}

fn double(x int) -> int => x * 2           // тоже без Async
```

Под капотом — fiber-based scheduler. Функции могут suspend на
yield-point'ах (network, sleep, channel.recv, async-Db). Это
**деталь реализации**, не контракт типа.

Цвета функции нет — нет деления sync/async. Программист пишет код,
fiber-runtime сам решает где можно вытесняться.

`spawn`, `parallel for`, `supervised` (+`deadline:`/`timeout:` — D408,
субсумировали ex-`with_timeout`), `race` (stdlib) — остаются как
**runtime-конструкции** (keyword'и или библиотечные функции), не как
эффекты:

```nova
// Гомогенный fan-out — массив результатов через parallel for.
fn fetch_dashboard(uid int) Net Fail -> Dashboard {
    ro users_and_posts = parallel for kind in ["users", "posts"] {
        fetch_section(uid, kind)
    }
    Dashboard.ok(users_and_posts)
}

// Гетерогенная параллельность — mut-захваты в supervised.
fn handle_request(req Request) Net Db -> Response {
    mut users = []
    mut posts = []
    supervised {
        spawn { users = fetch_users() }     // spawn — fire-and-forget statement
        spawn { posts = fetch_posts() }
    }
    Response.ok(users, posts)
}
```

`spawn body` сам по себе **возвращает unit** — не результат body.
Результат — только через прямой вызов (async прозрачный), `parallel
for` (массив) или mut-захваты (см. [D50 п. 2](../decisions/06-concurrency.md#d50)).

#### Почему так

В backend-коде Nova **почти каждая нетривиальная функция async** —
ходит в Db, Net, sleep'ит. Если `Async` в сигнатуре — он там
**везде**. Информативность нулевая. Это шум.

Решение: убрать `Async` из типов. Программист не пишет, не выводит,
не помнит. Fiber-runtime просто работает.

Прецедент: Go (горутины могут вытесняться где угодно, нет `async`-
keyword'а), Erlang/Elixir (то же). Async в типах остаётся в Rust
(где async важен из-за no-runtime), C# (где async из-за callbacks),
Koka (academic effects). В Nova не нужен.

### Правило 4. `effect` vs `protocol` — критерий resource-capability

#### Формулировка

> **Эффект описывает resource-capability — нечто, что можно подменить
> handler'ом в скоупе. Suspension и runtime-механизмы — не resource,
> а ambient mechanic, общая для всех асинхронных операций; они НЕ
> эффекты.**

Resource-capability — концептуальная единица, к которой имеет смысл
question «может ли это быть подменено в тесте?». Если **да** — это
effect (handler-substitution). Если **нет** — это либо runtime-mechanic
(не существует в типах), либо обычный protocol на значении.

Применение к стандартным эффектам:

| Эффект | Resource-capability? | Подменяется в тесте? |
|---|---|---|
| `Time` | clock | `fixed_ms(ms)` ✓ — фиксированный момент; `mut_clock(start_ms)` ✓ — sleep продвигает виртуальное время |
| `Random` | RNG | `seeded(seed)` ✓ — xoshiro256++ deterministic PRNG |
| `Db`/`Net`/`Fs` | соединение/socket/fd | in-memory handler ✓ |
| `Mem` | alloc counter | mock-counter (для leak-тестов) ✓ |
| `Detach` | background supervisor | `SyncDetach` ✓ |
| `Blocking` | OS-thread pool | mock ✓ |
| `Async` | fiber scheduler | **не подменяется** (runtime mechanic) — НЕ effect |

> **Источник test-handler'ов:** `std/testing/handlers.nv` экспортирует
> `seeded(seed u64) -> Effect[Random]` (xoshiro256++ — tier с Go
> math/rand v2 PCG и Rust `rand` ChaCha8), `fixed_ms(ms u64) -> Effect[Time]`,
> `mut_clock(start_ms u64) -> Effect[Time]`. Production-handler'ы
> (`secure()` CSPRNG, `system_clock()` realtime) — отдельный план
> (требуют runtime hooks: BCryptGenRandom/getrandom + libuv).

#### Decision flow для программиста

В Nova два разных способа описать «что-то с операциями»:

- **«Как делать что-то»** — функция объявляет, что ей нужны
  такие-то операции, а какая реализация будет под ними — решает
  вызывающий код через `with`-блок (например, для прода —
  Postgres, для теста — in-memory). Это **эффект**, объявляется
  через `type X effect { ... }`.
- **«Что умеет значение»** — реализация жёстко привязана к типу:
  `int` хешируется так-то, `str` — так-то, и менять это нельзя.
  Это **протокол**, объявляется через `type X protocol { ... }`.

**Когда использовать эффект, а когда протокол в коде:** если
хочется при тестировании использовать другую реализацию — это
эффект. Если при тестировании мы просто работаем со значениями
типа, и подменять там нечего — это протокол.

Особый случай — **runtime mechanic** (`Async`/fiber scheduler,
GC/region) — в типах не объявляется ни как `effect`, ни как
`protocol`. См. `Async`, `Mem`/`Trace` instrumental эффекты в
[D26](08-runtime.md#d26).

#### Decision matrix — канонические случаи

| Тип / контракт | Resource? | Continuation? | Решение | Why |
|---|---|---|---|---|
| **Структурные protocols (значения)** | | | | |
| `Hash` | нет (у каждого значения свой hash) | нет | `protocol` | bound на T в `HashMap[K Hash, V]` |
| `Ord` | нет | нет | `protocol` | bound в priority queue, сортировке |
| `Eq` | нет | нет | `protocol` | bound в множествах |
| `Iter[T]` | нет (конкретный итератор) | нет | `protocol` | for-in / collect через D58 |
| `From[T]` / `Into[T]` | нет | нет | `protocol` | conversion ([D73](08-runtime.md#d73)) |
| `TryFrom[T,E]` | нет | нет | `protocol` | fallible conversion ([D77](08-runtime.md#d77)) |
| **Resource-capabilities (effects)** | | | | |
| `Db` | соединение к БД | нет | `effect` | mock в тестах через `with Db = ...` |
| `Net` | сокет/HTTP-клиент | нет | `effect` | recorded responses |
| `Fs` | файловая система | нет | `effect` | virtual-fs handler |
| `Time` | clock | нет | `effect` | `fixed_ms(...)` ✓ (uuid v7, jwt); `mut_clock(...)` ✓ (rate_limiter, retry, cron — advance via sleep) |
| `Random` | RNG | нет | `effect` | `seeded(...)` ✓ — xoshiro256++ (uuid v4, ulid, snowflake, bcrypt) |
| `Log` | logger sink | нет | `effect` | capture-log в тестах |
| `Trace` | distributed tracer | нет | `effect` | в-memory trace |
| `Io` | stdout/stderr | нет | `effect` | mock-stdout |
| `Cache[K,V]` | кэш-провайдер | нет | `effect` | in-memory mock |
| `Authn`/`Authz` | identity / capability | нет | `effect` | fixed-user в тестах |
| `Idempotency` | dedup-store | нет | `effect` | in-memory mock |
| **Continuation-effects** | | | | |
| `Fail[E]` | error reporter | **да** (throw → never) | `effect` | один на язык, особый |
| **Resource + instrumental** | | | | |
| `Mem` | alloc counter | нет | `effect` (instrumental) | observability, ambient ([D26](08-runtime.md#d26)) |
| **Не существует в типах** | | | | |
| `Async` | fiber scheduler | — | **runtime mechanic** | suspension ambient (D14/D62) |
| GC / region | memory allocator | — | **runtime mechanic** | implicit ([D6](05-memory.md#d6)) |

#### Кейсы где границы нечёткие

- **`Logger` как protocol**: возможно, если используется через
  `fn f(log Logger)` parameter passing без mock. Но 99% случаев —
  effect (тесты подменяют). Default — `effect`.

- **`Compare` vs `Ord` effect**: `Ord` всегда protocol (bound).
  Если нужно «глобальный compare-handler в тесте» — это **очень
  редкий** use-case, лучше через named-fn-параметр.

- **`Cache[K,V]`**: effect, потому что нужен mock в тестах (бесплатный
  `with Cache = noop_cache`). Если cache — value-handle (как
  Channel), то protocol; но обычно — handler-driven.

#### Аналогия со статическим классом

`effect` — это как **статический класс с методами** в C#/Java
(`Math.sqrt`, `Math.abs`): нет инстансов, методы вызываются через имя
(`Db.query(...)`). В отличие от обычного статического класса, у
`effect`:
- Реализация **подменяется** через `with` (статический класс не
  подменяется без рефлексии).
- Operations могут **захватывать continuation** через `throw` /
  `interrupt`.

Если эти два свойства не нужны — это просто `protocol` на инстансе,
не `effect`.

#### Compile-time enforcement = последствие

Type checker ловит несоответствие:

- Тип объявлен через `effect` — используется в effect-position
  сигнатуры (`fn f() Db -> ...`), operations через имя `X.op(...)`.
- Тип объявлен через `protocol` — используется в позиции значения
  (параметр, поле, generic-bound), operations через инстанс
  `x.op(...)`.

Смешение — compile error. Качество ошибок:

```
error: `Db` is an effect, not a protocol-bound
  in fn f[T Db](x T)
                ^^ effect cannot appear as type-bound
  hint: use effect-position instead:
        fn f(x T) Db -> ...
```

Это **gatekeeper**, ловит ошибки выбора; не диктует, какой выбор
делать.

### Правило 5. `Mut[T]` убран из стандартного набора

`Mut[T]` как generic эффект **не существует** в стандартной библиотеке
Nova. Реальные сценарии mut-state покрываются:

- **Локальные `let mut x`** — обычная mutable переменная, без эффекта.
- **Глобальное мутабельное состояние** — через специализированные
  effect'ы (`Counter`, `Cache`, `IdGen`, etc.) с понятными именами и
  operations.
- **Атомарные счётчики, mutex'ы** — `Atomic[T]`, `Mutex[T]` как
  тип-значения, не эффекты.

Каждый раз когда возникает соблазн «нужен Mut[T]», есть лучшая
альтернатива: дать состоянию **имя** через специализированный
эффект.

Если когда-то понадобится истинно generic Mut[T] — добавится
отдельным D-блоком. На данный момент — не нужен.

### Что меняется в R-главах

#### R5.2 «Сигнатура = полное описание»

**Было:** «по сигнатуре функции LLM/человек знает все побочные
действия».

**Стало:** «сигнатура показывает прямые эффекты функции + Fail
транзитивно. Side-effects через вложенные вызовы транзитивно
warning'ом подсвечиваются — программист обязан знать, но не обязан
писать».

Это сознательное ослабление ради компактности сигнатур в реальном
backend-коде. Полная карта эффектов — расчётный артефакт, не часть
spec'а.

#### R5.6 «Self-describing API»

**Было:** «по сигнатурам модуля видна полная карта эффектов».

**Стало:** «по сигнатурам видна карта прямых эффектов + полный
throw-граф через Fail». IDE/линтер дают полную транзитивную карту
по запросу.

#### R6 «Capability-режим»

**Было:** «функция без `Net` в сигнатуре физически не может ходить
в сеть».

**Стало:** «функция без `Net` в сигнатуре прямо ходить в сеть не
может; через вложенные вызовы — может, если их сигнатуры это
допускают». Реальная capability-sandbox реализуется на closure-
границах (декларация `fn() -> T` для callback'а гарантирует что
callback ничего не делает) или через явный whitelist эффектов
проекта (Nova.toml). Compile-time гарантия не транзитивная.

#### R7 «Async — эффект, не вирус»

**Было:** «Async — обычный эффект в сигнатуре. Без Future<T> в типе.»

**Стало:** «Async — невидимая инфраструктура. Не часть типа. Цвета
функции нет. Fiber-runtime под капотом. Программист не пишет, не
видит, не помнит». Глава переименована в «Fiber runtime —
прозрачный async».

#### R3 «Детерминированный режим тестирования»

**Было:** «любую программу можно запустить полностью детерминированно,
если все эффекты заменены».

**Стало:** «программу можно запустить детерминированно, заменив
все используемые эффекты. IDE подсказывает какие эффекты вовлечены
по транзитивному графу. Compile-time гарантия только для прямых».

### Что НЕ меняется

- **Грамматика effect-row** в сигнатуре — без изменений.
- **D11** with-синтаксис — без изменений.
- **D25** Fail/throw/? — без изменений семантики, только подтверждается
  что Fail транзитивен.
- **D31** handler-литералы — без изменений.
- **D61** effect/handler keywords, interrupt, Effect[E] — без
  изменений. D62 это **философское** уточнение, не синтаксическое.

### Стандартный набор эффектов (после D62)

```
| Эффект     | Что описывает                         |
|------------|---------------------------------------|
| Fail[E]    | Контракт для перехвата и обработки ошибки типа E |
| Io         | stdin/stdout/stderr                   |
| Fs         | Файловая система                      |
| Net        | Сетевые запросы                       |
| Db         | Базы данных                           |
| Time       | Часы, таймеры, задержки               |
| Random     | RNG                                   |
| Log        | Структурированный лог                 |
| Trace      | Распределённая трассировка            |
| Ask[T]     | Чтение из контекста (Reader)          |
| Alloc[R]   | Аллокация в регионе R                 |
```

Убраны: `Async` (ambient), `Mut` (специализированные эффекты вместо),
`Par` (runtime-keyword, не эффект).

### Почему

1. **Прагматизм vs дидактика.** Полная транзитивность даёт
   максимально честные сигнатуры, но в реальном backend-коде
   эффект-row растёт до 8-10 имён, что тяжело читать. Прямые
   эффекты + Fail strict — баланс.

2. **AI-first сохраняется частично.** LLM по сигнатуре всё ещё
   знает прямое использование функции и полную throw-картину.
   Транзитивные side-effects через помощь IDE — не трагедия для
   AI, который и так читает несколько уровней.

3. **Async как ambient — единственный разумный выбор.** В
   backend-коде он везде. Если он эффект — он шум. Если ambient —
   программисту не надо думать. Прецедент: Go.

4. **Mut[T] не нужен.** Каждый раз когда возникает идея «mut-cell» —
   правильнее дать ей имя. Generic Mut[T] провоцирует анти-паттерн
   «безымянное shared state».

5. **`effect`/`protocol` правило через подмену.** Sniff-test
   «подменяю ли через with в тестах» — практически проверяемый
   критерий, не философская абстракция.

6. **R5.2 ослабление обоснованно.** Чистая транзитивность в
   эффектах не существует ни в одном мейнстрим-языке. Nova остаётся
   впереди других языков (Java, Go, Python) в плане видимости
   throw + прямых эффектов, но не пытается решить «полную карту
   через типы», что неподъёмно для production-кода.

### Что отвергнуто

- **Полная транзитивность всех эффектов** — обоснованно для
  революционной заявки, но громоздко в реальном коде. Принят
  компромисс «прямые + Fail strict».
- **`..E` row-tail polymorphism** — не нужен с прямыми эффектами.
  Closure-параметры не пробрасывают эффекты caller'у.
- **`Async` как явный эффект** — везде в backend, шум.
- **`Mut[T]` как generic эффект** — анти-паттерн «безымянное
  shared state», предпочтительны специализированные.
- **Полное удаление эффектов из сигнатур** (Java/Python style) —
  теряется проверка throw, теряется handler-substitution-видимость.
  Не идём так далеко.
- **Compile-time гарантия capability через все границы** — только
  на closure-границах с явной декларацией. Полная транзитивная
  capability-sandbox не дается типами.

### Связь

- [D2](#d2-эффекты-вместо-ключевых-слов-asyncthrowsunsafe),
  [D3](#d3-синтаксис-эффектов-типы-между--и--) — синтаксис
  effect-row, без изменений.
- [D11](#d11-имена-эффектов-и-синтаксис-with) — `with` синтаксис.
- [D25](#d25-throw-и-параметризация-faile) — Fail/throw/?, теперь
  явно strict-транзитивный.
- [D28](#d28-вывод-эффектов-private--выводится-public--обязательно-явно) —
  effect inference, теперь только для **прямых** эффектов в private.
  Транзитивных нет.
- [D61](#d61-полная-семантика-эффектов-effect-keyword-handler-литерал-handlere-interrupt) —
  effect/handler keywords, Effect[E], interrupt. D62 это
  философское уточнение D61.
- [01-philosophy.md → D10](01-philosophy.md#d10) — AI-first
  пересмотрен в R-главах.
- [revolutionary.md](../revolutionary.md) — R2/R3/R5.2/R5.6/R6/R7
  обновлены.

### Цена

1. **Sweep по spec и examples** — убрать `Async` из всех сигнатур
  (~30+ мест). Перепроверить что в сигнатурах только прямые эффекты
  (большинство уже так — реальные функции используют свои эффекты
  напрямую).
2. **Bootstrap-компилятор**: warning для транзитивных эффектов,
  strict для Fail. Атрибут `#allow_transit` в парсере (опционально).
3. **R-главы переписать** — революционная заявка ослабляется. Это
  важно для маркетинга/документации, README.

### Эволюция

D62 финализирует длительную дискуссию о транзитивности эффектов.
Изначально (до D62) Nova была транзитивной по всем эффектам — что
обоснованно для «AI-first язык где сигнатура говорит правду».
Опыт с реальными примерами (effect-density/) показал что сигнатуры
накапливают 8-10 эффектов, нечитаемые. Обсуждалось row polymorphism
(`..E`), но это сложно в type checker'е. Финальное решение:
прямые + Fail strict — баланс компактности и проверки control-flow.

`Async` всегда был спорным эффектом — везде в backend-коде, шум.
D62 переводит его в ambient capability. Глава R7 переписывается:
«не эффект-не-вирус», а «вообще не часть типа».

`Mut[T]` упоминался в R2 списке эффектов, но не имел реальных
use-case'ов. Каждый раз когда возникал — оказывался лучше через
специализированный эффект. D62 убирает его.

> **✅ ENFORCED (Plan 221.1 №131, 2026-07-26):** «Использование операции
> эффекта прямо в теле» (§Правило вывода п.1 в [D28](#d28-вывод-эффектов-
> private--выводится-public--обязательно-явно)) означает буквально
> **любой** вызов вида `Effect.op(...)` — включая СЫРОЙ вызов операции
> эффекта, без промежуточной именованной fn, чья СОБСТВЕННАЯ сигнатура
> несёт этот эффект. До этого энфорса чекер проверял эффект-строки ТОЛЬКО
> на рёбрах вызова именованной функции (`self.sig.fn_decls`/
> `method_table`) — сырой оп (`Log.info("x")` прямо в теле, без вызова
> обёрточной fn) проходил `check`/`--strict-effects` без единой ошибки
> или warning'а, ноль исключений (найдено эмпирически: opus design-note
> `docs/plans/222.20-design-note.md` §0 Ф-C, пробы 4b–4j на релиз-бинаре —
> `fn f() -> int { Log.info("x"); 0 }` с чистой сигнатурой PASS). D2-
> ценность эффект-подписей была дырява ровно для этого call-shape: «явная
> декларация обязательна» — декорация, не гарантия. Решение владельца
> 2026-07-26 (реестр №131): закрыть по D62, не узаконивать амендментом.
>
> **Механика:** `[E_RAW_EFFECT_OP_UNDECLARED]` (`compiler-codegen/src/
> types/mod.rs` — `check_capabilities_at`'s "1. Effect-op call" branch →
> `check_raw_effect_op_declared`, рядом с существующим
> `check_transitive_effect_strict`). **Scope — ТА ЖЕ граница, что у D85
> (№113, `E_BANG_REQUIRES_FAIL`):** энфорс ТОЛЬКО для `export fn`
> (`CapState.is_export`) — это баллы «прямой эффект обязателен» именно
> для **публичного** API (D28 §"Public API — почему обязательно явно").
> Приватные fn (`!is_export`, а также `main` — которая в этой кодовой
> базе объявляется bare `fn main()`, НЕ `export fn`, см.
> `examples/flagship/aggregator/src/main.nv`) остаются под D28
> auto-inference: `infer_effects` (тот же проход, что уже молча
> подставляет `Fail` при `throw`) теперь ТАКЖЕ молча добавляет в
> сигнатуру приватной fn имя каждого эффекта, чья операция вызвана прямо
> в её теле (`collect_raw_effect_ops_in_fn`/`raw_effect_op_head`,
> `types/mod.rs`) — «этот эффект добавляется», не warning. **Локальный
> `with EffectName = … { … }` вокруг вызова гасит обязательство** (то же
> D11/Правило-4 рассуждение, что у транзитивного класса) — `export fn`
> без `EffectName` в собственной строке, но с обвязывающим `with`,
> легален. **Test-блоки** exempt той же границей, что и транзитивный
> класс (`state.effect_root` — де-факто покрыто уже тем, что тестовый
> корень никогда не `is_export`). **Handler-литералы** (`effect X { op()
> => … }`) — тело опа НЕ проверяется этим правилом как отдельная
> «сигнатура»: у него нет `export`/`fn`-декларации, которую можно было
> бы нарушить; сырой оп ДРУГОГО эффекта внутри тела опа принадлежит
> лексическому контексту УСТАНОВКИ (`with`-обвязка снаружи), не самому
> телу — уже существовавшее поведение чекера (`ExprKind::HandlerLit`
> пропускается этим walker'ом безусловно), сохранено НЕ тронутым (проба
> `p_effect_in_effect`, design-note §0: «Time из лексического окружения
> main», зелёная и после этого окна).
>
> **`#default_handler` (D431) НЕ обсуждает и не отменяет это правило.**
> D431-текст (см. секцию ниже) описывает ТОЛЬКО runtime-механизм
> lazy-конструирования дефолта — «ambient lazy default-handler factory»;
> НИГДЕ не заявляет снятие STATIC-требования декларации. Пробный вывод
> design-note (§0 Ф-E, проба 3: `greet() Log` с `#default_handler(Log)`,
> вызванная из чистой `main`, — FAIL) корректен и ожидаем, НЕ вторая
> дыра — просто явное экспериментальное подтверждение того, что текст
> D431 и так предполагал.
>
> **Миграция (`nova`, та же волна):** `std/src/io/console.nv` — три
> экспортных conformer-метода `Stdin mut @read`/`Stdout mut @write`/
> `Stderr mut @write` звали `Io.read_in`/`Io.write_out`/`Io.write_err`
> сырыми опами без `Io` в собственной строке; `io.Read`/`io.Write`
> (`std/src/io/core.nv`) — протоколы **effect-agnostic by design**
> (module-doc: «a conformer carries its OWN plumbing effect»,
> Q15/D122) — тот же паттерн, что `TcpStream`/`File` уже применяют
> (`Net`/`Fs` прямо в сигнатуре `@read`/`@write`, `std/src/net/tcp.nv`,
> `std/src/fs/fs.nv`) — добавление `Io` НЕ нарушение протокола, а тот же
> устоявшийся канон. `spec_tests/`/`examples/` — 0 находок (мега-CU
> `spec_tests/conformance` + флагман `--strict-effects` зелёные без
> дополнительных правок). **Внешние репы** (`nova-polaris`/`nova-http`/
> `nova-tls`, НЕ мигрированы этим слиянием — интегратор их отдельно):
> `nova-polaris` — ровно один экспортный сайт,
> `BackgroundTasks.@drain()` (`src/background.nv:137`, `Log.error(...)`
> внутри `if failed { … }`); `nova-http`/`nova-tls` — 0 (единственный
> кандидат в каждом — `nova-http/src/client/client.nv`'s
> `http_seam_send`/`nova-tls/src/stream.nv`'s `fill_from_tcp` — оба уже
> ПРИВАТНЫЕ и уже несут `Http`/`Net` явно в своей строке).

---

## D65. Полная семантика `Fail`: гибрид `Fail[E]` / `Fail`, lookup, prelude `RuntimeError` и `Error`

### Что

Закрывающий блок по теме обработки ошибок. Объединяет четыре связанных
решения:

1. **Гибридная параметризация `Fail`** — `Fail[E]` типизированный
   (рекомендуется для public API) и `Fail` без параметра как сахар
   для `Fail[any]` (catch-all, quick-and-dirty).
2. **Subtype-aware lookup при throw**: точный тип E → `Fail` (any) →
   runtime panic. Match по конкретным вариантам sum — внутри
   handler'а через обычный `match`.
3. **Re-throw** внутри handler'а через `throw expr` — ищется outer
   handler в стеке.
4. **Prelude-типы** для runtime-ошибок: sum-тип `RuntimeError` с
   фиксированным набором вариантов + record `Error { msg }` для
   пользовательских ошибок с сообщением.

D65 заменяет ранее существовавший unit-маркер `type Error` в prelude
([D26](08-runtime.md#d26)) на полноценный record. Также формализует
лукап-правило handler'ов для `Fail`, которое раньше было implicit.

### Правило 1. Гибридная параметризация: `Fail[E]` или `Fail` ≡ `Fail[any]`

```nova
// Типизированный — рекомендуется для public API
fn parse(s str) Fail[ParseError] -> int {
    if invalid(s) { throw ParseError.Bad }
}

// Сахар — Fail ≡ Fail[any], catch-all
fn quick_helper(s str) Fail -> int {
    if bad { throw "raw string error" }
}

// Generic с явным [E] параметром — типизация через generics
fn retry[T, E](attempts int, body fn() Fail[E] -> T) Time Fail[E] -> T

// Caller:
retry(3, || parse("..."))
//              ↑ возвращает Fail[ParseError] → E = ParseError
//                retry имеет signature Fail[ParseError]
```

Семантика — две формы:

| Форма | Семантика | Use-case |
|---|---|---|
| `Fail[E]` | **typed** — точный тип ошибки | public API, библиотечный код |
| `Fail` ≡ `Fail[any]` | **catch-all** — сахар над erasure-формой; ловит `throw` любого типа | private fn, quick scripts, top-level supervisors |

`Fail` без параметра — синтаксический сахар над `Fail[any]`. Одна
форма с одной семантикой; никакой placeholder-инференс E не делается.
Если программисту нужна типизация — пишет `Fail[E]` явно (или
использует generic-параметр `[E]` как в `retry` выше).

Convention (рекомендация, частично enforce'ится линтером):

| Контекст | Форма | Линт |
|---|---|---|
| `export` (public API) | `Fail[E]` с конкретным типом | warning если `Fail` без E |
| Library, переиспользуемый код | `Fail[E]` | warning |
| Internal/private helper | `Fail[E]` или `Fail` | ok |
| Quick-and-dirty / scripts / тесты | `Fail` | ok |
| Generic в `retry`, `transaction` | `Fail[E]` через `[E]` | ok |
| Catch-all logger / supervisor | `Fail` или `Fail[any]` | ok (намеренный паттерн) |

Линтер может предупреждать «public-fn использует `Fail` без параметра» —
suppressable через настройку проекта.

#### Зачем `Fail` без параметра — catch-all use-case

`Fail` (sugar над `Fail[any]`) — не косметика. Это **отдельная семантика
catch-all handler'а**, без которой не выражаются три canonical паттерна:

**1. Top-level supervisor:**

```nova
fn main() Io -> () {
    with Fail = |e| Log.error("uncaught: ${e}") {
        run_app()
    }
}
```

`run_app()` может бросать любые `Fail[E1]`, `Fail[E2]`, ... — все
ловятся одним handler'ом. Без `Fail` (any) пришлось бы перечислять
все типы ошибок, что невозможно для composable systems.

**2. Untrusted plugin / user code:**

```nova
fn run_plugin(p Plugin) -> Result[(), str] {
    with Fail = |e| interrupt Err(str.from(e)) {
        Ok(p.execute())
    }
}
```

Plugin может бросать что угодно (типы из его собственного кода,
неизвестные caller'у). Catch-all позволяет sandboxить.

**3. Quick scripts / REPL:**

```nova
fn quick_check() Fail -> int {
    ro n = parse(input)?     // Fail[ParseError]
    ro v = lookup(n)?        // Fail[LookupError]
    v + 1
}
```

В quick-and-dirty коде программист не хочет писать
`Fail[ParseError | LookupError]` — `Fail` достаточно.

#### Safety сохранена

Эффект `Fail` **остаётся видимым в сигнатуре** — главное свойство
системы эффектов не нарушено: caller знает, что функция может
бросить. Тип ошибки не указан — это compile-time рекомендация
(линт `export-fail-untyped`), не нарушение effect-safety.

#### Trade-off

| Форма | Use-case | Compile-time check |
|---|---|---|
| `Fail[E]` | typed business errors | exhaustive match по E |
| `Fail` ≡ `Fail[any]` | catch-all / supervisor / scripts | runtime `is`-check на handler-стороне |

Сознательный trade-off: catch-all теряет exhaustiveness в `match`
(handler получает значение типа `any`, программист использует
`is`-проверки или `str.from(e)`), взамен покрывает три use-case'а
выше.

#### Прецеденты

- **Java unchecked exceptions** (`RuntimeException`) — catch-all без
  typed checked exceptions. Известная проблема: catch-all **невидим**
  в сигнатуре. Nova решает: видим, но не типизирован.
- **Go `error` interface** — единственный тип ошибки, runtime-typed.
  Прямой аналог Nova `Fail[any]`.
- **Rust `Box<dyn Error>`** — explicit erasure для top-level error
  handling. Тоже прямой аналог.

### Правило 2. Lookup при `throw expr`

`throw expr` это keyword-сахар над операцией эффекта `Fail[E].fail(expr)`,
где `E = type-of(expr)`. Runtime ищет handler в стеке:

1. **Точное совпадение** — handler `Fail[E]` где E совпадает с типом
   значения. Если найден — вызывается.
2. **Catch-all** — handler `Fail` (≡ `Fail[any]`). Если найден — вызывается.
3. **Runtime panic** «no handler for Fail» — если ни один не найден.

Lookup идёт **сверху вниз стека** (свежие handler'ы первыми, как для
любого эффекта).

#### Match по sum-вариантам — внутри handler'а

Для перехвата конкретного варианта sum-типа использоваться **handler
один на тип** + match внутри:

```nova
type RuntimeError enum DivByZero | Overflow | IndexOutOfBounds

fn risky() Fail[RuntimeError] -> int {
    throw RuntimeError.DivByZero          // тип значения: RuntimeError
}

with Fail[RuntimeError] = |err| match err {
    DivByZero => interrupt 0
    Overflow  => interrupt MAX_INT
    _         => interrupt -1
} {
    risky()
}
```

Тип брошенного значения — `RuntimeError`, не `DivByZero` (`DivByZero` это
sum-вариант, не отдельный тип). Поэтому `Fail[DivByZero]` **не существует**
для этого случая. Один handler `Fail[RuntimeError]`, разбор внутри.

#### Subtype-aware lookup НЕ делается

Lookup проверяет точное совпадение типа, не subtype-relations.
`Fail[RuntimeError]` не ловит автоматически `Fail[DivByZero]` (если
`DivByZero` отдельный тип) и наоборот. Если нужна гибкость — программист
явно использует `Fail` (any) как catch-all.

Это сохраняет **локальное reasoning**: программист видит handler
`Fail[X]` и знает что он перехватывает только `throw expr` где
`type-of(expr) == X`.

### Правило 3. Re-throw для частичной обработки

`throw expr` внутри handler-method'а — это **обычная** операция эффекта
Fail. Runtime ищет handler в стеке, **минуя** текущий handler-frame
(текущий обрабатывает throw, не может ловить сам себя). Если outer
есть — он перехватит. Если нет — runtime panic.

```nova
with Fail[RuntimeError] = |err| interrupt log_and_default(err) {
    with Fail[RuntimeError] = |err| match err {
        DivByZero => interrupt 0       // обработали локально
        other     => throw other        // пробросили дальше — найдёт outer
    } {
        risky()
    }
}
```

Это позволяет:
- Обрабатывать **подмножество** sum-вариантов локально.
- Пропускать остальные дальше по стеку.
- Композиция handler'ов через nested-with.

### Правило 4. Prelude-типы для ошибок

#### `RuntimeError` — sum-тип runtime-сбоев

```nova
// в prelude (D26)
type RuntimeError
    | DivByZero
    | Overflow
    | IndexOutOfBounds { index int, length int }
    | TypeMismatch(str)
    | AssertFailed(str)
    | NoHandler(str)
```

Встроенные runtime-операции бросают конкретные варианты:

| Операция | Бросает |
|---|---|
| `a / b` (b == 0) | `RuntimeError.DivByZero` |
| `arr[i]` (i out of bounds) | `RuntimeError.IndexOutOfBounds { index: i, length: arr.len }` |
| `(x as Type)` (cast fail) | `RuntimeError.TypeMismatch("expected ..., got ...")` |
| `assert(cond)` (false) | `RuntimeError.AssertFailed("...")` |
| `Db.query(...)` (no handler) | `RuntimeError.NoHandler("Db")` |

**Переполнение знаковой целочисленной арифметики `int`** (`a + b`,
`a - b`, `a * b` за границами `int.MIN..int.MAX`) — **`panic`, не
`Fail`** (Plan 33.8 Ф.1.1, решение 2026-05-21). Не ловится в коде; как
`StackOverflow`/`OutOfMemory`. Причина: переполнение — баг программы, а
не ожидаемая ошибка; делать каждую арифметическую операцию эффектной
(`Fail` в сигнатуре) недопустимо эргономически. Sized-типы
(`u8`/`u16`/`u32`/`u64`/`i8`/`i16`/`i32`) — иная семантика: wrap-around
по модулю 2^N (см. Plan 33.7). Вариант `RuntimeError.Overflow`
сохранён в типе для явных checked-арифметических API stdlib, но
оператор `+` его НЕ бросает.

> **AMEND (Plan 140.4, 2026-06-14) — элизия пруфом.** Этот overflow-`panic`
> always-on (debug И release), но **Z3-доказуемо-безопасные** операции
> элидируют checked-форму (zero-cost) — модель enforce-with-elision
> [D24](09-tooling.md)/[D272](09-tooling.md). Элизия **только пруфом** (доказать
> `INT64_MIN <= a OP b <= INT64_MAX`): never by `#unchecked` — always-safe
> множество (loop/литералы) элидируется всегда, contract-based (`requires`) — лишь
> при enforced контрактах; недоказанные операции остаются проверяемыми и в release.
> Soundness неизменна (паника на реальном overflow гарантирована).

`StackOverflow` и `OutOfMemory` **не входят** в `RuntimeError` — они
panic'и, не Fail. Не ловятся в коде. См. [D13](08-runtime.md#d13).

#### `Error` — record для пользовательских ошибок с сообщением

```nova
// в prelude (D26)
type Error {
    ro msg str
}

fn Error.new(msg str) -> Error => { msg }
```

Quick-and-dirty замена `throw "string"`:

```nova
fn validate(x int) Fail[Error] -> () {
    if x < 0 { throw Error.new("negative not allowed") }
}
```

Используется когда:
- Программист не хочет придумывать typed sum.
- Сообщение для лога/UI достаточно (не разбор по вариантам).

Альтернатива — типизированный sum для domain-логики:

```nova
type ValidationError enum NegativeNotAllowed | TooLarge(int)

fn validate(x int) Fail[ValidationError] -> () {
    if x < 0 { throw ValidationError.NegativeNotAllowed }
}
```

Для production-API typed sum предпочтительнее (compile-time
exhaustiveness в match).

#### Замена ранее существовавшего unit-маркера `Error`

В D26 (08-runtime.md) ранее был `type Error` как unit-тип-маркер для
`Fail` без параметра. D65 **заменяет** его на record `Error { msg str }`,
полезный для quick-and-dirty.

### Правило 5. Транзитивность с гибридом — уточнение D62

D62 фиксирует «Fail strict транзитивен». С гибридом это уточняется:

#### Совместимость по подтипу

| Caller declared | Callee может бросать |
|---|---|
| `Fail[E]` | только `Fail[E]` (тот же тип) или ничего |
| `Fail` (any) | `Fail[E]` любого `E`, `Fail` (any), `throw` любого значения |

То есть **`Fail` (any) поглощает любой `Fail[E]`** — это естественно,
`any` это top-type.

В обратную сторону — `Fail[E]` не покрывает `Fail` (any). Caller с
`Fail[E]` не может вызывать функцию с `Fail` (any) без явной обёртки.

#### Несовместимость

Если callee имеет `Fail[E']`, а caller декларировал `Fail[E]` (E ≠ E'):

- **Compile error**, не warning.
- Программист обязан выбрать:
  1. Объявить `Fail[E']` как **дополнительный** эффект (multi-Fail в row).
  2. Использовать `Fail` (any) — поглощает оба.
  3. Обернуть через `.map_err(...)?` для конверсии E' → E.
  4. Локально поймать через `with Fail[E'] = ... { ... }` и не пробрасывать.

Multi-Fail в row синтаксически валиден:

```nova
fn process(s str) Fail[ParseError] Fail[RuntimeError] -> int {
    parse(s)?            // throws ParseError
    safe_div(n, 2)?      // throws RuntimeError
}
```

Две раздельные Fail-записи. Caller обязан установить два handler'а
или один `Fail` (any).

#### Coercion через sum-variant — отложено

«Если `E` имеет однозначный конструктор для типа источника `E'`,
`?` автоматически coerce'ит» — отложено как Q-fail-coercion. Сейчас
требуется явный `.map_err(...)` или multi-Fail.

### Что меняется по сравнению с D25

D25 (`throw` и параметризация `Fail[E]`) **остаётся валиден** в основной
части. D65 **уточняет**:

1. `Fail` без параметра — теперь явно сахар над `Fail[any]`, не unit-маркер.
2. Lookup-правило (точный тип → catch-all → panic) явно зафиксирован.
3. Re-throw через `throw err` в handler'е явно описан.
4. Prelude-типы `RuntimeError` и `Error` — новые, заменяют unit-маркер.

Раздел «Эволюция» D25 апдейтится с указанием на D65.

### Почему

1. **Гибрид удобства и точности.** `Fail[E]` для production даёт
   compile-time exhaustiveness и точный caller-knows-what-to-catch.
   `Fail` (any) для quick-and-dirty не заставляет придумывать тип.
   Один способ был бы крайностью.

2. **Простой lookup без subtype-magic.** Точное совпадение типа —
   локально проверяемо. Match внутри handler'а покрывает sum-варианты.
   Не нужно расширять type system'у на subtype-aware lookup.

3. **Re-throw позволяет композицию handler'ов.** Локальная обработка
   подмножества + проброс остальных — стандартный pattern, работает
   через standard effect mechanics.

4. **`RuntimeError` sum** даёт типизированный set встроенных ошибок.
   Caller match'ит варианты, добавление новой ветки в `RuntimeError`
   ломает существующие caller'ы (через non-exhaustive match warning).
   Это **фича** — программист обновляется консистентно.

5. **`Error` record** — низкоуровневый escape hatch. Не sum-тип
   (нечего match'ить, кроме `msg`), но удобный для логов и UI.

### Что отвергнуто

- **`throws E` keyword** (Java-style) — не нужен, единая запись
  `Fail[E]` единообразна с другими эффектами. Прецедент вводить
  второе имя для одного концепта (`throws` ≡ `Fail`) нарушает D40-style
  «один способ для одного случая».
- **Subtype-aware lookup** (`Fail[RuntimeError]` ловит `Fail[DivByZero]`
  если DivByZero ⊆ RuntimeError) — отвергнуто. Match внутри handler'а
  достаточно. Subtype-aware расширил бы type system на sum-subtype-relations,
  цена/польза неудачное.
- **Auto-coercion `?` через однозначный sum-variant** — отложено как
  Q-fail-coercion. Сейчас явный `.map_err(...)`.
- **`Fail` без параметра как **отдельный** эффект**, не алиас на
  `Fail[any]` — отвергнуто. Лишняя сущность; алиас даёт ту же
  семантику.
- **Auto-inference `Fail[RuntimeError]`** для функций использующих
  встроенные операции — отвергнуто. Программист пишет руками
  (для public — D62 strict; для private — D28 inference, который
  выводит на основе тела). Если в теле есть `arr[i]` или `a/b`,
  D28-inference добавляет `Fail[RuntimeError]` в инферированную
  сигнатуру, но программист может явно написать `Fail` (any) или
  `Fail[CompositeError]` если делает map_err.
- **`throws SomeError | throw SomeError`** — путаница keyword'ов.
  В Nova `throw` это keyword (как `return`), `Fail[E]` это эффект-тип.
  Они на разных уровнях: `throw` — control-flow в теле, `Fail[E]` —
  декларация в сигнатуре.

### Связь

- [D2](#d2-эффекты-вместо-ключевых-слов-asyncthrowsunsafe), [D3](#d3-синтаксис-эффектов-типы-между--и--) — синтаксис effect-row.
- [D4](#d4--для-пробрасывания-ошибки) — `?` пробрасывание ошибки.
- [D86](#d86--coalesce-оператор-fallback-для-resultoption) — `??` coalesce / fallback.
- [D11](#d11-имена-эффектов-и-синтаксис-with) — `with` синтаксис.
- [D25](#d25-throw-и-параметризация-faile) — `throw` и `Fail[E]`.
  D65 уточняет `Fail` без параметра, lookup, re-throw.
- [D26](08-runtime.md#d26) — prelude. `Error` и `RuntimeError`
  добавлены/обновлены.
- [D31](#d31-handler-лямбда-для-эффектов-с-одной-операцией) — handler-лямбда
  для одно-операционных эффектов. Работает для `Fail` (одна операция
  `fail`).
- [D53](02-types.md#d53) — `any` как top-type через пустой protocol;
  основа для `Fail` ≡ `Fail[any]`.
- [D54](03-syntax.md#d54) — `is` для runtime-проверок типа в catch-all
  handler'е `Fail` (any).
- [D61](#d61-полная-семантика-эффектов-effect-keyword-handler-литерал-handlere-interrupt) —
  effect/handler keywords, interrupt. D65 не меняет.
- [D62](#d62-прагматичная-семантика-эффектов-прямые-в-сигнатуре-fail-strict-async-ambient-правило-effectprotocol) —
  Fail strict. D65 уточняет совместимость типов при транзитивности.

### Цена

1. **Sweep по spec и examples** — заменить `Fail` (там где quick-and-dirty)
   на корректные формы:
   - `transaction[T](body fn() Db Fail -> T) Fail -> T` — generic
     параметр `[T, E]`: `transaction[T, E](body fn() Db Fail[E] -> T) Db Fail[E] -> T`
   - Конкретные функции (`parse(s) Fail`) — `Fail[ParseError]` или
     оставить `Fail` (any) для скрипт-кода.
   - Эталоны в spec (`fn parse(s str) Fail -> int`) — переписать с
     явным `Fail[ParseError]` для clarity.
2. **Bootstrap-компилятор**:
   - Парсер уже принимает `Fail` без параметра (как имя эффекта).
   - Type checker нужно расширить на subtype-aware «Fail (any)
     поглощает Fail[E]».
   - Re-throw в handler'е работает через стандартную effect mechanics.
3. **Prelude в bootstrap'е**: добавить `RuntimeError` sum и `Error`
   record. Заменить старый unit-маркер `Error`.

### Эволюция

`Fail` без параметра существовал в D25 как сахар над `Fail[Error]`,
где `Error` был unit-маркером. Это работало, но `Error` без полей был
бесполезен. D65 переопределяет:

- `Error` теперь record `{ msg str }` — полезный.
- `Fail` без параметра теперь сахар над `Fail[any]` (universal).
- Lookup с приоритетом «точный тип → catch-all → panic».

Дискуссия привела через несколько итераций:
- Сначала рассматривался `Fail` strict-only (всегда явный тип).
  Отвергнуто — quick-and-dirty неудобно.
- Потом `Fail` = `Fail[RuntimeError]` (фиксированный тип).
  Отвергнуто — ограничивает универсальность.
- Финал: гибрид `Fail` (any) + `Fail[E]` typed.

`RuntimeError` как sum-тип был очевидным решением — встроенные
операции имеют конечный набор runtime-сбоев, sum покрывает.

`Error` как record (не sum) — для случаев когда программист не хочет
типизированный domain-sum, но хочет message. Это replacement старого
unit-маркера.

#### Откат «трёх форм» (2026-05-07)

В одной из итераций рассматривалась трёхформенная семантика
(`Fail` placeholder ≠ `Fail[any]` erasure), где `Fail` без параметра
означал бы «inference placeholder — компилятор выводит конкретный E».
Откатано к простой `Fail ≡ Fail[any]` по двум причинам:

1. **Различие наблюдаемо только при полном type-inference**, которого
   bootstrap не реализует. В runtime/codegen «голый Fail» эрейзится
   через lookup как catch-all (Правило 2), что эквивалентно `Fail[any]`.
   Production-компилятор может реализовать placeholder-семантику через
   точную D28-инференс E, но это отдельное расширение, не часть
   базового D65.

2. **Catch-all use-case требует erasure-семантики.** Программист пишет
   `with Fail = |e| Log.error(e) { ... }` чтобы поймать любой throw
   независимо от типа. Если бы `Fail` был placeholder (ждёт inference),
   у `with Fail = handler` не было бы контекста для inference — паттерн
   терял бы чёткость. С `Fail ≡ Fail[any]` семантика однозначна: handler
   принимает значение типа `any`, в теле — `is`-проверки или
   `str.from(e)` для message.

Реализация bootstrap'а (commit `284b2074`) уже соответствует откатанной
формулировке — добавляет голый `Fail` без E через D28-inference;
дальше lookup эрейзит его как catch-all.

---

## D63. `forbid X { body }` — capability sandbox

### Что

Keyword-блок, **запрещающий** использование операций перечисленных
эффектов внутри body. Реализуется на двух уровнях:

1. **Compile-time**: для каждой функции, вызываемой в body, type
   checker проверяет, что её **прямые** эффекты не пересекаются с
   forbid-set. Иначе compile error.
2. **Runtime**: при операции forbid-эффекта runtime ловит и
   fail'ится — даже если функция была пропущена compile-time
   проверкой (через D62 transit warning или handler-substitution).

`forbid` непреодолим: код в body не может выйти из sandbox через
`with X = ...`. Установка нового handler'а для forbid-эффекта внутри —
**compile error**.

R6 (revolutionary.md) ссылается на D63 как на формализацию capability
mode.

### Capability sandbox: три механизма, разные цели

В Nova есть три инструмента для ограничения «что код может делать»,
часто их путают. Разница важна:

| Механизм | Что ограничивает | Где задаётся | Что нарушение даёт |
|---|---|---|---|
| **`forbid X { body }`** ([D63](#d63)) | использование эффектов из set | вокруг блока кода | compile error + runtime fail |
| **`realtime { body }`** ([D64](#d64)) | suspension (приостановка fiber'а) | вокруг блока кода | runtime panic |
| **closure границы** ([D62](#d62) capture rules) | какие handler'ы захватываются | при создании handler'а | type error если handler'а нет |

Когда какой использовать:

- **`forbid`** — когда нужно гарантировать «эта подсистема НЕ
  обращается к Net/Db/Fs» (sandbox для plugins, contract-функций,
  pure_view).
- **`realtime`** — когда нужно гарантировать «здесь нельзя
  приостанавливаться» (real-time loops, ISR-like обработчики, hot
  paths). Async — runtime-факт, не эффект, поэтому `forbid Async`
  невозможен; `realtime` — отдельный inverse-маркер.
- **closure границы** — автоматически: при создании handler-литерала
  компилятор проверяет, что захваченные handler'ы валидны в момент
  использования. Не вмешательство программиста.

Они **не пересекаются** по семантике — каждый закрывает свою
категорию проверок:

```nova
fn pure_view(u User) -> str =>
    forbid Net, Db, Fs {           // нельзя side effects
        realtime nogc {            // нельзя suspension и аллокации
            format(u)
        }
    }
```

Композиция работает: `forbid` запрещает effect-вызовы, `realtime`
дополнительно запрещает suspend-точки и (при `nogc`) аллокации.
Программист выбирает один или оба в зависимости от того, что
гарантировать.

### Правило

```nova
forbid Net, Fs, Db { body }
```

Внутри body:
- Прямой вызов операции `Net.op(...)`, `Fs.op(...)`, `Db.op(...)` — **compile error**.
- Вызов функции с `Net`/`Fs`/`Db` в **прямой** сигнатуре — **compile error**.
- `with Net = h { ... }` (или `Fs`, `Db`) — **compile error**: «cannot
  install handler for forbid-effect».
- Транзитивный вызов через функцию которая **не** объявила forbid-эффект,
  но вызывает что-то с ним — compile-time **warning** (по D62), runtime
  **fail** на момент операции.

#### Runtime барьер

Реализуется через специальный sentinel-frame в handler-стеке:

```
handler-стек (lookup сверху вниз):
  ┌────────────────────────┐
  │ FORBID(Net, Fs, Db)    │  ← sentinel, push'нут при входе в forbid
  │ Db = postgres_handler  │  ← старый, ниже
  │ ...                    │
  └────────────────────────┘
```

При операции forbid'ed эффекта (`Db.query(...)`):
- Runtime ищет handler сверху вниз.
- Видит `FORBID(Db)` **первым** — fail с «effect Db is forbidden in
  current scope».

#### Установка нового handler'а внутри запрещена

Если бы установка `with Db = other { ... }` внутри `forbid Db { ... }`
была разрешена, новый handler оказался бы **выше** sentinel'а в стеке
и lookup нашёл бы его раньше — sandbox обходится. Запрещаем установку
compile-time:

```nova
forbid Db {
    with Db = mock_db {       // COMPILE ERROR
        ...
    }
}
```

Это делает sandbox **непроницаемым**.

### Пример: плагин в sandbox

```nova
fn run_plugin(plugin Plugin) -> str {
    forbid Net, Fs, Db {
        plugin.invoke()       // compile-time + runtime гарантия
                              // что plugin не ходит в Net/Fs/Db
    }
}
```

### Пример: детерминированное вычисление

```nova
fn compute_pure(input []u8) -> []u8 {
    forbid Time, Random, Io, Net, Fs, Db {
        process(input)        // гарантированно детерминировано
    }
}
```

### `Async` нельзя forbid'ить

`Async` это **не type-system эффект** ([D62](#d62-прагматичная-семантика-эффектов-прямые-в-сигнатуре-fail-strict-async-ambient-правило-effectprotocol)),
а ambient capability fiber-runtime'а.

```nova
forbid Async { ... }    // COMPILE ERROR: «Async is not a type-system
                        // effect, use `realtime { ... }` block instead»
```

Для запрета приостановки используется отдельный `realtime { ... }`
блок ([D64](#d64-realtime--block--гарантия-не-приостановки)) — это
runtime-конструкция, не часть type system'ы.

### Запретить можно только `effect`-типы

`forbid` принимает только effect-типы ([D62 правило effect/protocol](#d62-прагматичная-семантика-эффектов-прямые-в-сигнатуре-fail-strict-async-ambient-правило-effectprotocol)):

```nova
forbid Hash { ... }    // COMPILE ERROR: Hash это protocol, не effect
forbid Net { ... }         // OK: Net это effect
```

### Семантика `Fail`

`Fail[E]` — обычный effect, можно forbid:

```nova
forbid Fail[ParseError] { ... }     // запрет throw'а ParseError
forbid Fail { ... }                  // запрет любого throw (Fail any)
```

Если внутри есть `throw expr` который соответствует forbid'ed `Fail` —
compile error. Runtime fail если транзитивно через несовместимую
функцию.

### Грамматика

```
forbid-block = 'forbid' effect-list block
effect-list  = type-ref { ',' type-ref }
```

`type-ref` это полная ссылка на effect-тип, включая generic-параметры:
`Fail[ParseError]`, `Fail[E]` (из generic-контекста).

### Почему

1. **Capability sandbox без runtime-only решений.** Java SecurityManager
   — runtime, не compile-time. Compile-time даёт feedback при
   разработке.
2. **Симметрия с `with`**: `with X = h { ... }` устанавливает handler;
   `forbid X { ... }` запрещает. Pair-of-opposites.
3. **Прецедент Effekt language** — capability tracking через тип,
   forbid через ограничение row.
4. **Использования**: плагины, песочницы для AI-сгенерированного кода,
   детерминированные вычисления, тестирование «функция не делает X».

### Что отвергнуто

- **Только compile-time forbid** (без runtime барьера) — D62 ослабил
  R5.2 для прямых эффектов, поэтому compile-time не ловит
  транзитивные вызовы. Runtime барьер нужен для полной гарантии.
- **Только runtime forbid** (без compile-time) — теряется immediate
  feedback в IDE при написании кода.
- **Soft forbid** (warning вместо error) — sandbox должен быть
  **гарантирован**, не «вежливое предупреждение».
- **Forbid Async** — Async не существует в типах ([D62](#d62));
  `realtime { ... }` для запрета приостановки ([D64](#d64-realtime--block--гарантия-не-приостановки)).
- **Forbid non-effect-types** (protocol, sum, record) — не имеет
  смысла; forbid это про эффекты-как-capabilities.

### Связь

- [D11](#d11-имена-эффектов-и-синтаксис-with) — `with` синтаксис.
  forbid синтаксически близок.
- [D62](#d62-прагматичная-семантика-эффектов-прямые-в-сигнатуре-fail-strict-async-ambient-правило-effectprotocol) —
  effect/protocol правило, прямые эффекты.
- [D64](#d64-realtime--block--гарантия-не-приостановки) — `realtime`
  для async-запрета (отдельный механизм).
- [revolutionary.md → R6](../revolutionary.md#r6-capability-режим-для-безопасной-композиции) —
  capability mode описан, D63 формализует.

### Цена

- **Bootstrap-компилятор**:
  - Lexer: keyword `forbid`.
  - Parser: `forbid effect-list { body }` блок.
  - AST: `ExprKind::Forbid { effects, body }`.
  - Interp: sentinel-frame в handler-стеке; runtime-проверка операций.
  - Type checker (опционально для bootstrap): compile-time валидация
    прямых эффектов callee'ев.
- **Спека**: D63 + R6 ссылка на D63.

#### Реализация в bootstrap (2026-05-09, Plan 16 Ф.1-Ф.6)

Compile-time enforcement реализован в `compiler-codegen/src/types/mod.rs`
через `CapabilityCtx`. Walk модуля проходит fn-bodies + test-bodies со
state'ом `forbidden_stack: Vec<HashSet<String>>`. На входе/выходе из
`ExprKind::Forbid { effects, body }` push/pop. На каждом call-site
union forbidden-стека пересекается с callee.effects → R5.3 error.

Forbid-handler-ban (D63 §3473): `ExprKind::With { bindings, body }`
проверяет, что устанавливаемые handler'ы не пересекаются с
forbidden_union. Иначе error «cannot install handler for `X` inside
`forbid X` block».

Pure-fn (callee.effects пустой) — всегда OK.

Транзитивные эффекты (callee → callee → effect) пока **не trace'ятся**
(D62 говорит — warning). Закроется после полного effect-row inference
(отдельный план).

---

## D64. `realtime { body }` / `blocking { body }` — гарантия не-приостановки _(RETRACTED by Plan 113)_

> **⚠️ RETRACTED (Plan 113, 2026-05-29).** Block-forms `realtime { }` и `blocking { }`
> **удалены из языка**. Логика и семантика переехали в D172 attribute-only model:
> - `realtime { body }` → extract в `#realtime fn` (callee guarantee)
> - `blocking { body }` → extract в `#blocking fn` (fn-level threadpool offload)
>
> Атрибут `#realtime` на функции — **сохранён** (callee guarantee модель, D172).
> D64 retract'ирован как block-form spec.
>
> _История ниже сохранена для понимания эволюции._

### Что (историческое, retracted)

Runtime-блок, гарантирующий что код внутри **не приостанавливается**
на yield-point'ах fiber-runtime'а. Применяется для real-time-зон,
hot loops, lock-критичного кода.

`realtime` это **не эффект** (Async убран из type system по [D62](#d62-прагматичная-семантика-эффектов-прямые-в-сигнатуре-fail-strict-async-ambient-правило-effectprotocol)),
а runtime-конструкция. Семантика — fiber-runtime отказывается
выполнять suspend-операции внутри realtime-блока, fail'ится при
попытке.

### Правило

```nova
realtime { body }
```

Внутри body:
- **Synchronous вычисления** — OK (математика, локальные mut, локальные
  структуры на стеке).
- **Доступ к ambient handler'ам без suspend** — OK (например, `Log.info`
  если log-handler не блокирующий).
- **Suspend-операции** — runtime panic «cannot suspend in realtime block».
  Включает: `Net.get(...)`, `Fs.read(...)`, `Db.query(...)`,
  `Time.sleep(...)`, `Channel.recv(...)`, любая операция, которая в
  fiber-runtime приводит к yield'у.

#### Compile-time

Type checker (production-компилятор) может **частично** ловить нарушения:
- Вызовы функций с эффектами `Net`, `Fs`, `Db`, `Time` (известно что
  они suspend) — compile error.
- Это **не полная гарантия** — пользовательский effect может suspend
  через свой handler. Runtime барьер всё равно нужен.

#### Runtime

Fiber-runtime устанавливает флаг при входе в realtime-блок. Каждая
suspend-точка проверяет флаг — если активен, runtime panic.

### Пример: hot loop без suspend

```nova
fn checksum(data []u8) -> int {
    realtime {
        mut sum = 0
        for b in data { sum += b as int }
        sum
    }
}
```

### Пример: lock-критичная секция

```nova
fn update_counter(counter mut Counter) {
    counter.lock()
    realtime {
        counter.value += 1     // не должно yield'нуть с захваченным lock'ом
    }
    counter.unlock()
}
```

### Атрибут `#realtime` на функции

Sugar для функции целиком (атрибут-префикс `#` — см.
[D96](09-tooling.md#d96-синтаксис-атрибутов-name-без-квадратных-скобок)):

```nova
#realtime
fn checksum(data []u8) -> int {
    mut sum = 0
    for b in data { sum += b as int }
    sum
}

// эквивалентно:
fn checksum(data []u8) -> int {
    realtime {
        mut sum = 0
        for b in data { sum += b as int }
        sum
    }
}
```

### Что внутри запрещено

| Операция | Запрет | Почему |
|---|---|---|
| `Net.get(...)` | да | network roundtrip → suspend |
| `Fs.read(...)` | да | disk I/O → suspend |
| `Db.query(...)` | да | network → suspend |
| `Time.sleep(d)` | да | явный sleep → suspend |
| `Time.now()` | нет | обычно sync (timer read) |
| `Random.next()` | нет | sync RNG |
| `Log.info(...)` | зависит | если handler не blocking — OK |
| `Channel.recv()` | да | блокирующий wait → suspend |
| `Channel.send_nonblocking()` | нет | non-blocking — OK |
| `spawn ...` | да | создаёт fiber, нарушает «нет suspend» |
| Аллокация в managed heap | зависит | если GC может paus'ить — да |

Точный список — задача production-компилятора и runtime'а; D64
фиксирует **принцип**: «всё что может yield — запрещено».

### Опционально — запрет аллокации

Для **жёсткого** real-time-mode'а можно запретить аллокацию в managed
heap (GC pause-free):

```nova
realtime nogc { body }
```

Внутри `realtime nogc` — никаких аллокаций, кроме как в region'е
([05-memory.md → D6](05-memory.md#d6)).

Это **расширение** `realtime`, опциональное. Базовый `realtime` запрещает
suspend, не аллокацию.

### Грамматика

```
realtime-block = 'realtime' [ 'nogc' ] block
```

`nogc` — опциональный модификатор для жёсткого режима.

### Async концепт **полностью удалён** из языка

Это окончательно фиксирует:
- `Async` не существует как тип эффекта.
- Не пишется в сигнатурах.
- Не упоминается в effect-row.
- Программист про него **не знает**.

Если нужна гарантия не-приостановки — `realtime { body }`. Это
**inverse-маркер**: дефолт «может suspend», `realtime` — «гарантированно
нет».

### Почему

1. **Inverse-семантика лучше для AI-first.** В большинстве кода
   suspend разрешён (это дефолт). Программист пишет специальный
   маркер только когда **отличается** от дефолта. Меньше cognitive
   load.
2. **Реальные use-cases**: real-time системы, hot loops в backend,
   lock-критичный код. Не везде, но достаточно часто.
3. **Прецедент**: Erlang has `:hibernate` for non-yielding paths,
   Rust has `#[no_std]` for no-allocation, Java has `@RealTime`
   annotations. Nova consolidates через один keyword.
4. **Симметрия с `forbid`**: оба — runtime-ограничения. forbid для
   эффектов в типах, realtime для невидимой приостановки.

### Что отвергнуто

- **`Async` как явный эффект в сигнатурах** ([D62](#d62-прагматичная-семантика-эффектов-прямые-в-сигнатуре-fail-strict-async-ambient-правило-effectprotocol)) —
  везде в backend-коде шум.
- **`@no_suspend` атрибут только** — `realtime` block более гибкий
  (зона внутри функции), атрибут это sugar.
- **`sync` keyword** — `sync` имеет другие коннотации (синхронизация,
  thread-sync) в других языках.
- **`pinned` keyword** — слишком узкое значение (real-time
  terminology), не покрывает hot loops.
- **`forbid Async`** — Async не в типах, нечего forbid'ить через
  type-system механизм.

### Связь

- [D62](#d62-прагматичная-семантика-эффектов-прямые-в-сигнатуре-fail-strict-async-ambient-правило-effectprotocol) —
  Async ambient, не пишется в сигнатурах. D64 — inverse-механизм.
- [D63](#d63-forbid-x--body---capability-sandbox) — capability
  sandbox для type-system эффектов; параллельный механизм для
  type-effects, тогда как D64 для async-runtime.
- [05-memory.md → D6](05-memory.md#d6) — `region { ... }` для
  GC-free аллокации; `realtime nogc` использует region семантику.
- [06-concurrency.md → D14](06-concurrency.md#d14) — fiber runtime,
  yield-points; D64 запрещает yield внутри блока.
- [revolutionary.md → R7](../revolutionary.md#r7-async--невидимая-инфраструктура) —
  «Async — невидимая инфраструктура»; D64 формализует противоположное
  направление.

### Цена

- **Bootstrap-компилятор** (опционально):
  - Lexer: keyword `realtime`.
  - Parser: `realtime [nogc] { body }`.
  - AST: `ExprKind::Realtime { nogc bool, body }`.
  - Interp: runtime флаг + проверка на suspend-операциях.
  - Можно отложить — bootstrap не имеет полноценного fiber-runtime'а
    (синхронное исполнение); `realtime` no-op в bootstrap.
- **Production-компилятор**: type-level проверки + runtime барьер +
  оптимизация (LLVM может удалить safepoint'ы внутри realtime).

### Эволюция

Изначально `Async` был эффектом в типах, как у Koka. Опыт показал
что в реальном backend-коде он везде, что обесценивает его как
информативный маркер. [D62](#d62-прагматичная-семантика-эффектов-прямые-в-сигнатуре-fail-strict-async-ambient-правило-effectprotocol)
сделал `Async` ambient capability — не пишется в типах, но «существует»
концептуально.

Дискуссия про `forbid Async` показала: если Async не в типах, его
нельзя forbid'ить через type-system механизм. Нужен **отдельный**
runtime-маркер. Так появился `realtime { body }`.

Окончательно: **`Async` концепт удалён из языка целиком**. Программист
не знает про него; есть только `realtime` как inverse-маркер. Это
приближает Nova к Go/Erlang модели «горутины могут suspend, нет
async-keyword'а».

---

## D67. `?` оператор: семантика для `Result` через Fail, для `Option` через ранний return

> ⚠️ **ОТМЕНЕНО 2026-05-10, см. [D85](#d85-операторы--и--унифицированное-поведение-для-result-и-option-throw-стиль-через-).**
> D85 унифицирует семантику `?`: для **обоих** `Result` и `Option`
> делает ранний return обёртки. Throw-стиль через `Fail` теперь
> выражается отдельным оператором `!!` или явным `?? throw E`.
> `?` больше не задействует эффект `Fail`.
>
> Текст ниже сохранён для исторической справки. Актуальная семантика —
> [D85](#d85-операторы--и--унифицированное-поведение-для-result-и-option-throw-стиль-через-).

### Что
Постфиксный оператор `?` имеет **две разные семантики** в зависимости
от типа выражения:

1. **`Result[T, E]`** — `?` desugar'ится в `match` + `throw` через
   эффект `Fail[E]` ([D4](#d4)).
2. **`Option[T]`** — `?` desugar'ится в `match` + ранний `return None`
   из текущей функции, **без эффекта Fail**.

На любом другом типе `?` — синтаксическая ошибка.

### Правило

#### `?` на `Result[T, E]`

Требует `Fail[E]` в сигнатуре функции. Точная семантика — D4:

```nova
expr?  ≡  match expr {
              Ok(v)  => v
              Err(e) => throw e          // через эффект Fail[E]
          }
```

#### `?` на `Option[T]`

**Не** требует эффекта в сигнатуре. Превращается в ранний return:

```nova
expr?  ≡  match expr {
              Some(v) => v
              None    => return None     // ранний выход из текущей fn
          }
```

Возвращаемый тип функции должен быть `Option[U]` — иначе compile error
(`return None` несовместим с return type'ом).

```nova
fn first_pos(xs []int) -> Option[int] {
    ro head = xs.first()?         // Option[int]; на None: return None
    if head > 0 { Some(head) } else { None }
}
```

#### `?` НЕ работает на `Fail[E] -> T`

Если выражение **бросает через эффект Fail** (а не возвращает Result-
значение), `?` после него — синтаксическая ошибка:

```nova
fn save(u User) Fail[DbError] -> () => Db.exec(...)

fn caller(u User) Fail[DbError] -> () =>
    save(u)?           // ОШИБКА: save возвращает (), не Result/Option
```

`?` ожидает значение типа `Result` или `Option`, а не throw'а — throw
от `save` сам собой пробрасывается через `Fail[DbError]` в caller'е,
без `?`.

### Семантика на handler-методах

Внутри handler-method'а `?` подчиняется тем же правилам:

```nova
type Db effect {
    in_transaction[T](body fn() Db Fail -> T) Fail -> T
}

effect Db {
    in_transaction(b) => real.in_transaction(b)?    // ОШИБКА: in_transaction
                                                    // возвращает T, не Result/Option
    in_transaction(b) => real.in_transaction(b)     // правильно: throw сам
                                                    // пробрасывается через Fail
}
```

Это частая ошибка при написании middleware-handler'ов: программист
думает «обернуть и вернуть» через `?`, но `?` нужен только когда
callee возвращает Result/Option как значение.

### Почему две семантики

- **Result** — про **обработку ошибок**: нужен механизм propagation
  через стек, единый с `throw`. Эффект `Fail[E]` даёт это.
- **Option** — про **отсутствие значения**: семантически отдельная
  категория, не «ошибка». Использовать `Fail` для каждого `None` —
  шум: `lookup`, `find`, `parse_int` бросали бы Fail везде.
  Ранний return `None` из функции с `-> Option[T]` — естественнее.

#### Признанное напряжение с D10 «всё — handler»

`?` на `Option` — **второй механизм control-flow** в Nova: ранний
return из функции, не перехватываемый через `with`-handler. Это
**признанное исключение** из «всё взаимодействие с внешним миром —
эффект»:

- `Option` это **значение**, не эффект. `None` это валидный результат,
  не ошибка. Поэтому propagation через эффект-stack здесь
  неприменимо — нет «вверх» куда передавать.
- Альтернатива `Fail[NoneError]` создавала бы фантомные эффекты в
  сигнатурах для каждой функции с `lookup`/`find`/`parse_int`, что
  **значительно** хуже по AI-first критерию (R5.2).
- Compile-time правило тривиально: `?` на `Option[T]` валиден только
  в функции с return type `Option[U]` для какого-то U.

Это **прагматичный компромисс** — в духе D62 (Fail strict, остальное
ослаблено). Полная унификация через эффект-stack теряет больше чем
выигрывает.

Альтернативы рассмотрены:
- **`?` на Option через `Fail[NoneError]`** — отвергнуто: засоряет
  сигнатуры неинформативным типом ошибки.
- **`?` только на Result** — отвергнуто: Option-чтение через `match`
  бойлерплейт; `?` для unwrap-or-return — естественный pattern.
- **Отдельный оператор `??!` для Option** — отвергнуто: `??` уже
  используется как coalesce ([D86](#d86--coalesce-оператор-fallback-для-resultoption)),
  цена ещё одного символа выше пользы.

### Что отвергнуто

- **`?` на произвольном sum-type'е с двумя вариантами** — слишком
  магично; программист может назвать варианты как угодно, парсер
  не знает «какой Ok какой Err».
- **`?` на `Result[T, E]` без `Fail[E]` в сигнатуре** — нарушает
  D4 правило (`?` через `throw`).
- **Auto-coercion `Result` ↔ `Option`** через `?` — отвергнуто
  ([Q-fail-coercion](../open-questions.md#q-fail-coercion)). Программист
  явно конвертирует через `.ok()` / `.into()`.

### Связь

- [D4](#d4) — `?` для Result + Fail[E].
- [D25](#d25) — `throw` как операция Fail[E].
- [D26](08-runtime.md#d26) — Option/Result в prelude.
- [D62](#d62) — Fail strict, транзитивность.
- [D65](#d65) — `Fail[any]` для catch-all.

### Эволюция

В D4 (изначально) `?` был определён только для Result. Семантика для
Option работала де-факто в bootstrap-интерпретаторе через ранний
return, но не была зафиксирована — это был open question
([D26 открытые вопросы](08-runtime.md#d26)).

D67 формализует обе семантики и явно отделяет от случая «callee
бросает через Fail» (где `?` не нужен и является ошибкой).

### Что НЕ меняется

D4 продолжает определять `?` для Result. D67 — расширение, не пересмотр.

---

## D68. Stateful handlers: через closure capture или `@as_handler` метод record

### Что
Handler-литерал ([D61](#d61)) содержит **только методы операций** —
поля внутрь добавлять нельзя. Stateful handlers (handler'ы со
своим состоянием) делаются одним из двух способов:

1. **Closure capture** — state живёт в локальной переменной (или
   параметре функции-фабрики), handler-method'ы захватывают её
   через closure. Каноничный «лёгкий» способ.
2. **`@as_handler` метод record'а** — state живёт в полях обычного
   record'а, метод record'а возвращает `Effect[E]`, который через
   `@field` обращается к полям. Канонично когда state нужно
   **проинспектировать снаружи** после `with`-блока.

### Правило

#### Способ 1: closure capture (легковесный)

State — локальная переменная или параметр свободной функции:

```nova
// State прямо в `with` — captured by closure
mut counter = 0
with Counter = effect Counter {
    next() {
        counter += 1
        return counter
    }
} {
    do_work()
}
// counter здесь = число вызовов Counter.next() в do_work
```

Или через handler-фабрику с параметром:

```nova
fn make_counter(initial int) -> Effect[Counter] {
    mut state = initial
    effect Counter {
        next() {
            state += 1
            return state
        }
    }
}

with Counter = make_counter(100) {
    do_work()
}
// state здесь недоступен — он внутри closure
```

Когда применять: state используется только во время handler-life'а,
после `with` его инспектировать не нужно (или достаточно `with`-сnopa).

#### Способ 2: `@as_handler` метод record'а

State — поля обычного record'а с `mut`. Метод записи возвращает
`Effect[E]`:

```nova
type CounterState { mut value int }

fn CounterState @as_handler() -> Effect[Counter] => effect Counter {
    next() {
        @value += 1                // обращение к полю receiver'а
        return @value
    }
}

ro s = CounterState { value: 0 }
with Counter = s.as_handler() {
    do_work()
}
println(s.value)                    // публичное состояние, инспектируется снаружи
```

Когда применять:
- State нужно **проверить после** `with`-блока (типичный testing-сценарий:
  `assert s.value == expected`).
- Один state используется **несколькими** handler-инстансами (один
  handler на запись, другой на чтение).
- State имеет **смысл сам по себе** как доменный объект (не deal-с-handler-detail).

### Семантика `@field` внутри handler-литерала

Handler-литерал `effect E { ... }` внутри `@`-метода типа `T` —
это **обычное выражение**, и `@field` указывает на receiver метода
(инстанс типа `T`). То есть:

```nova
fn CounterState @as_handler() -> Effect[Counter] =>
    effect Counter {
        next() {
            @value += 1     // @value — это поле receiver'а CounterState,
                            // не «self» handler'а (handler не имеет полей)
            return @value
        }
    }
```

Внутри handler-method'а нет своего `@self` — handler не имеет полей.
`@` в теле handler-method'а ссылается **на receiver внешнего метода**,
если handler-литерал создан внутри метода.

### Почему два способа

- **Closure capture** — простой, локальный, без объявления отдельного
  типа. Хорош для одноразовых handler'ов и тестов с in-flight state.
- **`@as_handler`** — даёт state имя и публичный API. Хорош когда
  state — это часть домена (счётчик ID, кэш-стат, in-memory БД).

Это **не два инструмента для одного** — выбор детерминирован сценарием
(нужен ли state наружу). D40 «один способ» не нарушается.

### Что отвергнуто

- **Handler-литерал с полями** (`effect Counter { state int = 0; next() {...} }`).
  Отвергнуто: путает handler с record'ом, парсер не однозначен,
  смысл «инстанс с полями + методами» не нужен — это обычный record
  с методом-фабрикой.
- **Скрытые «handler trait» objects** (как Java: класс реализует
  interface). Отвергнуто: handler — обычное значение, fabriqué из
  closure'а или метода. Никаких неявных классов.
- **`self` keyword внутри handler-method'а**. Отвергнуто: `@` уже
  определён как «field/method receiver'а» в [D35](03-syntax.md#d35),
  использование внутри handler-литерала естественно ссылается на
  внешний receiver.

### Связь

- [D11](#d11) — handler-литерал, основной синтаксис.
- [D31](#d31) — handler-лямбда для одно-операционных эффектов.
- [D35](03-syntax.md#d35) — `@`-методы и `@field`.
- [D61](#d61) — `handler` keyword.
- [D66](02-types.md#d66) — `Self` universal (можно использовать
  в return type'е `@as_handler`).

### Thread safety

D68 stateful handlers работают на **одном fiber'е** по умолчанию.
Если handler передаётся между fiber'ами через `spawn`/`detach`/
`parallel for` — **программист обязан** использовать thread-safe
state:

```nova
// ❌ Race: shared между fiber'ами без атомика
mut counter = 0
parallel for url in urls {
    with Counter = effect Counter {
        next() {
            counter += 1     // race condition
            return counter
        }
    } { ... }
}

// ✅ Atomic для shared counter:
ro counter = Atomic[int].new(0)
parallel for url in urls {
    with Counter = effect Counter {
        next() => counter.fetch_add(1) + 1
    } { ... }
}

// ✅ Или per-fiber state:
parallel for url in urls {
    mut local = 0
    with Counter = effect Counter {
        next() {
            local += 1
            return local
        }
    } { ... }
}
```

**Правило:** state, захваченный handler-method'ом, должен быть либо
fiber-local (новый let на каждый fiber), либо thread-safe
(`Atomic[T]`, `Mutex[T]`). Compile-time enforcement — открытый
вопрос ([Q12](../open-questions.md#q12) concurrency model). Bootstrap
не проверяет.

### Эволюция

D68 формализует два устоявшихся паттерна. Closure-capture
использовался во всех `nova_tests/` и в большинстве `examples/*.nv`
(`make_counter`, `in_memory_db_handler` и т.д.). Паттерн через
`@as_handler` явно ещё не использовался — D68 рекомендует его как
канонический способ для stateful handler'ов с публичным state.

---

## D85. Операторы `?` и `!!` — унифицированное поведение для `Result` и `Option`, throw-стиль через `!!`

> **AMEND (2026-07-07, решение владельца): метод-близнец `@unwrap()` РЕТРАКТИРОВАН.**
> `Option[T] @unwrap()` / `Result[T,E] @unwrap()` (prelude/core.nv) дублировали `x!!`
> (D9 — один канонический путь; факт дрейфа: 33 вызова метода при канон-операторе).
> Миграция `[M-unwrap-twins-retraction]` (волна-2 §4а): `.unwrap()` → `!!`, методы
> снесены из прелюдии.

> **Закрывает [D67](#d67--оператор-семантика-для-result-через-fail-для-option-через-ранний-return)** (отменён 2026-05-10).
> Унифицирует семантику `?`: для **обоих** `Result` и `Option` —
> ранний return обёртки. Throw-стиль через `Fail` теперь выражается
> новым оператором `!!` или явным `?? throw E`.
>
> **⚠️ Уточнение 2026-06-20, spec-closure 2026-07-06 ([Plan 174.2](../../docs/plans/174.2-question-mark-return-only.md)):**
> `?` подтверждён **return-only** — только на `Result`/`Option`, проброс значением; в
> Fail-эффект-функциях `?` запрещён (там `!!`/`throw`). Прежний throw-режим `?` (семантика
> D67-эры, где `?` на `Result` задействовал `Fail`) убран. Авто-`From` конверсия ошибки
> (Rust-стиль) **рассмотрена и ОТКЛОНЕНА** — держим explicit `.map_err`; полное обоснование —
> в блоке [«Авто-`From` конверсия ошибки»](#что-отвергнуто) ниже (не только NB-врезка).
> Устаревшие секции `## D4` и дубль `#### `?` — сахар над match + throw`` выше несут
> retraction-баннеры на D85 (проставлены при enforcement 173 Ф.1 #3) — противоречие снято.
>
> **✅ ENFORCED (Plan 173 Ф.1 #3, 2026-07-04):** чекер отвергает свободно стоящий `?` в
> функции, чей return-тип **не** `Result[T,E]` / `Option[T]`, диагностикой **`[E_TRY_IN_FAIL_FN]`**
> (`types/mod.rs` — per-fn walker в `check_fn`; подсказка «используй `!!` / `throw`»). Проброс
> ошибки значением осмыслен лишь там, где есть `return Err/None`.
> **Два EXEMPT-контекста** (де-риск 173 показал реальное использование):
> 1. **consume-init `?` (D196 form 2):** `consume X = expr? { body }` — `?` здесь unwrap-маркер
>    init-выражения (разворачивает `Result[T,E] → T` для biнga), НЕ свободный проброс; codegen
>    эмитит throw через enclosing `Fail` (`emit_c.rs` `in_fail_ctx`-ветка сохранена). Форма
>    задокументирована в D196 и остаётся каноном.
> 2. **`?` внутри `defer`-body:** governed by **D158** (`?` разрешён, если enclosing fn-sig несёт
>    `Fail[E]` или обёрнут `with Fail`), НЕ данным правилом.

> **✅ ENFORCED (Plan 221.1 №113, 2026-07-25):** чекер отвергает `expr!!` в EXPORTED
> (`export fn`) функции, чья сигнатура **не** несёт `Fail[E']`-совместимый эффект (и
> throw не пойман локальным `with Fail = ... { ... }`), диагностикой
> **`[E_BANG_REQUIRES_FAIL]`** (`types/mod.rs` — `check_fn` / `check_bang_requires_fail_block`,
> та же per-fn механика, что у `E_TRY_IN_FAIL_FN`). До этого энфорса `expr!!` без
> `Fail` в сигнатуре молча проходил и check, и `--strict-effects` — throw без
> декларации в effect-row (звучность-дыра, найдена на примере `fn build_router()
> -> Router` с `top.get(...)!!`).
> **Scope:** энфорс — ТОЛЬКО для exported fn (D62: «явная декларация обязательна» для
> публичного API). Приватные (`!is_export`) fn остаются под **D28 auto-inference**
> (`infer_effects`, `main.rs`) — `Fail` молча подставляется в effect-row private fn,
> использующей throw/`!!` в теле, ДО финального `check_module`-прохода; безусловный
> энфорс сломал бы устоявшийся приватный идиом (bare `!!` без явного `Fail` — норма
> для private helper'ов). Дыра №113 была именно в отсутствии проверки EXPORTED fn.
> **Fixup-канон при миграции** (не самодеятельность на месте, выбор по категории):
> (1) fn логически fallible, вызывающие готовы принять эффект — добавить `Fail[E]`
> в сигнатуру, оставить `!!`; (2) `!!` защищает программный инвариант (caller-authored
> литерал, не runtime-данные — типовой пример: `@header(name, value)` сеттеры,
> `HeaderName::validate` не может провалиться на статической литеральной строке) —
> `?? panic("...")` (D86), Fail в сигнатуру НЕ тащим; (3) `!!` внутри `with Fail = ...`
> — throw пойман локально, сигнатура fn не меняется. Итог миграции (Plan 221.1 №113):
> в `std/src` — 0 нарушений (все exported `!!`-сайты уже легальны: либо явный `Fail`,
> либо приватная fn под D28); во флагман-примере (`examples/flagship/aggregator`) — 2
> сайта, оба категория (2) (`json_encode(dto) ?? panic(...)` — плоский `Serialize`
> DTO, encode-failure структурно невозможен).

### Что
В Nova **два постфиксных оператора** для работы с `Option[T]` и
`Result[T, E]`, выбираемых программистом по стилю обработки:

1. **`expr?`** — **return-стиль**: «не получилось — обёртка наверх как
   значение». Локальное продолжение цепочки, без эффектов.
2. **`expr!!`** — **throw-стиль**: «не получилось — throw через эффект
   `Fail`». Эффект попадает в сигнатуру, ловится handler'ом.

Программист на месте использования выбирает, какой стиль обработки
ему нужен. Один и тот же тип (`Option[T]` или `Result[T, E]`)
поддерживает оба оператора.

`?` **больше не задействует `Fail`** — это унификация дизайна.

### Правило

#### `expr?` для `Result[T, E]` — return-стиль

```nova
expr?  ≡  match expr {
              Ok(v)  => v
              Err(e) => return Err(e)
          }
```

Внешняя функция должна возвращать `Result[U, E']` где `E'` совместим
с `E` (тот же тип или supertype через sum-расширение). Иначе compile
error.

```nova
fn pipeline(s str) -> Result[int, ParseError] {
    ro n = parse(s)?            // на Err: return Err(e)
    ro v = validate(n)?
    Ok(v)
}
```

#### `expr?` для `Option[T]` — return-стиль

```nova
expr?  ≡  match expr {
              Some(v) => v
              None    => return None
          }
```

Внешняя функция должна возвращать `Option[U]`. Иначе compile error.

```nova
fn first_pos(xs []int) -> Option[int] {
    ro head = xs.first()?       // на None: return None
    if head > 0 { Some(head) } else { None }
}
```

#### `expr!!` для `Result[T, E]` — throw-стиль

```nova
expr!!  ≡  match expr {
               Ok(v)  => v
               Err(e) => throw e
           }
```

Внешняя функция должна иметь `Fail[E']` в сигнатуре, где `E'`
совместим с `E`. Иначе compile error.

```nova
fn pipeline(s str) Fail[ParseError] -> int {
    ro n = parse(s)!!           // на Err: throw e
    ro v = validate(n)!!
    v
}
```

#### `expr!!` для `Option[T]` — throw-стиль

```nova
expr!!  ≡  match expr {
               Some(v) => v
               None    => throw RuntimeNoneError
           }
```

Внешняя функция должна иметь `Fail[RuntimeNoneError]` в сигнатуре.
Иначе compile error.

```nova
fn extract(json Json) Fail[RuntimeNoneError] -> str {
    ro user  = json.get("user")!!     // None → throw RuntimeNoneError
    ro email = user.get("email")!!
    email.as_str()!!
}
```

`RuntimeNoneError` — unit-тип в prelude ([D26](08-runtime.md#d26)),
введён специально для `expr!!` на `Option`. Это **отдельный тип**,
не вариант `RuntimeError` — разные категории (отсутствие значения vs
аппаратные сбои).

#### `expr??` — coalesce / кастомный fallback

Параллельно с `?` и `!!` работает **`??`** ([D86](#d86)) — coalesce
для default или явного custom-throw'а:

```nova
ro port = config.get("port") ?? 8080                       // default
ro port = config.get("port") ?? throw ConfigError.MissingPort   // custom throw
ro port = config.get("port") ?? panic("config must have port")  // panic (D13)
ro port = config.get("port") ?? exit(1, "no port in config")    // exit (D13)
```

`??` — для случаев, когда программисту нужен **конкретный fallback**:
конкретное значение, конкретный тип ошибки, panic, exit. `!!`
оптимизировано под **дефолтный шаблон** throw'а; `?? throw E` —
расширенная форма для кастомизации типа. Полная семантика `??` — в
[D86](#d86).

#### Смешение `?`, `!!`, `??` в одном выражении

Все три оператора валидны параллельно и могут сочетаться по типу
вмещающей функции:

```nova
// Функция возвращает Option — используем ?
fn first_word_pos(s str) -> Option[int] =>
    s.find(' ')?

// Функция бросает Fail — используем !!
fn first_word(s str) Fail[RuntimeNoneError] -> str =>
    s.split(' ')!!.first()!!.into()

// Mix: разные операнды, разные стили
fn process(s str) Fail[ParseError] -> int {
    ro raw = config.get("raw") ?? "default"
    ro n = parse(raw)!!
    n
}
```

#### Парсер

`!!` — **постфиксный** оператор, имеет высший приоритет (тот же
уровень, что `?`). Грамматически: `expr!!` всегда парсится как
постфикс, независимо от пробелов вокруг.

Префиксное `!!` (двойной boolean not) формально валидно (`!!cond` =
`cond`), но семантически бессмысленно — линтер может предупреждать.
Конфликт с постфиксом разрешается **позицией**: префикс не следует
за выражением, постфикс следует.

Edge-case `b!!c` — парсится как `(b!!) c`, что синтаксически
бессмысленно (два выражения подряд) → compile error. Программист
пишет с пробелом, оператором или скобками: `b!! - c`, `(b!!) c_call()`.

Одиночный `!` остаётся только префиксным (boolean not, [D46](03-syntax.md#d46)
`@not`). Постфиксный `!` **не используется** — оставлен под будущие
расширения.

#### `?` НЕ работает на `Fail[E] -> T`

Если выражение бросает через эффект `Fail` (а не возвращает Result-
значение), `?` после него — синтаксическая ошибка:

```nova
fn save(u User) Fail[DbError] -> () => Db.exec(...)

fn caller(u User) Fail[DbError] -> () =>
    save(u)?           // ОШИБКА: save возвращает (), не Result/Option
```

`?` ожидает значение типа `Result` или `Option`. Throw от `save` сам
пробрасывается через `Fail[DbError]` в caller'е, без `?`.

То же для `!!`:

```nova
fn caller(u User) Fail[DbError] -> () =>
    save(u)!!          // ОШИБКА: save возвращает (), не Result/Option
```

#### Семантика на handler-методах

Внутри handler-method'а `?` и `!!` подчиняются тем же правилам, что
снаружи. Тип возврата handler-method'а определяет, какой оператор
валиден.

```nova
type Db effect {
    in_transaction[T](body fn() Db Fail -> T) Fail -> T
}

effect Db {
    in_transaction(b) => real.in_transaction(b)        // правильно: throw сам пробрасывается
    in_transaction(b) => real.in_transaction(b)?       // ОШИБКА: in_transaction возвращает T
    in_transaction(b) => real.in_transaction(b)!!      // ОШИБКА: то же
}
```

### Почему

#### Зачем унификация `?`

В D67 `?` имел **две разные семантики**:
- `?` на `Result` → throw через `Fail` (engaged эффект, требовал `Fail[E]` в сигнатуре).
- `?` на `Option` → ранний return None (без эффекта).

Это создавало **категориальную неоднородность**: один оператор
выражал две разные операции. D67 признавал это «исключением из
принципа эффектов» в секции «Признанное напряжение». На деле никакого
«нарушения принципа» не было — `Option`-форма это `match + return`,
обычные конструкции, не имеющие отношения к handler'ам. Просто
дизайн D67 пытался впихнуть две разные операции в один символ ради
краткости.

D85 разводит две операции на **два символа** — `?` для return-стиля,
`!!` для throw-стиля. Каждый оператор делает **одно** и делает это
консистентно для обоих типов (`Option` и `Result`).

#### Зачем throw-стиль вообще

Throw-стиль через `Fail` — центральный механизм обработки ошибок в
Nova ([R1 в revolutionary.md](../revolutionary.md)). Handler
перехватывает throw в `with`-блоке, реализует transaction, retry,
log, тестовый mock. Без короткого синтаксиса для throw'а программисты
вынуждены писать длинные `match`'ы или `?? throw e_from_result`,
что замусоривает hot-path.

`!!` — короткий шаблон **дефолтного** throw'а (E как есть для
Result, RuntimeNoneError для Option). `?? throw E` остаётся для
**кастомизации** типа ошибки.

#### Почему `!!`, не `!`

**Несущая причина — грамматика.** Одиночный `!` занят под boolean not
(`!cond` → `cond.@not()`). Чтобы использовать его как постфикс,
потребовалось бы правило про обязательный пробел перед префиксным `!`,
без пробела — постфикс. Это работает, но хрупко: `!cond` vs `! cond` vs
`cond !` становятся принципиально разными. `!!` решает это естественно:
префикс/постфикс однозначно различаются позицией в грамматике, без
пробельных правил — `expr!!` = `(expr)!!`.

**Глиф-логика (самосогласованность набора `?`/`!!`/`??`).** Несущая
ось — *что происходит с ошибкой*: остаётся ли она **значением** или
становится **управляющим эффектом**. `?` (return `Err`/`None`) и
`?? fb` (поглотить fallback'ом) оба держат ошибку **значением** и не
трогают эффект-сигнатуру → делят глиф `?` («ошибка остаётся данными»).
`!!` единственный пересекает границу value→effect (добавляет `Fail[E]`
в row, throw) → берёт **другой** глиф `!` («ошибка стала эффектом»).
Смена глифа ровно на границе value↔effect — несущая семантика, не
эстетика. Пространство исходов исчерпано: вернуть значением (`?`),
бросить в эффект (`!!`), подставить fallback (`??`); четвёртый исход
«краш» намеренно БЕЗ оператора (D85, только видимый `?? panic(...)`).

**Про удвоение `!!`/`??`:** оно значит ровно одно — «это **не** тот
односимвольный оператор, что ты подумал» (дизамбигуация от занятого
моноглифа: `??` ≠ унарный `?`, `!!` ≠ префиксный `!`). Удвоение **НЕ**
кодирует «эскалацию/настойчивость» (иначе конфликтовало бы с `??`,
который удвоен, но не эскалация).

**Мнемонические бонусы (НЕ несущие обоснования):** `!!` визуально
тяжелее `?` (сигналит «здесь throw»); форма знакома по Kotlin. ⚠ Но
Nova `!!` ≠ Kotlin `!!`: у нас **recoverable typed throw** через
`Fail[E]` (ловится `with Fail`), а не непойманный NPE-crash. Одиночный
постфикс `!` намеренно оставлен свободным (нет force-unwrap-краша
коротким синтаксисом, D85).

> Анализ: 7-языковое сравнение (Rust/Go/TS/Kotlin/Java/Zig/Swift) +
> adversarial-оценка альтернатив (`expr!` / `expr?!` / `try expr` /
> `.unwrap_throw()` / «только `?? throw`») — все строго хуже или
> не-замена; `!!` оптимален. См.
> [docs/research/2026-06-29-bang-bang-operator-review.md](../../docs/research/2026-06-29-bang-bang-operator-review.md).

#### Почему `?` работает только на Option/Result, не на Fail

`?` это сахар над `match + return`. `match` работает только над
значением. Функция `Fail[E] -> T` возвращает `T`, не `Result[T, E]` —
сделать `match` на её результате нельзя.

Если программист хочет сконвертировать throw в Result, он использует
обычный `with`-блок:

```nova
ro r Result[int, ParseError] = with Fail[ParseError] = handler {
    fail(e) { interrupt Err(e) }
} {
    Ok(parse(s))    // parse без !! — throw сам ловится handler'ом
}
```

#### Цена миграции

D85 ломает текущий идиоматический Nova-стиль:
- Все `parse(s)?` в коде с `Fail[E] -> T` сигнатурой перестают
  работать как раньше. Нужно либо **переписать на `parse(s)!!`**
  (если хотим оставить throw-стиль), либо **изменить сигнатуру** на
  `Result[T, E]` (если хотим return-стиль).

В stdlib и тестах это десятки-сотни мест. Миграция запланирована как
отдельная задача (см. Plan-task post-D85).

### Что отвергнуто

- **Оставить D67 как был.** Категориальная неоднородность сохранилась бы.
- **Унификация через throw для обоих типов.** Каждый `lookup`/`find`/
  `parse_int` обязан был бы иметь `Fail[NoneError]` в сигнатуре —
  засоряет сигнатуры частных функций неинформативным эффектом
  ([R5.2](../revolutionary.md)).
- **Унификация через ранний return для обоих + полное удаление
  Fail-стиля.** `Fail` остаётся центральным механизмом языка через
  `throw`, `with`, handler'ы — `?` это просто перестаёт быть его
  сахаром. Полное удаление сломало бы R1.
- **Одиночный `!` под throw.** Конфликт с префиксным `!`, требует
  правил про пробелы. См. «Почему `!!`, не `!`».
- **`expr try` (Swift-style префикс).** Длиннее, не симметрично с
  `?` (постфиксом).
- **`!?` или `?!` как throw.** `?` уже занят, добавление к нему
  суффикса визуально путаниец.
- **Force-unwrap (Rust `.unwrap()`/Swift `!`) как краткий оператор.**
  В Nova нет force-unwrap-оператора — для краша используется panic
  через `?? panic(...)`, для throw — `!!` или `?? throw`. Никаких
  скрытых panic'ов через короткий синтаксис.
- **Авто-`From` конверсия типа ошибки (Rust-стиль `?`).** Рассмотрено:
  `?` сам конвертит тип ошибки через `From`-impl, как Rust
  (`Err(e) => return Err(From::from(e))`). **Отклонено** ([Plan 174.2](../../docs/plans/174.2-question-mark-return-only.md),
  Часть B). Причины: **(1)** противоречит принципу «не магия / single
  canonical path» — авто-`From` = спец-правило (компилятор ищет
  `From`-impl за тебя). **(2)** Боль меньше, чем в Rust: у Rust нет
  эффектов → ошибки текут только значениями, ремаппинг на каждом слое
  задалбывает; у Nova `Fail`-эффект пробрасывает сам, а ремаппинг типов
  идёт явно в `with Fail`-хендлере — авто-`From` помог бы только
  value-style `?` (ýже, не самый идиоматичный путь). **(3)** Скрывает
  конверсию в точке вызова (`parse(s)?` не показывает смену типа ошибки;
  может незаметно терять поля). **(4)** Асимметрия обратимости: добавить
  авто-`From` позже (если explicit задолбает) — легко и неломающе; убрать
  магию, когда на неё завязан код, — больно. Молодому языку — начать
  строго. **Замена:** при `E ≠ E'` — явный `expr.map_err(|e| E'.from(e))?`
  (видно на месте, не теряет данные молча). Ловушка при возможном
  пересмотре: `From[T] for T` (identity-blanket) существует для всех `T`,
  поэтому авто-конверсия допустима только при `E ≠ E'` И наличии
  **не-identity** `From[E]` на `E'`.

### Связь

- [D67](#d67--оператор-семантика-для-result-через-fail-для-option-через-ранний-return) — отменено D85.
- [D4](#d4) — `?` через Fail (отменено вместе с D67).
- [D25](#d25), [D65](#d65) — `Fail[E]` остаётся центральным
  механизмом throw'а; `!!` — её краткий синтаксис.
- [D26](08-runtime.md#d26) — prelude: `RuntimeNoneError` добавлен
  как unit-тип для `expr!!` на `Option`.
- [D13](08-runtime.md#d13) — `panic` / `exit` как fallback в
  `?? panic(...)` / `?? exit(...)`.
- [D46](03-syntax.md#d46) — `!` как boolean not, остаётся только
  префиксом.
- [D86](#d86) — `??` coalesce / fallback. Параллельный механизм:
  D85 (`?` / `!!`) для канонического return-/throw-стиля,
  D86 (`??`) для кастомного fallback'а с любым выражением.
- [R1 в revolutionary.md](../revolutionary.md) — обновляется: `?` для
  Fail-стиля заменён на `!!` в примерах.
- [Plan 19](../../docs/plans/19-closure-and-error-ops.md) — план
  атомарной реализации (closure-rev + D85 в одном PR).

### Эволюция

- **D67 (2026-04-XX)** — две семантики `?`, секция «Признанное
  напряжение» признавала кривизну дизайна.
- **D85 (2026-05-10)** — унификация: `?` всегда return, `!!` всегда
  throw. Обе работают для обоих `Option` и `Result`. `?` отвязан от
  `Fail`. Признанное напряжение снято — это были две операции, теперь
  у каждой свой символ.

#### Миграция кода

| Было (D67) | Стало (D85) |
|---|---|
| `parse(s)?` в `Fail[E] -> T` функции | `parse(s)!!` (если parse возвращает Result) или сменить сигнатуру на `-> Result` |
| `xs.first()?` в `-> Option[T]` функции | без изменений |
| `xs.first()?` в `Fail[E] -> T` функции | `xs.first()!!` (бросает RuntimeNoneError, требует `Fail[RuntimeNoneError]`) |
| `lookup(k) ?? throw E` | без изменений (или `lookup(k)!!` если устраивает RuntimeNoneError) |

Полный план миграции stdlib — отдельная задача.

---

## D86. `??` coalesce-оператор — fallback для `Result`/`Option`

> **AMEND (2026-07-07, решение владельца): методы-близнецы `@unwrap_or(v)` /
> `@unwrap_or_else(f)` РЕТРАКТИРОВАНЫ** — дублировали `x ?? v` (правая часть и так
> ленивая — сахар над match, покрывает и `_or_else`; факт дрейфа: 29+0 вызовов методов
> против 2 у канон-оператора). Ниша Result-`unwrap_or_else(fn(E) -> T)` с доступом к
> ошибке — 0 использований; когда ошибка нужна — явный `match`. Миграция
> `[M-unwrap-twins-retraction]` (волна-2 §4а): `.unwrap_or(v)` → `?? v`.

> **AMEND (2026-07-16, Plan 200 Пункт 14):** та же философия отбора применена
> в **обратную** сторону — `flat_map` (Option + Result) и `filter` (Option)
> **добавлены** в `std/prelude/core.nv`, т.к. это единственные канонические
> комбинаторы, невыразимые через `??`/`!!`/`.map`/`match` (bind без ручного
> снятия вложенности `M[M[U]]`; отбрасывание `Some` по предикату).
> `or_else`/`unwrap_or[_else]`/`map_or[_else]` — рассмотрены и **отклонены**,
> тот же класс, что unwrap-twins выше (выразимы `?? v` / `?? f()` /
> `.map(f) ?? d`). См. полный каталог и разбор —
> [08-runtime.md → D26 AMEND 2026-07-16](08-runtime.md#d26-базовая-stdlib-и-prelude),
> research `docs/research/2026-07-16-option-result-combinators.md`.

> **AMEND (2026-07-23, решение владельца): форма `X ?? return R` (fallback
> «`return ...` для раннего выхода из enclosing fn») РЕТРАКТИРОВАНА.**
> Основание: из 15 сайтов корпуса, ручным `match`'ем реализующих этот
> паттерн, 12 уже имеют канон КОРОЧЕ (`?` / `.ok()?` / `.map_err(..)?`) —
> то есть форма была второй дверью к [`?`](#d85-операторы--и--унифицированное-поведение-для-result-и-option-throw-стиль-через-)
> из соседнего D85, а не независимой нишей. По смыслу `??` — «подставь
> значение и продолжай вычисление»; ранний возврат из ДРУГОЙ функции — это
> поток управления, для которого в языке уже есть отдельные операторы
> (`?`/`!!`) и оператор `return`. Смешивать оба под одним `??` — категориальная
> ошибка того же рода, что унифицировал D85 (см. «Зачем унификация `?`» там же).
> Парсер уже фактически вёл себя как после ретракции (`E_COALESCE_RETURN_
> FALLBACK` — см. ниже), поэтому ретракция стоит ноль кода на уровне
> рантайм-семантики; добавлен только диагностический слой.
>
> **Чем заменяется** (таблица решений, воспроизводится из brief-инвентаря
> `[M-coalesce-return-fallback-unparsed]`):
>
> | ситуация | канон |
> |---|---|
> | та же обёртка наружу (`Option`→`Option` или `Result`→`Result`, тот же `E`) | `X?` ([D85](#d85-операторы--и--унифицированное-поведение-для-result-и-option-throw-стиль-через-)) |
> | `Result` → функция отдаёт `Option` | `X.ok()?` |
> | `Result` → функция отдаёт `Result`, но меняется тип ошибки | `X.map_err(fn(_ E) -> F => <ошибка>)?` (D85 отклонил авто-`From` ради явности; closure-full параметр обязан быть типизирован — голый `fn(_)` не парсится) |
> | `Option` → функция отдаёт `Result` | `X.ok_or(<ошибка>)?` |
> | обёртки для проброса нет (`bool`, кортеж и т.п. — `glob.nv`-класс) | явный `match` — **законен, не дефект** |
>
> Остальные три fallback-формы (значение, `throw`, `panic`) **без изменений** —
> все проверены рабочими на бинаре `6221c669b`; ретракция касается ТОЛЬКО
> `return`.
>
> **Диагностика:** `X ?? return R` парсится в AST (rustc-style
> parse-then-diagnose — подсказка контекстна, зависит от типа `X` и
> return-типа enclosing fn, которых парсер типов не знает), но чекер ВСЕГДА
> отвергает форму диагностикой **`E_COALESCE_RETURN_FALLBACK`** с
> контекстным `Suggestion` по таблице выше (`types/mod.rs::check_coalesce_
> return_fallback` / `coalesce_return_fallback_advice`). Три остатка
> (`std/src/path/glob.nv:48,89,122` — `return false` / `return (false, pi)`
> из `bool`/кортеж-функций) остаются `match`'ем законно (последняя строка
> таблицы) — диагностика на них не срабатывает (нет `Suggestion`, нет
> ошибки: `??`-форма для них и не существовала бы, замены нет).
>
> **Линт** `W_MANUAL_COALESCE` (`lints.rs`) ловит ОБРАТНУЮ сторону —
> ручной `match X { Ok(v) => v, Err(_) => D }` / `{ Some(v) => v, None => D }`
> (identity-рука) как дрейф от канона `X ?? D`; подсказка строится той же
> decision-функцией.

### Что

`expr ?? fallback` — **coalesce-оператор**: если `expr` это `Some(v)`
или `Ok(v)`, возвращает `v`; иначе возвращает значение `fallback`.

В отличие от [`?`](#d4--для-пробрасывания-ошибки) и
[`!!`](#d85-операторы--и--унифицированное-поведение-для-result-и-option-throw-стиль-через-)
— `??` **не требует** `Fail[E]` в сигнатуре. Это локальная чистая
операция, **поглощающая** ошибку/`None`, заменяя на fallback.

### Правило

```nova
ro v = lookup(id) ?? 0                // None → 0
ro r = parse(s)   ?? -1               // Err(_) → -1
ro port = config.get("port") ?? 8080  // default
```

Fallback может быть:
- **значением** того же типа, что внутри `Some`/`Ok`:
  ```nova
  ro port = config.get("port") ?? 8080
  ```
- **`throw err`** (custom ошибка):
  ```nova
  ro port = config.get("port") ?? throw MissingPortError
  ```
- **`panic("...")`** ([D13](08-runtime.md#d13)):
  ```nova
  ro port = config.get("port") ?? panic("port required")
  ```
- ~~`return ...` для раннего выхода из enclosing fn~~ — **РЕТРАКТИРОВАНО**
  (AMEND 2026-07-23 выше, `E_COALESCE_RETURN_FALLBACK`). Канон вместо этой
  формы — `X?` / `.ok()?` / `.map_err(..)?` / `.ok_or(..)?` / явный `match`
  (см. таблицу в AMEND).
- произвольным выражением, чей тип совместим с `T` (внутри `Some(T)` /
  `Ok(T)`) или имеет тип `never` (`throw` / `panic`).

Семантически — сахар над `match`:

```nova
expr ?? fallback
// разворачивается в:
match expr {
    Some(v)  => v
    None     => fallback
    Ok(v)    => v
    Err(_)   => fallback
}
```

`Err`-значение **не доступно** в fallback — `??` его отбрасывает.
Если нужен доступ — использовать `match` явно или `expr ?? throw`
(пробрасывает новую ошибку, не оригинальную).

#### Сравнение `?`, `!!`, `??`

| Оператор | На `Some(v)` / `Ok(v)` | На `None` / `Err` | Эффект |
|---|---|---|---|
| `expr?` | разворачивает в `v` | early-return из enclosing fn ([D67](#d67--оператор-семантика-для-result-через-fail-для-option-через-ранний-return)) | требует `Fail[E]` если `expr` это `Result` |
| `expr!!` | разворачивает в `v` | `throw err` (для `Option` — `RuntimeNoneError`) ([D85](#d85-операторы--и--унифицированное-поведение-для-result-и-option-throw-стиль-через-)) | требует `Fail[E]` |
| `expr ?? fb` | разворачивает в `v` | возвращает `fb` (или `throw`/`panic` если fb это они; `return` ретрактирован — AMEND 2026-07-23) | **без эффекта** для default-value fallback |

### Почему

1. **Локальная замена ошибки на default** — частый паттерн (config,
   lookup в map'е, parse с fallback'ом). `?` / `!!` для таких случаев
   слишком тяжёлы — заставляют завести `Fail[E]` в сигнатуре только
   ради того, чтобы тут же его catch'ить.
2. **Пуристая операция** — coalesce-оператор не требует эффектной
   системы. Чистая функция, видна на уровне выражения.
3. **Fallback может быть любым выражением** — включая `throw` для
   замены типа ошибки, или `return` для раннего выхода. Гибкость без
   накручивания grammar'а.
4. **Прецедент.** Swift `??`, JS/TS `??`, Rust `Option::unwrap_or` —
   узнаваемая convention.

### Что отвергнуто

- **`??=` null-coalescing assignment** ([rejected.md](history/rejected.md#%D0%BD%D1%83%D0%BB%D1%8C-coalescing-assignment))
  — десахар `a ??= e ≡ a = a ?? e` ломается типами: LHS `Option[T]`,
  RHS `T` (потому что `??` разворачивает Option), type mismatch.
- **Семантика «set-if-None»** для `??=` (`if a is None { a = e }`) —
  отличается от других compound-assignment операторов; вводит
  исключение в правило десахара.
- **`?? else { ... }`** — лишний синтаксис; `??` уже принимает любое
  выражение справа, включая block-`{ ... }`.
- **Доступ к `Err`-значению через `?? |e| ...`** — это уже `match`,
  не coalesce. `??` для случая «ошибка не важна, default достаточен».

### Связь

- [D4](#d4--для-пробрасывания-ошибки) — `?` early-return оператор,
  парный к `??` (распространение vs поглощение).
- [D85](#d85-операторы--и--унифицированное-поведение-для-result-и-option-throw-стиль-через-)
  — `?` и `!!` унифицированы для `Result`/`Option`; `??` — третья
  форма обработки.
- [D67](#d67--оператор-семантика-для-result-через-fail-для-option-через-ранний-return)
  — старая семантика `?`, поглощена D85; ссылки `?? для Option` из
  D67 теперь указывают сюда.
- [D13](08-runtime.md#d13) — `panic(...)` как fallback.
- [history/rejected.md](history/rejected.md) — отклонённый `??=`.

### Эволюция

В первых ревизиях `??` описан как подсекция [D4](#d4--для-пробрасывания-ошибки)
без собственного D-номера. 2026-05-10: выделен в отдельное решение
**D86** для:
- возможности независимой эволюции (`??` это про fallback, `?` про
  пробрасывание — разные роли);
- явных ссылок из spec / docs / runtime errors;
- симметрии с D85 (`?` и `!!`) — каждый постфиксный оператор имеет
  свой D.

Семантика **не изменилась** — это формальное выделение, не
содержательный пересмотр.

---

## D87. `Effect[E, IRT]` — параметризация `Handler` типом interrupt'а

### Что
Тип `Effect[E]` параметризован **двумя** generic-параметрами:
эффектом `E` и типом interrupt'а `IRT` (interrupt-return type).
Полная форма — `Effect[E, IRT]`. Default `IRT = never` через
[D88](03-syntax.md#d88-default-значения-generic-параметров) — то есть
`Effect[E]` ≡ `Effect[E, never]`.

`Effect[E, never]` — handler, который **не делает** `interrupt`
(только `return`/финальное выражение в handler-method'ах). Если
такой handler пытается сделать `interrupt v` — compile error.

`Effect[E, T]` (для `T` ≠ `never`) — handler, который **может**
сделать `interrupt v` где `v: T`. При использовании в `with`-блоке
type-checker унифицирует `T` с типом with-выражения (`W`) по правилам
[D61 секция 10](#d61).

### Зачем

Без D87 тип `Effect[E]`, возвращаемый из named функции, **не сообщает**
о том, делает ли handler `interrupt`. Программист, использующий такой
handler в `with`-блоке, не может локально (без чтения тела) понять,
какой тип получит `with`-выражение и совместим ли он с body. Это
противоречит принципу Nova R1 «эффекты и связанные контракты всегда
видны в сигнатуре».

### Правило

#### Базовая форма

```nova
type Logger effect {
    log(msg str) -> ()
}

// Handler без interrupt'а — sugar `Effect[Logger]` ≡ `Effect[Logger, never]`
fn console_logger() -> Effect[Logger] => effect Logger {
    log(msg) => println(msg)
}

// Handler с interrupt'ом типа int
fn fatal_logger() -> Effect[Logger, int] => effect Logger {
    log(msg) {
        if msg.starts_with("FATAL") { interrupt -1 }
        println(msg)
    }
}
```

#### Использование в `with`-блоке

```nova
// Effect[Logger, never] — interrupt запрещён, with-блок даёт T_body:
ro r = with Logger = console_logger() {
    Logger.log("hello")
    "ok"                    // T_body = str
}
// r: str

// Effect[Logger, int] — IRT = int должен быть совместим с T_body:
ro r = with Logger = fatal_logger() {
    Logger.log("FATAL: oom")
    "ok"                    // ❌ T_body = str, IRT = int → несовместимы
}
// COMPILE ERROR: cannot unify with-block type
//   handler interrupt type: int
//   body type:              str
```

Чтобы пример работал — нужно привести типы:

```nova
ro r = with Logger = fatal_logger() {
    Logger.log("FATAL: oom")
    -1                       // T_body = int, совпадает с IRT
}
// r: int
```

или явно указать общий supertype:

```nova
ro r any = with Logger = fatal_logger() {
    Logger.log("FATAL: oom")
    "ok"
}
// r: any (programmer opted into dynamic typing)
```

#### Compile-time проверки

Компилятор enforce'ит:

| Проверка | Когда |
|---|---|
| `Effect[E, never]` не содержит `interrupt` в handler-method'ах | при compilation handler-литерала |
| `interrupt v` где `typeof(v) ⊑ IRT` | при compilation handler-литерала |
| `IRT ⊑ W` (где `W` — тип with-выражения) | при compilation `with` |

Если условие не выполнено — compile error со ссылкой на конкретное
место.

#### Inference IRT

`IRT` чаще всего **выводится** из тела handler-литерала:

```nova
// IRT выводится из interrupt-выражений:
fn make_handler() -> Effect[Logger, _] => effect Logger {
    log(msg) {
        if msg.starts_with("FATAL") { interrupt -1 }    // IRT = int
        println(msg)
    }
}
// эквивалентно:
fn make_handler() -> Effect[Logger, int] => effect Logger { ... }
```

Для return-position parent fn'а компилятор смотрит на тип return и
проверяет совместимость с inferred IRT.

#### Несколько `interrupt` с разными типами

Если handler-method содержит несколько `interrupt v_1`, `interrupt v_2`,
... — `IRT` выводится как **наименьший общий supertype** их типов:

```nova
fn make_handler() -> Effect[Logger, Result[(), str]] => effect Logger {
    log(msg) {
        if msg.starts_with("ERROR") { interrupt Err("logged error") }
        if msg.starts_with("FATAL") { interrupt Ok(()) }
        println(msg)
    }
}
// IRT = Result[(), str] — supertype Err(str) и Ok(())
```

Если supertype'а нет — compile error «handler has incompatible
interrupt types».

#### Inline handler в `with`-блоке

Когда handler-литерал стоит **прямо** в `with`-блоке (не передаётся
как value через named fn), `IRT` определяется по правилам D61 секция 10
(unify body ↔ interrupt). Параметризация `Effect[E, IRT]` тут
**неявная** — компилятор знает контекст и не требует явных аннотаций:

```nova
ro r = with Fail[E] = effect Fail[E] {
    fail(err) => interrupt -1
} {
    fetch_count()
}
// IRT inferred = int (из interrupt -1)
// W = int (из body fetch_count())
// совместимо → r: int
```

### Migration: handler-лямбда

Handler-лямбда [D31](#d31-handler-лямбда-для-эффектов-с-одной-операцией)
автоматически работает с D87:

```nova
// Inline в with — IRT inferred из контекста:
with Fail[E] = |err| interrupt Err(err) {       // IRT = Result[T, E]
    Ok(work())
}

// Returned from named fn — нужен явный IRT:
fn make_fail_handler() -> Effect[Fail[E], Result[T, E]] =>
    |err| interrupt Err(err)
```

### Что отвергнуто

- **`Effect[E]` без второго параметра разрешает `interrupt`** —
  отвергнуто. Тогда тип не сообщает о возможности interrupt'а, и
  программист не может локально понять что будет в `with`-блоке.
- **Effect-row для interrupt'ов** (`fn make() -> Effect[E] interrupts T`) —
  отвергнуто. Сложнее парсить, нет прецедентов, не композируется с
  generic'ами так же чисто как второй параметр.
- **Implicit IRT через inference во всех случаях** — отвергнуто.
  Inference работает для inline и для local fn (Plan 19 first-use),
  но для **public API** функций IRT должен быть явно в сигнатуре —
  иначе программист (или LLM) не увидит контракт без чтения тела.

### Связь

- [D61](#d61-полная-семантика-эффектов-effect-keyword-handler-литерал-handlere-interrupt) —
  семантика `interrupt`, тип with-блока, базовое определение `Effect[E]`.
  D87 расширяет до `Effect[E, IRT]`.
- [D31](#d31-handler-лямбда-для-эффектов-с-одной-операцией) —
  handler-лямбда `|x| body`. Совместима с D87: IRT inferred из тела.
- [D88](03-syntax.md#d88-default-значения-generic-параметров) —
  default-значения generic'ов; `IRT = never` использует этот механизм.
- [D26](08-runtime.md#d26) — `never` как bottom-type.
- [revolutionary.md → R1](../revolutionary.md) — принцип «контракты
  видны в сигнатуре».

### Эволюция

Зафиксировано 2026-05-10. Закрывает gap, выявленный при обсуждении
closure-rev и handler-лямбды на `|x|`: тип `Effect[E]` не сообщал о
способности handler'а делать `interrupt`, что нарушало R1 «контракты
в сигнатуре».

Migration: ~10 примеров `Effect[E]` в spec/, где handler делает
`interrupt`, перевести на `Effect[E, IRT]`. Inline handler-литералы
в `with`-блоках не требуют миграции (IRT inferred неявно по D61).

### Plan 97 amendment (2026-05-22) — Handler → Effect rename

С [D142](02-types.md#d142) (Plan 97 Ф.3) builtin переименован:

| Pre-D142  | Post-D142  |
|---|---|
| `Handler[E]`        | `Effect[E]`        |
| `Handler[E, IRT]`   | `Effect[E, IRT]`   |
| `Handler[E, never]` | `Effect[E, never]` |

Семантика **полностью идентична** — это renaming, не пересмотр.
`Effect[E, IRT]` остаётся встроенным generic-типом с двумя
параметрами (E — эффект, IRT — interrupt-return-type),
default-значение `IRT = never` через [D88](03-syntax.md#d88).

Имя `Effect` выбрано для симметрии с keyword'ом литерала `effect`:

```nova
// declaration ─ keyword `effect` после имени
type Logger effect { log(msg str) -> () }

// literal ─ тот же keyword `effect` префиксом (без `type`)
fn console_logger() -> Effect[Logger] => effect Logger {
    log(msg) => println(msg)
}

// type-position ─ builtin `Effect[...]`
fn run(h Effect[Logger]) -> () => with Logger = h { ... }
```

Три use-site'а (declaration / literal / type-position) пишутся
через одну вариативность keyword'а: `effect` — для синтаксиса,
`Effect[...]` — для типа значения. Тавтологии «Handler для Effect»
больше нет — `Effect[Logger]` читается как «значение-effect для
протокола Logger» (то же, что `Result[int, str]` — «значение-result
для int и str»).

Миграция (clean break, без backwards-compat) — sweep одной CL'ой по
prelude / std / nova_tests / examples / spec.

---

## D120. `#pure` views + axioms + `#verify`/`#trusted` handlers

**Статус:** Принято (Plan 33.3 Ф.9, реализовано 2026-05-14)

### Решение

Эффект или протокол может объявлять **`#pure` операции** (чистые
проекции состояния) и **`axiom`** — утверждения об их поведении,
используемые SMT-движком при верификации контрактов.

```nova
effect Db {
    setBalance(id AccountId, x money) -> ()
    #pure balance(id AccountId) -> money
    axiom non_negative(id) =>
        balance(id) >= 0
    axiom balance_after_set(id, x) =>
        post(setBalance(id, x))(balance(id)) == x
}
```

`with`-binding для эффекта, у которого объявлены axioms, обязан
явно указывать `#verify` или `#trusted`:
- **`#verify`** — компилятор символически проверяет handler'а против axioms.
- **`#trusted`** — axioms принимаются без proof (handler содержит FFI,
  IO или ветвление, не поддерживаемое V1 symbolic execution).

```nova
with #trusted Db = ffi_handler { ... }   // контракт принят на доверие
with #verify  Db = pure_handler { ... }  // V1: gate принят, Ф.9.7 — pending
```

Без явного атрибута использование такого handler — **compile error**.

### Обоснование

Эффекты с axioms приобретают формальный контракт, проверяемый SMT.
Это позволяет `ensures Db.balance(to) == old(Db.balance(to)) + amount`
доказываться без знания тела handler'а — достаточно axiom'ов эффекта.
Nova — единственный mainstream-язык с effect-aware contracts в сигнатуре.

### Реализация

- `compiler-codegen/src/ast/mod.rs` — `OpKind::PureView`, `EffectAxiom`,
  поле `axioms: Vec<EffectAxiom>` в `TypeDecl`.
- `compiler-codegen/src/parser/mod.rs` — синтаксис `#pure op`, `axiom name(binders) => formula`.
- `compiler-codegen/src/types/mod.rs` — type-check axiom-body, gate `#verify`/`#trusted`.
- `compiler-codegen/src/verify/encode.rs` — кодировка `#pure` view → UF,
  axiom → `Z3_mk_forall_const`; inconsistency check pre-flight.

### Ограничения V1

- `#verify` (Ф.9.6) принимает атрибут и применяет gate, но symbolic
  handler verification (Ф.9.7) — placeholder, реализация в Plan 33.4 Ф.1.
- Поддерживаются только static axioms (`balance(id) >= 0`); axioms про
  state transition (`post(action)(view) == X`) — V2.

---

## D115. Axiom binder — `BinderType` enum вместо `Option<TypeRef>`

**Статус:** Принято (Plan 33.4 P1-5, реализовано)

### Решение

Параметры axiom-формулы ранее представлялись как `Vec<(String, Option<TypeRef>)>`,
где `None` означало «без типа». Семантически существуют **три** различных
состояния:

| Состояние | Смысл |
|---|---|
| `Untyped` | Binder без аннотации (`axiom foo(x)`) |
| `Typed(TypeRef)` | Binder с конкретным типом (`axiom foo(x AccountId)`) |
| `Generic(String)` | Binder через generic-параметр эффекта (`axiom foo(x T)`) |

Введён enum:

```rust
pub enum BinderType {
    Untyped,
    Typed(TypeRef),
    Generic(String),
}
pub struct BinderDef {
    pub name: String,
    pub kind: BinderType,
    pub span: Span,
}
```

`EffectAxiom.binders` теперь `Vec<BinderDef>`.

### Обоснование

`Option<TypeRef>` не различал `Untyped` и `Generic` — оба давали `None`.
Enum устраняет двусмысленность и позволяет SMT-encoder правильно
выводить sort для каждого binder'а (Generic → sort из параметра эффекта).

### Реализация

`compiler-codegen/src/ast/mod.rs`, `parser/mod.rs`, `types/mod.rs`,
`verify/pipeline.rs` — механический рефактор (4 файла).

---

## D118. Typed `Fail[E]` codegen — payload preservation via fail-frame

> **Status:** active (spec). Реализация — [Plan 61](../../docs/plans/61-typed-error-effect-codegen.md).
> Расширяет [D25](#d25)/[D65](#d65)/[D85](#d85).

### Что

`throw expr` где `expr: T` (T ≠ `nova_str`, e.g. record/sum variant) —
payload передаётся через `NovaFailFrame.error_user_payload` +
`NovaFailFrame.error_user_type_id` (NovaTypeId). Handler-arm `|e: E|`
читает payload как `(E*)payload` через transparent C cast.

До Plan 61: codegen делал `Nova_Fail_fail(nova_int_to_str((nova_int)val))` —
silent pointer-to-int pun. Handler получал garbage string. **Silent UB #1**
закрыт Plan 61 Ф.2/Ф.3.

### Правило

1. **Throw lowering** (Stmt + Expr):
   - `expr: nova_str` → legacy `Nova_Fail_fail(msg)`.
   - `expr: T*` или value → `nova_throw_typed(msg_repr, payload, NOVA_TID_<T>)`.
     Value-types heap-boxed inline (`nova_alloc(sizeof(T))` + copy).

2. **Handler-arm typed binding:**
   - `with Fail[E] = |e| body` — compiler infer'ит `e: E` из effect-type
     (Plan 61 Ф.3 inference в `desugar_handler_lambda`).
   - В body `Ident(e)` resolves через
     `(E*)_nova_fail_top->error_user_payload` (pointer) или
     `*(E*)_nova_fail_top->error_user_payload` (value).
   - Pattern-match `match e { ... }` работает natural — field-access
     проходит через typed cast.

3. **Dispatch precedence** (`nova_throw_typed` в effects.h):
   1. `_nova_handler_Fail_any` — erased typed slot (Fail ≡ Fail[any]
      catch-all D65 правило 1). Если установлен, вызывается с
      `(payload, tid)`.
   2. `_nova_handler_Fail` — legacy string slot. Вызывается с `msg_repr`
      (typeid name). Handler arm может typed (читает payload через
      frame) или string (читает msg).
   3. Unwind через fail-frame с typed payload preserved
      (`error_user_payload` set'нут на step 0 — до dispatch).

4. **D65 правило 3 (re-throw):** `_nova_handler_Fail = current->prev`
   swap во время handler-body invocation — корректно работает с typed
   throws потому что `nova_throw_typed` reuses тот же swap pattern.

5. **`expr!!`:** codegen эмитит `Nova_Fail_fail(err_payload)` для
   bootstrap-stage Result (hardcoded на `Err = nova_str`). После Plan 14/56
   generic Result mono'd — Err получит real type, codegen перейдёт на
   `nova_throw_typed`. Plan 61 Ф.4 removed `nova_throw_value` placeholder
   macro — был **Silent UB #2** (silently замещал payload на строку
   `"Result::Err"`).

### Codegen representation

```c
/* NovaFailFrame extended (Plan 61 Ф.2): */
typedef struct NovaFailFrame {
    jmp_buf            jmp;
    nova_str           error_msg;
    NovaThrowKind      error_kind;          /* USER / CANCEL / USER_TYPED */
    void*              error_reason_ptr;    /* Plan 49 typed cancel */
    void*              error_user_payload;  /* Plan 61 typed user payload */
    NovaTypeId         error_user_type_id;  /* Plan 61 type tag */
    struct NovaFailFrame* prev;
} NovaFailFrame;

/* NovaTypeId (Plan 61 Ф.1, typeid.h): */
typedef uint32_t NovaTypeId;
/* Reserved 1..16 для primitives. User types — IDs from USER_BASE = 17
 * через compile-time auto-register в codegen (splice'тся в preamble как
 * #define NOVA_TID_USER_<X> N). */

/* Erased typed slot (Plan 61 Ф.2): */
typedef struct NovaVtable_Fail_any {
    void*                            ctx;
    nova_unit                       (*fail)(void* _ctx, void* err, NovaTypeId tid);
    struct NovaVtable_Fail_any*       prev;
} NovaVtable_Fail_any;

extern __thread NovaVtable_Fail_any* _nova_handler_Fail_any;
```

### Plan 61 followup (production-grade closure 2026-05-17)

Все 4 ранее-deferred items закрыты production-grade в followup session:

1. **Cross-effect throw в handler-arm** (`with Fail[A] = |e| throw B {...}`)
   — закрыто через **owner_iframe** поле в NovaVtable_Fail / NovaVtable_Fail_any
   + новый TLS slot `_nova_current_handler_iframe` (set/restored dispatcher'ом
   в Nova_Fail_fail / nova_throw_typed / per-E throw entries).
   `nova_interrupt` / `nova_interrupt_ptr` сначала смотрят этот slot —
   handler-arm `interrupt v` jump'тся в OUR with-block, не в
   _nova_interrupt_top (который может быть inner nested). Это
   architectural fix interrupt-frame routing для cross-effect dispatch.

2. **Stdlib migration** — `semver_range.parse_version` мигрирован на
   idiomatic D65 правило 3 form (`with Fail[A] = |_e| throw NewErr {...}`)
   после Plan 61 fu#1. Other stdlib usages (`retry.nv` Result-wrap для
   last_error capture, `http.nv` / `audit.nv` convert-to-Response
   patterns) — legitimate patterns, не workaround; задокументировано.

3. **Generic Result typed Err** — *(история: Plan 61 fu#3)* hybrid через
   extended `Nova_Result` struct (`err_typed_payload` + `err_typed_type_id`)
   + `nova_make_Result_Err_typed(payload, tid)`. **✅ Заменено полной
   мономорфизацией (Plan 59 Ф.7.5 increment 2, 2026-05-21):** `Result[T,E]`
   → per-(T,E) тип `NovaRes_<ok>_<err>*`, где `payload.Err._0` несёт
   реальное typed-значение Err напрямую. `Err(custom_value)` строит
   mono-инстанс (hybrid `nova_make_Result_Err_typed` early-return удалён);
   `expr!!` для non-str Err — `nova_throw_typed` с реальным payload'ом.
   typed-Err поля (`err_typed_payload`/`err_typed_type_id`) сохранены в
   схеме mono-типа для `Result[T, str]`-кейсов.

4. **Per-E TLS slots + per-E vtable** — реализовано через preamble splice
   `/*__PER_E_FAIL_DECLS__*/`. Для each E type registered in
   per_e_fail_types — эмиттится typedef `NovaVtable_Fail_<E>` (typed `(void*
   ctx, E* err)` signature), TLS slot `_nova_handler_Fail_<E>`, fast-path
   `_nova_throw_typed_<E>(E* payload)` dispatcher. **Dual-install** в
   emit_with: для `with Fail[E] = ...` install legacy `_nova_handler_Fail`
   (current) **AND** per-E slot через adapter wrapper (sets typed payload
   в fail-frame, delegates к legacy handler). `Stmt::Throw` /
   `ExprKind::Throw` для concrete E emit per-E throw entry (fallback к
   erased path preserves payload via fail-frame).

### Что отвергнуто

- **String-only `Fail`** — ломает D65 правило 1.
- **`nova_throw_value` placeholder** — УДАЛЁН в Plan 61 Ф.4 (Silent UB #2).
- ~~**Full per-(T, E) Nova_Result mono struct** — требует extension Plan
  48/59 mono на sum types. Hybrid через extended Nova_Result (typed slot)
  даёт equivalent semantics для bootstrap; full mono — future polish.~~
  **✅ РЕАЛИЗОВАНО (Plan 59 Ф.7.5 increment 2, 2026-05-21):** full
  per-(T,E) Result mono — `NovaRes_<ok>_<err>*`. Hybrid через extended
  `Nova_Result` снят, остался лишь как back-compat `#define`-алиас.

### Связь

- [D25](#d25)/[D65](#d65) — Fail семантика, правила 1-5.
- [D85](#d85) — `expr!!` semantics.
- [Plan 11](../../docs/plans/11-method-values-and-overload.md) — закрыт
  cross-effect throw bug в Plan 61 followup #1 (owner_iframe routing).
- [Plan 59 Ф.7.5](../../docs/plans/59-tuple-monomorphization.md) —
  full per-(T,E) Result mono struct `NovaRes_<ok>_<err>` ✅ реализован
  (increment 2, 2026-05-21); заменил hybrid extended `Nova_Result` из
  Plan 61 fu#3.
- [Plan 49](../../docs/plans/49-cancel-throw-routing.md) — симметричная
  typed-payload infra для CANCEL kanal. Plan 61 — для USER kanal. Две
  оси параллельны.
- [D158](03-syntax.md#d158) — failable defer/errdefer body. Расширяет
  `NovaFailFrame` полем `error_suppressed` (singly-linked `NovaErrorChain`)
  для multi-error composition при cleanup-fail во время propagation.
  Plan 100.4.1 (2026-05-23 proposed; runtime impl extends этот D118
  fail-frame layout).

---

## D185. `ResourceTrace` effect — observability-only handler dispatch

> **Plan 110 Ф.7.** Принято 2026-05-31. **Статус: ACTIVE** (Plan 110.4.4.a/b
> codegen emits enter/exit dispatch, 2026-06-01). **Амендмент Plan 173 Ф.2.R1 (2026-07-04, RENAME-only):**
> эффект переименован `Cleanup`→**`ResourceTrace`** (освобождает имя `Cleanup` для протокола
> `Cleanup[E]`, ex-`Consumable`, D314), операции `on_scope_enter/exit`→**`on_resource_enter/exit`**.
> **Амендмент Plan 173 Ф.5 п.2 (2026-07-10, D192-ретракт):** параметр `timeout` ДРОПНУТ из
> `on_resource_enter` (§3a/п.8) — порог стал внутренним параметром watchdog-варна; exit-событие
> получило `duration_ms int` (измеренная длительность cleanup-вызова) и `overrun bool`
> (true = cleanup превысил watchdog-порог из 3-level D192-resolution). Прежние
> D195-Application-override тесты (`timeout_application_level2_t3_8`, `application_cross_fiber_t8_7`)
> мигрированы на поведенческое наблюдение порога через overrun.
> Observability-only effect для tracing resource-scope entry/exit. Default handler — no-op,
> zero-overhead если не использован. Orthogonal к `Cleanup[E].@cleanup` (ex-`Consumable.on_exit`,
> resource lifecycle) — слой для metrics/tracing.

### Что

```nova
effect ResourceTrace {
    on_resource_enter(label str) -> ()
    on_resource_exit(label str, outcome ScopeOutcome, duration_ms int, overrun bool) -> ()
}
```

Default handler — no-op:

```nova
fn ResourceTrace.default() -> ResourceTraceHandler => ResourceTraceHandler { /* no-op */ }
```

### Codegen integration

При входе в `consume X = init() { body }` codegen эмитит (если `ResourceTrace`
effect handler активен):

```c
perform_ResourceTrace_on_resource_enter(type_label(X));
// ... body ...
/* duration измеряется вокруг cleanup-вызова; overrun = duration > threshold */
perform_ResourceTrace_on_resource_exit(type_label(X), _outcome, _duration_ms, _overrun);
```

Если handler === default no-op (compile-time check) — calls elided через
[D194](03-syntax.md#d194)-style optimization. Zero overhead.

### Handler restrictions

1. **Handler не может `throw`** — observability должна быть idempotent.
   Compile error `D185-resourcetrace-handler-throw` если signature handler'а
   `throw`'ит.

2. **Return type должен быть `()`** — observability-only. Compile error
   `D185-resourcetrace-handler-non-unit-return`.

3. **Handler не может `suspend`** — observability должна быть sync
   relative to scope-entry/exit. Async export через off-thread queue в
   handler implementation если нужен.

### OpenTelemetry wire format (D185 §otel)

Reference implementation `CleanupHandler.to_otel(exporter)`:

#### on_resource_enter — создаёт span

```
attributes = {
    "resource.label":         label,
    "resource.start_time_ns": now_ns(),
}
span_kind = INTERNAL
parent = active_span()
```

#### on_resource_exit — закрывает span

```
status = match outcome {
    Success      => OK
    Failure(_)   => ERROR { code: "cleanup_failed" }
    Panic(_)     => ERROR { code: "cleanup_panic" }
}
attributes.duration_ms = duration_ms   // из exit-события (длительность cleanup)
attributes.overrun     = overrun       // watchdog-порог превышен (D192-ретракт: варн, не прерывание)
end_time = now()
```

#### Trace context propagation

Spans nested correctly через scope-stack ([D188](03-syntax.md#d188) §R5).
Parent span = enclosing scope-handler's span. Cross-fiber propagation
через [D80](#d80) effect snapshot.

#### Compatibility

Compatible с std OpenTelemetry SDK через FFI bridge (cross-ref [Plan
100.5](../../docs/plans/100.5-ffi-external-integration.md)).

### Use cases

- Production tracing — per-resource cleanup duration → APM.
- Debugging — long-running slow cleanup → визуальные spans.
- Audit — какие resource'ы cleanup'или в каком порядке.
- Performance regression detection — baseline cleanup performance.

### Что НЕ ResourceTrace effect

- ❌ Не resource lifecycle — это `Cleanup[E].@cleanup` (ex-`Consumable.on_exit`, D314).
- ❌ Не для cancel control — это shield (D188 R3).
- ❌ Не для timeout adjustment — это scope-дедлайн `supervised(deadline:/timeout:)` (§3a; D192-ретракт).

### Связь

- [D80](#d80) — effect snapshot для cross-fiber.
- [D188](03-syntax.md#d188) §R5 — scope-stack LIFO.
- [Plan 100.5](../../docs/plans/100.5-ffi-external-integration.md) — FFI bridge.
- [Plan 100.8](../../docs/plans/100.8-performance-ide-tooling.md) — performance + tooling.
- [Plan 110 Ф.7](../../docs/plans/110-scoped-resources-radical-simplification.md).

---

## D195. `Application` effect — nesting + finalizer scoping + cross-fiber propagation

> **Plan 110 Ф.8.** Принято 2026-05-31. **Статус: ACTIVE** (Plan 110.4.6.a
> Level-2 + 110.4.7 cross-fiber D80 snapshot landed 2026-06-01). Application
> как ambient capability для top-level lifecycle: finalizers + default
> exit_timeout. Cross-ref [D188](03-syntax.md#d188) §R4 +
> [D192](03-syntax.md#d192) Level-2.

### Что

```nova
effect Application {
    fn register_finalizer(f fn() -> ()) -> ()
    fn default_exit_timeout() -> Duration
}

type ApplicationHandler {
    mut finalizers                []fn() -> ()
    ro  default_exit_timeout_value Duration
}

fn Application.handler(default_exit_timeout Duration = 5.s()) -> ApplicationHandler
    => ApplicationHandler { finalizers: [], default_exit_timeout_value: default_exit_timeout }

fn ApplicationHandler @register_finalizer(f fn() -> ()) -> () => @finalizers.push(f)
fn ApplicationHandler @default_exit_timeout() -> Duration => @default_exit_timeout_value

// Handler сам Cleanup — finalizers fire при выходе из with-блока:
fn ApplicationHandler consume @cleanup(_outcome ScopeOutcome) -> () {
    for f in @finalizers.reverse() { f() }
}
```

### Idiomatic main pattern

```nova
fn main() Io -> () {
    with Application = Application.handler(default_exit_timeout: 10.s()) {
        run_server()
        // anywhere глубоко: Application.register_finalizer(|| { ... })
    }
    // handler.cleanup fires finalizers в reverse order
}
```

### R1 — Inner handler wins (effect-stack semantics)

```nova
with Application = h2 {
    with Application = h1 {
        // Application.X operations бьют по h1 здесь
    }
    // здесь — по h2
}
```

Стандартная effect-stack семантика — inner handler побеждает.

### R2 — Finalizer registry NOT inherited

Inner handler `h2` имеет **свой пустой** registry. Finalizers registered
внутри `with Application = h2` scope не visible снаружи; на exit h2
запускаются h2.finalizers, h1.registry не trogается.

```nova
with Application = h1 {
    Application.register_finalizer(|| println("h1.A"))
    with Application = h2 {
        Application.register_finalizer(|| println("h2.A"))
        // h2.finalizers = [h2.A]
        // h1.finalizers = [h1.A]
    }
    // h2 exits → prints "h2.A"
}
// h1 exits → prints "h1.A"
```

### R3 — Default exit_timeout NOT inherited

`h2` имеет **свой** `default_exit_timeout_value`. Если `h2` создан без
аргумента — использует hardcoded default `5.s()`, **не** h1's value:

```nova
with Application = Application.handler(default_exit_timeout: 30.s()) {  // h1
    with Application = Application.handler() {                          // h2 — 5s, не 30s
        consume tx = db.begin() { ... }  // timeout 5s
    }
}
```

Deliberate — позволяет inner scope override без implicit inheritance
(test isolation use case).

### R4 — Test isolation

Каждый test получает свой isolated Application; не shareit finalizers с
runner'ом:

```nova
fn test_user_registration() Io -> () {
    with Application = Application.handler() {
        Application.register_finalizer(|| cleanup_test_db())
        run_scenario()
    }
    // finalizers fire здесь, runner не affected
}
```

### R5 — Integration с D192

Codegen `nv_resolve_exit_timeout` Level-2 check:

```c
nv_handler_t* app = nv_effect_lookup("Application");
if (app) {
    return nv_call_method(app, "default_exit_timeout");
}
```

Inner handler побеждает через effect-stack (R1) — `nv_effect_lookup`
возвращает active handler from top of stack.

### R6 — Cross-fiber propagation

При `spawn { ... }` дочерний fiber видит родительский effect-stack
([D75](06-concurrency.md#d75) cancel-token model extension), включая активный Application:

```nova
with Application = Application.handler(default_exit_timeout: 10.s()) {
    spawn {
        Application.register_finalizer(|| ...)   // регистрирует в parent's handler
        consume tx = db.begin() { ... }          // использует parent's 10s
    }
}
```

Snapshot effect-stack at spawn-point ([D80](#d80) semantics). Child видит
parent's Application даже после exit parent — refcount keeps handler
alive до последнего fiber.

### R7 — Boot order

`Application.handler(...)` constructor должен **полностью завершиться**
до входа в `with`-блок. Никаких регистраций finalizer'ов во время
construction — только из body. Если constructor throws — `with` не
входит, `cleanup` не вызывается ([D188 R1](03-syntax.md#d188) partial-construction
safety).

### R8 — Abort / SIGKILL не fires finalizers

Документировано как ограничение всех языков:
- `abort()` / SIGKILL / SIGSEGV → process killed; OS unmaps memory;
  finalizers NOT run.
- `exit(code)` — fires handler.cleanup (controlled exit) → finalizers run.

`#[run_on_abort]` атрибут — follow-up Plan 110.X (если будет нужно).

### Связь

- [D75](06-concurrency.md#d75) — CancelToken model.
- [D80](#d80) — cross-fiber effect snapshot.
- [D188](03-syntax.md#d188) §R1 boot-order, §R5 LIFO.
- [D192](03-syntax.md#d192) Level-2 — 3-level resolution integration.
- [D198](03-syntax.md#d198) — realtime bypass этого Level-2.
- [Plan 100.4.1](../../docs/plans/100.4.1-failable-cleanup-body.md) — handler cleanup mechanism.
- [Plan 110 Ф.8](../../docs/plans/110-scoped-resources-radical-simplification.md).

---

## D209 — Protocol method `@` syntax + receiver mutability (Plan 108.4, 2026-06-09)

**Plan:** [108.4-protocol-method-receiver-mut.md](../../docs/plans/108.4-protocol-method-receiver-mut.md).
**Status:** ACTIVE.
**Depends on:** [D58](03-syntax.md#d58-range-литерал-itert-protocol-for-x-in-c-implicit-iter) (structural protocols), [D72](02-types.md#d72-generic-bounds-через-t-protocol--protocol-как-тип) (generic bounds), [D186](02-types.md#d186--impip1--p2---opt-in-annotation-для-protocols) (`#impl` annotation), Plan 108.1/108.2/108.3 (default-ro family).

### Что

Protocol instance-methods требуют `@` перед именем метода. Receiver
mutability prefix (`mut`/`ro`/`consume`) опционален перед `@`.
Default = `ro` (consistent с Plan 108.1/108.2/108.3 default-ro paradigm
для params/locals/loops/patterns).

**Visual distinction — protocols vs effects:**

```nova
// Effect — набор функций (нет receiver, нет @):
type Logger effect {
    log(msg str) -> ()
}

// Protocol — набор методов (есть receiver = @):
type Closeable protocol {
    consume @close() -> Result[(), Error]
}
```

### Правило

**`@` обязателен** перед именем instance-метода в `type X protocol { }` declaration.
Static methods используют `.method()` (leading dot, без изменений).
Effect `effect { }` blocks — без изменений, нет `@`.

```nova
type Next[T] protocol {
    mut @next() -> Option[T]               // mut receiver
}

type Iter[I] protocol {
    @iter() -> I                           // ro receiver
}

type Closeable protocol {
    consume @close() -> Result[(), Error]  // consume receiver
}

type Compare[T] protocol {
    @compare(other ro T) -> int            // ro receiver, ro param
}

type Hash protocol {
    @hash() -> u64                          // ro — детерминированный hash
}
```

### Грамматика

```ebnf
// Protocol — @ обязателен для instance; . для static (без изменений)
proto_method_decl  ::= ("mut" | "ro" | "consume")? "@" IDENT "(" param_list? ")"
                       effect_list? ("->" type)?
proto_static_decl  ::= "." IDENT "(" param_list? ")" effect_list? ("->" type)?

// Effect — без изменений (функции без receiver)
effect_method_decl ::= IDENT "(" param_list? ")" effect_list? ("->" type)?
```

Default receiver: `ro` (без prefix'а). `ro @method()` ≡ `@method()` (explicit, но redundant).

### Enforcement

**Parse-time errors:**
- `E_PROTO_METHOD_NEEDS_AT` — instance-метод без `@` (bare `method()`) в protocol declaration.
  Hint: «add `@` before method name: `@method()`».
- `E_PROTO_METHOD_MOD_CONFLICT` — multiple modifiers (`mut ro @foo()`, `mut consume @bar()`).

**Type-checker errors (impl mismatch):**
- `E_PROTO_IMPL_RO_FOR_MUT` — protocol `mut @m()`, impl ro.
- `E_PROTO_IMPL_MUT_FOR_RO` — protocol `@m()` (ro), impl mut.
- `E_PROTO_IMPL_MUT_FOR_CONSUME` — protocol `consume @m()`, impl mut/ro.
- `E_PROTO_IMPL_CONSUME_FOR_MUT` — protocol `mut @m()`, impl consume.

Enforcement paths:
1. **`#impl(P)` annotation** (D186) — declares-conformance: type-checker matches every
   method's `receiver_mut` at type-declaration site.
2. **Structural conformance** (D58) — at use-site (for-in / generic bound `[T Protocol]`).

### Сравнение с mainstream

| Язык | Receiver mutability в protocol/trait/interface |
|---|---|
| Rust | `trait Iterator { fn next(&mut self) -> Option<Self::Item> }` — explicit `&mut self` |
| Swift | `protocol IteratorProtocol { mutating func next() -> Element? }` — `mutating` keyword |
| Go | `interface { Next() *T }` — implicit pointer mut |
| Kotlin/Java | нет static mutability tracking |
| **Nova (Plan 108.4)** | `protocol { mut @next() -> Option[T] }` — `@` обязателен + `mut`/`ro`/`consume`, enforced |

### Stdlib migration (Ф.3)

All existing protocol declarations updated (Plan 108.4 Ф.3 sweep):

| Protocol | Старый метод | Новый метод |
|---|---|---|
| `Next[T]` (ex `Iterable[T]`, Plan 138) | `next() -> Option[T]` | `mut @next() -> Option[T]` |
| `Hash` | `hash() -> u64` | `@hash() -> u64` |
| `Equal` | `equals(other Self) -> bool` | `@equal(other Self) -> bool` |
| `Compare[T]` | `compare(other Self) -> int` | `@compare(other Self) -> int` |
| `Clone` | `clone() -> Self` | `@clone() -> Self` |
| `Display` | `fmt(sb StringBuilder)` | `@display(sb StringBuilder)` |
| `Debug` | `debug_fmt(sb StringBuilder)` | `@debug(sb StringBuilder)` |
| `Cleanup[E]` | `cleanup(...)` | `consume @cleanup(outcome ScopeOutcome) Fail[E] -> ()` |
| `WithExitTimeout` | `exit_timeout_ms() -> int` | `@exit_timeout_ms() -> int` |
| `Into[U]` | `into() -> U` | `@into() -> U` |
| `TryInto[U,E]` | `try_into() -> Result[U,E]` | `@try_into() -> Result[U, E]` |
| `Generator[T]` (testing) | `generate() -> T`, `shrink(...) -> Iter[T]` | `@generate() -> T`, `@shrink(...) -> Iter[T]` |

Bootstrap comment in `std/prelude/collections.nv` (explaining why `@` wasn't used)
has been removed — parser now fully supports `@`-prefix.

### Связь

- [D58 amend](03-syntax.md#d58-range-литерал-itert-protocol-for-x-in-c-implicit-iter) —
  `Next[T]` signature → `mut @next()` (explicit receiver); `Iterable[T]` удалён.
- [D72 amend](02-types.md#d72-generic-bounds-через-t-protocol--protocol-как-тип) —
  `[T Next[U]]` bound: mut consistency check at use-site.
- [D186 amend](02-types.md#d186--impip1--p2---opt-in-annotation-для-protocols) —
  `#impl(P)` annotation now checks receiver_mut in addition to method signature.
- Plan 108.1/108.2/108.3 — consistency story (default-ro everywhere).

---

## D295 (AMENDED V2) — `DnsNet` effect — async DNS resolution (Plan 91.12 Ф.9 + Plan 91.13, 2026-06-16)

> ⚠ **RECONCILE-PENDING (owner-decision 2026-07-03):** `TcpNet`/`UdpNet`/`DnsNet` — дробление, отклоняющееся от канона D62 (ОДИН `Net`). Принято решение **консолидировать обратно в единый `Net`**; миграция кода едет с net byte-surface sweep Plan 178 §13.2 (`[M-net-merge-to-single-effect]`). До миграции этот D-блок описывает transitional split; после — амендится на `Net`. AddrNet ретрактируется в pure независимо (Plan 178 §13.2).

**Source:** Plan 91.12 Ф.9, 2026-06-16. **Amended:** Plan 91.13, 2026-06-16. **Status:** ✅ ACTIVE (V2).
**Связь:** [D365](04-effects.md#d365), [D364](02-types.md#d364), [D294](08-runtime.md#d294), [Plan 91.12](../../docs/plans/91.12-net-effect-and-hardening.md), [Plan 91.13](../../docs/plans/91.13-dns-multi-address.md).

### Мотивация

`TcpNet.connect` и `UdpSocket` принимают `SocketAddr` — числовой IP-адрес. Для подключения
по имени хоста (`"example.com"`) необходима DNS-резолюция. В runtime она асинхронна
(`uv_getaddrinfo` через libuv callback); она должна паркировать fiber, а не блокировать поток.

### Декларация (V2)

```nova
// std/net/effect.nv
#stable(since = "0.1")
export type DnsNet effect {
    lookup(host str, port u16) -> Result[[]SocketAddr, NetError]
}
```

### Публичный API (V2)

```nova
// std/net/dns.nv
#stable(since = "0.1")
export fn SocketAddr.lookup(host str, port u16) DnsNet -> Result[[]SocketAddr, NetError] {
    DnsNet.lookup(host, port)
}
```

`SocketAddr.lookup` является основным публичным входом. Прямой вызов `DnsNet.lookup` через
vtable также работает в V2: исправление `is_generic_stub_c` в `emit_c.rs` (Plan 91.13) устранило
ошибку классификации монорфизованных generic-инстансов как stubs, что приводило к erasure
`Ok`-типа до `nova_int`. Подробнее — раздел «Codegen fix».

### Реализации (V2)

| Функция | Описание |
|---|---|
| `real_dns_net()` | Конкретный handler: `dns_lookup(host.as_ptr(), host.byte_len(), port)` → `uv_getaddrinfo` → fiber park → resume → строит `[]SocketAddr` через `dns_addr_at(i)` для `i in 0..count` |
| `mock_dns_net()` | Mock handler: всегда `Ok([SocketAddr._from_raw(socket_addr_loopback(0))])` (Vec, один элемент) |

### Семантика V2

- Возвращает **все** разрешённые адреса (`[]SocketAddr`, ≥1 элемент при успехе).
- Запрашивает OS resolver → блокирующий вызов внутри libuv thread pool.
- Паркует вызывающий fiber; другие fiber'ы продолжают выполнение.
- Вызов без `DnsNet` effect в области видимости — compile error.
- `addrs[0]` — первый (предпочтительный) адрес; `addrs.len()` — полное число результатов.

### C runtime (`compiler-codegen/nova_rt/net.c`)

```c
typedef struct {
    nova_coro*  fiber;
    nova_int    count;    // число результатов; <0 = ошибка
    void*       addrs[8]; // first 8 resolved addresses (TLS)
} NovaDnsReq;

static __thread void* _net_dns_addrs[8];
static __thread int   _net_dns_count;

static void _dns_getaddrinfo_cb(uv_getaddrinfo_t* req, int status, ...);

nova_int dns_lookup(const uint8_t* host, nova_int host_len, uint16_t port);
nova_int dns_addr_at(nova_int i);
```

`dns_lookup` возвращает `count` (число адресов, <0 = ошибка); адреса доступны через
`dns_addr_at(i)` для `i in 0..count`. Nova-side `real_dns_net()` строит `Vec[SocketAddr]`
в цикле, вызывая `dns_addr_at(i)` для каждого индекса.

### Codegen fix (Plan 91.13)

**Проблема (V1):** `is_generic_stub_c` в `emit_c.rs` классифицировал монорфизованные
generic-инстансы (например `Nova_Vec____NovaValue_SocketAddr*`) как unresolved stubs —
отсутствовала проверка `!name.contains("____")`. Это приводило к эrasure `Ok`-типа до
`nova_int` в Result-арме vtable, делая `Result[[]SocketAddr, NetError]` недостижимым.

**Fix:** добавлена проверка `&& !name.contains("____")` в `is_generic_stub_c`
(`compiler-codegen/src/codegen/emit_c.rs`). Аналогичный guard уже применялся в
Vec-array-арме (line 5850) и Option-арме. Result-арм был единственным пропуском.

### Тесты

- `nova_tests/plan91_12/net_v2_dns_smoke.nv` — 6 тестов (4 pos + 2 neg), все PASS.
  - Pos: mock_dns_net lookup → `Ok` + `addrs[0].is_v4()` + `port == 0` + multi-call + `addrs.len() >= 1`.
  - Neg: custom fail-mock → `Err(ConnectionRefused)` / `Err(NotFound)` preserved.
- `nova_tests/plan91_12/net_v2_dns_real_slow.nv` — opt-in real DNS test (`_slow` suffix, `NOVA_SLOW_TESTS=1`).
  - `assert(r.is_ok())` с реальным `localhost` resolver.

### Маркеры (закрыты Plan 91.13)

| Маркер | Статус |
|---|---|
| [M-91.13-dns-iter-boxing] | ✅ CLOSED 2026-06-16 — is_generic_stub_c fix + DnsNet V2 []SocketAddr |
| [M-91.13-real-dns-integration-test] | ✅ CLOSED 2026-06-16 — net_v2_dns_real_slow.nv (_slow, opt-in) |

## D377 — UDP Socket Split: `UdpSendHalf` + `UdpRecvHalf` (Plan 166, 2026-06-17)

**Source:** Plan 166, 2026-06-17. **Status:** ✅ ACTIVE.
**Связь:** [D365](04-effects.md#d365), [D364](02-types.md#d364), [Plan 91.12](../../docs/plans/91.12-net-effect-and-hardening.md), [Plan 166](../../docs/plans/plan166-udp-split.md).

### Мотивация

`UdpSocket` требовал все операции в одном файбере — `send_to` и `recv_from`
делили одни поля `recv_scope`/`recv_slot`. Это исключало паттерн «сервер-loop»:
один файбер принимает датаграммы, другой одновременно отправляет ответы.

Кроме того, в `send_to` существовал TOCTOU-баг: `recv_scope` выставлялся
ПОСЛЕ вызова `uv_udp_send`. На Windows loopback callback'может сработать
синхронно до выставления `recv_scope` → файбер паркуется без пробуждения (TIMEOUT).

### Часть 1: TOCTOU-фикс (`send_to`)

- Добавлены отдельные поля `send_scope` + `send_slot` в `NovaRt_UdpSocket`
  (параллельно `recv_scope`/`recv_slot`)
- `send_to` выставляет `send_scope`/`send_slot` **до** вызова `uv_udp_send`
- Добавлен `_udp_send_stop_cb` для корректной поддержки cancellation
- `_udp_send_cb` использует `send_scope`/`send_slot` (не `recv_scope`/`recv_slot`)

### Часть 2: UDP Socket Split

```nova
export type UdpSendHalf consume value { priv handle CUdpSocket }
export type UdpRecvHalf consume value { priv handle CUdpSocket }

export fn UdpSocket consume @split() -> (UdpSendHalf, UdpRecvHalf)

// UdpSendHalf: только операции отправки
export fn UdpSendHalf mut @send_to(data str, addr SocketAddr) UdpNet Blocking -> Result[int, NetError]
export fn UdpSendHalf consume @close() UdpNet -> ()

// UdpRecvHalf: только операции приёма
export fn UdpRecvHalf mut @recv_from(max int) UdpNet Blocking -> Result[(str, SocketAddr), NetError]
export fn UdpRecvHalf mut @local_port() UdpNet -> u16
export fn UdpRecvHalf mut @local_addr() UdpNet -> SocketAddr
export fn UdpRecvHalf consume @close() UdpNet -> ()
```

### Контракт конкурентности

- `UdpSendHalf` использует `send_scope`/`send_slot` — безопасно с concurrent recv
- `UdpRecvHalf` использует `recv_scope`/`recv_slot` — безопасно с concurrent send
- Один файбер на half — внутри каждого half операции последовательны
- Два файбера могут одновременно использовать send_half и recv_half

### Семантика владения / close

- `UdpSocket.split()` потребляет сокет и возвращает два consume-значения
- Оба half ДОЛЖНЫ быть закрыты (enforced через consume type system компилятора)
- Close использует atomic refcount: последний close фактически закрывает OS-сокет
- `UdpSocket` без split: refcount=1, `close()` работает как прежде

### Три новых операции `UdpNet` effect

```nova
split_socket(handle CUdpSocket) -> (CUdpSocket, CUdpSocket)
close_send_half(handle CUdpSocket) -> ()
close_recv_half(handle CUdpSocket) -> ()
```

### Негативные случаи (проверяются компилятором)

- Использование `UdpSocket` после `split()` → consume violation (moved value)
- `UdpSendHalf.recv_from` → type error (нет такого метода)
- `UdpRecvHalf.send_to` → type error (нет такого метода)
- Незакрытый half → consume violation (consume value must be used)

## D301 — TCP Stream Split: `TcpReadHalf` + `TcpWriteHalf` (Plan 91.16, 2026-06-17)

**Source:** Plan 91.16, 2026-06-17. **Status:** ✅ ACTIVE.
**Связь:** [D365](04-effects.md#d365), [D364](02-types.md#d364), [D377](04-effects.md#d377), [Plan 91.12](../../docs/plans/91.12-net-effect-and-hardening.md).

### Мотивация

`TcpStream` делил единственную пару `op_scope`/`op_slot` между `connect`,
`read` и `write`. Это исключало паттерн полнодуплексного соединения:
один файбер читает входящий поток, другой одновременно пишет ответы на том
же соединении. При попытке конкурентного read+write на одной паре slot'ов
park-bookkeeping одной операции затирался другой (TOCTOU, тот же класс бага,
что в D377 для UDP `send_to`).

Это TCP-аналог UDP split из [D377](04-effects.md#d377): я делю `TcpStream` на
read- и write-половины с НЕЗАВИСИМЫМИ C-side park-слотами.

### API

```nova
export type TcpReadHalf  consume value { priv handle CTcpStream }
export type TcpWriteHalf consume value { priv handle CTcpStream }

export fn TcpStream consume @split() TcpNet -> (TcpReadHalf, TcpWriteHalf)

// Дополнительно: write_all на самом TcpStream (loop до полной записи).
// NB: эффект `Blocking` отозван (Plan 91.15 P0, D172 завершение) — все
// сетевые операции несут только `TcpNet`; suspend/offload — внутри handler'а.
export fn TcpStream mut @write_all(data str) TcpNet -> Result[(), NetError]

// TcpReadHalf: только чтение + интроспекция адресов.
export fn TcpReadHalf mut @read(max int) TcpNet -> Result[str, NetError]
export fn TcpReadHalf @local_port() TcpNet -> u16
export fn TcpReadHalf @peer_port() TcpNet -> u16
export fn TcpReadHalf @local_addr() TcpNet -> SocketAddr
export fn TcpReadHalf @peer_addr() TcpNet -> SocketAddr
export fn TcpReadHalf consume @close() TcpNet -> ()

// TcpWriteHalf: только запись + интроспекция адресов.
export fn TcpWriteHalf mut @write(data str) TcpNet -> Result[int, NetError]
export fn TcpWriteHalf mut @write_all(data str) TcpNet -> Result[(), NetError]
export fn TcpWriteHalf @local_port() TcpNet -> u16
export fn TcpWriteHalf @peer_port() TcpNet -> u16
export fn TcpWriteHalf @local_addr() TcpNet -> SocketAddr
export fn TcpWriteHalf @peer_addr() TcpNet -> SocketAddr
export fn TcpWriteHalf consume @close() TcpNet -> ()
```

### Контракт конкурентности

- `TcpReadHalf` паркуется на `read_scope`/`read_slot` — безопасно при concurrent write.
- `TcpWriteHalf` паркуется на `write_scope`/`write_slot` — безопасно при concurrent read.
- Один файбер на half — внутри каждого half операции последовательны.
- Два файбера могут одновременно использовать read_half и write_half на одном соединении.
- `connect`-эра пары `op_scope`/`op_slot` после split не используется (connect уже завершён).

### Семантика владения / close

- `TcpStream.split()` потребляет поток и возвращает два consume-значения,
  оба несущие ОДИН и тот же C-handle (`NovaRt_TcpStream*`).
- Оба half ДОЛЖНЫ быть закрыты (enforced через consume type system).
- Close использует atomic refcount (`split_refcount`): `split()` ставит refcount=2,
  каждый `close()` делает `__atomic_sub_fetch`; `uv_close` фактически выполняется
  только когда последний half закрывается (refcount → 0).
- `TcpStream` без split: `split_refcount=0`, `close()` работает как прежде
  (отдельный путь через `NovaRt_TcpStream_method_close`).

### Операции `TcpNet` effect (новые)

```nova
write_all(stream TcpStream, data str) -> Result[(), NetError]
split_stream(stream TcpStream) -> (TcpReadHalf, TcpWriteHalf)
read_half_read(half TcpReadHalf, max int) -> Result[str, NetError]
read_half_close(half TcpReadHalf) -> ()
read_half_local_port / read_half_peer_port -> u16
read_half_local_addr / read_half_peer_addr -> SocketAddr
write_half_write(half TcpWriteHalf, data str) -> Result[int, NetError]
write_half_write_all(half TcpWriteHalf, data str) -> Result[(), NetError]
write_half_close(half TcpWriteHalf) -> ()
write_half_local_port / write_half_peer_port -> u16
write_half_local_addr / write_half_peer_addr -> SocketAddr
```

### write_all семантика

`write_all` гарантирует запись ВСЕХ байт (в отличие от `write`, который может
вернуть после частичной записи). На C-уровне libuv `uv_write` ставит в очередь
весь буфер целиком, поэтому одиночный вызов либо пишет всё, либо ошибка —
`tcp_stream_write_all` / `tcp_write_half_write_all` делегируют единичной записи.

### Негативные случаи

- Использование `TcpStream` после `split()` → consume violation (требует `consume`-binding
  на исходном потоке; покрыто `tcp_split_stream_after_split_neg.nv`).
- `TcpReadHalf.write` / `TcpWriteHalf.read` → type error (нет такого метода).
- **Ограничение V1:** consume-tracking не пробрасывается через tuple-destructuring
  (`mut (rd, wr) = s.split()`): парсер не принимает `consume (rd, wr) = ...`, а
  `mut`-bound значения не отслеживаются на double-consume. Поэтому double-close
  одной из половин НЕ ловится компилятором в V1 (refcount защищает на runtime).
  Followup-маркер: [M-91.16-tuple-consume-binding].

## D302 — `NetError.Eof`, `NetError @to_str()`, `SocketAddr @ip()` rename (Plan 91.15 P1, 2026-06-17)

**Source:** Plan 91.15 Phase P1, 2026-06-17. **Status:** ✅ ACTIVE.
**Связь:** [D301](04-effects.md#d301), [Plan 91.12](../../docs/plans/91.12-net-effect-and-hardening.md).

### Мотивация

Три отдельных огреха публичного API `std/net`, которые я закрываю одним блоком:

1. **EOF возвращался как `Ok("")`.** Когда peer закрывал соединение, `read`
   возвращал `Ok("")` — неотличимо от (теоретически возможного) пустого чтения и
   требовало от каждого вызывающего проверять `data.len() == 0`. Закрытие
   соединения — это событие, а не данные; ему место в `Err`-ветке.
2. **`NetError` нельзя было напечатать.** `IoError(str)`/`InvalidAddr(str)` несут
   строку, но достать её без exhaustive `match` нельзя — ошибка, которую нельзя
   залогировать, бесполезна.
3. **`@host_str()` — нестандартное имя.** Суффикс `_str` протекает тип в имя
   метода; ни один язык так не называет (Rust: `.ip()`).

### Изменения

**(1) `NetError.Eof`.** Новый вариант enum (после `NotFound`). `TcpStream.read` /
`TcpReadHalf.read` теперь возвращают `Err(NetError.Eof)` при закрытии соединения
peer'ом; `Ok(data)` всегда непуст.

C-контракт: `tcp_stream_read_bytes` / `tcp_read_half_read` возвращают сентинел
`NOVA_NET_READ_EOF` (`-2`) на EOF (раньше `0`). Nova-handler мапит `-2` →
`Err(NetError.Eof)`, `-1` → generic error, `>= 0` → `Ok(data)`. Константы
`NOVA_NET_READ_ERR`/`NOVA_NET_READ_EOF` в `net.h`.

**(2) `NetError @to_str() -> str`.** Метод (без эффектов) даёт lowercase
human-readable описание каждого варианта. `IoError(msg)` → `msg`;
`InvalidAddr(msg)` → `"invalid address: ${msg}"`.

**(3) `SocketAddr @host_str()` → `@ip()`.** Полное переименование: метод в
`addr.nv`, операция `AddrNet.ip`, extern `socket_addr_ip` (C-символ
`socket_addr_host_str` → `socket_addr_ip` в `net.c`/`net.h`). Внутренний
`NovaRt_SocketAddr_method_host_str` оставлен (не literal entry point).

### Совместимость

Breaking для пользователей `host_str()` и для кода, полагавшегося на `Ok("")` как
EOF-сигнал. `std/net` ещё `#stable(since = "0.1")`, но не зарелижен — миграция в
рамках pre-release окна.

### Дополнение — `PermissionDenied` / `ConnectionReset` (Plan 91.15 P2, 2026-06-17)

Добавил два типизированных варианта `NetError`, чтобы две распространённые
OS-ошибки больше не сваливались в `IoError(str)`/`BrokenPipe`:

- **`PermissionDenied`** — OS отказала в операции (`UV_EACCES`), например bind
  привилегированного порта без прав. `to_str() == "permission denied"`.
- **`ConnectionReset`** — peer форсированно сбросил соединение (`UV_ECONNRESET`).
  `to_str() == "connection reset by peer"`. Раньше эта ошибка классифицировалась
  как `BrokenPipe`; теперь это отдельный вариант (BrokenPipe остаётся для
  «запись в закрытый peer» без RST).

**C-контракт.** Net-ошибки доходят до Nova-слоя строкой (вывод `uv_strerror`) и
классифицируются в `std/net/tcp.nv net_error()`. Для этих двух кодов рантайм
(`_nova_net_uv_err` в `net.c`) нормализует сообщение к фиксированной канонической
строке (`NOVA_NET_MSG_PERMISSION_DENIED` / `NOVA_NET_MSG_CONNECTION_RESET` в
`net.h`), поэтому строковый матч в Nova платформо-стабилен. Прочие коды проходят
через `uv_strerror` без изменений.

**Effect-naming.** Зафиксировал соглашение об именах операций effect-семейства в
`std/net/effect.nv` (per-handle префиксы `listener_*`/`stream_*`/`socket_*`/
`read_half_*`/`write_half_*`); существующие имена НЕ переименованы (нулевой
user-visible эффект, высокий churn) — только задокументированы.

### Дополнение — `NetError` → `io.ErrorKind` проекция + `TcpStream` io.Read/io.Write (Plan 176 Ф.4, Q3, 2026-07-09)

**Q3-решение** (см. [Plan 176](../../docs/plans/176-io-fs-os.md) §3.0): один общий
`io.IoError{kind, raw_os, op}` для io/fs/os; `net` **не сливается** в него (`NetError`
остаётся отдельным типом со своими `#stable`-строками `@to_str()` — ИЗМЕНЕНИЙ здесь
НЕТ, выбран путь «сохранить строки» вместо «обновить все net-фикстуры под общий
`kind_to_str`», меньший дифф), но получает **аддитивную** best-effort проекцию:

- `NetError @to_error_kind() -> io.ErrorKind` — маппинг каждого варианта на ближайший
  `ErrorKind` (`ConnectionRefused`/`AddrInUse`/`AddrNotAvailable`/`NotFound`/`TimedOut`/
  `BrokenPipe`/`ConnectionReset`/`PermissionDenied` — прямые соответствия;
  `Eof`→`UnexpectedEof`; `Closed`→`NotConnected`; `Cancelled`→`Interrupted`;
  `IoError(_)`→`Other(0)`; `InvalidAddr(_)`/`InvalidPort`→`InvalidInput` — лоссово,
  текстовая деталь не переносится). `raw_os` результирующего `IoError` — всегда `0`
  (исходный uv-код уже потреблён `classify()` при постройке `NetError` и не
  восстановим).
- `NetError @to_io_error(op str) -> io.IoError` — тонкая обёртка (`IoError.of`).

**`TcpStream.@read`/`@write`** (`std/net/tcp.nv`) теперь возвращают
`Result[int, io.IoError]` (через эту проекцию) вместо `Result[int, NetError]` —
структурная конформность `io.Read`/`io.Write` (Plan 176 Ф.4(b), поверх byte-surface
D407 Ф.2-Ф.4). `@flush()` — no-op (TCP не буферизован на стороне Nova, тот же
контракт, что `File`, D322/D323). **Остальной `Net`-эффект не тронут**:
`write_all`/`read_bytes`/`read_text`/`write_str`, `TcpReadHalf`/`TcpWriteHalf`,

> **AMEND (2026-08-01, владелец):** `@read_to_vec` переименован в `@read_bytes` —
> симметрия content-пары с `@read_text` (прецедент имени: `ReadBuffer.@read_bytes`);
> свежие D-блоки (Plan 83.12, teardown-протокол D-раздела runtime) уже использовали имя `read_bytes` —
> std приведён в соответствие. Жёсткое переименование без алиаса (до релиза).
`UdpSocket`, `resolve` — все по-прежнему возвращают `NetError` напрямую.

**Координация 178:** `HttpError.ErrSource.Net(NetError)` (`std/http/error.nv`)
разгейчен — `HttpError.from_net(kind, e)` несёт типизированный `NetError` вместо
строки-плейсхолдера; `std/http/transport/real.nv` (dns/connect/write) использует его.
Предполагавшийся namespace-shadow (`NetError.InvalidPort` vs `ParseUrlError.InvalidPort`,
см. баннер `std/http/transport/real.nv`/`std/http/servernet/servernet.nv`) при
проверке не подтвердился — `ParseUrlError` с тех пор переименовал этот вариант в
`MalformedPort`; `std.http` компилируется с прямым `import std.net.{NetError}` без
коллизий.

Conformance: `spec_tests/conformance/d302_neterror_iokind.nv`.

---

## D407 — `std/net` переработка: один слой FFI, байтовый транспорт, zero-copy, M:N-безопасность (Plan 183, 2026-07-06)

**Source:** Plan 183 Ф.0, 2026-07-06. **Status:** ✅ ACTIVE — Ф.0-Ф.4 SHIPPED (2026-07-06):
`net2.c`/`net2.h` (Ф.1) + `std/net2` .nv-обвязка (Ф.2) + потребители (`std/http`, тесты,
`examples/net/*`) мигрированы (Ф.3) + M:N-стресс/эхо-замер (Ф.4, amend ниже). **Остаток —
Ф.5-хвост, НЕ этот D-блок:** физическое удаление старого `std/net`/`net.c` + namespace-ренейм
`net2`→`net`, гейтовано на санацию `nova_tests` — `[M-183-old-net-removal-after-182]`
(`docs/plans/backlog-followups.md`); до этого старый слой живёт с `// DEPRECATED`-баннером.
**Амендит:** [D173](../decisions/08-runtime.md#d173-stdnet--async-tcpudp-socket-stdlib-via-libuv)
(байтовый транспорт вместо `str`), [D282](../decisions/08-runtime.md#d282-new--extern-nova-fn--extern-c-fn--двух-abi-синтаксис-для-ffi-plan-9112-ф-1)
(один слой FFI, без `NovaRt_*_method_*`), [D301](#d301)/[D302](#d302) (split без
дублирующего C-API; EOF/ошибки — коды). **Связь:** [D357](#d357) (Http-транспорт
поверх байтового `Net`), [D322](#d322)/[D323](#d323) (byte-surface соседи).

### Мотивация (три дефекта старого `net.c`)

- **Д1 двойная обёртка.** `net.c` имитировал манглинг методов Nova (`NovaRt_*_method_*`)
  + второй слой literal-name (`ffi.nv`). C-код зависел от деталей манглинга, которых
  знать не должен. Порядок подключения stdlib = `extern "C"` (образец fs/os).
- **Д2 M:N-небезопасность.** Результаты операций возвращались через 6 статических
  `__thread`-слотов (`_net_tcp_read_data`, `_net_recv_data`, `_net_recv_sender`,
  `_net_dns_addrs`, `_net_parse_result`, `_net_tls_last_error`). Волокна мигрируют
  между OS-потоками (work-stealing) → пишет в слот потока A, читает слот потока B →
  чужие/пустые данные. Тот же класс, что STALE-slot M:N-гонка; сюда же
  детерминированный сегфолт live-socket-теста.
- **Д3 `str` как носитель байтов.** Сеть возвращает произвольные байты; `str` — UTF-8.
  Носитель обязан быть `[]u8`; текст — явной конверсией с валидацией у пользователя.

### Решение

1. **Один слой FFI.** Публичные C-функции — `nova_net_*` с C-ABI-сигнатурами
   ([D282 rule 2](../decisions/08-runtime.md#d282): скаляры / указатель+длина /
   out-параметры / код-возврата). НИКАКИХ `nova_str`/`NovaRt_*_method_*` в транспорте.
   Nova-типы (`TcpStream`, `SocketAddr`, …) и вся логика — в `.nv` поверх `extern "C"`.
2. **Байтовый транспорт.** Транспортные опы: вход `(const uint8_t* buf, int64_t len)`,
   выход `(uint8_t* buf, int64_t cap) -> int64_t n`. Эффект `Net` — `[]u8`-сигнатуры.
   `str`-удобства (`read_text()` и т.п.) — пользовательские `.nv`-хелперы через
   `Result[str, Utf8Error]`, НЕ операции эффекта.
3. **Zero-copy (модель Go/Rust `read(buf)->n`).** read: `alloc_cb` отдаёт libuv срез
   буфера **вызывающего** (указатель+ёмкость сохранены в handle) — сеть пишет прямо
   в память Nova-буфера; `read_cb` даёт `n`. write: `uv_write` получает указатель
   прямо на `[]u8`; буфер жив на стеке волокна (консервативный GC его видит).
   В hot-path read/write/send/recv-payload: `malloc`/`memcpy`/`nova_alloc` данных = **0**.
4. **Без статических слотов.** Результат — значением: код-возврата (`int`/`int64`,
   `<0` = −UV-код) + out-параметры (`NovaNetAddr* sender`, `NovaNetAddr** dns_arr`).
   Текст ошибки строит Nova-сторона из кода (`nova_net_strerror(code, buf, cap)`).
   Инвариант: `grep -E "__thread|__declspec\(thread\)" net2.c` = **0**.
5. **`SocketAddr` = value-запись** (снимает `[M-net-socketaddr-value-record]`): адрес —
   данные (16 байт адреса + порт + вид семейства + паддинг = **20-байтный образ**
   `NovaNetAddr`, `std/net2/addr.nv ADDR_IMAGE_BYTES`), не handle. Убирает `_nova_alloc_addr`
   и `_net_recv_sender`-слот. Единственные **неизбежные** копии (как у Rust/Go, поимённо):
   (а) `sockaddr_storage`→`NovaNetAddr` при запросе адреса (accept/peer/local); (б) sender
   UDP recv; (в) `addrinfo`→GC-массив `NovaNetAddr` в DNS — **одним `getaddrinfo`-вызовом**
   (libuv уже держит весь список адресов в колбэке, C выделяет GC-массив точного `count` —
   нет повторного/угадывающего запроса, `nova_net_dns_lookup`). НЕ в hot-path payload.
6. **Split без дублирующего C-API** (упрощение D301). У stream-handle с рождения
   раздельные `read_scope`/`write_scope` park-слоты → read и write независимы (full-duplex)
   БЕЗ отдельного набора `tcp_read_half_*`/`tcp_write_half_*`. «Split» на Nova-стороне =
   раздать один handle двум half-значениям; close — по **refcount** (`nova_net_tcp_mark_split`
   ставит `split_refcount=2`, каждый `close()` декрементирует, реальный `uv_close` — на 0).
7. **Парковка/пробуждение/отмена — без изменений** (park/wake поверх libuv, `stop_cb` +
   `nova_loop_defer_close`): M:N-корректны (park-слот в scope, не в потоке). Дефект Д2 был
   в передаче результатов, не в парковке.

**AMEND (Plan 183 Ф.4, 2026-07-06 — loop-affinity контракт, найден M:N-стресс-тестом).**
Пункт 7 неполон: парковка корректна, но обнаружен **отдельный** M:N-инвариант, которому
она подчиняется. Причина исходного UDP-флейка (~1-2/10 TIMEOUT) — НЕ lost-wake и НЕ
потеря датаграммы (обе гипотезы проверены трейсом и отвергнуты), а **loop-affinity**: uv-handle
пришпилен к libuv-loop'у, на котором создан (`nova_current_loop()` в bind/connect/accept);
libuv-loop'ы не thread-safe, единственный cross-thread-safe вход — `uv_async_send`
(его использует `nova_loop_defer_close`). Под M:N (каждый worker — свой loop) uv-оп,
выданный на handle с loop'а ДРУГОГО потока (в т.ч. с main-loop `_evloop`, пока main крутит
его `uv_run(UV_RUN_ONCE)` в supervised-drain), — конкурентная cross-thread мутация loop'а:
req теряется, completion-callback не приходит, `park_until`-предикат никогда не истинен →
волокно виснет навсегда. **Контракт** (задокументирован в заголовке `net2.c`, «LOOP-AFFINITY
CONTRACT»): создавай handle ВНУТРИ волокна, которое им оперирует; все дальнейшие uv-опы на
этом handle — только с того же волокна/worker'а. TCP не проявлял флейк, т.к. `connect`/`accept`
создают stream на loop'е текущего worker'а естественно; UDP-тесты (`socket.bind()` в
управляющем волокне, `send_to`/`recv_from` в spawn-волокнах) нарушали контракт неявно.
После приведения тестов к контракту: 60/60 seq + 128/128 16-way-parallel (было ~1/40, ~1/96).
Остаточный узкий класс (work-stealing миграция волокна МЕЖДУ парковками → следующий оп с
чужого worker'а) и полный субстратный фикс (маршалинг issue-стороны каждого uv-опа на
owning-loop-thread через defer-op-очередь, обобщение `nova_loop_defer_close`) —
`[M-183-net2-loop-affinity-cross-thread-op]` (backlog, P2, НЕ регрессия — контракт достаточен
для всех текущих потребителей).

### Миграция

Фазная (план 183): Ф.1 новый `net2.c` рядом со старым; Ф.2 namespaced .nv-обвязка;
Ф.3 миграция потребителей (`std/http`, тесты, examples) + удаление `net.c`/`ffi.nv`/
`str`-опов/`NovaRt_*_method_*` атомарно. Breaking внутри pre-release-окна (`std/net`
ещё `#stable(since="0.1")`, не зарелижен). Критерии приёмки — план §4 (grep-инварианты
=0, live-socket M:N-smoke детерминированно зелёный, эхо-замер не хуже старого слоя).

**Ф.5-факт (2026-07-06):** Ф.1-Ф.4 SHIPPED как описано выше (мотивация/решение — не
изменились, реализация подтверждена построчной сверкой с `net2.c`/`std/net2/*.nv`); из
пяти критериев §4 плана 4 закрыты полностью, 5-й («один слой», п.1) закрыт **для нового
слоя** (`net2.c`: 0 `NovaRt_*_method_*`, 0 `__thread`) — старый `net.c` физически ещё
существует (потребители `nova_tests/plan83_12/91_12/91_15/91_16/plan178` не мигрированы,
намеренно, до Plan 182), поэтому global-grep по репозиторию пока не 0; это отслеживается
как `[M-183-old-net-removal-after-182]`, не как незакрытый критерий D407. Побочные
компиляторные дефекты, вскрытые в ходе реализации (НЕ дефекты этого D-блока, задокументированы
в `docs/plans/backlog-followups.md` под Plan-183-заголовками): GC-трассировка `Vec[value-record
с heap-полем]` сквозь vtable/generic-erasure, `Result[_, XError].unwrap()` на typed-error,
type-inferred `[]u8`-буфер теряющий `resize`, `nova build` ICE на consume-результате
effect-операции, same-module `to_str()`-коллизия на `int`-receiver'е.

---

## D325 — Единый fallible-контракт: публичный std возвращает `Result` (Plan 177, 2026-06-25)

**Source:** Plan 177, 2026-06-25 (после развилки A→B1→Вариант 1 + adversarial-критика).
**Status:** ✅ ACTIVE как нейминг-канон (sign-off владельца 2026-06-25). **Миграция завершена (Plan 177 ЗАКРЫТ 2026-07-04):** stable-std public-fallible = Result-everywhere (Ф.2a base64/json/complex, Ф.2b parse/read_buffer + де-хардкод, Ф.2c коллекторы `sequence`/`partition`); guard + conformance (41/0) зелёные. **Остаток честно маркирован** (Plan 177 §14): (a) `std/concurrency` `race2`/`with_timeout` throw bare-`str` = Plan 173-домен `[M-177-concurrency-throw-fallibility]`; (b) весь `std/_experimental` = defer до стабилизации `[M-177-experimental-fallible-migration]`; (c) codegen-хвост `[M-177-d77-codegen-4way-retract]` (D77 4-way→2-way emit_c) + `[M-172.1-opt-result-over-userenum-typedef-order]`.
**Amends:** D77 (08-runtime.md) — 4-way auto-derive → **2-way** (убрать bare-throws Fail-форму).
**Retracts:** D178 (08-runtime.md) — `str.parse_int` bare + `parse_int_opt`.
**Связь:** [D25](#d25) (Fail остаётся в языке), [D85](#d85) (`?`/`!!`), [D86](#d86) (`??`), D73 (From/Into), D77 (TryFrom), D178.
**Гигиена нумерации:** D316–D324 зарезервированы планами 175/175.1/176 → взят D325; gap отмечен в `spec/decisions/README.md`. (2026-07-04: D316 внесён — Plan 175 Ф.1, единый источник схемы `Time` + `TimerMetrics`-split; D317–D324 остаются reserved.)

### Что

Любая падающая **публичная** операция std возвращает `Result[T, <Domain>Error]`. Дуальный `bare`(throw)/`try_`(Result)/`_opt`(Option)-нейминг ретрактируется из std. Эффект `Fail[E]` остаётся механизмом языка — для пользовательского кода и внутренних хелперов; std им свои ошибки наружу не отдаёт.

### Правило

- **(R0) Граница panic vs Result ([D13](08-runtime.md#d13)).** «Падающая операция» в R1 = **expected/environmental failure** (пользовательский ввод, I/O, парсинг, ресурсы среды). **Contract-violation / programming error → panic** per D13, НЕ `Result` и НЕ `Fail`. Пары: `v[i]` OOB → panic / `v.get(i)` → `Option`; integer overflow, div/0 → panic; `s[a..b]` mid-codepoint → panic / `parse_int` → `Result`. Прецедент: Rust и Zig держат panic/unreachable **вне** error-канала. Без R0 R1 читалось бы как «Result вместо panic» — это не так.
- **(R1)** Любая падающая публичная операция std → `Result[T, <Domain>Error]`. Один структурный `XError` на домен. Нет bare-throws-близнецов, нет `try_`-дублей, нет `_opt`.
- **(R2)** Имя обычное, без префикса: `parse_int -> Result`, `read_u32 -> Result`, `open -> Result` (как Rust `str::parse`).
- **(R3)** Префикс `try_` — **только** чтобы отличить fallible-вариант одноимённого **infallible** (`from`/`try_from`, `into`/`try_into`). В одиночных fallible-операциях (нет infallible-сиблинга) префикса НЕТ.
- **(R4)** `Option` — только genuine absence (`find`/`get`/`env`/`parent`), НЕ fallibility. **Критерий-тест:** *>1 причины отказа ИЛИ вызывающему нужна причина → `Result`; единственный нормальный исход «нет» → `Option`.* `Result → Option` через `.ok()`. Никаких `_opt`-имён. Edge `env` (non-unicode путь на Windows → `Result[VarError]` vs документированная lossy-гарантия) — решает Plan 176.
- **(R5)** Эффект `Fail[E]` в публичной std-сигнатуре запрещён для **собственных** ошибок (→ `Result`), но разрешён для прозрачного **проброса** `Fail[E]` из closure-параметра (effect-polymorphic forwarding: `retry`/`parallel`/`in_transaction` над телом пользователя).

Эргономика throw на call-site сохранена операторами (D85): `expr!!` (throw), `expr?` (проброс), `expr.ok()` (→Option), `match` (ветвление).

### Почему

1. `Result` безопасен в 100% операций; bare-throws — нет (для must-consume `close` глотает ошибку → потеря данных, см. Plan 176).
2. Нет границы «I/O vs scalar» = нет вечного вопроса «а это куда?» (был первым же на snowflake).
3. Ошибка-как-значение фундаментальнее, чем как-throw: кладётся в `Vec`, мапится, собирается, шлётся в канал; брошенный `Fail` — control-flow.
4. Меньше имён на операцию (одно vs до трёх); меньше доков и путаницы «какой звать».
5. `!!` уже даёт throw там, где он нужен — реальная потеря лишь 2 символа на проброс в glue-скриптах.

### Что отвергнуто

- **A** (bare=throws everywhere) — close-footgun на must-consume.
- **B1** (две категории: I/O=`Result`, scalar=дуал + граница) — вечная граница + сложность для рядового разработчика. Концепция «двух категорий» удалена целиком.
- Удаление эффекта `Fail` из языка — НЕ делаем.

### Эталон

`std/net` — Result-everywhere, 0 `Fail[`. Под-паттерны: **fallible-итерация** `@next() -> Option[Result[Item, E]]` (Rust-модель: exhaustion снаружи как `Option`, ошибка элемента внутри как `Result`); absence → `Option`; инфаллибл-аксессор → значение.

### Amend-пакет (Ред. 2, Plan 177, 2026-07-03 · sign-off владельца)

- **Nesting-канон fallible-итерации:** `@next() -> Option[Result[T, E]]` — exhaustion (`None`) отделён от per-element ошибки (`Err`). Прежняя формулировка «`DirIter.next -> Result[Item, E]`» (первая ред.) **уточнена** — она сливала «поток кончился» и «элемент упал» в один канал.
- **Explicit exempt-list** (для conformance-guard §8.2 Plan 177 — иначе false-positive на легальных `Fail[`):
  1. `std/prelude/core.nv` — `extern Option@unwrap` / `Result@unwrap` с `Fail[...]` — это **сам D85-мост `!!`**, by-design.
  2. `std/prelude/protocols.nv` — protocol-member `@cleanup(...) Fail[E]` (Cleanup-протокол) — user-`E`, R5-forwarding.
  3. `std/testing/property.nv` — `assert_prop`/`assert_prop_msg`/`property`/`property_with` (4 сигнатуры с `Fail`) — **exempt** (sign-off 2026-07-03): assert/test-DSL-семантика («упади сейчас» = смысл assert'а); миграция в `Result` отвергнута (шум в тестах).
- **Коллекторы `[]Result`:** работа со списком результатов — `sequence: []Result[T,E] -> Result[[]T,E]` (fail-fast) и `partition: []Result[T,E] -> ([]T,[]E)` (prelude, Plan 177 Ф.2c; прецеденты: Rust `FromIterator for Result`, Go `errors.Join`).
- **Cross-domain композиция (trade-off):** авто-`From`-конверсия ошибок при `?` **отклонена** (D85 amend 174.2) → смешение доменов (`IoError` + `ParseIntError` в одной fn) требует `.map_err(...)` на сайте либо явный domain-sum-error. Обратное (обернуть Fail-код в Result) — идиома `with Fail[E] = |e| interrupt Err(e) { … }` (аналог Kotlin `runCatching` / Swift `Result(catching:)`).

### Amend — `str @to_bool()` / `str @to_char()` (Plan 232.1 Т1, owner decision «добавить», 2026-07-26)

Закрыты два ❓-пробела `spec/conversions.md` (str→bool, str→char), оставшиеся
после Plan 174.1 «конверсия — метод на источнике». Оба — R1/R2-конформны
(обычное имя `to_*`, без `try_`-префикса — infallible-сиблинга нет), домфайл
`std/src/runtime/string/parse.nv` (рядом с `to_int`/`to_f64`, тот же
`#no_prelude` модуль `runtime.string`):

- **`fn str @to_bool() -> Result[bool, ParseBoolError]`** — строго
  `"true"`/`"false"`, lowercase-only (Rust `str::parse::<bool>`-канон, БЕЗ
  case-insensitive/`"1"`/`"0"`-алиасов). `ParseBoolError enum Empty |
  Invalid` — тот же двухвариантный `Empty`/`Invalid`-паттерн, что уже
  `ParseFloatError` (D178 amend V2 соседствует, тот же `runtime.string`
  движок-файл).
- **`fn str @to_char() -> Result[char, ParseCharError]`** — ровно один
  Unicode codepoint (не байт); пусто → `Err(Empty)`, >1 codepoint →
  `Err(TooManyChars)`. **Новый** `ParseCharError enum Empty | TooManyChars`
  — НЕ переиспользует `CharFromError` (`std/runtime/char.nv`,
  `(cp int).to_char()`): тот домен — «int вне диапазона Unicode scalar
  value/surrogate», недостижим для str→char (байты str уже валидный UTF-8,
  R-UTF8), другая семья ошибок — тот же принцип «не переиспользовать
  нерелевантный error-тип», что уже `RangeError` vs `ParseIntError` (D430
  §Связь).

Тесты рядом (`std/src/runtime/string_test.nv`, module test-peer). Канон
`check std/src` не сдвинулся (142/27/1040 — новые `test`-блоки внутри уже
существующего файла, не новый файл).

## D316 — `Time`: плумбинг-эффект, единый источник схемы + `TimerMetrics`-split (Plan 175 Ф.1, 2026-07-04)

> **Амендмент (Plan 200 П12-хвост, 2026-07-21):** свободные обёртки `sleep(d Duration)` и
> `sleep_until(deadline Monotonic)` РЕТРАКТИРОВАНЫ (surface = методы, D9): канон —
> `d.sleep()` / `deadline.sleep_until()` (`Monotonic @sleep_until()`, monotonic.nv).
> Effect-op `Time.sleep(ms int)` — слой примитива, без изменений.

**Source:** Plan 175 (time-system-rework), Ф.1. **Amends:** [D11](#d11)/[D14](#d14)/[D62](#d62) (prelude `Time`-decl), [D124](#d124) (wall/monotonic-разделение).
**Status:** ✅ ACTIVE (Ф.1 — единый источник + split; **Ф.1b/Ф.3 SHIPPED 2026-07-04 — amend ниже**; **unit-rename side-task SHIPPED 2026-07-06 — единицы в именах опов, amend ниже, не путать с формальной Ф.4 (sleep-семантика/tolerance, остаётся TODO)**). Overflow-политика — **D317 ✅ SHIPPED (Ф.1c, 2026-07-06)**; monotonic non-regression — **D318 ✅ SHIPPED (Ф.1c, 2026-07-06)**. Typed **effect-ops** (`timestamp()->Timestamp` в схеме, mock на typed-record'ах) — **🚩 OWNER-GATED** (retire int-wire, Ф.2; см. amend).

**AMEND (Plan 175 Ф.1b/Ф.3, 2026-07-04 — option C: typed `.nv`-слой поверх НЕизменённого int-wire-эффекта):**
- `Duration`/`Timestamp`/`Monotonic` — теперь **`value`-records** (single-i64 `nanos`, stack, zero-GC). Static-конструкторы возвращают **по имени типа** (`-> Duration`), не `-> Self` (self_value-trap). `Monotonic.now()` — value-builtin (эффектонезависим → допустим в `realtime{}`).
- **User-facing typed surface** доставлен на `.nv`-обёртках, БЕЗ смены схемы эффекта: `Timestamp.now()` = `Timestamp.from_unix_millis(Time.now())` (int-wire ms → value Timestamp); `@is_past`/`@time_until`/`@elapsed` — **int-based** (`@nanos` vs `Timestamp.now().nanos`), теперь РАБОТАЮТ; value-record арифметика `@plus/@minus/@times/@div/@neg/@compare`/`==`. `wait_for(Duration)`/`close_after(Duration)`/`close_at(Monotonic)` — value-`Duration`/`Monotonic` пересекают C-границу by-address (extern) / `.nanos` (dispatch). Mock (`fixed_ms`/`mut_clock`) оперирует **int ms** (wire), не typed-record'ами.
- **Ф.2 (retire int-wire → typed effect-ops в схеме) остаётся OWNER-GATED:** `Time`-decl в prelude/effects.nv (ZERO-imports-на-примитивах) не может ссылаться на `Timestamp`/`Duration`; 85/96 файлов зовут bare-int `Time.sleep(N)`. Sign-off: «typed effect surface» (prelude⟷std.time coupling, 3 net-zero) vs «typed sugar над int-эффектом» (SHIPPED). `[M-time-now-schema-mismatch]` закрыт **частично** (user-surface typed; wire int).
**Связь:** [D25](#d25) (Fail — пред-регистрируемый эффект), [D64](#d64) (Time — suspend-эффект, запрещён в `realtime {}`), [feedback-maximize-nv-sourcing] §3 (типы/схемы из `.nv`), 172.1 U.1 (codegen читает декларацию — прецедент RuntimeError/MemOrdering sum-schema).
**Нумерация:** D316 из reserved-диапазона D316–D324 (README §gap; 175 = D316–318). Ф.1 занимает D316-slot механикой единого источника; typed-surface — amend этого же D316 в Ф.2.

### Что (Ф.1)

`Time` — **внутренний плумбинг-эффект** (как `TcpNet`/`AddrNet`): user-код ходит через type-методы (`Timestamp.now()` / `Monotonic.now()` / free `sleep`), не зовёт `Time.op()` напрямую. Схема эффекта имеет **ОДИН источник** — декларацию `type Time effect { … }` в `std/prelude/effects.nv`; codegen **читает** её оттуда (ветка RUNTIME_DEFINED_TYPES / `TypeDeclKind::Effect` в `emit_type_decl` строит `effect_schemas["Time"]` из методов), а не из хардкод-зеркала. 5 timer-observability-счётчиков **вынесены** из `Time` в отдельный read-only эффект `TimerMetrics`.

### Правило (Ф.1)

- **(R1)** Единый источник схемы `Time`: только `.nv`-декларация. Хардкод `effect_schemas.insert("Time", …)` в codegen удалён; закомментированное 5-е зеркало в `std/time/duration.nv` удалено. (Fail/Mem пока сохраняют pre-register — вне scope Ф.1.)
- **(R2)** Int-провод сохранён **без смены поведения** в Ф.1: `sleep(ms int) -> ()`, `now() -> int`, `now_monotonic() -> int` (wire raw i64; `Monotonic`-record оборачивается на Nova-стороне). Типизация опов — Ф.2/Ф.3 (amend).
- **(R3)** `TimerMetrics` (**NEW**) — read-only introspection timer-runtime: `timer_alloc_total`/`timer_alloc_active`/`timer_fired`/`timer_cancelled`/`timer_longest_pending_ms`, все `() -> int`. Дispatch — direct-C (`Nova_TimerMetrics_timer_*`, nova_rt/channels.h), без vtable, симметрично `Mem`. **НЕ** suspend-эффект → разрешён в `realtime {}` (в отличие от `Time`). Тест-handler'ам `Time` больше не нужно стабить 5 бессмысленных опов (Q1).
- **(R4)** ns — канонная единица storage+wire (уточняется в Ф.2/D317).

### Почему

1. Пять расходящихся зеркал одной схемы (prelude-decl / codegen-hardcode / C-vtable / handler-литералы / закомментированная decl) — правка требовала синхронного изменения 5 мест; единый `.nv`-источник убирает дрейф ([feedback-maximize-nv-sourcing] §3; прецедент RuntimeError 78 Ф.2, 172.1 U.1).
2. `TimerMetrics` — интроспекция timer-runtime (Plan 66 territory), не «время»: держать её в `Time` раздувало плумбинг-эффект и заставляло каждый mock-clock-handler стабить read-only счётчики (Q1).
3. Ф.1 — refactor без смены поведения (int-провод неизменен) → низший риск; типизация и overflow-безопасность идут отдельными фазами поверх стабильного единого источника.

**AMEND (owner decision, 2026-07-06 — единицы времени в именах операций; side-task вне формальной Ф-нумерации плана 175, отдельно одобрен владельцем):**
- `Time`-эффект переименован **без смены поведения провода**: `now()` → `now_unix_ms()`, `now_monotonic()` → `now_monotonic_ns()`. `sleep(ms int)` не тронут — единица уже в имени параметра.
- **Факт-единицы провода** (зафиксированы, не изменены этим amend'ом): `now_unix_ms()` — миллисекунды Unix-epoch (см. `Timestamp.from_unix_millis(Time.now_unix_ms())` в `std/time/duration.nv`); `now_monotonic_ns()` — наносекунды (`_nova_monotonic_ns()` в `nova_rt/fibers.h` оборачивает `uv_hrtime()` напрямую, без деления).
- **Сахар:** `Duration.@sleep()` (**NEW**, `std/time/duration.nv`) — `Time.sleep(@to_millis_ceil())`, округляет ВВЕРХ до целых миллисекунд (никогда не спит МЕНЬШЕ запрошенного; усечение ns→ms вниз недосыпало бы).
- **Почему:** голое `now()`/`now_monotonic()` не сообщает единицу на call-site — читатель должен помнить конвенцию или лезть в докблок. Имя операции = единственный источник правды на месте вызова (симметрично `sleep(ms int)`, где единица уже в параметре).
- Обновлены все вызовы в `std/` (schema-декларация, `std/testing/handlers.nv` mock-handler'ы, `std/time/duration.nv`, `std/concurrency/*`, `std/_experimental/concurrency/rate_limiter.nv`); codegen (`emit_c.rs`) схему НЕ хардкодит (читает из `.nv`, R1) → изменений в диспатч-логике не потребовалось, только докблок-комментарии.

**AMEND ([M-time-default-handler-not-wallclock], 2026-07-06 — боевой default-обработчик `now_unix_ms()` отдавал monotonic uptime вместо wall-clock):**
- **Дефект:** default (без `with Time = handler {...}`) обработчик `Time.now_unix_ms()` вызывал `_nova_time_default_now()` → `_nova_monotonic_ms()` (`uv_hrtime()`-based, epoch реализация-зависим, фактически uptime процесса), хотя факт-единица D316 деклараровала «unix epoch ms» (§ выше: `Timestamp.from_unix_millis(Time.now_unix_ms())`). Любой боевой код, читающий `Timestamp.now()` как настоящее календарное время (логи, TTL, сравнение с внешними timestamp'ами) без явного `with Time = handler`, получал ложный epoch.
- **Фикс:** новая `_nova_wall_unix_ms()` (`nova_rt/fibers.h`, рядом с `_nova_monotonic_ms`/`_nova_monotonic_ns`) — настоящий wall-clock через `uv_gettimeofday(uv_timeval64_t*)` (libuv, POSIX `gettimeofday`-эквивалент на всех платформах); `_nova_time_default_now()` переключён на неё. `Nova_Time_now_unix_ms`/`Nova_Time_now_ms`/`Nova_Time_now_ns` (default-путь, без handler'а) получают исправление автоматически — все три делегируют к `_nova_time_default_now()`.
- **НЕ затронуто:** `now_monotonic_ns()` (`_nova_monotonic_ns()`/`uv_hrtime()` — монотоника,D124/D318 non-regression), mock-обработчики `fixed_ms`/`mut_clock` (`std/testing/handlers.nv` — подменяют весь vtable, свой `now_unix_ms`-слот).
- **Тест-детектор:** `std/time/units_test.nv` — `Timestamp.now()` без `with`-обработчика > `1_700_000_000_000` мс (после 2023-11-14; monotonic uptime короткого теста — единицы-десятки секунд, на порядки меньше).
- **Нумерация:** amend того же D316 (боевой default-handler wall-clock ops — та же секция D316, что и unit-rename amend выше).

**AMEND (Plan 175 Ф.3(a-d)/Ф.5(d), 2026-07-10 — Monotonic.now() builtin→`.nv`-сахар (мокабелен), free `sleep`/`sleep_until`, `@minus(Monotonic)`, `@display`/`@debug`, elapsed-measurement→Monotonic; Ф.2 int-wire retirement — ОСТАЁТСЯ OWNER-GATED, но с конкретной новой находкой):**

- **(Ф.3a) `Monotonic.now()` builtin RETIRED.** Все 4 dispatch/inference-сайта в `emit_c.rs` (Member ×2 / Path ×2 — реальные символы на момент фикса: `emit_call` Member/Path :~31076/:~34026, `infer_expr_c_type` Member/Path :~45627/:~46311; НЕ `nova_monotonic_now_record`/`"Nova_Monotonic*"` из старого снимка плана — emit_c.rs дрейфанул под 172.1.2, пере-grep подтвердил актуальные имена) удалены; заменены обычной `.nv`-функцией `fn Monotonic.now() Time -> Monotonic => { nanos: Time.now_monotonic_ns() as i64 }` (`std/time/duration.nv`), тем же паттерном, что `Timestamp.now()`. **Реальный недостающий кусок был в C runtime, не в архитектуре prelude/std.time** (три предыдущих net-zero захода искали блокер не там): `NovaVtable_Time` (`nova_rt/effects.h`) не имел слота под `now_monotonic_ns` — `Nova_Time_now_monotonic_ns()` (`nova_rt/fibers.h`) безусловно читал real-clock, игнорируя handler. Добавлен vtable-слот + NULL-safe dispatch (handler без слота прозрачно падает на real-clock — backward-compat со старыми handler-литералами). `std/testing/handlers.nv` `fixed_ms`/`mut_clock` реализуют `now_monotonic_ns()` когерентно с `now_unix_ms()` (mock-coherence, Ред.2 Q14 / Swift TestClock-паритет — ОДИН handler двигает оба чтения). **Закрывает `[M-monotonic-mock-support]`.**
- **(Ф.3b) Free `sleep(d Duration) Time`/`sleep_until(deadline Monotonic) Time`** (`std/time/duration.nv`) — канонический способ вызвать suspend-sleep (Q6/Q8, юзер не трогает `Time` напрямую). `sleep_until` — MVP-обёртка `sleep(deadline.elapsed_since(Monotonic.now()))`: прошлый дедлайн saturate-to-zero (D318) → немедленно, без re-arm timer (→ Plan 66).
- **(Ф.3c) `Monotonic @minus(other Monotonic) -> Duration`** overload (alias `elapsed_since`) — симметрия с `Timestamp @minus(Timestamp)`; `m2 - m1` dispatch'ится сюда, не в `@minus(Duration)` (point-shift). Существующий `@elapsed_since` СОХРАНЁН (не удалён — обе формы валидны, `@minus` эргономичнее для operator-стиля).
- **(Ф.3d) `@display`/`@debug` (D237-amend ниже) на всех трёх типах.** Побочный codegen-фикс, найденный и починенный ЭТОЙ ЖЕ волной (§4а): `"${d}"`/`"${d:?}"`-интерполяция для `value`-records (D226) была сломана в двух местах `emit_c.rs::emit_interpolated_str` (+ `str.from` Path-call) — (1) `debt_strip_nova_trim_start_no_ws` снимал только префикс `Nova_`, не `NovaValue_` → lookup пользовательского `@display`/`@debug` мисс → молчаливый fallback в `nova_int_to_str((nova_int)v)` (struct→int cast, CC-FAIL); заменено на существующий `debt_strip_value_prefix_or_nova_trim_start`. (2) диспатч передавал receiver BY VALUE, а value-record method ABI (D226/A6) ожидает pointer-receiver — использован существующий `prepare_method_recv`. До Ф.3d ни один value-record не реализовывал `@display`/`@debug`, поэтому баг не проявлялся; не time-специфичный фикс.
- **(Ф.5d) `measure[T]` мигрирован на `Monotonic.now()`** (был `Timestamp.now()`) — elapsed-measurement (стопвотч/бенчмарк) иммунен к wall-clock skew (NTP/DST во время измерения раньше мог дать отрицательный/вздутый `elapsed`). Сигнатура не меняется (`Duration` clock-agnostic). `deadline_in(Duration) -> Timestamp` НАМЕРЕННО НЕ мигрирован (return type committed к wall-clock, D124) — канон для монотонных дедлайнов = `Monotonic.now() + d` напрямую (коорд. 173 §3a `supervised(timeout:)`). `is_past`/`time_until`/`@elapsed` на `Timestamp` корректно ОСТАЮТСЯ wall-clock (сравнение self к `Timestamp.now()`, тот же домен — миграция на Monotonic была бы D124-нарушением, старый Ф.5.d line-list из авторинга плана 175 §6 в этой части устарел).

**AMEND (Ф.2 — retire int-wire — ЭМПИРИЧЕСКАЯ находка 2026-07-10, ЧЕТВЁРТЫЙ заход, net-zero, откачен чисто):** предыдущие 3 захода искали блокер в prelude⟷std.time coupling; этот заход **решил** ту часть (перенос `Time`-decl в `std/time/duration.nv`, рядом с `Timestamp`/`Duration`/`Monotonic` — типизированная схема РЕЗОЛВИТСЯ, cross-import `duration.nv⟷testing.handlers` НЕ образует блокирующий цикл на практике) — но упёрся в **НОВЫЙ, глубже архитектурный барьер**: mock-handler (`with Time = effect Time { monotonic() => ... }`) обязан СКОНСТРУИРОВАТЬ typed `Monotonic`-значение внутри handler-тела, но (a) `Monotonic` НАМЕРЕННО opaque — без публичного `from_*` (Q-decision, Rust `Instant`-паритет, защита от фабрикации фейковых монотонных моментов юзер-кодом); (b) codegen handler-literal body не поддерживает anonymous record-literal (`{ nanos: ... }`) — `codegen error: anonymous record literal without spread not supported in codegen` (новый, ранее не задокументированный codegen-гэп, не в списке эмпирик 3-го захода). Ни одно public-API решение не существует без ИЛИ (i) exposing внутреннего `Monotonic`-конструктора юзабельного из `std.testing` (подрывает opacity-контракт — тот же конструктор виден и обычному юзер-коду), ИЛИ (ii) компиляторной работы над anon-record-literal-в-handler-body (реальная codegen-инженерия, не косметика). **Это ОБЪЕКТИВНО обосновывает, почему уже отгруженный `option C` (int-wire эффект + typed `.nv`-сахар, Ф.1b/Ф.3/Ф.3a) — корректная итоговая архитектура, а не временный обход**: сахар оборачивает int→typed СНАРУЖИ handler-тела (в `Monotonic.now()`'s собственном теле, в родном модуле типа), а не внутри mock-handler'а, поэтому opacity-контракт и codegen-ограничение не конфликтуют. **Рекомендация:** закрыть Ф.2 (typed-wire-в-схеме) как SUPERSEDED предпочтением option C; `[M-time-now-schema-mismatch]` остаётся закрыт **частично по конструкции** (user-surface полностью typed + мокабелен; wire — int, и это теперь обоснованная, не временная, архитектура). Если owner всё же желает typed-wire — предпосылка: сначала либо (i), либо (ii) выше, ОТДЕЛЬНЫМ sign-off.

**AMEND (2026-07-10, sign-off владельца): «подмена источника» ≠ «фабрикация значения» — нормативное разграничение.**
Вопрос владельца «now_monotonic_ns() в эффекте = возможность мокнуть Monotonic где угодно — не дыра ли в непрозрачности?» разрешён так:

- **Подмена источника — ЛЕГАЛЬНА и желанна.** Хендлер `Time` управляет тем, *что показывают часы* (числом на проводе), лексически-скоупно и видимо в коде (`with Time = …`, философия D11/D61). Внутри with-скоупа ВСЕ чтения (wall + monotonic) идут от одного подменённого источника — одна согласованная шкала, elapsed-арифметика осмысленна (mock-coherence Q14). Эталон-паритет: Rust `tokio::time::pause()` подменяет часы целиком, при этом `Instant` из числа собрать нельзя.
- **Фабрикация значения — ЗАПРЕЩЕНА.** Публичного `Monotonic.from_*` нет и не будет (Q13): значение «из ниоткуда» (десериализация, литерал, число из чужого процесса) смешивает временные шкалы невидимо для читателя кода. Оп эффекта возвращает **число, не `Monotonic`**; единственная точка заворачивания числа в тип — `Monotonic.now()` в родном модуле (владелец: «так и должно быть», 2026-07-10).
- Следствие: перевод `Monotonic.now()` на прямой `extern "C"` (мимо эффекта) ОТВЕРГНУТ — убил бы мокабельность (анти-модель Zig из таблицы 7 языков) и нарушил бы module-conventions §0 (импурность — за эффектом).

**AMEND (Plan 175.1, 2026-07-10 — `local_offset_sec()` эффект-оп, closes [M-175.1-local-offset-effect-op]):** owner decision: системный часовой пояс машины ДОЛЖЕН быть доступен из Nova — постоянный обход (D321 §impl-отступления) закрыт этой волной, не остаётся follow-up'ом.
- **Новый оп `local_offset_sec() -> int`** добавлен в схему `Time` (`std/prelude/effects.nv`) — системный UTC-сдвиг ТЕКУЩЕЙ локальной зоны машины в секундах, DST уже учтён (сдвиг, который наблюдал бы свежий `Timestamp.now()` ПРЯМО СЕЙЧАС).
- **Vtable-слот:** `NovaVtable_Time.local_offset_sec` (`nova_rt/effects.h`) — тот же NULL-safe handler-extension pattern, что `now_monotonic_ns` (amend Ф.3a выше): handler-литералы без явного `local_offset_sec() => ...` оставляют слот NULL (C99 designated-init zero-fill) и прозрачно падают на реальный OS-хук — backward-compat, никакой миграции существующих handler-литералов не требуется.
- **OS-хук `_nova_local_offset_sec()`** (`nova_rt/fibers.h`, рядом с `_nova_wall_unix_ms`) — Windows `GetTimeZoneInformation` (Bias/DaylightBias/StandardBias → секунды, знак инвертирован: `Bias` — минуты ДОБАВИТЬ к local чтобы получить UTC); POSIX `localtime_r` + `tm_gmtoff` (BSD/glibc/macOS-libc extension — секунды к востоку от UTC, DST уже свёрнут).
- **`std/testing/handlers.nv`:** `fixed_ms`/`mut_clock` реализуют `local_offset_sec() => 0` (mock-coherence — фиксированные часы = фиксированная UTC-зона, детерминизм важнее реалистичности; симметрично `now_monotonic_ns`-coherence).
- **Nova-сторона:** `Offset.local() Time -> Offset` (`std/time/civil/offset.nv`) — явный запрос числового сдвига (см. D321 amend ниже: НЕ implicit-зона, D319 R1 не меняется).
- **Тесты:** `std/testing/handlers.nv` (fixed_ms/mut_clock coherence + custom handler-литерал non-zero); `std/time/civil/zoned_test.nv` (`Offset.local()` через handler-литерал — доказывает полную мокабельность нового опа).

**UPD 2026-07-10 (волна handler-annot, «один канал»):** барьер (b) — codegen-гэп anonymous-record-literal-в-handler-body — **СНЯТ**. Причина была не в «anon-record вообще», а в том, что оп-тела handler-литерала эмитились в отдельные C-функции БЕЗ переключения типового контекста (`expected_record_type`/`current_fn_return_ty` оставались от внешней функции); фикс подвёл к оп-телам ТОТ ЖЕ единый канал разметки, что у обычных fn/лямбд/протокол-методов (per-op контекст из effect-схемы, `emit_c.rs::emit_handler_lit`). Матрица: `nova_tests/plan175_handler_annot/repro_matrix.nv` (анонимные heap/value record'ы, tuple, sum-вариант, конструктор, захваты, вложенные хендлеры — все PASS). **Решение по `Time` НЕ пересматривается:** барьер (a) — намеренная opacity `Monotonic` (нормативное разграничение в AMEND выше) — самодостаточен, и `option C` (int-wire + typed `.nv`-сахар) остаётся итоговой отгруженной архитектурой (решение владельца). Провод `Time` этой волной не менялся.

## D317 — Duration/instant overflow-policy: trap-default + `checked_*`/`saturating_*` (Plan 175 Ф.1c, 2026-07-06)

**Source:** Plan 175 (time-system-rework), Ф.1c. **Amends:** [D316](#d316) (ns-канон → overflow-safe арифметика). **Реализация:** `std/time/duration.nv` (чистый `.nv`-слой; codegen НЕ тронут).
**Status:** ✅ ACTIVE / SHIPPED. Тесты: inline unit-блоки `std/time/duration.nv`; `spec_tests/conformance/d317_duration_overflow_policy.nv`; trap-фикстуры `nova_tests/time/rt/*` (`EXPECT_RUNTIME_PANIC`); cross-module `nova_tests/time/plan175_f1c_overflow_safe.nv`.
**Нумерация:** D317 из reserved-диапазона D316–D324 (175 = D316–318).

### Что

`Duration`/`Timestamp`/`Monotonic` — знаковые `i64`-ns записи. До Ф.1c ВСЕ операторы (`@plus`/`@minus`/`@neg`/`@times`/`@div`/`@abs`) были сырой **unchecked i64** — two's-complement **WRAP** на ±292 годах (Go-ловушка «the trap to avoid»). D317 вводит **3-tier дисциплину** (Rust/Swift-паритет; бьёт Go silent-wrap и Zig build-mode-UB).

### Правило (3-tier)

- **(R1) Операторы траппят.** `+`/`-`/унарный `-`/`*`/`/` на `Duration` **паникуют на overflow в debug И release** — никогда silent wrap (Go-антипример), никогда build-mode-зависимость (Zig `ReleaseFast`-UB антипример; Swift integer-арифметика трапает всегда = прецедент). Реализация — module-private `*_or_trap` хелперы поверх явной overflow-детекции (bare i64 `+`/`*` wrap by design → overflow детектируется ЯВНО, не полагается на trap примитива).
- **(R2) `checked_*` → `Option[T]`.** `@checked_add`/`@checked_sub`/`@checked_mul`/`@checked_div` на `Duration`; `@checked_add`/`@checked_sub` на `Timestamp`; `@checked_duration_since` на `Monotonic` (D318). `None` на overflow/`÷0`/`i64::MIN÷-1`.
- **(R3) `saturating_*` → clamp.** `@saturating_add`/`@saturating_sub`/`@saturating_mul` на `Duration` → clamp к **±(2⁶³−1)** (симметрично; `i64::MIN` = `-2⁶³` исключён, домен симметричен → `@neg`/`@abs` тотальны). Инстанты `Timestamp`/`Monotonic` `@plus(Duration)`/`@minus(Duration)`/`@minus(инстант)` → **saturate at i64-boundary** (зеркало Go `addSec`-clamp).
- **(R4) Асимметрия two's-complement.** `@abs(i64::MIN)` **saturate к `i64::MAX`** (НЕ UB/wrap; `|i64::MIN| > i64::MAX`). `@neg(i64::MIN)` → trap. `@div(0)` и `@div(i64::MIN, -1)` → trap.
- **(R5) f64-конверсии.** `@to_seconds()`/`@times(f64)`/`@div(f64)` (в т.ч. `÷0.0`→`±inf`) — trap на `NaN`/`±inf`/out-of-`i64`-range; non-trapping варианты `@checked_to_seconds()`/`@checked_mul_f64`/`@checked_div_f64` → `Option`. Не молчаливый мусор-cast (Rust `mul_f64(NaN)` паникует = прецедент).

### Границы / честные уступки (Q11/Q16)

- **`Duration`** = знаковый `i64` ns, диапазон **±(2⁶³−1) ns ≈ ±292 года**.
- **`Timestamp`** = unix-epoch ns, окно **1677-09-21 .. 2262-04-11** (i64 ±292y, Q16) — контракт задокументирован. Zig `nanoTimestamp() -> i128` не имеет 2262-горизонта; Nova принимает i64 **осознанно** (i128 ломает Q2 single-i64 scalar-bridge и value-ABI ради горизонта >2262). `from_unix_nanos(i64::MAX)` + `checked_add` → `None`; `@plus` → saturate (НЕ wrap в 1677) — pos-фикстура d317.
- **Отложено:** публичные консты `Duration.MAX`/`Duration.MIN` (Plan 178 запрашивал `@timeout(Duration.MAX)`) НЕ введены — user type-const с именем `MAX`/`MIN` **шэдоуит builtin numeric `.MAX`/`.MIN`** в type-set-bound generics (`fn[T Ints] f(x T) => x == T.MAX`, `spec_tests` d310) → мис-типизация `T.MAX` как record + CC-FAIL. Фикс — в checker member-const-резолюции (172-зона, owner-gated). Follow-up `[M-175-type-const-max-shadows-builtin]`. Saturation-границы доступны через builtin `i64.MAX`/`i64.MIN` (Plan 200 Step 1 — заменили internal `i64_max()`/`i64_min()` fn-хелперы, тот же контракт) — функциональность D317 полная без публичной консты.

### Почему

Silent two's-complement wrap на ±292y — это ровно Go-ловушка; Rust/Java/Kotlin/Temporal/Swift детектят overflow. Nova достигает паритета Rust/Java/Swift и обходит Go (silent-wrap) и Zig (UB-в-ReleaseFast, build-mode-зависимость). Trap-default безопасен by construction; `checked_*` — Rust-эскейп для восстановления; `saturating_*` — для «no timeout»-семантики (Plan 178).

> **AMEND (2026-07-16, Plan 200 Step 2, владелец) — конструкторы `from_*` →
> `to_*`-бланкет.** `Duration.from_nanos/micros/millis/secs/mins/hours/days/
> weeks/secs_f64` и `Timestamp.from_unix_secs/millis/nanos` — **ретрактированы**
> вместе с per-width bare-fluent (`int @seconds()`, singular `1.second()` и
> т.п.). Заменены единым `fn[T Ints] T @to_nanos/to_micros/to_millis/to_seconds/
> to_minutes/to_hours/to_days/to_weeks() -> Duration` (и симметричный
> `to_unix_seconds/to_unix_millis/to_unix_nanos() -> Timestamp`) — один бланкет
> вместо ×8 почти-идентичных конкретных методов, зеркалит Plan 206
> `checked_*`-бланкеты (D423). Приёмник `@` явно widen'ится в `i64` до
> арифметики (иначе узкие ширины типа `i8` переполнились бы на `* 1_000` до
> приведения). `f64 @to_seconds()` — единственный float-конструктор («только
> секунды», остальные f64-юниты retracted без замены — repo-wide grep на
> использование = 0); `Duration.try_from_secs_f64` (fallible) на момент ЭТОГО
> амендмента (2026-07-16) был сохранён без изменений имени (не `from_*`-
> паттерн конструктора, отдельная Option-семья) — см. AMEND 2026-07-17 ниже,
> статик впоследствии тоже ретрактирован.
> Singular-алиасы (`1.second()`, `1.hour()`, ...) убраны без замены — DRY,
> `1.to_seconds()`/`1.to_hours()`. Getter/constructor коллизия имён снята:
> `d.nanos()` (голое, `Duration → i64`) vs `5.to_nanos()` (`to_`, `int →
> Duration`) — разные имена, один и тот же тип-набор `Ints` работает на обеих
> сторонах моста без дублирования тела. Зависело от `[M-primitive-receiver-
> bounded-blanket-dispatch]` (Plan 196.8/196.9, закрыт) — примитивный ресивер
> (`i8`..`u64`) должен честно резолвиться в bounded-бланкет, а не мис-
> диспатчиться в конкретный одноимённый метод постороннего типа в том же CU.

> **AMEND (2026-07-17, lint-разкраснение, владелец) — `Duration.try_from_secs_f64`
> RETRACTED, заменён `f64 @checked_to_seconds()`.** `W_TRY_WITHOUT_SIBLING`
> (`try_`-префикс легален только как fallible-половина инфаллибельной пары —
> `from`/`try_from`, D77/R3 D325) поймал этот статик-метод без сиблинга.
> Решение владельца — не exception, а снос статики целиком, тем же курсом,
> что уже убрал `Duration.from_*` (амендмент выше): "мы убрали все
> `Duration.from_*`". Ресиверная форма на ИСТОЧНИКЕ (`f64 @checked_to_seconds()
> -> Option[Duration]`), зеркалящая non-trapping half пары `@to_seconds()`/
> `@checked_to_seconds()` — тот же паттерн, что `@times(f64)`/`@checked_mul_f64`
> на `Duration`. Сигнатура/семантика (`None` на `NaN`/`±inf`/out-of-`i64`-range)
> не изменились, только форма вызова: `Duration.try_from_secs_f64(s)` →
> `s.checked_to_seconds()`.

## D318 — Monotonic: non-regression + clock-source contract (Plan 175 Ф.1c, 2026-07-06)

**Source:** Plan 175, Ф.1c. **Amends:** [D124](#d124) (wall/monotonic-разделение). **Реализация:** `std/time/duration.nv`.
**Status:** ✅ ACTIVE / SHIPPED. Тесты: `spec_tests/conformance/d318_monotonic_non_regression.nv`; inline `std/time/duration.nv`; `nova_tests/time/plan175_f1c_overflow_safe.nv`.

### Правило (контракт из двух частей)

- **(R1) Non-regression.** `Monotonic` never goes backwards by contract. При кажущемся регрессе часов (later mark < earlier — HW/VM/OS-баг, JDK-6458294): `@elapsed_since` **SATURATE-to-ZERO** (возвращает `Duration.ZERO`, **никогда negative, никогда panic, без global-lock** — урок Rust 1.60-saga, стабильный контракт, не флип-флопить). `@checked_duration_since(other)` → `None` на регрессе, `Some(self − other)` иначе (`Some(ZERO)` на равенстве). `Monotonic ± Duration` → saturate at boundary (D317). `Monotonic` **non-serializable** (process-local; Ф.6 верифицирует отсутствие derive-пути — Q13).
- **(R2) Clock-source (Q14).** `monotonic()` читает `uv_hrtime()`: Linux `CLOCK_MONOTONIC` / macOS `mach_absolute_time` (оба **suspend-EXCLUDED**) / Windows QPC (suspend-поведение платформозависимо). Nova гарантирует **только монотонность + non-regression**, НЕ suspend-inclusion; `sleep_until` через сон устройства = unspecified-but-monotonic. Индустрия расходится (Zig `Instant` = `CLOCK_BOOTTIME`, Rust/Go = `MONOTONIC`, Swift экспонирует ОБА) → молчание = footgun. BOOTTIME-аналог (`ContinuousClock`) → `[M-monotonic-boottime]` (вводить при use-case).
- **(R3) Infallibility (Q15).** `monotonic()` **infallible by contract** на tier-1 libuv (Win/Linux/macOS; `uv_hrtime` не фейлит); Zig-style error-union отклонён (вирусит call-sites ради платформ, которых нет).

### Почему

HW/VM/OS могут дать кажущийся регресс монотонных часов; паниковать на hot-path (retry-budgets, deadlines) недопустимо, лочить (Rust 1.60-saga) — тоже. Saturate-to-zero + `checked_*`-эскейп = стабильный, lock-free, negative-free контракт. Раздельные типы (D124) + non-serializable Monotonic закрывают Go-footgun (`m=…` течёт в `String()`).

## D319 — Civil-time модель: type-ladder Plain/Offset/Zoned + proleptic Gregorian + epoch-day repr (Plan 175.1 Ф.0/Ф.1, 2026-07-10)

**Source:** Plan [175.1](../../docs/plans/175.1-civil-time.md), Ф.0/Ф.1/Ф.3. **Amends:** ничего (аддитивный слой поверх D316-318). **Реализация:** `std/time/civil/` — folder-module `time.civil` (чистый `.nv`; codegen не тронут).

**Статус:** ✅ ACTIVE.

### Правило

- **(R1) Type-ladder — нормативный инвариант.** `Date`/`TimeOfDay`/`DateTime` (**Plain** — нет зоны, *неоднозначны*, НЕ точка на временнóй оси) → `Offset`/`ZonedDateTime` с `zone=Fixed`/`Utc` (**Offset** — точка, фикс. сдвиг, без DST-правил) → `ZonedDateTime` с `zone=Iana` (**Zoned** — точка, IANA-rule-aware). Plain → `Timestamp` **только** через явный `Offset`/`TimeZone` + `Disambiguation`, и это **fallible** (`Result[Timestamp, DateError]`/`Result[ZonedDateTime, DateError]`). Нет неявного дефолта зоны — компилятор не подставляет «локальную» зону молча.
- **(R2) Value-records, immutable.** `type Date value { ro epoch_day i64 }`, `TimeOfDay value { ro nanos_of_day i64 }`, `DateTime value { ro date Date, ro time TimeOfDay }`, `Offset value { ro seconds i32 }` (±64800, ±18h), `ZonedDateTime value { ro dt DateTime, ro offset Offset, ro zone TimeZone }`, `YearMonth value { ro year i32, ro month i32 }`, `MonthDay value { ro month i32, ro day i32 }`. Stack, zero-GC, structural `==` (D183 `@compare`).
- **(R3) Proleptic Gregorian ONLY (Q10).** year 0 = 1 BCE, отрицательные годы допустимы. Non-Gregorian (Japanese/Hijrah/…) — design-reject, вне scope, без `Chronology`.
- **(R4) Epoch-day repr + Hinnant-алгоритм.** `Date.epoch_day` — дни от 1970-01-01, branch-light `days_from_civil`/`civil_from_days` (Howard Hinnant, overflow-safe; идентично chrono). Leap: `÷4 кроме веков, если не ÷400`. `day_of_week` — по модулю epoch_day.
- **(R5) Range/overflow → Result, never silent (Q4/Q6).** `Date.new(y,m,d) -> Result[Date, DateError]` — validate-by-default, НИКОГДА не нормализует молча (`Date.new(2024,Feb,30)` → `Err(InvalidField(...))`). Явный opt-in `Date.from_normalized(y,m,d) -> Date` — единственный normalize-путь. Civil↔Timestamp вне ±292y окна (наследует Timestamp-окно D317) → `Err(RangeOverflow(...))`; MIN/MAX civil подобраны так, что `Timestamp+Offset→civil` **инфаллибельно** (Err только civil→Timestamp).
- **(R6) Leap-seconds ignore (Q5).** День=86400s, минута=60s, `second` 0..59. Parse `:60` → clamp к `:59` (унаследовано из строгого канона 175 «день=86400s»).
- **(R7) `TimeZone` — sum-type, не flat value.** `type TimeZone enum Utc | Fixed(Offset) | Iana(IanaZone)` — `Utc`/`Fixed` value-weight (нет DST); `Iana` несёт `IanaZone` (reference-тип, handle к transition-таблице, Ф.4/D321). `OffsetDateTime` отдельным типом НЕ вводится (jiff-подход) — `ZonedDateTime{zone: Fixed(_)}` покрывает.

### Почему

Единый flat `time.Time`+`*Location` (Go) прячет «wall-clock как момент» баг в рантайме — компилятор не различает Plain/Zoned. java.time/Temporal/kotlinx разделяют явно; Nova делает то же через value-records (дешевле heap-объектов Java) + fallible-конструкторы (не throw/NaN).

## D320 — `Period` (календарный y/m/d) ≠ `Duration` (ns) + DateBased/TimeBased-разделение (Plan 175.1 Ф.2, 2026-07-10)

**Source:** Plan 175.1, Ф.2. **Amends:** ничего (Duration — D316/D317 не меняются). **Реализация:** `std/time/civil/period.nv`.

**Статус:** ✅ ACTIVE.

### Правило

- **(R1) Два тип-различных amount, без неявной коэрсии.** `Duration` (точный `i64`-ns, из 175) и `Period value { ro years i32, ro months i32, ro days i32 }` (календарный). Нет implicit-conversion между ними.
- **(R2) Enforcement на уровне типов приёма.** `Date` принимает только `Period` (`@plus(Period)`/`@minus(Period)`), **отвергает** `Duration` — «add hours to a Date» = **compile-error** (нет перегрузки `Date @plus(Duration)`, отсутствие метода = отказ на этапе резолва). `TimeOfDay` — наоборот, принимает только `Duration`. `DateTime`/`ZonedDateTime` принимают оба (раздельные перегрузки), с разной семантикой (R4).
- **(R3) Calendar-арифметика — CLAMP biggest-unit-first (Q7).** month/year-add: сначала years→months (clamp к длине результирующего месяца — `Jan31+1mo` → `Feb28`/`Feb29`), затем days (exact, через epoch-day). `checked_plus`/`checked_minus` → `Result`; `saturating_plus`/`saturating_minus` → clamp к `Date.MIN`/`MAX`; голый `@plus`/`@minus` (`+`/`-` операторы) — **trap-on-overflow** (наследует D317, НЕ wrap). `Period.normalize()` схлопывает months↔years (12mo→1y), **days НИКОГДА не сворачиваются** (calendar length varies).
- **(R4) Wall-vs-elapsed асимметрия (DateTime/ZonedDateTime).** `dt.plus(Period{days:1})` — календарный сдвиг (та же wall-time; через DST у Zoned даёт 23ч/25ч elapsed). `dt.plus(Duration.from_hours(24))` — точный elapsed-сдвиг (может сдвинуть wall-time через DST-границу). Документируется как non-invertible/non-associative в общем случае (`d.plus(p).minus(p) != d` возможен на clamp-границах — напр. `Jan31.plus(1mo).minus(1mo) == Feb28`, не `Jan31`).
- **(R5) `Period.between(Date, Date) -> Period`** — calendar-diff (years/months/days biggest-unit-first); exact-дни — `@days_until(Date) -> int`. Operator-форма `Date - Date` ретрактирована реализацией (`[M-175.1-minus-overload-arg-type]`, см. D321 §impl-отступления).

### Почему

Temporal (единая `Duration`) узнаёт «календарная она или нет» только в рантайме (`RangeError`); Go вообще не имеет `Period` (3 голых int в `AddDate`, order-dependent overflow, golang#71334). Type-separated приём делает ошибку класса «add hours to a Date» **compile-time**, не рантайм.

## D321 — DST `Disambiguation` (4-way) + `OffsetConflict` + parse-strictness + структурные `DateError`/`ParseDateTimeError` + IANA tz-db (Plan 175.1 Ф.3/Ф.4/Ф.5, 2026-07-10)

> **Амендмент (Plan 200 П21, 2026-07-21, владелец):** композит-конструктор
> `DateTime.new(y i32, m Month, d i32, h int = 0, min int = 0, s int = 0, ns int = 0)
> -> Result[DateTime, DateError]` — арность-сиблинг `DateTime.new(date, time)`; default-полночь
> (`DateTime.new(2026, Jun, 8)` == 00:00:00.0), Python-паритет одним вызовом
> (`datetime(y, m, d, h, min)`), тело = композиция `Date.new` + `TimeOfDay.new` (валидация не
> дублируется). `Month`-enum и `Result` сохранены намеренно. Парная форма `Date @at(h, m = 0,
> s = 0, ns = 0)` ОТЛОЖЕНА: инстанс-перегрузка по арности на конкретном типе коллидирует в
> C-мангле (`[M-concrete-instance-arity-overload-mangle]`) — вернётся после фикса.

**Source:** Plan 175.1, Ф.3 (Offset/Fixed/Zoned) + Ф.4 (IANA) + Ф.5 (parse-strictness). **Amends:** ничего. **Реализация:** `std/time/civil/{offset,zoned,tz,tzif,parse,format}.nv`.

**Статус:** ✅ ACTIVE (Ф.3/Ф.5 — полностью; Ф.4 IANA — **реализовано с задокументированным сужением данных**, см. §tzdb ниже).

### Правило

- **(R1) `Disambiguation` — 4-way Result-значение, default `Compatible` (Q8).** `type Disambiguation enum Compatible | Earlier | Later | Reject`. Gap (spring-forward, wall-time не существует) → `Compatible`: push-forward на длину gap (after-offset); `Reject` → `Err(Ambiguous(...))`. Overlap (fall-back, wall-time существует дважды) → `Compatible`: earlier-offset; `Earlier`/`Later` — явный выбор. Ambiguity surface как значение (`is_gap`/`is_overlap` на промежуточном resolve-результате), не exception — превосходит java.time (только earlier/later вручную), Go/kotlinx (silent), chrono (`None` путает gap с load-error).
- **(R2) `OffsetConflict` — 4-way, default `RejectMismatch` (Q9).** `type OffsetConflict enum RejectMismatch | Use | Prefer | Ignore` — резолвит рассогласование между offset, сохранённым в parsed-строке, и текущими правилами зоны (post-tzdb-change drift). Только для explicit-offset строк (RFC-9557 `[zone]`-bracket с offset). **Амендмент имени (реализация 2026-07-10):** jiff/Temporal-имя `Reject` коллидировало с `Disambiguation.Reject` — пространство имён вариантов флоское, а qualified `Enum.Variant` как значение — ICE `[M-175.1-qualified-variant-value]` → вариант `RejectMismatch` (`[M-175.1-variant-name-collision]`). Дефолт применяется arity-split-перегрузкой (`s.to_zoned_datetime()` / `s.to_zoned_datetime(conflict)`, прецедент D324 `env(k)`/`env(k,v)`; default-значение enum-варианта на call-site не эмитится — `[M-175.1-enum-default-param]`).
- **(R3) Parse-strictness — STRICT by default (Q9).** Reject Feb-30/out-of-range/missing-parts/zoned-строка-без-`[zone]`-bracket. Lenient — отдельный opt-in-параметр. «parses» == «constructs» (закрывает java.time SMART-trap, где `parse` тише `of`).
- **(R4) Структурные ошибки.** `type DateError enum InvalidField(FieldKind, i64, i64, i64) | RangeOverflow(str) | Ambiguous(str) | UnknownZone(str) | BadTzData(str)` (последние два — tz-загрузка Ф.4); `type ParseDateTimeError enum FormatMismatch(int, str) | InvalidValue(int, FieldKind, i64)`; `type FieldKind enum Year | Month | Day | Hour | Minute | Second | Nano | OffsetSec | Weekday`. Нет default-panicking конструктора нигде в civil-API.
- **(R5) `Utc`/`Fixed` — always unambiguous.** Нет DST-правил → `Disambiguation` не активируется (gap/overlap невозможны by construction).
- **(R6) IANA tz-db (Ф.4, §tzdb).** `type IanaZone { ro id str, ro rules ZoneRules }` (reference-запись); загрузка по имени — §1а-конверсия на источнике **`s.to_timezone() -> Result[TimeZone, DateError]`** (`TimeZone.try_from` из черновика ретрактирован каноном 174.1 «конверсия — метод на источнике»); плюс `TimeZone.from_tzif(id, bytes)` (raw-TZif) и `load_timezone(name) Fs Os` (OS-first). Именованные зоны дают **реальный** gap/overlap → 4-way `Disambiguation` (R1) работает не вхолостую.

### §tzdb — источник данных (задокументированное сужение, Ф.4)

Полный layered-model (OS-tzdata-first TZif-parser + `$ZONEINFO` override) **реализован** в `std/time/civil/tzif.nv` (бинарный RFC-8536 TZif v1/v2/v3 parser поверх `std.fs`; `load_timezone(name) Fs Os`: `$ZONEINFO`-override → `/usr/share/zoneinfo/<name>` (POSIX) → embedded). **Embedded-fallback** (обязателен на Windows — нет `/usr/share/zoneinfo`) реализован как **rule-based таблица** (`std/time/civil/tz.nv`) для curated-списка зон (`Utc`+алиасы, фикс-оффсеты `±HH:MM[:SS]`, `America/New_York`, `Europe/London`, `Europe/Moscow`, `Australia/Sydney` — транзишены генерируются из современных правил на 1996..2100, реальный spring-forward/fall-back), а **не** полный скомпилированный ~450KB IANA-snapshot (акт данных — задача упаковки/дистрибуции, не архитектуры; полный snapshot — follow-up `[M-175.1-full-tzdb-embed]`). `tzdb_version() -> str` возвращает версию curated-таблицы (`"nova-curated-2026a"`). Футер-строка POSIX-TZ (правила за последним переходом) не интерпретируется — за пределами таблицы действует последний сдвиг (документировано).

### Почему

4-way `Disambiguation`+`OffsetConflict` как значения (не throw/silent) — отличие от всех пяти peers одновременно (см. Plan 175.1 §2 таблица). Rule-based embedded-fallback — прагматичный компромисс: полный TZif-парсер работает по архитектуре (эксплуатируется на POSIX, где `/usr/share/zoneinfo` реально есть), embedded-таблица даёт корректный современный DST для тестируемых зон без версионирования полного snapshot-датасета в этой волне.

### §impl-отступления (реализация 2026-07-10, все с маркерами)

- ~~`Time.local_offset() -> Offset` эффект-оп НЕ поставлен~~ — **ЗАКРЫТО** тем же 2026-07-10 (см. AMEND ниже + [D316](#d316) amend): `[M-175.1-local-offset-effect-op]` больше не открыт.
- Операторная форма `Date - Date -> Period` не введена (`[M-175.1-minus-overload-arg-type]` — overload-резолв оператора слеп к типу аргумента); канон — `Period.between(start, end)` + `@days_until`. По той же причине оператор `d + duration` на Date не отлавливается компилятором (`[M-175.1-operator-arg-type-blind]`) — гейт D320 R2 держит метод-форма (`d.plus(duration)` — compile-error, neg-fixture).
- `Date.MIN`/`MAX`-консты → static-фны `Date.min_value()`/`max_value()` (чекер-гэп `[M-175-type-const-max-shadows-builtin]` + `[M-175-value-record-const-ref]`); `TimeOfDay.MIDNIGHT` → `TimeOfDay.midnight()`.
- Интерполяция `"${date}"` value-record'а минует user `@to_str` (`[M-175.1-interp-value-record-display]`, pre-existing класс) — Display-тела корректны, подключение — компилятор-волна.
- Декларация `DateTime` живёт в `time_of_day.nv` (порядок эмиссии value-record структур лексикографический по файлам, by-value поле требует complete-типа) — `[M-175.1-value-in-value-emit-order]`; методы на variant-литералах (`Sun.next()`) — `[M-175.1-variant-literal-receiver]` (в тестах bound-local ресиверы).

**AMEND (2026-07-10, owner decision — system-offset доступен):** системный часовой пояс машины ДОЛЖЕН быть доступен из Nova — `Time.local_offset_sec()` эффект-оп поставлен ([D316](#d316) amend: vtable-слот `NovaVtable_Time.local_offset_sec` + OS-хук Windows `GetTimeZoneInformation`/POSIX `tm_gmtoff`), Nova-сахар `Offset.local() Time -> Offset` (`std/time/civil/offset.nv`). **Зона в `ZonedDateTime` остаётся ЯВНОЙ** — эта AMEND НЕ вводит implicit-default нигде в civil-API (R1 не меняется): `Offset.local()` — explicit query той же природы, что java.time `ZoneId.systemDefault()` / Temporal `Now.timeZoneId()` (вызывающий явно запрашивает системный сдвиг и явно передаёт его дальше — `dt.to_zoned(TimeZone.Fixed(Offset.local()))`; никакого auto-fallback на «локальную зону», если пользователь не указал зону). Закрывает `[M-175.1-local-offset-effect-op]` (см. §impl-отступления выше).

## D322 — io-core: `io.Read`/`io.Write`/`io.Seek`, `IoError`, `Io` effect (Plan 176 Ф.1, 2026-07-04)

**Статус:** IMPLEMENTED (Ф.1 — io-core; fs=D323 Ф.2 / os=D324 Ф.3 — реализованы). Модуль `std/io`.

### Протоколы (byte I/O)

```nova
type io.Read  protocol { mut @read(buf mut []u8) -> Result[int, IoError] }
type io.Write protocol { mut @write(data []u8) -> Result[int, IoError]; mut @flush() -> Result[(), IoError] }
type io.Seek  protocol { mut @seek(pos SeekFrom) -> Result[int, IoError] }
type SeekFrom | Start(int) | End(int) | Current(int)   // всё int (i64); Start(<0) → InvalidInput
```

- **Эффект-агностичны (Q15, D122-amended):** конформер несёт СВОЙ плумбинг-эффект
  (`File`→`Fs`, `TcpStream`→`TcpNet`, консоль→`Io`), всплывающий транзитивно при
  мономорфизации. Generic-вызовы через io-bound — **mono-dispatch only** (vtable для
  effectful-bounds запрещён).
- **Sibling prelude text-sink `Write`** (D374): байтовый `io.Write` ссылается квалифицированно
  (`io.Write`); мост text→bytes — явный `write_str`.
- **EOF/partial/EINTR-контракт (Q9):** `read` → `Ok(0)` = EOF **только при непустом буфере**;
  short-read — норма; partial-write легален (`write_all` loop); `Ok(0)` mid-write → `WriteZero`;
  `Interrupted`(EINTR) — retry в std-хелперах. НЕ Go `(n>0, EOF)`.

### Хелперы (free generic fns, mono-dispatch)

`read_exact`(→UnexpectedEof) / `read_to_end` / `read_to_string`(→InvalidData на невалидном UTF-8, Q11) /
`write_all`(→WriteZero) / `write_str` / `copy` / `lines`(Q7: strip trailing `\r`, финал без `\n` — yield,
embedded lone `\r` НЕ сепаратор — делегат `str.@lines`) / `byte_lines`(raw `\n`-split).
In-memory конформеры: `BytesReader` (Read+Seek, cursor), `BytesWriter` (Write, growable sink).

### `IoError` (структурный, Rust `ErrorKind`-precedent)

```nova
type IoError { ro kind ErrorKind, ro raw_os int, ro op str }   // Ф.2 добавит path Option[Path] + boxed source
type ErrorKind | NotFound | PermissionDenied | AlreadyExists | ... | Unsupported | Other(int)   // OPEN → wildcard-arm обязателен
```

- **Heap record** (НЕ `value`): как Rust `io::Error` (внутренне boxed) — pointer-sized, дёшево течёт через
  `Result[T, IoError]` в generic-хелперах (by-value record ловит `Result[T, ValueRecord]`-mono-gap).
- `raw_os` **authoritative**; `kind = kind_from_errno(raw_os)` — best-effort projection (общие POSIX errno;
  редкие → `Other(raw)`, §3b). **Per-op error sets (Zig) — considered/REJECTED (Q14):** один открытый `ErrorKind`
  + `raw_os` + (Ф.2) `source`-chain композируется, а не дробит обработку.
- `Utf8Error{byte_offset}` + `str.from_bytes(bytes)->Result[str, Utf8Error]` — Ф.0.5 (D325-канон; ретайр
  интринзика `str.try_from([]u8)`).

### `BufReader` / `BufWriter` (Q10, D133)

- **`BufWriter[W] consume`** — **must-consume (D133)**: `@close()` (flush + Result); незакрытый = compile-error
  `D133-not-consumed`; double-close = use-after-consume. Нет silent flush-on-drop. Бьёт Go `bufio.Flush` /
  Rust `Drop`-swallow / Zig ручной flush (§1a #1). `@write`/`@flush`/`@write_str` — io.Write.
- **`BufReader[R]`** — буферизует чтения chunk'ами; сам io.Read.

### `Io` effect (консоль, мокабельна, §3c)

```nova
type Io effect {
    read_in(buf mut []u8) -> Result[int, IoError]    // buffer-fill (Result[[]u8] Ok-payload эрейзится vtable'ом)
    write_out(data []u8) -> Result[int, IoError]
    write_err(data []u8) -> Result[int, IoError]
}
```

- Хендлы `stdin()`/`stdout()`/`stderr()` конформят io.Read/io.Write поверх `Io`.
- **`real_io()`** — fd-хуки `io_read_fd`/`io_write_fd` (`nova_rt/io_console.h`, C stdio FILE*; return `-errno` на
  ошибке → `IoError.from_os`).
- **`mock_io(cap IoCapture)`** — capture stdout/stderr + scripted stdin; детерм. консоль-тесты без терминала
  (мокабельность, §1a; носитель §8.4).

### Реализационные ноты (обход codegen-ограничений; НЕ упрощения семантики)

1. **Heap `IoError`** (см. выше) — by-value record не мономорфизируется в generic-`Result`.
2. **Хелперы инлайнят циклы** (не форвардят один bounded-generic в другой generic-fn — чекер не проносит bound
   через такой форвард).
3. **`BufReader`/`BufWriter` строятся с ЯВНЫМИ type-args** (`BufWriter[BytesWriter].new(...)`): inference-only
   конструкция generic-wrapper'а не материализует мономорфизированные методы (иначе — NULL-stub → крах).
   Followup `[M-176-generic-wrapper-mono-inference]`.
4. **`SeekFrom.start/end/current`** — статические конструкторы (cross-module литерал payload-варианта
   `SeekFrom.Start(n)` ловит checker-gap на возвратном типе конструктора; pattern-match не затронут).
   Followup `[M-176-xmod-payload-variant-ctor]`.

## D323 — fs: byte-backed `Path`, `Fs` effect, `File` must-consume, `Metadata` (Plan 176 Ф.2, 2026-07-04)

**Статус:** IMPLEMENTED (Ф.2). Модуль `std/fs`. Строится над io-core (D322): `File impl io.Read/io.Write/io.Seek`.

### byte-backed `Path` (Q1)

```nova
type Path value { ro bytes []u8, ro style PathStyle }   // НЕ str — несёт raw OS-байты
type PathStyle | Posix | Windows
```

- **`value`-record над `[]u8`** (Rust `OsStr`/`Path`, Swift-system `FilePath`, Zig `[]const u8`): non-UTF-8 Unix /
  WTF-8 Windows имена round-trip'ят лосслесс. `from_str`/`from_bytes` (host-style), `posix`/`windows` (pinned
  style — один тест-прогон проверяет ОБЕ платформы), `styled`.
- **Lexical (pure, без effect):** `is_absolute` (Posix `/`; Windows drive `C:\` + UNC `\srv\share`; `C:foo`/`\foo`
  НЕ absolute), `parent`/`file_name`/`extension`/`stem`/`components`/`normalize`(collapse `.`/`..`, canonical
  separator; НЕ резолвит symlinks)/`join`/`with_extension`. `to_str`→`Option[str]` (lossless), `display`→`str`
  (lossy U+FFFD, print-only), `as_os_bytes`→`[]u8`, `equals` (byte+style exact).
- **Windows separator:** и `/` и `\` разделяют; canonical output — `\`. Drive/UNC-префиксы распознаются.

### `Fs` effect — ТОНКИЙ int-primitive слой (§3/§0)

Операции возвращают **сырые `int`/`i64`/`str`-коды** (не `Result`/`Metadata`/`DirEntry`): effect-vtable **стирает**
rich `Result[T, IoError]`-возврат в canonical `nova_int`/`nova_str` пару (теряя value-`IoError` и Ok-record), поэтому
всё построение `IoError`/`Metadata`/`DirEntry` — в pure-Nova обёртках ВНЕ effect-границы (там закрытый value-record
keystone работает). Коды зеркалят fs.c-хуки 1:1: `>= 0` успех (fd/байты/0), НЕГАТИВНЫЙ POSIX errno на ошибке.
`stat`/`lstat`/`fstat` → 0/-errno + кэш; `stat_size`/`stat_kind`/`stat_mtime_ns`/... читают кэш (cooperative-safe).

**Триада:** `real_fs()` (libuv `uv_fs_open/read/write/close/stat/lstat/scandir/mkdir/unlink/rename/realpath/symlink/
chmod/fsync/copyfile`, park/wake ТОЧНО как net.c; best-effort-cancel Q4: `uv_cancel` на queued, in-flight
дорабатывает) + `mock_fs(MemFs)` (in-memory byte-Path-дерево, ENOSPC-инъекция для close-error/torn-write тестов —
детерминизм без диска, §1a-differentiator).

### `File` must-consume (D133) — §1a #1 differentiator

```nova
type File consume { priv fd int, priv readable bool, priv writable bool, priv pos int }
fn File consume @close() Fs -> Result[(), IoError]   // ЕДИНСТВЕННАЯ явная разрядка; незакрытый = compile-error
```

- Незакрытый `File` = `D133-not-consumed`; double-close/use-after = use-after-consume. Ошибка close (ENOSPC/EIO/
  quota — часто видна ТОЛЬКО на close) НЕ-игнорируема. Бьёт Go `defer Close()`/Rust `Drop`/Java suppressed/Zig
  `close()->void`. **NB:** enforcement работает для consume-**параметров** + прямых `consume x = Ctor()`; tracking
  через `Result`/match-extract (`match File.open(p){Ok(f)=>…}`) — checker-gap `[M-176-consume-through-result-match]`
  (общий с net `TcpStream`).
- `File` несёт СВОЙ `pos` и использует **positioned** read_at/write_at (portable — без непортируемого OS-`offset=-1`
  «current position»). `OpenOptions` (read/write/append/truncate/create/create_new, Q13; `append+truncate` →
  `InvalidInput`; append стартует cursor на EOF). `read_at`/`write_at`/`seek`/`sync_all`(fsync)/`sync_data`
  (fdatasync)/`metadata`(fstat).

### `Metadata` / `DirEntry` / `Permissions`

`Metadata` (heap): `len`/`file_type`/`is_file`/`is_dir`/`is_symlink`/`permissions`/`modified`/`accessed`/`created`
(каждый timestamp → `Option[Timestamp]`, Plan 175; birth-time отсутствует → `None`). `DirEntry`: `file_name`(Path)/
`file_type`/`path(dir)`. `Permissions value { read_only bool, mode int }` — портабельный readonly + unix-mode (Q8/Q12).

### Durability + FFI (§3c)

- **`write_atomic` (5-шаг durable):** (1) temp в ТОЙ ЖЕ директории `O_EXCL` → (2) write_all → (3) fsync файла →
  (4) atomic rename → (5) best-effort fsync родительской директории (no-op на Windows). Бьёт Swift `.atomic`/Zig
  `AtomicFile` (tmp+rename БЕЗ fsync — не durable). Torn-write через mock-ENOSPC → rollback + удаление temp.
- **FFI-граница:** путь → NUL-terminated `*u8` (`c_path` reject interior-NUL → `InvalidInput`); libuv сам конвертит
  UTF-8/WTF-8 → UTF-16 на Windows → **CWStr не нужен на libuv-бэкенде** (`[M-176-cwstr-direct-winapi]`). Данные →
  `(*u8, int)`. Non-blocking `fs_seek` (lseek) + platform-predicate — в `io_console.h` (без libuv).

### Реализационные ноты (обход codegen-ограничений; НЕ упрощения семантики)

1. **`Fs` effect — int-primitive** (не rich `Result`): effect-vtable стирает value-`IoError`-error в `nova_str`;
   обёртки строят `IoError`/`Metadata`/`DirEntry` вне effect-границы.
2. **value-record литералы** (`Path`/`OpenOptions`/`Permissions`/`FileType`): typed-форма (`Path { … }`) в
   блок-позиции; anonymous (`{ … }`) в `=>`-теле с объявленным возвратным типом (checker: typed-prefix redundant в `=>`).
3. **std.fs free-fn имена** не коллидят с std.io generic-хелперами (coarse-by-name резолв): `read_text`/`write_text`/
   `copy_file` (не `read_to_string`/`write_str`/`copy` — те резолвятся в `std.io.*[Path]` mono).
4. **`IoError.path`/`source`** (§3b full-shape) — отложены: io↔path module-cycle + value-`Option[Path]`-mono
   blast-radius на io-core baseline; `kind`(NotFound/…) сохранён (все тесты/§8.3 на нём). Followup.

### Амендмент D323 (2026-07-16, Plan 210 Ф.6б): `ReadFs` — read-only VFS-протокол

**Решение.** `ReadFs` (`std/src/fs/readfs.nv`) — read-only виртуальная ФС, объединяющая
чтение из реальной ФС (`DirFs`) и из вшитой папки (`EmbeddedDir`, D412-амендмент, `03-syntax.md`)
под ОДНИМ generic-bound. Главный кейс: статика веб-сервера «dev = с диска (live-reload),
prod = embedded» — один и тот же generic-код `fn serve[F ReadFs](assets F, ...)`, мономорфизуемый
дважды.

**Протокол эффект-АГНОСТИЧЕН** (модель `io.Read`, тот же D322): методы объявлены БЕЗ аннотации
эффекта — конформер несёт СВОЙ (`DirFs` → `Fs`, `EmbeddedDir` → чистый), всплывающий транзитивно
при mono (Q15). Subsumption эффектов не нужен — протокол никогда не объявляет `Fs`, поэтому
ситуации «impl имеет МЕНЬШЕ эффектов, чем протокол» не возникает.

```nova
export type ReadFs protocol {
    @read_file(path str) -> Result[[]u8, IoError]
    @path_exists(path str) -> Result[bool, IoError]
}
```

- `@read_file` — `Err(IoError{NotFound})` = файла нет; прочие `Err` — реальный I/O-сбой (только
  эффектные impl). `@path_exists` — соло fallible-операция без инфаллибельного сиблинга: обычное
  имя + `Result`, без `try_`-префикса (R3 D325; `@exists` недоступно — reserved-квантор). Ключ —
  POSIX `/`, case-sensitive, без ведущего `./` (конвенция `embed_dir`).

  **AMEND (2026-07-17, lint-разкраснение, владелец):** метод переименован `@try_exists` →
  `@path_exists` — `W_TRY_WITHOUT_SIBLING` (`try_`-префикс легален только как fallible-половина
  инфаллибельной пары, `from`/`try_from`, D77); одиночная fallible-операция не подходит под это
  правило (R3 D325, `nv-coding-style` §1). Сигнатура/возврат `Result` не меняются, только имя.
- `list`/directory-index **вне протокола**: у реальной ФС обход дорог (`Fs`-эффект),
  недетерминирован между вызовами (dev live-reload) и выводит наружу symlink/dot-ловушки; у
  `EmbeddedDir` — дёшево, но дробление протокола («минимальный протокол», как `io.Read`/`io.Write`/
  `io.Seek`) не платит обходом там, где нужно только чтение. Future — отдельный `ListFs`.
- **`EmbeddedDir` конформит EXTENSION-методами** (D287): `@read_file`/`@path_exists` объявлены в
  `std.fs` (не в `EmbeddedDir`'s home-модуле `prelude.embed`); родной Option-API (`@get`/`@has`/
  `@paths`) не тронут. **Эмпирически подтверждено** (`std/src/fs/readfs_test.nv`): структурная
  conformance по generic-bound `[F ReadFs]` видит extension-метод НАРАВНЕ с inherent — wrapper-
  newtype fallback (предусмотренный на случай провала) не понадобился.
- **`DirFs { priv root Path }`** — read-only вид на поддерево реальной ФС с корнем `root`.
  `DirFs.new(root)` — чистый конструктор (канонизация root — НЕ в кторе, это `Fs`-эффект; root
  может не существовать в момент конструирования — dev). Чтения ограничены `realpath(root)`:
  (1) лексически — `Path.normalize()` отвергает абсолютный путь и сохранившийся ведущий `..`;
  (2) symlink-hard — `canonicalize(root)`/`canonicalize(join)` + component-граничная prefix-
  проверка (строковая граница по ОБОИМ разделителям `/`/`\`, не единственному `canonical_sep`:
  `canonicalize` под `mock_fs` всегда отдаёт POSIX-ключи независимо от host-style-тега на `Path`,
  тогда как реальный диск на Windows — `\`). Нарушение → `PermissionDenied`; отсутствующий файл
  (после успешного лексического/symlink-чека) → `NotFound` от `canonicalize`/`read`, транзитом
  наружу.
- **`DirFs`/`EmbeddedDir` дают ОДИН и тот же ключ на один и тот же относительный путь** (dev==prod
  паритет путей, конвенция с `embed_dir`).
- **dev/prod-выбор — ветка на точке инстанциации, НЕ dyn-значение**: effectful-vtable-dispatch не
  поддержан (D122-амендмент выше) — существential `ReadFs` с эффектным методом (`DirFs.read_file`
  несёт `Fs`) потребовал бы vtable, которого нет. `if dev_mode { serve(mut mux, DirFs.new(...)) }
  else { serve(mut mux, embed_dir("...")) }` — один `if` мономорфизует `serve` дважды.

**Аддитивно, НЕ язык-меняюще**: `ReadFs` — ещё один std-протокол поверх готовой structural-protocol
+ mono-dispatch машины (та же, что несёт `io.Read`); новых языковых конструкций нет.

## D324 — os: `Os` effect (env / args / cwd / dirs / process) (Plan 176 Ф.3, 2026-07-06)

**Статус:** IMPLEMENTED (Ф.3). Модуль `std/os`. Тот же паттерн, что `Fs` (D323): тонкий int/str-primitive
эффект + pure-Nova обёртки, строящие `Option`/`Result`/`Path` вне effect-границы. Reuse `IoError` (Q3).
Subprocess (`Command`/`Child`/`spawn`) — НЕ здесь: под-план **176.1** (Q5).

### `Os` effect — ТОНКИЙ int/str-primitive слой (§3/§0)

```nova
type Os effect {
    arg_count() -> int;  arg_at(i int) -> str                              // argv (arg_at(0) = программа)
    env_get(key []u8) -> Option[str];  env_has(key []u8) -> bool           // значение (raw bytes-as-str) / наличие
    env_set(key []u8, val []u8) -> int;  env_remove(key []u8) -> int       // 0 / -errno
    env_len() -> int;  env_key_at(i int) -> str;  env_val_at(i int) -> str // snapshot-итерация vars
    cwd() -> str;  set_cwd(path []u8) -> int                               // "" = error; 0 / -errno
    temp_dir() -> str;  home_dir() -> str                                  // home "" = none
    exit(code int) -> int;  pid() -> int;  hostname() -> str               // exit: real не возвращается; mock записывает
}
```

- Как `Fs`: string-getter'ы несут **raw байты-as-`str`** (пустая строка == недоступно/ошибка), мутаторы →
  `0` / НЕГАТИВНЫЙ POSIX errno. Rich-типы (`Option`/`Result`/`Path`/`EnvVar`) строятся в `os.nv`-обёртках ВНЕ
  effect-vtable (которая стёрла бы их).
- **byte-first (Q1-прецедент):** env-ключи/значения и пути кросят как `[]u8` (handler NUL-терминирует их для C
  через `os_cstr`, зеркало fs `c_path`); `env_get` несёт байты verbatim → non-UTF-8 Unix env-значение
  round-trip'ит лосслесс через `env_bytes` (Rust `var_os`-прецедент). `str`-удобная форма (`env`) несёт
  те же байты (Go-модель).

### Public API (`os.nv`, все несут `Os`)

`args() -> []str` (argv, [0]=программа); `env(key str) -> Option[str]` / `env_bytes(key []u8) -> Option[[]u8]`
(перегрузка по арности; unset vs empty различимы); `has_env`; `env(key str, value str)` / `env_bytes(key []u8, value []u8)` / `remove_env -> Result[(), IoError]`;
`vars() -> []EnvVar` (snapshot, Go `os.Environ`/Rust `env::vars`); `cwd() -> Result[Path, IoError]` /
`cwd(Path) -> Result[(), IoError]`; `temp_dir() -> Path`; `home_dir() -> Option[Path]`; `exit_process(code int)` (flush
stdout/stderr + terminate; Go `os.Exit`/Rust `process::exit`; **имя `exit_process`** — bare `exit(code, msg)` —
язык-builtin D13); `pid() -> int`; `hostname() -> Result[str, IoError]`.

### Триада (плумбинг, мокабельность §1a)

**`real_os()`** — нативные хуки `nova_rt/os_env.h` (`getenv`/`setenv`/`_putenv_s`/`getcwd`/`chdir`/`getpid`/
`gethostname`/… — **non-blocking**, header-only static-inline, как `io_console.h`; НЕ libuv-park/wake — это для
реального блокирующего I/O); argv захватывается в `main()` через `nova_os_set_args(argc, argv)` (`int main(int
argc, char** argv)` — единственная точка эмиссии, `emit_c.rs`). **`mock_os(MockOs)`** — in-memory env/args/cwd
map; `exit` **записывается** (`did_exit()`/`exit_code()`), НЕ терминирует → наблюдаемо в тесте без убийства
харнесса; env-значения хранятся как raw `[]u8` + `str.from_bytes_unchecked` (byte-transparent, как real).

### Concurrency-контракт (§3c)

`env(key, value)`/`cwd(path)` мутируют process-global state → inherently racy (Rust сделал `set_var` `unsafe` в
1.84). Nova НЕ делает их unsafe, но документирует single-threaded-mutation контракт: мутировать env/cwd только в
setup, до спавна конкурентной работы, читающей их. Чтения (`env(key)`/`args`/`cwd()`) — безопасны.

### Реализационные ноты

1. **`os` зависит от `fs`** (для `Path`) — не цикл (`fs` не импортит `os`; `io` не импортит ни того ни другого).
2. **`exit_process`, НЕ `exit`** — bare `exit` = язык-builtin (D13, `-> never`, message-bearing abort).
3. **`cwd()`/`hostname()` ошибка → `IoError.from_os(0, op)`** (kind `Other`), а НЕ `IoError.of(ErrorKind.Other(0),
   …)`: `Other(int)` — payload-вариант, cross-module литерал-конструкция ловит checker-gap
   `[M-176-xmod-payload-variant-ctor]`; `from_os`/`kind_from_errno` строят `Other` ВНУТРИ `std.io`.
4. **Free-fn имена не коллидят** (coarse-by-name резолв, D323-нота #3): приватные хелперы `os_cstr`/`os_wrap_unit`
   (не `c_path`/`wrap_unit` — те в `std.fs`).

### Амендмент D324 (2026-07-07) — оп env_get -> Option; публичная поверхность перегрузка по арности

**Решение владельца:** Два уточнения:

1. **`env_get` в эффекте** → `Option[str]` (вместо `str` с сентинелём-""). Присутствие ключа (ранее определяемое через `env_has` снаружи опа) теперь решается ЗДЕСЬ: `match os_cstr(key) { Ok(ck) => if unsafe { os_env_has(ck.as_ptr()) } == 1 { Some(unsafe { os_env_get(ck.as_ptr()) }) } else { None }, Err(_) => None }`. Осмысление: сентинель-"" не различал `KEY=` (пусто) и отсутствие ключа; Option явен и безопасен (правило 4 стиля).

2. **Публичная поверхность — перегрузка по арности (канон D117-семьи):**
   - `env(key str) -> Option[str]` — чтение переименование с `get_env`; `env_bytes(key []u8) -> Option[[]u8]` с `get_env_bytes`.
   - `env(key str, value str) -> Result[(), IoError]` — писание; `env_bytes(key []u8, value []u8) -> Result[(), IoError]` с `set_env_bytes`.
   - `cwd() -> Result[Path, IoError]` — чтение с `current_dir`; `cwd(Path) -> Result[(), IoError]` — писание с `set_current_dir`.
   - `has_env`, `remove_env` остаются без изменений.

Реализация: `real_os()` обновлён в `std/os/os.nv`; `mock_os()` и `MockOs` в `std/os/mock.nv` возвращают `Option[str]` из `@mem_env_get`. Миграция: 7 вызовов `get_env` ← `env`, 3 `get_env_bytes` ← `env_bytes`, 8 `set_env` ← `env`, 1 `set_env_bytes` ← `env_bytes`, 7 `current_dir` ← `cwd`, 2 `set_current_dir` ← `cwd` (~28 мест в тестах; пуст в spec_tests/examples). Импорты обновлены.

## D357 — `Http` client transport seam (Plan 178 Ф.2, 2026-07-04)

**Решение.** HTTP-client — value-types + Nova-логика над тонким байт-seam'ом `Http`.
Триада (module-conventions): `type Http effect { send(host str, port int, secure bool, request str) -> Result[str, HttpError] }` + `real_http()` (над `Net`, `std.http.transport`) + `mock_http()` (in-memory, `std.http.client`).

- **Один hop = один `send`.** `request` — полностью сериализованные wire-байты (несомы как byte-`str` через `str.from_bytes_unchecked`, НЕ `[]u8` — `[]u8`-effect-op erasure, то же обоснование что net byte-surface Ф.0.5); возврат — сырые response-байты. Redirect-loop, auth-strip, keep-alive-решение, chunked-decode, парсинг — **Nova-логика клиента** (`std.http.client/wire.nv`,`client.nv`), НЕ в seam.
- **`real_http` — effect-over-effect:** handler-op body выполняет `Net` (resolve/connect/write/read); допустимо (эффект перформится при вызове op под активным `with Net`). CORE = `Connection: close` + read-to-EOF; `secure=true` (https) → `Err(Tls)` (🔴 gate Plan 116).
- **`mock_http` — data-driven** (`MockResponse` = raw-wire ИЛИ status+headers+body; `MockHttp.on(method,path,resp)`); диспатч через ТОТ ЖЕ `parse_response` → chunked/malformed покрыты без сокетов. `MockResponse.echo_request_header(name)` — детерминированная проверка отправленного (auth-strip).
- **Структура (ревизия §3 плана):** nested submodules `std.http.client`/`std.http.transport` вместо flat `std.http` — изолирует `std.net`+`json`-зависимости от lean message-model core (и обходит два pre-existing codegen-бага: forward-decl-return-type unit-closure-call в single-CU + handler-closure-env GC-root).
- **Установка mock:** канон = inline-handler `with Http = effect Http { send(..){ m.reply(request) } }` (frame-capture, conservative-GC-safe); `MockHttp.build()->Effect[Http]` объявлен, но heap-closure-env НЕ GC-rooted (`[M-178-mock-handler-gc-trace]`).

**Амендмент net byte-surface** — D173/D301 (см. Ф.0.5). **Gated:** timeout←173, decompress←179, typed json[T]←180, https/h2←116.

## D360 — HTTP client policies: redirect / auth-strip / status / transfer (Plan 178 Ф.2, 2026-07-04)

**Решение (CORE-приземлённое подмножество).**
- **Redirect:** `RedirectPolicy | NoFollow | Limited(int)` (default `Limited(10)`); превышение → `Err(HttpError{kind: TooManyRedirects(n)})`. GET-ify: 303 и (301/302 на не-GET/HEAD) → метод GET + тело/body-headers сброшены; 307/308 сохраняют метод+тело.
- **Cross-origin auth-strip (Q9, security-инвариант):** при hop в другой origin (`Url.@origin` (scheme,host,port) отличается) — `Authorization` и `Cookie` удаляются. pos-тест (cross→strip) + control (same-origin→preserve).
- **Status:** 4xx/5xx — **валидный `Response`, НЕ ошибка**; конверсия opt-in `Response.consume @error_for_status() -> Result[Response, HttpError]` (`Err(Status(code))` для не-2xx/3xx; CORE материализует+пересобирает Response, error-body дренится).
- **Transfer:** `Content-Length` (identity) + `Transfer-Encoding: chunked` (decode) — оба; двойной CL / CL+TE → `Err(Protocol)` (smuggling, RFC 9112 §6.1, через `HeaderMap.@content_length`). CR/LF/NUL/non-tchar в headers → reject (Ф.1).
- **Body:** must-consume (D133/D359); `.bytes()`/`.text()`/`.drain()`/`.json()`(dynamic JsonValue).

**Gated за CORE (маркеры, НЕ упрощения — `[M-178-client-policy-surface]`):** Proxy+CONNECT-tunnel / NO_PROXY-матрица (Q23), SSRF-guard (Q24), cookie-jar (Q10), idempotent-retry + pool-eviction (Q16), live keep-alive-reuse, 1xx-interim loop, TE:trailers, Expect:100-continue, auto-decompress (←179). Conformance-фикстуры d357/d360 отложены (compiler forward-decl баг `[M-178-conformance-d357-d360-forwarddecl-bug]`) → покрыто `nova_tests/http*`.

## D333 — codec-контракт `std/encoding/compress`: PURE-codec, byte-first, Result (Plan 179 Ф.1/Ф.3, 2026-07-04)

**Статус:** IMPLEMENTED (Ф.1 inflate + Ф.3 encode — pure Nova, БЕЗ C; brotli=D337 gated). Модуль-декларация — `module encoding.compress` (folder-module `std/encoding/compress`, рядом с `json`/`base64`/`utf16`).
**Класс-соседи:** io-core D322 / fs D323 (byte-surface stdlib) + fallible-канон D325.
**Нумерация:** D333 из reserved-диапазона D333–D339 (Plan 179; `README.md` §reservation; grep-verify коллизий=0). *Прежний план/код-комментарии указывали файл `spec/decisions/05-stdlib.md` — такого файла нет (аспирация; `05` занят `05-memory.md`); блок приземлён к классу-соседям в `04-effects.md`, ссылки в коде/тестах синхронизированы (§5).*

### Что

`std/encoding/compress` — **PURE-codec без эффекта**: ни I/O, ни effect-триады (нет `real_*`/`mock_*` — нечего мокать). Все fallible-операции возвращают `Result[T, CompressError]`; вход и выход — байты (`[]u8`), `str` в кодеке НЕ участвует. Единая форма сигнатур: decode = `fn(data, max_output)`, encode = `fn(data, level)`.

### Правило

- **(R1) PURE, no-effect (явное conventions-исключение).** Кодек — plain fallible-функции + coder-value, НЕ триада (module-conventions «PURE codec/serde need NO effect»). mock-тест НЕ обязателен. Зеркалит `json`/`base64`. Это **исключение, а НЕ violation** effect-триады (owner sign-off, §9 плана).
- **(R2) Byte-first.** Вход/выход строго `[]u8`; `str` не пересекает границу кодека (вызывающий делает `.to_bytes()`/`str.from_bytes(...)` сам). Соответствует net/io byte-surface (D302/D322).
- **(R3) D325-нейминг.** Bare-имена без `try_`: `inflate`/`zlib_decode`/`gzip_decode`/`deflate`/`zlib_encode`/`gzip_encode`. Fallible → `Result[T, CompressError]` (R1/R2 D325); `Fail[E]` наружу запрещён (R5 D325). `Option` — только genuine absence (streaming-EOF через `is_done()`, см. D335).
- **(R4) Единый структурный `CompressError`** (value-record) + **OPEN** `ErrorKind` (sum-type — wildcard-арм обязателен у потребителя):

  ```nova
  export type CompressError value { ro kind ErrorKind, ro offset Option[int] }
  export type ErrorKind
      | InvalidData(str) | UnexpectedEof
      | Checksum { ro kind ChecksumKind, ro expected u32, ro got u32 }
      | BadHeader(str) | Bomb(int) | UnsupportedMethod(str) | TrailingData | Other(str)
  export type ChecksumKind | Crc32 | Adler32 | Isize
  ```
  `Checksum` несёт фактические `expected`/`got` (диагностика); `@to_str()` — человекочитаемое описание. `offset` = байт-позиция во входе (`None` для framing/checksum).
- **(R5) Одна форма на направление.** Decode one-shot: `inflate(data []u8, max_output int)` / `zlib_decode(...)` / `gzip_decode(...)` → `Result[[]u8, CompressError]`. Encode one-shot: `deflate(data []u8, level CompressLevel)` / `zlib_encode(...)` / `gzip_encode(...)` → `Result[[]u8, CompressError]`.
- **(R6) `CompressLevel`** — `value`-record `{ priv n u8 }`, raw 0..11, интерпретация **per-codec**: `fastest()`=1 / `default()`=6 / `best()`=sentinel→9 / `none()`=0 / `new(n u8) -> Result[...]` (n>11 → `InvalidData`); **deflate 0..9** (`10..11` → `InvalidData`, «brotli-only» — D337). `priv`-поле читается только own-методом `@raw()` (cross-module `priv`-read через свободную функцию ловит `E_FIELD_MODULE_PRIVATE` на disk-loaded std-модуле — D220/D281).
- **(R7) Целочисленность.** Размеры — `int`; checksums/ISIZE — `u32`; ISIZE = `(uncompressed_len mod 2^32)` (D336). Bit-reader bounds-checked; distance>window → `InvalidData`. Incomplete-Huffman: единственный distance-code принимается (RFC 1951 §3.2.7, Q13). Trailing-data после BFINAL: raw/zlib strict → `TrailingData`; gzip lenient (multi-member).

### Почему

1. Кодек не касается среды — навязывать effect-триаду/mock значило бы фиктивный boilerplate; конвенция сама выводит PURE codec из-под mock-mandatory (§9).
2. Byte-first убирает вопрос «а где кодировка» и совместим с net/io/http-байтовым слоем (потребитель Plan 178 передаёт wire-байты напрямую).
3. Один `CompressError` + OPEN `ErrorKind` композируется (кладётся в `Result`, мапится), а не дробит обработку по под-форматам; wildcard-арм держит форму расширяемой.

### Что отвергнуто

- **effect-триада для кодека** (`Compress` effect + `real/mock`) — нечего мокать, чистая функция; отвергнуто как фиктивный слой.
- **`str`-API поверх байт** — лишняя кодировочная неоднозначность; отвергнуто в пользу `[]u8`.
- **per-формат отдельные error-типы** — раздувают match; отвергнуто в пользу единого OPEN `ErrorKind`.

### Связь

- [D325](#d325) (fallible Result-everywhere — нейминг/форма), [D322](#d322)/[D323](#d323) (byte-surface stdlib соседи), [D302](#d302) (net byte-surface).
- [D334](#d334) (bomb-cap decode-инвариант), [D335](#d335) (streaming coder), [D336](#d336) (checksum), [D337](#d337) (brotli C-FFI).
- [D133](02-types.md#d133) (must-consume — только `BrotliReader`), [D215](02-types.md#d215)/[D228](02-types.md#d228) (`value`-record `CompressLevel`), [D220](02-types.md#d220)/[D307](02-types.md#d307) (`priv`-поле).

### Эволюция

- 2026-07-04: приземлён по факту Ф.1 (inflate/gzip/zlib decode) + Ф.3 (deflate/gzip/zlib encode). **Landed-отклонения от плана:** (1) module-декларация `encoding.compress`, не `std.encoding.compress`; (2) файл-адрес D-блока = `04-effects.md`, не аспирационный `05-stdlib.md`; (3) encode БЕЗ `max_output` (§3.5 плана — «выход<входа», bomb на компрессии невозможен), несмотря на упоминание в task-prompt.

## D334 — bomb-cap: обязательный `max_output` decode-инвариант против decompression-DoS (Plan 179 Ф.1, 2026-07-04)

**Статус:** IMPLEMENTED (Ф.1). §8.0-critical. Инвариант — не опция.

### Что

Каждый **decode**-путь (one-shot + streaming + будущий brotli-FFI) обязан нести `max_output`. Превышение выхода **ИЛИ** прогресс-входа (anti-flood) → `Err(CompressError{kind: Bomb(limit)})` инкрементально — ДО аллокации сверх лимита, НЕ post-factum. Encode `max_output` НЕ несёт (см. Почему).

### Правило

- **(R1) See-it-in-the-signature.** `max_output` — обязательный параметр каждой decode-сигнатуры (`inflate(data, max_output)`, `Inflater.new(max_output)`, …). Вызов decode без него — **compile-error** (neg-фикстура `inflate(data)` без `max_output`), а не runtime-сюрприз.
- **(R2) Инкрементальная проверка.** Cap проверяется на КАЖДЫЙ выходной байт (`@emit`: `if max_output > 0 && total_out >= max_output → Bomb`), не после материализации всего выхода. Никакого OOM/hang до отказа.
- **(R3) Граница.** `output == max_output` → ok; **первый байт сверх** cap → `Bomb(limit)`. `total_out` — общий счётчик, разделяемый членами multi-member gzip (общий cap across членов).
- **(R4) Anti-flood.** Прогресс-вход (100k пустых gzip-членов, гигантский `FNAME`) капится тем же инвариантом / header-field-длиной → `Bomb`/`InvalidData`, а не unbounded-skip.
- **(R5) Escape-hatch.** `max_output == 0` = «без лимита» (low-level caller-trust). Plan 178 всегда передаёт реальный cap (`max_decompressed`, 100 MiB) — 0 не используется на HTTP-пути.
- **(R6) Encode без cap.** Encode-сигнатура строго `fn(data, level)` без `max_output`: выход компрессии ограничен ~размером входа (вход уже в памяти → амплификации/бомбы нет). Осознанное решение §3.5, НЕ пропуск.

### Почему

1. Decompression-bomb (малый вход → гигантский выход) — реальный DoS-вектор; cap в **сигнатуре** делает защиту невозможной к забыванию (прецедент: Zig `window_size_max`, Node `maxOutputLength`, zstd `window_size_max`).
2. Инкрементальность (не post-factum) — единственный способ не аллоцировать бомбу до отказа.
3. Encode симметрии cap не требует by-construction — навязывать его значило бы шум в API без инварианта.

### Связь

- [D333](#d333) (форма кодека), [D335](#d335) (streaming — `read(max_emit)` bounded-per-call как второй bound), [D325](#d325) (`Bomb` — вариант `CompressError`).
- Plan 178 `max_decompressed` → `max_output` (потребитель gate Q12).

## D335 — streaming incremental coder: `feed`/`read`/`finish` + SYNC-FLUSH + Plan 178 BodyReader-мост (Plan 179 Ф.1/Ф.3, 2026-07-04)

**Статус:** IMPLEMENTED (Ф.1 decode-readers + Ф.3 writers). Pure-Nova-кодеры — plain `value` (НЕ consume); consume только у `BrotliReader` (D337).

### Что

Инкрементальные кодеры-значения поверх того же ядра, что one-shot (→ streaming-по-1-байту == one-shot байт-в-байт by-construction). Decode: `Inflater`/`ZlibReader`/`GzipReader`. Encode: `Deflater`/`ZlibWriter`/`GzipWriter`. Контракт `feed`/`read`|`flush`/`finish`; окно/bit-leftover/checksum-state сохраняются между вызовами.

### Правило

- **(R1) Decode-reader** (`Inflater` образец; ZlibReader/GzipReader оборачивают его + framing):
  ```nova
  export fn Inflater.new(max_output int) -> Inflater          // plain value, НЕ consume (Q6)
  export fn Inflater mut @feed(chunk []u8) -> Result[(), CompressError]
  export fn Inflater mut @read(max_emit int) -> Result[[]u8, CompressError]   // bounded-per-call
  export fn Inflater @is_done() -> bool
  export fn Inflater mut @finish() -> Result[(), CompressError]
  ```
  - **Bounded-per-call:** `read` отдаёт ≤ `max_emit` байт (anti-single-huge-alloc — второй bound поверх bomb-cap D334).
  - **EOF-семантика:** пустой результат `read` = «пока нечего»: `is_done()==true` → поток завершён (clean EOF); иначе нужен ещё `feed`. `finish` пампит до конца, валидирует: незавершённый битстрим (нет BFINAL) → `UnexpectedEof`; мусор после BFINAL (raw strict) → `TrailingData`; финальный checksum/ISIZE — здесь.
  - **Landed-отклонение (амонд):** форма `read -> Result[[]u8, _]` + `is_done()` вместо планового `read -> Result[Option[[]u8], _]` — обход codegen-ограничения по `Option[Vec[u8]]`; семантика D335 (EOF ≠ need-more) сохранена через `is_done()`.
- **(R2) Encode-writer** (`Deflater` образец):
  ```nova
  export fn Deflater.new(level CompressLevel) -> Deflater      // value, НЕ consume
  export fn Deflater mut @feed(chunk []u8) -> Result[[]u8, CompressError]
  export fn Deflater mut @flush() -> Result[[]u8, CompressError]
  export fn Deflater mut @finish() -> Result[[]u8, CompressError]
  ```
- **(R3) SYNC-FLUSH.** `@flush` = byte-align + пустой stored-блок (маркер `00 00 FF FF`); после flush накопленный выход — **decodable-префикс** (декодится в ровно скормленный вход). Основа SSE/chunked поверх gzip/deflate (прецеденты Go `Writer.Flush`, Node `Z_SYNC_FLUSH`). В decode такие interleaved-маркеры прозрачно глотаются (interop с Go/Node-стриминг-серверами).
- **(R4) Multi-member (gzip).** `is_done`/read→пусто = конец ВСЕХ членов; граница члена в V1 не наблюдаема (single-member opt-out — followup §11). Общий bomb-cap across членов (D334 R3).
- **(R5) Plan 178 BodyReader-мост.** `BodyReader.@next_chunk` → `feed` → `read(max_emit)` — фиксируется здесь как контракт auto-decompress (`Content-Encoding` gzip/deflate). Compress НЕ импортирует `std.http` (glue живёт в `real_http`, §3.4 плана).

### Почему

1. One-shot строится поверх streaming-ядра → нет двух реализаций и «== one-shot» гарантируется конструктивно.
2. `read(max_emit)` — единственный способ обслужить bounded-memory-стриминг (feed 32 KB → распухание под cap с ограниченной резидентной памятью).
3. SYNC-FLUSH — обязателен для интерактивных потоков (SSE); без него gzip-стриминг буферизует до `finish`.
4. Pure-Nova-кодеры не держат внешний ресурс (GC-окно) → plain value, без must-consume долга; только brotli держит C-instance → consume (D337).

### Связь

- [D333](#d333) (кодек-форма), [D334](#d334) (bomb-cap — `read` bounded-per-call второй bound), [D337](#d337) (`BrotliReader` consume).
- [D133](02-types.md#d133) (must-consume — контраст: pure-кодеры НЕ consume), [D228](02-types.md#d228) (`value`-record coder), Plan 178 D357/D360 (BodyReader-потребитель).

## D336 — checksum-контракт: CRC-32 (gzip) / Adler-32 (zlib) / ISIZE verify-by-default (Plan 179 Ф.1, 2026-07-04)

**Статус:** IMPLEMENTED (Ф.1). Модуль `encoding.compress`, файл `checksum.nv`. CRC-32 промоут из `std/_experimental/checksums/crc32.nv` (free-function-форма as-is, Q15, owner sign-off 2026-07-03); Adler-32 — NEW.

### Что

Целостность декодированного потока проверяется **по умолчанию**: gzip несёт CRC-32 + ISIZE, zlib — Adler-32. Несовпадение → `Err(CompressError{kind: Checksum{kind, expected, got}})`. Checksum-функции экспортируются самостоятельно (integrity, PNG, ETag).

### Правило

- **(R1) CRC-32** (IEEE 802.3, reversed poly `0xEDB88320`) — gzip trailer. `crc32(data []u8) -> u32` + incremental `crc32_init`/`crc32_update`/`crc32_finalize` (init `0xFFFFFFFF`, finalize XOR `0xFFFFFFFF`). Вектор: `crc32("123456789".to_bytes()) == 0xCBF43926`.
- **(R2) Adler-32** (RFC 1950 §9, mod `65521`) — zlib trailer. `adler32(data []u8) -> u32` + incremental `adler32_init`(=1)/`adler32_update`/`adler32_finalize`(identity). Вектор: `adler32("Wikipedia".to_bytes()) == 0x11E60398`.
- **(R3) ISIZE** — gzip: `(uncompressed_len mod 2^32) == ISIZE` (НЕ raw-длина; >4 GiB честно wrap'ается mod 2^32 и НЕ ложно-Checksum).
- **(R4) Verify-by-default.** Trailer сверяется при `finish`/one-shot; mismatch → `Checksum{kind: Crc32|Adler32|Isize, expected, got}` (несёт фактические значения). `ChecksumKind` различает источник.
- **(R5) Таблицы CRC — runtime-lazy** (`crc32_table_value`); comptime-const-array — followup §11 (обход, НЕ упрощение семантики).

### Почему

1. Silent-truncate без checksum-verify — реальный класс багов (bit-flip/усечение проходят молча); verify-by-default бьёт «декодировали мусор как валидное».
2. `Checksum{expected, got}` несёт значения — диагностируемо (какой байт/сумма разошлись), а не «просто ошибка».
3. ISIZE mod 2^32 — точная семантика RFC 1952 (иначе >4 GiB ложно-fail).

### Связь

- [D333](#d333) (`Checksum` — вариант `CompressError`), [D334](#d334) (verify на том же decode-пути).
- Промоут `_experimental/checksums/crc32.nv` (Q15); Adler-32 NEW.

## D337 — brotli C-FFI-контракт (Plan 179 Ф.2)

> ✅ **LANDED (2026-07-06) — decode (one-shot).** `[M-179-brotli-vendor-lib]` снят: google/brotli v1.2.0 **декодер** собран (`common/` + `dec/`, MSVC x64 `/MT /O2`) и вендорен как **заголовки + статическая lib** (без исходников — стиль libuv): headers `compiler-codegen/nova_rt/brotli/include/brotli/{decode,types,port,shared_dictionary}.h`, lib `compiler-codegen/nova_rt/brotli/lib/libbrotlidec.lib` (+ build-cache `target/brotli-cache/`). `brotli_decode(data, max_output)` работает на официальных RFC 7932-векторах (`tests/testdata/*.compressed`), bomb-cap инкрементально поверх FFI, ошибки типизированы. **Streaming `BrotliReader` (R2) — deferred `[M-179-brotli-reader-streaming]`** (см. «Реальность»).
>
> **Условная линковка (ключевой факт, аменд §5).** libuv — **mandatory** (линкуется ВСЕГДА, Plan 22 F2). brotli — **CONDITIONAL**: lib попадает в команду компоновки ТОЛЬКО когда CU реально ВЫЗЫВАЕТ декодер. Механизм — «модуль→библиотека», введён этим планом (libuv не тронут): (1) shim `nova_rt/brotli_shim.{h,c}` — прототипы всегда `#include`-аются в `nova_rt.h` (пустая зависимость), (2) `brotli_shim.c` компилируется и `libbrotlidec.lib` линкуется в `test_runner.rs` `build_command` лишь если генерённый `.c` содержит **call-site** к `brotli_decode` (не просто определение — std-fn'ы эмитятся даже мёртвыми; детектор фильтрует forward-decl/definition-header), (3) при `NOVA_USE_BROTLI` shim = реальный декодер, без — **feature-gate-заглушки** (`dec_new`→0 → `UnsupportedMethod`, НЕ link-error, Q11). Доказано: программа с `gzip_decode` но без brotli → `uses_brotli=false` → lib НЕ линкуется; brotli/http-`br` → линкуется.

### Что

brotli-decode — **C-FFI к `libbrotlidec`** (НЕ pure-Nova V1: 120 KB встроенный словарь + нет в Zig-std → nv-sourcing не feasible; C-FFI by-necessity, как net/libuv). Тонкий Nova-API `brotli_decode(data, max_output)` + streaming `BrotliReader` поверх C-instance.

### Правило (контракт при материализации)

- **(R1) FFI в `ffi.nv`, `extern "C"`.** Extern-сигнатуры — **C-ABI без `[]u8`/GC-типов** (raw-ptr+len / out-буфер), per D355 (ex-D282) + координация Plan 174.6 M1 (`E_FFI_NON_C_ABI_TYPE`; в std нет ни одного extern с `[]u8` — grep=0).
- **(R2) `BrotliReader` — `consume value`** (D133): держит C-instance, `@finish`/consume освобождает его. **Единственный consume-кодер** в модуле (pure-Nova-кодеры D335 — plain value). Не-consume `BrotliReader` → `EXPECT_COMPILE_ERROR`; double-consume/use-after-consume — тоже compile-error.
- **(R3) Bomb-cap-over-FFI** (D334): output-cap инкрементально поверх C-стрима (`max_emit`-капинг per read). Window ≤16 MiB (lgwin ≤24, фикс-bounded) — **output-cap ≠ window-cap** (документируется отдельно; критик-gap).
- **(R4) Error-маппинг.** `brotli_dec_error` → `CompressError`: truncated → `UnexpectedEof`; malformed → `InvalidData(str)`; превышение cap → `Bomb`; `UnsupportedMethod` при отсутствии C-фичи.
- **(R5) Level-резерв.** `CompressLevel` 10..11 зарезервированы за brotli (deflate их отвергает, D333 R6). Encode-brotli — followup §11 (asymmetric, vendor `enc/` не тащится в V1).

### Почему

1. brotli requires 120 KB dictionary + сложный decoder — pure-Nova V1 не feasible; C-FFI by-necessity (nv-sourcing даёт .nv где возможно, тяжёлый native → FFI, прецедент libuv).
2. C-instance — внешний ресурс → `consume` (D133) обязателен: release-долг виден в типе, no silent leak.
3. Output-cap ≠ window-cap: lgwin ограничивает окно, но выход всё равно надо капить инкрементально (иначе DoS через большой валидный выход).

### Реальность (landed vs deferred, 2026-07-06)

- **`brotli_decode(data, max_output)` — LANDED.** Реализован в `std/encoding/compress/brotli.nv` поверх стрим-шима: `new` → `feed`(весь вход, копируется в шиме) → цикл `pull`(bounded-budget) с инкрементальным bomb-cap → `free` на КАЖДОМ пути выхода. Бомба ловится по границе (`output == max_output` ok, первый байт сверх → `Bomb`); budget = `min(64 KiB, max_output − total + 1)` → перебор ≤1 байта, память bounded. Extern-сигнатуры (R1) — `nova_brotli_dec_{new,feed,pull,done,needs_input,error,free}` с `*u8`/`*mut u8`+`int` (модель fs `fs_read`/`fs_write`), НИ ОДНОГО `[]u8` (grep=0).
- **`BrotliReader` streaming (R2) — DEFERRED `[M-179-brotli-reader-streaming]`.** C-примитивы шима (`feed`/`pull` инкрементально) её поддерживают — это тонкая Nova-обёртка `consume`-типа. Отложена сознательно: owner-deliverable Ф.2 = `brotli_decode`, а http-auto-decompress (единственный потребитель `br`) использует **one-shot** (симметрично `gzip_decode`/`zlib_decode` в `finalize_response`). НЕ tech-debt-без-плана — followup с rationale; consume-neg-тест приземлится вместе с ней.

### Связь

- [D333](#d333) (кодек-форма), [D334](#d334) (bomb-cap-over-FFI), [D335](#d335) (streaming — контраст consume vs plain value), [D282](#d282)/D355 (ex-D282, extern "C" C-ABI FFI), [D133](02-types.md#d133) (must-consume).

---

## D423. Арифметическая overflow-политика: примитив `@overflowing_*` + trap-дефолт для ВСЕХ `Ints` + `.nv`-бланкеты политик (Plan 206 Ф.0/Ф.1/Ф.1b/Ф.2, 2026-07-15)

**Статус:** закреплён 2026-07-15 (owner sign-off, Plan 206 наблюдение 2026-07-14 + решение 2026-07-15). Ф.2 (`checked_*`/`saturating_*`/`wrapping_*` бланкеты + миграция overflow-зависимого std-кода) landed той же волной 2026-07-15 — см. §R4. Amends [D310](02-types.md#d310-type-set-bounds-plan-1723) (`Ints` — full-union exemption от `E_TYPE_SET_MIXED_SIGNEDNESS`), расширяет [D13](#) (trap-дефолт `+`/`-`/`*`, ранее только для безграничного `int`) на ВСЕ sized-типы. `div`/`neg`/`mod` — вне рамок, вынесены в подплан [206.1](../../docs/plans/206.1-div-neg-trap.md).

### Что

**Мотив:** `Duration.checked_*`/`saturating_*` (`std/src/time/duration.nv`) вручную дублируют overflow-детект (`if b>0 && a>i64_max()-b {None}`), второе окно overflow-логики рядом с `nova_int_checked_add` (`__builtin_add_overflow`, `effects.h:1044`). Хуже: `emit_c.rs` до этой правки лоуэрил checked-форму (`nova_int_checked_*`) ТОЛЬКО для безграничного `int` — sized-типы (`i8`..`i64`, `u8`..`u64`) шли в сырой C `+`/`-`/`*` (signed = C-UB, unsigned = тихий wrap). Обзор прайор-арта (2026-07-15): Swift — всегда-трап (все размеры); Rust — debug-паника/release defined-wrap; Zig — safe-паника/fast-UB; Go — always defined-wrap; C — signed-UB. Nova была ЕДИНСТВЕННОЙ, кто оставлял signed-overflow как UB.

**Ключевая идея — один примитив, пять исходов.** `__builtin_*_overflow` пишет обёрнутый результат ВСЕГДА (даже при overflow), т.е. один интринсик даёт пару `(wrapped, overflowed)`, из которой выводимы: trap (паника при overflow), `checked` (→`Option`), `saturating` (клампит), `wrapping` (игнорирует флаг), `unchecked` (не проверяет, ОТЛОЖЕН — сырой `a+b` без trap, unsafe, реализуется только по реальному запросу).

### Правило

**(R1) Type-set `Ints`.** `std/src/prelude/protocols.nv`: `type Ints set i8|i16|i32|i64|int|u8|u16|u32|u64|uint` — полное объединение `SignedInt ∪ UnsignedInt`. Amends D310 §«Знаковость»: тот запрещает ЧАСТИЧНЫЙ signed/unsigned микс (`u64.MAX ∉ i64` — несовместимые value-domains для произвольного подмножества); ПОЛНОЕ объединение (ровно `SignedInt ∪ UnsignedInt`, без пропусков) — отдельно разрешённый случай, т.к. per-member монорфизация (D310 §«Семантика тела») уже резолвит `T.MAX`/`T.MIN` per-instance, и знак-агностичные сравнения (`rhs < 0`) для unsigned членов константно `false` — никакого междоменного сравнения не возникает. Тот же случай, что иллюстративный `AnyNumber` в самом тексте D310. Чекер (`E_TYPE_SET_MIXED_SIGNEDNESS`, `types/mod.rs`) пропускает full-union; партиальный микс (`{i32, u32}` и т.п.) остаётся ошибкой.

**(R2) Примитив `@overflowing_add`/`@overflowing_sub`/`@overflowing_mul`.** Компиляторный интринсик (НЕ выразим в `.nv` — нужен аппаратный флаг переполнения): `fn[T Ints] T @overflowing_add(rhs T) -> (T, bool)` (аналогично `_sub`/`_mul`). Лоуэринг — прямой `__builtin_{add,sub,mul}_overflow(recv, rhs, &wrapped)`, результат — пара `(wrapped, overflowed)` через `_NovaTuple_2_...`-мономорфизацию (тот же механизм, что `(a, b)`-литерал, `register_mono_tuple`). Резолвится как обычный `@`-метод на ЛЮБОМ `Ints`-примитиве (per-type подстановка `__builtin` по ширине/знаку C-типа приёмника — существующая codegen-инфраструктура method-dispatch на примитивах, D109-класс, НЕ generic-extern-с-type-set-bound декларация — той машинерии в компиляторе ещё нет, см. «Неопределённости»).

Именование — прайор-арт, не самодеятельность: Rust `overflowing_add -> (T, bool)`, Swift `addingReportingOverflow -> (T, Bool)`, Zig `@addWithOverflow -> .{result, u1}`, LLVM `llvm.sadd.with.overflow.iN -> {iN, i1}`. Семейство `checked`/`saturating`/`wrapping`/`unchecked` — тоже Rust-имена (внутренний прецедент: `std/src/runtime/sync.nv` атомики уже Rust-именованы — `compare_exchange`, `fetch_add`).

**(R3) Trap-дефолт для ВСЕХ `Ints` (вариант A, Swift-модель).** `+`/`-`/`*` трапят на переполнении для КАЖДОГО члена `Ints` (не только безграничного `int`, как раньше) — `nova_<T>_checked_add/sub/mul` per sized-тип (`effects.h`, зеркало `nova_int_checked_add`), лоуэринг типизированного `+`/`-`/`*` в них (было — сырой C-оператор). Unsigned overflow (переполнение вверх) тоже трапит — `__builtin_add_overflow` ловит его так же, как signed. Обоснование: (1) консистентность с уже-трапящим `int`; (2) закрывает signed-C-UB; (3) always-on-safety БЕЗ Rust'ова release-wrap-компромисса — есть Z3-элизия (`--contracts=optimized`, D140.4/[194](09-tooling.md#d421)), снимающая доказуемо-безопасные проверки. Перф-паритет там, где overflow доказуемо избыточен.

**(R4) Политики-обёртки (Ф.2, LANDED 2026-07-15).** ТРИ `.nv`-бланкета над `@overflowing_*` (`std/src/prelude/protocols.nv`, сразу после `type Ints`), каждый `fn[T Ints]` (один бланкет вместо ×2 SignedInt/UnsignedInt): `@checked_add/_sub/_mul(rhs T) -> Option[T]` (overflow → `None`), `@wrapping_add/_sub/_mul(rhs T) -> T` (модульно, флаг игнор), `@saturating_add/_sub/_mul(rhs T) -> T` (клампит по **op-специфичной** формуле направления — add: `rhs>0 ? MAX : MIN`; sub: `rhs<0 ? MAX : MIN`; mul: `(a<0)==(rhs<0) ? MAX : MIN`; все три формулы автоматически корректны и для unsigned, т.к. `x<0` у unsigned константно `false`). `@unchecked_*` — ОТЛОЖЕН (сырой `a+b` без trap, требует отдельного компиляторного лоуэринга, не `.nv`; во многом дублирует `--contracts=optimized`).

**Миграция overflow-зависимого std-кода на `wrapping_*` (Ф.2).** Trap-дефолт (§R3) ловит переполнение корректно, но ломает код, которому wraparound нужен ПО СПЕЦИФИКАЦИИ алгоритма (модульная арифметика). Мигрированы: `std/src/testing/handlers.nv` (`seeded` — production xoshiro256++/splitmix64 PRNG) + его inline-репродьюсер `spec_tests/conformance/inline_xoshiro_determinism.nv`; `std/src/checksums/fnv.nv` (FNV-1a `hash32`/`hash64`/`Fnv64State.update` — было RED, `RUN-FAIL … integer overflow: *`); `std/src/collections/bloom_filter.nv` (внутренний FNV-подобный `hash1`/`hash2` + double-hashing комбинация в `@insert`/`@contains`); `std/src/crypto/md5.nv`/`sha1.nv`/`sha256.nv` (mod-2^32 compression-функция по RFC 1321/FIPS 180-4 — `a+b+c+…` заменено на цепочку `.wrapping_add`, `& 0xFFFFFFFF`-маски оставлены как явная документация домена; `total_len * 8`→`.wrapping_mul(8)`, длина-в-битах по спецификации берётся mod 2^64). Только МОДУЛЬНАЯ по спецификации арифметика мигрирована — byte-counting `total_len +=` (накопление реального размера, не рассчитано на wrap) оставлено как есть там, где overflow означал бы genuine misuse, не намеренный wraparound (за исключением `md5`/`sha1`/`sha256` — там `total_len` тоже мигрирован, т.к. RFC/FIPS явно определяют длину как mod 2^64).

**Известные codegen/checker-разрывы, найденные при написании бланкетов/тестов (Ф.2, НЕ фиксятся в рамках 206 — pre-existing, отдельно трекнутые):**
- Chaining `.checked_add`/… НАПРЯМУЮ на primitive type-conversion CALL (`i32(10).checked_add(5)`) бьётся в `[P67-LEGACY] method call return type unknown` ICE (`emit_c.rs:51424`) — obj_ty не выводится, когда receiver generic type-set-bound бланкета сам является Call-выражением. НЕ бьётся на plain-ident/field/index/cast/free-function-call receiver (`a.checked_add(5)`, `w[i].wrapping_add(x)`, `(x as u64).wrapping_mul(y)`, `f().wrapping_add(y)` — все ОК). Обход в тестах: типизированная локаль перед вызовом. Тот же класс дыр, что остальные `P67-LEGACY` (Plan 196.5 Stage-D — активная отдельная чистка).
- `Option[T] == Some(int-литерал)` для sized не-`int` T не адаптирует литерал к `T` (`NovaOpt_nova_int` vs `NovaOpt_int32_t` CC-FAIL) — уже зарегистрированный pre-existing `[M-option-eq-some-literal-elem-adapt]` (`docs/plans/backlog-followups.md`, OPEN, Plan 172.2, P2). Обход — тот же (типизированная локаль внутри `Some(...)`).
- Тестировать `.nv`-объявления внутри `std.prelude.*` (auto-import global prelude отключён там, cycle protection) ломает `assert()`-инфраструктуру (`Nova_StringBuilder` struct-tag CC-FAIL) — до Ф.2 в `prelude.*` не было ни одного `*_test.nv`. Тесты бланкетов размещены рядом с numeric/math-поверхностью (`std/src/math/overflow_policy_test.nv`), не рядом с `protocols.nv`.
- `spec_tests/conformance` — 970 файлов ОДНИМ логическим модулем (`module spec_tests.conformance`, все co-equal) → `nova test`/`nova check` на ЛЮБОМ отдельном файле в этой папке разрешает и компилирует ВЕСЬ каталог (десятки минут), точечный per-файл прогон невозможен. `spec_tests/soundness/**` — каждый файл свой уникальный модуль, точечный прогон быстрый (~25-30s).

**(R5) Duration мигрирует (Ф.3, отдельная волна).** `std/src/time/duration.nv` приватные ручные `checked_add_i64`/`saturating_*` (`i64`) делегируют `i64 @checked_add`/`@saturating_add`; ручные range-проверки удаляются (дедуп). Публичный `Duration.checked_add`/… — байт-паритет поведения (D317-тесты δ0).

**(R6) Вне рамок.** `div`/`mod`/`neg` — иной механизм (у `__builtin` нет div-overflow; div-by-zero сейчас SIGFPE), подплан [206.1](../../docs/plans/206.1-div-neg-trap.md). `f64`-арифметика не затронута (уже отдельная trap-политика, D317). Z3-элизия сама не меняется (её МЕХАНИЗМ, `overflow_site_elided`, span-based — распространяется на sized-типы автоматически тем же вызовом; см. «Неопределённости» насчёт полноты Z3-стороны доказательства для sized-ширин).

### Неопределённости / известные разрывы (зафиксировано честно, 2026-07-15)

- **Ф.1 `@overflowing_*` — dispatch НЕ через generic-extern-с-type-set-bound.** У компилятора нет прецедента `extern "nova" fn[T Bound] ...` (D310 `T.parse` из примеров спеки — НЕ реализован, Plan 174.1 отложен). Реализация — D109-класс hardcoded existence+emission (как `.hash()`/`.clone()`/`.abs()` до миграции на `.nv`), НЕ полноценная checker-уровня generic-сигнатура в `method_table`. Практическое следствие: return-type/arg-type checking для `.overflowing_*` на checker-стороне слабее, чем для обычного `.nv`-декларированного метода (полагается на codegen-side `infer_expr_c_type`, а не на `method_table`-резолв сигнатуры). Достаточно для прямых вызовов на конкретных примитивах и для мономорфизированных generic-тел; НЕ протестировано на глубокую checker-диагностику (типа неверной арности) — предмет Ф.2 hardening при переходе на `.nv`-обёртки.
- **Z3-элизия sized-путей.** `overflow_site_elided`/`index_site_elided` — span-based механизм, применяется к binop-выражению независимо от типа операнда; технически покрывает sized-типы автоматически ТЕМ ЖЕ вызовом, что и `int`. Полнота Z3-СТОРОНЫ доказательства (кодирует ли верификатор sized-ширины настолько же полно, как безграничный `int`) — НЕ проверена этой волной; если sized-путь СМТ-кодируется хуже (например, verifier трактует sized как безграничный int и теряет wraparound-границы), элизия может быть излишне консервативной (недостаток перфоманса, не корректности) либо (хуже) излишне агрессивной. Пин-тесты этой волны проверяют ТОЛЬКО «trap срабатывает / не срабатывает при обычной арифметике», не корректность Z3-элизии на sized. Followup: `[M-206-sized-z3-elision-audit]`.

### Связь

[D310](02-types.md#d310-type-set-bounds-plan-1723) (type-set bounds, amended §R1) · [D13](#) (int overflow trap, расширен §R3 на все `Ints`) · [D317](#d317) (Duration overflow 3-tier — параллельная, отдельная от int-примитива область, мигрирует в Ф.3 §R5) · [D140.4/194](09-tooling.md#d421) (Z3-элизия `--contracts=optimized`, §R3/§«Неопределённости») · [D129](02-types.md#d129)/[reference-nova-int-intptr-not-i64] (`int`=`nova_int`=`intptr_t` ≠ `i64`=`int64_t`, оба члены `Ints`) · Plan [206](../../docs/plans/206-arithmetic-overflow-policy.md) · Plan [206.1](../../docs/plans/206.1-div-neg-trap.md) (div/neg — реализовано в [D427](#d427) ниже).
- Plan 174.6 M1 (`E_FFI_NON_C_ABI_TYPE`), Plan 178 (закрывает `br`-ветку auto-decompress — LANDED, `Content-Encoding: br` → `brotli_decode`; `Accept-Encoding` дополнен `br`).

## D427. `div`/`mod`/unary-`neg` always-on trap-guard + `.nv`-бланкеты политик (Plan 206.1, 2026-07-16)

**Статус:** закреплён 2026-07-16 (owner go, P1 — деление на ноль ранее было сырым C `/` → SIGFPE, неконтролируемый крэш процесса). **Amends** D423 (расширяет trap-дисциплину на `div`/`mod`/unary-`neg`, которые D423 §R6 явно вынес за рамки) и D13 (int overflow trap — теперь div-by-zero/div-overflow/neg-overflow тоже всегда-on для `int`, не только `+`/`-`/`*`). **Другой механизм, чем D423:** у `__builtin_*_overflow` нет div/neg-варианта — guard пишется как обычное сравнение, не аппаратный флаг.

### Что

**Мотив:** `Div`/`Mod`/унарный `neg` лоуэрились в сырой C `/`/`%`/`-` БЕЗ guard'а (`emit_c.rs`, все Binary/Unary арм для `Ints`). Деление на ноль = UB → на x86 `#DE` → **SIGFPE**, неконтролируемый крэш процесса без диагностики (не просто UB — реальный частый crash-вектор). `T.MIN / -1` (частное не влезает в тип) и `neg(T.MIN)` (унарный минус) — тот же класс: overflow-UB для signed sized-типов и `int`.

### Правило

**(R1) Always-on guard для `/`/`%` (все `Ints`, обе сигнатуры знака).** Перед делением:
- `b == 0` → **паника** `"division by zero"` (ВСЕГДА, во всех режимах contracts; это домен-ошибка, не overflow — трапит одинаково для signed И unsigned).
- **signed** `a == T.MIN && b == -1` → **паника** `"division overflow"` (частное вне диапазона). Не применимо к unsigned (делимое/делитель unsigned не может overflow-ить при делении — MIN=0, `0/x` всегда представимо). На x86 `idiv` вычисляет частное И остаток ОДНОЙ инструкцией и фолтит на overflow частного независимо — поэтому `%` получает ТОТ ЖЕ guard, что и `/`, даже хотя истинный остаток для этой пары математически равен 0.
- Иначе — обычный `/`/`%`.

Лоуэринг (`emit_c.rs`): `nova_<T>_checked_div`/`nova_<T>_checked_rem` (`nova_rt/effects.h`), зеркало `nova_<T>_checked_add/sub/mul` (D423 §R3) для КАЖДОГО члена `Ints` (`nova_int` — отдельный хардкод-арм, как и там; sized — общий `sized_checked_helper`, расширенный на `BinOp::Div`/`Mod`). Три сайта лоуэринга: главный `Binary`-арм `emit_expr`, typed-target propagation (`emit_expr_with_target_type`), compound-assign `/=` (`AssignOp::Div` — в языке НЕТ `%=`, только `Div`-вариант `AssignOp`). Unsigned-хелперы (`NOVA_DEFINE_CHECKED_UNSIGNED_DIVMOD`) содержат ТОЛЬКО `b==0`-guard — MIN/-1-ветка структурно отсутствует (не «всегда false», а буквально не сгенерирована), что делает деление unsigned overflow-guard бесплатным.

**(R2) Always-on guard для унарного `-x`, SIGNED-ONLY.** `x == T.MIN` → **паника** `"negation overflow"`; иначе обычный `-x`. Unsigned `-x` НЕ трогается (не добавляется guard) — unsigned negation уже полностью определена C-стандартом как `(2ⁿ − x) mod 2ⁿ` (two's-complement wraparound), никогда не UB, так что нет UB-дыры для закрытия. Раскрытая до 206.1 разница с сигнатурой `Ints` (полный union signed+unsigned) не проблема: guard применяется на C-типе receiver'а — `sized_checked_neg_helper` возвращает `None` для unsigned C-типов, оставляя сырой `-x` байт-идентичным. Три сайта: главный `Unary`-арм `emit_expr`, typed-target propagation (`emit_expr_with_target_type`) — компилятор не трогает file-scope `const`-инициализаторы (`emit_const_expr_typed`) и diagnostic-only `expr_to_display` — первое компилируется C-компилятором (не Nova runtime, другой risk-профиль, div-by-zero там уже ловится `E_CONST_FN_DIV_ZERO` в `const_fn_eval.rs` до попадания в codegen), второе никогда не эмитит исполняемый код (текст сообщения контракта).

**(R3) `.nv`-бланкеты политик (`std/src/prelude/protocols.nv`, `fn[T Ints]`, ЕДИНЫЙ бланкет и для signed, и для unsigned — как в D423 §R4, НЕ split SignedInt/UnsignedInt).** `@overflowing_div`/`@overflowing_neg`-компиляторный интринсик (D423 §R2 паттерн) сюда НЕ применяется — у деления/negation нет аппаратного флага, поэтому бланкеты написаны напрямую на `.nv`, не как обёртка над generic-интринсиком:
- `@checked_div(rhs T) -> Option[T]` / `@checked_rem(rhs T) -> Option[T]` — `None` на `rhs==0` ИЛИ на единственном overflow-случае; `Some(...)` иначе.
- `@wrapping_div(rhs T) -> T` / `@wrapping_rem(rhs T) -> T` — **паникуют на `rhs==0`** (у division-by-zero нет wrap-семантики, тот же контракт что голый `/`/`%` — прайор-арт Rust: `wrapping_div`/`wrapping_rem` там ТОЖЕ паникуют на `/0`, wrap применяется ТОЛЬКО к `T.MIN/-1`); единственный overflow-случай wrap'ается в `T.MIN` (div) / `0` (rem — истинный математический остаток).
- `@checked_neg() -> Option[T]` / `@wrapping_neg() -> T` — единое тело для ОБОИХ знаковостей (см. §R4 — reformulation через `@overflowing_sub`, не прямая проверка `x==T.MIN`, у которой знак инвертирован между signed/unsigned).

Именование — прайор-арт Rust (`checked_div`/`checked_rem`/`wrapping_div`/`wrapping_rem`/`checked_neg`/`wrapping_neg`), тот же принцип, что D423 §R2 «Именование».

**(R4) Реализационная находка: единый `Ints`-бланкет для div/neg НЕ может использовать буквальный литерал `-1` или прямую signed/unsigned-ветку — два независимых, эмпирически подтверждённых препятствия (Plan 206.1 progress log):**
1. **Негативный литерал в unsigned-моно.** Чекер (D227 Rule 4, `types/mod.rs`) жёстко отклоняет `-1`, СРАВНИВАЕМЫЙ/ПРИСВАИВАЕМЫЙ unsigned-члену — `rhs == -1` в теле `fn[T Ints]` компилируется ПОВТОРНО на каждый конкретный член `T`, и падает при монmorphизации на `u8`/…/`uint`. Решение: детект overflow-случая НЕ через `rhs == -1`, а через `rhs.negate() == 1` — вычисляется существующим hardware-checked `@overflowing_sub` интринсиком (`0 - rhs`, receiver строится как `rhs - rhs` чтобы избежать cast-receiver'а — см. п.2), который для unsigned НИКОГДА не даёт `(1, false)` (unsigned `checked_neg` успешен только на 0) — т.е. лишний guard-код математически no-op для unsigned, а не «условно пропущен» веткой.
2. **Прямая signed/unsigned-ветка для `neg` инвертирована, не «схлопывается».** В отличие от `saturating_*` (D423 §R4, где `rhs < 0` константно `false` для unsigned и формула КОРРЕКТНО схлопывается), условие `x == T.MIN` для `checked_neg` буквально ПРОТИВОПОЛОЖНО между signed (overflow ⟺ `x==MIN`) и unsigned (overflow ⟺ `x != 0`, т.е. `x != MIN`) — Rust имеет РАЗНУЮ логику `checked_neg` per-signedness. Единое тело возможно ТОЛЬКО через reformulation в терминах уже-корректного (per D423) `@overflowing_sub`-флага: `checked_neg(x) = (0).checked_sub(x)` даёт ОБА случая правильно из ОДНОГО hardware-флага (`__builtin_sub_overflow`), без явной signed/unsigned-ветки в `.nv`.
3. **Nested `.nv`-blanket-в-blanket call — отдельный, pre-existing ICE** (НЕ specific для div/neg): вызов ОДНОГО `.nv`-written generic-blanket-метода (`@checked_sub`/`@wrapping_sub`) ИЗНУТРИ ДРУГОГО generic-blanket-тела (`fn[T Ints] @checked_neg()`) падает в `[P67-LEGACY] method call return type unknown` (emit_c.rs) НЕЗАВИСИМО от вида receiver'а (Ident/Cast/self — все падают одинаково; шире, чем уже задокументированный в D423 §«Известные разрывы» частный случай "Call-receiver"). Обход — вызывать ТОЛЬКО компиляторный интринсик `@overflowing_sub` НАПРЯМУЮ (никогда другой hand-written `.nv`-бланкет), тот же вызов что уже делают `@checked_sub`/`@wrapping_sub`/`@saturating_sub` сами. Followup `[M-206.1-nested-blanket-call-p67]` — не фиксится в рамках 206.1 (общий checker/codegen generic-return-type-inference разрыв, требует отдельной волны).

**(R5) Границы.** Только целочисленные `div`/`mod`/`neg`. `f64`-деление НЕ трогается (IEEE `/0 = inf/nan`, не trap — уже отдельная trap-политика с иным контрактом, D317). Z3-элизия (`overflow_site_elided`) МЕХАНИЧЕСКИ подключена на всех новых guard-сайтах (та же span-based проверка, что D423 §R3 использует для sized add/sub/mul) — но **Z3-СТОРОНА доказательства div/neg-safety НЕ реализована этой волной**: `verify/pipeline.rs::prove_int_overflow_sites` фильтрует проверяемые сайты на `BinOp::Add | Sub | Mul` (жёсткий `matches!`), div/mod/neg-сайты туда не попадают → guard всегда emit'ится (`overflow_site_elided` — no-op на этих сайтах, корректно консервативно, НЕ silent gap). Followup `[M-206.1-div-neg-z3-elision]` — расширение Z3-энкодинга на div-safety (`b≠0`, `¬(a=MIN∧b=−1)`) обязательств вне рамок P1-фикса. Duration (`std/src/time/duration.nv`) НЕ мигрирована на новые generic-бланкеты — её ручные `checked_div_i64`/`div_or_trap`/`checked_neg_i64`/`neg_or_trap` УЖЕ реализуют идентичную trap-дисциплину (D317, дособытийно) с СОБСТВЕННЫМИ panic-сообщениями (`"Duration arithmetic error: division by zero"` и т.п., отличными от новых generic-сообщений) — миграция рискует поменять message-текст существующих D317-тестов ради чистого dedup без функциональной необходимости; НЕ входит в объём 206.1 (в отличие от 206 Ф.3, который явно называл Duration add/sub). Followup `[M-206.1-duration-div-neg-dedup]` (P3, opportunistic).

### Связь

Amends [D423](#d423) (§R6 явно выносил div/neg за рамки — теперь закрыто) · Amends [D13](#) (int overflow trap, расширен на div/mod/neg) · [D317](#d317) (Duration overflow — параллельная, НЕ мигрирована, см. §R5) · [D227](03-syntax.md#d227) Rule 4 (негативный литерал в unsigned — hard error, мотивирует reformulation §R4.1) · [D140.4/194](09-tooling.md#d421) (Z3-элизия — механически подключена, СТОРОНА доказательства не реализована, §R5) · Plan [206](../../docs/plans/206-arithmetic-overflow-policy.md) · Plan [206.1](../../docs/plans/206.1-div-neg-trap.md).

## D430. Проверяемое числовое сужение — `try_to_<T>` чейн-семья + `RangeError` ([M-numeric-try-narrowing], 2026-07-20)

**Статус:** закреплён 2026-07-20 (owner-approved backlog item, форма зафиксирована владельцем ДО реализации). Новый std-API surface — не язык-меняющее слияние (ни синтаксиса, ни type-checker/codegen-логики компилятор не менял, только `.nv`-бланкеты над УЖЕ существующим `Ints`-инфраструктурой D310/D423).

### Что

**Мотив:** `as`-сужение молча обрезает (`300u32 as u8` = 44 — тихая порча данных, Rust-прецедент `u8::try_from(300u32) → Err`). Нужен проверяемый вариант, симметричный уже принятой чейн-конвенции Nova (`str.to_i8`/`checked_*`/`saturating_*`), а НЕ Rust-static `u8::try_from(...)`.

**Форма:** `(300u32).try_to_u8() -> Result[u8, RangeError]` — метод на ИСХОДНОМ значении (владелец, 2026-07-20: чейн, не static-конструктор на целевом типе). Имя: приставка `try_` = проверяемая версия, `to_<T>` = целевой тип — тот же принцип именования, что `str.to_i8`/`to_u16` (Plan 174.1/numeric-parity-2) и `checked_*`-семья (D423). `as` остаётся быстрым обрезающим кастом, без изменений.

### Правило

**(R1) `RangeError` — новый unit-тип** (`std/prelude/errors.nv`), НЕ переиспользование `ParseIntError` (str-parse-специфичный: варианты `Empty`/`InvalidDigit`/`InvalidRadix` не имеют смысла для число→число, только `Overflow` был бы релевантен — навязывать вызывающему матчить нерелевантные варианты хуже, чем завести узкий тип) и НЕ `CharFromError`/`TryFromCharError` (char-конверсия, другой домен). Прецедент формы — ТЕ ЖЕ unit-типы («факт без данных»), `#stable(since = "0.1")`, re-exported через `std/prelude.nv` facade (`PRELUDE_VERSION` 18→19).

**(R2) Бланкет-стратегия — ОДИН `fn[S Ints] S @try_to_<T>()` на целевой тип `<T>` (10 бланкетов: i8/i16/i32/i64/int/u8/u16/u32/u64/uint), НЕ N²=100 ручных методов и НЕ два SignedInts/UnsignedInts-бланкета на одно имя** (в файле нет прецедента двух одноимённых бланкетов над непересекающимися type-set — решено НЕ открывать этот вопрос, единый `Ints`-бланкет с sign-agnostic `if @ < 0 {...} else {...}` веткой, та же форма что `@saturating_pow` уже использует, закрывает и signed, и unsigned источники одним телом). Каждый бланкет покрывает ВСЮ матрицу 10 источников × 1 цель (мономорфизация per член `Ints`) — 10 бланкетов = полные 100 пар источник×цель, включая 10 identity-пар (`i8.try_to_i8()`) и same-signedness WIDENING-пары (`i8.try_to_i64()`) — те тривиально всегда `Ok`, включены НЕ как scope creep, а потому что Nova type-sets не имеют width-based исключающего механизма: ручное выкусывание ~45 «безопасных» пар из `Ints` означало бы возврат к N² ручным методам — именно то, что бланкет-форма призвана избежать. Тот же выбор, что у Rust `TryFrom` numeric impls (тоже полная матрица, widening-плечи документированы как «never fails», не опущены).

**(R3) Soundness-инвариант.** Каждое тело `@try_to_<T>` расширяет `@` до full-width домена СВОЕЙ знаковости ПЕРЕД сравнением с границами цели — `i64` на ветке `@ < 0` (достижима только для signed членов; для unsigned членов константно `false`, ветка мертва в рантайме, но всё равно компилируется), `u64` на ветке `@ >= 0` (достижима для ЛЮБОГО члена — `@ < 0` константно `false` для unsigned). `i64`/`u64` вмещают ЛЮБОЙ член `SignedInts`/`UnsignedInts` точно И вмещают `MIN`/`MAX` ЛЮБОЙ цели точно (цель сама — тоже какой-то член `Ints`) — ни один cast в бланкетах не обрезает/не переполняется значением, от которого зависит сравнение. Наивная альтернатива — кастовать `MAX`/`MIN` цели ВНИЗ в исходный тип `S` — ломается, когда цель ШИРЕ `S` (`u32.MAX as i32` даёт `-1`, портит проверку `i32 @try_to_u32()`); расширение `@` ВВЕРХ вместо сужения границы ВНИЗ обходит этот класс багов целиком.

**(R4) D227 Rule 4 (негативный литерал в unsigned-моно) НЕ триггерится нигде в этих бланкетах.** `i8.MIN`/…/`int.MIN` — компайл-тайм константы ФИКСИРОВАННОГО signed-типа, кастуются ТОЛЬКО в `i64` (тоже signed) на ветке `@ < 0` — никогда в unsigned. Единственное сравнение с неявной нижней границей unsigned-цели — `@ < 0` (литерал `0`, неотрицательный, легален независимо от знаковости `S` — та же форма, что `checked_div`/`saturating_pow` уже используют). `@ as i64` внутри `Ints`-бланкета имеет рабочий прецедент — `std/time/duration/core.nv`, `fn[T Ints] T @to_nanos() -> Duration => { nanos: @ as i64 }`.

**(R5) Компилятор НЕ менялся.** Реализация целиком в `.nv` (`std/prelude/protocols.nv` + `std/prelude/errors.nv` + facade `std/prelude.nv`) поверх уже принятых `SignedInts`/`UnsignedInts`/`Ints` type-set'ов (D310) и уже рабочего `@ as i64`/`@ as u64`-паттерна (D423/D427-семья, `duration/core.nv`). `compiler-codegen/src/{types/mod.rs, codegen/emit_c.rs, lints.rs}` не тронуты (заняты параллельной Duration-волной на момент реализации).

### Границы

**Известный, НЕ новый разрыв, задетый при написании тестов:** generic type-set-bound бланкет-метод (`fn[S Ints] S @try_to_<T>()`), вызванный на receiver'е, который сам — bound переменная `for`-цикла (`for v in vec { v.try_to_u8() }`), падает в pre-existing `[P67-LEGACY]` "method call return type unknown" (тот же класс, что уже задокументирован в заголовке `std/src/math/overflow_policy_test.nv` для inline-conversion-call receiver — здесь тот же checker-gap для другого receiver-shape, `for`-bound Ident, а не Call). НЕ specific для `try_to_*` (общий generic-blanket-dispatch разрыв, вне зоны этой волны) — обходится в тестах чтением элемента в типизированную `ro`-локаль перед вызовом (тот же обход, что весь остальной файл уже применяет). Followup НЕ заводился отдельно — покрыт существующим общим "generic-blanket receiver must be typed local" классом (`[P67-LEGACY]`, см. D423/D427 §«Известные разрывы»/§R4.3).

**Тесты** — `std/src/math/try_narrowing_test.nv` (НЕ рядом с `protocols.nv`: тот же pre-existing gap с отключённым auto-import prelude внутри `std.prelude.*`, что у `overflow_policy_test.nv`).

### Связь

Использует [D310](02-types.md#d310) (`SignedInts`/`UnsignedInts`/`Ints` type-set'ы) · Использует [D423](#d423)/[D427](#d427) (`@ as i64`/`@ as u64`-паттерн внутри `Ints`-бланкета, sign-agnostic `if @ < 0 {} else {}` форма) · [D227](03-syntax.md#d227) Rule 4 (негативный литерал в unsigned — НЕ триггерится, см. §R4) · Соседствует с `str.to_i8`/`to_u16` (Plan 174.1/numeric-parity-2, тот же range-check-после-разбора паттерн, другой error-тип `ParseIntError` — НЕ переиспользован, см. §R1) · Backlog `[M-numeric-try-narrowing]` (docs/plans/backlog-followups.md).

## D431. `#default_handler(X)` — ambient lazy default-handler factory для эффектов (Plan 175 Ф.2-v2, 2026-07-21/22)

**Статус:** Ф.1 (компилятор-механизм + Time-хук) LANDED этой волной (Plan 175 Ф.2-v2); typed-schema retype Time (D316-полный `sleep(Duration)`/`now()->Timestamp`/`now_monotonic()->Monotonic`) LANDED следующей волной (Plan 175 Ф.2-v3, см. amend ниже) — scalar-bridge реализован через per-op marshalling, НЕ через отложенный per-Time-struct-редизайн. Ambient-retraction (D62 amend) остаётся OPEN.

### Контекст

До этой волны единственный «дефолт без `with X = …`» механизм жил как хардкод ВНУТРИ hand-written C-диспетчеров конкретных эффектов (`Nova_Time_sleep`/`_now_unix_ms`/… в `nova_rt/fibers.h`: `if (_nova_handler_Time) {…} else {…захардкоженный real-clock impl прямо в C…}`). Не-generic (каждый будущий built-in-подобный эффект требовал бы своей C-копипасты) и противоречит §3 (`[[feedback-maximize-nv-sourcing]]` — реализация должна быть в `.nv`, не в Rust/C, где портируемо).

### Решение

Новый атрибут `#default_handler(EffectName)` перед свободной zero-arg `fn … -> Effect[EffectName]`, тело которой — обычный handler-литерал (`effect EffectName { op() => … }`), компилируемый ОБЩИМ путём (никакого нового codegen для тел — тот же `emit_handler_lit`, что и любой `with`-мок).

1. **Семантика.** Транзитивное использование опа `EffectName` БЕЗ объемлющего `with EffectName = …` в текущем потоке синтезирует lazy, once-per-thread construct-and-install дефолта: `if (!_nova_handler_X) { _nova_handler_X = <default-ctor>(); }` перед диспетчем (компилируется в само тело диспетчер-функции `Nova_X_op`, эмиттируемой `emit_effect_type`/эффект-специфичным hand-written vtable-путём). Явный `with X = …` по-прежнему полностью переопределяет как раньше (`emit_with`'ов save/install/restore не тронут). «Ленивость» тут — per-thread memoization в САМОМ TLS-слоте `_nova_handler_X` (once set, никогда не NULL для этого потока), а не отдельный флаг.
2. **DCE-дружественность.** Эффект без `#default_handler` не платит ничего (opt-in per эффект); эффект С атрибутом остаётся живым в дереве reachability-DCE (Plan 81 Ф.7.2/159 Ф.1) через явный worklist-seed (симметрично `main`), т.к. единственная ссылка на ctor — сырая C-строка в generated-`main()`-прологе (или диспетчер-теле), невидимая AST-walker'у `collect_used_names`.
3. **Несколько дефолтов / порядок.** НЕ нужен явный upfront topo-sort: lazy-per-thread-конструирование делает порядок ЭМЕРДЖЕНТНЫМ — если ctor эффекта X внутри своего тела зовёт оп эффекта Y (у которого ТОЖЕ есть `#default_handler`), Y лениво сконструируется ПЕРВЫМ (обычный вызов op триггерит ту же lazy-init-проверку). Цикл (X ctor транзитивно зависит от Y ctor, который зависит от X) НЕ разрешается рантаймом (ушёл бы в бесконечную рекурсию/стек-overflow) → COMPILE-TIME ошибка `E_DEFAULT_HANDLER_CYCLE` (`check_default_handlers`, простой DFS по графу «X-ctor тело ссылается на эффект Y» среди зарегистрированных default-handler'ов).
4. **Валидация (`check_default_handlers`, types/mod.rs).** Ровно один `#default_handler` на эффект (`E_DEFAULT_HANDLER_DUPLICATE`); имя эффекта обязано резолвиться в декларированный `type X effect {…}` в этом CU (`E_DEFAULT_HANDLER_UNKNOWN_EFFECT`); fn обязана быть zero-arg free fn (`E_DEFAULT_HANDLER_ARITY`) возвращающей РОВНО `Effect[EffectName]` (`E_DEFAULT_HANDLER_RETURN_TYPE`); граф-цикл — `E_DEFAULT_HANDLER_CYCLE` (см. п.3).
5. **Generic front-end, per-эффект runtime back-end.** Компилятор (parser/checker/DCE-root-seed/registry) обрабатывает `#default_handler` для ЛЮБОГО эффекта одинаково — но фактическое «врезание» lazy-init-проверки в диспетчер-функцию требует, чтобы этот диспетчер БЫЛ известен компилятору как единая точка (сегодня — только `Time`, через существующий hand-written `NovaVtable_Time`-путь: hook-переменная `_nova_time_default_ctor` — `NovaVtable_Time* (*)(void)`, `nova_rt/effects.h`/`effects.c`; `Nova_Time_sleep`/`_now_unix_ms`/`_now_monotonic_ns`/`_local_offset_sec` в `fibers.h` лениво вызывают её при `_nova_handler_Time == NULL`). Для эффектов, чей vtable/dispatch auto-генерируется `emit_effect_type` (Random, ResourceTrace, Supervisor, Application, …), АНАЛОГИЧНАЯ проверка уже встроена ГЕНЕРИЧЕСКИ (`if (!_nova_handler_X) { _nova_handler_X = <ctor>(); }` эмиттируется прямо в `Nova_X_op`-теле, если `default_handler_fns` содержит `X`) — механизм полностью работает для ЛЮБОГО такого эффекта без доп. кода; Time — единственный эффект с hand-written vtable, поэтому единственный, кому нужен отдельный fn-pointer-хук. Следующий built-in-подобный эффект (Fs/Net/Os — НЕ мигрированы этой волной) добавляет аналогичный однострочный хук, если тоже останется hand-written; если станет auto-generated — работает из коробки.
6. **`Time`-дефолт (`std/src/time/duration/core.nv`, `time_default`).** Тело — `effect Time { sleep(ms) => _nova_time_default_sleep(ms); now_unix_ms() => _nova_wall_unix_ms(); now_monotonic_ns() => _nova_monotonic_ns(); local_offset_sec() => _nova_local_offset_sec() }`, где четыре `extern "C" fn` — тонкие примитивы поверх УЖЕ существующих `nova_rt/fibers.h`-реализаций (переименование/экспонирование, не новая C-логика). C-хардкод-fallback В КАЖДОМ диспетчере (`Nova_Time_*`) СОХРАНЁН как bootstrap-safety-net для CU, не пуляющих в себя `std.time` (`_nova_time_default_ctor == NULL`) — backward-compat, не форсированная миграция.

### Границы / отложено

- ~~**Typed-schema retype Time**~~ — **ЗАКРЫТО** Plan 175 Ф.2-v3 (см. amend «D316/D431 Ф.2-v3: typed Time-схема + снос рукописного диспатча» ниже). Решение НЕ было «отложенный per-Time-struct-редизайн»: hand-written `NovaVtable_Time` struct/slot ОСТАЛСЯ (channels.h/runtime.c нужен стабильный C-тип, компилируемый once, не per-CU) — но его WIRE переехал на raw int64 nanoseconds под НОВЫМИ typed op-именами, а marshalling typed⇄wire живёт в generated dispatch-fn (`emit_effect_type`) и handler-install-thunk (`emit_handler_lit`), СИММЕТРИЧНО тому же ABI-identity приёму, что `Nova_Mutex_method_lock_for` уже применяет для Duration-таймаута (`sync_primitives.h`).
- ~~**Ambient-retraction Time (D62 amend)**~~ — **ЗАКРЫТО** Plan 175 Ф.2-v3 Фаза 4 (2026-07-22): владелец запросил ретракцию — каждая fn, транзитивно зовущая Time-опы, обязана нести `Time` в сигнатуре под `--strict-effects`, симметрично Fs/Net/Db. Аудит по всем трём уровням: `std/` — 0 находок (предыдущая волна уже держала дисциплину); 5 CI-целей (aggregator + echo net/tls) — built `--strict-effects` чисто; `spec_tests/conformance` (577 файлов, полный check) — РОВНО одна находка (`standalone/vr_binop_arith_dce.nv`, `fn main() Io -> ()` звало `Monotonic.now()` — добавлен `Time`). Масштаб оказался НЕ сопоставим с 755+-сайт retyping'ом (предположение при постановке было завышенным) — единственный реальный фикс. `[M-175-time-ambient-retraction]`.
- Fs/Net/Os — НЕ мигрированы на `#default_handler` (механизм — generic, но их C-хардкод-дефолт не тронут этой волной; следующий шаг «по образцу»).

### Связь

Родня [D316](#d316)/[D317](#d317)/[D318](#d318) (Plan 175 Time-семья) · amends [D62](#d62-прагматичная-семантика-эффектов-прямые-в-сигнатуре-fail-strict-async-ambient-правило-effectprotocol) (ambient-retraction Time — CLOSED Ф.2-v3 Фаза 4, см. amend ниже) · [[feedback-maximize-nv-sourcing]] (реализация в `.nv`, не в C) · Plan [175](../../docs/plans/175-time-system-rework.md) Ф.2-v2 · `[M-effect-handler-body-record-literal]` (см. amend в §«Handler-literal capture mechanism» ниже — CLOSED, common closure-capture path заменил `#define`-макросы).

## Amend D316/handler-literal capture mechanism: `[M-effect-handler-body-record-literal]` CLOSED (Plan 175 Ф.2-v2, 2026-07-21/22)

**Что было.** Handler-literal (`with X = effect X {…}`) op-тела эмитились ОСОБЫМ inline-путём (`emit_handler_lit`), беднее общего fn-пути: захваты — через `#define <cap> (*_c-><cap>)`-макросы (коллизия с любым struct-field-token, шарящим имя, включая capture-полю ВЛОЖЕННОГО closure/protocol-lit, построенного внутри того же op-тела — задокументированный воркэраунд в `emit_lambda`'s `mangled_field`); анонимный record-литерал (D55, `make() => { x: 1 }`) внутри op-тела до фикса «один канал» (2026-07-10) падал `[E5xxx] anonymous record literal without spread not supported in codegen` — этот конкретный дефект уже был закрыт ДО этой волны (см. коммит 02d5da526).

**Что сделано этой волной (архитектурная замена, не догоняющий патч).** Захваты handler-литерала теперь используют РОВНО ту же схему, что closures (`emit_lambda`): мутабельные захваты в ESCAPING-хендлерах (возвращаемых из factory-fn как `Effect[X]`) heap-promoted (`var_boxed`-регистрация, `(*box)`-дереф через `ExprKind::Ident`, БЕЗ макросов); в INLINE-хендлерах (`with X = … { body }`) — `&cap_name` напрямую (НЕ box: `emit_with` заворачивает handler-конструирование в свой interrupt-frame C-блок, поэтому НОВАЯ boxed-переменная там была бы C-scope-leak — найдено на `spec_tests/conformance/repro_matrix.nv`'s two-level nested-handler-capture фикстуре, `[M-175-handler-lit-boxed-var-c-scope-leak]`, CLOSED тем же коммитом). Поля ctx-struct — mangled (`_nv_fv_<handler_id>_<name>`), исключает macro-collision-класс структурно (не point-фиксом). Op-тело эмиттируется через ТОТ ЖЕ `current_fn_return_ty`/`expected_record_type`-канал, что `emit_fn`/lambda/protocol-method (уже был подведён фиксом «один канал» 2026-07-10) — анонимные record-литералы (heap И value) полностью работают в op-теле, включая мок-хендлеры, строящие `Monotonic{…}`/`Timestamp{…}`-подобные value-records напрямую.

**Побочная находка/фикс (тот же коммит):** `[M-spawn-var-boxed-leak]` — `spawn {}`/`detach {}`/`blocking {}`-тела резолвят захваты через СВОЙ, отдельный `current_spawn_captures`-механизм (`_c->name`), НЕ через `var_boxed`; но `var_boxed`-проверка в `ExprKind::Ident`-резолюции стоит РАНЬШЕ `current_spawn_captures`-проверки — stale outer `var_boxed`-запись (от closure/handler-literal РАНЕЕ в той же enclosing fn) затеняла корректный spawn-capture-путь. Пофикшено изоляцией `var_boxed` (save/take/restore) вокруг тела КАЖДОГО из этих трёх body-swap-сайтов (`emit_spawn`, `emit_detach`, оба `blocking`-work-fn-сайта) — mirror того же паттерна, что `emit_lambda`/`emit_monomorphized_method` уже применяют к своим телам.

### Связь

[D431](#d431-default_handlerx--ambient-lazy-default-handler-factory-для-эффектов-plan-175-ф2-v2-2026-07-2122) · Plan [175](../../docs/plans/175-time-system-rework.md) Ф.2-v2 · `docs/plans/backlog-followups.md` (`[M-effect-handler-body-record-literal]` → CLOSED).

## Amend D316/D431 Ф.2-v3: typed Time-схема + снос рукописного диспатча (Plan 175 Ф.2-v3, 2026-07-22)

**Статус:** ЗАКРЫВАЕТ `[M-175-time-typed-schema-scalar-bridge]` (D431 «Границы/отложено») и историческую находку Ф.2 (4× откат — vtable эмитился раньше value-record typedef'ов).

### Фаза 1 — снос рукописного `Nova_Time_*`-диспатча

Hand-written dispatch-функции (`Nova_Time_sleep`/`_now_unix_ms`/`_now_monotonic_ns`/`_local_offset_sec`, `nova_rt/fibers.h`) и парный `_nova_time_default_ctor`-хук (`effects.h`/`effects.c`) СНЕСЕНЫ. `Time` теперь генерирует dispatch-функции ЧЕРЕЗ ОБЩИЙ `emit_effect_type`-путь (тот же, что user-эффекты) — единый источник и для схемы (уже был, Ф.1), и для диспетчер-тел (новое). `#default_handler(Time)` (`time_default`) ПЕРЕЕХАЛ из `std/time/duration/core.nv` в `std/prelude/effects.nv` (сразу после декларации `Time`) — prelude auto-import'ится в КАЖДЫЙ CU, поэтому ambient-fallback (Time работает без `with`/явного `import std.time`) больше НЕ зависит от того, попал ли `time.duration`-модуль транзитивно в конкретный CU (раньше — зависел, backward-compat держался на hand-written C-fallback, который теперь снесён).

**Узкое ОСТАЮЩЕЕСЯ исключение** (не архитектурная дыра, реальная необходимость): typedef `NovaVtable_Time` + TLS-слот `_nova_handler_Time` остаются hand-written в `effects.h`/`.c` — `nova_rt/channels.h` (`ChanReader.close_after` mock-time path) и `nova_rt/runtime.c` (worker-thread TLS registration) — HAND-WRITTEN C вне codegen, компилируемый ОДИН раз (не per-CU), — обращаются к `_nova_handler_Time->sleep(...)`/`->ctx` напрямую по конкретным полям; нужен один стабильный named-struct-тип, который anonymous per-CU-generated struct дать не может (плюс typedef-redefinition конфликт). `emit_effect_type` для `name == "Time"` пропускает шаги 1+2 (struct+TLS-slot-decl) — они здесь — и эмитит ТОЛЬКО шаг 3 (dispatch-функции, generic как всегда).

**Регрессия, найденная и закрытая В ТОЙ ЖЕ волне:** снятие explicit `_nova_handler_Time`-pre-registration с main-thread пути (`emit_main_wrapper`) при сохранении hardcoded pre-registration на worker-thread (`runtime.c`) давало РАЗНЫЙ snapshot-index для остальных эффектов (напр. `Application`) между main и worker — child fiber наследовал ЧУЖОЙ handler (`spec_tests/conformance/app_effect_basic_t8_1` ловил дефект). Fix: `Time` регистрируется ТОЛЬКО через generic `_nova_register_effects_fn` на ОБОИХ путях — единый источник порядка индексов.

### Фаза 2 — value-record typedef'ы ДО effect-vtable struct (2-pass emission)

Корень исторического 4× отката: главный "type declarations first" проход в `emit_module` эмитил effect-vtable struct (function-pointer поля) в ТОМ ЖЕ проходе по `module.items`, что record/value-record тела — если `type X effect {...}` встречался РАНЬШЕ value-record'а, чьим типом типизирован один из его опов (Duration), C получал by-value поле несовершенного типа. Fix: проход разбит на ДВА — сперва ВСЁ кроме `TypeDeclKind::Effect`, затем ВСЕ эффекты. Порядок в merged item-list больше не важен. Изолированный репро (RED→GREEN): user-эффект с Duration-типизированным опом, handler-тело строит Duration через `.plus()`.

### Фаза 3 — typed Time-опы

Схема (`std/prelude/effects.nv`): `sleep(ms int)->()`/`now_unix_ms()->int`/`now_monotonic_ns()->int` → `sleep(d Duration)->()`/`now()->Timestamp`/`now_monotonic()->Monotonic` (голые имена, единица в ТИПЕ). `local_offset_sec()->int` не менялся (плоский оффсет — не time-величина).

**Wire остался raw int64 nanoseconds** (НЕ typed) в hand-written `NovaVtable_Time` (Фаза-1-исключение выше не позволяет struct'у именовать per-CU `NovaValue_Duration`/`Timestamp`/`Monotonic`). Codegen маршалит на границе — ТОТ ЖЕ ABI-identity приём, что `Nova_Mutex_method_lock_for` уже применяет для Duration-таймаута (`sync_primitives.h`: `void* timeout` + "first field int64_t nanos" контракт), применён систематически к трём clock-типизированным опам Time:

- `emit_effect_type` (dispatch-функция, ЗНАЕТ complete typed-подпись, т.к. эмитится после Фазы-2-переупорядочивания): извлекает `.nanos` из by-value typed-параметра перед вызовом wire-слота; оборачивает raw int64 wire-результат в typed value-record (`(NovaValue_Timestamp){.nanos = ...}`) компаунд-литералом перед возвратом typed-вызывающему.
- **Per-field NULL-check + real-clock fallback** (восстанавливает backward-compat для ЧАСТИЧНЫХ handler-литералов — `nova_tests/plan83_10/handler_isolation_per_fiber.nv` определяет ТОЛЬКО `now()`; `sleep`/`now_monotonic`/`local_offset_sec` обязаны не падать на NULL fn-pointer, а прозрачно упасть на `_nova_wall_unix_ms`/`_nova_monotonic_ns`/`_nova_local_offset_sec`/`_nova_time_default_sleep` — тот же контракт, что был у ex-`now_monotonic_ns`/`local_offset_sec`-слотов до Ф.2-v3, расширен defensively на ВСЕ четыре опа).
- `emit_handler_lit` (handler-literal install): typed op-тело оборачивается тонким marshalling-THUNK с wire-сигнатурой (`(void*, int64_t)`/`(void*) -> int64_t`) перед записью в vtable-slot — иначе function-pointer-signature mismatch (by-value struct vs raw int64 — НЕ ABI-совместимые типы функции, несмотря на layout-идентичность самого значения).

### Латка снята НАВСЕГДА — sleep = ТОЛЬКО метод (владелец, П12-снос → постоянное решение)

Свободные `sleep(Duration)`/`sleep_until(Monotonic)` (временная латка после Ф.2-v2, восстановленная для d316-строгости) RETRACTED из `std/time/duration/core.nv` БЕЗ замены. Канон — ТОЛЬКО методы: `d.sleep()` (`Duration @sleep()`, зовёт `Time.sleep(@)` напрямую — полная ns-точность, ceil-to-ms больше НЕ на `.nv`-сахарном слое, а внутри real-clock `#default_handler`) и `deadline.sleep_until()` (`Monotonic @sleep_until()`, pre-existing с "П12-хвоста"). `Timestamp.now()`/`Monotonic.now()` упрощены до прямого `Time.now()`/`Time.now_monotonic()` (typed op возвращает готовый value — обёртке больше нечего строить вручную).

**Побочная находка (не регрессия этой волны, но впервые релевантна):** `[M-175-realtime-ban-method-call-blind]` — D64 realtime-suspend-effect-check (`types/mod.rs`, `check_expr_forbid`/`check_callee_effects`) СИНТАКСИЧЕСКИЙ на форму вызова: ловит `Effect.op(...)`-shaped path (`path.len()==2 && effect_decls.contains(path[0])`) и qualified free-fn/static-method calls через `method_table`, но НЕ instance-method call на произвольном expression-receiver (`expr.method()`, напр. `d.sleep()`) — тот путь явно помечен "dynamic member-call; не resolve'им" / "instance-method через obj.method требует type-инференции, отложен" уже ДО этой волны. Раньше это было некритично, т.к. канонический sleep-вызов (`sleep(d)`/`Time.sleep(d)`) попадал в ПОКРЫТУЮ форму; теперь, когда `d.sleep()` — promoted idiom, D64-гард на `realtime`-функциях, зовущих sleep ТОЛЬКО через метод, слеп. Негатив-фикстура `spec_tests/conformance/neg/d316_realtime_sleep_neg.nv` сознательно продолжает звать `Time.sleep(d)` (не `.sleep()`) — обе формы валидны, эффект-оп не ретрактирован, только `.nv`-сахарные free-функции. Расширение D64/D63-скана на instance-method-call — отдельный followup (нужна receiver-type-инференция в этом checkpoint'е), не блокирует эту волну.

### Фаза 4 — ambient Time retraction (D62 amend) верифицирована до нуля

Владелец: каждая fn, транзитивно зовущая Time-опы, обязана нести `Time` в effect-row под `--strict-effects` (симметрично Fs/Net/Db) — D431 «Границы» предполагал это НЕ начатым, масштаб сопоставимым с 755+-сайт retyping'ом из §6. Аудит (`nova check --strict-effects`) по трём уровням дал СИЛЬНО меньший реальный остаток:

- **`std/`** (142 файла) — 0 находок: предыдущая волна уже держала дисциплину (функции вроде `Uuid.v7()` уже несут `Time Random` в сигнатуре).
- **5 CI-целей** (aggregator + echo net/tls client/server) — built `--strict-effects` чисто.
- **`spec_tests/conformance`** (577 файлов, полный check) — РОВНО одна находка: `standalone/vr_binop_arith_dce.nv`, `fn main() Io -> ()` звало `Monotonic.now()` без `Time`. Единственный fix: `fn main() Io Time -> ()`.

Ретракция закрыта верифицируемо (не «сделано и не проверено») — исходное предположение о масштабе (>200 сайтов) не подтвердилось, реальный остаток — один файл.

### Фаза 5 — две регрессии, найденные бисекцией на ПОЛНОМ мега-CU (2026-07-22)

Точечные гейты Фаз 1-4 (per-file/per-directory `nova test`/`nova check`) НЕ поймали два дефекта — оба видны ТОЛЬКО на полном `spec_tests/conformance` mega-CU (`nova test --positive --compile-error spec_tests/conformance` — ВЕСЬ каталог как ОДИН compile-unit, см. `project-conformance-single-cu-run`). Урок: точечный d316-подсет ≠ полный мега-CU gate.

**(1) `[M-175-lazy-const-crossmodule-collision]`** — `spec_tests/conformance/standalone/repro_const_dup.nv` (CC-FAIL: "redefinition of 'ZERO'/'SECOND' with a different type"). Корень: module-level `ro NAME = expr` (`Item::Let`, non-constexpr initializer → lazy-init путь, `emit_lazy_const`) НИКОГДА не проходил через module-qualification (`private_const_c_names`, Plan 91.12/D307) — только `Item::Const` получал mangled-имя при collision. Пока `std.time.duration` не было transitively reachable из КАЖДОГО CU (до Ф.2-v3), это было безобидно (bare-имя `ro`-биндинга почти никогда не сталкивалось с чужим bare-именем); Ф.1's `import Duration/Timestamp/Monotonic` в `std/prelude/effects.nv` сделало `Duration.ZERO`/`SECOND` (exported consts, `duration/core.nv`) присутствующими в КАЖДОМ CU — файл с собственным приватным `ro ZERO`/`ro SECOND` (как `repro_const_dup.nv`) столкнулся с ними на уровне C-символа (`_nova_const_ZERO_value` для обоих, хотя checker резолвит каждую ссылку однозначно в СВОЕМ файле). Fix (`emit_c.rs`):
  - group-const pre-pass (`emit_module`, Plan 91.12 Step 1) расширен на `Item::Let` — приравнен к non-exported non-file-private `Item::Const` (LetDecl не имеет `is_export` вообще — module-level `ro` неэкспортируем в принципе). Ловушка при первой попытке: bare ALL-CAPS-имя (`ZERO`) парсится в `Pattern::Variant { path: [name], kind: Unit }`, НЕ `Pattern::Ident` — недостающий match-arm (нужно зеркалить существующий `Item::Let`-цикл в `emit_module`) дал молчаливый no-op на первом заходе.
  - `emit_lazy_const` теперь берёт ДВА разных параметра — `name` (bare source identifier, управляет `lazy_consts`/`var_types`/topo-sort deps) и `c_name` (qualifier, подставляемый В `_nova_const_<c_name>_value`-обёртку — обёртка СОХРАНЕНА байт-в-байт для non-colliding случая, чтобы не тронуть keyword-safety другого назначения обёртки).
  - REFERENCE-site (`ExprKind::Ident`) для lazy const теперь ТОЖЕ смотрит `private_const_c_names` (используя тот же qualifier внутри той же обёртки) — раньше bare-имя читалось безусловно.

**(2) `[M-175-realtime-ban-method-call-blind]` (частично ЗАКРЫТ)** — `spec_tests/conformance/neg/handler_sleep_neg.nv` (NEG-NO-ERROR: `E_SUPERVISOR_HANDLER_SUSPEND` перестал ловиться). Ф.2-v3-followup, ранее помеченный «не блокирует эту волну», ОКАЗАЛСЯ блокирующим для ЭТОГО конкретного чекера — красный негатив = блокер по конвенции, «не блокирует» относилось только к D64 realtime-scan (types/mod.rs `check_callee_effects`/`realtime_suspend_effect`), а НЕ к отдельному Supervisor-хендлер suspend-scan'у (Q-блок 173.2, `walk_expr_for_handler_lits`) — ДВА РАЗНЫХ чекера с одинаковым структурным изъяном (оба синтаксические, без type-инференции). Fix: Supervisor-хендлер-скан расширен — вместо ТОЛЬКО `Time.sleep(...)`/`Path(["Time","sleep"])`, теперь ЛЮБОЙ `.sleep()`/`.sleep_until()` method-call (независимо от receiver'а) считается suspend-кандидатом — эти имена в std принадлежат исключительно `Duration`/`Monotonic`, а контекст (Supervisor-хендлер) достаточно узкий, чтобы гипотетический false-positive на постороннем одноимённом методе был приемлемой ценой. D64 realtime-ban (types/mod.rs, ДРУГОЙ чекер) остаётся ОТКРЫТЫМ — см. followup, `neg/d316_realtime_sleep_neg.nv` сознательно держится на `Time.sleep(d)`-форме (см. amend выше).

### Гейт

5/5 d316-фикстур (`spec_tests/conformance/neg/d316_*`, `standalone/d316_time_effect_typed_surface`) PASS · `std/src/time`+`std/src/testing` suite 8/8 PASS · `nova check std` 142/142 реальных файлов (17 "FAIL" — все intentional `*_neg`) · `tls_handler_per_fiber_armed`/`tls_handler_race_repro`/`handler_isolation_per_fiber` (armed M:N per-fiber handler isolation под typed-схемой) PASS · `examples/flagship/aggregator` + 4 echo-примера (net/tls × client/server) `--strict-effects` built+boot · **ПОЛНЫЙ мега-CU** `nova test --positive --compile-error --timeout 600 spec_tests/conformance` — **PASS: 527 FAIL: 0 SKIP: 55** (byte-for-byte совпадает с историческим чистым baseline до этой волны — регрессии закрыты, не просто скрыты).

### Связь

Amends [D316](#d316) (typed-schema retype — теперь LANDED) · [D431](#d431-default_handlerx--ambient-lazy-default-handler-factory-для-эффектов-plan-175-ф2-v2-2026-07-2122) (`#default_handler` mechanism, `[M-175-time-typed-schema-scalar-bridge]` → CLOSED) · `nova_rt/sync_primitives.h` `Nova_Mutex_method_lock_for` (тот же void*+first-field ABI-identity приём, прецедент) · Plan [175](../../docs/plans/175-time-system-rework.md) Ф.2-v3 · `[M-175-realtime-ban-method-call-blind]` (Supervisor-хендлер-ветка CLOSED эта волна; D64-ветка остаётся open followup, docs/plans/backlog-followups.md) · `[M-175-lazy-const-crossmodule-collision]` (новый, CLOSED эта же волна, docs/plans/backlog-followups.md) · `project-conformance-single-cu-run` (мега-CU gate convention).

## Amend D316/D431 Ф.2-v3 — extern-нейминг: `_nova_*` → доменный `time_*` (владелец code-review, 2026-07-22)

**Статус:** закрывает несоответствие §5а (`docs/compiler-conventions.md` — «Имена C-символов на FFI-границе», согласовано 2026-07-08). Ф.2-v3 (амендмент выше, та же дата) ввела в `std/prelude/effects.nv` четыре `extern "C" fn` с vendor-префиксом `_nova_` (`_nova_wall_unix_ms`/`_nova_monotonic_ns`/`_nova_local_offset_sec`/`_nova_time_default_sleep`) — нарушение уже зафиксированного правила «модульные C-шимы — `<модуль>_<имя>` БЕЗ vendor-префикса», прецедент `fs_open`/`fs_close`/`fs_chmod` (`std/fs/ffi.nv` ↔ `nova_rt/fs.c`), `net_addr_loopback_into`, `os_env_get`.

**Фикс:** переименованы в `time_wall_unix_ms`/`time_monotonic_ns`/`time_local_offset_sec`/`time_default_sleep` — синхронно на обеих сторонах FFI-границы:
- `.nv`: `extern "C" fn`-декларации, `std/prelude/effects.nv` (`time_default` default-handler body).
- C: определения в `nova_rt/fibers.h`; внутренние вызовы `time_monotonic_ns()` в `nova_rt/sync_barrier.h`/`sync_condvar.h`/`sync_countdown_latch.h`/`sync_primitives.h`/`sync_semaphore.h` — этот хелпер общий низкоуровневый rt-примитив (timeout-ветки mutex/rwlock/condvar/semaphore/barrier/countdown-latch), НЕ приватный для `Time`-эффекта; переименован везде одним движением, семантика не менялась.
- Rust codegen (`emit_c.rs`) — hardcoded C-строки install-once vtable-пути `Time` (`sleep`/`now`/`now_monotonic`/`local_offset_sec` fallback-ветки) обновлены синхронно, иначе unresolved-symbol CC-FAIL.

Заодно (тот же владелец-ревью, П2): default-handler-тело `sleep(d)` в `time_default` получило явный тип параметра — `sleep(d Duration) => ...` (handler-method-параметры в handler-литерале синтаксически опциональны по типу, D40 — компилятор выводит их из декларации эффекта; явная аннотация здесь чисто для читаемости, единственный параметризованный оп default-реализации). `now()`/`now_monotonic()`/`local_offset_sec()` — без параметров, менять нечего.

**Границы:** переименование C-symbol'ов и добавление одной явной type-аннотации — НЕ поведение/ABI типов Nova-уровня. `Time`-эффект-схема, `sleep(Duration)`-контракт, wire-формат (raw int64 nanos) не менялись. Публичный Nova-код эти C-имена не видит (internal rt↔codegen контракт).

### Гейт

Полный мега-CU `spec_tests/conformance` (см. коммит-гейт волны) · `std/src/time/duration` suite (33 теста, вынесенные в `core_test.nv` тем же слиянием — см. соседний followup вынос-тестов) · 5 CI-целей `--strict-effects` built+boot · `nova check std --strict-effects` 0 новых находок.

### Связь

§5а (`docs/compiler-conventions.md`, 2026-07-08, C-symbol naming) · Amends [D316](#d316)/[D431](#d431-default_handlerx--ambient-lazy-default-handler-factory-для-эффектов-plan-175-ф2-v2-2026-07-2122) Ф.2-v3 (описание ДО этого переименования, историческая точность сохранена, не переписана) · [D40](03-syntax.md#d40) (handler-method `=> expr`/`{ block }`, параметры опциональны по типу).

## Amend D316/D431 Ф.2-v4 — эффект-API-полировка (owner code-review, 2026-07-22)

**Статус:** ✅ LANDED. Вычитка эффект-поверхности ДО тегов v0.1 (владелец, 2026-07-22) вскрыла ряд стилевых/инкапсуляционных недоделок в `Time`, оставленных предыдущими волнами (Ф.2-v2/Ф.2-v3). Все пункты — language-changing, поэтому амендмент той же веткой (`p-fx-api-polish`), что и правки.

**П6 — `Time` + `#default_handler`-фабрика переехали ОБРАТНО из prelude в `std/time/duration/time_effect.nv`** (co-equal файл модуля `time.duration`, рядом с `core.nv`/`monotonic.nv`/`timestamp.nv`). Prelude-резидентство (Ф.2-v3) покупало ambient-доступность БЕЗ import — но ambient (Time без `with`) был ретрактирован ещё в Ф.2-v3/Ф.3 (D62-амендмент выше): каждая fn обязана нести `Time` в effect-row явно, как `Fs`/`Net`/`Os`. Effect-row имена резолвятся compile-unit-wide вне зависимости от модуля объявления (07-modules §393; прецедент — `std/time/civil/tzif.nv` пишет `Fs Os -> ...` без импорта ни того, ни другого имени) — значит prelude-размещение не покупало НИЧЕГО, что уже не давала бы ретракция. Реальный выигрыш переноса — П5 (ниже): handler, живущий В `time.duration`, конструирует `Monotonic`/`Timestamp` через bare record-literal БЕЗ публичного `from_ns`, т.к. module-private поля видны из любого co-equal файла модуля. `std/prelude/effects.nv` теряет `import std.time.duration.{...}` (снова строго ZERO-imports) и Time-декларацию целиком; сохраняет только pointer-комментарий на новое место. (П11, ниже, ПЫТАЛСЯ добавить `time.duration → time.civil` import — межмодульные циклы explicit ПЕРМИТТЕНЫ, Plan 162 Ф.2 / D29 rev-5 — но П11 сам был откачен по несвязанной codegen-причине, так что этот конкретный import не приземлился.)

**П11 — `local_offset()->Offset` (ПОПРОБОВАН, ОТКАЧЕН той же волной).** Цель была — ретипизировать `local_offset_sec()->int` на `Offset` (`std.time.civil`), консистентность с уже-typed `sleep`/`now`/`now_monotonic`. Реализовано (schema + real-handler + mock-хендлеры + `Offset.local()` упрощён), но авторитетный `spec_tests/conformance` мега-CU СЛОМАЛСЯ: сделав `Offset` (и ЛЮБОЕ его конструирование — даже bare `{ seconds: … }` без `Result`/`from_seconds`) достижимым из `time_effect.nv` (транзитивно — почти из КАЖДОГО CU, использующего `Time`), не связанный bare/erased-`Option` if-let (`d34_pattern_bind_conditions.nv`, `fn d34_cache_get(key int) -> Option { ... }`, БЕЗ явного `[T]`) начал резолвить свой result-тип как `NovaValue_Offset` вместо `nova_int` (CC-FAIL "assigning to NovaValue_Offset from incompatible type nova_int" — физически в СОВСЕМ другом файле). Подтверждено воспроизводимо (убирание НОВОЙ фикстуры волны не чинит, чинит ТОЛЬКО полный откат ретайпа) и НЕ специфично для `Offset.from_seconds`/`Result` (bare record-literal тоже триггерит). Корень-причина не идентифицирован (похоже на CU-глобальное type-inference/mono-ordering состояние, бleedящее между несвязанными выражениями при появлении НОВОГО reachable value-record типа — тот же класс, что уже задокументированные typedef-ordering/erasure-collision маркеры, но глубокая compiler-internals работа за пределами объёма этой волны). **Schema и `Offset.local()` ОСТАЮТСЯ как были до этой волны** (`local_offset_sec()->int`). Полный диагноз — file-header комментарий `std/time/duration/time_effect.nv` + `[M-offset-result-mono-bleed-if-let]` (`docs/plans/backlog-followups.md`).

**П5 — `nanos` на `Duration`/`Timestamp`/`Monotonic`: `ro` → `priv`** (D220 per-field; bare `priv` = module-private per D281 — не type-private, см. D220 §amend). Раньше `ro` означало «публично конструируемо `{ nanos: … }` из ЛЮБОГО модуля» — можно было слепить структурно-невалидный `Monotonic`/`Timestamp` руками. Публичное чтение (`@nanos()`/`@unix_nanos()` accessors) не затронуто — это методы, не field access. Публичное конструирование остаётся через существующие `to_*`-бланкеты (`Duration.to_nanos()`/`to_millis()`/…, `Timestamp.to_unix_nanos()`/…) — все уже живут В `time.duration`, module-private доступ не нарушен. **`Monotonic` не имел эквивалентного raw-конструктора** (только `.now()`, эффект-based, круговая зависимость для мок-хендлеров) — добавлен МИНИМАЛЬНЫЙ публичный escape-hatch `fn[T Ints] T @to_monotonic_nanos() -> Monotonic` (`monotonic.nv`), симметричный уже существующему `@nanos()` reader; НЕ `Monotonic.from_ns(...)` статик-конструктор (канон — receiver-form `T.to_*()`, D410-амендмент). Единственные внешние потребители (`std.testing.handlers` mock-фабрики + 2 ad-hoc test-хендлер-литерала в `testing/handlers/core_test.nv`/`time/civil/zoned_test.nv`) переведены на `.to_monotonic_nanos()`.

**⚠️ Найденный ПРЕ-СУЩЕСТВУЮЩИЙ (не внесённый этой волной) checker-gap, НЕ закрытый в рамках П5:** `E_PRIV_FIELD_INIT`/`E_FIELD_MODULE_PRIVATE` проверяет ТОЛЬКО explicit-typed record-литералы (`TypeName { field: … }`); bare-литерал с типом из контекста (`{ field: … }`, тип из return-position/let-annotation/…) обходит проверку — репро на СОВЕРШЕННО не связанном pre-existing `Stdin { priv unit int }` (`std.io`) подтверждает: архитектурный пробел уровня языка, не Time-специфичный. См. `[M-priv-field-bare-literal-context-infer-bypass]` (`docs/plans/backlog-followups.md`) — вне объёма этой волны (масштаб фикса — унификация priv-check в одном пост-резолюции чекпоинте вместо 25+ разбросанных `RecordLit`-сайтов, риск регрессии высок).

**П7 — `#default_handler` без аргумента.** `(EffectName)` теперь опционален: bare `#default_handler` инферит эффект из fn'ового собственного `-> Effect[X]` return-type (`default_handler_infer_effect_name`, `check_default_handlers`/`emit_c.rs` pre-pass читают ОДИНАКОВУЮ логику). DRY — имя всегда было избыточно с return-type. Explicit `#default_handler(X)`-форма остаётся валидной (unchanged) для случаев, когда автор хочет явности. Если ни explicit-имя, ни inference не дают эффект (fn не возвращает `Effect[X]` вовсе) — `E_DEFAULT_HANDLER_RETURN_TYPE` с явным сообщением про bare-форму.

**П8 — `time_default` → `real_time()`.** Нейминг-симметрия с `real_fs()`/`mock_fs()`-семьёй (описательное имя constructor'а — «что это» — вместо суффикса `_default`, который описывал РОЛЬ атрибута, не саму фабрику).

**П9 — `#default_handler` распространён на `real_fs()`/`real_io()`/`real_net()`/`real_os()`** (`std/src/fs/fs.nv`, `std/src/io/console.nv`, `std/src/net/tcp.nv`, `std/src/os/os.nv`) — bare-форма (П7). Эти четыре real-конструктора уже существовали (built отдельными предыдущими волнами) но НЕ авто-конструировались до `main` без явного `with` — теперь консистентны с `Time`/`real_time()`: любой оп базового эффекта, вызванный транзитивно без `with`, лениво устанавливает real-handler один раз на поток. `Mem`/`TimerMetrics` (built-in hardcoded vtable, `BUILTIN_VTABLE_NAMES`) и `Fail` (намеренно хардкод) НЕ тронуты — пост-релиз (`[M-...-mem-timermetrics-default-handler]`, вне объёма). `Random` не имеет `real_*()`-конструктора (CSPRNG-дефолт — hardcoded runtime-путь, `secure()`-фабрика — открытый вопрос) — не тронут.

### Гейт

Полный мега-CU `spec_tests/conformance` · `std/time`+`testing`+`concurrency`+`fs`+`net`+`os`+`io` targeted · все `with`-Time/effect-фикстуры (включая 78+ handler-литералов, мигрированных на полную декларацию — см. [D434](#d434)) · 5 CI-целей + `examples/flagship/aggregator` built `--strict-effects` · `nova check std --strict-effects` 0 новых находок (18 pre-existing `_neg`-фикстур + один pre-existing `nova check`-batch-artifact на `tzif.nv`, оба НЕ регрессия — подтверждено бисектом на нетронутом HEAD) · репро priv (внешний explicit-typed `Monotonic { nanos: … }` → `E_PRIV_FIELD_INIT`; bare-форма — известный gap, см. выше).

### Связь

Amends [D316](#d316) (перенос; `local_offset` retype ПОПРОБОВАН и ОТКАЧЕН, см. П11 выше) · Amends [D431](#d431-default_handlerx--ambient-lazy-default-handler-factory-для-эффектов-plan-175-ф2-v2-2026-07-2122) (опциональный аргумент, распространение на Fs/Net/Os/Io) · [D220](02-types.md#d220-per-field-visibility--priv-keyword--type-level-default-flip)/[D281](02-types.md#d281) (`priv` module-private per-field, П5) · [D410](02-types.md#d410) (`to_*` receiver-конструктор канон, П5 `to_monotonic_nanos`) · [07-modules §393](07-modules.md) (module-private видимость из co-equal файлов) · [D434](#d434) (mandatory handler-op decl, П4) · `docs/plans/175.2-typed-effects.md` Ф.2-v4 · `docs/plans/backlog-followups.md` (`[M-priv-field-bare-literal-context-infer-bypass]`, новый).

## D434. Handler-literal ops — обязательная полная декларация (`-> Type`) (Plan 175.2 Ф.2-v4 П4, 2026-07-22)

**Статус:** ✅ ACTIVE.

### Контекст

Везде в языке декларация операции полностью типизирована: обычные `fn` — всегда `-> ReturnType` (явно или инферится компилятором из тела, но AST несёт тип); effect-декларации (`type X effect { op() -> T }`) — return-type опционален ТОЛЬКО как sugar для implicit `()`, но контракт есть. Единственное исключение оставалось у handler-ЛИТЕРАЛОВ (`effect X { op(params) => … }` / `{ … }`) — синтаксис никогда не поддерживал `-> Type` вовсе (не «опционально и не писали», а «парсер не принимал»); codegen молча выводил ЛЮБОЙ nужный тип из эффект-схемы, независимо от того, что (если бы можно было) написал автор хендлера. Несоответствие вскрыто вычиткой владельца: единственное частично-типизированное место в языке.

### Решение

1. **Парсер (`parse_handler_methods`).** После списка параметров — опциональный `-> Type` (тот же `TokenKind::Arrow`, что у `fn`/effect-decl), парсится ОДИНАКОВО для ОБОИХ потребителей общей грамматики — `effect X {…}` (`HandlerLit`) И `protocol P {…}` (`ProtocolLit`, method-impl). Хранится в новом `HandlerMethod.ret_ty: Option<TypeRef>`.
2. **Обязательность — ТОЛЬКО для `HandlerLit`.** Новый чекер-проход `check_handler_op_declarations` (`types/mod.rs`, вызывается сразу после `check_default_handlers`) walk'ает модуль (entry + все peer files, включая нехватку в `check_handler_never_ops`, закрытую здесь той же инфраструктурой) в поисках `ExprKind::HandlerLit`; для каждого известного (effect-декларация резолвится в этом CU) опа: `ret_ty.is_none()` → `E_INCOMPLETE_HANDLER_OP_DECL`; `ret_ty.is_some()` → структурное сравнение (`typeref_equal`) с собственным `return_type` эффект-декларации (implicit `()`, если эффект-оп сам не указал `-> Type`) — mismatch → `E_HANDLER_OP_RETURN_TYPE_MISMATCH`. `ProtocolLit` НЕ затронут (не в объёме амендмента, method-impl остаются как были — `ret_ty` просто не обязателен).
3. **Codegen — БЕЗ изменений.** `emit_handler_lit` уже резолвит param/return C-типы из `effect_schemas` (эффект-декларации), игнорируя то, что написано в самом хендлер-литерале, — эта проверка чисто чекер-side (compile-time contract), не меняет как компилируется тело.
4. **Миграция.** ~78 handler-литерал-сайтов по всему `std`+`examples`+`spec_tests` (Fail/Time/Random/Supervisor/Application/ResourceTrace/Fs/Net/Os/Io/Db + ad-hoc test-only эффекты типа `Counter`/`Clock`/`Eff01..40`/`MakesPoint`/…) — механически переведены на полную форму (`op(params) -> Type => …`), сигнатуры взяты из соответствующих effect-деклараций.

### Границы

Требование применяется ТОЛЬКО когда и эффект, и оп резолвятся в известную `type X effect {…}` декларацию в этом CU — неизвестный эффект/оп не порождает НОВУЮ диагностику этим амендментом (оставлено другим механизмам). `ProtocolLit` (метод-имплементации протоколов) сознательно исключены — не в объёме находки владельца («handler-опы», не protocol-impls), синтаксическая опциональность там сохранена как была.

### Гейт

См. Гейт секции амендмента D316/D431 Ф.2-v4 выше (общий гейт волны).

### Связь

[D40](03-syntax.md#d40) (handler-method `=> expr`/`{ block }` синтаксис, параметры опциональны по типу — return-type теперь обязателен, симметрично) · [D142](#d142) (`protocol P {…}` литерал, ТА ЖЕ `parse_handler_methods`, НЕ затронут требованием) · Amend D316/D431 Ф.2-v4 (owner code-review, П4) выше · `docs/plans/175.2-typed-effects.md` Ф.2-v4.
