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

### **Plan 57.C — Runtime integration & deeper hooks — ~3-4 dev-days**

Sub-items (каждый — отдельный commit):

1. **57.C.1 — Per-pass `PerfTimer` hooks в compiler.**
   - `PerfTimer` struct в compiler-codegen, RAII guard wraps каждого pass.
   - `NOVA_PERF_TIMER=1` env enables; emits `__PERF__ <pass> <ns>` на stderr.
   - Passes: parse / imports-resolve / type-check / desugar / callnorm /
     codegen / c-compile / link.
   - **LOC:** ~250.

2. **57.C.2 — `gc.last_pause_ns()` API extension (Plan 32 ext).**
   - `alloc.c` / `alloc_boehm.c`: track latest pause time delta.
   - `external fn gc.last_pause_ns() -> int` в `std/runtime/gc.nv`.
   - Codegen dispatch в `emit_c.rs` (рядом с `gc.heap_size()`).
   - **LOC:** ~150.

3. **57.C.3 — Heap sampler thread в `nova_rt/bench.h`.**
   - Activated по `NOVA_BENCH_HEAP_SAMPLE_MS=N` env.
   - Background thread читает `nova_gc_heap_size()` каждые N ms.
   - Emit'ит `__HEAP_SAMPLE__ <ns> <bytes>` на stderr.
   - CLI parses → histogram + `--profile heap` real output.
   - **LOC:** ~200.

4. **57.C.4 — CPU instructions per-sample runtime (Linux only).**
   - Linux-only block в `nova_rt/bench.h` с `perf_event_open` syscall.
   - `nova_bench_run` добавляет counter reset/start/stop вокруг каждого
     measure batch.
   - JSON v1 extension: optional `cpu_instructions_per_iter` field.
   - `--measurement instructions` flag в `nova bench run`.
   - **LOC:** ~300 (Linux-only).

5. **57.C.5 — Recursive bench directory discovery.**
   - `nova bench run <dir>` walks recursively (mirror `walk_nv`).
   - Каждый .nv compiled + executed separately, aggregated results.
   - **LOC:** ~150.

6. **57.C.6 — `nova bench history-squash` retention.**
   - `--before-date YYYY-MM-DD` flag: squash older entries в single commit.
   - Yearly squash recommended (см. perf-conventions.md).
   - **LOC:** ~150.

7. **57.C.7 — Bench DSL lint warnings.**
   - Warn если `Time.sleep` / `Io.println` внутри `measure` body.
   - Warn если `measure` body empty.
   - Warn если `bench.opaque` на constant literal (no-op).
   - Integrated в `lints.rs` через walk_bench_lints.
   - **LOC:** ~200.

8. **57.C.8 — `nova bench corpus` subcommand.**
   - `nova bench corpus <file> --breakdown` invokes compile (`nova check
     -v` mode) + parses `__PERF__` from stderr → JSON per-pass timings.
   - `nova bench corpus --all` runs whole `bench/corpus/` directory.
   - **LOC:** ~200.

**Total 57.C estimate:** ~1600 LOC.

### **Plan 57.D — Backlog closure — ~1-2 dev-days**

Sub-items (каждый — отдельный commit):

1. **57.D.1 — PerfTimer hooks для `nova test` pipeline.**
   - Extend `compile_for_profile` style wraps к test_runner internals.
   - `NOVA_PERF_TIMER=1 nova test` emits __PERF__ markers per-test
     (parse/check/codegen aggregate).
   - **LOC:** ~50.

2. **57.D.2 — sleep-lint contextual effect-aware detection.**
   - Current `bench-sleep-in-measure` ловит `Time.sleep(...)` как method
     call. Если `Time` resolved как effect ident (handler context) —
     match fails.
   - Cover после name resolve: проверять что obj.kind == Ident("Time")
     ИЛИ resolved-to-effect-named-Time.
   - **LOC:** ~30.

3. **57.D.3 — Aggregated JSON output для recursive bench mode.**
   - `nova bench run <dir> --out file.json` сейчас warns "Phase D".
   - Aggregate per-file RunResultParsed → single JSON с benches[]
     across all files; preserve metadata.
   - **LOC:** ~80.

4. **57.D.4 — CI multi-runner baseline support.**
   - Multiple `bench-history-<runner-id>` branches per machine.
   - `nova bench history-add --branch bench-history-$RUNNER`.
   - `nova bench dashboard --history-branch X` уже supports.
   - Workflow YAML matrix template с per-runner branch.
   - **LOC:** ~100.

