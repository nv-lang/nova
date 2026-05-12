// SPDX-License-Identifier: MIT OR Apache-2.0
# План 35: Cross-file resolve

> **Статус v2:** план, не начат. Высокий приоритет (блокер для multi-file
> stdlib через codegen, например `std/encoding/hex.nv` → `import std.collections.range`).
> **Создан:** 2026-05-12. **Обновлён:** 2026-05-12 (v2: 3-way audit
> vs Rust + Go + Nova-specific = 65 gaps закрыто, MVP boundary declared,
> sub-plans 35.A-E).
> **Выделен из:** [Plan 14](14-stdlib-codegen-gaps.md) (Ф.5), при закрытии того плана 2026-05-12.

---

## Зачем

Сейчас в репо два частично работающих механизма:

1. **type-check cross-file** (`nova check`) — **partial**: types и static methods
   через PascalCase ident резолвятся cross-file (см. таблицу ниже), но
   bare-name `fn_name()` и wildcard `import X.Y.*` не работают.
2. **codegen cross-file** (`nova build`) — **не работает вообще**. При
   `import std.collections.range` codegen игнорирует Import items, тип
   `Range` не зарегистрирован, fallback → `for-in: unsupported iterator
   type 'nova_int'`.

**Real-world blockers (2026-05-12):**
- `std/encoding/hex.nv` использует `(0..s.len).step_by(2)` — компиляция
  падает на cross-file Range.
- `std/crypto/{hmac,jwt,sha256,bcrypt}.nv` зависят от `WriteBuffer` —
  работают **только** через `ExternalRegistry` `include_str!` hack для
  `std/runtime/{string_builder,write_buffer,read_buffer,char}.nv`.
  Все остальные cross-module зависимости (15+ файлов) **не компилируются**.
- `std/collections/{lru,priority_queue,deque}.nv` зависят от `HashMap`.
- `std/encoding/{csv,toml}.nv` зависят от `HashMap`.

**Что уже сделано** (commit `e019a47128`, Plan 36 followup):
- `record_schemas.contains_key("Range")` gate для emit и infer — позволяет
  `(0..N).step_by(K)` работать **same-file** (когда Range объявлен в том
  же файле). Cross-file — этот план.

---

## Текущее состояние (verified 2026-05-12)

| Случай | `nova check` | `nova build` |
|---|---|---|
| Cross-file типы (`Timestamp` из другого модуля) | ✅ резолвятся без import | ❌ |
| Cross-file static methods (`Timestamp.from_unix_millis`) | ✅ резолвятся без import | ❌ |
| Cross-file `import X.Y as alias` + `alias.fn()` | ✅ работает | ❌ |
| Cross-file bare-name `fn_name()` без import | ❌ "undefined identifier" | ❌ |
| Wildcard `import X.Y.*` | ❌ парсер не принимает `*` | ❌ |
| `import X.Y` без alias + bare `fn_name()` | ❌ не открывает bare names | ❌ |
| Cross-file Range / iterator types в for-in | ❌ method_overloads empty | ❌ |
| `std/runtime/*.nv` opaque types | ✅ через ExternalRegistry | ✅ через ExternalRegistry |

**Что есть в текущем коде:**
- `Module { name, imports: Vec<Import>, items, span }` — imports парсятся.
- `Import { path: Vec<String>, alias: Option<String>, span }`.
- `CEmitter::emit_module` **игнорирует** `module.imports` (silent skip).
- `ExternalRegistry` (`compiler-codegen/src/codegen/external_registry.rs`)
  — pattern для load + register cross-file types/methods via `include_str!`.
  Используется только для 4 runtime opaque types.

---

## Архитектурное решение (resolved)

### AD1. Гранулярность: **module = file** (preserve D78)

[G02] поднимал «directory = package» (Go). **Решение:** оставляем
D78 file-based. Causes:
- Spec уже зафиксирован D78 path-enforcement (07-modules.md).
- Преимущество directory-as-package (single namespace) не оправдывает
  breaking change в spec.
