// SPDX-License-Identifier: MIT OR Apache-2.0
# План 25: Production readiness — honest gap analysis vs Go/Rust

> **Статус:** roadmap, не план-исполнения. Анализ остающегося отставания
> runtime'а Nova от Go/Rust state-of-the-art.
> **Создан:** 2026-05-11.
> **Не блокирует** текущую stdlib работу (Plan 18) — фиксирует honest
> picture для решения «когда сможем сказать production-grade для
> backend / multi-core / real-time».

---

## Зачем этот документ

После Plan 22 hardening (Ф.7-Ф.11) runtime считается «production-grade
для Windows». Это **правда для single-core CLI tools / scripts**.
Это **неправда для multi-core backend / high-throughput proxy / real-time
system**. Plan 25 фиксирует разницу честно, чтобы не было false claims
в README и project-creation.txt.

Источники сравнения:
- Go runtime (1.22+): M:N scheduler, growable stacks, concurrent GC,
  preemption через signals, work-stealing.
- Rust async (tokio): stackless tasks, multi-threaded work-stealing
  scheduler, cooperative budget preemption, drop-cancel.
- libuv as event loop layer: общее с Node.js, известные production
  characteristics на всех major platforms.

Этот документ — **dispassionate gap analysis**, не critique. Nova
делает разумные trade-off'ы (single-threaded scheduler проще для
bootstrap, fiber stacks дают более intuitive concurrency model),
но эти trade-off'ы должны быть **видны** в спеке и плане, не
скрыты под маркетинговым «production-grade».

---

## 7 пунктов отставания

Каждый пункт — что отстаёт, почему важно, blocker до closing'а.

### G1. Single-threaded scheduler (N:1)

**Что.** `nova_supervised_run` крутит fiber'ов round-robin на одном
OS-thread'е ([D71](../../spec/decisions/06-concurrency.md#d71)). На
16-core машине — 1/16 CPU.

**Сравнение.**
- Go: GOMAXPROCS=число ядер, M:N work-stealing. Default scaling.
- Rust tokio (multi_thread runtime): аналогично, work-stealing
  scheduler с N worker threads.

**Импакт.** Блокирует все multi-core use-cases:
- HTTP server обрабатывает запросы последовательно (только async-IO
  overlap, не CPU-параллельность).
- `parallel for` — cooperative scheduling в одном потоке, не реальный
  параллелизм.
- CPU-bound workload (compression, parsing, validation) использует
  одно ядро.

**Blocker до closing'а:** [Plan 23](23-mn-runtime-roadmap.md) (M:N
runtime roadmap). Уже существует как separate plan со списком
компонентов: lock-free deque, atomic primitives, concurrent GC,
per-fiber state migration. **Реализация — отдельная задача**,
roadmap описывает контуры.

**Acceptance:** `parallel for` на 8-core машине показывает 6-8×
speedup на CPU-bound workload (parser, validation). 1M concurrent
HTTP requests при 16 cores — все cores used.

---

### G2. Stack-per-fiber через minicoro (4-8 KB на fiber)

**Что.** Каждый fiber имеет fixed-size stack ~4-8 KB через minicoro.
На миллион fiber'ов = 4-8 GB RAM only для stacks.

**Сравнение.**
- Go: growable stacks, начинают с 2 KB, grow on demand до GB.
  Миллион горутин на 1-2 GB heap — нормально.
- Rust async: stackless через state machines. Future ~ 64 bytes
  + capture sizes. Миллион tasks на ~100 MB.
- Erlang/BEAM: growable stacks, ~309 bytes initial. Миллионы
  процессов норма.

**Импакт.** В spec/overview.md записано «миллион fiber'ов на машину —
норма» — это **неточно для Nova bootstrap**. Реалистично: 100k-200k
на 1 GB heap. Это всё равно лучше Java threads (1 MB stack), но
не на уровне Go.

**Blocker до closing'а:**
- Growable stacks в minicoro — патчить minicoro нельзя
  ([feedback_third_party_libs](../../memory/feedback_third_party_libs.md)).
  Либо найти другую coroutine library, либо реализовать свой
  stack-growth mechanism в обёртке.
- Альтернатива: переход на stackless модель через codegen-rewrite
  (state-machine generation как Rust). Это **большая работа** —
  меняет всё IR codegen.

**Acceptance:** 1M idle fiber'ов на 1-2 GB heap. Stack-growth
benchmark показывает <10% overhead на typical workloads.

**Status:** не открыта работа. Сначала Plan 23 (M:N) — он не зависит
от stack-growth. Потом отдельный план.

---

