# Nova's time system — the `Time` effect, `Duration`/`Timestamp`/`Monotonic`

**English** | [Русский](time.ru.md)

> Plan 175 (time-system-rework). Civil (calendar) time is a separate
> document — [`datetime.md`](datetime.md) (Plan 175.1, `std/time/civil`).

## Model

`Time` is an **internal plumbing effect** (like `TcpNet`/`AddrNet`, `std/net/effect.nv`):
user code does NOT call it directly — it goes through types and free functions instead:

```nova
import std.time.duration

with Time = th.mut_clock(0 as u64) {   // подмена часов в тестах (D11/D61)
    ro start = Monotonic.now()
    sleep(500.millis())
    ro elapsed = Monotonic.now().elapsed_since(start)
    assert(elapsed == 500.millis())
}
```

Three types, three roles (never mixed — D124 separates them at the type level):

| Type | Role | Source | Can go backward? | Serializable? |
|---|---|---|---|---|
| `Timestamp` | wall-clock, Unix epoch ns | `gettimeofday`/`GetSystemTimeAsFileTime` | yes (NTP/DST) | yes |
| `Monotonic` | process-local monotonic instant | `CLOCK_MONOTONIC`/QPC (`uv_hrtime`) | never (saturates to zero on an apparent regression) | **no** (opaque, process-local) |
| `Duration` | duration, signed, ±292 years | — (pure arithmetic) | — | yes |

The effect's schema (internal, `std/prelude/effects.nv` is the single source — codegen
reads it from the `.nv` file, it doesn't hardcode it):

```nova
export type Time effect {
    sleep(ms int) -> ()
    now_unix_ms() -> int
    now_monotonic_ns() -> int
    local_offset_sec() -> int
}
```

`local_offset_sec()` (Plan 175.1, D316 amend + D321, 2026-07-10) — the system
UTC offset of the machine's CURRENT local zone, in seconds (owner decision: the
system zone MUST be available). Nova sugar: `Offset.local()`
(`std/time/civil/offset.nv`) — closes `[M-175.1-local-offset-effect-op]`.
Only a numeric offset — the zone in `ZonedDateTime` stays EXPLICIT (D319 R1),
no implicit fallback to "the local zone".

**The wire stays int** (see "Ф.2 — why the typed effect wire wasn't shipped" below) —
the entire user-facing surface is, nonetheless, **fully typed** and **fully
mockable**, including `Monotonic` (Plan 175 Ф.3a).

## Before → after

| Operation | Before (pre–Plan 175) | After |
|---|---|---|
| wall-clock read | `Time.now() -> int` (schema/runtime mismatch — `[M-time-now-schema-mismatch]`) | `Timestamp.now()` (typed sugar over the int wire `Time.now_unix_ms()`) |
| monotonic read | compiler-builtin, 4 hardcoded sites in `emit_c.rs`, not mockable | `Monotonic.now()` — an ordinary `.nv` function, mockable via `with Time = handler {...}` |
| sleep | `Time.sleep(ms int)`, bare ms | effect (int wire) + free `sleep(d Duration)`/`sleep_until(deadline Monotonic)` |
| `now_ms`/`now_ns` | vtable+handler-only leftover | don't exist (were only an int-wire artifact) |
| 5 timer counters | inside `Time` | moved out into a separate read-only `TimerMetrics` |
| unit | ms/ns drift between sources | ns is canon (storage); op names carry the unit (`now_unix_ms`/`now_monotonic_ns`) |
| overflow | silent two's-complement wrap at ±292 years | trap-on-overflow (debug AND release) + `checked_*`/`saturating_*` (D317) |
| `m2 - m1` | no typed API | `@minus(Monotonic)`/`elapsed_since` — saturates to zero (D318) |
| `@display` | `"μs"` (U+03BC, non-ASCII) in `@into()` | ASCII `"us"`; byte-exact `@display`/`@debug` (D237) across all three types |
| elapsed measurement | `measure[T]` measured via `Timestamp.now()` (wall-clock — vulnerable to NTP/DST skew) | `Monotonic.now()` (immune to wall-clock skew) |

## Overflow policy (D317) — a 3-tier discipline

1. **Operators trap.** `+`/`-`/unary `-`/`*`/`/` on `Duration` — panic on overflow,
   **in debug AND release** (never a silent wrap — the Go trap; never a build-mode
   dependency — the Zig `ReleaseFast`-UB anti-example).
2. **`checked_*` → `Option[T]`.** `checked_add`/`checked_sub`/`checked_mul`/`checked_div`
   (Duration); `checked_add`/`checked_sub` (Timestamp); `checked_duration_since` (Monotonic).
