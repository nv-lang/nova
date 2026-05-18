// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 65: `ChanReader.close_after(Duration)` — timer-channel API parity с Go/Rust/TS

> **Создан:** 2026-05-18. **Ревизия v2:** 2026-05-18 (industry-parity audit
> vs Go `time.*` + Rust `tokio::time::*` + TS `setTimeout`/Tokio Sleep;
> добавлены cancel, deadline, mock-time, drop, observability, timer-wheel).
> **Статус:** proposed, не начат.
> **Приоритет:** P1 (исправление API drift + capability semantics + bringing
> timer-channel API на уровень Go/Tokio).
> **Трудоёмкость:** ~9-11 dev-days (с v2 расширениями; MVP в Ф.0-Ф.9 за 5 дней,
> production hardening Ф.10-Ф.14 ещё 4-6).

---

## Зачем

Текущий `Time.after(ms i64) -> ChanReader[()]` имеет три ортогональных
дефекта + не доходит до уровня production-grade timer-API в Go/Tokio.

### Дефекты текущего API

1. **Domain vs return type mismatch.** Функция в `Time`, возвращает
   только read-capability канала. Discovery плохой: чтобы найти
   «как сделать канал с таймером», надо угадать что искать в `Time`.

2. **Нет type safety.** `Time.after(1000)` — 1 сек или 1000 мкс? Bare
   int. `Duration.from_secs(1)` уже есть, но `Time.after` его не берёт.

3. **Capability mismatch с D91.** Получение `ChanReader[()]` через
   `Time.X` неявно подразумевает что Time владеет также `ChanWriter`
   (на самом деле — runtime). Нарушает D91 ментальную модель.

### Gaps относительно industry (что ДОЛЖНО быть, но нет)

4. **Нет explicit cancel.** Go `Timer.Stop()`, TS `clearTimeout(id)`,
   Tokio `drop(sleep)`. Сейчас нельзя отменить `Time.after` до срабатывания
   — таймер живёт до тика (или GC + libuv close). **Resource leak risk**
   в long-running workloads с jitter retry / opportunistic timeouts.

5. **Нет absolute deadline.** Tokio `sleep_until(Instant)`, Go
   `time.Until(deadline)` + `time.After(...)`. Без него jittered retries
   / scheduled tasks с фиксированным deadline'ом — clunky.

6. **Нет интеграции с D75 CancelToken.** Nova-specific преимущество:
   `supervised(cancel: tok)` уже есть. Timer должен наследовать cancel
   от parent scope автоматически. Сейчас не наследует — изоляция.

7. **Нет mockable time в тестах.** Tokio `pause()/advance(d)`, Jest
   `useFakeTimers()`. Nova имеет `Time` effect для мока часов
   (`with Time = mock { ... }`), но `Time.after` его **игнорирует**
   (идёт прямо в libuv). **Тесты с timing flaky** или требуют real sleep.

8. **Нет observability.** Tokio `tokio-console` показывает in-flight
   timers; Go expvar/runtime stats. Сейчас нет способа узнать сколько
   таймеров живёт + не leak'аем ли.

9. **Нет timer-wheel оптимизации.** Каждый `Time.after` = новый libuv
   `uv_timer_t` handle. На сотнях коротких таймеров (HTTP timeouts,
   retry budgets) — significant overhead vs Tokio's TimerEntry wheel
   или Go's runtime/timer heap.

### Real impact

- **16 call sites** в `nova_tests/concurrency/*` (7 файлов) — все используют
  bare int ms.
- **5+ упоминаний в spec D94** — учит неправильному паттерну.
- **`Time` effect mock не работает** для select-timeout тестов — нужен
  real sleep (flaky CI).
- **Plan 44.1 Ф.2 B7** уже добавил `on_select_lost` callback для cleanup
  не-winning arm; это **partial cancel**, но user-facing API нет.

---

## Industry parity table

| Capability | Go | Rust (tokio) | TS (Node) | Nova v1 (Time.after) | Nova v2 (Plan 65) |
|---|---|---|---|---|---|
| **One-shot timer-channel** | `time.After(d)` `<-chan time.Time` | `tokio::time::sleep(d).await` (Future) | `setTimeout(fn, ms)` | `Time.after(ms)` | `ChanReader.close_after(Duration)` |
| **Type-safe Duration** | `time.Duration` (typed) | `Duration` (typed) | ❌ number (ms) | ❌ int (ms) | ✅ `Duration` ⭐ |
| **Cancel before fire** | `timer.Stop() bool` | drop the Future / `Sleep::reset` | `clearTimeout(id)` | ❌ нет | ✅ `tok.cancel()` через D75 ⭐ |
| **Absolute deadline** | `time.Until(t)` + `After` | `sleep_until(Instant)` | manual `setTimeout(fn, t - Date.now())` | ❌ | ✅ `ChanReader.close_at(Instant)` (Ф.12) |
| **Periodic ticker** | `time.NewTicker(d)` + `.C` | `interval(d).tick()` | `setInterval(fn, ms)` | ❌ | ⚠️ `ChanReader.tick_every(Duration)` (Ф.13 sketch, MVP в Plan 66) |
| **Missed-tick policy** | drop (channel cap=1) | `MissedTickBehavior::{Burst,Delay,Skip}` | drop | n/a | follow Tokio: enum в `tick_every` (Plan 66) |
| **Mockable time в тестах** | manual (interface) | `tokio::time::pause()/advance(d)` | `jest.useFakeTimers()` | ❌ (Time effect ignored) | ✅ `with Time = mock { ... }` интегрирован (Ф.10) ⭐ |
| **Resource leak detection** | `runtime.NumGoroutine()` + profiling | `tokio-console` in-flight timers | `process._getActiveResources()` | ❌ | ✅ `NOVA_TIMER_METRICS=1` + bench-history (Ф.11) |
| **Timer wheel optimization** | runtime/timer heap (4-min) | TimerEntry wheel + bucket | libuv heap | libuv per-timer | ⚠️ libuv (Ф.0); wheel опционально (Plan 66) ✦ |
| **Drop semantics (GC)** | runtime cleanup | tokio handles via Drop | inspector tracks | unclear | ✅ explicit reader-drop → close libuv handle (Ф.2) |
| **Sub-ms precision** | ns native | ns native | ms only | ms input only | ns input, ms granularity (libuv); honest-doc (Ф.2) |
| **Cancel propagation в select** | manual `<-done` arm | `select! { _ = sleep => ..., _ = cancel => ... }` | `Promise.race([timer, abortPromise])` | ❌ | ✅ `tok.cancel()` будит ВСЕ pending waiters (D75 + Plan 44.1) ⭐ |
| **Spawn-aware** (timer тикает в worker, не в parent) | yes (goroutines) | yes (tokio runtime) | event loop | yes (libuv per-loop) | yes — preserved (Ф.8 cross-toolchain test) |

