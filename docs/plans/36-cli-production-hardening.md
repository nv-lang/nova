# Plan 36: CLI production hardening — `nova check` / `nova test`

> **Статус:** план, не начат.
> **Создан:** 2026-05-12. **Обновлён:** 2026-05-12 (после 3-way audit: cargo / go+CI / nova-specific = 85 gaps).
> **Приоритет:** ВЫСОКИЙ.

---

## Зачем

Сейчас три independent pipeline'а проверки:

1. **`nova test`** — гоняет `nova_tests/**/*.nv` через C-codegen → exe →
   exit code. Path хардкодит `<repo>/nova_tests/`.
2. **`nova check <file>`** — single-file type-check одного файла. **БАГ:**
   не вызывает `lint_module` и `infer_effects` (unlike `cmd_build`).
   Принимает только file, не dir; нет recurse; нет parallel.
3. **stdlib / examples** — нет automated coverage. PowerShell loop ad-hoc.

**Реальные проблемы:**

- `nova check` молчит про lints + effect-inference которые `nova build`
  ловит — **silent correctness gap**.
- Регрессии в std/ обнаруживаются поздно.
- examples/ не покрыты вообще.
- No machine-readable output (NDJSON / SARIF / JUnit) — CI tooling не
  интегрируется.
- Exit codes сливают usage-error с diagnostic-error.

---

## Mainstream reality check

**Cargo:** package-graph через `-p <SPEC>`, не paths. JSON через
`--message-format=json` со stable schema (`reason`, `target`,
`message.spans[]` с byte ranges). Exit 0/1/101. `--keep-going`,
`--frozen`, `--locked`, `CARGO_TARGET_DIR`, `RUSTC_WRAPPER`, `--color`,
`-v`/`-vv`, `--config KEY=VAL`, `--message-format=json-diagnostic-rendered-ansi`
(human inside JSON), `rustc --explain Exxxx`.

**Go:** package patterns (`./...`), `vendor/`/`_*`/`.*` hard-skip. NDJSON
streaming per-package. Exit 0/1/2. `go test -race`, `-coverprofile`,
`-bench`, `-short`, `-tags`, `-timeout`, `-shuffle`, `-failfast`.
`gotestsum` для JUnit. `$GOCACHE` content-hash с **transitive dep
hashing**.

**CI standards:** SARIF 2.1.0 (GitHub Code Scanning, Sonar), JUnit XML
(GitHub Actions, GitLab, Jenkins), `.pre-commit-hooks.yaml` (pre-commit.com),
LSP textDocument/publishDiagnostics, `GITHUB_STEP_SUMMARY` /
`::error file=,line=::msg` annotations, `NO_COLOR` / `CLICOLOR_FORCE` env.

**Что typically missed (Bun, Deno, Zig):** stable JSON schema, separate
exit codes 0/1/2, implicit-skip dirs, canonicalize/dedup, transitive
cache invalidation, SARIF, LSP.

---

## Архитектурное решение для Nova

**Не cargo-style** (cargo отказался от path args). Идём по **go-pattern**:
positional path, file-or-directory polymorphic, hard-coded skip,
recursive default для dir. Без `./...` суффикса — slash-style proven в
`clippy <path>`, `eslint`, `prettier`, `ruff`, `black`.

```
nova check                          # walk-parents до nova.toml, project root
nova check std/                     # dir → recurse
nova check std/collections/vec.nv   # file → single
nova check std/ examples/           # multi-path
nova check std/ --keep-going --format json
```

Симметрично `nova test`. Existing `--tests-dir` deprecated.

---

## Requirements (R1-R30, MUST/SHOULD/COULD priorities)

Каждое R прослежено в Acceptance. **MUST** = Ф.1-Ф.4 (production gate).
**SHOULD** = Ф.5-Ф.7. **COULD** = Ф.8+, deferred.

### Core path semantics (MUST)

**R1. Polymorphic path argument.** `path.is_file()` / `path.is_dir()` —
дискриминация. Несуществующий path → exit=2. Никаких glob/pattern в
v1.

**R2. Recursive default for directory.** `is_dir()` всегда recurse. Нет
`--recursive` флага (clippy/eslint/ruff convention).