### G3. Memory management — главное упрощение runtime'а

**Что обнаружено (2026-05-11 honest pass):** дефолтный `alloc.c` —
**plain malloc без GC и без RC**. Объекты создаются через `nova_alloc`
(= `malloc`) и **никогда не освобождаются**. `nova_release` — no-op.
Codegen НЕ генерирует RC retain/release calls.

Это значит: **любой long-running процесс упадёт по OOM**. Работает
только потому что CLI tools и тесты — короткие процессы.

В spec/decisions/05-memory.md заявлено: «Concurrent GC с pauses <1ms»
— это **цель дизайна**, не текущая реализация (явно помечено как
«MVP/v1.0+ roadmap»). Но в `spec/overview.md` чуть путаннее («паузы
<1ms» без подписи "цель") — нужно sync'нуть.

#### G3a. Default malloc-only — реальный production blocker

**Сравнение.**
- Go: concurrent GC sub-ms pauses, production-grade на день один.
- Rust: no GC (RAII), нет проблемы.
- Java ZGC/Shenandoah: <1ms на multi-TB heaps.
- Nova **сейчас**: leak forever.

**Что есть в репо:**
- `alloc_boehm.c` — Boehm GC implementation (готов, требует vcpkg gc.lib).
  **vcpkg уже установлен** в `compiler-codegen/vcpkg_installed/x64-windows-static/`
  — gc.lib доступен. Не подключён к test_runner.
- `alloc_rc.c` — RC implementation (готов **как backend**, но codegen
  НЕ inserts `nova_retain`/`nova_release` — это нужно делать в каждой
  copy/move/scope-exit). RC backend бесполезен без codegen support.

**Blocker до closing'а:**

**Шаг 1 (быстро, ~1 день):** добавить `--gc=malloc|boehm` flag в
test-all. Default остаётся malloc для compatibility. Сделать Boehm
opt-in через build flag. Verify что Boehm work'ает (test alloc_count
vs live_count после workload — должен decrease).

**Шаг 2 (средне, ~3 дня):** сделать Boehm GC **default** для всех
non-realtime workloads. Realtime блоки (`realtime nogc { }`) остаются
на arena allocator (region API, отдельный план).

**Шаг 3 (долго, недели):** RC codegen — `nova_retain` на каждый
copy/return-from-function, `nova_release` на scope-exit. Более
production-grade для real-time (no pauses), но cycle-leaks. Это
**альтернативная стратегия** Boehm — выбор после benchmarks.

**Acceptance:**
- `memory_growth_check` тест показывает `live_count` ограниченный
  под Boehm (был ~unbounded под malloc).
- Long-running stress (1M iter создания-разрушения objects) не
  растёт linearly в memory.

#### G3b. GC pause time не measured

**Зависит от G3a.** Если default GC — Boehm: измерить pause
distribution на realistic workloads (10k/100k/1M objects).
Если pauses слишком велики (>10ms p99 на 1 GB) — рассматривать
альтернативные GC (RC, custom incremental).

**Текущее spec заявление в `spec/decisions/05-memory.md`:**
«Concurrent GC, паузы <1ms p99» — **goal**, не текущее состояние.
Spec корректно помечает это как «v1.0+ дизайн-цель».

В `spec/overview.md` ту же фразу без disclaimer'а — **нужен
honest update** "паузы <1ms — целевое требование, текущая реализация
upper bound TBD после G3a".

**Status:** G3a в работе (этот retro fixes documentation, runtime
implementation — отдельный план Plan 27).

**Update 2026-05-12 (audit):** Plan 27 Ф.4 ✅ ЗАКРЫТА — default backend
переключён на Boehm (`GcKind::default()` = Boehm в `test_runner.rs:649`;
`nova-cli/src/main.rs:752,765` — `gc.as_deref().unwrap_or("boehm")`).
Acceptance criteria выполнены: `memory_growth_check` PASS, Plan 32
`gc_differential.nv` показывает bounded heap (348 KB на 500k allocs
vs ожидаемых 16 MB при linear leak). **G3a closed.** Остаются:
- G3b (pause measurement) — Plan 27 Ф.3 reopened, см. audit-section
  плана 27.
- Plan 27 Ф.5 (Linux libgc detect) — отдельный gap, см. там же.

---

### G4. Linux smoke test (Plan 22 Ф.11 deferred)

**Что.** Cross-platform build_libuv готов в коде (test_runner.rs),
но never tested на Linux. Production deployment на Linux без smoke
test = roulette.

