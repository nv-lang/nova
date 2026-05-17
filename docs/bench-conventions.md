# Nova Performance — conventions and bench infrastructure

> Plan 57 — production-grade benchmark infrastructure (Criterion+benchstat-level rigor).

## Quick start

```sh
# Compile + run all benches in a single file:
nova bench run bench/micro/hashmap.nv

# Filter by substring (comma-separated):
nova bench run bench/micro/hashmap.nv --filter insert,get

# Tune sampling:
nova bench run bench/micro/hashmap.nv --samples 200 --warmup-ms 1000 --time-budget 30

# Save JSON v1 for diff/gate:
nova bench run bench/micro/hashmap.nv --out baseline.json

# Compare two JSON results — Welch's t-test + geomean:
nova bench diff baseline.json new.json

# CI gate (exit 1 если регрессии):
nova bench gate baseline.json new.json --config bench.toml
```

## Writing a benchmark

`bench` — контекстуальный keyword (parses как top-level item only когда
за ним идёт string-literal):

```nova
module bench.my_module

bench "name of this benchmark" {
    // Setup — НЕ measured, выполняется один раз.
    let mut m = HashMap[int, int].new()
    let n = 1000

    // Measured block — adaptive sampling: warmup → calibration → 100 samples.
    measure {
        let mut i = 0
        while i < n {
            m.insert(i, i)
            i = i + 1
        }
        bench.elements(n)  // throughput annotation
    }

    // Teardown (optional) — НЕ measured.
}
```

### Built-in `bench.*` functions

| Function | Purpose | Аналог |
|---|---|---|
| `bench.opaque(v)` | Prevent constant-folding; identity barrier | Rust `hint::black_box`, Go `runtime.KeepAlive` |
| `bench.iterations()` | Current `iters_per_sample` (для manual batch loops) | Go `b.N` |
| `bench.reset_timer()` | Reset sample timer (skip setup-в-measure) | Go `b.ResetTimer()` |
| `bench.bytes(n)` | Throughput annotation: bytes/iter | Criterion `Throughput::Bytes`, Go `b.SetBytes` |
| `bench.elements(n)` | Throughput annotation: elements/iter | Criterion `Throughput::Elements` |
| `bench.allocs()` | Snapshot текущий alloc count (Plan 32) | Go `b.ReportAllocs` |
| `bench.now_ns()` | Monotonic high-res timer (uv_hrtime под NOVA_USE_LIBUV) | — |

### Adaptive sampling protocol

`nova bench run` для каждого `bench "..." { measure { ... } }`:

1. **Warmup** (default 500ms): крутит `measure` блок до достижения warmup budget; результаты выбрасываются. Цель — cache-warm + branch-predictor-warm.
2. **Calibration** (~ 1 iter): измеряет single-iter time. Compute `iters_per_sample = max(1, target_sample_ns / single_iter_ns)`. Default target 1ms.
3. **Sampling** (default 100 samples × `iters_per_sample`): записывает per-iter time каждого batch.
4. **Stop early** если total elapsed > `time_budget_ns` (default 10s).
5. **Analyze** в CLI: median, MAD, mean, stddev, p25/p75, IQR, Tukey outliers, bootstrap 95% CI, slope+R².

### Tunable via env / flags

| Flag | Env | Default |
|---|---|---|
| `--samples N` | `NOVA_BENCH_SAMPLES` | 100 |
| `--warmup-ms T` | `NOVA_BENCH_WARMUP_NS` | 500ms |
| `--time-budget S` | `NOVA_BENCH_TIME_BUDGET_NS` | 10s |
| — | `NOVA_BENCH_TARGET_NS` | 1ms |
| `--filter PATTERN` | `NOVA_BENCH_FILTER` | (all) |

## Output formats

- **Terminal table** (default) — coloured когда TTY, иначе plain.
- **JSON v1** (`--out file.json`) — stable schema, used by diff/gate. Содержит metadata (CPU, OS, governor, gc_mode, compiler) + per-bench raw samples + statistics.
- **CSV** (`--out-csv file.csv`) — spreadsheet-friendly.
- **Markdown** (`--out-md file.md`) — для GitHub PR comments.

JSON schema versioning — `format_version: "1"`. Migrations через soak period (см. Plan 57 §R3).

## Compare и regression gating

### `nova bench diff baseline.json new.json`

- **Welch's t-test** для unequal variance (Satterthwaite df).
- **Significance marks**: `***` p<0.001, `**` p<0.01, `*` p<0.05.
- **Geomean delta** across suite (геометрическое среднее ratios).
- **Compatibility check**: warning если CPU model / OS / arch / C-compiler / GC mode различаются.
- Formats: `--format terminal|markdown|json`.

