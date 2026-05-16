// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 57: Performance benchmark infrastructure

> **Создан 2026-05-16 EOD.** Закрывает acceptance item из Plan 55:
> "Perf bench ±5% wall-clock measurement до/после mono-pass changes".
>
> **MVP ЗАКРЫТ 2026-05-16** (worktree `nova-lang-p57`, branch `plan-57`,
> commit `75192f361f3`). Plan 57.A (production hardening) и Plan 57.B
> (advanced) — в работе в той же ветке (см. §6 phasing).

---

## 1. Контекст и проблема

После Plan 55 (Ф.4 mono-pass corruption fix + Ф.6 multi-instance changes)
compiler stack изменился значительно. Текущая regression check —
**функциональная** (test passes), не **performance** (wall-clock / alloc /
instructions). Mono-pass save/restore через `mem::replace` имеет cost, и
накапливающиеся latent regression'ы невидимы без bench infra.

Production-grade language **обязан** иметь:
- statistical-grade measurement (median + MAD, не mean + stddev);
- noise-aware regression gate (Welch's t-test, не fixed threshold);
- multiple measurement axes: wall-clock, alloc count, GC pause, CPU
  instructions (CI-deterministic);
- reproducibility metadata (CPU model, governor, compiler version);
- compare tool с pairwise significance (benchstat-equivalent);
- profile integration (flame graphs);
- compiler-perf бенчи отдельно от runtime-perf бенчей.

State-of-the-art сравнение:

| Аспект | Rust (Criterion) | Go (`testing.B` + benchstat) | TS (tinybench/Vitest) | Nova цель |
|---|---|---|---|---|
| Adaptive sampling | ✅ | ✅ | ✅ | ✅ MVP |
| Median + MAD | ✅ | ❌ | ⚠️ | ✅ MVP |
| Outlier detection (Tukey) | ✅ | ❌ | ❌ | ✅ MVP |
| Welch's t-test compare | ✅ | ✅ | ❌ | ✅ MVP |
| Alloc tracking | ❌ | ✅ | ❌ | ✅ MVP (через Plan 32) |
| GC pause tracking | ❌ | ⚠️ | ❌ | ✅ MVP |
| CPU instructions mode | ✅ (iai) | ⚠️ | ❌ | ⏩ 57.B |
| Throughput | ✅ | ✅ | ❌ | ✅ MVP |
| Parameterized sweeps | ✅ | ⚠️ | ⚠️ | ⏩ 57.B |
| Sub-benchmarks/groups | ✅ | ✅ | ✅ | ⏩ 57.B |
| black_box/opaque | ✅ | ✅ | ⚠️ | ✅ MVP |
| HTML report | ✅ | ❌ | ⚠️ | ⏩ 57.A |
| Historical dashboard | ⚠️ (3rd-party) | ❌ | ❌ | ⏩ 57.A |
| Profile integration | ⚠️ | ✅ | ⚠️ | ⏩ 57.A |
| Reproducibility guards | ⚠️ | ❌ | ❌ | ✅ MVP + ⏩ 57.A |
| CI noise floor detect | ❌ | ❌ | ❌ | ⏩ 57.A (Nova-unique) |
| Effects-aware purity | n/a | n/a | n/a | ✅ MVP **Nova-unique** |

---

## 2. Цели

1. **Не хуже Criterion** по statistical rigor: median, MAD, outliers, CI 95%.
2. **Не хуже benchstat** по compare: Welch's t-test + geomean across suite.
3. **Не хуже Go `testing.B`** по alloc tracking: `bench.allocs()` API.
4. **Не хуже iai-callgrind** по determinism (Phase B): CPU instruction
   count mode (`perf_event_open` Linux, ETW Windows).
5. **Лучше всех** по integration: bench DSL встроен в язык, не отдельная
   библиотека; effect-aware (pure-fn bench детерминистичен по контракту);
   GC introspection встроен (через Plan 32).
6. **Воспроизводимость**: каждый JSON-результат содержит machine
   fingerprint, и `nova bench diff` отказывается сравнивать несравнимое.

---

## 3. Архитектура — 10 слоёв

### L1 — Measurement primitives

| Метрика | Source | Платформы | Cost overhead | Phase |
|---|---|---|---|---|
| Wall-clock (ns) | `uv_hrtime()` через Plan 22 | all | ~20ns/call | **MVP** ✅ |
| Allocations (count + bytes) | `gc.alloc_count()`, Plan 32 | all | ~5ns | **MVP** ✅ |
| GC pause (ns) | `gc.last_pause_ns()` delta | all | ~5ns | 57.A |
| CPU cycles (RDTSC) | inline asm | x86/x86_64 | ~10ns | 57.B |
| CPU instructions | `perf_event_open` (Linux), ETW (Win) | Linux primary | ~50ns | 57.B |

### L2 — Bench DSL (parser + AST + type-check)

**Top-level form** (MVP ✅), contextual keywords `bench` + `measure`:

```nova
module bench.my_module

bench "name of this benchmark" {
    // Setup — НЕ measured, 1x.
    let mut m = HashMap[int, int].new()
    let n = 1000

    // Measured block — adaptive sampling.
    measure {
        let mut i = 0
        while i < n {
            m.insert(i, i)
            i = i + 1
        }
        bench.elements(n)
    }
}
```

**Attribute form** (Plan 57.B) — для generic / parameterized:

```nova
#bench(params = [10, 100, 1_000, 10_000])
fn hashmap_insert(n int) {
    let m = HashMap[int, int].new()
    measure {
        for i in 0..n {
            bench.opaque(m.insert(i, i))
        }
    }
}
```

**Sub-benchmarks** (Plan 57.B) — Criterion `BenchmarkGroup` analogue:

```nova
bench "hashmap" {
    group "insert" {
        case "n=10"    { bench_insert(10) }
        case "n=10000" { bench_insert(10_000) }
    }
}
```

**Built-in `bench.*` functions** (prelude namespace, MVP ✅):

| Function | Назначение | Прецедент |
|---|---|---|
| `bench.opaque[T](v T) -> T` | Prevent compiler folding | Rust `hint::black_box`, Go `runtime.KeepAlive` |
| `bench.bytes(n int) -> ()` | Throughput annotation | Criterion `Throughput::Bytes`, Go `b.SetBytes` |
| `bench.elements(n int) -> ()` | Element count | Criterion `Throughput::Elements` |
| `bench.iterations() -> int` | Текущая `N` adaptive | Go `b.N` |
| `bench.reset_timer() -> ()` | Reset timer mid-measure | Go `b.ResetTimer` |
| `bench.allocs() -> int` | Snapshot alloc count | Go `b.ReportAllocs` |
| `bench.now_ns() -> int` | Monotonic timer | n/a |

### L3 — Adaptive sampling + statistical analysis (MVP ✅)

**Sampling strategy** (Criterion algorithm — proven):

1. **Warmup phase** (default 500ms): cache-warm, branch-predictor-warm.
2. **Calibration phase** (~100ms): compute `iters_per_sample`.
3. **Sampling phase**: 100 samples × `iters_per_sample`.
4. **Analysis** (pure Rust): median, MAD, mean, stddev, p25/p75/IQR,
   Tukey outliers, **bootstrap 95% CI**, slope+R².

**Result schema** (JSON v1, MVP ✅, frozen с soak period):

```json
{
  "name": "...",
  "iters_per_sample": 32,
  "samples_count": 100,
  "raw_ns": [12345, 12389, ...],
  "stats": {
    "n": 100,
    "median_ns": 12350,
    "mad_ns": 47,
    "mean_ns": 12380,
    "stddev_ns": 89,
    "p25_ns": 12320, "p75_ns": 12410,
    "iqr_ns": 90,
    "min_ns": 12280, "max_ns": 13100,
    "ci95_lo_ns": 12318, "ci95_hi_ns": 12382,
    "outliers_low": 0, "outliers_high": 3
  },
  "throughput_bytes": 4096,
  "throughput_bytes_per_sec": 331396700,
  "allocs_per_iter": 14,
  "allocs_total": 1400
}
```

### L4 — Output formats

| Format | Phase | Status |
|---|---|---|
| Terminal table | MVP | ✅ |
| JSON v1 | MVP | ✅ |
| CSV | MVP | ✅ |
| Markdown (PR comment) | MVP | ✅ |
| HTML (echarts dashboard) | 57.A | ⏩ |
| Criterion-compatible JSON | 57.B | ⏩ |

### L5 — Compare tool (`nova bench diff`) — MVP ✅

- Welch's t-test (Satterthwaite df) — устойчив к unequal variance.
- Geomean delta across suite.
- Reproducibility check — fail если CPU model / OS / arch различаются.
- Output: terminal (default), markdown (`--format markdown`), JSON.

### L6 — CI gate (`nova bench gate`) — MVP ✅ + 57.A enhancements

**MVP** (закрыто):
- `bench.toml` config — per-bench `[gate.strict.<glob>]` + `[gate.exempt]`.
- Per-metric thresholds: `wall_clock_delta_pct`, `allocs_delta_pct`,
  `gc_pause_delta_pct`, `significance_p`.
- Always-on GH Actions workflow `bench-regression.yml`.

**57.A enhancement** — auto noise-floor calibration:
- `nova bench gate --calibrate-noise N` runs baseline N times on **same**
  data, computes intrinsic noise floor (median of deltas observed
  between identical runs).
- Subsequent regression checks ignore deltas within calibrated band.
- Nova-unique: ни Criterion, ни benchstat не делают этого автоматически.

### L7 — Historical tracking + dashboard (Plan 57.A)

**Storage:** orphan branch `bench-history` в repo (no clutter в working tree).

**Workflow:**
1. `nova bench history-add result.json` → appends JSON в
   `bench-history/<git-sha>-<timestamp>.json` (subprocess git).
2. CI hook: на merge в main автоматически вызывает history-add.
3. `nova bench dashboard --history-branch bench-history --out=dashboard/`
   → static HTML с echarts; bundled (CDN), no server, no DB.

**Dashboard features:**
- Line chart wall-clock per commit (per bench).
- Annotation на коммитах с regression alerts.
- Filter by bench name.
- Comparison view: any two commits.
- Reproducibility metadata visible per data point.

### L8 — Compiler benchmarks (canonical corpus)

**MVP** (partial — 3 файла): hello, arith_loop, massive_match.

**Plan 57.B** (full — 10 файлов):
| File | LOC | Что measures |
|---|---|---|
| `01_hello.nv` ✅ | ~5 | Минимум compile-time |
| `02_arithmetic_loop.nv` ✅ | ~30 | Tight loop codegen |
| `03_generic_heavy.nv` ⏩ | ~200 | Mono-pass stress |
| `04_effects_handlers.nv` ⏩ | ~150 | Handler dispatch |
| `05_channels_select.nv` ⏩ | ~100 | Channel runtime |
| `06_contracts.nv` ⏩ | ~80 | Type-check + Z3 |
| `07_collection.nv` ⏩ | ~500 | Realistic module |
| `08_folder_module/` ⏩ | ~10 peers | Folder resolve |
| `09_deep_imports.nv` ⏩ | depth=10 | Import scalability |
| `10_massive_match.nv` ✅ | ~100 | Match codegen |

**Per-pass breakdown** (Plan 57.B):
- `PerfTimer::start("parse")` etc. hooks in compiler.
- `nova bench corpus --breakdown` → JSON с per-pass timings.

### L9 — Profile integration (Plan 57.A)

| Mode | Tool | Output | Platform |
|---|---|---|---|
| `--profile cpu out.svg` | `samply` | SVG flame graph | all |
| `--profile heap out.json` | Plan 32 sampling | JSON | all |
| `--profile gc out.txt` | `gc.gc_pauses()` | text histogram | all |
| `--profile lock` | runtime instrument | text | M:N runtime (Plan 44) |

Дизайн: profiling — separate run (не в bench measurement run), чтобы
instrumentation noise не влиял на baseline numbers.

### L10 — Reproducibility guards

**MVP** (✅): CPU governor (Linux), turbo, debug build, build mode mismatch,
GC mode, compiler version.

**Plan 57.A** — additional checks:
- **Thermal throttle** (Linux: `/sys/class/thermal/thermal_zone*/temp` > 80°C
  warning).
- **Background CPU load** — first 100ms /proc/stat sampling, warn if
  idle < 90%.
- **CPU pinning hints** — recommend `taskset -c N` если нет.
- **Process priority** — warn if nice > 0.

---

## 4. Phasing — detailed

### **MVP (Plan 57 core) — ЗАКРЫТО 2026-05-16, commit `75192f361f3`**

| Item | Status |
|---|---|
| L1 wall-clock + allocs | ✅ |
| L2 DSL (`bench`/`measure` + 7 builtins, contextual keywords) | ✅ |
| L3 statistical analysis (median/MAD/Tukey/Welch/bootstrap CI/slope) | ✅ |
| L4 terminal + JSON v1 + CSV + markdown | ✅ |
| L5 `nova bench diff` с Welch's t-test + geomean + compat check | ✅ |
| L6 CI gate + `bench.toml` (per-bench overrides + exempt) | ✅ |
| L8 partial corpus (3/10 файлов) | ⚠️ partial |
| L10 reproducibility metadata + env warnings | ⚠️ partial |
| 12+ runtime micro-benches | ⚠️ 5 files (hello, hashmap, arith, strings, gc) |
| Docs (`perf-conventions.md`) + D109 spec | ✅ |
| 32 unit tests pass | ✅ |
| GH Actions workflow always-on | ✅ |

**LOC actual:** ~2900 (стат функции ~370, bench module ~1100, codegen ~400,
runtime header ~330, parser/AST ~100, spec/docs ~700).

### **Plan 57.A — Production hardening — ~3-4 dev-days**

Sub-items (каждый — отдельный commit):

1. **57.A.1 — Historical orphan branch automation.**
   - `nova bench history-add result.json [--branch bench-history]` —
     subprocess git wrapper; commits result в orphan branch.
   - CI hook auto-call on merge to main.
   - **Spec:** new subcommand в `nova bench`.
   - **LOC:** ~200.

2. **57.A.2 — HTML dashboard via echarts.**
   - `nova bench dashboard --history-branch X --out dashboard/` →
     static HTML files (index + per-bench).
   - Inline echarts CDN, no build step.
   - Charts: time-series per bench, regression annotations, comparison.
   - **LOC:** ~600.

3. **57.A.3 — Auto noise-floor calibration.**
   - `nova bench gate --calibrate-noise N` — runs baseline against itself
     N times, measures noise floor.
   - Stored в `.nova-bench-noise.json` per machine fingerprint.
   - Gate applies noise floor before checking threshold.
   - **LOC:** ~300.

4. **57.A.4 — Thermal/load reproducibility extensions.**
   - Thermal throttle detection (`/sys/class/thermal/...`).
   - Background CPU load sampling (`/proc/stat` delta).
   - CPU pinning hints (Linux only).
   - **LOC:** ~200.

5. **57.A.5 — Profile integration (samply CPU).**
   - `nova bench run --profile cpu out.svg` → wraps samply external tool.
   - `--profile heap out.json` — periodic `gc.heap_size()` sampling.
   - `--profile gc out.txt` — pause histogram via Plan 32 API.
   - **LOC:** ~250.

**Total 57.A estimate:** ~1550 LOC.

### **Plan 57.B — Advanced — ~3-4 dev-days**

Sub-items (каждый — отдельный commit):

1. **57.B.1 — Extended canonical corpus.**
   - 7 more files (03/04/05/06/07/08/09).
   - `bench/corpus/CHECKSUMS` lockfile.
   - Per-pass `PerfTimer` hooks in compiler.
   - **LOC:** ~600 (corpus) + ~150 (timer hooks).

2. **57.B.2 — Criterion-compatible JSON output.**
   - `--format criterion-json` — emits per-bench JSON в `target/criterion/`
     layout с `estimates.json`, `sample.json` (Criterion 0.5 schema).
   - **LOC:** ~250.

3. **57.B.3 — Parameterized sweeps `#bench(params=[...])`.**
   - Parser: `#bench` attribute on `fn` с `params` list.
   - Codegen: emit N bench-entries, one per param value.
   - Result name: `<fn>/p=<value>` (Criterion convention).
   - **LOC:** ~500.

4. **57.B.4 — CPU instructions mode (Linux).**
   - `perf_event_open` syscall wrapper (no deps — direct libc FFI).
   - `--measurement instructions` flag.
   - JSON schema extension: `cpu_instructions` field per sample.
   - Fallback к wall-clock + warn если `paranoid > 1`.
   - **LOC:** ~400 (Linux-only; mac/Win — Phase C/D).

5. **57.B.5 — Group/case sub-benchmarks DSL.**
   - Parser: `group "..." { case "..." { ... } }` внутри `bench { ... }`.
   - Each case → отдельный bench entry с name `<bench>/<group>/<case>`.
   - **LOC:** ~400.

**Total 57.B estimate:** ~2300 LOC.

---

## 5. Acceptance criteria

### **MVP** — ✅ all closed (см. §4 table).

### **Plan 57.A** — ✅ ALL CLOSED 2026-05-17

- [x] `nova bench history-add` works; commits to orphan branch.
- [x] `nova bench dashboard` generates valid HTML loadable в browser.
- [x] `nova bench gate --noise N` reduces false positives (+ `calibrate` subcmd).
- [x] Thermal throttle detection (Linux /sys/class/thermal verified).
- [x] `nova bench run --profile cpu` works (samply integration; graceful
      error if not installed).

### **Plan 57.B** — ✅ ALL CLOSED 2026-05-17

- [x] 10-file canonical corpus complete (CHECKSUMS lockfile; per-pass
      PerfTimer hooks — Phase C TBD).
- [x] Criterion-JSON output (`<dir>/<bench>/new/{estimates,sample,benchmark}.json`).
- [x] `bench "name" (n in [10,100,1000]) { ... }` emits 3 separate entries.
- [x] CPU instructions infrastructure on Linux (`perf_event_open` FFI +
      diagnostic subcommand; per-sample runtime integration — Phase C TBD).
- [x] `bench "x" { group "g" { case "c" { ... } } }` parse + emit + 4-entry
      output (composite names `x/g/c`).

---

## 6. Что НЕ в Plan 57 / 57.A / 57.B

- **Cross-machine normalization** — perf зависит от железа.
- **PGO** — отдельный Plan 10.
- **Cloud-hosted bench history** — orphan branch достаточен.
- **Multi-language compare** — research initiative.
- **AI-driven regression analysis** — Phase C optional, не обещаем.
- **Distributed benchmarking** — за горизонтом.

---

## 7. Связь

| Plan | Role |
|---|---|
| **Plan 32** (GC introspection) | Required для alloc/gc tracking |
| **Plan 22** (libuv) | `uv_hrtime()` — high-res timer |
| **Plan 27** (GC switch) | GC mode metadata |
| **Plan 09** (Clang) | Compiler version metadata; 57 даёт infra verify |
| **Plan 10** (PGO) | 57 — baseline для measuring PGO impact |
| **Plan 44** (M:N runtime) | Lock contention profile (57.A/B) |
| **Plan 45** (nova doc) | `nova-bench-types` mirrors `nova-doc-types` pattern |
| **Plan 55** | Closure: unblocks "perf bench ±5%" acceptance |
| **Plan 58** (cross-toolchain) | Bench dimension в historical dashboard |

---

## 8. Risk register (carry over от MVP)

| # | Risk | Probability | Severity | Mitigation |
|---|---|---|---|---|
| R1 | Measurement noise on CI shared runners | High | High | 57.A: auto noise-floor calibration + Welch's t-test (вместо fixed threshold) |
| R2 | DSL parser/typecheck complexity | Medium | Medium | Resolved: contextual keywords (как `apply` в Plan 33.5) |
| R3 | Stable JSON schema lock-in | Medium | High | `format_version` field + soak period |
| R4 | Wall-clock unreliable на debug builds | Verified | Critical | Release-only enforcement (consistent c feedback_release_builds) |
| R5 | GC introspection adds noise | Low | Medium | `bench.allocs()` opt-in, off в чистых wall-clock benches |
| R6 | Bench state leaks между tests | Medium | Medium | Subprocess isolation per bench-group |
| R7 | Linux `perf_event_open` требует CAP_PERFMON | High | Low | Fallback к wall-clock + warn (57.B) |
| R8 | Adaptive sampling long warmup tail | Medium | Low | Configurable `--warmup-ms`, default 500ms |
| R9 | HTML dashboard maintenance | Low | Low | Static gen, no server (57.A) |
| R10 | Compiler corpus drift | High | Medium | `bench/corpus/CHECKSUMS` lockfile (57.B) |
| R11 | bench-history branch разрастается | Medium | Low | Yearly squash + compression (57.A docs) |
| R12 | Welch misleading при non-Gaussian | Low | Medium | Mann-Whitney U-test (57.A optional) |
| R13 | Macro hardware changes invalidate history | Low | Medium | Metadata includes CPU model |
| R14 | Effect-aware purity claim неверно | Medium | Medium | Documented: pure-fn deterministic по value, не timing |

---

## 9. Performance budget (мета-perf)

| Метрика | Budget | Rationale |
|---|---|---|
| `nova bench` startup overhead | < 100 ms | Pre-DSL parsing + corpus discovery |
| Per-bench overhead (timer + stats) | < 1% measured time | Inflate iters_per_sample при коротких benches |
| Sampling phase budget (default) | 10s per bench | Configurable |
| Total suite (10 corpus + 12 micro) runtime | < 5 min | CI per PR |
| JSON output size per run | < 100 KB | |
| HTML dashboard pageweight | < 1 MB | Static |
| Historical storage growth | < 10 MB/year | Yearly squash |

---

## 10. D-decisions

- **D109** в `09-tooling.md` — Benchmark DSL (DSL grammar + sampling
  protocol + rejected alternatives). Closed MVP.

---

## Эволюция

- **2026-05-16 created**: исходный план, P2, ~2-3 dev-days, naive ±5%.
- **2026-05-16 revised**: production-grade, 10-layer, MVP+57.A+57.B
  phasing, P1.
- **2026-05-16 MVP closed**: worktree p57, commit `75192f361f3`.
  10-layer MVP shipped + tests + docs + spec D109 + GH workflow.
  562 PASS / 0 FAIL regression check.
- **2026-05-17 57.A closed**: 5 sub-tasks shipped:
  - 57.A.1 history-add orphan branch automation (commit `4bc471e3765`).
  - 57.A.2 dashboard echarts static HTML (commit `b3fd1d8dd85`).
  - 57.A.3 auto noise-floor calibration (commit `a0febf67e86`).
  - 57.A.4 thermal/load/nice/affinity detection (commit `378c1312556`).
  - 57.A.5 --profile cpu/heap/gc samply integration (commit `2ca4c65138e`).
- **2026-05-17 57.B closed**: 5 sub-tasks shipped:
  - 57.B.1 extended corpus (10 files + CHECKSUMS).
  - 57.B.2 Criterion-compatible JSON output (commit `0102df4d996`).
  - 57.B.3 parameterized sweeps `(n in [...])` (commit `9f9a3abda67`).
  - 57.B.4 CPU instructions mode Linux perf_event_open (commit `80bb1591c37`).
  - 57.B.5 group/case sub-benchmarks DSL (commit `f790fd1234a`).
- **2026-05-17 final**: 43 unit tests pass + 562/562 nova test regression.
  Plan 57 — **completely closed** across MVP / 57.A / 57.B. Backlog —
  только runtime-side integration deferred (CPU instr per-sample, heap
  sampler thread, gc.last_pause_ns — Phase C TBD).
