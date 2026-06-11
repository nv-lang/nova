<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 83-study-go-c-mn: Port Go 1.4 C-era M:N runtime into Nova

> **Создан:** 2026-06-11 (из ACTION `[M-83-study-go-c-mn]` в [83-mn-runtime-roadmap.md](83-mn-runtime-roadmap.md)).
> **Статус:** 🟡 Ф.1 в работе; Ф.2-Ф.8 PLANNED. Декомпозиция выведена из research-workflow (11 агентов, Go 1.4 fetch + Nova M:N map + gap-анализ).
> **Приоритет:** P0 — закрывает оба открытых M:N race-маркера структурно.
> **Оценка:** крупная многонедельная работа (по сути переписывание планировщика); 8 фаз.
> **Родитель:** [Plan 83](83-mn-runtime-roadmap.md) (M:N umbrella).
> **Worktree:** `D:\Sources\nv-lang\nova-p83-gomn`, ветка `plan-83-go-cmn` from `main`.
> **Закрывает:** `[M-83.11-grow-vs-wake-race]` (Ф.1+Ф.2+Ф.3), `[M-83.10.4-iso-cancel-startup-race]` (Ф.5).

---

## 0. Источник истины и метод

Декомпозиция получена research-workflow'ом: 4 агента смапили текущий M:N Nova
(scheduler core, scope/fiber/spawn, driver+timers+blocking, cancel+races), 5 агентов
зафетчили и проанализировали Go 1.4 C-рантайм (G-P-M proc.c, netpoll, timers, sysmon,
park/note), 2 агента-синтезатора выдали gap-анализ (9 gaps) + атомарную декомпозицию.

**Go = последняя C-версия рантайма (go1.4 / go1.4.3, сентябрь 2015)** — последний релиз
до C→Go перевода рантайма в go1.5. Источники: `src/pkg/runtime/proc.c`, `runtime.h`,
`lock_futex.c`/`lock_sema.c`, `netpoll*.c`, `time.goc`.

### 0.1 Стратегия V1: максимально дословный порт Go-кода

**Решение (user, 2026-06-11): для первой версии — акцент на ИДЕНТИЧНОЕ копирование
Go-кода**, а не ре-имплементацию. Обоснование: Go-код проверен годами в production;
дословный перенос наследует эту корректность и минимизирует риск внести свой баг при
переписывании планировщика (главный риск этой работы).

**Достижимая цель = «структурно идентично», НЕ byte-identical.** Go C-код ссылается на
сотни Go-runtime символов, которых в Nova нет (структуры `G`/`M`/`P`, stack-switch
`mcall`/`gogo`/`systemstack`, `note`/futex, `runtime·cas`/`atomicload`, GC write-barriers,
`runtime·sched`/`allp`) → дословный `proc.c` не компилируется. Поэтому политика порта:

- **Сохраняем 1:1:** control flow, имена переменных (`h`, `t`, `n`, `batch`), инлайн-
  комментарии оригинала (`// load-acquire, synchronize with consumers` и т.д.), структуру
  функций. Каждая ported-функция несёт ссылку на go1.4 origin (`port of go1.4 proc.c::X`).
- **Заменяем только:** типы (`G*`→`mco_coro*`), атомики (`runtime·cas`/`atomicload`/
  `atomicstore`→C11 `__atomic_*`), глобальный `sched.lock`→Nova-спинлок, `runtime·throw`→
  defensive return (не abort процесса на stale-snapshot).
- **Префикс `nova_`** на публичных именах (`nova_runq_put` ↔ `runqput`) — обязателен по
  Nova-конвенции (избежать symbol-collision), тело идентично.
- **Где verbatim невозможен в принципе** (фазы со stack-switch — gopark/mcall; note —
  futex/sema на g0-стеке): переносим ЛОГИКУ, механизм отличается (minicoro вместо
  Go-assembly). Это явно отмечается в плане по фазам.