### `nova bench gate baseline.json new.json [--config bench.toml]`

Apply thresholds from `bench.toml`:
- `wall_clock_delta_pct` (default 5%) — flag только если delta > threshold AND p < significance_p.
- `auto_noise_floor` — auto-calibrate noise floor (Phase A, см. Plan 57 §3 L6).
- Per-bench `[gate.strict.<glob>]` — tighter thresholds для hot-path benches.
- `[gate.exempt]` — skip benches с known noise (sleep, IO).
- Exit 0 = pass, 1 = regression(s) detected.

## Reproducibility recommendations

Per-machine setup — без этого noise может быть ±15-20%:

### Linux

```sh
# 1. CPU governor → performance.
sudo cpupower frequency-set -g performance

# 2. Disable turbo boost (Intel; ±5-10% noise reduction).
echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo

# 3. Disable simultaneous multithreading (HT) если bench single-threaded.
echo off | sudo tee /sys/devices/system/cpu/smt/control

# 4. Pin process to specific cores (isolcpus в kernel cmdline; нужен reboot).
#    Bench запуск:
taskset -c 4,5 nova bench run bench/...

# 5. Renice + ionice.
nice -n -20 ionice -c 2 -n 0 nova bench run bench/...
```

### Windows

```powershell
# 1. Power plan → High Performance.
powercfg /setactive 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c

# 2. Pin process to specific cores:
Start-Process nova -ArgumentList 'bench','run','...' `
    -ProcessorAffinity 0xF0

# 3. Disable Windows Defender real-time scanning для bench directory
#    (одноразово, восстановить после):
Add-MpPreference -ExclusionPath D:\Sources\nova-lang-p57\bench
```

### macOS

- Disable Spotlight indexing для bench directory: `mdutil -i off /path/to/bench`.
- macOS не позволяет disable turbo без 3rd-party tools.

## Reproducibility metadata в JSON

Каждый JSON v1 содержит `metadata` block с:
- `hostname`, `os`, `kernel`, `arch`
- `cpu_model`, `cpu_count`, `governor`, `turbo`
- `gc_mode` (malloc / boehm), `build_mode` (debug / release)
- `compiler.nova_sha`, `compiler.nova_version`, `compiler.c_compiler`
- `runner_id` (от `NOVA_BENCH_RUNNER_ID` env)
- `timestamp_unix`
- `sampling.warmup_ns / target_ns / samples / time_budget_ns`

`nova bench diff` использует metadata для **compatibility check** — отказывается comparing если CPU model / OS / arch различаются. Compiler version / GC mode — warning, не fail.

## Avoiding common pitfalls

1. **NEVER bench debug build.** 5-20× slower и highly noisy. `--mode release` (default) обязателен.
2. **Don't `Time.sleep` внутри `measure`** — noise. Warning emitted (Plan 57.A).
3. **Don't `Io.println` внутри `measure`** — IO is variable. Warning.
4. **Use `bench.opaque(v)`** для значений которые компилятор может вычислить compile-time (constant folding eliminates всё).
5. **Compare benches на одной машине.** Cross-machine numbers не sensical (выводы только относительные).
6. **Watch shared-runner noise** на CI — для serious work используйте self-hosted runner с pinned cores.

## Plan 57 phases roadmap

- **MVP** (этот документ): L1 wall-clock+allocs, L2 DSL, L3 stats, L4 terminal/JSON/CSV/MD, L5 diff, L6 gate, L8 corpus, L10 reproducibility.
- **Plan 57.A** (production hardening): L7 historical orphan branch + HTML dashboard via echarts, L9 profile integration (samply flame graphs), auto noise-floor calibration runs.
- **Plan 57.B** (advanced): L1 CPU instruction count mode (`perf_event_open` on Linux), Criterion-compatible JSON output, parameterized sweeps (`#bench(params=[...])`).
- **Plan 57.C–F** (closed 2026-05-17): runtime hooks (PerfTimer / GC pause / heap sampler / CPU instructions per-sample), recursive discovery, retention squash, DSL lint warnings, corpus subcommand, multi-runner CI matrix, HTML compiler-perf dashboard, dashboard drill-down (histogram + Tukey + sidebar + comparison), PELT changepoint anomaly detection, e2e shell tests, SSH distributed bench, AI regression interpretation (`--explain`), memory bandwidth measurement Linux.
- **Plan 57.G** (audit-driven small batch, closed 2026-05-17): JSON drift slope+R² fields, defensive idioms, errno decoder для perf_event_open paths, ASCII histogram terminal output, `bench.metric(name, value, unit)` custom metrics DSL.
- **Plan 57.H** (cross-binary + cross-platform, closed 2026-05-17): multi-group geomean (benchstat-style per-group lines), `nova bench hyperfine` cross-binary timing, `nova bench callgrind` cross-platform CPU instructions via valgrind subprocess.

