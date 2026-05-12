// SPDX-License-Identifier: MIT OR Apache-2.0
# План 40: Channel hardening — production parity с Go/Rust

> **Статус:** план, не начат. **P1 blocker для Plan 23 (M:N runtime)**;
> отдельные мелкие фиксы — P2/P3.
> Обнаружен 2026-05-12 при audit'е Plan 30/31 после закрытия Plan 39.

**Цель.** Закрыть production-gaps в `compiler-codegen/nova_rt/channels.h`,
обнаруженные post-close audit'ом Plan 30/31 относительно Go runtime
(`runtime/chan.go`) и Rust (`std::sync::mpsc`, `crossbeam::channel`,
`tokio::sync::mpsc`). Большинство gap'ов сегодня **не проявляются**
благодаря single-thread runtime'у — но Plan 23 (M:N) их немедленно
обнажит.

---

## Контекст / триггер

Plan 30 (send→bool + tx.clone()) и Plan 31 (select) закрыты как
✅ ЗАКРЫТО. Post-close review (Plan 30 Ф.4, commit `88504b87c`)
закрыл 4 дефекта (Б1/Б2/Н1/Н2), но оставил T1/T2/T3 как «tech debt».

Re-audit 2026-05-12 нашёл **дополнительные** gap'ы и переоценил
приоритеты:

| ID | Gap | Текущий приоритет | Реальный |
|---|---|---|---|
| T1 (был) | `writer_count` не atomic | «Plan 23» | **P1 blocker для Plan 23** |
| T2 (был) | WaiterList O(n) unlink | «при нагрузочных» | **P1** (O(N²) на select) |
| T3 (был) | try_recv None различение | «после generics» | оставляем как есть |
| B2 (новое) | Wake race в select retry | — | **P1 blocker для M:N** |
| B5 (новое) | NOVA_SELECT_MAX_ARMS=16 silent skip | — | **P1** (silent зависание) |
| B7 (новое) | Time.after timer leak risk | — | P2 |
| B9 (новое) | Channel.new capacity-check после alloc | — | P3 (косметика) |
| B10 (новое) | Spec D94 не обновлён | — | P3 (документация) |
| B4 (новое) | Time.after alloc per call | — | P2 |
| B8 (новое) | recv_many batch API | — | P3 |

---

## Scope

### Ф.1 — P1 blockers для M:N (Plan 23 prerequisite)

#### B1 — atomics для shared state

`writer_count`, `closed`, `count`, `head`, `recv_waiters`, `send_waiters`
сейчас обычные `int32_t` / `bool` / pointer. Под M:N **любая operation
на канале → data race**.

Go: `chan.go` использует `lock_t` (внутренний mutex) + ranged atomics
для closed flag. Rust mpsc: `Mutex<Inner>` + `Condvar`. crossbeam:
lock-free через atomics + epoch GC.

**Решение** — две опции:

- **A. Mutex-based** (паритет с Rust std mpsc, проще):
  - `Nova_ChannelState` получает `nova_mutex_t mu` (cross-platform wrapper
    на pthread_mutex / SRWLOCK).
  - Все мутации внутри `nova_mutex_lock/unlock`.
  - `closed` атомарно через `atomic_bool` для fast-path `is_closed()`
    без lock'а.
  - **Cost:** 2-3% overhead на send/recv. Acceptable.

- **B. Lock-free** (паритет с crossbeam):
  - SPSC fast path через atomic head/tail.
  - MPMC через CAS на enqueue slot.
  - **Cost:** очень сложно правильно реализовать, нужна формальная
    верификация (TLA+ / Loom).

**Рекомендация:** **A** для bootstrap, оставить B как post-1.0
optimization. crossbeam потратил 5+ лет на lock-free; мы не успеем.

#### B2 — race-free select wake

В `nova_select_park` после `nova_sched_park` идёт retry
`nova_select_try_immediate(ctx)` (channels.h:558). На M:N между wake
и retry **другой fiber-thread может выхватить значение из буфера**
→ try_immediate retry не найдёт ничего, `ctx->which = -1`.

Сегодня не проявляется (single thread между wake и retry нет других
инструкций). Под M:N — **silent select dispatch failure**.

**Решение** (Go-style `selectdone`):
- `SelectWaiter` получает `atomic_int fired` (0=pending, 1=fired).
- Channel wake helper делает CAS `fired: 0 → 1`. Первый выигрывает.
- Wake **не извлекает** значение из буфера, только маркирует waiter.
- Просыпающийся select смотрит `fired`-flag в **своём** waiter'е,
  знает arm_idx → читает значение из буфера атомарно (внутри lock'а
  из B1).