5. **57.D.5 — HTML compiler-perf dashboard для `nova bench corpus`.**
   - Generate отдельный HTML report из corpus JSON (echarts stacked
     bar per-pass time, per-file).
   - `nova bench corpus <dir> --html out.html`.
   - **LOC:** ~300.

**Total 57.D estimate:** ~560 LOC.

### **Plan 57.E — Production extensions — ~3-4 dev-days**

Sub-items (каждый — отдельный commit):

1. **57.E.1 — HTML dashboard drill-down interactivity.**
   - Existing per-bench HTML pages — base. Расширить с:
     - Histogram of raw samples (echarts histogram).
     - Outliers highlighted (Tukey ±1.5·IQR markers).
     - Stats sidebar: median/MAD/CI95/slope/R².
     - Comparison view ("vs latest commit", "vs 30 days ago") если history
       has multiple entries.
   - **LOC:** ~250 (extend bench/dashboard.rs).

2. **57.E.2 — Distributed bench coordination (design sketch).**
   - SSH-based remote bench orchestrator: `nova bench remote run user@host:repo
     <args>` запускает bench через SSH + fetches result JSON.
   - Aggregation: gather results from N hosts → unified dashboard.
   - **Status:** design-sketch in plan + simplifications [M-57.E.2]
     для future implementation. Не реализуется в этой phase — требует
     external infra (SSH key management, remote runner setup).
   - **LOC если impl:** ~500.

3. **57.E.3 — AI-driven regression interpretation (design sketch).**
   - LLM consumer JSON diff → natural-language summary с suggestion
     "likely cause: mono-pass change в commit X" или "noise spike,
     re-run".
   - **Status:** design-sketch. Требует external API integration
     (Anthropic / OpenAI) с pricing implications.
   - **LOC если impl:** ~300 (prompt + parse + display).

4. **57.E.4 — Memory bandwidth measurement (design sketch, Linux-only).**
   - Intel MBM (Memory Bandwidth Monitoring) через `perf_event` (PERF_TYPE_RAW
     с CHAS umask). AMD analogue через QoS.
   - Linux-only, требует root or CAP_PERFMON, kernel CONFIG_X86_INTEL_RDT.
   - **Status:** design-sketch. Hardware-dependent + privilege-gated.
   - **LOC если impl:** ~200.

5. **57.E.5 — Statistical changepoint anomaly auto-detection.**
   - Apply PELT (Pruned Exact Linear Time, Killick 2012) к historical
     time-series median per bench.
   - Identifies regimes (multiple stable means) + change-points where
     means shift significantly.
   - Output: `nova bench history-anomalies --branch X` printing
     changepoint list с before/after stats и approx commit-time.
   - **LOC:** ~250 (PELT algorithm + CLI).

6. **57.E.6 — E2E shell tests для CLI subcommands.**
   - PowerShell + bash test scripts covering:
     - `nova bench run/diff/gate`
     - `nova bench history-add/list/squash`
     - `nova bench dashboard` (HTML output validation)
     - `nova bench calibrate`
     - `nova bench corpus --json/--html`
     - `nova bench runner-branch` + auto-resolve.
     - `nova bench cpu-instr-check`
   - Stored в `nova_tests/plan57_e2e/`.
   - **LOC:** ~300 shell scripts + helper validation.

**Total 57.E implementation estimate:** ~800 LOC (E.1+E.5+E.6 only;
E.2/E.3/E.4 design-sketched, not implemented).

### **Plan 57.F — Sketches → implementation — ~2-3 dev-days**

Pick-up предыдущих E.2/E.3/E.4 design-sketches. Production-grade
implementations с opt-in defaults (нет mandatory external dependencies
для basic Plan 57 usage).

Sub-items (каждый — отдельный commit):

1. **57.F.1 — SSH distributed bench (E.2 impl).**
   - `bench/remote.rs`: `RemoteConfig` (host/user/repo path/runner_id)
     parsed из `~/.nova-bench-remotes.toml`.
   - `nova bench remote list/ping/run` subcommands.
   - SSH через `std::process::Command("ssh", ...)` — no FFI.
   - Parallel exec через `std::thread::spawn` per host.
   - Result fetch через `scp` (или `ssh ... cat result.json` для
     simplicity).
   - **LOC:** ~400.