**R3. Implicit excludes** (hard-coded skip):
- `target/`, `node_modules/`, `vendor/`, `.git/`, `.hg/`, `.svn/`
- `_*` и `.*` directories at any level
- `std/runtime/` — auto-gen (Plan 13)
- `*.c` рядом с `*.nv` (codegen artifact)
- **`.gitignore` / `.novaignore`** respect через `ignore` crate

Override через `--include-runtime`, `--no-exclude <pattern>`,
`--no-respect-gitignore`.

**R4. No-argument behaviour.** Walk parents до `nova.toml`. Не найден →
exit=2 `nova.toml not found in <cwd> or any parent`.

**R5. Multi-path dedup + canonicalisation.** Canonicalize → dedup set.
Path outside nova.toml project root → exit=2 (unless `--allow-outside-project`).
Symlink resolved.

**R6. Wrong-extension / non-existent paths.** Non-existent → exit=2.
File без `.nv` → exit=2. No-read inside walk → warning, skip.

**R7. Exit codes quintuplet** (refined after cargo/go review):
- **0** — all PASS (no diagnostics).
- **1** — diagnostic failures (type errors, parse errors, lints с `--deny`).
- **2** — CLI usage error (bad flag, path not found, no nova.toml).
- **3** — codegen / build failure (separate from type-check).
- **101** — internal panic / tool bug.

Cargo использует 101 для panic — мы reuse. Это **fixes G11** (build vs
diagnostic conflation).

**R8. Failure aggregation.** Default: continue within single path-arg,
fail-fast between paths. `--keep-going` / `--no-fail-fast` (alias):
continue everywhere. `--fail-fast`: stop on first.

### Output (MUST)

**R9. Output mode dichotomy + rendered field.**

Modes: `--format human|json|short|json-rendered|junit|sarif`.

- **human** — colored, file:line:col, source snippet, suggestion. Default.
- **json** — NDJSON, **stable schema** (`schema_version: "1"`), один
  объект на диагностику:
  ```json
  {"schema_version":"1","severity":"error","file":"std/foo.nv",
   "line":12,"column":5,"byte_start":234,"byte_end":248,
   "code":"E0042","message":"undefined identifier `bar`",
   "spec_link":"decisions/02-types.md#d72",
   "suggestion":"did you mean `baz`?",
   "effects":[],"capabilities":{}}
  ```
  Финальная: `{"schema_version":"1","summary":{"passed":38,"failed":6,"skipped":2}}`.
- **json-rendered** — JSON envelope с `rendered: "..."` field (ANSI-colored
  human form inside). Cargo `json-diagnostic-rendered-ansi` equivalent.
  Закрывает G10.
- **short** — `<file>:<line>:<col>: <msg>` без snippet.
- **junit** — XML, GitHub Actions / GitLab / Jenkins compatible.
  Reuse existing `Plan 27 Б.6` JUnit infrastructure из test_runner.
- **sarif** — SARIF 2.1.0 для GitHub Code Scanning. **MUST** для security
  CI integration.

**R10. Color control.** `--color auto|always|never`. Auto = isatty.
Respect `NO_COLOR=1`, `CLICOLOR=0`, `CLICOLOR_FORCE=1`, `TERM=dumb`,
`CI=true` (auto-disable). GitHub Actions special: emit `::error file=,line=::msg`
annotations + `GITHUB_STEP_SUMMARY` markdown.

**R11. Verbosity ladder.** `-q` / `-v` / `-vv` / `-vvv`. Default normal.
`-q` only failures + final summary. `-vv` includes timing per file.

**R12. Streaming output.** NDJSON flushes per-diagnostic (not at end).
Critical для CI live dashboards.

### Correctness — fix existing bugs (MUST)

**R13. `cmd_check` runs FULL pipeline.** Currently `cmd_check` runs only
parse + check_module_path + check_module — **misses `infer_effects` +
`lint_module`** что `cmd_build` делает. Это **silent correctness gap**.

Fix: extract `pub fn check_file_full()` в `nova-cli` lib, reuse в
`cmd_check` и `cmd_build`. Включает sequence:
1. parse
2. check_module_path
3. types::check_module
4. effects::infer_effects (D28 inference)
5. capability::check (Plan 16 forbid/realtime)
6. lints::lint_module (anonymous-embed warnings и т.д.)
7. (optional) bound_check propagation
8. emit diagnostics через unified `Diagnostic` API