Это меняет protocol channel↔select wake — нетривиальный refactor.

#### B5 — NOVA_SELECT_MAX_ARMS overflow

Сейчас `nova_select_set_recv/set_send` на `n >= 16` делает silent
`return` (channels.h:375, 385). Parser (`parse_select`) и codegen
(`emit_select`) **не валидируют** arm count. Результат: `select { 17 arms }`
→ 17-я arm не зарегистрирована → select висит вечно, ожидая её ready.

**Решение** — две опции:

- **A. Compile-time error** (рекомендация): в `emit_select` проверить
  `n_ch <= NOVA_SELECT_MAX_ARMS`, выдать diagnostic с line:col. Cap
  поднять до 32 если 16 окажется тесно. Простой fix, ~10 строк.

- **B. Heap-allocate SelectCtx** (Go-parity): `nova_alloc(sizeof(SelectCtx) + n*sizeof(SelectSlot))`,
  убрать static `arms[16]`. ~50 строк, GC pressure +1 alloc per select.

**Рекомендация A** — 16 arms покрывает 99% реальных кейсов; для редкого
кейса heavy-fan-in пользователь явно увидит сообщение и сможет
рефакторить (или мы поднимем cap).

#### T2 — O(n) waiter unlink

`_nova_channel_waiter_unlink` (channels.h:84) и `_nova_sel_waiter_unlink`
(channels.h:466) — singly-linked traversal. После `select_park` мы
unlink'аем N waiter'ов из N каналов → **O(N²)** на каждый select dispatch.

При N=8 (типичный select) это 64 указательных hop'а — приемлемо.
При тысячах select'ов в секунду на больших fan-in'ах — заметно.

**Решение:** doubly-linked list. Каждый `ChannelWaiter` / `SelectWaiter`
получает `prev`-pointer. Unlink — O(1).

**Cost:** +8 bytes на waiter, +1 store на enqueue. Justified.

### Ф.2 — P2 hardening (после M:N runtime)

#### B7 — Time.after timer cleanup

Когда select выиграл по другому arm'у, `Time.after`-timer **всё равно
сработает** (через uv_timer_cb), попытается `try_send` в already-discarded
channel. NovaAfterState heap-allocated, GC может его собрать раньше
чем timer fire.

Под Boehm GC сейчас работает (Boehm сканирует libuv `timer.data`
указатель). Под malloc-only fallback — **leak** до конца программы.

