// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 66: Timer-wheel runtime + `ChanReader.tick_every(Duration)` periodic ticker

> **Создан:** 2026-05-18 (outline; Plan 65 Ф.13 namespace squat).
> **Статус:** proposed (outline only — full plan to be written when
> Plan 65 stabilizes in production).
> **Приоритет:** P2 (optimization — Plan 65 libuv-per-timer is correct,
> just suboptimal for high-throughput loads).
> **Трудоёмкость:** ~6-8 dev-days (estimate; refine when full plan
> drafted).

---

## Зачем

Plan 65 закрыл API parity для **one-shot timer** (`ChanReader.close_after`),
но оставил два gap'а относительно Go/Tokio:

1. **Periodic ticker** — `time.NewTicker(d)` (Go), `tokio::time::interval(d)`
   (Rust), `setInterval` (TS). В Plan 65 `tick_every` зарезервировано как
   `#unstable` stub с panic body (см. `std/concurrency/timer.nv` Plan 65 Ф.13).

2. **Timer-wheel optimization** — каждый Plan 65 `close_after` =
   `uv_timer_t` handle (~120 байт + 1 syscall). На сотнях коротких
   таймеров (HTTP timeouts, retry budgets) overhead vs Tokio's
   hierarchical TimerEntry wheel или Go's runtime/timer heap.

---

## Scope

### R1. `ChanReader.tick_every(d Duration) -> ChanReader[()]`

Periodic channel — `recv()` returns `Some(())` каждые `d` миллисекунд.
Drop-on-overflow behavior:

| Behavior | Tokio enum value | Description |
|---|---|---|
| `Skip` (default) | `MissedTickBehavior::Skip` | если recv лагает > d, пропускаем missed ticks (rate-limited burst) |
| `Burst` | `MissedTickBehavior::Burst` | накапливаем missed ticks, выдаём подряд при recv |
| `Delay` | `MissedTickBehavior::Delay` | сдвигаем schedule вперёд от momenta когда recv удался |

API draft:
```nova
let ticks = ChanReader.tick_every(Duration.from_secs(1))
loop {
    let _ = ticks.recv()  // wait 1 second
    do_periodic_work()
}

// Or with explicit behavior:
let ticks = ChanReader.tick_every_with(Duration.from_secs(1), TickBehavior.Burst)
```

### R2. Custom timer-wheel runtime

Заменить `uv_timer_t` per close_after на hierarchical-bucket wheel
(Tokio-style):

- O(1) insert (vs libuv heap O(log N))
- Bucket granularity: 1ms → 64ms → 4s → 256s → 16k s (4 levels × 64
  buckets)
- 1 background fiber tick advance @ ms resolution
- Wake'ает fiber'ов через channel send (same as Plan 65)

### R3. Config gate

`runtime.timer_wheel = "auto" | "libuv" | "wheel"`:
- `auto` (default): switch к wheel если concurrent timer count > N (config)
- `libuv`: force Plan 65 behavior
- `wheel`: force new wheel

### R4. Backwards compatibility

- `ChanReader.close_after(Duration)` API unchanged
- Performance: wheel-path должен быть ≤ libuv-path для small N (< 100
  timers), значительно лучше для large N (1000+)
- Existing Plan 65 tests все PASS

### R5. Bench coverage

- `bench/micro/timer_alloc_throughput` — 1000 timers alloc+fire
- `bench/micro/timer_wheel_overhead` — measure wheel-induced fixed cost
- Both run на baseline + new runtime, CI gate (no regression > 5%)

---

## Связь

- **Plan 65** — Plan 66 builds on top of Plan 65's API surface, replaces
  internal implementation. Plan 65 hardening (Ф.10-Ф.14) provides:
  - CancelToken integration (preserved as-is in wheel impl)
  - Time-effect mock (preserved)
  - NOVA_TIMER_METRICS counters (extended w/ wheel-specific stats)
  - Monotonic deadline support (preserved)
- **Plan 22** — libuv timer infra still used for sleep + low-N wheel
  fallback.
- **Plan 23** — M:N scheduler. Wheel must be M:N-safe (per-worker
  wheel'ы + cross-worker steal'ы для load balance).

---

## Открытые вопросы

1. **Per-worker vs global wheel** под M:N? Per-worker locks contention,
   global mutex. Decision TBD.
2. **Timer cancellation cost** — wheel insert O(1), но cancel требует
   list-walk или O(1) freelist. TBD.
3. **Sub-ms timer precision** — wheel granularity 1ms, sub-ms requests
   round-up как сейчас. Acceptable?

---

## Что НЕ входит

- `Timer.reset()` API (Tokio) — separate plan.
- `select!` macro syntax sugar для timer-arms — already covered by
  Plan 31 D94.
- Wall-clock timers (cron-like absolute time) — Plan 64 (HTTP/scheduler).

---

## Эволюция плана

- **2026-05-18 v1**: outline only, namespace squat в Plan 65 Ф.13.
  Full plan to be drafted when Plan 65 stabilizes.