**Ring-функции Ф.1 (`runqput/get/grab/steal/putslow`) — самодостаточны** (работают над
`runq[256]` + 2 uint32 + global-list), поэтому фиделити максимальный — практически
построчный порт go1.4 `proc.c`.

### 0.1.1 Лицензия и атрибуция

Go распространяется под **BSD-3-Clause** — совместима с Nova (MIT OR Apache-2.0).
Алгоритмы/идеи **не охраняются copyright** (переносимы свободно), но при дословном
переносе кода BSD требует сохранить copyright Google + текст лицензии + disclaimer.
Заведён **[`THIRD_PARTY/go-LICENSE`](../../THIRD_PARTY/go-LICENSE)** (полный BSD-3-Clause +
copyright The Go Authors + список ported-файлов). Каждый ported-файл несёт in-file ссылку
на go1.4 origin.

### 0.2 Model / effort / thinking

| Фаза | Модель | Effort | Thinking | Обоснование |
|---|---|---|---|---|
| **Ф.1** fixed runq | Opus 4.8 (1M) | High | ON | Scheduler rewrite; load-bearing; flag-gated parallel path → atomic cut-over |
| **Ф.2** gopark ordering | Opus 4.8 (1M) | High | ON | Касается всех blocking-примитивов; unlockf veto-and-resume subtle |
| **Ф.3** nspinning + note | Opus 4.8 (1M) | High | ON | Windows lock_sema register-then-check — subtlest piece на primary platform |
| **Ф.4** global queue + steal-half | Opus 4.8 (1M) | High | ON для runnext-policy | Policy-слой над Ф.1, no new race surface |
| **Ф.5** iso-cancel latch | Opus 4.8 (1M) | High | ON | P1 race с failed tactical history — нужен структурный фундамент Ф.2 |
| **Ф.6** per-worker timer heaps | Opus 4.8 (1M) | High | ON | Cancellation semantics change — риск reopening iso-cancel |
| **Ф.7** sysmon observe/recover | Opus 4.8 (1M) | High для observer | Safety net поверх, НЕ замена структурного фикса |
| **Ф.8** netpoll evaluation | Opus 4.8 (1M) | High | ON | Largest behavioral change; readiness (POSIX) vs completion (IOCP). Stage LAST; может закрыться как evaluation-only |

**Inherits feedback rules** Plan 83.11 §0 (commit-per-phase, git add specific, no Co-Authored-By,
diff --cached перед commit, no Nova syntax invention, update logs).

---

## 1. Контекст — почему порт Go-планировщика

Текущий Nova M:N имеет **два открытых race-маркера**, на которых tactical-попытки провалились:
- `[M-83.11-grow-vs-wake-race]` — растущие массивы `NovaSchedState` (`parked[]`,
  `pending_wake[]`, ...) реаллоцируются в `nova_sched_grow_state` **plain non-atomic
  pointer-swap'ом**, конкурентно с driver-потоком, читающим их в `nova_sched_wake` →
  torn-base-pointer → lost-wake → детерминированный hang. 3 tactical-фикса застряли на 55-65%.
- `[M-83.10.4-iso-cancel-startup-race]` — cancel в startup-окне теряется (`nova_sched_find_state`
  возвращает NULL). 83.10.5 застрял на 55%.

**Go C-рантайм решил ровно эти классы проблем структурно** (G-P-M, fixed runq, gopark):
прецедент доказан — Go реализовал production M:N work-stealing scheduler в C (go1.0-1.4).

---

## 2. Gap-анализ (Nova vs Go 1.4) — 9 gaps