**⭐** = Nova-улучшение vs baseline (typed Duration, native CancelToken,
mockable through effect).
**✦** = parity gap, осознанный defer (libuv heap → wheel требует
self-host runtime, Plan 66+).

**Итог parity:** Plan 65 v2 закрывает Nova up to Tokio-уровня по 11/13
capabilities; на 12-м (timer wheel) — осознанный gap с roadmap. На 4
capabilities Nova объективно лучше (Duration type-safety, CancelToken
integration, Time-effect mock, select cancel propagation).

---

## Текущее состояние (verified 2026-05-18)

| Слой | Где | Что |
|---|---|---|
| Spec | `spec/decisions/06-concurrency.md` D94 + 5 упоминаний | `Time.after(d)` примеры с bare int |
| Compiler schema | `emit_c.rs:1042-1046` | `time_schema.insert("after", ([i64], "Nova_ChanReader*"))` |
| Codegen type inference | `emit_c.rs:18195-18198` | Member-call `Time.after` → `Nova_ChanReader*` |
| Runtime | `nova_rt.c` / channel impl | `nova_time_after_ms(int64)` — libuv `uv_timer_t` per call |
| Tests | 7 файлов в `nova_tests/concurrency/` | 16 call sites с ms-int literals |
| Plan doc | Plan 31 Ф.5, Plan 44.1 Ф.2 B7 | Historical reference, partial cleanup |

---

## Архитектурные решения

### AD1. New API: `ChanReader.close_after(d Duration) -> ChanReader[()]`

```nova
let t = ChanReader.close_after(Duration.from_secs(1))
select {
    Some(v) = rx.recv() => process(v)
    None    = t.recv()  => log_idle()
}
```

Namespace = capability возврата (D91). Метод описывает **механизм**
(канал закроется через d) — pattern-matches с `None = t.recv()` armом
в select.

### AD2. Atomic clean break (Time.after удаляется)

Bootstrap convention (Plan 60 size-accessor, Plan 47 D75 revision):
API переименовывается atomically. Diagnostic с machine-applicable
suggestion ловит legacy code. Нет deprecated alias (избегаем drift).

### AD3. Duration → ns в API, ms в runtime (с honest-doc)

User передаёт `Duration` (ns precision). Runtime конвертит в ms
(libuv ограничение). Sub-ms округляется **вверх** к 1ms (не вниз к 0).

```c
Nova_ChanReader* nova_chan_reader_close_after_ns(int64_t nanos) {
    if (nanos < 0) {
        nova_panic("ChanReader.close_after: negative duration: %lld ns", (long long)nanos);
    }
    if (nanos == 0) {
        return nova_chan_reader_already_closed();  // fast-path, no timer alloc
    }
    int64_t ms = (nanos + 999999) / 1000000;
    return nova_libuv_timer_close_after_ms(ms);
}
```

### AD4. Codegen: Duration unpacking inline (zero-alloc)

`Duration` — record `{ nanos i64 }`. Для `ChanReader.close_after(d)`
codegen эмитит `nova_chan_reader_close_after_ns(d.nanos)` — прямой
field access, без intermediate copy.

**Const-folding optimization (Ф.8):** literal `Duration.from_secs(1)`
→ compile-time const `1_000_000_000LL` → emit `nova_chan_reader_close_after_ns(1000000000LL)`. Plan 60 mono pipeline уже умеет это для
similar constants.

### AD5. Cancel через D75 CancelToken (Nova-unique advantage)

Go: `timer.Stop()` явный API на Timer object.
Tokio: `drop(future)` cancel'ит implicitly.
Nova: **integrate с existing D75 CancelToken** — timer наследует cancel
от родительского `supervised(cancel: tok)` scope.

```nova
let tok = CancelToken.new()
supervised(cancel: tok) {
    spawn fn() => {
        let t = ChanReader.close_after(Duration.from_secs(60))  // long timeout
        select {
            Some(v) = rx.recv() => process(v)
            None    = t.recv()  => log_timeout()
        }
        // если tok.cancel() из другого fiber'а:
        //   `t` (libuv timer) ОТМЕНЁН, recv'ы пробуждены, runtime cleanup;
        //   pre-existing D75 cancel-propagation работает.
    }
}

// elsewhere:
tok.cancel()  // → libuv timer закрывается без firing, no leak
```

**Реализация:** `ChanReader.close_after` регистрирует timer как
**cancel-aware resource** в current scope's `CancelToken` (если есть).
Plan 47 Ф.6 уже добавил cancel-resource hooks для channels — переиспользуем.

### AD6. Mockable time через Time effect (тестируемость)

Существующий `Time` effect (см. Plan 34 Ф.7 + Plan 22) умеет mock'ать
часы:

```nova
with Time = Test.mock_clock(start_ms: 1000) {
    let t = ChanReader.close_after(Duration.from_secs(5))
    Time.advance(Duration.from_secs(4))  // virtual time
    assert(t.is_closed() == false)
    Time.advance(Duration.from_secs(2))  // total +6s
    assert(t.is_closed() == true)
}
```

**Реализация:** runtime `nova_chan_reader_close_after_ns` проверяет
**effect-handler-bound** `Time` — если есть mock-handler, использует
virtual deadline + manual `Time.advance` triggering вместо libuv timer.
Real-clock path = default (если effect не bound).

Это закрывает **gap vs Tokio `pause()/advance(d)`**. AI-friendly: тесты
становятся deterministic.

### AD7. Drop / GC semantics — explicit cleanup

Когда `ChanReader[()]` GC-collect'ится (no live references), его
libuv timer **должен** быть закрыт без firing — иначе resource leak.