- Wildcard `import X.Y.*` (Ф.1) даёт похожий UX без structural change.

Но: добавить D-решение «Q-module-granularity» в spec/open-questions.md
с explicit "rejected for v1.0; revisit after stdlib growth".

### AD2. Loader unified с ExternalRegistry [N03]

Critical — иначе **два независимых path** для cross-file:
`include_str!` embedded vs filesystem. **Решение:** new `trait ModuleLoader`:

```rust
pub trait ModuleLoader {
    fn load(&self, path: &[String]) -> Result<Module, LoadError>;
}

pub struct EmbeddedLoader;     // std/runtime/* через include_str!
pub struct FilesystemLoader { roots: Vec<PathBuf> }
pub struct CompositeLoader { loaders: Vec<Box<dyn ModuleLoader>> }
```

`ExternalRegistry::merge_from_module` re-use'ится для **both** sources.
Один codepath: parse → typecheck → register types/methods → optionally emit.

### AD3. Phase separation: signature-pass + body-pass [R5]

Critical bug-risk (Rust signature/body split). **Решение:** 2-pass
typecheck:

1. **Pass 1 (metadata extraction):** обойти все loaded modules, собрать
   только **signatures** (type decls, fn signatures без body). Записать
   в `ModuleEnv`. Позволяет **mutual recursion across modules**.
2. **Pass 2 (body typecheck):** обойти fn bodies, expressions, statements.

В bootstrap текущий `check_module` делает single-pass. Эта split — большая
работа, но **обязательна** для корректности mutual-recursive cross-file.

Cycle detection [G01, R6]: detect during signature-pass. Error format:
```
error[N0501]: import cycle detected
   --> std/encoding/hex.nv:3:1
    |
  3 | import std.collections.range
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
note: cycle: hex → range → utils → hex
```

### AD4. Cache layers [G03, R4, N24]

- **In-memory** (mandatory MVP): `HashMap<canonical_path, Arc<Module>>`
  shared across single `nova build`. Без него diamond-deps O(N²) parse.
- **On-disk** (sub-plan 35.B): content-hash key, persisted в
  `$NOVA_TARGET_DIR/check-cache/modules/<hash>.rmeta`. Аналог Rust
  `rmeta`. Includes typecheck result, not full AST.
- **Invalidation** [N04]: cache key = `blake3(content + nova_version +
  flags)`. Изменение `std/runtime/*.nv` через regen-runtime → automatic
  cache miss.

### AD5. Symbol mangling [R8, R16]

[R8] критичный: текущий план `<module>__<fn>` имеет collisions с C
reserved identifiers (`__`-prefix). **Решение:** двухсимвольный
separator + length-prefixed paths:

```
nova_fn_3std10collections5range7step_by
       ^   ^    ^           ^    ^
       len name len            ^    name
```

Аналог Rust v0 mangling (упрощённый). Deterministic, no collisions,
demangler-friendly. Полная spec — sub-plan 35.D.

**MVP shortcut:** `nova_fn_std_collections_range__step_by` (через
underscore separator). Если type name содержит `_` — error "module
path may not contain consecutive underscores or double-underscore".
Зафиксировать как known limit в MVP.

### AD6. Cross-file diagnostic spans [N11]

Critical для DX. **Решение:** добавить `file_id: u32` в `Span`.
`SourceMap { files: Vec<FileEntry>, ... }` — registry всех файлов
текущей compilation unit. Diagnostic render берёт source через
`source_map.get(span.file_id)`.

Span chain "required from here":
```rust
pub struct Diagnostic {
    pub primary_span: Span,
    pub message: String,
    pub notes: Vec<(Span, String)>,  // "required from here" chain
    ...
}
```