**R14. Severity discrimination.** Diagnostics имеют 4 severity: `error`,
`warning`, `info`, `help`. Plan 15 generic bounds, Plan 16 forbid — `error`.
Lint (anonymous-embed override) — `warning`. `--deny warnings` повышает
все warnings до errors (exit=1). `-W` / `-A` / `-D <lint>` overrides.

**R15. Diagnostic code registry.** Каждый diagnostic имеет stable code
(`E0001`..`E9999`, `W0001`..`W9999`). Registry в
`compiler-codegen/src/diag/codes.rs`. `nova explain <code>` subcommand
показывает full explanation (rustc-style). Codes immutable — translation-
stable identifiers (H22).

**R16. Spec links in diagnostics.** Каждый diagnostic optional `spec_link`
field. Human form: `For more info, see spec/decisions/02-types.md#d72`.
JSON: `"spec_link": "..."`. Reuse existing `diag.rs::Diagnostic`.

### Caching (MUST)

**R17. Content + transitive deps caching.** Cache dir:
`$NOVA_TARGET_DIR/check-cache/` (default `<project>/target/check-cache/`).
Key: `blake3(file_content || nova_version || flags || transitive_deps_hash)`.

**Transitive deps hash** — это **closes G19/H01**. Naive `blake3(content)`
даёт false-PASS если изменился импортируемый файл. Решение:
- v1 (до Plan 35): hash = `blake3(content + version + flags)`. Cache
  invalidates на любое изменение nova-codegen или flag. False-PASS
  возможен только при cross-file import (документируем как known
  limitation; mitigation — `--no-cache` в CI).
- v2 (после Plan 35): build import graph, hash transitive closure.

`--no-cache` / `--frozen` (никогда не пишет cache) / `--locked` (fail
если cache miss). `$NOVA_TARGET_DIR` env override.

**R18. Reproducible cache.** Relative paths в cache; no timestamps в JSON
output (или `--no-timestamps`); respect `SOURCE_DATE_EPOCH` env
(Debian/NixOS standard).

### Parallelism (MUST)

**R19. Parallel by default.** `-j N` / `--jobs N`. Default = num_cpus.
`-j 1` для deterministic output. Per-file parallel (no deps в bootstrap).
After Plan 35: topological. Output sort-by-file-name перед finalize для
determinism.

### Nova-specific (MUST)

**R20. GC backend awareness for check.** `nova check --gc boehm|malloc`
(symmetric с test/build). Если файл имеет `// ALLOC_REQUIRES <backend>`
(Plan 27 marker) и `--gc` mismatch → **skip с reason='alloc-backend'**
(не error). Cache key включает `gc_kind`.

**R21. Module path enforcement is hard-fail.** D78 `check_module_path`
hard-fail (current behaviour). `--no-check-module-path` для bootstrap
debug; `--strict-module-path` (default) для CI.

**R22. nova.toml schema validation.** `nova check --manifest` (отдельный
mode) или часть default check'а. Validate `[package].name`, `[lib].src`,
`[dependencies]` (после Plan 03). Invalid manifest → exit=2.

**R23. Effect/capability info в JSON output.** Per-function effects
(`["Random","IO","Fail[E]"]`) + forbid/realtime annotations. **Nova-only
feature**, no precedent в cargo/go. Закрывает N01.

**R24. Cross-pipeline divergence documentation.** Plan explicitly
documents: `check` PASS ≠ `run` PASS ≠ `build` PASS ≠ `test` PASS. Each
pipeline имеет свои limitations (interp baseline 43/92, codegen flat
var_types, etc). Emit warning в README + man page.

**R25. Test-block discovery unified.** `test "name" { }` blocks внутри
production .nv. `nova check <file>` проверяет test-блоки (parse + type).
`nova test <file>` — runs them. Same parser, разные phases.

### CI integration (SHOULD)

**R26. pre-commit.com framework.** Publish `.pre-commit-hooks.yaml` в
repo:
```yaml
- id: nova-check
  name: nova check
  entry: nova check
  language: system
  files: '\.nv$'
- id: nova-test
  name: nova test
  entry: nova test
  language: system
  stages: [pre-push]
```
Users добавляют в `.pre-commit-config.yaml`. **No custom `install-hooks.ps1`**
(closes H14).

**R27. GitHub Actions native output.** Auto-detect `$GITHUB_ACTIONS` env.
Emit `::error file=,line=,col=,endLine=,endColumn=::message` annotations
(in-line PR diff). Emit `GITHUB_STEP_SUMMARY` markdown с pass/fail table.