**Реализация:** `Nova_ChanReader` имеет finalizer (Boehm `GC_REGISTER_FINALIZER`),
который вызывает `uv_close(&timer)` на pending timer. Plan 27
(Boehm GC switch) уже обеспечивает finalizer infrastructure.

**Acceptance test:** spawn 10_000 `ChanReader.close_after(Duration.from_secs(60))`
без recv'ов, force GC, проверить что in-flight timer count ↓ к 0.

### AD8. Resource leak observability — `NOVA_TIMER_METRICS=1`

Env var включает per-process counter:

```
$ NOVA_TIMER_METRICS=1 nova test
...
NOVA TIMER STATS:
  alloc_total:       1234
  alloc_active:      0       (leak if > 0 after main exits)
  fired:             1200
  cancelled:         34       (via CancelToken or finalizer)
  longest_pending:   125 ms
```

Counters экспортируются в `nova bench` history (Plan 57) для
regression detection: «alloc_active > 0 после теста» → automatic flag.

### AD9. Timer-wheel — осознанный defer

libuv per-timer handle = ~120 bytes + 1 syscall per timer. Для idiomatic
loads (10-100 concurrent timers) — приемлемо. Для **HTTP-server с
10_000+ short timeouts** (per-request) — нужна timer-wheel.

**Решение:** в Plan 65 — libuv (single source path). В **Plan 66** —
custom timer-wheel (Tokio-style hierarchical bucketing) с runtime
benchmark gate: switch если concurrent timer count > N (config).

Документировано в honest-defer; bench `timer_alloc_throughput` (Ф.11)
запишет baseline для будущего сравнения.

### AD10. select-internals compatibility — no special-case

`ChanReader[()]` создаваемый через `close_after` — обычный ChanReader,
никакой special-case в select runtime. `Plan 44.1 Ф.2 B7 on_select_lost`
callback продолжает работать (cancel non-winning arm timer).

**Acceptance test:** `select_timer_stress.nv` (500 iter) passes
unchanged.

### AD11. Migration tool — token-aware с edge cases

| Pattern | Transform | Edge case handling |
|---|---|---|
| `Time.after(<INT_LIT>)` | `ChanReader.close_after(Duration.from_millis(<INT_LIT>))` | straight rewrite |
| `Time.after(<FLOAT_LIT>)` | `ChanReader.close_after(Duration.from_secs_f64(<FLOAT_LIT>))` | float → secs |
| `Time.after(<EXPR>)` non-literal | emit `// MIGRATE_MANUAL: Plan 65` + leave call | manual review (CI gate fails) |
| `Time.after` в string literal | **skip** (lexer aware) | regex would corrupt strings |
| `Time.after` в `//` или `///` comment | **skip** | preserve documentation/history notes |
| `Time.after` в `#cfg`-skipped block | **rewrite** | cfg-resolved AST, all code paths |
| `Time.after` в doc-test code block | **rewrite** | doc-tests compile (Plan 45 Ф.7) |

**Idempotency:** повторный запуск на migrated file = no-op.

**Dry-run mode:** `migrate_plan65 --dry-run` печатает planned changes
без записи; exit 0 если no changes, 2 если есть changes, 1 если есть
manual markers.

### AD12. Spec stability — `#stable(since)` bump policy

Новый API объявляется `#stable(since = "0.6")`. Поскольку Plan 65
переименовывает существующий публичный API (хоть и bootstrap), это
**breaking change**. Bump к 0.6 — конвенция: minor для breaking
до 1.0.

---

## Requirements (R1-R32)

### Core API (MVP, Ф.0-Ф.9)

**R1.** `ChanReader.close_after(d Duration) -> ChanReader[()]` доступен
без import (через prelude — если `ChanReader` там; иначе через
`std/concurrency/channel`).

**R2.** Принимает только `Duration`. Bare int → compile error +
machine-applicable fix-it suggestion.

**R3.** Канал closing **по истечении** d (monotonic deadline). recv()
вернёт `None` единожды; повторный recv → `None` (idempotent).

**R4.** Negative Duration → panic с указанием call site + actual value.

**R5.** Zero Duration → канал closed immediately (next recv returns
None без yield). Fast-path: no libuv timer allocation.

**R6.** Sub-ms duration → округление вверх к 1ms (libuv limit).
Документировано в `///` `# Performance`.

### Compiler

**R7.** `ChanReader.close_after` через external_registry (паттерн
`Channel.new`). Type-checker accept'ит только Duration.

**R8.** Codegen `emit_c.rs`:
   - `infer_expr_c_type`: `ChanReader.close_after(...)` → `Nova_ChanReader*`
   - Method dispatch lowering: `nova_chan_reader_close_after_ns(d.nanos)`
   - Const-folding для literal Duration (Ф.8)

**R9.** `Time.after(...)` диагностика E5101 с `SuggestedFix` (Plan 36
R7 structured diagnostic format):

```
error[E5101]: `Time.after` was removed in Plan 65 (D94 revision)
  --> select_test.nv:194:33
194|     Some(_) = Time.after(50) => { branch = 2 }
   |               ^^^^^^^^^^^^^^^ use `ChanReader.close_after(Duration)`
   = note: `Time.after(ms)` accepted raw integers — no unit safety.
   = note: `ChanReader.close_after(Duration)` is the capability-split
           replacement (D91 + D94 revision via Plan 65).
help: replace with `ChanReader.close_after(Duration.from_millis(50))`
194|     Some(_) = ChanReader.close_after(Duration.from_millis(50)) => { branch = 2 }
```

### Runtime

**R10.** Extern: `nova_chan_reader_close_after_ns(int64 nanos) -> Nova_ChanReader*`.

**R11.** Existing `nova_time_after_ms` → renamed `nova_internal_chan_close_after_ms`
(internal helper; не доступен из user code).

**R12.** Timer cleanup contract preserved (Plan 44.1 Ф.2 B7):
non-winning arm в select cancel'ит pending timer. Existing
`on_select_lost` callback remains.

**R13.** Finalizer-based cleanup (AD7): GC of unreferenced ChanReader
с pending timer → `uv_close()` без firing.

### CancelToken integration (AD5)

