// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 123 — Field-cache optimization migration guide

> Production teams. Last updated 2026-06-02.

## TL;DR

Plan 123 V1-V6 field-cache optimization is **ON by default** starting
this release. Pure AST→AST transformation, semantic equivalence
guaranteed. If you observe behavior changes (you shouldn't), disable
via `NOVA_FIELD_CACHE=0` env var or CLI flag.

## What changed

Four optimization layers added to Nova codegen:

| Layer | D-block | What |
|---|---|---|
| V1 | D217 | Direct `@field` cache at method-body prefix |
| V2 | D218 | Loop-Invariant Code Motion for `@field` reads |
| V3 | D219 | Pure-call result cache (`@<#pure_method>()`) |
| V4 | D217 amend | Chain cache `@a.b.c` |
| V5 | D217 §6 | `--explain-cache` CLI diagnostic |
| V6 | D217 §7 | Telemetry + production CLI flags |

Each layer independently controllable via env vars or CLI flags.

## Rollout strategy

### Step 1: baseline differential (recommended)

Before deploying, run differential test:
```sh
# Build with cache OFF.
NOVA_FIELD_CACHE=0 nova test --jobs 16
# Build with cache ON (default).
nova test --jobs 16
```

If both pass identically, you're safe to deploy.

### Step 2: production monitoring

Use `--telemetry-cache` для baseline metrics:
```sh
nova check src/ --telemetry-cache --telemetry-json > cache-metrics.json
```

Recommended baseline metrics:
- `methods_affected_pct`: typical 5-15% for application code, 20-40%
  for hot stdlib paths.
- `caches_per_method_median`: typical 1-2.
- `caches_per_method_p99`: should NOT exceed `max_per_fn` cap (default 8).

### Step 3: gradual rollout

If unsure:
1. Deploy V1 only: `NOVA_FIELD_CACHE_LICM=0`,
   `NOVA_FIELD_CACHE_PURE=0`, `NOVA_FIELD_CACHE_CHAIN=0`.
2. Monitor for 1 week.
3. Enable V2 LICM: remove `_LICM=0`.
4. Monitor; repeat для V3, V4.

## Detecting regressions

### Symptoms

If a regression occurs (very unlikely given semantic equivalence
guarantee + 50+ fixture differential testing):
- Tests pass under `NOVA_FIELD_CACHE=0` but fail with default ON.
- Behavior differs between debug builds (`-O0`) and release (`-O2`).

### Diagnostic workflow

1. Identify which layer:
```sh
# Disable layers one at a time.
NOVA_FIELD_CACHE_CHAIN=0 nova test     # V4 off
NOVA_FIELD_CACHE_PURE=0 nova test      # V3 off
NOVA_FIELD_CACHE_LICM=0 nova test      # V2 off
NOVA_FIELD_CACHE=0 nova test           # All off
```

2. Inspect generated `.c`:
```sh
nova compile failing_file.nv --no-annotate-source
diff <(NOVA_FIELD_CACHE=0 nova compile failing_file.nv -o /dev/stdout) \
     <(nova compile failing_file.nv -o /dev/stdout)
```

3. Use `--explain-cache` to see decisions:
```sh
nova check failing_file.nv --explain-cache
```

4. Report issue at github with reproducer + cache decisions.

## CLI flags reference

### Disable layers

| Env Var | CLI Flag | Default | Disable Value |
|---|---|---|---|
| `NOVA_FIELD_CACHE` | `--no-field-cache` | ON | `0` |
| `NOVA_FIELD_CACHE_LICM` | `--no-field-cache-licm` | ON | `0` |
| `NOVA_FIELD_CACHE_PURE` | `--no-field-cache-pure` | ON | `0` |
| `NOVA_FIELD_CACHE_CHAIN` | `--no-field-cache-chain` | ON | `0` |

### Tune thresholds

| Env Var | Default | Meaning |
|---|---|---|
| `NOVA_FIELD_CACHE_THRESHOLD` | 2 | Min reads для D217 V1 |
| `NOVA_FIELD_CACHE_LICM_THRESHOLD` | 2 | Min reads inside loop |
| `NOVA_FIELD_CACHE_PURE_THRESHOLD` | 2 | Min `@<method>()` calls |
| `NOVA_FIELD_CACHE_CHAIN_THRESHOLD` | 2 | Min `@a.b.c` occurrences |

### Bound resources

| Env Var | Default | Meaning |
|---|---|---|
| `NOVA_FIELD_CACHE_MAX` | 8 | Per-fn cap (всех layers combined) |
| `NOVA_FIELD_CACHE_LICM_MAX` | 4 | Per-loop cap |
| `NOVA_FIELD_CACHE_CHAIN_DEPTH` | 4 | Max chain depth |

## FAQ

### Will my benchmarks change?

`-O0` (debug) builds will see consistent 15-30% reduction in pointer
derefs. `-O2` (release) builds — smaller gain (5-10%) due to C
compiler's own CSE. Profile to confirm.

### What if my code relies on specific allocation patterns?

Pure-call caching (V3 D219) skips если `#pure` method allocates
(observably-pure vs contract-pure distinction). Skip V3 if you
depend on alloc counts: `NOVA_FIELD_CACHE_PURE=0`.

### Does Plan 123 affect debugging?

No. Generated `_at_*` locals have `Span` pointing to first source
`@field` access. Debuggers (DWARF/PDB) show variable names mapped
back to source positions.

### Edition gating?

V1-V6 unconditional (no edition required). Future versions могут
require edition opt-in for breaking changes.

## See also

- `docs/field-cache-optimization.md` — user-level overview.
- `spec/decisions/08-runtime.md` D217/D218/D219 — formal semantics.
- `docs/plans/123-receiver-field-cse.md` — umbrella plan.
