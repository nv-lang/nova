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
ro ticks = ChanReader.tick_every(Duration.from_secs(1))
loop {
    ro _ = ticks.recv()  // wait 1 second
    do_periodic_work()
}

// Or with explicit behavior:
ro ticks = ChanReader.tick_every_with(Duration.from_secs(1), TickBehavior.Burst)
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
4. **Cancel propagation для `tick_every` ticker (КРИТИЧНО для design).**
   `tick_every` создаёт периодический channel — кто его закрывает?
   D91 capability-split: `ChanReader` **read-only**, нет метода `close`.
   Сейчас в Plan 65 Ф.13 stub — ticker leak'ит при cancel (fiber парк
   на `recv()` ждёт следующий тик, не реагирует на `tok.cancel()` до
   него). Три design-варианта на рассмотрение:

   **(a) `detach (cancel: tok) { ... }` — block-scoped cancel**
   ```nova
   detach (cancel: tok) {
       ro ticker = ChanReader.tick_every(d)   // auto-bound to tok
       while true {
           match ticker.recv() {
               Some(_) => f()
               None    => break    // tok.cancel() → runtime closes ticker
           }
       }
   }
   ```
   Pros: mirror `supervised(cancel: tok)` D75, ergonomic, всё внутри
   block видит ambient cancel. Cons: parser changes (detach сейчас
   `detach { ... }` без аргументов), `Detach` effect semantics
   меняется.

   **(b) `tick_every(d, cancel: tok)` — per-constructor**
   ```nova
   ro ticker = ChanReader.tick_every(d, cancel: tok)
   while true {
       match ticker.recv() {
           Some(_) => f()
           None    => break
       }
   }
   ```
   Pros: explicit per-timer, не trogает `detach`. Cons: boilerplate
   proliferation (каждый timer/channel constructor должен принимать
   cancel param), inconsistent с Plan 65 `close_after` (там ambient
   scope hook без явного arg).

   **(c) Return `Ticker` type вместо `ChanReader[()]` — Go-style**
   ```nova
   ro ticker = ChanReader.tick_every(d)   // returns Ticker, not ChanReader
   defer ticker.stop()                      // explicit cleanup
   while true {
       match ticker.recv() {
           Some(_) => f()
           None    => break
       }
   }
   ```
   `Ticker` имеет `.recv()` + `.stop()` + (опц.) `.reset(new_d)`.
   Pros: matches Go `time.NewTicker` API, defer-based cleanup явный,
   позволяет `reset` для rearm. Cons: новый тип (не просто
   ChanReader), требует Ticker `.recv()` proxy на internal channel,
   no ambient cancel — programmer должен явно `defer stop()`.

   **(d) Гибрид: Ticker + ambient cancel hook (Plan 65 Ф.10 pattern)**
   - `tick_every(d)` возвращает `Ticker` (как c)
   - Ticker auto-регистрируется в surrounding `CancelToken` (если
     bound) — Plan 65 Ф.10 reuse
   - `defer ticker.stop()` опционально (для explicit cleanup);
     ambient cancel чистит сам
   - `while true { match ... }` достаточно — никакой `while !is_cancelled()`
     polling

   **Recommendation tentatively: (d)** — лучшее из всех: Go-style
   Ticker type + Nova-native auto-cancel hook + defer fallback для
   ручного управления. Аналогичная семантика для `close_after`
   валидирована Plan 65.

   **Why this matters:**
   - Без правильного дизайна `set_interval`-helper не реализуем
     корректно — ticker leak'ит после cancel.
   - User-side `while !tok.is_cancelled()` polling — workaround, не
     решение (latency = period).
   - Decision блокирует начало Plan 66 implementation.

---

## Use case driver: `set_interval` stdlib helper

Прямой клиент `tick_every` — JS-style helper для periodic callback'ов:

```nova
// std/concurrency/timer.nv (после Plan 66)
export fn set_interval(d Duration, f fn()) Detach -> CancelToken {
    ro tok = CancelToken.new()
    detach (cancel: tok) {           // option (d): block-scoped cancel
        ro ticker = ChanReader.tick_every(d)
        while true {
            match ticker.recv() {
                Some(_) => f()
                None    => break     // tok.cancel() → runtime closes ticker
            }
        }
    }
    tok
}
```

**Семантика, которую гарантируем:**
- Period — fixed-rate (как Tokio `interval`, не drift как sleep-loop)
- Cancel — `tok.cancel()` → ticker close → loop exit, latency ≈ 0
- No leak — ticker cleanup atomic с cancel (Plan 65 Ф.10 pattern)
- Multiple users могут shared `tok` (через CancelToken cascade)

**Альтернатива через `Time.sleep` (rejected):**

До Plan 66 теоретически можно implement set_interval через sleep-loop:
```nova
export fn set_interval(d Duration, f fn()) Detach -> CancelToken {
    ro tok = CancelToken.new()
    ro ms = d.nanos / 1_000_000     // precision loss
    detach {
        while !tok.is_cancelled() {
            Time.sleep(ms)
            if !tok.is_cancelled() { f() }
        }
    }
    tok
}
```

Отвергнуто — **3 проблемы**:
1. **Precision loss:** `Time.sleep(int ms)` теряет sub-ms; `d.nanos / 1_000_000`
   округляет вниз.
2. **Drift:** реальный период = `sleep_ms + f_runtime`. Если `f()` занимает 200ms,
   а d=1s → период 1.2s. Накапливается. Tokio `interval` использует deadline
   arithmetic чтобы НЕ drift'ить.
3. **Зависит от `[M-time-now-schema-mismatch]`** — Time.sleep сейчас int ms,
   не Duration; schema mismatch с latent bug.

Sleep-loop **не подходит** для production set_interval. Ждать Plan 66.

**`while true` vs `while !tok.is_cancelled()` — design note:**

С правильным дизайном (option (a) или (d) — ambient cancel автоматически
закрывает ticker) **`while true { ... break on None }` достаточно**:
- `tok.cancel()` → runtime closes ticker channel
- Park'нутый `recv()` пробуждается с `None`
- `None => break` выходит из цикла мгновенно

Polling `while !tok.is_cancelled()` — **defensive workaround** для broken
state когда ticker не закрывается на cancel:
- Latency = до `d` секунд (fiber парк на recv, не проверит флаг до next tick)
- Один лишний вызов `f()` после cancel (если тик уже в канале)

Plan 66 implementation должен гарантировать channel-close семантику —
тогда `while true` это норма, polling — antipattern.

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