**R14.** `ChanReader.close_after` регистрирует timer в текущем
`CancelToken` scope (если bound). При `tok.cancel()`:
   - libuv timer закрывается без firing
   - все pending recv-waiters пробуждаются с None
   - cleanup atomic с другими cancellable resources

**R15.** Если нет cancel-scope — timer работает как сейчас (без
cancel-aware regsitration). Backwards-compat для top-level main.

### Time-effect mock (AD6)

**R16.** Runtime проверяет наличие `Time` effect-handler в current
fiber's effect-stack. Если есть mock handler — uses virtual clock:
   - `Time.now()` возвращает virtual ms
   - `Time.advance(Duration)` triggers due timers
   - Никаких libuv calls (deterministic, fast tests)

**R17.** Real-clock path = default (effect unbound). Production code
не платит за mock-check overhead (single load + branch-predictable).

### Negative-test scope (production-grade)

**R18.** `nova_tests/plan65/`:
   - `f1_close_after_int_neg.nv` — `EXPECT_COMPILE_ERROR`: bare int
     argument с проверкой что diagnostic содержит structured suggestion
   - `f2_time_after_removed.nv` — `EXPECT_COMPILE_ERROR`: legacy
     `Time.after` с проверкой E5101 + fix-it
   - `f3_negative_duration.nv` — `EXPECT_RUNTIME_PANIC` «negative duration»
   - `f4_zero_duration.nv` — positive: канал готов immediately
   - `f5_sub_ms_precision.nv` — positive: 500_000 ns → ≥ 1ms
     wait (verified via Time.now() pre/post)
   - `f6_huge_duration.nv` — overflow: `Duration.from_days(10_000)`
     → handle (либо compile-error либо runtime warn, не silent UB)
   - `f7_cancel_via_token.nv` — positive: `tok.cancel()` отменяет timer
     до срабатывания; assert no leak (NOVA_TIMER_METRICS)
   - `f8_mock_time_advance.nv` — positive: mock `Time` effect +
     `Time.advance(...)` deterministic firing
   - `f9_drop_no_leak.nv` — positive: 1000 ChanReader.close_after без
     recv, force GC, assert in-flight = 0
   - `f10_select_cancel_propagation.nv` — positive: `select` с close_after,
     parent scope cancel'ит → recv возвращает None
   - `f11_concurrent_timer_alloc.nv` — stress: 1000 concurrent
     close_after из разных fibers, no contention crash, all fire/cancel correctly

### Migration

**R19.** Migration tool processes std/, nova_tests/, examples/.
Spec/ — manual в Ф.6.

**R20.** Post-migration:
   - `grep -rn "Time\.after" std/ nova_tests/ examples/` → 0 hits
   - `nova test` → 0 regressions

**R21.** Migration tool **idempotent** (re-run = no diff).
**Dry-run mode** + exit code 1 если manual markers.

### Documentation

**R22.** `///` doc-comments на `ChanReader.close_after`:
   - One-line summary
   - `# Examples` block с select-pattern + CancelToken integration
   - `# Errors` block: negative Duration panic
   - `# Performance` note: libuv ms-granularity rounding +
     timer-wheel deferred
   - `# Testing` note: mock через `Time` effect
   - `#stable(since = "0.6")` badge

**R23.** doc-test через `nova doc --test`:
   - Positive: select с close_after, recv возвращает None после d
   - Cancel test: tok.cancel() → timer cancelled, no firing
   - Mock test: Time.advance deterministic

### Observability

**R24.** `NOVA_TIMER_METRICS=1` env var → counters exposed в:
   - `Time.timer_stats() -> TimerStats { alloc_total, alloc_active, fired, cancelled, longest_pending_ms }`
   - bench output (auto-snapshot per bench)
   - process-end summary (если env set)

**R25.** Counter `alloc_active > 0` после `main()` exit → log warning
«possible timer leak» с stack frames первых 10 leaked.

### Cross-toolchain (Plan 58 matrix)

**R26.** Build + test pass на Clang/MSVC/GCC. Critical: ns→ms conversion
uses portable arithmetic (int64, нет int128/compiler intrinsics).

**R27.** Bench `timer_alloc_throughput` (Plan 57) на каждом backend —
baseline для Plan 66 wheel сравнения.

### Compatibility

**R28.** Select internals unchanged (AD10) — `select_timer_stress.nv`
500-iter PASS unchanged.

**R29.** `Channel.new(cap)` / `ChanWriter.send` / `ChanReader.recv`
API не trogается.

**R30.** Plan 44.1 timer cleanup contract preserved (Ф.2 B7
`on_select_lost`).

### Spec / Project docs

**R31.** D94 amendment с «Эволюция» note + new examples.

**R32.** `docs/simplifications.md`:
   - mark `[M-time-after-bare-int]` RESOLVED
   - add `[M-libuv-ms-granularity]` honest-note
   - add `[M-timer-wheel-deferred]` roadmap → Plan 66

---

## Phases

### Ф.0 — Audit baseline (½ day) ✅ 2026-05-18

- [x] `nova test` baseline на main — **698 PASS / 0 FAIL / 44 SKIP**.
- [x] `grep -rn "Time\.after"` в std/, nova_tests/, examples/, spec/,
      docs/plans/ — 13 live call-sites in 7 test files, 12 spec refs,
      9 plan-doc refs.
- [x] Audit runtime extern — entry point is `Nova_Time_after(nova_int ms)`
      inline в `channels.h:1133`; no separate `nova_time_after_ms` symbol.
      Will be replaced by `nova_chan_reader_close_after_ns` directly in Ф.2
      (no rename intermediate alias).
- [x] Confirm `Duration` API completeness — all `from_*` methods present;
      `nanos` field is `readonly i64`.
- [⚠️] **Boehm GC finalizer infra** — `GC_REGISTER_FINALIZER` not currently
      wired; `alloc_boehm.c:17` says Boehm cooperation requires opt-in.
      `NovaAfterState` is `malloc`-owned (channels.h:1071-1084), not
      GC-managed. AD7 finalizer cleanup honest-deferred behind
      `[M-chanreader-gc-finalizer]`; f9 test scope adjusted to scope-exit
      cleanup (timer-fire OR on_select_lost), not GC-drop.
