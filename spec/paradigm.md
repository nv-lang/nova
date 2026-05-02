> ⚠️ **УСТАРЕЛО.** Этот документ описывает парадигму ранней версии (D1–D17),
> до решений D24/D31/D33–D42. Многое здесь синтаксически некорректно:
> `mut self` в параметрах, `trait`/`impl`, `throws` без `Throws[]`, `:` в
> аннотациях типа, `type X = { поля }` вместо `type X { поля }`.
>
> **Актуальная парадигма** — в [decisions.md](../decisions.md), [syntax.md](syntax.md).
> Этот файл будет переписан целиком (см. open-questions Q8).

# Nova — парадигма: traits + data, без классов

Классов нет. Наследования нет. Вместо них — связка из четырёх вещей,
которая покрывает всё, что обычно делают классами, но без их проблем.

## Четыре строительных блока

1. **`type`** — данные (record, sum-type, alias). Просто структура.
2. **`fn T.method(self, ...)`** — методы, привязанные к типу.
   Как в Go, но синтаксис ближе к Rust `impl`.
3. **`trait`** — контракт (что-то вроде Rust trait / Go interface).
   Структурный по умолчанию, номинальный по требованию.
4. **`impl Trait for Type`** — реализация трейта. Можно для чужого типа
   (как в Rust).

Никакого `extends`, `super`, `protected`, `abstract class`. Вместо
наследования — **композиция + делегирование** одной строкой.

## Пример: «как класс, только лучше»

```nova
// === ДАННЫЕ ===
type Account = {
    id: u64
    owner: str
    balance: money
    mut closed: bool   // mut — единственный способ мутации поля
}

// === КОНСТРУКТОР — это просто функция ===
fn Account.new(owner: str) -> Account =
    Account { id: ids.next(), owner, balance: money.zero, closed: false }

// === МЕТОДЫ ===
fn Account.deposit(mut self, amount: money) throws -> () =
    if self.closed { throw ClosedAccount }
    if amount <= 0 { throw InvalidAmount }
    self.balance += amount

fn Account.withdraw(mut self, amount: money) throws -> () =
    if amount > self.balance { throw Overdraft }
    self.balance -= amount

// Чистый геттер — выводится как pure, без побочных эффектов
fn Account.is_solvent(self) = self.balance > 0
```

Использование:

```nova
let mut acc = Account.new("alice")
acc.deposit(100)?
acc.withdraw(30)?
print(acc.balance)  // 70
```

`mut self` в сигнатуре — единственный способ мутировать. Если метод не
пишет — `self` без `mut`, и компилятор это проверяет.

## Полиморфизм через trait

```nova
trait Printable {
    fn show(self) -> str
}

impl Printable for Account {
    fn show(self) = "Account(${self.owner}, ${self.balance})"
}

impl Printable for int {
    fn show(self) = self.to_str()
}

fn log_all(xs: [impl Printable]) =
    for x in xs { print(x.show()) }
```

Структурный bonus: если `Account` уже имеет метод `show(self) -> str`,
его не обязательно объявлять `impl Printable` явно — компилятор видит
совпадение по форме. Но если хочется номинальной строгости, пишешь
`impl` явно.

## Вместо наследования — embed + delegate

```nova
type AuditedAccount = {
    use Account            // встраивание: все поля + методы Account доступны напрямую
    audit_log: [AuditEntry]
}

// Переопределяем только то, что нужно
fn AuditedAccount.deposit(mut self, amount: money) throws -> () =
    self.Account.deposit(amount)?       // явный вызов «родителя»
    self.audit_log.push(AuditEntry.deposit(amount))
```

`use Account` — это **delegation**, а не наследование: компилятор генерирует
прокси-методы. Никакого виртуального диспатча, никакого diamond problem.

## Sum-types вместо иерархии классов

```nova
type Shape =
    | Circle    { radius: f64 }
    | Square    { side: f64 }
    | Triangle  { a: f64, b: f64, c: f64 }

fn Shape.area(self) = match self {
    Circle { radius }     -> 3.14159 * radius * radius
    Square { side }       -> side * side
    Triangle { a, b, c }  -> heron(a, b, c)
}
```

Добавил новый вариант — компилятор показывает все `match`, где не хватает
ветки.

## Динамический диспатч — через `dyn Trait`

```nova
let items: [dyn Printable] = [acc, 42, "hello"]
for x in items { print(x.show()) }  // vtable-вызов
```

По умолчанию — мономорфизация (нулевая стоимость). `dyn` — только когда
явно нужен runtime-полиморфизм.

## Инкапсуляция — на уровне модуля

```nova
type Account = { ... }              // публичный
type _internal_state = { ... }      // приватный (префикс _)

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
| Виртуальные методы | trait + `dyn Trait` или мономорфизация |
| Абстрактный класс | `trait` с дефолтными методами |
| Интерфейс | `trait` (структурный или номинальный) |
| Перегрузка методов | нет, разные имена |
| Перегрузка операторов | только через стандартные traits (`Add`, `Eq`, …) |
| `protected` | нет, только pub / module-private |
| `static` методы | просто функции в модуле |
| Singleton | модуль-уровень `let` |
| `instanceof` | `match` на sum-type |

## Главный тезис

«ООП vs функциональный» — ложная дихотомия. **Данные отдельно, поведение
отдельно, контракты отдельно** — это даёт всё хорошее от ООП (инкапсуляция,
полиморфизм) без плохого (наследование, fragile base class, божественные
классы).
