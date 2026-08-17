<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Runtime tuning — fiber arena (Plan 149 / D233)

**English** | [Русский](runtime-tuning.ru.md)

Nova programs run user code on lightweight **fibers** scheduled over worker
threads by **Vela** (M:N runtime). Each worker owns a **fiber arena**: a reserved (lazily-committed)
virtual region carved into fixed-size **slots**, one per concurrent fiber. Two
knobs let you tune the arena per workload — they are GOMAXPROCS-style runtime
properties of the *finished program*, NOT compiler flags.

## Knobs

| What | Env var | `nova.toml [runtime]` | Default | Range | Auto-correction |
|------|---------|-----------------------|---------|-------|-----------------|
| Per-fiber stack | `NOVA_FIBER_STACK` | `fiber_stack` | `4MB` | `256KB`–`256MB` | rounded UP to page size |
| Max fibers / worker | `NOVA_FIBERS_PER_WORKER` | `fibers_per_worker` | `16384` | `64`–`262144` | rounded UP to a multiple of 64 |

- **Per-fiber stack** is the usable stack each fiber gets (minus a 16KB guard
  page). Raise it for deeply-recursive fiber bodies; lower it to pack more
  fibers into the same memory. The builtin default was lowered from 8MB to 4MB
  for 2× fiber density out of the box.
- **Max fibers / worker** is the per-worker concurrent-fiber ceiling. Total
  process capacity = `fibers_per_worker × NOVA_MAXPROCS` (one arena per worker).

### Human-friendly sizes

`NOVA_FIBER_STACK` and `fiber_stack` accept a bare byte count or a binary
suffix (case-insensitive): `KB`/`K` = 1024, `MB`/`M` = 1024², `GB`/`G` = 1024³.

```
NOVA_FIBER_STACK=8MB        # 8 388 608 bytes
NOVA_FIBER_STACK=2097152    # same as "2MB"
NOVA_FIBER_STACK=512KB
```

`NOVA_FIBERS_PER_WORKER` is normally a plain integer (`20000`); `K`/`M` suffixes are
also accepted for symmetry.

## Precedence

```
env  >  nova.toml [runtime] (-D compile-time default)  >  builtin default
```

`nova.toml [runtime]` bakes the value into the build as a compile-time default;
the matching env var, read fresh when each worker arena initializes, overrides
it at runtime. Example:

```toml
# nova.toml — project-baked defaults
[runtime]
fiber_stack = "2MB"
fibers_per_worker  = 8192
```

With the manifest above, the program ships with a 2MB stack default. Setting
`NOVA_FIBER_STACK=8MB` at launch overrides it to 8MB without recompiling.

`nova build` and `nova bench` (single-file, dir, and `--profile`) resolve the
`[runtime]` section (and the Plan 115 `[ffi]` section) from the package
`nova.toml` exactly like `nova test` does, so the baked default applies no matter
which front-end produced the binary. The precedence above is unchanged — only
the set of commands that honor the manifest expanded.

## Auto-correction and safety

You can write **any** value — the runtime fixes it up:

- **Round UP, never reject for alignment.** A stack is rounded up to the page
  size; a fiber count is rounded up to a multiple of 64 (`NOVA_FIBERS_PER_WORKER=20000`
  → `20032`).
- **Clamp out-of-range + warn.** Stack below `256KB` → floored to `256KB`
  (`nova: NOVA_FIBER_STACK ... below floor — using 256KB`). Max above the
  compile-time ceiling (`262144`) → clamped (`nova: NOVA_FIBERS_PER_WORKER ... exceeds
  max — clamped`).
- **Garbage → warn + default, never crash.** `NOVA_FIBER_STACK=banana` prints
  `nova: invalid NOVA_FIBER_STACK — using default 4MB` and runs on the default.

## Stack overflow

If a fiber recurses past its usable stack it crashes cleanly on the guard page
with a hint:

```
nova: fiber stack overflow in slot N ...
Hint: increase NOVA_FIBER_STACK (env / nova.toml [runtime].fiber_stack) or reduce recursion depth.
```

## Memory budget

Slots reserve virtual address space lazily (POSIX `mmap` MAP_NORESERVE; Windows
`VirtualAlloc` MEM_RESERVE) — physical RAM is committed only for touched pages.
Still, the *virtual* reservation is `fibers_per_worker × stack × workers`. Tuning both
to extremes (e.g. `262144 × 256MB × 16`) can exhaust the user virtual address
space; size the product against what the host allows.

## Notes

- Bitmap cost is fixed at the compile-time ceiling (`262144` slots ⇒ 32KB per
  arena), independent of the runtime default — raising `NOVA_FIBERS_PER_WORKER` above
  the default does not grow per-platform arrays.
- The guard page (16KB) and scheduler/GC invariants are unaffected by tuning;
  the arena config is read once at worker init.
- On 32-bit targets the arena is intentionally tiny: `SLOT_COUNT_MAX` = 1024 and
  the runtime default is 64 slots (= 256MB virtual). The internal
  `NOVA_FIBERS_PER_WORKER_BUILTIN` literal was corrected `16`→`64` — round-UP to a
  multiple of 64 plus the `MIN`=64 floor already forced 64 at runtime, so the old
  `16` (and its "64MB" comment) was dead. 64-bit / Windows defaults are
  unchanged (`16384`).

See spec **D233** (`spec/decisions/08-runtime.md`) for the full contract.