- [x] Confirm `Time` effect mock infra — `std/testing/handlers.nv`
      `th.fixed_ms` works for `now()`; close_after runtime hook for
      virtual deadline TBD in Ф.10.
- [x] Confirm CancelToken cancel-aware resource hook (Plan 47 Ф.6).
- [x] Записать baseline в `docs/plans/65-artifacts/baseline-2026-05-18.md`.

**Acceptance:** ✅ baseline.md captured; infra readiness summarized; one
honest-defer (`[M-chanreader-gc-finalizer]`) documented in Эволюция.

**Регрессия:** 698 PASS / 0 FAIL (baseline; matches main).

### Ф.1 — Compiler registration (1 day) ✅ 2026-05-18

- [x] `ChanReader.close_after(d Duration) -> Nova_ChanReader*` registered
      via hardcoded receiver dispatch in `emit_c.rs` (mirrors `Channel.new`
      pattern — not via external_registry because ChanReader is a
      compiler-builtin opaque type with no .nv decl). Both Member-form
      (`ChanReader.close_after(...)` parsed as `Member`) and Path-form
      (parsed as `Path(["ChanReader", "close_after"])`) handled.
- [x] `infer_expr_c_type`: both forms → `Nova_ChanReader*`.
- [x] `Time.after` registration unchanged — atomic switch deferred to Ф.5.
- [x] Side-fix (blocked Plan 65 progress): handler-method param annotation
      bridge in `emit_handler_lit` so a user-annotated record-typed param
      (e.g. `sleep(d Duration)`) can safely shadow the schema-wire int and
      access `d.nanos` without invalid C. Restricted to `eff != "Fail"`
      (Fail uses Plan 61 Ф.3 fail_e_map path) and `schema_ty == "nova_int"`
      (struct wire types cannot round-trip via `intptr_t`). Tracked as
      [M-handler-duration-schema-mismatch] in simplifications.md.

**Acceptance:** ✅ smoke `ChanReader.close_after(Duration.from_millis(10))`
compiles and runs inside a `supervised{ spawn { ... } }` fiber.

### Ф.2 — Runtime layer + drop semantics (1 day) ✅ 2026-05-18 (folded with Ф.1)

- [x] Implemented `nova_chan_reader_close_after_ns(int64_t)` inline in
      `channels.h` (alongside `Nova_Time_after`):
   - Negative → `fprintf+abort` panic (R4)
   - Zero → already-closed reader, no timer allocation (R5 fast-path)
   - Sub-ms → round-up to 1 ms (R6, libuv ms granularity)
   - Normal → delegate to existing `Nova_Time_after(ms)` libuv path
- [x] Internal rename SKIPPED — current runtime entry is `Nova_Time_after`
      (inline in channels.h), no separate `nova_time_after_ms` symbol exists.
      New `nova_chan_reader_close_after_ns` reuses the same backend without
      a renamed alias (cleaner — see Plan 65 Эволюция).
- [⚠️] **Finalizer registration (AD7)** — deferred. Boehm
      `GC_REGISTER_FINALIZER` is not wired anywhere in runtime (verified
      Ф.0). `NovaAfterState` remains `malloc`-owned with libuv-driven
      cleanup. Honest-defer [M-chanreader-gc-finalizer]; f9 test adjusted.
- [x] Codegen emit: `ChanReader.close_after(d)` →
      `nova_chan_reader_close_after_ns((d)->nanos)`. Duration record-unpack
      inline (AD4). Compile-time error on bare int (no `.nanos` field on
      `nova_int` — caught via Plan-65-specific Err return in codegen, not C
      compiler).
- [x] Smoke test in `nova_tests/plan65/smoke_close_after.nv` validates
      both normal (10 ms fires) and zero-duration (already-closed) paths.

**Acceptance:** ✅ runtime handles all in-scope cases; finalizer test
deferred behind [M-chanreader-gc-finalizer].

**Регрессия:** 700 PASS / 0 FAIL / 44 SKIP (baseline 698 + 2 smoke tests).

### Ф.3 — Negative-test fixtures (½ day) ✅ 2026-05-18

- [x] `nova_tests/plan65/f1_close_after_int_neg.nv` —
      `EXPECT_COMPILE_ERROR` bare int rejection с Plan 65 message.
- [x] `f2_time_after_removed.nv` — deferred to Ф.5 (Time.after still
      exists in parallel until Ф.5 atomic switch).
- [x] `f3_negative_duration.nv` — `EXPECT_RUNTIME_PANIC` "negative
      duration" — validates AD3 / R4 runtime panic path.
- [x] `f4_zero_duration.nv` — positive: 3 cases (from_nanos(0),
      from_secs(0), via Duration.from_nanos for the constant-access
      workaround). All verify R5 fast-path (no timer alloc).
- [x] `f5_sub_ms_precision.nv` — positive: 500_000 ns → ≥ 1 ms wait
      (Time.now() pre/post delta verified).
- [x] `f6_huge_duration.nv` — positive: from_days(10_000) +
      from_hours(1_000_000) — no overflow/panic.

**Acceptance:** ✅ 6 tests PASS (1 negative + 1 runtime-panic + 4 positive).

**Регрессия:** plan65/ suite — 6 PASS / 0 FAIL.

Note: Discovered orthogonal limitation — `Duration.ZERO` const-access via
Path-form does not propagate the record type into `infer_expr_c_type`
(returns `nova_int`). f4 works around by using `Duration.from_nanos(0)`.
Tracked: Plan 60 / Plan 53 follow-up territory, not Plan 65 scope.

### Ф.4 — Migration tool (1 day) ✅ 2026-05-18

- [x] `nova-cli/src/bin/migrate_plan65.rs` (~500 LoC):
   - Token-aware via `nova_codegen::lexer` (AD11) — strings + comments
     naturally skipped.
   - Rules: int literal → `Duration.from_millis`; float literal →
     `Duration.from_secs_f64`; non-literal → `/* MIGRATE_MANUAL ... */`
     comment + leave call (CI gate exits 1).
   - Preserves underscored literals (`10_000`) via span-based text extract.
   - Unary-minus on literal honoured.
   - Markdown mode (`--md`): walks ```nova fenced blocks + inline backticks.
- [x] 7 unit tests in `#[cfg(test)]`: int / float / manual marker /
      strings+comments skip / idempotent / negative / underscored. All PASS.
