# Plan 36: stdlib + examples coverage через `nova check --recursive`

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

### Новая subcommand-форма

```
nova check                 # single-file (back-compat) — обязателен <path>
nova check <file>          # single-file
nova check --recursive <dir>  # рекурсивно: все .nv в <dir>
nova check -r <dir>        # short flag
```

Семантика `--recursive`:
1. Walk `.nv` файлов через существующий `walk_nv()`.
2. Sort path для детерминизма.
3. Для каждого: parse + path-check + type-check.
4. Параллелизация: rayon thread pool, jobs = num_cpus (как в `run_all`).
5. Aggregate summary: `PASS: X  FAIL: Y` + список FAIL с first-error
   pointer (file:line:col).
6. Exit code = 0 если все PASS, 1 если ≥1 FAIL.

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

### Ф.1 — `nova check --recursive <dir>`

В [nova-cli/src/main.rs](../../nova-cli/src/main.rs):

1. Расширить `Cmd::Check` чтобы `path` принимал и file, и dir.
2. Добавить флаги:
   - `-r` / `--recursive`: bool.
   - `--jobs <N>` (0=num_cpus).
   - `--filter <substr>`.
   - `--skip <substr>` (повторяемый).
   - `--include-runtime`: bool, default false.
   - `--format text|json`, default text.
3. Логика:
   - Если `--recursive` или `path.is_dir()` → recursive flow.
   - Иначе — current single-file flow.
4. Реюзать `nova_codegen::test_runner::walk_nv()` (сделать `pub` если не).
5. Parallel через `std::thread::scope` (как `run_all`) или `rayon`.
6. Aggregate Result<Vec<CheckResult>> + final print + exit code.

Acceptance:
- `nova check --recursive std/` exit 0 на чистом дереве (после fix'а
  существующих regressions, если есть).
- `nova check --recursive examples/` exit 0.
- `nova check std/collections/vec.nv` (single-file) — back-compat.

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