См. [docs/plans/57-perf-benchmark-infrastructure.md](plans/57-perf-benchmark-infrastructure.md) для полной архитектуры (10-layer design, risk register, perf budget).

## Phase G/H quick reference (2026-05-17)

### Custom metrics — `bench.metric(name, value, unit)`

Per-call user metric для domain-specific perf signals (cache hits, lock
waits, decoded frames). Closes Go `b.ReportMetric` gap.

```nova
bench "decoder_throughput" {
    measure {
        let n = bench.iterations()
        let mut decoded = 0
        for i in 0..n {
            decoded = decoded + decode_frame(i)
        }
        bench.metric("frames_decoded", decoded, "frames")
        bench.elements(n)
    }
}
```

JSON output: `custom_metrics[{ name, unit, count, min, max, sum, median, samples }]`.

**Caveat:** called per-iteration → N*S total samples (N=iters_per_sample,
S=samples_count). Для per-sample-end metric — call ONCE после inner loop.

### `--histogram` flag — ASCII distribution

```bash
nova bench run bench/micro/loop.nv --histogram
```

```
Distribution: int_add_tight_loop
  histogram (40 buckets, max count = 9):
    ▁ ▂▄▆█▇▅▃▂▁
              M  [        ]
    58.5 µs … 70.0 µs  (M=median, [ ]=Tukey fences)
```

### Drift detection — `drift_slope_ns_per_sample` + `drift_r_squared`

Plan 57.G.1 поля в `stats` block JSON output. Slope of (sample_index,
raw_ns) linear regression. Высокий R² + non-zero slope → systematic
drift (cache warmup leak, thermal); низкий R² → noise only. Computed
для всех benches автоматически.

### Multi-group geomean (benchstat-style)

`nova bench diff` automatically detects bench-name groups (first
`/`-segment): `hashmap/insert`, `hashmap/lookup`, `vec/push` → groups
`hashmap` и `vec`. Отдельная geomean line per group (показывается
только если >1 group):

```
                                          geomean delta: +12.9%

Per-group geomean (group = first '/'-segment of bench name):
  hashmap                       +20.0%  (2 benches)
  vec                           +0.0%  (1 benches)
```

### Cross-binary timing — `nova bench hyperfine`

Compare timing arbitrary external binaries:

```bash
nova bench hyperfine \
    "old=./nova-v1 build large.nv" \
    "new=./nova-v2 build large.nv" \
    --warmup 3 --samples 10 --timeout 300 --out cmp.json

nova bench diff cmp.json other-machine.json
```

JSON output совместим с `bench diff` (per-binary entry в `benches[]`).

### Cross-platform CPU instructions — `nova bench callgrind`

Linux-only `perf_event_open` → cross-platform fallback через
`valgrind --tool=callgrind` (Linux + macOS; Windows не supports
valgrind):

```bash
nova bench callgrind-check                # diagnostic
nova bench callgrind ./bench-exe --cache-sim --out cg.json
```

Output: instructions (Ir), optional I1/D1/LL miss counts.
Determinism guarantee: identical binary + input → exactly same Ir
count run-to-run. Cost: ~50x native slowdown — для single-shot
deterministic comparison, не для high-throughput sampling.

### Distributed bench — `nova bench remote`

Plan 57.F.1: parallel bench runs across N remote machines via SSH.
Config в `$HOME/.nova-bench-remotes.toml`:

```toml
[remote.linux-xeon]
host = "perf-1.example.com"
user = "bench"
repo = "/srv/nova-lang"
runner_id = "linux-xeon-perf"
```

```bash
nova bench remote list
nova bench remote ping linux-xeon
nova bench remote run bench/micro/loop.nv --remotes all --gather-into ./results/
```

### AI regression interpretation — `nova bench diff --explain`

Plan 57.F.2: opt-in LLM-driven root cause analysis. Requires
`NOVA_AI_API_KEY` env. Privacy warning printed to stderr.

```bash
NOVA_AI_API_KEY=sk-... nova bench diff baseline.json new.json --explain
NOVA_AI_API_KEY=sk-... nova bench diff a.json b.json --ai-dry-run  # без API call
```