- [x] Idempotency: re-running on already-migrated source = no diff.
- [x] Bin registered in `nova-cli/Cargo.toml`.

**Acceptance:** ✅ tool migrates **13 call sites in 7 files** (Plan-doc
audit reported 16 with comments; pure executable calls = 13 — matches
Ф.0 baseline). 0 MIGRATE_MANUAL markers — full automatic migration.
Exit code semantics: 0/1/2 (idempotent/manual/changed).

**Регрессия:** unit tests 7/7 PASS; integration deferred to Ф.5 (atomic
switch will actually invoke `--apply`).

### Ф.5 — Atomic switch (½ day) ✅ 2026-05-18

- [x] Migration tool extended with auto-injection of
      `import std.time.duration` (Ф.4 follow-up) — required because
      migrated tests now reference Duration. 10/10 unit tests PASS.
- [x] Ran `migrate_plan65 --apply --paths nova_tests std examples`:
      **13 rewrites in 7 files**, 0 MIGRATE_MANUAL markers.
      `nova_tests/concurrency/{fiber_arena_compact, plan40_channel_hardening,
      plan40_perf_bench, select_closed_test, select_test,
      select_timer_cleanup, select_timer_stress}.nv` all migrated to
      `ChanReader.close_after(Duration.from_millis(N))` + import injected.
- [x] Removed `Time.after` registration from compiler:
   - `emit_c.rs:1045`: schema entry deleted (only `sleep`/`now` remain).
   - `emit_c.rs:18247`: Member-form type inference branch deleted.
- [x] Added E5101 diagnostic with structured fix-it suggestion in
      `emit_call`, both Member-form and Path-form guards. Format includes
      the migrated suggestion line (`ChanReader.close_after(Duration.from_millis(<arg>))`)
      built from the original arg-expr display, plus a pointer to
      `cargo run --bin migrate_plan65 -- --apply`.
- [x] New negative test `nova_tests/plan65/f2_time_after_removed.nv`
      verifies E5101 fires.

**Acceptance:** ✅ baseline preserved (705 PASS / 0 FAIL / 44 SKIP);
legacy diagnostic catches residual; 0 `Time.after` code-level call sites
remain in std/ nova_tests/ examples/ (only historical mentions in
comments survive — intentional).

**Регрессия:** 705 PASS / 0 FAIL / 44 SKIP (baseline 698 + 7 plan65).

### Ф.6 — Spec sync (½ day) ✅ 2026-05-18

- [x] D94 amend: 4 inline examples → new API (lines 93, 1320, 2706 plus
      §Timeout subsection full rewrite).
- [x] D94 «Эволюция API» sub-section: Plan 65 rationale + date + 3
      orthogonal defects + migration-tool pointer + forward-link to
      Plan 66 hardening.
- [x] D94 TOC line update (line 18).
- [x] Other concurrency.md sections (1124, 2619, 2758, 2771, 2783).
- [⚠️] Plan 31 / Plan 44.1 docs — not edited (historical plan docs are
      append-only — see Эволюция note in 06-concurrency.md provides the
      forward link).
- [⚠️] `spec/decisions/README.md` — not edited (no Plan 65 D-block added —
      the change is amendment to existing D94, not new decision).

**Acceptance:** ✅ spec examples all new API; «Эволюция» documented; only
historical/Эволюция-context `Time.after` mentions remain in
06-concurrency.md (intentional cross-references).

### Ф.7 — Stdlib documentation (½ day) ✅ 2026-05-18

- [x] Created `std/concurrency/timer.nv` doc-only stub. Since
      `ChanReader.close_after` is a compiler builtin (no .nv decl),
      this file hosts the canonical doc-comment surface for `nova doc`
      and AI agents searching stdlib. The actual lowering happens in
      `compiler-codegen/src/codegen/emit_c.rs` + the runtime helper in
      `nova_rt/channels.h::nova_chan_reader_close_after_ns`.
- [x] `///` doc-comments cover R22 sections: one-line summary, Examples,
      Errors (negative panic), Edge cases (zero/sub-ms), Performance
      (libuv granularity + Plan 66 wheel roadmap), Testing (mock-time
      planned for Ф.10), Migration (from Time.after + tool pointer).
      `#stable(since = "0.6")` badge present.
- [⚠️] doc-test embedded — code examples inside `///` blocks render
      via `nova doc`. doc-test execution (Plan 45 Ф.7) defers because
      this file's only Nova fn is a marker stub and doc-tests run a
      synthesized executable — out of scope until ChanReader becomes a
      real Nova decl.
- [x] `nova doc std/concurrency/timer.nv` renders without warning
      (verified manually).

**Acceptance:** ✅ doc-only file in place; `nova doc` renders clean.
Spec is the source of truth for the runtime contract (06-concurrency.md
D94 + Эволюция API).

### Ф.8 — Const-folding + cross-toolchain (½ day)

- [ ] Codegen optimization: literal `Duration.from_secs(N)` →
      compile-time const `N * 1_000_000_000LL`.
- [ ] Cross-toolchain matrix (Plan 58): Clang/MSVC/GCC build + test.
- [ ] Perf-sanity: `select_timer_stress` 500-iter на всех backend.

**Acceptance:** все toolchain PASS; const-fold verified in generated C.

### Ф.9 — Project docs MVP (½ day)

- [ ] `docs/project-creation.txt`: 2026-05-XX entry Plan 65 MVP closed.
- [ ] `docs/simplifications.md`:
   - `[M-time-after-bare-int]` RESOLVED
   - `[M-libuv-ms-granularity]` honest-defer note
   - `[M-timer-wheel-deferred]` → Plan 66 roadmap
- [ ] Plans README row Plan 65 update.

**Acceptance:** MVP scope (Ф.0-Ф.9) complete; ready для production hardening.

---

### **Production hardening phases (Ф.10-Ф.14) — выводят API на Tokio-уровень**

### Ф.10 — CancelToken integration + Time-effect mock (1.5 days)

- [ ] **Cancel hook (AD5):** при call `close_after` в supervised scope —
      зарегистрировать timer в `CancelToken.resources` list. Cancel
      handler закрывает libuv timer.