2. **57.F.2 — AI regression interpretation (E.3 impl).**
   - `bench/ai.rs`: prompt builder + HTTP client + 2 providers
     (Anthropic, OpenAI).
   - HTTP через `std::net::TcpStream` + TLS (rustls? — adds dep;
     OR use system `curl` wrapper for simplicity → no deps).
   - `nova bench diff ... --explain` flag.
   - Token budget enforcement + cost logging.
   - **LOC:** ~350 (curl wrapper approach).

3. **57.F.3 — Memory bandwidth (E.4 impl, Linux-only).**
   - Extend `bench/cpu_instr.rs` с MBM event detection (sysfs probe).
   - Per-sample counter в `nova_rt/bench.h` Linux block.
   - `nova bench mbm-check` diagnostic subcommand.
   - `--measurement bandwidth` flag.
   - **LOC:** ~250 (Linux-only stubs на other OS).

4. **57.F.4 — Extend test coverage (e2e + .nv tests).**
   - E2E sections для F.1 (remote list/ping smoke без real SSH host),
     F.2 (--explain dry-run validates prompt construction),
     F.3 (mbm-check graceful).
   - **LOC:** ~150 (test scripts).

**Total 57.F estimate:** ~1150 LOC + tests.

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

### **Plan 57.F** — ✅ ALL CLOSED 2026-05-17

- [x] **F.1** SSH distributed bench coordination
      (`bench/remote.rs` 340 LOC + `nova bench remote {list,ping,run}`
      subcommands; parallel `std::thread::spawn` per host; scp gather).
- [x] **F.2** AI regression interpretation
      (`bench/ai.rs` 430 LOC + `nova bench diff ... --explain` flag;
      Anthropic + OpenAI providers; system `curl` HTTP — no Rust dep;
      `--ai-dry-run` cost estimation; privacy warning stderr; SHA
      auto-detect from JSON metadata).
- [x] **F.3** Memory bandwidth measurement (Linux)
      (`bench/membw.rs` 330 LOC + `nova bench membw-check`;
      `perf_event_open(PERF_COUNT_HW_CACHE_MISSES)` fallback +
      `/sys/devices/uncore_imc_*` Intel MBM probe + AMD `amd_df` /
      `amd_l3` BwMon detection; cross-platform stubs).
- [x] **F.4** Extended test coverage (+22 e2e asserts → 65 total;
      59 bench module unit tests, all PASS on Windows release build).

**Phase F totals:** ~1100 LOC (3 modules) + 113 LOC tests; all 4
deferred E-sketches are now production code with graceful fallbacks
on missing dependencies (ssh/curl/perf_event_open).

### **Plan 57.G — Audit-driven small batch — 2026-05-17**

After complete Phase F closure, a structured audit (cross-codebase
search for stubs/TODO + comparison with Criterion / Go testing.B /
tinybench / mitata) outlined low-hanging fruit. Phase G batches
all small (~10-100 LOC) improvements; Phase H — larger.

