# План 27: runner polish + alloc-backend selection

**Статус:** roadmap, не начат.
**Дата создания:** 2026-05-11.
**Тип:** инфраструктурный (DX + correctness coverage). Не меняет
семантику Nova, добавляет недостающее CI-coverage для memory backends
+ финальный polish test-runner'а до 100% production-grade.

---

## Тезис

Plan 26 закрыл runner до ~99% production-grade. Остался один
**архитектурный gap** который выявил юзер и который реально влияет на
correctness coverage: **test_runner всегда линкует `alloc.c` (Phase-0
plain malloc, no GC)**. В репо лежат готовые `alloc_boehm.c` (Boehm
GC) и `alloc_rc.c` (RC), но runner их игнорирует — значит ни Boehm,
ни RC **никогда не проходят CI**.

Плюс осталось 6 polish-задач из cargo-nextest / go test parity:
ф.5 test caching (отложенная), per-test EXPECT_TIMEOUT_MS,
verbose-capture, slow-tests report, --list для CI sharding,
NamedTempFile-pattern.

Plan 27 закрывает обе ветви.

---

## Часть A — alloc-backend selection (новое, основное)

### A.1 Проблема

Сейчас в `compiler-codegen/src/test_runner.rs` hardcoded:

```rust
let rt_alloc = opts.rt_dir.join("alloc.c");
// ...
c.arg(&rt_alloc);  // только alloc.c, всегда
```

`nova_rt/` содержит 3 backends с одинаковым API (`nova_gc_init`,
`nova_alloc`, `nova_retain`, `nova_release`, `nova_gc_shutdown`):

| Файл | Реализация | Спецификация |
|---|---|---|
| `alloc.c` | plain `malloc`, no free, no GC | Phase-0 default, fast compile |
| `alloc_rc.c` | reference counting (retain/release) | RC experiment |
| `alloc_boehm.c` | **Boehm conservative GC** (real GC) | D6 production target — managed heap |

Это значит:
- **Memory leaks не отлавливаются** в default-сборке (plain malloc
  растёт до OOM на long-running тестах).
- **Boehm никогда не tested в CI** — если регрессия в Boehm-specific
  code path'е, не узнаем.
- **RC backend тоже untested** — если кто-то правит `nova_retain`/
  `nova_release`, никаких regression-checks.
- **D6 spec говорит «managed heap by default»**, но реальность
  Phase-0 — это не D6, это лишь stub.

### A.2 Решение — `--alloc {malloc, boehm, rc}` flag