**R28. CI matrix support.** Workflow examples в repo:
- `os: [windows-2022, ubuntu-22.04, macos-14]`
- `gc: [boehm, malloc]`
- `nova-version: [main, latest-tag]`

Не enforce — provide working `.github/workflows/{check,test}.yml`
examples.

**R29. Test artifacts contract.** Stable layout:
- `target/test-artifacts/junit.xml` — JUnit XML
- `target/test-artifacts/sarif.json` — SARIF report
- `target/test-artifacts/coverage.lcov` — (future, deferred)
- `target/test-artifacts/bench.json` — (future)
- `target/test-artifacts/timings.json` — per-test perf

Для `actions/upload-artifact` integration.

### Deferred (COULD)

**R30. Future hooks** (architecture leaves space, not implemented v1):
- LSP server mode (`nova check --lsp`).
- Code coverage (`--coverprofile`).
- Race detection (`nova test --race`) через TSan compile flag.
- Benchmark integration (`nova test --bench`).
- `.editorconfig` respect (for future `nova fmt`).
- Telemetry opt-in.
- Crash reporting infrastructure.
- `nova explain <code>` markdown registry (R15 codes are ready, explain
  command — separate).
- Performance regression gate (`hyperfine` integration).
- Pluggable checkers (для Plan 33 contracts, Plan 16 advanced lints).

---

## Phases

### Ф.0 — fix `cmd_check` correctness bug (R13)

**Pre-requisite**. Extract `check_file_full()` в `nova-codegen` lib.
`cmd_check` и `cmd_build` оба call this. Без этого Ф.1 будет cementing
silent gap.

Acceptance:
- `nova check foo.nv` repository emits lint warnings (anonymous-embed
  override) и effect-inference errors — currently missed.
- `nova build foo.nv` использует same path (no behaviour change).
- Tests: artificial file with anonymous-embed override → warning visible
  в `nova check` output.

### Ф.1 — `nova check <path>` core (R1-R8, R10-R12, R19-R22)

В [nova-cli/src/main.rs](../../nova-cli/src/main.rs):

1. `Cmd::Check.paths: Vec<PathBuf>` (clap `num_args = 0..`).
2. Walk-parents для nova.toml (R4).
3. Canonicalize + dedup (R5) + boundary check (R12).
4. R6 validation (extension, exists).
5. `walk_nv()` + R3 implicit-excludes filter (включая `.gitignore`).
6. R8 aggregation logic.
7. R10 color control + R11 verbosity.
8. R19 parallel `thread::scope`.
9. R20 GC backend awareness + R21 module-path strict mode.
10. R7 exit codes 0/1/2/3/101.

Acceptance: см. ниже Acceptance section.

### Ф.2 — output formats (R9, R14-R16, R23)

1. `--format human|json|short|json-rendered|junit|sarif`.
2. R14 severity (error/warning/info/help) + `--deny warnings` / `-W` / `-A` / `-D`.
3. R15 diagnostic codes registry (E0001..E9999, W0001..W9999) в
   `compiler-codegen/src/diag/codes.rs`.
4. R16 spec links в diag emissions.
5. R23 effect/capability info в JSON.
6. SARIF 2.1.0 conformance.
7. R12 streaming flush per-diagnostic.

Acceptance: см. ниже.

### Ф.3 — caching (R17, R18)

1. `$NOVA_TARGET_DIR/check-cache/` (default `target/check-cache/`).
2. Blake3 key с version + flags + gc_kind (R20 dependency).
3. `--no-cache`, `--frozen`, `--locked` flags.
4. R18 reproducible: relative paths, `SOURCE_DATE_EPOCH` respect,
   `--no-timestamps`.

Acceptance:
- Second run `nova check std/` < 500ms.
- After nova-codegen rebuild, cache invalidated.
- `SOURCE_DATE_EPOCH=0 nova check ... --format json` produces identical bytes.

### Ф.4 — `nova test <path>` symmetric (R1-R8 applied)

1. `Cmd::Test.paths: Vec<PathBuf>` positional.
2. `--tests-dir` deprecated с warning.
3. Implicit excludes (R3), boundary (R5/R12).
4. R11 path-filter ⊥ test-name-filter (orthogonal).
5. Reuse existing Plan 27 `--shuffle`, timeout, etc.