| # | Область | Nova сейчас | Go-подход | Severity | Закрывает |
|---|---------|-------------|-----------|----------|-----------|
| 1 | **Per-worker run queue** | Chase-Lev deque (`deque.h`) realloc'ит `NovaDequeArray*` + `NovaSchedState` 4 массива realloc'ятся **plain store** | `P.runq` = `G* runq[256]` inline в P, **fixed, never realloc**; overflow → global linked-list (`runqputslow` spill-half) под `sched.lock` | **P0** | grow-vs-wake (GROW половина) — load-bearing |
| 2 | **Wake-side lost-wakeup guard** | отсутствует (`nmspinning`/re-scan нет; worker блокируется в `uv_run(ONCE)`) | `nmspinning` budget (`wakep` CAS 0→1) + re-scan всех `P.runq` перед park (`findrunnable` drop-spinning-then-rescan) + note (futex/sema register-then-check) | **P0** | grow-vs-wake (WAKE половина, тот самый 65% плато) |
| 3 | **Park/wake correctness** | `pending_wake` CAS-counter, t1..t4 checkpoints + t4-race reload | `gopark(unlockf,...)` — flip-to-Waiting → detach → unlockf на g0-стеке | P1 | grow-vs-wake (partial) — удаляет pending_wake |
| 4 | **I/O readiness** | отдельный libuv driver-поток | netpoll в scheduler loop (epoll/kqueue/IOCP), `netpoll()` из `findrunnable` | P1 | grow-vs-wake + iso-cancel (largely) |
| 5 | **Timers** | libuv `uv_timer` на driver | in-runtime 4-ary min-heap + `timerproc` | P1 | grow-vs-wake (sleep slice) |
| 6 | **Sysmon** | preemption есть, нет syscall-retake/progress-edge | `sysmon` P-less loop + `retake` | P2 | grow-vs-wake (observe-only, НЕ фикс) |
| 7 | **Global queue + work-stealing** | single-item round-robin steal | global overflow + steal-half (`runqsteal` half), 2×n random victims | P2 | grow-vs-wake (enabling) |
| 8 | **Blocking offload** | libuv threadpool callback | `entersyscall`/`exitsyscall` P-handoff | P3 | none (completeness) |
| 9 | **Iso-cancel startup** | cancel до scope-establish → drop (NULL state) | gopark unlockf под scope-lock + READY-latch на стабильном per-fiber state | P1 | iso-cancel-startup-race |

**Главная находка:** баг grow-vs-wake — в **реаллокации**, не в memory ordering (fences в
`deque.h` корректны). Fixed `runq[256]` (стабильный базовый адрес) структурно исключает
torn-base-pointer. Это load-bearing фикс.

---

## 3. Целевая архитектура (Go-1.4-grounded)

Маппинг: **Nova worker = Go M · логический контекст планирования = Go P (сделать явным:
runq + run-permit = один юнит) · minicoro fiber = Go G · `NovaSpawnCtxBase` = struct G**
(стабильный per-fiber дескриптор).

**Три структурных инварианта + observability:**

1. **Fixed-size runq, never realloc.** Каждый worker владеет inline `mco_coro* runq[CAP]`
   (CAP=256, mask=255; monotonic uint32 head/tail, mask только при доступе; len=tail−head;
   full=tail−head≥CAP; empty=tail==head). Single-producer tail (store-release, no CAS);
   multi-consumer head (CAS на каждый advance). Overflow → spill HALF в global queue
   (intrusive `schedlink` на `SpawnCtxBase`) под `nova_sched_lock` (`runqputslow`).
   **Park/wake state переезжает на сам fiber** — `SpawnCtxBase` (стабильный GC-pinned адрес,
   индексируется по pointer, не по slot): 4-state latch nil/WAIT/READY/dispatched. Растущие
   `NovaSchedState` массивы + Chase-Lev deque + `nova_sched_grow_state` **ретайрятся**.
   Fences переносятся как есть (они корректны).

2. **gopark ordering + one-shot note.** flip-to-Waiting → detach → unlockf(veto-and-resume).
   goready ASSERT'ит Waiting перед re-queue. Удаляет pending_wake CAS-counter.

3. **netpoll + per-worker timer heaps** (поздние фазы) — убирают cross-thread coupling
   driver↔worker (источник обеих гонок), переносят I/O-readiness и timer-wake ВНУТРЬ
   scheduler loop.

