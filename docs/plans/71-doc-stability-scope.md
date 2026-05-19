// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 71: `doc-check` stability-tier scope — opt-in vs implicit

> **Статус:** ✅ **ЗАКРЫТ 2026-05-19** (Ф.0-Ф.6: план + spec D127 + idiom page + manifest field + lint config/severity + fixture-skip + stdlib opt-in + 17 unit + 11 integration tests + smoke green).
> **Создан:** 2026-05-19. **Приоритет:** P1 (CI блокер для nova-doc workflow).
> **Трудоёмкость:** ~0.5 dev-day (фокусированная правка lint + manifest flag + tests).
> **Зависит от:** Plan 45 Ф.23.12 (style-guide lints), [D105](../../spec/decisions/09-tooling.md#d105-doc-атрибуты).

---

## Зачем

### Проблема

`nova doc --check` (Plan 45 Ф.23.12) emit'ит **error** `public-missing-stability`
на каждый exported item без `#stable` / `#unstable` / `#experimental` атрибута.
Правило применяется **унифицированно** ко всем `.nv` файлам, в том числе:

- `nova_tests/doc/fixtures/**/*.nv` — fixtures для doc-collector self-tests,
- `examples/**/*.nv` — учебные примеры в репозитории,
- `bench/corpus/**/*.nv` — benchmark корпус.

Результат: CI job `nova-doc` (`.github/workflows/nova-doc.yml`) падает на
`nova_tests/doc/fixtures/basic/sample.nv`:

```
doc-check: [public-missing-stability] nova_tests.doc.fixtures.basic.sample:
  public module has no stability tier (#stable / #unstable / #experimental)
doc-check: [public-missing-stability] nova_tests.doc.fixtures.basic.sample::abs:
  exported item has no stability tier (#stable / #unstable / #experimental)
doc-check: 2 issue(s)
```

Fixture структурно не "публичный API" — это тестовые данные для doc-collector'а.
Требовать stability annotations с них — методологическая ошибка: каждый новый
fixture файл будет рекуррентно падать в CI до момента его добавления каждому
exported символу.

### Industry baseline

| Язык | Где требуется stability tier |
|---|---|
| **Rust** | Только в stdlib (`libcore`, `libstd`, …) под `#![feature(staged_api)]`. User crates — никогда. Test fixtures внутри `libcore/tests/` — обычно `#[unstable]` или private. |
| **Swift** | Не требуется в принципе; implicit availability от target OS version. |
| **Kotlin** | `@RequiresOptIn` — только для opt-in API surface, не на каждом export. |
| **C# / .NET** | `[Obsolete]` опционально; нет аналога stability tier. |
| **Scala 3** | `@experimental` / `@apiStatus.Stable` опционально. |
| **Go / Python / OCaml / Haskell** | Convention в docstring/comment, без enforcement. |

**Вывод:** Rust stdlib — единственный прецедент **строгого** enforcement'а
stability tier'а. И там это применяется только к стандартной библиотеке,
включается явным feature flag'ом, и test fixtures освобождены.

### Текущее поведение Nova vs industry

Nova сейчас — **строже Rust stdlib**: применяет `public-missing-stability`
ко **всем** `.nv` файлам в проекте без opt-out механизма. Это блокирует
работу даже над тестовыми fixture'ами.

---

## Что делаем

### Резюме

Делаем enforcement stability tier'а **opt-in** через manifest flag
`enforce-stability = true` в секции `[lib]` `nova.toml`. По умолчанию
(если flag не задан) — `public-missing-stability` понижается в **warning**
а не error.

Дополнительно: для путей `nova_tests/`, `tests/`, `examples/`, `bench/`
правило skip'ается даже под `enforce-stability = true` (test fixtures
никогда не "public API").

Эта работа — analog Rust'овского `#![feature(staged_api)]` opt-in pattern'а:
строгость только когда явно включена, и только там где это семантически
оправдано.

### Решение

D127 (новый, в `spec/decisions/09-tooling.md`):

1. **Default:** lint `public-missing-stability` → severity = **warning**.
2. **Opt-in strict mode:** `nova.toml` → `[lib]` → `enforce-stability = true`
   повышает severity → **error**.