**Решение:**
- `Time.after` возвращает не только `rx`, но и `cancel_handle` (skip
  на Nova-уровне — codegen генерирует cleanup в exit path select'а).
- Альтернатива: register timer в `NovaFiberQueue->timers` list, cancel
  при scope exit. Сложнее.

#### B4 — Time.after pool

Каждый `Time.after(ms)` allocate:
- `Nova_ChannelPair` (state + buf + tx + rx) = 4 allocs.
- `NovaAfterState` = 1 alloc.
- `uv_timer_init` — heap внутри libuv.

= ~6 allocs per timeout. Tokio достигает 0 allocs через poll-state
без channel.

**Решение** — отложить до Plan 22 follow-up (timer pool в eventloop.h).
Не блокер.

### Ф.3 — P3 cleanup

#### B9 — Channel.new capacity check ordering

Сейчас (channels.h:113-125): alloc Nova_ChannelState, alloc buf,
**потом** check `capacity <= 0` → throw. Если throw — buf и state
утекают (GC eventually соберёт, но это lazy).

**Решение:** переставить check **перед** alloc. ~3 строки.

#### B10 — spec D94 обновить

Plan 31 Definition of Done содержит `[ ] D94 в spec обновлён (TODO)`.
План закрыт как ✅, но TODO открыт.

**Решение:** обновить spec/decisions/09-tooling.md (или где D94)
актуальной реализацией: `_ = rx` vs `Some(v) = rx`, all-closed panic,
Time.after семантика.

#### B8 — recv_many batch API

Tokio 1.32+ добавил `recv_many(&mut Vec<T>, limit)` — забирает все
доступные сообщения за один lock-cycle. Latency win для batched
consumers (logging, metrics).

**Решение:** `Nova_ChanReader.recv_many(limit int) -> []T` после M:N
(нужен lock для atomic batch).

---

## Зависимости

- **Plan 23 (M:N runtime)** — Ф.1 этого плана **обязателен**
  prerequisite. Без atomics + race-free wake M:N не работает.
- **Plan 22 (libuv eventloop)** — closed, базис для B4/B7.

---

## Acceptance criteria

**Ф.1 — M:N prerequisites:**
- Все каналы операции thread-safe (loom-style test через
  pthread + concurrent send/recv не падает).
- `select` на 17 arms → compile-error с понятным сообщением.
- O(1) waiter unlink (benchmark: `n_arms × n_selects` linear, не
  quadratic).
- `selectdone`-style race-free wake (concurrent select race-test
  PASS).
- 191/191 nova_tests + 45/45 std type-check без регрессий.

**Ф.2 — hardening:**
- Time.after timer cleanup verified (no leak в long-running test
  на 100k таймаутов).
- (отложено в Plan 22 follow-up) timer pool.

**Ф.3 — cleanup:**
- Channel.new check-before-alloc.
- spec D94 актуален.
- (опционально) recv_many.

---

## Не входит

- **SPSC lock-free** (crossbeam-style). Post-1.0.
- **Channel rendezvous** (capacity=0). Plan 30 явно отказал.
- **Generic T payload** через `nova_int`. T3 из Plan 30 — отдельная
  задача после generics monomorphization.
- **`fan_in` / `fan_out` высокоуровневые helpers** в std/concurrency/.
  Отдельный план.

---

## Сравнение Go / Rust / Nova после Ф.1

| Концепт | Go | Rust mpsc | crossbeam | Nova (после Ф.1) |
|---|---|---|---|---|
| Thread-safe ops | mutex+atomic | Mutex+Condvar | lock-free | mutex+atomic ✅ |
| Select fairness | Fisher-Yates | — | sel ranking | Fisher-Yates ✅ |
| Select wake race | selectdone CAS | — | epoch GC | selectdone CAS ✅ |
| Waiter unlink | O(1) doubly-linked | O(1) | O(1) | O(1) ✅ |
| Recv timeout | pooled timer | poll-state | — | per-call alloc (B4) |
| Bounded select arms | unlimited | — | unlimited | 16 + diagnostic |
| Cancel timer | runtime.timer | Sleep::reset | — | scope cleanup (B7) |

После Ф.1 — production parity с Rust mpsc / Go (lock-free crossbeam-уровень
— отдельная цель).

---

## Файлы

- `compiler-codegen/nova_rt/channels.h` — основные изменения
  (atomics, mutex wrapping, doubly-linked waiters, selectdone).
- `compiler-codegen/nova_rt/sync.h` — новый или extend (nova_mutex_t
  cross-platform wrapper).
- `compiler-codegen/src/codegen/emit_c.rs:emit_select` — B5
  diagnostic.
- `compiler-codegen/nova_rt/eventloop.h` — B4/B7 если делается.
- `nova_tests/concurrency/` — race tests, fan-in stress, select
  overflow negative.
- ~600-1000 строк для Ф.1.

---

## Риски

- **Mutex overhead на single-thread** — добавит ~2-3% на send/recv.
  Mitigation: можно сделать `nova_mutex_t` no-op при `--gc malloc`
  + single-thread mode (если Plan 23 не активен). Сложность не
  стоит — лучше принять overhead.
- **selectdone protocol change** — channel wake helpers (`_nova_channel_wake_recv/send`)
  меняют семантику (mark, не commit). Все callers (recv/send/try_*)
  должны адаптироваться. Большой surface area.
- **Boehm GC + atomics** — Boehm не знает про atomic_load/store, но
  они работают как обычные memory ops на L1-aligned данных. Risk:
  alignment на 32-bit полях. Mitigation: `_Alignas(8)` на atomic'ах.

---

## Связанные планы

- [Plan 21](21-channel-revision-implementation.md) — Channel
  capability-split base.
- [Plan 23](23-mn-runtime-roadmap.md) — M:N runtime, Ф.1 этого плана
  prerequisite.
- [Plan 30](30-channel-improvements.md) — send→bool + tx.clone();
  post-close review зафиксировал T1/T2/T3, этот план их расширяет.
- [Plan 31](31-channel-select.md) — select; B2/B5/B10 — newly
  обнаруженные gap'ы.

---

## Открытые вопросы

- **Q-channel-mutex-impl:** Mutex (опция A) vs lock-free (опция B)?
  Рекомендация — A, но финальное решение перед Ф.1 start.
- **Q-select-cap:** 16 arms hard, 32, или heap? Bench needed.
- **Q-timer-pool:** уносим в Plan 22 follow-up или делаем здесь?
