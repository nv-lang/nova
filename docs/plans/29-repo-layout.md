// SPDX-License-Identifier: MIT OR Apache-2.0
# План 29: реорганизация корня репозитория

> **Статус:** план, не начат.
> **Создан:** 2026-05-11.
> **Приоритет:** низкий — чисто косметический, не блокирует ни один план.
> **Зависит от:** Plan 28 ✅ (nova CLI готов — пути теперь в одном месте).
> **Открыт:** наблюдение после Plan 28 что корень смешивает
> compiler/, cli/, runtime/, tests/, stdlib/ без явного разделения.

---

## Зачем

Текущий корень:

```
nova-lang/
  compiler-codegen/   ← Rust crate, внутренний инструмент
  nova-cli/           ← Rust crate, пользовательский CLI
  nova_tests/         ← Nova package, тест-корпус
  std/                ← Nova package, stdlib
  examples/           ← Nova package, туториалы
  spec/               ← документация
  docs/               ← документация
  editors/            ← LSP/editor configs
```

Проблема: смешаны Rust crate'ы, Nova packages и docs без явной
группировки. Новый контрибьютор не видит сразу «где что».

Желаемый layout:

```
nova-lang/
  compiler/           ← был compiler-codegen/ (Rust crate)
  cli/                ← был nova-cli/ (Rust crate)
  runtime/            ← был compiler-codegen/nova_rt/ (C sources, выделен)
  tests/              ← был nova_tests/ (Nova package)
  stdlib/             ← был std/ (Nova package)
  examples/           ← без изменений
  spec/               ← без изменений
  docs/               ← без изменений
  editors/            ← без изменений
```

---

## Оценка сложности

| Переименование | Сложность | Главный риск |
|---|---|---|
| `nova-cli/` → `cli/` | **низкая** | 1 путь в Cargo.toml + docs |
| `compiler-codegen/` → `compiler/` | **средняя** | пути в nova-cli, build scripts, docs |
| `nova_tests/` → `tests/` | **высокая** | D78: имя директории = имя пакета; все `.nv` модули `nova_tests.*` |
| `std/` → `stdlib/` | **высокая** | то же самое: `std.*` → `stdlib.*` во всех `.nv` |
| Выделить `nova_rt/` → `runtime/` | **средняя** | пути в test_runner.rs, build scripts, compile commands |

**Суммарно:**

- ~5 изменений Rust-кода (paths в Cargo.toml + resolve_paths)
- ~1 изменение nova.toml (workspace members)
- ~150+ `.nv` файлов с `module nova_tests.*` → `module tests.*`
  (или оставить `nova_tests` как package name — см. вариант Б ниже)
- Все `import std.*` → `import stdlib.*` (если переименуем std)
- docs, README, plan-файлы

---

## Варианты

### Вариант А — полный rename (как описано выше)

Переименовать директории + обновить module names в `.nv` + обновить
все импорты.

**Pros:** корень выглядит чисто.
**Cons:** D78 требует менять имена пакетов и все `module`/`import`
строки в >150 `.nv` файлах. Если пакет называется `tests` — это
слишком общее имя для будущего package registry.

### Вариант Б — половинчатый rename (рекомендуется)

Переименовать только Rust crates (внешние пользователи их не видят);
Nova packages оставить как есть.

```
compiler-codegen/ → compiler/      # Rust internal crate
nova-cli/         → cli/           # Rust user-facing crate
nova_rt/ (inline) → runtime/       # C runtime, выделить из compiler/
```

Nova packages остаются:
```
nova_tests/    — без изменений (package name = "nova_tests", D78 compliant)
std/           — без изменений (package name = "std")
```

**Pros:** Nova пакеты не меняют имён — нет риска сломать module resolution.
**Cons:** корень всё ещё имеет `nova_tests/` и `std/`.

### Вариант В — только документирование (minimal)

Ничего не переименовывать. Добавить `docs/layout.md` с объяснением
структуры — «nova_tests это тест-корпус, std это stdlib, ...».

**Pros:** нулевой риск.
**Cons:** проблема не решена, но задокументирована.

---

## Рекомендация

**Вариант Б** — переименовать только Rust crates.

Это убирает confusion «что такое compiler-codegen vs nova-cli», но
не ломает Nova module system. `nova_tests` и `std` — технически
правильные имена для Nova packages (D78 enforcement).

Если в будущем появится package registry — имена там задаются в
`nova.toml`, не в директориях, и можно будет изменить независимо.

---

## Фазы (Вариант Б)

### Ф.1 — `compiler-codegen/` → `compiler/`

**Файлы:**
- `git mv compiler-codegen compiler`
- `nova-cli/Cargo.toml`: `path = "../compiler-codegen"` → `path = "../compiler"`
- `build_libuv.ps1`: путь к `compiler-codegen\nova_rt\libuv`
- `compiler/build_c.ps1`, `compiler/build_c.sh` (внутри): пути к `nova_rt` не меняются (они относительны)
- `nova.toml`: комментарии
- `README.md`, `compiler-codegen/README.md` → теперь `compiler/README.md`
- `docs/test-conventions.md`: ссылки на `compiler-codegen/`
- `docs/plans/*.md`: упоминания пути
- `nova-cli/src/main.rs`: `resolve_paths` — `repo.join("compiler-codegen")` → `repo.join("compiler")`
- `compiler/src/main.rs`: default path для `cg_include` и `rt_dir`

**Объём:** ~30 изменений в файлах + git mv.

### Ф.2 — `nova-cli/` → `cli/`

**Файлы:**
- `git mv nova-cli cli`
- `cli/Cargo.toml`: path dep `../compiler-codegen` → `../compiler` (уже из Ф.1)
- README.md, docs: ссылки `nova-cli/` → `cli/`
- `.gitignore`: если есть `nova-cli/target/` — нет, покрыт `target/`

**Объём:** ~10 изменений.

### Ф.3 — выделить `compiler/nova_rt/` → `runtime/` (опционально)

Вынести C runtime sources из compiler crate в отдельную директорию
на уровне корня.

**Сложность высокая:** `compiler/` Rust crate включает `nova_rt/`
через hardcoded paths в `build_command`. Выделение потребует:
- Обновить все пути к `alloc.c`, `effects.c`, `fibers.c` в test_runner.rs
- `detect_or_build_libuv` — путь к libuv submodule
- `build_c.ps1`, `build_c.sh`
- Решить вопрос: git submodule libuv живёт внутри compiler/ сейчас

**Рекомендация:** отложить Ф.3 до выделения runtime в отдельный
Cargo-independent build (Plan 27+ era). Сейчас runtime жёстко связан
с compiler Rust crate.

---

## Acceptance criteria

- `nova test` работает после rename (все пути в resolve_paths обновлены)
- `nova build nova_tests/basics/literals.nv` работает
- `nova regen-runtime --check` → exit 0
- `cargo build` в `compiler/` работает
- `cargo build` в `cli/` работает
- `.gitignore` покрывает target/ корректно
- `nova.toml` workspace members не изменены (nova_tests, std, examples)

---

## Что НЕ входит

- Переименование Nova packages `nova_tests` / `std` — слишком высокий
  риск, нет пользы до наличия package registry
- Выделение `runtime/` (Ф.3) — отложено
- Rust workspace `Cargo.toml` в корне — отдельное решение (требует
  анализа: workspace breaking change для nova-codegen как standalone
  internal tool)

---

## Связь

- Plan 28 ✅ — nova-cli создан; пути захардкожены в одном месте
  (`resolve_paths` в `nova-cli/src/main.rs`), что упрощает Ф.2.
- Plan 27 — GC switch; не зависит от этого плана.