4. **sysmon observe/recover** — safety net: tick-edge progress detection, конвертирует
   silent 50s TIMEOUT в detected/self-healed event. НЕ замена структурного фикса.

---

## 4. Фазы

> Детальные atomic_tasks / acceptance_criteria каждой фазы — в `tmp_gomn_research.json`
> (research deliverable) и переносятся в per-phase секции по мере исполнения. Ниже —
> цель, файлы, depends, risk, ключевые acceptance.

### Ф.1 — Fixed-size inline ring runq + global overflow + steal-half  [risk: high] [depends: —]
**Цель:** заменить растущий Chase-Lev deque + растущие `NovaSchedState` массивы на
Go-style fixed `mco_coro* runq[256]` + global overflow queue. Park/wake state → на
`SpawnCtxBase`. Load-bearing фикс GROW-половины grow-vs-wake.
**Файлы:** `runq.h` (NEW), `deque.h` (retire), `nova_sched.h`, `fibers.h`, `runtime.c`,
`runtime.h`, `nova_rt.h`, `sync.h`.
**Стратегия:** parallel path за флагом `NOVA_RUNQ_FIXED` → валидация под AUTOARM=0-gated
stress n≥66 → atomic cut-over + удаление старого пути в той же фазе.
**Ключевые acceptance:** build clang+MSVC 0 errors; `nova_sched_grow_state` symbol gone;
`stress_iso_3e.nv` 66/66; `stress_iso_large.nv` (990+ fibers) 66/66; new
`grow_overflow_spill_stress.nv` (>256 fibers/worker) 30/30 exact completion count;
throughput ±10%; GC reachability (`gc_correctness`/`deep_gc`/`ctx_pins_scope_cleanup_loop`) 30/30.

### Ф.2 — gopark ordering primitive + goready Waiting-assert  [risk: high] [depends: Ф.1]
**Цель:** Go-gopark park-примитив (flip-Waiting → detach → unlockf veto-and-resume);
goready ASSERT'ит Waiting. Удаляет t1..t4 pending_wake CAS-dance + `_nova_park_unlock_fn` TLS.
**Файлы:** `nova_sched.h`, `runtime.c`, `fibers.h`, `channel.h`, `sync.h`, `stdlib/std/runtime/sync.nv`.
**Ключевые acceptance:** все sync-fixtures 30/30; `mn_runtime_cross_channel` 66/66;
`gopark_ready_before_wait.nv` (NEG, READY-latch) 66/66; `goready_assert_not_waiting.nv`
(NEG, double-wake → assert, не double-push) 30/30 no SEGV.

### Ф.3 — nspinning budget + re-scan-before-park + Windows note  [risk: high] [depends: Ф.1, Ф.2]
**Цель:** Go два lost-wakeup механизма: `nspinning` (wakep CAS 0→1, anti-stampede) +
re-scan всех runq перед park; one-shot note (Windows lock_sema register-then-check).
Закрывает WAKE-половину (65% плато).
**Файлы:** `runtime.c`, `runtime.h`, `note.h` (NEW), `sync.h`, `fibers.h`.
**Ключевые acceptance:** `maxprocs1_lost_wake_stress.nv` 66/66; `stress_iso_3e.nv` **200/200**
(margin над плато); `note_reuse_stress.nv` (MSVC, timed-out un-register corner) 66/66 no
semaphore-handle leak; **`[M-83.11-grow-vs-wake-race]` CLOSED** (no fprintf in path).

### Ф.4 — Global runqueue fairness + steal-half hardening + runnext bound  [risk: medium] [depends: Ф.1, Ф.3]
**Цель:** полный Go `findrunnable` ordering (runnext → local → global(61-tick) → steal-half
2×n random → park); `globrunqget` fair-share; runnext-starvation policy.
**Файлы:** `runtime.c`, `runq.h`, `runtime.h`, `fibers.h`.
**Ключевые acceptance:** `imbalanced_load_stealhalf.nv` work distributed within 2× even
split 30/30; `global_overflow_fairness.nv` no starvation 30/30; throughput ±5% vs Ф.1.

