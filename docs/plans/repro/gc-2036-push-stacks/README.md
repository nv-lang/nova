# Repro / evidence: #2036 — Windows GC OOM from chained `GC_push_all_stacks` (regression of the #2027 fix)

Base: worktree `gc-2036`, branch `fix/gc-push-stacks-gate` over `origin/main`
(`91424bb15`). Windows, clang, vcpkg `gc.lib` (GWW_VDB, incremental by
default). Date: 2026-11-27.

## Mechanism

The #2027 fix (`f588e70fc`) chained the previous `GC_push_other_roots`
callback — on `GC_WIN32_THREADS` that is `GC_push_all_stacks()`. When a
thread runs a fiber its TIB is swapped to the coroutine stack, so the SP
Boehm obtains for the thread is outside its native stack range and
`GC_push_all_stacks` falls back to "Thread stack pointer out of range,
pushing everything" — a huge conservative superset that retains garbage.
Reproduces in BOTH incremental and full stop-the-world mode (the stale SP
comes from the TIB swap, not from scanning a running world).

## Twin carrier (`probe.nv`)

200 fibers (`spawn`), each looping: allocate a 8–64 KiB `Vec[int]`,
`fetch_add`, `sleep(10ms)`; a monitor fiber prints `gc.heap_size()` every
5 s. Build with `nova build probe.nv --toolchain clang`, run with
`NOVA_MAXPROCS=8`. Full logs: `arm_*.log.txt`, `fix_probe.log.txt`.

A/B 2×2 (chained call on/off × incremental on/off), heap after the run:

| arm | chained push | incremental | result |
|---|---|---|---|
| a (`arm_a.log.txt`) | on  | on  | ~535 MB/s growth: 42.8 GB in 80 s (killed by watchdog at 40 GB) |
| b (`arm_b.log.txt`) | off | on  | flat 45,256,704 B for 360 s, 4.21M iterations |
| c (`arm_c.log.txt`) | on  | off | ~535 MB/s growth: 40.1 GB in 75 s (killed) |
| d (`arm_d.log.txt`) | off | off | flat 33,955,840 B for 360 s, 4.18M iterations |

Fix (`ea8584178`: no chaining; native-stack registry for threads without a
fiber arena): flat 45,355,008 B for 360 s (`fix_probe.log.txt`).

## #2027 red/green control (`stress-results.txt`)

Merged conformance CU (`nova test spec_tests --filter
a_q3_println_debug_record --keep-artifacts`), run directly with
`NOVA_MAXPROCS=16 NOVA_DIAG_SEGV=1`, 8 concurrent lanes × 40 rounds per
variant, both variants concurrently (the `megacu-sigsegv-gc-roots`
protocol):

| exe | fiber_arena_win.c | result |
|---|---|---|
| fixed.exe  (SHA-256 `80ab8c66…1fd070c`) | this fix | **320/320 exit 0** |
| pre2027.exe (SHA-256 `7001ea97…f0552`) | `f588e70fc~1` (unchained override, no registry) | **3/320 SIGSEGV (0xC0000005)** |

The harness still detects the #2027 gap; the registry closes it.

## Gap mini-repro (`gap2036.nv`)

20,000 fibers over heap payloads with checksum verification + a
`gc.collect()` hammer (400 collections), `NOVA_MAXPROCS=16`: fixed runtime
`done=20000 bad=0 GAP-CHECK PASS`. Note: the same binary built against the
pre-#2027 runtime also passed 5/5 — the #2027 loss is timing-sensitive at
worker startup and needs the merged-CU load to surface; the authoritative
red/green control is the stress above.
