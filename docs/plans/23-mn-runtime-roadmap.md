// SPDX-License-Identifier: MIT OR Apache-2.0
# План 23: M:N runtime — архитектурный roadmap

> **Статус:** roadmap, **не** план-исполнения. Целит на milestone v1.0+.
> **Создан:** 2026-05-11.
> **Зависит от:** Plan 22 (libuv + park/wake API), Plan 21 (Channel
> capability-split — owner-actor паттерн), Plan 18 (P0 stdlib для real
> backend use-case).
> **Не блокирует:** ничего из текущей работы.
>
> **Executable companions (started 2026-05-13):**
> - [Plan 23.1](23.1-mn-runtime-stage0.md) — Этап 0 закрыт: infrastructure
>   liveness (NovaWorker struct, runtime.init/shutdown, smoke test,
>   std.runtime.runtime API). 266 PASS / 0 FAIL Windows.
> - Plan 23.2+ — следующие этапы (work-stealing, TLS migration, blocking
>   pool, std.sync impl, spec semantics для shared mut). Открываются
>   когда Plan 23.1 acceptance met.

---

## Что это

Архитектурная карта перехода Nova с **N:1** (один OS-thread, миллион
fiber'ов на нём — текущий bootstrap + Plan 22) на **M:N** (N OS-thread'ов
по числу ядер, fiber'ы work-steal'ятся между ними — Go/Erlang/OCaml 5
модель).

**Не план-исполнения.** Слишком много неизвестных, чтобы фиксировать
фазы с acceptance. Roadmap описывает: какие компоненты нужны, какие
зависимости, какие open Q должны быть закрыты до старта реализации,
что уже готово после Plan 22.

---

## Зачем M:N