- [ ] Test: `f7_cancel_via_token.nv` (R18) — verify no leak post-cancel
      (NOVA_TIMER_METRICS check).
- [ ] **Mock time (AD6):** runtime check current fiber's Time-effect
      handler; if mock-bound, use virtual deadline + manual advance
      trigger вместо libuv.
- [ ] Test: `f8_mock_time_advance.nv` (R18) — deterministic firing.
- [ ] Test: `f10_select_cancel_propagation.nv` — full integration.
- [ ] Stdlib doc update: `# Cancellation` + `# Testing` sections.
- [ ] Migration of 1-2 existing flaky timing tests на mock-Time pattern.

**Acceptance:** 3 tests PASS; flaky timing tests stabilized.

### Ф.11 — Observability (1 day)

- [ ] Implement counters в runtime (R24): alloc_total / alloc_active /
      fired / cancelled / longest_pending_ms.
- [ ] `NOVA_TIMER_METRICS=1` env: enable + dump at process exit.
- [ ] `Time.timer_stats() -> TimerStats` API в stdlib.
- [ ] Bench-history integration: per-bench snapshot.
- [ ] Leak warning: `alloc_active > 0` post-main → log first 10 leak
      sources (best-effort stack capture).
- [ ] Bench `timer_alloc_throughput` (1000 timers, alloc+fire) — record
      baseline для Plan 66 wheel comparison.

**Acceptance:** metrics work; bench recorded; leak warning fires on
synthetic leak test.

### Ф.12 — `ChanReader.close_at(Instant)` — absolute deadline API (1 day)

- [ ] Add `ChanReader.close_at(deadline Instant) -> ChanReader[()]` —
      Tokio `sleep_until` parity, Go `time.Until(t) + After` shortcut.
- [ ] Runtime: convert `deadline - Time.now()` → ns → call `close_after_ns`.
- [ ] Edge cases:
   - past deadline (`deadline < now`) → already-closed reader, no leak
   - future deadline overflow ns → use ms fallback
- [ ] Tests: positive (5s in future) + negative (1s in past, immediate
      close) + edge (now exactly).
- [ ] Stdlib doc update.

**Acceptance:** `close_at` works; 3 tests PASS; documented.

### Ф.13 — `ChanReader.tick_every(Duration)` API sketch + namespace squat (½ day)

- [ ] **Не реализуем** в Plan 65 — это full periodic semantic (drop
      vs queue, MissedTickBehavior, jitter). Отдельный Plan 66.
- [ ] **Зарезервировать имя** в stdlib с `#unstable` + body вызывающим
      `panic("not implemented; see Plan 66")`. Цель: предотвратить
      collision с user code если кто-то определит `ChanReader.tick_every`
      в external crate.
- [ ] Spec D-block stub в spec/decisions/06-concurrency.md (D124?).
- [ ] Plan 66 outline создаётся отдельным commit'ом.

**Acceptance:** namespace зарезервирован; Plan 66 outlined.

### Ф.14 — Stress + concurrent timer alloc + final audit (1 day)

- [ ] `f11_concurrent_timer_alloc.nv` (R18): 1000 concurrent close_after
      из 10 fibers — TSan/ASan/UBSan clean под Linux Docker (Plan 44.1
      infra).
- [ ] Perf: alloc throughput vs Plan 22 sleep baseline (no regression
      > 5%).
- [ ] Cross-toolchain repeat + Linux validation.
- [ ] **Final audit pass** (Plan 60 паттерн): 25-point checklist:
   - API parity с table в этом плане
   - All `Time.after` references gone (грeп)
   - `Duration` всегда required (compile error на bare int)
   - CancelToken hook verified (leak test)
   - Mock-time deterministic (flake-free 100-run)
   - Drop finalizer no-leak (10k synthetic GC test)
   - Metrics counters correct (alloc==fired+cancelled+leaked)
   - Const-fold verified (codegen output inspected)
   - Cross-toolchain все backend
   - Stress 1000 timers no crash
   - select_timer_stress 500-iter unchanged
   - spec D94 fully migrated
   - doc-tests PASS
   - migration tool idempotent
   - dry-run mode works
   - E5101 diagnostic readable + actionable
   - Plan 66 namespace reserved
   - Project docs synced
   - Plans README updated
   - Bench history baseline recorded
   - Honest-defers documented в simplifications.md
   - Backward-compat nothing else touched
   - Plan-doc эволюция section
   - No new Rust crate deps
   - Self-host migration ready (no internal hardcodes)

**Acceptance:** все 25 пунктов ✅ → Plan 65 production-grade closed.

---

## Acceptance criteria (production-grade)

### MVP gates (Ф.0-Ф.9)

- [ ] `grep -rn "Time\.after" std/ nova_tests/ examples/` → **0 hits**
- [ ] `grep -rn "Time\.after" spec/ docs/plans/` — только historical
      эволюция notes
- [ ] `ChanReader.close_after(Duration)` enforces type (compile error
      на bare int)
- [ ] E5101 diagnostic + machine-applicable fix-it works
- [ ] `nova test` (release) — 0 regressions vs Ф.0 baseline
- [ ] Cross-toolchain: PASS на Clang / MSVC / GCC
- [ ] D94 spec amended; «Эволюция» note
- [ ] doc-test PASS
- [ ] Migration tool idempotent

### Production hardening gates (Ф.10-Ф.14)

- [ ] **CancelToken integration**: f7+f10 tests PASS; no leak post-cancel
- [ ] **Mock time**: f8 test deterministic; 100-run flake-free
- [ ] **Drop semantics**: f9 test (10k timers GC) — alloc_active → 0
- [ ] **Concurrent stress**: f11 (1000 timers, 10 fibers) — TSan/ASan
      clean Linux
- [ ] **Observability**: NOVA_TIMER_METRICS env works; leak warning
      fires correctly
- [ ] **`close_at`**: 3 tests PASS; past-deadline handled gracefully
- [ ] **Tick namespace squatted**: stub + Plan 66 outline
- [ ] **25-point final audit**: все ✅

### Industry parity gates

