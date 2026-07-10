<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Как создать модуль Nova

> Общий гайд: от пустого каталога до публикуемого пакета. Native-backed модуль
> (обёртка над `.c`/`.lib`/`cargo`-staticlib) — **частный случай** в конце (§7).
>
> Смежные документы (не дублируются здесь, а до-читываются по ссылке):
> [module-conventions](../module-conventions.md) (дизайн модуля: эффект-плумбинг,
> value/must-consume-типы, домен ошибок), [nv-coding-style](../nv-coding-style.md)
> (стиль `.nv`-кода), [ffi-cookbook](../ffi-cookbook.md) (механика FFI:
> `extern "C"`, указатели, `CStr`, `[ffi.staticlib]`), [spec D78](../../spec/decisions/07-modules.md#d78-package-tooling-novatoml-novalock-registry-chain-workspace)
> (нормативные правила `nova.toml` / module-path).

## 0. TL;DR

1. Создай каталог; положи в корень `nova.toml` с `[package] name`.
2. Пиши `.nv`-файлы — **путь файла = путь модуля** (`foo/bar.nv` ⇒ `module foo.bar`).
3. Тесты — рядом, в файлах `*_test.nv` (или `test "…" { }`-блоки внутри модуля).
4. Публичную поверхность помечай `export` + `#stable(since = "X")`.
5. Нужны C/Rust-артефакты — задекларируй их в `[ffi]` / `[ffi.staticlib]`; при
   импорте модуля они соберутся и слинкуются автоматически (§7).

## 1. Layout пакета

Пакет — это каталог с `nova.toml` в корне. **Source root = корень пакета**
(отдельного `src/` нет — D78, 2026-05-22). Модули лежат прямо в подкаталогах:

```
nova-greet/                 репозиторий: nova-<пакет> (§8)
├── nova.toml               манифест (обязателен)
├── LICENSE
├── README.md
├── greet.nv                module greet          (корневой модуль пакета)
├── greet_test.nv           тесты рядом с модулем
└── format/
    ├── ascii.nv            module format.ascii
    └── ascii_test.nv       тесты рядом
```

Служебные каталоги (`target/`, `.git/`, скрытые `.`-префикс) резолвер
пропускает. Не-`.nv` каталоги (`assets/`, `docs/`) модулями не считаются.

## 2. `nova.toml` — манифест

Минимум — `[package] name`; `version` желателен. Полная схема — [D78](../../spec/decisions/07-modules.md#d78-package-tooling-novatoml-novalock-registry-chain-workspace).

```toml
[package]
name = "greet"                     # snake_case (D30); имя пакета = префикс модулей
version = "0.1.0"                  # semver
nova-version = "0.5"               # минимальная версия Nova
description = "Приветствия на разных языках"
license = "MIT OR Apache-2.0"      # SPDX
repository = "https://github.com/you/nova-greet"

[[bin]]                            # опц.: бинарная точка входа
name = "greet"
path = "bin/greet.nv"

[dependencies]                     # опц.: внешние пакеты
some-lib = "1.2"                                        # из реестра
internal = { path = "../internal" }                     # локальный
remote   = { git = "https://github.com/…", tag = "v1" } # git (Plan 03.1/03.2)
```

Пакет **по умолчанию — библиотека**: его `export`-декларации импортируемы
другими пакетами без какой-либо `[lib]`-секции. `[[bin]]` добавляет бинарные
точки входа (пакет может быть и библиотекой, и набором бинарей).

## 3. Module path = file path (D78)

Компилятор **обязательно** сверяет объявление `module …` с путём файла;
несоответствие — `E_D78_MODULE_PATH_MISMATCH` с подсказкой. Правило (rev-3):

| Файл (от корня пакета `greet`) | Объявление | Импорт |
|---|---|---|
| `greet.nv` | `module greet` | `import greet.{hello}` |
| `format/ascii.nv` | `module format.ascii` | `import format.ascii.{…}` |
| `format/ascii/upper.nv` (peer папки) | `module format.ascii` | — |

Папка = ОДИН модуль из co-equal файлов (peer-файлы делят одно объявление
`parent.folder`). Файл и папка одного имени в одном каталоге запрещены.

## 4. Публичная поверхность и стабильность

- `export` — то, что видно снаружи модуля/пакета; без `export` элемент
  module-private. Межпакетный импорт — только через `export` (D216-ecosystem).
- `#stable(since = "X")` на каждом публичном элементе — semver-контракт. Для
  библиотек это можно сделать **обязательным**: `[lib] enforce-stability = true`
  превращает отсутствие маркера в ошибку `nova doc --check` (D127).
- Незрелый API — `#unstable` / `#experimental` вместо `#stable`.

```nova
module greet

#stable(since = "0.1")
export fn hello(name str) -> str => "Привет, ${name}!"
```

## 5. Тесты рядом с модулем

Тесты живут **рядом** с модулем — в файлах `*_test.nv` (исключаются из
release-графа) либо `test "…" { }`-блоками внутри самого модуля. Не складывать
тесты в отдельное дерево. Классификация pos/neg — по `EXPECT_*`-маркеру, не по
папке ([test-conventions](../test-conventions.md)).

```nova
module greet

test "hello вставляет имя" {
    assert(hello("Ada") == "Привет, Ada!")
}
```

Для эффект-модулей (§6) обязателен **mock-handler-тест** — детерминизм без
реального ресурса.

## 6. Дизайн модуля (кратко; полное — module-conventions)

Для I/O-, OS- и ресурсных подсистем канон Nova — **эффект-плумбинг + фасад на
типах** ([module-conventions](../module-conventions.md)):

- **Эффект** — внутренняя dispatch-точка (`type Fs effect { … }`); юзер его не
  зовёт напрямую → мокабельность (`with Fs = mem_fs() { … }`).
- **User-API** — методы на типах + free-fns (`File.open(path)`), эффект виден в
  effect-row сигнатуры, а не в имени опа.
- Мелкие значения — `value`-record; ресурсы — must-consume `@close() -> Result`.
- Ошибки — один структурный `XError { kind, … }` + OPEN `ErrorKind`.
- byte-first: сырой I/O — `[]u8`; `str` только через `from_utf8 -> Result`.

Чистая алгоритмика (парсинг, кодировки, календарь) — обычные `.nv`-функции без
эффекта.

## 7. Native-backed модуль (частный случай)

Модуль может стоять поверх C-библиотеки или Rust-крейта. Тонкий слой FFI
(`extern "C" fn`, типы-хендлы, `CStr`, указатели, ABI) — целиком в
[ffi-cookbook](../ffi-cookbook.md); здесь — только **как подключить артефакты
к сборке**, чтобы `import` модуля тянул их автоматически.

Два вида native-зависимостей декларируются в `nova.toml`:

### 7.1. Готовые `.c`-шимы и системные `.lib` — `[ffi]`

```toml
[ffi]
c_shims      = ["native/sqlite3_shim.c"]   # компилируются и линкуются
include_dirs = ["native/", "third_party/sqlite3/"]  # clang -I
libs         = ["sqlite3"]                 # системные: clang -lsqlite3 / sqlite3.lib
```

### 7.2. Собираемый staticlib (cargo/make) — `[ffi.staticlib]` (Plan 192)

Когда артефакт надо **построить** (например Rust-staticlib поверх `rustls`):

```toml
[ffi.staticlib]
kind         = "rust-staticlib"          # способ сборки (пока поддержан он)
path         = "native/tls_shim"         # каталог крейта (относительно nova.toml)
lib          = "nova_tls_shim"           # basename артефакта (без lib-/.a-/.lib-)
build        = "cargo build --release"   # команда сборки (cwd = path)
cache        = "target/native-cache/tls" # опц. кэш собранного артефакта
link_windows = ["bcrypt", "ntdll"]       # доп. системные либы (по платформам)
link_unix    = []                        # для Linux/macOS (пусто — тянутся через libuv)
```

Билд-система резолвит staticlib **лениво, по факту использования** модуля:
`cache → артефакт крейта → cargo build-on-demand`, с **mtime-инвалидацией** по
исходникам крейта (правка `.rs` → пересборка). Линковка — автоматическая при
`import` модуля. `link`/`link_windows`/`link_unix` дают платформо-корректный
набор системных зависимостей.

> **Эталон паттерна** — репозиторий-образец `nova-tls` (standalone-пакет `tls`
> поверх `native/tls_shim`) и `std/tls` в монорепо (то же `[ffi.staticlib]`).
> Полная механика FFI-границы — [ffi-cookbook §staticlib-манифест](../ffi-cookbook.md#staticlib-манифест-plan-192).

## 8. Именование и публикация (внешний пакет)

Конвенция для внешних (в т.ч. native-backed) пакетов — [D78-амендмент, Plan 192](../../spec/decisions/07-modules.md#именование-внешних-пакетов-репозиториев-амендмент-plan-192-2026-07-10):

| Сущность | Конвенция | Пример |
|---|---|---|
| Репозиторий | `nova-<пакет>` | `nova-tls` |
| Имя пакета (`[package] name`) | `<пакет>` | `tls` |
| Корень модуля | `<пакет>.*` | `import tls.{TlsStream}` |
| Native-артефакты | `native/` | `native/tls_shim/` |

Публикация: закоммить пакет в репозиторий `nova-<пакет>`; потребитель
подключает его как git-зависимость —
`[dependencies] tls = { git = "https://…/nova-tls", tag = "v0.1.0" }`. Реестр
(named `<пакет> = "1.2"`) — Plan 03.3, отдельно.

## 9. Чек-лист нового модуля

1. `nova.toml` с `[package] name` в корне.
2. `.nv`-файлы: `module path = file path`; папка = один модуль.
3. Публичное — `export` + `#stable(since)`; для либы — `enforce-stability = true`.
4. Тесты рядом (`*_test.nv` / `test`-блоки); эффект-модуль → mock-тест.
5. Дизайн по module-conventions (эффект-плумбинг + фасад; value/must-consume; один `XError`).
6. Native — `[ffi]` (готовые шимы/libs) или `[ffi.staticlib]` (собираемый staticlib).
7. Внешний пакет — репо `nova-<пакет>`, native в `native/`, git-зависимость.
