---
source_rev: 07df7d2c9
source_date: 2026-08-02
---

<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Как создать модуль Nova

[English](authoring-a-module.md) | **Русский**

> Общий гайд: от пустого каталога до публикуемого пакета. Native-backed модуль
> (обёртка над `.c`/готовой `.lib`) — **частный случай** в конце (§7).
> **`[ffi.staticlib]` (собираемый cargo/make-staticlib) RETRACTED владельцем
> (Plan 195, 2026-07-10)** — native-модуль обязан собираться БЕЗ Rust/cargo,
> только `.nv` + опционально `.c` (компилит clang) + опционально готовая
> `.lib`/`.a` (линкуется, не собирается). §7.2 ниже оставлен как исторический
> контекст (что было и почему убрано).
>
> Смежные документы (не дублируются здесь, а до-читываются по ссылке):
> [module-conventions](../dev/module-conventions.md) (дизайн модуля: эффект-плумбинг,
> value/must-consume-типы, домен ошибок), [nv-coding-style](../dev/nv-coding-style.md)
> (стиль `.nv`-кода), [ffi-cookbook](ffi-cookbook.md) (механика FFI:
> `extern "C"`, указатели, `CStr`, `[ffi]`), [spec D78](../../spec/decisions/07-modules.md#d78-package-tooling-novatoml-novalock-registry-chain-workspace)
> (нормативные правила `nova.toml` / module-path).

## 0. TL;DR

1. Создай каталог; положи в корень `nova.toml` с `[package] name`.
2. Пиши `.nv`-файлы — **путь файла = путь модуля** (`foo/bar.nv` ⇒ `module foo.bar`).
3. Тесты — рядом, в файлах `*_test.nv` (или `test "…" { }`-блоки внутри модуля).
4. Публичную поверхность помечай `export` + `#stable(since = "X")`.
5. Нужны C-артефакты — задекларируй их в `[ffi]` (готовый `.c`-шим + опционально
   готовая `.lib`/`.a`); при импорте модуля они соберутся и слинкуются
   автоматически (§7).

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

Папка = ОДИН модуль из равноправных файлов (файлы одного модуля делят одно
объявление `parent.folder`). Файл и папка одного имени в одном каталоге
запрещены.

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
папке ([test-conventions](../dev/test-conventions.md)).

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
типах** ([module-conventions](../dev/module-conventions.md)):

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
[ffi-cookbook](ffi-cookbook.md); здесь — только **как подключить артефакты
к сборке**, чтобы `import` модуля тянул их автоматически.

Native-зависимость декларируется в `nova.toml` через единственную секцию:

### 7.1. Готовые `.c`-шимы и системные `.lib` — `[ffi]`

```toml
[ffi]
c_shims      = ["native/sqlite3_shim.c"]   # компилируются и линкуются
include_dirs = ["native/", "third_party/sqlite3/"]  # clang -I
libs         = ["sqlite3"]                 # системные: clang -lsqlite3 / sqlite3.lib
```

Если системная `.lib` не в стандартном search-path (vcpkg-триплет, vendored
копия) — линковка резолвится и подключается прямо в build-пайплайне
(`test_runner.rs::build_command`), тем же условным-по-факту-использования
паттерном D337, что у brotli/`net.c`; см. `std/tls` ниже.

### 7.2. `[ffi.staticlib]` (собираемый cargo/make-staticlib) — RETRACTED (Plan 195)

**Существовало (Plan 195), ретрактировано владельцем 2026-07-10.** Позволяло
модулю требовать **построить** native-артефакт (`cargo build`, `make`) как
часть своей сборки — единственным пользователем был `compiler-codegen/
tls_shim/` (Rust-staticlib поверх `rustls`). Противоречит канону тулчейна
(компилятор Nova + clang, БЕЗ Rust/cargo) — снято целиком
(`FfiStaticlibConfig`/`resolve_ffi_staticlib`/парсинг секции убраны из
`manifest.rs`/`test_runner.rs`). `tls_shim/` заменён на `nova_rt/tls_c_shim.c`
(mbedTLS) — обычный `[ffi]`-путь (§7.1), без cargo/build-скрипта вообще:
mbedTLS ставится ЗАРАНЕЕ через `vcpkg install` (готовая `.lib`, не собираемая
на лету), `tls_c_shim.c` компилируется/линкуется условно как ЛЮБОЙ другой
рантайм-модуль (`net.c`/`brotli_shim.c`), безо всякой манифест-декларации.

> **Эталон паттерна** (2026-07 актуальный) — `std/tls` в монорепо
> (`nova_rt/tls_c_shim.c` + vcpkg mbedTLS, БЕЗ `[ffi.staticlib]`, БЕЗ
> манифест-декларации вообще). Полная механика — [ffi-cookbook §retracted](ffi-cookbook.md#ffistaticlib--retracted-plan-195).

## 8. Именование и публикация (внешний пакет)

Конвенция для внешних (в т.ч. native-backed) пакетов — [D78-амендмент, Plan 195](../../spec/decisions/07-modules.md#именование-внешних-пакетов-репозиториев-амендмент-plan-192-2026-07-10):

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
6. Native — `[ffi]` (готовые `.c`-шимы + готовая `.lib`/`.a`; `[ffi.staticlib]`
   собираемый-cargo/make — RETRACTED, Plan 195).
7. Внешний пакет — репо `nova-<пакет>`, native в `native/`, git-зависимость.
