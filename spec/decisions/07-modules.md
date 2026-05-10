# Modules — модули, импорты, видимость

Решения этой группы определяют, как код Nova организован в файлы и
как видимость деклараций контролируется между модулями.

| # | Решение |
|---|---|
| [D5](#d5-видимость-только-export-или-приватно) | Видимость: только `export` или приватно |
| [D29](#d29-модули-и-импорты) | Модули и импорты |
| [D47](#d47-видимость-деклараций) | Видимость деклараций (расширение D5) |
| [D78](#d78-package-tooling-novatoml-novalock-registry-chain-workspace) | Package tooling: `nova.toml`, `nova.lock`, registry chain, workspace |

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
export fn Account mut @deposit(amount money) Fail {        // публичный
    @validate_amount(amount)
    @balance += amount
}

fn Account @validate_amount(amount money) Fail =>          // приватный
    if amount <= 0 || amount >= money.MAX { throw InvalidAmount }

// ── Протоколы ─────────────────────────────────────────────────────
export type Hashable protocol {
    hash() -> u64
    eq(other Self) -> bool
}

type _InternalIter[T] protocol {           // приватный protocol
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

---

## D78. Package tooling: `nova.toml`, `nova.lock`, registry chain, workspace

### Что
Манифест проекта `nova.toml` (TOML), lockfile `nova.lock`, цепочка
реестров пакетов с поддержкой private mirror'ов / proxy для closed
networks, SAT-resolution алгоритм, workspace для monorepo, опциональный
prelude opt-out, настраиваемый source root.

### Правило

#### Манифест проекта — `nova.toml`

Минимальные обязательные поля: `name`, `version`. Всё остальное —
опционально.

```toml
[package]
name = "my-project"                        # snake_case (D30)
version = "0.1.0"                           # semver, std.semver
nova-version = "0.5"                        # минимальная версия Nova
authors = ["Alice <alice@example.com>"]     # опционально
description = "Один абзац про что"           # опционально
license = "MIT OR Apache-2.0"               # SPDX
repository = "https://github.com/..."       # опционально

[lib]                                       # опционально, если пакет — библиотека
src = "src"                                 # default — переопределяемо

[[bin]]                                     # опционально, для каждого бинаря
name = "my-tool"
src = "src/bin/my_tool.nv"

[dependencies]
serde = "1.2"                               # из реестра, semver-range
internal = { path = "../internal" }         # local dependency
foo = { git = "https://github.com/...", tag = "v1.0" }  # git

[dev-dependencies]
test-utils = "0.1"                          # только для тестов

[workspace]                                  # опционально, monorepo
members = ["server", "client", "common"]

[features]                                   # опционально, conditional compilation
default = ["std"]
std = []
realtime = []
```

`[[bin]]` — TOML array of tables (двойные скобки) — позволяет
несколько бинарей в одном пакете. Каждое появление `[[bin]]` —
новый элемент массива.

`[lib]` / `[[bin]]` — секции **взаимодополняющие**: пакет может быть
и библиотекой (`[lib]`), и иметь бинари (`[[bin]]`).

#### Source root — настраиваемый, default `src/`

```toml
# nova.toml
[lib]
src = "src"                  # default
# src = "lib"                # переопределение
# src = "."                  # корень проекта = source root (для маленьких)
```

Корень для резолвинга путей module ↔ file. `module admin.audit` ↔
`<src>/admin/audit.nv`.

Если `[lib]` отсутствует — `src/` по умолчанию.

#### Path / module enforcement

Компилятор **обязан** проверить соответствие пути файла и имени модуля.
Несоответствие — **compile error с suggestion**:

```
error: module declaration does not match file path
  in src/audit/main.nv:1:1
  │
  1 │ module admin.audit
  │ ^^^^^^^^^^^^^^^^^^ this declares `admin.audit`
  │
  expected one of:
  - move file to: src/admin/audit.nv (preserve module name)
  - rename module to: audit.main (match current path)
```

Это AI-friendly: LLM получает не просто ошибку, а конкретное действие.

#### Структура nova-lang репозитория

Сам репозиторий языка (этот) использует **specific layout**, который
служит **эталоном** для других пакетов и закреплён в spec'е чтобы
будущие libs/тулинг знали где что искать:

```
nova-lang/
├── compiler-codegen/        Rust компилятор: парсер, type-checker, treewalk-interp + C-codegen
├── std/                     стандартная библиотека Nova (.nv)
│   ├── collections/         hashmap, set, deque, vec, queue, ...
│   ├── crypto/              md5, sha1, sha256, hmac, jwt, bcrypt
│   ├── encoding/            base64, hex, json, csv, ini, toml, url
│   ├── identifiers/         uuid, ulid, snowflake
│   ├── checksums/           crc32, fnv
│   ├── time/                duration, cron
│   ├── path/                path, glob
│   ├── math/                complex, statistics
│   ├── text/                regex, markdown_minimal, diff
│   ├── data/                semver, semver_range, sql
│   ├── concurrency/         rate_limiter, retry
│   └── (новые домены — net/, io/, testing/, и т.д.)
├── examples/                демо-программы и tutorial snippets
│   └── *.nv                 (НЕ stdlib — это именно examples)
├── nova_tests/              .nv-тесты bootstrap'а (package `nova_tests`)
├── spec/                    spec-документация (этот файл здесь)
└── docs/                    plans, articles, research
```

**Принципы layout'а:**

1. **`std/` ≠ `examples/`.** `std/` — production-код стандартной
   библиотеки. `examples/` — туториальные snippets и демо-программы,
   которые **используют** stdlib.

2. **Имя папки `std/` = префикс модуля `std.`.** Module path 1:1
   соответствует file path **без специальных маппингов** (см. правило
   `Path / module enforcement` выше). Файл `std/encoding/base64.nv`
   объявляет `module std.encoding.base64`; ничего не разворачивается
   в имени, ничего не подразумевается.

3. **Группировка по домену, не по типу артефакта.** Каждая папка в
   `std/` — semantic domain (`crypto`, `encoding`, `time`),
   а не «source-files vs tests vs docs». Тесты живут рядом с
   модулем (внутри `.nv`-файла через `test "..." { }` блоки) —
   см. [03-syntax.md → D29](03-syntax.md#d29).

4. **Плоская иерархия внутри домена.** Без подпапок второго уровня
   (`std/crypto/md5.nv`, не `std/crypto/hash/md5.nv`). Если
   домен растёт до 15+ файлов — рассматривать подкатегорию.

5. **Прецеденты.** Go (`net/http/`, `encoding/json/`), Python
   (`Lib/http/`, `Lib/json/`), Rust (`library/std/src/collections/`).
   Все три используют **доменную flat-иерархию**, не type-based
   (как Maven/Gradle src/main/java). Имя `std/` повторяет Rust'овский
   `library/std/`, но без обёртки `library/`.

Этот layout фиксируется как **convention**, не как обязательное
правило для пользовательских пакетов — `nova.toml` через `[lib].src`
позволяет любую структуру. Но `std/` — каноничный пример.

Вопрос «почему `std/`, а не `stdlib/`?» — короткое имя сохраняет
краткость импортов (`import std.encoding.json` vs
`import stdlib.encoding.json`) и **избавляет от спецправила**
«папка `stdlib/` мапится в префикс `std.`» — path 1:1 = module без
исключений. `stdlib/` рассматривался; отвергнут в пользу `std/`
по этим двум причинам.

#### Lockfile — `nova.lock`

TOML-формат, имя lowercase, единообразно с `nova.toml`. Auto-generated,
commit в VCS обязателен:

```toml
# nova.lock — auto-generated, не редактировать вручную
version = 1                                 # формат lockfile

[[package]]
name = "serde"
version = "1.2.5"
source = "registry+https://nova-registry.org"
hash = "sha256:abc123..."
dependencies = ["std-utils 0.3.1"]

[[package]]
name = "internal-utils"
version = "0.1.0"
source = "git+https://github.com/...#commitsha"
hash = "sha256:def456..."

[[package]]
name = "local-lib"
version = "0.1.0"
source = "path+../local-lib"                # для local — без hash
```

Префикс в `source` показывает тип источника:

| Префикс | Значение |
|---|---|
| `registry+<url>` | взято из реестра, скачан tarball |
| `git+<url>#<commit>` | git clone + checkout commit |
| `path+<path>` | local path (нестабильно между машинами, без hash) |

Без префикса формат был бы неоднозначным. С префиксом — однозначно
для tooling'а и для человека.

#### Resolution — SAT с lockfile

Используется **SAT-алгоритм** (стандарт индустрии 2020+: Cargo, npm,
Poetry, Maven). Конкретная реализация — **pubgrub** (open-source,
доказательно корректный, используется Cargo и Dart). Это implementation
detail, не часть language spec.

**Свойства:**
- Решает «найди такой набор версий чтобы все ограничения выполнились»
- Может выбрать **более новую** версию транзитивной dep если безопасно
- При обновлении одной dep лочит остальные через lockfile
- Воспроизводимость через lockfile

`Go-style MVS` отвергнут: проще, но «застывает» зависимости — security
updates требуют ручного bump'а каждой dep отдельно.

#### Registry chain — цепочка реестров

Глобальная конфигурация в `~/.nova/config.toml`:

```toml
[registry]
default = [
    "https://nova.bank.local/proxy",        # 1. Внутренний прокси банка
    "https://nova-registry.org",             # 2. Публичный реестр
    "direct"                                  # 3. git URLs из deps clone'аются напрямую
]
```

**Алгоритм для `serde = "1.2"`:**
1. GET `https://nova.bank.local/proxy/serde/...` — найдено? Берём.
2. Не найдено / network error → следующий source.
3. GET `https://nova-registry.org/serde/...` — найдено? Берём.
4. Не найдено → fail (для name+version из реестра; `direct` имеет
   смысл только для `git`-зависимостей).

**Per-project override** в `nova.toml` — то же поле локально:

```toml
# nova.toml
[registry]
default = ["https://my-team.local/registry"]    # override global
```

**Auth/credentials** — отдельный файл, **вне VCS**:

```toml
# ~/.nova/credentials.toml
[registry."nova.bank.local"]
token = "..."
```

**Offline mode** через CLI flag — для closed networks:

```sh
nova build --offline                            # только локальный кэш, без сети
```

**Назначение цепочки:**

| Среда | Конфигурация |
|---|---|
| Корпоративная (банк, санкции) | внутренний прокси первым; кэширует public, audit'ит |
| Closed network | только internal, public недоступен |
| Open-source разработка | public registry + direct |
| Mix (private + public deps) | internal-then-public chain |

Прецедент — Go `GOPROXY` с chain'ом + Maven/Cargo per-project override.

#### Workspace — monorepo

`[workspace]` секция в root `nova.toml` объявляет multi-package
проект:

```
my-project/
├── nova.toml                # [workspace] members = ["server", "client", "common"]
├── nova.lock                # ОДИН lockfile на всё workspace
├── server/
│   └── nova.toml            # [package] name = "server"
├── client/
│   └── nova.toml            # [package] name = "client"
└── common/
    └── nova.toml            # [package] name = "common"
```

```toml
# Корневой nova.toml
[workspace]
members = ["server", "client", "common"]
```

**Свойства:**
- Один lockfile на всё workspace — гарантирует одинаковые версии
  транзитивных deps между пакетами
- Один build cache (быстрее перекомпиляция)
- Внутренние deps по path: `server/nova.toml` пишет
  `common = { path = "../common" }`

Стандартная практика для backend monorepo (microservices в одном репо).

#### Prelude opt-out

Per-file декларация в module-line:

```nova
module my.realtime no_prelude

// Никаких автоматических импортов; даже Option/Result надо импортировать
import std.option.Option
import std.result.Result
```

Применение:
- **Real-time / embedded** — где prelude содержит код использующий GC
- **Bootstrap уровни** — реализация самого prelude
- **Обучающие примеры** — для AI/преподавания иногда нужно «всё видно»

Без `no_prelude` — стандартный prelude в скоупе (D26).

#### Конвенции имён

- **Пакет** — `name` в `nova.toml`, snake_case (`my_project`).
- **Модуль** — иерархическое имя через точки (`admin.audit`).
- **Файлы** — snake_case (`audit.nv`, не `Audit.nv`).
- **Workspace member** — имя папки = имя пакета (по convention).

### Почему

1. **TOML формат.** Cargo/npm поверх JSON/TOML; TOML читаемее для
   человека и LLM, поддерживает комментарии.
2. **Минимум обязательных полей.** `name` + `version` достаточно для
   простого проекта. Всё остальное — постепенный opt-in.
3. **Registry chain — Go GOPROXY style.** Один из лучших дизайнов
   индустрии: простая ENV-переменная или TOML-список, понятная
   семантика «попробуй по очереди». Покрывает корпоративные и
   open-source сценарии без отдельных модов.
4. **Lockfile обязателен в VCS.** Воспроизводимые сборки — стандарт
   2020+. Отдельный файл (vs встроенный в manifest) — manifest
   стабильнее.
5. **SAT resolution** — стандарт индустрии, лучший trade-off между
   гибкостью и автоматическими updates. MVS (Go) проще, но «застывает»
   зависимости.
6. **`source = "registry+<url>"` префикс.** Cargo прецедент,
   однозначное различение типов источников без множества полей.
7. **Workspace** — критично для backend monorepo. Cargo доказал
   эффективность.
8. **Path/module enforcement** — AI-friendly. LLM получает
   compile error с suggestion, а не undefined behavior.
9. **Prelude opt-out per-file** — гранулярнее чем per-project, согласовано
   с D64 `realtime { ... }` блоками per-function.
10. **Корень src/ default + override** — простой default + power user
    customization. Прецедент Cargo (`[lib] path = "..."`), Maven (`src/main/java`).

### Что отвергнуто

- **JSON manifest** (npm-стиль `package.json`). JSON не поддерживает
  комментарии, плохо читается LLM/человеком в больших файлах.
- **YAML manifest**. Whitespace-sensitive, известный источник ошибок,
  обширное spec'ирование (3 редакции).
- **`Cargo.toml` имя**. PascalCase нарушает D30 (snake_case для модулей
  и файлов). `nova.toml` единообразно.
- **Lockfile в JSON** (`package-lock.json`). TOML единообразен с
  manifest.
- **MVS (Go-style)** алгоритм резолюции. Простое, но «застывает» deps.
  Современный consensus за SAT.
- **Decentralized registry** (Go style — каждый dep это URL). Сложнее
  для security audit, нет центрального discovery. Chain покрывает
  корпоративные сценарии без decentralization.
- **Wildcard в version requirements** (`serde = "*"`). Запрещён
  resolver'ом — порождает невоспроизводимые сборки.
- **`[bin]` (single section)** для бинарей. `[[bin]]` (array of tables)
  единообразен для 1+ бинарей, не требует переключения форматов.
- **Несколько lockfile в workspace**. Один lockfile гарантирует единые
  версии транзитивных deps — иначе server и client получают разные
  serde, что ломает type-compat.
- **Auto-detect manifest fields** (через scan кода). Явное лучше
  неявного, AI-friendly.

### Цена

1. **Tooling сложность.** SAT resolver — нетривиальная реализация
   (pubgrub можно использовать готовый). Registry protocol, lockfile
   format, workspace coordination — это всё нужно реализовать.
2. **Bootstrap problem.** Сама Nova-stdlib должна жить **до** появления
   tooling'а. Решение: stdlib монолитна на ранних этапах (всё в
   `std/<домен>/*.nv`), package tooling появляется параллельно
   с self-hosted compiler.
3. **Central registry — открытый вопрос.** Когда появится,
   нужна команда поддержки (хостинг, security policy, DMCA, etc.).
   До появления — только local + git URLs.
4. **Backward-compat lockfile format.** Поле `version = 1` — для
   будущих миграций. v2 lockfile должен парситься старым tooling'ом
   с понятным error'ом.

### Связь

- [D29](#d29-модули-и-импорты) — модули, иерархия, импорты. D78
  расширяет: где лежат модули относительно корня, как резолвятся
  внешние пакеты.
- [D26](08-runtime.md#d26) — prelude. D78 описывает opt-out.
- [D30](03-syntax.md#d30) — конвенции имён модулей и файлов.
- [D64](04-effects.md#d64) — `realtime { }` блок; prelude opt-out
  полезен для real-time uses.
- [std/data/semver.nv](../../std/data/semver.nv) —
  semver используется для `version` поля и для resolver-сравнений.

### Открытые вопросы

- **Central registry hosting.** Когда появится `https://nova-registry.org`?
  До появления — только git URLs и local paths. Q-central-registry.
- **Registry API spec.** Точный HTTP API реестра (как
  `https://crates.io/api/v1/crates`). Q-registry-api.
- **Security model.** SBOM (software bill of materials), supply-chain
  attacks, signed packages? Q-package-security.
- **Build script / hooks.** `cargo build.rs` — нужны ли native build
  steps в Nova? Q-build-scripts.
- **Feature combinations validation.** SAT над features (как Cargo)
  vs simple bool flags. Q-features-resolution.
- **Cross-compilation.** Target-specific deps (`[target.x86_64-linux]`).
  Q-cross-compile.

### Эволюция

До D78 в [D29](#d29-модули-и-импорты) `nova.toml` был упомянут как
«манифест проекта», но **без** описания формата. Резолвинг сторонних
пакетов и lockfile вообще не были зафиксированы. На практическом
backend-проекте это блокирующий пробел.

D78 фиксирует **полный tooling-стек**: формат manifest'а, lockfile,
registry chain (с поддержкой closed networks / proxy), SAT-resolution,
workspace, prelude opt-out, source root override, path/module
enforcement.

Прецеденты:
- **Cargo.toml + Cargo.lock** — основной образец (TOML, sections, SAT).
- **Go GOPROXY chain** — для registry mirror.
- **npm workspaces / Cargo workspaces** — multi-package monorepo.
- **Maven settings.xml mirrors** — corporate proxy stories.

---

## Пример: иерархическая структура test-suite (D29 в действии)

Реальный пример организации test-suite показывает применение D29
(strict file ↔ module correspondence) + D30 (snake_case naming) +
D78 (nova.toml package).

### `nova_tests/` layout

> **Note (2026-05-07):** Папка переименована из `tests-nova/` в
> `nova_tests/`. Имя директории должно совпадать с `package.name`
> внутри `nova.toml` (D78 path/module enforcement: иначе declared
> `module nova_tests.basics.literals` не сматчится с file path
> `tests-nova/basics/literals.nv`).

```
nova_tests/
├── nova.toml                # workspace member, src = ".", name = "nova_tests"
├── basics/                  # = D-области 01-philosophy + базовый syntax
│   ├── literals.nv          # module nova_tests.basics.literals
│   ├── operators.nv         # module nova_tests.basics.operators
│   └── ...
├── types/                   # = 02-types
├── syntax/                  # = 03-syntax
├── effects/                 # = 04-effects
├── concurrency/             # = 06-concurrency
├── runtime/                 # = 08-runtime
└── modules/                 # = 07-modules
```

### Соглашения

1. **Группа = тематическая область** spec/decisions/. Тест на feature
   D71 (parallel for → []T) лежит в `concurrency/`, тест на D76 (Mem
   effect) — в `runtime/`.
2. **Имена файлов** — snake_case без нумерации (D30). Файл
   `concurrency/parallel_for.nv` — это `module nova_tests.concurrency.parallel_for`.
3. **Module path = filesystem path.** Первая компонента — package name
   (`nova_tests` из `nova.toml`), затем подкаталоги, затем файл.
4. **Keyword collisions избегаются.** Если файл/группа конфликтует с
   keyword'ом (`cancel_scope` — keyword), используется `_test` суффикс:
   `concurrency/cancel_scope_test.nv` → `module nova_tests.concurrency.cancel_scope_test`.

### Преимущества (vs плоский нумерованный layout)

- **Findability.** Concurrency-тесты лежат рядом, не размазаны через
  весь корень по нумерации `38, 40, 41, 42, ..., 52`.
- **By-topic, не by-creation-order.** При добавлении нового D-фичо тест
  ложится в соответствующую группу — нет поиска свободного numeric slot.
- **AI-friendly.** AI генерирует test imports / module-paths по
  filesystem-path — нет magic-mapping `01_literals → spec.literals`.
- **D29-compliance** автоматически.

### Anti-pattern: плоская нумерация

`01_literals.nv`, `02_operators.nv`, ..., `57_unwrap_or.nv` — был
ранний bootstrap-стиль. Проблемы:
- D30 запрещает не-snake_case символы (нумерные префиксы — это шум).
- Order-by-creation, не by-topic — рядом неродственные feature.
- Хрупкость: insert требует shift всех соседей или поиск slot'а.

После миграции (commit `a33b245`) этот anti-pattern удалён.