3. **`saturating_*` → clamp** to ±(2⁶³−1) ns (≈±292 years).

`Timestamp` is additionally bounded to a **window of 1677-09-21 .. 2262-04-11** (i64 ns
around the Unix epoch) — `checked_add`/`checked_sub` return `None` outside it, while the
bare `@plus`/`@minus` saturates (never wraps back to 1677).

```nova
ro d = Duration.from_nanos(i64_max())
d.checked_add(1.nanos())     // → None (не паника, explicit escape hatch)
d.saturating_add(1.seconds()) // → clamp к i64_max()
d + 1.nanos()                 // → trap (оператор — default-safe)
```

## Monotonic: non-regression + clock source (D318)

- **Non-regression:** `@minus(Monotonic)`/`elapsed_since` **saturate to zero** on an
  apparent regression (an HW/VM/OS bug, cf. JDK-6458294) — never negative, never
  a panic, **with no global lock** (the lesson of the Rust 1.60 saga — Rust once
  panicked on such a regression, then rolled back to saturate; Nova commits to
  saturate immediately and permanently, no flip-flopping).
- **Clock source (per OS):** Linux `CLOCK_MONOTONIC` / macOS `mach_absolute_time` /
  Windows QueryPerformanceCounter (via `uv_hrtime()`). The guarantee is **only**
  monotonicity + non-regression; **suspend-inclusion is NOT guaranteed** (device
  sleep is unspecified-but-monotonic). A `ContinuousClock` analog (BOOTTIME)
  hasn't been introduced — `[M-monotonic-boottime]`, pending a use case.