3. **Test/example exemption:** независимо от `enforce-stability`, файлы
   под путями (relative to manifest root):
   - `nova_tests/**` / `tests/**` — fixtures для self-test compiler'а,
   - `examples/**` — учебные примеры,
   - `bench/**` — benchmark corpus,
   
   получают **silent skip** правила (даже не warning). Эти контексты —
   не "stable API surface", требовать annotation'ов методологически неверно.
4. **Stdlib opt-in:** `std/nova.toml` приобретает `enforce-stability = true`
   (зафиксировано в Plan 71 Ф.3) — stability tier обязателен для всех
   `std.**` exports.

---

## Архитектура изменений

### Файлы

```
compiler-codegen/src/
  doc/lints.rs              ← lint_module / lint_item: severity logic
  doc/mod.rs                ← API: run_lints принимает LintConfig
  manifest.rs               ← Manifest.enforce_stability: bool field
  
nova-cli/src/main.rs        ← propagation manifest flag в lint pipeline

nova_tests/doc/fixtures/    ← unchanged (test exemption auto-skip'ает)

spec/decisions/09-tooling.md ← D127 (new)
docs/plans/45-nova-doc.md   ← обновить §11.5 №7 reference на D127
docs/idioms/stability-tiers.md ← новый: когда нужны, когда не нужны

std/nova.toml               ← добавить enforce-stability = true
```

### Manifest extension

```toml
# nova.toml
[lib]
src = "."
enforce-stability = true   # ← новый optional flag, default false
```

### LintConfig API

```rust
// compiler-codegen/src/doc/lints.rs
pub struct LintConfig {
    /// Если true — `public-missing-stability` = error.
    /// Если false — warning. Source: manifest `enforce-stability`.
    pub strict_stability: bool,
    /// Test/example/bench exemption (auto-derived от file paths).
    /// Если file под одной из этих директорий — `public-missing-stability` skip.
    pub fixture_dirs: Vec<PathBuf>,
}

pub fn run_lints(tree: &DocTree, config: &LintConfig) -> Vec<DocLintViolation> { ... }
```

`DocLintViolation` приобретает поле `severity: Severity { Error, Warning }`.
CLI `--check` exit 1 только если есть **error**-уровень нарушение. Warnings
печатаются в stderr, не блокируют CI.

### Test/example detection

Per-file: при сборе DocTree сохраняем абсолютный path. В `run_lints`
проверяем path relative to manifest root, match по prefix:

```rust
const FIXTURE_PREFIXES: &[&str] = &[
    "nova_tests/", "tests/", "examples/", "bench/",
];

fn is_fixture_path(rel: &Path) -> bool {
    let s = rel.to_string_lossy().replace('\\', "/");
    FIXTURE_PREFIXES.iter().any(|p| s.starts_with(p))
}
```

---

## Фазы реализации

### Ф.1 — Manifest field

**Файлы:**
- `compiler-codegen/src/manifest.rs`

**Изменения:**
- `Manifest` приобретает `pub enforce_stability: bool` (default `false`).
- `parse_manifest`: при `[lib]` секции читает `enforce-stability = true/false`,
  пишет в поле.

**Acceptance:**
- Unit test: `parse_manifest` корректно парсит `enforce-stability = true`.
- Unit test: при отсутствии flag — default `false`.
- Unit test: `enforce-stability = "garbage"` → ignored (False).

**Трудоёмкость:** ~30 мин.

### Ф.2 — Lint config + severity

**Файлы:**
- `compiler-codegen/src/doc/lints.rs`
- `compiler-codegen/src/doc/mod.rs`
- `nova-cli/src/main.rs`

**Изменения:**

1. `DocLintViolation` приобретает `pub severity: Severity` (enum
   `Error | Warning`).
2. `pub struct LintConfig` с `strict_stability` + `fixture_dirs`.
3. `run_lints(tree, config)` — signature change.
4. `lint_module` / `lint_item` для `public-missing-stability`:
   - Если path under fixture dir → skip entirely (никакого violation).
   - Иначе severity = `Error` если `config.strict_stability`, иначе `Warning`.
5. `nova-cli/src/main.rs --check` callsite:
   - Загружает manifest (`find_manifest` относительно file).
   - Строит `LintConfig` из flag.
   - Передаёт в `run_lints`.
   - Exit 1 только если есть `Severity::Error`.

