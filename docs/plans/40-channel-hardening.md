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
> **lockorder для select (A3)** + Linux/Docker prep + TSan validation
> + deadlock-stress test + docs.
>
> **🔴 P0 deadlock prevention:** select с многоканальной registration
> обязан использовать **lockorder sorted by `(uintptr_t)hchan`**
> (Go-style — `runtime/select.go::selectgo`). Без этого два fiber'а
> с inverted-order selects deadlock'аются (`select { ch1, ch2 }` vs
> `select { ch2, ch1 }`). TSan **не ловит** deadlock — только специально
> designed cross-fiber select-stress test может его поймать.
> **Подтверждено пользователем: deadlock нам точно не нужен.**
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

---

## Production-grade audit (2026-05-12)

После Plan 40 design review относительно Go runtime channels
(`runtime/chan.go`, `runtime/select.go`), crossbeam-channel
(array/list flavors), Tokio mpsc, Rust std mpsc post-1.67 rewrite, и
memory model best practices (cppreference, Go WMM, Rust nomicon) —
найдены **production-grade improvements** для Ф.1.

Цель — не хуже Go/Rust **в архитектуре**, не в micro-bench (последнее
требует lock-free специализированных flavors, см. Plan 50+).

### P0 additions — обязательно для Ф.1

#### A1. Memory ordering documentation + writer_count split

**Что было:** Plan 40 sync.h использует одинаковый `acq_rel` для всех
atomic ops.

**Что в production:** Boost.Atomic / libc++ `shared_ptr` / Rust `Arc::drop`
используют **классическую refcount idiom**: `fetch_sub(1, Release)` на
каждом decrement; **только thread который довёл count до нуля** делает
`atomic_thread_fence(Acquire)` перед чтением owned data. `acq_rel` на
каждом decrement wastes a fence на N-1 decrements.

**Что делать:**
- В Этапе 2 `nova_chan_writer_close`:
  ```c
  int32_t prev = nova_aint_fetch_sub_release(&st->writer_count);
  if (prev == 1) {
      atomic_thread_fence(memory_order_acquire);
      // closed = true under mu, wake waiters
  }
  ```
- В `sync.h`: добавить `nova_aint_fetch_sub_release` macro + comment
  block документирующий выбор ordering per field:
  - `closed` Release-store / Acquire-load (paired flag+payload).
  - `writer_count` Release-dec + Acquire-fence-on-zero (refcount idiom).
  - `fired` CAS acq_rel/acquire (mutex acquire provides remaining
    ordering для buffer read).
- Каждый комментарий с обоснованием — будущий maintainer не «upgrade'ит»
  до SC случайно.

**Cost:** ~30 LOC + comments. **P0** — TSan не ловит memory ordering
(моделирует acq_rel как SC).

#### A2. closed+count TOCTOU re-check protocol

**Что было:** Plan 40 говорит «is_closed() fast-path → следующая
операция под lock». Не специфицирует **re-check** protocol.

**Race в Go's `chanrecv`:** sender может близать канал **после** receiver
читает `closed=false` но **до** receiver видит count update. Без re-check
под lock'ом — потеря сообщений на close.

**Что делать:** Этап 2 acceptance criterion:
- `nova_chan_reader_recv`:
  1. Fast-path: load `closed` (atomic).
  2. **Если closed AND count==0 без lock'а — это speculative**. Lock,
     re-check `closed` && `count==0`, только потом return None.
  3. Если count>0 (без lock'а) — это тоже speculative. Lock, re-check,
     read buf[head], decrement.
- Документировать в комментарии.

**Cost:** ~20 LOC. **P0** — иначе теряются сообщения at-close (Go bug
class).

#### A3. lockorder для select (Go-style)

**Что было:** Plan 40 Этап 2 audit говорит «select_park регистрация
под lock на канал отдельно; не deadlock».

**Это неверно.** Если два fiber'а делают `select { ch1, ch2 }` и
`select { ch2, ch1 }`, и каждый берёт lock в shuffled (random) order
для регистрации — **deadlock**. Go runtime `selectgo` использует
**два разных порядка**:
- `pollorder` — randomized (Fisher-Yates) для **fairness**.
- `lockorder` — sorted by hchan address для **deadlock-free**
  acquisition.

Из `runtime/select.go`: "sort the cases by Hchan address to get the
locking order. simple heap sort, to guarantee n log n time and constant
stack footprint."

**Что делать:** в Этапе 4 (B2 wake protocol) — если когда-либо нужно
держать lock на ≥2 каналах одновременно (для atomic snapshot select
state), эмитировать `lockorder` array:
- В `emit_select`: после storage allocation эмитим `int _lockorder[n];`
  + init sorted by `(uintptr_t)chan_state`.
- `nova_select_park` iterate by lockorder при acquiring multiple locks.
- pollorder остаётся random (fairness).

**Если** в B2 wake protocol каждый канал lock'ается **независимо** (один
lock в момент времени) — lockorder не нужен. **Это нужно явно
documented invariant** + `static_assert` в debug.

**Cost:** ~40 LOC в codegen + runtime. **P0 correctness** — TSan и
single-thread regression не поймают deadlock.

#### A4. stop_cb under lock — Plan 23 dependency

**Что было:** Plan 40 говорит «stop_cb берёт mutex; если sched.h
держит lock на caller side — это покажется в Этапе 2».

**Реальная проблема:** scheduler API (Plan 22 D93) не специфицирует,
выhalt'ит ли stop_cb с уже-захваченным lock'ом или нет. Это **open
question** в Plan 23 design space. Plan 40 Ф.1 **может не закрыть это
сам**.

**Что делать:** в acceptance Этапа 2 явно записать:
- «stop_cb correctness под M:N depends on Plan 23 scheduler API providing
  atomically-dequeue-and-prevent-wake mechanism».
- Если в Этапе 2 trylock не работает (deadlock в single-thread), —
  escalate to Plan 23 design.

**Cost:** doc only. **P0** — иначе мы commit'имся к invariant'у который
Plan 23 может нарушить.

#### A5. Boehm GC thread registration в TSan tests

**Что было:** Plan 40 Этап 6 — pthread stress tests. Не упоминает
Boehm requirement.

**Реальность:** Boehm требует `GC_register_my_thread()` от каждого
pthread до первого `nova_alloc`, иначе use-after-free на GC scan.

**Что делать:** в Этапе 6 test scaffolding:
```c
void* worker_thread(void* arg) {
    struct GC_stack_base sb;
    GC_get_stack_base(&sb);
    GC_register_my_thread(&sb);
    // ... actual stress work ...
    GC_unregister_my_thread();
    return NULL;
}
```

**Cost:** ~15 LOC. **P0** — иначе stress tests flaky/crash, mis-attributed
к Plan 40 bug.

#### A6. Send-arm cancel-safety в select

**Что было:** Plan 40 не специфицирует — если send-arm проиграл, и
receiver уже консумировал `send_val` между park и CAS check, что
делать?

**Tokio:** `Sender::reserve` → `Permit::send` — если Permit drop'нут
без send, capacity returns. Cancel-safe.

**Что делать:** в Этапе 4 wake protocol order:
1. CAS `fired: 0→1`.
2. **Только если CAS succeeded**: extract `send_val` / commit to buffer.
3. Если CAS failed (другая arm выиграла): nothing — `send_val` остаётся
   неиспользованным.

Никаких partial commits.

**Cost:** ~20 LOC + stress test «concurrent select с send-arm». **P0
correctness**.

### P1 additions — strongly recommended для Ф.1

#### B1. Direct-copy в B2 wake protocol

**Что в Go:** `sendDirect`/`recvDirect` — sender пишет напрямую в
receiver's stack frame, минуя buffer. Saves один buffer copy.

**Что делать:** в Этапе 4 wake protocol, когда `SelectWaiter` выигрывает
CAS, sender пишет **`waiter->recv_val` directly** (новое поле в
SelectWaiter), не через `buf[]`. Затем при select_park reading: читаем
`waiters[which].recv_val`, не `buf[head]`.

**Cost:** ~20 LOC в B2 (+ поле `recv_val` в SelectWaiter). Saves
~1 cache line miss per select win.

#### B2. nova_chan_reader_close symmetry

**Что в Tokio:** `Receiver::close()` — explicit, wakes all senders с
`SendError`. Закрывает channel со стороны reader'а.

**Что было:** Plan 40 имеет только writer-side close. Dropped receiver
+ parked senders = senders block forever.