Это **breaking change** в `diag::Diagnostic` API (~50 call sites).
Объединить с Plan 36 sub-plan 36.A (Diagnostic extension).

### AD7. Effect/capability propagation [N05, N06]

Critical для soundness. Cross-file fn calls должны видеть **effects**:

- `effect_schemas` merge через `register_imported_module`.
- Capability checker (Plan 16) ходит по call graph — для cross-file
  callees берёт `effects` field из imported `FnDecl`.
- `// ALLOC_REQUIRES boehm` marker propagation: если любой imported
  module имеет `ALLOC_REQUIRES boehm`, текущий модуль тоже tied к
  boehm. CLI должен fail если `--gc malloc` + transitive
  `ALLOC_REQUIRES boehm`.

### AD8. Path canonicalization [G10]

Windows: case-fold + symlink resolve. `std::fs::canonicalize` plus
case-fold на Windows через explicit `to_lowercase()` для key
(но preserve original casing для diagnostic).

### AD9. Visibility enforcement (export check) [N22]

Cross-file resolver видит **только `export`-items**. Не-export items
filtered при loading. Это **закрывает spec violation** где current
type-check magic ignored `is_export`.

### AD10. Build determinism [G08]

Cross-file emit обязан produce byte-identical `.c` для same inputs:
- Sort imports by path lexically before resolve.
- Sort registered types/methods by name within each module.
- Stable iteration order для `record_schemas`, `method_overloads`.

Acceptance: `nova build foo.nv` → `nova build foo.nv` → diff = 0.

### AD11. Re-export / `export use` [R1, G06]

Critical для library design. **Решение:**

```nova
// std/prelude.nv
export use std.collections.HashMap
export use std.collections.range.Range
```

`export use X.Y` — re-exports `Y` под текущим module path. Implementation:
register imported items в текущий module's exported set, без duplicating
emit. **MVP:** не реализуем, документируем как `Q-re-export-syntax`.
Sub-plan 35.A.

### AD12. Prelude concept [R18]

Sufficient consistency: либо explicit prelude module (Rust-style
`std::prelude::v1`), либо documented "bare names of std types are
always visible" (current de-facto). **Решение:** explicit prelude
file `std/prelude.nv`, auto-imported в каждый module. Sub-plan 35.A.

---

## Requirements (R1-R30) — production-grade

Each tagged: **[MVP]**, **[35.A-E]**, или **[deferred R30]**.

### Core resolution (MVP)

**R1. [MVP] File-system loader.** `import std.X.Y` → `<root>/std/X/Y.nv`.
Root = `<repo>/std/` (по nova.toml walks-parents). Multiple roots:
`std/` + project-local имеет ли import? **MVP:** single root `std/`,
plus same-package imports (relative `<package_root>/X/Y.nv`).

**R2. [MVP] In-memory module cache** (AD4 layer 1). `HashMap<PathBuf,
Arc<Module>>` shared. Diamond-dep deduplication.

**R3. [MVP] Topological dependency walk + cycle detection** (AD3, G01).
Cycle = error with deterministic edge-path message. No infinite loop.

**R4. [MVP] Module-name uniqueness** (N16). Two files declaring same
`module X.Y.Z` (e.g. `std/X/Y/Z.nv` + `vendor/X/Y/Z.nv`) → error
"duplicate module declaration".

**R5. [MVP] D82 `external fn` propagation** (N01). Cross-file
`external fn` registers C-runtime symbol mapping (`nova_rt/X.h`),
NO body emission. Аналогично текущему ExternalRegistry path.

**R6. [MVP] Visibility filter** (AD9, N22). Cross-file resolver видит
только `is_export=true` items. Non-exported items — filtered out.

**R7. [MVP] Missing import error** (N19). `import X.Y` без файла →
`error[N0502]: cannot find module 'X.Y'; searched: <path>`.

### Output integrity (MVP)