### Ф.5 — gopark+READY-latch + sched-lock idle/grow  [risk: high] [depends: Ф.2, Ф.3]
**Цель:** закрыть iso-cancel startup race структурно: cancel под scope-lock через gopark
unlockf veto + READY-latch на стабильном `SpawnCtxBase` (не lazy NULL state) +
sched-lock-serialized idle/create-worker. Убрать 83.10.5 tactical-митигации, снять
AUTOARM=0 с 3 re-enabled fixtures.
**Файлы:** `nova_sched.h`, `fibers.h`, `runtime.c`, `driver.c`, `cancel.h`.
**Ключевые acceptance:** 3 re-enabled iso-cancel fixtures 66/66; `cancel_before_park_latch.nv`
(NEG) 66/66; **`[M-83.10.4-iso-cancel-startup-race]` CLOSED**; AUTOARM=0 gates removed.

### Ф.6 — Per-worker 4-ary timer min-heaps + in-runtime timer step  [risk: high] [depends: Ф.2, Ф.5]
**Цель:** заменить libuv-driver `uv_timer` для Time.sleep на per-worker 4-ary min-heaps +
timer step на M:N pool — timer-wake originate ВНУТРИ scheduler (`goready` из того же пула).
Ретайрит `NovaSleepState` 6-state machine + `armed_sleeps_head` + ARM_SLEEP/CANCEL_TIMER jobs.
**Файлы:** `timerheap.h` (NEW), `fibers.h`, `runtime.c`, `runtime.h`, `driver.c`,
`nova_sched.h`, `stdlib/std/time.nv`.
**Ключевые acceptance:** sleep-fixtures 30/30; `cancel_during_natural_fire.nv` exactly-once
66/66; `fibers_10k_sleep_cancel.nv` no leaked timers 30/30; iso-cancel (Ф.5) остаётся green.

### Ф.7 — Sysmon tick-edge observe/recover + atomic preempt reads  [risk: medium] [depends: Ф.1, Ф.3, Ф.4]
**Цель:** апгрейд Plan 44.7 preempt-sysmon до OBSERVE/RECOVER: tick-edge progress detection,
atomic loads `current_fiber_start`/`preempt_flag` (torn-read fix), force-poll каждые ~10ms,
last-worker-nobody-polling deadlock guard, grow-vs-wake observer (silent TIMEOUT → detected).
**НЕ фикс — safety net поверх.**
**Файлы:** `runtime.c`, `runtime.h`, `fibers.h`, `sync.h`.
**Ключевые acceptance:** preempt-fixtures 30/30; `single_busy_worker_io_pending.nv` no
deadlock 30/30; injected stuck-fiber (NOVA_DIAG_INJECT_STUCK) detected/recovered ≤bounded
window, не 50s TIMEOUT 10/10.

### Ф.8 — Netpoll-in-scheduler evaluation + IOCP shim  [risk: high] [depends: Ф.1,2,3,5,6]
**Цель (EVALUATION GATE):** замерить residual cross-thread wake surface после Ф.1-Ф.6.
Если оба маркера закрыты+стабильны и socket-race surface нетривиален → перенести socket
readiness в scheduler loop (process-wide poller, lastpoll single-poller token, IOCP shim
как Go `netpoll_windows.c`). **Может закрыться как evaluation-only с deferral-маркером.**
**Файлы:** `netpoll.h` (NEW), `netpoll_windows.c` (NEW), `driver.c`, `runtime.c`,
`fibers.h`, `stdlib/std/net.nv`.
**Ключевые acceptance:** go/no-go decision в этом плане с замером residual surface; если GO —
Plan 83.12 net-suite 30/30 под NOVA_NETPOLL + `net_echo_stress.nv` 66/66 single-poller token.

