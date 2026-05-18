// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 58: Cross-toolchain CI matrix (Clang / MSVC / GCC × 3 OS)

> **Создан 2026-05-16 EOD.** **Ревизия v2 2026-05-18:** дополнен до
> production-grade (был proposal-stub). Закрывает acceptance item из
> Plan 55 «Cross-toolchain MSVC verify» + блокирует hardening Plan 65
> Ф.14 + Plan 67 Ф.4 (оба требуют matrix verification).
>
> **Приоритет:** P1 (был P2 — повышен: 2 plans уже упёрлись в
> отсутствие infra).
> **Трудоёмкость:** ~5-7 dev-days (was 2 — недоoценено; production CI
> требует cache + diff infra + baselines, не только matrix файл).

---

## Контекст

Nova bootstrap supports три C-compilers (Plan 09 closure):
- **Clang** — default (Plan 09 Ф.5: 10-15% perf improvement vs MSVC).
- **MSVC** — fallback на Windows если Clang недоступен.
- **GCC** — Linux/macOS default.

Plan 55 verification — **только Clang**. MSVC + GCC paths не проверены
после Plan 55 changes. Аналогично:
- Plan 65 (timer-channel API) Ф.14 stress — не может запуститься без matrix
- Plan 67 (println overload) Ф.4 cross-toolchain — same blocker
- Plan 27 GC switch Ф.5 Linux smoke — ждёт CI

Latent bugs возможны и **накапливаются** с каждым плановым sweep'ом:
- MSVC strict mode (`/W4`) может flag код который Clang accepts
- Endianness / aliasing assumptions
- Boehm GC linking per-OS (vcpkg / libgc-dev / brew gc) — разная setup
- libuv linkage (ABI break при mismatched версиях)
- `static inline` semantics — MSVC vs GCC расхождения
- Float-精度 (`-ffast-math` differences)

---

## Industry baseline reference

| Project | What they do | Что взять |
|---|---|---|
| **Rust** | tier-1 (CI gate) / tier-2 (no-CI promise) / tier-3 (best-effort). 200+ targets matrix через bors+Crater | Tier model для honest scope |
| **Go** | builders.golang.org runs all platforms every commit; dashboards | Per-OS dashboard idea |
| **CPython** | buildbot.python.org × N configs; "first failure" alert | Email/slack on first regression |
| **Zig** | drone.io matrix + sanitizer builds | Sanitizer integration |
| **LLVM** | `check-all` per backend; opt-in expensive tests via `-DLLVM_ENABLE_EXPENSIVE_CHECKS` | Quick-vs-full split |

**Plan 58 цель:** не догонять Rust полностью (200 targets — overkill для bootstrap), но **гарантировать tier-1 для 3 OS × 3 toolchains** + honest tier-2/tier-3 для остального.

---

## Tier model (honest scope)

| Tier | OS × toolchain | Gate | Failure ownership |
|---|---|---|---|
| **Tier 1** (must work) | Linux/Clang, Linux/GCC, Windows/Clang, Windows/MSVC, macOS/Clang | PR-blocking gate | Maintainer immediate fix |
| **Tier 2** (best-effort) | macOS/GCC (Homebrew), Linux/MSVC (via wine? — no), Windows/GCC (mingw) | Nightly only, не PR-blocking | Triaged per failure |
| **Tier 3** (community) | BSD, Solaris, ARM Linux (raspberry), Apple Silicon native | No CI; user-reported | Community PRs welcome |

**Note:** Apple Silicon ARM64 — Tier 1 как только GitHub Actions добавит
free macos-arm64 runners (M1 macOS-14+ runner). Сейчас macOS-12/13 = x86_64.

---

## Архитектурные решения

### AD1. Один matrix workflow, не три

`.github/workflows/cross-toolchain.yml` — single source of truth. Не
плодить per-platform файлы (дублирование cache config, failure handling).

### AD2. Caching обязателен — иначе CI cost killer

Без cache: 3 OS × 3 toolchains × ~15 min build + 10 min test = **~225
runner-min per PR**. С cache: ~60 runner-min.

