# Plan 36: CLI production hardening — `nova check` / `nova test`

> **Статус:** план, не начат.
> **Создан:** 2026-05-12.
> **Обновлён:** 2026-05-12 (v4: после 4-way audit, разрешены contradictions,
> объявлен MVP, расписаны architectural decisions + prerequisites).
> **Приоритет:** ВЫСОКИЙ.

---

## Зачем

Сейчас три independent pipeline'а:

1. **`nova test`** — гоняет `nova_tests/**/*.nv` через C-codegen → exe →
   exit code. Path хардкодит `<repo>/nova_tests/`.
2. **`nova check <file>`** — single-file type-check. **BUG (I03/N17):**
   не вызывает `lint_module` + `infer_effects` (unlike `cmd_build`).
   Принимает только file. Нет recurse, parallel, JSON.
3. **stdlib / examples** — нет automated coverage. PowerShell loop ad-hoc.

**Real problems:**
- `nova check` silent на lints + effect-inference которые `nova build`
  ловит — **silent correctness gap** (Ф.0 MVP fix).
- Регрессии в std/ обнаруживаются поздно.
- examples/ не покрыты.
- No machine-readable output → CI tooling не интегрируется.
- Exit codes сливают usage-error и diagnostic-error.

---

## MVP boundary (declared explicitly — closes I25)

План **многосессионный**. MVP — самый узкий subset который даёт **real
production value** без полного scope.

### MVP = Ф.0 + Ф.1 only

1. **Ф.0**: fix `cmd_check` correctness bug (no API changes; ~50 строк).
2. **Ф.1**: `nova check <path>` + `nova test <path>` polymorphic, общий
   фундамент walk/parallel/exit codes/manifest. **Оба в одной фазе** —
   расщеплять не имеет смысла, общий код. (~500 строк, потенциально
   больше с тестами.)

MVP shippable как **standalone plan-36-mvp**. Покрывает:
- R1, R2, R3 (без `.gitignore` v1), R4, R5, R6, R7, R8, R10 (без CI auto-
  detect v1), R11 (path-filter only), R13, R19, R20, R21, R25 (test reuses
  same path semantic).

После MVP — separate plans для каждой подгруппы (см. ниже).

### Post-MVP plans (split into separate)

| Sub-plan | Содержание | Зависимости |
|---|---|---|
| **36.A** | JSON/SARIF/JUnit output (R9, R12, R14, R15, R16, R23) | + `Diagnostic` extension (A04), `DiagRenderer` trait (A02) |
| **36.B** | Caching (R17, R18) | + `blake3` dep approval, bootstrap-minimality policy review (I06) |
| **36.C** | CI ecosystem (R26, R27, R28, R29) | + `nova` distribution story (I18), GitHub Actions auto-detect (R27 → narrow to GHA only v1) |
| **36.D** | Advanced ergonomics (R11 verbosity, progress, completions, `--explain`, `--dry-run`) | none |
| **36.E** | Workspace + manifest (R22, A15 nested nova.toml) | + `nova-manifest` crate |

Каждый sub-plan — отдельный план файл, независимый scope. Это **закрывает
I25** (multi-session realism).

---

## Mainstream reality check (compressed; see v3 history для full)

- **Cargo:** package-graph, exit 0/1/101, `--message-format=json`,
  `--frozen`/`--locked`/`--offline`, `CARGO_TARGET_DIR`, `--color`,
  `-v`/`-vv`, `--explain`, `--message-format=json-diagnostic-rendered-ansi`.
- **Go:** path patterns (`./...`), implicit-skip dirs, exit 0/1/2, NDJSON
  streaming, `$GOCACHE` с transitive deps hash, `-race`, `-cover`,
  `-bench`, `-tags`, `-short`, `-shuffle`, `-failfast`, `gotestsum` для
  JUnit.
- **CI standards:** SARIF 2.1.0, JUnit XML, pre-commit.com,
  `NO_COLOR`/`CLICOLOR`/`CI=true`, GitHub Actions annotations
  (`::error file=,line=,col=,endLine=,endColumn=::msg`).