Текущая Nova ([D71](../../spec/decisions/06-concurrency.md#d71)) —
single-threaded N:1: scheduler в `nova_supervised_run` крутит fiber'ов
round-robin на одном OS-thread'е. Это значит:

- **Сервер на Nova использует одно ядро.** На 16-core машине — 1/16
  CPU. Для backend/CLI ниши (Plan 18) — блокер v1.0.
- **CPU-bound параллельность невозможна.** `parallel for` сейчас —
  cooperative scheduling одного потока, не реальный параллелизм.
- **Blocking C-call замораживает всё.** Один `Blocking`-effect call
  ([D50](../../spec/decisions/06-concurrency.md#d50)) без detach
  блокирует scheduler. В M:N один thread блокируется, остальные
  продолжают.

Альтернативы (`worker_threads` Node.js-style, `spawn_blocking` пулы) —
**локальные patches**, не системное решение. Nova-философия — единый
fiber-runtime, программист не различает «light» и «heavy» fiber'ы.
M:N делает это честно.

---

## Что готово после Plan 22

**Уже имеется** в bootstrap'е (после Plan 22):

| Компонент | Источник | Готовность |
|---|---|---|
| Корутины (fiber stacks) | minicoro | ✅ работает, thread-agnostic |
| Thread API | libuv `uv_thread_*`, `uv_mutex_t`, `uv_async_t`, `uv_cond_t` | ✅ vendored |
| Event loop | libuv `uv_loop_t` | ✅ глобальный (Plan 22 Ф.2) |
| Park/wake API | `nova_rt/sched.h` (Plan 22 Ф.3) | ✅ нормативный primitive |
| Cancel-wake mechanism | generic stop_cb (Plan 22 R4) | ✅ работает для любых блокирующих операций |
| Channel (owner-actor) | Plan 21 capability-split | 🟡 spec'ом, реализация после Plan 22 |

**Что не готово:**

| Компонент | Что нужно |
|---|---|
| Lock-free deque | для work-stealing |
| Atomic primitives wrapping | std.sync `Atomic[T]` |
| Concurrent GC | mark-and-sweep / drop-in libgc |
| Per-fiber state migration | handler-stack, fail-frame, interrupt-frame — переезд с TLS на per-fiber |
| Per-thread event loops | `uv_loop_t` per worker thread (либо shared) |
| Cross-thread wake | `uv_async_t` для пробуждения idle worker'ов |
| Memory model для shared mut | spec semantic (UB? atomic-required? channel-only?) |

---

## Архитектурная карта

### Слой 1: Thread pool

`N` worker thread'ов, где `N` ≈ `nproc` либо переопределяется через
`NOVA_THREADS` env. Каждый worker:

```c
void worker_main(void* arg) {
    NovaWorker* w = (NovaWorker*)arg;
    uv_loop_t* loop = &w->loop;        /* per-worker event loop */
    uv_loop_init(loop);

    for (;;) {
        /* (1) Local ready-queue: round-robin как сейчас. */
        while (drain_local_queue(w)) { /* mco_resume */ }

        /* (2) Work-stealing: попробовать украсть у соседа. */
        if (try_steal_from_other_worker(w)) continue;

        /* (3) Все idle — спим в kernel'е до libuv-события. */
        uv_run(loop, UV_RUN_ONCE);
    }
}
```

**Открытые вопросы слоя 1:**
- Сколько loop'ов: один shared (как Tokio current_thread) или per-worker
  (как Tokio multi_thread)? Per-worker — естественнее для M:N (callback
  выполняется на том же thread'е, где fiber park'нулся), но сложнее
  миграция.
- Affinity: pin'им ли worker'а к CPU core? Erlang делает; Go нет (полагается
  на kernel scheduler).
- Sleep-stratей: spin-then-park (Go) vs eager-park (Tokio)? Spin
  даёт latency, park даёт energy.

### Слой 2: Lock-free work-stealing deque

Каждый worker имеет **deque** (двусторонняя очередь fiber'ов):
- Owner push'ит/pop'ит с **bottom** (LIFO для локального cache locality).
- Stealer'ы pop'ят с **top** (FIFO, минимизирует contention с owner'ом).

**Каноническая реализация** — Chase-Lev deque (2005, Chase & Lev),
~200 строк C с C11 atomics. Wait-free для owner'а, lock-free для
steal'ера. Используется в Go, JVM ForkJoinPool, Rust crossbeam,
OCaml 5.

**Альтернативы:**
- `liblfds` — готовая C-library с deque/queue/stack/hash. Production-
  ready, BSD-license. Минус: лишняя dependency.
- `concurrentqueue` (moodycamel) — C++ only.
- Свой mutex-based deque — простой, но contention под нагрузкой.

**Рекомендуемое:** Chase-Lev своя реализация. ~200 строк, академически
проверено, не тянем dependency, поддерживается C11 atomics.

### Слой 3: Per-fiber state (TLS migration)

Сейчас (single-thread) — `__declspec(thread)` для:
- `_nova_fail_top` (throw-protection chain)
- `_nova_interrupt_top` (interrupt-frame chain)
- `_nova_handler_Time`, `_nova_handler_Log` и др. ([D80](../../spec/decisions/06-concurrency.md#d80))
- `_nova_active_scope`, `_nova_active_slot`

При M:N **fiber может park'нуться на thread A и resume'нуться на
thread B**. TLS-globals при этом «остаются на старом thread'е» — это
**UB**. Решение: вся per-fiber-state переезжает в `NovaFiberState`
struct, хранится в `mco_get_user_data(co)`, switch'ится при `mco_resume`.

Это **большой refactor**:
- Каждый `__declspec(thread)` globals → поле `NovaFiberState`.
- `mco_resume(co)` обёртывается: save outer state, load fiber-state,
  resume, save fiber-state, restore outer.
- Codegen для `with X = h { ... }` пишет в `current_fiber_state->handler_X`,
  не в `_nova_handler_X` global.

Bootstrap уже **частично** имеет это в `NovaFiberQueue` (поля
`fiber_fail_top[]`, `fiber_interrupt_top[]`, `fiber_effect_snapshot[]`,
[`fibers.h:80-97`](../../compiler-codegen/nova_rt/fibers.h#L80-L97)) — но они
живут на scope-уровне, не на fiber'е напрямую, и switch происходит
только внутри `nova_supervised_step`. M:N requires более general
mechanism.

### Слой 4: Cross-worker communication

**Park-on-worker-A, wake-from-worker-B** — нужно разбудить thread A
из callback'а на thread B. libuv даёт `uv_async_t`:

```c
/* Один uv_async_t per worker — для cross-worker wake'ов. */
uv_async_t worker[i].wake_handle;

/* Callback на worker B хочет разбудить fiber на worker A: */
void cross_worker_wake(NovaWorker* target, NovaFiber* f) {
    /* push fiber в target's deque (lock-free) */
    deque_push_top(&target->deque, f);
    /* trigger uv_async — будит target из uv_run */
    uv_async_send(&target->wake_handle);
}
```

Это паттерн Go's `netpoll` и Tokio: cross-worker push через lock-free
структуру + событие в event loop.

### Слой 5: Atomic primitives для пользователя

После M:N **shared mut между fiber'ами — UB без синхронизации**.
Plan 18 P0 предлагает `std.sync`:

```nova
let counter = Atomic[int].new(0)
parallel for _ in 0..1000 {
    counter.fetch_add(1)        // safe
}
assert(counter.get() == 1000)
```

Реализация — wrapping C11 `<stdatomic.h>` через FFI / built-in. API
зафиксирован в Plan 18 как «M:N-correct сразу», bootstrap-impl упрощённая
(single-thread без CAS). Под M:N тот же API становится настоящим CAS.

**`Mutex[T]`, `RwLock[T]`, `Semaphore`, `WaitGroup`, `Once[T]`** —
аналогично, через libuv-обёртки + park/wake mechanism для блокирующих
acquire.

### Слой 6: Concurrent GC

**Главный архитектурный choke-point.** Текущий bootstrap GC ([D6](../../spec/decisions/05-memory.md#d6)
обещает «modern concurrent GC, паузы <1ms», но реализация — простой
mark-and-sweep single-thread). Под M:N нужно либо:

**Вариант A: BDW-GC (`libgc`)** — готовая conservative GC, multi-thread
mutators из коробки, mark-stack parallel. Используется в Mono, GCC's
libobjc, многих interpreted языках.

Плюсы:
- ✅ Готовая. Меньше работы.
- ✅ Multi-thread support из коробки.
- ✅ Conservative scanning — не требует precise type info, работает с
  любым С-кодом.
- ✅ Опционально incremental + generational режимы.
- ✅ License: MIT-style.

Минусы:
- ❌ Conservative scanning = false retention (значение похожее на pointer
  держит объект живым). На 32-bit — много false positives; на 64-bit
  редко, но бывает.
- ❌ Stop-the-world паузы есть (минимизируются incremental режимом,
  но не нулевые).
- ❌ Tuning под Nova — фиксированный набор knobs, нельзя глубоко
  адаптировать.
- ❌ Чужая кодовая база — debug сложнее.

**Вариант B: Свой Go-style concurrent mark-and-sweep**

Плюсы:
- ✅ Полный контроль. Tuning под Nova типы (precise scanning через
  type info из codegen).
- ✅ Минимальные паузы (Go ZGC-style — <1ms wall-clock).
- ✅ Write-barrier'ы adaptивны под Nova heap-layout.
- ✅ Может evolve'нуть в region-aware GC ([D6](../../spec/decisions/05-memory.md#d6)
  упоминает regions opt-in).

Минусы:
- ❌ Research-level effort. Go потратил несколько релизов чтобы
  привести свой GC в production-grade.
- ❌ Каждый write на heap-pointer — write-barrier (~1-2 инструкции
  overhead).
- ❌ Stack scanning требует precise GC roots — Nova codegen должен
  эмитить stack maps.

**Trade-off**:

| Критерий | BDW-GC | Свой |
|---|---|---|
| Время до v1.0 | месяцы | годы |
| Pause latency | 1-10ms | <1ms |
| Overhead на mutator | низкий | средний (write-barrier) |
| False retention | возможна | нет |
| Tuning под Nova | ограничен | полный |
| Risk | низкий (отлажен) | высокий (research) |

**Рекомендация:** **BDW-GC сначала, свой потом**. Аналогично Plan 22
выбору «libuv vendored, не свой netpoll». BDW даёт работающий M:N
runtime быстро; миграция на свой GC — отдельный milestone после
v1.0, когда workload и метрики покажут где BDW не хватает. Это
**эволюционный путь**, не «build-and-throw-away».

Финальное решение фиксируется отдельным D-блоком до старта реализации
M:N (см. open Q ниже).

### Слой 7: Spec semantics

**Что нужно зафиксировать в spec перед реализацией:**

- Memory model: что считается **UB** при concurrent доступе к shared
  mut? Atomic-required? Channel-only?
- Effect handler scoping ([D80](../../spec/decisions/06-concurrency.md#d80))
  при миграции fiber между worker'ами — semantics не меняется
  (per-fiber state мигрирует), но spec должен это явно сказать.
- `realtime nogc { }` ([D64](../../spec/decisions/04-effects.md#d64))
  — пин fiber'а к worker'у на время блока (запрет миграции, чтобы
  избежать GC interaction)?
- `Blocking`-effect ([D50](../../spec/decisions/06-concurrency.md#d50))
  — fiber на dedicated blocking-pool thread'е, не на worker'е. Plan
  M:N добавляет honest blocking-pool (сейчас bootstrap inline'ит).
- `Detach`-effect — какому worker'у принадлежит global supervisor's fiber?

Эти вопросы открываются как **Q-пункты в `open-questions.md`**, не
новые D-блоки. По мере проработки реализации — закрываются D-блоками.

---

## Список prerequisite-планов

| План | Что даёт M:N | Статус |
|---|---|---|
| **Plan 22** | libuv в build chain, park/wake API, event loop | активный, не начат |
| **Plan 21** | Channel capability-split (owner-actor — главный паттерн под M:N) | DRAFT |
| **Plan 18** | std.sync API (`Atomic[T]`, `Mutex[T]`, `Channel`) — фиксируется как «M:N-correct сразу» | DRAFT |
| **Plan 09** | Clang migration — нужен для C11 atomics на Linux/macOS | активный, не начат |
| **Plan 35** | cross-file resolve — std/* должен быть собран до std.sync impl (выделено из Plan 14 Ф.5) | план, не начат |

Plan 23 не блокирует ни один из них — он опирается на их **результаты**.

---

## Open Q (выявленные, добавляются в open-questions.md)

> Эти вопросы открываются в spec/open-questions.md отдельным коммитом
> при принятии этого roadmap'а. Здесь — preview формулировок.

**Q-mn-1: Memory model для shared mut при M:N.**

Что происходит когда fiber A на worker'е 1 и fiber B на worker'е 2
оба пишут в одно managed-heap поле без synchronization?

Варианты:
- (a) UB (Rust-style): запрещено компилятором через ownership analysis.
- (b) Atomic-required: shared mut требует `Atomic[T]` обёртки.
- (c) Channel-only: shared mut между fiber'ами в принципе запрещён;
  общение только через `Channel`. Owner-actor pattern obligatorily.

Влияет на: type-checker rules, std.sync API surface, generic-bounds.

**Q-mn-2: Fiber-migration boundary для `realtime nogc`.**

`realtime nogc { body }` ([D64](../../spec/decisions/04-effects.md#d64))
обещает «no GC pauses». При M:N — fiber может мигрировать между
worker'ами; миграция через GC safepoint = pause. Решения:
- (a) Pin к worker'у на время блока.
- (b) Запрет миграции через атрибут fiber'а (`no_migrate`).
- (c) Запрет M:N для fiber'ов, прошедших через realtime-блок.

**Q-mn-3: Concurrent GC choice.**

BDW-GC drop-in vs свой concurrent mark-and-sweep. См. «Слой 6»
выше. Решение фиксируется отдельным D-блоком (D-mn-gc) до старта
реализации.

**Q-mn-4: Worker count auto-tuning.**

По умолчанию `nproc`? Через `NOVA_THREADS` env? Через `nova.toml`?
Configurable runtime через `Runtime.set_workers(n)` API?

**Q-mn-5: `Blocking` effect — pool size.**

Plan M:N добавляет honest blocking-pool thread'ов. Размер пула —
fixed, auto-grow, либо bounded?

**Q-mn-6: Effect handler stack — concurrent access.**

`with X = h { spawn { ... } }` — после spawn fiber имеет snapshot
handler-stack'а ([D80](../../spec/decisions/06-concurrency.md#d80)).
Под M:N — handler-объект может быть **shared** между worker'ами.
Если handler stateful (captures `let mut` через closure) — UB. Spec
должен это явно сказать.

**Q-mn-7: SIGINT / signal handling в multi-thread runtime.**

Какой thread получает SIGINT? libuv даёт `uv_signal_t` — какому
worker'у его прибивать? Mainscope (D92) cancellation через SIGINT —
как routing?

---

## Прецеденты — что взять и не взять

### Что взять

- **Chase-Lev deque** (academic paper 2005) — реализовать самим, 200
  строк. Используется везде.
- **Go scheduler design** (статьи Rob Pike, Dmitry Vyukov) — schedule
  algorithm, work-stealing strategy, blocking-pool integration.
- **OCaml 5 domains design** — fiber-as-effect-handler, миграция
  через effects, memory model.
- **Java ForkJoinPool** — work-stealing rationale, blocking-call
  escape (`ManagedBlocker`).

### Что НЕ повторять

- **Rust async + Send/Sync trait bounds** — Nova не имеет ownership,
  не сможет static-проверить fiber-safety без переделки type system.
  Полагаемся на runtime + channel-discipline.
- **Cgo blocking-call thunk** — Go обёртывает каждый C-call в
  M-thread escape. У нас FFI режим простой, blocking-pool отдельный.
- **Erlang's per-process heap** — Nova имеет single heap (managed
  GC), не process-isolated. Эрланг-модель копирования сообщений не
  подходит — у нас shared-heap channel.

---

## Связь с другими планами

- [Plan 22](22-sleep-libuv-integration.md) — фундамент (libuv,
  park/wake API). M:N расширяет: park/wake остаётся, добавляется
  cross-worker wake через `uv_async_t`.
- [Plan 21](21-channel-revision-implementation.md) — Channel
  capability-split. Под M:N owner-actor становится **обязательным**
  паттерном (Q-mn-1 option c).
- [Plan 18](18-stdlib-roadmap.md) — std.sync уже fixed «M:N-correct
  API сразу». Plan 23 включает их в runtime в multi-thread режиме.
- [D6](../../spec/decisions/05-memory.md#d6) — managed GC; pre-Plan 23
  bootstrap GC требует переделки на concurrent (Слой 6).
- [D14](../../spec/decisions/06-concurrency.md#d14),
  [D50](../../spec/decisions/06-concurrency.md#d50),
  [D71](../../spec/decisions/06-concurrency.md#d71) — concurrency model,
  обновляются после Plan 23 (исключают пометку «N:1 в bootstrap»).
- [D64](../../spec/decisions/04-effects.md#d64) — `realtime nogc`,
  Q-mn-2 уточняет под M:N.
- [D79](../../spec/decisions/06-concurrency.md#d79),
  [D91](../../spec/decisions/06-concurrency.md#d91) — Channel —
  главный shared-data mechanism под M:N.
- [D80](../../spec/decisions/06-concurrency.md#d80) — per-fiber
  handler scoping, semantics не меняется но требует TLS migration
  (Слой 3).

---

## Definition of "ready to start"

Plan 23 переходит из roadmap'а в исполняемый план когда:

1. **Plan 22 завершён** — libuv + park/wake API доступны.
2. **Plan 21 завершён** — Channel capability-split реализован.
3. **Q-mn-1, Q-mn-2, Q-mn-3 закрыты** — memory model, realtime
   semantics, GC choice зафиксированы D-блоками.
4. **Bench-target определён** — конкретный workload, на котором
   M:N даёт ≥4× speedup на 4-core машине (HTTP-server throughput,
   parallel sort, и т.д.). Без bench-цели M:N — academic exercise.
5. **`std.sync` API стабилен** ([Plan 18](18-stdlib-roadmap.md) P0
   готов хотя бы spec'ом).

До выполнения этих условий — Plan 23 остаётся roadmap'ом, не
исполняемым планом.

---

## Что НЕ входит даже в M:N milestone

- **Distributed runtime** (multi-machine fiber'ы). Это уже Erlang/OTP-
  уровень, отдельная архитектура, не M:N.
- **NUMA-aware scheduling** — оптимизация для >2-socket серверов.
  Поверх M:N, не часть M:N.
- **Real-time guarantees** (priority queues, deadline scheduling) —
  отдельная задача под embedded use-case.
- **GPU offload** — не runtime, а stdlib + comptime.

---

## Оценка

**Объём** (предварительно, ±2x):
- Слой 1-2 (thread pool + deque): ~600 строк C.
- Слой 3 (TLS migration): ~400 строк refactor + codegen update.
- Слой 4 (cross-worker): ~150 строк.
- Слой 5 (std.sync M:N-correct): ~500 строк (большая часть в Plan 18 P0).
- Слой 6 (GC): BDW-GC integration — ~200 строк wrapper + tuning;
  свой GC — ~3000+ строк research-level.
- Слой 7 (spec): 3-5 новых D-блоков, обновление D14/D50/D71/D80.

**Time-scale:**
- Roadmap → executable plan: после закрытия Q-mn-1/2/3 (естественно
  после Plan 22 + 21 + 18-P0).
- Execution: 6-12 месяцев одного разработчика для BDW-GC варианта;
  18-36 месяцев для свой-GC варианта.

**Risk:**
- M:N runtime — самый сложный компонент language implementation'а
  (после type-checker'а с effects). Реалистично — это работа уровня
  Go's runtime, который писали 5+ инженеров годами.
- BDW-GC снижает риск кардинально (months → weeks для GC-части).
- TLS migration рисково: per-fiber state — pervasive change по
  всему runtime.

---

## Решение зафиксировать roadmap

Этот файл — **карта**, не план. Принятие = добавление в `docs/plans/README.md`
со статусом «roadmap, v1.0+ milestone», и фиксация Q-mn-* в
`spec/open-questions.md`. D-блоки **не открываются** до старта
реализации.