**Что кешируется:**
- **Rust** (Cargo): `target/`, `~/.cargo/registry/`, `~/.cargo/git/`
  через [Swatinem/rust-cache](https://github.com/Swatinem/rust-cache).
  Key: `Cargo.lock` hash + toolchain + OS.
- **vcpkg** (Windows Boehm GC + Z3): `vcpkg/installed/`, `vcpkg/buildtrees/`
  через [actions/cache](https://github.com/actions/cache).
  Key: `vcpkg.json` hash + os.
- **apt-cache** (Linux): `/var/cache/apt/archives/` через apt-cache-action.
- **brew-cache** (macOS): `~/Library/Caches/Homebrew/` через actions/cache.
- **Z3 binary** (libz3): cached separately, рестор быстрее чем
  пересборка (vcpkg Z3 build = 5-8 min).

### AD3. Deterministic codegen invariant

**Inviolable rule:** same `.nv` input → byte-identical `.c` output across
toolchain (compiler-codegen Rust code is the same; output только зависит
от source, не от target toolchain).

**Verification:**
- Phase 3.5: после сборки matrix cells, выкачать `.c` файлы из 3+
  toolchains, `sha256sum` сравнить. Diff = **automatic CI fail**.
- Защищает от accidental `cfg(target_os)` в codegen output (не должно
  быть; cfg = в runtime headers).

### AD4. Per-toolchain baseline + drift tracking

Каждый toolchain имеет **expected test count** stored в repo:
`ci/baseline-{os}-{toolchain}.json`. Содержит:
```json
{
  "pass": 705,
  "fail": 0,
  "skip": 44,
  "xfail": ["msvc_strict_warning_legacy_pattern"],
  "wall_clock_ms_p50": 142000,
  "compiler_version": "MSVC 19.39"
}
```

CI cell сравнивает: если PASS ↓ vs baseline → fail. Если PASS ↑ vs
baseline → require update baseline в same PR (gate против silent
test removal).

`xfail` list — tests которые ожидаемо падают на этом toolchain (документированные incompatibilities). Если xfail test **passes** —
fail (списать xfail label).

### AD5. Sanitizer pass (Tier 1, separate job)

ASan/UBSan/TSan — обычно дорогие и медленные. Дать им **отдельный job**,
один toolchain (Linux/Clang — самый стабильный sanitizer support):

| Sanitizer | What | When |
|---|---|---|
| ASan | use-after-free, heap overflow | PR-blocking (~+50% wall-clock) |
| UBSan | UB (overflow, alignment) | PR-blocking (~+10% wall-clock) |
| TSan | data races (M:N runtime!) | PR-blocking on concurrency-relevant changes; nightly otherwise |
| MSan | uninit reads | Nightly (slow, gcc fragile) |

Plan 44.1 уже имеет TSan/ASan/UBSan stress tests — переиспользуем.

### AD6. Boehm GC setup per-OS (build-deps)

Без libgc сборка падает на link. CI должен установить:
- **Linux** (Ubuntu): `apt-get install libgc-dev` (commit `fcbbb04d19d`
  уже сделал это для contracts-z3.yml — расширить на matrix).
- **macOS**: `brew install bdw-gc`.
- **Windows**: vcpkg `bdwgc` (уже vendored в проекте, но CI vcpkg cache
  через AD2).

Verify в matrix prelude step `nova check --gc=boehm <smoke-file>`.

### AD7. Compiler version pinning

| Toolchain | Pinned version | Rationale |
|---|---|---|
| Clang | LLVM 18 (latest stable as of 2026-05) | Plan 09 default; LLVM 17+ has improvements для bootstrap codegen |
| MSVC | VS 2022 17.10+ (cl.exe 19.40+) | Project minimum; older = no `__VA_OPT__` |
| GCC | 13 (Ubuntu 24.04 default) | C17 support, libstdc++-13 |

Pin через `.github/workflows/cross-toolchain.yml` setup step. Не
использовать "latest" — surprise breakages.

Ежеквартально pinning review (отдельный план — не Plan 58 scope).

### AD8. Local reproducibility — `nova ci-matrix --local`

Дев должен уметь запустить ту же матрицу локально (хоть и ограниченно):
```
nova ci-matrix --local
  → пробует Clang ✅, MSVC ❌ (not installed), GCC ❌ (Windows host)
  → запускает только available toolchains
  → output identical CI format (JSON + summary)
```

Реализация: `nova-cli/src/bin/ci_matrix.rs` (~300 LOC, wraps test
runner).

### AD9. Failure triage workflow

При красном matrix cell:
1. Job log includes **`@maintainer-handle ATTENTION:` tag** в Annotations
   (GitHub displays на top of PR).
2. First failure on main → email/slack notify (configurable, opt-in).
3. Auto-create issue если же failure persists 3 consecutive nightly runs
   (use `peter-evans/create-issue-from-file`).

### AD10. Plan 57 bench integration

Cross-toolchain bench для **performance regression**:
- Run `nova bench corpus run --quick` на каждом matrix cell.
- Compare wall-clock per toolchain vs baseline (`ci/bench-baseline-{cell}.json`).
- **Regression gate:** > 10% slower than baseline → CI fail (configurable
  threshold per cell).
- Bench results uploaded to bench-history (Plan 57.A.1 orphan branch).

### AD11. Skip-patterns для PRs

Не каждый PR trogает codegen. Чтобы не платить full matrix:
- PR touches только `docs/**`, `**/*.md`, `nova_tests/**` → skip codegen-changing matrix; runs только docs lint + test runner smoke.
- PR touches `compiler-codegen/src/codegen/**` → **full matrix mandatory**.
- PR touches `std/**` only → matrix без MSVC strict-warning job (stdlib tests = same codegen output).

Heuristics через `dorny/paths-filter@v3`.

### AD12. Honest excluded — что НЕ в scope

- **MinGW** на Windows (legacy, мало users)
- **icc** (Intel C compiler)
- **TinyCC / Cosmopolitan**
- **Cross-compilation** (host ≠ target) — native-only
- **WSL** на Windows (Native MSVC + WSL Ubuntu = duplicate Linux job)
- **Apple Silicon native runners** (когда GitHub Actions добавит free
  M-series runners → Plan 58.1)

---

## Requirements

### Core matrix infrastructure

**R1.** `.github/workflows/cross-toolchain.yml` — single file с 5 jobs:
   - `linux-clang`, `linux-gcc`, `windows-clang`, `windows-msvc`, `macos-clang`
   - + 3 опциональных jobs: `sanitizers` (linux-clang), `bench` (linux-clang),
     `deterministic-codegen-diff` (post-aggregation)

**R2.** Каждый job:
   - Setup toolchain (action или manual install)
   - Restore caches (AD2)
   - Install OS-level deps (libgc, libz3, libuv)
   - Build nova-cli + compiler-codegen
   - Run `nova test` full suite (release)
   - Upload artifacts (test JSON output, generated `.c` для AD3 diff)
   - Update baseline JSON в same PR if PASS count changed (AD4)

**R3.** **PR-blocking** gate: все Tier-1 jobs (5 cells) **must** PASS
для merge.

**R4.** **Fail-fast: false** — collect all matrix results (не fail
после первого).

### Caching (AD2)

**R5.** Cache hit-rate targets:
   - Cold (first run): 25+ min per cell (acceptable)
   - Warm (cache hit): ≤ 8 min per cell (must)
   - 90+% cache hit rate post-stabilization

**R6.** Cache invalidation deterministic — key includes `Cargo.lock`,
`vcpkg.json`, toolchain version. Никаких `latest` keys.

### Deterministic codegen (AD3)

**R7.** Aggregation job `deterministic-codegen-diff` после matrix:
   - Download `.c` artifacts из 3+ cells (Clang Linux, MSVC Win, GCC Linux)
   - Run `sha256sum` matrix; diff = fail
   - **0 mismatches** required

### Baselines (AD4)

**R8.** `ci/baseline-{os}-{toolchain}.json` для всех Tier-1 cells:
   ```json
   {"pass": N, "fail": 0, "skip": M, "xfail": [...], "wall_clock_ms_p50": K, "compiler_version": "..."}
   ```

**R9.** Drift checker (Rust binary `ci/baseline_diff.rs`):
   - PASS ↓ → fail
   - PASS ↑ → require baseline update в same PR
   - xfail test now passing → require xfail removal
   - wall_clock > 1.5× baseline → warn (perf regression)

### Sanitizers (AD5)

**R10.** Separate job `sanitizers-linux-clang`:
   - ASan + UBSan на all tests
   - TSan на all `nova_tests/concurrency/**` + `nova_tests/plan65/**` (timer
     concurrent) + Plan 44.1 stress
   - PR-blocking
   - Cache: same as linux-clang

### Boehm GC (AD6)

**R11.** Pre-test step `nova check --gc=boehm nova_tests/gc/smoke.nv`
для verify Boehm setup корректен per OS. Failure = misconfiguration, не
test bug.

### Compiler version (AD7)

**R12.** Pinned versions в workflow `setup-toolchain` action. Bump через
отдельный PR с changelog «expected per-cell delta».

### Local reproducibility (AD8)

**R13.** `nova ci-matrix --local` бинарь:
   - Detects available toolchains (`which clang`, `where cl.exe`, etc.)
   - Runs available subset
   - Output JSON identical to CI format (для `git diff` comparison
     с CI logs)
   - Exit code 0 если all-available PASS

### Failure triage (AD9)

**R14.** Job annotations с maintainer-handle tag.
**R15.** Nightly notify (opt-in via repo settings; default off для
contributors).
**R16.** Auto-issue после 3 consecutive nightly fails (deterministic
threshold).

### Bench integration (AD10)

**R17.** Per-cell bench-corpus run (Plan 57).
   - Baseline в `ci/bench-baseline-{cell}.json`
   - Regression gate: > 10% slower → fail (configurable)
   - Upload history (Plan 57.A.1)

### Skip-patterns (AD11)

**R18.** `dorny/paths-filter@v3` step:
   - Codegen-changing files (`compiler-codegen/src/codegen/**`,
     `compiler-codegen/nova_rt/**`, `compiler-codegen/src/types/**`) →
     full matrix
   - Docs-only / tests-only → reduced subset (1 cell, fast)
   - Mixed → full matrix safe default

### Documentation

**R19.** `docs/toolchain-matrix.md`:
   - Tier model + supported list
   - Per-cell expected behaviour
   - Local-repro instructions (`nova ci-matrix --local`)
   - Known incompatibilities + workarounds
   - Compiler version pinning policy
   - How to add new toolchain
   - How to triage failure

**R20.** README badge: cross-toolchain status (live от GitHub Actions).

---

## Phases

### Ф.0 — Audit + readiness (½ day)

- [ ] Inventory current `.github/workflows/*.yml` (что уже есть; `contracts-z3.yml` + `nova-doc.yml` known).
- [ ] Audit codegen для `target_*` cfg в выводе (не должно быть; AD3).
- [ ] Confirm Plan 57 bench history infra working (Plan 65 Ф.0 уже
      проверял).
- [ ] List all OS-level deps (libgc, libz3, libuv, vcpkg packages).
- [ ] Identify XFAIL candidates (existing test failures on specific toolchains).

**Acceptance:** baseline audit doc + dep list ready.

### Ф.1 — Single-cell smoke (½ day)

- [ ] `linux-clang` job в new workflow `cross-toolchain.yml`.
- [ ] Caching working (rust-cache).
- [ ] `nova test` PASS на CI.
- [ ] Baseline JSON committed (`ci/baseline-linux-clang.json`).

**Acceptance:** один cell зелёный, infra working.

### Ф.2 — All Tier-1 cells (1 day)

- [ ] Add 4 cells: linux-gcc, windows-clang, windows-msvc, macos-clang.
- [ ] Per-cell deps install (Boehm, libuv per OS).
- [ ] Baselines committed (4 файла).
- [ ] Все 5 cells PASS на main branch.

**Acceptance:** matrix зелёный на пустом diff PR.

### Ф.3 — Sanitizers + bench (1 day)

- [ ] `sanitizers-linux-clang` job (ASan/UBSan/TSan per scope).
- [ ] `bench-linux-clang` job (Plan 57 corpus).
- [ ] Bench baselines committed.

**Acceptance:** sanitizer clean; bench regression gate working.

### Ф.4 — Deterministic codegen diff (½ day)

- [ ] `deterministic-codegen-diff` aggregation job.
- [ ] Upload `.c` artifacts из 3 cells.
- [ ] sha256sum matrix verify.

**Acceptance:** 0 mismatches; if mismatch found — это **bug** в Plan 58
discovery → отдельный issue.

### Ф.5 — Drift checker + baseline gate (½ day)

- [ ] `ci/baseline_diff.rs` бинарь (или shell script if simpler).
- [ ] Wired into matrix as post-step.
- [ ] Test: artificially скрытить test → CI fails с clear message.
- [ ] Test: add test → CI requires baseline update.

**Acceptance:** baseline drift caught reliably.

### Ф.6 — Skip-patterns + caching tuning (½ day)

- [ ] `paths-filter` step (AD11).
- [ ] Verify cache hit-rate ≥ 90% после 5 runs warmup.
- [ ] Reduced subset работает для docs-only PR.

**Acceptance:** average CI cost < 90 runner-min per PR (vs naive 225).

### Ф.7 — Local repro tool (½ day)

- [ ] `nova-cli/src/bin/ci_matrix.rs` (~300 LOC).
- [ ] Toolchain detection.
- [ ] Output identical to CI JSON.
- [ ] Doc snippet в `docs/toolchain-matrix.md`.

**Acceptance:** `nova ci-matrix --local` работает на dev machine.

### Ф.8 — Failure triage workflow (½ day)

- [ ] Job annotations с maintainer-handle.
- [ ] Nightly notify config (opt-in env var в repo secrets).
- [ ] Auto-issue creator (3-consecutive-fails threshold).
- [ ] Test all paths (force-fail PR; verify alerts).

**Acceptance:** failure paths working.

### Ф.9 — Documentation + README badge (½ day)

- [ ] `docs/toolchain-matrix.md` (R19 contents).
- [ ] README badge (cross-toolchain status from GH Actions).
- [ ] Update Plan 58 status — closed.
- [ ] Unblock Plans 65 Ф.14, 67 Ф.4 (notify через cross-references в plan-doc).

**Acceptance:** docs published; downstream plans unblocked.

### Ф.10 — Stabilization (½ day) **+ post-merge monitoring**

- [ ] Watch first 5-10 real PRs.
- [ ] Tune false-positives (flaky tests → mark flaky, retry budget).
- [ ] Cache hit-rate confirm ≥ 90%.
- [ ] Wall-clock confirm ≤ 60 min per PR end-to-end.

**Acceptance:** 1 неделя зелёного PR-stream без интervention.

---

## Acceptance criteria (production-grade)

### Functional

- [ ] 5 Tier-1 cells (linux-clang, linux-gcc, windows-clang, windows-msvc, macos-clang) PASS на main.
- [ ] Sanitizers (ASan/UBSan/TSan) — clean.
- [ ] Bench regression gate — no > 10% wall-clock regression.
- [ ] Deterministic codegen — 0 sha256 mismatches across toolchains.

### Operational

- [ ] PR-blocking gate работает.
- [ ] Cache hit-rate ≥ 90% post-stabilization.
- [ ] Wall-clock ≤ 60 min per PR (warm cache).
- [ ] Drift checker катит — PASS ↑/↓ требует baseline update в PR.
- [ ] Skip-patterns: docs-only PR → reduced subset.

### Triage

- [ ] Failure annotations visible.
- [ ] Nightly notify opt-in working.
- [ ] Auto-issue после 3-consecutive fails.

### Documentation

- [ ] `docs/toolchain-matrix.md` published.
- [ ] README badge live.
- [ ] Plans 65 + 67 unblock notes added (cross-reference).

### Local

- [ ] `nova ci-matrix --local` runs on dev machine.

---

## Open questions

1. **macOS GitHub Actions runner cost** — больше чем Linux runner.
   Worth ли $ для каждого PR vs nightly-only? **Decision:** PR-blocking
   for now; review после 1 месяца usage data.

2. **MSVC `/W4` vs `/W3`** — strict warnings могут flag legitimate
   patterns. **Decision:** start с `/W3` (default), Plan 58.1 ↑ к `/W4`
   с per-warning audit.

3. **Wall-clock budget** — 60 min target. Если упрёмся → split matrix
   на «quick» (PR-blocking, 30 min) + «full» (post-merge, 90 min).

4. **Apple Silicon free runners** — GitHub roadmap. Когда станут
   доступны → Plan 58.1 add `macos-arm64-clang` cell.

5. **Bench baseline noise** — > 10% threshold для regression gate.
   Может быть too sensitive на macOS CI runner (shared hardware).
   **Decision:** per-cell threshold (macOS = 15%, Linux = 10%, Windows
   = 12%); tune после observation.

---

## Что НЕ в Plan 58

- **MinGW / icc / TinyCC** — Tier 3, community-supported.
- **WSL Linux tests** на Windows runner — duplicate Linux Docker.
- **Cross-compilation** — native-only.
- **Apple Silicon native runner** — Plan 58.1 когда GitHub добавит.
- **Mutation testing matrix** — Plan 45 Ф.30 separate.
- **Fuzzing matrix** — отдельная инициатива (future).
- **Coverage matrix (lcov/gcov)** — Plan 58.2 (отдельно).

---

## Risks

| Risk | Mitigation |
|---|---|
| **CI cost balloon** | Caching (AD2); skip-patterns (AD11); per-cell budget cap. |
| **Flaky tests на macOS shared runners** | Retry budget per test (2); flaky-test marker; tune threshold. |
| **MSVC `__VA_OPT__` / C17 gap** | Pin VS 2022 17.10+; document minimum. |
| **vcpkg слом при apt-обновлении CI image** | Pin vcpkg commit hash; weekly auto-bump через separate workflow. |
| **TSan race с libuv internals** | Ignore-list для libuv known races (`tsan-ignore.txt`); document. |
| **Boehm GC ABI break** | Pin libgc version в vcpkg.json + apt; sentinel test для ABI smoke. |
| **`#cfg(target_os)` drift** в codegen output (AD3 violation) | Codegen-diff gate ловит **сразу**; rollback PR. |

---

## Связь

- **Plan 09** (Clang migration) — toolchain detection. Plan 58 verifies
  no regression в Plan 09 work.
- **Plan 27** (Boehm GC switch) — Plan 58 разблокирует Ф.5 Linux smoke
  (текущий status «ждёт CI»).
- **Plan 44.x** — TSan/ASan stress tests переиспользуем (AD5).
- **Plan 55** — closure: «Cross-toolchain MSVC» acceptance.
- **Plan 57** — bench integration (AD10).
- **Plan 65** (timer-channel API) — Ф.8 + Ф.14 unblock.
- **Plan 67** (println overload) — Ф.4 unblock.
- **Plan 27 G3a closer** — Linux smoke gate.

---

## Эволюция плана

- **2026-05-16 v1**: proposal-stub (4 phases, 2 dev-days).
- **2026-05-18 v2**: production-grade rewrite. Audit показал что v1
  не отвечал на 12 critical CI questions (cache, baselines, drift gate,
  sanitizers, deterministic codegen, local repro, failure triage, bench
  integration, skip-patterns, version pinning, tier model, honest excluded).
  Расширен до 11 phases (Ф.0-Ф.10), 5-7 dev-days. 20 requirements, 12 AD.
  Priority повышен P2 → P1 (плоды 2 plan'а уже упёрлись).