**R8. [MVP] Symbol mangling** (AD5, R8). `nova_fn_<module_path>__<name>`.
Collision error на consecutive underscores.

**R9. [MVP] Cross-file types в forward decls** (N25, Plan 36 fix). Pre-pass
`user_type_fwd_decls` (commit e019a47128) обязан включать cross-file
imported types. Иначе NovaOpt typedef'ы для cross-file types падают.

**R10. [MVP] Schemas merge order** (N15). Imported types регистрируются
ДО emit_preamble (where forward decl marker лежит). `register_imported_module`
вызывается между ExternalRegistry init и emit_preamble.

**R11. [MVP] Build determinism** (AD10). Byte-identical `.c` output для
same inputs. Acceptance test.

### Soundness (MVP)

**R12. [MVP] Sum-type schemas merge** (N09). `sum_schemas` +
`record_variant_field_order` merged from imported modules. Without —
cross-file pattern match на foreign Result/Option variants fails.

**R13. [MVP] Effect schemas merge** (N10). `effect_schemas` merged.
User-defined effects cross-file работают.

**R14. [MVP] Effect propagation в capability check** (AD7, N05). Plan
16 capability check видит effects транзитивно. Без этого soundness hole
(forbid(IO) пропускает cross-file IO call).

**R15. [MVP] ALLOC_REQUIRES transitive** (N06). Plan 27 marker
propagates через imports. CLI fail если transitive requires boehm + `--gc malloc`.

**R16. [MVP] `Self` binding preservation** (N17). При emit'е cross-file
method body, `Self` остаётся bound к receiver-type из import'ируемого
модуля, не к локальному context.

### Diagnostics (MVP, depend on Plan 36.A)

**R17. [MVP-blocked-on-36.A] `FileId` в Span** (AD6, N11). `Span` имеет
`file_id: u32`. Diagnostic chain "required from here". Объединить с
Plan 36 sub-plan 36.A (Diagnostic extension).

**R18. [MVP] Module path conflict diagnostic** (N16). Точный error при
collision: какие два файла объявляют один module.

### Tooling (MVP)

**R19. [MVP] `nova check` cross-file file-existence validation** (N20).
Если `nova build` fail'нет на missing import, `nova check` должен fail'нуть
тоже. Currently `nova check` валидирует только текущий файл — это
divergence.

**R20. [MVP] Path canonicalization** (AD8, G10). Windows case-fold +
symlink resolve для cache key. Без этого `std\Collections\HashMap.nv`
и `std\collections\hashmap.nv` — два cache entries → linker collisions.

**R21. [MVP] Dead-code elimination flag** (N18). По умолчанию emit'ить
**только используемые** cross-file items (tree-shaking). Без этого
`import std.collections.range` тянет 20+ методов в `.c`.

### Build infrastructure [35.B]

**R22. [35.B] On-disk module cache** (AD4 layer 2, R4). `.rmeta`-like
files в `$NOVA_TARGET_DIR/check-cache/modules/`. Content-hash invalidation.

**R23. [35.B] Incremental rebuild** (N24). Если только один файл изменён,
rebuild только его + transitive dependents.

**R24. [35.B] Memory cache invalidation для regen-runtime** (N04). Когда
`std/runtime/*.nv` regenerates, drop relevant cache entries.

**R31. [35.B-prereq] Unified compilation pipeline** (HIGH PRIORITY).
Сейчас Nova имеет **3 независимых compile codepath'а**:
- `cmd_check` (nova-cli/src/main.rs) — parse + types + lints.
- `cmd_build` (nova-cli/src/main.rs) — parse + types + codegen + cc → exe.
  **+ resolve_imports_inline** (Plan 35 Ф.1 MVP).
- `test_runner::codegen_to_c` (compiler-codegen/src/test_runner.rs) —
  parse + types + codegen + cc → exe + run tests. **БЕЗ
  resolve_imports_inline.**