**Сравнение.**
- Go: validated на linux/amd64, linux/arm64, darwin/*, windows/* через
  extensive CI на каждый release.
- Rust: validated на 5+ tier-1 platforms через CI.

**Импакт.** Любой Linux deployment может найти platform-specific
bugs (errno mapping, path separators, ABI mismatch). Сейчас Nova =
Windows-only despite cross-platform code.

**Blocker до closing'а:** Linux/WSL environment access. Это не
техническая, а **операционная** проблема — нужен Linux machine либо
GitHub Actions CI setup.

**Acceptance:**
- 138/138 nova_tests PASS на Linux Clang (WSL Ubuntu / native Linux).
- CI badge либо verified manual testing log.

**Status:** Plan 22 Ф.11 deferred TBD. Откладывается до **первого
Linux deployment trigger** либо параллельно с Plan 18 std.net (там
Linux validation обязательна).

---

### G5. Preemption budget (long compute без yield-point'а)

**Что.** Nova fully cooperative — fiber yield'ает только через
эффект-вызовы (Time.sleep, Net.get, Channel.recv). Long compute
без эффектов (parsing, hashing, validation, CPU-bound loop)
блокирует scheduler.

**Сравнение.**
- Go 1.14+: async preemption через SIGURG signals — после 10ms
  compute горутина forcibly preempted. Fairness гарантирован.
- Tokio: cooperative + budget-based — каждый `.await` чек'ает budget,
  если превышен — yield обязателен. Tasks которые «никогда не yield'ят»
  — instrumented как problem.
- Erlang/BEAM: reduction-count preemption — каждые N reductions
  scheduler switch.

**Импакт.**
- Fairness не enforced. Один CPU-bound fiber starve'ит остальных в
  scope'е (если не делает Time.sleep(0)).
- Cancel propagation cooperative — cancel доходит до fiber'а только
  на next yield-point. Long compute → cancel «зависает» (см. G6).

**Blocker до closing'а:**
- **Опция A** (Go-style preemption через signals): сложно cross-
  platform (Windows signals weak), требует tight integration с
  minicoro internals.
- **Опция B** (Tokio-style budget): codegen-уровень — каждые N
  C-statements emit'ить `_nova_budget_check()`. Это **большая
  работа** — меняет каждый emitted function.
- **Опция C** (explicit): require programmer to write `Time.sleep(0)`
  в long compute. Worst для UX, проще всего реализовать.

**Acceptance:**
- Benchmark: fiber с `for _ in 0..1_000_000 { compute_heavy() }` не
  блокирует sibling fiber'ов более 50ms p99.

**Status:** не открыта работа. Скорее всего — после Plan 23 (M:N),
там preemption важнее.

---

### G6. Cancel propagation — cooperative, не enforced

**Что.** Cancel-token set'ит `cancel_requested = true` на scope'е,
fiber'ы видят это **только на next yield-point** (Plan 22 R4
обеспечивает immediate wake для parked fiber'ов через generic
stop_cb, но **только для blocked-в-эффекте**). Fiber в long pure
compute не видит cancel пока не вызовет эффект.

**Сравнение.**
- Rust async: drop semantics — futures drop'аются immediate когда
  scope cancelled. Compute не приостанавливается, но **next await**
  не выполняется. Это аналогично Nova.
- Go: `ctx.Done()` channel-based, программист сам полл'ит. Cooperative
  как Nova.

**Импакт.** Long compute → cancel «зависает» до следующего эффект-
вызова. На большинстве workload'ов приемлемо (Net/Db/Time everywhere),
но pure-compute workload (parser, validator, hash) — нет.

**Связь с G5.** Preemption budget (G5) **автоматически** решает
cancel propagation — budget-check inserts cancel-poll.

**Blocker до closing'а:** связан с G5. Если G5 решим Опцией B
(budget) — G6 closes автоматически.

**Acceptance:**
- Benchmark: cancel-token cancelled во время long pure compute fiber'а
  пробуждается в <50ms p99.

**Status:** не открыта работа. Решается вместе с G5.

---

### G7. Ф.8 close-cb state machine — ✅ ЗАКРЫТО (2026-05-11)

**Было.** `uv_run NOWAIT` busy-loop в `_nova_sleep_via_libuv`
close-wait phase. 1-2 iter typical, но на high-load adds latency.
Last busy-loop в production path (R7 «no busy-loops anywhere»
нарушен в одном месте).

**Решено.** [Plan 22 Ф.8](22-sleep-libuv-integration.md#ф8--close-callback-state-machine-reorg--d93-syncasync-stop_cb---✅-done):
- D93 расширен `NovaStopMode` enum `{SYNC, ASYNC}` — формальный
  contract для cancel-during-park flow.
- `nova_sched_cancel_all_pending` различает SYNC (unpark immediate)
  vs ASYNC (ждёт backend wake).
- Sleep state-machine `{PENDING, CLOSING, CLOSED}` — единый park на
  весь lifecycle, timer_cb инициирует close без wake, close_cb wake'ает.
- Busy-loop удалён, R7 fully enforced.

**Result.** Sleep_real_clock + cancel_stress + sleep_bench 10k +
sleep_leak_check PASS. Q-D93-sync-async-stop закрыт.

**Side effect.** Каждый sleep теперь добавляет ~2-3ms на ASYNC
close_cb wait. sleep_leak_check #2 (1000 sequential sleep(10))
budget релакс'нут с 15s → 30s для Windows timer-resolution headroom.
Sequential sleep workloads чуть медленнее, concurrent workloads
не affected (parallel close_cb).

### G8. Open-coded defer (Go 1.14+ style)

**Сейчас.** Plan 20 Ф.4/Ф.8 closures defer/errdefer cleanup через
`setjmp`/`longjmp` пары:
- На каждый scope с defer'ом эмитим `NovaFailFrame` + `NovaInterruptFrame`,
  два push/pop в thread-local stack + setjmp (сохраняет ~200 байт
  jmp_buf и регистров).
- Cleanup-cascade на throw реализован через cascading longjmp (каждый
  scope re-throws после своего cleanup'а).
- Throw-path и normal-path делят один setjmp wrapper.

**Cost.** На каждый вход в scope с defer:
- 2× setjmp (jmp_buf save) + 2× push (TLS write).
- На exit: 2× pop. **Даже если throw никогда не происходит.**
- Hot loops с defer внутри (например `for _ in items { defer cleanup() }`)
  накапливают этот cost на каждой итерации.

Семантически корректно, но измеримо медленнее zero-cost Rust Drop
или Go 1.14+ open-coded defer.

**Go 1.14 решение (PGO + statically-allocated slots).** Компилятор
делает inline-анализ:
1. Определяет max-depth defer stack для функции статически (через
   AST walk).
2. Аллоцирует boolean flags в stack frame ("defer N active").
3. На `defer body` — устанавливает flag.
4. На exit (normal/return/panic/...) — inline эмитит проверку flags
   + cleanup-code прямо в exit-pathway. Без setjmp/longjmp.
5. Panic-path использует **unwind frame metadata** (Go stack unwinder)
   для invocation cleanup'ов вместо longjmp.

**Production benefit:** open-coded defer **zero-cost when not fired**
(идентично hand-written cleanup), 30%+ ускорение Go workloads
(measured в Go 1.14 release notes).

**Trade-off для Nova:**
- Refactor codegen `enter_defer_scope`/`leave_defer_scope` на flag-based.
- Throw-path требует другой механизм: либо emit-cleanup-в-каждой-exit-
  pathway (code duplication), либо stack-walk unwinder (требует
  frame-pointer + debug info, доп. complexity).
- Panic-path: NovaFailFrame можно оставить как top-level fn-frame, но
  с inline-cleanup перед re-throw.

**Status.** Plan 20 Ф.4/Ф.8 semantically complete (нулевые регрессии,
27 positive + 6 negative tests). Performance overhead приемлемый для
bootstrap. G8 — future codegen optimization, **отдельный план**
(оценка 1-2 недели работы, требует benchmarks для measure'а реального
impact'а на realistic workloads).

**Когда делать.** После Plan 18 (stdlib) — когда есть realistic benchmark
с defer'ами (file I/O cleanup, channel close, transaction rollback)
для measure'а реального cost'а. Если cost <2% — не приоритет; если
>10% — закрытие приоритетное.

**Связь:** [D90](../../spec/decisions/03-syntax.md#d90) — defer spec
(семантика unchanged); [Plan 20 Ф.4 codegen](20-defer-implementation.md#ф4-codegen--scope-stack-defer-callbacks)
— текущая реализация через setjmp.

---

## Сводная таблица

| # | Gap | Plan / blocker | Приоритет | Impact |
|---|---|---|---|---|
| G1 | Single-threaded scheduler | [Plan 23](23-mn-runtime-roadmap.md) | **Высокий** | Multi-core unblock |
| G2 | Fixed fiber stacks | TBD после Plan 23 | Средний | 1M fibers target |
| **G3a** | **Default malloc-only (leaks forever)** | Plan 27 (новый) — Boehm switch | **Высокий** | **Long-running processes падают по OOM** |
| G3b | GC pause time не measured | После G3a | Средний | Real-time / latency tail |
| G4 | Linux smoke | Plan 22 Ф.11 (нужен env) | **Высокий** | Deployment gate |
| G5 | Preemption budget | TBD после Plan 23 | Средний | Long-compute fairness |
| G6 | Cancel propagation | Связан с G5 | Низкий | Pure-compute cancel UX |
| G7 | ~~Ф.8 close-cb busy-loop~~ | ✅ ЗАКРЫТО (Plan 22 Ф.8, D93 ASYNC) | — | — |
| G8 | Open-coded defer (Go 1.14+) | TBD после Plan 18 + benchmarks | Низкий | Hot-loop defer overhead (oценка 2-10%) |

---

## Что значит «production-grade» для разных use-case

После Plan 22 hardening:

| Use case | Status | Что блокирует |
|---|---|---|
| Single-core CLI tool / script (короткое время жизни) | ✅ **Production-grade** | — |
| Single-host server (low traffic) | ❌ Blocked | **G3a (memory leaks forever под default malloc)** |
| Linux deployment | ⏸ Blocked | G4 (Linux smoke) |
| Multi-core backend server | ❌ Blocked | G1 (M:N) + **G3a (GC)** |
| 1M+ concurrent connections | ❌ Blocked | G1 + G2 (stacks) + **G3a** |
| Real-time (audio, gaming, trading) | ❌ Blocked | **G3a (GC)** + G3b (pauses verified <1ms) |
| Hard-real-time | ❌ Не цель | — (Nova не RT-OS) |

**Honest summary:** Текущий runtime — **только short-lived процессы**
(CLI tools, build scripts, тесты). Любой server / daemon / long-running
worker упадёт через минуты-часы под нагрузкой из-за memory leaks.

**Реалистичный честный summary для README:**

> Nova bootstrap (v0.x) — production-grade для Windows CLI tools и
> low-traffic single-host backend'ов. Multi-core backend, 1M+ fibers,
> real-time use-cases — требуют Plan 23 (M:N runtime) и GC pause
> validation (Plan 25 G3). Linux deployment — Plan 22 Ф.11 (требует
> environment).

---

## Что делать дальше — приоритизация

**Срочное (production blocker):**

1. **G3a (Plan 27 — GC switch)** — переключить default на Boehm GC.
   vcpkg gc.lib уже available в репо. Без этого Nova **не usable**
   для любых long-running workloads.

**Quick wins после G3a (дни-недели):**

2. **G3b** — GC pause measurement (после G3a имеет смысл).
3. **G4** — Linux smoke setup в WSL. ~1-2 дня. Cross-platform code
   never tested на Linux.

**Большие задачи (недели-месяцы):**

4. **Plan 23 (M:N runtime, = G1)** — самый большой рычаг. После него
   Nova competitive на multi-core. См. отдельный roadmap.
5. **G5/G6 preemption** — после Plan 23, потому что M:N меняет
   scheduling fundamentally.

**Низкоприоритетное (после v1.0):**

6. **G2** (growable stacks) — переход на stackless либо custom
   stack-growth. Большая работа, маленький impact для bootstrap
   нишы (CLI / mid-traffic backend).

---

## Что НЕ входит в Plan 25

- **Конкретные implementation steps.** Plan 25 = gap analysis, не план
  работы. Implementation для каждого G — отдельный план либо
  существующий roadmap.
- **Performance benchmarks.** Бенчмарки появятся вместе с
  implementation (G1 — Plan 23 bench, G3 — Step 1, G5 — TBD).
- **Решение «когда v1.0».** v1.0 определяется отдельно — не «когда
  все G closed», а «когда synthesis достаточен для target audience».
  v1.0 может быть достигнут с G1 закрытым (multi-core backend) даже
  если G2 (1M fibers) — нет.

---

## Связь с другими планами

- **Plan 22** — закрыл baseline runtime (libuv, park/wake, sigint,
  heap arrays). Plan 25 = honest assessment что осталось.
- **Plan 23** — M:N roadmap, закрывает G1. Самостоятельный план.
- **Plan 21** — Channel implementation, требует Ф.8 (G7) closed
  через D93 sync/async stop_cb enum.
- **Plan 18** — stdlib P0. Не зависит от Plan 25 напрямую, но Linux
  validation (G4) обязательна для std.net.

---

## История

- **2026-05-11** — создан после hardening Ф.7-Ф.11 как honest
  follow-up к завышенному «production-grade» в Plan 22 retro.
  Триггер — discussion: «есть что-то, что в Go/Rust сделано лучше?».
