# Nova — парадигма: protocols + data, без классов

Классов нет. Наследования нет. Вместо них — связка из четырёх вещей,
которая покрывает всё, что обычно делают классами, но без их проблем.

## Четыре строительных блока

1. **`type`** — данные (record, sum-type, alias). Просто структура.
2. **`fn Type @method(...)`** — методы, привязанные к типу, с неявным
   self. Мутирующий метод — `fn Type mut @method(...)`.
3. **`protocol`** — контракт (что-то вроде Rust trait / Go interface),
   объявляется формой `type X protocol { ... }`. Структурный —
   единственный способ: номинальной формы нет.
4. **Структурное соответствие — автоматическое.** Тип, у которого уже
   есть методы нужной формы, удовлетворяет протоколу без отдельного
   шага «реализации»; блоков вроде `impl Protocol for Type` не
   существует.

Никакого `extends`, `super`, `protected`, `abstract class`. Вместо
наследования — **композиция + делегирование** одной строкой.

## Пример: «как класс, только лучше»

```nova
// === ДАННЫЕ ===
type Account {
    id u64
    owner str
    balance money
    mut closed bool   // mut — единственный способ мутации поля
}

// === КОНСТРУКТОР — это просто функция ===
fn Account.new(owner str) -> Account =>
    Account { id: ids.next(), owner, balance: money.zero, closed: false }

// === МЕТОДЫ ===
fn Account mut @deposit(amount money) Fail -> () {
    if @closed { throw ClosedAccount }
    if amount <= 0 { throw InvalidAmount }
    @balance += amount
}

fn Account mut @withdraw(amount money) Fail -> () {
    if amount > @balance { throw Overdraft }
    @balance -= amount
}

// Чистый геттер — выводится как pure, без побочных эффектов
fn Account @is_solvent() => @balance > 0
```

Использование:

```nova
mut acc = Account.new("alice")
acc.deposit(100)?
acc.withdraw(30)?
print(acc.balance)  // 70
```

`mut` перед `@name` в сигнатуре — единственный способ мутировать поля.
Если метод не пишет — `@name` без `mut`, и компилятор это проверяет.

## Полиморфизм через protocol

`Display` — не учебный пример, а реальный built-in protocol стандартной
библиотеки (`std/prelude/protocols.nv`,
[D422](decisions/02-types.md#d422)):

```nova
export type Display protocol {
    @display(mut f Fmt) -> ()
}
```

Пользовательский тип удовлетворяет ему структурно — без отдельного
шага «реализации»:

```nova
fn Account @display(mut f Fmt) -> () {
    f.write("Account(${@owner}, ${@balance})".bytes())
}
```

Ещё один тип-запись — та же схема; `Point` — собственный тип, никакого
extension-метода на чужом типе не требуется:

```nova
type Point {
    x f64
    y f64
}

fn Point @display(mut f Fmt) -> () {
    f.write("Point(${@x}, ${@y})".bytes())
}
```

Структурная совместимость — единственный механизм: если `Account` уже
имеет метод `@display(mut f Fmt) -> ()`, он автоматически удовлетворяет
`Display`. Отдельного шага «реализации» нет, `impl`-блоков в языке не
существует; номинальной формы тоже нет.

## Вместо наследования — embed + delegate

```nova
type AuditedAccount {
    use account Account    // встраивание: все поля + методы Account доступны напрямую
    audit_log []AuditEntry
}

// Переопределяем только то, что нужно
fn AuditedAccount mut @deposit(amount money) Fail -> () {
    @account.deposit(amount)?       // явный вызов «родителя» через имя поля
    @audit_log.push(AuditEntry.deposit(amount))
}
```

`use account Account` — это **delegation**, а не наследование: компилятор
генерирует прокси-методы. Никакого виртуального диспатча, никакого diamond
problem.

## Sum-types вместо иерархии классов

```nova
type Shape enum
    | Circle    { radius f64 }
    | Square    { side f64 }
    | Triangle  { a f64, b f64, c f64 }

fn Shape @area() => match @ {
    Circle { radius }     => 3.14159 * radius * radius
    Square { side }       => side * side
    Triangle { a, b, c }  => heron(a, b, c)
}
```

Добавил новый вариант — компилятор показывает все `match`, где не хватает
ветки.

## Динамический диспатч — protocol-тип в обычной позиции (existential)

По умолчанию — мономорфизация (нулевая стоимость). Коллекция
протокольных значений — обобщённая функция с bound'ом `[T Display]`
([D72](decisions/02-types.md#d72)), одна мономорфизация на конкретный `T`:

```nova
fn log_all[T Display](xs []T) -> () {
    for x in xs { print(x) }
}

log_all(accounts)      // []Account — одна мономорфизация
log_all([42, 7, 13])   // []int — другая, отдельная
```

Protocol-тип в обычной (не generic) позиции **параметра** — это и есть
runtime-полиморфизм (existential, vtable-вызов), без отдельного
keyword'а (D72: `fn f(x Hash)`):

```nova
fn describe(x Display) -> str => "${x}"

describe(acc)         // Account — структурное соответствие
describe(42)          // int — встроенный @display (std/prelude/protocols.nv)
describe("hello")     // str — встроенный @display
```

## Инкапсуляция — на уровне модуля

```nova
type Account { ... }                // публичный
type _internal_state { ... }        // приватный (префикс _)

pub fn Account.new(...) = ...       // публично
fn validate(...) = ...              // приватно для модуля
```

Два уровня видимости: либо `pub`, либо нет.

## «Как в ООП, только…»

| ООП-понятие | Nova |
|---|---|
| Класс | `type` + методы |
| Конструктор | обычная функция `Type.new(...)` |
| Наследование | `use Parent` (delegation) |
| Виртуальные методы | protocol (existential-позиция) или мономорфизация |
| Абстрактный класс | нет аналога: протоколы не несут реализаций по умолчанию ([D15](decisions/02-types.md#d15)) |
| Интерфейс | `protocol` (структурный — единственная форма) |
| Перегрузка методов | нет, разные имена |
| Перегрузка операторов | только через стандартные protocol'ы (`Add`, `Eq`, …) |
| `protected` | нет, только pub / module-private |
| `static` методы | просто функции в модуле |
| Singleton | модуль-уровень `let` |
| `instanceof` | `match` на sum-type |

## Главный тезис

«ООП vs функциональный» — ложная дихотомия. **Данные отдельно, поведение
отдельно, контракты отдельно** — это даёт всё хорошее от ООП (инкапсуляция,
полиморфизм) без плохого (наследование, fragile base class, божественные
классы).