**Это архитектурный debt:** каждый codepath эволюционировал отдельно
(Plan 24/28). Cross-file resolve добавлен **только в один**. Аналог в
Go/Rust: единый pipeline (`go tool compile`, `rustc`) — все команды
(`go test`/`go build`/`cargo test`/`cargo check`) идут через него.

**Решение:** extract в shared lib `compiler-codegen`:
- `pub fn resolve_imports_inline(entry_path, &mut module, repo, stdlib_dir) -> Result<()>`
  — перенести из `nova-cli/src/main.rs`.
- Вызывать из всех трёх codepath'ов (`cmd_check`, `cmd_build`,
  `test_runner::codegen_to_c`).
- Future R22 on-disk cache естественно подключается через единый entry.

**Real-world impact:** `nova test foo.nv` с `import std.X.Y` сейчас
падает («cannot resolve iterator type 'nova_int'»). После R31 —
работает identical с `nova build foo.nv`.

**Estimate:** ~80-120 LOC (extract + 3 call site updates).
**Приоритет:** P1 — блокер для stdlib testing.

### Spec / DX [35.A]

**R25. [35.A] Wildcard `import X.Y.*`** (parser + resolver). Bare-name
visibility всех `export` items.

**R26. [35.A] `export use` re-export** (AD11, R1, G06).

**R27. [35.A] Prelude module** (AD12, R18). `std/prelude.nv`
auto-imported.

**R28. [35.A] `pub`/`pub(crate)`/`pub(super)` granularity** (R2). Или
explicit decision "no — `export` is binary, `internal/` convention enough".

### Advanced / future [35.C-E + R30]

**R29. [35.C] Cross-file generic bounds** (N08, R15-Rust). `[T Hashable]`
где Hashable cross-file. Monomorphization stays per-module (Plan 14
followup).

**R30. [deferred]** Hot-reload (watch-mode), `internal/` convention,
conditional compilation `#[cfg(...)]`, edition system, orphan rules
(если traits добавятся), `extern "C"` no-mangle escape hatch, build
constraints (`//go:build linux`), error recovery (продолжить typecheck
после ошибок).

---

## MVP boundary (declared explicitly)

**MVP = Ф.0 + Ф.1 only.**

- **Ф.0 prerequisite:** `FileId` в Span + Diagnostic chain (R17).
  Объединить с Plan 36 sub-plan 36.A (Diagnostic extension). Без
  этого cross-file diagnostics неинформативны.
- **Ф.1:** Core resolution (R1-R21). Single-root filesystem loader,
  in-memory cache, signature/body 2-pass, cycle detection, schema
  merge, effect/capability propagation, ALLOC_REQUIRES transitive,
  mangling, deterministic output, DCE.

**MVP scope:** ~1000-1500 строк в `compiler-codegen/src/codegen/`,
~200 строк в `nova-cli/src/main.rs` (cmd_build extension). **Многосессионная**
работа.

**Acceptance MVP:**
- `nova build std/encoding/hex.nv` → exe (закрывает blocker).
- `nova_tests/modules/import_range.nv` — explicit cross-file test.
- `nova_tests/modules/import_cycle.nv` — cycle detection negative test.
- `nova_tests/modules/import_diamond.nv` — diamond-dep deduplication.
- `nova_tests/modules/import_mutual_recursion.nv` — mutual fns across
  modules (R5/AD3).
- Existing 199/200 nova_tests PASS без regression.
- `nova build foo.nv` × 2 → identical `.c` byte output (R11/AD10).

### Post-MVP sub-plans

