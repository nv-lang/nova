# Modules — модули, импорты, видимость

Решения этой группы определяют, как код Nova организован в файлы и
как видимость деклараций контролируется между модулями.

## Терминология иерархии

Код Nova организован в **четыре уровня** — сверху вниз:

```
Workspace   — nova.toml с [workspace]; группирует пакеты (monorepo). Опционально.
  └─ Пакет  — [package] name + version; свой nova.toml; единица версии,
     │         зависимости и публикации. Библиотека по умолчанию + бинари ([[bin]]).
     └─ Модуль — файл `X.nv` или папка `X/` с peer-файлами; `module parent.target`;
        │        единица namespace и импортов.
        └─ Item — fn / type / effect / protocol / const внутри модуля.
```

- **Workspace** — `[workspace]` в `nova.toml` (`members = [...]`). Monorepo
  из нескольких пакетов. Опционально — одиночный пакет workspace не требует.
- **Пакет (package)** — директория с `[package]` (`name` + `version` —
  обязательны, [D78](#d78-package-tooling-novatoml-novalock-registry-chain-workspace)).
  Имя пакета = имя его директории. Единица **версионирования**,
  **зависимости** (`[dependencies]`) и **публикации** (registry). Также —
  граница относительных импортов `./`/`../`.
- **Модуль (module)** — файл `X.nv` либо папка `X/` с peer-файлами внутри
  пакета; объявляется `module parent.target` ([D29](#d29-модули-и-импорты)).
  Единица **namespace** и **импорта**.
- **Item** — объявление внутри модуля (функция, тип, эффект, протокол,
  константа). Единица **видимости** — `export` или приватно
  ([D5](#d5-видимость-только-export-или-приватно)).

**Соответствие терминов в других языках.** Go «переворачивает» названия —
если думать в Go-терминах, легко перепутать:

| Nova | Go | Cargo (Rust) | npm |
|---|---|---|---|
| модуль (namespace) | package | module внутри crate | — |
| пакет (единица версии/зависимости) | module (`go.mod`) | package / crate | package |

Nova-«пакет» = Go-«module» = Cargo/npm-«package». С Cargo и npm
терминология совпадает; расходится только с Go.

---

| # | Решение |
|---|---|
| [D5](#d5-видимость-только-export-или-приватно) | Видимость: только `export` или приватно |
| [D29](#d29-модули-и-импорты) | Модули и импорты |
| [D47](#d47-видимость-деклараций) | Видимость деклараций (расширение D5) |
| [D78](#d78-package-tooling-novatoml-novalock-registry-chain-workspace) | Package tooling: `nova.toml`, `nova.lock`, registry chain, workspace |
| [D99](#d99-conditional-compilation-filename-suffix--cfg) | Conditional compilation: filename suffix + `#cfg` |
| [D100](#d100-_modulenv-peer--module-config-convention) | `_module.nv` peer — module-config convention |
| [D101](#d101-doc-attribute--module-level-inline-documentation) | `#doc` attribute — module-level inline documentation |
| [D134](#d134-symbol-mangling-v0--c-имена-свободных-функций) | Symbol mangling v0 — C-имена свободных функций |
| [D138](#d138-межпакетный-импорт--только-через-объявленную-зависимость) | Межпакетный импорт — только через объявленную `[dependencies]`-зависимость |
| [D139](#d139-version-диапазоны-git-зависимостей--резолв-по-тегам-репозитория) | Version-диапазоны git-зависимостей — резолв по тегам репозитория |
| [D140](#d140-effect-aware-зависимости--effect-surface-и-forbid-на-границе) | Effect-aware зависимости — effect-surface и `forbid` на границе |

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

**Enforcement (Plan 81 Ф.1, 2026-05-21).** До Plan 81 флаг `is_export`
был информационным — не-`export` элементы импортированного модуля были
доступны снаружи (нарушение этого D-блока). Plan 81 Ф.1 ввёл реальный
enforcement: type-checker скрывает не-`export` top-level элементы за
границей модуля; обращение к приватному → выделенный диагностик.
Уровень — как Go (Caps) / Rust (`pub`) / TS (`export`). Peer-файлы
folder-модуля и `_test.nv` сохраняют white-box доступ к приватному
(внутри границы модуля).

---

## D29. Модули и импорты

> **2026-05-12 (rev-2):** modulу разрешено быть **multi-file**
> (folder = module, Go-style peers). Все файлы в папке объявляют
> один и тот же `module X` и share namespace. Single-file модули
> продолжают работать. См. подраздел «Модуль — файл или папка».
>
> **2026-05-13 (rev-3):** module declaration format = **`parent.X`**
> (parent folder + target name) для **обоих** случаев (single-file
> и folder-module). См. подраздел «Объявление модуля».
>
> **2026-05-22 (rev-4):** добавлены **относительные импорты** `./` / `../`
> — package-scoped (Plan 84). См. подраздел «Относительные импорты».

### Что
**Модуль — это файл `X.nv` ИЛИ папка `X/` с одним или несколькими
`.nv` файлами.** Все файлы папки-модуля объявляют `module <parent>.<X>`
(одинаковый для всех peers) и **share namespace** (как в Go).
Имя модуля объявляется первой строкой `module parent.name`. Импорт
через `import path.to.module.{Names}`. Wildcard и циклические импорты
запрещены. Re-export — `export import`.

### Правило

#### Модуль — файл или папка

**Single-file** (раньше единственный вариант):

```nova
// admin/audit.nv
module admin.audit                              // parent=admin, name=audit
```

**Multi-file (folder = module, peers)** — для больших модулей. Все
файлы в папке объявляют **одинаковый** `module <parent>.<X>` (где
parent — родитель папки `X/`, X — имя самой папки) и share namespace —
функции/типы из одного файла видны в другом без import:

```
app/admin/
├── users.nv          module app.admin     (peer; parent=app, name=admin)
├── audit.nv          module app.admin     (peer)
├── permissions.nv    module app.admin     (peer)
└── helpers.nv        module app.admin     (peer; internal, без export)
```

Все 4 файла начинаются с `module app.admin`. Если какой-то файл
объявляет другое имя (например `module admin` или `module app.admin.users`)
— compile error.

**Sub-modules — через nested folders, не через single peer file:**

```
app/admin/
├── users.nv               module app.admin            (peer)
└── billing/               
    ├── invoice.nv         module admin.billing        (peer; parent=admin, name=billing)
    └── subscription.nv    module admin.billing        (peer)
```

`app/admin/billing/` — это **независимый** модуль `admin.billing`.
Файлы в нём peers друг другу, но **не** peers с `app/admin/users.nv`.
Чтобы использовать invoice из users.nv, нужен явный
`import admin.billing.{Invoice}`.

**Конфликт `X.nv` + папка `X/` на одном уровне — Rule E** (Plan 42,
2026-05-14; spec sync 2026-05-19 в рамках Plan 62 cleanup):

`X.nv` single-file и папка `X/` рядом сосуществуют по правилам:

- **(a) Conflict** — `X/` содержит direct `.nv` файлы, **который объявляет
  `module <parent>.X`** (т.е. peer-files of folder-module `X`).
  В этом случае `X.nv` и `X/peer.nv` оба claim'ят name `X` (parent=tame).
  Compile error «ambiguous module 'X'».
- **(b) Валидно (facade pattern)** — `X.nv` существует и `X/` содержит
  **только nested sub-modules** (declaring `module X.<sub>`), либо
  только sub-folders без direct `.nv`. В этом случае `X/` — не module,
  а namespace-container; `X.nv` — single-file module `X` который может
  re-export'ить nested sub-modules через `export import X.<sub>.{...}`.

**Example facade pattern (Plan 62 splittable prelude):**

```
std/
├── prelude.nv                 module std.prelude (facade re-export)
└── prelude/                   (namespace-container, не module)
    ├── core.nv                module prelude.core (independent sub-module)
    ├── runtime.nv             module prelude.runtime (independent sub-module)
    └── ...
```

`std/prelude.nv` объявляет `module std.prelude` и через `export import
std.prelude.core.{...}` re-export'ит declarations из nested sub-modules.
`std/prelude/core.nv` объявляет `module prelude.core` (parent=prelude,
target=core — rev-3 правило). Каждый sub-module независим, peers
только внутри своей папки. См. также `docs/plans/42-folder-modules.md`
Rule E и Plan 62.

#### Объявление модуля — правило `parent.X`

```nova
module parent.name                          // первая строка файла
```

**Универсальное правило (rev-3):** `module = <parent_of_target>.<target_name>`

где:
- **target** = file basename (для single-file) или folder name
  (для folder-module peer).
- **parent_of_target** = имя directory **сразу над** target.

Примеры:

| Файл | parent | target | declaration |
|---|---|---|---|
| `app/main.nv` (single-file) | `app` | `main` | `module app.main` |
| `app/admin.nv` (single-file) | `app` | `admin` | `module app.admin` |
| `app/std/admin.nv` (single-file) | `std` | `admin` | `module std.admin` |
| `app/std/user/admin.nv` (single-file) | `user` | `admin` | `module user.admin` |
| `app/admin/users.nv` (peer of folder `admin/`) | `app` | `admin` | `module app.admin` |
| `app/std/encoding/hex.nv` (single-file) | `encoding` | `hex` | `module encoding.hex` |
| `app/std/encoding/json/parse.nv` (peer of `json/`) | `encoding` | `json` | `module encoding.json` |
| `app/std/encoding/json/stringify.nv` (peer of `json/`) | `encoding` | `json` | `module encoding.json` |

**Свойства правила:**

1. **Declaration обычно 2 segments** (parent.target), не зависит от
   глубины nesting. Исключение — `internal/` (rev-3.1, см. ниже).
2. **Refactor safety:** при move файла или папки compiler сравнивает
   declaration с (parent, target) пары и эмитит error при mismatch.
3. **Folder-module и single-file consistent:** оба используют parent.X,
   нет split-личностей в codebase.
4. **Cycle detection и import resolution** работают через
   полный **filesystem path** (compiler ведёт internal mapping
   `decl ↔ canonical path`); declaration — это **identity check**,
   не routing key.

#### rev-3.1: `internal/` extended naming (Plan 42.13)

> **2026-05-14 (rev-3.1):** single-file и folder-module внутри
> `internal/` получают **3-segment** declaration `owner.internal.target`.

Без этого исключения у нескольких модулей со своими `internal/`
получались бы **одинаковые** declarations:

```
admin/internal/token.nv     → module internal.token   ← collision
billing/internal/token.nv   → module internal.token   ← collision
```

С rev-3.1:

| Файл | owner | declaration |
|---|---|---|
| `admin/internal/token.nv` (single) | admin | `module admin.internal.token` |
| `billing/internal/token.nv` (single) | billing | `module billing.internal.token` |
| `admin/internal/codec/enc.nv` (peer of `codec/`) | admin | `module admin.internal.codec` |

**Edge case:** если `internal/` САМА folder-module (peers лежат прямо
в `internal/`, не в sub-folder) — declaration = `owner.internal`
(2 segments, без дублирования `internal.internal`):

| Файл | declaration |
|---|---|
| `folder_internal/internal/encode.nv` (peer) | `module folder_internal.internal` |
| `folder_internal/internal/token.nv` (peer) | `module folder_internal.internal` |

- **owner** = directory сразу перед `internal`. Если `internal` на
  root level — owner = package name.
- **target** = file basename (single-file) или folder name
  (folder-module peer).
- Берётся **первый** `internal` сегмент в path. Nested `internal/`
  глубже одного уровня не поддерживается.

**Import** для `internal/` тоже использует full path:
`import admin.internal.token.{make_token}` — но только из `admin.*`
descendants (правило H).

**Import — полный путь от корня пакета либо относительный** (`./` / `../`,
rev-4 — см. подраздел «Относительные импорты»):

```nova
import std.encoding.hex.{decode}            // полный путь от корня пакета
import ./sibling.{decode}                   // относительный — сосед (rev-4)
```

Compiler находит модуль по filesystem path, проверяет что declaration
matches `(parent, target)` пары, и связывает с full path.

Один файл = одна `module parent.name` декларация. Никаких
`module X { ... }`-блоков внутри файла.

#### Структура проекта

```
project/
├── main.nv                  module project.main       (single-file)
├── admin/                                             (folder-module)
│   ├── users.nv             module project.admin
│   ├── audit.nv             module project.admin
│   ├── permissions.nv       module project.admin
│   └── helpers.nv           module project.admin
├── http/                                              (folder-module)
│   ├── server.nv            module project.http
│   ├── client.nv            module project.http
│   └── handler/                                       (nested folder-module)
│       ├── auth.nv          module http.handler
│       └── log.nv           module http.handler
└── nova.toml                манифест проекта
```

Корень пакета — `project/`. Single-file `main.nv` = module
`project.main` (parent=project, target=main). Folder `admin/` (с 4
файлами) = module `project.admin` где все 4 файла peers. Nested folder
`http/handler/` = независимый module `http.handler` (parent=http,
target=handler). Где лежит `nova.toml` и как устроен пакет — это
**tooling**, см. открытые вопросы.

#### Visibility внутри folder-module

- `export` — public наружу (вне модуля).
- Без `export` — module-private: видно из **всех файлов того же
  module** (всех peers в folder).

Это даёт **internal helpers**: `_helpers.nv` без `export fn helper()`
видна из `users.nv`, `audit.nv`, etc. — но **не** извне `admin`.

#### Test isolation — `_test.nv` suffix (Plan 42 правило F)

Peer-файл с suffix `_test.nv` (basename ends with `_test`) — **test-
only**. Включается в test mode (`nova test`), исключается из release
build (`nova build` / `nova run`). Имеет full доступ к module-private
items как обычный peer (для close-box testing).

```
admin/
├── users.nv          (production peer, всегда compiled)
├── users_test.nv     (test-only peer, только в test mode)
└── helpers.nv        (production peer)
```

Аналог Go `*_test.go`. Inline `test "..."` блоки в production-peer-
файлах работают как раньше (test-runner их собирает, codegen
исключает из release).

#### Internal modules — `internal/` directory (Plan 42 правило H)

Folder `<X>/internal/<...>` доступен **только** из `<X>/...`
descendants (Go-style library boundary).

```
admin/
├── users.nv                              (peer of admin)
└── internal/
    ├── token.nv                          module admin.internal
    └── crypto.nv                         module admin.internal (peer)
```

- `admin/users.nv` может `import admin.internal.{Token}` — OK
  (descendant of `admin`).
- `http/handler.nv` НЕ может `import admin.internal.{Token}` —
  compile error «cannot import internal module from outside parent».

Critical для production library development: refactor internal без
breaking external API.

#### File-level `#forbid` attribute (Plan 42 Sub-plan 42.1)

Attribute `#forbid Eff1, Eff2` **перед** `module X` declaration
(Plan 42.16) applies к **этому файлу** (per-file scope). Все
functions/tests в этом файле получают forbidden effects.

```nova
#forbid Net, Fs
module admin

// все fn в этом файле гарантированно не используют Net/Fs.
export fn create_user(name str) Db -> User { ... }
```

**Semantics:**

- **Per-file scope.** Применяется только к этому файлу. Peers
  folder-module имеют независимые `#forbid` declarations
  (intentional — peers равноправны).
- **Layered с function-scope** D63. Function-internal `forbid X { body }`
  union'ится с file-level forbid.
- **`#requires` отвергнут.** Module-level implicit effects (auto-injected
  в function signatures) противоречат AI-first explicit principle
  (D62 «эффекты в сигнатуре»). Используй explicit `Eff` в каждой
  signature как обычно.

**Cross-peer consistency:** convention — programmer пишет одинаковый
`#forbid` в каждом peer module если хочет module-wide constraint.
Compiler **не** auto-propagates between peers (peers равноправны).
Lint rule (sub-plan 42.7) — warn при inconsistent `#forbid` declarations
between peers same module.

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

#### Относительные импорты `./` / `../` (rev-4, Plan 84)

Импорт без префикса — **абсолютный** (путь от корня пакета). Префиксы
`./` и `../` делают импорт **относительным** — путь резолвится от
директории импортирующего файла:

```nova
import ./sibling.{Name}            // модуль в директории текущего файла
import ../neighbor.{Name}          // на уровень вверх
import ../../shared.types.{T}      // на два уровня вверх, затем путь
export import ./facade_part.{X}    // re-export относительного модуля
import ./sub.{Y} as Z              // алиасы — как обычно
```

- `./` — директория импортирующего файла; `../`×n — n уровней вверх.
- После префикса — обычный точечный путь, резолвящийся от выбранной
  директории (теми же правилами, что абсолютный — от корня пакета).
- **Префикс — синтаксический дискриминатор:** `import a.b` всегда
  абсолютный, `import ./a` / `import ../a` всегда относительный —
  неоднозначности нет by construction, правило приоритета не нужно.
- `./` не может сопровождаться `../` (`.././x` — ошибка); для подъёма
  пишется `../` напрямую.

**Граница пакета — жёсткая.** Относительный импорт **не может выйти за
корень своего пакета** (директория ближайшего `nova.toml`). Цепочка
`../`, ушедшая выше корня пакета → **compile error**. Межпакетные
ссылки — только полным путём от корня. Это сохраняет package-уровневую
location-independence (Rust-модель `super`/`self` внутри крейта; Go
относительных импортов не имеет — слишком строго; TS — безграничные,
`../../../`-hell — слишком вольно; Nova берёт середину).

**Почему это не противоречит AI-first.** Явный префикс `./`/`../` сам
по себе — сигнал «это сосед, внутри пакета»; читатель (и LLM) видит
ровно это, без поиска «откуда имя». Нарушал бы локальность только
безграничный *межпакетный* относительный импорт — он запрещён границей
пакета.

Относительные импорты резолвятся в canonical filesystem path — поэтому
cycle-detection и `internal/` rule H применяются к ним без изменений.

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
`Some`, `None`, `Ok`, `Err`, `Error`, `never`, базовые типы,
стандартные эффекты, `print`/`println`/`panic` — **автоматически в
скоупе**, без `import`.

Это **не противоречит** «нет неявных импортов»: prelude документирован,
LLM знает фиксированный список — «известная имплицитность», не магия.

### Почему

1. **Модуль = file ИЛИ folder** — small things stay small (single-file
   для маленьких модулей, AI-friendly: open file = full module);
   big things scale gracefully (folder-module когда модуль вырастет
   >800 LOC; нет facade boilerplate, internal helpers естественны).
2. **Folder-module = Go-style peers** — все файлы папки объявляют
   одинаковое `module X` и share namespace. Этот pattern proven
   успешным в Go (самый высокий статистический LLM-success rate
   для backend кода). AI работает с файлами/символами, не whole-module.
3. **Иерархия через точки** — даёт намекающее имя в коде без
   рефлексии или magic-resolution. `module admin.users` либо
   single-file `admin/users.nv`, либо folder-module `admin/users/`
   (с одним или несколькими peer-файлами).
4. **Явные импорты** улучшают локальность контекста ([D10 AI-first](01-philosophy.md#d10)).
5. **Запрет циклов** — сильнее, чем в Rust (где `use` циклы разрешены
   через forward declaration), но проще для рассуждения.
6. **Module-level internal helpers** — `_helpers.nv` без `export`
   виден из всех peers того же folder-module, не извне. Это **главная
   missing feature** single-file модели — теперь работает естественно.

### Что отвергнуто

- **Wildcard import** — скрывает источник, ломает локальность.
- **Циклические импорты** — усложняют рассуждение, требуют forward
  declarations.
- **`module X { ... }`-блоки внутри файла** — нарушает «file/folder =
  один модуль».
- **`mod.rs`-style entry-marker** (Rust 2015) — двойственность файлов
  `mod.rs` vs `<name>.rs`; Rust 2018+ сам движется от mod.rs.
- **Name-mirror entry** (`admin/admin.nv`) — дублирование имени в пути;
  при folder rename нужен файл-rename. Лишний boilerplate.
- **`module.nv` entry-marker** — лишний boilerplate file, часто почти
  пустой. Peers (Go-style) не требуют entry.
- **Implicit `module` declaration** (Rust/Python style — derive from
  path) — менее AI-friendly: открыл файл, не видишь к чему принадлежит.
  Go preserves explicit `package <name>` именно для AI-friendliness
  even хотя имя derive'able из path.
- **`#include` или препроцессор** — отвергнуто.
- **`pub` + `use` (Rust-стиль)** — `use` занят для embed/delegation
  ([D39](02-types.md#d39)).
- **CamelCase для видимости** (Go) — мешает естественному именованию.
- **`public`/`private`/`protected`/`package`** (Java) — overkill.

### Связь

- [D5](#d5-видимость-только-export-или-приватно) — два уровня видимости.
- [D47](#d47-видимость-деклараций) — детальные правила.
- [D78](#d78-package-tooling-novatoml-novalock-registry-chain-workspace) —
  path enforcement (extended для folder-modules).
- [02-types.md → D39](02-types.md#d39) — `use` для embed/delegation
  (поэтому не `use` для модулей).
- [08-runtime.md → D26](08-runtime.md#d26) — prelude.

### Эволюция

- **rev-1** (2026-04-..): «Файл = модуль». Один файл — один module.
  Иерархия через точки. AI-friendly: open file = full module.
- **rev-2** (2026-05-12, Plan 42): добавлена folder-module variant
  (Go-style peers). Single-file модели продолжают работать без
  изменений. Backward-compatible.
- **rev-3** (2026-05-13, Plan 42): module declaration format =
  **`parent.X`** (parent folder + target name) для **обоих** случаев
  (single-file и folder-module). До rev-3 было «full path от source
  root» (`module std.encoding.hex`); стало «parent + target»
  (`module encoding.hex`). Свойства: всегда 2 segments, refactor
  safety (move → mismatch detected), consistent между single-file и
  folder-module. **Требует миграцию существующих std/* файлов** —
  одноразовая операция (язык не в проде, breaking change приемлем).
- **rev-4** (2026-05-22, Plan 84): добавлены **относительные импорты**
  `./` / `../` — package-scoped (резолв от директории импортирующего
  файла, строго в пределах своего пакета; `../` за корень пакета →
  compile error). Bare-путь остаётся абсолютным — фича аддитивна.

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
prelude opt-out.

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

[[bin]]                                     # опционально, для каждого бинаря
name = "my-tool"
path = "bin/my_tool.nv"                     # путь к файлу-точке-входа от корня пакета

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

Пакет **по умолчанию — библиотека**: его `export`-декларации
импортируемы другими пакетами без какой-либо `[lib]`-секции. `[[bin]]`
объявляет бинарные точки входа. Пакет может быть библиотекой и иметь
бинари одновременно.

#### Source root — корень пакета

Source root (корень для резолвинга путей module ↔ file) — **сам корень
пакета** (директория с `nova.toml`). Отдельной директории `src/` и
настройки `[lib] src` **нет** (2026-05-22: убраны). Это совпадает с
фактической практикой — `std/`, `examples/`, `nova_tests/` кладут модули
прямо в корень пакета.

`module admin.audit` ↔ `<package-root>/admin/audit.nv`.

Резолвер сканирует `.nv`-файлы от корня пакета, **исключая** служебные
директории: `target/`, `.git/`, `.nova-cache/` и скрытые (`.`-префикс).
Не-`.nv` директории (`assets/`, `docs/`, …) модулями не являются —
резолвер видит модулем только директорию с `.nv`, поэтому отдельный
«забор» source-кода не нужен.

`.nv`-файлы, которые **не** должны попадать в библиотечный граф,
объявляются явно: бинарные точки входа — через `[[bin]]`; тест-онли
файлы — суффиксом `_test.nv` (D29, исключаются из release-сборки).

#### Path / module enforcement

Компилятор **обязан** проверить соответствие пути файла и имени модуля.
Несоответствие — **compile error с suggestion**:

```
error: module declaration does not match file path
  in app/audit/main.nv:1:1
  │
  1 │ module admin.audit
  │ ^^^^^^^^^^^^^^^^^^ this declares `admin.audit`
  │
  expected one of:
  - move file to: app/admin/audit.nv (preserve module name)
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
правило для пользовательских пакетов. `std/` — каноничный пример
доменной flat-иерархии.

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

> **Реализация (Plan 03.1).** Пример выше — целевой формат registry-
> эпохи. Реализованное подмножество (`path`/`git`-deps) использует
> раздельные поля вместо комбинированного `source`-префикса — точный
> формат см. в разделе «Реализация: Plan 03.1» ниже.

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

#### Prelude control attributes (D174, Plan 107, 2026-05-27)

Управление auto-import prelude через **pre-module атрибуты** — консистентно с
`#cfg`/`#doc`/`#forbid` (D99 §«Позиция»):

```nova
// Полный opt-out — никаких авто-импортов
#no_prelude
module my.realtime

// Selective opt-in — только core + runtime
#prelude(core, runtime)
module my.dsl

// Комбинирование: prelude + shadow suppressor
#prelude(core, runtime, errors)
#allow(shadow)
module collections.range
```

**`#no_prelude`** — полный opt-out. Применение: real-time/embedded (prelude
содержит GC-код), bootstrap уровни, обучающие примеры с explicit visibility.

**`#prelude(names…)`** — selective opt-in. Валидные имена:
- `core` — `Option`/`Result`/`Some`/`None`/`Ok`/`Err`/`Error`/`Ordering`.
- `runtime` — `panic`/`exit`/`assert`/`debug_assert`/`print`/`println`.
- `errors` — `RuntimeError` (6 variants) + `ReadBufferError`.
- `collections` — `Iter[T]` protocol.
- `protocols` — `From`/`Into`/`Hashable`/`Equatable`/`Comparable`/`Display`.
- `effects` — `Fail[E]` + `Time` + `Mem`.

Имена валидируются resolver'ом — `#prelude(badname)` → compile error.
Пустой `#prelude()` → **compile error**: `"use #no_prelude for empty prelude"`.

**`#allow(shadow)`** — suppress `W_PRELUDE_SHADOW` lint (D125) на уровне
модуля. Применение: DSL с переопределением prelude имён, test fixtures.

**`_module.nv` inheritance:** `#no_prelude` / `#prelude(...)` в `_module.nv`
наследуются всеми peers folder-module (D174 + D100). Per-file override
folder-level: если peer сам объявляет `#prelude(core)`, а `_module.nv` — `#no_prelude`,
peer wins. Полная спецификация — [D174](#d174-prelude-control-attributes).

Без атрибутов — full prelude facade (default, D26).

> **История:** Plan 62.F (2026-05-15) ввёл inline-клаузы `no_prelude` /
> `partial_prelude(...)` на module-line; Plan 62.F.bis (2026-05-18) добавил
> `allow_prelude_shadow`. Plan 107 (2026-05-27) перенёс их в pre-module
> атрибуты (`#no_prelude` / `#prelude(...)` / `#allow(shadow)`) согласно
> D174 — inline-формы удалены с hard error + migration hint.

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
   с D64 `realtime { ... }` блоками per-function. Pre-module атрибуты
   `#no_prelude` / `#prelude(...)` / `#allow(shadow)` (D174, Plan 107)
   дают спектр от полного opt-out до tone-down диагностики при shadowing.
   Наследуемы через `_module.nv` (D100).
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

### Реализация: Plan 03.1 / 03.2 (`path`/`git`-зависимости, version-резолюция)

D78 выше описывает **полный** целевой tooling-стек. Реализованы первые
срезы — [Plan 03.1](../../docs/plans/03.1-path-git-dependencies.md)
(`path`/`git`-зависимости) и
[Plan 03.2](../../docs/plans/03.2-version-resolution.md) (semver-диапазоны
+ резолвер). Без central registry.

**Что работает:**

- Парсинг `[dependencies]`: `{ path = "..." }`, `{ git = "...",
  rev|tag|branch = "..." }`, и `"<version>"` (registry-форма — парсится,
  но **не резолвится** до Plan 03.3).
- Межпакетный резолв: первый сегмент import-пути — имя объявленной
  зависимости; модуль резолвится в её дереве правилами D29. Импорт чужого
  пакета **обязан** идти через `[dependencies]` — workspace-членство само
  по себе пакет импортируемым не делает (explicit dependency-граф).
  `std` — неявное исключение. `internal/` (rule H) зависимости снаружи
  недоступен. Правило видимости формализовано отдельным решением —
  [D138](#d138-межпакетный-импорт--только-через-объявленную-зависимость).
- `git`-зависимости: bare-клон + worktree-checkout по commit'у в кэше
  (`$NOVA_HOME/git` либо `~/.nova/git`); offline-режим `NOVA_OFFLINE=1`.
- `nova add` / `nova update` — правка `[dependencies]` + `nova.lock`.

**Фактический формат `nova.lock` (v1, реализованное подмножество)** —
раздельные поля вместо комбинированного `source`-префикса; registry-
записи (с `hash`) добавит Plan 03.3:

```toml
version = 1

[[package]]
name = "mathlib"
source = "path"
path = "../mathlib"

[[package]]
name = "gitlib"
source = "git"
git = "https://example.org/gitlib.nv"
pin = "version:^1.0"                 # исходный пин из nova.toml
version = "1.4.2"                    # выбранная резолвером semver (Plan 03.2)
commit = "a1b2c3d4e5f6..."          # точный commit — integrity-пин
```

`git`-commit криптографически адресует дерево исходников — это и есть
tamper-evidence 03.1 (паритет с `Cargo.lock`). Отдельный `sha256` дерева,
подписи и SBOM — supply-chain hardening Plan 03.4. `path`-deps без
hash-пина (локальны, мутабельны). Неизвестные ключи парсер игнорирует —
формат расширяется без breaking change.

**Plan 03.2 ✅ — version-резолюция.** Добавлена пин-форма
`{ git = "URL", version = "^1.2" }`: версия выбирается среди semver-тегов
репозитория backtracking-резолвером (транзитивно согласованно для всего
дерева), `nova.lock` фиксирует поле `version` → воспроизводимость
(`pin`-поле хранит исходный диапазон, `version` — выбранную версию).
`nova update [--precise NAME@VERSION]` пере-резолвит. Нормативно — D139.

**Отложено:** central registry (Plan 03.3), `nova audit` /
effect-surface / capability-confined deps (Plan 03.4),
`[dev-dependencies]`, `[features]`-резолюция; PubGrub-CDCL — followup
registry-масштаба (D139).

### Связь

- [D29](#d29-модули-и-импорты) — модули, иерархия, импорты. D78
  расширяет: где лежат модули относительно корня, как резолвятся
  внешние пакеты.
- [D26](08-runtime.md#d26) — prelude. D78 описывает opt-out.
- [D30](03-syntax.md#d30) — конвенции имён модулей и файлов.
- [D138](#d138-межпакетный-импорт--только-через-объявленную-зависимость)
  — расширяет D78: правило видимости межпакетного импорта (только через
  объявленную `[dependencies]`-зависимость).
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
workspace, prelude opt-out, path/module enforcement.

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
├── nova.toml                # workspace member, name = "nova_tests"
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
   keyword'ом (`select` — keyword), используется `_test` суффикс:
   `concurrency/select_test.nv` → `module nova_tests.concurrency.select_test`.

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


---

## D99. Conditional compilation: filename suffix + `#cfg`

### Что

Два механизма platform/feature-conditional кода:

1. **Filename suffix convention** (Go-style) — для **OS routing**:
   ```
   std/net/
   ├── tls.nv               // shared signatures, always active
   ├── tls_windows.nv       // Windows-only peer
   ├── tls_linux.nv         // Linux-only peer
   └── tls_macos.nv         // macOS-only peer
   ```
   Recognized suffixes: `_windows`, `_linux`, `_macos`, `_unix`,
   `_posix`. Применяется к **peer-files** в folder-modules и к
   standalone tests. Без suffix → active всегда.

2. **`#cfg` attribute** — для **feature flags** и item-level OS routing.

   **Позиция (Plan 42.16):** module-level атрибуты идут **ПЕРЕД**
   `module` declaration (консистентно с item-level — `#cfg`/`#realtime`/
   `#pure` перед `fn`):
   ```nova
   #cfg(feature = "experimental_io_uring")
   module net.tls

   export fn connect(host str) -> Connection { ... }
   ```

   **Синтаксис predicate (Plan 42.16 — операторы `|| && !`):**
   ```text
   cfg_expr := cfg_or
   cfg_or   := cfg_and ('||' cfg_and)*
   cfg_and  := cfg_not ('&&' cfg_not)*
   cfg_not  := '!' cfg_not | cfg_atom
   cfg_atom := '(' cfg_expr ')' | key '=' string
   ```
   - `#cfg(feature = "X")` — feature flag из `NOVA_FEATURES`.
   - `#cfg(target_os = "Y")` — OS check.
   - `#cfg(A || B)` — OR. `#cfg(A && B)` — AND. `#cfg(!A)` — negation.
   - Precedence: `!` > `&&` > `||` (как C/Rust/Go). Скобки override.
   - Пример: `#cfg((target_os = "linux" || target_os = "macos") && !feature = "legacy")`
   - **Эволюция:** Plan 42.14 Ф.1 ввёл функц-форму `any/all/not`;
     Plan 42.16 заменил на операторы `|| && !` — компактнее, привычнее.
     AST internal (`Any/All/Not`) не изменился — поменялся только
     синтаксис ввода.

### Семантика

| Mechanism | Scope | Detection |
|---|---|---|
| Filename suffix | peer-file / standalone test | filename stem suffix |
| `#cfg(feature)` | **перед** module / **перед** item | `NOVA_FEATURES` env / `--features` CLI |
| `#cfg(target_os)` | **перед** module / **перед** item | `NOVA_TARGET_OS` env / host OS default |
| `#cfg(... || && !)` | то же — operator predicate | рекурсивный eval |

**Item-level `#cfg`** (Plan 42.14 Ф.2): `#cfg(...)` перед top-level
item (Fn/Type/Const). Inactive predicate → item полностью парсится но
дропается (`parse_item → None`). `module`-декларация — разделитель:
атрибуты до неё = file-level, после (перед item) = item-level.

**AND semantic** для multiple `#cfg` атрибутов: peer/item active iff
**все** cfg атрибуты match. Внутри одного атрибута — операторы
`|| && !`. Один inactive → peer skip целиком (не parsed items, не
register peer_file, не recurse imports).

### Что НЕ входит

- `#cfg` в expression position (`if #cfg(target_os = ...) { ... }`).
- `#cfg(target_arch = ...)` — на ARM/x86 differences. Future, если
  понадобится.
- Cross-compile toolchain integration — `--target=linux` на Windows-
  host требует cross C-toolchain (separate plan).

### Почему

Конкретные numbers vs альтернатив:

- **Go-only filename suffix** — мощно для OS routing, слабо для
  feature flags. Plan 18 P0 (TLS) требует и того, и другого.
- **Rust full `#[cfg(...)]` system** — рабочее, но cfg-expr +
  cfg-attr — лишняя complexity. Nova взяла `any/all/not` (Plan 42.14)
  но БЕЗ cfg-в-expression-position и cfg-attr.
- **Только runtime branching** — dead code в binary + security risk
  (включён код для другой OS).

Filename + `#cfg` (с `any/all/not`, но без cfg-expr) покрывает
production кейсы при меньшей complexity чем полный Rust.

### Связь

- [Plan 42](../../docs/plans/42-folder-modules.md) — folder-modules.
- [Plan 42.12](../../docs/plans/42.12-cfg-conditional-compilation.md) — реализация.
- [Plan 18](../../docs/plans/18-stdlib-roadmap.md) P0 — unblock'aется D99.


---

## D100. `_module.nv` peer — module-config convention

### Что

Опциональный special peer-файл с именем `_module.nv` в folder-module
с module-level attributes (`#forbid`, `#doc`, `#cfg`, `#no_prelude`, `#prelude(...)`).
Эти attributes **наследуются всеми peers** этого folder-module.

```
admin/
├── _module.nv      // #forbid Net  (module-level)
├── users.nv        // inherits #forbid Net
├── audit.nv        // inherits #forbid Net
└── helpers.nv      // inherits #forbid Net
```

### Правила

1. `_module.nv` обязан декларировать тот же `module parent.X` что и
   остальные peers (rev-3 manifest check).
2. Может содержать **ТОЛЬКО** module-level attributes — никаких
   `items` (Fn/Type/Const/Test/Let). Парсер не enforced'ит, но
   convention strict.
3. `#forbid X, Y` — applied capability-check'ом ко всем functions
   compiled module (включая peer'ы и transitive imports).
4. `#doc "..."` — accumulated for [D101](#d101) → Plan 45 consumer.
5. `#cfg(...)` — определяет active state ВСЕГО folder-module (если
   `_module.nv` cfg-off, весь folder skip'ается).
6. `_module.nv` НЕ запускается как standalone test (test_runner walker
   exclude'ит).
7. `#no_prelude` и `#prelude(...)` (D174) — наследуются peers; resolver
   pre-scan'ит `_module.nv` до принятия решения об auto-import prelude.
   Per-file override folder-level: если peer сам декларирует prelude-атрибут,
   он имеет приоритет над `_module.nv` (Plan 107 Ф.3).

### Семантика propagation

Реализация: `resolve_imports_inline_ex` обнаруживает `_module.nv` peers
импортированных folder-modules, его attrs push'ятся в `inherited_attrs`,
которые merg'аются в **entry's** `module.attrs` в конце resolve loop.
CapabilityCtx видит merged attrs естественно.

Bootstrap limitation: attrs `_module.nv` **entry's own** folder-module
не пропагируются назад в entry (entry парсится first). Workaround:
объявить `#forbid` в самом entry-peer. Production-grade refactor —
parse entry's folder-module unified первым.

### Связь

- [Plan 42](../../docs/plans/42-folder-modules.md) правило I.
- [Plan 42.10](../../docs/plans/42.10-module-level-forbid.md) — реализация.
- [Plan 42.01](../../docs/plans/42-folder-modules.md#42-1) — file-level `#forbid`.

---

## D101. `#doc` attribute — module-level inline documentation

### Что

Module-level documentation через `#doc "..."` attribute. Multi-line
через несколько `#doc` строк.

```nova
#doc "Модуль admin — управление пользователями и аудит."
#doc ""
#doc "- Все операции требуют Auth capability."
#doc "- create_user логирует в audit."
module app.admin

export fn create_user(...) { ... }
```

### Правила

1. `#doc "..."` идёт **перед** `module` declaration (Plan 42.16 —
   консистентно с `#forbid`/`#cfg`).
2. Каждый `#doc` — одна строка text (regular `"..."` string literal,
   без интерполяции).
3. Multiple `#doc` накапливаются в порядке появления (вкладывается в
   `Module.attrs` как `ModuleAttrKind::Doc(String)`).
4. Codegen и type-checker **игнорируют** `#doc` — это инструмент для
   tooling (Plan 45 `nova doc`).
5. Multi-peer: каждый peer добавляет свои `#doc`. Plan 45 consumer
   определяет порядок merge (рекомендация: alphabetical filename).

### AI-first rationale

Rust `//!`-style inner doc-comments — IDE hover показывает purpose.
Nova `nova doc <module>` (Plan 45) — CLI. Inline `#doc` — **в коде**,
LLM получает context при чтении файла без CLI invoke.

Convention для multi-line: каждая строка отдельным `#doc`. Heredoc-
syntax не вводится (bootstrap simplicity).

### Связь

- [Plan 42.11](../../docs/plans/42.11-inline-module-doc.md) — реализация.
- [Plan 45](../../docs/plans/45-nova-doc.md) — `nova doc` consumer.
- [D100](#d100) — `_module.nv` может содержать `#doc` strings.

---

## D174. Prelude control attributes — `#no_prelude`, `#prelude(...)`, `#allow(shadow)`

### Что

Три **pre-module атрибута** для управления auto-import prelude (D26).
Консистентны с `#cfg`/`#doc`/`#forbid` — идут **ПЕРЕД** `module` declaration
(D99 §«Позиция», Plan 107, 2026-05-27). Заменяют inline-клаузы Plan 62.F /
Plan 62.F.bis.

### Правило

#### Грамматика (расширение `file-level-attrs`, D99)

```
file-level-attr  := prelude-attr | allow-attr | cfg-attr | doc-attr | …
prelude-attr     := "#no_prelude"
                  | "#prelude" "(" prelude-names ")"
prelude-names    := prelude-name ("," prelude-name)*    // ≥ 1; пустой запрещён
prelude-name     := "core" | "runtime" | "errors"
                  | "collections" | "protocols" | "effects"
allow-attr       := "#allow" "(" allow-target ")"
allow-target     := "shadow"                            // extensible
```

#### `#no_prelude` — полный opt-out

```nova
#no_prelude
module my.realtime

// Никаких авто-импортов; Option/Result нужно явно:
import std.prelude.core.{Option, Result}
```

Применение: real-time/embedded (prelude содержит GC-код), bootstrap уровни,
обучающие примеры с explicit visibility.

#### `#prelude(names…)` — selective opt-in

```nova
#prelude(core, runtime)
module my.dsl

// Видимо: Option/Result/Some/None/Ok/Err/Error/Ordering (core)
// + panic/exit/assert/debug_assert/print/println (runtime).
// НЕ видимо: RuntimeError (errors), Iter (collections),
//            From/Hashable/… (protocols), Fail[E]/Time/Mem (effects).
```

Валидные имена `prelude-name`:

| Имя | Содержимое |
|---|---|
| `core` | `Option`/`Result`/`Some`/`None`/`Ok`/`Err`/`Error`/`Ordering` |
| `runtime` | `panic`/`exit`/`assert`/`debug_assert`/`print`/`println` |
| `errors` | `RuntimeError` (6 variants) + `ReadBufferError` |
| `collections` | `Iter[T]` protocol |
| `protocols` | `From`/`Into`/`Hashable`/`Equatable`/`Comparable`/`Display` |
| `effects` | `Fail[E]` + `Time` + `Mem` |

Имена валидируются resolver'ом — `#prelude(badname)` → compile error со
списком валидных имён.

Пустой `#prelude()` → **compile error**:
```
error: `#prelude()` with empty list is not allowed
  use `#no_prelude` to disable all prelude auto-imports (D174)
```

#### `#allow(shadow)` — suppress W_PRELUDE_SHADOW

```nova
#allow(shadow)
module my.dsl

type Option { foo int }         // W_PRELUDE_SHADOW silenced
const PRELUDE_VERSION int = 99  // W_PRELUDE_SHADOW silenced
```

Подавляет structured `W_PRELUDE_SHADOW` lint (D125) на уровне модуля.
User-declaration по-прежнему wins; `#allow(shadow)` убирает только предупреждение.

Item-level suppress — DEFERRED (requires generic attribute parser).

#### Конфликт `#no_prelude` + `#prelude(...)`

На одном файле нельзя комбинировать оба:

```
error: conflicting prelude attributes: `#no_prelude` and `#prelude(...)` cannot both be present
```

#### `_module.nv` inheritance

`#no_prelude` и `#prelude(...)` в `_module.nv` наследуются всеми peers
folder-module (D100 rule 7):

```
realtime/
├── _module.nv   // #no_prelude \n module realtime.lib
├── sched.nv     // module realtime.lib → inherits #no_prelude
└── timer.nv     // module realtime.lib → inherits #no_prelude
```

Per-file wins over folder: если peer сам декларирует `#prelude(core)` —
этот атрибут имеет приоритет над `#no_prelude` из `_module.nv`.

Resolver pre-scan'ит `_module.nv` до принятия решения об auto-import prelude
(Plan 107 Ф.3 — fix pre-existing bug где `_module.nv` attrs merge'ились
ПОСЛЕ prelude decision).

### Почему

1. **Консистентность с D99.** Все module-level attrs идут ПЕРЕД `module` —
   `#cfg`, `#doc`, `#forbid`. Inline-клаузы (Plan 62.F) были единственным
   исключением — нарушение, исправлено.
2. **`_module.nv` inheritance.** Pre-module attrs propagate через `_module.nv`
   естественно (D100 infrastructure). Inline-клаузы не могли.
3. **Короче.** `#prelude(core)` vs `partial_prelude(core)` — `partial_` лишнее.
   `#allow(shadow)` vs `allow_prelude_shadow` — с 19 до 13 символов.
4. **No footgun.** `partial_prelude()` молча трактовалось как `no_prelude`;
   теперь compile error с actionable hint.
5. **Расширяемость.** `#allow(X)` extensible до других suppressors.
   `#prelude(...)` extensible до аддитивного `+name`/`-name` если нужно.

### Что отвергнуто

- **Inline-клаузы (Plan 62.F / 62.F.bis).** Удалены (D174). Hard error с
  migration hint при встрече старого синтаксиса:
  ```
  error: inline `partial_prelude(...)` clause removed (D174, Plan 107)
    change:  module <path> partial_prelude(core, runtime)
    to:      #prelude(core, runtime)
             module <path>
  ```
- **`#prelude(*)` для explicit full prelude.** Не нужен — default уже full.
  Deferred Q-prelude-explicit-full.
- **`+name`/`-name` аддитивный синтаксис.** Deferred Q-prelude-additive-syntax.
  Нужен только если prelude вырастет настолько, что `#prelude(all-but-effects)`
  читается чище чем `#prelude(core, runtime, errors, collections, protocols)`.

### Сравнение с индустрией

| Язык | Механизм | Гранулярность | Позиция |
|---|---|---|---|
| **Rust** | `#![no_std]` | crate-level | inner attribute (crate root) |
| **Haskell** | `NoImplicitPrelude` + `import Prelude hiding (...)` | file-level | pragma + import |
| **Kotlin** | `@file:Suppress(...)` | file-level | before `package` |
| **Nova** | `#no_prelude` / `#prelude(...)` | file-level + folder-level | before `module` |

Nova лучше Rust (file vs crate granularity), лучше Haskell (не требует
отдельного `import Prelude hiding (...)` — группы достаточны), паритет с
Kotlin (`@file:` паттерн аналогичен `#attr` до декларации).

### Цена

1. **Breaking change.** 15 файлов на 2026-05-27 с inline-клаузами — compile
   error до миграции. Nova pre-production → grace period не нужен.
2. **Pre-scan `_module.nv`.** Один extra `fs::read_to_string` + partial parse
   в hot path. Mitigated: raw-text check перед full parse.
3. **Item-level `#[allow(shadow)]` не реализован.** Deferred. `#allow(shadow)`
   module-level достаточен для 99% use-cases.

### Связь

- [D26](08-runtime.md#d26) — prelude items (что находится в каждой группе).
- [D78](#d78-package-tooling-novatoml-novalock-registry-chain-workspace) — §«Prelude opt-out», amended.
- [D99](09-tooling.md#d99) — `#cfg` position rule. D174 следует тому же.
- [D100](#d100) — `_module.nv` inheritance (rule 7 добавлен).
- [D125](08-runtime.md#d125) — `W_PRELUDE_SHADOW` lint, подавляется `#allow(shadow)`.
- [Plan 107](../../docs/plans/107-prelude-attribute-syntax.md) — реализация.

### Открытые вопросы

- **Q-prelude-additive-syntax.** Нужна ли форма `#prelude(-effects)` для
  «full prelude минус один модуль»?
- **Q-item-level-allow-attr.** Item-level `#[allow(shadow)]` перед конкретным
  объявлением — когда добавить?

---

## D134. Symbol mangling v0 — C-имена свободных функций

**Статус:** принято, реализовано ([Plan 81](../../docs/plans/81-module-resolution-hardening.md) Ф.6).

**Контекст.** Nova компилируется в C. Свободная функция пользователя
эмитилась как C-функция `nova_fn_<name>` — **без пути модуля**. Глобальный
C-namespace без стабильной схемы: имя символа зависело от порядка
регистрации overload'ов (первый получал «голое» имя) — т.е. C-имя
функции могло **меняться** в зависимости от набора импортов. Две функции
с одним именем в разных модулях — потенциальная коллизия линковки.

**Решение.** Версионированная схема mangling **v0**: C-имя свободной
функции кодирует путь объявляющего модуля.

```
nova_fn_  <L1><seg1>  <L2><seg2>  …  <Ln><name>
```

- Префикс `nova_fn_` — **зарезервирован**: не пересекается с
  runtime-символами (`nova_str_*`, `nova_int_*`, `nova_alloc`, …) и с
  mangled-методами (`Nova_<Type>_method_*`).
- Каждый сегмент пути модуля и имя функции кодируются как
  `<десятичная-длина><идентификатор>`. Length-prefix **однозначен**:
  Nova-идентификаторы не начинаются с цифры, поэтому граница «число
  длины ↔ идентификатор» определяется без разделителя (разделитель `_`
  был бы неоднозначен — `_` встречается в snake_case-именах модулей).
- Результат — корректный C-идентификатор (`[A-Za-z_][A-Za-z0-9_]*`),
  ASCII-only (лексер Nova ASCII-only — punycode не нужен).

**Пример.** Функция `want_int` в модуле `nova_tests.plan79.assign` →
`nova_fn_10nova_tests6plan796assign8want_int`.

**Перегрузки.** D84-overload'ы одного имени дополнительно различаются
param-type-суффиксом поверх mangled base (как и раньше).
**Мономорфизация.** Mono'д generic-инстансы — кодирование type-аргументов
поверх mangled base (`compute_mono_name`).

**Лимит длины.** При превышении безопасного порога C-идентификатора
(240 символов) — усечение до 216 + суффикс `_h<FNV1a-32-hex>`
(сохраняет уникальность).

**Exempt (не mangled'ятся).** Runtime/`external`-функции (`builtins.nv`
registry) и ABI-символы — FFI/ABI-поверхность, остаются с текущими
именами. Синтетический entry `nova_fn_main_impl` и closure-type
адаптеры (`nova_fn_vi`, `nova_fn_ii`, …) — не пользовательские функции,
exempt. Функции, чей модуль не определён (peer_files не заполнены) —
fallback на legacy `nova_fn_<name>`.

**Почему «v0».** Схема версионирована: будущие ревизии (например,
кодирование сигнатур, `nova demangle` для стек-трейсов) — отдельные
версии. v0 фиксирует базовый инвариант: **стабильное, не зависящее от
набора импортов, бесколлизионное C-имя с путём модуля**.

### Связь

- [Plan 81](../../docs/plans/81-module-resolution-hardening.md) Ф.6 — реализация.
- [D84](08-runtime.md#d84) — overload-резолюция (param-суффикс).
- [Plan 48](../../docs/plans/48-closures-in-generics.md) — mono-кодирование.
- [Plan 03](../../docs/plans/03-package-ecosystem-roadmap.md) — multi-crate
  будущее, где модульный mangling строго обязателен.

---

## D138. Межпакетный импорт — только через объявленную зависимость

**Статус:** принято, реализовано ([Plan 03.1](../../docs/plans/03.1-path-git-dependencies.md) Ф.3).

**Контекст.** Первый сегмент import-пути — имя пакета ([D29](#d29-модули-и-импорты)).
До [Plan 03.1](../../docs/plans/03.1-path-git-dependencies.md) резолвер
искал модули по repo-root: модуль **любого** пакета в дереве находился
неявно. В monorepo это значило, что член workspace мог импортировать
другого члена, **не объявляя** зависимости от него. Looseness: неявный
dependency-граф, засорённый namespace, нет единой точки для версии и
ограничений зависимости.

**Решение.** Импорт, чей первый сегмент резолвится в файл **другого
пакета** (иной корень `nova.toml`), требует, чтобы этот пакет был
**явно объявлен** в `[dependencies]` импортирующего пакета.

- **Workspace-членство само по себе импортируемости не даёт** (модель
  Cargo, не Go). `[workspace] members` группирует пакеты для сборки и
  единого `nova.lock` — но `import` между ними всё равно проходит через
  `[dependencies]` (обычно `{ path = "..." }`).
- **`std` — неявное исключение:** стандартная библиотека доступна без
  записи в `[dependencies]` (как Rust `std`). Машинерия D138 — для
  **не-`std`** пакетов.
- Импорт чужого пакета мимо `[dependencies]` — **ошибка компиляции** с
  указанием обоих пакетов и подсказкой объявить зависимость.
- Внутри своего пакета межмодульный импорт не затронут: путь от корня
  пакета либо относительный `./` / `../` ([D29](#d29-модули-и-импорты)
  rev-4) — `package_root_of` тот же.
- `internal/`-граница ([D29](#d29-модули-и-импорты) rule H) соблюдается
  и через границу пакета: `dep.internal.*` снаружи недоступен.

**Почему.**

- **Explicit dependency-граф — AI-first.** Откуда взялся импортируемый
  символ — видно прямо в манифесте, без сканирования дерева. Агент
  (и человек) читает `[dependencies]` и знает полную внешнюю поверхность.
- **Гигиена namespace.** Член workspace, от которого ты не зависишь, не
  должен быть случайно импортируемым — иначе любой пакет «видит» все
  остальные.
- **Единая точка истины.** Версия, `forbid`-ограничения (capability-
  confined deps, Plan 03.4), effect-surface зависимости — привязаны к
  одной записи `[dependencies]`.

**Коллизия имён.** Объявленная зависимость и локальный модуль с
одинаковым первым сегментом — выигрывает зависимость (объявление
явное). Имя `std` в `[dependencies]` запрещено (зарезервировано).
Дубликат имени зависимости — ошибка конфигурации.

### Связь

- [D78](#d78-package-tooling-novatoml-novalock-registry-chain-workspace)
  — `nova.toml` / `[dependencies]` / workspace; D138 фиксирует **правило
  видимости** межпакетного импорта поверх формата D78.
- [D29](#d29-модули-и-импорты) — модули, импорты, `internal/` rule H,
  относительные импорты.
- [Plan 03.1](../../docs/plans/03.1-path-git-dependencies.md) — реализация
  (`lookup_dependency` + ужесточение repo-root резолва).
- [Plan 84](../../docs/plans/84-relative-imports.md) — относительные
  импорты package-scoped; межпакетное — всегда полный путь.

---

## D139. Version-диапазоны git-зависимостей — резолв по тегам репозитория

**Статус:** принято, реализовано ([Plan 03.2](../../docs/plans/03.2-version-resolution.md)).

**Контекст.** [Plan 03.1](../../docs/plans/03.1-path-git-dependencies.md)
дал `git`-зависимости с **точным пином** (`rev`/`tag`/`branch`). Но
точечный пин — это ручное управление: чтобы получить багфиксы, надо
вручную менять `tag`. Нужны **semver-диапазоны** (`^1.2`) — и
**resolver**, выбирающий согласованные версии для всего дерева. До
центрального registry ([Plan 03.3](03-package-ecosystem-roadmap.md))
неоткуда взять «вселенную версий» пакета.

**Решение.** `git`-зависимость принимает пин-форму
`{ git = "URL", version = "<semver-диапазон>" }` (взаимоисключающую с
`rev`/`tag`/`branch`).

- **Вселенная версий — semver-теги репозитория.** Тег `v1.2.0` либо
  `1.2.0` (префикс `v` опционален) парсится как semver; не-semver теги
  игнорируются. Среди подходящих диапазону выбирается **наибольший**.
- **Диапазоны** (semver 2.0.0): `^1.2`, `~1.2.3`, `>=1.0, <2.0`,
  `1.2.*`, `*`, `=1.2.3`; голая `1.2.3` ≡ `^1.2.3` (конвенция Cargo).
  Pre-release-версии вне диапазонов по умолчанию (паритет с Cargo).
- **Резолвер** — корректный backtracking (DFS, highest-version-first,
  распространение ограничений, откат при конфликте): транзитивно
  согласованный набор версий (одна версия пакета на всё дерево);
  при неразрешимости — диагностируемый конфликт. *Не* полный PubGrub:
  CDCL-обучение (оптимизация для больших графов) — followup
  registry-эры; backtracking корректен и полон для git-tag-масштаба.
- **`nova.lock`** фиксирует `version` (выбранная semver) рядом с
  `commit` → воспроизводимость: повторная сборка не двигает версию,
  даже если upstream появился новый тег. `nova update` пере-резолвит;
  `nova update --precise NAME@VERSION` — точная фиксация.

**Почему теги, а не registry.** Источник версий абстрагирован
(`DependencyProvider`): git-теги — то, что доступно **без**
registry-инфраструктуры. Registry ([Plan 03.3](03-package-ecosystem-roadmap.md))
подключит свой источник к тому же резолверу. Cargo для `git`-deps
диапазонов не даёт (только точечный пин) — Nova здесь строго мощнее:
`git`-зависимость может «жить на ветке версий», а не на фиксированном
теге, сохраняя воспроизводимость через lock.

### Связь

- [D78](#d78-package-tooling-novatoml-novalock-registry-chain-workspace)
  — `nova.toml` / `nova.lock` / «Resolution»; D139 — реализованный
  срез резолюции версий.
- [D138](#d138-межпакетный-импорт--только-через-объявленную-зависимость)
  — межпакетный импорт через `[dependencies]`; D139 добавляет в
  `[dependencies]` версионную форму.
- [Plan 03.2](../../docs/plans/03.2-version-resolution.md) — реализация
  (`semver.rs`, `resolver.rs`, `git_cache::list_versions`).
- Ориентир: Cargo (semver, caret/tilde, pre-release-policy; PubGrub).

---

## D140. Effect-aware зависимости — effect-surface и `forbid` на границе

**Статус:** принято, реализовано ([Plan 03.4](../../docs/plans/03.4-effect-aware-tooling.md)).

**Контекст.** Nova трекает **эффекты в типах** ([D62](04-effects.md#d62)).
Это даёт менеджеру пакетов то, чего не может ни Cargo, ни Go: в Cargo/npm
узнать, что зависимость ходит в сеть, **невозможно** без аудита кода —
supply-chain-атаки годами этим пользуются (внезапная сетевая активность
в патч-релизе). Раз эффекты уже в сигнатурах — менеджер обязан их
использовать.

**Решение.** Три effect-aware-механизма поверх `[dependencies]`:

- **effect-surface** — агрегированный effect-row **публичного** API
  пакета: объединение эффектов всех `export`-функций. D28 (public fn
  объявляет эффекты явно) делает surface точной **by construction** —
  без межпроцедурного анализа. `nova info <pkg>` показывает surface +
  разбивку «эффект → какие функции его вносят» (видно *откуда* `Net`).
- **effect-diff** — разница effect-surface двух версий.
  `nova info <new> --diff <old>` → добавленные/убранные эффекты.
  Появление `Net`/`Fs` в minor/patch-релизе ранее «чистого» API —
  **supply-chain red flag**; `--fail-on-new` даёт CI-gate.
- **capability-confined deps** — `[dependencies] foo = { …,
  forbid = ["Net", "Fs"] }`: компилятор при сборке вычисляет
  effect-surface зависимости и **падает**, если она содержит
  запрещённый эффект. Песочница на уровне **типов**, не рантайма —
  строго сильнее Deno-permissions (те рантаймовые, ловят нарушение
  только при исполнении).

**Почему.** Эффект-система уже оплачена языком ([D62](04-effects.md#d62));
effect-surface — её «бесплатное» применение в tooling'е. Это превращает
экосистему из «догнать Cargo» в «структурно безопаснее Cargo по
supply-chain» ([Plan 03](../../docs/plans/03-package-ecosystem-roadmap.md) §4).

**Граница.** Surface — по **объявленным** эффектам публичных сигнатур;
приватные функции в неё не входят (внешнему потребителю невидимы).
`forbid` здесь — на **границе зависимости** (что дозволено пакету-
зависимости), в отличие от [D63](04-effects.md#d63) `forbid { }` —
блока в коде. effect-diff registry-эры (effect-surface в `nova.lock` /
в метаданных registry) — [Plan 03.3](../../docs/plans/03-package-ecosystem-roadmap.md)+.

### Связь

- [D62](04-effects.md#d62) — эффекты в сигнатурах; фундамент D140.
- [D63](04-effects.md#d63) — `forbid { }`-блок в коде; D140 переносит
  `forbid` на границу зависимости.
- [D78](#d78-package-tooling-novatoml-novalock-registry-chain-workspace)
  — `[dependencies]`; D140 добавляет туда `forbid`-поле.
- [Plan 03.4](../../docs/plans/03.4-effect-aware-tooling.md) — реализация
  (`effect_surface.rs`, `nova info`).
- [Plan 45](../../docs/plans/45-nova-doc.md) — `effect_matrix` (`nova doc`)
  — источник per-fn эффектов.
