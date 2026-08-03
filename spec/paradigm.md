> ⚠️ **Частично актуализировано (2026-08-03).** Синтаксис примеров
> приведён к действующему языку: `mut self` → `mut @field`
> ([D35](decisions/03-syntax.md#d35)); `trait`/`impl` → `protocol`
> как kind-токен, `impl`-блоков в языке нет
> ([D53](decisions/02-types.md#d53), [D15](decisions/02-types.md#d15));
> `throws` → `Fail`/`Fail[E]` ([D25](decisions/04-effects.md#d25),
> [D65](decisions/04-effects.md#d65)); двоеточие в аннотациях типа
> убрано (бесколонная форма); `type X = { поля }` → `type X { поля }`,
> sum-тип получил обязательный `enum`-маркер
> ([D52](decisions/02-types.md#d52), [D406](decisions/02-types.md#d406)).
> Пункты `alias через =`, эффекты `Async`/`Mut`/`Par`, keyword `resume`
> и `to_str`/`ToStr`-протокол в тексте документа не встречались — менять
> было нечего.
>
> Три места остались непереведёнными — прямого эквивалента в текущем
> языке нет, отмечены `<!-- TODO(paradigm-actualize) -->` по месту и
> ждут решения владельца: пример «`Printable` для `int`» и завязанный на
> него пример `items []Printable` (extension-методы на чужом/примитивном
> типе запрещены, D46), и строка «Абстрактный класс» в таблице «Как в
> ООП, только…» (в языке нет default-методов протокола, D15, и нет
> аналога abstract class).
>
> **Актуальная парадигма** — в [decisions/](decisions/), [syntax.md](syntax.md).

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

```nova
type Printable protocol {
    show() -> str
}

fn Account @show() -> str => "Account(${@owner}, ${@balance})"
```

<!-- TODO(paradigm-actualize): пример «Printable для int» ниже не имеет
     прямого эквивалента в действующем языке — extension-методы на
     чужом/примитивном типе запрещены (03-syntax.md#d35, раздел
     «Receiver — любой тип, включая примитивы»: методы на встроенных
     типах определяются только в stdlib-модулях, пользовательский код
     не может добавить методы на чужих типах). `int` — foreign type
     для пользовательского кода, `int @show()` пользователь объявить
     не может. Ниже — исходный (устаревший, `impl`/`self`) фрагмент,
     оставлен как есть до решения владельца, чем заменить иллюстрацию. -->

```nova
impl Printable for int {
    fn show(self) = self.to_str()
}
```

```nova
fn log_all(xs []Printable) {
    for x in xs { print(x.show()) }
}
```

Структурная совместимость — единственный механизм: если `Account` уже
имеет метод `show() -> str`, он автоматически удовлетворяет
`Printable`. Отдельного шага «реализации» нет, `impl`-блоков в языке не
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

<!-- TODO(paradigm-actualize): пример ниже зависит от того же
     заблокированного случая, что и «Printable для int» выше (42,
     "hello" должны удовлетворять Printable, а extension-методы на
     чужих/примитивных типах запрещены, D46) — оставлен как есть до
     решения владельца. -->
```nova
ro items []Printable = [acc, 42, "hello"]
for x in items { print(x.show()) }  // vtable-вызов
```

По умолчанию — мономорфизация (нулевая стоимость): `fn f[T Printable](x T)`.
Protocol-тип в обычной (не generic) позиции значения — это и есть
runtime-полиморфизм (existential, vtable-вызов), без отдельного keyword'а.

## Инкапсуляция — на уровне модуля

```nova
type Account { ... }                // публичный
type _internal_state { ... }        // приватный (префикс _)

pub fn Account.new(...) = ...       // публично
fn validate(...) = ...              // приватно для модуля
```

Два уровня видимости: либо `pub`, либо нет.

## «Как в ООП, только…»

<!-- TODO(paradigm-actualize): строка "Абстрактный класс" ниже оставлена
     в исходном виде (`trait` с дефолтными методами) — прямого
     эквивалента нет: protocol не может нести дефолтные тела методов
     (D15, "Что отвергнуто: Дефолтные методы"), и конструкции уровня
     «абстрактный класс» в языке нет вовсе. Ждёт решения владельца, чем
     заменить строку. -->

| ООП-понятие | Nova |
|---|---|
| Класс | `type` + методы |
| Конструктор | обычная функция `Type.new(...)` |
| Наследование | `use Parent` (delegation) |
| Виртуальные методы | protocol (existential-позиция) или мономорфизация |
| Абстрактный класс | `trait` с дефолтными методами |
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