| Sub-plan | Содержание | Зависимости |
|---|---|---|
| **35.A** | Wildcard imports (R25), `export use` re-export (R26), prelude (R27), visibility granularity (R28). Spec-heavy: новые D-decisions для import semantics. | После Q-import-semantics в spec |
| **35.B** | On-disk module cache (R22), incremental rebuild (R23), regen-runtime invalidation (R24). Performance, не correctness. | После MVP |
| **35.C** | Cross-file generic bounds resolution (R29). Аналог Rust trait coherence (для future protocols). | Plan 14 + Plan 15 |
| **35.D** | Stable mangling spec (R8 full v0-style). Aналог Rust v0. | Когда FFI use-cases появятся |
| **35.E** | Conditional compilation (`#[cfg(...)]`), `internal/` convention, edition system, hot-reload. Roadmap-level. | v1.0+ |

---

## Phases — MVP

### Ф.0 — Diagnostic + Span FileId prerequisite

Совмещается с Plan 36 sub-plan 36.A. Кратко:
1. `Span { start: u32, end: u32, file_id: u32 }` (add `file_id`).
2. `SourceMap` registry — все файлы текущей compilation unit.
3. `Diagnostic.notes: Vec<(Span, String)>` для chain.
4. Migration 50+ call sites — file_id = 0 default (single-file legacy).

**Acceptance:** existing single-file diagnostics не сломаны. Cross-file
diagnostic в Ф.1 эмитит chain.

### Ф.1 — Cross-file resolver MVP