**Acceptance:**
- `nova doc fixture.nv --check` (где `fixture.nv` в `nova_tests/`) → exit 0
  даже без `#stable`. Не печатает warning тоже.
- `nova doc user.nv --check` (user code, manifest без flag) → exit 0,
  печатает warning `public-missing-stability` в stderr.
- `nova doc std/foo.nv --check` (manifest имеет `enforce-stability = true`)
  → exit 1, error.

**Трудоёмкость:** ~1.5 часа.

### Ф.3 — Stdlib opt-in + spec amendment

**Файлы:**
- `std/nova.toml` — добавить `enforce-stability = true`.
- `spec/decisions/09-tooling.md` — D127 (новый).
- `docs/plans/45-nova-doc.md` — §11.5 №7 (rule 7) cross-link на D127.

**Изменения D127:**

```markdown
### D127. Stability-tier enforcement scope (Plan 71)

`#stable` / `#unstable` / `#experimental` (определены в [D105](#d105-doc-атрибуты))
не требуются на каждом exported item. Lint `public-missing-stability` (Plan 45
§11.5 №7) имеет следующий scope:

1. **По умолчанию** — severity = `warning`. CI не блокируется.
2. **`enforce-stability = true` в `nova.toml` `[lib]`** — severity = `error`.
   Opt-in для пакетов, обязующихся документировать stability публичного API
   (stdlib, library crates с обратной совместимостью).
3. **Fixture/example exemption** — независимо от flag, файлы под путями
   `nova_tests/**`, `tests/**`, `examples/**`, `bench/**` (relative to
   manifest root) skip'ают правило полностью.

**Почему:** test fixtures — не public API surface, требовать stability
annotation'ов с них методологически неверно. Industry baseline — Rust
stdlib enforces только под `#![feature(staged_api)]`, user crates никогда.
Nova консервативно расширяет: default `warning` (учит правильной conventions),
opt-in `error` (для production library crates).

