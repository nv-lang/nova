# Plan 42: Folder-modules (Go-style peers)

> **Создан 2026-05-12.** Реализует D29 rev-2: модуль может быть либо
> single-file (`X.nv`), либо folder (`X/` с одним или несколькими `.nv`
> файлами как peers, share namespace).
>
> **Зависит от:** Plan 35 R31 (unified resolver), Plan 35 Ф.1 R3
> (cycle detection), Plan 35.A R26+R27 (selective import + prelude).
> Все уже выполнены.
>
> **Backward-compat:** все существующие single-file модули продолжают
> работать без изменений. Folder-modules — opt-in новая capability.

---

## Зачем

Текущая single-file модель упирается в две проблемы при росте std/*:

1. **Internal helpers paint a corner.** Helper-function в bootstrap
   код-base сейчас либо public (через `export`), либо запихнута в тот
   же файл с public кодом. Нет способа сделать «module-private».
   Real production-grade библиотека (Rust `tokio`, Go `net/http`)
   имеет десятки internal helpers per module.

2. **Big modules не scale.** Ожидаемый рост std (Plan 18 P0:
   networking, crypto, codecs) даст файлы по 1500-3000 LOC. Один файл —
   плохо читается LLM (контекст переполняется); refactor через
   facade-pattern (`module.nv` + `export import`) — boilerplate.

Folder-module (Go-style peers) закрывает оба:
- Все файлы папки share namespace → internal helpers естественны.
- Большие модули разбиваются на тематические файлы (`users.nv`,
  `audit.nv`, `permissions.nv`) без facade boilerplate.

---

## Архитектурное решение

**Peers (Go-style).** Все файлы папки объявляют один и тот же `module X`,
share namespace. Нет специального entry-marker (`module.nv` / `mod.nv` /
`<name>.nv`).

```
admin/
├── users.nv          module admin
├── audit.nv          module admin
├── permissions.nv    module admin
└── _helpers.nv       module admin (internal — convention)
```

Compiler выводит folder-module из filesystem state: если папка `X/`
содержит ≥1 `.nv` файл, и все файлы объявляют `module ...` matching
path — это folder-module `X`.

**Sub-modules через nested folders.** `admin/billing/` (с файлами или
single `billing.nv` рядом с папкой — но не оба) — независимый module
`admin.billing`. Не «sub-module внутри admin».

**Conflict resolution:** одновременное наличие `X.nv` и папки `X/` на
одном уровне — compile error «ambiguous module X».

---

## Правила (spec D29-rev)

1. **Модуль = file `X.nv` ИЛИ folder `X/` с ≥1 `.nv` файлом.**
2. **Все файлы в folder-module объявляют одинаковый `module <full-path>`.**
   Если разные — compile error.
3. **`module` declaration matches folder path** (Go-style precedent).
   Файл в `src/admin/users.nv` (peer of folder-module `admin`)
   объявляет `module admin`. Файл в `src/admin/billing/invoice.nv`
   (peer of `admin.billing`) объявляет `module admin.billing`.
4. **Visibility:**
   - `export` — public наружу module.
   - Без `export` — module-private (видно из **всех peers** того же
     folder-module).
5. **Sub-modules** — через nested folders. Внутри одного folder-module
   нет вложенности через точки.
6. **Conflict** `X.nv` + `X/` на одном level — compile error.
7. **Backward-compat:** существующие single-file модули остаются
   валидными. Никаких миграций.

---

## Phases

### Ф.1 — Spec finalize

- [x] D29 rev-2 в spec/decisions/07-modules.md.
- [x] D29 «Почему» / «Что отвергнуто» / «Эволюция» дополнены.
- [ ] Update D78 (path enforcement) для folder-modules: проверить что
  все файлы folder объявляют одинаковое `module X` matches folder
  path. Update в `compiler-codegen/src/manifest.rs::check_module_path`.

### Ф.2 — Resolver: collect peers

`compiler-codegen/src/imports.rs`:
- При resolve `import admin` resolver проверяет:
  1. Файл `admin.nv` (single-file) — как сейчас.
  2. ИЛИ папка `admin/` с `.nv` файлами — **новый path**.
- Если папка: collect все `.nv` файлы → parse → verify все объявляют
  одинаковый `module admin` → merge AST'ы в один Module.
- Conflict (file + folder) → error.

### Ф.3 — Manifest check

`compiler-codegen/src/manifest.rs::check_module_path`:
- Update для folder-modules: разрешить `module admin` файл в
  `src/admin/*.nv` (раньше требовалось `module admin.<basename>`).
- Detect и reject inconsistent `module` declarations внутри folder.

### Ф.4 — Tests

`nova_tests/modules/folder_module_*.nv`:
- `folder_module_basic.nv` + helper folder `folder_module_basic_lib/`
  (с 3 файлами peers, share namespace, internal helper).
- `folder_module_internal_helper.nv` — verify что internal (без export)
  виден из peer но не извне.
- `folder_module_nested.nv` — verify sub-modules через nested folders.

Negative tests:
- `negative_capability/folder_module_inconsistent_decl.nv` — peers
  объявляют разные `module` → compile error.
- `negative_capability/folder_module_file_vs_folder_conflict.nv` —
  `X.nv` + `X/` на одном уровне → compile error.

### Ф.5 — Docs

- D29 update done в Ф.1.
- README sections обновить (если упоминают «single-file = module»).
- project-creation.txt + simplifications.md entries.

### Ф.6 — Optional: std/* migration

Если bootstrap std module начнёт упираться в single-file — convertable
example (например `std/encoding/json.nv` → `std/encoding/json/`).
**Не блокер для closure Plan 42.** Migration — opt-in, не обязательно.

---

## Critical files

| Файл | Действие |
|---|---|
| `spec/decisions/07-modules.md` | D29 rev-2 ✅ |
| `compiler-codegen/src/imports.rs::resolve_one` + `resolve_import_path` | Расширить для folder-modules |
| `compiler-codegen/src/manifest.rs::check_module_path` | Update path matching |
| `nova_tests/modules/folder_module_*.nv` | Positive tests (3) |
| `nova_tests/negative_capability/folder_module_*.nv` | Negative tests (2) |
| `README.md`, `README.ru.md` | Mention folder-module если уместно |

---

## Acceptance criteria

- `import admin` где `src/admin/` содержит 3 peer-файла → все 3
  merge'ятся в один module, namespace shared.
- Internal helper (без `export`) в одном peer виден из другого peer
  того же folder-module.
- Internal helper не виден извне (только через explicit `export`).
- Sub-module через nested folder (`admin/billing/`) — независимый
  module `admin.billing`, требует `import admin.billing.{...}`.
- Conflict `admin.nv` + `admin/` → clear compile error.
- Inconsistent `module` declarations в peers → clear compile error.
- Existing single-file модули work без изменений (regression 261+/261).

---

## Risks / Trade-offs

- **Module discovery cost.** Resolver теперь должен `read_dir` для
  folder-modules. Bootstrap MVP: filesystem cache не нужен; bootstrap
  std достаточно мал. Future: in-memory cache (sub-plan 35.B).
- **«Two ways to do one thing»** (file vs folder). Mitigated convention:
  «начинай с file, конвертируй в folder при >800 LOC». Не enforce'им
  — программист решает.
- **AI «куда смотреть first».** Mitigation: convention — называй
  файлы по logical concept (`users.nv`, `audit.nv`); `_*.nv` для
  internal. LLM навигирует через filename.

---

## Что НЕ входит

- **`module.nv` entry-marker** — отвергнуто, лишний boilerplate.
- **Name-mirror entry** (`admin/admin.nv`) — отвергнуто, дублирование.
- **`mod.rs`-style** — отвергнуто, Rust сам уходит.
- **Sub-modules внутри folder через точки в `module` declaration** —
  отвергнуто, sub-modules только через nested folders.
- **Conditional compilation** (`cfg`) per peer — отдельный sub-plan 35.E.
- **Per-file visibility внутри folder-module** — отвергнуто. Visibility
  binary (`export` или module-private).

---

## Estimate

~150-250 LOC (resolver + manifest changes + tests). Single session
работы (включая spec finalize и docs).
