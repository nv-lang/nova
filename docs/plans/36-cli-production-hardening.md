# Plan 36: polymorphic CLI paths для `nova check` / `nova test`

> **Статус:** план, не начат.
> **Создан:** 2026-05-12. **Обновлён:** 2026-05-12 (после cargo/go reality check).
> **Приоритет:** ВЫСОКИЙ — отсутствие automated coverage для stdlib/examples
> означает что регрессии в codegen/type-checker'е там обнаруживаются вручную
> через ad-hoc PowerShell loops.

---

## Зачем

Сейчас три independent pipeline'а проверки:

1. **`nova test`** — гоняет `nova_tests/**/*.nv` через C-codegen → exe →
   exit code. Path хардкодит `<repo>/nova_tests/` (есть legacy
   `--tests-dir`).
2. **`nova check <file>`** — single-file type-check одного файла. Принимает
   **только file**, не directory; нет recurse; нет parallel; нет
   aggregation.
3. **stdlib (`std/`) + examples (`examples/`)** — нет automated coverage.
   Пользователь руками гоняет PowerShell loop:
   ```pwsh
   Get-ChildItem -Recurse -Path std -Filter *.nv | ... nova check $_
   ```

**Реальные проблемы:**

- **Регрессии в std/ обнаруживаются поздно.** Когда меняется codegen
  или type-checker, std-файлы могут сломаться. `nova_tests/` этого не
  ловят (не импортируют std/).
- **examples/ не покрыты вообще.** README обещает рабочие примеры, но
  никто их не type-check'ает в CI/pre-commit. drift с current spec
  накапливается тихо.
- **No single-command pre-commit gate.** Разработчику нужно помнить про
  ручной loop — легко забыть.

---

## Mainstream reality check (cargo / go)

После research'а cargo + go toolchain'ов:

**Cargo** **НЕ** принимает path arguments. Selection через `-p <SPEC>`,
`--workspace`, `--exclude`, `--manifest-path`. Это **намеренная позиция:**
package-graph, не файл-граф.

**Go** работает с **package patterns**: `./...` (recursive descent от
CWD), multi-pattern `go vet ./a/... ./b/...`, hard-coded skip для
`vendor/`, `_*`, `.*`, `testdata/`. Single-file form работает только
если все `.go`-файлы пакета перечислены явно (синтезирует «fake
package»).

**Exit codes — две разные семантики:**
- Go: 0 success, 1 diagnostic/build failure, 2 tool-invocation error
  (bad flag, unknown command).
- Cargo: 0 ok, 101 panic/internal, 1 для всего остального.

**Stable JSON output:** Cargo `--message-format=json` со schema
(`reason`, `target`, `message.spans[]` с `byte_start/byte_end` +
`line_start/column_start`). Go `-json` per-package, NDJSON-like
(один JSON-объект на пакет). Human и JSON — **взаимоисключающие**
моды.

**Parallelism:** Cargo `-j N` per-crate; Go `-p N` per-package + topo-order
по dependency-graph. Внутри unit — sequential.

**Failure aggregation:** Cargo по умолчанию fail-fast по unit; `--keep-going`
для cross-unit. Go vet собирает diagnostics по всем пакетам, exit=1
если хоть один failed, не останавливается на первом.

**Что у naive подходов (Bun, Deno, Zig) хуже:** нет stable JSON schema,
exit codes сливаются (0/1 без 2), нет implicit-skip директорий, нет
canonicalize/symlink dedup.

---

## Архитектурное решение для Nova

Nova **не cargo-style** — мы хотим path arguments (cargo же отказался
от path). Идём **по go-pattern**: positional path, file-or-directory
polymorphic, hard-coded skip, recursive default для dir. Но без
`./...` суффикса (slash-style proven в `clippy <path>`, `eslint`,
`prettier`, `ruff`, `black`).

```
nova check                          # walk-parents до nova.toml, project root
nova check std/                     # dir → recurse
nova check std/collections/vec.nv   # file → single
nova check std/ examples/           # multi-path
nova check std/ --keep-going        # continue после first error
nova check std/ --format json       # NDJSON output for CI
```

Симметрично `nova test`. Existing `--tests-dir` deprecated.

---

## 14 production-grade requirements (closed by this plan)

Помечены [R1..R14] для traceability.

### R1. Argument typology
Path argument **polymorphic** (file-or-dir). Никаких pattern-spec'ов
(`./...`, glob). Дискриминация через `path.is_file()` / `path.is_dir()`.
Несуществующий path → exit=2.

### R2. Recursive default for dir
`is_dir()` всегда recurse. Никакого `--recursive` флага — следуем
clippy/eslint/ruff convention. Pattern matching (`*.nv`) deferred —
shell expansion достаточен.