**1.1 Module loader (AD2).**
- `trait ModuleLoader` + `EmbeddedLoader` (для std/runtime/*) +
  `FilesystemLoader { roots: Vec<PathBuf> }`.
- `find_repo_root()` + `paths.stdlib_dir` в `cmd_build` — pass'ить в
  loader.
- Canonical path resolve (R20, G10).

**1.2 Dependency walk (R3, G01).**
- Recursive load `module.imports`. Track via `visited: HashSet<canonical_path>`.
- Topological sort via DFS post-order.
- Cycle detection: parallel `path: Vec<canonical_path>` — если current
  path appears in path stack, emit cycle error.

**1.3 In-memory cache (R2).**
- `HashMap<PathBuf, Arc<Module>>` в `cmd_build`. Passed to loader.

**1.4 Signature-pass + body-pass split (AD3).**
- Phase 1: walk all loaded modules, collect type decls + fn signatures
  (no bodies). Build `ModuleEnv` per module.
- Phase 2: typecheck bodies with cross-module env access.

**1.5 `CEmitter::register_imported_module` (AD2, R10, R12-R16).**
- Merge `record_schemas`, `sum_schemas`, `effect_schemas`,
  `method_overloads`, `method_receivers`, `from_targets`, `into_targets`,
  `iter_returns`, `record_variant_field_order`, `tuple_element_types`.
- Visibility filter (R6, AD9): только `is_export=true`.
- Module path conflict detection (R4, R18).
- `Self` binding tag (R16): record `originating_module: String` для
  каждого method, restore при emit body.

**1.6 Emit ordering (R9, R10).**
- `register_imported_module` calls **до** `emit_preamble` (so forward
  decls include cross-file types).
- Cross-file types регистрируются в `user_type_fwd_decls` (Plan 36
  pre-pass).

**1.7 Symbol mangling (R8/AD5).**
- `mangle_fn(module_path: &[String], fn_name: &str, params: &[Type]) -> String`.
- MVP shortcut: `nova_fn_<dotted_module_with_underscores>__<fn_name>__<param_mangle>`.
- Error при consecutive underscore в module path.

**1.8 Dead-code elimination (R21).**
- Build call graph from main module via BFS reachability.
- Emit только reachable cross-file fns. Types — always (cheap, no body).

**1.9 Effect propagation (R14, AD7).**
- Capability checker (Plan 16) walks call graph через imported modules.
- `effects` field of imported `FnDecl` accessible.

**1.10 ALLOC_REQUIRES transitive (R15).**
- Parse marker comments из each imported module (re-use
  `parse_alloc_constraint` from test_runner).
- Aggregate via UNION. If any imported module requires Boehm and CLI
  `--gc malloc` → error.

**1.11 Build determinism (R11/AD10).**
- Sort imports lexically before processing.
- Sort registered items by name within each module's registration call.

**1.12 `nova check` parity (R19).**
- `cmd_check` идёт через тот же resolver. Cross-file file-existence
  validated.

### Ф.2 — Tests

`nova_tests/modules/` (новая директория):

1. **import_range.nv** — basic single-import (Range from std/collections/range).
2. **import_diamond.nv** — A imports B and C, both import D. D parsed once.
3. **import_cycle.nv** — A→B→A. Negative test, expects error.
4. **import_mutual_recursion.nv** — fn `f` in A calls fn `g` in B,
   `g` calls `f`. Both type-check via signature-pass.
5. **import_collision_module.nv** — two files declare same module. Error.
6. **import_missing.nv** — `import std.nonexistent`. Error.
7. **import_visibility.nv** — import non-exported item. Error
   "X not exported by module Y".
8. **import_external_fn.nv** — D82 external fn cross-file resolves
   to nova_rt symbol.
9. **import_effects.nv** — effect/capability propagation через imports.
10. **import_alloc_transitive.nv** — ALLOC_REQUIRES transitive.
11. **build_determinism.nv** — same input → byte-identical .c.

### Ф.3 — Documentation

- `docs/spec/decisions/07-modules.md` D78 amend — cross-file rules.
- New `D-block`: import-semantics (Q-import-semantics close).
- `compiler-codegen/README.md` — remove limitation #2.
- `nova_tests/modules/README.md` — explains test scenarios.

---

## Files (estimate)

- `compiler-codegen/src/codegen/module_loader.rs` (new) — `trait ModuleLoader`
  + impls (~200 LOC).
- `compiler-codegen/src/codegen/emit_c.rs`:
  - `register_imported_module` method (~150 LOC).
  - `emit_module_with_imports` entry point (~50 LOC).
  - Mangling helpers (~50 LOC).
- `compiler-codegen/src/types/mod.rs`:
  - Signature-pass extraction (~200 LOC).
  - Body-pass refactor (~100 LOC).
- `compiler-codegen/src/diag.rs`:
  - `FileId` field в Span (Ф.0, ~30 LOC + 50+ migration sites).
  - `SourceMap` registry (~80 LOC).
- `nova-cli/src/main.rs`:
  - `cmd_build` import resolution loop (~80 LOC).
  - `cmd_check` parity (~30 LOC).
- `nova_tests/modules/` — 11 tests (~400 LOC).

**Total estimate:** ~1500 LOC + ~50 migration touches.

---

## Acceptance criteria — MVP

### Functional
- `nova build std/encoding/hex.nv` → exe (был blocker).
- All 11 tests в `nova_tests/modules/` PASS.
- Existing 199/200 nova_tests PASS — no regression.
- `nova check` cross-file file-existence parity с `nova build`.

### Quality
- Build determinism: `nova build foo.nv` × 2 → byte-identical `.c`.
- Cycle detection: deterministic error message with full edge path.
- Missing import: clear error с searched paths.
- Module collision: clear error with both file locations.
- Visibility: clear error при non-exported access.

### Performance
- Diamond-dep: транзитивный module parsed **once** (in-memory cache).
- `nova build` со средним stdlib chain (~5 imports) < 3s cold cache на 16-core.

### Soundness
- Capability check (Plan 16) catches IO call через 2-level cross-file
  import.
- ALLOC_REQUIRES boehm transitive: `--gc malloc` fails build (clear
  error), not silent crash at runtime.

---

## Risks / Trade-offs

- **Signature/body split breaking change.** `types::check_module` — single
  function сейчас. Split на 2 passes — invasive refactor. Mitigation:
  Phase 1 (signature extraction) можно сделать **дополнительной** pre-pass
  без полного refactor existing check_module; signatures cache used
  только для cross-file lookups.

- **`FileId` migration scope** [N11]. 50+ Span call sites. Mitigation:
  shared с Plan 36 sub-plan 36.A; default `file_id=0` для legacy. Builder
  pattern `Span::with_file(span, file_id)` минимизирует touches.

- **`AD2 ModuleLoader unified` rollout.** ExternalRegistry уже стабильный.
  Mitigation: ExternalRegistry — **первая реализация** trait'а; новый
  filesystem loader — вторая. Backward compat full.

- **Symbol mangling collision** [R8 MVP shortcut]. Underscore-only —
  fragile (module name `foo_bar` collides with `foo.bar`). Mitigation:
  error на underscore в module name as part of file validation
  (parser-level). Полная v0-style mangling — sub-plan 35.D.

- **Performance regression на cold builds**. Cross-file walks adds I/O
  cost. Mitigation: in-memory cache (R2), tree-shaking (R21). On-disk
  cache (35.B) для CI.

- **Spec gaps в visibility semantics** [N22, R6]. Текущий `nova check`
  silently ignores `is_export`. Enforcing visibility — breaking change
  для existing files which may have unintentional cross-file deps. Mitigation:
  audit existing std/* для cross-module references перед enforcement,
  возможно `--no-strict-visibility` escape hatch для migration.

- **Cycle detection edge cases** [G01, R6]. Type-level cycle (recursive
  data structure) — legal. Fn-level cycle (mutual recursion) — legal.
  Const-init cycle — illegal. План должен явно specify which cycles
  legal.

- **Plan 36 36.A dependency.** R17 (FileId) — blocked-on Plan 36.A.
  Mitigation: do Ф.0 (FileId migration) как **shared phase** между
  Plan 35 и Plan 36.

---

## Связь

- **Plan 14 (CLOSED)** — родительский план, Ф.5 описание.
- **Plan 16** — capability enforcement, cross-file effects propagation (AD7).
- **Plan 18 (DRAFT)** — общий roadmap stdlib.
- **Plan 27** — GC backend, ALLOC_REQUIRES transitive (R15).
- **Plan 33** — contracts, cross-file SMT — deferred to 35.C / Plan 33.x.
- **Plan 34** — stdlib typecheck-fix, использует cross-file для resolution.
- **Plan 36 / sub-plan 36.A** — Diagnostic extension (`FileId` shared).

---

## Audit history

- **2026-05-12 v1:** initial draft, выделен из Plan 14 Ф.5.
- **2026-05-12 v2 (this):** 3-way audit (Rust R1-R20, Go G01-G14, Nova
  N01-N25 = 65 gaps). Главные изменения:
  - **MVP boundary** declared (Ф.0 + Ф.1 only).
  - **Sub-plans 35.A-E** split out.
  - **12 architectural decisions** (AD1-AD12).
  - **Cycle detection** promoted из risks в MVP requirement (G01, R6).
  - **Signature/body 2-pass** (R5, AD3) — critical for mutual recursion.
  - **In-memory cache** (R2/G03) — mandatory MVP.
  - **`FileId` в Span** (R17, N11) — shared с Plan 36.A.
  - **Effect/capability propagation** (R14, N05) — soundness gate.
  - **ALLOC_REQUIRES transitive** (R15, N06).
  - **Visibility enforcement** (R6, AD9, N22) — closes existing spec violation.
  - **Module loader unification** с ExternalRegistry (AD2, N03).
  - **Symbol mangling MVP shortcut** + reserved sub-plan 35.D for v0-style.
  - **Build determinism** (R11, AD10).
  - **DCE для cross-file** (R21, N18).
  - **`nova check` parity** (R19, N20).
  - **Path canonicalization** (R20, G10).
  - **Re-export / wildcard / prelude** → sub-plan 35.A (R25-R28).
  - **On-disk cache** → sub-plan 35.B.

Gaps closed: **65/65** (Rust 20 + Go 14 + Nova 25 = 59 unique + 6 cross-listed).
