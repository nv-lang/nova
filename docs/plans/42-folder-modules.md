# Plan 42: Folder-modules (Go-style peers) — production-grade

> **Создан 2026-05-12, ревизия 2026-05-13** (Этап 95 + audit с чистого
> листа против Go/Rust production-практик).
>
> **СТАТУС 2026-05-14:** MVP + sub-plans **42.01-42.17 закрыты**.
> Sub-plans пронумерованы цифрами с leading zeros (`42.01`, …, `42.17`);
> старая буквенная нумерация «42.A/42.B» в тексте ниже — **legacy**,
> мигрирована (42.A → 42.10, 42.B → [Plan 45](45-nova-doc.md)). Финальный
> audit (Plan 42.17) закрыл doc drift, dead code, Rule H, слабые тесты.
>
> Реализует [D29 rev-2](../../spec/decisions/07-modules.md#d29-модули-и-импорты):
> модуль может быть либо single-file (`X.nv`), либо folder (`X/` с одним
> или несколькими `.nv` файлами как peers, share namespace).
>
> **Зависит от:** Plan 35 R31 (unified resolver), Plan 35 Ф.1 R3
> (cycle detection), Plan 35.A R26+R27 (selective import + prelude),
> Plan 36.A (FileId в Span — уже есть).
>
> **Backward-compat:** все существующие single-file модули продолжают
> работать без изменений. Folder-modules — opt-in новая capability.

---

## Зачем

Текущая single-file модель упирается в две проблемы при росте std/*:

1. **Internal helpers paint a corner.** Helper-function сейчас либо
   public (через `export`), либо запихнута в тот же файл с public
   кодом. Нет module-private visibility в multi-file смысле. Real
   production-grade библиотека (Rust `tokio`, Go `net/http`) имеет
   десятки internal helpers per module.

2. **Big modules не scale.** Ожидаемый рост std (Plan 18 P0:
   networking, crypto, codecs) — файлы 1500-3000 LOC. LLM context
   переполняется; facade-pattern boilerplate.

Folder-module (Go-style peers) закрывает оба.

---

## Архитектурное решение (финальное)

**Peers (Go-style)** + **module declaration format = `parent.X`**
(D29 rev-3). Все файлы папки объявляют **одинаковый** `module
<parent>.<X>` и share **declarations namespace** (но не imports —
см. правило C ниже). Никакого entry-marker.

```
src/admin/
├── users.nv          module src.admin       (peer; parent=src, target=admin)
├── audit.nv          module src.admin       (peer)
├── permissions.nv    module src.admin       (peer)
└── helpers.nv        module src.admin       (peer; internal, без export)
```

Compiler выводит folder-module из filesystem: папка `X/` с ≥1 `.nv`
файлом, где **все** файлы объявляют `module <parent>.<X>` (parent —
родитель папки `X/`, X — имя самой папки) и **share namespace
declarations** = folder-module.

**Sub-modules — только через nested folders**, не через peers с
точечным именем. `src/admin/billing/invoice.nv` объявляет `module
admin.billing` (parent=admin, target=billing) — независимый module
от `src.admin`. Чтобы использовать `Invoice` из `src/admin/users.nv` —
explicit `import admin.billing.{Invoice}` (через full path).

**Module declaration format — `parent.X`** (rev-3, 2026-05-13):

| Файл | parent | target | declaration |
|---|---|---|---|
| `src/main.nv` (single-file) | src | main | `module src.main` |
| `src/admin.nv` (single-file) | src | admin | `module src.admin` |
| `src/std/admin.nv` (single-file) | std | admin | `module std.admin` |
| `src/std/user/admin.nv` (single-file) | user | admin | `module user.admin` |
| `src/admin/users.nv` (peer of `admin/`) | src | admin | `module src.admin` |
| `src/std/encoding/hex.nv` (single-file) | encoding | hex | `module encoding.hex` |
| `src/std/encoding/json/parse.nv` (peer of `json/`) | encoding | json | `module encoding.json` |

- **target** = file basename (для single-file) или folder name (для
  folder-module peer).
- **parent_of_target** = имя directory **сразу над** target.
- Declaration **всегда 2 segments**, не зависит от глубины nesting.
- **Import** всегда использует full path: `import std.encoding.hex.{decode}`.
- Compiler maintains internal `decl ↔ canonical filesystem path`
  mapping; declaration — identity check (refactor safety), не
  routing key.

**Conflict resolution:** одновременное наличие `X.nv` (single-file) и
папки `X/` (folder-module) на одном уровне — compile error «ambiguous
module X». **Но:** `admin.nv` + `admin/billing/` (где `admin/` сама
не содержит direct `.nv` файлов, только sub-folder `billing/`) —
**валидно**: `admin/` тогда не module, а namespace-container для
sub-modules.

---

## Production-grade правила (A-L)

После production audit (2026-05-13) выявлено 12 gaps в первоначальном
плане. Все включены в production scope.

### A. Cycle detection по module-name, не file-path

Cycle = `module X` импортирует `module Y` который импортирует `module X`
(direct/transitive). Не importance is **module-level**, не **peer-file-
level**.

`compiler-codegen/src/imports.rs`: `in_progress: HashSet<Vec<String>>`
(module-path), не `HashSet<PathBuf>`. Diamond-deps через несколько
peers одного folder-module **не ложно срабатывают** как cycle.

`visited` остаётся `HashSet<Vec<String>>` (closed-set по module).
File-path canonical используется только для **dedup чтения файла**.

### B. Alphabetical filename ordering (deterministic build, R11)

Все peer-файлы folder-module **сортируются alphabetically** по
basename перед merge'ом в Module AST. Codegen эмитит C-функции в
этом порядке.

Гарантия: один и тот же source-tree → один и тот же `.c` output
(byte-identical). Это R11 / AD10 Plan 35 — без него incremental
cache (sub-plan 35.B) невалиден.

### C. Per-file imports remain per-file scope

В Go: `imports` block в каждом `.go` файле — видимы только в нём.
Это **good practice** — peer-файл `audit.nv` не «загрязнён» imports
из `users.nv`. Уменьшает noise.

В Nova folder-module: **каждый peer-файл имеет свои `import`
statements**, видимые **только в нём**. Peers share **declarations**
(functions, types, constants), но **не imports**.

Реализация: при merge'е AST'ов peers — `module.imports` остаётся
**per-peer**, нужен новый AST node `Module.peer_files: Vec<PeerFile>`
где `PeerFile { path, imports, items_originating_here }`. Resolver
рассматривает каждый PeerFile независимо для **resolving имён** в
его собственных expressions, но **declarations namespace shared**
между всеми PeerFile одного module.

### D. Cyclical references между peers — разрешены (2-pass codegen)

`users.nv` использует `fn audit::log` (через shared namespace,
без import). `audit.nv` использует `fn users::find` (тоже shared).
Это cycle на функции-уровне между peers **одного** module —
**разрешено** (Go разрешает, Rust разрешает в пределах one module).

**Требует 2-pass codegen** в `emit_c.rs` для folder-modules:
1. **Pass 1 (signatures):** emit forward declarations для **всех**
   functions/types из всех peers — без bodies.
2. **Pass 2 (bodies):** emit function bodies. Все cross-peer refs
   resolved через forward decls.

В bootstrap single-pass codegen этого нет. **Architectural change**
в `emit_module` для folder-modules. Single-file модули продолжают
работать single-pass (forward refs внутри одного файла уже
поддерживаются через TypeDecl ordering).

### E. `X.nv` + `X/` с только nested sub-folders — валидно

`admin.nv` single-file + `admin/billing/` папка (которая содержит
`.nv` файлы, но `admin/` сам не имеет direct `.nv`) — **валидно**.

В этом случае:
- `admin.nv` = module `admin` (single-file).
- `admin/billing/invoice.nv` = module `admin.billing` (peer of
  folder-module `admin.billing`).
- `admin/` сама не module — namespace-container.

**Conflict** только когда `X.nv` существует ИЛИ `X/` содержит direct
`.nv` files **который объявляет `module X`**. Этот case error.

### F. Test isolation: `_test.nv` convention

Peer-файл с suffix `_test.nv` (basename ends with `_test`) —
**test-only**: компилируется только при `nova test`, исключается из
`nova build` / `nova run`.

```
admin/
├── users.nv          module admin           (peer, production + test)
├── users_test.nv     module admin           (peer, test-only)
├── audit.nv          module admin           (peer)
└── helpers.nv        module admin           (peer)
```

`users_test.nv` имеет доступ к module-private items (видит helpers
без import), как обычный peer. Но **не попадает в release build**.

Aналог Go `*_test.go`. Better чем Rust `#[cfg(test)] mod tests { ... }`
(attribute), better чем inline `test "..."` блоки в production файле
(не отделено).

### G. ~~Module overview convention~~ — ОТВЕРГНУТО

Первоначально предлагался `overview.nv` peer с signatures-only
copy + compiler verification. **Отвергнуто 2026-05-13:** программист
будет забывать обновлять, overview будет отставать от реальности
— typical Go `doc.go` failure mode. Нет way to make это work без
дублирования signatures (которое мы избегаем — Nova не header/source).

**Что вместо:** `nova doc <module>` tooling (sub-plan 42.B) собирает
public API из всех peers автоматически из `export`-items + их
doc-comments. Source-of-truth = реализация, всегда актуально.

LLM получает API через `nova doc admin` (tool call), не через
файл-снимок который может отстать.

### H. `internal/` convention для library boundaries (Go-style)

Folder `<module>/internal/<sub>/` доступен только из `<module>/...`
descendants. Снаружи — compile error «cannot import internal module».

```
admin/
├── users.nv                module admin                (peer)
└── internal/
    └── token.nv            module admin.internal.token  (private)
```

`admin/users.nv` может `import admin.internal.token.{...}`.
`http/handler.nv` НЕ может — `internal` rule.

Critical для production library development: позволяет refactor
internal modules без breaking external API.

### I. Module-level effect / capability declarations

Один из peer-файлов folder-module может содержать `module`-level
declaration с capability constraints:

```nova
// admin/_module.nv (special peer; convention)
module admin

#forbid Net, Fs            // module-level capability sandbox
#requires Db Logger        // module-level effect requirements
```

Все peers folder-module наследуют эти constraints. Functions в
любом peer module-level получают `Db Logger` в effect-row автоматом,
не могут вызвать `Net`/`Fs` операции.

**Better than Go/Rust:** ни в одном из них нет module-level capability
declaration. Это **Nova-specific advantage**. Production-grade security
boundary.

**Sub-plan 42.10** ✅ ЗАКРЫТ 2026-05-14: `_module.nv` convention
реализован. Module-level `#forbid` пропагируется всеми peers через
`inherited_attrs` в `resolve_imports_inline_ex`. CapabilityCtx
применяет к compiled module. `#requires` НЕ реализован — отвергнут
(implicit effects in signatures противоречат D62). См.
[42.10-module-level-forbid.md](42.10-module-level-forbid.md).

### J. `nova doc <module>` — collect public API from all peers

Tooling (roadmap, не блокер): команда собирает все `export` items
из всех peers одного module → unified doc-output. AI-friendly:
LLM получает API за один запрос.

В Plan 42 — **резервируем design**: данные для tool'а доступны
через `Module.items` после merge'а. Сама команда — [Plan 45](45-nova-doc.md)
(вынесена в отдельный план, production tooling).

### K. Incremental rebuild — единица = whole folder-module

Когда `admin/users.nv` изменён в folder-module: rebuild **всего**
folder-module `admin`, не только peer. Причина: namespace shared,
любое имя в `users.nv` может быть referenced из `audit.nv`.

Cache key (для sub-plan 35.B): blake3 hash от **всех peer-файлов**
folder-module + transitive deps. Меняется любой peer → invalidates
весь module.

Это **проще** Rust (где cache invalidation per crate, но crate
может быть big) и **проще** Go (где каждый package — atomic unit
rebuild).

В Plan 42 — design hook'и для cache; реализация cache — sub-plan
35.B.

### L. Diagnostic quality для peer-файлов

При cross-peer ошибки (например `users.nv` ссылается на missing
`fn audit_log` в `audit.nv`):

```
error: cannot find function `audit_log` in module `admin`
   ┌─ admin/users.nv:42:5
   │
42 │     audit_log(event)
   │     ^^^^^^^^^ not found
   │
note: searched in peers of `module admin`:
   ┌─ admin/audit.nv (peer)
   ┌─ admin/permissions.nv (peer)
   ┌─ admin/helpers.nv (peer)
   │
note: did you mean `audit::log_event`? defined at:
   ┌─ admin/audit.nv:18:1
```

Используется FileId через Plan 36.A Span — error точно указывает
**module** + **peer-файл** + line. Lists all peers, suggests
similar names. **Better than Go** (Go показывает только filename),
**comparable to Rust** (Rust имеет good diagnostics через rustc).

---

## Сравнение с Go / Rust / Better

| Feature | Go | Rust | Plan 42 Nova |
|---|---|---|---|
| File-module | Нет (только folder=package) | `name.rs` или `name/mod.rs` | `name.nv` |
| Folder-module | Folder = package (peers) | `name/mod.rs` + submodules | Folder = module (peers) |
| Entry-marker | Нет | `mod.rs` или `lib.rs` | Нет (как Go) |
| Module declaration | `package name` в каждом файле | `mod name;` в parent | `module path.name` в каждом файле (Go-style) |
| Internal helpers | `lowerCase` convention | `pub` keyword granularity | без `export` keyword (uniform) |
| Sub-modules | Nested folders only | `mod foo;` + folder OR file | Nested folders only (как Go) |
| File ordering | Alphabetical | По `mod` declarations | Alphabetical (как Go) |
| Imports scope | Per-file | Per-file | Per-file (как Go/Rust) |
| Cycle detection | Per-package | Per-crate | **Per-module** ✅ |
| Test isolation | `_test.go` suffix | `#[cfg(test)]` | `_test.nv` suffix (как Go) |
| Internal protection | `internal/` directory | `pub(crate)`/`pub(super)` | `internal/` directory (как Go) |
| Module overview | `doc.go` convention | `lib.rs` top-level | `nova doc <module>` tooling (auto-collect из peers) — source-of-truth = реализация |
| Cross-peer cycles | OK | OK (within module) | OK (2-pass codegen) ✅ |
| Module-level effects | ❌ none | ❌ none | ✅ `#forbid`/`#requires` (Nova-unique) |
| Module-level capability | ❌ none | ❌ none | ✅ `#forbid Net` (Nova-unique) |
| `nova doc` from peers | `godoc` | `cargo doc` | `nova doc` (roadmap) |
| Incremental rebuild | Per-package | Per-crate | Per-module (finer than Rust crate) ✅ |
| Diagnostic quality | Filename | rustc-grade | rustc-grade + module-aware ✅ |

**Что Nova делает лучше Go/Rust:**

1. **Module-level capability** (`#forbid`, `#requires`) — first-class
   security/effect boundary at module level. Nova-unique.
2. **Cycle detection по module-name** — semantically правильнее чем
   per-file, не ложно срабатывает на diamond через peers.
3. **2-pass codegen для cross-peer cycles** — clean architecture, не
   ad-hoc forward declarations.
4. **Per-module incremental rebuild** — finer granularity чем Rust
   per-crate.

---

## Phases

### Ф.1 — Spec finalize ✅

- [x] D29 rev-2 в spec/decisions/07-modules.md (folder-module Go-style).
- [x] D29 rev-3 — module declaration format = `parent.X` (2026-05-13).
- [x] D29 «Почему» / «Что отвергнуто» / «Эволюция» дополнены.
- [ ] Update D78 (path enforcement) для folder-modules + parent.X
  rule. Manifest check для inconsistent `module` decls (Ф.3).

### Ф.1.5 — Migration std/* + nova_tests/* + examples под `parent.X`

Все существующие `module a.b.c.d` (full path) → `module c.d` (parent.X).
Tool/script: walk все `.nv` файлы в репо, для каждого вычислить
правильный `module parent.target` из filesystem path, заменить
declaration.

Также обновить **все imports** в этих файлах — imports остаются
full path (не меняются), но компилятор теперь связывает full import
path с modules через `(parent, target)` identity check.

Acceptance: после migration full regression PASS (261+/261).

### Ф.2 — Resolver: collect peers + per-file imports

`compiler-codegen/src/imports.rs`:

1. `resolve_import_path` расширен: для path `admin.users` пробуем:
   - **a.** `<base>/admin/users.nv` (single-file).
   - **b.** `<base>/admin/users/` folder с direct `.nv` files
     объявляющими `module admin.users` (folder-module).
   - **c.** Conflict (a) ∧ (b) → error.

2. Если folder-module: `collect_peer_files(folder) -> Vec<PathBuf>`:
   - read_dir, filter `*.nv`, **alphabetical sort** (правило B).
   - Exclude `*_test.nv` если не test mode (правило F).
   - Recursive skip: nested folders **не** peers.

3. Parse each peer → verify все объявляют **одинаковый** `module`
   matching folder path. Inconsistent → error (правило 2).

4. Merge AST'ы:
   - `Module.items` — concat of all peers (alphabetical order).
   - `Module.peer_files: Vec<PeerFile>` — новый field. Каждый
     PeerFile содержит свои `imports` + Span/FileId origin (правило C).
   - При name resolution внутри expression каждого peer'а используется
     **его** import set, не объединённый.

5. Cycle detection: `in_progress` хранит `Vec<String>` (module-name),
   visited тоже. canonical paths используются только для file-read
   dedup (правило A).

### Ф.3 — Manifest check (path enforcement D78 rev)

`compiler-codegen/src/manifest.rs::check_module_path` под **rev-3
правило `parent.X`**:

- Single-file `admin/users.nv` → expected `module admin.users`
  (parent=admin, target=users).
- Folder-module peer `admin/users/foo.nv` → expected `module
  admin.users` (parent=admin, target=users — фолдер).
- All peers в folder-module declare **identical** `module
  <parent>.<target>` (parent + folder name) → если разные, return
  Err с listing всех мисматчей + suggested fix.
- Conflict `X.nv` + `X/` (с direct `.nv` files) → return Err.
- Эдж: `admin.nv` + `admin/billing/foo.nv` (admin/ has no direct
  .nv) → OK (правило E). `admin.nv` имеет declaration по своей
  parent (например `src.admin`), `admin/billing/foo.nv` —
  `admin.billing`.
- File at workspace root (`main.nv` без parent folder в repo) —
  parent = «имя самого root folder» (берётся `nova.toml` parent
  directory имя, обычно `src` или `nova_tests`).

### Ф.4 — Codegen: 2-pass для folder-modules (правило D)

`compiler-codegen/src/codegen/emit_c.rs::emit_module`:

- Detect: `module.peer_files.len() > 1` → folder-module path.
- **Pass 1:** emit forward declarations для всех Type/Fn/Const
  items из всех peers. Type forward decl сейчас уже есть; нужно
  расширить для Fn (signature only, no body).
- **Pass 2:** emit function bodies. Все cross-peer refs resolved
  через Pass-1 forward decls.

Single-file модули (peer_files.len() == 1 или single-file marker)
продолжают single-pass — backward compat.

Risk: cross-peer cycle через **types** (`users.nv` имеет `type
UserCtx { audit AuditLog }` и `audit.nv` имеет `type AuditLog {
user UserCtx }`) — это **mutual recursion** на типах. Forward
decl типов уже работает (Plan 36 followup). Verify на test'е.

### Ф.5 — Tests (positive)

`nova_tests/modules/folder_*/`:

- **folder_basic/** — 3 peers (`users.nv`, `audit.nv`, `helpers.nv`).
  `nova_tests/modules/folder_basic_use.nv` импортирует и использует.
  Verify shared namespace, internal helper visibility, ordering.

- **folder_nested/** — `outer.nv` peer + `outer/inner.nv` (nested
  folder = sub-module). Verify independent modules.

- **folder_cross_peer_cycle/** — `a.nv` использует fn из `b.nv`, и
  `b.nv` использует fn из `a.nv`. Verify 2-pass codegen works.

- **folder_with_single_file/** — `admin.nv` + `admin/sub/foo.nv`
  (правило E). Verify both valid simultaneously.

- **folder_internal/** — folder с `internal/` sub-dir (правило H).
  External access to `module/internal/...` rejected.

- **folder_test_isolation/** — peer `users.nv` + `users_test.nv`.
  `nova test` запускает оба; `nova build` исключает `_test.nv`
  (правило F).

### Ф.6 — Tests (negative)

`nova_tests/negative_capability/folder_*.nv`:

- **folder_inconsistent_decl/** — peers объявляют разные `module`.
  EXPECT_COMPILE_ERROR.

- **folder_file_vs_folder_conflict/** — `X.nv` + `X/` с direct .nv.
  EXPECT_COMPILE_ERROR.

- **folder_internal_external_import/** — внешний module пытается
  `import other.internal.foo`. EXPECT_COMPILE_ERROR (правило H).

- **folder_cycle_between_modules/** — folder-module A imports B,
  B imports A. EXPECT_COMPILE_ERROR (правило A — cycle по module,
  не file).

### Ф.7 — Docs + project-creation/simplifications + commit

- README.md и README.ru.md — секция «What works today» mention
  folder-module support.
- compiler-codegen/README.md — раздел про folder-modules в
  «Cross-file resolve».
- spec/decisions/README.md — статус Plan 42 done.
- project-creation.txt + simplifications.md — implementation entry.
- discussion-log Этап для private repo.

### Ф.8 — Optional std/* migration (separate sub-plan)

Если первый std/* модуль вырастет >800 LOC — convertable example.
Не блокер для Plan 42 closure.

### Sub-plans (пронумерованы 42.1, 42.2, ...)

- **42.1 — file-level `#forbid`** ✅ ЗАКРЫТ 2026-05-13.
  Attribute `#forbid X, Y` на module-top (после `module X`
  declaration) applies к **этому файлу** (per-file scope, не
  cross-peer — peers равноправны, наследование между peers было бы
  inconsistent). Каждый peer объявляет свои constraints. Enforce'ится
  через type-checker `CapabilityCtx` (file-level initial frame
  в `forbidden_stack`). `#requires` отвергнут (нарушает AI-first
  explicit principle — implicit effects in function signatures).
  Tests: `modules/file_forbid_clean.nv` + `negative_capability/file_forbid_violation.nv` PASS.
- **42.2 — `nova doc <module>`** tooling — **вынесен в отдельный
  [Plan 45](45-nova-doc.md)** (2026-05-14). Auto-collect public API
  из всех peers, source-of-truth = реализация; заменяет отвергнутый
  `overview.nv` (правило G). Большой scope (~700-1000 LOC: lexer +
  parser + CLI subcommand + formatters), отдельная сессия.
- **42.3 — function-level `#forbid`** ❌ ОТВЕРГНУТО 2026-05-14.
  Изначально предлагался attribute `#forbid X, Y` перед `fn` как
  shortcut для `forbid X { body }`. **Отказ:** это TIMTOWTDI
  (two ways to do one thing) — дублирующий syntax поверх
  существующего `forbid X { body }` scope-block (D63). Nova
  philosophy «один способ для одной вещи» (AI-first consistency:
  LLM не должен выбирать между equivalent syntaxes). Convenience
  win минимален (один блок-wrap), стоимость — два keyword'а с
  идентичной семантикой. Тот же anti-pattern что 42.7 (см.
  simplifications.md). Если когда-нибудь fn-level scope станет
  настолько частым случаем что block wrap стал code-smell —
  пересмотреть; до тех пор `forbid X { body }` достаточен.
- **42.04 — per-file imports scope** ✅ ЗАКРЫТ 2026-05-14 — детальный
  план в [42.04-per-file-imports-scope.md](42.04-per-file-imports-scope.md).
  Шаги 1-3 + позитивный тест. Rule C частично enforced (Path-form не
  проверяется — см. [M10] в simplifications.md).
- **42.5 — 2-pass codegen** (правило D) ✅ **SATISFIED** (verified Plan
  42.17 audit). Правило D достигнуто **глобальным forward-decl pass**:
  `emit_c.rs` Pass 1 эмитит `emit_fn_forward_decl` для **всех**
  `module.items` (порядко-независимо), Pass 2 — bodies. Folder-module
  peers flat-merge'атся в `module.items`, поэтому cross-peer взаимная
  рекурсия резолвится автоматически. Тест `folder_mutual_recursion`
  (`even.nv`↔`odd.nv`) компилирует двунаправленную рекурсию. Отдельный
  folder-module-specific 2-pass **не нужен**.
- **42.6 — Migration std/* + nova_tests/* под parent.X** (D29 rev-3) ✅
  ЗАКРЫТ 2026-05-13. `scripts/migrate_modules_rev3.ps1` — automated
  walker: для каждого `.nv` файла computes expected `module parent.X`
  по filesystem path и переписывает legacy `module package.full.path`
  declaration. Применён к `std/` (package=`std`) и `nova_tests/`
  (package=`nova_tests`): **324 файла мигрированы**, 16 пропущено
  (folder-module peers — уже rev-3; single-file at source root — rev-3
  совпадает с rev-1). Hardcoded compat-checks (`is_stdlib_runtime_module`,
  `is_prelude_self_module`) — вынесены в `manifest.rs` как helpers,
  используются в `types/mod.rs::check_module` и `imports.rs::resolve_imports_inline_ex`.
  Compat mode остаётся (rev-1 declarations accepted) для backward-compat —
  не блокер удалять. Регрессия: **274 PASS / 0 FAIL / 3 SKIP** (3 z3 SKIP
  = свежие тесты Plan 33 V1, out of scope). examples/* не мигрированы —
  файлы там в произвольных форматах (часть `module hello`, часть rev-1),
  не enforced manifest check'ом (отдельная задача-cleanup).
- **42.7 — Cross-peer consistency lint** ❌ ОТВЕРГНУТО 2026-05-14.
  Изначально предлагался warning при разных `#forbid` между peers
  одного folder-module. **Отказ:** file-level `#forbid` *by design*
  per-peer (Sub-plan 42.1), peers равноправны, разные constraints —
  это **корректная** capability decomposition, не smell. Use-cases:
  один peer нуждается в `Net` (webhook), другой — `#forbid Net`; один
  пишет в log (нужен `Fs`), другие — `#forbid Fs`. Lint срабатывал бы
  на legitimate designs → false positives → noise или потеря
  выразительности. «Catch typos» аргумент тоже не валиден: парсер
  `#forbid` принимает имена capabilities из enum'а, invalid имя —
  compile error. Lint solved a phantom problem.

---

## Critical files

| Файл | Действие |
|---|---|
| `spec/decisions/07-modules.md` | D29 rev-2 ✅; D78 path enforcement update |
| `compiler-codegen/src/ast/mod.rs` | новый `Module.peer_files: Vec<PeerFile>` |
| `compiler-codegen/src/imports.rs` | resolve_import_path + collect_peer_files + per-file imports |
| `compiler-codegen/src/manifest.rs::check_module_path` | folder-module path enforcement |
| `compiler-codegen/src/codegen/emit_c.rs::emit_module` | 2-pass для folder-modules |
| `compiler-codegen/src/types/mod.rs` | name resolution respects per-file imports |
| `nova_tests/modules/folder_*/` | 7 positive test scenarios |
| `nova_tests/negative_capability/folder_*.nv` | 5 negative test scenarios |
| `README.md`, `README.ru.md`, `compiler-codegen/README.md` | mention folder-modules |

---

## Acceptance criteria (production-grade)

**Correctness (MVP):**

- `import admin` где `src/admin/` содержит 3 peer-файла → все 3
  merge'ятся в один module, namespace shared.
- Internal helper (без `export`) в одном peer виден из другого peer
  того же folder-module.
- Internal helper не виден извне (только через explicit `export`).
- Sub-module через nested folder (`admin/billing/`) — независимый
  module, требует `import admin.billing.{...}`.
- `X.nv` + `X/` с direct `.nv` → clear compile error.
- `X.nv` + `X/` где `X/` содержит только sub-folders → OK (правило E).
- Inconsistent `module` declarations в peers → clear compile error.
- Cross-peer cyclical refs работают (правило D, 2-pass codegen).
- Per-file imports respected — peer A не видит imports peer B (C).
- Alphabetical filename ordering → deterministic codegen output (B).
- Cycle detection по module-name, не file-path (A).
- Existing single-file модули work без изменений (full regression PASS).

**Production polish:**

- `_test.nv` peers compile в test mode only (F).
- `internal/` path protection (H).
- Diagnostic quality для cross-peer errors (L) — указывает module +
  peer-file + line + similar suggestions.

**Sub-plans (закрыты):**

- Module-level `#forbid` (I) — [Plan 42.10](42.10-module-level-forbid.md) ✅.
- `nova doc <module>` (J) — [Plan 45](45-nova-doc.md) (вынесен отдельно).

---

## Risks / Trade-offs

- **Module discovery cost (filesystem).** Resolver делает `read_dir`
  для folder-modules. Bootstrap std достаточно мал (~50 модулей);
  cache не нужен в MVP. Future: in-memory cache (sub-plan 35.B).

- **«Two ways to do one thing»** (file vs folder). Mitigated:
  convention «начинай с file, конвертируй в folder при >800 LOC».
  Не enforce'им.

- **2-pass codegen risk** (правило D). Forward decls для fn должны
  matchить body signature. Risk: bootstrap codegen не имеет
  infrastructure для fn-forward-decls (только type forward). Нужна
  новая phase в emit_module. ~100-200 LOC.

- **Test isolation regression** (правило F). Existing `test "..."`
  блоки в production файлах продолжают работать. `_test.nv`
  convention — additive. Verify в regression.

- **`internal/` adoption** (правило H). Library boundary enforcement
  через convention. Critical для library development, но bootstrap
  std пока не использует. Implement в MVP (низкая стоимость), use —
  opt-in.

---

## Что НЕ входит

- **`module.nv` entry-marker** — отвергнуто, лишний boilerplate.
- **Name-mirror entry** (`admin/admin.nv`) — отвергнуто, дублирование.
- **`mod.rs`-style** — отвергнуто, Rust сам уходит.
- **Sub-modules внутри folder через точки в `module` declaration** —
  отвергнуто, sub-modules только через nested folders.
- **Conditional compilation** (`cfg`) per peer — отдельный sub-plan 35.E.
- **`pub(crate)`/`pub(super)`** granularity — отвергнуто D5;
  `internal/` convention closes this need.
- **Module-level `#forbid`** (I) — [Plan 42.10](42.10-module-level-forbid.md)
  ✅ (`#requires` отвергнут — нарушает D62 explicit-effects).
- **`nova doc`** tooling (J) — [Plan 45](45-nova-doc.md).

---

## Estimate (revised)

**MVP (правила A, B, C, D, E, F):** ~400-600 LOC.
- AST: PeerFile struct (~30 LOC).
- imports.rs: collect_peer_files + per-file imports + cycle by
  module-name (~150 LOC).
- manifest.rs: path enforcement folder-aware (~50 LOC).
- emit_c.rs: 2-pass для folder-modules (~150 LOC).
- types/mod.rs: per-file import scope (~50 LOC).
- Tests: 6 positive + 4 negative (~250 LOC test code).

**Production polish (H/L):** +~130 LOC.
- internal/ path protection (~40 LOC).
- Diagnostic quality (~90 LOC через Plan 36.A FileId).

**Sub-plans 42.A/B:** отдельные планы, отдельные оценки.

**Sessions:** 2-3 сессии для MVP + production polish.

---

## Что улучшает vs Go/Rust (summary)

1. **Module-level capability/effect declarations** (правило I) —
   Nova-unique. Production-grade security boundary.
2. **Cycle detection по module-name** (правило A) — semantically
   правильнее.
3. **2-pass codegen для cross-peer cycles** (правило D) — clean
   architecture без ad-hoc forward decls.
4. **Per-module incremental rebuild** (правило K) — finer чем Rust
   per-crate.
5. **`internal/` + folder-module unified** — Go-style protection,
   Rust-grade integration с module system.
6. **Test isolation `_test.nv`** + folder-module peer scoping —
   как Go, но с capability inheritance из module-level constraints.

### Module overview (правило G) — отказались

В обсуждении 2026-05-13 решили отказаться от `overview.nv` convention:
программист будет забывать обновлять, overview будет отставать от
реальности (typical Go `doc.go` failure mode). Source-of-truth —
**реализация**; tooling `nova doc <module>` (sub-plan 42.B) auto-
collects API из всех peers. Никакого ручного дублирования.
