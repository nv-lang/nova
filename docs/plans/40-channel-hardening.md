// SPDX-License-Identifier: MIT OR Apache-2.0
# План 40: Channel hardening — production parity с Go/Rust

> **Статус (2026-05-12):** Ф.2 ✅, Ф.3 ✅ (включая B5-storage **без cap'а**), Ф.1 отложено.
> **Ф.3 (cleanup + storage refactor):**
> - ✅ B5 evolution (3 итерации):
>   - **v1:** cap 16→32 + compile-error на overflow.
>   - **v2:** cap 32→64 + adaptive storage `SelectSlot*`/`SelectWaiter*`.
>   - **v3 final:** **cap полностью убран.**
>     - `SelectCtx.arms` / `.waiters` → `SelectSlot*` / `SelectWaiter*`
>       (caller-provided storage).
>     - `emit_select` эмитит compound literal `SelectSlot _arms[n_ch];
>       SelectWaiter _waiters[n_ch];` на стеке fiber'а — размер literal
>       на codegen-time, MSVC-compatible (не VLA).
>     - `nova_select_try_immediate` использует `alloca(n*sizeof(int))`
>       для внутреннего `order[]` массива (Fisher-Yates shuffle) —
>       cross-platform (MSVC `<malloc.h>` / POSIX `<alloca.h>`).
>     - `NOVA_SELECT_MAX_ARMS` **полностью удалён** из кода. Никаких
>       compile-time или runtime ограничений на arm count.
>     - Stack frame ~80n байт + 4n байт (order) = ~84n байт на одну
>       select-операцию. На default minicoro 56 KB stack n=600+
>       безопасно. Реальные select'ы — 2-8 arms.
> - ✅ B9: Channel.new capacity-check **перед** alloc'ом.
> - ✅ B10: spec D94 sync.
>
> **Ф.2 (timer hardening, P2):**
> - ✅ B7: `Nova_ChannelState.on_select_lost` callback + `cancelled`
>   flag в `NovaAfterState`.
>
> **Ф.1 (M:N prerequisites, P1): в работе (2026-05-12).**
>
> Решение: реализовать B1+T2+B2 на Windows (single-thread regression
> = code correctness on uncontended path), затем валидировать на
> Linux через Docker + TSan (concurrent stress tests на pthread layer,
> обходя fiber scheduler). Это даёт real M:N race-detection без
> зависимости от Plan 23 M:N runtime.
>
> Storage refactor (B5 final) **уже сделан** — Ф.1 остаётся:
> atomics+mutex (B1) + doubly-linked (T2) + selectdone CAS (B2) +
> Linux/Docker prep + TSan validation + docs.
>
> **Detailed roadmap — §«Ф.1 Implementation Plan» ниже.**
> Обнаружен 2026-05-12 при audit'е Plan 30/31 после закрытия Plan 39.

---

## Ф.2 + Ф.3 implementation log (2026-05-12)

### Изменения runtime

**`compiler-codegen/nova_rt/channels.h`:**
- `Nova_ChannelState.on_select_lost` (function pointer) + `cleanup_data`
  (void*) — optional cleanup hook для каналов с background-resources
  (uv_timer). NULL для обычных каналов = zero overhead.
- `NovaAfterState.cancelled` (bool) — idempotent guard для cancel из
  multiple sources (select wake + timer cb race-window).
- `Nova_Time_after` сетит `pair.rx->state->on_select_lost =
  _nova_after_on_select_lost` + `cleanup_data = NovaAfterState*`.
- `_nova_after_on_select_lost` делает `uv_timer_stop` +
  `nova_chan_writer_close(tx)` + `uv_close` — idempotent через `cancelled`.
- `_nova_after_timer_cb` early-returns при `cancelled` (window между
  `uv_timer_stop` и event-loop dispatch'ем).
- `NOVA_SELECT_MAX_ARMS`: 16 → 32 с комментарием «Plan 40 Ф.1 заменит
  на per-call storage».
- `nova_select_park`: после `try_immediate` retry для определения winner'а,
  итерируется по всем enabled arms, для не-winner вызывает
  `arm->chan->on_select_lost(arm->chan)` если установлен.
- `nova_channel_new`: capacity-check **перед** allocate'ом (B9).

**`compiler-codegen/src/codegen/emit_c.rs:emit_select`:**
- Hard cap проверка: `n_ch > NOVA_SELECT_MAX_ARMS(=32)` → compile-error
  «select: too many channel arms (N); maximum is 32. Split into nested
  selects or refactor into separate operations.»

### Тесты

**Базовые** (commit 655033cce6):
- `nova_tests/concurrency/select_max_arms_boundary.nv` — positive, 32 arms.
- `nova_tests/expected_runtime/select_overflow_compile_error.nv` — negative,
  33 arms → compile-error pattern.
- `nova_tests/concurrency/select_timer_cleanup.nv` — 2 sub-tests:
  - 50 быстрых wins с активным Time.after — assert(wins==50).
  - Pure timeout case (`Time.after` wins) — assert(timeout_fired) для
    подтверждения что cleanup не сломал normal path.
- `nova_tests/expected_runtime/channel_zero_capacity_panic.nv` — `Channel.new(0)`
  → runtime panic.

**Focused functional + perf** (commit 4393e94105):
- `nova_tests/concurrency/plan40_channel_hardening.nv` (7 sub-tests):
  - B9 positive: `Channel.new(1)` / `Channel.new(1024)` с send/recv stream.
  - B5 positive: 1-arm fast path, 16-arm middle (старая cap, теперь mainstream).
  - B7 positive:
    - alternating recv-wins / timeout-wins (20 iterations) — validates
      что cleanup корректен для recv-wins И normal path работает для
      timeout-wins, без накопления state'а между ними.
    - 1ms timer — самый короткий timer wins.
    - 10s timer с recv-wins-fast — **критический тест cleanup'а**:
      без B7 supervised держал бы event-loop 10 секунд; с B7 закрывается
      за <200ms (assert).
- `nova_tests/concurrency/plan40_perf_bench.nv` (3 sub-tests):
  - **select dispatch throughput** — 1000 iterations × 4-arm select.
    Baseline для Ф.1 mutex overhead measurement в будущем.
  - **Time.after cleanup throughput** — 200 quick recv-wins с 5-секундным
    timer'ом каждый. Без B7 event-loop держал бы 200×5s=1000s+ timer'ов;
    с B7 — все 200 итераций + compile < 30s (assert).
  - **channel send/recv throughput** — 10000 ops через cap=1024 buffer.
    Baseline для Ф.1 mutex measurements.

**Validation:** все 6 файлов / 15 sub-tests прогнаны через release nova
(`nova-cli/target_alt/release/nova.exe`) — **15/15 PASS, 0 FAIL**.

**После B5 v3 final (no-cap):**
- `nova_tests/concurrency/select_many_arms.nv` (1 sub-test) —
  **100 arms на стеке**. Storage ~8 KB на одну select-операцию.
  Доказательство что cap полностью убран и compound-literal +
  alloca работают для значений >>32.
- `select_overflow_compile_error.nv` **удалён** — overflow-error
  больше не существует.
- Full regression: **257/257 PASS** + 46/46 std type-check.

### Regression

- **237/237 PASS** (231 baseline + 4 новых nova_tests, plus 2
  параллельных-агента pre-existing tests).
- **46/46 std type-check PASS.**
- **Zero regressions.**

### Что осталось открытым

- **B1/B2/B5-storage/T1/T2:** Plan 40 Ф.1 — атомарность + mutex +
  selectdone CAS + doubly-linked + per-call adaptive storage. Делать
  с Plan 23 M:N.
- **B4:** Time.after per-call allocs = ~6. Tokio 0-alloc через
  poll-state без channel. Post-1.0 optimization.
- **B8:** `recv_many` batch API. Post-1.0.

### Commits

TBD после написания.

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

## Открытые вопросы — закрыты в сессии 2026-05-12

- **Q-channel-mutex-impl:** Mutex (опция A) vs lock-free (опция B)?
  → **Решено:** Mutex через C11 `mtx_t` + `<stdatomic.h>` (опция A).
  MSVC поддерживает `<threads.h>` с VS2019 16.8+. Без отдельного
  wrapper-файла, без `#ifdef`-веток. Lock-free crossbeam-уровень —
  post-1.0 (требует TLA+/Loom verification).

- **Q-select-cap:** 16 arms hard, 32, или heap?
  → **Решено:** caller-provided storage через **compound literal в
  emitted коде** (опция C — лучший вариант, без VLA и без heap-alloc):
  ```c
  // codegen эмитит:
  SelectSlot   _arms[3];     // literal size, известный на compile-time
  SelectWaiter _waiters[3];  // ровно столько сколько нужно
  SelectCtx ctx = { .arms = _arms, .waiters = _waiters, .n_arms = 3, ... };
  nova_select_set_recv(&ctx, 0, ...);
  ```
  `SelectCtx.arms` становится `SelectSlot*` (не inline-массив).
  Plus: **временный safety cap = 32** в emit_select (`Err(...)` если
  больше) на случай неинициализированных edge-cases — снимется при
  Ф.1.
  - VLA отвергнут: MSVC не поддерживает (Plan 33.1 sweep требовал
    cross-toolchain).
  - Heap-alloc (Go-parity) отвергнут: 3 nova_alloc per select call →
    GC pressure.

- **Q-timer-pool:** уносим в Plan 22 follow-up или делаем здесь?
  → **Решено:** B7 (timer cleanup) — здесь в Ф.2. B4 (timer pool/
  per-call alloc count) — отдельный план как Plan 22 follow-up,
  не блокер.

---

## Detailed design Ф.1 (для сессии с Plan 23)

### Ф.1.1 — nova_rt/sync.h (новый файл)

```c
#ifndef NOVA_RT_SYNC_H
#define NOVA_RT_SYNC_H

#include <stdatomic.h>
#include <threads.h>

typedef mtx_t           nova_mutex_t;
typedef atomic_int      nova_atomic_int;
typedef atomic_bool     nova_atomic_bool;
typedef atomic_intptr_t nova_atomic_ptr;

#define nova_mutex_init(m)  mtx_init((m), mtx_plain)
#define nova_mutex_lock(m)  mtx_lock(m)
#define nova_mutex_unlock(m) mtx_unlock(m)
#define nova_mutex_destroy(m) mtx_destroy(m)
/* note: nova_alloc'd structs are zero-init by Boehm; mtx_init still
 * required для acquiring OS handle. Add to nova_channel_new path. */

#define nova_atomic_load(p)   atomic_load_explicit((p), memory_order_acquire)
#define nova_atomic_store(p, v) atomic_store_explicit((p), (v), memory_order_release)
#define nova_atomic_cas(p, expected, desired) \
    atomic_compare_exchange_strong_explicit((p), (expected), (desired), \
                                             memory_order_acq_rel, memory_order_acquire)
#define nova_atomic_inc(p)  atomic_fetch_add_explicit((p), 1, memory_order_acq_rel)
#define nova_atomic_dec(p)  atomic_fetch_sub_explicit((p), 1, memory_order_acq_rel)

#endif
```

### Ф.1.2 — Nova_ChannelState rewrite

```c
struct Nova_ChannelState {
    nova_mutex_t      mu;            /* held during всю send/recv/close logic */
    nova_atomic_bool  closed;        /* fast-path read без lock */
    nova_atomic_int   writer_count;  /* CAS on close для ref-count */
    nova_int*         buf;
    int64_t           cap;
    int64_t           head;          /* under mu */
    int64_t           count;         /* under mu */
    ChannelWaiter*    recv_waiters;  /* under mu; doubly-linked */
    ChannelWaiter*    send_waiters;  /* under mu */
};
```

### Ф.1.3 — Doubly-linked ChannelWaiter

```c
struct ChannelWaiter {
    NovaFiberQueue*    scope;
    int                slot;
    Nova_ChannelState* channel;  /* NULL = unlinked */
    bool               is_recv;
    nova_int           send_val;
    ChannelWaiter*     prev;     /* NEW: doubly-linked */
    ChannelWaiter*     next;
    nova_atomic_int    fired;    /* NEW: selectdone CAS (0=pending, 1=fired) */
};

static inline void _nova_channel_waiter_unlink(ChannelWaiter* w) {
    /* assumes channel->mu held */
    if (!w->channel) return;
    Nova_ChannelState* st = w->channel;
    ChannelWaiter** head = w->is_recv ? &st->recv_waiters : &st->send_waiters;
    if (w->prev) w->prev->next = w->next;
    else         *head = w->next;
    if (w->next) w->next->prev = w->prev;
    w->channel = NULL;
}
```

### Ф.1.4 — Selectdone CAS protocol

Channel wake helper меняется с «pop value, wake fiber» на «CAS fired flag, wake fiber»:

```c
static inline void _nova_channel_wake_recv(Nova_ChannelState* st) {
    /* assumes mu held */
    ChannelWaiter* w = st->recv_waiters;
    while (w) {
        ChannelWaiter* next = w->next;
        int expected = 0;
        if (nova_atomic_cas(&w->fired, &expected, 1)) {
            _nova_channel_waiter_unlink(w);
            nova_sched_wake(w->scope, w->slot);
            return;
        }
        /* waiter already fired (won другую arm в select) — skip */
        w = next;
    }
}
```

При wake select-waiter'а **не извлекаем значение из буфера**. Просыпающийся select видит `fired=1` на своём waiter'е → знает `arm_idx` → читает значение из буфера атомарно (под mu).

### Ф.1.5 — SelectCtx с pointer storage

```c
typedef struct {
    SelectSlot*     arms;       /* caller-provided storage, см. emit_select */
    SelectWaiter*   waiters;    /* caller-provided */
    int             n_arms;
    int             which;
    nova_int        recv_val;
    NovaFiberQueue* scope;
    int             slot;
} SelectCtx;

static inline SelectCtx nova_select_init_v2(int n_arms,
                                              SelectSlot* arms_storage,
                                              SelectWaiter* waiters_storage) {
    SelectCtx ctx;
    ctx.arms = arms_storage;
    ctx.waiters = waiters_storage;
    ctx.n_arms = n_arms;
    ctx.which = -1;
    ctx.recv_val = 0;
    ctx.scope = NULL;
    ctx.slot = -1;
    /* zero-fill ровно n_arms слотов */
    for (int i = 0; i < n_arms; i++) {
        ctx.arms[i].chan = NULL;
        ctx.arms[i].is_recv = false;
        ctx.arms[i].send_val = 0;
        ctx.arms[i].guard = false;
        ctx.arms[i].wildcard = false;
    }
    return ctx;
}
```

### Ф.1.6 — emit_select changes

Codegen эмитит storage:

```c
{
    SelectSlot   _sel_arms[3];      // literal n_ch
    SelectWaiter _sel_waiters[3];
    SelectCtx ctx = nova_select_init_v2(3, _sel_arms, _sel_waiters);
    nova_select_set_recv(&ctx, 0, ...);
    // ...
}
```

Safety cap = 32 остаётся (теоретическая защита от runaway codegen);
hard error на overflow убирается, потому что storage now adaptive.

### Acceptance Ф.1

- Concurrent send/recv stress test: pthread × 8 threads × 1M ops →
  no data race (TSan под GCC/Clang).
- selectdone CAS race test: 2 fibers concurrent recv на 1 channel + 1 sender →
  ровно 1 fiber wins (никогда 2, никогда 0).
- O(1) waiter unlink: benchmark N_arms=64 × 10000 selects → linear scaling.
- 231/231 single-thread regression PASS.
- 46/46 std type-check PASS.

### Risks для Ф.1

- C11 `<threads.h>` на MSVC требует VS2019 16.8+. Чек: project-creation.txt
  упоминает поддержку MSVC, надо проверить версию build farm'а.
- `_Alignas(8)` на atomic'ах для Boehm GC scan alignment.
- Inline-эмиссия storage увеличит stack frame fiber'а на ~80n байт;
  для n=32 это 2.5 KB при default minicoro stack 56 KB — приемлемо.

---

## Ф.1 Implementation Plan (2026-05-12, согласован)

Подход: реализовать на Windows (single-thread regression = uncontended
path correctness), валидировать на Linux через Docker + TSan
(concurrent stress tests на pthread layer, обходя fiber scheduler).
Это даёт real M:N race-detection без зависимости от Plan 23 runtime.

### Этап 1 — sync.h (✅ done in session)

- `compiler-codegen/nova_rt/sync.h` создан: C11 `mtx_t` + atomics
  wrappers (`nova_mutex_*`, `nova_aint_*`, `nova_abool_*`).
- Подключён в `nova_rt.h` перед `channels.h`.
- C11 `<threads.h>` + `<stdatomic.h>` работают на нашем clang
  (verified test).

**Acceptance:** ✅ build clean.

### Этап 2 — B1: atomics + mutex на Nova_ChannelState

**Изменения `channels.h`:**
- `Nova_ChannelState` поля:
  - `nova_mutex_t mu` — protects buf/head/count/waiter-lists.
  - `nova_atomic_bool closed` — fast-path read без lock'а.
  - `nova_atomic_int writer_count` — CAS-based decrement в close().
  - `count`/`head`/`buf` остаются обычными int64_t — под mutex.
  - waiter-lists остаются обычными pointer — под mutex.
- `nova_channel_new`: `nova_mutex_init` + atomic init.
- Все operations (`recv`/`send`/`try_*`/`close`/`clone`):
  - `closed` check — fast-path через `nova_abool_load`.
  - Modify operations — внутри `nova_mutex_lock`/`unlock` block.
- Layout `ChannelWaiter`/`SelectWaiter` неизменён (Этап 3 трогает).

**Acceptance:**
- Build clean.
- 257/257 single-thread regression PASS.
- Plan 40 functional + perf tests PASS.
- ~150 строк, 1 commit.

### Этап 3 — T2: doubly-linked waiter list

**Изменения `channels.h`:**
- `ChannelWaiter.prev` (новое поле) + `SelectWaiter.prev` в
  layout-compatible позиции (первые N полей всё ещё match'аются
  через cast).
- `_nova_channel_waiter_unlink` → O(1) через prev/next pointer swap.
  То же для `_nova_sel_waiter_unlink`.
- Все enqueue paths (recv/send park, select park):
  - `new->prev = NULL` (head-insert).
  - Если `*head != NULL`: `(*head)->prev = new`.
  - `new->next = *head; *head = new;`
- Wake helpers (`_nova_channel_wake_recv/send`) корректно обновляют
  `head` и его `prev = NULL` (новый head).

**Acceptance:**
- Build clean.
- 257/257 regression.
- New sub-test в `plan40_perf_bench.nv`: 64-arm select × 1000 iter —
  linear (не quadratic) scaling.
- ~120 строк, 1 commit.

### Этап 4 — B2: selectdone CAS

**Изменения `channels.h`:**
- `SelectWaiter.fired` (новое поле, `nova_atomic_int`). `ChannelWaiter`
  не получает `fired` (одиночный waiter, race невозможен).
- Channel wake helpers (`_nova_channel_wake_recv/send`) — protocol
  change:
  - Old: «pop first waiter, extract value, wake fiber».
  - New: iterate через waiters, для каждого CAS `fired: 0→1`. Если
    waiter — `SelectWaiter` (распознать через extended header?) —
    CAS-mark и продолжить (не extract). Если обычный `ChannelWaiter` —
    pop+extract+wake как раньше.
  - **Унификация:** если каждый waiter имеет fired (включая
    ChannelWaiter с always-0-no-CAS), путь единый. Это упрощает.
- `nova_select_park` после wake:
  - Iterate `ctx->waiters[i]`. Тот с `fired=1` — winner.
  - Читает value из канала **под mutex'ом** (B1 lock acquired).
  - Updates `ctx->which` + `ctx->recv_val`.
- Unlink остальных waiters (T2 doubly-linked даёт O(1) на каждый).

**Acceptance:**
- Build clean.
- 257/257 regression.
- ~250 строк, 1 commit.

### Этап 5 — Linux/Docker preparation

**Решение (согласовано):** vcpkg на Linux (тот же `vcpkg.json`,
добавить `x64-linux` triplet).

**Файлы:**
- `docker/Dockerfile`: Ubuntu 22.04 + clang-15 + cmake + vcpkg setup
  + bdwgc install + libuv build.
- `docker/Dockerfile.tsan`: тот же base + `CFLAGS=-fsanitize=thread`
  для тестовых run'ов.
- `docker/build-and-test.sh`: build nova-cli + run 257/257.
- `docker/README.md`: usage instructions.

**Acceptance:**
- `docker build -f docker/Dockerfile -t nova-test .` собирает.
- `docker run nova-test` прогоняет 257/257 на Ubuntu/clang.
- ~100 строк, 1 commit.

### Этап 6 — TSan validation

**pthread-based stress tests (direct C, обходят fiber layer):**
- `nova_tests/plan40_tsan/b1_mutex_stress.c` — 8 threads × 100k
  concurrent send/recv на shared channel. Validates B1 mutex.
- `nova_tests/plan40_tsan/b2_selectdone_race.c` — 2 threads park'ятся
  через select на один channel, 1 sender → assert ровно 1 fired.
  Validates B2 CAS.
- `nova_tests/plan40_tsan/t2_waiter_churn.c` — high-frequency
  enqueue/unlink на doubly-linked list. Validates T2.

**Запуск:**
- `docker build -f docker/Dockerfile.tsan -t nova-tsan .`
- `docker run nova-tsan ./test_b1_mutex_stress` etc.
- Expected: exit 0, нет TSan warning'ов.

**Acceptance:**
- 3 stress tests PASS под TSan.
- ~300 строк, 1 commit.

### Этап 7 — Documentation

- Plan 40: status Ф.1 → ✅ (все 4 prerequisites закрыты:
  storage ✅ + B1 ✅ + T2 ✅ + B2 ✅).
- Spec D94: M:N safety 🟡 → ✅ (с TSan validation).
- project-creation.txt + simplifications.md + discussion-log:
  implementation log.
- 1 commit.

### Сводка объёма

| Этап | Файл/строк | Commit |
|---|---|---|
| 1 sync.h | new ~70 | (в этой сессии) |
| 2 B1 | channels.h ~150 | 1 |
| 3 T2 | channels.h ~120 | 1 |
| 4 B2 | channels.h ~250 | 1 |
| 5 Docker | docker/ ~100 | 1 |
| 6 TSan | nova_tests/plan40_tsan/ ~300 | 1 |
| 7 Docs | 5 docs files | 1 |
| **Итого** | **~990 строк, ~10 файлов** | **6 commits** |

### Honest disclosure

- Этапы 2-4 (B1/T2/B2) — single-thread regression validates **uncontended
  correctness path**. Real race detection — Этап 6 (TSan на Linux).
- Этап 7 закрывает Ф.1 как «channel layer M:N-safe, integration с fiber
  scheduler — pending Plan 23».
- Plan 23 (M:N runtime) — отдельный план, intermixing scheduler + Plan 40
  channel layer. Plan 40 Ф.1 — prerequisite, не Plan 23 в целом.

### Open questions

- **`SelectWaiter` layout-compatible с `ChannelWaiter`:** после
  добавления `fired` к SelectWaiter и `prev` к обоим — первые N полей
  должны быть identical. Возможно потребуется reordering.
- **fired в ChannelWaiter:** добавить и игнорировать (always-load-0) для
  unified wake protocol, или оставить два path'а? Решение — в Этапе 4
  по простоте.
- **Boehm GC + atomics alignment:** `_Alignas(8)` на atomic_int полях
  для x86 atomicity guarantees. Проверить в bench.

---

## Полный аудит channels.h перед Ф.1 (2026-05-12)

После re-read'а channels.h (681 строка) выявлены **все race window'ы**
которые нужно закрыть в B1+T2+B2. Это **расширяет scope** по сравнению
с первым видением «just wrap ops in mutex».

### Race window'ы под M:N (все требуют B1 mutex)

1. **`nova_chan_writer_clone`** (channels.h:317) — `tx->state->writer_count++`
   non-atomic increment. Решение: `nova_aint_inc(&st->writer_count)`.

2. **`nova_chan_writer_close`** (channels.h:295-314) — sequence:
   `writer_count--` → `if (writer_count > 0) return` → `closed = true`
   → wake all waiters. Под M:N два thread'а могут одновременно
   decrement; race на `closed=true` + wake loop. Решение: atomic
   `fetch_sub` с проверкой результата `== 0`; затем взять lock и
   wake под lock'ом (waiter list trav).

3. **`is_closed()` calls** на fast-path (`nova_chan_reader_recv` line 177,
   `nova_chan_writer_send` line 242, и др.) — read без lock'а OK для
   atomic_bool, но **последующая операция** (fetch from buffer) должна
   быть под lock'ом, иначе race на `count`/`head`.

4. **`stop_cb`** (`_nova_channel_waiter_stop_cb`, `_nova_select_waiter_stop_cb`)
   — iterate waiter list **без lock'а**. Под M:N producer может одновременно
   wake первый waiter, unlink его, а stop_cb видит inconsistent state.
   Решение: `nova_mutex_lock(&w->channel->mu)` в начале stop_cb.
   **Caveat:** stop_cb может быть вызван из scheduler-internal context,
   где lock уже взят — нужен **trylock**-вариант или re-entrant mutex.
   Требует анализа sched.h API на момент Этапа 2.

5. **`nova_select_park`** (channels.h:519-605) — регистрирует waiter в
   каждом channel'е без lock'а. Между регистрацией waiter 1 и 2 producer
   может wake (потому что одна arm уже ready) → fiber wakes раньше, чем
   waiter 2 registered → waiter 2 «orphan» в channel'е после select exit.
   Решение: lock каждого канала на момент регистрации в нём.

6. **`_nova_channel_wake_recv/send`** (channels.h:151-169) — pop+mutate
   buffer без lock'а. Решение: внутри mutex (вызывать из под lock'а
   caller'а).

### Atomic fields в Nova_ChannelState (B1)

```c
struct Nova_ChannelState {
    nova_mutex_t      mu;            /* B1: protects everything below */
    nova_int*         buf;
    int64_t           cap;
    int64_t           head;          /* under mu */
    int64_t           count;         /* under mu */
    nova_atomic_bool  closed;        /* B1: atomic — fast-path read без lock */
    nova_atomic_int   writer_count;  /* B1: atomic CAS-decrement в close */
    ChannelWaiter*    recv_waiters;  /* under mu; T2: doubly-linked */
    ChannelWaiter*    send_waiters;  /* under mu; T2: doubly-linked */
    void           (*on_select_lost)(Nova_ChannelState*);
    void*             cleanup_data;
};
```

### Waiter layout (T2 + B2)

```c
struct ChannelWaiter {
    /* первые 6 полей MUST match SelectWaiter — channel wake helpers
     * cast'ят ChannelWaiter* и читают scope/slot/channel. */
    NovaFiberQueue*    scope;
    int                slot;
    Nova_ChannelState* channel;
    bool               is_recv;
    nova_int           send_val;
    ChannelWaiter*     next;
    ChannelWaiter*     prev;       /* T2: NEW — doubly-linked O(1) unlink */
    nova_atomic_int    fired;      /* B2: NEW — для unified wake protocol */
};

struct SelectWaiter {
    /* layout-compatible: первые 8 полей идентичны */
    NovaFiberQueue*      scope;
    int                  slot;
    Nova_ChannelState*   channel;
    bool                 is_recv;
    nova_int             send_val;
    struct SelectWaiter* next;
    struct SelectWaiter* prev;
    nova_atomic_int      fired;
    /* select-only: */
    int                  arm_idx;
};
```

**fired в ChannelWaiter:** добавляем для unified wake protocol. На
single-thread CAS быстрый (lock cmpxchg ≈ 5 ns). Worth simplifying
two-path code. Initial value `0`; wake helper CAS'ит `0→1`.

### Wake protocol после B2

`_nova_channel_wake_recv(st)`:
1. Walk `st->recv_waiters` (head→tail).
2. Для каждого waiter: CAS `fired: 0→1`.
3. Если CAS succeeded: unlink (O(1) через T2 doubly-linked) + extract
   value if SelectWaiter (через arm_idx → channel value already in buf)
   OR commit buffer change if ChannelWaiter (как раньше) + nova_sched_wake.
   Break loop.
4. Если CAS failed (waiter уже fired by select-race): continue к next.
5. Если все waiters fired: data остаётся в буфере, ждёт следующего recv.

`select_park` после wake: iterate own waiters, найти fired=1, прочитать
value из канала под mutex'ом, set `which`/`recv_val`.

### Stack frame impact

`ChannelWaiter` после T2+B2: +16 байт (prev pointer + atomic_int с alignment).
`SelectWaiter` после T2+B2: +16 байт.

`SelectCtx storage` на стеке (Plan 40 Ф.3 final): `n × sizeof(SelectSlot)`
+ `n × sizeof(SelectWaiter)` = `n × (40 + 56)` = `~96n` байт. На 56 KB
fiber stack — n=580+ безопасно. Раньше было 84n; рост незначительный.

### Scope estimates (после re-audit)

| Этап | Было | После re-audit |
|---|---|---|
| 2 B1 | ~150 строк | **~250 строк** (включая close+clone atomics, stop_cb lock, select_park lock на регистрацию) |
| 3 T2 | ~120 строк | ~120 (без изменений) |
| 4 B2 | ~250 строк | **~300 строк** (unified wake protocol для обоих waiter types) |
| **Итого Ф.1 code** | ~520 | **~670 строк** |

### Решено в re-audit

- **`fired` в обоих waiter types** (унификация wake protocol).
- **stop_cb под lock**: использовать `mtx_lock` напрямую; если sched.h
  держит lock на caller side — это покажется в Этапе 2 при build/test.
- **select_park регистрация под lock**: lock каждого канала отдельно
  на время регистрации waiter в нём. Не deadlock (один lock в момент
  времени).
- **Alignment:** atomic_int — 4 байта, на x86 lock-free на 4-byte
  aligned addresses. Без `_Alignas(8)` обходимся (atomic_bool ─ 1 байт,
  atomic_int ─ 4 байта; их natural alignment достаточно).