---

## Архитектурное решение (resolved)

**Path semantic:** go-pattern (path argument, file-or-dir polymorphic).
Без `./...` суффикса — slash-style proven в clippy/eslint/ruff/prettier/
black.

```
nova check                          # walk parents до nova.toml
nova check std/                     # dir → recurse
nova check std/foo.nv               # file → single
nova check std/ examples/           # multi-path
nova check std/ --keep-going --format json
```

Симметрично `nova test`. `--tests-dir` **удаляется** (clean break, без
deprecation cycle — bootstrap, не в проде).

### Architecture decisions (A01-A25 closed)

**AD1. Code placement** (closes A01, A03, A07, A19, A24).
- Создаётся новый crate **`nova-frontend`** (sibling к `nova-codegen` /
  `nova-cli`). Содержит:
  - `check_file_full()` — pipeline orchestration.
  - `walk_nv()` — moved из `test_runner.rs` (где сейчас private).
  - `Environment` struct — env var aggregation (NO_COLOR / CI / etc).
  - Output renderers (`trait DiagRenderer` + 6 impls).
  - Cache layer (`trait CheckCache` + FsCache + MemoryCache).
- `compiler-codegen` остаётся **bootstrap-minimal** (closes A08, I06):
  blake3 / ignore / is-terminal — все в `nova-frontend` или `nova-cli`,
  не в core.
- `nova-cli` остаётся thin: `Cmd` dispatch + flag parsing.
- `nova-cli` нужен **`lib.rs`** — split на `nova-cli` (lib) + `nova` (bin).
  Закрывает A24.

**AD2. `Diagnostic` extension** (closes A04, A05, I02).
- `compiler-codegen/src/diag.rs::Diagnostic` расширяется:
  ```rust
  pub struct Diagnostic {
      pub message: String,
      pub span: Span,
      pub severity: Severity,        // NEW (R14)
      pub code: Option<DiagCode>,    // NEW (R15)
      pub spec_link: Option<String>, // NEW (R16)
      pub suggestion: Option<String>,// NEW
      pub notes: Vec<String>,        // NEW (closes U17 — rustc chain)
  }
  pub enum Severity { Error, Warning, Info, Help }
  pub struct DiagCode(pub &'static str); // "E0042", "W0001"
  ```
- `LintWarning` (lints.rs:24) удаляется, merges в `Diagnostic { severity:
  Warning }`. **Clean break** — все call sites обновляются (bootstrap,
  не в проде).
- Все call sites (50+) обновляются через `Diagnostic::error(msg, span)`,
  `Diagnostic::warning(msg, span)` constructors — default severity, code
  опционально.

**AD3. `DiagRenderer` trait** (closes A02).
- В `nova-frontend/src/render/`:
  ```rust
  pub trait DiagRenderer {
      fn emit(&mut self, d: &Diagnostic);
      fn finish(&mut self, summary: &Summary);
  }
  ```
- 6 impls: `HumanRenderer`, `JsonRenderer` (NDJSON streaming),
  `JsonRenderedRenderer` (JSON envelope с rendered ANSI inside),
  `ShortRenderer`, `JUnitRenderer`, `SarifRenderer`.
- Test/check используют **общий** trait — closes A22.

**AD4. Concurrency model** (closes A09, A10, U23 contradiction).
- Worker threads (`thread::scope`) пишут в `mpsc::Sender<DiagEvent>`.
- Single drain thread читает events, передаёт в `DiagRenderer`.
- **Order policy:** group events by file, drain in **file-discovery
  order** (deterministic). Внутри файла — phase-order (parse → types →
  effects → caps → lints).
- **Streaming + ordering** не конфликтуют: events stream'ятся, но
  **per-file** буфер собирается до завершения файла, потом дренируется
  целиком. Это **per-file streaming** — закрывает U23.
- Determinism test: `-j 16 vs -j 1` — byte-identical output (acceptance
  Ф.1).

