---
source_rev: 21dff1b37
source_date: 2026-08-02
---

[English](language-tour.md) | **Русский**

<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Экскурсия по языку Nova

Рабочая экскурсия по Nova для читателя, который никогда не видел язык, —
не полная спецификация. Каждый пример на этой странице — реальный,
компилирующийся и запускаемый `.nv`-файл (`nova build` + запуск бинаря, или
`nova test`, где отмечено); ничего из этого не «задумано». Исходники лежат в
[`examples/tour/`](../../examples/tour) в репозитории Nova, если хотите
собрать их сами.

Nova компилируется в C, а затем в нативный бинарь — интерпретатора нет. См.
[quickstart.md](quickstart.md), если вы ещё ничего не собирали.

## 1. Hello, функции, переменные

`ro` объявляет read-only биндинг (никогда не переназначается); `mut`
объявляет переназначаемый. Типы выводятся почти во всех позициях —
пишите их явно только там, где это помогает читателю (сигнатура функции или
`mut`-биндинг, чьё начальное значение не делает тип очевидным).

```nova
// hello.nv — functions, let/mut, type inference.
module tour.hello

fn add(a int, b int) -> int => a + b

fn main() {
    ro name = "Nova"                 // inferred: str
    mut count int = 0                // explicit type, still inferred-friendly
    count = count + 1
    println("Hello, ${name}! count=${count}, add(2,3)=${add(2, 3)}")
}
```

```
Hello, Nova! count=1, add(2,3)=5
```

## 2. Типы: записи, типы-суммы, Option/Result, дженерики