В `test_runner.rs` ввести enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocBackend {
    /// Phase-0 malloc, no free, no GC. Default для fast TDD.
    Malloc,
    /// Boehm conservative GC. D6-compliant managed heap. Production target.
    Boehm,
    /// Reference counting. Experimental.
    Rc,
}
```

В `build_command`: вместо `rt_dir.join("alloc.c")` использовать:

```rust
let alloc_file = match opts.alloc {
    AllocBackend::Malloc => rt_dir.join("alloc.c"),
    AllocBackend::Boehm  => rt_dir.join("alloc_boehm.c"),
    AllocBackend::Rc     => rt_dir.join("alloc_rc.c"),
};
```

CLI:
```bash
./run_tests.sh --alloc malloc     # default, fast
./run_tests.sh --alloc boehm      # D6-correct, real GC
./run_tests.sh --alloc rc         # RC experiment
```

### A.3 Boehm linkage — Windows + Linux

**Linux/macOS:** Boehm устанавливается через `apt install libgc-dev`
(Ubuntu/Debian), `dnf install gc-devel` (Fedora), `brew install bdw-gc`
(macOS). Clang/GCC автоматически найдёт `<gc.h>` через `pkg-config`
или через `/usr/include`. Линковка: `-lgc`.

**Windows:** вариантa два:
1. **vcpkg** (рекомендуется): `vcpkg install bdwgc:x64-windows-static`.
   `/I <vcpkg_installed/x64-windows-static/include>` + `gc.lib`.
2. **Submodule + build at runtime** (как мы делаем с libuv в Plan 22):
   `compiler-codegen/nova_rt/bdwgc/` submodule, lazy-build `bdwgc.lib`
   через cmake.

Auto-detection в `test_runner.rs`:
```rust
fn detect_boehm(repo_root: &Path) -> Option<BoehmConfig> {
    // 1. NOVA_BOEHM env-var (explicit path).
    // 2. vcpkg manifest в repo root.
    // 3. /usr/include/gc.h, /opt/homebrew/include/gc.h.
    // 4. compiler-codegen/nova_rt/bdwgc/include/gc.h (submodule).
}
```

Если Boehm не найден и `--alloc boehm` запрошен — clear error с
инструкциями setup.

### A.4 Test stratification — какие тесты на каком backend'е

Не все 150 тестов имеют смысл на всех 3 backends:
- `concurrency/deep_gc.nv` — **только boehm** (тестирует cycle collection).
- `concurrency/sleep_leak_check.nv` — **только boehm** (memory leak detection).
- `basics/*.nv` — **все backends** (basic correctness).

Решение — per-file аннотация:
```nova
// ALLOC_REQUIRES boehm
// ALLOC_EXCLUDES rc
test "cycle collection works" { ... }
```

D89 расширяется 6-м маркером `ALLOC_REQUIRES <backend>` / 7-м
`ALLOC_EXCLUDES <backend>`. Runner skip'ает с status `SKIP-ALLOC` если
не match.

### A.5 CI matrix

Будущий `.github/workflows/test.yml`:

```yaml
strategy:
  matrix:
    os: [ubuntu-latest, windows-latest, macos-latest]
    alloc: [malloc, boehm]   # rc — experimental, не в CI
steps:
  - run: ./run_tests.sh --alloc ${{ matrix.alloc }} --format junit --retries 2
```

Это 6 combinations = 6 GitHub Actions runners. Все тесты проходят на
malloc+boehm на 3 OS — это real production-grade signal.

### A.6 Acceptance criteria часть A

- ✅ `nova-codegen test-all --alloc boehm` — линкует `alloc_boehm.c` + libgc.
- ✅ `--alloc rc` — линкует `alloc_rc.c`.
- ✅ `--alloc malloc` (default) — текущее поведение.
- ✅ ALLOC_REQUIRES/ALLOC_EXCLUDES маркеры parsятся и применяются.
- ✅ Полный прогон на all 3 backends (Windows Clang) green кроме
  ALLOC_REQUIRES-skipped тестов.
- ⏸️ Linux/macOS smoke — отложено до access.

### A.7 Объём

~200 строк Rust в test_runner.rs (enum + parsing + detect_boehm +
build_command branch + skip-logic) + ~50 строк CLI args + 1-2 thousand
testfile annotations (постепенно по мере надобности).

---

## Часть B — runner polish (cleanup из Plan 26 review)

### Б.1 Ф.5 test caching (carry-over из Plan 26)

Hash-based test caching в `target/test-cache/<hash>/`. Cache key:
- SHA-256 от `.nv` source.
- mtime всех `nova_rt/*.c`.
- mtime `nova-codegen.exe`.
- Toolchain (clang/msvc/gcc) + mode (dev/release) + alloc backend.
- libuv enabled/disabled.

Cache hit → пропустить codegen + cc, сразу use cached `.exe`.

CLI: `--no-cache` (force rebuild), `--cache-dir <path>` (override).
Default: `--no-cache` для CI, опт-ин для local dev.

Объём: ~150 строк. **Sensitivity high** — false-positive cache hit
(когда invariant нарушен) даёт «PASS» когда должен FAIL. Поэтому
default disabled.

### Б.2 EXPECT_TIMEOUT_MS per-test marker

Сейчас все тесты используют global `--timeout`. Real-world test'ы
имеют разные SLA:
- `basics/literals` — 5s overhead на codegen+cc хватит.
- `concurrency/sleep_leak_check` — 15s budget на bench-loop.

Решение — D89 6-й маркер `EXPECT_TIMEOUT_MS <N>`:
```nova
// EXPECT_TIMEOUT_MS 30000
test "long bench" { ... }
```

Per-test override `opts.timeout` в `run_one`. CLI `--timeout` остаётся
fallback для тестов без маркера.

Объём: ~30 строк.

### Б.3 `--verbose` capture stdout PASS-тестов

Сейчас `Verbosity::Verbose` — маркер без эффекта (`TODO` comment в коде).
Реализация: в `run_one` сохранять `stdout`/`stderr` в `Outcome::Pass {
captured_stdout: Option<String>, captured_stderr: Option<String> }`.
В `--verbose` mode `print_summary` показывает captured для PASS-тестов
тоже.

Объём: ~30 строк + расширение Outcome enum.

### Б.4 Slow tests report

В конце прогона при `--format text` показывать top-10 самых медленных:
```
===== SLOWEST TESTS =====
  3.214s  concurrency/sleep_real_clock
  2.103s  std/checksums/fnv
  ...
```

Объём: ~20 строк (sort by elapsed, take 10, format).

### Б.5 `--list` для CI sharding

`nova-codegen test-all --list` — выводит все тесты по одному на строку,
без запуска. CI делит между runner'ами:
```bash
./run_tests.sh --list | awk "NR % 4 == 0" > shard-0.txt
./run_tests.sh --filter-from shard-0.txt --format junit
```

`--filter-from <file>` — новый flag, читает имена тестов из файла,
прогоняет только их.

Объём: ~40 строк.

### Б.6 Retry count в JUnit XML

Если `--retries 2` и тест PASS после retry — это **информация для CI
report**. JUnit поддерживает `<testcase retries="N">` через
`<system-out>`-comment либо custom attribute.

Решение — `Outcome::Pass { retries: u32 }`. В JUnit emit
`<system-out>retry-N times</system-out>` для retried tests.

Объём: ~15 строк.

### Б.7 NamedTempFile pattern

Сейчас `tmp_dir/t-<hash>/` создаётся явно + cleanup явно. Если worker
panic'ит до cleanup — directory orphan'ится. `tempfile` crate решает
через Drop trait — automatic cleanup.

Но: `tempfile` — extra dependency (~50KB binary). Для bootstrap compiler'а
лишнее. Можно сделать **own NamedTempDir** с Drop в ~30 строк:
```rust
struct TempSubdir(PathBuf);
impl Drop for TempSubdir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
```

Объём: ~30 строк.

### Б.8 Reproducible test ordering

Сейчас `inputs.sort_by(|a, b| a.0.cmp(&b.0))` — детерминированный
alphabetical. Иногда flaky тесты проявляются только на определённом
ordering (test1 leaks state, test2 fails). `cargo-nextest --seed N`
shuffles тесты для flakiness-detection.

CLI: `--shuffle [SEED]`. SEED= 0 — random (system time), N — explicit.

Объём: ~30 строк (xorshift PRNG + Fisher-Yates).

### Б.9 Acceptance criteria часть B

- ✅ `--cache-dir target/test-cache` — переиспользование между прогонами,
  10× speedup при unchanged sources.
- ✅ `EXPECT_TIMEOUT_MS 30000` в `.nv` overrides `--timeout`.
- ✅ `-v` показывает stdout PASS-тестов.
- ✅ Text-mode summary включает SLOWEST TESTS список.
- ✅ `--list` + `--filter-from` для CI sharding.
- ✅ JUnit `<testcase>` помечает retried tests.
- ✅ `--shuffle [SEED]` для flakiness detection.
- ✅ Tmp directory cleanup при panic worker'а.

---

## Trade-offs

### Boehm как extra dependency

vcpkg + bdwgc на Windows — non-trivial setup. Linux/macOS — easy
(`apt install`). Для CI на Windows придётся либо:
1. Указать vcpkg setup в `.github/workflows/test.yml` (cache vcpkg
   installed).
2. Использовать **GitHub Actions Windows image** который уже имеет
   vcpkg (актуальные ubuntu/windows-latest имеют vcpkg pre-installed).
3. Submodule + build (как libuv) — Plan 22 уже сделал шаблон.

Выбор: option 2 (рассчитываем на pre-installed vcpkg в CI image).
Local dev — manual `vcpkg install bdwgc`.

### Test-cache invariant — fragile

False-positive cache hit опасен (PASS когда должен FAIL). Поэтому
default `--no-cache`. Для local TDD рекомендация:
```bash
./run_tests.sh --cache-dir target/test-cache --filter <subset>
```

В CI — без cache.

### `--shuffle` vs детерминированный ordering

`cargo test` default ordering — детерминированный, `--shuffle` opt-in.
Соответствует разработке: «при отладке падающего теста хочу понимать
точный repro», не «каждый прогон случайный». Делаем то же.

### Backwards-compat для Outcome

Расширение `Outcome::Pass { detail, elapsed }` → `Outcome::Pass {
detail, elapsed, captured_stdout: Option<String>, retries: u32 }`
ломает existing callers (lib-тесты test_runner модуля). Mitigation —
сохранить старые поля, добавить новые, обновить lib-тесты в том же PR.

---

## План работ

### Часть A — alloc backends (4-5 часов)
1. **A.1** AllocBackend enum + CLI flag.
2. **A.2** detect_boehm + build_command branch.
3. **A.3** ALLOC_REQUIRES/EXCLUDES маркеры + skip-logic.
4. **A.4** Test annotations для concurrency/* и runtime/* (gc-sensitive).
5. **A.5** Full prod test на all 3 backends (Windows Clang).
6. **A.6** Linux/macOS — отложено до access.

### Часть B — runner polish (~6 часов)
1. **Б.1** Ф.5 test caching (sensitive — самое сложное).
2. **Б.2** EXPECT_TIMEOUT_MS marker.
3. **Б.3** verbose capture stdout.
4. **Б.4** Slow tests report.
5. **Б.5** --list + --filter-from.
6. **Б.6** JUnit retried tests.
7. **Б.7** TempSubdir Drop pattern.
8. **Б.8** --shuffle [SEED].

### Атомарные PR'ы

Часть A: A.1+A.2 → A.3+A.4 → A.5.
Часть B: Б.1 → Б.2+Б.3 → Б.4+Б.6 → Б.5 → Б.7+Б.8.

---

## Оценка

**1 неделя** работы (10-12 часов фокусированно):
- A: ~250 строк Rust + ~50 строк tests + ~1000 строк annotations.
- B: ~400 строк Rust + ~50 строк tests.
- Docs: README + test-conventions updates ~100 строк.

---

## Что разблокирует

- **D6-compliant testing.** Запуск всех тестов на real GC = первое реальное
  validation D6 spec'а («managed heap by default»). Сейчас D6 — claim
  без proof.
- **Memory leak detection.** Boehm имеет `GC_get_total_bytes()` —
  можно tracking'ить bytes-allocated per test, flag тесты с unexpected
  growth.
- **CI matrix.** GitHub Actions с (3 OS × 2 alloc) = 6 runners на каждый
  PR. Покрытие, которое cargo-test даёт Rust'у out-of-box.
- **Linux self-host roadmap.** При переходе Nova compiler'а на Nova
  (self-host), тестовая инфра уже cross-platform + multi-backend
  ready.
- **`--list` для CI sharding** — distribute тесты между runner'ами
  pri scale (5000+ тестов когда self-host).

---

## Связь с другими планами

- [Plan 26](26-test-runner-hardening.md) — foundation. Plan 27
  продолжает polish.
- [Plan 09](09-clang-migration.md) — Clang toolchain. Plan 27
  использует Clang для linker'а `-lgc` (Linux/macOS).
- [Plan 22](22-sleep-libuv-integration.md) — libuv submodule pattern.
  Plan 27 A.3 переиспользует pattern для bdwgc submodule (fallback
  если vcpkg недоступен).
- [Plan 25](25-production-readiness-roadmap.md) — production-readiness
  gap analysis. Plan 27 закрывает GC-related gap для test infrastructure.

---

## Ссылки

- `nova_rt/alloc.c` — Phase-0 malloc.
- `nova_rt/alloc_boehm.c` — Boehm GC implementation.
- `nova_rt/alloc_rc.c` — RC experiment.
- spec D6 — managed heap requirement.
- spec D89 — EXPECT_* marker conventions (расширяется ALLOC_REQUIRES).
- Boehm GC: https://github.com/ivmai/bdwgc
- `cargo-nextest`: https://nexte.st/ — reference design for retry,
  shuffle, list, shard features.