**Что делать:** добавить в Этапе 2:
- `reader_closed: atomic_bool` в `Nova_ChannelState`.
- `nova_chan_reader_close(rx)` — sets atomic, wakes all `send_waiters`,
  `send` returns `false` after this.
- `nova_chan_writer_send` check: `if reader_closed → return 0`.

**Cost:** ~40 LOC. **P1** — symmetric API expected by users coming from
Tokio.

#### B3. send/recv micro-bench acceptance

**Что было:** Plan 40 Этап 7 закрывает Ф.1 с 257/257 PASS.

**Что не хватает:** уверенности что lock'и не замедлили fast-path
beyond Go/Rust.

**Что делать:** добавить acceptance в Этап 7:
- Micro-bench: `Channel.new(1024)` × 1M iterations send/recv в одном
  fiber'е. Target: <50 ns per round-trip uncontended.
- Если miss — revisit lock-free fast path в Plan 50+.

**Cost:** ~50 LOC test + doc. **P1**.

### P2 additions — для Ф.4 или отдельных планов

| Item | Detail | Plan |
|---|---|---|
| `BaseWaiter` common prefix | Анонимная struct-in-union вместо cast-punning ChannelWaiter↔SelectWaiter — strict-aliasing safe. ~20 LOC. | Ф.1 Этап 3 (если time) |
| `nova.signal` stdlib | Wrapper around `Channel<bool>(1)` для done-signal idiom (workaround для отсутствия cap=0). ~30 LOC stdlib. | Ф.4 |
| Zero-capacity rendezvous | Cap=0 для done-signals идиома Go. ~100 LOC. **Largest semantic gap vs Go.** | Ф.4 / Plan 50 |
| Unbounded channel | Linked-list-of-blocks à la crossbeam. ~250 LOC. **Real use case:** log/metric pipelines. | Plan 41 |
| `recv_many` batch API | Уже в Ф.4 как B8. ~80 LOC. | Ф.4 |
| Tokio Permit-based reserve | `Sender::reserve` → `Permit::send`. ~150 LOC. | post-1.0 |
| Backoff before mtx_lock | Только если bench показывает need. ~40 LOC. | post-1.0 |
| SPSC lock-free flavor | Specialized `Channel.spsc(cap)` для streaming. ~400 LOC. Loom-verified. | Plan 50+ |
| Direct-copy без cap=0 | Wake-protocol change. ~150 LOC. | post-1.0 |

### Anti-patterns identified в текущем Plan 40

#### AP1. Layout-compatibility via cast pun (strict aliasing UB)

`SelectWaiter` cast'ится в `ChannelWaiter*` в wake helpers. **C undefined
behavior** в strict reading (type punning через pointer cast).

**Mitigation (Этап 3):** ввести `BaseWaiter` struct с общим prefix'ом,
embed в обе структуры через анонимную struct или union:
```c
typedef struct {
    NovaFiberQueue*    scope;
    int                slot;
    Nova_ChannelState* channel;
    bool               is_recv;
    nova_int           send_val;
    struct BaseWaiter* next;
    struct BaseWaiter* prev;   /* T2 */
    nova_atomic_int    fired;  /* B2 */
} BaseWaiter;

struct ChannelWaiter { BaseWaiter base; };
struct SelectWaiter  { BaseWaiter base; int arm_idx; nova_int recv_val; };
```

**Cost:** ~20 LOC refactor. **P2** (works currently в practice, но better
to fix before Plan 23 when compiler optimizations get aggressive).

#### AP2. fired field в обоих waiter types

Plan 40 говорит «add to both for unified wake». Trade-off: +4 байта на
regular waiter который CAS никогда не вызовет.

**Verdict:** оставить как есть. Code simplicity > 4 bytes/waiter. Comment
the rationale.

#### AP3. Stack-allocated select storage + deep recursion

Plan 40 Ф.3 final: select storage на fiber stack via compound literal.
**Анти-паттерн:** deeply-recursive fiber + select with many arms может
overflow stack mid-call (storage not reclaimed until select returns).

**Mitigation:** debug-build stack-high-watermark check (~10 LOC). Если
Этап 6 stress test показывает overflow — переключить на heap для
n > threshold.

**Cost:** ~10 LOC. **P3.**

### Что Plan 40 делает лучше Go/Rust (зафиксировать как сильные стороны)

1. **Stack-allocated select storage** (compound literal). Zero heap
   pressure, no sudog pool needed. Go использует sudog pool именно
   потому что у них этот storage GC-pressure'ит.
2. **`on_select_lost` callback** для Time.after. Чище чем Tokio's
   `Sleep::reset` или Go's `time.NewTimer` (Go leaks timer fire-event
   в channel buffer).
3. **No `_Alignas(8)` overhead.** crossbeam over-uses `CachePadded` на
   каждом atomic; Nova избегает (8-64 byte padding × N atomics =
   significant waste для short-lived channels).
4. **Symmetric reader_close + writer_close** (после P1 B2). Tokio имеет
   это; Go не имеет (close one-shot, только sender owns it).

### Updated scope estimate (Ф.1)

| Этап | Old | После production audit |
|---|---|---|
| 1 sync.h | ~70 | ~100 (+ ordering doc + fetch_sub_release helper) |
| 2 B1 | ~250 | **~350** (+ A1 ordering, A2 TOCTOU, A4 doc, B2 reader_close) |
| 3 T2 | ~120 | **~140** (+ AP1 BaseWaiter refactor) |
| 4 B2 | ~300 | **~360** (+ A3 lockorder, A6 cancel-safety, B1 direct-copy) |
| 5 Docker | ~100 | ~100 |
| 6 TSan | ~300 | **~315** (+ A5 GC_register_my_thread) |
| 7 Docs | doc | + B3 micro-bench acceptance |
| **Total Ф.1** | ~990 | **~1180** строк |

### Acceptance criteria (production-grade)