**Cross-references:** [D105](#d105-doc-атрибуты), Plan 45 §11.5 №7, Plan 71.
```

**Acceptance:**
- `cd std && nova doc foo.nv --check` → exit 1 на отсутствие `#stable`.
- Spec amendment ревьюется на consistency с D105.

**Трудоёмкость:** ~1 час.

### Ф.4 — Docs idiom page

**Файлы:**
- `docs/idioms/stability-tiers.md` (новый).

**Содержание (skeleton):**

```markdown
# Stability tiers — когда нужны, когда не нужны

## TL;DR

- **Library crates с обратной совместимостью** (stdlib, public библиотеки):
  включить `enforce-stability = true` в `nova.toml`. Каждый exported item
  должен иметь `#stable` / `#unstable` / `#experimental`.
- **Application code** (binaries, internal tools): default OK. Lint warning
  напоминает, но не блокирует.
- **Test fixtures / examples / benchmarks**: пишите свободно. Auto-exempt.

## Когда использовать какой tier

| Tier | Когда |
|---|---|
| `#stable(since = "1.0.0")` | API готов к long-term commitment. Breaking change → semver major bump. |
| `#unstable(feature = "x")` | API в работе. Caller должен явно opt-in через `#cfg(feature = "x")`. |
| `#experimental(note = "...")` | Proof-of-concept. Может исчезнуть. Use-site emit'ит warning. |

## Best practices

- Module-level tier пропагируется на items (D105). Если 90% items одного
  tier'а — задайте на module, override на исключениях.
- `#stable(since = ...)` без version → допустимо, но лучше указать. CI
  changelog tooling (Plan 45 Ф.12) использует `since` для генерации.

## Anti-patterns

- Не ставьте `#stable` на experimental API "чтобы скрыть warning". Это
  обещание перед users — breaking change станет major release event.
- Не оборачивайте все exports в `#experimental` "на всякий случай". Lint
  `experimental-overuse` (Plan 45 future) поймает это.

## См. также

- [D105 doc-атрибуты](../../spec/decisions/09-tooling.md#d105-doc-атрибуты)
- [D127 stability-tier scope](../../spec/decisions/09-tooling.md#d127-stability-tier-enforcement-scope)
- [Plan 45 nova doc](../plans/45-nova-doc.md)
```

**Acceptance:**
- Doc rendering smoke-test (если есть CI для idiom pages).
- Внутренние links валидны.

**Трудоёмкость:** ~30 мин.

### Ф.5 — Tests

**Файлы:**
- `compiler-codegen/tests/doc_lints_scope.rs` (новый интеграционный тест).

**Coverage:**

1. `default_warning_non_strict` — manifest без flag, user .nv с `export fn foo`
   без `#stable` → lint emit Warning, не Error. exit 0.
2. `strict_error_with_flag` — manifest с `enforce-stability = true`, тот же
   код → emit Error. exit 1.
3. `fixture_exempt_default` — file под `nova_tests/`, без flag → no
   violation (silent skip).
4. `fixture_exempt_strict` — file под `nova_tests/`, с flag → still skip
   (test exemption берёт верх).
5. `examples_exempt` — file под `examples/` → skip.
6. `bench_exempt` — file под `bench/corpus/` → skip.

**Acceptance:**
- 6/6 PASS под `cargo test --test doc_lints_scope`.

**Трудоёмкость:** ~1.5 часа.

### Ф.6 — CI workflow update + smoke

**Файлы:**
- `.github/workflows/nova-doc.yml` — без изменений (текущий setup
  использует `nova_tests/doc/fixtures/**/sample.nv`, после Ф.2 auto-skip'ается).

**Smoke verification:**

```bash
# Под current behavior:
./nova-cli/target/release/nova doc nova_tests/doc/fixtures/basic/sample.nv --check
# Expected: exit 0, no `public-missing-stability` warnings printed
```

**Acceptance:**
- Local: smoke runs green.
- CI: nova-doc workflow → green on next PR.

**Трудоёмкость:** ~15 мин.

---

## Acceptance Plan 71 (целиком)

После Ф.1-Ф.6:

1. `nova_tests/doc/fixtures/**/*.nv` — без изменений, доходят до nova-doc
   workflow зелёным.
2. `enforce-stability = true` в `nova.toml [lib]` поднимает severity до
   error для file под `[lib].src/`, но НЕ для путей под fixture-dirs.
3. Default behavior — warning (учит), не error (не блокирует).
4. D127 в spec, cross-link с D105 и Plan 45 §11.5 №7.
5. `docs/idioms/stability-tiers.md` объясняет когда / какой tier.
6. 6 интеграционных тестов в `compiler-codegen/tests/doc_lints_scope.rs`.

---

## Не входит в Plan 71

- **Per-file override** (`#allow(public-missing-stability)`) — Plan 71.A.
- **`--strict` CLI flag** для `nova doc` overriding manifest setting —
  уже существует частично (`--strict` для warnings → errors), интеграция
  с stability scope — отдельная sub-задача если потребуется.
- **Lint config из CLI** (`--lint-deny=public-missing-stability`) — Plan
  45.A (CLI lint configuration).
- **Rename "public" → "exported"** в rule name — backward compat broken
  change, не делаем сейчас.

---

## Срок

~0.5 dev-day.

---

## Open questions

1. **Default state на `enforce-stability` для root `nova.toml`?**
   - Текущий план: false (warning только).
   - Альтернатива: на верхнем (`nova/nova.toml`) задаём `true` чтобы
     workspace-wide enforce. **Решение:** оставляем false; pkg `nova_tests`
     и др. — explicit opt-in если хотят.

2. **Fixture exemption — hardcoded paths или manifest config?**
   - Текущий план: hardcoded `nova_tests/ tests/ examples/ bench/`.
   - Альтернатива: `nova.toml [lib] fixture_dirs = ["custom/path"]`. Override.
   - **Решение:** hardcoded для V1. Manifest-config — V2 если будет user
     запрос.

3. **Warning для `experimental-overuse`?**
   - Idiom page упоминает, но не реализовано.
   - **Решение:** out of scope Plan 71. Будущий Plan 45.A item.

---

## Implementation log

### 2026-05-19 — Ф.0-Ф.6 implementation closure

**Worktree:** `nova-p71` (branch `plan-71`, base `main` 02b487a53d1).

**Pre-implemented (commit `db76aaf8386` 2026-05-19 morning):**
- Plan 71 doc itself (this file)
- D127 в `spec/decisions/09-tooling.md` (+73 LOC)
- `docs/idioms/stability-tiers.md` (new, 118 LOC)
- `docs/plans/README.md` entry
- `docs/plans/45-nova-doc.md` §11.5 №7 cross-link к D127

**Implementation сегодняшней сессии (3 commits):**

1. **Ф.1 — `Manifest.enforce_stability` field + parser** (commit `a533d772695`)
   - `compiler-codegen/src/manifest.rs`: новый `pub enforce_stability: bool`
     field; парсер `[lib] enforce-stability = true/false`; conservative
     parsing (anything кроме literal `true` → false); `parse_manifest`
     повышен до `pub` для использования из nova-cli fallback path +
     integration tests
   - 6 unit-tests acceptance: enforce_stability_true / _default_false /
     _garbage_ignored / _explicit_false / _trailing_comment /
     _wrong_section_ignored — все PASS

2. **Ф.2 + Ф.5 + Ф.6 — lint config/severity + fixture-skip + tests**
   (commit `b8640610283`)
   - `compiler-codegen/src/doc/lints.rs`:
     - new pub enum `Severity { Error, Warning }` + `as_str()`
     - new pub struct `LintConfig { strict_stability, fixture_dirs }`
       + Default / `with_defaults` / `default_fixture_dirs` /
       `is_fixture_module`
     - `DocLintViolation` приобретает `pub severity: Severity`
     - `run_lints(tree, &LintConfig)` — signature change
     - Rule 7 (`public-missing-stability`): severity = Error если
       `strict_stability`, else Warning; skip полностью если path под
       fixture_dirs
     - Other rules: `Severity::Error` (historical default preserved)
   - `compiler-codegen/src/doc/mod.rs`: pub use re-exports +
     `run_lints(tree, &LintConfig)` signature
   - `compiler-codegen/src/doc/doctree.rs`: `DocModule` приобретает
     `pub source_paths: Vec<PathBuf>`
   - `compiler-codegen/src/doc/collector.rs`: `collect_one` populates
     `source_paths` из `module.peer_files`
   - `compiler-codegen/src/doc/doctests.rs`: test fixture updated
   - `compiler-codegen/src/doc/watch_cache.rs`: seed peer_files[0] во
     время cache miss/stale path (`nova doc --watch`)
   - `nova-cli/src/main.rs`:
     - new `build_lint_config_for(path)`: loads manifest → maps
       enforce_stability → strict_stability
     - `cmd_doc_check(tree, format, &LintConfig, strict)` — signature
       change; severity-aware exit code
     - new `ensure_entry_peer_path`: seed peer_files[0] для doc MVP
       single-file + workspace flows (parser leaves peer_files empty)
     - все 3 cmd_doc_check call-sites updated (single-file / workspace /
       watch)
     - `cmd_doc_watch` signature gains `strict: bool`
   - `compiler-codegen/tests/doc_lints_scope.rs` (new, 11 tests):
     - Plan 71 Ф.5 acceptance (6 required): default_warning_non_strict
       / strict_error_with_flag / fixture_exempt_default /
       fixture_exempt_strict / examples_exempt / bench_exempt
     - +5 robustness extras: tests_dir_exempt /
       windows_separator_fixture_detection /
       empty_source_paths_not_fixture / stdlib_path_lints_module_level
       / other_rules_remain_error_severity
     - все 11/11 PASS

3. **Ф.3 stdlib opt-in — `std/nova.toml`** (commit `6391ec7df5c`)
   - `enforce-stability = true` flag — stdlib обязан документировать
     stability tier на каждом export

**Ф.6 smoke verification:**
- `nova doc nova_tests/doc/fixtures/basic/sample.nv --check` →
  `doc-check: ok (3 item(s), 0 link(s))`, exit 0 (no warnings printed)
- `nova doc std/concurrency/timer.nv --check` →
  `doc-check: [public-missing-stability] concurrency.timer: ...`,
  exit 1 (stdlib enforce-stability = true → Error severity)

**Regression check:**
- `cargo test --lib doc::` → 154 passed / 0 failed
- `cargo test --test doc_lints_scope` → 11 passed / 0 failed
- `cargo test --lib manifest::` → 6 passed / 0 failed
- Pre-existing failure `parser::tests::fn_static_method` — не относится
  к Plan 71 (existed on `main` before changes).