Acceptance: см. ниже.

### Ф.5 — CI integration (R26-R29)

1. R26 `.pre-commit-hooks.yaml` в repo root.
2. R27 GitHub Actions output adapter (auto-detect, emit annotations).
3. R28 example workflows `.github/workflows/{check,test}.yml`.
4. R29 test artifacts layout standardisation.

### Ф.6 — Baseline + delta gate

1. Generate baseline: `nova check std/ examples/ --format json >
   docs/baselines/check.json`.
2. `nova-tools baseline-compare <baseline> <current>` helper script —
   exits non-zero если число PASS уменьшилось vs baseline (или количество
   FAIL increased).
3. CI runs baseline-compare после каждого `check`.

### Ф.7 — Documentation

1. Man page (`docs/man/nova-check.md`, `nova-test.md`).
2. R24 cross-pipeline divergence section в README.
3. JSON schema spec (`docs/schema/check.json` для validation).
4. Migration guide для `--tests-dir` deprecation.

---

## Acceptance criteria (overall)

### Path handling (R1-R6)
- `nova check std/` exit 0 на чистом дереве (после Plan 34).
- `nova check examples/` exit 0 (после fix module-path).
- `nova check non_existent.nv` exit **2**, message `path not found`.
- `nova check std/foo.txt` exit **2**, `not a Nova source`.
- `nova check ../outside` exit **2** (boundary).
- `nova check` без nova.toml → exit=2.

### Exit codes (R7)
- 0 для all PASS.
- 1 для type-check error.
- 2 для bad flag / missing nova.toml / wrong extension.
- 3 для codegen failure (когда применимо).
- 101 на panic.

### Correctness (R13)
- `nova check foo.nv` где foo.nv имеет anonymous-embed override → выводит
  warning (currently missed).
- `cmd_check` и `cmd_build` используют same `check_file_full()`.

### Output (R9, R10, R23)
- `--format json | jq .` — valid NDJSON, схема включает `schema_version`,
  `byte_start/byte_end`, `code`, `spec_link`, `effects`.
- `--format sarif` — valid SARIF 2.1.0 (validated против JSON schema).
- `--format junit` — valid JUnit XML (validated GitHub Actions test
  reporter).
- `--color never` + `NO_COLOR=1` — no ANSI codes в output.
- `CI=true` env auto-detect → `--color never` + `--format short` by
  default.
- GitHub Actions detect → emit `::error::` annotations.

### Performance (R17, R19)
- `nova check std/` (44 файла) cold cache < 5s на 16-core.
- Second run < 500ms.
- `--no-cache` re-runs full.

### Nova-specific (R20-R22)
- `nova check --gc malloc <file_with_ALLOC_REQUIRES_boehm>` → SKIP.
- `nova check --gc boehm <file_with_ALLOC_REQUIRES_boehm>` → runs.
- Cache invalidates на nova_version change OR gc_kind change.
- Invalid nova.toml → exit=2.

### CI integration (R26-R29)
- `.pre-commit-hooks.yaml` validated against pre-commit.com schema.
- Example `.github/workflows/check.yml` parses + runs end-to-end.
- Baseline-compare script catches regression.

### `nova test` symmetric (Ф.4)
- `nova test nova_tests/concurrency/` — только concurrency.
- `nova test some_file.nv` — single file.
- `nova test --tests-dir foo` — works + deprecation warning.
- `nova test --filter "snapshot"` — orthogonal к path.

---

## Что НЕ входит (deferred)

- **Fix std/examples failures** — Plan 34.
- **Cross-file resolve / transitive cache** — Plan 35 (R17 v2).
- **Glob patterns** (`*.nv`, `**`) — shell sufficient.
- **`./...` go-style suffix** — slash-style proven.
- **Pattern-spec arguments** (cargo `-p`) — не наш case.
- **`--fix` suggestions** — Plan 14 заложил базу, отдельная задача.
- **LSP server mode** — R30 deferred.
- **Code coverage** — R30 deferred.
- **Race detection** — R30 deferred.
- **Benchmarks** — R30 deferred (gc_bench уже есть, но integration с
  test runner — другая задача).
- **Telemetry** — R30 deferred.
- **`nova explain <code>`** — R15 готовит registry, command сам — другой
  план.
