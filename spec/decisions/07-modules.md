# Modules — модули, импорты, видимость

Решения этой группы определяют, как код Nova организован в файлы и
как видимость деклараций контролируется между модулями.

| # | Решение |
|---|---|
| [D5](#d5-видимость-только-export-или-приватно) | Видимость: только `export` или приватно |
| [D29](#d29-модули-и-импорты) | Модули и импорты |
| [D47](#d47-видимость-деклараций) | Видимость деклараций (расширение D5) |

---

## D5. Видимость: только `export` или приватно

### Что
Два уровня видимости: **`export`** (экспортируется из модуля) и
**ничего** (приватно для модуля). Никаких `protected`, `internal`,
`package-private`. Полная семантика — в [D47](#d47-видимость-деклараций).

### Правило

```nova
export fn process(...)        // видна снаружи модуля
fn helper(...)                // видна только внутри
```

Применяется ко всем видам деклараций — `type`, `fn` (свободная,
static, метод), `const`, `let`, `protocol` ([D42](02-types.md#d42)).

### Почему

- **Go видимость через CamelCase** мешает называть вещи естественно.
- **Java/C# четыре уровня** — overkill, никто не помнит правила.
- **Два уровня** — достаточно для 99% случаев, остальное решается
  через структуру модулей.

### Что отвергнуто

- **`pub` (Rust-стиль)** — сначала использовался, заменён на `export`:
  - **Симметрия с `import`** — `export X` / `import X` точные
    противоположности.
  - **Освобождает `use`** для embed/delegation внутри `type`
    ([D39](02-types.md#d39)). В Rust `pub` + `use` оба про модули; в
    Nova `use` уже занят embed'ом.
  - **AI-first** — слово длиннее, но смысл прозрачнее без знания
    Rust-сокращений.
- **`protected`/`package-private`** — сложно, никто не помнит.

### Связь

- [D29](#d29-модули-и-импорты) — полный синтаксис `import`/`export`.
- [D47](#d47-видимость-деклараций) — детали по полям, методам, type-level.
- [03-syntax.md → D30](03-syntax.md#d30) — convention `_prefix` для полей.

### Эволюция

Изначально использовалось `pub` (Rust-style), заменено на `export` для
симметрии с `import`. Подробно — [history/evolution.md](history/evolution.md).

---

## D29. Модули и импорты

### Что
**Файл = модуль.** Имя модуля объявляется первой строкой
`module path.to.name`. Импорт через `import path.module.{Names}`.
Wildcard и циклические импорты запрещены. Re-export — `export import`.

### Правило

#### Объявление модуля

```nova
module admin.audit                              // первая строка файла
```

Имя модуля — иерархическое через точки. **Соответствует пути в
файловой системе** относительно корня проекта: `admin.audit` →
`admin/audit.nv`.

Один файл = один модуль. Никаких `module X { ... }`-блоков внутри
файла.

#### Структура проекта

```
project/
├── src/
│   ├── main.nv                  module main
│   ├── admin/
│   │   ├── audit.nv             module admin.audit
│   │   └── users.nv             module admin.users
│   └── http/
│       └── server.nv            module http.server
└── nova.toml                    манифест проекта
```

Корень — `src/`. Каждый `.nv` файл — отдельный модуль. Структура
проекта (где `src/`, как устроен `nova.toml`) — это **tooling**, см.
открытые вопросы.

#### Импорт

```nova
// импорт одного имени
import std.io.File

// импорт нескольких имён из модуля
import std.io.{File, Reader, Writer}

// импорт модуля как namespace
import std.collections                          // → std.collections.HashMap

// алиас одного имени
import std.option.Option as Opt

// алиас модуля
import std.collections as cols                  // → cols.HashMap

// смешанный — несколько имён с алиасами
import std.io.{File as F, read_all}
```

Импортированные имена попадают в **локальный скоуп модуля**. После
импорта используются без префикса (если не объявлен namespace-импорт).

#### Wildcard `import x.*` — запрещён

```nova
import std.io.*                                 // ❌ ошибка компиляции
```

Причины запрета:

1. **Скрывает источник.** Программист и LLM не видят, откуда пришло
   имя.
2. **Ломает at-distance.** Добавление имени в импортируемый модуль
   может конфликтовать с локальным — breaking change библиотек.
3. **AI-first** — LLM не должен искать «откуда `File`» по 10 импортам.

#### Циклические импорты — запрещены

```nova
// модуль A
module app.a
import app.b.SomeType

// модуль B
module app.b
import app.a.AT                                 // ❌ cycle a → b → a
```

Compile error с указанием полного цикла. Решается выделением общих
типов в третий модуль (`app.shared`), который импортируется обоими.

#### Re-export

```nova
module app.facade

export import std.io.{File, Reader}             // File и Reader доступны
                                                //  как app.facade.File, app.facade.Reader
export import std.collections.HashMap as Map    // алиас при re-export
```

`export import X` делает имя `X` доступным **извне моего модуля**, как
если бы оно было объявлено в нём. Полезно для facade-модулей.

#### Конфликты имён

Если импортированное имя совпадает с локальным или другим импортом —
**ошибка компиляции**. Решается алиасом через `as`:

```nova
import std.option.Option
import my.Option as MyOption

let x Option[int] = Some(42)                    // std.option.Option
let y MyOption = ...                            // my.Option
```

#### Prelude — без `import`

Prelude ([08-runtime.md → D26](08-runtime.md#d26)) — `Option`, `Result`,
`Some`, `None`, `Ok`, `Err`, `Error`, `Never`, базовые типы,
стандартные эффекты, `print`/`println`/`panic` — **автоматически в
скоупе**, без `import`.

Это **не противоречит** «нет неявных импортов»: prelude документирован,
LLM знает фиксированный список — «известная имплицитность», не магия.

### Почему

1. **Один файл = один модуль** — простая ментальная модель, легко
   найти source-of-truth.
2. **Иерархия через точки** — даёт намекающее имя в коде без
   рефлексии или magic-resolution.
3. **Явные импорты** улучшают локальность контекста ([D10 AI-first](01-philosophy.md#d10)).
4. **Запрет циклов** — сильнее, чем в Rust (где `use` циклы разрешены
   через forward declaration), но проще для рассуждения и согласуется
   с принципом «один файл = один модуль, без forward declarations».

### Что отвергнуто

- **Wildcard import** — скрывает источник, ломает локальность.
- **Циклические импорты** — усложняют рассуждение, требуют forward
  declarations.
- **Несколько модулей в одном файле** — нарушает «один файл = один
  модуль».
- **`#include` или препроцессор** — отвергнуто.
- **`pub` + `use` (Rust-стиль)** — `use` занят для embed/delegation
  ([D39](02-types.md#d39)).
- **CamelCase для видимости** (Go) — мешает естественному именованию.
- **`public`/`private`/`protected`/`package`** (Java) — overkill.

### Связь

- [D5](#d5-видимость-только-export-или-приватно) — два уровня видимости.
- [D47](#d47-видимость-деклараций) — детальные правила.
- [02-types.md → D39](02-types.md#d39) — `use` для embed/delegation
  (поэтому не `use` для модулей).
- [08-runtime.md → D26](08-runtime.md#d26) — prelude.

---

## D47. Видимость деклараций

### Что
**`export`** перед декларацией = публичная. **Без `export`** =
приватная для модуля. Применяется единообразно к типам, функциям,
методам, константам и протоколам. Поля record публичны (MVP),
convention `_prefix` для приватных-по-договору. `_prefix`
применяется **только к полям**, не к функциям/методам.

### Правило

```nova
module account

// ── Типы ──────────────────────────────────────────────────────────
export type Account {                    // публичный тип
    readonly owner str
    balance money
    _internal_id u64                     // convention: _prefix = приватное-по-договору
}

type InternalState {                     // приватный тип
    pending_ops []Op
}

// ── Константы ─────────────────────────────────────────────────────
export const ACCOUNT_MIN_BALANCE money = 0
const _INTERNAL_TIMEOUT_MS int = 5_000

// ── Static-функции и конструкторы ─────────────────────────────────
export fn Account.new(owner str) -> Account =>
    Account { owner, balance: money.zero, _internal_id: gen_id() }

fn Account.from_db_row(row DbRow) Fail -> Account => ...   // приватная

// ── Методы инстанса ───────────────────────────────────────────────
export fn Account @balance() => @balance                   // публичный
export fn Account mut @deposit(amount money) {             // публичный
    @validate_amount(amount)?
    @balance += amount
}

fn Account @validate_amount(amount money) =>               // приватный
    amount > 0 && amount < money.MAX

// ── Протоколы ─────────────────────────────────────────────────────
export protocol Hashable {
    hash() -> u64
    eq(other Self) -> bool
}

protocol _InternalIter[T] {                // приватный protocol
    next() -> Option[T]
}
```

#### Видимость полей record

**MVP:** все поля **`export`-типа публичны** для упрощения. Convention
**`_prefix`** для приватных:

```nova
export type Account {
    readonly owner str       // публичное
    balance money            // публичное
    _internal_id u64         // приватное-по-конвенции (НЕ enforced)
}
```

Convention `_prefix` — **не enforced** компилятором. Программист может
обратиться к `acc._internal_id` извне, компилятор не остановит. Это
**сознательный компромисс MVP**.

**`_prefix` применяется только к полям**, не к функциям и методам.
Для функций/методов видимость **двухуровневая** через `export`/без —
третьего «совсем-приватного» уровня нет:

```nova
export fn public_op(...) => ...       ✅ публичная
fn private_helper(...) => ...         ✅ приватная для модуля
fn _internal_helper(...) => ...       ❌ не нужно — нет третьего уровня
```

#### Метод приватного типа

Если тип приватный (`type X` без `export`), все его методы фактически
**недоступны снаружи модуля**, даже если помечены `export`:

```nova
type InternalState { ... }

export fn InternalState @do_thing() => ...   // export бесполезен — тип приватный
```

Компилятор выдаёт **предупреждение** «`export` method on non-exported
type — visibility limited to module».

#### Re-export через `export import`

См. [D29](#d29-модули-и-импорты) — модуль может re-export'ить чужие
декларации:

```nova
module my_lib

export import std.duration              // вся duration видна пользователям my_lib
import std.internal_helpers             // только внутри my_lib
```

### Почему

1. **Один keyword `export`** работает для всех видов деклараций — type,
   fn, const, protocol, метод. Учить нечего.
2. **Default приватный** соответствует трендам современных языков
   (Rust, Swift, Java/C# для классов). Безопасно по умолчанию.
3. **Convention `_prefix`** для полей — Python/Ruby-стиль, знакомо
   LLM, не требует языковых средств.
4. **Постепенная эволюция:** если понадобится enforcement полей —
   расширим без breaking changes (новый keyword добавит strictness,
   старый код останется валидным).

### Что отвергнуто

- **Per-field `export`** — многословно, преждевременная оптимизация
  для MVP.
- **`private` keyword** — Java-стиль, конфликтует с философией Nova
  «явно публичное».
- **Видимость по регистру** (Go-стиль `Capital = public`) — Nova
  использует разные conventions для разных уровней
  ([03-syntax.md → D30](03-syntax.md#d30)).
- **`_prefix` для функций как «третий уровень»** — нет, видимость
  двухуровневая.

### Сравнение с другими языками

| Язык | Default | Public marker | Field-level visibility |
|---|---|---|---|
| **Nova** | private (модуль) | `export` | все public, `_prefix` convention |
| **Rust** | private | `pub` | per-field `pub` |
| **Go** | regex-based | Capital letter | то же |
| **Swift** | internal | `public`/`private`/`fileprivate` | per-field |
| **Kotlin** | public | `private`/`internal` | per-field |
| **Java** | package-private | `public`/`private` | per-field |
| **Python** | public | — | `_prefix` convention |
| **OCaml** | через signature | в `.mli` файле | через signature |

Nova ближе всего к **Python** для полей и к **Rust** для top-level.

### Связь

- [D5](#d5-видимость-только-export-или-приватно) — базовое решение.
- [D29](#d29-модули-и-импорты) — модули, re-export.
- [03-syntax.md → D30](03-syntax.md#d30) — `_prefix` в правилах
  именования.
- [03-syntax.md → D35](03-syntax.md#d35) — методы получают тот же
  `export`-механизм.
- [02-types.md → D42](02-types.md#d42) — `export protocol X` делает
  контракт публичным.

### Цена

1. **Convention `_prefix` не enforced** — опытные программисты
   напомнят друг другу в code review. Linter может предупреждать.
2. **Будущее расширение** — если потребуется per-field enforcement,
   добавим `export` для полей. Не break.
3. **Документация** — нужно зафиксировать convention в стайл-гайде,
   чтобы не было сюрприза «почему `_balance` всё-таки доступен
   снаружи».