**AD5. Capability check phase** (closes A13, I01).
- `capability::check` **не существует** как separate module — логика в
  `types::check_module`. Plan **не выделяет** в отдельную phase;
  `check_file_full()` имеет 4 phases (не 7):
  1. parse
  2. check_module_path (через manifest)
  3. types::check_module (включает D72 bounds + D63/D64 capability)
  4. lints::lint_module + emit_module warnings
- `infer_effects(&mut module)` — вызывается **внутри** `check_module`
  (current behaviour). План v3 имел 7 phases — было wrong.

**AD6. Manifest resolver** (closes A15, I08).
- 4 nested `nova.toml`: repo root (workspace), `nova_tests/`,
  `examples/`, `std/`. Сейчас `find_repo_root` и `find_manifest`
  возвращают **разное** — bug.
- Unified `ManifestResolver`:
  - `find_owning_package(file)` — innermost nova.toml с `[package]`.
  - `find_workspace_root()` — outermost nova.toml с `[workspace]`.
  - Multi-path run: каждый path resolves в свой owning package.
  Cross-package run discouraged, но allowed.
- В MVP только `find_owning_package`; workspace concept — sub-plan 36.E.

**AD7. Cache abstraction** (closes A06, A16, I05).
- `trait CheckCache` (см. AD1).
- **v1 (MVP+36.B):** key = `blake3(file_content || nova_codegen_binary_hash
  || flags || gc_kind)`.
  - `nova_codegen_binary_hash` = hash of `nova-codegen` executable file
    (catches local dev iterations) — closes I05.
- **Known false-PASS limitation:** cross-file import changes не invalidate
  caller cache. Mitigation:
  - `--no-cache` mandatory в CI до Plan 35.
  - Warning при `import` statement в файле + cache hit.
  - Документировано explicitly в R17 sub-plan.
- v2 (post-Plan 35): transitive import hash.

**AD8. Output streaming + sort resolution** (closes U23).
- **NDJSON streaming:** per-file buffer drained на file completion в
  file-discovery-sorted order.
- **`--sorted` flag:** force buffer-all + sort-by-file. Disables streaming.
- **`--no-sort`:** stream events as soon as worker emits (non-deterministic
  order, fastest output).
- Default: per-file streaming (best of both).

**AD9. Diagnostic merging** (closes A11, A12).
- `DiagnosticCollector { diags: Vec<Diagnostic>, max_per_phase: Option<usize> }`
  собирает per-file.
- **Phase ordering:** sequential; failed phase **continues** к next
  (unlike current `cmd_build` which `?`-propagates). Это enables
  collecting all diagnostics. Caller controls через `--keep-going` (=
  default in check) vs `--fail-fast`.
- **Dedup:** key = `(span.start, span.end, code, message_hash)`. Same
  diagnostic emitted by multiple phases → reported once.

**AD10. Manifest validation** (closes A18, I07, sub-plan 36.E).
- MVP: `Diagnostic`-emit для module-path mismatch (R21) — заменяет
  current `anyhow!`.
- Workspace `[workspace]` parser — sub-plan 36.E.
- Schema validation `[package]`/`[lib]`/`[dependencies]` — sub-plan 36.E.

**AD11. Plugin hooks для future plans** (closes A20).
- `trait Phase` registry в `check_file_full`:
  ```rust
  pub trait Phase {
      fn name(&self) -> &str;
      fn run(&self, m: &Module, collector: &mut DiagnosticCollector);
  }
  ```
- MVP: 4 встроенных Phase (parse, manifest_check, types, lints).
- Plan 33 contracts добавит `ContractsPhase` без touching check_file_full.

**AD12. Drift detection для auto-gen** (closes A14, N24).
- `nova check` НЕ запускает `regen-runtime --check` (separate concern).
- CI workflow (Ф.5) sequences: `regen-runtime --check` → `nova check` →
  `nova test`. Documented в Ф.5.

---

## Requirements (R1-R30)

Re-organized по MVP boundary. Каждое R помечено: **[MVP]**, **[36.A-E]**,
или **[deferred R30]**.

