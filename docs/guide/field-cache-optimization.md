// SPDX-License-Identifier: MIT OR Apache-2.0
# Field-cache optimization — user guide

> Plan 123 umbrella (V1-V5 active). Last updated 2026-06-02.

## What it does

Nova compiler automatically caches `@field` reads и `@<pure_method>()`
calls in method bodies, eliminating redundant `self->X` pointer
dereferences в generated `.c` output. Hot-path methods (ReadBuffer,
StringBuilder, HashMap iterators) typically see 15-30% reduction в
pointer derefs under `-O0` debug builds.

The optimization is **transparent** — semantic equivalence guaranteed.
You can disable it any time с environment variables (see Escape
hatches below).

## What gets cached

Four layers operate together:

### D217 V1 — direct field cache (Plan 123.1)
For ro fields accessed 2+ times → cached at method-body start:
```nova
fn Point @sum_squared() -> int {
    @x * @x + @y * @y      // Before: 4 pointer derefs
}
// After D217 V1:
//   ro _at_x = @x; ro _at_y = @y; _at_x * _at_x + _at_y * _at_y
```

### D218 V2 — LICM loop hoist (Plan 123.2)
For invariant field reads inside loops → hoisted immediately before
loop:
```nova
fn Buf @sum_n(n int) -> int {
    mut total = 0
    for i in 0..n {
        total = total + @data[i] + @size   // @size invariant
    }
    total
}
// D218 hoists @size immediately before for-loop.
```

### D219 V3 — pure-call cache (Plan 123.3)
For `@<pure_method>()` calls 2+ times → cached:
```nova
#pure
fn Vec3 @magnitude_sq() -> int => @x * @x + @y * @y + @z * @z

fn Vec3 @double() -> int {
    @magnitude_sq() + @magnitude_sq()   // Cached single call.
}
```

### D217 V4 — chain cache (Plan 123.4)
For nested `@a.b.c` accesses 2+ times → cached:
```nova
fn Outer @check() -> int {
    @inner.cfg.limit + @inner.cfg.limit + @inner.cfg.limit
    // D217 V4 caches @inner.cfg.limit once.
}
```

## How to inspect cache decisions

CLI flag `--explain-cache` on `nova check`:

```sh
nova check src/buffer.nv --explain-cache
```

Sample output:
```
=== src/buffer.nv ===
  fn ReadBuffer @try_read_u32_le — 4 cache(s):
    D217 field cache: data, pos
    D219 pure-call cache: len
    D217 V4 chain cache: @header.signature

field-cache total: 1 method(s) affected, 4 cache(s) inserted
```

## Escape hatches

Disable all caching:
```sh
NOVA_FIELD_CACHE=0 nova build
```

Disable individual layers:
```sh
NOVA_FIELD_CACHE_LICM=0    # disable D218 LICM
NOVA_FIELD_CACHE_PURE=0    # disable D219 pure-call
NOVA_FIELD_CACHE_CHAIN=0   # disable D217 V4 chain
```

Tune thresholds:
```sh
NOVA_FIELD_CACHE_THRESHOLD=3        # default 2 (D217 V1)
NOVA_FIELD_CACHE_LICM_THRESHOLD=3   # default 2 (D218)
NOVA_FIELD_CACHE_PURE_THRESHOLD=3   # default 2 (D219)
NOVA_FIELD_CACHE_CHAIN_THRESHOLD=3  # default 2 (D217 V4)
```

Cap caches per fn (stack-frame budget):
```sh
NOVA_FIELD_CACHE_MAX=12   # default 8 — total across all 4 layers
```

## Performance expectations

- `-O0` builds: 15-30% reduction in pointer derefs on hot paths.
- `-O2` builds: smaller gain (C compiler already does NoAlias-based
  CSE). Still measurable due to Nova's deterministic emission.
- Cross-platform: identical AST output на Windows MSVC / Linux clang
  / macOS clang.
- Stack-frame impact: ≤ 8 cache locals per fn × 8 bytes ≈ 64 bytes.

## Semantic equivalence

All 4 layers are **pure AST→AST transformations**. Disabling any
layer (or all layers) produces identical observable behavior:
- stdout / stderr identical.
- Panics raised in same conditions.
- File system / network effects identical.
- GC behavior identical.

Verified via differential testing (umbrella nova_tests/plan123_*
fixtures all PASS identically под enabled и disabled).

## Spec references

- D217 (Plan 123.1) — baseline field cache + V4 chain extension.
- D218 (Plan 123.2) — LICM semantics.
- D219 (Plan 123.3) — pure-call cache.
- D24 (Plan 33.1+33.2) — `#pure` Purity infrastructure.

## Followups + future versions

- V5 (Plan 123.5, this) — LSP code-lens (deferred) + CLI flag.
- V6 (Plan 123.6) — telemetry + production rollout + full CLI flags.
- V7 (Plan 123.7) — inter-procedural analysis (IPA) для precise
  invalidation.
