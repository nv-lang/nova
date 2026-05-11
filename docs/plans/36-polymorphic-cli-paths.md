# Plan 36: polymorphic CLI paths для `nova check` / `nova test`

> **Статус:** план, не начат.
> **Создан:** 2026-05-12.
> **Приоритет:** ВЫСОКИЙ — отсутствие automated coverage для stdlib/examples
> означает что регрессии в codegen/type-checker'е там обнаруживаются вручную
> через ad-hoc PowerShell loops.

---

## Зачем

Сейчас в репо три independent pipeline'а проверки:

1. **`nova test`** — гоняет `nova_tests/**/*.nv` через C-codegen → exe → exit
   code. Acceptance: PASS/FAIL.
2. **`nova check <file>`** — single-file type-check одного файла. Используется
   IDE / разработчиком вручную.
3. **stdlib (`std/`) + examples (`examples/`)** — нет automated coverage.
   Пользователь руками гоняет PowerShell loop:
   ```pwsh
   Get-ChildItem -Recurse -Path std -Filter *.nv | ... nova check $_
   ```

**Реальные проблемы:**

- **Регрессии в std/ обнаруживаются поздно.** Когда меняется codegen или
  type-checker, std-файлы могут сломаться. Обычные тесты в `nova_tests/`
  этого не ловят (они не импортируют std/).
- **examples/ не покрыты вообще.** README обещает рабочие примеры, но
  никто их не type-check'ает в CI/pre-commit. drift с current spec
  накапливается тихо.
- **No single-command pre-commit gate.** Разработчику нужно помнить про
  ручной loop — легко забыть.

Mainstream-прецеденты:
- **Rust:** `cargo check` рекурсивен по workspace по умолчанию.
- **Go:** `go vet ./...` / `go build ./...` — рекурсивно по пакетам.
- **Cargo:** `cargo build --workspace` проверяет все crate'ы.

Nova сейчас не имеет аналога — это **gap в инфраструктуре**.

---

## Что готово в репо

- `nova check <file>` ([nova-cli/src/main.rs:282](../../nova-cli/src/main.rs))
  — single-file flow: parse → check_module_path → check_module.
- 44 файла в `std/` (без `std/runtime/`).
- 20 файлов в `examples/`.
- `nova_codegen::types::check_module()` — единая точка type-check'а.
- `walk_nv()` в `test_runner.rs` — рекурсивный walker `.nv` файлов
  (используется `run_all`).

---

## Архитектура

### Polymorphic path argument

Прецеденты: `cargo check <path>`, `go vet ./...`, `clippy <path>`. Все
mainstream-CLI tools принимают path как позиционный аргумент и **сами**
определяют file/dir → recurse/single. Никаких `--recursive` флагов.

Применяем к Nova:

```
nova check                          # CWD, recurse
nova check std/                     # dir → recurse
nova check std/collections/vec.nv   # file → single
nova check std/ examples/           # multiple paths
```

Семантика:
1. Если path не указан → CWD.
2. Для каждого path:
   - `is_file()` → parse + check этого файла.
   - `is_dir()` → walk_nv() + parse + check каждого.
3. Sort path'ов для детерминизма.
4. Параллелизация: thread::scope, jobs = num_cpus.
5. Aggregate summary: `PASS: X  FAIL: Y` + список FAIL.
6. Exit code = 0 если все PASS, 1 если ≥1 FAIL.

### `nova test <path>` — симметрично

Аналогично переводим `nova test` на positional polymorphic path:

```
nova test                          # CWD/nova_tests/ (default project flow)
nova test std/                     # альтернативная директория
nova test some_file.nv             # single file (если есть test-блоки)
```

**Существующий `--tests-dir <dir>` флаг становится legacy.** Принимаем
back-compat-режим: если `--tests-dir` передан И позиционный path
отсутствует → используем legacy путь. Иначе игнорируем с deprecation
warning'ом. Удалить флаг — в следующем major.

### Output format

Default (text):
```
ok: std/collections/vec.nv
FAIL: std/crypto/jwt.nv:123:45: undefined identifier `expired_at`
ok: std/encoding/base64.nv
...

===== SUMMARY =====
PASS: 38  FAIL: 6
```

JSON output (`--format json`) — для CI:
```json
{"passed":38,"failed":6,"errors":[{"file":"std/crypto/jwt.nv","line":123,"col":45,"msg":"..."}]}
```

### Filters (как в `nova test`)

- `--filter <substr>` — по display name (relative path).
- Skip directories через `--skip <pattern>` (например `--skip /runtime/`).

`std/runtime/` всегда auto-skip (auto-gen из registry, не stdlib-код в
смысле library work). Под флагом `--include-runtime` можно включить.

---

## Фазы

### Ф.1 — `nova check <path>` polymorphic + parallel

В [nova-cli/src/main.rs](../../nova-cli/src/main.rs):

1. Расширить `Cmd::Check.path` — принимать `Vec<PathBuf>` (1+ позиционных
   аргументов; пустой = CWD).