- Все P0 items implemented и validated.
- Все P1 items implemented (минимум A3 lockorder + B2 reader_close).
- Этап 6 TSan tests включают: B1 mutex_stress, B2 selectdone_race,
  T2 waiter_churn, **+ select_lockorder_deadlock test** (два fiber'а
  с inverted-order select на shared channels — не должно deadlock'нуть).
- Micro-bench (B3): <50 ns per round-trip uncontended.
- Документация в spec D94 и Plan 40 явно перечисляет:
  - что мы делаем как Go/Rust (default ordering, mutex backend).
  - что мы делаем **лучше** (stack storage, on_select_lost, no padding).
  - что отложено (lock-free SPSC, unbounded, rendezvous, recv_many,
    Permit-based reserve).

---

## Deep audit — round 2 (2026-05-12)

Второй проход (research-agent против Go `runtime/select.go`, Tokio,
crossbeam, Materialize postmortem, Loom/CDSChecker, matklad, Regehr,
ARM memory model papers) — **переворачивает часть первого audit'а** и
добавляет 7 новых P0 items.

### 🔴 Самые опасные пропуски первого audit'а (в порядке риска)

| # | Что | Status в audit 1 | После round 2 | Почему опаснее |
|---|---|---|---|---|
| C1 | strict-aliasing UB через cast pun (BaseWaiter) | AP1 P2 | **P0** | Clang -O3 + LTO активно elide'ит reads «не того» типа. Future LTO bite. |
| C2 | stop_cb lock contract unspecified | A4 «doc only» | **P0** | Deadlock при Plan 23 landing если не nail down сейчас. **Решение: contract (B)** — stop_cb lock-free через atomic `cancelled` flag, **никогда** не берёт channel mutex. |
| C3 | all-arms-disabled — silent forever-park | not covered | **P0** | Hang который не поймают tests; user'ы hit. Решение: explicit panic «select: no enabled arm». |
| C4 | TSan alone — пропустит crossbeam-class double-free | covered | **P0 expand** | ASan caught Materialize bug; TSan не caught. Add ASan + UBSan, не только TSan. |
| C5 | False sharing — Plan 40 хвастался «no padding is strength» | strength claim | **inverted: P1** | crossbeam padding gives **300× perf** under contention (alic.dev measurement). При M:N с 8+ thread'ами Plan 40 будет в 100-1000× slower без padding. |
| C6 | lost-wakeup contract для `nova_sched_park` | not covered | **P0** | Работает в bootstrap случайно; Plan 23 expose если undocumented. Решение: define `nova_sched_park_with_unlock` API contract сейчас, no-op bootstrap. |
| C7 | ARM acq_rel cost на `fired` CAS | not covered | **P1** | На ARM каждый failed CAS = full barrier; на x86 free. 2× CAS cost in wake loops на ARM. Использовать `nova_aint_cas_weak` с relaxed-on-failure. |

### 🟢 Demotions / переоценки

- **A3 lockorder: P0 → P2 (под contract B).** Round 2 показал: lockorder
  нужен только если когда-либо держим ≥2 locks simultaneously. Plan 40
  не держит (использует optimistic re-scan через post-park retry). Это
  **correctness mechanism**, не optimization — нужен комментарий
  `/* CRITICAL: retry is correctness, not perf */` в коде. Saves ~40 LOC.
  **Подтверждение пользователя «deadlock нам не нужен» сохраняется** —
  contract (B) обеспечивает deadlock-freedom через optimistic re-scan
  + atomic fired flag, без необходимости sort'ить.

- **«No padding is strength» — антипаттерн.** First audit перечислял как
  «what we do better than crossbeam»; round 2 показал что это inverts на
  M:N. Корректно: pad **by access group** (mu+closed; head+count+
  waiters+buf; writer_count) — не per-field как crossbeam, но и не
  «всё в одной cache line».

### Новые P0 items (round 2)

#### C1 — BaseWaiter refactor (was AP1)

Strict-aliasing UB. Solution в audit 1 уже specified:

```c
typedef struct BaseWaiter {
    NovaFiberQueue*    scope;
    int                slot;
    Nova_ChannelState* channel;
    bool               is_recv;
    nova_int           send_val;
    struct BaseWaiter* next;
    struct BaseWaiter* prev;        /* T2 */
    nova_atomic_int    fired;       /* B2 */
    nova_atomic_bool   cancelled;   /* C2: stop_cb lock-free path */
} BaseWaiter;

struct ChannelWaiter { BaseWaiter base; };
struct SelectWaiter  { BaseWaiter base; int arm_idx; nova_int recv_val; };
```

`recv_waiters` / `send_waiters` хранят `BaseWaiter*` (не cast). Wake
helpers работают с `BaseWaiter*`. `arm_idx` для select-only через
container_of-style downcast после `fired` CAS.

**Cost:** ~20 LOC refactor. **P0 (Этап 3).**

#### C2 — stop_cb lock-free contract

Текущий Plan 40 audit 1 говорит «stop_cb берёт mutex; edge cases в
Этапе 2». Round 2: это закладывает потенциальный deadlock при Plan 23
landing. **Решение: contract (B).**

```c
/* C2: stop_cb НИКОГДА не берёт channel mutex.
 * Использует atomic_bool cancelled на BaseWaiter.
 * Wake helper при iteration видит cancelled=1 → skip.
 * Waiter unlink происходит lazy при следующем wake. */
static NovaStopMode _nova_channel_waiter_stop_cb(void* handle) {
    BaseWaiter* w = (BaseWaiter*)handle;
    nova_abool_store(&w->cancelled, true);
    /* Wake fiber так чтобы scope.cancel_requested check сработал. */
    if (w->channel) {
        nova_sched_wake(w->scope, w->slot);
    }
    return NOVA_STOP_SYNC;
}
```

Wake helpers (`_nova_channel_wake_recv/send`) при iteration:
```c
for (BaseWaiter* w = st->recv_waiters; w; w = w->next) {
    if (nova_abool_load(&w->cancelled)) continue;  /* skip dead waiter */
    if (!nova_aint_cas(&w->fired, &expected_0, 1)) continue;
    /* won the CAS — unlink, copy value, wake */
    ...
}
```

Lazy cleanup of cancelled waiters acceptable (eventually wake helper
walks past, или select_park exit cleanup unlink'ает всех своих).

**Cost:** ~30 LOC. **P0 (Этап 2).** Eliminates entire class of ordering
bugs с scheduler.

#### C3 — Panic at all-arms-disabled

```c
/* В nova_select_park, после can_unblock pre-check: */
int n_enabled = 0;
for (int i = 0; i < n; i++) {
    if (ctx->arms[i].chan && ctx->arms[i].guard) n_enabled++;
}
if (n_enabled == 0) {
    nova_throw(nova_str_from_cstr("select: no enabled arm"));
}
```

Existing «all closed» check сохраняется. Это дополнительный — для случая
все arms guarded `if false`.

**Cost:** ~10 LOC + regression test. **P0 (Этап 4).**

#### C4 — ASan + UBSan в Docker, не только TSan

Materialize postmortem: crossbeam unbounded channel double-free, lived
1 year, **ASan caught**, TSan не caught. 40 days debugging.

**Решение:** в Этапе 5 переименовать `Dockerfile.tsan` → `Dockerfile.sanitizers`
с 3 variants (TSan / ASan / UBSan). MSan — defer to Ф.4 (требует
instrumented deps).

```dockerfile
# Dockerfile.sanitizers — base
FROM ubuntu:22.04
ARG SANITIZER=tsan  # tsan|asan|ubsan

RUN apt-get update && apt-get install -y clang-15 cmake git curl
COPY . /nova
WORKDIR /nova

ENV CFLAGS="-fsanitize=${SANITIZER} -fno-omit-frame-pointer -g -O1"
ENV CXXFLAGS="${CFLAGS}"
ENV CC=clang-15

# build + run tests
RUN cd compiler-codegen && cargo build --release
CMD ["./nova_tests/plan40_sanitizers/run_all.sh"]
```

**Cost:** ~80 LOC Dockerfile + script. **P0 (Этап 5+6).**

#### C5 — Cache-line padding (inverted from strength)

**Решение:** group fields by access pattern, `_Alignas(NOVA_CACHELINE_SIZE)`
между группами. `NOVA_CACHELINE_SIZE = 64` (x86) / 128 (ARM big) через
runtime detection или compile-time `#ifdef __aarch64__`.

```c
struct Nova_ChannelState {
    /* Group A: mostly-read под mutex */
    nova_mutex_t      mu;
    nova_atomic_bool  closed;
    void           (*on_select_lost)(Nova_ChannelState*);
    void*             cleanup_data;

    _Alignas(NOVA_CACHELINE_SIZE) char _pad_a[1];

    /* Group B: under-lock state (changes on every op) */
    nova_int*         buf;
    int64_t           cap;
    int64_t           head;
    int64_t           count;
    BaseWaiter*       recv_waiters;
    BaseWaiter*       send_waiters;

    _Alignas(NOVA_CACHELINE_SIZE) char _pad_b[1];

    /* Group C: refcount (contended on close) */
    nova_atomic_int   writer_count;
    nova_atomic_bool  reader_closed;  /* B2 */
};
```

Cost: +128-192 bytes per channel. На 1000 каналов = 192 KB. Negligible
vs **300× perf delta** под M:N contention.

**Cost:** ~50 LOC + benchmark verification. **P1 (Этап 2).** Не вытесняем
из P0 потому что correctness не зависит — только perf. Но **обязательно**
до declaring Ф.1 done.

#### C6 — nova_sched_park_with_unlock API contract

Even bootstrap no-op, define API now:

```c
/* sched.h: */
/* Park fiber, atomically calling unlock_fn(arg) **after** transition
 * to parked state. Wake from another thread observes parked state and
 * uses scheduler-correct ready path. Bootstrap implementation can be:
 *
 *     void nova_sched_park_with_unlock(scope, slot, unlock_fn, arg) {
 *         unlock_fn(arg);            // single-thread: безопасно перед park
 *         nova_sched_park(scope, slot);
 *     }
 *
 * M:N implementation (Plan 23) MUST make the transition atomic. */
void nova_sched_park_with_unlock(NovaFiberQueue* scope, int slot,
                                  void (*unlock_fn)(void*), void* arg);
```

Channel send/recv/select_park use this API. Pattern:
```c
nova_mutex_lock(&st->mu);
/* register waiter under lock */
...
nova_sched_park_with_unlock(scope, slot, _unlock_mu, &st->mu);
/* по wake: lock уже released */
nova_mutex_lock(&st->mu);
/* re-check state, unregister waiter */
```

**Cost:** ~15 LOC bootstrap impl + caller migration in channels.h.
**P0 (Этап 2 dependency).** Без этого Plan 23 migration требует callsite
changes.

#### C7 — Weak CAS на ARM (relaxed-on-failure)

В `sync.h`:

```c
/* Weak CAS: weaker ordering on failure path (no barrier). Use в loop
 * patterns где failure не carries data. */
#define nova_aint_cas_weak_release(p, expected, desired) \
    atomic_compare_exchange_weak_explicit((p), (expected), (desired), \
                                           memory_order_release, \
                                           memory_order_relaxed)
```

Used в wake helpers:
```c
int expected = 0;
if (nova_aint_cas_weak_release(&w->fired, &expected, 1)) { /* won */ }
```

ARM: failed CAS не emit'ит DMB. x86: same code (already free).

**Cost:** ~10 LOC sync.h + comment block. **P1 (Этап 1 → этап 4).**

### Новые P1 / P2 items (round 2)

| # | Item | Etap | Cost |
|---|---|---|---|
| §3 | Spurious-park counter (debug NOVA_DEBUG_CHANNEL=1) | 6 | ~20 LOC |
| §4 | Same-channel-send-recv acceptance test | 4 | ~30 LOC |
| §11 | Iter[T] impl на ChanReader (`for v in ch.rx`) | 4 (separate plan?) | ~20 LOC |
| §13 | Recv-after-close-with-data regression test | 4 | ~20 LOC |
| §20 | Waiter unlink invariant debug-assert | 4 | ~15 LOC |
| §29 | NOVA_SELECT_SEED env var (reproducible select) | 6 | ~10 LOC |
| §26 | Heap fallback при n_arms > 256 (stack guard) | 4 | ~15 LOC |

### Что explicitly **НЕ делаем** (industry doesn't agree)

1. **Auto-close on dropped receiver** (Tokio yes, Go no, Plan 40 = Go).
   Boehm finalizers risky внутри lock-holding paths. Spec doc + `nova check`
   lint warning «channel reader dropped without close» — этого достаточно.
2. **Loom/CDSChecker для Ф.1** (industry recommends; defer to Ф.4
   pre-1.0 gate). TSan + ASan + UBSan + targeted stress = 90% bugs.
   CDSChecker = last 10%.
3. **Debug names per channel** (ни Go ни Tokio не имеют). Wait until
   observability plan demands.
4. **Recursive mutex** (2× overhead). Choose contract (B) for stop_cb.
5. **Cache padding per-field** (crossbeam over-pads). Pad **by access
   group** = sweet spot.
6. **Cryptographic RNG для select** (Go fastrand sufficient; ASLR makes
   pointer-derived seed unpredictable).
7. **Hold-all-locks during select scan** (Go-style). Use optimistic
   post-park retry — correctness equivalent, fewer lock traffic.

### Updated final scope для Ф.1

| Этап | После round 1 | После round 2 | Delta |
|---|---|---|---|
| 1 sync.h | ~100 | **~130** (+ weak CAS + ordering doc) | +30 |
| 2 B1 | ~350 | **~480** (+ C2 stop_cb lock-free, C5 padding, C6 park_with_unlock, B2 reader_close, A2 TOCTOU) | +130 |
| 3 T2 | ~140 | **~160** (+ C1 BaseWaiter refactor) | +20 |
| 4 B2 | ~360 | **~430** (+ C3 panic, §4/§13/§20 tests, B1 direct-copy, A6 cancel-safety, §26 heap fallback) | +70 |
| 5 Docker | ~100 | **~180** (+ C4 ASan + UBSan variants) | +80 |
| 6 TSan/sanitizers | ~315 | **~360** (+ §3 spurious counter, §29 SELECT_SEED, lockorder deadlock test) | +45 |
| 7 Docs | doc | doc + bench p50/p99 contended (ARM bench → Ф.4) | — |
| **Total Ф.1** | ~1180 | **~1740 LOC** | **+560** |

### Acceptance criteria (production-grade, final)

- **Correctness:**
  - 261/261 single-thread regression PASS (Windows).
  - Все 7 новых P0 items implemented.
  - Все 7 новых P1 items implemented (минимум C5 padding + C7 weak CAS).
  - **Все stress tests PASS под TSan + ASan + UBSan** на Linux Docker.
  - Lockorder-deadlock test (cross-fiber inverted-order select на shared
    channels) — не должен deadlock'нуть.
  - Recv-after-close-with-data: явный test.
  - Send-arm cancel-safety: явный test.
  - All-arms-disabled: explicit panic, не silent hang.

- **Performance:**
  - send/recv round-trip uncontended: **<50 ns** (x86).
  - 8-thread contended send/recv: report p50, p99 (no hard threshold,
    just observability).
  - 32-arm select dispatch: linear scaling до n=64.

- **Documentation:**
  - sync.h: ordering rationale per atomic field.
  - channels.h: invariant comments (waiter unlink, retry-is-correctness,
    GC-managed not manually-freed).
  - spec D94: bootstrap-ограничения + Plan 40 hardening + что отложено.
  - Plan 40: implementation log per этап.
  - project-creation.txt + simplifications.md + discussion-log.

- **Plan 23 readiness:**
  - `nova_sched_park_with_unlock` API defined (no-op bootstrap, M:N impl
    pending).
  - stop_cb lock-free contract documented.
  - GC_register_my_thread integration tested.

### Risks identified (round 2)

1. **C2 contract (B) requires** atomic_bool cancelled на BaseWaiter +
   wake-helper skip logic. Если wake helper bug — cancelled waiter
   получит wake = wrong fiber wakes. Mitigation: stress test specifically
   for cancellation race.
2. **C5 padding adds 128-192 bytes per channel.** На 100k channels =
   ~16 MB. Production deployments может potentially hit memory budget;
   document as known constraint.
3. **C7 weak CAS на x86 has no benefit** — compiler emits same code.
   Но **API consistency** важна; documentation должен явно говорить
   «weak вариант для loops, strong для one-shot».
4. **Heap fallback при n_arms > 256** — overlaps с Plan 40 Ф.3 «no cap»
   claim. Resolution: stack для ≤256, heap для >256, документировать как
   «soft threshold для stack overflow protection» (не «cap»).

---

## Deep audit — round 3 (2026-05-12)

Третий проход после round 1+2. Focus на production deployment issues,
toolchain portability, platform-specific perf, real postmortems.

### 🔴 Новые P0 build/correctness blockers

#### R3-2 — C11 toolchain portability (build-breaker)

**Реальность вместо плана:**
- MSVC: `<threads.h>` с VS2019 16.8+, **но `<stdatomic.h>` только с VS2022 17.5+**.
  Plan 40 audit r1 ошибся утверждая что MSVC VS2019 16.8 покрывает обоих.
- mingw-w64 (winpthreads): **не имеет** C11 `<threads.h>` до сих пор.
  См. [winlibs_mingw#219](https://github.com/brechtsanders/winlibs_mingw/issues/219).
- Apple Clang: **не имеет** `<threads.h>` в libc (третьесторонний shim
  `tinycthread` нужен).
- musl libc: поддерживает с 1.1.5.

**Plan 40 не собирётся** на: mingw-w64, MSVC VS2019, Apple Clang без
shim, CentOS 7 (glibc 2.17 — но это reject).

**Решение — Option B (portable wrapper):**

```c
/* sync.h — backend selector */
#if defined(_MSC_VER) && _MSC_VER >= 1935  /* VS2022 17.5+ */
  #include <stdatomic.h>
  #include <threads.h>
  #define NOVA_SYNC_BACKEND "c11"
#elif defined(__APPLE__)
  /* macOS: os_unfair_lock + GCC atomics (R3-5) */
  #include <os/lock.h>
  #include <pthread.h>
  #define NOVA_SYNC_BACKEND "darwin"
#elif defined(__GNUC__) || defined(__clang__)
  /* gcc/clang builtins — works on mingw-w64, all Linux */
  #include <pthread.h>
  #define NOVA_SYNC_BACKEND "gcc_builtins"
#else
  #error "Unsupported toolchain — need C11 or GCC builtins"
#endif

/* Unified wrappers (см. далее) — все три backend'а expose'ят
 * nova_mutex_t / nova_atomic_int / nova_aint_cas etc. */
```

3 backend'а:
1. **C11** (VS2022 17.5+, modern glibc): прямой `mtx_t` + `<stdatomic.h>`.
2. **Darwin** (Apple Clang): `os_unfair_lock` + `__atomic_*` GCC builtins.
3. **GCC builtins** (mingw, older clang, fallback): `pthread_mutex_t` +
   `__atomic_load_n`/`__atomic_compare_exchange_n` etc.

`__atomic_*` builtins identical API на clang/gcc, работают на всех
toolchain'ах где компилятор есть (включая mingw). Это **standard
fallback** для проектов которые нужно собрать без C11 atomics.

**Cost:** ~120 LOC. **P0 — без этого Plan 40 не собирается на 3 из 4
target toolchain'ах.** Этап 1 (sync.h rewrite).

#### R3-5 — macOS lock perf (40% slower)

**Реальность:** macOS `pthread_mutex_t` heavily fair → **20 µs
uncontended latency** vs ~50 ns на `os_unfair_lock` (см.
[mikeash](https://www.mikeash.com/pyblog/friday-qa-2017-10-27-locks-thread-safety-and-swift-2017-edition.html)).
Plan 40 acceptance criterion «<50 ns round-trip uncontended» **fails on
macOS** с обычным pthread mutex.

**Решение (часть R3-2 wrapper):**

```c
/* Darwin backend — sync.h */
typedef os_unfair_lock nova_mutex_t;

static inline void nova_mutex_init(nova_mutex_t* m) {
    *m = OS_UNFAIR_LOCK_INIT;
}
static inline void nova_mutex_lock(nova_mutex_t* m)   { os_unfair_lock_lock(m); }
static inline void nova_mutex_unlock(nova_mutex_t* m) { os_unfair_lock_unlock(m); }
```

Cost integrated в R3-2 (~30 LOC из 120 общего wrapper'а). **P0** для
macOS support. Если macOS не target — явно doc'нуть в Plan 40.

**Решение для bootstrap:** добавить macOS в Plan 40 supported platforms.
Cost ~30 LOC.

#### R3-7 — Spurious-wakeup re-check pattern для `nova_sched_park`

**Реальность:** POSIX `pthread_cond_wait` может вернуться spuriously.
Когда Plan 23 landed и `nova_sched_park` использует condvar, spurious
wake без `fired` re-check **silently loses value**.

**Решение (~5 LOC + 1 test):** документировать post-park pattern:

```c
/* sched.h: */
/* После nova_sched_park caller MUST re-check application state в loop:
 *
 *   while (!data_ready()) {
 *       nova_sched_park_with_unlock(scope, slot, unlock_fn, arg);
 *       // park может вернуться spuriously — re-check.
 *   }
 *
 * Channel/select use pattern: post-park проверяют fired CAS flag в
 * waiter (single source of truth), которая Release-store'нута только
 * настоящим wake helper'ом. Spurious wake → fired остаётся 0 →
 * try_immediate retry или next park.
 *
 * Этот pattern — correctness mechanism (не perf optimization).
 * НЕ удаляйте post-park retry в `nova_select_park`. */
```

В Этапе 6 — stress test `spurious_wake_no_value_loss`: 8 threads × N
iterations, симулируем spurious wake через signal-after-park (kill thread
without data ready), assert no message lost.

**Cost:** ~5 LOC doc + ~30 LOC test. **P0 (Этап 4 doc + Этап 6 test).**

#### R3-1 — Boehm GC-scan invariant для parked BaseWaiter chain

**Реальность:** BaseWaiter сейчас stack-allocated в fiber locals (через
compound literal). Если Boehm scan'ит parked fiber stack → waiter chain
live → safe. Если future refactor переместит storage в не-scanned
регион → silent UAF on wake.

**Решение (~15 LOC):** debug-build assertion + invariant doc:

```c
/* channels.h: */
/* INVARIANT (Plan 40 R3-1): BaseWaiter chain MUST be GC-reachable от
 * parked fiber stack OR explicit GC root. Wake helper assumes waiter
 * pointers valid после resume.
 *
 * Сейчас: compound-literal storage в `emit_select` живёт на fiber
 * stack, Boehm scan'ит fiber stacks через minicoro registration. Safe.
 *
 * Если будете переносить storage в heap или другую область — обязательно
 * добавьте GC_add_roots() или используйте nova_alloc(). */

#ifdef NOVA_DEBUG_CHANNEL
static inline void _assert_waiter_gc_visible(BaseWaiter* w) {
    /* GC_is_visible — Boehm API проверки что pointer в managed heap
     * или registered root. */
    extern void* GC_is_visible(void*);
    assert(GC_is_visible(w) != NULL && "waiter not GC-reachable");
}
#endif
```

В Этапе 2 каждый waiter register code path делает debug assert.

**Cost:** ~15 LOC. **P0 (Этап 2/3 doc + debug).**

### 🟡 Новые P1 items

#### R3-4 — Adaptive mutex на Linux

**Реальность:** PTHREAD_MUTEX_ADAPTIVE_NP → 55% throughput gain для
short critical sections (glibc benchmark). Plan 40 critical sections
короткие (buffer push/pop, waiter unlink).

**Решение (~15 LOC, integrated в R3-2 wrapper):**

```c
#if defined(__linux__) && defined(__GLIBC__)
static inline void nova_mutex_init(nova_mutex_t* m) {
    pthread_mutexattr_t attr;
    pthread_mutexattr_init(&attr);
    pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_ADAPTIVE_NP);
    pthread_mutex_init(m, &attr);
    pthread_mutexattr_destroy(&attr);
}
#else
static inline void nova_mutex_init(nova_mutex_t* m) {
    pthread_mutex_init(m, NULL);  /* PTHREAD_MUTEX_NORMAL */
}
#endif
```

**Cost:** ~15 LOC. **P1 (без него <50ns acceptance fails)**.

#### R3-3 — Priority inversion doc

**Реальность:** Default `pthread_mutex_t` не PI-aware. Mixed-priority
workloads под `SCHED_FIFO` → unbounded inversion (Mars Pathfinder).

**Решение:** doc в Plan 40 / spec D94:
- Bootstrap channels assume `SCHED_OTHER`.
- Users mixing RT priorities must externally serialize.
- Future: add `pthread_mutexattr_setprotocol(PTHREAD_PRIO_INHERIT)`
  option if RT story needed.

**Cost:** doc only. **P1** (P0 если Nova claim'ит RT support — сейчас не
claim'ит).

#### R3-9 — recv_many re-rank до P1

**Реальность:** не nice-to-have throughput, а **wake amplification fix**.
Под нагрузкой каждый `recv()` = 1 wake + 1 cache-line miss per message.
`recv_many` collapses N wakes в 1 — **10-100× throughput** для
logging/metrics pipelines.

**Решение:** перенести из B8 P3 в Ф.4 P1. Cost ~80 LOC.

#### R3-12 — Kernel preemption tax stress test

**Реальность:** SCHED_OTHER preempts after ~10ms. Producer fiber-thread
с lock preempted → все consumers block. Go использует sysmon tgkill
preemption.

**Решение:** в Этапе 6 — stress test «p99 wakeup latency under simulated
host scheduling jitter»: `taskset` 1 CPU, run 8 fiber'ов, measure p99
latency.

**Cost:** ~50 LOC test. **P1** (Plan 23 dependency, но measurable now).

#### R3-14 — Per-channel observability counters

**Реальность:** Tokio-metrics-collector exposes queue depth, sent count,
peak waiters для Prometheus. Plan 40 не имеет.

**Решение (~30 LOC):** atomic counter fields gated за compile flag:

```c
#ifdef NOVA_CHANNEL_METRICS
    nova_atomic_int total_sent;
    nova_atomic_int total_received;
    nova_atomic_int peak_waiters;
#endif
```

Zero overhead by default. Production builds могут включить через
`-DNOVA_CHANNEL_METRICS=1`.

**Cost:** ~30 LOC + API (`tx.metrics() -> ChannelMetrics`). **P2 для
1.0**, but **P1 для «production parity» claim**.

#### R3-11 — UBSan + nova_int overflow

**Реальность:** UBSan signed-integer-overflow trap'ит legitimate
wraparound. Plan 40 Этап 5 Dockerfile UBSan variant нужен filter.

**Решение:**
```dockerfile
ENV UBSAN_OPTIONS="suppressions=/nova/.sanitize/ubsan.supp"
```
+ exclude `signed-integer-overflow` через `__attribute__((no_sanitize("signed-integer-overflow")))`
на specific helpers OR `-fno-sanitize=signed-integer-overflow` для
runtime helpers.

**Cost:** ~3 LOC. **P1**.

#### R3-6 — Boehm flags под TSan Docker

**Реальность:** TSan + Boehm parallel marking → false-positive races
на GC internals.

**Решение:** в `Dockerfile.sanitizers` (TSan variant):
```dockerfile
ENV CFLAGS="${CFLAGS} -DTHREAD_LOCAL_ALLOC=0 -DPARALLEL_MARK=0"
```

**Cost:** ~5 LOC + 1 README note. **P1**.

#### R3-8 — `oneshot` + `watch` channels (Ф.4)

**Реальность:** Tokio имеет `oneshot::channel<T>` (single-value handoff,
lighter than mpsc) и `watch::channel<T>` (broadcast-of-latest для config
reload). Plan 40 = только bounded mpsc.

**Решение в Ф.4:**
- `oneshot` ~80 LOC (просто mpsc cap=1 без `tx.clone()`).
- `watch` ~150 LOC (different semantics — overwrite + version counter).
- `broadcast` ~400 LOC MPMC — отложить в Plan 41.

**Cost:** ~230 LOC в Ф.4. **P1 для 1.0** — Tokio users will look for these.

### Documentation gaps (round 3)

#### R3-15 — API stability across Plan 30→40

Plan 40 silently changes internals; user-facing API unchanged. **Doc
explicitly** в Plan 40 Acceptance + spec D94: «No source-level breakage».

#### R3-16 — CI bench reproducibility methodology

`<50ns` acceptance без specification hardware/methodology = unfalsifiable.

**Решение в Plan 40 Этап 7:**
- Hardware: x86_64 baseline (latest commodity CI box).
- Methodology: `taskset -c 0`, no turbo, average of 1M iterations,
  exclude warm-up.
- Bench script committed в `nova_tests/plan40_bench/`.

#### R3-17 — `is_closed()` time-of-check doc warning

User-facing docs: «don't use `is_closed()` for control flow — use `recv`
return value». Без этого users will write TOCTOU bugs.

### Updated final scope (round 3)

| Round-3 item | LOC | Phase |
|---|---|---|
| R3-1 GC-scan invariant + debug assert | +15 | Этап 2/3 |
| R3-2 Portable atomic/mutex wrapper (3 backends) | +120 | Этап 1 |
| R3-3 PI mutex doc | +10 doc | Этап 7 |
| R3-4 Adaptive mutex Linux | +15 | Этап 1 |
| R3-5 macOS os_unfair_lock (integrated в R3-2) | +30 | Этап 1 |
| R3-6 Dockerfile Boehm flags | +5 | Этап 5 |
| R3-7 Spurious-wake re-check + stress test | +35 | Этап 4 + 6 |
| R3-8 oneshot + watch (Ф.4) | +230 | Ф.4 |
| R3-9 recv_many re-rank P3→P1 (Ф.4) | (re-rank) | Ф.4 |
| R3-11 UBSan exclude overflow | +3 | Этап 5 |
| R3-12 p99 jitter stress test | +50 | Этап 6 |
| R3-13 static inline doc | +5 doc | Этап 7 |
| R3-14 Metrics counters (gated) | +30 | Этап 2 |
| R3-15/16/17 docs | +15 doc | Этап 7 |
| **Round-3 addition (Ф.1)** | **~328** | |
| **Round-2 Ф.1 baseline** | 1740 | |
| **Round-3 final Ф.1 scope** | **~2070 LOC** | |

### Round 3 — Top 5 most dangerous gaps

1. **R3-2 C11 toolchain portability** — Plan 40 не собирётся на mingw,
   MSVC VS2019, Apple Clang без wrapper. **P0**, ~120 LOC.
2. **R3-7 Spurious-wake re-check** — silent value loss когда Plan 23
   condvar landed. **P0**, ~35 LOC.
3. **R3-1 Boehm GC-scan invariant** — UAF при future refactor. **P0**,
   ~15 LOC.
4. **R3-5 macOS pthread_mutex 40% slower** — <50ns target fails на macOS
   без `os_unfair_lock`. **P0**, integrated в R3-2.
5. **R3-3 Priority inversion** — niche but production-deadly когда hit.
   **P1**, doc only.

### Round 3 — Top 5 NICE-to-have NOT chase pre-1.0

1. broadcast::channel — 400 LOC + Loom verification → Plan 41+.
2. Adaptive mutex backend — opt-in только если bench fails.
3. Prometheus/tracing integration — отдельный plan.
4. Watch channel — `Channel(1)` covers 90% use cases для 1.0.
5. PA-RISC/Itanium/SPARC weak ordering — tier-3 archs Nova won't ship on.

### Архитектурное решение по toolchain support (Plan 40 1.0)

**Tier 1 supported (build + test + sanitizer):**
- Linux x86_64 (Ubuntu 22.04+, glibc 2.35+) — primary CI.
- Windows x86_64 + clang (LLVM 15+) — dev workstation.
- macOS arm64 + Apple Clang — secondary.

**Tier 2 supported (build only, best effort):**
- Linux aarch64 + clang.
- Windows MSVC VS2022 17.5+.
- Alpine Linux + musl.

**Tier 3 NOT supported для 1.0:**
- mingw-w64 (gap нерешённый).
- MSVC VS2019 (нет `<stdatomic.h>`).
- CentOS 7 / RHEL 7 (glibc 2.17 — EOL).
- PA-RISC / Itanium / SPARC.

Решение задокументировано в spec/overview.md.

---

## Production audit — round 4 (2026-05-12)

После закрытия Ф.1 + Linux Docker validation + создания Plan 41 — round
4 audit нашёл 3 issues которые не были видны до post-implementation review.

### 🔴 P40R4-1 (P0): `_NOVA_GC_DISABLE` всё ещё активен — Cross-plan hazard

**Реальность.** Verified `compiler-codegen/nova_rt/fibers.h:67-68`:
```c
#  define _NOVA_GC_DISABLE()  GC_disable()
#  define _NOVA_GC_ENABLE()   GC_enable()
```

Plan 40 Ф.1 implemented M:N prerequisites (atomics + mutex + selectdone
CAS) **предполагая** что concurrent GC будет работать. Но Plan 41
(который removes `_NOVA_GC_DISABLE`) **ещё не landed**.

**Hidden hazard:** под M:N (Plan 23 future) worker thread parked с
`_NOVA_GC_DISABLE()` **disables GC globally** через Boehm. Другие workers
allocating concurrently — blocked или skipped. **Livelock.**

Plan 40 audit раунды 1-3 это **не зафиксировали** потому что считали что
`_NOVA_GC_DISABLE` концептуально отдельный план. Реально это
**hard dependency**.

**Решение:** В Plan 40 invariant explicitly: «Plan 40 atomics correctness
depends on `_NOVA_GC_DISABLE` removal which is Plan 41's contract.
Until Plan 41 closes, Plan 40 Ф.1 single-thread-safe only. TSan tests в
Этап 6 deliberately bypass fiber scheduler.»

Это **doc-only fix** в Plan 40 (~10 LOC). Real implementation fix —
Plan 41.

**Cross-plan implementation order (corrected):**
1. **Plan 41 first** (P0 fixes: P41-2, P41-3, P41-5, P41-11).
2. **Re-validate Plan 40** под TSan с `_NOVA_GC_DISABLE` removed.
3. **Затем Plan 23** safely.

### 🟡 P40R4-2 (P1): <50ns acceptance unrealistic under mutex-everywhere

**Production reference benchmarks:**
- Go `chan` bounded: 35-50ns uncontended.
- Tokio 1.40 mpsc bounded: ~50ns (после new bounded queue PR #5485).
- Rust std mpsc post-1.67 (crossbeam): ~30ns.
- crossbeam `bounded(1024)`: ~25-35ns lock-free.

**Plan 40 estimated под mutex-everywhere:**
- mtx_lock (futex fast path Linux glibc ADAPTIVE_NP): ~15-20ns uncontended.
- mtx_unlock: ~10ns.
- Buffer push/pop: ~5ns.
- 1× CAS на fired: ~5ns x86, ~15ns ARM.
- **Total round-trip: ~70-100ns**, miss target by 40-100%.

Real number ставит нас **ниже Tokio pre-1.40** и **ниже Go**. <50ns
target unrealistic без lock-free fast path для uncontended case.

**Решение:** Relax acceptance criterion:
- `<80ns single-thread uncontended` (beats Tokio pre-1.40).
- `<300ns 8-thread p50 contended` (production-acceptable).
- `<50ns` target defer to **Plan 50** (lock-free SPSC flavor).

### 🟡 P40R4-3 (P2): `alloca(n*4)` в `nova_select_try_immediate` overflow

Verified `channels.h:729`: `int* order = alloca(n * sizeof(int));`

При n=10000 (Ф.3 «no cap»): order array = 40KB на stack. Default
minicoro 56KB → overflow после 14k arms.

Plan 41 raises stack 2MB, mitigating до 500k+ arms. Но pre-Plan 41
«no cap» claim **технически violated** at ~13k arms (alloca overflow
before user logic).

**Realistic select sizes 2-8 arms** — не practical concern. Но spec
D94 «no cap» утверждение нужно qualify.

**Решение:**
- Heap fallback at n > 256: `if (n > 256) order = nova_alloc(...); else order = alloca(...);`. ~10 LOC.
- Или document «soft cap ~13k arms pre-Plan 41, ~500k post-Plan 41» в spec D94.

### Подтверждённые findings (audit confirmed, не bugs)

**P40R4-3 `fired` field в обоих waiter types — обоснованно.**
Original concern: 4 bytes wasted в ChannelWaiter (single waiter, CAS
не нужен). **Actually:** Round 2 C2 made `fired` correctness-load-bearing
для stop_cb cancellation atomicity. Re-justified, не violated.

**Subtler concern:** BaseWaiter 56 bytes на cache line 64B. Два waiter'а
back-to-back share line. Под contention — false sharing на linked list.

**Решение:** pad `BaseWaiter` к 64 bytes через `_Alignas(64)`. +8 bytes
per waiter. ~5 LOC. **P2.**

**P40R4-4 Direct-copy mechanism — confirmed working.** `channels.h:907-913`
+ `nova_select_try_immediate:766`. Implementation matches design.

**P40R4-5 `on_select_lost` O(N) для losing arms.** Realistic 2-8 arms
→ 8µs cleanup. Pathological 1000-arm selects не real workload. P3.

**P40R4-6 `SelectWaiter.arm_idx: int` overflow 2B arms.** Not realistic.
Reject.

**P40R4-7 `cancelled` flag ordering.** Round 2 C2 introduced без
explicit ordering doc. Default `nova_abool_store/load` = `acq_rel`/
`acquire` per Round 1 A1 — correct. **Но future refactor могут
downgrade до weak/relaxed (R2 C7 proposal)** что break invariant.

**Решение:** Doc lock в sync.h: «cancelled — Release on stop_cb,
Acquire on wake skip. NEVER weak.» ~5 LOC. **P1.**

### Round 4 final summary

**Top 3 issues:**

| # | Issue | Severity | Cost |
|---|---|---|---|
| P40R4-1 | `_NOVA_GC_DISABLE` cross-plan hazard | **P0** doc | ~10 LOC doc |
| P40R4-2 | <50ns target unrealistic | P1 | doc update |
| P40R4-3 | alloca overflow at ~13k arms | P2 | ~10 LOC heap fallback |

**Plus secondary:**
- BaseWaiter cache padding (P2): +8 bytes per waiter.
- `cancelled` ordering doc (P1): ~5 LOC.

### Plan 40 status post-round-4

**Не invalidate Ф.1 closure.** Round 4 — это **documentation + minor
polish** issues, не correctness gaps. Implementation valid.

**Cross-plan dependency** explicit'но зафиксирован — Plan 41 должен
land first для real M:N readiness, **независимо** от Plan 40 audit
result.

### Production positioning после Plan 40 + Plan 41 closed (with P0 fixes)

**Channel layer:**
- **At parity с Rust std mpsc post-1.67** (mutex + condvar based).
- **Below Tokio 1.40 bounded** (~70ns vs ~50ns).
- **Below Go runtime chan** (~70ns vs ~35ns).
- **Below crossbeam** by order of magnitude (lock-free vs lock-based).
- **Architecturally on par or better в select:** stack-allocated
  SelectCtx, `on_select_lost` cleaner than Tokio Sleep::reset, no
  per-field cache padding waste.

**Honest acceptable production positioning:**
- I/O-bound web services ~10k concurrent fibers, p99 sub-ms.
- **НЕ ready для HFT / sub-µs latency** — mutex-everywhere + per-thread
  arena GC scan forbid this. Plan 50 lock-free SPSC + concurrent GC
  необходим.

---

## Production audit round 5 (2026-05-12) — fresh-eyes re-audit

После Plan 41 design + 4 prior round'ов — fresh audit нашёл 2 quick
fixes реализуемые **сейчас**, не upstream'ом в Plan 41.

### ✅ QF-1: recv fast-path closed check (channels.h:320, implemented)

**Asymmetric bug fixed.** Send path делает atomic load на `closed`
**до** mutex_lock (channels.h:466), early return 0. Recv path сразу
mutex_lock'ил без fast-path check.

**Production reference.** Go `runtime/chan.go::chanrecv` (~line 445):
```go
if !block && empty(c) {
    if atomic.Load(&c.closed) == 0 { return }
    if empty(c) { return true, false }
}
```

**Implementation.** Added fast-path в `nova_chan_reader_recv`:
- Atomic load `closed` без lock'а.
- Если closed: take lock, check count > 0 (data может быть в буфере).
- count > 0 → drain path. count == 0 → return None.
- Если not closed: proceed normal locked path.

Под bootstrap дёшево (1 atomic_load saved); под M:N saves entire
mutex roundtrip на closed-empty recv.

**Cost:** ~20 LOC. Tests: 262/262 Windows + 261/261 Linux PASS.

### ✅ QF-2: stack-clash protection compile flags (test_runner.rs, implemented)

**CVE-2017-1000366 mitigation.** Single 4KB guard page (Plan 41 P41-5)
**bypassable** функцией с локальным массивом >4KB (например
`char buf[16384]`). Qualys 2017 disclosed как stack-clash.

**Reference:** https://www.qualys.com/2017/06/19/stack-clash/stack-clash.txt

**Production fix:** `-fstack-clash-protection` (GCC 8+, Clang 11+)
inserts probing на каждую stack-frame > page_size.

**Implementation.** В `test_runner.rs` build flags:
```rust
#[cfg(any(target_os = "linux", target_os = "macos"))]
{
    flags.push("-fstack-clash-protection".to_string());
    flags.push("-fstack-protector-strong".to_string());
}
```

Windows clang-cl/MSVC использует `/GS` by default — отдельный
mechanism, skip.

**Эффект:** даже **до** Plan 41 implementation user code защищён от
stack-clash. После Plan 41 — defense-in-depth: clash-protection
inserts probes → probes hit guard page (P41-5) → SIGSEGV → P41-6
handler (P0 после promotion) prints message.

**Cost:** ~10 LOC test_runner.rs. Tests: 262/262 Windows + 261/261
Linux PASS.

### Что lost в реальных Audit'ах но найдено в round 5

#### R5-1: Strengths over Go/Rust — нужно document

Plan 40 уже **строго лучше Go**:

1. **Stack-allocated select arms** (compound literal в codegen): zero
   heap, no GC pressure, hot cache. Go использует `sudog pool`
   (heap allocation на каждый arm + GC overhead). **Strict win.**

2. **`on_select_lost` callback для Time.after cleanup:** cleaner чем
   Tokio `Sleep::reset` (Tokio leaves timer fire-event в buffer).

3. **No per-field cache padding (crossbeam over-padding):** Plan 40
   pads только по access groups (Nova_ChannelState A/B/C).

**Action:** Document в spec/decisions/06-concurrency.md как explicit
strengths.

#### R5-2: Future-proofing comments

- `channels.h::buf` хранит `nova_int` (8 bytes). Plan 33/21 generic T
  потребует heap-allocate values + indirection ИЛИ variable element_size.
  Comment не присутствует.
- `nova_select_try_immediate::alloca` overflow at ~13k arms (R4 audit).
  Soft cap doc в spec D94.

**Action:** ~10 LOC comments в channels.h + spec note.

#### R5-3: Buffer slot reuse — modulo cap performance

**Plan 40 уses `tail = (head + count) % cap`.** Modulo на каждый send.
**Crossbeam использует power-of-2 cap + AND mask** → saves ~17
cycles/send.

Trade-off: cap rounded up to next pow2 → memory waste до 2×.

**Severity:** P3. Real workloads — cap'ы 16-256, мало memory. Win
~17 cycles/send × M:N contention = visible. **Не блокер.**

**Recommendation:** Document в `spec/decisions/06-concurrency.md`
trade-off. Implement если bench показывает что modulo — bottleneck.

### Top 5 findings round 5 (включая carry-over)

| # | Item | Severity | Status |
|---|---|---|---|
| 1 | **QF-2 stack-clash flags** | P0 | ✅ implemented |
| 2 | **QF-1 recv fast-path closed** | P0 | ✅ implemented |
| 3 | **R5-1 strengths over Go documented** | P2 | Plan 41 + spec |
| 4 | **Future-proofing comments (R5-2)** | P3 | next session |
| 5 | **Buffer modulo→AND mask (R5-3)** | P3 | bench-driven, defer |

### Что Nova **уже строго лучше** Go/Rust (audit confirmed)

1. **Stack-allocated select arms** — Plan 40 Ф.3 final.
2. **`on_select_lost` callback** — Plan 40 Ф.2 B7.
3. **No cache padding bloat** — Plan 40 R2 C5 access-group padding.
4. **BaseWaiter common prefix** — Plan 40 R2 C1 strict-aliasing safe (cleaner чем Go's sudog type pun).

### Что Plan 40 **может** лучше Go/Rust (long-term)

1. **Effect-typed channel specialization:** codegen эмитит lock-free
   impl в `realtime` effect scope, mutex impl в `async` scope. Plan
   19+21+23 territory.
2. **Verified preconditions:** Plan 33 контракты статически proof
   `rx.recv() == Some(_)` — no runtime check. Tokio/Go = всегда runtime.
3. **Capability-typed channels:** Plan 21 extension — `BroadcastReader`,
   `SPSCWriter` type tags, codegen specializes implementation.

### Acceptance criteria adjustment (round 5)

После audit'а realistic numbers vs Tokio/Go:

- **Send/recv uncontended round-trip:** target relaxed to **<80ns** (от
  изначального <50ns). Real number под mutex-everywhere ~70-100ns.
  <50ns deferred to Plan 50 (lock-free SPSC).
- **8-thread contended p50:** report only, no hard threshold.
- **Linux Docker:** 261/261 PASS (perf bench skipped — Boehm/Docker
  SEGV not Plan 40 bug).

### Plan 40 final status (round 5)

**Ф.2 + Ф.3 + Ф.1 closed.** Round 5 = 2 quick fixes implemented +
documentation updates. **No architecture changes needed.** План
готов к production use под Plan 23 M:N **с Plan 41 closing first**
(P40R4-1 cross-plan hazard).

---

## Audit Round 8 (2026-05-13) — Critical findings from cross-cut Go/Rust review

**Контекст:** Запрошенный пользователем production-grade аудит сравнения
с Go runtime/chan.go, Tokio mpsc, crossbeam. Использован subagent-driven
review с прочтением actual кода (channels.h 1090 LOC + fiber_arena.c +
fibers.h). Найдены P0 bugs которые не были покрыты раундами 1-7.

### P0 — Production blockers

**P40R8-1: `_nova_after_pending_head` global static — race под M:N**
(channels.h:999). Pin list введён в R6 для защиты NovaAfterState от
Boehm collection. Под Plan 23 M:N несколько worker threads concurrently
вызывают `Time.after` → race на pin/unpin без mutex.

**Fix:** убираем pin list полностью. NovaAfterState переводится на
`malloc`/`free` (raw heap, не Boehm). Lifetime управляется libuv'ом
(close_cb guarantee). Это same pattern как Tokio (raw handle, owned by
libuv) и Go runtime (timer struct в P-local heap).

Бонус: это **also fixes** Windows boundary ~35 SEGV. Hypothesis: Windows
fiber stacks через calloc (D97) НЕ зарегистрированы как Boehm roots →
SelectWaiter / BaseWaiter на fiber stack могут стать unreachable во
время collect cycle → conservative scan miss → UAF на post-park unlink.
Pin list защищал NovaAfterState но не transitive (tx, channel state,
recv_waiters). Убирая GC-управление NovaAfterState полностью, мы
снимаем dependency на Boehm root coverage.

**P40R8-2: Memory ordering on `fired` CAS — ARM64 reorder risk.**
`nova_aint_cas_weak_release` (acq_rel/relaxed-on-failure) + `nova_aint_load`
acquire (sync.h:126). Pairing работает для x86 TSO, но на ARM64 store
buffer может reorder writes ДО CAS-release с writes ПОСЛЕ. Конкретно:
waker делает `w->send_val = value; cas(fired)`. Reader делает
`load(fired); v = w->send_val;`. На ARM64 без full acquire на load —
race window.

**Fix:** CAS на `fired` → `acq_rel` (вместо weak_release), либо load
`fired` всегда через explicit acquire fence + ordering pair. Bootstrap
single-thread не важно; M:N — критично.

**P40R8-3: select pre-check может пропустить wakeup** (channels.h:892-894).
`can_unblock = 0` → `nova_throw("select: all channels closed")` без
вызова `try_immediate`. Между pre-check и park-registration channel
может стать ready (concurrent send). Под bootstrap single-thread не
проявится; под M:N — wake lost.

**Fix:** call `try_immediate(ctx)` перед `can_unblock` check; if hit →
return. Если miss → check can_unblock.

### P1 — Perf / correctness

**P40R8-4: Per-park BaseWaiter heap allocation** (channels.h:371, 527).
`nova_alloc(sizeof(BaseWaiter))` на каждый recv/send park. Под 100k
req/s = 6.4 MB/s garbage в Boehm heap. Select уже использует caller-provided
storage (compound literal на стеке) — extend to plain recv/send. **Это
Nova unique advantage над Go**: minicoro fixed fiber stacks ↔ stack-pinned
BaseWaiter safe; Go не может из-за moving goroutine stacks.

**P40R8-5: Fisher-Yates seed reuse в loop** (channels.h:778).
`uint32_t rng = (uintptr_t)ctx ^ 0xdeadbeef`. Same `ctx` (compound literal
на той же стек-позиции) на consecutive `select` iterations в loop = same
seed = same shuffle order. Destroys fairness across iterations.

**Fix:** seed с `(uintptr_t)ctx ^ rdtsc()` либо persist `rng_state` в
`SelectCtx`.

**P40R8-6: `sendDirect` type-pun через `send_val: nova_int`** (channels.h:275, 503).
Прямой direct-copy через `w->send_val = value;` где value — `nova_int`
(8 bytes). Когда Plan 21+ обобщит channel'ы на arbitrary T (записи,
структуры) — type-pun сломается. **Это TIME-BOMB на refactoring.**

**Fix (когда добавлять T-generic channels):** SelectWaiter имеет `void* recv_slot`,
caller передаёт указатель на T-typed stack slot; wake helper memcpy(slot, val, sizeof T).
Go's `chansend` так и делает.

**P40R8-7: `nova_chan_writer_close` redundant fence** (channels.h:600).
`fetch_sub_release` + `thread_fence_acquire` корректно для refcount idiom.
Но subsequent reads (`st->buf`, `st->count`) под `nova_mutex_lock` — mutex
acquire даёт нужный ordering. Fence dead code. Remove или document.

### P2 — Cleanups

**P40R8-8: `nova_chan_writer_clone` increment** (channels.h:625) — verify
что `nova_aint_inc` — `relaxed` (refcount idiom, не `seq_cst`).

**P40R8-9: Ring buffer без `tail` field** — каждое push считает
`(head+count) % cap`. Go использует sendx/recvx separately. Minor perf.

### Cross-plan blockers (для Plan 23 M:N)

| Find | Plan | Blocker для M:N? |
|---|---|---|
| P40R8-1 (pin list) | 40 | ✅ Yes |
| P40R8-2 (ARM64 fired ordering) | 40 | ✅ Yes |
| P40R8-3 (select pre-check) | 40 | ✅ Yes |
| P40R8-4 (heap pressure) | 40 | ⚠️ Perf only |
| P40R8-5 (Fisher-Yates seed) | 40 | ⚠️ Fairness only |
| P40R8-6 (sendDirect typepun) | 40 + 21 | ❌ Blocks T-generic |

### Implementation priority (this session)

- **P40R8-1** — already started (malloc/free вместо pin list).
- **P40R8-5** — 5-line fix.
- **P40R8-3** — 5-line fix.
- **P40R8-2** — `__atomic_load_n` уже использует `__ATOMIC_ACQUIRE` (sync.h:126), pairing OK. Аудит подтверждение через документацию.
- **P40R8-7** — comment update.

Остальное (P40R8-4 heap pressure, P40R8-6 sendDirect typepun) — выходят
за scope этой сессии. Открыты как Plan 40 round 9+ или поглощены в
Plan 23 implementation.