Audit findings on production-grade aspects:
- 0 TODOs/FIXMEs/unimplemented! в bench/* modules.
- 0 `#[ignore]` tests; 1 conditional skip in cpu_instr.rs (graceful).
- Resource cleanup audit: all fd/threads/malloc have explicit Drop.
- 3 `unwrap()` sites where invariant would be self-documenting via
  `expect("...")` или `entry().or_insert_with()` idiom.
- `slope_ns_per_iter + r_squared` computed in stats.rs **но не emit'ятся**
  в JSON v1 output (Criterion does).
- `errno`-only error messages в perf_event_open / membw paths.
- ASCII histogram absent in terminal output (mitata-style).

Sub-items (commits independent):

1. **57.G.1 — Schema completeness: slope + R² в JSON.**
   - `bench/schema.rs::analyzed_to_json`: serialize `stats.slope_ns_per_iter`
     + `stats.r_squared` (already in `AnalyzedBench`).
   - Schema version stays v1 (backward-compatible field addition).
   - **LOC:** ~15.

2. **57.G.2 — Defensive unwraps → expect/idiomatic.**
   - `schema.rs:119`: `obj.as_object_mut().unwrap()` → `expect("invariant: json!({}) builds object")`.
   - `config.rs:143`: `get_mut(name).unwrap()` → `entry(name).or_insert_with()`.
   - `dashboard.rs:324-325`: `min/max().unwrap()` → match-arm-protected.
   - **LOC:** ~10.

3. **57.G.3 — errno decoder для perf_event_open paths.**
   - New `bench/errno.rs`: maps EACCES/EPERM/ENOENT/ENOSYS к actionable
     текст (CAP_PERFMON hints, kernel version check).
   - Wires в `cpu_instr.rs::InstrCounter::new` + `membw.rs::LlcMissCounter::new`.
   - **LOC:** ~80.

4. **57.G.4 — ASCII histogram в terminal output.**
   - `bench/report.rs::terminal_histogram`: 20-bucket binning of raw_ns,
     blocks `█▇▆▅...` rendering, indicates median + Tukey fences.
   - Opt-in via `--histogram` flag в `bench run`.
   - **LOC:** ~120.

5. **57.G.5 — Custom metrics: `bench.metric(name, value, unit)`.**
   - DSL: new builtin `bench.metric[T](name str, value T, unit str)`.
   - Codegen: emit `__BENCH_METRIC__ <name> <value> <unit>` markers.
   - CLI parses → JSON `custom_metrics[]` field per sample.
   - Closes biggest gap vs Go `b.ReportMetric`.
   - **LOC:** ~200.

**Total 57.G estimate:** ~425 LOC + tests.

### **Plan 57.H — Cross-binary + cross-platform — 2026-05-17**

Larger items from audit; each may take ~1 day.

Sub-items:

1. **57.H.1 — Multi-group geomean (benchstat-style).**
   - `bench/diff.rs`: detect group prefix (`hashmap/insert/*`,
     `hashmap/lookup/*`) → per-group geomean lines in addition to suite.
   - **LOC:** ~80.

2. **57.H.2 — `nova bench hyperfine <binary> <binary>` cross-binary.**
   - New subcommand: time arbitrary external commands (compiler self-host,
     comparing two nova versions).
   - Subprocess wrapper с warmup + median + Welch's t-test.
   - JSON output совместимый с `bench diff`.
   - **LOC:** ~250.

3. **57.H.3 — Cross-platform CPU instructions via valgrind callgrind.**
   - For macOS/Linux where `perf_event_open` is gated, fallback к
     `valgrind --tool=callgrind` subprocess.
   - Parses `Ir` (instructions) from `callgrind.out.<pid>` output.
   - Determinism guarantee equivalent to iai-callgrind на Rust.
   - **LOC:** ~200.

**Total 57.H estimate:** ~530 LOC + tests.

### **Plan 57.G** — ✅ ALL CLOSED 2026-05-17 (audit-driven small batch)

- [x] **G.1** Drift slope + R² emitted в JSON v1
      (`drift_slope_ns_per_sample` + `drift_r_squared` поля в SampleStats;
      computed from raw_ns vs sample-index regression — detects cache
      warmup leak, thermal drift across run). `3abacb6943a`
- [x] **G.2** 3 defensive unwraps → expect/idiomatic
      (`schema.rs:119` invariant-doc expect; `config.rs:143`
      `entry().or_insert_with()`; `dashboard.rs:324-325` invariant-doc).
- [x] **G.3** errno decoder для perf_event_open paths
      (`bench/errno.rs` 80 LOC, maps EPERM/ENOENT/EACCES/EBUSY/EINVAL/
      EMFILE/ENOSYS к actionable hints + 6 unit tests).
- [x] **G.4** ASCII histogram в terminal output
      (Unicode block chars ▁▂▃▄▅▆▇█, 20-40 bins, M=median + [ ] Tukey
      fences, `--histogram` opt-in flag).
- [x] **G.5** `bench.metric(name, value, unit)` custom metrics DSL
      (closes biggest gap vs Go `b.ReportMetric`; aggregated per-bench
      с count/min/max/sum/median в JSON `custom_metrics[]` field).
      `3e52b19b2fb`

**Phase G totals:** ~525 LOC + tests; 96 unit + 98 e2e asserts всё PASS.

### **Plan 57.H** — ✅ ALL CLOSED 2026-05-17 (cross-binary + cross-platform)

- [x] **H.1** Multi-group geomean (benchstat-style per-group lines based
      on bench name `group/subname` prefix; `group_key()` +
      `per_group_geomeans()` + terminal/markdown/JSON renderers + 5
      unit tests). `e5426de07dc`
- [x] **H.2** `nova bench hyperfine "name=binary args..." [...]` —
      cross-binary wall-clock timing (`bench/hyperfine.rs` 210 LOC +
      6 unit tests; warmup + samples + timeout + workdir; JSON output
      совместим с `bench diff`). `21e1d026471`
- [x] **H.3** Cross-platform CPU instructions via valgrind callgrind
      subprocess (`bench/callgrind.rs` 210 LOC + 6 unit tests;
      `nova bench callgrind <binary> [--cache-sim] [--out]` +
      `callgrind-check` diagnostic; works на Linux + macOS; Windows
      → "use perf_event_open или WSL" hint). `21e5d2567b5`

**Phase H totals:** ~600 LOC + tests; section 23 +13 e2e asserts →
110 / 110 ALL PASS.

### **Plan 57.E** — ✅ ALL CLOSED 2026-05-17 (3 impl + 3 design-sketch)

- [x] HTML dashboard drill-down (histogram + Tukey fences + sidebar + comparison).
- [x] PELT changepoint anomaly auto-detection (`nova bench history-anomalies`).
- [x] E2E shell tests для CLI subcommands (25 asserts, 11 sections).
- [sketched] Distributed bench coordination — `docs/plans/57.E.2-distributed-bench-sketch.md`.
- [sketched] AI-driven regression interpretation — `docs/plans/57.E.3-ai-regression-interpretation-sketch.md`.
- [sketched] Memory bandwidth measurement — `docs/plans/57.E.4-memory-bandwidth-sketch.md`.

### **Plan 57.D** — ✅ ALL CLOSED 2026-05-17

- [x] PerfTimer hooks для `nova test` pipeline (aggregation mode
      `NOVA_PERF_TIMER_AGGREGATE=1` + summary table).
- [x] sleep-lint contextual effect-aware detection (`Time.sleep` Path-form
      + bare `sleep(...)`).
- [x] Aggregated JSON output для recursive bench mode (per-file →
      merged JSON / CSV / MD / Criterion-compat).
- [x] CI multi-runner baseline support (NOVA_BENCH_RUNNER_ID →
      per-runner `bench-history-<id>` branches; `nova bench
      runner-branch` helper; workflow matrix).
- [x] HTML compiler-perf dashboard для `nova bench corpus`
      (`--html out.html` echarts stacked bars).

### **Plan 57.C** — ✅ ALL CLOSED 2026-05-17

- [x] PerfTimer hooks в compiler (10 passes) + `NOVA_PERF_TIMER=1` env.
- [x] `gc.last_pause_ns()` API extension (Plan 32 ext) — alloc.c +
      alloc_boehm.c (monotonic timer wraps GC_gcollect) + alloc_rc.c stub.
- [x] Heap sampler thread + CLI stderr parser → histogram min/median/max.
- [x] CPU instructions per-sample runtime (Linux only via
      `perf_event_open`, ioctl reset/enable/disable/read).
- [x] Recursive `nova bench run <dir>` discovery (skip `corpus/`,
      hidden dirs; cheap text pre-filter `bench ` keyword).
- [x] `nova bench history-squash --before-date YYYY-MM-DD` retention
      policy + `--dry-run` preview.
- [x] Bench DSL lint warnings: bench-sleep-in-measure,
      bench-io-in-measure, bench-empty-measure, bench-opaque-literal.
- [x] `nova bench corpus <file> [--json] [--mode] [--toolchain]` —
      per-pass compile-time breakdown.

---

## 6. Что НЕ в Plan 57 / 57.A / 57.B

- **Cross-machine normalization** — perf зависит от железа.
- **PGO** — отдельный Plan 10.
- **Cloud-hosted bench history** — orphan branch достаточен.
- **Multi-language compare** — research initiative.
- ~~**AI-driven regression analysis**~~ — implemented в Phase F.2 (opt-in).
- ~~**Distributed benchmarking**~~ — implemented в Phase F.1 (SSH-based).

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
- **2026-05-17 Phase C closed**: 8 sub-tasks shipped (1 commit per
  sub-task), 44 unit tests pass, 562/562 regression:
  - 57.C.1 PerfTimer hooks `4744b170150`
  - 57.C.2 gc.last_pause_ns `37c0a360cd6`
  - 57.C.3 heap sampler thread `a933355d065`
  - 57.C.4 CPU instructions runtime (Linux) `ce5e694395f`
  - 57.C.5 recursive bench discovery `ed481691217`
  - 57.C.6 history-squash retention `198ffa452a4`
  - 57.C.7 bench DSL lint warnings `8fac1d1edd9`
  - 57.C.8 nova bench corpus subcommand `94c54561b8c`
  Plan 57 — **MVP + A + B + C fully closed** (20 commits total в
  worktree plan-57). Backlog → Phase D: aggregated JSON output для
  recursive mode, sleep contextual-keyword detection.
- **2026-05-17 Phase D closed**: 5 sub-tasks shipped (~530 LOC total):
  - 57.D.1 PerfTimer aggregation для nova test `e9a77932ba0`
  - 57.D.2 sleep-lint Path-form `b6dd5274510`
  - 57.D.3 aggregated JSON для recursive bench mode `6ef7c2b3397`
  - 57.D.4 multi-runner baseline support `9bdeff26ea8`
  - 57.D.5 HTML compiler-perf dashboard `a5fe41de6a8`
  Plan 57 — **MVP + A + B + C + D ALL closed** (28 commits в plan-57).
  44 unit tests pass. Regression: 562/0 pre-merge baseline; после
  merge main подтянулись 5 unrelated plan56/* HashMap clone/merge
  failures (Plan 56 partial closure, не от Phase D работы).
- **2026-05-17 plan57 test suite**: 11 .nv tests (4 positive + 7
  negative). `nova test --filter plan57` → 11 PASS / 0 FAIL.
- **2026-05-17 Phase E closed**: 3 sub-tasks impl + 3 design-sketches.
  - 57.E.1 dashboard drill-down (histogram + sidebar + comparison)
    `b3c4a1778da`
  - 57.E.5 PELT changepoint anomaly detection `01137b3be46`
  - 57.E.6 e2e shell tests (25/25 PASS) `b0e7b4ce01d`
  - 57.E.2/3/4 deferred design-sketches в `docs/plans/57.E.X-*.md`.
  Plan 57 — **ALL 5 phases COMPLETE** (38+ commits в plan-57).
  47 unit tests pass (44 bench:: + 3 anomaly::). 11 .nv tests + 25
  e2e asserts.
- **2026-05-17 Phase F closed**: все 4 deferred E-sketches теперь
  production code (~1100 LOC + 113 LOC e2e tests):
  - 57.F.1 SSH distributed bench coordination (E.2 impl) `098948f5ca7`
  - 57.F.2 AI regression interpretation (E.3 impl, opt-in) `c3eae50bf92`
  - 57.F.3 Memory bandwidth measurement (E.4 impl, Linux) `600375fb81f`
  - 57.F.4 extended e2e tests (+22 asserts → 65 total) `c83c0b50644`
  Plan 57 — **MVP + A + B + C + D + E + F COMPLETE**. 59 unit tests
  pass (включая 13 new для remote/ai/membw). 65 e2e asserts. 11 .nv
  tests. Все deferred sketches doc converted to working impl. No
  external Rust crates added: AI uses system `curl`, distributed uses
  system `ssh`/`scp`, membw uses raw `perf_event_open` FFI.
- **2026-05-17 Phase G + H closed (audit-driven)**: после Phase F
  cross-codebase audit (TODO/FIXME scan + Criterion/testing.B/tinybench
  comparison) outlined 8 production-grade improvements; все 8
  implemented в Phase G (5 small) + H (3 larger).
  - 57.G.1 drift slope + R² emitted в JSON (+ G.2-G.4) `3abacb6943a`
  - 57.G.5 bench.metric custom metrics DSL `3e52b19b2fb`
  - 57.H.1 multi-group geomean (benchstat-style) `e5426de07dc`
  - 57.H.2 nova bench hyperfine cross-binary `21e1d026471`
  - 57.H.3 valgrind callgrind cross-platform CPU instr `21e5d2567b5`
  Plan 57 — **MVP + A + B + C + D + E + F + G + H COMPLETE**. 113+
  unit tests pass (incl. 17 new для diff/hyperfine/callgrind/errno).
  110 e2e asserts. Все 8 audit gaps закрыты. Не добавлено ни одной
  Rust crate dep: hyperfine uses std::process::Command, callgrind uses
  system valgrind subprocess, errno decoder без libc beyond raw_os_error.