### R3. Implicit excludes
Hard-coded skip (даже без `--skip`):
- `target/` — Rust build output
- `node_modules/`, `vendor/` — package deps
- `.git/`, `.hg/`, `.svn/` — VCS
- `_*` и `.*` directories at any level (kebab-style hidden)
- `std/runtime/` — auto-gen Nova-runtime (Plan 13)
- любая `*.c` рядом с `*.nv` (codegen artifact)

Override через `--include-runtime` (для `std/runtime/`),
`--no-exclude <pattern>` (для других).

### R4. No-argument behaviour
`nova check` без аргументов → walk parents в поиске `nova.toml`.
Найден → use as root (recurse). Не найден до filesystem root →
exit=2 с message `nova.toml not found in <cwd> or any parent`.

### R5. Multi-path dedup + canonicalisation
1. Canonicalize каждый path (resolve symlinks, normalize separators).
2. Если canonical-path уже в set → skip с warning'ом `duplicate path
   ignored: <orig>`.
3. Если canonical-path **outside nova.toml project root** → exit=2 с
   `path outside project root: <orig>`. Не пропускаем за boundary даже
   через symlink.

### R6. Wrong-extension / non-existent paths
- Non-existent → exit=2 `path not found: <p>`.
- File without `.nv` extension → exit=2 `not a Nova source: <p>` (даже
  если позднее окажется что нужно проверить).
- File без read permission → exit=2 с system error.
- Файлы без read access внутри recursive walk → warning, skip, продолжаем.

### R7. Exit codes triplet
- **0** — all PASS.
- **1** — ≥1 diagnostic failure (type-check error, parse error). Это
  "real" failure от user code.
- **2** — CLI usage error: bad flag, path not found, non-`.nv` file,
  no nova.toml, outside project root.
- **≥3** — internal tool error (panic). Reserved для future; сейчас
  `clap` сам падает с exit=2 на arg parsing — оставляем.

Никогда не сливать 1 и 2. Cargo делает это (всё = 1) — это **regression**
от Go-стандарта.

### R8. Failure aggregation
**Default:** continue в пределах единичного path-argument, fail-fast
между разными path-arguments. Это go-vet style.

`--keep-going` — продолжать **всё**, собрать diagnostics даже после
fatal error в одном из path'ов.

`--fail-fast` — обратное: stop на первой ошибке (полезно для большого
std/).

### R9. Output mode dichotomy
`--format human|json|short`. Default human.

- **human:** colored, file:line:col, source snippet, suggestion hint.
- **json:** NDJSON, один JSON per diagnostic, stable schema:
  ```json
  {"severity":"error","file":"std/foo.nv","line":12,"column":5,
   "byte_start":234,"byte_end":248,"code":"E0042",
   "message":"undefined identifier `bar`",
   "suggestion":"did you mean `baz`?"}
  ```
  Финальная строка: `{"summary":{"passed":38,"failed":6,"skipped":2}}`.
- **short:** `<file>:<line>:<col>: <msg>` без snippet (для grep-friendly
  output).

**Никогда одновременно human + json.**

### R10. Caching
**Content-hash based**, не mtime. Cache dir: `<project>/target/check-cache/`.
- Key: blake3 hash of (file content + nova-codegen version + check-options).
- Hit → skip парсинг + check, mark file as PASS (cached).
- `--no-cache` — disable lookup, force re-check.

Caching применяется **после** path-arg filter resolution (filter сначала
определяет какие файлы, потом cache lookup per-файл).

Cache invalidation на rebuild nova-codegen — встроено в key (version).

### R11. Path-filter ⊥ test-name-filter
Эти два axes **независимы**:
- `nova check std/ --filter "vec"` — все файлы в std/ где display name
  содержит "vec" (string match, не regex).
- `nova test foo/` — все test'ы в foo/.
- `nova test foo/ --filter "snapshot"` — test'ы в foo/ где **имя
  test-блока** содержит "snapshot" (test-name regex, не file path).

Для check filter применяется к file paths; для test — к test names.
Filter для check добавить только если есть use-case (deferred).

### R12. Project root boundary
Уже в R5 — не выходить за nova.toml root через symlinks. Дополнительно:
`--allow-outside-project` для CI который нарочно проверяет foreign
trees (например vendoring tools).

### R13. Parallelism = num_cpus by default
`-j N` (= `--jobs N`) override. `-j 1` для deterministic output в CI.
Per-file parallel (Nova не имеет cross-file resolve в bootstrap, поэтому
dependency-graph trivial: каждый файл независим).

После Plan 35 (cross-file resolve) добавится **topological ordering**:
зависимости checkнутся первыми, downstream — после. Сейчас — flat parallel.

### R14. Dependency graph (future-proof)
Per-file сейчас. После Plan 35 — построение import-graph, topological
sort, parallel-with-deps. Аналогично go-vet `-p N` с dependency awareness.

Сейчас это **не реализуем** — но архитектура должна оставлять место
(`Vec<CheckJob>` → `DepGraph<CheckJob>` будущий рефактор).

---

## Что готово в репо

- `nova check <file>` ([nova-cli/src/main.rs:282](../../nova-cli/src/main.rs))
  — single-file flow: parse → check_module_path → check_module.
- `nova test --tests-dir <dir>` — уже работает, но legacy form.
- 44 файла в `std/` (без `std/runtime/`).
- 20 файлов в `examples/`.
- `nova_codegen::types::check_module()` — единая точка type-check'а.
- `walk_nv()` в `test_runner.rs` — рекурсивный walker `.nv` файлов.
- `find_repo_root()` в `nova-cli/src/main.rs` — walk parents для `nova.toml`.
- `thread::scope` + jobs infrastructure в `run_all` — переиспользуем.

---

## Фазы

### Ф.1 — `nova check <path>` core (R1-R7)

В [nova-cli/src/main.rs](../../nova-cli/src/main.rs):

1. `Cmd::Check.paths: Vec<PathBuf>` (clap `num_args = 0..`).
2. Если empty → R4 walk-parents для nova.toml.
3. Для каждого path:
   - canonicalize (R5) → dedup set.
   - boundary check (R5/R12).
   - `is_file()` + не `.nv` → R6 exit=2.
   - `is_dir()` → `walk_nv()` + R3 implicit-excludes filter.
4. Aggregate `Vec<CheckJob>`.
5. Exit code R7 (0/1/2 разделены).

Acceptance:
- `nova check std/` — recurse, выводит PASS/FAIL.
- `nova check std/collections/vec.nv` — single-file.
- `nova check non_existent.nv` → exit=2, message "path not found".
- `nova check std/foo.txt` → exit=2, "not a Nova source".
- `nova check` без nova.toml в дереве → exit=2 (R4).
- `nova check ../outside` → exit=2 (R12).

### Ф.2 — output formats (R9) + flags

1. `--format human|json|short` (R9).
2. JSON-schema согласован с диагностическим API (`diag.rs::Diagnostic`
   уже имеет `Span` с byte_start/byte_end).
3. `--keep-going` / `--fail-fast` flags (R8).
4. `--include-runtime` / `--no-exclude <pat>` (R3 override).
5. `--allow-outside-project` (R12).

Acceptance:
- `nova check std/ --format json | jq .` валидный JSON-stream.
- Каждый diagnostic с byte_start/byte_end.
- `--keep-going` continues после fatal в одном path.

### Ф.3 — parallelism (R13)

1. `--jobs N` (R13). Default = num_cpus, 0 = num_cpus, 1 = sequential.
2. `thread::scope` + `mpsc` для aggregate.
3. `check_module` reentrancy audit — найти mutable state, копировать
   per-thread если есть. Сейчас type-checker иммутабельный по AST —
   safe.
4. Output ordering: при parallel сохранять deterministic order
   (sort-by-file-name перед finalize).

Acceptance:
- `nova check std/` на 44 файлах < 5 sec на 16-core (sequential ~30 sec).
- `--jobs 1` — deterministic same output как `--jobs N` после sort.

### Ф.4 — caching (R10)

1. `target/check-cache/` directory.
2. Key = blake3(file_content || version || flags).
3. Cache hit → skip parse + check (very fast PASS).
4. `--no-cache` flag.
5. Cache cleanup на mismatch nova-codegen version (version в key
   автоматически invalidates).

Acceptance:
- Second run `nova check std/` < 500ms (cache hits).
- `--no-cache` re-checks everything.
- После rebuild nova-codegen — cache invalidated automatic.

### Ф.5 — `nova test <path>` symmetric (Ф.1 features applied to test)

1. `Cmd::Test.paths: Vec<PathBuf>` positional (clap `num_args = 0..`).
2. `--tests-dir <dir>` deprecated — print warning, использовать как
   fallback если `paths` пуст. **Не удаляем** до next major.
3. Implicit excludes (R3) — те же.
4. Boundary check (R5/R12).
5. Path-filter ⊥ test-name-filter (R11): `--filter` остаётся
   test-name regex, path argument отдельно.

Acceptance:
- `nova test` — как раньше.
- `nova test nova_tests/concurrency/` — только concurrency.
- `nova test some_file.nv` — single file.
- `nova test --tests-dir foo` — works + warning.

### Ф.6 — pre-commit hook + scripts

1. `scripts/install-hooks.ps1` / `.sh` — устанавливают `pre-commit`
   который вызывает `nova check std/ examples/`.
2. **Не enforced** — opt-in через explicit `install-hooks`.
3. Hook содержит `--fail-fast` чтобы быстро падать.

### Ф.7 — Baseline + CI integration

1. После Ф.1-Ф.4 запустить `nova check std/` + `nova check examples/`.
2. Зафиксировать **baseline** в `docs/baselines/check.json`:
   ```json
   {"std":{"pass":N,"fail":M,"failed":["std/foo.nv","std/bar.nv"]},
    "examples":{"pass":K,"fail":L}}
   ```
3. CI delta check — fail если число PASS уменьшилось vs baseline.
4. `.github/workflows/check.yml`:
   ```yaml
   - run: nova check std/ examples/ --format json > check.json
   - run: nova-tools baseline-compare docs/baselines/check.json check.json
   ```

`nova-tools` — helper-cli, отдельная задача. До этого момента — manual diff.

---

## Acceptance criteria (overall)

- Все 14 R1-R14 закрыты или явно отложены с rationale (R14 → Plan 35).
- `nova check std/` exit 0 на чистом дереве (после Plan 34 fix'ов).
- `nova check examples/` exit 0 (после fix module-path в examples).
- `nova check non_existent.nv` exit **2** (не 1).
- `nova check std/foo.txt` exit **2**.
- `nova check ../outside` exit **2**.
- `nova check std/` < 5 sec на 16-core машине first-run.
- Second run < 500ms (cache).
- `--format json` — valid NDJSON, валидируется через `jq`.
- `--keep-going` собирает все errors из все paths.
- `nova test <path>` положительный flow без regression.
- `--tests-dir` deprecated с warning (не удалён).
- `find_repo_root()` boundary enforced — не выйти через symlink.

---

## Что НЕ входит

- **Исправление существующих fail'ов** в `std/` / `examples/` — Plan 34.
- **Cross-file resolve** (R14 dependency-graph parallel) — Plan 35.
- **Glob patterns** (`*.nv`, `**`) — shell expansion достаточен; добавим
  если real demand.
- **`./...` go-style suffix** — slash-style proven мейнстрим, не нужен.
- **Pattern-spec arguments** (`pkg::Foo`) — cargo style, не наш case.
- **`--fix` suggestions** — отдельная задача (Plan 14 diag work
  заложил базу).
- **LSP integration** — JSON output совместим с LSP, но dedicated server
  отдельный план.
- **examples runtime check** — Plan 28 nova run.
- **Workspace concept** (multi-package monorepo) — пост-Plan 03.

---

## Связь

- **Plan 14** — std blockers, частично fix существующие type-check fail'ы.
- **Plan 34** — fix конкретных std-файлов.
- **Plan 35** — cross-file resolve для глубокой validation + R14
  dependency-graph parallel.
- **Plan 24** — test runner infrastructure, переиспользуем `walk_nv`
  и parallel-pool.
- **Plan 28** — nova-cli, основа для Cmd::Check / Cmd::Test.

---

## Риски / Trade-offs

- **Parallel-safety типчекера.** `check_module` использует mutable
  state'ы? Если да — параллелизация даст false-positives/negatives.
  Mitigation: Ф.3 reentrancy audit; первый запуск с `--jobs 1`,
  проверить детерминизм; per-thread copy если нужно.
- **Cache корректность.** Blake3 коллизия → false PASS. Mitigation:
  version в key + explicit `--no-cache` всегда доступен. Production
  blake3 коллизии астрономически маловероятны (256-bit).
- **Stale baselines.** Если Ф.7 baseline зафиксирован и std/ деградирует,
  никто не заметит. Mitigation: CI delta check — fail если число PASS
  уменьшилось.
- **R3 implicit-excludes hard-coded.** Что если кто-то делает Nova-проект
  внутри `node_modules/` (unlikely но)? Mitigation: `--no-exclude
  node_modules` override; documented в man-page.
- **Canonicalization perf.** Каждый path canonicalize = syscall. На
  больших trees (10k+ files) заметно. Mitigation: только top-level paths
  canonicalize'ить; внутри walk_nv — relative paths.

---

## История

- 2026-05-12 — создан после user'ского запроса "как правильно тестировать
  stdlib & examples", выявившего gap в test infrastructure.
- 2026-05-12 — переименован из `36-stdlib-examples-coverage.md` после
  обсуждения positional vs `--recursive` flag.
- 2026-05-12 — переписан после cargo/go reality check. Добавлены
  R1-R14 production-grade requirements, exit code triplet (0/1/2),
  caching, JSON schema с byte-ranges, implicit excludes, symlink
  canonicalize/dedup, project root boundary, output mode dichotomy.