- **Opaque by contract** — there is no `Monotonic.from_*` (like Rust's `Instant`):
  the only way to obtain a `Monotonic` is `Monotonic.now()` or arithmetic over an
  existing value. This guards against fabricating fake monotonic instants and — an
  important architectural consequence — is the reason the typed effect wire (see
  below) is architecturally more expensive than it looks at first glance.
- **Non-serializable** — `Monotonic` has NO `#impl(Serialize)` (verified,
  `spec_tests/conformance/neg/d316_monotonic_non_serializable_neg.nv`): it's a
  process-local value, meaningless outside the process (the Go anti-pattern, where
  `Time.String()` can leak an `m=…` monotonic component into a log).

## Sleep semantics (Ф.4)

- `sleep(d)`/`Duration.@sleep()` — `d <= 0` resolves **immediately** (Go/tokio
  parity), never panics on a zero/negative duration.
- The guarantee is **"sleeps NO LESS than `d`"**, granularity is the libuv timer
  wheel (~1ms).
- `sleep_until(deadline Monotonic)` — an MVP wrapper over
  `sleep(deadline.elapsed_since(now))`; a deadline already in the past → saturate
  to zero → immediate. A drift-free true re-arm timer is future work (Plan 66).
- The signature is future-proofed for an optional `tolerance` (Swift
  `sleep(until:tolerance:)` parity — energy efficiency/coalescing) —
  `[M-sleep-tolerance]`, not introduced yet.
- `sleep_until` accepts **only** `Monotonic` — `sleep_until(Timestamp)` is not
  introduced (a wall-clock deadline is immune to NTP only via monotonic; the
  explicit wall alternative is `sleep(ts.time_until())`, the footgun is visible at
  the call site).

## Mockability (AI-first testing)

One handler moves **both** the wall clock, the monotonic clock, **and** sleep
coherently (Rev.2 Q14 — Swift `TestClock` parity, but WITHOUT a viral generic
parameter on every signature):

```nova
import std.testing.handlers as th

test "rate limiter refills after 1s" {
    with Time = th.mut_clock(0 as u64) {
        ro m0 = Monotonic.now()
        sleep(1.second())               // виртуальные часы, не реальное ожидание
        assert(Monotonic.now().elapsed_since(m0) == 1.second())
        assert(Timestamp.now().as_unix_millis() == 1000)  // ОДИН источник, оба сдвинулись
    }
}
```

`fixed_ms(ms)` — the clock is frozen (deterministic timestamps, `sleep` is a
no-op). `mut_clock(start_ms)` — a virtual clock; `sleep`/`Time.sleep` advance it
without any real waiting.

**Auto-idle-advance (Plan 175, owner TODO closure, 2026-07-10):** tokio
`time::pause()` / Kotlin `TestCoroutineScheduler.advanceUntilIdle()` parity —
concurrent `spawn` fibers under ONE `mut_clock` no longer need an explicit
`sleep()` call at every step. Each `sleep(ms)` computes its own ABSOLUTE deadline
(`current_ms + ms`, before parking) and parks the calling fiber
(`vclock.park_until`, `nova_vclock_park_until` in nova_rt/fibers.h) in a per-scope
registry; once ALL live fibers of the scope are virtually parked (idle — no real
work left), the one with the nearest deadline wakes up (it can be a different
fiber) — the clock advances by `current_ms = max(current_ms, deadline)` (not
`+=`, so the contribution of already-fired siblings isn't double-counted). A flat
sequential flow (no fiber at all) still resolves immediately — BEHAVIOR IS
UNCHANGED for the overwhelmingly common case. Tests — `std/testing/handlers.nv`
(tokio-style `sleep(10_000)` is instant; three concurrent `sleep`s of different
lengths wake in deadline order, not spawn order; the final clock is the max, not
the sum).

```nova
test "конкурентные sleep будятся в порядке дедлайна" {
    with Time = th.mut_clock(0 as u64) {
        supervised {
            spawn { Time.sleep(100); /* ... */ }   // проснётся ТРЕТЬИМ
            spawn { Time.sleep(10);  /* ... */ }   // проснётся ПЕРВЫМ
            spawn { Time.sleep(50);  /* ... */ }   // проснётся ВТОРЫМ
        }
    }
}
```

**M:N contract:** the default (real-clock) handler is stateless/thread-safe.
`mut_clock` is **stateful** (it mutates a captured `current_ms`; auto-idle-advance
adds a non-atomic per-scope registry on top) — under concurrent
`spawn`/`parallel for` it needs `NOVA_MAXPROCS=1` (determinism for the
handler-state write race — see [[reference-mn-race-case-study]]).

**`[M-175-vclock-armed-mn-scope-identity]` (documented narrowing):** the
deadline-order guarantee of auto-idle-advance is verified and holds under the
cooperative spawn path (`NOVA_MAXPROCS=1` + `NOVA_AUTOARM=0` — cooperative/local
`nova_fiber_spawn_into`, where `_nova_active_scope` inside a fiber is the SHARED
scope of the whole `supervised{}` block). Under the DEFAULT armed M:N runtime
(auto-arm on the first `spawn`), `_nova_active_scope` inside a fiber is the
WORKER's OWN `w->scope` (`_worker_run_one_fiber`), not the siblings' shared scope
— the registry isn't shared correctly across siblings, so the mechanism degrades
SAFELY (every virtual sleep still resolves, no hang/crash) but WITHOUT the
deadline-order guarantee (spawn order instead — the old behavior). Fixing the
general M:N case needs a different anchor (e.g. resolving via
`NovaSpawnCtxBase._nova_parent_scope`) — out of scope for this pass.

## Ф.2 — why the typed effect wire wasn't shipped (an architectural finding)

The original plan called for re-taxing the int wire onto a fully typed scheme
(`timestamp() -> Timestamp`/`monotonic() -> Monotonic`/`sleep(d Duration)` —
directly in the effect declaration). Four attempts (including this one) showed:
the prelude⟷std.time coupling is solvable (moving the `Time` declaration into
`std.time`, next to the types), but it runs into a **deeper** barrier — a mock
handler must **construct** a typed `Monotonic` value inside the handler body, and
(a) `Monotonic` is deliberately opaque (no public constructor) and (b)
handler-literal codegen doesn't support an anonymous record literal. Exposing an
internal constructor specifically for test handlers would undermine the opacity
contract (the same constructor would be visible to ordinary user code too).

**Conclusion:** the shipped architecture — typed `.nv` sugar ON TOP of the
int-wire effect (`Timestamp.now()`/`Monotonic.now()`/free `sleep`/`sleep_until`)
— is not a temporary compromise but the correct final answer given the
compiler's current capabilities: the typed wrapper lives in the type's own
module (where an anonymous record literal is an ordinary function body, not a
handler literal), so opacity and the codegen limitation don't conflict.
`[M-time-now-schema-mismatch]` is closed **partially by construction** (the
user-facing surface is fully typed and mockable; the wire is int).

**UPD 2026-07-10 (handler-annot wave):** codegen limitation (b) — an anonymous
record literal in a handler body — has been **lifted** (a single typing channel
now feeds op-body emission; see the D316-amend UPD in
`spec/decisions/04-effects.md` and the matrix
`nova_tests/plan175_handler_annot/repro_matrix.nv`). This does NOT affect
`Time`'s architecture: barrier (a) — `Monotonic`'s deliberate opacity — is
self-sufficient on its own; option C (int wire + typed sugar) remains the
owner's final decision; the `Time` wire was not changed.

## Nova vs. 7 languages

| | Go | Rust | TypeScript/JS | Kotlin | Java | Zig | Swift | **Nova** |
|---|---|---|---|---|---|---|---|---|
| wall vs. monotonic — separate types | no (one `Time`, a mode bit) | yes (`SystemTime`/`Instant`) | no (`Date`/`performance.now()` — both bare numbers) | no (`Clock`/`TimeSource`/`TestCoroutineScheduler` — THREE unrelated ones) | partial (`Instant`/`nanoTime()` — a `long`, not a type) | no (bare `i64`/`i128`) | yes (strongest of all — TWO distinct monotonic clocks: `ContinuousClock`/`SuspendingClock`) | yes (D124) |
| clock injection / mock | monkey-patch/`synctest`-bubble | no std (crates) | `@sinonjs/fake-timers` (monkey-patch) | `Clock` DI is viral, silently falls back to the real clock if you forget to thread it through | DI is viral | **none at all** | a `Clock` protocol, `TestClock`, but viral (`<C: Clock>` through every signature) | **the handler is lexically scoped, ambient, doesn't infect signatures** (D11/D61) |
| `now()` fallibility | infallible | infallible | infallible | infallible | infallible | **error-union** (honest about platforms with no monotonic clock) | infallible | infallible-by-contract (tier-1 libuv; Q15) |
| overflow policy | **silent wrap** (anti-pattern) | trap (panic) | float precision loss | JVM `long` wrap | JVM `long` wrap | **UB in ReleaseFast** (build-mode dependent) | trap (integer arithmetic always traps) | trap (debug AND release) + `checked_*`/`saturating_*` |
| instant width | `int64` ns (monotonic component) | `i64`+`u32` (sec+nanosec) | `f64` ms (float!) | `Long` ns | `long` ns | **`i128`** (no 2262 horizon) | `Int128`-like (atto-epoch, wide) | `i64` ns, **±292y, a documented boundary** (Q11/Q16) |
| `sleep`/`sleep_until` typed | bare `time.Duration` (int64) | typed (`Duration`) | bare ms (`number`) | typed | typed | bare `u64 ns` (footgun) | typed, **+`tolerance`** (unique) | typed, `tolerance` — future (`[M-sleep-tolerance]`) |
| `sleep_until` clock | wall (`time.Time`) | both (`Instant`/`SystemTime`) | no direct analog | both | wall (`parkUntil`, JDK-8146730 — a bug!) | none | both (`Clock.sleep(until:)`) | **Monotonic only** (type-safely forbids a wall-based sleep_until) |

## Footguns, explicitly documented

- **`sleep(100)` — a compile error** (no implicit int→Duration): the anti-Zig
  footgun (`sleep_bare_int_neg`).
- **`sleep_until(Timestamp)` — a compile error** (E7301 type mismatch): a
  wall-clock deadline is immune to NTP only explicitly (`sleep(ts.time_until())`).
- **`Monotonic ± Timestamp` — a compile error** (no overload): mixing clock
  domains is inexpressible at the type level (D124).
- **`Monotonic.as_unix_*`/`.from_*` — no such method**: the opaque contract.
- **`d.sleep()` (method form) — AVAILABLE** (owner side-task,
  `Duration.@sleep()`, 2026-07-06) — the free `sleep(d)` remains the canon
  (Q6/Q8: the user doesn't touch `Time` directly), but the method form is NOT
  forbidden (this differs from the original §3.0-Q6 plan intent — an amendment
  by fact).
- **`Time.sleep` inside `#realtime fn`** — a compile error (D64 suspend-effect
  ban), but the diagnostic is a plain message, NOT the named
  `E_REALTIME_SYNC_PARK` (that code is specific to `#parks`-annotated sync
  primitives).

## Related documents

- [`datetime.md`](datetime.md) — civil (calendar) time (Plan 175.1).
- [D316](../../spec/decisions/04-effects.md#d316) — the `Time` effect + amendments.
- [D317](../../spec/decisions/04-effects.md#d317) — overflow policy.
- [D318](../../spec/decisions/04-effects.md#d318) — Monotonic non-regression.
- [D124](../../spec/decisions/06-concurrency.md#d124-monotonic-vs-timestamp--раздельные-типы-для-wall-clock-и-монотонных-часов) — wall/monotonic separation + amend.
- [D237](../../spec/decisions/02-types.md#d237-protocol-naming-convention-method-name-capitalized-plan-137-2026-06-09) — Display/Debug naming + amend.