### Core path semantics

**R1. [MVP] Polymorphic path argument.** `path.is_file()` /
`path.is_dir()`. Non-existent → exit=2. No glob/pattern v1.

**R2. [MVP] Recursive default for directory.** `is_dir()` всегда recurse.
Нет `--recursive` флага.

**R3. [MVP] Implicit excludes** (hard-coded):
- `target/`, `node_modules/`, `vendor/`, `.git/`, `.hg/`, `.svn/`
- `_*` и `.*` directories
- `std/runtime/` (auto-gen)
- `*.c` рядом с `*.nv`

`.gitignore` respect через `ignore` crate — **[36.A]** (deferred — dep
approval needed для `nova-frontend`).

Override: `--include-runtime`, `--no-exclude <pattern>`.

**R4. [MVP] No-argument behaviour.** `ManifestResolver::find_workspace_root()`
(AD6). Не найден → exit=2.

**R5. [MVP] Multi-path dedup + canonicalisation.** Canonicalize → dedup.
Path outside project root → exit=2 (unless `--allow-outside-project`).
Symlink resolved через `fs::canonicalize`. Windows junction points
обрабатываются same (test'ится). Long paths — `\\?\` prefix через
`dunce` crate (если нужно, оценим в Ф.1).

**R6. [MVP] Wrong-extension / non-existent paths.** Non-existent → exit=2.
File без `.nv` → exit=2. No-read in walk → warning, skip. Windows device
names (`CON`/`NUL`/etc) — treat as non-existent.

**R7. [MVP] Exit codes quintuplet.**
- **0** — all PASS.
- **1** — diagnostic failures.
- **2** — CLI usage error (bad flag, path not found, no nova.toml).
- **3** — codegen / build failure (для `nova test`/`build`, not `check`).
- **101** — panic. **Guaranteed cross-platform** через
  `std::panic::set_hook { std::process::exit(101) }` — closes I12.

**R8. [MVP] Failure aggregation.** Default: continue within path (collect
all per-file diagnostics), fail-fast between paths. `--keep-going` (alias
`--no-fail-fast`) — continue between paths. `--fail-fast` — stop on
first.

### Output (mostly [36.A])

**R9. [36.A] Output formats.** `--format human|json|short|json-rendered|junit|sarif`.
- MVP: только `human` (+ test reuses Plan 27 Б.6 JUnit для `nova test`).
- 36.A: остальные 5 + stable JSON schema с `schema_version`.

**R10. [MVP basic + 36.A advanced] Color control.**
- MVP: `--color auto|always|never` + `NO_COLOR` respect. Auto = tty
  detect через `is-terminal` crate (closes I15).
- 36.A: `CLICOLOR`/`CLICOLOR_FORCE`/`TERM=dumb`/`CI=true` auto-detect.
- 36.C: GitHub Actions annotations.

**R11. [36.D] Verbosity ladder.** `-q`/`-v`/`-vv`/`-vvv`. Deferred — MVP
имеет только default.

**R12. [36.A] Streaming output.** Per-file streaming (AD8). Final
summary как separate NDJSON object с `"type":"summary"` discriminator
(closes U16).

### Correctness — MVP critical fix

**R13. [MVP] `cmd_check` runs FULL pipeline.** Closes existing bug
(I03/N17). `check_file_full()` (AD1) reused by `cmd_check` и `cmd_build`.
Phases (AD5):
1. parse
2. check_module_path (через ManifestResolver, AD6)
3. types::check_module (включает effects + capability inference)
4. lints::lint_module + emit_module warnings (lint warnings captured как
   `Diagnostic { severity: Warning }`)

**R14. [36.A] Severity discrimination.** 4 levels (AD2). `--deny
warnings` повышает в errors. MVP — только error level (closes A05).

**R15. [36.A] Diagnostic code registry.** `compiler-codegen/src/diag/codes.rs`.
Migration phased — MVP не emit'ит codes; 36.A adds + migrates 50+ call
sites через `Diagnostic::error(msg, span).with_code("E0042")` builder.
Closes I02/I23 (codes + `nova explain` ship together в 36.D, не
separately).

**R16. [36.A] Spec links.** Optional `spec_link` field. Builder
`.with_spec_link("decisions/02-types.md#d72")`.

### Caching [36.B]

**R17. [36.B] Content + transitive deps caching.** AD7.
- `$NOVA_TARGET_DIR/check-cache/` (env override; closes G06).
- v1 key = `blake3(content + nova_codegen_binary_hash + flags + gc_kind)`.
- `--no-cache` / `--frozen` (never write) / `--locked` (fail on miss).
  Naming разъяснён в man page: **Nova semantics differ from cargo** —
  Nova не имеет lockfile concept (closes U01, I20). Maybe rename в
  `--cache-readonly` / `--cache-required` чтобы избежать confusion —
  decide в 36.B.
- v2 (post-Plan 35): transitive import hash.
- **Concurrent invocations** (closes U18): atomic write через temp file
  + rename. flock на cache directory.
- **Ctrl-C** (closes U19): no partial cache writes (atomic rename
  guarantee).

**R18. [36.B] Reproducible cache.** Relative paths в cache. No timestamps
в JSON output (closes U21 + I21 — only **build timestamps** removable;
diagnostic events don't have timestamps anyway). `SOURCE_DATE_EPOCH`
respected где применимо.

### Parallelism

**R19. [MVP] Parallel by default.** `-j N` (default num_cpus, 0 =
num_cpus, 1 = sequential). Per-file parallel. Concurrency model AD4.
**Acceptance:** byte-identical output `-j 16` vs `-j 1` (deterministic
test, closes A10).

### Nova-specific

**R20. [MVP] GC backend awareness.** `nova check --gc boehm|malloc`.
`ALLOC_REQUIRES` marker (Plan 27) — file skipped if mismatch. Cache key
includes `gc_kind`. **`parse_alloc_constraint`** moved из `test_runner.rs`
в `compiler-codegen/src/alloc_constraint.rs` (shared between check/test,
closes A19).

**R21. [MVP] Module-path enforcement.** D78 hard-fail через `Diagnostic`
(не anyhow). Span указывает на `module` declaration. `--no-check-module-path`
debug override.

**R22. [36.E] nova.toml manifest validation.** Sub-plan 36.E.

**R23. [36.A] Effect/capability info в JSON.** Per-function effects
serialized. Builder в `nova-frontend/src/render/json.rs`.

**R24. [36.D] Cross-pipeline divergence documentation.** README + man
page section.

**R25. [DROP]** ~~Test-block discovery unified~~. Already works (I22 —
`Item::Test` traversed by `check_module`). **Removed from plan.**

### CI integration [36.C]

**R26. [36.C] pre-commit.com framework.** `.pre-commit-hooks.yaml`.
Distribution story (closes I18): document `cargo install --git <repo>`
+ future GitHub release binaries.

**R27. [36.C] GitHub Actions annotations.** Auto-detect `$GITHUB_ACTIONS`.
**Narrow to GHA in v1** (closes I24). GitLab/Buildkite/CircleCI — separate
follow-up if demand. **Requires `Span` end-position** (closes I04): add
`Diagnostic::span_end_line_col()` helper в `nova-frontend` (walks source).

**R28. [36.C] CI matrix examples.** Reference workflows. **macOS row**
gated on actual testing (closes I17): mark as `experimental` until
verified.

**R29. [36.C] Test artifacts contract.**
- `nova check --output-dir <dir>` flag (closes I11).
- `nova test --junit-output <path>` flag.
- Stable layout `target/test-artifacts/{junit.xml,sarif.json,timings.json}`.

### Deferred [R30]

LSP server mode, code coverage, race detection, benchmark integration,
`.editorconfig`, telemetry, crash reporting, `nova explain` markdown
content, performance regression gate, pluggable checkers beyond AD11,
i18n. См. sub-plan 36.D + R30 list.

---

## Phases — MVP cut

### Ф.0 [MVP] — fix `cmd_check` correctness bug (R13)

**Prerequisite для всех остальных фаз.**

1. Создать crate `nova-frontend` (skeleton).
2. Перенести из `nova-cli/src/main.rs`:
   - `cmd_check` body → `nova_frontend::check_file_full()`.
3. Phases:
   1. parse (existing)
   2. check_module_path (existing)
   3. **types::check_module** (existing — capability + effect inference
      внутри)
   4. **lints::lint_module** (was missing in cmd_check — **the bug**)
   5. **emit_module warnings collection** (was missing)
4. Output unchanged для MVP — текстовый.
5. `cmd_check` и `cmd_build` оба зовут `check_file_full()`.

Acceptance:
- `nova check foo.nv` где foo.nv имеет anonymous-embed override → emit'ит
  warning (currently missed).
- Existing `nova check` tests pass без regression.
- `nova build` unchanged.
- New unit test: file with effect-inference issue triggers diagnostic в
  check.

### Ф.1 [MVP] — `nova check <path>` + `nova test <path>` polymorphic

В `nova-cli/src/main.rs` + `nova-frontend`. Оба subcommand'а в одной
фазе — общие фундамент: walk + path validation + parallel + exit codes
+ ManifestResolver. Делать раздельно нет смысла — будет copy-paste.

**Общий фундамент (`nova-frontend`):**
1. `walk` module:
   - `walk_nv()` (extract из test_runner.rs).
   - R3 implicit-excludes filter (hardcoded, без `.gitignore` v1).
2. R5 canonicalize + dedup + boundary check.
3. R6 validation (non-existent, wrong extension).
4. R4 `ManifestResolver::find_workspace_root()` (AD6 minimal).
5. R7 exit codes + R8 aggregation (mpsc-based, AD4).
6. R10 basic color (NO_COLOR + tty detect via `is-terminal`).
7. R19 parallel `thread::scope` + AD4 concurrency.
8. R20 `--gc` + `parse_alloc_constraint` (moved per AD7).
9. R7 panic hook (`std::panic::set_hook` для guarantee exit=101).
10. `parse_alloc_constraint` move в `compiler-codegen/src/alloc_constraint.rs`
    (AD7).

**`nova check <path>`:**
1. `Cmd::Check { paths: Vec<PathBuf>, jobs, color, keep_going, fail_fast,
   no_check_module_path, allow_outside_project, gc, include_runtime,
   no_exclude }`.
2. R21 module-path via `Diagnostic` (not anyhow).
3. Reuses `check_file_full()` из Ф.0.

**`nova test <path>`:**
1. `Cmd::Test.paths: Vec<PathBuf>` positional, replaces `--tests-dir`.
2. **`--tests-dir` удалён** (clean break, без deprecation).
3. R11 path-filter ⊥ test-name-filter (`--filter` остаётся test-name,
   path — отдельно).
4. R7 exit codes consistent с check (closes U25).
5. file → single test, dir → walk + run all.
6. Reuses Plan 27 test runner infrastructure.

Dependencies added (closes I06):
- `nova-frontend/Cargo.toml`: `is-terminal`, `dunce` (Windows path
  helper if R5 needs).
- `nova-codegen/Cargo.toml`: **no changes** (bootstrap-minimal preserved).

Acceptance: см. Acceptance section (covers both check + test).

### Ф.2 [POST-MVP] — sub-plan 36.A: outputs

Separate plan file `36-a-cli-outputs.md`. Includes:
- `Diagnostic` extension (AD2 — severity, code, spec_link, suggestion,
  notes).
- `DiagRenderer` trait (AD3) + 6 impls.
- R9 formats, R11 verbosity, R12 streaming, R14 severity, R15 codes
  registry + migration of 50+ call sites, R16 spec links, R23
  effect/capability info.
- 36.A зависит от MVP (Ф.0 + Ф.1).

### Ф.3 [POST-MVP] — sub-plan 36.B: caching

Separate plan file `36-b-cli-caching.md`. Includes R17, R18 + concurrent
locking + Ctrl-C cleanup.

### Ф.4 [POST-MVP] — sub-plan 36.C: CI

Separate plan file `36-c-cli-ci.md`. Includes R26, R27, R28, R29 +
distribution story для `nova` binary.

### Ф.5 [POST-MVP] — sub-plan 36.D: advanced ergonomics

`36-d-cli-ergonomics.md`. R11, progress, `--dry-run`, `--list`, completions,
`nova explain`.

### Ф.6 [POST-MVP] — sub-plan 36.E: workspace

`36-e-cli-workspace.md`. R22 manifest validation + nested `nova.toml` +
workspace concept.

### Ф.7 [POST-MVP] — baseline + delta gate

Зависит от всех. Generate `docs/baselines/check.json`. `nova-tools
baseline-compare` helper script. Gating closes A25 — **explicit
"current best" baseline** allows Ф.7 to ship without Plan 34/35.

---

## Acceptance criteria — MVP only

### Path handling (R1-R6)
- `nova check std/` exit 0 на чистом дереве — **gated on Plan 34/35**
  (closes A25). Baseline "current best" в Ф.7 sub-plan, не блокирует MVP.
- `nova check std/foo.nv` (single-file) — works.
- `nova check non_existent.nv` exit **2**, `path not found`.
- `nova check std/foo.txt` exit **2**, `not a Nova source`.
- `nova check ../outside` exit **2** (boundary).
- `nova check` без nova.toml → exit=2.

### Exit codes (R7)
- 0 для all PASS.
- 1 для diagnostic.
- 2 для usage.
- 3 для codegen (test/build only — not check).
- 101 для panic — **explicit hook**, cross-platform Windows+Linux+macOS.

### Correctness (R13 — **the bug fix**)
- Artificial file с anonymous-embed override → warning visible в `nova
  check` output (currently missed — this is the bug).
- `cmd_check` и `cmd_build` share `check_file_full()` (no duplication).
- Existing tests unchanged.

### Output (R10 basic)
- `--color never` + `NO_COLOR=1` → no ANSI.
- `--color always` overrides tty detect.
- Default — auto via `is-terminal`.

### Parallelism (R19)
- `nova check std/` (44 files) cold cache < 5s on 16-core.
- `-j 1` deterministic same output as `-j 16` after AD8 ordering.

### Nova-specific (R20-R21)
- `nova check --gc malloc <file_with_ALLOC_REQUIRES_boehm>` → SKIP.
- `nova check --gc boehm <file_with_ALLOC_REQUIRES_boehm>` → runs.
- Module path mismatch emits **structured `Diagnostic`** (not anyhow
  String).

### `nova test <path>` (Ф.1 together с check)
- `nova test nova_tests/concurrency/` — works.
- `nova test some_file.nv` — works.
- `nova test --tests-dir foo` — exit=2 `unknown flag` (clean break, без
  deprecation).
- Exit codes consistent с check (R7 quintuplet).

---

## Что НЕ входит в MVP

- 6 output formats — sub-plan 36.A.
- Caching — sub-plan 36.B.
- pre-commit / CI annotations / matrix — sub-plan 36.C.
- Verbosity / progress / completions / `--explain` — sub-plan 36.D.
- Workspace concept + manifest schema — sub-plan 36.E.
- LSP, coverage, race, bench, telemetry, i18n — R30 deferred.
- Fix std/examples failures — Plan 34.
- Cross-file resolve — Plan 35.

---

## Связь и Prerequisites

### Plan dependencies (closes A25)
- **Plan 34** — fix std/examples (required для Ф.7 baseline, не для MVP).
- **Plan 35** — cross-file resolve (required для R17 v2; v1 cache имеет
  known false-PASS limitation).

### Internal codebase dependencies
- **Ф.0 fix** = breaking change для silent assumptions. **Risk:**
  surface bugs в existing std/ files (closes I15 — Ф.0 acceptance
  включает "regression list of newly-surfaced warnings" документация).
- **AD2 `Diagnostic` extension** = breaking change в `nova-codegen`
  library API (closes A17, I02). Phased rollout (не для compat, для
  scope management):
  - MVP: добавляются поля с defaults (severity = Error, code = None).
    Existing call sites компилируются без changes.
  - 36.A: переписать 50+ call sites — fill codes, set severity.

### External dependencies
- **MVP добавляет:** `is-terminal`, optionally `dunce` (Windows paths).
  В **`nova-frontend`** crate, не `nova-codegen` (preserves
  bootstrap-minimality, closes A08/I06).
- **36.A добавляет:** `serde_json` для NDJSON, `quick-xml` для JUnit,
  SARIF — can use `serde_json` (SARIF is JSON-based).
- **36.B добавляет:** `blake3`. Approval needed (bootstrap-minimal
  policy).
- **36.C добавляет:** none (workflows are YAML).

---

## Риски / Trade-offs

- **Ф.0 surfaces existing bugs в std/.** Mitigation: запуск на std/
  ДО merge MVP, фиксация "expected newly-surfaced warnings" list. Если
  too many — gate MVP на Plan 34.
- **`Diagnostic` API break.** Mitigation: builder pattern с defaults
  упрощает callsite-update. 50+ переписаний в 36.A — clean break,
  bootstrap не в проде (см. [feedback_revolutionary_changes]).
- **Cache false-PASS до Plan 35.** Mitigation: `--no-cache` mandatory в
  CI, warning при cache hit для файла с `import` statement.
- **Parallel reentrancy.** Mitigation: Ф.1 acceptance имеет explicit
  determinism test (byte-identical `-j 16` vs `-j 1`).
- **Multi-session scope.** Mitigation: **MVP declared** = Ф.0 + Ф.1
  (объединённая для check + test). ~500-700 строк с тестами. Остальное —
  отдельные sub-plans 36.A-E.
- **bootstrap-minimality.** Mitigation: AD1 — все CLI/output deps в
  `nova-frontend`, `nova-codegen` остаётся minimal.
- **4 nested nova.toml.** Mitigation: AD6 unified resolver; MVP только
  `find_workspace_root` simple variant.
- **macOS untested.** Mitigation: R28 marks macOS "experimental" в CI
  matrix. Linux CI достаточно для MVP.

---

## Audit history

- **2026-05-12 v1:** initial.
- **2026-05-12 v2:** cargo/go reality check → R1-R14.
- **2026-05-12 v3:** 3-way audit (G01-G30, H01-H30, N01-N25 = 85 gaps) →
  R1-R30, 8 phases.
- **2026-05-12 v4 (this):** 4-way audit (I01-I25 implementability,
  U01-U25 ergonomics, A01-A25 architecture = 75 new gaps) →
  - **MVP declared** = Ф.0 + Ф.1 only (check + test объединены в одну
    фазу с общим фундаментом).
  - **Sub-plans split** into 36.A (outputs), 36.B (caching), 36.C (CI),
    36.D (ergonomics), 36.E (workspace).
  - **Architectural decisions AD1-AD12** explicit.
  - **R12 streaming vs R19 sort contradiction** resolved via AD8
    per-file streaming + `--sorted` flag.
  - **R25 dropped** (already works, I22).
  - **Phase sequence corrected** — `capability::check` не отдельный
    module, ушёл в types::check_module (AD5).
  - **Prerequisites enumerated**: Plan 34 (baseline), Plan 35 (cache
    v2), `nova-cli` lib.rs split (A24), `Diagnostic` extension breaking
    change (AD2), `nova-frontend` crate creation (AD1),
    `parse_alloc_constraint` move (AD7), unified `ManifestResolver`
    (AD6).
  - **Distribution story** acknowledged как gap (I18) — sub-plan 36.C
    включает `cargo install`/binary release decision.
  - **Realism check:** MVP ~500 строк = one focused session. Each
    sub-plan separate.

Gaps closed: **160/160** (85 v3 + 75 v4), with clear MVP boundary.