`type X { ... }` объявляет **запись** (record) — heap-аллоцированный,
управляемый GC ссылочный тип. **Тип-сумма** требует маркер `enum`
(`type X enum A | B | C`, [D406](../../spec/decisions/02-types.md#d406-sum-type-синтаксис-через-enum-маркер-2026-07-01)) —
более старая «голая» форма с ведущим `|` выведена. `Option[T]`/`Result[T, E]` —
обычные типы-суммы из prelude. `[T]` на функции вводит параметр
дженерического типа.

```nova
// types.nv — records, sum types (D406 `enum` marker), Option/Result, generics.
module tour.types

// record — heap-allocated reference type, GC-managed (`{}` braces).
type Point { x f64, y f64 }

// sum type — the `enum` marker is mandatory (D406); leading `|` alone is not
// valid syntax anymore. Inline form: first variant has no leading `|`.
type Shape enum
    | Circle(f64)
    | Rectangle(f64, f64)

fn area(s Shape) -> f64 => match s {
    Circle(r)       => 3.14 * r * r
    Rectangle(w, h) => w * h
}

// Generic function — [T] introduces a type parameter.
fn first[T](xs []T) -> Option[T] {
    if xs.len() == 0 { None } else { Some(xs[0]) }
}

fn main() {
    ro p = Point { x: 1.0, y: 2.0 }
    println("p = (${p.x}, ${p.y})")

    ro c = Circle(2.0)
    ro r = Rectangle(3.0, 4.0)
    println("area(circle)=${area(c)} area(rect)=${area(r)}")

    ro xs = []int.of(10, 20, 30)
    ro empty = Vec[int].new()   // `.of()` requires >=1 arg (D259); empty is `.new()`
    println("first(xs)=${first(xs)} first(empty)=${first(empty)}")
}
```

```
p = (1, 2)
area(circle)=12.56 area(rect)=12
first(xs)=Some(10) first(empty)=None
```

## 3. Методы, протоколы, property-методы

Метод объявляется *вне* тела типа: `fn Type [mut] @name(...) -> R`.
Внутри тела `@field` читает поле приёмника. **Свойства по арности**
(D84/D409) позволяют одному имени быть и геттером, и сеттером: `@x() -> T`
читает, `mut @x(v T) -> @` пишет и флюэнтно возвращает приёмник (явный
`return self` не нужен — D409 делает это автоматически). `protocol` объявляет
структурный интерфейс; `#impl(...)` явно оптово подключает тип к протоколу
(D186).

```nova
// methods.nv — @-methods, protocols (#impl), property methods by arity.
module tour.methods

type Counter value { mut n int }

fn Counter mut @inc() -> () {
    @n += 1
}

// Property-by-arity (D84/D409): reading is `@x() -> T`, writing is
// `mut @x(v T) -> @` — a fluent setter that returns the receiver.
fn Counter @value() -> int => @n
fn Counter mut @value(v int) -> @ {
    @n = v   // D409: fluent setter, receiver returned automatically
}

// `protocol` = structural interface (behavior contract on a type).
// `#impl(...)` opts a type into a protocol explicitly (D186).
type Sized protocol {
    @size() -> int
}

type Box { items []int }
fn Box @size() -> int => @items.len()

fn sum_sizes[T Sized](a T, b T) -> int => a.size() + b.size()

fn main() {
    mut c = Counter { n: 0 }
    c.inc()
    c.inc()
    println("counter after two inc = ${c.value()}")

    ro c2 = c.value(100)   // fluent setter, returns @ (Counter)
    println("after fluent set = ${c2.value()}")

    ro b1 = Box { items: []int.of(1, 2, 3) }
    ro b2 = Box { items: []int.of(4, 5) }
    println("sum_sizes = ${sum_sizes(b1, b2)}")
}
```

```
counter after two inc = 2
after fluent set = 100
sum_sizes = 5
```

## 4. Сопоставление с образцом

`match` поддерживает литеральные паттерны, гварды (`n if n > 0`) и
деструктуризацию вариантов типа-суммы. `if <Pattern> = expr { } else { }` —
это if-let-форма Nova: она сопоставляет одновариантный паттерн и биндит
внутри `if`, падая в `else` при отсутствии совпадения (отдельного ключевого
слова `if let` нет).

```nova
// patterns.nv — match, guards, `if <Pattern> = expr` (if-let form), record match.
module tour.patterns

type Shape enum
    | Circle(f64)
    | Square(f64)

fn describe(n int) -> str => match n {
    0          => "zero"
    n if n > 0 => "positive"
    _          => "negative"
}

fn area(s Shape) -> f64 => match s {
    Circle(r) => 3.14 * r * r
    Square(a) => a * a
}

fn main() {
    println("describe(0)=${describe(0)} describe(5)=${describe(5)} describe(-3)=${describe(-3)}")
    println("area(Circle(2.0))=${area(Circle(2.0))}")

    ro opts []Option[int] = [Some(1), None, Some(3)]
    mut total = 0
    mut missing = 0
    for o in opts {
        if Some(v) = o {
            total += v
        } else {
            missing += 1
        }
    }
    println("total=${total} missing=${missing}")
}
```

```
describe(0)=zero describe(5)=positive describe(-3)=negative
area(Circle(2.0))=12.56
total=4 missing=1
```

## 5. Ошибки: Result + `?`, panic

Правило обработки ошибок в Nova ([docs/dev/idioms/error-handling.md](../dev/idioms/error-handling.md)):
**panic** — для нарушенного контракта вызывающего (баг программиста —
выход за границы, нарушенный `requires`) и никогда не восстановим;
**`Result[T, E]`** — для восстанавливаемого отказа с исследуемой причиной,
а `?` пробрасывает `Err` наружу из объемлющей функции, возвращающей `Result`
(D85). `Option[T]` зарезервирован для подлинного отсутствия (`find`, `get`),
а не для отказа — «опционального» близнеца у падающей операции нет.

```nova
// errors.nv — Result + `?`, panic for programmer-bug invariants.
module tour.errors

type ParseErr enum Empty | BadDigit

fn parse_digit(s str) -> Result[int, ParseErr] {
    if s.byte_len() == 0 { return Err(Empty) }
    ro c = s.bytes()[0]
    if c < 48 || c > 57 { return Err(BadDigit) }
    Ok((c as int) - 48)
}

// `?` propagates Err out of a Result-returning function (D85).
fn parse_two(a str, b str) -> Result[int, ParseErr] {
    ro x = parse_digit(a)?
    ro y = parse_digit(b)?
    Ok(x + y)
}

fn main() {
    match parse_two("3", "4") {
        Ok(sum)  => println("sum = ${sum}")
        Err(_e)  => println("parse failed")
    }
    match parse_two("3", "x") {
        Ok(sum) => println("sum = ${sum}")
        Err(_e) => println("parse failed as expected")
    }

    // panic: an invariant the CALLER was required to satisfy — a genuine
    // programmer bug, not a recoverable external-input error. Never used for
    // bad user input/network/files (those are Result).
    ro xs = []int.of(1, 2, 3)
    if xs.len() > 5 {
        ro _oob = xs[10]  // would panic: out-of-bounds is not reached here
    }
    println("done")
}
```

```
sum = 7
parse failed as expected
done
```

## 6. Эффекты в типах

Сеть, диск, часы, случайность, логирование, мутация, ошибки — в Nova всё это
**эффекты**. Функция объявляет в сигнатуре ровно те эффекты, которые *сама*
выполняет; вызов другой функции не подтягивает её эффекты в сигнатуру
вызывающего (единственное исключение — `Fail`, распространяющийся
транзитивно). У каждого эффекта есть **обработчик**, перехватывающий его
операции и подставляемый через `with Handler = effect X { ... } { body }` —
подмените на детерминированную заглушку для тестов, не трогая
тестируемую функцию. Это отличается от `protocol`: эффект — это «как что-то
сделать» (подменяемая реализация), протокол — «что значение умеет»
(фиксировано для типа). Внутренние модули (`std/**`) и программы
(`examples/**`) собираются под `--strict-effects` (`nova build
--strict-effects`) — экспериментальным флагом, который повышает
предупреждения о необъявленных транзитивных эффектах и стирании эффектов до
жёстких ошибок.

```nova
// effects_tour.nv — effects in function signatures, handler substitution (D61).
module tour.effects_tour

type Counter effect {
    next() -> int
}

// `Counter` in the signature says: this function performs Counter's
// operations, but does NOT say who handles them.
fn count_three() Counter -> int {
    ro a = Counter.next()
    ro b = Counter.next()
    ro c = Counter.next()
    a + b + c
}

fn main() {
    mut state = 0
    with Counter = effect Counter {
        next() {
            state += 1
            return state
        }
    } {
        ro total = count_three()
        println("total = ${total}")   // 1 + 2 + 3 = 6
    }
}
```

```
total = 6
```

## 7. Владение: consume, defer, авто-`@cleanup` (D432 — новое в этом релизе)

`consume`-типизированный биндинг отслеживается по владению. Исторически (D133)
он должен был быть потреблён **ровно один раз**, иначе компилятор отклонял
программу — строго линейный. **D432** (новое в этом релизе) позволяет типу
`consume` оптово перейти на **аффинную** дисциплину: если он объявляет
эффект-чистый `@cleanup(outcome ScopeOutcome) -> ()`, компилятор
автоматически вставляет его вызов на любом пути выхода, где значение ещё
живо — забыть употребить больше не ошибка. Типы без `@cleanup` сохраняют
старое строго-линейное поведение без изменений. `defer { ... }` выполняется
при выходе из области видимости, LIFO для нескольких `defer` в одной области
(D189).

```nova
// consume_tour.nv — consume-params/bindings, defer, auto-`@cleanup` (D432).
module tour.consume_tour

// `consume` = an ownership-tracked type; a `consume` binding must be
// consumed exactly once UNLESS it declares `@cleanup` (see below).
type Resource consume { id int }

// A type with an effect-pure `@cleanup` shifts from linear (must-consume)
// to affine (may-forget) — the compiler auto-inserts `@cleanup(outcome)` on
// any dangling exit path (D432, new in this release). Without `@cleanup`,
// forgetting to consume is a compile error (D133) — that's the strict form.
fn Resource consume @cleanup(_outcome ScopeOutcome) -> () {
    ()
}

fn Resource consume @close() -> () { () }

fn make(id int) -> Resource => { id }

fn main() {
    // `defer { ... }` runs at scope exit, LIFO for multiple defers (D189).
    {
        defer { println("first defer registered, runs LAST") }
        defer { println("second defer registered, runs FIRST") }
        println("inside scope")
    }
    println("explicit-consume demo below")
}

// A bare consume-let, never explicitly consumed: this COMPILES (D432
// auto-cleanup covers it) — before D432 this was a D133 compile error.
test "D432: bare consume-let never touched still compiles + runs" {
    consume r = make(1)
    ro _id = r.id
    assert(_id == 1)
}

// Explicit consume still works exactly as before D432 (strict linear form).
test "explicit consume + close still works (pre-D432 form)" {
    consume r2 = make(2)
    r2.close()
    assert(true)
}
```

Вывод `nova build` (только `main` — `test`-блоки запускаются под `nova test`):

```
inside scope
second defer registered, runs FIRST
first defer registered, runs LAST
explicit-consume demo below
```

`nova test` на том же файле: `PASS: 2  FAIL: 0`.

## 8. Конкурентность: spawn, parallel for, supervised, каналы

Нет ни `async fn`, ни `.await`, ни `Future<T>`. `Time`, появившаяся в
сигнатуре функции, — единственный маркер того, что она касается часов:
конкурентность структурная, а не отдельный async-диалект. `spawn` внутри
блока `supervised` запускает fiber в стиле fire-and-forget; `supervised(deadline:)`
даёт блоку общий дедлайн, и spawn, не уложившийся в него, по-настоящему
отменяется, а не остаётся работать с выброшенным результатом.
`parallel for` разворачивает однородную работу и собирает результаты в
`[]T` по порядку. `Channel.new(cap)` возвращает пару с разделёнными
капабилити `{ tx, rx }`. Это та же форма, что в
[`examples/mini_aggregator.nv`](../../examples/mini_aggregator.nv) (см.
[quickstart.md](quickstart.md)) и флагманском демо
[`examples/flagship/aggregator`](../../examples/flagship/aggregator).

```nova
// concurrency.nv — spawn, parallel for, supervised(deadline:), channels.
module tour.concurrency

import std.time.duration

fn probe(latency_ms int, deadline Monotonic) Time -> str {
    ro { tx, rx } = Channel.new(1)
    // A `with Fail[T]` handler runs IN THE FIBER of the failing operation,
    // not the installing scope's fiber (D441, spec/decisions/06-concurrency.md)
    // — a bare `mut` flag captured there is a data race under M:N.
    // `AtomicBool` is the synchronized alternative (D415 whitelist).
    mut timed_out = AtomicBool.new(false)
    with Fail[TimeoutError] = |_e| { timed_out.store(true) } {
        supervised(deadline: deadline) {
            spawn {
                Time.sleep(latency_ms)
                ro _ = tx.try_send(true)
            }
        }
    }
    if timed_out.load() {
        "cancelled"
    } else {
        match rx.try_recv() {
            Some(_) => "done"
            None    => "cancelled"
        }
    }
}

// `parallel for` — homogeneous fan-out: all iterations start at once,
// results collected into a []T in order.
fn fan_out(latencies []int, deadline Monotonic) Time -> []str {
    ro outcomes = parallel for i int in 0..latencies.len() {
        probe(latencies[i], deadline)
    }
    outcomes
}

fn main() Time {
    ro latencies []int = [10, 20, 300]   // ms; last one misses the budget
    ro t0 = Monotonic.now()
    ro deadline = t0 + 60.to_millis()
    ro outcomes = fan_out(latencies, deadline)
    mut done = 0
    mut cancelled = 0
    for i int in 0..outcomes.len() {
        if outcomes[i] == "done" { done += 1 } else { cancelled += 1 }
    }
    println("done=${done} cancelled=${cancelled}")
}
```

```
done=2 cancelled=1
```

(В общем случае зависит от таймингов, но структурно: два источника
укладываются в бюджет 60 мс, 300-мс источник отменяется — а не молча
выбрасывается после завершения.)

## 9. Коллекции: Vec, HashMap, итераторы

`[]T` — **синтаксический алиас** для `Vec[T]` — методы работают прямо со
значением `[]T`, никакого boilerplate `.iter()` для вызова адаптеров не
нужно. Map-литерал `[k: v, ...]` напрямую конструирует `HashMap[K, V]`
(D108). У `Option`/`Result` есть настоящий монадический `flat_map` (bind,
без вложенности `Option[Option[U]]`) и `filter` (отбросить `Some` по
предикату).

```nova
// collections_tour.nv — Vec ([]T is a syntactic alias), HashMap, iterators.
module tour.collections_tour

import std.collections.hashmap.{HashMap}
import std.collections.vec_iter

fn main() {
    // `[]T` is a syntactic alias for `Vec[T]` — methods work directly, no
    // `.iter()` needed to call adapters like `.filter()`/`.count()`.
    ro xs []int = []int.of(1, 2, 3, 4, 5, 6, 7, 8)
    println("count=${xs.iter().count()} evens=${xs.iter().filter(|x| x % 2 == 0).count()}")

    // Map-literal `[k: v, ...]` constructs a HashMap[K, V] directly (D108).
    ro m HashMap[str, int] = ["a": 1, "b": 2, "c": 3]
    ro key = "b"
    ro val = m.get(key)
    println("m.len()=${m.len()} m[b]=${val}")

    // Option/Result combinators: flat_map for real monadic bind (no nested
    // Option[Option[U]]), filter to drop a Some by predicate.
    ro port = Some(10).flat_map(|x| Some(x * 100)) ?? 8080
    ro none_port = (None as Option[int]).flat_map(|x| Some(x * 100)) ?? 8080
    println("port=${port} none_port=${none_port}")

    ro evens_only = Some(10).filter(|x| x % 2 == 0)
    ro odds_dropped = Some(3).filter(|x| x % 2 == 0)
    println("evens_only=${evens_only} odds_dropped=${odds_dropped}")
}
```

```
count=8 evens=4
m.len()=3 m[b]=Some(2)
port=1000 none_port=8080
evens_only=Some(10) odds_dropped=None
```

## 10. Строки и форматирование: `${}`, Display против Debug

`"${expr}"` интерполирует через протокол `Display` (`@display`) — просто,
для показа пользователю. `"${expr:?}"` маршрутизируется через `Debug`
(`@debug`) — диагностическая форма: `str` в Debug берётся в кавычки с
эскейпами, а в Display остаётся без кавычек; `int`/`bool` в обоих случаях
выглядят одинаково. `Option`/`Result` отлаживаются как
`Some(v)`/`None`/`Ok(v)`/`Err(e)`.

```nova
// strings_tour.nv — `${}` interpolation, Display vs Debug format-spec `:?`.
module tour.strings_tour

fn main() {
    ro name = "Nova"
    ro n = 42
    // Plain interpolation routes through Display (@display) — bare values.
    println("hello ${name}, n=${n}")

    // `${expr:?}` routes to Debug (@debug) instead of Display — e.g. a str
    // gets quoted with escapes under Debug, bare under Display; primitives
    // like int/bool are the same either way.
    ro s = "hi"
    println("display=${s} debug=${s:?}")

    ro some = Some(7)
    ro none = None as Option[int]
    println("debug(some)=${some:?} debug(none)=${none:?}")
}
```

```
hello Nova, n=42
display=hi debug="hi"
debug(some)=Some(7) debug(none)=None
```

Тип также может оптово включить `#impl(Debug)`, чтобы получить производный
компилятором почленный рендер `TypeName { field: value }` (см.
[D229](../../spec/decisions/02-types.md) и
`spec_tests/conformance/d229_debug_format_spec.nv`) — там это проверяется
через `nova test` и `assert`.

## 11. Модули: папка = модуль, импорты, nova.toml

**Модуль** — это либо один файл `X.nv`, либо **папка** `X/`, чьи
равноправные файлы объявляют один и тот же путь `module` и разделяют одно
пространство имён — элементы одного равноправного файла видны в другом без
импорта. Каждый импорт-путь полностью квалифицирован от **package**-корня
(директории с `nova.toml`); собственные модули пакета импортируются так же,
как внешнего пакета — например, `std.collections.vec` тянется в пакет `std`
откуда угодно, в том числе из другого модуля внутри самого `std`.

```nova
// greeter/core.nv — a FOLDER is one module made of co-equal peer files
// (tour.greeter), not one file per type. Both files here declare the same
// `module tour.greeter` (see loud.nv).
module tour.greeter

export type Greeting { text str }

export fn greet(name str) -> Greeting => { text: "Hello, ${name}!" }
```

```nova
// greeter/loud.nv — peer file, SAME module `tour.greeter` as core.nv.
// Items declared in either file are visible to both without an import —
// a folder-module shares one namespace across its peer files.
module tour.greeter

import std.unicode

export fn shout(g Greeting) -> str => g.text.to_upper()
```

```nova
// modules_tour.nv — importing a folder-module (tour.greeter, see
// tour/greeter/{core,loud}.nv). Every import path is fully qualified from
// the PACKAGE root — this file's package is `nova_examples` (declared in
// ../nova.toml), so a sibling folder-module is `nova_examples.tour.greeter`,
// same shape `std.collections.vec` uses to reach into the `std` package.
module tour.modules_tour

import nova_examples.tour.greeter.{greet, shout}

fn main() {
    ro g = greet("Nova")
    println(g.text)
    println(shout(g))
}
```

```
Hello, Nova!
HELLO, NOVA!
```

Минимальный `nova.toml` в корне пакета объявляет имя пакета и версию — см.
[quickstart.md](quickstart.md#hello-nova) для самого маленького из возможных.
Workspace (`[workspace] members = [...]`) группируют несколько пакетов в
monorepo, как это делает корневой `nova.toml` этого репозитория для
`std/`, `examples/` и `spec_tests/`.

## 12. FFI и unsafe, кратко

Тип opaque-указателя Nova — `*()` (указатель на unit — `void*` в C); старый
встроенный тип `ptr` удалён. Оборачивайте сырой `*()` в запись для
**типизированного хендла**, чтобы разные нативные ресурсы (файловый хендл
против сокетного) не были взаимозаменяемы на этапе компиляции, хотя на C-стороне
оба — `void*`. `external fn name(args) -> ret` (D82) объявляет привязку к
C-символу; полный cookbook — послойные обёртки, кортежные возвраты по
значению, линковка статической/разделяемой библиотеки через
`[ffi]`/`[ffi.staticlib]` в `nova.toml` — в
[docs/guide/ffi-cookbook.md](ffi-cookbook.md).

```nova
// ffi_tour.nv — FFI basics: opaque pointer `*()`, typed handles, `external fn`.
// Full cookbook: docs/guide/ffi-cookbook.md. `ptr` as a built-in type was removed
// (Plan 134) — `*()` (pointer to unit = `void*` in C) is used everywhere.
module tour.ffi_tour

// A typed handle wraps a raw `*()` in a record so distinct resources
// (FileHandle vs SocketHandle) are NOT interchangeable at compile time,
// even though both are `void*` on the C side.
type FileHandle { ro value *() }

fn main() {
    // NULL literal — bitwise-zero opaque pointer.
    ro nothing *() = (0 as *())

    // *() constructed from an integer (normally this would come back from
    // an `external fn` call into a C library).
    ro raw *() = 0x1000 as *()

    // Round-trip cast: *() -> int -> *() (same bit pattern).
    ro raw_as_int = raw as int
    ro raw_back *() = raw_as_int as *()
    println("raw == raw_back: ${raw == raw_back}")

    ro handle = FileHandle { value: raw }
    println("handle.value as int = ${handle.value as int}")
    println("nothing as int = ${nothing as int}")
}
```

```
raw == raw_back: true
handle.value as int = 4096
nothing as int = 0
```

## Куда дальше

- [spec/overview.md](../../spec/overview.md) — центральная идея (эффекты),
  killer-юзкейс и поддерживающие дизайн-решения на одной странице.
- [spec/decisions/](../../spec/decisions/) — журнал решений с номерами D,
  авторитетный источник для каждого кусочка синтаксиса и семантики Nova;
  каждая конструкция в этой экскурсии прослеживается до решения там.
- [docs/guide/quickstart.md](quickstart.md) — установка, сборка и запуск
  `examples/mini_aggregator.nv` от начала до конца.
- [examples/flagship/aggregator](../../examples/flagship/aggregator) —
  полноразмерная версия примера конкурентности: настоящий HTTP-сервер, веб-UI
  и та же проверяемая эффектами сигнатура под `--strict-effects`.
- [docs/dev/idioms/error-handling.md](../dev/idioms/error-handling.md),
  [docs/guide/channels.md](channels.md), [docs/guide/ffi-cookbook.md](ffi-cookbook.md),
  [docs/guide/cleanup-cookbook.md](cleanup-cookbook.md) — глубже про
  ошибки, каналы/`select`, FFI и consume/cleanup соответственно.
- [docs/dev/test-conventions.md](../dev/test-conventions.md) — как работают
  `nova test` и маркеры `EXPECT_*`.
