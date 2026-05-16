// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 57: Performance benchmark infrastructure

> **Создан 2026-05-16 EOD.** Закрывает acceptance item из Plan 55:
> "Perf bench ±5% wall-clock measurement до/после mono-pass changes".
> В Plan 55 пропущен потому что нет очевидного индикатора regress'а,
> но для production-grade language нужна regression guard infrastructure.

---

## Контекст

После Plan 55 (особенно Ф.4 mono-pass corruption fix + Ф.6 multi-
instance changes) compiler stack изменился значительно. Текущая
regression check — только **functional** (test passes), не
**performance** (wall-clock).

Production-grade language имеет:
- **Bench suite** для critical paths (parser / type-check / codegen).
- **CI gate** на perf regression (±5% threshold).
- **Historical tracking** wall-clock тестового прогона по commits.

Без этого latent perf regression может накапливаться (особенно
в mono pass — каждый save/restore через `mem::replace` имеет cost).

---

## Scope

### Phase 1 — Baseline measurement

- **Wall-clock `nova test` (release)** на reference machine. Capture:
  - Total time (start of `nova test` → final SUMMARY).
  - Top-10 slowest tests (мы уже отдельно outputим).
  - Memory peak (через RSS measurement).
- **Per-stage breakdown:**
  - Parse time (sum across all tests).
  - Type-check time.
  - Codegen time.
  - C-compile time.
  - Run time (test execution).
- **JSON output:** `bench-results-<timestamp>-<git-sha>.json` для
  historical comparison.

### Phase 2 — Micro-benchmarks

- **`nova bench`** subcommand в nova-cli — запускает named benchmarks
  из `nova_bench/` directory (мирроринг `nova_tests/`).
- Bench module syntax:
  ```nova
  module bench.parser
  
  bench "parse 1000-line file" {
      // setup
      // measured block — wall-clock × iterations.
  }
  ```
- Baseline criteria: median × 1000 iterations, with warmup.

### Phase 3 — CI gate

- **GitHub Actions workflow** (`bench-regression.yml`):
  - Run on PRs labeled `perf-sensitive`.
  - Compare to baseline (main branch tip).
  - Fail if wall-clock > +5% on any tracked metric.
  - Comment PR с diff таблицей.

### Phase 4 — Historical dashboard

- **Static HTML page** generated from JSON history.
- Plot wall-clock per commit, per stage.
- Publish to GitHub Pages (optional).

---

## Acceptance criteria

- [ ] Phase 1 — `nova test --bench-out=bench.json` flag.
- [ ] Phase 2 — `nova bench [filter]` CLI command + `bench "..." { ... }`
      syntax + at least 5 micro-benchmarks (parser, type-check, codegen
      hot paths, channel send/recv, HashMap insert/get).
- [ ] Phase 3 — CI workflow gate (PR comment с diff).
- [ ] Phase 4 — historical HTML dashboard (optional, для public repo).
- [ ] **Documentation:** `docs/perf-conventions.md`.

---

## Estimate

| Phase | LOC | Зависимости |
|---|---|---|
| Phase 1 (test runner JSON output) | ~150 | nova test infra |
| Phase 2 (bench CLI + syntax) | ~300-500 | parser ext |
| Phase 3 (CI gate) | ~50 (yaml) | github actions |
| Phase 4 (dashboard) | ~200 | optional |
| **Total** | **~700-900 LOC** | self-contained |

**Estimate:** ~2-3 dev-days production-grade.

---

## Что НЕ в Plan 57

- **Cross-machine normalization** — perf зависит от железа; baseline
  per-runner.
- **Profile-guided optimization (PGO)** — отдельный Plan 10 (deferred).
- **Memory profiling beyond RSS** — отдельная инициатива (Plan 32
  GC introspection уже даёт API).

---

## Связь

- **Plan 55** — closure: this plan unblocks "perf bench ±5%"
  unfilled acceptance.
- **Plan 09** (Clang migration) — perf delta measurement (10-15%
  improvement claimed) — Plan 57 даст infrastructure verify.
- **Plan 10** (PGO) — Plan 57 baseline для measuring PGO impact.

---

## Priority

**P2** — production hardening, не блокер для feature work. Можно
запустить как side-investment в любой sprint.