- [ ] **Type safety** (vs TS): ✅ Duration required, no bare ms.
- [ ] **Cancel** (vs Go `Timer.Stop`): ✅ через CancelToken.
- [ ] **Absolute deadline** (vs Tokio `sleep_until`): ✅ `close_at`.
- [ ] **Mock time** (vs Tokio `pause`): ✅ Time-effect handler.
- [ ] **Drop** (vs Tokio Future drop): ✅ finalizer + finalize test.
- [ ] **Observability** (vs `tokio-console`): ✅ NOVA_TIMER_METRICS.
- [ ] **Stress under concurrency** (vs Go runtime): ✅ TSan-clean 1k.
- [ ] **Cancel propagation в select** (vs Tokio select!): ✅ inherited.
- [ ] **Timer wheel** (vs Tokio TimerEntry): ⚠️ Plan 66 roadmap (honest).

---

## Open questions

1. **`#unstable` keyword exists?** Если нет — Ф.13 namespace squat
   через `#doc(unstable = true)` + body panic. Проверить в Ф.0.

2. **Boehm GC finalizer ordering guarantee?** Если finalizer order не
   deterministic, `uv_close` может race с libuv event loop teardown.
   Decision: register finalizer **только** для pending timers; closed
   timer skips registration. Confirm в Ф.2.

3. **Mock-time mixed с real-time fibers?** Если один fiber bound к
   mock Time, другой — нет, как себя ведут shared channels? Decision:
   per-fiber effect-handler scope (existing D-block) — корректно
   изолированы. Verify в Ф.10.

4. **`close_at(Instant)` vs `Time.now() + Duration`?** Идиоматичность.
   Decision: оба valid; `close_at` для absolute, `close_after` для
   relative. Document в `# Examples`.

5. **`NOVA_TIMER_METRICS` overhead?** Disabled-path = 1 branch.
   Enabled = atomic counter increment per alloc/fire/cancel. Bench в
   Ф.11 запишет overhead — если > 1% при disabled, fix.

---

## Что НЕ делает (out of scope)

- Periodic ticker `tick_every` — sketch only (Ф.13), full impl Plan 66
- Timer wheel optimization — Plan 66
- Other `Time` methods (sleep, now) — preserved as-is
- `Channel.new` / `ChanWriter.send` API — не trogает
- HTTP-server-specific timeout semantics (Plan 64 Ф.1)

---

## Связь

- **[D75](../../spec/decisions/06-concurrency.md#d75)** —
  `supervised(cancel:)` + CancelToken. Plan 65 hooks into D75
  для timer cancel.
- **[D91](../../spec/decisions/06-concurrency.md#d91)** — capability-split
  ChanReader/ChanWriter. Plan 65 — natural extension on static constructors.
- **[D94](../../spec/decisions/06-concurrency.md#d94)** — select syntax.
  Plan 65 amends examples (механизм select unchanged).
- **[Plan 22](22-sleep-libuv-integration.md)** — libuv timer infra.
  Reused.
- **[Plan 27](27-gc-switch.md)** — Boehm GC. Finalizer infra reused (AD7).
- **[Plan 31](31-channel-select.md)** — original select impl
  (Time.after Ф.5). Historical reference + select-internals preserved.
- **[Plan 34 Ф.7](34-stdlib-typecheck-and-compile-fix.md)** — mock_clock
  effect infra. Reused для Ф.10 mock-time.
- **[Plan 44.1 Ф.2 B7](44.1-channel-hardening.md)** — timer cleanup
  contract. Preserved (R30).
- **[Plan 45 Ф.7](45-nova-doc.md)** — doc-tests infrastructure (R23).
- **[Plan 47 Ф.6](47-supervised-cancel.md)** — cancel-aware resource
  hook. Reused for AD5.
- **[Plan 57](57-perf-benchmark-infrastructure.md)** — bench history
  для timer_alloc_throughput baseline (R27, Ф.11).
- **[Plan 58](58-cross-toolchain-msvc-verification.md)** — cross-toolchain
  matrix (R26, Ф.8).
- **[Plan 60](60-len-access-uniformity.md)** — atomic migration tool
  precedent (AD2, AD11). Final-audit checklist pattern (Ф.14).
- **[Plan 66](66-timer-wheel-and-tick-every.md)** — *future plan*:
  periodic ticker `tick_every` + custom timer-wheel runtime.
  Outline created в Plan 65 Ф.13.
- **[std/time/duration.nv](../../std/time/duration.nv)** — Duration API.

---

## Эволюция плана

- **2026-05-18 v1**: исходный план, 10 фаз (Ф.0-Ф.9), 5 dev-days. MVP
  scope: rename + Duration + atomic switch + spec sync.
- **2026-05-18 Ф.0 audit**: baseline captured 698 PASS / 0 FAIL.
  Discovery: Boehm `GC_REGISTER_FINALIZER` not currently wired anywhere in
  runtime (`alloc_boehm.c:17,113`). `NovaAfterState` is `malloc`-owned, not
  GC-managed, so AD7's "GC drop → uv_close finalizer" cannot be implemented
  without first introducing Boehm finalizer infrastructure project-wide.
  Honest-deferred behind `[M-chanreader-gc-finalizer]`. `f9_drop_no_leak.nv`
  acceptance shifts from "force GC → in-flight=0" to "exit scope → in-flight=0
  via on_select_lost + scope cleanup". True drop-on-GC remains a future
  task once Boehm finalizers are project-wide.
  Discovery: `nova_time_after_ms` (named in Plan doc Ф.2/R11) does not exist
  as an external symbol — current runtime entry is `Nova_Time_after`
  (inline in `channels.h`). Ф.2 will introduce
  `nova_chan_reader_close_after_ns` directly without rename alias.
- **2026-05-18 v2**: industry-parity audit vs Go/Rust/TS обнаружил 6
  production-grade gaps:
  - cancel (Go Timer.Stop)
  - absolute deadline (Tokio sleep_until)
  - mockable time (Tokio pause)
  - drop semantics (Tokio Future drop / Go GC)
  - observability (tokio-console)
  - timer-wheel (Tokio TimerEntry)
  Added 5 production hardening phases (Ф.10-Ф.14), ~5 extra dev-days.
  Total ~9-11 dev-days. 4 capabilities Nova оказываются объективно
  лучше baseline (typed Duration, native CancelToken, Time-effect mock,
  select cancel propagation). 1 gap осознанно defer'ится в Plan 66
  (timer-wheel).