---

## 5. Global acceptance

- Оба маркера CLOSED с explicit verification scope: grow-vs-wake (Ф.1+Ф.2+Ф.3, обе половины),
  iso-cancel (Ф.5), каждый при stress **n≥66** (p≈0.07 → n≥66 per debugging-races.md) **БЕЗ
  diagnostic fprintf в критическом пути** (Lesson #1: race «фикснутый» printf'ом — артефакт
  memory-fence, не фикс).
- `nova build` на **обоих** toolchain (clang + MSVC/Plan 82 fiber-arena) на КАЖДОЙ границе фазы.
- Full `nova test` (C-codegen pipeline, НЕ test-interp) zero net new FAIL vs pre-Ф.1 baseline
  на каждой границе фазы (baseline захватить первым проходом на обоих toolchain).
- **No simplifications, no stubs, no AUTOARM=0 для маскировки** — тест, которому нужен
  AUTOARM=0, = признак неполного структурного фикса. Существующие AUTOARM=0, которые
  структурный фикс делает лишними, УБРАТЬ (Ф.5).
- Throughput ±10% (Ф.1) / ±5% (Ф.4) — fixed CAP=256 + spill-half не должен регрессить vs
  growable deque (steal-half/spill-half amortization обязателен).
- GC reachability: новый intrusive `schedlink` + on-SpawnCtx park state provably reachable
  (uncollectable/rooted) — не reopening §11.6 ctx_pins heisenbug при ≥2k fibers.
- Каждый diagnostic counter — RELAXED-atomic ring-buffer + post-hoc dump, env-gated
  (NOVA_DIAG_*), zero overhead unset — НЕ hot-path fprintf (non-negotiable).

## 6. Тест-план

**Positive:** `mn_runtime_smoke`, `mn_runtime_cross_channel`, `parallel_for[_array]`,
`sleep_real_clock`, `mn_runtime_sleep_in_worker`, `select_test`/`select_many_arms`/
`select_closed_test`, `sync_mutex`/`mutex_lock_cancel`/`condvar_wait_cancel`,
`supervised_cancel_test`/`nested_supervised_3_levels_cancel`; NEW
`grow_overflow_spill_stress`, `imbalanced_load_stealhalf`, `single_busy_worker_io_pending`,
`net_echo_stress` (Ф.8).

**Negative:** NEW `gopark_ready_before_wait` (READY-latch, no hang), `goready_assert_not_waiting`
(double-wake → assert, no arena-corruption), `cancel_before_park_latch` (cancel-before-park
latched), `deltimer_cancel_during_fire` (exactly-once), `runq_overflow_assert` (debug invariant);
+ existing `cancel_runtime_not_init`/`cancel_unbound_token`/`cancel_zero_fibers_in_scope`/
`close_cb_after_fiber_dead`/`scope_freed_during_cancel` (no regression).