- **i18n** — R15 stable codes готовят базу, локализация — отдельно.
- **Workspace concept** (multi-package monorepo) — Plan 03.

---

## Связь

- **Plan 14** — std blockers (closed), частично fixит type-check fail'ы.
- **Plan 15** — generic bounds, Diagnostic infrastructure (reuse в R9/R15/R16).
- **Plan 16** — forbid/realtime capability (R23 input).
- **Plan 27** — GC backend (R20).
- **Plan 28** — nova-cli foundation.
- **Plan 33** — contracts (R30 future hook).
- **Plan 34** — fix existing std/ fails (prerequisite для baseline).
- **Plan 35** — cross-file resolve (R17 v2 prerequisite).

---

## Риски / Trade-offs

- **Ф.0 correctness fix** может сломать существующие assumptions —
  `cmd_check` сейчас silent на effects/lints. После fix — may surface
  bugs во многих existing files. Mitigation: запуск на std/ baseline
  ДО enabling, фиксация regression list.
- **Cache корректность без cross-file resolve.** R17 v1 даёт false-PASS
  при transitive import. Mitigation: `--no-cache` mandatory в CI до
  Plan 35; warning при detection import statement в файле.
- **Parallel-safety типчекера.** `check_module` reentrancy audit
  needed. Mitigation: Ф.1 sequential first, parallel в Ф.3 с
  determinism test.
- **R28 CI matrix vs maintenance burden.** Matrix usage для 3 OS × 2
  GC × 2 versions = 12 jobs per push. Mitigation: matrix только на main
  branch; PR runs ubuntu+boehm only.
- **Stable diagnostic codes (R15).** После публикации нельзя renumber.
  Mitigation: reserve ranges `E0001-E0099` core types, `E0100-E0199`
  effects, etc. — semantic grouping для future-proofing.
- **SARIF 2.1.0 maintenance.** Schema спецификация большая. Mitigation:
  start с minimal subset (just diagnostics, no flow/codeFlows), grow
  on demand.
- **R23 effect/capability info exposes internals.** Spec evolves — JSON
  schema breaks. Mitigation: `schema_version` field + deprecation
  policy.

---

## Audit history

- **2026-05-12 (v1):** создан после user'ского запроса.
- **2026-05-12 (v2):** rewrite после cargo/go reality check — добавлены
  R1-R14.
- **2026-05-12 (v3, this):** rewrite после 3-way audit (cargo gaps G01-G30,
  go+CI gaps H01-H30, nova-specific gaps N01-N25 = 85 gaps total).
  Добавлены:
  - **R7 exit codes расширены до 5 кодов** (0/1/2/3/101) — closes G11/H11.
  - **R9 расширен** до 6 форматов включая SARIF, JUnit, json-rendered
    с ANSI inside — closes G10/G12/H10/H13.
  - **R10 color + env vars** — closes G03/G04/H29.
  - **R11 verbosity ladder** — closes G07.
  - **R12 streaming output** — closes H28.
  - **R13 cmd_check correctness fix** — closes N14/N17 (existing bug,
    not just plan gap).
  - **R14 severity discrimination** — closes G27/N10.
  - **R15 stable diagnostic codes** — closes H21/H22.
  - **R16 spec links в diagnostics** — closes N11.
  - **R17 transitive cache** + `$NOVA_TARGET_DIR` env — closes G06/H01/G19.
  - **R18 reproducible** — closes H20.
  - **R20 GC backend awareness** — closes N02/N07/N25.
  - **R21 module-path strict mode** — closes N05.
  - **R22 nova.toml schema** — closes N06.
  - **R23 effect/capability info в JSON** — closes N01.
  - **R24 cross-pipeline divergence docs** — closes N04.
  - **R25 test-block discovery unified** — closes N15.
  - **R26 pre-commit.com framework** — closes G27/H14.
  - **R27 GitHub Actions output adapter** — closes H18.
  - **R28 CI matrix** — closes H17.
  - **R29 test artifacts contract** — closes H30.
  - **R30 deferred hooks** (LSP, coverage, race, bench, telemetry,
    explain, hyperfine, .editorconfig, contracts checker hooks) —
    architecturally space, not implemented v1.
  - **Ф.0 phase added** — fix existing correctness bug before Ф.1.

Gaps закрыты: **85/85** (с разделением MUST/SHOULD/COULD приоритетов).