2. Добавить флаги:
   - `--jobs <N>` (0=num_cpus).
   - `--filter <substr>`.
   - `--skip <substr>` (повторяемый).
   - `--include-runtime`: bool, default false (auto-skip `std/runtime/`).
   - `--format text|json`, default text.
3. Логика для каждого path:
   - `path.is_file()` → single-file check.
   - `path.is_dir()` → walk_nv() + check каждого.
4. Реюзать `nova_codegen::test_runner::walk_nv()` (сделать `pub` если не).
5. Parallel через `std::thread::scope` (как `run_all`).
6. Aggregate `Vec<CheckResult>` + final print + exit code.

Acceptance:
- `nova check std/` exit 0 на чистом дереве (после Plan 34).
- `nova check examples/` exit 0 (после fix module-path в examples).
- `nova check std/collections/vec.nv` (single-file) — back-compat.
- `nova check std/ examples/` — multi-path.

### Ф.1.5 — `nova test <path>` symmetric

В `Cmd::Test`:

1. Добавить `path: Vec<PathBuf>` (позиционный, optional, default
   `[<repo>/nova_tests]`).
2. `--tests-dir` deprecated — print warning, использовать как fallback
   если `path` пуст.
3. Логика: для каждого path — file → single test, dir → walk + run all.

Acceptance:
- `nova test` — как раньше (default flow).
- `nova test nova_tests/concurrency/` — только concurrency-тесты.
- `nova test --tests-dir foo` — работает с warning'ом.

### Ф.2 — pre-commit gate

`.git/hooks/pre-commit` hook (опционально-добавляемый через
`scripts/install-hooks.sh`/`.ps1`):
```
nova check --recursive std/ examples/ || exit 1
```

Hook **не enforced** — лишь предлагается через docs. CI делает
обязательным.

### Ф.3 — `nova check --recursive` интеграция в `nova test`

Опционально: флаг `nova test --check-stdlib` запускает recursive check
перед прогоном тестов. На fail — early exit без trying to build/run.

Это для **safety net**: разработчик ловит type-check регрессии в stdlib
до того как они проявятся в тестах (где они могли бы маскироваться
runtime-PASS'ом).

### Ф.4 — Baseline cleanup

После Ф.1 запустить `nova check --recursive std/` и `nova check --recursive
examples/`. Зафиксировать текущие fail'ы как **baseline** в
`docs/baselines/stdlib-check.json`. Цель — снижать fail-count со временем.

**Не правим бoizet существующие fail'ы в этом плане** — это отдельная задача.
План 36 только даёт *инструмент* для измерения.

### Ф.5 — CI integration

Добавить в `.github/workflows/ci.yml` (когда CI появится):
```yaml
- run: nova check --recursive std/ examples/
- run: nova test
```

---

## Acceptance criteria

- `nova check --recursive std/` работает, выдаёт PASS/FAIL summary.
- `nova check --recursive examples/` работает.
- `nova check <file>` (single-file) — без regression'ов.
- Parallel-execution: на 44-файле std/ выполняется < 5 sec на 16-core
  машине.
- `--filter` / `--skip` / `--include-runtime` работают как описано.
- JSON output корректен (валидируется через `jq`).
- Exit code 0 на all-pass, 1 на любой fail.

---

## Что НЕ входит

- **Исправление существующих failures** в `std/` (если они есть) — отдельная
  задача после baseline'а.
- **Cross-file resolution** (Plan 35 уже draft'ит это). `nova check` пока
  type-check'ает per-file без знания о imports из других модулей. Это
  ограничение принимается как baseline.
- **examples runtime check** (нужны test'ы внутри examples/) — для now
  только type-check.
- **LSP integration** — `nova check --json` совместим с LSP diagnostics
  формат, но dedicated LSP server — отдельный план.
- **Fix-it suggestions** — Plan 14 D-block'и для diagnostic'ов работают,
  но рекомендации `--fix` не предлагаются.

---

## Связь

- [Plan 14](14-stdlib-codegen-gaps.md) — std blockers, частично fix'ит
  существующие type-check fail'ы.
- [Plan 34](34-stdlib-typecheck-fix.md) — fix конкретных std-файлов
  (создан другим агентом).
- [Plan 35](35-cross-file-resolve.md) — cross-file resolve для глубокой
  validation.
- [Plan 24](24-cross-platform-test-runner.md) — test runner infrastructure,
  переиспользуем `walk_nv` и parallel-pool.

---

## Риски / Trade-offs

- **Parallel-safety типчекера.** `check_module` использует mutable
  state'ы? Если да — параллелизация даст false-positives/negatives.
  Mitigation: первый запуск с `--jobs 1`, проверить детерминизм; если
  есть mutable state — выделить per-thread copy.
- **Memory usage.** 44 + 20 = 64 файла одновременно в parsed AST'ах.
  Каждый ~10 KB → ~640 KB. Безопасно.
- **Stale baselines.** Если Ф.4 baseline зафиксирован и std/ деградирует,
  никто не заметит. Mitigation: CI delta check — fail если число PASS
  уменьшилось по сравнению с baseline.

---

## История

- 2026-05-12 — создан после user'ского запроса "как правильно тестировать
  stdlib & examples", выявившего gap в test infrastructure.