**Stress** (via `scripts/stress_bisect.sh`, n per debugging-races.md heuristic): `stress_iso_3e`
66→200, `stress_iso_large` 66, NEW `maxprocs1_lost_wake_stress` 66, 3 iso-cancel fixtures 66,
`cancel_semantics_test` 66 (WATCHDOG_DUMP UNSET — Lesson #1), `cancel_storm_1000` 66,
`fibers_10k_sleep_cancel` 30, NEW `note_reuse_stress` 66 (MSVC), GC-suite 30; + state-dump
validation (no STALE / no lost-cancel impossible-state signatures).

## 7. Spec / D / Q обновления

- `spec/decisions/06-concurrency.md`: **D241** (fixed-size inline runq contract + global
  overflow, supersedes growable deque + nova_sched_grow_state), **D242** (gopark/goready
  ordering + READY-sentinel latch, retires D228-era pending_wake), **D243** (nspinning +
  re-scan + Go-note, worker-wakeup decoupled from uv_async), **D244** (per-worker 4-ary timer
  heap + cancel-during-fire exactly-once, amends D228 sleep-state-machine), **AMEND D103**
  (sysmon tick-edge + observe/recover), **D245** (conditional Ф.8: netpoll-in-scheduler + IOCP,
  или deferral decision).
- `docs/debugging-races.md`: §3 Tooling + §4 Lessons — RELAXED-atomic ring-buffer counters;
  lesson о валидации fixed-ring cut-over с WATCHDOG_DUMP UNSET.
- Q-memory-model: happens-before fixed-ring publish (slot store → store-release tail; consumer
  load-acquire tail → slot read) + goready/gopark wait-lock HB, supersedes deque.h PPoPP-2013 note.
- `docs/project-creation.txt` + `docs/simplifications.md` + `nova-private/discussion-log.md`:
  closure entries per phase + формальное закрытие обоих маркеров с verification scope.

## 8. Риски (из gap-анализа)

1. **Ф.1 scope большой** — нельзя инкрементально без racy промежуточных состояний.
   Митигация: parallel path за флагом → validate → atomic cut-over.
2. **CAP=256 меняет overflow** — частый spill → contention на global lock. Митигация:
   spill-HALF (128) amortization как Go, иначе throughput-регрессия.
3. **Windows note** — lock_sema register-then-check + timed-out un-register corner = subtlest
   piece на primary platform. Баг здесь = trade grow-vs-wake на worker-wake race.
4. **Non-perturbing diagnostic** — race маскируется printf-mfence; validation только
   ring-buffer RELAXED + post-hoc dump (почему 3 tactical застряли на 55-65%).
5. **netpoll (Ф.8)** — readiness (POSIX) vs completion (IOCP); частичная экстракция socket
   из libuv при file/DNS на libuv = две I/O-пути. Stage last.
6. **Timers off driver (Ф.6)** — меняет cancellation semantics; cancel-during-fire re-prove,
   иначе reopening iso-cancel.
7. **Single global timer-heap/runqueue** — Go later sharded (many-core bottleneck). Per-worker
   heaps + global только overflow-backstop обязательны (экстраполяция к Go later design).
8. **Boehm GC interaction** — `schedlink` + on-SpawnCtx park state = новые reachability chains;
   §11.6 ctx_pins fragility при ≥2k fibers → provably reachable обязательно.

## 9. Статус исполнения

| Фаза | Статус |
|---|---|
| Plan doc + декомпозиция | ✅ 2026-06-11 |
| V1 verbatim-port policy + `THIRD_PARTY/go-LICENSE` | ✅ 2026-06-11 |
| Ф.1 `runq.h` примитив (ring put/get/grab/steal/putslow + global overflow) | ✅ ported + unit-tested 2026-06-11 |
| Ф.1 интеграция (nova_sched.h park-state→SpawnCtxBase, runtime.c rewire, cut-over) | 🟡 в работе |
| Ф.2-Ф.8 | 📋 PLANNED |

### 9.1 Ф.1 progress — `runq.h` примитив (2026-06-11)

**Сделано:** `compiler-codegen/nova_rt/runq.h` — дословный порт go1.4 `proc.c`
ring-функций (`runqput`/`runqget`/`runqgrab`/`runqsteal`/`runqputslow` +
`globrunqputbatch`/`globrunqget`). Fixed `mco_coro* runq[256]`, monotonic uint32
head/tail (mask только при доступе), single-producer store-release tail, multi-consumer
CAS head, overflow → spill-half в global intrusive-list (schedlink). Go-комментарии
сохранены verbatim; атрибуция per-function + `THIRD_PARTY/go-LICENSE`.

**Верификация (изолированная, без полной сборки nova — clang `-O2 -Wall -Wextra`, 0 warnings):**
`test_runq.c` — T1 FIFO put/get; T2/T3 overflow spill-half exact counts + drain
no-dup/no-loss; T5 `runq_steal` half victim→self conservation; **T4 concurrent: 1 producer
(put+get) racing 4 thieves (grab) над 200k fibers → conservation (каждый fiber потреблён
ровно 1 раз, 0 потерь, ring+global пусты)**. 10/10 прогонов PASS; `grab_retries>0` —
inconsistent-snapshot path реально сработал и обработан (0 потерь подтверждает корректность).

> ⚠ Это unit-валидация ПРИМИТИВА в изоляции. Полная Ф.1 acceptance (build clang+MSVC,
> `nova test` no-regression, stress `stress_iso_3e` 66/66, throughput ±10%, GC
> reachability) требует интеграции в runtime.c + park-state на SpawnCtxBase — следующий шаг.

**Осталось по Ф.1:** перенести park/wake state с растущих `NovaSchedState` массивов на
`NovaSpawnCtxBase` (4-state latch, индекс по pointer); добавить `schedlink` поле; переписать
`nova_sched.h` park/wake/register/cancel по pointer; заменить `NovaWorker.deque`→`NovaRunq` +
rewire `_worker_main`/`_worker_dispatch_ready`/spawn paths; флаг `NOVA_RUNQ_FIXED` →
валидация под stress → atomic cut-over + удаление `deque.h`/`nova_sched_grow_state`.

### 9.2 Build env + pre-Ф.1 baseline (2026-06-11)

**Решение (user): «сборка + baseline на двух toolchain, потом стоп» — cut-over НЕ
активировать.** runq CAP по решению user → **4096** (под 10k-fiber/MAXPROCS=1 workload).

**Build env worktree:** libuv submodule скопирован из main (`.git` удалён); GC env vars
→ main `vcpkg_installed/x64-windows-static`; `nova-cli` собран release (1m33s, exit 0).
`runq.h`/`test_runq.c` присутствуют, но НИГДЕ не включены — на сборку не влияют (dormant).

**Baseline concurrency (clang) — ✅ ВАЛИДЕН: PASS 102 / FAIL 6.**
Pre-existing FAIL (НЕ от этой работы — runq dormant):
- runtime-релевантные: `condvar_wait_cancel` (TIMEOUT 53s), `deep_spawn` (RUN-FAIL),
  `time_handler` (RUN-FAIL) — кандидаты на race/flaky, цель улучшить интеграцией.
- codegen/CC (от других планов на main, не рантайм): `detach_test` (E_READONLY_FIELD),
  `fn_array_collect_test` (undefined `copy`), `sleep_real_clock` (SLACK_MS undeclared).

**Baseline MSVC — ⚠ ЗАБЛОКИРОВАН worktree-env (НЕ код):** harness MSVC-путь падает на
сборке libuv/net.c (`C1083`), т.к. в copied-libuv нет собранного `libuv.lib` и сборка
libuv под MSVC в worktree не проходит. В main MSVC работает (Plan 82: 1049/16) → это
worktree setup-issue. **Next-session prerequisite:** собрать/скопировать `libuv.lib` под
MSVC в worktree ИЛИ выполнять MSVC-валидацию интеграции в main-репо.

**Не сделано осознанно (per «потом стоп»):** полный `nova test` baseline (concurrency
достаточно как relevant-срез; full baseline снимать в начале integration-сессии — иначе
устареет от merge'ей main); struct-scaffold за флагом (NovaRunq в NovaWorker, schedlink в
SpawnCtxBase) — делать в начале cut-over, когда flag-ON сборка верифицируема на двух toolchain.

**Первые шаги integration-сессии:** (1) починить worktree MSVC libuv; (2) снять свежий full
baseline clang+MSVC; (3) struct-scaffold за `NOVA_RUNQ_FIXED` (OFF→зелёная сборка обоих,
ON→компилируется); (4) cut-over park-state→SpawnCtxBase + deque→runq; (5) stress 66/66.

> Per-phase closure-секции добавляются по мере исполнения (формат Plan 83.11 §13).
