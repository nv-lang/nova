# Concurrency — параллелизм и асинхронность

Решения этой группы определяют модель параллельных вычислений Nova:
как fiber-runtime обеспечивает невидимую приостановку, какие
structured-concurrency примитивы есть в языке, и как параллелизм
выражается в коде.

| # | Решение |
|---|---|
| [D14](#d14-fiber-runtime--невидимая-инфраструктура) | Fiber runtime — невидимая инфраструктура |
| [D50](#d50-concurrency-model-spawn-detach-blocking) | Concurrency model: `spawn`, `detach`, `Blocking` |
| [D71](#d71-bootstrap-concurrency-runtime) | Bootstrap concurrency runtime: cooperative scheduler, `Time.sleep` yield-point, capture-by-value |
| [D75](#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном) | ⚠️ REVISED: `supervised(cancel: tok)` — структурная отмена с внешним токеном (keyword `cancel_scope` удалён) |
| [D79](#d79-channels--coordination-между-fiber-ами) | ⚠️ Уточнено [D91](#d91): `Channel[T]` (старая Go-style модель — один объект, send+recv на нём) |
| [D80](#d80-handler-scoping-per-fiber) | Handler scoping per-fiber — `with X = handler` локален для fiber'а, наследуется через spawn |
| [D80](#d80-handler-scoping-per-fiber) | Handler scoping per-fiber — `with X = h` биндинги изолированы между fibers |
| [D91](#d91-channel-revision--capability-split-на-chanwriter--chanreader) | `Channel` revision: capability-split на `ChanWriter[T]` / `ChanReader[T]`; `send`→`bool`; `tx.clone()` multi-writer ✅ |
| [D94](#d94-select--multiplexed-channel-operations) | `select { ... }` — финальный синтаксис: `Some(v) = rx =>`, `ChanReader.close_after(Duration)` для timeout ✅ реализован (Plan 31 ✅; Plan 44.1 Ф.3 hardening; Plan 65 — Duration-typed API revision) |
| [D124](#d124-monotonic-vs-timestamp--раздельные-типы-для-wall-clock-и-монотонных-часов) | Monotonic vs Timestamp — раздельные типы для wall-clock и монотонных часов |
| [D138](#d138-default-on-mn-runtime--production-semantics-plan-83456-ф3-active-2026-05-24) | ✅ ACTIVE — Default-on M:N runtime production semantics (Plan 83.4.5.8 closure 2026-05-24) |
| [D168](#d168-sized-atomic-types--api-contract-plan-1032) | Sized atomic types API contract: 12 types × 13 ops, MemOrdering-aware overloads, wraparound semantics |
| [D169](#d169-mutex--rwlock--reentrantmutex-family-plan-1033) | `Mutex` / `RwLock` / `ReentrantMutex` family — fiber-aware locking, fair FIFO default, writer-priority RwLock, recursive ReentrantMutex |
| [D170](#d170-coordination-primitives--semaphore--barrier--countdownlatch--condvar-plan-1034) | Coordination primitives — `Semaphore` (bounded permits), `Barrier` (reusable N-party rendezvous), `CountDownLatch` (one-shot), `Condvar` (tied to Mutex) |
| [D172](#d172-realtimeblocking-sync-class-annotation-system-plan-1036) | `realtime { }` / `blocking { }` × sync-primitive enforcement: `#parks` / `#wakes` / `#realtime` annotation system |
| [D174](#d174-sync-primitives-consume-integration-plan-1039) | Consume guards V2 — `MutexGuard`, `ReadGuard`, `WriteGuard`, `Permit`, `OnceGuard` consume types; guard-returning API; D169–D171 cross-refs updated |
| [D425](#d425-cas-возвращает-свидетеля-провала-compare_exchange-bool--resultt-plan-207) | CAS возвращает свидетеля провала: `compare_exchange`/`compare_exchange_weak` `bool` → `Result[(), T]` (amends D168 §1) |
| [D426](#d426-atomic-семейство-консолидация-имён-atomicisizeatomicusize--atomicintatomicuint-легаси-atomicint-и-atomicptr-сняты-plan-207) | Atomic-семейство: консолидация имён — `AtomicIsize`/`AtomicUsize` → `AtomicInt`/`AtomicUint`, легаси `AtomicInt` и `AtomicPtr` сняты (amends D168, D425) |
| [D439](#d439-supervised--прямая-блокирующая-операция-в-теле-без-spawn-plan-2211-162) | `supervised{}` — прямая блокирующая операция в теле без `spawn` парkуется штатно (amends D14/D50/D71/D75); её "не защищена дедлайном/токеном ЭТОГО scope" boundary ОТМЕНЕНА амендментом D442 |
| [D442](#d442) | `supervised(cancel:/timeout:/deadline:)` покрывает ВЕСЬ блок, включая прямую блокирующую операцию тела — retracts D439's boundary (amends D75/D408) |

---

## D14. Fiber runtime — невидимая инфраструктура

> ⚠️ **REVISED → [D62](04-effects.md#d62), [D64](04-effects.md#d64).**
> Изначально D14 объявлял `Async` как эффект. После D62 `Async` **не
> является эффектом** — это runtime-инфраструктура, ambient capability.
> В сигнатурах не пишется. Гарантия не-приостановки даётся блоком
> [`realtime`](04-effects.md#d64) как inverse-маркер, а не отсутствием
> `Async` в сигнатуре. Структурный параллелизм через [D50](#d50)
> (`spawn`, `parallel`, `race`, `supervised(cancel:)`).
>
> 📋 **CONCRETIZED IN [D71](#d71-bootstrap-concurrency-runtime).**
> Bootstrap-runtime: round-robin scheduler `nova_supervised_run`,
> `nova_fiber_yield` для cooperative suspension. Без preemption,
> single-threaded. Production-runtime — расширение D71.

### Что
**Fiber runtime** обеспечивает приостановку без видимого `await`/
`Future<T>`. Цвет функции отсутствует: вызов sync-функции и
suspend-функции выглядят одинаково. Точки приостановки **невидимы в
типах** — программист и LLM их не видят (это сознательное решение D62:
runtime-факт, не tipo-факт). Если нужна гарантия, что приостановки
**нет**, используется блок [`realtime { ... }`](04-effects.md#d64).

Structured concurrency — **примитивы языка** (`spawn`, `supervised`
[+ опц. `cancel:`/`deadline:`/`timeout:` — D408], `select`, `parallel for`,
`detach`, `blocking`), `race` — stdlib поверх них, см. [D50](#d50),
[D75](#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном).
*(Plan 173 §3a п.4: `with_timeout` субсумирован `supervised(timeout:)` —
подлежит удалению, `[M-174-retract-with-timeout]`; `race {…}`-блок ниже —
эскиз, не реализован: N-арный `race` — дизайн 173.1 §2a.)*

### Правило

#### Внешне — синхронно выглядящий код

```nova
fn fetch(url str) Net -> Response => ...

fn handler(req Request) Net Db -> Response {
    ro user = fetch_user(req.id)            // никаких .await
    ro posts = fetch_posts(user.id)         // никаких .await
    Response.json(posts)
}
```

Тип возврата `Response`, **не `Future<Response>`**. Программист пишет
последовательный код, а компилятор + scheduler делают остальное.
`Async` НЕ присутствует в сигнатуре (D62).

#### Внутри — fiber scheduler

Под капотом — **fiber-based scheduler** (как в Go или OCaml 5).
Когда операция эффекта `Net` приостанавливается, fiber кладётся в
очередь ожидания, scheduler берёт другой fiber. Память —
сегментированный стек или cactus stack.

#### Structured-concurrency примитивы

Это **примитивы языка**, не функции stdlib — управление параллелизмом
нельзя выразить только через эффект:

```nova
// parallel for — ждёт всех, отменяет хвост при ошибке
fn fetch_all(urls []str) Net Fail -> []Response =>
    parallel for url in urls {
        fetch(url)
    }

// race — кто первый ответил, тот и победил
// (эскиз: block-форма НЕ реализована; сейчас std race2, общий N-арный
//  race — дизайн 173.1 §2a; см. нота выше)
race {
    fetch(url_a),
    fetch(url_b),
}

// select — ожидание любого из событий (финальный синтаксис — D94,
// Plan 65 revision: ChanReader.close_after заменил Time.after(ms))
ro t = ChanReader.close_after(Duration.from_secs_f64(5.0))
select {
    Some(msg) = channel_a.recv() => { process(msg) }
    Some(msg) = channel_b.recv() => { process(msg) }
    None      = t.recv()         => { default_value }
}

// supervised(cancel:) — структурная отмена с внешним токеном (D75)
ro tok = CancelToken.new()
supervised(cancel: tok) {
    spawn do_thing()
    spawn do_other()
}

// bound на время выполнения — параметр области (D408; ex-with_timeout,
// субсумирован Plan 173 §3a / Plan 174)
supervised(timeout: 2.seconds()) {
    Db.exec(sql`UPDATE counters SET v = v + 1`)
}
```

`region { ... }` ([05-memory.md → D6](05-memory.md#d6)) живёт в этой
же категории — runtime-примитив, отвечающий за память real-time зон.

### Почему

1. **Невирусность.** Отсутствие `await`/`Future<T>` снимает «цвет
   функции» — вызов suspend-функции из suspend-функции выглядит как
   обычный вызов, без обёрток. Это значительно упрощает рефакторинг и
   AI-генерацию.
2. **D62: Async — runtime-инфраструктура, не type-fact.** Программист
   не должен думать про suspension при чтении сигнатуры. Если в
   будущем какая-то операция станет sync — сигнатура не меняется.
   Тип отражает поведение, не реализацию. Гарантия отсутствия
   suspension даётся блоком `realtime { ... }` ([D64](04-effects.md#d64)).
3. **Прецеденты.** Erlang и Go доказали, что fiber-runtime работает
   на масштабе backend (миллионы fiber'ов на узел). OCaml 5 показал
   тот же подход в строго типизированном языке.
4. **Structured concurrency встроена.** Не нужны библиотеки типа
   Trio/structured-concurrency RFC — `parallel for`,
   `supervised(cancel:/deadline:/timeout:)` — часть языка (`race` — stdlib). Это значительно безопаснее
   для AI-генерации (нет утечек fiber'ов).

### Сравнение с Rust async

| | Rust async | Nova |
|---|---|---|
| Цвет функции | да (`async fn`) | нет |
| `await` нужен | да | нет |
| Тип возврата меняется | `Future<T>` | `T` (не меняется) |
| Стоимость задачи | ~64 байта (state machine) | ~4-8 KB (fiber stack) |
| Cancellation | ручная (Drop) | structured (`supervised(cancel:)`) |
| C interop blocking | без проблем | требует `detach to OS thread` |
| Видимость suspension в сигнатуре | есть | нет (см. D62) |

Nova ближе к **Erlang/Go** по runtime, к **Koka** по типам. Платит
**памятью** (fiber stacks) ради **простоты кода** (невирусность).

### Стоимость fiber'а

Каждый fiber — несколько килобайт минимум (растёт по необходимости).
Дороже Rust state machine, дешевле OS thread. **Миллион fiber'ов на
машину — норма** (как Erlang). Миллиард — нет, для таких задач есть
`Stream`/событийная модель.

### Async — runtime, не тип

«Всё — эффект» ([D10](01-philosophy.md#d10)) — это **типовая модель**,
не **runtime-модель**. На уровне типов `Async` НЕ существует
([D62](04-effects.md#d62)). На уровне runtime async требует
fiber-инфраструктуры, как memory regions требуют allocator'а
([D6](05-memory.md#d6)). Симметрия: GC, region и fiber-scheduler — три
runtime-капабилити, которые не отражаются в эффектах.

### Что отвергнуто

- **`Future<T>` в типе возврата** (Rust/TS-стиль) — заставляет
  программиста писать `await`, заражает все вызывающие функции цветом.
- **`async/await` keywords** — отвергнуты. Cuspension — runtime-факт,
  не в типах.
- **`Async` как эффект в сигнатуре** — отвергнуто в [D62](04-effects.md#d62).
  Программист не должен видеть suspension в типах; ему достаточно
  inverse-маркера `realtime` ([D64](04-effects.md#d64)) для гарантии
  no-suspend.
- **Stackless coroutines (Rust state machines)** — экономят память,
  но требуют `Pin`/`Send`/`Sync` бойлерплейта; не подходят для
  AI-кодинга.
- **OS threads as default** — слишком тяжёлые для миллионов задач.
- **Custom Promise** как магия компилятора — отвергнут. `Promise[T]`
  как пользовательская структура, если нужна, **пишется обычным
  кодом** (handler-обёртка над `Async`).

### Открытые вопросы

- **Реализация fiber stacks** — segmented vs cactus vs on-demand
  growable. Решается на этапе runtime-разработки.
- **Дефолтный размер fiber stack** — баланс между начальной
  стоимостью и частотой роста.
- **C interop для синхронных C-вызовов** — механизм `detach to OS
  thread` нужен для блокирующих C-функций (например, `libcurl`).

### Связь

- [01-philosophy.md → D10](01-philosophy.md#d10) — «всё — эффект»
  применимо к Net/Db/Fail/Log; suspension — исключение
  ([D62](04-effects.md#d62)).
- [04-effects.md](04-effects.md) — система эффектов в целом.
- [05-memory.md → D6](05-memory.md#d6) — `region` как родственный
  runtime-примитив.
- [08-runtime.md](08-runtime.md) — три режима компиляции работают с
  fiber'ами одинаково.

### Эволюция

D14 в первой редакции объявлял `Async` как эффект в сигнатуре. После
[D62](04-effects.md#d62) `Async` убран из type-system целиком —
suspension стала ambient capability runtime'а. Гарантия отсутствия
suspension даётся блоком [D64](04-effects.md#d64) `realtime { }` как
inverse-маркером.

Открытый вопрос про C interop через `detach to OS thread` закрыт
[D50](#d50-concurrency-model-spawn-detach-blocking) — эффект
`Blocking` + примитив `blocking { ... }`.

---

## D50. Concurrency model: `spawn`, `detach`, `Blocking`

> 🔴 **`blocking { }` block-form + `Blocking` effect REMOVED.**
> Plan 113 ([D172](#d172-realtimeblocking-sync-class-annotation-system-plan-1036),
> 2026-05-30) retracted the `blocking { ... }` block-form: the parser now
> rejects it with `[D172-block-form-removed]` and directs users to a
> `#blocking fn` attribute instead. Plan 91.15 (2026-06-17) completed the
> removal — the `Blocking` effect is gone from the compiler entirely (no
> longer a built-in effect name, no longer in the realtime-suspend-effect
> list) and from `std/net` (TCP/UDP I/O methods carry only `TcpNet` /
> `UdpNet`; the effect handler parks the fiber on the libuv event loop, so
> no separate `Blocking` declaration is needed). The `§4 blocking { … }`
> subsection below and every `Blocking`/`blocking { }` mention in this D50
> are **historical** — kept for context, no longer normative.
>
> ⚠️ **REVISED → [D62](04-effects.md#d62), [D64](04-effects.md#d64).**
> Исходный D50 трактовал `Async` как эффект и упоминал «единый эффект
> `Async`». После D62 `Async` — ambient runtime-инфраструктура, не
> эффект. `Par` тоже не существует. Гарантия не-приостановки даётся
> блоком [`realtime`](04-effects.md#d64). `Detach`/`Blocking`
> остаются эффектами — у них есть видимый side-effect для caller'а
> (fire-and-forget семантика и блокировка ОС-потока соответственно),
> что делает их кандидатами на type-level декларацию.
>
> ⚠️ **Detach-эффект — ЧАСТИЧНО (обновлено Plan 173 Ф.3 п.2, 2026-07-09):** «Detach» — зарезервированное builtin-имя (types/mod.rs); **требование эффекта в сигнатуре ТЕПЕРЬ enforced** в checker'е (`[E_DETACH_REQUIRES_EFFECT]`, [D414 §2](#d414-structured-error-propagation--primary-selection-precedence-detach-policy-channel-closed-vs-value-plan-173-ф3)); самостоятельного effect-type-декла в std всё ещё нет. Поведение сирот при ошибке/панике — LogAndDrop, зашито в runtime.c (не handler); escalate-to-scope — opt-in design (`[M-173-detach-escalate-to-scope]`).
>
> 📋 **PARTIALLY IMPLEMENTED IN [D71](#d71-bootstrap-concurrency-runtime).**
> Bootstrap'ом реализованы: `supervised`, `parallel for`, `detach`
> (Plan 83.4.5.2 Ф.4 amend — **default AsyncDetach** через
> `nova_runtime_spawn_orphan` primitive: armed runtime → worker pool
> fire-and-forget; bootstrap cooperative → global orphan scope drained
> on atexit либо через `runtime.drain_orphans()` explicit-sync API),
> `Time.sleep` как yield-point, `blocking { }` (Plan 83.3 — libuv-
> threadpool offload, см. §4 «Реализация»). Capture-by-value для
> immutable scalars. Статус (обновлено Plan 173 Ф.5.4, 2026-07-10):
> `select` РЕАЛИЗОВАН (D94 + None-арм D414 §3); `supervised(deadline:/timeout:)`
> РЕАЛИЗОВАНЫ (D408) — `with_timeout` субсумирован (ретракция отложена
> `[M-174-retract-with-timeout]`); общий N-арный `race` НЕ реализован
> (std `race2`; дизайн 173.1 §2a); cancellation/error-propagation между
> fibers — 173.0 retention + D414 precedence. Orphan errors → LogAndDrop
> в caller's stderr, non-propagate (escalate-to-scope —
> `[M-173-detach-escalate-to-scope]`).

### Что

Конкретизация D14:
- `spawn` разрешён **только** внутри structured-scope.
- `detach { ... }` — отдельный примитив для долгоживущих задач
  (требует эффекта `Detach`).
- `blocking { ... }` — примитив для синхронных C-вызовов
  (требует эффекта `Blocking`).
- Никакой синтаксической отметки на месте вызова suspend-функции
  (нет `await`/`?async`) — suspension это ambient (D62).

### Правило

#### 1. Suspension — ambient (D62), не эффект

Suspension fiber'а **не пишется в сигнатуре**. `parallel for`,
`select` — синтаксические примитивы языка (D14; `race` — stdlib поверх),
они работают на уровне fiber-runtime'а, не type-system'ы.

```nova
fn fan_out(urls []str) Net Fail -> []Response =>
    parallel for url in urls {
        fetch(url)
    }
// в сигнатуре только Net Fail; suspension — ambient
```

Декларация «эта функция может suspend» — через имя или док-коммент.
Гарантия не-suspend даётся блоком [`realtime { }`](04-effects.md#d64).

#### 2. `spawn` — только в structured-scope, возвращает unit

`spawn` — keyword-конструкция (не функция). Синтаксис: `spawn expr`, где
`expr` — любое выражение: вызов функции, блок, и т.д.

`spawn body` — это **statement** (fire-and-forget внутри scope). **Возвращает
unit, не результат body.** Это сознательное решение — см. «Почему» ниже.

```nova
spawn fetch_users()          // вызов функции
spawn { compute(x) }         // inline-блок

// ✗ ОШИБКА компиляции — spawn возвращает unit, нет смысла связывать
ro r = spawn fetch_a()
```

Чтобы получить результат от concurrent-выполнения:

| Сценарий | Идиома |
|---|---|
| Нужен результат, можно подождать sequentially | прямой вызов: `let users = fetch_users()` (async прозрачный — D62) |
| Гомогенный fan-out с массивом результатов | `let xs = parallel for url in urls { fetch(url) }` |
| Гетерогенная параллельность с разными типами | `mut`-захваты внутри `supervised` |

> **`parallel for → []T` — ЛЮБОЙ элемент `T`, ЛЮБОЙ итератор
> ([Plan 173.1](../../docs/plans/173.1-parallel-collect-and-supervised-value.md) Ф.2,
> 2026-07-09; D414 §4):** сбор — через канал (клон `Sender`'а создаётся в родителе
> на момент `spawn` → move в ребёнка → `send` trailing-значения → close на выходе
> ребёнка; выделенный drain-fiber внутри scope; буфер `K = min(len, 16)` —
> back-pressure, память O(CAP), не O(N)). Элемент — примитив, heap-record,
> value-record (D228), анонимный и именованный tuple (D215), sum-тип, вложенный
> `[]T`. **Порядок элементов = completion order** (плотный, без дыр); итерационный
> порядок НЕ гарантирован — коду, которому нужен порядок, — `xs.sort()` /
> сортировка по ключу. Прежний V1-guard `[E_PARFOR_RESULT_UNSUPPORTED]`
> (Plan 173 Ф.1 #7) и примитив-whitelist УДАЛЕНЫ
> (`[M-parfor-record-result-miscompile]` закрыт на всей матрице типов —
> `nova_tests/err173_1/parfor_elem_matrix.nv`).

Пример возврата результатов из детей (Ред. Plan 173.3 / [D415](#d415-data-race-freedom--share-атрибут-capture-check-consume-в-spawn-plan-1733) —
голый mut-захват в `spawn` теперь **`[E_CONCURRENT_MUT_CAPTURE]`**; результаты
идут через `#share`-примитивы или каналы):
```nova
mut a = AtomicInt.new(0)
mut b = AtomicInt.new(0)
supervised {
    spawn { a.store(compute_a()) }  // #share-примитив — санкционированный путь
    spawn { b.store(compute_b()) }
}
use_both(a.load(), b.load())
```

`spawn()` с пустыми скобками — **запрещено**: скобки не несут смысла и
создают иллюзию вызова функции. Подробно — [D43](03-syntax.md#d43).

`spawn` **запрещён** вне structured-блока. Допустимые скоупы:
`supervised` (в т.ч. `supervised(cancel:)`), `parallel for`, `select`;
а также stdlib-функции, построенные на них (`race2`; ex-`with_timeout`
— субсумирован `supervised(timeout:)`, D408), внутри своих тел. Вне такого скоупа `spawn foo()` — ошибка компиляции.

```nova
// ✓ ОК — spawn внутри supervised
supervised {
    spawn fetch_a()
    spawn fetch_b()
}

// ✗ ОШИБКА компиляции — spawn вне scope'а
fn handler(req Request) Net -> Response =>
    spawn write_audit(req)   // ← запрещено
    Response.ok()
```

Отмена прорастает от scope'а, ошибки одного fiber'а ловятся scope'ом
(D14 structured-concurrency).

#### 3. `detach { ... }` для долгоживущих задач

Если задача должна **пережить** caller'а (фоновый аудит, отложенная
запись, метрики) — `detach { ... }`. Это:
- syntactic primitive языка (как `region`, `parallel`),
- запускает блок как новый fiber,
- не возвращает handle (fire-and-forget),
- привязан к **глобальному runtime supervisor**, не к локальному
  scope'у.

Использование требует эффекта `Detach` в сигнатуре:

```nova
fn handle_request(req Request) Net Db Detach -> Response {
    ro resp = process(req)
    detach {
        write_audit(req, resp)         // живёт после возврата handler'а
    }
    resp
}
```

`Detach` — **обычный эффект** в системе (D2): handler в скоупе можно
подменить (для тестов), capability запретить (sandbox), линтер
проверяет на лишние detach'и.

```nova
// тесты — detach исполняется синхронно, никаких background-задач
with Detach = SyncDetach {
    handle_request(req)
}
```

Глобальный default-handler `Detach` — `LogAndDrop`: throw из detached-
fiber'а логируется как warning, panic — как critical (с D13 семантикой
«fiber мёртв»).

#### 3.1. Default detach semantic — AsyncDetach (Plan 83.4.5.2 Ф.4, 2026-05-23)

`detach { body }` под production runtime (armed M:N либо bootstrap
cooperative) — **fire-and-forget на orphan fiber** (паритет Go `go fn()`,
tokio `tokio::spawn` без JoinHandle, Kotlin `GlobalScope.launch { … }`).

Runtime routing:
- **armed M:N runtime** (`runtime.is_initialized() == true`): orphan body
  push'ится в worker deque через `nova_runtime_spawn_orphan` →
  `nova_runtime_spawn_global` (round-robin worker assignment). Caller
  возвращается мгновенно; body выполнится на одном из worker'ов.
- **bootstrap cooperative** (default до Plan 83.2 flip): orphan body
  push'ится в global `_nova_orphan_scope` queue; drain'ится через
  `nova_supervised_drain_main_scope` либо на `atexit`, либо явным
  вызовом `runtime.drain_orphans()`.

`runtime.drain_orphans()` — stdlib-API (analog Go `sync.WaitGroup.Wait()`
для anonymous-spawn'ов). Используется test-suite'ом для explicit-sync
между `detach { side_effect }` и assert; production-кодом редко
требуется (caller обычно не ждёт fire-and-forget).

```nova
// Test pattern — explicit sync:
mut x = 0
detach { x = 42 }
runtime.drain_orphans()    // wait для orphan body completion
assert(x == 42)            // OK
```

LogAndDrop errors (как и до 83.4.5.2): orphan body throw →
`fprintf(stderr, ...)` + fiber dies cleanly. Caller не abort'ится;
другие orphans + main flow продолжают.

Handler inheritance: orphan fiber видит outer `with X = h` биндинги
captured на spawn-time (Plan 83.4.5.4 spawn-time TLS snapshot — паритет
Node `AsyncLocalStorage`, Kotlin `CoroutineContext.Element`).

Bootstrap `SyncDetach` (inline в caller'е) — legacy semantic; всё ещё
работает через `with Detach = SyncDetach { … }` для test-mocking
patterns. AsyncDetach — production default.

#### 4. `blocking { ... }` для синхронных C-вызовов

Синхронные C-функции (`read(2)` без `O_NONBLOCK`, `pthread_mutex_lock`,
тяжёлые computational библиотеки) **блокируют ОС-поток**. На M:N
scheduler'е это значит, что весь worker встал. Решение — отдельный
pool ОС-потоков для блокирующих задач:

```nova
fn read_file_sync(path str) Blocking Fail[IoError] -> []u8 =>
    blocking {
        c_read_file(path)             // выполняется на blocking-pool потоке
    }
```

`blocking { ... }`:
- syntactic primitive языка,
- уводит тело на отдельный ОС-поток из blocking-pool, fiber паркуется,
- worker scheduler'а возвращается в общий пул, обслуживает другие
  fiber'ы,
- когда C-код вернулся — fiber резюмится на своём home-worker'е,
- **отдаёт значение** trailing-выражения тела (`let data = blocking
  { c_read() }`),
- requires эффект `Blocking` в сигнатуре enclosing-функции.

`Blocking`-эффект:
- виден в сигнатуре (caller знает «может заблокировать поток»),
- **запрещён внутри `realtime { }`-блока** ([D64](04-effects.md#d64)) —
  блок гарантирует не-suspension, а blocking-pool вызывает suspend
  на ОС-потоке.

Размер blocking-pool — runtime-конфиг (`NOVA_BLOCKING_THREADS`,
default 64). Если пул заполнен — fiber ждёт в очереди (graceful, не
дедлок).

##### Реализация: Plan 83.3 (2026-05-22)

Bootstrap-runtime реализует `blocking { }` через **libuv threadpool**
(`uv_queue_work`) — процесс-глобальный пул ОС-потоков:

1. fiber вызывает `blocking { }` → runtime пакует тело в `uv_work_t`,
   `uv_queue_work` на loop home-worker'а;
2. fiber **паркуется** (park/wake [D93](#d93), тот же путь, что
   `Time.sleep`) — worker свободен, берёт другой fiber;
3. `work_cb` исполняется на threadpool-потоке — делает блокирующую
   работу;
4. `after_work_cb` на loop'е home-worker'а **будит** fiber с результатом;
5. fiber резюмится со значением тела.

`NOVA_BLOCKING_THREADS` (default 64) пробрасывается в
`UV_THREADPOOL_SIZE` в runtime-прологе (`nova_evloop_init`); явный
пользовательский `UV_THREADPOOL_SIZE` уважается.

##### Аменд: Plan 83.11 Ф.4 (2026-06-08) — offload через centralized driver

После [D228](#d228) (centralized I/O driver) шаг 1 выше уточнён: когда
driver-thread запущен (production M:N), `nova_blocking_offload` **не**
вызывает `uv_queue_work(nova_current_loop())` на loop'е home-worker'а, а
submit'ит job `NOVA_DRV_JOB_ARM_BLOCKING` в driver queue. Driver thread в
`_nova_driver_handle_arm_blocking` вызывает `uv_queue_work` на **своём**
loop'е (`&_nova_driver.loop`). Соответственно шаг 4 (`after_work_cb`)
исполняется на **driver thread**, а не на home-worker'е, и будит fiber
кросс-потоково через `nova_sched_wake` (тот же dispatch-путь, что
driver-routed `Time.sleep` из [D228](#d228)). Legacy per-worker путь
сохранён для bootstrap/single-thread режима (`nova_driver_is_started()`
== false).

Cross-thread wake-before-park закрывается тем же механизмом, что у sleep:
(а) `park_until` fast-path проверяет done-predicate до yield; (б) перед
submit'ом job'а делается pre-init `nova_sched_get_state(scope)` — чтобы
wake с driver-потока, прилетевший до `register_pending`, не потерялся на
`find_state == NULL`; (в) `pending_wake[]`-counter (Plan 83.11 Ф.3.A).
`uv_cancel(&st->work)` thread-safe для work-request'ов на обоих loop'ах,
так что cancel-путь (`_nova_blocking_stop_cb`) работает без изменений.

> **✅ ЗАКРЫТО (Plan 173.0 Ф.1 / Plan 83.11, 2026-06-11).** Ранее существовавший
> pre-existing M:N race в общей park/wake машинерии (torn-чтение массивов
> `sched_state` в `nova_sched_grow_state` при росте scope, гонка с
> `nova_sched_wake` driver-потока) **структурно устранён**: park/wake storage
> (`NovaSchedState`) переведён на **chunked stable-address** директории
> (`fibers.h:398-445` — chunk=64, alloc ровно раз, RELEASE-publish +
> ACQUIRE-read, `capacity` публикуется ПОСЛЕДНЕЙ; grow = CAS-publish, не
> realloc), плюс single-winner park_state WAIT→DISPATCHED CAS (`fibers.h:1864`)
> и nested-cascade cancel-propagation (`fibers.h:893`) — torn-base физически
> невозможен. **supervised корректен под РЕАЛЬНЫМ M:N: `NOVA_MAXPROCS=1`
> БОЛЬШЕ НЕ ТРЕБУЕТСЯ** как race-guard (D14/D50/D75). Регресс-гарды:
> `nova_tests/concurrency/grow_vs_wake_explicit.nv`,
> `nova_tests/err173_0/supervised_drain_mn_guard.nv` (оба ARMED M:N, без
> `NOVA_MAXPROCS=1`/`NOVA_AUTOARM=0`). Трекер `[M-83.11-grow-vs-wake-race]` —
> CLOSED (simplifications.md); driver-routing blocking-offload корректен.

**`blocking { }` — примитив, не handler-эффект.** В отличие от `detach`
(`with Detach = SyncDetach`), `blocking { }` не диспетчеризуется через
handler: контекст-чувствительный codegen всегда либо offload'ит (в
fiber-контексте), либо выполняет тело inline (на main-потоке — нет
worker'а пинить). `Blocking` в сигнатуре — требование декларации, не
точка подмены.

##### V1 leaf-контракт (GC-safety)

`work_cb` исполняется на threadpool-потоке, который **не**
Boehm-GC-registered и **не** является fiber'ом. Поэтому **V1-контракт**:
тело `blocking { }` обязано быть **leaf** — FFI/syscall без

- GC-аллокации (`GC_malloc` с не-registered потока — UB),
- вызовов обратно в Nova-рантайм (нет fiber/event-loop-контекста),
- control-flow-escape наружу (`return`/`break`/`continue`,
  пересекающих границу `blocking { }`).

**Проверяется компилятором (Plan 83.3 Ф.6).** Тело `blocking { }`
type-check'ается как `nogc` + бан suspend-эффектов:
- alloc-вызовы (`[]T.new`, `HashMap.new`, `StringBuilder.new`,
  `str.from`, ...) внутри `blocking { }` → compile error;
- вызов функции/эффект-операции с эффектом `Net`/`Fs`/`Db`/`Time`
  внутри `blocking { }` → compile error (нужен event-loop, которого
  на threadpool-потоке нет).

Остаётся documented-риском, **не** enforced'ным: `nogc`-проверка —
консервативный whitelist (не ловит user-record-литералы); `throw`/`?`
(`Fail`-эффект) — `throw` делает `longjmp` без fail-frame на
threadpool-потоке. Спековый пример `blocking { c_read_file(path) }` с
`Fail[IoError]` под V1 безопасен только если FFI сигналит ошибку
возвратом (Result), а не Nova-`throw`.

Покрывает основной use-case — блокирующий FFI. **V2** (followup):
GC-регистрация threadpool-потоков (`GC_register_my_thread` once-per-
thread) разрешит произвольный Nova-код под `Blocking` (включая alloc и
`throw`); отложена — V1 достаточно для целевого паритета.

##### Cancellation

- **Не стартовавшая** `uv_work_t` отменяется `uv_cancel()` → fiber
  будится с cancel.
- **In-flight** блокирующая работа **не прерывается** — C-вызов
  непрозрачен и доводится до конца, результат отбрасывается, бросается
  cancel. Это **industry-standard**: Go не прерывает блокирующий
  cgo-вызов, tokio не отменяет running `spawn_blocking`. В обоих
  случаях `after_work_cb` отрабатывает → fiber гарантированно будится.
- Интеграция с `CancelToken` ([D75](#d75)) / supervised-cancel — через
  `stop_cb`, зарегистрированный в pending-таблице scope'а.

`Detach` и `Blocking` могут комбинироваться:
```nova
fn submit_log(event Event) Detach Blocking -> () =>
    detach {
        blocking {
            c_send_to_syslog(event)
        }
    }
```

#### 5. Никакого `await` / маркера на месте вызова

Подтверждение [D14 (REVISED)](#d14-fiber-runtime--невидимая-инфраструктура):
вызов suspend-функции из любой функции — обычный вызов, без
`.await`/`?async`/любого маркера. Suspension — ambient
([D62](04-effects.md#d62)), не type-fact. Точки suspend —
implementation detail (preemption после v1.0 делает их
несущественными).

### Почему

1. **Suspension как ambient (D62) упрощает ментальную модель.**
   Программист не выбирает между `Async` и `Par` — это деление
   искусственное и устранено. AI-friendly: suspension — runtime-факт,
   не type-факт. Гарантия non-suspension — через `realtime { }` блок
   (D64).

2. **`spawn` только в scope'е защищает от утечек fiber'ов.** Главная
   ошибка Go-style fire-and-forget — задачи, переживающие caller'а
   незаметно. Structured concurrency (Trio, Kotlin coroutines, Swift
   TaskGroup) — общепризнанный путь решения.

3. **`detach` как эффект делает long-lived задачи видимыми.** Если
   функция запускает что-то, переживающее её — это **видно в
   сигнатуре** (D10 «всё — эффект», AI-first). Без `Detach` в
   сигнатуре `detach { ... }` — ошибка компиляции, аналогично `throw`
   без `Fail[E]`.

4. **`Blocking` — явная модель Tokio.** Tokio (`spawn_blocking`)
   доказал, что явный примитив для блокирующих операций — рабочая
   модель. Альтернативы:
   - **Авто-детект (Go/Loom)** требует deep runtime hooks, сложен и
     хрупок.
   - **Без поддержки** превращает любой блокирующий syscall в
     bottleneck для всего scheduler'а.

5. **Отсутствие `await` — прецедент Erlang/Go/Java virtual threads.**
   Эти языки работают без маркера suspend много лет, на масштабе
   backend. Опыт показывает: маркер не даёт реального контроля
   (preemption всё равно вставляет suspend), но создаёт boilerplate.
   D14 уже зафиксировал это — D50 подтверждает.

6. **`spawn body` возвращает unit (а не результат body).** Async
   прозрачный (D62) делает синхронный результат от concurrent-вызова
   избыточным:
   - Если результат нужен sequentially → пиши прямой вызов
     `let users = fetch_users()`. Suspension случится сама собой,
     никакого `.await`/`.value()` не пишется.
   - Если нужна параллельность с гомогенным результатом →
     `parallel for ... { ... }` возвращает массив.
   - Гетерогенная параллельность → channels ([D79](#d79)) или
     `parallel { ... }` typed tuple (открытый
     [Q-parallel-tuple](../open-questions.md#q-parallel-tuple)).
     ⚠️ `mut`-захваты — race-prone в preemptive runtime, безопасны
     только в D71 single-threaded bootstrap; для production
     использовать channel или parallel-tuple.

   Альтернативы — implicit-await (= «цвет функции», D62 запрещает) или
   `Handle[T].value()` (= новый тип в системе, дополнительный
   boilerplate, типичный Rust-стиль). И то, и другое противоречит
   принципу D9 «один очевидный путь».

### Что отвергнуто

- **Раздельные `Async`/`Par`.** Искусственное разделение, AI-unfriendly,
  не даёт информации сверх «функция fan-out» (которая лучше через
  имя/док).
- **Fire-and-forget `spawn` свободно** (как Go). Утечки fiber'ов
  становятся систематическими, отмена не прорастает, supervision
  ломается.
- **`detach` без эффекта** (просто примитив). Скрывает важную
  информацию из сигнатуры — функция «что-то запускает в фоне» неотличима
  от обычной. Нарушает D10/D14.
- **Авто-детект блокирующих syscall'ов** (Go runtime hooks, Loom
  carrier-thread magic). Сложнее реализовать, хрупче на нестандартных
  C-библиотеках, прячет важное поведение от сигнатуры.
- **`await` / `?async` маркер на call site.** Не даёт реальных
  гарантий после введения preemption (v1.0+); добавляет boilerplate.
- **Отдельный supervisor для каждого detach.** Глобальный default
  supervisor (handler `Detach` = `LogAndDrop`) проще; явный supervisor
  ставится handler'ом в скоупе при необходимости.
- **`spawn body` возвращает результат body.** Изначальная редакция
  D50 / `syntax.md` подразумевала это (`let r = spawn { compute() }`).
  Отвергнуто: либо неявно блокирует caller'а до завершения spawn'а
  (тогда `supervised` теряет смысл — нет параллельности), либо
  требует implicit-await (= «цвет функции», нарушение D62), либо
  требует `Handle[T]` тип с blocking `.value()` (= boilerplate +
  новый тип в системе). Все три плохи. Async прозрачный (D62)
  делает синхронные значения от concurrent-вызова **избыточными** —
  если значение нужно, пиши прямой вызов. spawn — fire-and-forget
  statement; результаты через `#share`-примитивы (`Atomic*`/`Mutex`) /
  каналы ([D415](#d415-data-race-freedom--share-атрибут-capture-check-consume-в-spawn-plan-1733) — голый mut-захват отвергается) или `parallel for`
  (массив-результат).
- **`Handle[T]` / future-объект от spawn.** Aналог Rust `JoinHandle`
  или Kotlin `Deferred`. Отвергнуто: добавляет тип в систему,
  требует `.value()` синтаксиса (то же что implicit-await но явно
  в коде), не даёт ничего сверх `#share`-примитивов/каналов (D415).

### Связь

- [D14](#d14-fiber-runtime--невидимая-инфраструктура) — D50 конкретизирует
  D14 (suspension ambient, structured `spawn`, `detach`/`blocking` как
  отдельные примитивы с эффектами).
- [D2](04-effects.md#d2) / [D10](01-philosophy.md#d10) — `Detach`,
  `Blocking` — обычные эффекты в системе, handler-механизм работает
  одинаково.
- [D64](04-effects.md#d64) — `realtime { }` блок запрещает `Blocking`
  внутри (suspend на blocking-pool несовместим с гарантией
  не-приостановки).
- [D13](08-runtime.md#d13) — panic в detached-fiber'е = смерть fiber'а
  (как везде); глобальный supervisor логирует.
- [revolutionary.md R7](../revolutionary.md), [R9](../revolutionary.md)
  — structured primitives и supervision; D50 уточняет, что они —
  единственный способ запустить fiber внутри scope'а.

### Открытые вопросы

- `Channel[T]` API — формализован в [D79](#d79).
- `Mutex`/`Atomic` — stdlib (D167-D172 + D370 (ex-D370)), не prelude; owner-actor pattern
  предпочтителен, escape hatch через `import runtime.sync.{...}`.
- Размер blocking-pool по умолчанию — детали реализации runtime'а.
- Поведение при отмене detached-задачи — отдельный handler-сахар или
  работа через capability?

### Эволюция

D50 **active**. До его принятия D14 оставлял несколько вопросов
(Q12.1 spawn-семантика, Q12.2 Async vs Par, Q12.6 C interop) —
закрыты этим решением. Q12 в [open-questions](../open-questions.md)
сжимается до stdlib-API (переходит в Q9).

---

## D71. Bootstrap concurrency runtime

> **Status:** active. Конкретизирует [D14](#d14-fiber-runtime--невидимая-инфраструктура)
> и [D50](#d50-concurrency-model-spawn-detach-blocking) для bootstrap-компилятора:
> минимальная реализация `supervised`, `detach`, `parallel for`, `Time.sleep` —
> достаточно для тестов с реальным переключением корутин и pre-production-кода.
> Production-runtime будет надстройкой (preemption, timer-wheel, multi-thread,
> cancellation, error-propagation).

### Что

D71 фиксирует **минимальную, но spec-faithful** реализацию concurrency-примитивов
из D14/D50 в bootstrap-runtime'е:

1. **`supervised { body }` — round-robin scheduler над локальной очередью fiber'ов.**
2. **`spawn` имеет две семантики**, выбираемые контекстом:
   - **Внутри `supervised`** — кладётся в очередь scope'а, запускается scheduler'ом
     при выходе из scope.
   - **Вне `supervised`** — eager-blocking (запускается до завершения немедленно).
     Это **не** spec-compliant поведение D50 (по спеке должно быть compile error),
     но сохранено для bootstrap-совместимости. См. «Что упрощено».
3. **`detach { body }` — fire-and-forget.** Default-handler `SyncDetach` исполняет
   body inline (как обычный block). Эффект `Detach` в сигнатуре пока не требуется
   компилятором.
4. **`parallel for x in iter { body }` — D14 fan-out.** Десугарится в
   `supervised { for x in iter { spawn { body } } }`.
5. **`Time.sleep(ms)` — yield-point** с context-sensitive диспатчизацией.
6. **Capture-by-value для immutable scalars.** Без этого parallel for и любой
   spawn-в-цикле дают неправильную семантику (все queued fibers видят последнее
   значение loop-переменной).
7. **Heap-allocated ctx-struct в supervised.** Без этого N spawn'ов в одной
   итерации цикла разделяют один stack-slot.

### Правило

#### Тип результата `supervised` и `parallel for`

**`supervised { body }` — value-expression (Plan 173.1 Ф.1, 2026-07-09;
D414 §4):** возвращает своё trailing-выражение, вычисленное **ПОСЛЕ join'а
всех детей** (post-join). Void-форма (нет trailing) — по-прежнему unit.
Bootstrap-заглушка «возвращает unit, trailing отбрасывается `(void)`»
(2026-05-06) снята. `parallel for` = сахар над этой формой.

```nova
ro total = supervised {
    spawn { part_a() }
    spawn { part_b() }
    acc          // ← вычисляется после завершения ВСЕХ детей
}
```

Мутируемое состояние между детьми — не через голый mut-захват (с
[D415](#d415-data-race-freedom--share-атрибут-capture-check-consume-в-spawn-plan-1733)
это `[E_CONCURRENT_MUT_CAPTURE]`), а через `#share`-примитивы/каналы; результат
блока — trailing-выражение post-join.

**`parallel for x in iter { body }`** — по spec D14 это **expression**
типа `[]T` (где `T` — тип `body`). Это **параллельный map**, не loop.

```nova
ro responses []Response = parallel for url in urls { fetch(url) }
//                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
//                          параллельный map: 1 element → 1 response
```

**Codegen (Plan 173.1 Ф.2, 2026-07-09):** array-mode — канальный сбор для
ЛЮБОГО элемента `T` и ЛЮБОГО итератора (Iter-protocol, без `len()`):
клон `Sender`'а создаётся в родителе на момент `spawn` (refcount++ ДО
закрытия родительского tx) и move'ится в ребёнка; ребёнок `send`'ит
trailing-значение (int-скаляры — прямо в слот, heap-указатели — через
intptr, value-типы — boxed-копией) и закрывает клон на ЛЮБОМ выходе
(success/throw/cancel); родительский tx закрывается после enqueue-цикла;
выделенный drain-fiber внутри scope дренирует до `None` (= последний клон
закрыт) и пушит в результирующий `Vec[T]`. Буфер `K = min(len, 16)` —
back-pressure. **Порядок = completion order (плотный)** — упавший ребёнок
не шлёт, дыр нет; итерационный порядок НЕ гарантирован. Ошибка ребёнка →
отмена siblings + re-throw после дренажа (173.0) — накопленный массив НЕ
возвращается. Без trailing — statement-семантика (unit), включая
inline-threshold оптимизацию. См. `nova_tests/err173_1/` +
`nova_tests/concurrency/parallel_for_array.nv`.

#### `for` vs `parallel for` — разные семантики

**Обычный `for x in iter { body }` — это statement** (тип `unit`).
Тело выполняется ради side-effects:

```nova
for url in urls {
    Log.info(url)         // только side effect, ничего не собирается
}
// for сам — unit
```

Если нужен **sequential map** (собрать массив результатов
последовательно) — использовать `iter.map(|x| body)`:

```nova
ro names []str = users.map(|u| u.name)
// или с trailing-fn (для длинных тел):
ro names []str = users.map() fn(u) => u.name
```

**`parallel for` — expression** (тип `[]T`). Тело — функция от элемента
к результату:

```nova
ro responses []Response = parallel for url in urls { fetch(url) }
```

Сводная таблица:

| Форма | Тип | Семантика |
|---|---|---|
| `for x in iter { body }` | `unit` | statement, side-effects |
| `iter.map(\|x\| body)` | `[]T` | sequential map |
| `parallel for x in iter { body }` (body has trailing) | `[]T` | parallel map (fan-out) |
| `parallel for x in iter { body }` (no trailing) | `unit` | parallel side-effect loop |

Это **намеренное** различие — `for` для side-effects (большинство
случаев), `parallel for` для structured fan-out. Sequential map
выражается через method-chain, не через `for`-form, чтобы избежать
аллокации `[]unit` для side-effect-циклов и сохранить привычную
семантику `for` из Go/Rust/Java.

**Bootstrap-реализация (2026-05-06):** array-mode работает для
T ∈ {int, bool, f64, str} и для итераторов `a..b`, `a..=b`, array
literal. Pre-allocate `NovaArray_T*` размера N (`end - start [+1]`
для range, длина литерала для array), per-iteration ctx содержит
`_nova_par_idx` + `_nova_par_result`, spawn body's trailing
автоматически пишет в `result.data[idx]`. Если trailing отсутствует —
старая семантика (statement, unit). Spread в array literal не
поддержан в v1 — degrade to unit. См. `nova_tests/concurrency/parallel_for_array.nv`.

#### 1. `supervised { body }` — round-robin scope

```nova
supervised {
    spawn fiber_a()       // в очередь, не запускается
    spawn fiber_b()       // в очередь
    do_main_work()        // исполняется eager в текущем потоке
    spawn fiber_c()       // в очередь
}                         // ← scheduler крутит resume A, B, C по кругу
                          //   пока все не MCO_DEAD
```

Семантика:

- **Очередь scope'а** — локальная `NovaFiberQueue` с фиксированной capacity (64 в
  bootstrap). Превышение → runtime panic.
- **`spawn` в scope** — создаёт coroutine через `mco_create`, кладёт в очередь,
  **не делает resume**. Возвращает unit.
- **Scope-exit** — `nova_supervised_run` крутит цикл `do { step } while alive`,
  где `step` — один full pass очереди (resume каждый живой fiber один раз).
- **Тело `supervised`** исполняется eager в потоке вызвающего, **до** scheduler-runa.
  Yield-point на main-уровне (см. п. 5) даёт main-flow возможность переключиться
  с queued fiber'ами.
- **Captures** в spawn-body живут на стеке (по pointer) или копируются в ctx-struct
  (по value) — см. п. 6.

#### 2. `spawn` — две семантики по контексту

```nova
// (a) Внутри supervised — отложенный запуск
supervised {
    spawn { compute_a() }     // запустится при scope-exit (или раньше при yield)
    spawn { compute_b() }
}

// (b) Вне supervised — eager (legacy bootstrap-семантика)
ro r = spawn { compute_x() }    // запускается СРАЗУ до завершения,
                                  // r получает результат
```

В bootstrap'е разрешены оба варианта. По спеке D50 (b) должен быть compile error.
Закрытие этого расхождения — после миграции существующих тестов на `supervised`.

##### Тип результата `spawn`

**`spawn body` возвращает unit, всегда** (resolution от 2026-05-06).
Обоснование — D50 «Почему» п. 6: async прозрачный (D62) делает
синхронный результат от concurrent-вызова избыточным, альтернативы
(implicit-await, `Handle[T]`) хуже по AI-friendliness.

Идиомы получения значений от concurrent-выполнения:

```nova
// (1) Прямой вызов — async прозрачный (D62).
ro users = fetch_users()       // тип []User; suspension случается сама

// (2) parallel for — массив гомогенных результатов.
ro responses = parallel for url in urls { fetch(url) }   // []Response

// (3) #share-примитивы — гетерогенная параллельность (D415;
//     голый mut-захват = E_CONCURRENT_MUT_CAPTURE).
mut a = AtomicInt.new(0)
mut b = AtomicInt.new(0)
supervised {
    spawn { a.store(compute_a()) }
    spawn { b.store(compute_b()) }
}
use_both(a.load(), b.load())
```

**Bootstrap-исключение (legacy).** `spawn` вне `supervised` сейчас
работает в eager-blocking семантике (см. п. 2). Для совместимости с
существующими тестами до их миграции на `supervised` — `let r = spawn
{ body }` вне scope временно возвращает значение body (через
type-erased `nova_int` в ctx-поле `_nova_result`). Это **не
spec-faithful**, удалится вместе с переходом «`spawn` вне scope =
compile error».

После закрытия legacy-расхождения:
- `spawn body` всегда unit, во всех контекстах.
- Поле `_nova_result` в ctx-struct убирается.
- Все обращения к результату concurrent-вызова — через прямой вызов /
  `parallel for` / `#share`-примитивы и каналы (D415).

#### 3. `detach { body }` — fire-and-forget с default handler

```nova
fn handle_request(req Request) Net Db Detach -> Response {
    ro resp = process(req)
    detach { write_audit(req, resp) }
    resp
}
```

Default-handler `SyncDetach`: тело исполняется **inline** в потоке caller'а —
никакого fiber'а, никакого scheduler'а. Семантически валидно для тестов
(spec D50 явно описывает `with Detach = SyncDetach { ... }` как тестовый default,
bootstrap-default = это).

В bootstrap'е:
- ⚠ **UPDATED (Plan 173 Ф.3 п.2, [D414 §2](#d414-structured-error-propagation--primary-selection-precedence-detach-policy-channel-closed-vs-value-plan-173-ф3)):**
  требование эффекта `Detach` **теперь enforced** в checker'е —
  `detach` без `Detach` в сигнатуре → `[E_DETACH_REQUIRES_EFFECT]` (exempt:
  test-root / ambient `with Detach` / handler-op-тело). `Detach` остаётся
  зарезервированным builtin-именем (не самостоятельный effect-type-декл в std).
- Глобальный supervisor (для реального async-execution на отдельном OS-thread'е)
  — отложен до production-runtime.
- Panic-containment (`LogAndDrop`) — дефолт-политика ([D414 §2](#d414-structured-error-propagation--primary-selection-precedence-detach-policy-channel-closed-vs-value-plan-173-ф3)).

#### 4. `parallel for x in iter { body }` — fan-out

```nova
fn fetch_all(urls []str) Net Fail -> []Response =>
    parallel for url in urls {
        fetch(url)
    }
```

Семантически идентично `supervised { for x in iter { spawn { body } } }`.
Codegen строит этот AST синтетически и эмитит через общий путь.

Loop-переменная — **immutable scalar** (для range — всегда `int`; для array —
тип элемента). Captures её **по value** (см. п. 6), что обеспечивает корректный
snapshot в каждой итерации.

#### 5. `Time.sleep(ms)` — context-sensitive yield-point

```nova
fn anywhere() {
    Time.sleep(0)         // вне scope: no-op
}

supervised {
    spawn { ... }
    Time.sleep(0)         // в scope-body: один pass очереди
                          // (main-flow yield'ает queued fibers'ам)
    spawn {
        Time.sleep(0)     // в fiber: nova_fiber_yield()
                          // — corutine суспендится, scheduler крутит других
    }
}
```

В bootstrap'е **`ms` учитывается через monotonic wall-clock** (2026-05-06).
`sleep(0)` даёт один yield (compatibility с устоявшимся `Time.sleep(0)`
идиомом). `sleep(N>0)` ждёт реально N миллисекунд:

| Контекст вызова | Поведение для ms<=0 | Поведение для ms>0 |
|---|---|---|
| Внутри fiber-body (spawn) | `nova_fiber_yield()` (один yield) | yield-loop пока `_nova_monotonic_ms() < deadline`; каждый yield проверяет cancel |
| Вне fiber, внутри `supervised` body | `nova_supervised_step(&queue)` (drain один раз) | drain queue per pass пока `< deadline` |
| Полностью вне любого scope | no-op | native OS sleep (`Sleep` на Win, `nanosleep` на POSIX) |

`Time.now()` возвращает monotonic ms (`GetTickCount64` на Win,
`clock_gettime(CLOCK_MONOTONIC)` на POSIX). Эпоха unspecified —
тесты должны сравнивать только разности, не абсолютные значения.

Это **spec-faithful по D62** (Async — ambient): `Time.sleep` — обычная функция
без эффект-окраски, callable откуда угодно. Поведение зависит от ambient
runtime-окружения в точке вызова.

**Чем bootstrap отличается от production-timer-wheel:** bootstrap делает
busy-yield-loop с проверкой clock'а — fiber, ожидающий 100ms, всё это
время съедает CPU yield-проверками. Production-runtime поставит fiber в
sleep-list с deadline и scheduler пропустит sleeping fibers до их
пробуждения (нулевой CPU между yield'ами). Поведение из Nova-кода
неотличимо; это чисто оптимизация.

#### 6. Capture-by-value для immutable scalars

При запуске spawn внутри `supervised`, его захваты переменных делятся на:

- **By value** — переменная объявлена как `let` (не `let mut`) И тип scalar
  (`int`, `bool`, `f64`, `f32`, `u8`). Значение **копируется** в ctx-struct
  как `T name` — fiber видит snapshot на момент spawn'а.
- **By pointer** — переменная mutable (`let mut`) или non-scalar (record, array,
  string). В ctx-struct хранится `T* name`, fiber разделяет состояние с
  caller'ом и другими fiber'ами.

**Зачем это:** очередь supervised держит fiber'ы до scope-exit. Если бы все
captures были by-pointer, loop-переменные (после for'а указывающие на последний
элемент) видели бы все queued fibers как «последний элемент» — `parallel for x in
[1,2,3] { sum += x }` дал бы 9, не 6. By-value snapshot этого избегает.

**Mutable shared state работает как ожидается:** `let mut acc = 0; spawn { acc +=
x }` — `acc` остаётся by-pointer (mutable), все fiber'ы пишут в одну ячейку.

#### 7. Heap-allocated ctx-struct в supervised

ctx-struct для spawn внутри supervised аллоцируется через `nova_alloc` (не на
стеке). Без этого N итераций цикла перезаписали бы один stack-slot, и все
queued fibers видели бы только последнее значение captures. Stack-allocation
сохраняется для eager-blocking spawn вне scope.

### Почему

1. **Минимальный delta vs full D50.** D14/D50 определяют большой набор
   примитивов (`spawn`/`detach`/`parallel for`/`race`/`select`/`cancel_scope`/
   `with_timeout`/`blocking`). Без preemption и scheduler-thread'а реализуемы
   только cooperative-варианты — они и реализованы. Остальное — production.

2. **Spec-faithful по D62.** Async ambient → `Time.sleep` callable откуда угодно
   и не требует эффекта в сигнатуре. Context-sensitive диспатчизация в bootstrap
   — естественное следствие: где scheduler есть — yield, где нет — no-op.

3. **Capture-by-value для immutable closes a real correctness hole.** Без этого
   `parallel for` + любой spawn-в-цикле дают неправильную семантику. Это **не**
   опциональная оптимизация, а необходимость для базовой корректности.

4. **Heap-ctx — единственный способ дать каждой итерации независимый snapshot**
   при отложенном запуске. Альтернативы (stack-allocated array of ctx) сложнее
   и не лучше по производительности (всё равно нужно держать N структур до
   scope-exit).

5. **Eager-blocking `spawn` вне scope — bootstrap legacy.** Существующие тесты
   `38_deep_spawn.nv` (top section) рассчитывают на эту семантику. Перевод на
   strict-spec (compile error без supervised) требует одновременной миграции
   всех тестов — отдельная задача.

### Что отвергнуто

- **`spawn` всегда eager-blocking** (включая внутри supervised). Это убирает
  весь смысл `supervised` — нет очереди, нет round-robin, нет interleave.
  Отвергнуто.
- **`spawn` всегда deferred-into-queue** (включая вне scope). Ломает 28 legacy-
  тестов. Отвергнуто до миграции.
- **Implicit fiber-wrap для тела `supervised`.** Альтернатива main-yield: само
  тело scope'а становится первым fiber'ом в очереди. Семантически корректнее
  (главный flow тоже full participant), но требует переноса всех локальных
  переменных body в ctx-struct, что усложняет capture-семантику для других
  spawn'ов в том же scope. Отвергнуто в пользу простого
  `nova_supervised_step` для main-yield.
- **`#define cap (*_c->cap)` macro для capture access.** Использовалось до
  2026-05-06. Ломалось при nested supervised/spawn: имя `cap` рекурсивно
  расширялось в struct field-declarators (`nova_int* order;` → garbage).
  Заменено на inline rewrite в `ExprKind::Ident`.
- **Stack-allocated ctx внутри supervised.** Один slot шарится между
  итерациями цикла → bug. Heap-alloc обязателен.
- **`yield` keyword.** Альтернатива `Time.sleep(0)`. Отвергнут: D62 говорит
  «suspension — runtime, не type/syntax-level», keyword подсветил бы то что
  спека прячет. `Time.sleep` — обычная функция, валидная спецификационно.

### Открытые вопросы

- **Когда переключить `spawn` вне scope на compile error?** После миграции
  `38_deep_spawn.nv` верхней части на `supervised`-обёртки. Затрагивает 28
  существующих тестов.
- **`detach` через OS-thread в bootstrap?** Сейчас SyncDetach. Реальный
  background требует pthread/Win32-интеграции — большая работа, отложена.
- **Эффект `Detach` в effect-system.** Объявление + compile-time проверка
  требования в сигнатуре. Сейчас не выполняется.
- **Удалить eager-blocking `spawn` вне scope.** Закрыт спор о типе
  результата (`spawn` всегда unit), но bootstrap всё ещё разрешает
  legacy-семантику `let r = spawn ...` вне `supervised`. Удаляется
  одновременно с переходом «spawn вне scope = compile error» — после
  миграции 28 legacy-тестов в `38_deep_spawn.nv` верхней части.
- **Эффект `Time` в effect-system — РЕАЛИЗОВАН** (2026-05-06).
  По D11/D31/D62: pre-registered как built-in effect (`sleep(int)`,
  `now() -> int`); `Time.sleep`/`Time.now` идут через стандартный
  effect-dispatch путь (Nova_Time_sleep / Nova_Time_now).
  **Real wall-clock реализован (2026-05-06):** `Time.now()` возвращает
  monotonic ms (GetTickCount64 на Win, clock_gettime на POSIX);
  `Time.sleep(ms>0)` ждёт реально через yield-loop с deadline в fiber/
  scope-context'е, native OS sleep на top-level. `Time.sleep(0)` — один
  yield (compatibility-режим). User override через `with Time = effect
  Time { ... } { body }` — работает (тесты `46_time_handler.nv`).
  Что НЕ закрыто: production-timer-wheel (sleeping fiber'ы съедают CPU
  yield-проверками — бизнес-логика этого не видит, это оптимизация).
- **Cooperative cancellation propagation реализована** (2026-05-06):
  fiber-throw → scope `cancel_requested = true` → остальные fiber'ы
  при следующем yield (`Time.sleep` или scheduler step) делают
  `nova_throw("scope cancelled")`. Это spec-faithful по D50.
  Что НЕ работает: fiber без yield-точек не отменится (cooperative-only).
  Preemption — в production runtime (timer-based safepoint check).
- **Positive-тесты на throw из fiber.** Без top-level `try/catch` (D25)
  невозможно protected-call. `throw` из fiber → rethrow на main → abort
  работает корректно, но не testable как PASS.
- **`race`, `select`, `cancel_scope`, `with_timeout`.** Каждый — отдельная
  задача после cancellation propagation.
- **Channels (`Channel[T]`).** Формализованы в [D79](#d79) (2026-05-07).
  В D71 bootstrap-runtime реализация — следующая задача (single-
  threaded queue + yield). До тех пор producer-consumer тестируется
  через shared mut + yields (валидно только в D71 single-threaded).

### Связь

- [D14](#d14-fiber-runtime--невидимая-инфраструктура) — fiber-runtime как
  ambient capability. D71 даёт минимальный конкретный runtime.
- [D50](#d50-concurrency-model-spawn-detach-blocking) — language-level
  модель concurrency. D71 — её первая bootstrap-реализация.
- [D62](04-effects.md#d62) — Async ambient. Объясняет почему `Time.sleep`
  не требует эффекта в сигнатуре.
- [D64](04-effects.md#d64) — `realtime { }` запрещает suspension. По D71
  `Time.sleep` внутри realtime-блока должен давать compile error
  (compile-time check эффекта `Time` в сигнатуре). Не реализовано в
  bootstrap'е.

### Реализация

bootstrap-codegen (`compiler-codegen/`):

- `nova_rt/fibers.h`: `NovaFiberQueue`, `nova_supervised_step`,
  `nova_supervised_run`, `nova_fiber_yield`, `nova_fiber_spawn_into`.
- `src/codegen/emit_c.rs`: `emit_supervised`, `emit_detach`,
  `emit_parallel_for`, `emit_spawn` (with by-value/heap-ctx logic),
  context-sensitive `Time.sleep` dispatch.
- `src/lexer/`, `src/ast/`, `src/parser/`: keywords `supervised`, `parallel`,
  `detach`; AST variants `Supervised`, `Detach`, `ParallelFor`.
- Тесты: `nova_tests/concurrency/deep_spawn.nv` (section 10, 9 interleave-
  тестов), `detach_test.nv` (13), `parallel_for.nv` (12), `main_yield.nv`
  (11). Полный suite в `nova_tests/concurrency/`.

### Эволюция

- **2026-05-06:** D71 introduced — bootstrap busy-yield + cooperative
  cancellation через `nova_fiber_yield` re-check.
- **Plan 22 Ф.4 (2026-05-11):** scheduler становится libuv-event-loop
  driven. `Time.sleep` через park-on-`uv_timer_t` (см.
  [D93](#d93-park-wake--нормативный-runtime-primitive-для-блокирующих-операций))
  — CPU idle на sleep period вместо busy-yield. `nova_supervised_run`
  idle через `uv_run UV_RUN_ONCE` когда все живые fiber'ы parked.
- **Plan 22 Ф.5 (2026-05-11):** top-level main оборачивается в implicit
  supervised scope (см. [D92](#d92-top-level-main-как-implicit-supervised-scope))
  — `_nova_active_scope` всегда non-NULL в user-code.
- **Plan 22 Ф.6 (2026-05-11):** park/wake state production-grade lazy
  pointer-в-`NovaFiberQueue` (Вариант B) — O(1) lookup, нет cap'а на
  nested scopes, память выделяется только когда реально park'аем.
- **Plan 44 (M:N, milestone v1.0+):** scheduler становится work-stealing
  per-worker, park/wake API D93 расширяется на cross-worker wake через
  `uv_async_t`.

---

## D75. `supervised(cancel: tok)` — структурная отмена с внешним токеном

> ⚠️ **REVISED (2026-05-14).** Раньше D75 вводил отдельный keyword
> `cancel_scope { tok => body }`. Он **удалён**. Внешняя отмена теперь
> выражается именованным аргументом `cancel:` у `supervised`
> ([D102](03-syntax.md#d102-именованные-аргументы-и-значения-параметров-по-умолчанию)):
> `supervised(cancel: tok) { body }`. Никакого нового keyword'а, никакого
> scope-introduced `tok =>` binding (которого больше нет нигде в языке,
> ср. отмену `f(args) { x => body }` в [D43](03-syntax.md#d43)).
>
> Bootstrap-реализация старого keyword'а (`cancel_scope`, 2026-05-06)
> остаётся в дереве до миграции — см. [Plan 47](../../docs/plans/47-supervised-cancel.md).
> Старый текст D75 — в [history/](history/).

### Что

`supervised(cancel: tok) { body }` — обычный `supervised`-scope
([D50](#d50-concurrency-model-spawn-detach-blocking)), которому
**снаружи** можно сообщить «отмени всех fiber'ов внутри». Связь идёт
через `tok` типа `CancelToken` — **caller-owned** значение: создаётся
вызывающим кодом, передаётся в `supervised` именованным аргументом,
переживает scope.

```nova
fn fetch_all(urls []str, cancel CancelToken) -> []Response {
    mut results []Response = []
    supervised(cancel: cancel) {
        for url in urls {
            spawn { results.push(fetch(url)) }
        }
    }
    results
}

// caller-side:
ro tok = CancelToken.new()
spawn {
    Time.sleep(5_000)
    tok.cancel()        // через 5s валим scope извне
}
fetch_all(urls, tok)
```

`supervised` **без** `cancel:` — закрытый scope, извне не отменяемый
(только panic изнутри). `supervised(cancel:)` — escape hatch для
kill-switch'а (timeout-обёртка, user cancel button, fail-fast по
внешнему сигналу). Наличие `cancel:` делает код самодокументирующимся:
видно `cancel:` — scope намеренно отменяемый.

### Модель токена

`CancelToken` — это **caller-owned handle для запроса отмены**, не
scope-binding. Создаётся `CancelToken.new()`, живёт сколько нужно
вызывающему, может быть захвачен в замыкание / передан аргументом /
положен в канал. Все держатели ссылки работают с одним объектом.

**Capabilities:**

- `tok.cancel()` — запросить отмену. Если токен привязан к живому
  scope'у — все fiber'ы scope'а на следующем yield-point'е бросят
  `"scope cancelled"` (механизм `cancel_requested` из D71).
  Если токен **не привязан** или scope **уже завершён** — **no-op**
  (безвредно). Idempotent.
- `tok.is_cancelled() -> bool` — чтение флага без yield. Не throws.
- `child.cancelled_by(parent CancelToken)` — **направленный** каскад:
  `parent.cancel()` отменяет и `child`; обратно НЕ течёт
  (`child.cancel()` не трогает `parent`). Композиция в более широкий
  родительский kill-switch — как дерево `context` в Go. Имя несёт
  направление: «child отменяется по parent».

### Правило bind-check — один токен, один живой scope

`supervised(cancel: tok)` при входе **привязывает** `tok` к scope'у;
при выходе — **отвязывает**.

**Один `CancelToken` нельзя привязать к двум живым scope'ам
одновременно.** Повторный `supervised(cancel: tok)` с уже-привязанным
токеном — **ошибка** (runtime panic «token already bound to a live
scope»).

```nova
ro tok = CancelToken.new()
supervised(cancel: tok) {
    supervised(cancel: tok) { ... }   // ОШИБКА: tok уже привязан
}
```

Почему это безопасно ограничивать: делиться токеном «вниз» по
вложенности **не нужно**. Если внешний scope отменяется — его файбер
стоит на yield-point'е *внутри* вложенного scope'а, поэтому вложенный
рвётся автоматически как часть structured-отмены. Нужен независимо
отменяемый внутренний scope — заводится **новый** токен.

После выхода из scope'а токен отвязан и может быть привязан заново
(или, для простоты реализации, токены — single-use; решается в
[Plan 47](../../docs/plans/47-supervised-cancel.md)).

### Почему runtime-check, а не compile-time

Compile-time enforcement «токен не привязан дважды» потребовал бы
affine/linear-типов с borrow-различием (`&tok` для bind, `&tok` для
`cancel()`, borrow-checker следит за непересечением) — это Rust
borrow checker. В Nova его нет (GC + эффекты); тащить affine-типы
ради одной фичи несоразмерно.

Escape токена за пределы scope'а **не опасен**: `tok.cancel()` на
завершённом scope'е — no-op by design (в отличие от scope-handle,
через который можно `spawn` в мёртвый scope — вот это был бы UB).
Поэтому защищать надо только aliasing (double-bind), а он ловится
дёшево одним сравнением поля в `bind()` и проявляется на первом же
прогоне теста.

### Отличие форм `supervised`

| | `supervised { body }` | `supervised(cancel: tok) { body }` |
|---|---|---|
| Wait для всех fiber'ов | да | да |
| Cancel изнутри (через throw) | да | да |
| **Cancel снаружи** (`tok.cancel()`) | **нет** | **да** |
| Token-binding (родительский kill-switch) | нет | да (через `child.cancelled_by(parent)`) |

`supervised` остаётся **keyword'ом** — это неустранимая магия, точка,
куда `spawn` регистрирует fiber'ы (D14/D50; `spawn` — тоже
keyword-исключение в [D43](03-syntax.md#d43)). `cancel:` — обычный
именованный аргумент keyword-конструкции; новых keyword'ов D75 больше
не вводит.

### Семантика отмены

1. **Ручная отмена изнутри scope'а** (`tok.cancel()` в spawn-body) —
   допустима. Остальные spawn'ы в том же scope'е получают cancel-сигнал
   на следующем yield. Так реализован stdlib `race` (победитель
   отменяет проигравших).
2. **Auto-уборка fiber'ов:** на выходе из `supervised(cancel:) { ... }`
   гарантируется, что все spawn'ы завершились — сработала отмена или
   нет (как в обычном `supervised`).
3. **Throw + cancel:** `throw` внутри scope'а сначала ставит
   `cancel_requested = true`, потом re-throw'ит на main flow. Token
   остаётся cancelled.

### `race` / `with_timeout` — stdlib, не keyword'ы

`race` и `with_timeout` — **обычные функции стандартной библиотеки**,
построенные на `supervised(cancel:)` + `spawn` + `Channel` +
`ChanReader.close_after(Duration)` (Plan 65 revision),
вызываются через trailing-форму [D43](03-syntax.md#d43):

```nova
export fn within[T](dur Duration, body fn() -> T) -> T | Cancelled {
    ro tok = CancelToken.new()
    spawn { Time.sleep(dur); tok.cancel() }
    supervised(cancel: tok) { body() }
}

export fn race[T](competitors []fn() -> T) -> T {
    ro ch  = Channel[T].new(capacity: competitors.len())
    ro tok = CancelToken.new()
    supervised(cancel: tok) {
        for comp in competitors {
            ro c = comp
            spawn { ch.send(c()); tok.cancel() }   // self-cancel изнутри
        }
    }
    ch.recv()!!
}
```

### Что отвергнуто

- **Keyword `cancel_scope`** (старый D75) — отдельный keyword ради
  `supervised` + токен. Схлопывается в именованный аргумент `cancel:`
  без потери выразительности.
- **`cancel_scope { tok => body }` scope-introduced binding** —
  `tok =>` не существует больше нигде в языке (ср. отмену
  `f(args) { x => body }` в [D43](03-syntax.md#d43)). Один pattern
  вместо edge-case'а.
- **Compile-time token-scope enforcement** (affine/linear-типы) —
  несоразмерно; см. «Почему runtime-check».
- **Передача `tok` через channel** (Go `ctx.Done()`-стиль) — в Nova
  явный `bind`: композиция compile-time видима, без аллокации канала.
- **Auto-cancel через Drop** — Nova не имеет Drop. Cancellation —
  явная операция через `cancel()`, не побочный эффект scope-exit.

### Связь

- [D102](03-syntax.md#d102-именованные-аргументы-и-значения-параметров-по-умолчанию)
  — именованные аргументы; `cancel:` — обычный именованный аргумент.
  **Ревизия D75 зависит от D102.**
- [D43](03-syntax.md#d43) — trailing-форма для stdlib `race`/`within`.
- [D14](#d14-fiber-runtime--невидимая-инфраструктура) — fiber-runtime.
- [D50](#d50-concurrency-model-spawn-detach-blocking) — concurrency model.
- [D71](#d71-bootstrap-concurrency-runtime) — `cancel_requested` flag,
  cooperative cancellation propagation. D75 надстраивается над ним.
- [D93](#d93-park-wake--нормативный-runtime-primitive-для-блокирующих-операций)
  — park/wake API. `cancel()` через `nova_sched_cancel_all_pending`
  пробуждает parked-fiber'ов **immediate** через generic stop_cb
  mechanism (Plan 22 Ф.4).
- [Plan 47](../../docs/plans/47-supervised-cancel.md) — реализация:
  миграция bootstrap'а с keyword `cancel_scope` на `supervised(cancel:)`.

### История: keyword `cancel_scope` (2026-05-06) → удалён в Plan 47 (2026-05-14)

> ✅ Keyword `cancel_scope` **удалён** в [Plan 47](../../docs/plans/47-supervised-cancel.md).
> Раздел сохранён как контекст миграции.

Старая реализация — отдельный keyword `cancel_scope { tok => body }`:
лексер `KwCancelScope`, AST `CancelScope { token_name, body }`, парсер
`parse_cancel_scope`, codegen `emit_cancel_scope`, `NovaCancelToken` со
scope-owned моделью (токен хранил указатель на queue-frame).

**Plan 47 (2026-05-14) заменил это на `supervised(cancel: tok)`:**

- AST: `Supervised { body, cancel: Option<Expr> }`; вариант `CancelScope`
  удалён. Лексер/парсер/`emit_cancel_scope` — удалены.
- `NovaCancelToken` переписан на **caller-owned** модель: поля
  `cancel_requested` + `bound_scope` (nullable) + динамический `linked[]`.
  API: `nova_cancel_token_new` / `_bind` (scope-binding, panic при
  double-bind) / `_unbind` / `_cancel` / `_is_cancelled` / `_bind_cascade`
  (бывший `_bind` — каскад токенов).
- `emit_supervised` для cancel-формы: `nova_cancel_token_bind` перед
  `nova_supervised_run_cancel` (после тела — прямой throw в body-стейтменте
  не оставит dangling `bound_scope`); `unbind` — внутри
  `nova_supervised_run_cancel` на всех путях выхода (нормальный возврат +
  re-throw).

**Что caller-owned модель исправила:**

- **Dangling token.** Старый токен хранил указатель на queue-frame и после
  scope-exit'а становился dangling. Новый — caller-owned, `unbind` чистит
  `bound_scope`, `cancel()` на отвязанном токене — безвредный no-op, токен
  переживает scope.
- **`NOVA_CANCEL_LINKED_CAP=8`** — фиксированный массив каскадов заменён на
  динамический `linked[]` (GC-managed, геометрический рост).

**Унаследованное ограничение (вне scope Plan 47, см. §«Что НЕ входит»
плана):** cancel-throw на main flow приходит как plain `nova_throw`, не
через `Nova_Fail_fail`/handler-vtable. Корректный фикс требует различать
fiber-throw-from-handler vs cooperative-cancel-throw — отдельная задача.
Из-за этого stdlib `within`/`race` (Ф.5) пришлось бы оборачивать в
`with Fail[any]` handler с конфляцией реальных ошибок и timeout'а
(`[M-within-error-conflation]`) — Ф.5 отложена, см. план.

---

## D79. Channels — coordination между fiber'ами

> ⚠️ **Частично уточнено [D91](#d91)** (2026-05-10):
> Модель API изменена с Go-style (один `Channel[T]` объект с
> `send`/`recv`) на Rust mpsc-style (`(tx, rx) = Channel[T].new()` —
> capability-split на `Sender[T]` и `Receiver[T]`). Это **breaking
> change** API. Остальное в D79 (capacity-bounded buffer, owner-actor
> pattern, отказ от Mutex/Atomic, `select` через channels) остаётся.
> Старая формулировка ниже сохранена для исторического контекста.

### Что
`Channel[T]` — типизированный канал для передачи значений между
fiber'ами с blocking-семантикой. **First-class value** (не effect),
обеспечивает safe-by-default взаимодействие в concurrent коде.

`select { ... }` — мультиплексирование recv-операций по нескольким
каналам с опциональным `timeout` case. Был упомянут в D14/D50 как
пример без формальной декларации; D79 закрывает эту дыру.

Channels — **единственный safe способ** разделять данные между
fiber'ами в production-runtime (D14 с preemption). Альтернатива —
shared `mut` через захваты — ⚠️ undefined behavior в preemptive
runtime, разрешён только в D71 single-threaded bootstrap.

### Правило

#### Тип Channel[T]

```nova
type Channel[T] { ... }    // opaque в spec; реализация в runtime

fn Channel[T].new(capacity int) -> Channel[T]
//   capacity = 0   — unbuffered (rendezvous, send блокирует пока recv не пришёл)
//   capacity = N>0 — bounded buffer, send блокирует когда полон
```

`Channel[T]` — обычный value-тип. Передаётся между fiber'ами
**через capture в spawn-body** или **как параметр функции**. Это
single canonical pattern; никаких глобальных channel-handler'ов не
нужно (channel сам по себе — handle-объект).

#### Operations

```nova
fn Channel[T] @send(v T) -> ()              // блокирует если буфер полон
fn Channel[T] @recv() -> Option[T]          // None ⇔ closed и буфер пуст
fn Channel[T] @try_send(v T) -> bool        // true если послал, false если полон
fn Channel[T] @try_recv() -> Option[T]      // None если пусто (вне closed-семантики)
fn Channel[T] @close() -> ()                // idempotent
fn Channel[T] @is_closed() -> bool
fn Channel[T] @len() -> int                  // текущий размер буфера
fn Channel[T] @capacity() -> int             // фиксированный, из new()
```

**Семантика closed-channel:**

| Operation | Closed + buffer empty | Closed + buffer non-empty |
|---|---|---|
| `send(v)` | `false` (Plan 30 Ф.1) | `false` |
| `try_send(v)` | `false` | `false` |
| `recv()` | `None` | `Some(item)` — дренаж |
| `try_recv()` | `None` | `Some(item)` — дренаж |

**`send` на closed channel возвращает `false`, не panic** (Plan 30 Ф.1, D91).
Caller сам решает что делать с `false`. Это recoverable — не programming error.
Закрытый канал — нормальное runtime состояние (producer закрыл, pipeline
продолжает).

**`recv` после close**: дренаж буфера, потом None. Receivers могут
безопасно итерировать `while let Some(v) = ch.recv() { ... }` без
явной проверки is_closed.

#### Suspension и signature

Send/recv блокируют → требуют suspension. По D62 suspension — ambient
runtime mechanic, **не effect**. Сигнатура чистая:

```nova
fn process(ch Channel[Request]) Db -> () {
    while Some(req) = ch.recv() {
        Db.exec(req.sql)
    }
}
```

В сигнатуре только бизнес-эффекты (`Db`), никакого `Async`. Suspension
неявная.

#### `select { ... }` — мультиплексирование

> ⚠️ **УСТАРЕВШИЙ синтаксис** (`msg <- ch`, `timeout(expr) =>`) заменён D94.
> Финальный синтаксис — см. [D94](#d94-select--multiplexed-channel-operations).

```nova
// D94 финальный синтаксис (Plan 65 revision):
ro timeout = ChanReader.close_after(Duration.from_secs_f64(5.0))
select {
    Some(msg) = rx_a.recv() => { process_a(msg) }
    Some(msg) = rx_b.recv() => { process_b(msg) }
    None      = timeout.recv() => { default_action() }
}
```

**Семантика:**

1. Проверяет каждый arm в псевдослучайном порядке (Fisher-Yates). Если
   ≥1 готов немедленно — выполняет первый найденный без park'а.
2. Иначе — паркует fiber, регистрирует waiter для каждого arm. Первый
   готовый будит fiber; остальные waiters unlinked.
3. Если **несколько** готовы одновременно — выбор **non-deterministic**.
   Программист **не должен** полагаться на конкретный порядок.
4. **Closed channel** → `rx.recv()` возвращает `None` (после дренажа буфера);
   arm считается ready. **`None = rx`-паттерн различает closed от value**
   (реализовано Plan 173 Ф.3 п.3, [D414 §3](#d414-structured-error-propagation--primary-selection-precedence-detach-policy-channel-closed-vs-value-plan-173-ф3)):
   `Some(v) = rx` fires только на значение; `None = rx` — только на
   closed+empty; `_ = rx` — на любой результат (value ИЛИ closed).
5. **Без default** и все каналы закрыты (нет `None`/`_`-арма их ловящего) —
   panic "select: all channels closed".

Timeout — через `ChanReader.close_after(Duration)` возвращающий
`ChanReader[()]` (обычный recv arm, никакого специального синтаксиса).

#### Канонические patterns

**Producer/consumer:**

```nova
fn pipeline(input ChanReader[Request]) Db -> () {
    ro (processed_tx, processed_rx) = Channel.new(100)

    spawn {
        while Some(req) = input.recv() {
            ro resp = process(req)
            processed_tx.send(resp)
        }
        processed_tx.close()
    }

    spawn {
        while Some(resp) = processed_rx.recv() {
            Db.exec(resp.persist_sql)
        }
    }
}
```

**Fan-out:**

```nova
ro (work_tx, work_rx) = Channel.new(0)
for i in 0..10 {
    ro rx = work_rx   // capture by value
    spawn {
        while Some(task) = rx.recv() {
            task.run()
        }
    }
}
for t in tasks {
    work_tx.send(t)
}
work_tx.close()
```

**Worker pool с graceful shutdown (D94 select):**

```nova
ro (work_tx, work_rx) = Channel.new(0)
ro (stop_tx, stop_rx) = Channel.new(1)

spawn {
    select {
        Some(task) = work_rx.recv() => { task.run() }
        Some(_)    = stop_rx.recv() => { return () }
    }
}
```

#### Bootstrap-семантика (D71)

В D71 bootstrap-runtime (single-threaded cooperative):
- `send` на полный буфер — yield, продолжается когда recv освобождает место
- `recv` на пустой — yield, продолжается когда send добавит
- Memory ordering тривиальна (single thread)
- Fisher-Yates shuffle между select-armами (псевдослучайный, LCG)

В production-runtime (D14 future):
- Memory-barriers / atomic counters для buffer indexes
- Wait queues для blocked senders/receivers
- Channel — единственный гарантированно-safe primitive

#### `runtime.sync` — stdlib, не prelude (D167-D172 + D370 (ex-D370))

Channel — **предпочтительный** primitive для координации fiber'ов (owner-actor
pattern, Erlang-стиль). Однако Mutex, RwLock, Atomic и другие sync-примитивы
**доступны как stdlib** через `import runtime.sync.{...}` — в тех случаях
когда actor-модель избыточна.

**Default: owner-actor pattern.** Если мутируемое разделяемое состояние
нужно — первый выбор: **dedicated owner-fiber + channel**. Owner владеет
данными; остальные шлют сообщения через channel.

```nova
fn counter_actor(input Channel[CounterMsg], output Channel[int]) {
    mut value = 0
    while Some(msg) = input.recv() {
        match msg {
            Increment => value += 1
            Get       => output.send(value)
            Reset     => value = 0
        }
    }
}
```

Это **safe by construction** — нет shared state, только message-passing.

**Escape hatch: `runtime.sync`.** Когда actor-модель действительно избыточна
(счётчик статистики, одноразовая инициализация, read-heavy конфигурация),
используй explicit import:

```nova
import runtime.sync.{Mutex, AtomicI64, RwLock, Semaphore, Once}
// см. D370 decision tree — «когда что выбрать»
```

Детальное описание всех sync-примитивов:
- D167 — Memory ordering & `fence()` API
- D168 — Atomic типы (12 sized × 13 ops)
- D169 — `Mutex` / `RwLock` / `ReentrantMutex`
- D170 — `Semaphore` / `Barrier` / `CountDownLatch` / `Condvar`
- D171 — `Once` / `OnceCell` / `Lazy`
- D172 — `realtime { }` / `blocking { }` interaction matrix
- D370 — AI-first guidance: decision tree + canonical patterns

### Почему

1. **Закрывает реальный пробел spec'и.** D14/D50 упоминали `select`
   как structured-concurrency primitive без формальной декларации.
   D79 формализует; D94 фиксирует финальный синтаксис.

2. **Production-correctness.** В preemptive runtime (D14) shared `mut`
   между fiber'ами — UB. Channels единственный safe primitive по
   умолчанию.

3. **AI-first.** LLM пишет concurrent код по узнаваемому паттерну
   (Go-style channels). Никаких lock ordering задач, deadlock detection
   через структуру pipeline'а.

4. **D62-согласованность.** Suspension ambient → channel methods чистая
   сигнатура. Никаких Channel-effects в effect-row.

5. **`select` как primitive.** D14 уже описывал `select` как
   structured-concurrency primitive (наряду с `parallel for` / `race`);
   D79 даёт ему точную семантику относительно channels.

6. **Прецеденты:**
   - **Go** — channels + select как core feature; основа large-scale
     production систем (Kubernetes, Docker).
   - **Erlang/Elixir** — message-passing через mailboxes, та же
     философия.
   - **Crystal** — Go-style channels.
   - **Rust** (`std::sync::mpsc`) — channels как отдельный modul, не
     core; результат — community предпочла tokio crate с собственной
     моделью.
   - **OCaml 5** — domains + channels (effect-handlers).

### Что отвергнуто

- **`Channel[T]` как effect**, требующий `with Channel = ...`.
  Channel — это value-handle, не resource-capability. Подменять
  channel в тестах = передавать другой channel-объект (parameter
  injection), не handler-substitution.

- **Mutex / Atomic в prelude.** Низкоуровневые, легко misuse,
  deadlock-prone. Owner-actor pattern закрывает 99% use case'ов.
  Escape hatch доступен через `import runtime.sync.{...}` — stdlib,
  не prelude (D167-D172 + D370 (ex-D370)).

- **`<-` как recv-оператор в select.** Отвергнут в D94 — заменён
  на `Some(v) = rx.recv() =>`. Причина: `<-` вводил новый оператор
  только для select; `= rx.recv()` согласуется с `while let Some(v) = rx.recv()`
  (уже в языке) — никаких новых операторов.

- **Unbounded channels по умолчанию.** Bounded channel явно — лучшая
  practice для backpressure. `Channel[T].new(0)` для unbuffered;
  unbounded — **отвергнуто** (опасный antipattern). Если действительно
  нужен — через explicit buffer-grow в user-коде.

- **Channels как structural protocol.** Channel — конкретный type
  с runtime-implementation, не protocol. Возможны разные Channel-
  типы (например, `BroadcastChannel`), но они отдельные типы.

- **Builtin priority в select.** `select` non-deterministic между
  ready-armами. Если нужен приоритет — программист сам пишет
  if-cascade с try_recv.

### Цена

1. **Runtime сложность.** Channel требует buffer, lock-free queue
   (production), wait list, close-state machine. Bootstrap (D71) —
   проще: single-threaded queue + yield. Production — серьёзная
   реализация.

2. **`select` в parser.** Новая конструкция: `select { pattern = rx.recv() => body }`.
   Реализация — Plan 31 (D94). Синтаксис финализирован.

3. **Closed-channel panic vs throw.** Send на closed — panic. Это
   осознанный выбор: programmer error, не recoverable. Альтернатива
   (`Fail[ChannelClosed]`) усложнила бы каждый send. Cost: программист
   должен следить за close-protocol (обычно single owner закрывает).

4. **Non-determinism в select.** Программист не может полагаться
   на порядок arms. Тесты должны не зависеть от порядка (или
   использовать try_recv для строгого порядка).

### Связь

- [D14](#d14-fiber-runtime--невидимая-инфраструктура) — fiber-runtime
  основа; channels — primitive поверх него.
- [D50](#d50-concurrency-model-spawn-detach-blocking) — concurrency
  model; D79 формализует упомянутые там channels.
- [D62](04-effects.md#d62) — suspension ambient → чистые signatures
  для channel methods.
- [D71](#d71-bootstrap-concurrency-runtime) — bootstrap runtime;
  channels там тривиальная queue + yield.
- [D72](02-types.md#d72) — generic bounds; `Channel[T Clone]` если
  понадобится требование на T (пока не требуется).
- [D73](08-runtime.md#d73) — `From`/`Into`; для channels не применимо
  (channel — handle, не value-конверсия).
- [D75](#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном)
  — `supervised(cancel:)`; channels часто используются с cancellation
  tokens.
- [D13](08-runtime.md#d13) — panic vs Fail; close+send → panic.

### Открытые вопросы

- **Broadcast channels** (один send → все receivers). Q-broadcast —
  отдельная задача после v1.0. Pattern: можно реализовать через
  владельческий fiber, который рассылает в N output-каналов.
- **Channel of channels** для dynamic worker pools. Технически
  работает (Channel[Channel[T]]), нужны примеры в stdlib.
- **`@send_timeout(v T, d Duration)`** — отдельная вариация. Можно
  через select с timeout, но iдиома громоздкая. Q-send-timeout.
- **Memory model между fibers.** В preemptive runtime — strong
  ordering (как Go: channel send/recv — happens-before). В D14
  production-runtime — нужно явно зафиксировать. Q-memory-model.

### Эволюция

До D79:
- D14 (2024-2025) — упомянул `select` пример с `<- channel_a` без
  определения Channel.
- D50 — упомянул «channels» в обсуждении spawn'а с `mut`-захватами,
  но без типа.
- D71 (2026-05-06) — bootstrap runtime; channels отложены как
  «producer-consumer через shared mut + yields».
- spec-review (2026-05-07) — компиляторный агент идентифицировал
  Channel/Mutex как spec-gap.

D79 закрывает gap: формальная декларация Channel[T] + select +
семантика closed/non-deterministic ordering/owner-actor pattern.

Bootstrap-реализация — следующий шаг (компиляторный агент).

---

## D80. Handler scoping per-fiber

### Что
`with X = handler { body }` устанавливает binding `X = handler`
**только** для текущего fiber'а (D14). Другие fiber'ы — работающие
concurrent на том же OS-thread (D71 cooperative) или разных
OS-thread'ах (D14 production multithreaded) — **не видят** этот
binding.

При `spawn`/`parallel for`/`supervised`-spawn новый fiber **наследует**
текущий handler-stack (snapshot всех активных handler-pointers).
Изменения handler'ов внутри fiber'а (через дополнительные `with`-блоки)
видны только этому fiber'у.

### Правило

**Семантика:**

1. Каждый fiber имеет собственный snapshot handler-pointers для всех
   эффектов.
2. При resume fiber'а scheduler'ом: handler-state восстанавливается
   из fiber's snapshot.
3. После yield/return: handler-state сохраняется обратно в fiber's
   snapshot.
4. Handler-state восстанавливается к outer-flow state (как до resume).
5. `spawn` нового fiber'а наследует current handler-state как initial
   snapshot — structured-concurrency наследование.

**Грамматика без изменений** — это runtime-инвариант, не
языковая конструкция.

### Пример

Изоляция между fiber'ами:

```nova
fn use_clock_100() -> int {
    with Time = effect Time { sleep(_) => () now() => 100 } {
        Time.now()                   // ВСЕГДА 100, независимо от других fiber'ов
    }
}

fn use_clock_200() -> int {
    with Time = effect Time { sleep(_) => () now() => 200 } {
        Time.now()                   // ВСЕГДА 200
    }
}

supervised {
    spawn { ro a = use_clock_100() }   // a == 100, гарантированно
    spawn { ro b = use_clock_200() }   // b == 200, гарантированно
}
```

Inheritance + override:

```nova
with Time = effect Time { ... now() => 42 } {
    supervised {
        spawn {
            assert(Time.now() == 42)         // наследовал outer

            with Time = effect Time { ... now() => 999 } {
                assert(Time.now() == 999)    // inner override виден только здесь
            }

            assert(Time.now() == 42)         // outer восстановлен
        }
    }
}
```

### Почему

1. **Корректность.** Без per-fiber scoping handler одного fiber'а
   может быть перезаписан другим fiber'ом на shared TLS-globals.
   Тихий data corruption — наихудший класс багов в concurrent коде.

2. **D14 invariant.** «Невидимая инфраструктура fiber-runtime'а»
   подразумевает, что fiber'ы логически независимы. Shared mutable
   state — нарушение.

3. **AI-friendly.** LLM генерирует код по логической модели «каждый
   spawn — независимый поток вычисления». Без per-fiber scoping
   эта модель ломается на handler'ах.

4. **Прецеденты.**
   - **OCaml 5 effect handlers** — handler scope follows fiber-tree.
   - **Koka effect handlers** — то же.
   - **Rust `tokio::task_local!`** — explicit per-task storage с
     parent inheritance.

### Что отвергнуто

- **Shared TLS handlers** (старая bootstrap-семантика до 2026-05-07).
  Тихий data corruption между fiber'ами на одном OS-thread'е.
- **Explicit handler passing** через параметры. Нарушает D62
  «handler — implicit через with-scope».
- **Copy-on-write snapshot.** Premature optimization; bootstrap
  использует eager save/restore, ~µs overhead per resume.

### Цена

- **Memory:** один snapshot per fiber, размер = N × pointer (N =
  количество зарегистрированных эффектов). В bootstrap'е N ≤ 5,
  ~256 байт. Heap-allocated чтобы не overflow'ить fiber stack.
- **CPU:** save/restore — N memcpy-equivalent на каждый resume.
  Production может использовать lazy/COW snapshots.

### Implementation invariant: handler-storage **не static**

Codegen эмитит handler-storage (`_nova_handler_X` для каждого
эффекта `X`) с **external linkage** — без `static`:

```c
__declspec(thread) NovaVtable_X* _nova_handler_X = NULL;          // ✓ correct
__declspec(thread) static NovaVtable_X* _nova_handler_X = NULL;   // ✗ WRONG
```

`static` ограничивает visibility одним translation unit (TU). Это
**ломает D80** в трёх случаях:

1. **Registry в другом TU.**
   `nova_register_effect_storage(&_nova_handler_X)` вызывается из
   main wrapper. Если storage `static` в module-TU, а registry в
   `effects.c` — registry формально не должен видеть storage.
   В bootstrap'е (single-TU compilation) случайно работает, но
   архитектурно неверно.

2. **Production multi-module compilation.** При разделении проекта
   на multiple `.c` файлов user-defined effect, объявленный в
   module A, может использоваться в module B (через `import`).
   Storage обязан быть extern-видимым.

3. **Snapshot save/restore через `void**`.** Registry хранит `void**`
   (адрес slot'а). Доступ через TLS-pointer должен следовать
   правилам external linkage; со `static` это
   implementation-defined behavior.

Built-in эффекты (Fail, Time, Mem) в `nova_rt/effects.c` уже без
`static` — правильно. **User-defined effect storage обязан
следовать тому же правилу.** Codegen `compiler-codegen/src/codegen/
emit_c.rs` эмитит без `static` начиная с 2026-05-07 (commit
55d896de3); до этого эмитился `static`, что работало случайно
из-за single-TU bootstrap-компиляции.

### Связь

- [D14](#d14-fiber-runtime--невидимая-инфраструктура) — fiber-runtime
  как «невидимая инфраструктура». D80 уточняет, что handler-state
  входит в эту инфраструктуру (per-fiber, не shared).
- [D50](#d50-concurrency-model-spawn-detach-blocking) — `spawn`/`detach`
  естественно расширяются handler-наследованием.
- [D61](04-effects.md#d61) — effect/handler keywords; D80 — runtime
  invariant, который семантика D61 уже подразумевала.
- [D71](#d71-bootstrap-concurrency-runtime) — bootstrap runtime;
  снапшот save/restore реализован в `nova_supervised_step` (2026-05-07).
- [D92](#d92-top-level-main-как-implicit-supervised-scope) — implicit
  main-scope. D80 handler-snapshot работает одинаково внутри main-scope
  и любого supervised блока (D92 делает main симметричным).
- [D93](#d93-park-wake--нормативный-runtime-primitive-для-блокирующих-операций)
  — park/wake API. Park'нутый fiber сохраняет свой handler-snapshot
  (per-fiber invariant D80) до wake'а, callback от libuv не видит
  чужие handlers — он работает на main-thread context'е до resume.
- [D75](#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном)
  — `supervised(cancel:)` использует тот же per-scope state pattern.

### Производительность и roadmap оптимизации

Текущая bootstrap-реализация (snapshot save/restore через registry)
**корректна, но не оптимальна** по скорости. Зафиксируем стоимость
и варианты оптимизации для production-runtime.

#### Текущая стоимость (bootstrap)

При каждом fiber-switch в `nova_supervised_step`:

```
   1× snapshot_restore(outer)    — N pointer-copy
   for each fiber:
       1× snapshot_restore(fiber)  — N pointer-copy
       mco_resume                   — actual coroutine switch
       1× snapshot_save(fiber)      — N pointer-copy
       1× snapshot_restore(outer)   — N pointer-copy
```

**Итого: 4 × N memory operations per switch** (N = registered effects,
обычно ≤ 5 в bootstrap, потенциально 10-20 в большом проекте).

Дополнительно:
- **Heap allocation** snapshot'а при spawn (`nova_alloc(sizeof(snapshot))`
  ≈ 256 B) → GC pressure.
- **Indirection через registry**: `*registry.slots[i] = snap.values[i]`
  — extra pointer chase per restore.
- **`Nova_X.op()`**: один indirect call через TLS pointer + один indirect
  через vtable function pointer = 2 indirect calls вместо direct.

Для типичного backend-кода (handler'ы редко перезапускаются, fiber switches
на уровне сотен/секунду) — **negligible**. Для hot-path / real-time /
game-loop — может стать bottleneck.

#### Варианты оптимизации (от простого к сложному)

##### 1. **Linked-list cactus stack handler-frames** (умеренно быстрее)

Каждый fiber имеет указатель `current_handler_frame` в его coroutine
context. `with X = h { body }` пушит frame в linked list:

```c
typedef struct HandlerFrame {
    EffectId           effect_id;
    void*              vtable;
    void*              ctx;
    struct HandlerFrame* prev;
} HandlerFrame;

__declspec(thread) HandlerFrame* _nova_handler_top;

// Nova_X.op() walks the chain
static inline ret_t Nova_X_op(args) {
    for (HandlerFrame* f = _nova_handler_top; f; f = f->prev)
        if (f->effect_id == X_ID)
            return ((Vt_X*)f->vtable)->op(f->ctx, args);
    abort_no_handler();
}
```

**Плюсы:**
- Switch: O(1) — просто swap `_nova_handler_top` (один pointer вместо
  массива). Может быть встроено в mco-coroutine state, switch — free.
- No heap allocation для snapshot — frames живут на fiber stack.
- Spawn inheritance — копировать только указатель `_nova_handler_top`
  родителя.

**Минусы:**
- `Nova_X.op()` теперь O(depth) — walk handler-stack. На практике
  depth обычно 1-2, но в plagued-with-handlers коде может быть 5-10.
- Branch prediction менее предсказуем (depth разная per call).

**Сложность реализации:** ~100 строк runtime, codegen меняется минимально
(`with X = h { body }` → push/pop frame вместо assign/restore TLS).

**Целевой gain:** ~3-5× быстрее snapshot save/restore для switches.
`Nova_X.op()` слегка медленнее (1 extra branch + memory read).

##### 2. **Inline handler-frames на fiber stack + statically-resolved op-call**

Самое быстрое — Koka/Effekt-style runtime. Compiler **во время
type-check'а** определяет какой handler-frame будет активен в каждой
точке `Nova_X.op()` call'а (через effect-row analysis), и эмитит
**прямой call** через known offset.

```nova
fn process() X -> ()  =>  X.op()    // X известен в типе
```

Компилируется в:

```c
static void process(HandlerFrame_X* x_frame) {
    x_frame->op(x_frame->ctx);    // direct call, 0 overhead vs обычная функция
}
```

`with X = h { body }` создаёт `HandlerFrame_X` на стеке и передаёт
адрес в body как явный параметр (или через register).

**Плюсы:**
- `Nova_X.op()`: **0 overhead** vs обычная функция (один direct call).
- Switch: трогать handler-state не надо вообще — передаются с фреймом.
- Inlinable: компилятор может полностью inline `op()` если
  handler-литерал известен.

**Минусы:**
- Требует **полную мономорфизацию по effect-rows** в compiler'е.
- `Effect[X]` как first-class value (`fn make() -> Effect[X]`) сложнее —
  нужен fallback dynamic dispatch когда handler передан как value.
- Dependent на static effect-resolution; rank-2 effect polymorphism
  усложняется.
- Major compiler work — ~3-5k строк для proper effect type-checker'а.

**Целевой gain:** ~10-50× для hot-path effect ops (от dispatch overhead
до полного inline).

**Прецеденты:** Koka, Effekt (academic), OCaml 5 (multicore).

##### 3. **Lazy / Copy-on-Write snapshot**

Промежуточный вариант: оставить registry-based snapshot, но
**не делать save/restore** на каждом switch. Tracking dirty-bit
per effect:

```c
typedef struct {
    void*    values[N];
    uint64_t dirty_mask;   // bit i set if effect i was modified by this fiber
} Snapshot;
```

`with X = h { body }` устанавливает dirty bit. На fiber-switch:
- Restore: только те slots что были dirty в old fiber + те что
  dirty в new fiber.
- Save: только dirty slots.

**Плюсы:** для типичного кода где fiber меняет 0-1 handlers → 0-1
copy на switch (вместо N).

**Минусы:** добавляет complexity tracking + branch на каждый `with`.

**Целевой gain:** ~3-10× для typical code, нет gain для plagued-with-
handlers.

#### Рекомендуемый roadmap

| Phase | Что | Когда |
|---|---|---|
| **bootstrap (now)** | Snapshot save/restore (текущее) | done |
| **v0.5** | Cactus-list handler-frames | первый perf-critical use-case (game/real-time/proxy) |
| **v0.7+** | Static effect resolution + inline frames | при работе над production type-checker'ом (rank-2 effect polymorphism, Koka-style) |

**Принцип:** **не оптимизировать преждевременно.** Текущая реализация —
~µs overhead на switch, для backend-кода это <1% от стоимости request'а.
Когда найдётся реальный bottleneck (профилирование production-приложения)
— перейдём на cactus-list. Inline frames — финальная стадия, требует
significant compiler work и не имеет смысла до того как остальные
части compiler'а matured.

#### Что **не делать**

- **Локализовать handler в каждый scope** через текущее save/restore с
  меньшим N (через тонкую регистрацию). Добавляет complexity без
  существенного gain'а — N в bootstrap уже маленькое.
- **Atomic compare-and-swap для multi-thread** — преждевременно; D14
  production multithreaded — отдельный future D-decision (handler-mt),
  там handler-storage per OS-thread + per-fiber внутри thread.
- **Caching last-resolved handler in fiber state** — добавляет
  invalidation complexity без чёткого gain'а.

### Эволюция

До 2026-05-07 bootstrap-runtime хранил handler'ы в `__declspec(thread)`
TLS-globals **без** per-fiber изоляции — handler одного fiber'а на
том же OS-thread'е перезаписывал handler другого. Compiler-агент
выявил bug на тестах с разными `with Time = ...` handler'ами в
параллельных fiber'ах и пофиксил через snapshot save/restore вокруг
`mco_resume` + `nova_register_effect_storage` registry. D80
формализует invariant в spec'е (тесты:
`nova_tests/concurrency/per_fiber_handlers.nv` — 4 случая).

---

## D91. Channel revision — capability-split на `ChanWriter` / `ChanReader`

> **Уточняет** [D79](#d79-channels--coordination-между-fiber-ами) —
> модель API меняется с Go-style (один объект с `send`/`recv`) на
> Rust mpsc-style (capability-split). Остальное D79 (buffer,
> owner-actor pattern, `select`) сохраняется.
>
> **Plan 59.1 amend (2026-06-01):** signature `fn Channel[T].new(cap int)
> -> (ChanWriter[T], ChanReader[T])` теперь **буквально implementable** —
> generic anonymous tuple monomorphization работает для произвольных
> user fns после Plan 59.1 (см. [D354](02-types.md#d354-generic-anonymous-tuple-monomorphization)).
> Текущая реализация bootstrap-периода продолжает использовать runtime
> struct `Nova_ChannelPair` через 3 ad-hoc codegen branches (emit_c.rs
> 18435/20159/22694) — это implementation detail, не противоречит
> signature. Cleanup ad-hoc paths → followup
> `[M-59.1-channel-new-cleanup]` (требует Nova-side `external fn[T]
> Channel[T].new(...)` declaration через Plan 115 Pattern B).

### Что

`Channel[T].new(capacity)` возвращает **пару** объектов с разными
**capabilities**:

```nova
ro (tx, rx) = Channel[int].new(4)
tx.send(10)
ro v = rx.recv()
defer tx.close()                    // close — обязателен, см. D90
```

- **`ChanWriter[T]`** — capability «отправлять в канал». Методы: `send`,
  `try_send`, `close`, `clone`.
- **`ChanReader[T]`** — capability «получать из канала». Методы: `recv`,
  `try_recv`.

Внутренний state (buffer, sync) **скрыт** — не доступен напрямую,
только через capabilities.

> **Амендмент 2026-07-27 (Plan 221.1 №144, `[M-channel-heap-value-shared-
> not-moved]`): канал ЗАБИРАЕТ владение отправленным значением.** До этого
> амендмента спека про владение при передаче в канал МОЛЧАЛА — это и был
> корень измеренного дефекта: отправитель кладёт `Vec[int]` с одним
> элементом, ПОСЛЕ `send` дописывает второй — получатель видит ОБА
> (канал передаёт общий указатель на кучу без изоляции и без передачи
> владения; отправитель сохранял полный доступ к отправленному). Под M:N
> это гонка данных по построению: два fiber'а на разных ОС-потоках мутируют
> один объект.
>
> Разбор вариантов (владелец, 2026-07-27): (а) «только значения со стека» —
> отвергнут, режет каналы записей/векторов (основной сценарий); (б)
> «глубокое клонирование при send и recv» — отвергнут, дорого на каждой
> передаче и ломает сценарий передачи владения большим буфером; (в)
> **передача владения (`consume`) — принято**: `send(consume v)`/
> `try_send(consume v)` — после отправки `v` недоступен, алиасинга нет ПО
> ПОСТРОЕНИЮ, нулевая цена копирования, механизм совпадает с Rust
> `Sender<T>` (см. [reference-rustc-as-reference]). Использование значения
> после `send`/`try_send` ловится ОБЫЧНОЙ линейной проверкой (D131
> use-after-consume) — той же, что и для любого другого consume-параметра.
>
> **Намеренное разделение** остаётся возможным, но НЕ через повторное
> использование уже отправленного значения — только явно, на уровне
> КАНАЛА: `tx.share()`/`tx.clone()` создаёт дополнительный writer-handle
> над тем же буфером (см. «Multi-writer» ниже), каждый handle шлёт СВОИ
> значения. Общее value-level разделение вне канала — стандартный `#share`
> механизм spawn-захватов (D415 §2), к `send`/`recv` отношения не имеет.
>
> Связанное: [M-channel-elem-type-not-tracked] — `T` в `Channel[T].new`
> сегодня не отслеживается сквозь `send`/`recv` (типовая проверка
> элемента — отдельный, ещё не закрытый дефект); этот амендмент фиксирует
> ТОЛЬКО владение, не типизацию элемента.
>
> **Амендмент 2026-08-02 (Plan 221.1 №143/№286, окно p-chan): элемент `T`
> ТЕПЕРЬ отслеживается сквозь `send`/`recv`, когда объявлен явно.**
> Измерение интегратора (реестр 221.1 №286) показало: до этого амендмента
> `Channel[int]` принимал `send("строка")` БЕЗ ошибки чекера (ловилась
> только codegen-ограждение слота `E_CHANNEL_UNSOUND_ELEM_TYPE`, и то по
> РАЗМЕРУ C-представления, не по типу — `Channel[Meters]` молча принимал
> `Seconds` того же размера); `recv()` статически типизировался
> `Option[int]` НЕЗАВИСИМО от объявленного `T`, а доступ к полям/методам
> полученного значения резолвился через legacy name-only fallback —
> НЕДЕТЕРМИНИРОВАННО при co-присутствии одноимённых методов в одном
> compile-unit (тихая мискомпиляция, `[M-channel-recv-erased-elem-method-
> nondeterministic]`).
>
> **Правило теперь:** когда `T` объявлен ЯВНО — турбофишем
> (`Channel[T].new(cap)`) либо аннотацией параметра/локали
> (`ChanWriter[T]`/`ChanReader[T]`, см. «Passing to functions» в
> `docs/guide/channels.md`) — чекер отслеживает `T` СКВОЗНЫМ образом:
> `send(v)`/`try_send(v)` требуют `v` совместимого с `T` типа (ошибка
> `[E_CHANNEL_ELEM_TYPE_MISMATCH]`, включая различение двух newtype
> одинакового размера — `Meters` vs `Seconds`); `recv()`/`try_recv()`
> типизируются настоящим `Option[T]`, так что доступ к полям/методам
> полученного значения резолвится по РЕАЛЬНОМУ типу, не по имени.
> `Channel.new(cap)` БЕЗ турбофиша/аннотации по-прежнему оставляет `T`
> неотслеженным (permissive legacy-путь, обратная совместимость —
> поведение НЕ регрессирует для существующего непомеченного кода).
> Word-safe-ограничение рантайм-слота (`E_CHANNEL_UNSOUND_ELEM_TYPE`,
> ниже) остаётся независимой, ортогональной проверкой — само
> представление `T` в одном machine-word слоте `nova_rt/channels.h` этим
> амендментом не меняется.
>
> **НЕ закрыто этим амендментом** — снос 3 ad-hoc codegen-веток
> `Channel.new` (`Nova_ChannelPair`) в пользу настоящего generic-tuple
> возврата `(ChanWriter[T], ChanReader[T])`. Проверено: `Plan 115`
> (`external fn`/tuple-FFI ABI, изначально названный prerequisite'ом) до
> сих пор НЕ реализован (`docs/plans/115-ptr-type-and-tuple-ffi.md`,
> статус PLANNED) — и, как выяснилось, для этого конкретного случая не
> строго обязателен (`Channel.new` зовёт СОБСТВЕННЫЙ runtime-intrinsic,
> не внешнюю C-библиотеку), но регистрация `Channel.new` как настоящей
> generic-fn декларации в реестрах чекера (подключение к уже landed Plan
> 59.1 tuple-mono инфраструктуре) — риск для ВСЕХ существующих callsites
> по всему репозиторию, отдельного объёма/окна. `[M-59.1-channel-new-
> cleanup]` остаётся открытым; `Nova_ChannelPair` продолжает быть
> implementation detail, не противоречащим сигнатуре (см. Plan 59.1
> amend выше).
>
> **Амендмент 2026-08-09 (№513, окно p513-channel): голый `Channel.new(cap)`
> БЕЗ турбофиша/аннотации БОЛЬШЕ НЕ КОМПИЛИРУЕТСЯ.** Отменяет
> «permissive legacy-путь» амендмента 2026-08-02 непосредственно выше —
> тот путь позволял голой форме компилироваться с неотслеженным `T`;
> дерево показало 232 носителя, использующих голую форму как ОСНОВНУЮ (не
> запасную), включая 87 страниц публикуемой докой (`docs/guide/`), при
> каноне, который с самого начала D79 (выше, «Тип Channel[T]») требовал
> `Channel[T].new`. Реализация теперь приведена к решению, а не решение
> подогнано под реализацию: bare-форма (`ExprKind::Ident("Channel")`/
> `ExprKind::Path(["Channel","new"])` без турбофиша) — ошибка компиляции
> `[E_CHANNEL_NEW_BARE]`, гейт в чекере (`consume_walk_expr`,
> `compiler-codegen/src/types/mod.rs`), срабатывает на каждом `Call` в
> каждой fn/method body до кодогена. Единственная форма — явный турбофиш
> (`Channel[T].new(cap)`) или аннотация capability-типов
> (`ChanWriter[T]`/`ChanReader[T]`). Word-safe-ограничение рантайм-слота
> (`E_CHANNEL_UNSOUND_ELEM_TYPE`) и type-mismatch-проверка
> (`E_CHANNEL_ELEM_TYPE_MISMATCH`) из амендмента 2026-08-02 — без
> изменений, применяются к явно типизированной форме как и раньше.

### Правило

#### API — типы

```nova
// Writer capability:
type ChanWriter[T] protocol {
    send(consume v T) -> bool                     // ЗАБИРАЕТ владение v; true если послал; false если канал закрыт
    try_send(consume v T) -> bool                 // ЗАБИРАЕТ владение v; true если послал, false если полон или закрыт
    close() -> ()                                 // закрыть (idempotent; ref-counted при clone)
    clone() -> ChanWriter[T]                      // дополнительный writer на тот же буфер
}

// Reader capability:
type ChanReader[T] protocol {
    recv() -> Option[T]                           // blocking; None = closed+drained
    try_recv() -> Option[T]                       // None = пусто (НЕ означает closed)
}
```

`ChanWriter` и `ChanReader` — **protocols**. Конкретная реализация скрыта
внутри `Channel.new`. Типы-аннотации в сигнатурах функций:

```nova
fn fill(tx ChanWriter[int], items []int) { ... }
fn drain(rx ChanReader[int]) -> int { ... }
```

#### Factory

```nova
fn Channel[T].new(capacity int) -> (ChanWriter[T], ChanReader[T])
```

`capacity = 0` — unbuffered channel (rendezvous: send блокирует
пока recv не примет; так же как D79).

#### Close semantics

**Explicit close.** Nova не имеет deterministic destructor'ов
([D6](05-memory.md#d6) managed heap), поэтому **auto-on-drop**
(как Rust mpsc) **не работает predictably** — GC соберёт sender
«когда-нибудь», receiver висит непредсказуемо.

Решение: программист обязан **явно** вызвать `tx.close()`. Идиома —
через [D90](03-syntax.md#d90) `defer`:

```nova
fn run_pipeline() Net -> () {
    ro (tx, rx) = Channel[Job].new(10)
    defer tx.close()                              // гарантированный close

    supervised {
        spawn { for j in jobs { tx.send(j) } }
        spawn { while Some(j) = rx.recv() { process(j) } }
    }
}   // <- tx.close() сработает; rx.recv() в spawn'е получит None и завершится
```

`close()` — **idempotent**: повторный вызов не error.

После close:
- `tx.send(v)` — возвращает `false` (канал закрыт). Не panic — программист
  может проверить результат: `if !tx.send(v) { /* канал закрыт */ }`.
- `tx.try_send(v)` — возвращает `false`.
- `rx.recv()` — возвращает `Some(v)` пока буфер не пуст; потом `None`.
- `rx.try_recv()` — то же.

#### Multi-writer: tx.clone()

`ChanWriter` поддерживает `clone()` — создаёт дополнительный writer
на тот же буфер с ref-count семантикой:

```nova
ro (tx, rx) = Channel[Job].new(10)
ro tx2 = tx.clone()
supervised {
    spawn { tx.send(1);  tx.close() }
    spawn { tx2.send(2); tx2.close() }
    spawn { while Some(v) = rx.recv() { process(v) } }
}
```

**Семантика close с несколькими writers:** канал закрывается только
когда **все** writers вызвали `close()`. Внутри — ref-count
(`writer_count`): `Channel.new` инициализирует в 1, `clone()`
инкрементирует, `close()` декрементирует и закрывает при 0.

Идиома для spawn-fan-in:
```nova
ro (tx, rx) = Channel[int].new(8)
supervised {
    for item in work_items {
        ro worker_tx = tx.clone()
        spawn { worker_tx.send(process(item)); worker_tx.close() }
    }
    tx.close()                                    // close «корневого» writer'а
    spawn { while Some(v) = rx.recv() { collect(v) } }
}
```

> **Managed heap и captures.** Без `clone()` два `spawn` могут захватить
> один `tx` через managed reference — оба могут слать. Но `close()`
> первого spawn'а закрыл бы канал для второго. `clone()` решает это:
> каждый spawn держит свою capability и закрывает её независимо.

#### `select` после revision

`select` работает через `ChanReader` (Plan 31, не реализован):

```nova
ro (_, rx_a) = Channel[int].new(0)
ro (_, rx_b) = Channel[int].new(0)

select {
    Some(v) = rx_a.recv() => process_a(v)
    Some(v) = rx_b.recv() => process_b(v)
    _ = Time.sleep(5.0)   => default_action()
}
```

Синтаксис и D94-решение — в [Plan 31](../../docs/plans/31-channel-select.md).

### Почему

#### Зачем capability-split

В Go-style (D79 текущий) `Channel[T]` имеет и `send`, и `recv` на
одном объекте. Это удобно для simple случаев, но **проблематично**
в концurrency-патернах:

1. **Producer/consumer.** Producer должен **только** слать, consumer
   **только** получать. С Go-style — оба могут случайно вызвать `recv`/
   `send` на чужой стороне, типы это не запрещают.

2. **Передача в spawn.** Хочется передать в spawn только sender-
   capability (`spawn { for x in source { tx.send(x) } }`), без
   возможности recv'ить. С Go-style нельзя — передаётся весь объект.

3. **API дизайн.** Функция возвращает «вы можете только читать из
   этого» — нужен Receiver-only тип. Go-style не даёт.

Capability-split решает все три.

#### Прецеденты

| Язык | Модель |
|---|---|
| Go | один `chan T` с send/recv |
| **Rust mpsc** | `(Sender<T>, Receiver<T>)` через `channel()` |
| **Tokio mpsc** | то же |
| Python `Queue` | один объект (Go-style) |
| Python `multiprocessing.Pipe` | `(conn1, conn2)` (split) |
| JS `MessageChannel` | `(port1, port2)` (split) |
| OCaml 5 `Eio.Stream` | один объект (Go-style) |

Capability-split — **доминирующая модель** в Rust ecosystem. Nova
переходит на неё, потому что:
- Type-safety capabilities в сигнатуре функции.
- Структурное совпадение с Rust — programmers familiar.

#### Почему close — explicit, не auto-on-drop

В Rust auto-on-drop работает благодаря **deterministic destruction**
(ownership). Когда последний `Sender` уходит из scope —
`drop::drop()` вызывается **немедленно**, канал закрывается, receiver
видит None.

В Nova нет destructor'ов ([D6](05-memory.md#d6)). GC соберёт sender
**когда-нибудь** — может через 100ms, может через 10s. Если auto-
on-drop завязан на GC-сборку:

```nova
{
    ro (tx, rx) = Channel[int].new(4)
    tx.send(42)
    // tx уходит из scope здесь
}
// rx видит close — когда? Зависит от GC. Тесты flaky.
```

Это **неприемлемо**. Closing должно быть **детерминированным** —
от него зависят receiver'ы.

Решение: **explicit close** через `defer tx.close()` ([D90](03-syntax.md#d90)).
`defer` выполняется при exit'е scope'а, deterministically. Идиома:

```nova
fn pipeline() Net -> () {
    ro (tx, rx) = Channel[Job].new(10)
    defer tx.close()                              // в каждой функции, где tx уходит из scope
    // ...
}
```

#### Почему `recv() -> Option[T]`, не `Fail[Closed] -> T`

Closed-channel — **не ошибка**. Это валидный исход «source закончился».
Receiver-loop через `while let Some(x) = rx.recv() { ... }`
идиоматичен: цикл сам завершается на close.

Если бы `recv` бросал — каждый receiver-loop обёрнут handler'ом,
шум. `Option[T]` композируется с `?` и `match`, не требует
дополнительных эффектов.

Это согласовано с Rust mpsc `recv() -> Result<T, RecvError>` —
семантически то же, но Result там в Rust-context, в Nova
`Option[T]` чище (нет специального `RecvError` типа).

### Migration от D79 (Go-style)

**Было (D79):**

```nova
ro ch = Channel[int].new(4)
ch.send(10)
ro v = ch.recv()
ch.close()
```

**Стало (D91):**

```nova
ro (tx, rx) = Channel[int].new(4)
defer tx.close()
tx.send(10)
ro v = rx.recv()
```

**Изменения:**
1. `Channel.new(N)` возвращает `(ChanWriter, ChanReader)`, не `Channel`.
2. `send` через `tx`, `recv` через `rx`.
3. `close` через `tx.close()` (или `defer tx.close()`).
4. `Channel[T]` как type-аннотация **не используется** в коде — есть
   только `ChanWriter[T]` и `ChanReader[T]`.

**Что нужно мигрировать:**
- `std/` — нет существующих `Channel`-API.
- `nova_tests/runtime/channels.nv` — переписать все тесты.
- Bootstrap `nova_rt/channels.h` — переделать API: state-struct
  + sender/receiver wrappers.
- `select { ... }` — синтаксис заменён на D94 (`Some(v) = rx.recv() =>`), см. Plan 31.

Реализация — отдельный план (Plan 22+).

### Что отвергнуто

- **Auto-on-drop (Rust-style).** Не работает в managed heap без
  deterministic destruction. См. «Почему close — explicit».
- **`recv() Fail[Closed] -> T`.** Closed — не ошибка, валидный исход.
  `Option[T]` композируется чище.
- **Auto-close по GC.** Не работает — GC недетерминирован, explicit
  `tx.close()` обязателен (см. «Почему close — explicit»).
- **Сохранить Go-style как альтернативу.** Два API для одной задачи —
  нарушение D40 «один очевидный путь». Полная замена D79 →
  D91-семантика.
- **Многотиповые каналы (broadcast, oneshot, watch как в Tokio).**
  Не в bootstrap. mpsc — основной use-case. Остальные — расширения
  позже.

### Связь

- [D79](#d79-channels--coordination-между-fiber-ами) — частично
  пересмотрено. API меняется, остальное (buffer, owner-actor pattern,
  `select`) сохраняется.
- [D14](#d14-fiber-runtime--невидимая-инфраструктура), [D50](#d50-concurrency-model-spawn-detach-blocking) —
  fiber-runtime для blocking send/recv.
- [D6](05-memory.md#d6) — managed heap, мотивирует **explicit close**
  (нет destructor'ов).
- [D90](03-syntax.md#d90) — `defer` для гарантированного close.
- [D85](04-effects.md#d85) — `?` для composing `recv() -> Option[T]`.
- [Q-keyword-symmetry](../open-questions.md#q-keyword-symmetry) —
  capability-split factory как use-case для anonymous protocol-impl.

### Bootstrap-status

- ✅ **Реализовано в Plan 21** (2026-05-11). **Улучшено в Plan 30** (2026-05-11).
- `nova_rt/channels.h` — D91 capability-split: `Nova_ChanWriter*` / `Nova_ChanReader*`,
  park/wake через D93 sched API, heap-allocated waiters (safe под M:N Plan 44).
- `emit_c.rs` — `Channel.new(cap)` → `Nova_ChannelPair`, dispatch по типу объекта.
- `nova_tests/runtime/channels.nv` — 23 теста: FIFO, ring-buffer, closed-channel,
  try_send/try_recv, while-let, concurrent spawn, producer-consumer, ping-pong,
  передача `ChanWriter[T]`/`ChanReader[T]` в функции; send→bool тесты; fan-in тесты.
- Негативные тесты: `channel_sender_no_recv`, `channel_receiver_no_send` (EXPECT_CC_ERROR).
- **Plan 30 Ф.1** (2026-05-11): `send()` возвращает `nova_bool` — `false` если канал
  закрыт, не бросает; `assert(tx.send(v))` и `let ok = tx.send(v)` работают.
- **Plan 30 Ф.2** (2026-05-11): `tx.clone()` — multi-writer ref-count (`writer_count`
  в `Nova_ChannelState`); канал закрывается только когда все writers вызвали `close()`.
- `select` — вынесен в Plan 31 (отдельный план с runtime SelectWaiter).


---

## D93. Park/wake — нормативный runtime primitive для блокирующих операций

> **Введён:** Plan 22 Ф.3 (2026-05-11).
> **Реализация:** `compiler-codegen/nova_rt/sched.h` (header-only inline).

### Что

Runtime exposes стандартный API через `nova_rt/sched.h` для park/wake
fiber'ов. Любая блокирующая операция в runtime'е (Time.sleep, Channel.recv,
socket-read, file-read) **обязана** использовать этот API. Это
contract на котором держится unified event-loop driven scheduling
(Plan 22 Ф.4+), Channel D91 (Plan 21), и любые будущие IO operations
([Plan 83.12](../../docs/plans/83.12-async-net-stdlib.md) std/net ✅, std.fs Plan 18+).

### API surface

```c
/* ─── Park / wake ───────────────────── */
void      nova_sched_park(NovaFiberQueue* scope, int slot);
void      nova_sched_wake(NovaFiberQueue* scope, int slot);
nova_bool nova_sched_is_parked(NovaFiberQueue* scope, int slot);

/* ─── Cancel-integration (Plan 22 Ф.8 sync/async contract) ──── */
typedef enum {
    NOVA_STOP_SYNC  = 0,   /* handle полностью freed после return; unpark immediate */
    NOVA_STOP_ASYNC = 1,   /* close initiated; wake придёт от backend (close_cb / waitlist) */
} NovaStopMode;

typedef NovaStopMode (*NovaCancelStopCb)(void* handle);

void nova_sched_register_pending(NovaFiberQueue* scope, int slot,
                                  void* handle, NovaCancelStopCb stop_cb);
void nova_sched_unregister_pending(NovaFiberQueue* scope, int slot);

/* ─── Introspection ────────────────── */
int nova_sched_count_alive(NovaFiberQueue* scope);
int nova_sched_count_parked(NovaFiberQueue* scope);
int nova_sched_count_ready(NovaFiberQueue* scope);
```

### Семантика

**1. Park atomic-with-yield.** `nova_sched_park` ставит `parked[slot] = true`
и сразу делает `mco_yield`. Race-window нулевой — single-thread bootstrap
обеспечивает это естественно. Под M:N ([Plan 44](../../docs/plans/44-mn-runtime-roadmap.md))
потребуется memory fence перед yield'ом.

**1a. Park-with-predicate** (Plan 83.4.1, ред. 2026-05-23). Новые
блокирующие операции под M:N **обязаны** использовать
`nova_sched_park_until(scope, slot, pred, ctx)` вместо bare
`nova_sched_park`. Park возвращается ТОЛЬКО когда `pred(ctx) → true`;
spurious wake (включая M:N drain-quiescence-wake до завершения
async close_cb / after_work_cb) автоматически re-park'ится в loop'е.
Memory ordering contract: предикат-функция читает опубликованное
состояние с **ACQUIRE**-ordering; wake-сайт публикует predicate-
affecting состояние с **RELEASE**-ordering ДО `nova_sched_wake`.
Это индустриальный паттерн: POSIX `pthread_cond_wait` + caller-loop,
C++ `std::condition_variable::wait(lock, pred)`, Go `gopark(unlockf)`,
tokio `Notify::notified()`. Existing sites: sleep-park
(`_nova_sleep_via_libuv`) и blocking-offload (`nova_blocking_offload`)
обновлены в Plan 83.4.1; bare `nova_sched_park` остаётся для
legacy/caller-loop сценариев (channels park_with_unlock + `BaseWaiter.
fired` recheck по Plan 44.1 R2 C6).

**2. Wake idempotent.** Повторный wake без park'а между ними — no-op.
Это упрощает callback'и: libuv `uv_close` cleanup может вызвать wake
после нормального wake, нужно быть устойчивыми.

**3. Wake безопасен из libuv-callback'а.** Callback'и выполняются в
`uv_run` под main-thread. В этот момент никакой fiber не resume'ен —
ставить `parked[slot] = false` безопасно без atomic-операций.

**4. Scheduler skips parked.** `nova_supervised_step` пропускает
`parked[i]` slot'ы (но считает их alive, чтобы scheduler не выходил
раньше времени). Когда нет ready-fiber'ов и есть parked — main-loop
будет уходить в `uv_run UV_RUN_ONCE` (Plan 22 Ф.4 добавит этот path).

**5. Cancel-during-park — sync/async stop_cb contract (Plan 22 Ф.8).**

Любая операция, паркующая fiber, **обязана** зарегистрировать handle
через `nova_sched_register_pending`. stop_cb возвращает `NovaStopMode`:

**SYNC** — handle полностью cleaned после stop_cb return. Используется
когда cleanup synchronous (отвязать waitlist-node, освободить buffer):
1. Вызывается `stop_cb(handle)` → cleanup inline, возвращает SYNC.
2. `cancel_all_pending` сразу делает `parked[slot] = false` —
   fiber resume'ится на ближайшем `supervised_step`.
3. Fiber видит `scope->cancel_requested == true` → throw `"scope cancelled"`.

**ASYNC** — stop_cb лишь **инициировал** close, wake придёт от backend
(uv close_cb / waitlist callback). Используется когда cleanup
asynchronous (uv_close на handle с close_cb, uv_cancel на request):
1. Вызывается `stop_cb(handle)` → инициирует close, возвращает ASYNC.
2. `cancel_all_pending` **НЕ** unpark'ает — fiber остаётся parked.
3. Backend выполняет cleanup → callback fires → ставит final state +
   `nova_sched_wake(scope, slot)`.
4. Fiber resume'ится, видит `cancel_requested` → throw.

Это **единственный** способ корректно прервать blocking-операцию.
Плата за нерегистрацию — fiber виснет навсегда при cancel.

**Use-cases:**

| Backend | Mode | Reason |
|---|---|---|
| Sleep (uv_timer_t) | ASYNC | uv_close требует close_cb pass |
| Channel waitlist | SYNC | отвязка node inline, no async cleanup |
| Socket read (uv_tcp_t) | ASYNC | uv_read_stop + uv_close → close_cb |
| File read (uv_fs_t) | ASYNC | uv_cancel async на request |

**6. Multiple pending per slot — запрещено в bootstrap.** Slot держит
один (handle, stop_cb) — достаточно для всех known use-cases (один
fiber = одна блокирующая операция в момент времени). Если будущая
операция потребует multi-handle (например `select` на N receiver'ах) —
расширение через `pending_handle_list[]` со cap'ом.

### Контракт пользователя API

**Любая** операция, использующая park/wake, следует паттерну:

```c
NovaXxxState st = { ... };
nova_xxx_init_handle(&st.handle);

/* (1) Регистрация для cancel-wake — ОБЯЗАТЕЛЬНО ПЕРЕД park'ом. */
nova_sched_register_pending(_nova_active_scope, _nova_active_slot,
                             &st.handle, _nova_xxx_stop_cb);

/* (2) Park: scheduler не resume'ит, пока кто-то не вызовет wake. */
nova_sched_park(_nova_active_scope, _nova_active_slot);
/* ← control возвращается сюда после wake (callback либо cancel). */

/* (3) Cleanup + cancel-check. */
nova_sched_unregister_pending(_nova_active_scope, _nova_active_slot);
if (st.handle_active) {
    nova_xxx_close_handle(&st.handle);
}
if (_nova_active_scope && _nova_active_scope->cancel_requested) {
    nova_throw(nova_str_from_cstr("scope cancelled"));
}
```

Воспроизводится для:
- **Plan 22 Ф.4**: `Time.sleep` → `uv_timer_t` + `uv_timer_stop` stop_cb.
- **Plan 21 Ф.1+**: `Channel.recv`/`send` → waitlist node + waitlist-remove stop_cb.
- **Plan 83.12 `std/net`** ✅: `TcpStream.read_bytes` → `uv_read_start` + wake из `_tcp_read_cb`. See [D370](08-runtime.md#d173-stdnet--async-tcpudp-socket-stdlib-via-libuv).
- **Plan 44+ `std.fs`**: `File.read` → `uv_fs_t` + `uv_cancel` stop_cb.

### Почему

Без D93 каждый блокирующий primitive писал бы свою park/wake логику.
В bootstrap'е до Plan 22 sleep делал busy-yield ([D71](#d71-bootstrap-concurrency-runtime),
секция Time.sleep), `Channel.recv` — busy-spin на буфере. Cancel
работал через cooperative yield-check в `nova_fiber_yield` — это
терпимо для busy-yield, но **не работает** при настоящем park'е (на
yield-point нет потому что fiber suspend'ит на libuv handle).

D93 фиксирует **единый mechanism**:
- Park = выход из ready-queue.
- Wake = возврат в ready-queue.
- Cancel = generic stop_cb, прерывает любой pending handle.

Любая будущая блокирующая операция через тот же contract = автоматически
cancel-aware, автоматически CPU-idle при ожидании, автоматически
интегрируется с event loop. Это **revolutionary** изменение —
unifies сейчас раздроблённые blocking-mechanism'ы.

### Связь

- [D14](#d14-fiber-runtime--невидимая-инфраструктура) — fiber-runtime
  обоснование. Park/wake — implementation primitive для невидимого Async.
- [D50](#d50-concurrency-model-spawn-detach-blocking) — `Blocking`
  effect; D93 не покрывает (Blocking использует OS-thread pool, не park).
- [D71](#d71-bootstrap-concurrency-runtime) — bootstrap scheduler.
  D93 — расширение D71 park-state. Update'ится в Plan 22 Ф.6 с указанием
  на D93 как точку перехода с busy-yield на event-loop driven.
- [D75](#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном)
  — `supervised(cancel:)`. D93 описывает как cancel прерывает
  blocking-операции через generic stop_cb mechanism (вместо
  cooperative yield-check).
- [D79](#d79-channels--coordination-между-fiber-ами),
  [D91](#d91-channel-revision--capability-split-на-sender--receiver) —
  Channel `recv`/`send` будут реализованы через D93 API в Plan 21.
- [D80](#d80-handler-scoping-per-fiber) — per-fiber handler scoping.
  D93 park/wake не меняет handler state (snapshot уже per-fiber).

### Эволюция

- **Pre-Plan 22:** sleep, channel recv — busy-yield либо busy-spin.
  Cancel — cooperative через `nova_fiber_yield` re-check.
- **Plan 22 Ф.3:** введён D93 API. NovaFiberQueue расширен `parked[]`,
  `pending_handle[]`, `pending_stop_cb[]`. cancel_token_cancel
  итерируется по pending_stop_cb. Stop_cb тип — `void (*)(void*)`,
  unpark всегда synchronous после stop_cb (предположение).
- **Plan 22 Ф.4:** Time.sleep переходит на D93 (`uv_timer_t` park-on-timer).
  `nova_supervised_run` расширяется: idle → `uv_run UV_RUN_ONCE`.
  Sleep close-wait через ms-busy `uv_run NOWAIT` loop (~1-2 iter).
- **Plan 22 Ф.7:** sched_state arrays → heap-allocated с capacity-doubling.
  NOVA_SCOPE_CAP cap ушёл.
- **Plan 22 Ф.8:** stop_cb тип расширен — возвращает `NovaStopMode`
  enum `{SYNC, ASYNC}`. Sleep stop_cb теперь ASYNC: stop_cb инициирует
  uv_close, wake приходит из close_cb (не synchronous из cancel_all_pending).
  Это убирает ms-busy close-wait loop из sleep'а (R7 «no busy-loops
  anywhere» полностью enforced). Channel waitlist (Plan 21) — SYNC.
- **Plan 21:** Channel.recv/send переходят на D93 (waitlist + SYNC stop_cb).
- **Plan 83.12 (std/net)** ✅: socket-read/write/connect/accept — ASYNC stop_cb; std.fs — Plan 18+.
- **Plan 44 (M:N):** park/wake становится cross-worker. Wake может
  идти из worker B в fiber на worker A через `uv_async_t` queue.

### Bootstrap-status

- ✅ **Header-only API** в `nova_rt/sched.h`.
- ✅ **NovaFiberQueue** расширен `parked[]`, `pending_handle[]`,
  `pending_stop_cb[]` (Ф.7: heap-allocated с capacity-doubling).
- ✅ **nova_supervised_step** skips parked.
- ✅ **nova_cancel_token_cancel** проходит по pending_stop_cb.
- ✅ **Sync/async stop_cb contract** (Ф.8) — `NovaStopMode` enum,
  cancel_all_pending различает SYNC (unpark immediate) vs ASYNC
  (ждёт backend wake).
- ✅ **Time.sleep** через D93 (Ф.4 register/park; Ф.8 ASYNC close_cb wake).
- 🟡 **Channel waitlist** (Plan 21) — SYNC stop_cb, ждёт реализации.
- ✅ **std/net IO** (Plan 83.12) — ASYNC stop_cb, реализован [D370](08-runtime.md#d173-stdnet--async-tcpudp-socket-stdlib-via-libuv).
- 🟡 **std/fs IO** (Plan 18+) — ASYNC stop_cb, ждёт реализации.

---

## D92. Top-level `main` как implicit supervised scope

> **Введён:** Plan 22 Ф.5 (2026-05-11).
> **Реализация:** `compiler-codegen/src/codegen/emit_c.rs` (emit_main_wrapper)
> + `nova_rt/fibers.h` (nova_supervised_drain_main_scope).

### Что

Каждый `fn main()` codegen'ится с **implicit supervised scope** —
`NovaFiberQueue _nova_main_scope` обёрнутый вокруг user-body. Это
унифицирует runtime-семантику: внутри main user-code всегда имеет
`_nova_active_scope != NULL`, как любая функция внутри supervised-блока.

### Правила

**Правило 1 — `_nova_active_scope` всегда non-NULL в user-code.** Все
блокирующие операции (Time.sleep, Channel.recv, IO) опираются на это
для park/wake API ([D93](#d93-park-wake--нормативный-runtime-primitive-для-блокирующих-операций)).

**Правило 2 — drain до quiescence.** Main-body завершается → emit_main
вызывает `nova_supervised_drain_main_scope(&_nova_main_scope)`. Этот
drain работает пока есть alive fiber'ы:
- Detach-fiber'ы ([D50](#d50-concurrency-model-spawn-detach-blocking))
  доработают.
- Pending libuv-handle'ы (Plan 22 Ф.4 sleep'ы) отстреливают callback'и.
- Все fiber'ы пробуждённые callback'ами после main-body завершаются.

После quiescence — `nova_evloop_close()` → `nova_gc_shutdown()` → `return 0`.

**Правило 3 — error propagation.** Throw в main-body → propagates как
обычно (через D85 `Fail` mechanism, либо panic). Throw в detach-fiber
**после** main-body — **logged to stderr**, но процесс завершается
exit code 0. Это согласовано с D50 fire-and-forget семантикой detach'а:
detach не имеет owner для re-throw, и abort процесса из-за detach-error
неприемлем (другие detach'ы могут быть корректны).

**Правило 4 — `exit(code, msg)` bypass'ит drain.** D13 `exit()` гасит
процесс **немедленно**, без drain, без cleanup'ов. Это согласовано с
D90 §8 (`exit` обходит defer'ы): catastrophic shutdown, не graceful.

**Правило 5 — `detach` в top-level кладёт fiber в main-scope.** До D92
top-level detach был `SyncDetach` (inline-исполнение). После D92 — fiber
в implicit main-scope, доживёт до drain'а. **Это поведенческое изменение,
breaking change** для кода полагавшегося на inline'ность top-level detach.

**Правило 6 — РЕТРАКТИРОВАНО (Plan 221.1 №108, 2026-07-25).** Было:
`_nova_active_slot = -1` означает main-flow — slot −1 не индексирует
fiber-array, park/wake API не работает (main-flow не может park'нуться
через `mco_yield` — нет coroutine'ы), top-level `Time.sleep` шёл через
busy-yield (`supervised_step`), блокирующий `TcpListener.accept()`
(и любой другой D93 park-based primitive) ПРЯМО в `main()` крашил
`nova_sched_park: invalid scope/slot` — «вторая дверь» на одну и ту же
семантику (park/wake), диагноз владельца: «выглядит как несколько дверей
на одно и то же».
Стало: **main-body исполняется КАК ФАЙБЕР** — полноценный слот
планировщика (mco-coroutine), а не сырой C-поток. `emit_main_wrapper`
спавнит его в implicit main-scope и гоняет планировщик до завершения
(`nova_supervised_run`). `_nova_active_slot >= 0` для user-кода на всём
протяжении main — park/wake (D93) работает напрямую: блокирующий
`accept()`/`Time.sleep()`/`Channel.recv()` и т.п. легальны ПРЯМО в
`main()`, без обёртки `supervised { spawn { … } } }`. Ошибки main-body
пробрасываются как обычно (Правило 3 не изменилось — top-level всё ещё
без fail-frame, необработанная ошибка всё ещё `abort()`'ит с тем же
диагностическим сообщением, что и раньше).

**Спавн-путь — ТОЛЬКО bootstrap, НИКОГДА armed (амендмент к амендменту,
интегратор-гейт 2026-07-25, мега-CU 577/5, все 5 TIMEOUT).** Первая версия
этого фикса спавнила main-fiber ТЕМ ЖЕ dual bootstrap/armed-M:N путём, что
`emit_spawn` — `nova_runtime_is_initialized() ? nova_runtime_spawn_into(...)
: nova_fiber_spawn_into(...)`. ОШИБКА: `nova_runtime_auto_arm()` (несколько
строк выше в `int main()`, unconditional, D138 default-on) взводит
`_armed = true` ДО этой точки — `nova_runtime_is_initialized()` здесь
ВСЕГДА true, так что main-body ВСЕГДА пушился в WORKER-THREAD deque, а не
исполнялся на настоящем главном OS-потоке. Это молча превращало КАЖДЫЙ
top-level `supervised{}`/`detach{}`/cross-effect-throw/test-runner-chunk
ЛЮБОЙ программы в «вложенный supervised на worker-потоке» с точки зрения
рантайма (`_nova_on_worker_thread()` — thread-identity-based) — другой,
более узкий кодопуть (cooperative `nova_runtime_worker_pump_scope` вместо
plain main-thread `uv_run`-ожидания, watchdog отключён, другие допущения
о thread-affinity в orphan-scope/signal_main). Симптом: 4 из 5
таймаутов (top-level detach ×2, folder-CU test-runner на сотнях чанков,
multierror-scope) — зависания без крашей, ровно сигнатура «тихо попал не
в ту дверь». Фикс: main-fiber ВСЕГДА `nova_fiber_spawn_into` (bootstrap,
пришпилен к ВЫЗЫВАЮЩЕМУ OS-потоку — истинному процесс-main), НЕЗАВИСИМО
от process-wide armed/unarmed состояния. Не потеря конкурентности: дети,
которых user-код спавнит/detach'ит ИЗНУТРИ main-body, по-прежнему проходят
ОБЫЧНОЕ armed/bootstrap решение в СВОИХ СОБСТВЕННЫХ точках `spawn{}`/
`detach{}` (`emit_spawn`/`emit_detach`, не тронуты) — пришпилен только
хостинг САМОГО main-body, так что каждое существующее допущение рантайма
«я на главном потоке или на worker'е» видит ровно то же, что видело до
№108.

**D61 cross-effect handler-arm routing — main-fiber = main-flow-эквивалент
для этого gate'а (тот же гейт-раунд).** 5-й таймаут-файл (`repro_cross_
effect_throw`) после спавн-фикса выше перестал висеть, но начал давать
НЕВЕРНОЕ значение (не таймаут). Корень: `nova_interrupt`/`nova_interrupt_
ptr` (effects.c, D61 Plan 61 followup #1) гейтуют cross-effect
handler-arm fast-path (throw ДРУГОГО типа изнутри handler-arm должен
skip'нуть направо к OWNER-у, минуя текущий handler) условием
`!mco_running()` — раньше корректный proxy «точно main-flow, точно один
стек» (top-level ВСЕГДА исполнялся на сыром потоке, никогда в coroutine).
№108 сделал main body ВСЕГДА `mco_running()!=NULL` — proxy перестал
соответствовать своему исходному свойству ДАЖЕ для plain top-level кода
без единой `spawn`/`supervised`-границы. Фикс: `_nova_main_fiber_co`
(effects.h/effects.c, `void*` — не видит `mco_coro`, чтобы не тянуть
minicoro.h в effects.h) — `mco_coro*` корневого main-fiber'а, ставится/
чистится в `_nova_main_fiber_entry`. Гейт заменён на
`nova_cross_effect_route_safe()` = `!mco_running() || mco_running() ==
_nova_main_fiber_co` — восстанавливает исходное свойство («точно
main-fiber, без риска cross-fiber stack») для main-hosted кода, оставляя
ГЕНУИННО spawn'нутые/detach'нутые дети (где риск cross-stack longjmp
реален) на старом, безопасном deferred-`interrupt_pending`-пути.

Стек main-файбера НЕ выделен
особым образом — тот же arena-slot (`NOVA_FIBER_STACK` env /
`nova.toml [runtime].fiber_stack`, builtin-default 4MB), что у любого
другого fiber'а: (а) держит main внутри Boehm-видимого arena-range
(бespoke больший стек вне арены потребовал бы отдельного `GC_add_roots`
— лишний движущийся элемент ради спорной выгоды); (б) 4MB-дефолт уже
щедрее типичного OS-стека главного потока на большинстве платформ; (в)
единая ручка `NOVA_FIBER_STACK` для «насколько глубоко Nova-код может
рекурсировать» — что в `spawn{}`, что в `main()` — без бимодального
сюрприза. Если глубокий top-level `main()` реально упрётся в лимит —
отдельный `NOVA_MAIN_FIBER_STACK`-оверрайд заводится ТОГДА (YAGNI сейчас).
Второй дверь снесена (busy-yield main-flow ветка `time_sleep_ms`'s
`else if (_nova_active_scope)` теперь структурно недостижима для
top-level sleep — `mco_running()` всегда non-NULL внутри main-body).

**Правило 7 (future, не реализуется в Plan 22):** SIGINT/Ctrl+C через
`uv_signal_t` отменяет main-scope cancel-token, fiber'ы получают
cooperative cancel. Optional extension, отдельный план если потребуется.

### Семантика codegen

`emit_main_wrapper` эмитит (Plan 221.1 №108, Правило 6 ретракция —
main-body теперь спавнится ФАЙБЕРОМ в implicit main-scope вместо
прямого вызова на сыром C-потоке; `_nova_main_fiber_entry` — hand-
transcribed копия `emit_spawn`'s generated entry-fn, `NovaSpawnCtxBase`
без доп. полей; ВСЕГДА bootstrap — см. интегратор-гейт followup выше,
`nova_runtime_spawn_into` для main конкретно бьёт thread-identity
допущения по всему рантайму):

```c
static void _nova_main_fiber_entry(mco_coro* _co) {
    NovaSpawnCtxBase* _c = (NovaSpawnCtxBase*)mco_get_user_data(_co);
    _nova_main_fiber_co = (void*)_co;  /* D61 cross-effect gate, см. выше */
    _c->_nova_worker_slot = -1;  /* всегда bootstrap — nova_supervised_step владеет slot'ом */
    NovaFailFrame _ff;
    nova_fail_push(&_ff);
    if (setjmp(_ff.jmp) == 0) {
        nova_fn_main_impl();
        nova_fail_pop();
    } else {
        /* kinded error report — local path only (никогда remote/worker) */
        ...
    }
    _nova_main_fiber_co = NULL;
    /* нет epilogue: bootstrap-fiber'ы не инкрементят pending_remote */
}

int main(int argc, char** argv) {
    nova_gc_init();
    nova_evloop_init();
    /* effect-storage registration ... */

    /* D92: implicit main-scope. */
    NovaFiberQueue _nova_main_scope;
    nova_scope_init(&_nova_main_scope);
    _nova_active_scope = &_nova_main_scope;
    _nova_active_slot  = -1;
    nova_evloop_install_sigint(&_nova_main_scope);

    /* Plan 221.1 №108: spawn main-body as a real fiber into the scope,
     * then drive the scheduler until it completes — `_nova_active_slot
     * >= 0` for the ENTIRE duration of user code, so D93 park/wake works
     * directly. ALWAYS `nova_fiber_spawn_into` (bootstrap, pinned to THIS
     * calling — the true process main — OS thread), NEVER
     * `nova_runtime_spawn_into` (would push main-body onto a worker
     * thread's deque — see the gate-red followup note above). */
    NovaSpawnCtxBase* _nova_main_ctx = (NovaSpawnCtxBase*)nova_alloc(sizeof(NovaSpawnCtxBase));
    _nova_main_ctx->_nova_parent_scope = NULL;
    nova_fiber_spawn_into(&_nova_main_scope, _nova_main_fiber_entry, _nova_main_ctx);
    nova_supervised_run(&_nova_main_scope);

    /* D92: drain detach'ов / pending fiber'ов до quiescence (Правила
     * 1-5 неизменны — detach top-level идёт через отдельный orphan-scope,
     * не через _nova_main_scope; этот вызов остаётся belt-and-braces —
     * идемпотентен, nova_supervised_run уже полностью дренировал ЭТУ
     * scope-очередь, единственный ребёнок которой — сам main-fiber). */
    nova_supervised_drain_main_scope(&_nova_main_scope);

    _nova_active_scope = NULL;
    _nova_active_slot  = -1;

    nova_evloop_close();
    nova_gc_shutdown();
    return 0;
}
```

### Почему

До D92 top-level main не имел scope — `_nova_active_scope = NULL`.
Это создавало корзину edge cases:
- `Time.sleep` на top-level → kernel-blocking (Plan 22 Ф.4 не мог
  использовать park/wake без scope).
- `detach` на top-level → inline execution (SyncDetach), не настоящий
  fire-and-forget.
- IO operations (Plan 83.12 `std/net` ✅, Plan 18+ `std/fs`) на top-level — не
  работают через park/wake API, требовали бы special-case.

D92 устраняет эти edge cases одним решением: main всегда внутри scope.
User-code не видит разницы (семантика sleep / detach / IO одинакова
из любого контекста). Runtime simplifies — нет двух кодопутей для
fiber-context vs main-context.

### Что отвергнуто

**(a) Эволюция D71 без нового D-блока.** Изменение значимое —
behavioural breaking change для detach. Заслуживает отдельного
D-номера для discoverability.

**(b) Не оборачивать main в scope, оставить top-level kernel-blocking.**
Это сохранило бы простоту, но рассыпает Plan 22 Ф.4 цель — единый
event-loop driven scheduler. Под Plan 83.12 (`std/net` ✅) и Plan 18 (std.fs+)
все IO operations требовали бы special-case для top-level. Нежелательно.

**(c) Implicit scope с full `nova_supervised_run` (re-throw fiber-errors).**
Re-throw на main-flow после main-body завершён = abort. Detach-fiber
throw'ы в D50 fire-and-forget — должны быть logged, не abort. Поэтому
**drain-no-throw** variant (`nova_supervised_drain_main_scope`).

### Связь

- [D13](08-runtime.md#d13) — `panic` / `exit` семантика. D92
  Правило 4: `exit()` bypass'ит drain.
- [D50](#d50-concurrency-model-spawn-detach-blocking) — `detach`
  fire-and-forget. D92 Правило 3 + 5: detach-throw logged not abort'ed.
- [D71](#d71-bootstrap-concurrency-runtime) — bootstrap scheduler.
  D92 расширяет: main всегда в scope.
- [D75](#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном)
  — `supervised(cancel:)`. D92 Правило 7 (future): SIGINT через
  main-scope cancel.
- [D90](03-syntax.md#d90) — `defer` / `errdefer`. D92 Правило 4:
  `exit()` обходит defer'ы (согласовано с D90 §8).
- [D93](#d93-park-wake--нормативный-runtime-primitive-для-блокирующих-операций)
  — park/wake API. D92 обеспечивает `_nova_active_scope != NULL` в
  user-code, что необходимо для park/wake.

### Bootstrap-status

- ✅ Codegen `emit_main_wrapper` оборачивает в implicit scope.
- ✅ Runtime `nova_supervised_drain_main_scope` drain до quiescence.
- ✅ Detach behavior change verified (no regression в `detach_test.nv`).
- ✅ **Plan 221.1 №108 (2026-07-25): main-body = файбер.** Правило 6
  ретрактировано — `_nova_main_fiber_entry` спавнится в implicit
  main-scope ВСЕГДА bootstrap-путём (`nova_fiber_spawn_into`, пришпилен
  к вызывающему — истинному процесс-main — OS-потоку; НИКОГДА
  `nova_runtime_spawn_into`, см. интегратор-гейт followup выше — armed
  path пушил main-body на worker-поток, молча меняя thread-identity для
  ЛЮБОГО top-level `supervised{}`/`detach{}` в программе), `nova_
  supervised_run` гоняет планировщик до завершения.
  `_nova_active_slot >= 0` для user-кода всю жизнь main — блокирующий
  `TcpListener.accept()`/`Time.sleep()`/`Channel.recv()` ПРЯМО в
  `main()` (без `supervised { spawn { … } } }`) работает штатно через
  D93 park/wake. Busy-yield main-flow ветка `time_sleep_ms`'s
  `else if (_nova_active_scope)` структурно недостижима для top-level
  sleep теперь (`mco_running()` всегда non-NULL). D61 cross-effect
  handler-arm routing (`nova_interrupt`/`nova_interrupt_ptr`) исправлен
  симметрично — `_nova_main_fiber_co` даёт main-fiber-hosted коду тот же
  fast-path, что раньше давал `!mco_running()`. Фикстуры:
  `spec_tests/conformance/standalone/m2211_108_main_fiber_accept.nv`,
  `m2211_108_main_fiber_sleep.nv`.
- 🟡 **SIGINT handler** (Правило 7) — future extension, НЕ пересмотрено
  этим окном: `nova_evloop_install_sigint(&_nova_main_scope)` по-прежнему
  целится в внешний `_nova_main_scope`, а не в тот момент, когда
  main-fiber реально паркуется внутри своего собственного слота — та же
  зона неполноты, что и раньше (не хуже, просто не решена).

---

## D94. `select { ... }` — multiplexed channel operations

> **Введён:** Plan 31 (2026-05-11). **Статус:** ✅ реализован (2026-05-11),
> ✅ hardening Plan 44.1 Ф.3 (2026-05-12).
> **Уточняет** [D79](#d79-channels--coordination-между-fiber-ами) —
> финализирует синтаксис и семантику `select`.
>
> **Реализованный синтаксис** (bootstrap, Plan 31):
> - `Some(v) = rx => { }` — recv с binding
> - `_ = rx => { }` — recv wildcard (срабатывает на Some и None/closed)
> - `tx.send(val) => { }` — send arm
> - `Some(v) = rx if guard => { }` — recv с guard
> - `_ => { }` — default (non-blocking)
>
> **Bootstrap-ограничения:**
> - `None = rx => { }` — отдельный arm для закрытого канала **не** введён;
>   используйте `_ = rx => { }` (wildcard срабатывает на Some и на None/closed)
>   + `match` внутри тела arm'а для дифференциации, либо `rx.is_closed()`
>   после recv'а.
> - `Some(v) = rx` arm на already-closed канале **не** срабатывает —
>   только wildcard `_ = rx` ловит closed-state. См. Plan 31 §«Отличия от spec».
>
> **Реализовано в полной форме (Plan 31 Ф.6, Plan 44.1 Ф.2/Ф.3):**
> - Panic «select: all channels closed» при all-closed без default — ✅
>   (Plan 31 Ф.6; работает и в main-thread context'е через pre-check).
> - `ChanReader.close_after(Duration)` timer cleanup при non-winning arm — ✅
>   (Plan 44.1 Ф.2 B7: `on_select_lost` callback + idempotent `cancelled`
>   flag на `NovaAfterState`; reused by Plan 65 close_after API).
> - `Channel.new(0)` — explicit panic «capacity must be >= 1` **перед**
>   allocate'ом (Plan 44.1 Ф.3 B9, без leak'а на throw).
> - **Adaptive per-call storage без cap'а на arm count** (Plan 44.1
>   Ф.3 B5): codegen эмитит `SelectSlot _arms[n_ch]; SelectWaiter
>   _waiters[n_ch];` на стеке fiber'а через compound literal (literal
>   size, MSVC-compatible — не VLA). `nova_select_try_immediate`
>   использует `alloca(n*sizeof(int))` для внутреннего shuffle order.
>   Stack frame ~84n байт. На default minicoro 56 KB stack ≈ 600+ arms
>   безопасно. Идиоматический Go код = 2-8 arms; cap'а нет.
>
> **Plan 44.1 Ф.1 (2026-05-12, ✅ Этапы 1-6 закрыты — production-grade M:N
> prerequisites):**
> - Atomics + mutex на shared state (B1): writer_count/closed/reader_closed
>   atomic; head/count/waiter-lists под mutex. Все ops lock-then-mutate.
> - Refcount idiom Release-dec + Acquire-fence-on-zero (A1, Arc::drop pattern).
> - Race-free select wake через `selectdone` CAS (B2) — unified protocol
>   для recv/send/select waiters. Direct-copy sender→waiter (Go's sendDirect
>   equivalent).
> - Doubly-linked waiter list O(1) unlink (T2).
> - BaseWaiter common prefix (C1, strict-aliasing safe).
> - stop_cb lock-free contract (C2, atomic cancelled flag).
> - `nova_sched_park_with_unlock` API (C6, lost-wakeup-free park).
> - Cache padding by access group (C5, 300× perf win под contention).
> - TOCTOU re-check protocol (A2).
> - Symmetric `nova_chan_reader_close` (R1 B2, Tokio Receiver::close parity).
> - All-arms-disabled panic (C3, не silent forever-park).
> - Linux Docker validation infrastructure + pthread stress tests
>   (b1_mutex_stress/b2_selectdone_cas/t2_waiter_churn) под TSan/ASan/UBSan.
>
> **Tier 1 toolchain backends (sync.h):**
> - Linux x86_64 (Ubuntu 22.04+, glibc 2.35+) — pthread + ADAPTIVE_NP.
> - Windows + clang LLVM 15+ — SRWLOCK native.
> - macOS arm64 + Apple Clang — os_unfair_lock (40% faster than pthread).
> - Atomics: `__atomic_*` GCC/Clang builtins.
>
> **Что отложено в Plan 44.2+ / Plan 50+:**
> - `oneshot::channel<T>` / `watch::channel<T>` / `broadcast::channel<T>` —
>   Tokio type variants (Plan 44.2).
> - `recv_many` batch API (Ф.4 follow-up).
> - Lock-free SPSC flavor (Plan 50+, Loom-verified).
> - Loom/CDSChecker formal verification (Plan 50+).
> - NUMA-aware allocation (Plan 50+ multi-socket servers).
> - Priority inheritance mutex (RT scheduling, doc only сейчас).
> - Zero-capacity rendezvous channels (`Channel.new(0)` — cap=0 case).
> - Per-channel metrics (`NOVA_CHANNEL_METRICS=1` opt-in).
>
> **Plan 44 (M:N runtime) integration** — Plan 44.1 Ф.1 = prerequisite,
> теперь готов. M:N runtime отдельный план; этот блок channels гарантирует
> что под M:N scheduler'ом channel layer thread-safe.

### Что

`select` ожидает сразу несколько channel-операций, пробуждается по
первому готовому arm'у.

```nova
select {
    Some(v) = rx1.recv()    => { process(v) }
    Some(v) = rx2.recv()    => { process(v) }
    None    = rx1.recv()    => { break }           // rx1 закрылся
    _       = tx.send(val)  => { /* sent */ }      // send arm
    default                 => { /* non-blocking */ }
}
```

**Грамматика:**

```
select-expr  = 'select' '{' NL* select-arm+ '}'
select-arm   = channel-arm | default-arm
channel-arm  = pattern '=' (recv-op | send-op) guard? '=>' arm-body NL*
recv-op      = expr '.' 'recv' '(' ')'
send-op      = expr '.' 'send' '(' expr ')'
guard        = 'if' expr
default-arm  = 'default' '=>' arm-body NL*
arm-body     = block | stmt
```

Синтаксис `pattern = rx.recv()` согласуется с `while let Some(v) = rx.recv()`
(уже в языке). Оператор `<-` не вводится (отвергнут).

### Timeout через `ChanReader.close_after(Duration)`

Специального `timeout` arm'а нет — timeout через обычный recv arm:

```nova
ro t = ChanReader.close_after(Duration.from_secs(1))     // ChanReader[()] закрывается через 1 сек
select {
    Some(v) = rx.recv()  => { process(v) }
    None    = t.recv()   => { log_idle() }    // timeout сработал
}
```

`ChanReader.close_after(d Duration) -> ChanReader[()]` —
capability-split static constructor в stdlib/concurrency.
Select не знает про "timeout" специально. **Plan 65 revision**:
ранее API назывался `Time.after(int ms)` (bare int, без type
safety); переименован и переведён на `Duration` для D91 capability
namespace + строгой type safety. Migration tool
`cargo run --bin migrate_plan65 -- --apply` автоматически переводит
literal arguments.

### Правило

1. **Guard evaluation** — `if <expr>` после паттерна делает arm disabled если false.
2. **Immediate check** — проверяет все enabled arms в псевдослучайном порядке
   (Fisher-Yates). Если ≥1 ready — выполняет без park'а.
3. **Park** — если ни один не ready и нет `default`: регистрирует waiter для
   каждого arm, паркует fiber.
4. **Wake** — первый готовый arm будит fiber; остальные waiters unlinked.
   `done`-флаг предотвращает double-wake при одновременной готовности.
5. **Fairness** — Fisher-Yates shuffle на каждой итерации (нет starvation).
6. **`default`** — если присутствует: шаг 2 всегда succeeds (не паркуем).
7. **Все каналы закрыты + нет default** → panic "select: all channels closed".
8. **cancel** (`tok.cancel()` от `supervised(cancel:)`) — отменяет все
   pending waiters, fiber просыпается, проверяет `cancel_requested`.

### Arm guards

```nova
select {
    Some(v) = rx.recv() if v > 0 => { process(v) }   // arm активен только если v > 0
    Some(v) = rx.recv()           => { skip(v) }
}
```

Guard — pre-condition (arm disabled если false). Аналог `if` в Rust Tokio `select!`.
Go не поддерживает guards в select.

### Почему

1. **Ключевой primitive для fan-in.** Без select нельзя элегантно объединить
   несколько producers в одном consumer'е.
2. **`ChanReader.close_after(Duration)` вместо `timeout(expr)`** — timeout
   как обычный channel (Go-style `time.After`, но с type-safe Duration вместо
   bare int). Нет специального синтаксиса, нет special-casing в runtime.
   **Plan 65 revision:** ранее `Time.after(int ms)` — bare int был отвергнут
   как unsafe, переименован в D91 capability namespace.
3. **`=` вместо `<-`** — согласованность с `while let Some(v) = rx.recv()`.
   Один оператор recv по всему языку.
4. **Fisher-Yates shuffle** — fairness (Go использует то же). Нет starvation
   при постоянно-готовых arms.

### Что отвергнуто

- **`<-` оператор в select** — нарушает consistency; отдельный оператор
  только для select (было в D79, удалено).
- **`timeout(expr) =>` arm** — special-casing в грамматике и runtime
  ради того, что решается обычным `ChanReader.close_after(Duration)`
  channel'ом (см. Эволюция: bare `Time.after(int)` исторический artefact,
  заменён в Plan 65).
- **Biased mode** — детерминированный выбор arm'а (Tokio `biased`).
  Достигается через `--jobs 1` + фиксированный seed в тестах.
- **Вложенный select запрещён** — излишнее ограничение; снято.

### Bootstrap-status

- ✅ Runtime: Plan 31 Ф.1 — `SelectCtx`, `SelectWaiter`, `nova_select_*` API
- ✅ Send arm: Plan 31 Ф.2
- ✅ Parser + codegen: Plan 31 Ф.3
- ✅ Arm guards: Plan 31 Ф.4
- ✅ `Time.after(d)` + тесты: Plan 31 Ф.5
   - **Plan 65 (2026-05-18) revision:** `Time.after(int ms)` removed,
     replaced by `ChanReader.close_after(Duration)`. See "Эволюция API"
     subsection below.
- ✅ All-closed panic: Plan 31 Ф.6 (с pre-check для main-thread)
- ✅ Hardening: Plan 44.1 Ф.2 (timer cleanup, formerly Time.after, now
  `ChanReader.close_after` after Plan 65) + Ф.3 (select cap=32 +
  compile-error overflow + Channel.new check ordering)
- 🟡 M:N safety: Plan 44.1 Ф.1 (atomics + selectdone CAS + doubly-linked +
  per-call storage) — отложено вместе с Plan 44 M:N runtime

### Эволюция API — timeout channel constructor (Plan 65, 2026-05-18)

`Time.after(int ms) -> ChanReader[()]` (D94 v1, Plan 31 Ф.5)
переименован в **`ChanReader.close_after(d Duration) -> ChanReader[()]`**
(D94 v2, Plan 65). Три ортогональных дефекта закрыты:

1. **Domain mismatch:** функция возвращает read-capability ChanReader,
   но жила в `Time` namespace — discoverability проседала. Capability
   namespace по D91 — `ChanReader.<constructor>`.
2. **Type safety:** bare int `(1000)` неоднозначно (мс/мкс/сек). Duration
   делает unit explicit: `Duration.from_secs(1)`.
3. **Capability mismatch D91:** получение reader через `Time.X` неявно
   подразумевало что Time владеет также writer — на самом деле runtime.

**Семантика runtime неизменна** — внутренне всё ещё `Nova_Time_after`
(libuv timer) + `on_select_lost` cleanup. Атомарный break без
deprecated alias (Plan 60 atomic-migration convention): legacy
вызов ловится диагностикой E5101 с machine-applicable fix-it
suggestion. Migration tool `migrate_plan65` автоматически переводит
literal arguments (int → from_millis, float → from_secs_f64).

Дополнительные production-grade capabilities — cancel via D75
CancelToken, mockable virtual time via `Time` effect, absolute
deadline `close_at(Instant)`, observability counters — добавляются
в Plan 65 Ф.10-Ф.14 (hardening), либо отложены в Plan 66
(periodic ticker + custom timer-wheel optimisation).


## D97. Fiber stack allocation — per-thread mmap arena (Linux/macOS)

> **Status:** active. **Ред. 3** (Plan [259](../plans/259-fiber-arena-guard-pages.md)
> Слой 1, 2026-08-09) — guard-страницы (Linux/macOS `mprotect(PROT_NONE)`)
> пробиваются **лениво, на первое использование СЛОТА**, а не все разом
> при инициализации арены. Причина — №457: eager-пробивка ВСЕХ слотов
> (16384 по умолчанию) на арену КАЖДОГО воркера умножала `mprotect`-
> вызовы (каждый берёт `mmap_lock` процесса на запись) в десятки/сотни
> тысяч на старте — сериализованный шторм системных вызовов,
> не укладывающийся в пользовательский бюджет `timeout:`, и тот же
> механизм отдельно бил в `vm.max_map_count` (`[M-187-docker-linux-
> runtime-hang]`). Инициализация арены — теперь `O(1)` syscalls
> (`mmap` + `madvise`, БЕЗ `mprotect`-цикла); guard слота пробивается
> ровно один раз, в момент, когда `nova_fiber_alloc` впервые выдаёт этот
> слот (`slot == high_water` на момент аллокации — слоты выдаются в
> порядке возрастания при первом использовании, переиспользование НЕ
> пробивает guard повторно). Число VMA процесса теперь следует за
> числом РЕАЛЬНО живших файберов, а не за `NOVA_MAX_FIBERS`; зажим
> `NOVA_ARENA_VMA_RESERVE`/`NOVA_ARENA_VMA_SAFETY_PCT` (Plan
> `[M-187-docker-linux-runtime-hang]`, лечил VMA-шторм следствием) снят
> как более не нужный. Guard-page-per-slot ИНВАРИАНТ (описанный ниже)
> не меняется — меняется только МОМЕНТ, когда конкретный guard создаётся;
> любой реально используемый слот guard имеет ровно как раньше. Windows
> arena (`fiber_arena_win.c`) уже была lazy-commit до этого амендмента
> (see «Windows» ниже) — Ред. 3 не меняет Windows-путь, только
> Linux/macOS `mmap`-путь. Полное обоснование и связь с материализацией
> пула (Слой 2) — [D451](#d451-mn--пул-воркеров-материализуется-на-старте-процесса-amends-d137d138).
>
> **Ред. 2** (Plan 82, 2026-05-22) — Windows
> переведён с calloc на `VirtualAlloc` lazy-commit arena; диагноз
> Plan 44.3 «Windows fiber arena fundamentally blocked» **опровергнут**.
> Ред. 1 (Plan 44.2 Этапы 1-3, 2026-05-12) — Linux/macOS `mmap`-arena.
> Уточняет [D14](#d14-fiber-runtime--невидимая-инфраструктура) для
> bootstrap-runtime: где живут fiber stacks и как они видны GC.

### Что

Suspended fiber stacks **не на OS-стеке** — они лежат в пользовательской
памяти, выделяемой allocator'ом minicoro. Поскольку Boehm GC сканирует
только OS-стек активного потока + явно зарегистрированные roots, fiber
stacks нужно сделать видимыми GC явно. D97 фиксирует **единую стратегию**
— per-thread large-reserve arena с lazy commit — реализованную разными
OS-примитивами по платформам:

**Linux/macOS — per-thread mmap arena с lazy commit:**

- На первое использование thread'a резервируется **8 GB virtual** через
  `mmap(MAP_NORESERVE)` — `4096 слотов × 2 MB`.
- Lazy commit: physical pages приходят только при touch'е (lazy COW).
- 4 KB **guard page** в начале каждого слота — stack overflow ловится
  через `SIGSEGV` (не silent corruption).
- Bitmap free-list для reuse слотов после fiber termination.
- **Один GC root на тред** — `[base, base + high_water * slot_size]` —
  снимает MAX_ROOT_SETS=128 ограничение Boehm.
- `madvise(MADV_DONTNEED)` после dealloc — physical memory возвращается
  ОС.
- `madvise(MADV_NOHUGEPAGE)` для guard-page granularity.
- pthread_key cleanup освобождает arena при thread exit.

**Windows — per-thread `VirtualAlloc` arena с lazy commit (Plan 82):**

- Per-thread арена — один `VirtualAlloc(MEM_RESERVE)`: 16384 слота ×
  8 MB = 128 GB виртуального резерва (нулевой commit-charge; на 64-bit
  резерв адресного пространства бесплатен).
- **Lazy commit.** Физический commit — только под minicoro-header +
  начальное окно стека у вершины слота. Рост стека — **OS-native**:
  после TIB-свопа minicoro-asm'а ядро Windows растит коро-стек штатно
  через `PAGE_GUARD`-фолт (как `CreateFiber`-стек). Декоммит
  освобождённого слота — **послотный**, при переиспользовании
  (idle-batch по 128 GB-диапазону на Windows деградирует — Plan 82 §3).
- **16 KB hard guard** (`PAGE_NOACCESS`, reserved) в начале каждого
  слота + движущаяся `PAGE_GUARD`-вершина над minicoro-header'ом. Stack
  overflow → детерминированный `STATUS_STACK_OVERFLOW` + диагностика
  «`nova: fiber stack overflow in slot N`» (паритет с Linux `SIGSEGV` —
  было silent corruption calloc-стека).
- **Atomic bitmap free-list** для reuse слотов; cross-thread dealloc
  (work-stealing migration A→B) — арена-владелец по адресу.
- **GC-видимость — `GC_set_push_other_roots`-колбэк, НЕ плоский
  `GC_add_roots`.** На Windows conservative-чтение `MEM_RESERVE`-но-не-
  `MEM_COMMIT` страницы — `STATUS_ACCESS_VIOLATION`; плоский root уронил
  бы сканер. Колбэк на mark-фазе пушит только закоммиченные диапазоны
  `[committed_low, top]` каждого живого fiber'а + native scheduler-стеки
  всех worker'ов (`GC_push_all_eager`). Число `GC_add_roots`-записей на
  fiber-арену = **0** → ограничение `MAX_ROOT_SETS=128` снято.
- Арены — heap-структуры в глобальном append-only списке; TLS хранит
  лишь указатель → арена переживает поток-владельца (нужно GC-колбэку и
  cross-thread dealloc).

**Корректировка диагноза Plan 44.3.** Ред. 1 D97 объявляла «Windows
arena fundamentally blocked»: minicoro `MCO_USE_ASM` якобы переключает
только RSP, не обновляя TIB. Plan 82 §1.1–1.2 **опроверг это**:
minicoro Windows-asm (`_mco_switch`) свопает 4 поля TIB
(`NT_TIB.StackBase`/`StackLimit`, `TEB.DeallocationStack`,
`NT_TIB.FiberData`) на **каждом** switch — ровно как corosensei /
Boost.Context. Git-археология: `minicoro.h` неизменен с 2026-05-05, а
4 провала 44.3 — 2026-05-13/14 → гипотеза «старый minicoro без
TIB-свопа» ложна. Настоящий блокер 44.3 был иным — conservative
GC-скан reserved-страниц арены (AV на первой незакоммиченной); Plan 82
решает его push-колбэком выше. SEH-unwind, `/GS`, `/guard:cf` через
arena-стек верифицированы (Plan 82 Ф.0/Ф.4); context-switch на
arena-стеке — 16–20 ns, паритет с Boost.Context (Ф.5).

### Зачем разные примитивы, единая стратегия

Со ред. 2 (Plan 82) обе платформы реализуют **одну стратегию** —
per-thread large-reserve arena с lazy physical commit, guard-page
overflow-детекцией и GC-видимостью suspended-стеков — но через **разные
OS-примитивы**, потому что семантика памяти ОС различается:

- **Linux/macOS:** `mmap(MAP_NORESERVE)` + `madvise(MADV_DONTNEED)` +
  `SIGSEGV`-handler. GC-root — плоский active-range `GC_add_roots`:
  чтение незакоммиченной `NORESERVE`-страницы даёт zero-page от ядра,
  conservative-скан fault-free.
- **Windows:** `VirtualAlloc(MEM_RESERVE)` + послотный
  `VirtualFree(MEM_DECOMMIT)` + VEH. GC-root — `push_other_roots`-
  колбэк: чтение `MEM_RESERVE`-страницы = `ACCESS_VIOLATION`, плоский
  root недопустим (Plan 82 §1.3). Колбэк пушит только закоммиченное.

Built-in minicoro `MCO_USE_VMEM_ALLOCATOR` отвергнут на **обеих**
платформах — он `MEM_COMMIT`-ит весь стек upfront (нет lazy commit).
GC-модель Windows (registry+push) **строже** Linux-овой active-range и
может быть бэкпортирована (Plan 82 Ф.6 — опциональная Linux-унификация,
gated «0 регрессий на Linux»).

### Introspection — `std.runtime.fibers`

Плакируется ([std/src/runtime/fibers.nv](../../std/src/runtime/fibers.nv)):

```nova
import std.runtime.fibers

ro virt    = fibers.virtual_reserved()  // bytes зарезервировано
ro total   = fibers.slot_count()         // 4096 Linux/macOS, 16384 Win
ro active  = fibers.slots_active()       // running fibers сейчас
ro peak    = fibers.high_water()         // peak concurrent
```

`slot_count() == 0` — honest sentinel «arena не активирована» (арена
ленивая — создаётся на первом fiber'е потока; до того статы нулевые).
Со ред. 2 арена активна на **всех трёх** платформах — нулевой
`slot_count()` больше не означает «Windows», только «поток ещё не
спавнил fiber'ов».

### Что отвергнуто

- **`MCO_USE_VMEM_ALLOCATOR` (built-in minicoro VMEM)** — commits all
  upfront; не работает с lazy commit semantics; ломает Windows budget.
- **`GC_add_roots` per-fiber** — упирается в `MAX_ROOT_SETS = 128`
  Boehm compile-time константу; нельзя bump'нуть без rebuild library
  (см. правило «не патчить сторонние библиотеки»).
- **`GC_disable` workaround вокруг scheduler tick** — был vestigial
  scaffolding в Plan 27 R4; удалён в Plan 44.2 Этап 2. Реальная защита
  приходила от single-thread cooperative invariant + arena root, не от
  disable.
- **Плоский `GC_add_roots` поверх Windows-арены** — conservative-скан
  читает root по-байтно; первая `MEM_RESERVE`-но-не-`MEM_COMMIT`
  страница → `ACCESS_VIOLATION` (на Linux безопасно — `NORESERVE`
  zero-page). Заменён `push_other_roots`-колбэком, пушащим только
  закоммиченное (Plan 82 §1.3, §5.2).
- **`MCO_USE_FIBERS` (`CreateFiber` API) для Windows** — ред. 1 D97
  называла его возможным обходом «TIB-блока». TIB-блок опровергнут
  (см. выше) → обход не нужен; `CreateFiber` к тому же не даёт
  arena-аллокатор (N независимых kernel-fiber'ов, нет lazy-commit
  контроля, нет cross-thread arena-dealloc).

### Bootstrap-status

- ✅ Arena infrastructure (Plan 44.2 Этап 1, commit `0b75bdcb06`)
- ✅ Wire-up в minicoro через `_NOVA_MCO_DESC_INIT` (Plan 44.2 Этап 1
  wire-up landing, commit `5ed208e84f`)
- ✅ Удаление `_NOVA_GC_DISABLE` (Plan 44.2 Этап 2, commit `810898de06`)
- ✅ `std.runtime.fibers` introspection (Plan 44.2 Этап 3, commit
  `f8d345e536`)
- ⏸ Linux Docker validation (Plan 44.2 Этап 4) — требует Docker daemon
- ⏸ SIGSEGV pretty handler (P41-6) — P2, отложено
- ✅ **Windows `VirtualAlloc` lazy-commit arena** (`fiber_arena_win.c`,
  Plan 82 Ф.1) — заменяет calloc-путь
- ✅ **Windows GC-интеграция fiber-стеков** — `push_other_roots`-колбэк
  (Plan 82 Ф.2); первая корректная GC-видимость fiber-стеков на Windows
- ✅ **M:N-safe arena** — cross-thread migration, multi-worker
  GC-колбэк, atomic bitmap (Plan 82 Ф.3)
- ✅ **Windows overflow-детекция** — guard-page → `STATUS_STACK_OVERFLOW`
  + VEH-диагностика (Plan 82 Ф.1); негативный тест
  `expected_runtime/fiber_stack_overflow.nv` (Plan 82 Ф.4)
- ✅ **Context-switch паритет** — 16–20 ns/switch на arena-стеке,
  класс Boost.Context (Plan 82 Ф.5)
- ⏸ Опциональная Linux-унификация на registry+push GC-модель (Plan 82
  Ф.6) — gated «0 регрессий на Linux»

## D98. Per-worker libuv loop — TLS `_nova_current_loop`

> **Правило.** Каждый OS-thread исполняющий fiber'ы имеет own
> `uv_loop_t`. Все timer / handle / I/O registrations в runtime
> (Time.sleep, Time.after, channel-select-timer, future Net/Fs)
> регистрируют libuv handles **на own loop текущего thread'а**, а не
> на global `nova_evloop()`. Discovery — через TLS `_nova_current_loop`.

### Проблема

libuv `uv_loop_t` — **thread-bound resource**. uv handles
(`uv_timer_t`, `uv_signal_t`, `uv_tcp_t`, `uv_async_t`) registered'ы
на конкретный loop; их callback'и fire'ются ТОЛЬКО когда тот loop
крутится через `uv_run`. Cross-thread callback firing — undefined.

В bootstrap N:1 ([D71](#d71)) был один thread + один loop — проблема не
существовала. Под M:N ([Plan 44](../../docs/plans/44-mn-runtime-roadmap.md))
worker thread имеет own loop ([NovaWorker.loop](../../compiler-codegen/nova_rt/runtime.c)).
Fiber на worker N park'нувшийся через `Time.sleep` создавал timer на
**main thread's loop** (через `nova_evloop()`); main thread не крутил
этот loop в синхронной точке (он либо в supervised_run, либо exit'нут);
worker N крутил own loop где timer не было. **Result:** fiber hangs
permanently.

### Решение

TLS `_nova_current_loop` (`uv_loop_t*`) — declared в
[eventloop.h](../../compiler-codegen/nova_rt/eventloop.h):

```c
#ifdef _MSC_VER
extern __declspec(thread) uv_loop_t* _nova_current_loop;
#else
extern __thread uv_loop_t* _nova_current_loop;
#endif

uv_loop_t* nova_current_loop(void);  /* TLS либо fallback на nova_evloop */
```

Set'ится:
- **Main thread**: в `nova_evloop_init()` = `_evloop` (глобальный
  default).
- **Worker thread**: в `_worker_main` (runtime.c) = `&worker->loop`
  сразу после `_current_worker_id = w->id`.

Все timer/handle creation в runtime call'ает `nova_current_loop()`:
- `_nova_sleep_via_libuv` (`fibers.h`) — fiber-context sleep.
- `_nova_time_default_sleep` (`fibers.h`) — main-flow sleep.
- `nova_supervised_run` / `nova_supervised_drain_main_scope` — idle uv_run.
- `Nova_Time_after` (`channels.h`) — select-timer.

`nova_evloop()` остаётся **только** для глобально main-thread операций:
- `nova_evloop_install_sigint` — single SIGINT handler per process.
- `nova_evloop_close` — finalize main loop в exit path.

### Fallback semantics

`nova_current_loop()` сначала проверяет TLS; если NULL — lazily set'ит
к `nova_evloop()` (default). Это покрывает:
- C-static initializer'ы что вызывают timer creation **до**
  `nova_evloop_init()`.
- Threads без `runtime.init()` (тесты что не активируют M:N).

### Ограничение D98

Fiber **pin'ится к worker'у** на котором park'нулся. Wake происходит
из close_cb на том же worker'е. Migration между workers требует
отдельной machinery (TLS state migration, handle re-registration на
target loop) — **отложено** в Plan 44.7+.

Practical implication: long-running fiber на worker A блокирует worker A
до завершения. Other workers продолжают независимо. Cooperative scheduling
работает в пределах one worker.

### Что отвергнуто

- **`nova_supervised_run(scope, loop)` параметризация через codegen** —
  early Plan 44.5 idea. Требовало menyat `emit_supervised`
  (codegen-side change) emit'ить `_nova_current_loop` в каждый call site.
  TLS-based подход transparent'но решил это без codegen изменений —
  любой call site читает TLS without API change.
- **`uv_default_loop()` per-worker** — нельзя, libuv даёт один default
  loop на process. Workers используют `uv_loop_init(&w->loop)` с **новой**
  `uv_loop_t` структурой.
- **Shared loop через mutex** — обходит изоляцию libuv, kills
  parallelism (один thread crank'ает — others ждут).

### Bootstrap-status

**Layer 3 (TLS loop) — Plan 44.5 L3 (originally Plan 44.6, re-merged):**
- ✅ TLS infrastructure (eventloop.h+c)
- ✅ `_worker_main` set TLS (runtime.c)
- ✅ Replace `nova_evloop()` → `nova_current_loop()` в fibers/channels
- ✅ Regression: 274/274 single-thread baseline сохранён
- ✅ 3 mn_runtime regression-теста PASS

**Layer 5 (implicit M:N — codegen routing) — Plan 44.5 L5 partial (2026-05-14):**
- ✅ Runtime atomics: `pending_remote` (Go's WaitGroup pattern) +
  `first_error_atomic` (Go's errgroup.errOnce pattern) + `_main_wake`
  uv_async + `nova_runtime_spawn_into` + `nova_runtime_signal_main`.
- ✅ Codegen routing: `emit_spawn` эмитит conditional
  `if (runtime_is_initialized) nova_runtime_spawn_into else nova_fiber_spawn_into`.
  Один и тот же `spawn { body }` работает single-thread и distributed
  (Go's `go func()` model).
- ✅ Cross-worker error propagation через atomic CAS на parent's
  `first_error_atomic` (first-writer-wins).
- ✅ `mn_runtime_actual_workload.nv` PASS — 16 fibers распределены на
  4 workers через `runtime.current_worker_id()` distribution
  (round-robin, не all одного worker'а).
- ✅ 278/278 PASS Windows.

**Critical fix:** `runtime.h` включён в `nova_rt.h` явно. Без этого
codegen использовал implicit-int declaration для
`nova_runtime_is_initialized` → ABI mismatch (bool vs int) → garbage
return → wrong code path → underflow `pending_remote` → infinite loop
(38 timeout'ов в первой попытке Plan 44.5 L5).

### Boehm `GC_THREADS` — обязательный client-side define (НЕ feature flag!)

> **Запомнить, чтобы не передиагностировать каждый раз.** Boehm bdwgc
> **уже собран thread-safe** — и vcpkg Windows (`build.ninja` содержит
> `-DGC_THREADS` в `DEFINES`), и `libgc-dev` Ubuntu. Никакой кастомный
> vcpkg `bdwgc[multithreaded]` feature **НЕ нужен** — это была неверная
> гипотеза.

Корень проблемы был в **клиентском** коде: `<gc.h>` прячет
`GC_register_my_thread` / `GC_unregister_my_thread` / `GC_get_stack_base`
за `#ifdef GC_THREADS`. Если клиент инклудит `<gc.h>` **без** `-DGC_THREADS`,
прототипы невидимы → worker'ы не регистрируются в GC → Boehm STW walker
пропускает их стеки → GC-объекты, на которые ссылается только worker stack,
преждевременно собираются → use-after-free / `SIGSEGV`.

**Правило (все платформы):** при сборке с Boehm GC клиент **обязан**
передать `-DGC_THREADS` (`/DGC_THREADS` для MSVC) тем же compiler invocation,
что инклудит `<gc.h>`. Это не Linux/macOS-специфично — Windows регистрирует
worker'ы точно так же.

Где зафиксировано в коде:
- [test_runner.rs](../../compiler-codegen/src/test_runner.rs) — `-DGC_THREADS` /
  `/DGC_THREADS` во всех 3 compiler-path (gcc/clang, MSVC, доп. path), рядом
  с `-DNOVA_GC_BOEHM`.
- [runtime.c](../../compiler-codegen/nova_rt/runtime.c) —
  `NOVA_GC_THREADS_REGISTER` активируется **безусловно** при `NOVA_GC_BOEHM`
  (никакого `&& defined(__linux__)` guard'а); `GC_register_my_thread` в
  `_worker_main`, `GC_unregister_my_thread` в cleanup.

Исправлено в commit `8fcbc67fddb` (Plan 44.5 Layer 5). Результат: Windows
multi-fiber `Time.sleep` перестал флакать (был ~14% segfault).

**Open:**
- ✅ Park/wake migration к worker scope (Time.sleep / Channel.recv в
  worker fiber'е) — закрыто Plan 44.5 Layer 5, commit `8fcbc67fddb`:
  TLS-swap + `nova_scope_alloc_slot` в entry preamble, `dispatch_ready`
  hook (same-thread → deque push, cross-thread → `wake_pending` +
  `uv_async_send`).
- ⏸ Linux Docker validation Plan 44.5 L5 — требует Docker daemon.


## D103. Preemption — sysmon-thread + codegen safepoints

> **Status:** active (Plan 44.7, Вариант B, закрыт 2026-05-14).
> **Note:** номер D103, а не D102 — D102 на ветке main занят «именованными
> аргументами» (Plan 46); preemption перенумерован при подготовке к sync.
> Дополняет [D71](#) (M:N прозрачность) и [D93](#d93-parkwake--нормативный-runtime-primitive-для-блокирующих-операций):
> fair CPU-sharing — часть гарантии прозрачности M:N.

### Что

CPU-bound fiber без явного `runtime.yield()` НЕ монополизирует worker
thread. Runtime автоматически вытесняет fiber'у, крутящуюся дольше
timeslice'а (~10ms), на ближайшем safepoint'е — peer fiber'ы получают CPU.

```nova
runtime.init(1)
supervised {
    spawn {
        mut i = 0
        while i < 1_000_000_000 { i = i + 1 }   // НЕ блокирует worker
    }
    spawn { Time.sleep(10); /* ... */ }          // запустится, не дождавшись
}                                                 // конца loop'а соседа
```

### Механизм

**sysmon thread** — отдельный OS-thread (аналог Go's `sysmon`), не привязан
к worker'ам. Каждые ~10ms проходит workers; если worker крутит одну fiber'у
дольше `NOVA_PREEMPT_SLICE_NS` — выставляет `NovaWorker.preempt_flag`.

**Codegen safepoints** — `nova_preempt_check()` эмитится в прологе каждой
Nova-функции и первым стейтментом тела каждого цикла. Читает живой флаг
через TLS `_nova_preempt_ptr` → при выставленном флаге кооперативно
`nova_fiber_yield()`. Стоимость на горячем пути: TLS-load + predicted-not-
taken branch (~1-2 такта). В single-thread режиме `_nova_preempt_ptr ==
NULL` → чистый no-op.

**yielded-FIFO** — вытесненный fiber кладётся в per-worker FIFO, не обратно
в LIFO-deque (иначе worker сразу re-pop'ит его, голодя peer'ов). Worker loop:
deque (свежие/разбуженные) → yielded-FIFO → steal → block.

**uv_run каждую итерацию** — worker сервисит libuv loop (`UV_RUN_NOWAIT`) на
каждой итерации, не только когда deque пуст. Без этого вытесненный CPU-fiber
держал бы deque непустым → таймеры (`Time.sleep`) никогда не fire'или бы.

### Отличие от Go — и почему так

Go использует `SIGURG` async signal + ASM `asyncPreempt`. Nova — кооперативные
codegen safepoint'ы. Причина: minicoro `mco_yield` НЕ async-signal-safe,
yield из signal handler = UB. Вариант B (safepoints) даёт **observable**
паритет — CPU-bound fiber не морит голодом соседей — за ~20% сложности
Варианта C. Непокрыто: tight loop целиком в inline-ASM/FFI без
codegen-backedge'а (нишевой кейс).

### Что отвергнуто

- **SIGURG/SuspendThread async preemption** (Вариант C) — 2-3 недели
  engineering, ASM-level, высокий риск; observable benefit над Вариантом B
  только для нишевого inline-ASM-loop кейса. См. docs/plans/44.7-preemption.md.
- **Snapshot флага в TLS перед resume** — worker застревает в `mco_resume`
  на весь CPU-loop, не может перечитать снапшот; sysmon выставляет флаг уже
  после старта fiber'ы. Поэтому `_nova_preempt_ptr` — указатель на живой
  `NovaWorker.preempt_flag`, а не копия.
- **Re-push вытесненного fiber'а в deque** — LIFO → мгновенный re-pop →
  starvation peer'ов. Отсюда отдельная yielded-FIFO.

### Файлы

- `compiler-codegen/nova_rt/runtime.c` — sysmon thread, `preempt_flag`,
  `current_fiber_start`, yielded-FIFO, `uv_run(NOWAIT)` каждую итерацию.
- `compiler-codegen/nova_rt/fibers.h` — `_nova_preempt_ptr` extern,
  `nova_preempt_check()`, `NOVA_UNLIKELY`.
- `compiler-codegen/nova_rt/effects.c` — `_nova_preempt_ptr` TLS def.
- `compiler-codegen/src/codegen/emit_c.rs` — safepoint emit в `emit_fn` +
  `emit_loop_body_inline`.
- `nova_tests/concurrency/mn_runtime_preemption.nv` — 2 positive + 2 negative.

---

## D124. Monotonic vs Timestamp — раздельные типы для wall-clock и монотонных часов

> **Введён:** 2026-05-18 (Plan 65 Ф.12 driver). **Статус:** принят;
> реализация в Plan 65 Ф.12.1-Ф.12.6. **Уточняет** существующий
> `Timestamp` (`std/time/duration.nv`) и `Time` effect (`emit_c.rs:1037-1046`).

### Что

Nova вводит **два различных типа** для представления «момента во времени»,
разделяя их по источнику clock'а:

```nova
type Timestamp { ro nanos i64 }    // wall-clock (Unix epoch nanos)
type Monotonic { ro nanos i64 }    // monotonic (process-local epoch)
```

Соответственно, `Time` effect имеет **два** метода:

```nova
Time.now()           -> Timestamp       // wall-clock: для логов, дат, сериализации
Time.now_monotonic() -> Monotonic       // monotonic: для timers, deadlines, profiling
```

Эти типы **не interconvertible** — компилятор отвергает `let t Monotonic = Time.now()`
(тип Timestamp), и наоборот. Сериализация `Monotonic` запрещена
(нет epoch, бессмысленно вне процесса).

### Правило

1. **`Timestamp`** — для **семантического времени**: логи, файлы,
   протоколы, БД, человеко-читаемые даты. Источник: `clock_gettime(CLOCK_REALTIME)`
   / `GetSystemTimeAsFileTime`. **Прыгает** при NTP-синхронизации, DST,
   manual time set. Сериализуется в Unix-epoch nanos.

2. **`Monotonic`** — для **измерения промежутков и дедлайнов**: таймеры,
   timeouts, retry, profiling. Источник: `clock_gettime(CLOCK_MONOTONIC)`
   / `QueryPerformanceCounter`. **Никогда не идёт назад**. Бессмысленно
   сериализовать (`Monotonic` одного процесса нерасшифровываема в
   другом). Сравнение `Monotonic` между процессами — compile-error.

3. **Арифметика:**
   - `Timestamp - Timestamp -> Duration` (wall-clock interval, может быть
     отрицательным при NTP backwards)
   - `Monotonic - Monotonic -> Duration` (monotonic interval, всегда ≥ 0
     если оба из same process)
   - `Timestamp + Duration -> Timestamp`
   - `Monotonic + Duration -> Monotonic`
   - `Timestamp - Monotonic` — **compile-error** «cannot subtract
     incompatible clock types»
   - `Monotonic.as_unix_secs()` — **compile-error** «Monotonic не
     представимы в Unix epoch»

4. **API контракты:**
   - `ChanReader.close_after(Duration)` — без изменений (длительность
     clock-agnostic).
   - `ChanReader.close_at(Monotonic)` — **только** Monotonic; иначе
     NTP может вызвать early/late fire (silent bug).
   - `Timestamp.from_unix_*` / `Timestamp.as_unix_*` — без изменений.
   - `Monotonic.now()` (== `Time.now_monotonic()`) — единственный
     способ construct'нуть; нет `Monotonic.from_nanos` (raw bytes
     бессмысленны).

5. **`Time` effect для тестов** (Plan 34 Ф.7 mock_clock):
   - Mock-handler **должен реализовать обоих** `now()` и
     `now_monotonic()` для consistency. Default mock: `now() == EPOCH +
     elapsed_virtual`, `now_monotonic() == elapsed_virtual` (от старта
     mock scope).

### Почему

**Проблема, которая закрывается:** silent bug при использовании
wall-clock для timing logic. Сценарий:

```nova
// БАГ под старым API (одна Timestamp на всё):
ro deadline = Time.now() + Duration.from_secs(60)
// ... 30 сек проходит ...
// NTP синхронизирует часы НАЗАД на 5 секунд:
//   Time.now() теперь "moment - 25s" вместо "moment - 30s"
// Таймер сработает через 35 реальных секунд вместо 30.
```

**Параллели в индустрии** (все пришли к разделению после bug-bash):

| Язык | Wall-clock | Monotonic | Когда разделили |
|---|---|---|---|
| **Rust** | `std::time::SystemTime` | `std::time::Instant` | с самого начала (1.0, 2015) |
| **Java** | `java.time.Instant` | `System.nanoTime()` (long, не тип) | partial — Java 8, full Type — never |
| **Go** | `time.Time` | `time.Time` с monotonic component | Go 1.9 (2017) — раньше использовали wall-clock everywhere → silent bugs |
| **C#** | `DateTime` / `DateTimeOffset` | `Stopwatch.GetTimestamp()` | partial |
| **Python** | `time.time()` | `time.monotonic()` | PEP 418 (Python 3.3, 2012) — явно разнесли после real-world failures |
| **JS** | `Date.now()` | `performance.now()` | DOM Performance API |

Все — после реальных production-инцидентов (Go 1.9 release notes:
«невозможно правильно измерять timeouts во время DST/NTP без monotonic»).

**Type safety > runtime documentation.** Альтернатива — «один Timestamp +
документация „не используйте для timers"» — ловит баги только при ревью,
не компилятором. Type-разделение делает ошибку **невыразимой**.

### Что отвергнуто

- **Один `Timestamp` тип с tag-field `kind: ClockKind`** — runtime
  branch на каждой арифметической операции; теряется compile-time
  guarantee.

- **Go 1.9-стиль (один `Time` с обоими компонентами)** — `Time` несёт
  и wall-clock, и monotonic; runtime сам выбирает что использовать.
  Проще для users, но: два syscall на каждый `now()`; сериализация
  требует drop'а monotonic component (silent footgun); невозможно
  type-checker'ом запретить misuse; историческая правка после bug-bash,
  не оригинальный дизайн.

- **`Instant` имя (Rust convention)** — отвергнут в пользу `Monotonic`
  потому что «Instant» в Java семантически = wall-clock (`java.time.Instant`),
  путает Java-разработчиков. «Monotonic» прямо описывает свойство, без
  культурных ассоциаций. Pair `Timestamp / Monotonic` читается симметрично.

- **`Time.tick()` как low-level i64 monotonic ns** — даёт raw int,
  теряет type-safety. Reserved как `Monotonic.@as_nanos() -> i64`
  (escape hatch для FFI / bench).

- **Отдельный `Duration_mono` для monotonic-interval'ов** — overkill;
  `Duration` уже type-safe (signed, nanos), unit-agnostic. `Mono - Mono`
  и `Ts - Ts` оба возвращают тот же `Duration`.

**AMEND (Plan 175, 2026-07-04..07-10 — актуализация под D316/D317/D318;
исходный текст выше писался до `Time`-schema-унификации, имена и детали
дрейфовали):**
- `Time.now()`/`Time.now_monotonic()` из §«Что» устарели по имени —
  переименованы в `Time.now_unix_ms()`/`Time.now_monotonic_ns()` (D316
  owner unit-rename, 2026-07-06; чистое переименование wire-op).
- `Timestamp`/`Monotonic` теперь `value`-records (Ф.1b), не heap `{}`.
- **`Monotonic.now()` больше НЕ compiler-builtin** (Ф.3a, 2026-07-10) —
  обычная `.nv`-функция (`std/time/duration.nv`), мокабельна через
  `with Time = handler {...}` (закрывает п.5 «mock-handler должен
  реализовать оба `now()`» — теперь буквально верно и для monotonic).
  `Monotonic.from_nanos` по-прежнему НЕ введён (п.4 остаётся в силе —
  opaque-контракт; см. также D316-amend «Ф.2-находка» — эта opacity
  ЕСТЬ причина, почему typed-wire-в-схеме архитектурно дороже, чем
  typed-сахар-поверх-int-wire).
  `Monotonic - Monotonic -> Duration` теперь ДВЕ формы: named `@elapsed_since`
  (было) + operator `@minus(Monotonic)` (Ф.3c, alias, `m2 - m1` дispatch).
  Non-regression — saturate-to-zero (D318), не «всегда ≥0 by assumption»
  как исходный текст п.3 наивно предполагал (HW/VM/OS-баг возможен).
- Serialization-запрет на `Monotonic` (п. «Что») — verification (derive-
  путь отсутствует) вынесена в план 175 Ф.6, ещё не пройдена в этой волне.

### Связь

- **[D75](#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном)** —
  `CancelToken` может иметь deadline (`tok.cancel_at(Monotonic)`) —
  только monotonic, иначе same NTP-skew bug.
- **[D94](#d94-select--multiplexed-channel-operations)** — `select` arms
  с timeout через `ChanReader.close_after(Duration)` (clock-agnostic) или
  `ChanReader.close_at(Monotonic)`.
- **[Plan 65](../../docs/plans/65-chanreader-close-after.md)** Ф.12 —
  driver для D124 (нужен `close_at` для absolute deadline).
- **[Plan 65 Ф.12.1-Ф.12.6](../../docs/plans/65-chanreader-close-after.md)** —
  реализация D124: `Monotonic` тип + `Time.now_monotonic()` + `close_at` +
  runtime `clock_gettime(CLOCK_MONOTONIC)` / `QueryPerformanceCounter` per OS.
  Driver — нужен для `ChanReader.close_at(Monotonic)`.
- **[Plan 22](../../docs/plans/22-sleep-libuv-integration.md)** — libuv
  monotonic timer infra (`uv_hrtime()`) reused for `now_monotonic`.

### Эволюция API

| Что | Сейчас | После Plan 68 (D124 closure) |
|---|---|---|
| `Time.now()` (compiler schema) | `() -> nova_int` (raw ms, **противоречит** stdlib usage) | `() -> Timestamp` (record) |
| `Time.now()` (stdlib calls) | `Timestamp` (`.gt()`, `.minus()`) — **silent mismatch** с schema | aligned с schema |
| `Time.now_monotonic()` | ❌ нет | `() -> Monotonic` |
| `Time.sleep(d)` | `(int ms)` (legacy) | `(Duration)` (out-of-scope для Ф.12; отдельная задача) |
| Deadline в API | `Time.now() + d` (wall-clock baked in) | `Monotonic.now() + d` (no NTP skew) |
| `ChanReader.close_at(...)` | ❌ нет | `(Monotonic) -> ChanReader[()]` (Ф.12.4) |

**Latent bug под текущим API** (resolved Plan 65 Ф.12.3): `time_schema`
в `emit_c.rs:1044` declares `Time.now() -> nova_int`, но stdlib
`std/time/duration.nv:538-714` использует как `Timestamp` record.
Работает сейчас через handler-bridge (тот же mechanism что Plan 65
fixed для Duration handler params, `[M-handler-duration-schema-mismatch]`).
Plan 65 Ф.12.3 aligns schema с реальным usage.

### Файлы (затронуты при реализации Plan 65 Ф.12)

- `std/time/duration.nv` — добавить `type Monotonic { ro nanos i64 }`
  + конструкторы только через `Monotonic.now()` / `Monotonic.@as_nanos()`.
- `compiler-codegen/src/codegen/emit_c.rs:1042-1046` — обновить
  `time_schema`: `now() -> Timestamp`, добавить `now_monotonic() -> Monotonic`.
- `compiler-codegen/nova_rt/time.c` (новый) — `nova_time_now_realtime_ns()`
  + `nova_time_now_monotonic_ns()` per-OS implementations.
- `nova_tests/plan65/f12_*` — типы не interconvertible (negative tests),
  NTP-skew resilience (mock Time effect), `close_at(Monotonic)` integration.


## D136. M:N worker-count — порядок разрешения и `NOVA_MAXPROCS`

> **Введён:** 2026-05-22 (Plan 83.1 Ф.1–Ф.3 driver). **Статус:** принят;
> реализация в `compiler-codegen/nova_rt/runtime.c`
> (`nova_runtime_resolve_maxprocs`). **Дополняет** D98 / D103 (M:N-рантайм).

### Что

Число worker-потоков M:N-рантайма резолвится из трёх источников по
строгому приоритету:

```
explicit runtime.init(n>0)  >  ENV NOVA_MAXPROCS  >  uv_available_parallelism()
```

- **explicit** — аргумент `runtime.init(n)` при `n > 0`.
- **`NOVA_MAXPROCS`** — переменная окружения (аналог `GOMAXPROCS` в Go).
  Невалидное значение (не целое / ≤ 0 / overflow) → диагностика на
  stderr + fallback на auto-detect (НЕ abort процесса).
- **auto-detect** — `uv_available_parallelism()` (libuv 1.52, уже
  cgroup- и affinity-aware).

Результат клэмпится в **[1, 1024]**. Запрос выше потолка (любой
источник) → клэмп до 1024 + диагностика на stderr.

`runtime.maxprocs()` возвращает резолвнутую цель (даже до `runtime.init`
и после `runtime.shutdown`); `runtime.worker_count()` — фактически
поднятые потоки.

### Почему

- Паритет с Go: `GOMAXPROCS` — стандартный способ управления
  параллелизмом; `NOVA_MAXPROCS` повторяет семантику и стиль имени
  (`NOVA_*`, как `NOVA_TARGET_OS`).
- `uv_available_parallelism()` уже корректен в контейнерах (cgroup-
  квота, CPU affinity) — переизобретать через `sysconf`/`GetSystemInfo`
  было бы регрессией по cgroup-корректности.
- explicit > env: явный код важнее окружения. env > auto: оператор
  деплоя может переопределить без пересборки.
- Клэмп [1, 1024]: 1 — минимум осмысленного пула; 1024 — потолок,
  выше которого запрос почти наверняка ошибка конфигурации, которую
  честнее диагностировать, чем исполнять.

### Известная дельта vs Go

cgroup-квота читается **один раз** при резолве (на `runtime.init` либо
первом `runtime.maxprocs()`). Go 1.25+ перечитывает квоту динамически и
ресайзит пул на лету. Динамический re-read — followup Plan 83.x
(требует Ф.4 lazy-spawn V2 с инкрементальным ростом пула). Для деплоев
с фиксированным лимитом контейнера (норма) статическое чтение корректно.

### Связь

- Plan 83.1 (M:N-инфраструктура) — реализация.
- Plan 83.2 — перевод M:N в дефолт (отдельное решение, gated на Plan 82).
- D98 / D103 — M:N-рантайм (per-worker loop, preemption).

## D137. M:N — ленивая материализация пула, runtime.init как тюнер

> **Введён:** 2026-05-22 (Plan 83.1 Ф.4). **Статус:** принят; реализация
> в `compiler-codegen/nova_rt/runtime.c`. **Дополняет** D136.
>
> ⚠️ **AMENDED by Plan 259 ([D451](#d451-mn--пул-воркеров-материализуется-на-старте-процесса-amends-d137d138))
> (2026-08-09).** Материализация пула больше НЕ ленивая — она происходит
> на старте процесса, а не на первом `spawn` (№457: ленивая
> материализация могла случайно попасть внутрь пользовательского
> `supervised(timeout:)`-бюджета). Секции «Что»/«Следствия» ниже —
> **историческое** описание V1; действующее поведение — в D451.

### Что

`runtime.init(n)` НЕ создаёт worker-потоки немедленно. Он лишь
**ARM'ит** рантайм: резолвит и фиксирует целевое число worker'ов
(порядок — D136). Реальный пул worker-потоков + sysmon-поток
материализуются **лениво** — на первом worker-bound `spawn`.

Следствия:
- Программа без `spawn` (hello-world) исполняется на одном главном
  потоке: 0 worker-потоков, 0 sysmon, даже если `runtime.init` вызван.
- `runtime.worker_count()` == 0 до первого spawn; `runtime.maxprocs()`
  возвращает целевое число с момента `init`.
- `runtime.is_initialized()` == true с момента `init` (armed), а не с
  момента материализации пула.

`runtime.init` — **одноразовый тюнер**:
- до материализации пула повторный `init(m)` — валидный re-tune
  целевого числа (последний выигрывает);
- после материализации `init` — диагностируемый no-op на stderr (не
  abort: существующий пул корректен и продолжает работать).

V1: на первом spawn поднимается **весь** пул `maxprocs`.
Инкрементальный рост пула (полный Go-`M`-паритет) — followup.

### Почему

- Нулевая цена для не-конкурентного кода: hello-world не платит за
  M:N-инфраструктуру. Паритет с Go (ленивый `M`) или лучше — у Go
  sysmon-поток живёт всегда.
- Готовит флип дефолта (Plan 83.2): когда M:N включится по умолчанию,
  ленивость гарантирует, что однопоточные программы не регрессируют
  по числу потоков.
- `init`-как-тюнер: явный `init(n)` остаётся опциональным override'ом
  числа worker'ов, но перестаёт быть «включателем».

### Связь

- D136 — резолв числа worker'ов (explicit > NOVA_MAXPROCS > auto).
- Plan 83.1 Ф.4 — реализация; Ф.5 — thread-budget для nova test/bench.
- Plan 83.2 — флип M:N в дефолт.

---

## D138. Default-on M:N runtime — production semantics (Plan 83.4.5.6 Ф.3, ACTIVE 2026-05-24)

> ✅ **ACTIVE — semantic specification finalized; activation completed Plan
> 83.4.5.8 (2026-05-24). M:N runtime default-on в compiled binaries:
> `nova_runtime_auto_arm()` calls at main start (codegen emit_main_wrapper).
> Все 8 prerequisite fix'ов сошлись:**
>
> ⚠️ **Rule 3 и acceptance-пункт «Hello-world без spawn — 0 worker
> threads» AMENDED by Plan 259 ([D451](#d451-mn--пул-воркеров-материализуется-на-старте-процесса-amends-d137d138))
> (2026-08-09)** — пул материализуется на старте процесса, не на первом
> spawn; hello-world под default-on M:N теперь поднимает `maxprocs()`
> worker-потоков без единого spawn. 0 потоков — только под
> `NOVA_AUTOARM=0`.
>
> - **Plan 83.4.1** park-with-predicate (D93 ASYNC close_cb).
> - **Plan 83.4.2** Ф.1+Ф.2 supervised_step worker-owned skip +
>   per-fiber handler-snapshot save/restore.
> - **Plan 83.4.3** B5 cancel_requested atomic.
> - **Plan 83.4.5.1** cancel-wake-all + dispatch_ready re-queue.
> - **Plan 83.4.5.2** AsyncDetach production-grade + orphan-spawn
>   tracking via _nova_orphan_scope.pending_remote.
> - **Plan 83.4.5.4** spawn-time handler-snapshot TLS capture.
> - **Plan 83.4.5.5** NOVA_NO_AUTOARM=1 escape hatch (cooperative-only
>   tests).
> - **Plan 83.4.5.7 Ф.1** atomic fiber state machine
>   (NovaSpawnCtxBase._nova_fiber_state + CAS guards mco_resume sites +
>   idempotent wake CAS на parked flag + nova_runtime_shutdown ordering).
> - **Plan 83.4.5.8** nova_alloc_uncollectable для SpawnCtx + worker-side
>   GC_free post mco_destroy (defeats Boehm GC race на Windows fiber arena
>   ctx visibility).

### Что

Compiled Nova-программы (Plan 83.2 flip) запускают M:N runtime **по
умолчанию** — паритет с Go (`GOMAXPROCS=NumCPU`), tokio multi-thread
runtime, Kotlin `Dispatchers.Default`. Hello-world без spawn не платит
за worker-потоки (lazy pool), но любая supervised{spawn}/parallel
for/detach автоматически распределяется на доступные ядра.

### Правило

1. **Default model.** Compiled binary — armed M:N по умолчанию. `nova run`
   (интерпретатор) остаётся однопоточным.

2. **Worker-count resolution** (D136 паритет):
   - explicit `runtime.init(n>0)` побеждает;
   - иначе `NOVA_MAXPROCS` env var;
   - иначе `uv_available_parallelism()` (cgroup/affinity-aware).
   Клэмп `[1, 1024]`.

3. **Lazy worker pool** (D137 паритет). Workers поднимаются на первом
   spawn; hello-world без spawn = 0 worker threads.

4. **Escape hatch.** Два режима:
   - `NOVA_MAXPROCS=1` — один worker (deterministic single-thread под
     M:N machinery; полезно для precision-bench'ей).
   - `NOVA_AUTOARM=0` — полный bootstrap mode (runtime never armed,
     spawn идёт через cooperative scope queue; полезно для tests,
     specifically проверяющих cooperative-only semantics). Plan
     83.4.5.9 (2026-05-24): renamed из legacy `NOVA_NO_AUTOARM=1`
     ради positive env-name convention (без двойного отрицания).
     Принимаются значения "0" / "false" / "no" / "n" / "f"
     (case-insensitive); unset либо "1" / "true" / другое → default
     (armed enabled).

5. **Worker blocking ban.** Worker НЕ делает блокирующую работу
   inline — все FFI / syscall'ы обязаны быть в `blocking { … }`
   (D50 §4; Plan 83.3 V1-контракт).

6. **Spawn ordering — НЕ специфицирован** (Go-паритет: "Spawn ordering:
   no guarantees"). Tests, опирающиеся на specific scheduler order,
   должны использовать set-equality assertions либо escape hatch
   `NOVA_AUTOARM=0`.

7. **Cancellation** — hierarchical через scope-tree (Plan 83.4.5.1
   `nova_scope_cancel_wake_all` + cancel_all_pending dispatch_ready
   re-queue для SYNC slots; tokio `CancellationToken.notify_waiters`
   паритет). Token-tree cascade через `linked[]` (Plan 49). Atomic
   cancel_requested flag (Plan 83.4.3 B5).

8. **Detach (D50 §3.1 amend, Plan 83.4.5.2)** — fire-and-forget на
   worker pool через `nova_runtime_spawn_orphan` (Go `go fn()` /
   tokio::spawn без JoinHandle / Kotlin GlobalScope.launch паритет).
   LogAndDrop fail-handler. Sync через `runtime.drain_orphans()`
   (Go sync.WaitGroup.Wait analog).

9. **Per-fiber state** — handler-snapshot per fiber (Plan 83.4.2 Ф.2
   worker save/restore + Plan 83.4.5.4 spawn-time TLS inheritance —
   Node `AsyncLocalStorage.run`, Kotlin `CoroutineContext.Element`
   auto-inherit паритет). Snapshot travels с fiber'ом cross-worker
   через work-stealing migration.

### Почему

- Default-on M:N — production-grade ожидание для современного
  concurrent runtime'а. Все референсные runtimes (Go, tokio, Kotlin)
  default к multi-thread; user opt-out, не opt-in.
- Lazy pool — нулевая цена для не-конкурентных программ. Hello-world
  не платит за worker-потоки.
- Escape hatches (MAXPROCS=1, NO_AUTOARM=1) — для test-suite specific
  needs без compromising production default.
- Spawn ordering unspecified — позволяет work-stealing scheduler
  максимальную свободу для CPU-affinity / load-balancing.

### Cross-runtime parity таблица

| Aspect | Go | tokio | Kotlin | Node | Nova цель |
|---|---|---|---|---|---|
| Default model | M:N | M:N (multi-thread) | M:N (Dispatchers.Default) | single-thread | M:N |
| Worker count | GOMAXPROCS | tokio::main(...) | Dispatchers.Default = NumCPU | n/a | NOVA_MAXPROCS |
| Lazy pool | M materialized on demand | task spawn → executor | Coroutine first launch | n/a | first spawn |
| Cancel | context.Done close | CancellationToken | Job.cancel cascade | AbortController | scope-tree + token-tree |
| Detach | go fn() | tokio::spawn (no JoinHandle) | GlobalScope.launch | setImmediate | nova_runtime_spawn_orphan |
| Per-fiber state | goroutine context | task_local! | CoroutineContext.Element | AsyncLocalStorage | fiber_effect_snapshot |
| Wake parked | runtime.gopark | Notify | JobSupport | n/a | nova_scope_cancel_wake_all |

### Связь

- [D14](#d14-fiber-runtime--невидимая-инфраструктура) — fiber runtime
  фундамент.
- [D50](#d50-concurrency-model-spawn-detach-blocking) §3.1 — detach
  semantic amend (AsyncDetach).
- [D71](#d71-bootstrap-concurrency-runtime) — bootstrap baseline.
- [D80](#d80-handler-scoping-per-fiber) — per-fiber handler scoping.
- [D93](#d93-park-wake--нормативный-runtime-primitive-для-блокирующих-операций) — park/wake D-block.
- [D136](#d136-mn-worker-count--порядок-разрешения-и-nova_maxprocs) +
  [D137](#d137-mn--ленивая-материализация-пула-runtime-init-как-тюнер)
  — worker resolution + lazy pool.
- Plan 83.2 — flip-default activation.
- Plan 83.4.5.6 — closure target.
- Plan 83.4.5.7 — multi-worker race fix (GATED dependency).

### Acceptance

- Compiled binary без `runtime.init()` использует все CPU при
  fiber-нагрузке.
- Hello-world без spawn — 0 worker threads.
- `NOVA_MAXPROCS=N` env var корректно clamp'ит worker count.
- `NOVA_NO_AUTOARM=1` env var полностью отключает auto-arm (bootstrap
  mode).
- 24 NEW regressions из Plan 83.4.5 Ф.0 enumeration все PASS под
  default-on M:N (после Plan 83.4.5.7 race fix).
- Speedup vs single-thread ≥3.0× на CPU-bound parallel_for (4 cores).

### Статус

📋 **DRAFT** — спецификация intended behavior. Имплементация
подготовлена (Plan 83.4.5.1-5 ✅). Activation в codegen-emit GATED
на Plan 83.4.5.7 multi-worker double-resume race fix. После fix'а
+ Plan 83.4.5.6 закроет roadmap Plan 83.

## D451. M:N — пул воркеров материализуется на старте процесса (amends D137/D138)

> **Введён:** 2026-08-09 (Plan [259](../plans/259-fiber-arena-guard-pages.md)
> Слой 2). **Статус:** ✅ принят; реализация в
> `compiler-codegen/nova_rt/runtime.c` (`nova_runtime_auto_arm()`).
> **Amends** [D137](#d137-mn--ленивая-материализация-пула-runtime-init-как-тюнер)
> §«Следствия» и [D138](#d138-default-on-mn-runtime--production-semantics-plan-83456-ф3-active-2026-05-24)
> правило 3 + acceptance «Hello-world без spawn — 0 worker threads».
> Реестр №457 (симптом), №470/№474 (тот же класс — запрет компенсировать
> дедлайн длительностью инициализации), `[M-187-docker-linux-runtime-hang]`
> (родственная волна, тот же файбер-арена subsystem).

### Что

Материализация worker-пула (потоки + sysmon-поток) больше НЕ
откладывается до первого worker-bound `spawn`. Она происходит
**синхронно на старте процесса**, внутри `nova_runtime_auto_arm()` —
первого рантайм-вызова эмитируемого `main()` (сразу после
`nova_gc_init()`/`nova_consts_init()`, до исполнения тела программы).

Отменяет описание D137 «пул поднимается лениво на первом spawn» и
связанное следствие D138 «Hello-world без spawn — 0 worker threads»:
теперь ЛЮБАЯ армированная программа (default-on M:N, т.е. без
`NOVA_AUTOARM=0`) поднимает полный целевой пул ДО первой строки
пользовательского кода — **включая hello-world без единого `spawn`**.

### Почему

Причина — №457: пул, поднятый лениво «на первом spawn», списывает цену
инициализации (потоки, sysmon, per-worker libuv loop, per-worker
fiber-арена) на ТОГО, кто случайно оказался первым, кто вызвал `spawn`
— которым может оказаться `spawn` ВНУТРИ пользовательского
`supervised(timeout:)`-блока. Инфраструктура рантайма обязана
укладываться в СВОЙ бюджет, а не вычитаться из чужого дедлайна —
прямой прецедент владельца по №470/№474 («не компенсировать дедлайн
длительностью паузы — прятать симптом»; здесь то же самое про
длительность инициализации). Перенос материализации на старт процесса
убирает саму возможность этой цене попасть в произвольный
пользовательский бюджет: она оплачивается ДО того, как какой-либо
таймер пользователя мог начать отсчёт.

Слой 1 того же плана (259, [D97](#d97-fiber-stack-allocation--per-thread-mmap-arena-linuxmacos)
амендмент — см. `docs/plans/259-fiber-arena-guard-pages.md`) сделал
guard-страницы файбер-арены ленивыми (пробиваются на первое
использование СЛОТА, а не все разом при инициализации арены каждого
воркера). Без слоя 1 материализация пула на старте оставалась бы такой
же дорогой, как была (шторм `mprotect` при создании арены каждого
воркера, №457 root cause) — просто переехавшей раньше по времени, а не
устранённой. Слои дополняют друг друга и нужны ОБА: слой 1 снижает
абсолютную цену инициализации арены, слой 2 гарантирует, что
ОСТАТОЧНАЯ цена (создание потоков, sysmon, per-worker libuv loop)
платится вне пользовательского бюджета, а не внутри него.

### Правило

1. `nova_runtime_auto_arm()` — единственный автоматический arm-путь,
   emit'ится codegen'ом безусловно первой содержательной строкой
   `main()` (`compiler-codegen/src/codegen/emit_c.rs`, `emit_main_wrapper`)
   — теперь материализует пул сразу после arm, а не откладывает это до
   первого spawn.
2. Явный `runtime.init(n)`, вызванный из пользовательского кода,
   исполняется ПОСЛЕ того, как `main()` уже начал выполняться — то
   есть ПОСЛЕ того, как `nova_runtime_auto_arm()` уже материализовал
   пул с auto-detect/`NOVA_MAXPROCS`-резолвнутым числом воркеров.
   `runtime.init(n)` остаётся валидным вызовом (не ошибка) — это
   диагностируемый no-op на stderr, поведение, которое D137 уже
   описывал для случая «после материализации»; теперь это
   **единственный** достижимый для пользовательского кода случай, а не
   крайний. Чтобы задать число воркеров под default-on M:N, остаётся
   `NOVA_MAXPROCS` (env, читается ДО материализации, на arm) —
   `runtime.init(n>0)` как «тюнер до первого spawn» для скомпилированных
   бинарников с default-on M:N больше недостижим тем способом, каким
   был раньше (вызов `init` в начале пользовательского `main` до
   первого `spawn`): пул уже поднят к моменту, когда пользовательский
   код может вызвать `init`.
3. `NOVA_AUTOARM=0` (полный bootstrap/cooperative-режим) — по-прежнему
   НЕ армит рантайм вовсе, соответственно НЕ материализует пул.
   Единственный оставшийся способ получить 0 worker threads на старте.
4. `runtime.worker_count()` возвращает целевое число воркеров сразу
   после старта armed-программы (даже без единого `spawn`), а не 0.
   `runtime.is_initialized()` не меняется (уже было `true` с момента
   arm, до этого амендмента).
5. Зажим `NOVA_ARENA_VMA_RESERVE`/`NOVA_ARENA_VMA_SAFETY_PCT` (арена
   файберов, `fiber_arena.c`, лечил VMA-шторм СЛЕДСТВИЕМ) снят Слоем 1
   того же плана — ленивые guard-страницы убирают причину, зажим не
   нужен.

### Cross-runtime parity

Go поднимает `sysmon`-поток безусловно при старте рантайма (не
лениво); worker-`M`-потоки — по требованию, но `GOMAXPROCS`-логическая
инфраструктура (`allp[]`) строится на старте через `schedinit()`, а не
на первом `go` statement. Данный амендмент сближает Nova с этой
моделью: инфраструктурная цена (не то же самое, что «поднять КАЖДЫЙ
worker-поток» — Go тоже создаёт `M`-потоки по требованию через
`newm`) переносится к моменту, когда рантайм точно знает свой бюджет
(старт процесса), а не к первому пользовательскому запросу параллелизма.

### Связь

- Plan [259](../plans/259-fiber-arena-guard-pages.md) — Слой 1 (ленивые
  guard-страницы файбер-арены) + Слой 2 (этот блок).
- №457 — таймаут `supervised(timeout:)` не укладывался в бюджет из-за
  ленивой материализации пула, случайно попавшей внутрь таймд-блока.
- №470/№474 — тот же класс дефекта («не компенсировать дедлайн»
  запрет).
- `[M-187-docker-linux-runtime-hang]` — родственная волна (VMA-шторм
  той же файбер-арены), закрыта Слоем 1 этого плана снятием зажима
  `NOVA_ARENA_VMA_*`.
- [D97](#d97-fiber-stack-allocation--per-thread-mmap-arena-linuxmacos) —
  файбер-арена (Слой 1 меняет guard-page timing внутри неё, отдельно
  от материализации пула).
- [D137](#d137-mn--ленивая-материализация-пула-runtime-init-как-тюнер) —
  amended (материализация больше не ленивая).
- [D138](#d138-default-on-mn-runtime--production-semantics-plan-83456-ф3-active-2026-05-24) —
  правило 3 + acceptance amended.

---

## D167. Memory ordering & happens-before между fiber'ами

> **Статус:** ✅ final (Plan 103.7, 2026-05-27). Реализован в Plan 103.1.

### Что

`MemOrdering` enum (5 вариантов) экспонирует C11/GCC `__ATOMIC_*` ordering
constants на Nova-уровне. Используется с `fence()` (Plan 103.1) и будущими
atomic operations с explicit ordering (Plan 103.2+). Контракт
happens-before между fiber'ами через paired Acquire/Release.

**Типы**: `MemOrdering` в `std/runtime/sync` — НЕ путать с prelude `Ordering`
(three-way comparison: Less|Equal|Greater).

### Варианты MemOrdering

| Вариант | C constant | Семантика | Valid для |
|---|---|---|---|
| `Relaxed` | `__ATOMIC_RELAXED` | Только атомарность; нет happens-before | load, store, RMW, fence (no-op) |
| `Acquire` | `__ATOMIC_ACQUIRE` | Все subsequent ops happen-after prior Release | load, RMW, fence |
| `Release` | `__ATOMIC_RELEASE` | Все prior ops happen-before subsequent Acquire | store, RMW, fence |
| `AcqRel`  | `__ATOMIC_ACQ_REL` | Acquire+Release combined | RMW, fence |
| `SeqCst`  | `__ATOMIC_SEQ_CST` | Total order на всех SeqCst ops | все ops |

**Default ordering** (Plan 103.2+ simple-overload методы): `SeqCst` — безопасно
для всех use cases, переопределяется через `_ordered` overloads для
perf-critical кода (design decision M1).

### fence(MemOrdering) семантика

- `fence(MemOrdering.Relaxed)`: no-op (валиден синтаксически, нет ordering-эффекта)
- `fence(MemOrdering.Acquire)`: sequenced ordering point для последующих loads/stores
- `fence(MemOrdering.Release)`: sequenced ordering point для предыдущих loads/stores
- `fence(MemOrdering.AcqRel)`: combination
- `fence(MemOrdering.SeqCst)`: total-order participation

### Validation правила (compile-time, Plan 103.2+)

| Operation | Запрещённые | Error code |
|---|---|---|
| load | Release, AcqRel | `E_INVALID_ORDERING_LOAD` |
| store | Acquire, AcqRel | `E_INVALID_ORDERING_STORE` |
| fence | — (все валидны; Relaxed = no-op) | — |
| RMW (swap, CAS, fetch_*) | — | — (Plan 103.2) |

Validation — compile-time при literal ordering (типичный случай: `MemOrdering.Acquire`).
Runtime-value ordering (let ord = pick()) → fallback runtime panic.

### Memory model contract

**Bootstrap-runtime** (single-fiber, single-threaded fiber pool):
All ordering variants are semantically equivalent — sequenced-before covers all.
`MemOrdering.Relaxed` и `MemOrdering.SeqCst` имеют идентичные эффекты.

**M:N runtime** (Plan 23, Plan 83.x): Full C11 memory model via `__atomic_*` GCC/Clang
builtins (cross-platform: Linux, macOS, Windows/Clang). Happens-before между fiber'ами
через paired Acquire/Release.

**Performance note**: x86 имеет strong memory model (TSO); большинство
ordering барьеров дёшевы (load-acquire ≈ plain load, SeqCst store ≈ MFENCE).
ARM64 — более explicit барьеры (DMB LD/ST/ISH); Relaxed vs Acquire savings
значительны для hot-path counters (~1-2ns per op).

### Связи

- D14, D50 (fiber runtime) — D167 specifies memory model для D14 production-runtime
- D79 (Channels) — channel send/recv → implicit happens-before (Go-style); see D91/D94
- D138 (Default-on M:N) — производительность зависит от корректного ordering
- Plan 103.1 — реализация MemOrdering enum + fence(MemOrdering) + codegen helper
- Plan 103.2 — AtomicX.load(MemOrdering) / .store(v, MemOrdering) / RMW overloads
- Plan 103.7 — финальная редакция D167; D370 AI-first guidance для паттернов ordering
- D370 — decision tree: когда нужен explicit ordering vs SeqCst-default

---

## D168. Sized atomic types — API contract (Plan 103.2)

> **Статус:** ✅ final (Plan 103.7, 2026-05-27). Реализован в Plan 103.2.

### Что

Nova предоставляет **11 sized atomic типов**<sup>†</sup> с детерминированной шириной:

| Тип | Ширина | Диапазон | C-репрезентация |
|---|---|---|---|
| `AtomicI8` | 8 бит | −128..127 | `int8_t` |
| `AtomicI16` | 16 бит | −32768..32767 | `int16_t` |
| `AtomicI32` | 32 бит | −2³¹..2³¹−1 | `int32_t` |
| `AtomicI64` | 64 бит | −2⁶³..2⁶³−1 | `int64_t` |
| `AtomicU8` | 8 бит | 0..255 | `uint8_t` |
| `AtomicU16` | 16 бит | 0..65535 | `uint16_t` |
| `AtomicU32` | 32 бит | 0..2³²−1 | `uint32_t` |
| `AtomicU64` | 64 бит | 0..2⁶⁴−1 | `uint64_t` |
| `AtomicInt` | платформенная | −2^(W−1)..2^(W−1)−1 | `nova_int` (= `intptr_t`) |
| `AtomicUint` | платформенная | 0..2^W−1 | `nova_uint` (= `uintptr_t`) |
| `AtomicBool` | 1 логический бит | `false`/`true` | `bool` (8-битное хранение) |

<sup>†</sup> **Амендировано [D426](#d426-atomic-семейство-консолидация-имён-atomicisizeatomicusize--atomicintatomicuint-легаси-atomicint-и-atomicptr-сняты-plan-207)**
(Plan 207, 2026-07-16): было 12 типов под именами `AtomicIsize`/`AtomicUsize`/`AtomicPtr`
(плюс отдельный легаси `AtomicInt`, int32-precision, вне этой таблицы). Консолидация:
`AtomicIsize`→`AtomicInt`, `AtomicUsize`→`AtomicUint` (Nova не вводит типы
`isize`/`usize` — `int`/`uint` уже address-sized, Plan 133); легаси `AtomicInt`
снят (API-подмножество покрыто новым `AtomicInt`); `AtomicPtr` снят целиком
(int-proxy дубль без generic `[T]`, GC-root integration не реализована —
см. D426).

Все типы являются **value types в Nova**, копируются по значению при передаче в
функцию / возврате. Семантически — **ячейки с атомарным доступом**, не
разделяемые reference-типы. Для разделения между fiber'ами — передавать
mutable-ссылку или хранить в heap-структуре.

### Правило

#### 1. Матрица операций

Все 11 типов поддерживают следующие операции (обозначение: `T` — тип значения,
`Bool` — `bool`, `Int` — `int`). Столбец `AtomicPtr` **снят** ([D426](#d426-atomic-семейство-консолидация-имён-atomicisizeatomicusize--atomicintatomicuint-легаси-atomicint-и-atomicptr-сняты-plan-207),
Plan 207, 2026-07-16) — см. §4 ниже:

| Операция | AtomicI*/U*/Int/Uint | AtomicBool |
|---|---|---|
| `new(v T) → AtomicX` | ✓ | ✓ (`v bool`) |
| `load() → T` | ✓ | ✓ |
| `load(ord MemOrdering) → T` | ✓ | ✓ |
| `store(v T)` | ✓ | ✓ |
| `store(v T, ord MemOrdering)` | ✓ | ✓ |
| `swap(v T) → T` | ✓ | ✓ |
| `swap(v T, ord MemOrdering) → T` | ✓ | ✓ |
| `compare_exchange(expected T, desired T) → Result[(), T]`* | ✓ | ✓ |
| `compare_exchange(exp T, des T, ord MemOrdering) → Result[(), T]`* | ✓ | ✓ |
| `compare_exchange_weak(exp T, des T) → Result[(), T]`* | ✓ | ✓ |
| `compare_exchange_weak(exp T, des T, ord MemOrdering) → Result[(), T]`* | ✓ | ✓ |
| `fetch_add(v T) → T` | ✓ (int/uint только) | — |
| `fetch_add(v T, ord MemOrdering) → T` | ✓ | — |
| `fetch_sub(v T) → T` | ✓ | — |
| `fetch_sub(v T, ord MemOrdering) → T` | ✓ | — |
| `fetch_or(v T) → T` | ✓ | ✓ (`v bool`) |
| `fetch_or(v T, ord MemOrdering) → T` | ✓ | ✓ |
| `fetch_and(v T) → T` | ✓ | ✓ |
| `fetch_and(v T, ord MemOrdering) → T` | ✓ | ✓ |
| `fetch_xor(v T) → T` | ✓ | ✓ |
| `fetch_xor(v T, ord MemOrdering) → T` | ✓ | ✓ |
| `fetch_nand(v T) → T` | ✓ | — |
| `fetch_nand(v T, ord MemOrdering) → T` | ✓ | — |
| `fetch_max(v T) → T` | ✓ | — |
| `fetch_max(v T, ord MemOrdering) → T` | ✓ | — |
| `fetch_min(v T) → T` | ✓ | — |
| `fetch_min(v T, ord MemOrdering) → T` | ✓ | — |

**Примечание по AtomicPtr (снят, [D426](#d426-atomic-семейство-консолидация-имён-atomicisizeatomicusize--atomicintatomicuint-легаси-atomicint-и-atomicptr-сняты-plan-207)):** ранее хранил `int` (адрес GC-объекта как `intptr_t`).
Арифметика не поддерживается (`fetch_add` нет). Typed generic form `AtomicPtr[T]`
с GC-root integration — откладывается в Plan 103.9+.

`*` **CAS-строки амендированы [D425](#d425-cas-возвращает-свидетеля-провала-compare_exchange-bool--resultt-plan-207)** (Plan 207, 2026-07-15): возврат
`bool` → `Result[(), T]` (`Ok(())` успех / `Err(actual)` провал с witness-значением).

#### 2. MemOrdering-aware overloads

Каждая операция, принимающая `MemOrdering`, является **overload** к базовой
операции без ordering-параметра. Обе формы — валидные публичные API:

```nova
ro a = AtomicI64.new(0)
a.store(42)                          // default: SeqCst
a.store(42, MemOrdering.Relaxed)     // explicit: Relaxed
ro v = a.load()                     // default: SeqCst
ro v2 = a.load(MemOrdering.Acquire) // explicit: Acquire
```

**Default ordering = SeqCst** для всех операций без явного параметра. Это
максимально безопасный выбор; производительность при нужде оптимизируется явным
указанием Relaxed/Acquire/Release.

**Ограничения (из D167):**
- `load` не принимает `Release`, `AcqRel` → CC error (undefined C call).
- `store` не принимает `Acquire`, `AcqRel` → CC error.
- Эти ограничения enforced через отсутствие соответствующих C-функций в
  `sync_primitives.h` — compiler error, не runtime.

#### 3. Wraparound semantics (целочисленный overflow)

**Все integer RMW операции используют модульную арифметику.** Переполнение
не вызывает panic, trap или UB — результат определён спецификацией:

```nova
// AtomicI8 range: -128..127
ro a = AtomicI8.new(127)
ro prev = a.fetch_add(1)    // prev = 127, a.load() = -128 (wraparound)

// AtomicU8 range: 0..255
ro b = AtomicU8.new(255)
ro prev2 = b.fetch_add(1)   // prev2 = 255, b.load() = 0 (wraparound)
```

Wraparound семантика идентична C `uint8_t`/`int8_t` unsigned/signed overflow
согласно стандарту C11 `_Atomic`. Это согласовано с поведением Nova non-atomic
integers (2's complement, no-panic overflow — Plan 101 решение).

**Почему не checked overflow:** atomic increment с panic при overflow —
бесполезен для counters, spinlocks, sequence numbers. Caller несёт
ответственность за выбор ширины типа, достаточной для его диапазона.

#### 4. AtomicPtr — СНЯТ (Plan 207, 2026-07-16 — см. [D426](#d426-atomic-семейство-консолидация-имён-atomicisizeatomicusize--atomicintatomicuint-легаси-atomicint-и-atomicptr-сняты-plan-207))

> Секция ниже — **историческая** (V1 API, Plan 103.2). `AtomicPtr` снят
> целиком: несмотря на заявленное ниже «GC safety», реализация в
> `sync_primitives.h` НИКОГДА не регистрировала GC roots (голый
> `__atomic_*` над `nova_int`) — чистый int-proxy дубль `AtomicInt` под
> другим именем. До появления настоящего typed generic `AtomicPtr[T]` с
> GC-tracing (Plan 103.7) тип не существует; заменить на `AtomicInt`.

`AtomicPtr` хранил `int` (= `intptr_t`) как GC-безопасный адрес (заявлено;
де-факто без root-регистрации). Это **не typed generic** — Plan 103.7
вводит `AtomicPtr[T]` с proper GC-tracing.

V1 семантика (Plan 103.2, снята):
- `AtomicPtr.new(v int)` — создать из raw int (адрес).
- `AtomicPtr.null()` — создать с значением 0 (null-адрес).
- `load()` → `int` — прочитать адрес как int.
- `store(v int)` — записать адрес.
- `swap(v int)` → `int` — атомарный обмен адресов.
- `compare_exchange(expected int, desired int)` → `Result[(), int]` — CAS на
  адресах ([D425](#d425-cas-возвращает-свидетеля-провала-compare_exchange-bool--resultt-plan-207): `Ok(())`/`Err(actual)`, было `bool`).
- Arithmetic (`fetch_add`) **не поддерживается** — AtomicPtr не счётчик.

**GC safety V1 (заявленная, не реализованная):** приложение несёт
ответственность за то, что int-значение в `AtomicPtr` остаётся живым
GC-объектом. V2 (Plan 103.7+): `AtomicPtr[T]` с типизированным GC-root —
откладывается (typed generic в codegen non-trivial); до его прихода
типа `AtomicPtr` в Nova нет.

#### 5. compare_exchange vs compare_exchange_weak

`compare_exchange(expected, desired)` — **strong**: гарантирует успех если
`*self == expected`. На CISC (x86/x64) реализован через `cmpxchg` — no spurious
failures.

`compare_exchange_weak(expected, desired)` — **weak**: может fail spuriously
даже если `*self == expected`. На RISC (ARM, RISC-V) реализован через LL/SC;
spurious fail = retry loop в caller. Более эффективен на ARM для retry loops:

```nova
// Правильный паттерн для weak CAS retry loop:
mut ok = false
while !ok {
    ro cur = a.load(MemOrdering.Relaxed)
    ok = a.compare_exchange_weak(cur, cur + 1, MemOrdering.Release)
}
```

На x86 `compare_exchange_weak` эквивалентен `compare_exchange` (нет
LL/SC — no spurious fails). На ARM различие значительно.

### Почему

1. **12 sized types вместо single `Atomic[T]` generic.** Детерминированная
   ширина критична для lock-free алгоритмов (ABA-prevention через tagged pointer
   требует точной ширины слова). Generic `Atomic[T]` потребовал бы мономорфизацию
   на C-уровне; текущая реализация — 12 конкретных C-struct'ов, по одному на тип.

2. **Default SeqCst + explicit ordering overloads.** Большинство кода не нуждается
   в тонкой настройке ordering. SeqCst по умолчанию безопасен и корректен.
   Explicit overloads — escape hatch для performance-critical hot paths (счётчики,
   sequence numbers, SPSC queues).

3. **Wraparound, не panic.** Атомарные операции применяются в tight loops
   (fetch_add счётчики, sequence numbers). Panic при overflow сделал бы AtomicI8/U8
   непригодными для cyclic counters. Поведение консистентно с Nova integer
   semantics.

4. **AtomicPtr как `int` в V1 (снят, [D426](#d426-atomic-семейство-консолидация-имён-atomicisizeatomicusize--atomicintatomicuint-легаси-atomicint-и-atomicptr-сняты-plan-207)).** Typed `AtomicPtr[T]` требует GC root integration
   в codegen — это non-trivial и откладывается в Plan 103.7+. `int`-proxy без
   root-регистрации оказался дублем `AtomicInt` под другим именем — снят
   целиком до прихода typed generic формы.

5. **compare_exchange_weak — отдельная операция.** На ARM разница ~30% на
   retry-heavy workloads. Наличие обеих форм позволяет писать переносимый код с
   оптимальными характеристиками.

### Что отвергнуто

- **Generic `Atomic[T]` вместо 12 конкретных типов.** Требует мономорфизации на
  C-уровне, усложняет ExternalRegistry (нет arity-based dispatch по типу элемента).
  Отклонено в V1 (12 конкретных типов покрывают все стандартные use cases; generic
  форма как future plan если докажет ценность).

- **Checked overflow для narrow types (AtomicI8).** `fetch_add` на переполненном
  AtomicI8 → panic неожиданен для счётчиков. Wraparound — стандартная C11
  семантика, консистентна с остальными Nova integers.

- **Arity-based dispatch через Nova type system.** Nova integer literals имеют тип
  `nova_int` (широкий), а typed atomic операции принимают `int32_t`/`uint8_t` —
  строгое type matching не работает. Решение (Plan 103.2): arity-based fallback в
  codegen (N vs N+1 параметров для default vs ordering overload) — финализировано.

- **`AtomicPtr.fetch_add(n)` — pointer arithmetic.** Небезопасно без bounds
  checking, противоречит Nova memory safety goals. Pointer arithmetic —
  отдельный unsafe-escaped API, не часть стандартного `AtomicPtr`.

- **Единственный `compare_exchange` без weak variant.** На ARM RISC-V LL/SC
  реализует CAS; weak variant существенно эффективнее для retry loops. Обе
  формы — часть стандартного C11 atomic API.

### Связь

- [D167](#d167-memory-ordering-model--формальная-семантика-happens-before-и-memoryordering)
  — ordering semantics, MemOrdering variants + запреты для load/store.
- [D26](02-types.md#d26-prelude-numeric-types) — prelude numeric types (int, uint,
  bool, u8..u64 и т.д.).
- [D50](#d50-concurrency-model-spawn-detach-blocking) — fiber concurrency model;
  atomic ops — примитив координации между fiber'ами.
- [D138](#d138-default-on-mn-runtime--production-semantics-plan-83456-ф3-active-2026-05-24)
  — production M:N runtime; атомарные операции используются внутри scheduler'а.
- Plan 103.1 — MemOrdering enum + fence(MemOrdering) + nova_mo_c() codegen helper.
- Plan 103.7 — D168 финальная редакция (это plan).
- D370 — AI-first guidance: counter/swap/CAS паттерны выбора atomic типа.
- Plan 103.9+ — AtomicPtr[T] typed generic с GC-root integration (deferred).

### Реализация (Plan 103.2, 2026-05-25)

- `std/runtime/sync.nv`: 12 типов объявлены как `external type AtomicX`
  с полным набором `external fn` объявлений.
- `compiler-codegen/nova_rt/sync_primitives.h`: C реализация через
  `__atomic_*` GCC/Clang builtins. `Nova_AtomicX` = struct с одним полем.
  `nova_mo_c()` helper конвертирует `Nova_MemOrdering*` tag в `__ATOMIC_*` константу.
- `compiler-codegen/src/codegen/emit_c.rs`:
  - ExternalRegistry `last_param_suffix` logic для overload disambiguation.
  - ExternalRegistry → `method_overloads` registration (multi-overload dispatch).
  - Arity-based fallback: когда strict type matching fails и все кандидаты `is_external` —
    arity alone disambiguates (N vs N+1 параметров).
  - Все 12 типов добавлены в `BUILTIN_RUNTIME_TYPES`.

### Эволюция

D168 введён как draft (Plan 103.2, 2026-05-25). Финализирован в Plan 103.7.
Амендирован [D426](#d426-atomic-семейство-консолидация-имён-atomicisizeatomicusize--atomicintatomicuint-легаси-atomicint-и-atomicptr-сняты-plan-207) (Plan 207, 2026-07-16) — консолидация имён.

**Backward compatibility note (устарела, см. D426):** формулировка ниже была
неточна уже на момент написания — pre-103.2 legacy `AtomicInt` был int32-
precision типом без `MemOrdering`, НЕ алиасом `AtomicI64`. Оба факта разошлись
с реальностью; D426 снимает legacy `AtomicInt` и переиспользует имя `AtomicInt`
для бывшего `AtomicIsize` (address-sized, `intptr_t`) — актуальное состояние
см. в таблице §Что выше.

Отложено в Plan 103.7+:
- `AtomicPtr[T]` typed generic с GC root integration (сам нетипизированный
  `AtomicPtr` снят D426 — см. §4).
- Lint `W_NARROW_ATOMIC_OVERFLOW_RISK` для подозрительного использования
  narrow types (AtomicI8/U8) с большими константами.
- ARM CI validation для `compare_exchange_weak` spurious-fail paths.

---

## D171. Once / OnceCell / Lazy — single-initialization primitives (Plan 103.5)

> **Статус:** ✅ final (Plan 103.7, 2026-05-27). Реализован в Plan 103.5. V2 API hygiene — Plan 103.9.

### Что

Nova предоставляет **три single-initialization примитива** для координации
одноразовой инициализации между fiber'ами:

| Тип | Назначение | Хранит значение | Init по требованию |
|---|---|---|---|
| `Once` | one-shot гейт без значения | — | — |
| `OnceCell[T]` | lazy cell, set вручную или через `get_or_init` | `Option[T]` | да |
| `Lazy[T]` | wrapper над `OnceCell[T]` с init-closure в конструкторе | `T` после force | да |

Все три типа являются **value types в Nova**, передаются по reference внутри
fiber-арены (через mutable param или heap-структуру). Семантически — **shared
state с гарантией exactly-once init** при произвольной concurrency.

### Правило

#### 1. OnceState — публичный sum-type

```nova
type OnceState =
    | Fresh       // init ещё не начат
    | Running     // init выполняется (другим fiber'ом)
    | Done        // init успешно завершён
    | Poisoned    // init panicked — все последующие операции re-throw
```

Tag-значения зафиксированы (Fresh=0, Running=1, Done=2, Poisoned=3) для
координации с C runtime (`Nova_OnceState_Tag` в `sync_primitives.h`).

#### 2. Once API

```nova
type Once

namespace Once {
    fn new() -> Once

    /// Выполняет body ровно один раз. Subsequent calls — no-op (DONE)
    /// или re-throw (POISONED).
    /// Concurrent callers: park (fiber) / spin (non-fiber) до завершения runner'а.
    /// Запрещён в realtime context (E_REALTIME_VIOLATION).
    fn call_once(self, body: () -> ()) throws Fail

    /// Heap-allocated snapshot текущего состояния (для match / introspection).
    fn state(self) -> OnceState

    /// true ⟺ state == Done. False для Fresh, Running, Poisoned.
    fn is_completed(self) -> bool

    /// DEPRECATED (W_ONCE_RUN_DONE_DEPRECATED): use call_once.
    /// run() возвращает true ровно одному вызывающему (становится runner'ом).
    /// done() требует matching run()==true иначе runtime panic.
    fn run(self) -> bool          // deprecated
    fn done(self)                 // deprecated, throws Fail if state != Running
}
```

**Poison semantics:** если body в `call_once` panic'ует (через `Fail` effect или
`nova_throw`), Once переходит в `Poisoned` permanently. Все waiting fiber'ы
просыпаются и re-throw тот же panic message. Все subsequent `call_once` тоже
re-throw. Восстановление невозможно — Once одноразовый.

#### 3. OnceCell[T] API

```nova
type OnceCell[T]

namespace OnceCell {
    fn new[T]() -> OnceCell[T]

    /// None если init ещё не выполнен; Some(value) если выполнен.
    fn get[T](self) -> Option[T]

    /// Idempotent set. Возвращает true если значение было установлено первым
    /// (winner); false если кто-то другой уже выполнил set/get_or_init.
    fn set[T](self, v: T) -> bool

    /// Если значение уже есть — вернуть его. Иначе выполнить body ровно один
    /// раз, сохранить результат и вернуть. Re-entrant guard: рекурсивный
    /// вызов get_or_init из тела body → runtime panic (deadlock-prevention).
    /// Запрещён в realtime context.
    fn get_or_init[T](self, body: () -> T) -> T throws Fail

    /// Извлечь значение и сбросить состояние в Fresh. Возвращает Some(v) если
    /// было Done; None для Fresh/Running. Poisoned cells остаются Poisoned.
    /// Не-atomic относительно concurrent get_or_init — caller отвечает за
    /// внешнюю синхронизацию.
    fn take[T](self) -> Option[T]
}
```

**Poison & retry:** в отличие от `Once`, panic в body функции `get_or_init`
не делает cell terminally poisoned в V1. Состояние возвращается в `Fresh`,
позволяя retry. Plan 103.9 пересмотрит poison semantics на основе real-world usage
(возможно введение `Poisoned` варианта с явным `recover()`).

**Re-entrant guard:** если внутри body, переданного в `get_or_init`, происходит
рекурсивный вызов `cell.get_or_init` на том же cell — runtime panic с message
"OnceCell.get_or_init: recursive initialization". Это deadlock-prevention,
не семантическая ошибка ленивой инициализации.

#### 4. Lazy[T] API

```nova
type Lazy[T]

namespace Lazy {
    /// Сохраняет init closure для отложенного вызова. Не выполняет body.
    fn new[T](init: () -> T) -> Lazy[T]

    /// При первом вызове — выполнить init body, сохранить значение,
    /// вернуть. Subsequent calls — вернуть кэшированное значение.
    /// Panic в body → Poisoned (terminal); все subsequent force() re-throw.
    /// Запрещён в realtime context.
    fn force[T](self) -> T throws Fail

    /// true ⟺ force() уже завершён успешно.
    fn is_forced[T](self) -> bool
}
```

**Poison semantics:** в отличие от `OnceCell.get_or_init` (retry-on-panic),
`Lazy.force` имеет terminal Poisoned state. Panic в init closure → все
subsequent `force()` re-throw тот же message. Восстановление невозможно.
Различие мотивировано тем, что init closure хранится в самом Lazy и не может
быть заменён — retry с тем же body даст тот же panic.

#### 5. Memory ordering (D167 contract)

Все три примитива гарантируют **Acquire/Release ordering** между init body и
последующими read'ами:

- Завершение init body **happens-before** любого `get()` / `force()` /
  `is_completed()`, возвращающего успешный результат.
- Запись result-значения (`OnceCell.value`, `Lazy.value`) использует
  `__ATOMIC_RELEASE`; fast-path read — `__ATOMIC_ACQUIRE`.
- Состояния (`Done`, `Poisoned`, `has_value`) публикуются через
  `__atomic_store_n(..., __ATOMIC_RELEASE)` и читаются через
  `__atomic_load_n(..., __ATOMIC_ACQUIRE)`.

Это гарантирует **data-race-free** доступ к закэшированному значению без
дополнительной синхронизации со стороны caller'а.

#### 6. Realtime context forbidden

`Once.call_once`, `OnceCell.get_or_init`, `Lazy.force` могут заблокировать
вызывающий fiber (park до завершения runner'а). Это противоречит realtime
гарантиям (bounded execution time), поэтому:

```nova
fn realtime_handler() with Realtime {
    ro v = lazy.force()       // CC error: E_REALTIME_VIOLATION
    once.call_once { ... }     // CC error: E_REALTIME_VIOLATION
    cell.get_or_init { 42 }    // CC error: E_REALTIME_VIOLATION
}
```

Проверка выполняется в `emit_c.rs` через `in_realtime` флаг (D87 effect-aware
codegen). `get()`, `set()`, `is_completed()`, `is_forced()`, `take()`,
`state()` — разрешены (lock-free fast paths).

### Почему

1. **Три отдельных типа, не один.** `Once` без значения дешевле OnceCell[T]
   когда нужен только гейт (lazy-init глобального ресурса без возврата
   значения). `Lazy[T]` удобнее `OnceCell[T] + get_or_init` когда init body
   известен в конструкторе. Three-tier API покрывает все стандартные use cases
   без overhead.

2. **OnceState public sum-type.** Pattern matching (`match once.state() { Done
   => ..., Poisoned => ... }`) — идиоматичный Nova-стиль для introspection.
   Отдельные предикаты (`is_completed`, `is_forced`) — для fast-path checks без
   аллокации.

3. **Poison terminal в Once/Lazy, retry в OnceCell.** В Once и Lazy init body
   фиксирован (Once: каждый вызов передаёт свой body, но первый panic
   poisonит для всех; Lazy: один body хранится в конструкторе) — retry даст
   тот же panic. В OnceCell body передаётся каждый раз → retry с другим body
   осмыслен. V1 поведение; Plan 103.9 пересмотрит на основе real-world usage.

4. **Re-entrant guard вместо deadlock.** Рекурсивный `get_or_init` без guard'а
   = вечный self-park. Panic с понятным message > undebuggable hang.

5. **Acquire/Release explicit.** SeqCst было бы избыточно (init publishes only
   once). Acquire/Release достаточен для happens-before между init и read,
   с меньшим overhead на ARM (no LDAR-after-DMB-ISH).

6. **Realtime forbidden, не silent slow.** `call_once` может park'нуть fiber
   на произвольное время (зависит от length init body другого fiber'а).
   В realtime context это violation contract'а; CC error лучше, чем missed
   deadline в production.

### Что отвергнуто

- **Single generic `Once[T]` instead of Once + OnceCell + Lazy.** Усложняет
  API (`Once[()]` для unit-case ugly), и `Lazy[T]` всё равно требуется как
  syntactic sugar поверх stored init. Three разных типа = ясный intent.

- **OnceCell poison terminal (как Lazy).** Усложняет retry-after-recover
  паттерн; V1 retry-on-panic совместим с traditional `lazy_static` semantics
  в других языках. Если окажется опасно — Plan 103.9 добавит `poison_mode`
  параметр в `new()`.

- **Lock-free OnceCell через CAS-only.** Реализация через `state` enum +
  mutex + waker list проще и подходит для M:N scheduler'а с park/wake.
  CAS-only сложнее (ABA-prevention для waker list) и не быстрее когда init
  body длинный. Может быть пересмотрено если профилирование покажет.

- **AtomicOnceCell для primitives.** Специализированная версия для
  `int`/`bool`/`f64` без mutex (через 2-word CAS). Избыточно для V1 — общая
  реализация с mutex достаточно быстра для типичных use cases (init
  выполняется один раз, дальше — lock-free fast path read).

- **`OnceCell.set_if_absent` / `swap`.** Лишние операции; `set` + `take` +
  `get_or_init` покрывают все use cases. Минимальный API легче эволюционировать.

### Связи

- [D50](#d50-concurrency-model-spawn-detach-blocking) — fiber model;
  Once/OnceCell/Lazy используются для lazy-init shared state между fibers.
- [D87](#d87-effect-aware-codegen--in_realtime) — `in_realtime` flag в codegen;
  основа для E_REALTIME_VIOLATION проверок.
- [D167](#d167-memory-ordering--happens-before-между-fiberами) — MemOrdering
  enum; Acquire/Release константы из этого D-block'а.
- [D168](#d168-sized-atomic-types--api-contract-plan-1032) — sized atomics;
  внутренние state-поля Once/OnceCell/Lazy реализованы через `__atomic_*`.
- Plan 103.5 — реализация Once hardening + OnceCell + Lazy.
- D370 — AI-first guidance: выбор Once/OnceCell/Lazy по паттерну; decision tree.
- Plan 103.9 — V2 API hygiene pass: возможно удаление `run`/`done`,
  пересмотр OnceCell poison semantics.

### Реализация (Plan 103.5, 2026-05-26)

- `std/runtime/sync.nv`:
  - `external type Once` + методы (`call_once`, `is_completed`, `state`,
    deprecated `run`/`done`).
  - `external type OnceCell[T]` + методы (`new`, `get`, `set`, `get_or_init`,
    `take`).
  - `external type Lazy[T]` + методы (`new`, `force`, `is_forced`).
  - `type OnceState = | Fresh | Running | Done | Poisoned` (declared in Nova).

- `compiler-codegen/nova_rt/sync_primitives.h`:
  - `Nova_Once` struct + `Nova_Once_method_call_once/is_completed/state` +
    deprecated `run`/`done`.
  - `Nova_OnceState` typedef + 4 constructor функций (`nova_make_OnceState_*`).
  - `Nova_Once_method_done` использует unconditional state check через
    `Nova_Fail_fail + nova_throw` (fix: NOVA_SYNC_ASSERT — no-op в Dev builds).

- `compiler-codegen/src/codegen/emit_c.rs`:
  - `emit_oncecell_instance(mangled, t_cty)` — мономорфизирует
    `OnceCell[T]` per instantiation (struct + 5 методов).
  - `emit_lazy_instance(mangled, t_cty)` — мономорфизирует `Lazy[T]` per
    instantiation (struct + 2 методов; stored init closure).
  - `in_realtime` флаг + E_REALTIME_VIOLATION для `call_once`/`get_or_init`/`force`.
  - W_ONCE_RUN_DONE_DEPRECATED warning при использовании `run`/`done`.
  - `"OnceState"` добавлен в `RUNTIME_DEFINED_TYPES` (skip emit_sum_type).

- 20 тестов в `nova_tests/plan103_5/`: 11 positive + 3 negative + 2 property
  + 1 stress (16 fibers × 100 calls). All PASS.

### Эволюция

D171 введён как draft (Plan 103.5, 2026-05-26). Финализирован в Plan 103.7.

D370 (этот plan) содержит AI-first guidance для init-pattern выбора (Once vs
OnceCell vs Lazy) — см. decision tree «exactly-once init» branch.

Отложено в Plan 103.9 (API hygiene pass):
- Удаление deprecated `run`/`done` (после миграционного периода).
- Пересмотр OnceCell poison semantics на основе real-world usage.
- Возможный typed-poison API (`recover() throws PoisonMsg`).

---

## D169. `Mutex` / `RwLock` / `ReentrantMutex` family (Plan 103.3)

> **Статус:** ✅ final (Plan 103.7, 2026-05-27). Реализован в Plan 103.3. V2 consume guards — Plan 103.9.

### Что

Nova предоставляет **три fiber-aware locking примитива**:

| Тип | Назначение | Reentrant | Fairness |
|---|---|---|---|
| `Mutex` | Взаимное исключение, baseline | ❌ (документировано) | fair FIFO default, unfair opt-in |
| `RwLock` | Concurrent reads / exclusive write | ❌ | writer-priority default, reader-priority opt-in |
| `ReentrantMutex` | Рекурсивный mutex для legacy-migration | ✅ | fair FIFO |

Все три типа — **value types в Nova**, передаются через mutable-ссылку или
heap-структуру внутри fiber-арены. Семантически это **blocking coordination
primitives**: вызов `lock()` / `read()` / `write()` при наличии contention
приостанавливает fiber (через `nova_sched_park_with_unlock`), не блокирует OS
thread.

### Правило

**AMEND (2026-07-17, lint-разкраснение W_TRY_WITHOUT_SIBLING, владелец):**
`Mutex.lock_for`/`ReentrantMutex.lock_for` (ex-`try_lock_for`), `RwLock.read_for`/
`RwLock.write_for` (ex-`try_read_for`/`try_write_for`), `Once.start`/`Once.start_won`
(ex-`try_start`/`try_start_won`), `CountDownLatch.await_for` (ex-`try_await_for`),
`Semaphore.acquire_for` (ex-`try_acquire_for`) переименованы — сняли `try_`-префикс.
Правило (R3 D325, nv-coding-style §1): `try_` легален ТОЛЬКО как fallible-половина
ОДНОИМЁННОГО infallible (`from`/`try_from`, D77); ни у одного из этих timeout/racy-
методов нет infallible-сиблинга с ТЕМ ЖЕ именем (`lock()`/`read()`/`write()`/`await()`/
`acquire()` — другая арность/семантика, не тайм-аутный/racy вариант) — соло-операция
без такого сиблинга берёт обычное имя. Сигнатуры/возвраты (`bool`/`Option[OnceGuard]`)
не менялись, только имена. Все примеры/таблицы ниже в этом разделе обновлены на новые
имена; диагностика `W_REALTIME_TRY_LOCK_FOR_TIMER` (§ realtime/blocking ниже) СОХРАНИЛА
свой id (переименование самого id — отдельный D-амендмент, вне периметра этой правки).

#### 1. Mutex API

```nova
module runtime.sync

/// Fair FIFO fiber-aware mutex. NOT reentrant.
/// lock() при contention: park fiber (не блокирует OS thread).
#stable(since = "0.1")
export external fn Mutex.new() -> Self

/// Unfair (LIFO-leaning) opt-in: лучший throughput на высоком contention,
/// возможна starvation. Использовать только после benchmark.
#stable(since = "0.1")
export external fn Mutex.new_unfair() -> Self

#stable(since = "0.1")
export external fn Mutex mut @lock()
#stable(since = "0.1")
export external fn Mutex mut @unlock()
#stable(since = "0.1")
export external fn Mutex mut @try_lock() -> bool

/// Попытаться получить lock в течение timeout.
/// true — acquired; false — timeout истёк.
/// Использует libuv uv_timer_t (Plan 22 / Plan 103.3 pattern).
#stable(since = "0.1")
export external fn Mutex mut @lock_for(timeout Duration) -> bool

/// Best-effort observability. НЕ atomic test-and-set — может гонка.
#stable(since = "0.1")
export external fn Mutex @is_locked() -> bool

/// PREFERRED V1 PATTERN. Closure-form: lock + defer unlock.
/// Unlock выполняется даже при panic в body.
/// V2 (Plan 103.9): тонкая обёртка над MutexGuard consume — без breaking change.
#stable(since = "0.1")
export fn Mutex mut @with_lock[R](body fn() -> R) -> R {
    self.lock()
    defer self.unlock()
    body()
}
```

**Unlock invariant:** `unlock()` без предшествующего `lock()` — unconditional
runtime panic (через `Nova_Fail_fail + nova_throw`), не зависит от build mode.

#### 2. RwLock API

```nova
/// Fiber-aware reader-writer lock. Writer-priority default (M7):
/// новый writer блокирует новых читателей → no writer starvation.
#stable(since = "0.1")
export external fn RwLock.new() -> Self

/// Reader-priority opt-in: читатели не блокируются ожидающим writer'ом.
/// Риск: writer starvation на read-heavy workloads.
#stable(since = "0.1")
export external fn RwLock.new_reader_priority() -> Self

#stable(since = "0.1")
export external fn RwLock mut @read()
#stable(since = "0.1")
export external fn RwLock mut @read_unlock()
#stable(since = "0.1")
export external fn RwLock mut @try_read() -> bool
#stable(since = "0.1")
export external fn RwLock mut @read_for(timeout Duration) -> bool

#stable(since = "0.1")
export external fn RwLock mut @write()
#stable(since = "0.1")
export external fn RwLock mut @write_unlock()
#stable(since = "0.1")
export external fn RwLock mut @try_write() -> bool
#stable(since = "0.1")
export external fn RwLock mut @write_for(timeout Duration) -> bool

/// best-effort снимок (не синхронизирован с reader_count)
#stable(since = "0.1")
export external fn RwLock @reader_count() -> int
#stable(since = "0.1")
export external fn RwLock @is_write_locked() -> bool

#stable(since = "0.1")
export fn RwLock mut @with_read[R](body fn() -> R) -> R {
    self.read()
    defer self.read_unlock()
    body()
}
#stable(since = "0.1")
export fn RwLock mut @with_write[R](body fn() -> R) -> R {
    self.write()
    defer self.write_unlock()
    body()
}
```

**Writer-priority алгоритм (default):**
- `read()`: если `write_locked || write_waiting` → park reader; иначе incr
  `reader_count`.
- `write()`: set `write_waiting=true`; ждать `reader_count=0 &&
  !write_locked` → set `write_locked=true`.
- `write_unlock()`: `write_locked=false`; если есть ожидающие writers →
  разбудить одного; иначе → разбудить всех readers.

**Unlock invariants:** `read_unlock()` без `read()`, `write_unlock()` без
`write()`, и `read_unlock()` после `write()` — unconditional runtime panic.

#### 3. ReentrantMutex API

```nova
/// Reentrant mutex: один fiber может lock() несколько раз без deadlock.
/// Unlock требует соответствующего количества unlock() от того же fiber'а.
///
/// Use case: legacy migration, callbacks-into-locked-context.
/// Recommended default: обычный Mutex (deadlock-detection на ранней стадии).
///
/// Взаимодействие с Condvar (Plan 103.4): Condvar.wait() освобождает ВСЕ
/// уровни lock (count → 0); пробуждение re-acquires с count = 1.
/// Исходная глубина рекурсии НЕ восстанавливается.
/// Диагностика: W_REENTRANT_CONDVAR_RECOMMEND при mix.
#stable(since = "0.1")
export external fn ReentrantMutex.new() -> Self

#stable(since = "0.1")
export external fn ReentrantMutex mut @lock()
#stable(since = "0.1")
export external fn ReentrantMutex mut @unlock()
#stable(since = "0.1")
export external fn ReentrantMutex mut @try_lock() -> bool
#stable(since = "0.1")
export external fn ReentrantMutex mut @lock_for(timeout Duration) -> bool

/// Глубина рекурсии для текущего fiber'а; 0 если не locked этим fiber'ом.
#stable(since = "0.1")
export external fn ReentrantMutex @lock_count() -> int

#stable(since = "0.1")
export fn ReentrantMutex mut @with_lock[R](body fn() -> R) -> R {
    self.lock()
    defer self.unlock()
    body()
}
```

**Owner tracking:** `owner_coro = mco_running()` (thread-local `mco_coro*`
из minicoro.h). `NULL` на main thread или вне `mco_resume`. Уникален на всём
протяжении жизни fiber'а (не переиспользуется пока mutex locked).

**Unlock invariant:** `unlock()` не от owner fiber → unconditional runtime panic.

#### 4. C runtime layer

Все три типа аллоцируются через `nova_alloc_uncollectable` (Boehm
`GC_malloc_uncollectable`):

```c
static inline Nova_Mutex* Nova_Mutex_static_new(void) {
    Nova_Mutex* m = (Nova_Mutex*)nova_alloc_uncollectable(sizeof(Nova_Mutex));
    ...
}
```

**Причина:** на Windows под M:N runtime первый `supervised{spawn{}}` вызывает
`_ensure_materialized()` → `nova_scope_grow` (7× `nova_alloc`) → Boehm GC
может не видеть pointer на sync primitive, хранящийся на стеке main thread'а,
и произвести premature collection. `GC_malloc_uncollectable` полностью
исключает эту проблему (объект не собирается GC, но сканируется на interior
pointers).

**Plan 151 (2026-06-13) — КОРНЕВОЙ фикс той же проблемы.** `GC_malloc_uncollectable`
выше защищает ТОЛЬКО sync-примитивы; произвольные heap-объекты на main-стеке (напр.
замыкание `body` в `supervised{spawn{ ro r = body() }}`) оставались уязвимы — при ≥4
worker'ах GC во время `_materialize_pool` (до создания ленивой main-арены) не видел
main-стек → premature collect → `closure->fn = 0` → worker зовёт NULL → RIP=0
(маскируется под «fiber stack overflow in slot 0»; диагностика — [docs/dev/debugging-races.md
§2.1.1](../../docs/dev/debugging-races.md)). **Фикс (`nova_rt/fiber_arena_win.c`):** main-thread
`NT_TIB.StackBase` фиксируется ДО создания worker-пула (`nova_fiber_arena_set_main_stack`,
зовётся из `_materialize_pool`); его **committed-only** регион (`MEM_COMMIT && !PAGE_GUARD
&& !PAGE_NOACCESS`) пушится в GC `push_other_roots`-колбэк. Обобщает решение на ВСЕ
main-stack-rooted объекты; `GC_malloc_uncollectable` для sync-примитивов остаётся как
defense-in-depth. **Инвариант (M:N):** каждый root-bearing native-стек (main + worker-
арены) ДОЛЖЕН сканироваться GC до любого STW. Дискриминаторы GC-collect-vs-overflow:
`GC_DONT_GC=1` чинит; `NOVA_FIBER_STACK=64MB` не помогает; `MAXPROCS≤3` pass / `≥4` fail.
Будущий hardening — единый global stack-registry (`[M-mn-gc-root-unified-stack-registry]`, → Plan 144).

#### 5. Realtime context ban

Методы `lock()` / `read()` / `write()` и `lock_for` / `read_for` /
`write_for` — **запрещены в `realtime { }` блоках** (Plan 103.6):
они могут park fiber, нарушая realtime-гарантию. `try_lock()` / `try_read()`
/ `try_write()` без timeout разрешены (no park, return bool немедленно).

Диагностика: `E_REALTIME_VIOLATION` (compile-time).

### Отвергнутые альтернативы

| Альтернатива | Причина отклонения |
|---|---|
| **`Mutex<T>` data-carrying** (Rust style) | M4: требует borrow checker; в Nova consume-типы (Plan 103.9) решают проблему по-другому |
| **Mutex poisoning** (`LockResult`) | M5: сложность без реального преимущества в fiber-модели; Nova предпочитает явные `defer panic` |
| **Upgradeable read lock** для RwLock | Сложная семантика (deadlock-risk); отдельный future plan |
| **RwLock reader-priority по умолчанию** | Writer starvation в нагрузочных тестах; writer-priority = better default |
| **`(scope, slot)` как ReentrantMutex owner-id** | Risk использования после fiber завершения; `mco_coro*` гарантированно валиден пока fiber активен |
| **UUID для owner tracking** | Overhead; `mco_coro*` проще и надёжнее в контексте Nova fiber runtime |

### Связь

- **[D50](#d50-concurrency-model-spawn-detach-blocking)** — `supervised`,
  `spawn`; Mutex park работает внутри supervised-дерева.
- **[D138](#d138-default-on-mn-runtime--production-semantics-plan-83456-ф3-active-2026-05-24)** —
  M:N runtime; uncollectable alloc fix специфичен для M:N + Windows Boehm.
- **[D168](#d168-sized-atomic-types--api-contract-plan-1032)** —
  AtomicI32 для `reader_count`; AtomicBool для `write_locked`.
- **[D171](#d171-once--oncecell--lazy--single-initialization-primitives-plan-1035)** —
  аналогичный паттерн uncollectable alloc для sync primitive structs.
- **[Plan 103.4](../../docs/plans/103.4-coordination-primitives.md)** —
  `Condvar` tied to `Mutex`; `W_REENTRANT_CONDVAR_RECOMMEND`.
- **[Plan 103.6](../../docs/plans/103.6-realtime-blocking-integration.md)** —
  `realtime { }` ban на park-ing methods.
- **[Plan 103.7](../../docs/plans/103.7-spec-d-blocks.md)** — D169 final closure.
- **[Plan 103.9](../../docs/plans/103.9-consume-guards-migration.md)** —
  V2: `MutexGuard consume`; `with_lock(fn)` → non-breaking migration path.

### Эволюция

D169 введён как draft (Plan 103.3, 2026-05-26). Финализирован в Plan 103.7.

D370 (этот plan) содержит AI-first guidance: выбор Mutex vs RwLock vs
ReentrantMutex по паттерну — см. decision tree «exclusive access» branch +
canonical patterns 3 (producer-consumer) и 4 (read-heavy snapshot).

Отложено в Plan 103.9 (V2):
- `MutexGuard consume` заменяет `lock()/unlock()` как primary API.
- `with_lock(fn)` становится thin wrapper над guard — user code не меняется.
- `W_REENTRANT_CONDVAR_RECOMMEND` переходит в E_REENTRANT_CONDVAR_ERROR если
  статически выявимо (Plan 103.4 + checker).

---

## D170. Coordination primitives — Semaphore / Barrier / CountDownLatch / Condvar (Plan 103.4)

> **Статус:** ✅ final (Plan 103.7, 2026-05-27). Реализован в Plan 103.4. V2 consume guards — Plan 103.9.

### Контекст

После [D169](#d169) (Mutex/RwLock/ReentrantMutex), `std.runtime.sync`
содержит lock-family. Coordination patterns (bounded concurrency, N-party
rendezvous, one-shot signal, "wait until predicate") требуют отдельные
примитивы. D170 закрывает industry gap vs Go (только channels), Rust
(`std::sync::Barrier` + tokio Semaphore), Java (полный набор), Kotlin
(только Semaphore).

### API surface

#### Semaphore — bounded counting permits (M11)

```nova
type Semaphore  /* opaque */

fn Semaphore.new(permits int) -> Self     /* permits >= 0 */
fn Semaphore mut @acquire()                /* parks until permit available */
fn Semaphore mut @release()                /* incr permits, wake FIFO head */
fn Semaphore mut @try_acquire() -> Option[Permit]  /* AMEND 2026-07-26 (№R3-ревью
   владельца, D325 R3): non-parking половина пары acquire/try_acquire, Permit-guard
   (@cleanup авто-release, D432). Прежние ДВЕ двери ретрактированы: bool-форма
   `try_acquire() -> bool` (manual-release анти-паттерн, D9) стала приватным
   мостом @try_acquire_raw; `try_acquire_permit` (R3-нарушитель: сиблинга
   acquire_permit не существовало) переименован в try_acquire. */
fn Semaphore mut @acquire_for(timeout Duration) -> bool
fn Semaphore mut @acquire_n(n int)         /* batch */
fn Semaphore mut @release_n(n int)
fn Semaphore @available_permits() -> int   /* best-effort */
fn Semaphore mut @with_permit[R](body fn() -> R) -> R  /* M15 V1 helper */
```

**Семантика:**
- **Bounded:** initial permits = upper bound; `release()` past initial → permits
  растёт (Java behavior; `W_SEMAPHORE_OVER_RELEASE` lint опционально в V2).
- **Fair FIFO** (M6 consistency с Mutex default). Unfair вариант — не V1.
- **Negative init permits** → runtime panic.
- **`with_permit(fn)`** — preferred V1 pattern (M15); V2 → `Permit consume`
  guard ([Plan 103.9](../../docs/plans/103.9-consume-guards-migration.md)).
- **`acquire`/`acquire_n`/`acquire_for`/`with_permit`** — park-ing methods,
  banned в `realtime { }` (Plan 103.6 enforcement; M12).

#### Barrier — reusable N-party rendezvous (CyclicBarrier-style)

```nova
type Barrier  /* opaque */

fn Barrier.new(parties int) -> Self                    /* parties >= 1 */
fn Barrier mut @wait() -> int                          /* arrival index 0..parties-1 */
fn Barrier mut @wait_with_action(action fn() -> ()) -> int
fn Barrier mut @wait_for(timeout Duration) -> Option[int]
fn Barrier @is_broken() -> bool
fn Barrier mut @reset()
```

**Семантика:**
- **Reusable cyclic:** после того как `parties` fibers вызвали `wait()`, счётчик
  атомарно сбрасывается, `generation++`; следующий round начинается.
- **Arrival index:** last-arrival fiber получает `parties-1` (может выполнить
  `action` если использован `wait_with_action`).
- **`wait_with_action(action)`:** action выполняется last-arrival fiber'ом ВНУТРИ
  барьера; остальные waiters wake только после завершения action.
- **`wait_for(timeout)`:** возврат `None` ⇒ barrier broken (все текущие waiters
  released с `broken=true`).
- **`broken` state:** если any fiber в barrier interrupted/cancelled/timed out
  → barrier broken; все waiters просыпаются и видят broken. `reset()` сбрасывает
  broken и начинает новый round.
- **`Barrier.new(0)`** → runtime panic (`parties >= 1`).
- **`wait`/`wait_with_action`/`wait_for`** — park-ing, banned в `realtime { }`.

#### CountDownLatch — one-shot signal (Java-style)

```nova
type CountDownLatch  /* opaque */

fn CountDownLatch.new(count int) -> Self           /* count > 0 */
fn CountDownLatch mut @count_down()                /* saturating: count==0 -> no-op */
fn CountDownLatch mut @count_down_n(n int)         /* batch saturating */
fn CountDownLatch @await()                         /* park until count == 0 */
fn CountDownLatch @try_await() -> bool
fn CountDownLatch @await_for(timeout Duration) -> bool
fn CountDownLatch @current_count() -> int          /* best-effort */
```

**Семантика:**
- **Immutable initial count** (safer than WaitGroup, который позволяет `add()`
  после `wait()` → race risk).
- **`count_down()` saturating:** вызов когда `count == 0` — no-op (НЕ panic);
  Java parity.
- **`count_down_n(n)`:** `n <= 0` → no-op; `n > current count` → saturates на 0.
- **`CountDownLatch.new(0)`** → runtime panic (`count > 0`).
- **`await`/`await_for`** — park-ing, banned в `realtime { }`.

#### Condvar — condition variable tied to Mutex (M10)

```nova
type Condvar  /* opaque */
type WaitResult { Notified | TimedOut }

fn Condvar.new() -> Self

/* Mutex overload (primary): */
fn Condvar @wait(m mut Mutex)
fn Condvar @wait_for(m mut Mutex, timeout Duration) -> WaitResult
fn Condvar @wait_until(m mut Mutex, predicate fn() -> bool)

/* ReentrantMutex overload (Java-pitfall-aware): */
fn Condvar @wait(m mut ReentrantMutex)

fn Condvar mut @notify_one()        /* wake FIFO head */
fn Condvar mut @notify_all()        /* wake all FIFO order */
```

**Семантика:**
- **Tied to Mutex (M10):** wait требует уже-locked mutex (precondition runtime-
  enforced unconditional throw, не debug-assert). Mutex освобождается атомарно
  с парковкой (`park_with_unlock` pattern); re-acquired на wake.
- **Spurious wakeup contract:** `wait()` может вернуться без notify (scheduler
  rebalance, M:N migration). Caller обязан использовать predicate loop или
  `wait_until(m, predicate)` helper.
- **`wait_for(m, timeout)` -> `WaitResult { Notified | TimedOut }`:** typed
  возврат, не bool — лучше Rust `Result` API.
- **ReentrantMutex overload:** wait освобождает ВЕСЬ recursive lock_count
  (count -> 0). На wake re-acquired как `count=1` (НЕ restored original count
  — Java pitfall: восстановление count может deadlock'нуть). Caller осведомлён
  через `W_REENTRANT_CONDVAR_RECOMMEND` lint (type-checker hook когда
  inferred тип = ReentrantMutex).
- **FIFO wake order:** `notify_one()` wakes oldest waiter (fair); `notify_all()`
  wakes all в порядке регистрации.
- **`wait`/`wait_for`/`wait_until`** — park-ing, banned в `realtime { }`.

### Дизайн-решения

- **M10 (Condvar tied to Mutex):** type-safer чем Java `Condition` (loosely tied
  к `Lock`). Compiler-enforced связь through API signature.
- **M11 (bounded Semaphore):** unbounded counting — отдельная concept, для этого
  use `AtomicI32` напрямую (D168). Dedicated unbounded type — over-engineering.
- **M15 (with_permit / with_lock — V1 helpers; consume guards — V2):**
  `with_permit(fn)` consistent с `Mutex.with_lock(fn)` — closure-based scoping
  ergonomic для V1. V2 ([Plan 103.9](../../docs/plans/103.9-consume-guards-migration.md))
  добавляет `Permit consume` guard (RAII-style); `with_permit` остаётся как
  thin wrapper, user code не меняется.
- **No `Phaser`:** Java's Phaser over-engineered (dynamic party count, multi-
  phase advancement). Barrier + WaitGroup покрывают realistic use cases.
- **No writer-priority Condvar:** Rust `parking_lot::Condvar` имеет
  `notify_one_writer` semantics — отложено в V2 если запрос.
- **No `Barrier.cyclic_action` через closure-with-effects (V1):** action runs
  inline в last-arrival fiber как `fn() -> ()`. Effect-typed action — V2.

### Запреты / соглашения

- **`realtime { }` ban:** все park-ing methods (`Semaphore.acquire`,
  `Barrier.wait`, `CountDownLatch.await`, `Condvar.wait`) banned внутри
  `realtime { }` блока (Plan 103.6 type-checker enforcement; M12).
- **`Condvar.wait(reentrant_mutex)` warning:** `W_REENTRANT_CONDVAR_RECOMMEND`
  — рекомендует regular Mutex (Java pitfall preempted by design).
- **Stability:** `#stable(since = "0.1")` на всё.

### Реализация

- **Runtime:** `compiler-codegen/nova_rt/sync_semaphore.h`,
  `sync_barrier.h`, `sync_countdown_latch.h`, `sync_condvar.h` —
  per-primitive header files (отдельные от `sync_primitives.h`).
- **GC race fix:** `nova_alloc_uncollectable` для all four `static_new()`
  (Plan 103.3 D169 pattern — Boehm misses pointer на main stack под M:N).
- **FIFO waiter lists:** doubly-linked, stack-allocated waiter nodes
  (WaitGroup precedent); dequeue под inner mutex.
- **Condvar park_with_unlock:** combined callback releases cv->mu AND user
  mutex atomically после yield (lost-wakeup fix).
- **Memory ordering:** `__ATOMIC_SEQ_CST` для parked[slot] store
  (Plan 83.10.2 + Plan 103.4 Ф.2 fix — `__ATOMIC_RELEASE` на x86
  компилируется в plain MOV без fence → store buffer не flush'ит).
- **Build:** `sync_primitives.h` includes `sync_<primitive>.h` (alphabetical
  parallel-merge markers — Plan 103.4 parallel agent split).

### Тестовое покрытие

`nova_tests/plan103_4/` — 25 tests:
- **Semaphore (7):** bounded_concurrency, batch_n, acquire_for_timeout,
  with_permit_panic_safety, no_overcommit_prop, release_more_than_acquired_neg,
  negative_init_permits_neg.
- **Barrier (5):** n_party_rendezvous, cyclic_reusable, wait_with_action,
  all_or_none_prop, zero_parties_neg.
- **CountDownLatch (4):** one_shot_signal, count_down_n,
  count_down_at_zero_neg, init_zero_or_negative_neg.
- **Condvar (9):** notify_one, notify_all, wait_for_timeout, wait_until_predicate,
  producer_consumer, no_lost_wakeup_prop, wait_without_lock_neg,
  in_realtime_neg (TODO Plan 103.6), with_reentrant_warn.

### Связь

- **[D168](#d168-sized-atomic-types--api-contract-plan-1032)** — sized atomic types (используются internal для counters).
- **[D169](#d169-mutex--rwlock--reentrantmutex-family-plan-1033)** — Mutex/RwLock/ReentrantMutex (required для Condvar).
- **[Plan 22](../../docs/plans/22-sleep-libuv-integration.md)** — `Duration`
  + libuv timer (для `*_for` timeouts).
- **[Plan 47](../../docs/plans/47-supervised-cancel.md)** — cancel-token
  propagation through wait methods (V2 cancel integration).
- **[Plan 103.6](../../docs/plans/103.6-realtime-blocking-integration.md)** —
  `realtime { }` ban enforcement.
- **[Plan 103.7](../../docs/plans/103.7-spec-d-blocks.md)** — D170 final closure.
- **[Plan 103.9](../../docs/plans/103.9-consume-guards-migration.md)** —
  V2: `Permit consume` для Semaphore.

### Эволюция

D170 введён как draft (Plan 103.4, 2026-05-27). Финализирован в Plan 103.7.

D370 (этот plan) содержит AI-first guidance: rate-limited workers (Semaphore),
N-party rendezvous (Barrier/CountDownLatch), wait-until-predicate (Condvar) —
см. canonical patterns 3 и 5 + decision tree lower branches.

Отложено в Plan 103.9 (V2):
- `Permit consume` guard заменяет `acquire()/release()` как primary API для
  Semaphore.
- `with_permit(fn)` остаётся thin wrapper над guard.
- Barrier/CountDownLatch/Condvar — НЕ мигрируются на consume guards
  (M16: stateless по природе).

---

## D172. `#realtime` / `#blocking` attribute-only model + sync-class annotations (Plan 103.6, amended Plan 113)

> **Статус:** ✅ final (Plan 103.7, 2026-05-27). **Amended by Plan 113** (2026-05-29): block-forms removed, `#realtime_safe` → `#realtime`, `#blocking fn` replaces `blocking { }`. V2 inference — Plan 103.8.

### Что

**Plan 113 (2026-05-29) — attribute-only simplification.** Две исходных формы
заменены единым механизмом — attribute на функции:

- **`#realtime fn`** — callee guarantee: тело fn может вызывать только другие
  `#realtime` fns/primitives. GC-pause-free, scheduler-interaction-free.
  Caller unrestricted — любая fn свободно вызывает `#realtime` fn.
  _До Plan 113: `realtime { }` block (D64 — retracted) и `#realtime_safe` SyncClass._
- **`#realtime nogc fn`** — амендмент №273 (2026-08-02): опциональный
  `nogc`-модификатор на `#realtime fn`, жёсткий режим. Всё из `#realtime`
  (suspend-ban) **плюс** запрет вызовов, аллоцирующих на managed heap
  (`Vec[T].new`/`.of`/`.with_capacity`, `HashMap.new`, `StringBuilder.new`,
  `str.from`, ...) — см. §7 ниже. Синтаксис и намерение — прямое
  продолжение retracted-блока `realtime nogc { }` (D64: «никаких аллокаций,
  кроме как в region'е»); D172 (Plan 103.7 final, Plan 113 amend) описывал
  только `#realtime`/`#blocking`/`#thread_affine` — атрибут `nogc` парсился
  и частично enforced'ился (Plan 16 `nogc_blacklisted_call`), но не был
  задокументирован в этом D-блоке. №273 закрывает и документационный
  разрыв, и найденную дыру enforcement'а (см. §7).
  _До Plan 113: `realtime nogc { }` block (D64 §«Опционально — запрет
  аллокации»)._ `region { ... }` (D6) исключение из запрета — в текущем
  компиляторе **не реализовано** (`region` не парсится как block-конструкция),
  поэтому исключений из nogc-запрета сейчас нет никаких.
- **`#blocking fn`** — runtime threadpool offload: вся fn выполняется на
  libuv threadpool worker, fiber паркуется до завершения.
  _До Plan 113: `blocking { }` block (D50 §4, Plan 83.3)._
- **`#thread_affine extern fn`** — M:N-небезопасный лист (A-V10, D441 §5
  amend, реестр 221.1 №167): маркирует `extern fn`, привязанную к ОС-потоку,
  который её впервые вызвал (thread-local состояние / нереэнтерабельный
  C-side handle) — семантика «привязан к ОС-потоку», НЕ «медленный»
  (`#blocking` — другая ось). Транзитивно поднимается по именованному графу
  вызовов (fn, зовущая лист прямо или транзитивно В СВОЁМ файбере, сама
  становится thread-affine); вызов на границе `spawn`/`detach`/`parallel for`
  — `E_THREAD_AFFINE_IN_FIBER` (см. D441 §5).

**Аналогия (Plan 113):** `#realtime` — как C++ `constexpr`: вызываема из
runtime-кода, но внутри только constexpr/realtime ops. Callee guarantee,
не caller constraint.

Исходные execution-context блоки (до Plan 113):

- **`realtime { }` (D64)** — *(retracted, Plan 113)* заменён на `#realtime fn`.
- **`blocking { }` (D50 §4, Plan 83.3)** — *(retracted, Plan 113)* заменён на `#blocking fn`.

### Проблема (до Plan 103.6)

Enforcement realtime/blocking-ограничений был **hardcoded в emit_c.rs**:
```rust
// До Plan 103.6 (hardcoded match-список)
fn is_realtime_blocking(recv: &str, method: &str) -> bool {
    matches!((recv, method),
        ("Mutex", "lock") | ("Mutex", "wait") | ("RwLock", "read") | ...)
}
```
Проблемы:
1. Список не синхронизирован с реальной реализацией sync-примитивов.
2. Добавление нового примитива требует патча compiler (не spec.nv).
3. Нет различия между `#parks` (park fiber) и `#wakes` (wake other fibers).
4. Нет механизма для user-определённых функций.

### Решение: annotation-driven sync-class

Plan 103.6 вводит **SyncClass attribute system**:

#### §1. Annotations (bare `#`-attributes)

```nova
#realtime  // Leaf op: no scheduler interaction. Safe in realtime{} and blocking{}.
#parks          // May park the current fiber. Forbidden in realtime{}. Error in blocking{}.
#wakes          // May wake another fiber (scheduler signal). Forbidden in realtime{}.
```

Аннотации ставятся **перед** `export external fn` в `.nv` файлах:

```nova
#parks
#stable(since = "0.1")
export external fn Mutex mut @lock()

#realtime
#stable(since = "0.1")
export external fn Mutex @try_lock() -> bool

#wakes
#stable(since = "0.1")
export external fn Mutex mut @unlock()
```

#### §2. Матрица sync-class

| Примитив | Метод | SyncClass | realtime{} | blocking{} |
|---|---|---|---|---|
| `Mutex` | `lock()` | `#parks` | ❌ E_REALTIME_SYNC_PARK | ❌ E_BLOCKING_SYNC_PARK |
| `Mutex` | `try_lock()` | `#realtime` | ✅ | ✅ |
| `Mutex` | `lock_for(d)` | `#realtime` ¹ | ⚠️ W_REALTIME_TRY_LOCK_FOR_TIMER | ✅ |
| `Mutex` | `unlock()` | `#wakes` | ❌ E_REALTIME_SYNC_WAKE | ✅ |
| `RwLock` | `read()` / `write()` | `#parks` | ❌ E_REALTIME_SYNC_PARK | ❌ E_BLOCKING_SYNC_PARK |
| `RwLock` | `try_read()` / `try_write()` | `#realtime` | ✅ | ✅ |
| `RwLock` | `unlock_read()` / `unlock_write()` | `#wakes` | ❌ E_REALTIME_SYNC_WAKE | ✅ |
| `Semaphore` | `acquire()` | `#parks` | ❌ E_REALTIME_SYNC_PARK | ❌ E_BLOCKING_SYNC_PARK |
| `Semaphore` | `try_acquire()` | `#realtime` | ✅ | ✅ |
| `Semaphore` | `release()` | `#wakes` | ❌ E_REALTIME_SYNC_WAKE | ✅ |
| `Barrier` | `wait()` | `#parks` | ❌ E_REALTIME_SYNC_PARK | ❌ E_BLOCKING_SYNC_PARK |
| `CountDownLatch` | `await()` | `#parks` | ❌ E_REALTIME_SYNC_PARK | ❌ E_BLOCKING_SYNC_PARK |
| `CountDownLatch` | `count_down()` | `#wakes` | ❌ E_REALTIME_SYNC_WAKE | ✅ |
| `Condvar` | `wait(m)` | `#parks` | ❌ E_REALTIME_SYNC_PARK | ❌ E_BLOCKING_SYNC_PARK |
| `Condvar` | `notify_one()` / `notify_all()` | `#wakes` | ❌ E_REALTIME_SYNC_WAKE | ⚠️ W_BLOCKING_NOTIFY_RISK ² |
| `WaitGroup` | `wait()` | `#parks` | ❌ E_REALTIME_SYNC_PARK | ❌ E_BLOCKING_SYNC_PARK |
| `WaitGroup` | `done()` | `#wakes` | ❌ E_REALTIME_SYNC_WAKE | ✅ |
| `OnceCell[T]` | `get_or_init(f)` | `#parks` | ❌ E_REALTIME_SYNC_PARK | ❌ E_BLOCKING_SYNC_PARK |
| `OnceCell[T]` | `get()` / `set(v)` | `#realtime` | ✅ | ✅ |
| `Lazy[T]` | `force()` | `#parks` | ❌ E_REALTIME_SYNC_PARK | ❌ E_BLOCKING_SYNC_PARK |
| `Lazy[T]` | `is_forced()` | `#realtime` | ✅ | ✅ |
| `Once` | `call_once(f)` | `#parks` | ❌ E_REALTIME_SYNC_PARK | ❌ E_BLOCKING_SYNC_PARK |
| `Once` | `is_completed()` | `#realtime` | ✅ | ✅ |
| `fence(ord)` | — | `#realtime` | ✅ | ✅ |
| `AtomicX.*` | все методы | `#realtime` | ✅ | ✅ |

¹ `lock_for` является `#realtime` технически (не парков fiber),
  но использует libuv timer → W_REALTIME_TRY_LOCK_FOR_TIMER предупреждает
  об overhead таймера внутри realtime-блока.

² `notify_one/notify_all` в blocking{} работает (wake технически возможен),
  но wake другого nova-fiber изнутри threadpool worker семантически сомнительен
  → W_BLOCKING_NOTIFY_RISK (design decision: prefer fiber-native patterns).

#### §3. Error codes

| Код | Уровень | Условие |
|---|---|---|
| `E_REALTIME_SYNC_PARK` | error | `#parks`-метод вызван внутри `realtime { }` |
| `E_REALTIME_SYNC_WAKE` | error | `#wakes`-метод вызван внутри `realtime { }` |
| `E_REALTIME_NESTED_SYNC_VIA_FN` | error | user-fn с `#parks`-аннотацией вызвана из `realtime { }` |
| `E_BLOCKING_SYNC_PARK` | error | `#parks`-метод вызван внутри `blocking { }` |
| `E_REALTIME_NOGC_ALLOC` | error | allocating-вызов (blacklist, §7) внутри `#realtime nogc fn` (амендмент №273) |
| `E_BLOCKING_NOGC_ALLOC` | error | allocating-вызов внутри `blocking { }` body (V1 leaf-contract, Plan 83.3) — код зарезервирован, ветка сейчас недостижима: block-form `blocking { }` retracted Plan 113, `#blocking fn` не заводит этот флаг (см. §7) |
| `W_REALTIME_TRY_LOCK_FOR_TIMER` | warning | `Mutex.lock_for` в `realtime { }` |
| `W_BLOCKING_NOTIFY_RISK` | warning | `#wakes`-метод в `blocking { }` |

#### §4. User-defined function propagation (V1)

В V1 (Plan 103.6 / Plan 113) propagation **только explicit**:

```nova
#parks
fn my_critical_wait() {
    mutex.lock()  // внутри — парков, поэтому fn annotated #parks
}

#realtime
fn audio_callback() {
    my_critical_wait()  // ❌ E_REALTIME_NESTED_SYNC_VIA_FN
}
```

V1 не поддерживает автоматический inference (transitive propagation): если `fn A`
вызывает `fn B` которая `#parks`, но `A` не annotated — вызов `A` из `#realtime` fn
**не** даёт ошибку. V2 (Plan 103.8) добавит inference-based propagation.

#### §5. Unseen / uninstrumented methods

Методы без явной аннотации (`#parks`/`#wakes`/`#realtime`) внутри
realtime{} консервативно трактуются как `#parks` (worst-case):

```
[E_REALTIME_SYNC_PARK] `T.method()` has no sync annotation and is conservatively
treated as park-ing; forbidden inside realtime{}.
Add #parks / #realtime annotation to declare intent.
```

Это предотвращает silent miscompilation при добавлении новых методов без аннотации.

#### §6. Implementation (V1, updated Plan 113)

**Compiler-side:**
- `SyncClass` enum в AST: `Realtime | Parks | Wakes` _(Plan 113: `RealtimeSafe` → `Realtime`)_
- `ContractAttrs.sync_class: Option<SyncClass>` — parsed из `#realtime`/`#parks`/`#wakes`
- `RealtimeAttr` enum на `FnDecl` — body-restriction enforcement для `#realtime fn` bodies
- `CEmitter.in_realtime: bool` / `CEmitter.in_blocking: bool` — flags set при входе в fn bodies
- `mono_fn_decls` расширен: non-generic `#parks`-annotated fns хранятся для lookup в emit_call
- Generic-type methods (OnceCell[T], Lazy[T]) — проверяются в `generic_type_methods` dispatch

**Runtime-side:**
- Нет runtime overhead: все проверки compile-time.
- `nova_fn_fence` / атомарные операции — безусловно safe (нет park/wake).
- `#blocking fn` — codegen wrap'ает вызов в `uv_queue_work` (Plan 113, Ф.3).

#### §7. `#realtime nogc fn` — no-alloc restriction (amend №273, 2026-08-02)

**Механизм — hardcoded call-name blacklist**, НЕ call-graph анализ:
`nogc_blacklisted_call` (`compiler-codegen/src/types/mod.rs`, checker-канал,
`CapabilityCtx::check_capabilities_at`) matches'ит `Type.method` пары
известных allocating-конструкторов (`[]T.new/.with_capacity/.of`,
`HashMap/Set/Vec/Deque/LinkedList/Lru/BloomFilter.new/.with_capacity/.of`,
`StringBuilder/WriteBuffer/ReadBuffer.new/.with_capacity/.from`,
`Channel.new/.with_capacity`, `str.from`). Внутри `#realtime nogc fn` вызов
одного из них → `[E_REALTIME_NOGC_ALLOC]`.

**Находка №273 (2026-08-02):** до этого амендмента список matches'ил
`.new`/`.with_capacity`, но **не** `.of` — вариадик-литерал-конструктор,
ставший каноническим способом строить `Vec[T]` из литерала (D259 amend);
`Vec[int].of(1, 2, 3)` внутри `#realtime nogc fn` проходил `nova check`
молча. Хуже: сама проверка вообще не доходила ни до какого имени, если
receiver вызова нёс **explicit generic type argument** — `Vec[int].new()`
парсится как `Member{obj: TurboFish{base: Ident("Vec"), ..}, name: "new"}`
(D38 turbofish), а path-extraction в `check_capabilities_at` не разворачивал
`TurboFish`, попадая в defensive `_ => return` — silently skip'ая **все**
capability-проверки этой функции (не только nogc-alloc, но и `forbid`/
effect-checks) для любого вызова с explicit generic-типизированным
receiver'ом. Оба дефекта закрыты в одном слиянии — детали в
`nogc_blacklisted_call`'s doc-comment (types/mod.rs) и `check_capabilities_at`.

**Известные пределы (documented risk, аналогично `blocking { }` V1
leaf-контракту §выше):**
- **Не call-graph.** Если `fn f()` без `#realtime`/`#realtime nogc`
  annotation сама вызывает allocating-конструктор, а `#realtime nogc fn`
  вызывает `f()` — не ловится: нет transitive inference, тот же V1-предел,
  что и `#parks`-propagation (§4). Честная альтернатива — call-graph
  may-GC-анализ (Plan 144.0, `nova gc-effect-analyze`,
  `codegen/may_gc.rs`, `MayGcSet`), но он рассчитан на пост-mono
  codegen-tier, другую фазу пайплайна, чем pre-mono checker-walk;
  интеграция — отдельная задача, вне периметра №273.
- **User-defined record-конструкторы** (`Foo.new()`) не покрыты: codegen
  всегда heap-боксит record-литералы, так что фактически любой
  record-литерал «аллоцирующий», но detection требует bigger inference —
  сейчас флагуются только статические factory-методы известных типов.
- **`Vec.of` allocates via variadic arg packing**, не через тело `of`
  (`export fn Vec[T].of(...args []T) -> Self => args` — тело тривиально);
  блэклист матчит по имени call-сайта, поэтому это не проблема, но
  подчёркивает: список — по callee-имени, не по анализу тела callee.
- **`region { ... }`-исключение (D6) не реализовано** — `region` не
  парсится как block-конструкция в текущем компиляторе (см. `#realtime
  nogc fn` в «Что» выше), поэтому сейчас у `nogc` нет способа явно
  разрешить arena-аллокацию внутри своего тела.

### Правило

1. **Annotate все external fn** в `.nv` stdlib с `#parks`/`#wakes`/`#realtime`.
2. **Compile-time enforcement**: `in_realtime` / `in_blocking` flags в CEmitter.
3. **Conservative default**: unannotated method inside realtime{} → E_REALTIME_SYNC_PARK.
4. **User fns**: explicit `#parks` annotation triggers E_REALTIME_NESTED_SYNC_VIA_FN.
5. **lock_for**: `#realtime` (no park) + W_REALTIME_TRY_LOCK_FOR_TIMER (timer overhead).

### Тесты

**Positive (10):** realtime_{atomic_load,atomic_fetch_add,mutex_try_lock,
semaphore_try_acquire,lazy_is_forced,oncecell_get,oncecell_set_first_call,fence}_ok,
blocking_{atomic_fetch_add,mutex_unlock}_ok

**Negative (14):** realtime_{mutex_lock,rwlock_read,rwlock_with_write,barrier_wait,
condvar_wait,countdown_await,semaphore_acquire,lazy_force,once_call_once,
oncecell_get_or_init}_neg, realtime_via_user_fn_neg,
blocking_{mutex_lock,condvar_wait}_neg,
realtime_{lock_for_zero_warn,mutex_lock_for_neg} (warnings)

### Связь

- **[D64](04-effects.md#d64)** — `realtime { }` semantics (GC-pause-free, no scheduler yield)
- **[D50](#d50-concurrency-model-spawn-detach-blocking)** — `blocking { }` semantics (threadpool offload)
- **[D168](#d168-sized-atomic-types--api-contract-plan-1032)** — AtomicX ops: все `#realtime`
- **[D169](#d169-mutex--rwlock--reentrantmutex-family-plan-1033)** — Mutex/RwLock: lock → `#parks`, try_lock → `#realtime`, unlock → `#wakes`
- **[D170](#d170-coordination-primitives--semaphore--barrier--countdownlatch--condvar-plan-1034)** — Coordination primitives sync-class matrix
- **[D171](#d171-once--oncecell--lazy--single-initialization-primitives-plan-1035)** — Once/OnceCell/Lazy: force/get_or_init/call_once → `#parks`, is_*/get/set → `#realtime`
- **[Plan 103.8](../../docs/plans/103.8-sync-propagation-v2.md)** — V2: transitive `#parks` inference (planned)

### Эволюция

D172 введён как draft (Plan 103.6, 2026-05-27). Финализирован в Plan 103.7.

**Amended Plan 113 (2026-05-29):**
- `#realtime_safe` → `#realtime` (rename SyncClass).
- `realtime { }` / `blocking { }` block forms **retracted** — заменены на `#realtime fn` / `#blocking fn`.
- `#blocking fn` — fn-level threadpool offload (вся fn выполняется на threadpool).
- `#realtime fn` — callee guarantee model: caller unrestricted; body restriction only.
- D64 retracted (block-form removed); D50 §4 blocking-block removed.

Отложено в Plan 103.8 (V2 sync propagation):
- Автоматический inference: если `fn A` вызывает `#parks`-fn, A также помечается `#parks`.
- LSP integration: hover shows sync-class; quick-fix добавляет `#parks` annotation.
- Полный propagation-граф: транзитивное закрытие через call graph.

**Amended №273 (2026-08-02, [M-realtime-nogc-silent-no-enforce]):** `#realtime
nogc fn` документирован явно (§«Что» + §7) — раньше D172 описывал только
`#realtime`/`#blocking`/`#thread_affine`, хотя `nogc`-модификатор парсился
и частично enforced'ился со времён Plan 16 (пробел документации, унаследованный
от retracted-блока D64). Закрыты два enforcement-дефекта одновременно: (1)
`nogc_blacklisted_call` не matches'ил `.of` (D259 amend variadic-конструктор);
(2) path-extraction в `check_capabilities_at` не разворачивал `TurboFish`
(explicit generic type argument на receiver'е, D38), из-за чего **любая**
capability-проверка (не только nogc) silently skip'алась для вызовов вида
`Vec[int].method()`. Новый код `E_REALTIME_NOGC_ALLOC` (+ зарезервированный,
сейчас недостижимый `E_BLOCKING_NOGC_ALLOC`).

---

## D370. AI-first guidance — sync-primitive decision tree (Plan 103.7)

> **Статус:** ✅ final (Plan 103.7, 2026-05-27). Новый D-блок; нет предшествующего draft.

### Зачем

Nova `runtime.sync` содержит 12+ sync-примитивов. Выбор правильного примитива
для конкретной задачи — типичный вопрос разработчика (и AI-агента, генерирующего
код). D370 формализует **decision tree** и **canonical patterns** — официальный
ответ Nova на вопрос «что использовать для X». Это Nova edge: ни один другой
язык не имеет in-spec guidance на этом уровне детализации.

### Правило: Decision tree

```
Нужно ли разделить mutable state между fiber'ами?
│
├── НЕТ → не нужен sync. Используй channel + actor pattern (D79).
│          Пример: counter_actor(input Channel[Msg]) с match msg { ... }
│
├── ДА, exactly-once init:
│       ┌── stateless action (no value)          → Once.call_once(fn)  [D171]
│       ├── value-capturing (return T)            → OnceCell[T].get_or_init(fn)
│       └── auto-init on first access (wrap T)   → Lazy[T].new(fn) + .force()
│
├── ДА, counter / numeric stat:
│       ┌── single counter / sequence number      → AtomicI64.fetch_add(delta, Relaxed)
│       ├── max/min tracking                      → AtomicI64.fetch_max(v, Relaxed)
│       └── bitset / flags                        → AtomicU32.fetch_or/fetch_and(bits, SeqCst)
│
├── ДА, one-shot ownership / «первый побеждает»:
│       ┌── bool flag (first caller wins)         → AtomicBool.swap(true) == false → winner
│       └── pointer publish (first-to-publish)    → см. ниже (AtomicPtr снят D426)
│               `AtomicPtr` снят (Plan 207, D426) — до `AtomicPtr[T]` (Plan 103.7)
│               использовать `Mutex`/`OnceCell[T]` для publish-once указателя;
│               голый int-адрес без GC-root — только через `unsafe`-эскейп.
│
├── ДА, exclusive access к complex state:
│       ┌── short critical section, general       → Mutex + with_lock(fn)  [D169]
│       ├── read-heavy, occasional writes         → RwLock + with_read(fn) / with_write(fn)
│       └── recursive callbacks (migration path) → ReentrantMutex (opt-in; prefer Mutex)
│
├── ДА, bounded concurrency / rate limit          → Semaphore.new(N) + with_permit(fn)  [D170]
│
├── ДА, N-party rendezvous (epoch sync):
│       ┌── reusable (cyclic, round-based)        → Barrier.new(N) + wait()
│       └── one-shot signal (latch-style)         → CountDownLatch.new(N) + count_down() / await()
│
└── ДА, «wait until predicate» (park until condition):
         → Mutex + Condvar + wait_until(m, predicate)  [D170]
```

### Canonical patterns (≥5)

#### Pattern 1. Counter (AtomicI64)

**Сценарий:** подсчёт событий/запросов между fiber'ами.

```nova
import runtime.sync.{AtomicI64, MemOrdering}

ro requests = AtomicI64.new(0)

// В любом fiber'е:
requests.fetch_add(1, MemOrdering.Relaxed)   // счётчик не синхронизирует другие данные

// Чтение для метрики (periodic reporter fiber):
ro total = requests.load(MemOrdering.Relaxed)
```

**Почему Relaxed:** счётчик событий не устанавливает happens-before с другими
данными — Relaxed достаточен и эффективен (на x86 дешевле SeqCst-store).

**Anti-pattern:** `Mutex.with_lock { counter = counter + 1 }` — избыточно,
serializes всех readers и writers. Atomic — wait-free, без парковки fiber.

---

#### Pattern 2. One-shot init (Once / OnceCell / Lazy)

**Сценарий:** ленивая инициализация глобального ресурса (connection pool,
config, singleton) — ровно один раз при первом обращении.

```nova
import runtime.sync.{Lazy}

// Глобальный Lazy: init-closure известен заранее
ro db_pool = Lazy.new(|| DbPool.connect(config.db_url()))

fn handle_request(req Request) {
    ro pool = db_pool.force()   // безопасно из любого fiber'а; init = once
    pool.execute(req.query)
}
```

Если возвращаемое значение неизвестно в точке объявления (нужен runtime-аргумент):

```nova
import runtime.sync.{OnceCell}

ro config_cell: OnceCell[Config] = OnceCell.new()

fn init_config(path str) {
    config_cell.set(Config.load(path))  // idempotent; первый вызов устанавливает
}

fn get_config() -> Config {
    config_cell.get_or_init(|| Config.default())
}
```

**Anti-pattern (DCL — Double-Checked Locking):**

```nova
// ❌ ОПАСНО: race condition без Acquire/Release fence
if !initialized {
    mutex.lock()
    if !initialized {
        value = expensive_init()
        initialized = true   // store может появиться до value готово (ARM)
    }
    mutex.unlock()
}
```

Используй `Once` / `Lazy` / `OnceCell` — они содержат корректные
`__ATOMIC_RELEASE` / `__ATOMIC_ACQUIRE` барьеры (D167 contract, D171).

---

#### Pattern 3. Producer-consumer bounded buffer (Mutex + Condvar)

**Сценарий:** типизированная очередь с backpressure, когда нативный `Channel[T]`
недостаточен (нужен custom flush, batch-drain, priority и т.д.).

```nova
import runtime.sync.{Mutex, Condvar}

ro mu = Mutex.new()
ro not_full  = Condvar.new()
ro not_empty = Condvar.new()
ro buffer: []Item = []

fn producer(item Item) {
    mu.with_lock { ||
        not_full.wait_until(mu, || buffer.len() < MAX_SIZE)
        buffer.push(item)
        not_empty.notify_one()
    }
}

fn consumer() -> Item {
    mu.with_lock { ||
        not_empty.wait_until(mu, || buffer.len() > 0)
        ro item = buffer.pop()
        not_full.notify_one()
        item
    }
}
```

**Spurious wakeup:** `wait_until(mu, predicate)` — всегда использовать predicate
loop (встроен в `wait_until`). Bare `condvar.wait(mu)` без предиката — уязвим
к spurious wakeups (D170 §spurious wakeup contract).

**Anti-pattern:** Если backpressure нативный и не нужна custom логика —
**Channel[T] лучше** в 90% случаев:

```nova
// ✅ Проще: нативный backpressure через Channel (D91 capability-split)
ro (tx, rx) = Channel.new[Item](MAX_SIZE)
// producer:  tx.send(item)
// consumer:  rx.recv()
```

---

#### Pattern 4. Read-heavy snapshot (RwLock)

**Сценарий:** структура данных часто читается (N readers), редко обновляется
(1 writer). Пример: конфигурация, routing table, кэш.

```nova
import runtime.sync.{RwLock}

ro config_lock = RwLock.new()
// config хранится снаружи (heap-структура, доступ через mutable ref)

fn read_config() -> str {
    config_lock.with_read { ||
        config.value   // много concurrent readers без блокировки
    }
}

fn update_config(new_value str) {
    config_lock.with_write { ||
        config.value = new_value   // эксклюзивный доступ
    }
}
```

**Почему writer-priority (M7):** default RwLock блокирует новых readers при
ожидающем writer'е → no writer starvation на read-heavy workloads.

**Anti-pattern:** `Mutex` вместо `RwLock` на read-heavy data:

```nova
// ❌ Sub-optimal: serializes ALL readers
mutex.with_lock { || config.value }   // только один reader за раз
```

На read-heavy data `RwLock` даёт N-кратное ускорение (N = кол-во читателей).

---

#### Pattern 5. Rate-limited workers (Semaphore)

**Сценарий:** ограничить количество одновременно выполняемых операций
(N concurrent HTTP-запросов, N worker'ов к базе данных и т.д.).

```nova
import runtime.sync.{Semaphore}

ro concurrency_limit = Semaphore.new(MAX_CONCURRENT)

fn handle_request(req Request) {
    concurrency_limit.with_permit { ||   // parks если MAX_CONCURRENT уже запущено
        process(req)
    }
    // permit автоматически освобождён после with_permit
}
```

**Batch acquire:** если одна операция потребляет N permits (напр., bulk-insert):

```nova
concurrency_limit.acquire_n(batch_size)
defer concurrency_limit.release_n(batch_size)
do_bulk_work()
```

**Anti-pattern (token channel):**

```nova
// ❌ Работает, но verbose, intent не очевиден + capability-split удваивает шум
ro (tok_tx, tok_rx) = Channel.new[unit](MAX_CONCURRENT)
for _ in 0..MAX_CONCURRENT { tok_tx.send(()) }

fn handle_request(req Request) {
    tok_rx.recv()           // acquire
    process(req)
    tok_tx.send(())         // release
}
```

`Semaphore` выражает intent явно; `Channel` как семафор — workaround.

### Правило выбора ordering (supplement к D167)

| Задача | Рекомендуемый ordering |
|---|---|
| Счётчик событий, метрики | `Relaxed` (нет happens-before требований) |
| Публикация данных (writer) | `Release` (гарантирует видимость записей) |
| Чтение опубликованных данных (reader) | `Acquire` (syncs с Release) |
| RMW в tight loop (retry CAS) | `Release` на success; `Relaxed` на failure |
| Любые случаи (safe default) | `SeqCst` (дороже, но всегда корректно) |
| spin/global coordination flag | `SeqCst` (total order требуется) |

**Default = SeqCst (D167 M1):** если не уверен — SeqCst всегда корректен.
Оптимизация на Relaxed/Acquire/Release — только после профилирования.

### Anti-patterns (сводная таблица)

| Anti-pattern | Проблема | Решение |
|---|---|---|
| `Mutex.with_lock { counter += 1 }` | Overkill для счётчика; parks fiber | `AtomicI64.fetch_add(1, Relaxed)` |
| DCL без `Once`/`Lazy` | Race condition на ARM (store ordering) | `Once.call_once` / `Lazy.new(fn)` |
| `Channel[unit]` как semaphore | Verbose; intent не очевиден | `Semaphore.new(N).with_permit(fn)` |
| `Mutex` вместо `RwLock` на read-heavy | Serializes всех readers | `RwLock.with_read / with_write` |
| `condvar.wait(mu)` без predicate | Spurious wakeup UB | `condvar.wait_until(mu, predicate)` |
| `ReentrantMutex` по умолчанию | Скрывает re-entrancy bugs | `Mutex` default; `ReentrantMutex` opt-in |
| Mutex в `realtime { }` | E_REALTIME_SYNC_PARK | `AtomicX` ops (`#realtime`) |
| Mutex в `blocking { }` (lock) | E_BLOCKING_SYNC_PARK | Restructure: lock вне blocking, pass result |

### Связь

- [D79](#d79-channels--select-formal-declaration) — Channels (actor-model
  alternative к shared state); decision tree первая ветвь.
- [D167](#d167-memory-ordering--happens-before-между-fiberами) — MemOrdering;
  ordering supplement таблица основана на D167 contract.
- [D168](#d168-sized-atomic-types--api-contract-plan-1032) — Atomic types;
  counter/CAS/swap patterns.
- [D169](#d169-mutex--rwlock--reentrantmutex-family-plan-1033) — Mutex/RwLock/
  ReentrantMutex; exclusive-access branch + Patterns 3/4.
- [D170](#d170-coordination-primitives--semaphore--barrier--countdownlatch--condvar-plan-1034) —
  Semaphore/Barrier/CountDownLatch/Condvar; rate-limit + rendezvous + predicate-wait branches.
- [D171](#d171-once--oncecell--lazy--single-initialization-primitives-plan-1035) —
  Once/OnceCell/Lazy; exactly-once init branch + Pattern 2.
- [D172](#d172-realtime---blocking---sync-class-annotation-system-plan-1036) —
  realtime/blocking sync-class; anti-pattern Mutex-in-realtime пункт.

### Эволюция

D370 введён в Plan 103.7 как новый D-блок (нет предшествующего draft).
Контент разработан для AI-readability: decision tree структурирован для
LLM-навигации (иерархические ветви с explicit mapping). Canonical patterns
содержат Nova-код с комментариями и anti-pattern сравнением.

Возможные расширения в будущих plans:
- Pattern 6: distributed counter через `AtomicI64` + periodic aggregation.
- Pattern 7: async-safe init с `cancel-shielding` (Plan 100.4.2 integration).
- D174 (Plan 103.9): consume-guards pattern (когда Permit consume > with_permit).

---

## D174. Sync primitives consume integration (Plan 103.9)

> **Статус:** ✅ final (Plan 103.9, 2026-05-27). V2 guard-returning API.
> **AMENDED 2026-08-01** (owner decision, срочный пакет звучности
> mut/захватов, [M-router-handler-mut-capture-escape-soundness] §4):
> `Mutex`/`RwLock`'s lock-acquire/lock-release API (`@lock`, `@unlock`,
> `@try_lock`, `@lock_for`, `@with_lock`, `@read`, `@read_unlock`,
> `@try_read`, `@read_for`, `@write`, `@write_unlock`, `@try_write`,
> `@write_for`, `@with_read`, `@with_write`) **DROPS the `mut` receiver →
> ro receiver** (`fn Mutex mut @lock()` → `fn Mutex @lock()`). Rationale
> (rustc-эталон, [feedback-rustc-as-reference]): real Rust's
> `Mutex::lock(&self)` takes a SHARED reference — the whole point of
> `Mutex`/`RwLock` is interior mutability THROUGH the opaque handle
> (`Mutex(*())`, `#share`-vouched, D415 §2), not through the Nova
> binding — mut-viral receiver was a category error the checker never
> caught (`Mutex(*())`'s own field is a raw pointer, not a value —
> "mutating" through it needs no `mut @` any more than mutating a
> heap-record's CONTENT needs a `mut` receiver, D246 B). This
> mut-virality was the ROOT CAUSE of every `mut lock = @lock`
> field-launder workaround in `nova-polaris/src/metrics.nv`
> (`[M-router-handler-mut-capture-escape-soundness]` §2 field-launder
> canal, closed separately in `02-types.md` D246 §72-amendment) — an
> `MetricsRegistry`-method declared `fn MetricsRegistry @method(...)`
> (ro receiver, D176-default) could not call `self.lock.lock()` directly
> while `@lock` required `mut`, forcing the launder. **Not migrated**
> (out of this window's scope, no live launder-forcing repro found):
> `Semaphore`/`Once`/`ReentrantMutex`/`Condvar` — same rustc-analogy
> rationale would apply, but their `mut @`-receiver methods are NOT the
> `metrics.nv` root cause and were left as a documented follow-up (do NOT
> assume covered; audit before relying on ro-receiver there).

### Зачем

V1 sync API (D169–D171) использует `lock()/unlock()` pair — API without
static enforcement. Два класса ошибок не обнаруживаются компилятором:

1. **Забытый unlock**: fiber паркуется навсегда (deadlock) или ресурс утечёт.
2. **Double unlock**: UB; Nova_Mutex состояние corrupted.

V2 (D174) применяет **Plan 100 consume-type mechanism (D131–D166)** к sync
примитивам: `lock()` возвращает `MutexGuard consume` — linear type, must-be-consumed.
Компилятор **статически** обнаруживает:

- забытый unlock = E_CONSUME_NOT_CONSUMED (D133);
- double unlock = E_CONSUMED_AFTER_USE (D133);
- утечку guard в другой fiber = E_CONSUME_CROSS_FIBER (D157).

### Правила API (Guard-returning contract)

#### Mutex V2

| Метод | Сигнатура | Примечание |
|---|---|---|
| `lock()` | `Mutex @lock() -> MutexGuard consume` | Parks; returns guard (D174 amend 2026-08-01: ro receiver) |
| `MutexGuard.unlock()` | `MutexGuard @unlock(consume self)` | Consumes guard; wakes next |
| `unlock()` (bare) | `Mutex @unlock()` | Deprecated V1; `W_BARE_UNLOCK_DEPRECATED` (D174 amend: ro receiver) |
| `with_lock(fn)` | `Mutex @with_lock[R](body fn() -> R) -> R` | Thin wrapper; backward compat (D174 amend: ro receiver) |

C mangling (Plan 100.6 D164):
- `MutexGuard.unlock(consume self)` → `Nova_MutexGuard_consume_unlock`
- `Mutex.lock()` → `Nova_Mutex_method_lock` (returns `Nova_MutexGuard*`)

#### RwLock V2

| Метод | Сигнатура | Примечание |
|---|---|---|
| `read()` | `RwLock @read() -> ReadGuard consume` | Parks; returns read guard (D174 amend: ro receiver) |
| `write()` | `RwLock @write() -> WriteGuard consume` | Parks; returns write guard (D174 amend: ro receiver) |
| `ReadGuard.unlock()` | `ReadGuard @unlock(consume self)` | Consumes guard; wakes if needed |
| `WriteGuard.unlock()` | `WriteGuard @unlock(consume self)` | Consumes guard; wakes next |
| `read_unlock()` (bare) | `RwLock @read_unlock()` | Deprecated V1 (D174 amend: ro receiver) |
| `write_unlock()` (bare) | `RwLock @write_unlock()` | Deprecated V1 (D174 amend: ro receiver) |
| `with_read(fn)` | `RwLock @with_read[R](...) -> R` | Thin wrapper; backward compat (D174 amend: ro receiver) |
| `with_write(fn)` | `RwLock @with_write[R](...) -> R` | Thin wrapper; backward compat (D174 amend: ro receiver) |

#### Semaphore V2

| Метод | Сигнатура | Примечание |
|---|---|---|
| `acquire()` | `Semaphore mut @acquire() -> Permit consume` | Parks; returns permit |
| `Permit.release()` | `Permit @release(consume self)` | Consumes permit; wakes next waiter |
| `release()` (bare) | `Semaphore mut @release()` | Deprecated V1 |
| `with_permit(fn)` | `Semaphore mut @with_permit[R](...) -> R` | Thin wrapper; backward compat |

#### Once V2

| Метод | Сигнатура | Примечание |
|---|---|---|
| `start()` | `Once mut @start() -> Option[OnceGuard consume]` | Nova body; Some = won race |
| `OnceGuard.commit()` | `OnceGuard @commit(consume self)` | Once → DONE; wakes waiters |
| `OnceGuard.abort()` | `OnceGuard @abort(consume self)` | Once → POISONED; wakes waiters (re-panic on resume) |
| `call_once(fn)` | `Once mut @call_once(body fn() -> ())` | V1 external; kept as-is |
| `run()` (bare) | `Once mut @run() -> bool` | Deprecated V1 |
| `done()` (bare) | `Once mut @done()` | Deprecated V1 |

`start()` is implemented as a Nova body:
```nova
export fn Once mut @start() -> Option[OnceGuard consume] {
    if self.start_won() {
        Some(self.make_guard())
    } else {
        None
    }
}
```
Where `start_won()` is the internal external fn (`Nova_Once_method_start_won` = alias to `run()`),
and `make_guard()` allocates the guard heap object (`Nova_Once_method_make_guard`).

### Guard type declarations

All 5 guard types are `consume` record types with a single `ptr int` field (opaque pointer to the owning primitive):

```nova
type MutexGuard consume { ptr int }   // → Nova_MutexGuard { nova_int ptr; }
type ReadGuard  consume { ptr int }   // → Nova_ReadGuard  { nova_int ptr; }
type WriteGuard consume { ptr int }   // → Nova_WriteGuard { nova_int ptr; }
type Permit     consume { ptr int }   // → Nova_Permit     { nova_int ptr; }
type OnceGuard  consume { ptr int }   // → Nova_OnceGuard  { nova_int ptr; }
```

C struct definitions live in `compiler-codegen/nova_rt/sync_primitives.h` (Plan 103.9 section).
They are listed in `RUNTIME_DEFINED_TYPES` in `emit_c.rs` to prevent duplicate struct emission.

### Decisions

**M-D174-1. Opaque ptr field:** guard stores `int` (= `nova_int` = `int64_t`),
cast from pointer. Avoids exposing internal Nova_Mutex/Nova_RwLock C types to Nova type system.
Safe: intptr_t can hold any pointer on LP64 / LLP64.

**M-D174-2. Drop without explicit consume → ERROR.** The consume-checker enforces
explicit call to `unlock()` / `release()` / `commit()` / `abort()`. No implicit
RAII — unlike Rust `Drop`. This makes the contract explicit and visible in code.

**M-D174-3. `with_lock(fn)` etc. preserved.** `with_lock` remains the recommended
pattern for most use cases (`#parks` + panic-safe). Guard form (`consume g = mu.lock()`)
is for advanced control (cross-scope unlock, conditional release, etc.).

**M-D174-4. `try_lock() -> bool` kept as V1.** To avoid breaking regression tests,
`Mutex.try_lock()` retains `bool` return type in this iteration. Guard-returning
`try_lock_guard() -> Option[MutexGuard consume]` is a future follow-up (Plan 103.9 V2.1).

**M-D174-5. Bare unlock deprecated, not removed.** Edition 0.2: `#deprecated` warning.
Edition V3 (future): removal candidate. Giving users migration runway via `with_lock`
wrappers which continue to work without modification.

**M-D174-6. Atomics NOT migrated.** M16 from Plan 103 master: AtomicX types are
shared-state primitives (multiple concurrent readers/writers), not resources. consume
semantics require single-owner transfer — incompatible with Atomic's sharing model.

**M-D174-7. OnceGuard.abort() → POISONED.** When the winning fiber aborts
initialization, Once transitions to POISONED (not back to NEW). Subsequent callers
of `start()` / `call_once()` re-panic with `OncePoisoned`. Rationale: abort
means the resource initialization failed — retrying typically fails again for the same reason.
If retry-after-failure is needed, use `OnceCell[T]` which allows re-initialization after `take()`.

### Backward compatibility

V1 patterns continue to work without modification:
```nova
// V1 (still works, bare unlock is #deprecated warning):
mu.lock()
defer mu.unlock()   // deprecated warning

// V1 with_lock (still works, now thin wrapper over guard):
mu.with_lock { || critical_section() }

// V2 (explicit guard):
consume g = mu.lock()
defer g.unlock()
```

### Связь

- [D131-D166](#) — Plan 100 consume foundation: D133 (not-consumed E), D157 (cross-fiber), D164 (mangling).
- [D169](#d169-mutex--rwlock--reentrantmutex-family-plan-1033) — V1 Mutex/RwLock contract (updated).
- [D170](#d170-coordination-primitives--semaphore--barrier--countdownlatch--condvar-plan-1034) — V1 Semaphore (updated).
- [D171](#d171-once--oncecell--lazy--single-initialization-primitives-plan-1035) — V1 Once (updated).
- [D370](#d370-ai-first-guidance--sync-primitive-decision-tree-plan-1037) — Decision tree updated to reference D174.

### Эволюция

D174 введён в Plan 103.9 (2026-05-27) как финальный D-блок Plan 103 серии.
Закрывает «V2 consume guards migration» задачу из Plan 100.7 (stdlib migration playbook).

Предполагаемые follow-ups:
- Plan 103.9 V2.1: `try_lock() -> Option[MutexGuard consume]` (M-D174-4 follow-up).
- Plan 103.9 V2.2: Edition-gated removal of deprecated bare `unlock()/release()/done()`.
- Plan 100.8 LSP: quick-fix «wrap in consume guard» for deprecated bare-unlock sites.

---

## D228. Centralized I/O driver — sleep cancel state-machine + scope-lifetime invariants (Plan 83.11)

> **Принято 2026-06-05.** Закрывает Plan 83.11 Ф.9 design debt
> ([83.11-centralized-io-driver.md](../../docs/plans/83.11-centralized-io-driver.md)).
> Сужает [D98](#d98) (per-worker libuv loop) — timer/sleep handles теперь
> регистрируются на dedicated driver-thread loop, а не на TLS
> `_nova_current_loop` worker'а. Связан с [D71](#d71) (`cancel_requested`),
> [D75](#d75) (`supervised(cancel: tok)`), [D93](#d93) (park/wake),
> [D167](#d167) (memory ordering). Закрывает [M-83.11-gc-cancel-token-alias]
> и [M-83.11-supervised-spawn-cancel-memcpy-segv].

### Что

Cancellation timer-cancel events в Nova runtime обрабатываются
**единственным dedicated OS-thread'ом** — *I/O driver* — а не
worker thread'ом, на котором fiber park'нулся. Driver владеет own
`uv_loop_t`, принимает работу через single MPSC очередь
(`NovaDriverJob`), пробуждается через `uv_async_send`, и является
**единственным мутатором** sleep-state структур
(`NovaSleepState`, `NovaFiberQueue.armed_sleeps_head` list).

Это **архитектурный pivot от [D98](#d98)** для timer-class handles:
вместо "каждый worker управляет своими timers на своём loop'е" — один
владелец всей timer-state. Worker'ы остаются полноценными исполнителями
fiber'ов и продолжают использовать TLS `_nova_current_loop` для прочих
loop-операций (idle drain, future Net/Fs handles).

Decision вводит:

1. **Driver-path sleep state machine** (`NOVA_SLEEP_DRV_*`) с явными
   CAS-точками race-разрешения между natural fire и external cancel.
2. **`expected_co` identity field** — fiber-pointer, capture'нутый в
   момент arm, против которого close_cb проверяет `scope->fibers[slot]`
   для детекции STALE-slot race (предотвращает WRONG-FIBER wake).
3. **`ctx_pins[]` GC-root anchor** — каноничный pattern для любого
   объекта, держащегося только через worker register / heap-internal
   указатель, который иначе может быть собран conservative GC во время
   inner-alloc.
4. **`pending_driver_jobs` lifetime counter** — каноничный pattern для
   stack-allocated ресурса, указатель на который пересекает thread
   boundary через job queue.

### Правило

#### 1. Sleep state-machine — single-mutator с явным CAS

Состояния (enum `NovaSleepStage`, [fibers.h:2032+](../../compiler-codegen/nova_rt/fibers.h)):

```
NEW (10) ──arm_sleep──▶ ARMED (11) ──┬─timer_cb (won CAS)───▶ FIRING (12) ──▶ CLOSED (14)
                                     │
                                     └─cancel_scope (won CAS)─▶ CANCEL_REQ (13) ─▶ CLOSED (14)
```

- **NEW → ARMED** — driver thread в `_nova_driver_handle_arm_sleep`
  после `uv_timer_init` + `uv_timer_start`. RELEASE-store, single-mutator
  (driver), no CAS нужен.
- **ARMED → FIRING** — `_nova_driver_sleep_timer_cb` при естественном
  истечении timer'а. **CAS** (loser → cancel-job уже выиграл, callback
  тихо выходит).
- **ARMED → CANCEL_REQ** — `_nova_driver_handle_cancel_scope` (CANCEL_SCOPE
  job) walk'ает `armed_sleeps_head` list и пытается CAS каждого. **CAS**
  (loser → timer_cb уже выиграл, тоже initiate close).
- **FIRING|CANCEL_REQ → CLOSED** — `_nova_driver_sleep_close_cb` после
  того как libuv вернёт handle. SEQ_CST store. Здесь же — wake worker'а
  через `nova_sched_wake` (или direct dispatch, см. §3 ниже).

**Инвариант.** Оба пути (natural fire и cancel) сходятся на одном
`uv_close((uv_handle_t*)&st->timer, _nova_driver_sleep_close_cb)`.
Wake fiber'а происходит **только из close_cb** — никогда из timer_cb
напрямую. Это гарантирует, что к моменту wake handle полностью
released, и subsequent re-arm безопасен.

#### 2. CAS race resolution — обе стороны конвергируют на close_cb

Когда natural fire и cancel-scope конкурируют за один slot:

```c
/* timer_cb */
int32_t expected = NOVA_SLEEP_DRV_ARMED;
if (!nova_aint_cas(&st->stage, &expected, NOVA_SLEEP_DRV_FIRING)) return;
uv_close((uv_handle_t*)&st->timer, _nova_driver_sleep_close_cb);

/* handle_cancel_scope (для каждого st в armed list) */
int32_t expected = NOVA_SLEEP_DRV_ARMED;
if (nova_aint_cas(&st->stage, &expected, NOVA_SLEEP_DRV_CANCEL_REQ)) {
    uv_close((uv_handle_t*)&st->timer, _nova_driver_sleep_close_cb);
}
/* CAS loser: timer_cb already won, will close. Skip. */
```

Loser молча выходит. Winner initiate close. **close_cb не различает,
какой именно путь привёл к close** — поведение wake одинаковое; разница
видна только worker fiber'у через его собственный cancel-check на
yield-point'е после wake.

#### 3. `expected_co` identity — защита от STALE-slot race

`NovaSleepState.expected_co` (`mco_coro*`) фиксируется в момент ARM_SLEEP
job-emit, **до** того как driver actually arms timer. В close_cb:

```c
mco_coro* actual_co = scope->fibers[slot];
if (actual_co != st->expected_co) {
    /* WRONG-FIBER — slot был переиспользован пока expected_co была parked.
     * Sub-case A: expected_co всё ещё MCO_SUSPENDED → direct dispatch её
     *             через sc->dispatch_ready, bypass nova_sched_wake (slot уже
     *             принадлежит другому fiber'у). Mark her ctx->_nova_worker_slot
     *             = -2 (DISPLACED) чтобы epilogue не вызвал free_slot.
     * Sub-case B: expected_co dead → just store CLOSED и выйти. */
}
```

**Почему это нужно.** `nova_sched_wake(scope, slot)` адресует *slot*,
не fiber. Без identity-check'а WRONG-FIBER приводил к hang оригинального
fiber'а (Plan 83.11 Ф.3 STALE-slot race, ~10 часов диагностики,
см. [§10.3](../../docs/plans/83.11-centralized-io-driver.md#103)).

#### 4. `armed_sleeps_head` — driver-thread-only linked list

`NovaFiberQueue.armed_sleeps_head` — singly-linked list всех в данный
момент ARMED timer'ов под данным supervised scope'ом, с `pprev_in_scope`
для O(1) unlink. **Single-mutator (driver thread)** — `arm_sleep`
вставляет, `close_cb` удаляет, `handle_cancel_scope` walk'ает.
**Никаких locks** — list mutation никогда не race'ит сама с собой,
потому что все три операции выполняются в driver loop'е sequentially.

Workers / main thread **никогда** не читают и не пишут эту list напрямую.

#### 5. `ctx_pins[]` — каноничный GC-root anchor

> **Инвариант.** Любой GC-managed объект, ссылка на который держится
> ТОЛЬКО через worker register или через указатель внутри другой
> heap-allocated структуры, **должен** быть pin'нут в supervised-scope's
> `ctx_pins[]` до того, как может произойти inner-alloc (любой
> `nova_alloc` / spawn / scope-grow).

Pattern:

```c
NovaFiberQueue _nova_scope_q_0 = {0};
nova_scope_init(&_nova_scope_q_0);
NovaCancelToken* _nova_cancel_tok_0 = tok;
nova_scope_pin_ctx(&_nova_scope_q_0, (void*)_nova_cancel_tok_0);   /* ← MANDATORY */
nova_runtime_spawn_into(&_nova_scope_q_0, ...);                    /* теперь spawn-loop безопасен */
nova_cancel_token_bind(_nova_cancel_tok_0, &_nova_scope_q_0);
nova_supervised_run_cancel(&_nova_scope_q_0, _nova_cancel_tok_0);
```

**Почему scope-frame.** `scope.ctx_pins[]` живёт на main stack (часть
stack-allocated `NovaFiberQueue _nova_scope_q_0`), который scanned
conservative GC как root безусловно. Объект, добавленный туда, остаётся
reachable пока scope жив — даже если регистр-копия cancel-token'а уже
сброшена компилятором.

**Когда срабатывает баг без pin.** `ctx_pins[]` сама удваивается на
степенях 2 (16 → 32 → … → 1024). При ~512 одновременно живых fiber'ов
grow вызывает `nova_alloc` → triggers Boehm GC sweep → token в регистре
не виден conservative scanner'у → token collected → следующий
`nova_alloc_uncollectable` (для `NovaSpawnCtxBase`) попадает на тот же
адрес → структурный aliasing (offset +8: `bound_scope` vs
`_nova_parent_scope`) → panic
«token already bound to a live scope». См.
[§11.2-11.3](../../docs/plans/83.11-centralized-io-driver.md#112-root-cause--структурный-aliasing).

Codegen ответственность ([emit_c.rs::emit_supervised](../../nova-cli/src/emit_c.rs)):
на каждый `supervised(cancel: tok)` сразу после декларации
`NovaCancelToken*` локальной emit'ить `nova_scope_pin_ctx(&queue, (void*)tok);`
**до** body. Plan 44.5 L5 использует тот же pattern для `NovaSpawnCtxBase`.

#### 6. `pending_driver_jobs` — каноничный stack-lifetime counter

> **Инвариант.** Если job в driver queue несёт указатель на caller's
> stack-frame (typical: `NovaFiberQueue*` для supervised scope),
> caller **обязан** дождаться completion job'а перед возвратом —
> иначе use-after-free, когда driver thread finally drain'нёт queue.

Pattern (`NovaFiberQueue.pending_driver_jobs`, ACQ_REL/RELEASE/ACQUIRE):

```c
/* worker thread — _nova_cancel_via_driver: */
nova_aint_inc(&scope->pending_driver_jobs);             /* ACQ_REL */
if (nova_driver_submit_job(job) != 0) {
    free(job);
    __atomic_fetch_sub(&scope->pending_driver_jobs, 1, __ATOMIC_ACQ_REL);
}

/* driver thread — _nova_driver_handle_cancel_scope (end): */
(void)__atomic_fetch_sub(&scope->pending_driver_jobs, 1, __ATOMIC_RELEASE);

/* main thread — nova_supervised_run_impl, after pending_remote == 0 loop exit: */
while (nova_aint_load(&q->pending_driver_jobs) > 0) {   /* ACQUIRE */
    uv_run(nova_current_loop(), UV_RUN_NOWAIT);
    if (nova_aint_load(&q->pending_driver_jobs) > 0) uv_sleep(1);
}
nova_sched_drop_state(q);
/* unbind + return ... */
```

**Why ordering is correct.** Inc(ACQ_REL) happens-before submit
(program order, same thread); submit happens-before driver's job-load
(uv_async_send is SEQ_CST inside libuv); driver's processing reads scope
fields (visible because scope stack-frame is still alive due to main's
wait); dec(RELEASE) synchronizes-with main's load(ACQUIRE). Main proceeds
only когда counter == 0 ⇒ все referenced jobs полностью processed.

Без этого counter'а driver thread может dequeue CANCEL_SCOPE job через
1-10 ms после того, как `nova_supervised_run_impl` уже вернулся,
test-runner pop'нул main's stack frame, и `_nova_scope_q_0.armed_sleeps_head`
читается из reused memory → wild pointer → SEGV. См.
[§12.31](../../docs/plans/83.11-centralized-io-driver.md#1231).

### Почему централизованный driver thread, а не per-worker

| Подход | Cross-thread visibility | Race surface | Cost |
|---|---|---|---|
| Per-worker loop (старое [D98](#d98) для всех handles) | timer на worker A, cancel из worker B — нужно сериализовать через handle migration или `uv_async_send` | Большая (N×N pair'ов worker↔worker) | Cheap parallelism — каждый сам себе хозяин |
| Centralized driver (D228) | Любой worker → driver через single MPSC job queue | Линейная (только worker↔driver) | Driver = single point of contention, но cheap для timer/cancel workloads |
| Sharded driver pool | Hash(scope) → driver_id | Линейная per shard | Сложность codegen + load balancing |

**Конкретные обоснования:**

1. **Tokio 1.x precedent.** Tokio multi-thread runtime в 2020 году
   сделал ровно такой pivot для timer driver по тем же причинам
   (cross-thread timer migration nightmares). Plan 83.11 портирует
   архитектуру [TimerEntry](https://github.com/tokio-rs/tokio/blob/master/tokio/src/runtime/time/entry.rs)
   pattern напрямую.
2. **Cross-thread visibility races eliminated.** Single owner timer-state
   означает, что любая мутация `NovaSleepState.stage` или
   `armed_sleeps_head` — sequential. Memory ordering сводится к
   acquire-release между worker (submit) и driver (process).
3. **STALE-slot race локализован.** Когда timer-cb и cancel-job race'или
   на одном worker'е (D98 модель), нужен был сложный wake-before-park
   futex (Plan 83.11 Attempts 1-5). С driver-thread моделью оба теперь
   сходятся через CAS на одной structure, owned одним thread'ом.
4. **Trade-off митигирован batching'ом.** Driver thread = single point
   of contention; для timer-heavy workload'а это потенциально bottleneck.
   Mitigated job batching'ом (драйнер обрабатывает все накопленные jobs
   в одном `uv_async` wake), и тем фактом, что typical Nova program
   создаёт O(fibers) timer'ов, не O(operations).

### Что отвергнуто

1. **Per-worker timer loops с handle migration на cancel.** Это
   изначальная D98 модель. Provoked Plan 83.10.x races + Plan 83.11
   STALE-slot race; 5+ attempts at wake-before-park futex
   ([§10.2](../../docs/plans/83.11-centralized-io-driver.md#102-ложные-пути-что-не-сработало)).
   Architectural root cause не решался tactically.
2. **`nova_alloc_uncollectable` для CancelToken.** Семантически верно
   (caller-owned), но требует явного `nova_free_uncollectable(tok)` на
   cleanup-пути, которого сейчас нет. Без cleanup-path → leak per token.
   `ctx_pins[]` решает корректнее: token GC-managed, держится scope-frame'ом,
   автоматически освобождается когда scope сам становится unreachable.
3. **Heap-allocate scope** вместо stack для решения use-after-free.
   Изменило бы D14/D50 (supervised — stack frame, не heap-handle) ради
   одного fix'а. Counter-based wait `pending_driver_jobs` достаточен и
   sound.
4. **Sharded driver pool** (`Hash(scope) % N`). Преждевременная
   оптимизация; добавляет load-balancing complexity и cross-shard race
   surface обратно. Закладываем как опцию в Q24 на случай measured
   bottleneck.
5. **Wake fiber'а из `timer_cb` напрямую** (skip close_cb). Сокращает
   latency на одну uv-loop iteration, но re-arm после wake может race
   с outstanding `uv_close` — нужно tracking «handle in flight».
   Уход convergence на single point (close_cb) дешевле.

### Связь

- [D14](#d14) — fiber-runtime; D228 уточняет structure cancel-path.
- [D50](#d50) — `spawn` / `supervised`; scope-frame инвариант усиливается.
- [D71](#d71) — bootstrap concurrency, `cancel_requested` flag;
  D228 раскрывает механику пробуждения parked-fiber'ов на cancel.
- [D75](#d75) — `supervised(cancel: tok)`; **обе invariant'а
  (`ctx_pins[]` pin для token + `pending_driver_jobs` wait) обязательны
  для корректной реализации D75 при ≥1 worker thread.**
- [D93](#d93) — park/wake; D228 строит timer-cancel путь поверх него.
- [D98](#d98) — per-worker libuv loop; **D228 сужает D98** — timer/sleep
  handles теперь живут на driver loop'е, не на worker'ском TLS-loop'е.
  Остальные планируемые handle-классы (Net, Fs, channels-select-timer)
  следуют centralized-driver pattern по умолчанию; per-worker — только
  для handles, у которых нет cross-worker cancel semantics.
- [D103](#d103) — preemption; orthogonal.
- [D167](#d167) — memory ordering; D228 §6 фиксирует ACQ_REL/RELEASE/ACQUIRE
  contract как нормативный для любого job-pointer-to-stack pattern.
- [Plan 83.11](../../docs/plans/83.11-centralized-io-driver.md) — full
  implementation history; §10 (STALE-slot post-mortem), §11.6
  (`ctx_pins` closure), §12.31 (`pending_driver_jobs` closure).
- [open-questions.md → Q24-Q27](../open-questions.md#q24) — driver
  sharding, drain-and-cancel barrier API, `ctx_pins[]` threshold tuning,
  sysmon introspection.
- Closes [M-83.11-gc-cancel-token-alias] (§11.6).
- Closes [M-83.11-supervised-spawn-cancel-memcpy-segv] (§12.31).
- Closes [M-83.10.1-armed-cancel-timer-hang] V2 sweep PARTIAL (2026-06-08): 15 тестов с
  `// ENV NOVA_AUTOARM=0` убраны — cancel+sleep корректен под armed M:N. 2 теста
  re-gated (park_wake_stress, semaphore_batch_n) — stress-верификация выявила TIMEOUT
  под armed M:N; причина — open [M-83.11-grow-vs-wake-race] (§13.6.1), не закрыт Plan 83.11 Ф.3.
  [M-83.10.1-armed-cancel-timer-hang] остаётся 🟡 PARTIAL до fix [M-83.11-grow-vs-wake-race].

### Открытые вопросы

- Должен ли driver thread шардироваться (`N drivers` через
  `hash(scope) % N`) для high-I/O workloads? — **Q24**.
- Нужен ли explicit `drain_and_cancel` barrier API (`scope.barrier()`)
  для пользовательского кода, который хочет дождаться полной обработки
  outstanding cancel'ов? — **Q25**.
- Должен ли `ctx_pins[]` doubling threshold (сейчас 16 → 32 → … → 1024)
  быть tunable env var'ом для embedded таргетов? — **Q26**.
- Должен ли `pending_driver_jobs` counter экспонироваться через
  `nova runtime introspect` / sysmon для observability long-running
  cancel storms? — **Q27**.
- ~~(Plan 83-go-cmn Ф.1b / D243) Должен ли guard читать capacity ACQUIRE?~~ —
  **Q28 ✅ ЗАКРЫТО 2026-06-11**: да, ACQUIRE (`nova_sched_cap_acq`), commit
  `98b4b05c6ae`. Закрывает `[M-83.11-f1b-acquire-capacity]`. См. D243 §Followup.
- (Plan 83-go-cmn Ф.4) Должен ли `runnext` (LIFO fast-slot) быть stealable под
  устойчивым дисбалансом (Go делает stealable после grace-tick) или остаться
  non-stealable (Tokio-parity, текущий выбор Nova)? — **Q29**.
- (Plan 83-go-cmn Ф.3→Ф.4, D245) Чтобы nspinning-coalescing стал safe, cross-thread
  работа должна попадать в очередь, которую spinner сканит. Routing'ить cross-thread
  spawn/goready в **global queue** (вместо per-worker `wake_pending`) — в Ф.4? Или
  сделать spinner-recheck дренящим чужие `wake_pending` (cross-worker access +
  синхронизация)? Первое чище и совпадает с Go shared-global топологией. — **Q30**.

---

## D243 — M:N run/park storage: fixed-size ring + chunked stable-address park-state (Plan 83-go-cmn Ф.1)

> **Создан:** 2026-06-11 (Plan 83-go-cmn Ф.1a+Ф.1b). Закрывает `[M-83.11-grow-vs-wake-race]`.
> Порт принципа Go 1.4 (fixed `P.runq[256]` never-realloc). D-нумерация: D241/D242 заняты
> Plan 138 (Iterable/Next/Iter) — семейство 83-go-cmn перенумеровано на D243+.

### Контракт

Любое хранилище, к которому конкурентно обращаются worker-owner и driver/peer-потоки на
hot-path планировщика (run queue + park/wake state), **ОБЯЗАНО иметь стабильные адреса
элементов** — никаких realloc-с-перестановкой-указателя под конкурентным доступом.
Перестановка базового указателя массива (realloc) гонится с читателем другого потока →
torn/orphaned base → потерянный wake → вечный hang. Это был root cause grow-vs-wake.

**1. Per-worker run queue — fixed-size inline ring (`runq.h`).**
- `mco_coro* runq[NOVA_RUNQ_CAP]` инлайн в `NovaWorker`; `CAP` = степень двойки (4096 в
  Nova; Go = 256 — Nova больше, т.к. `nova test` ставит MAXPROCS=1 → все fiber'ы на одном
  worker). Базовый адрес стабилен всю жизнь worker'а.
- monotonic `uint32` head/tail (маска только при доступе; len=tail-head; full=tail-head>=CAP).
- **single-producer tail** (owner: store-release, без CAS); **multi-consumer head** (owner
  `runq_get` + воры `runq_grab`/`runq_steal`: каждый advance — CAS).
- Overflow → spill HALF (`runq_put_slow`) в ОДНУ глобальную overflow-очередь
  (`NovaGlobalRunq`, intrusive singly-linked через `SpawnCtxBase.schedlink`, под spinlock).
- Consumer overflow ОБЯЗАН быть в find-work loop (`globrunq_get_one` после local+yielded,
  до steal) + shutdown drain + pump_scope — иначе spilled fiber'ы strand → hang.
- Memory-ordering как PPoPP-2013 deque: значение слота, прочитанное до head-CAS, стабильно
  до reuse (reuse требует CAP put'ов) → pre-CAS read race-free.
- Заменяет растущий Chase-Lev deque (`deque.h`, ретайрнут).

**2. Park/wake state — chunked stable-address (`NovaSchedState`, `nova_sched.h`).**
- 4 параллельных массива (`parked[]`, `pending_handle[]`, `pending_stop_cb[]`,
  `pending_wake[]`), индексируемые (scope,slot), **бэкаются директориями фиксированных
  chunk'ов** (`X_chunks[MAX_CHUNKS]`, chunk = `NOVA_SCHED_CHUNK`=64 элементов).
- **Chunk'и аллоцируются РАЗ и НИКОГДА не двигаются/освобождаются/realloc'атся** →
  `&parked[slot]` стабилен навсегда → torn-pointer структурно невозможен.
- `nova_sched_grow_state` больше НЕ realloc'ит — **CAS-публикует** chunk'и
  (`X_chunks[c]` NULL→chunk, RELEASE/ACQUIRE; проигравший CAS отбрасывает свой chunk, GC
  соберёт). grow НЕ single-writer (spawn_into/get_state/register_pending растят без
  slot_lock) → CAS-публикация обязательна. `capacity` публикуется RELEASE'ом ПОСЛЕДНИМ,
  после всех chunk-публикаций.
- Доступ: `*accessor(st,slot)` = `&X_chunks[slot>>6][slot&63]` (accessor ACQUIRE-загружает
  chunk-ptr). Все атомарные операции (SEQ_CST parked store, t1-t4 pending_wake CAS,
  register_pending SEQ_CST piggy-back, wake deliver-then-CAS) — **байт-идентичны** (меняется
  только lvalue, не `__ATOMIC_*` аргумент).
- `pending_handle`/`pending_stop_cb` в РАЗНЫХ директориях — register_pending SEQ_CST всё
  равно глобально упорядочивает их (store-buffer drain, не per-array). НЕ колоцировать.
- Потолок: `MAX_CHUNKS`=1024 ⇒ 65536 slots/scope (abort при превышении). На практике
  раньше упирается в Plan 82 fiber-arena (~16384 одновр. fiber'ов/worker).

### Альтернатива отклонена
Перенос park-state на `SpawnCtxBase` (per-fiber, Option A) — ставил бы slot_lock + 
mco_get_user_data в lock-free wake-путь И переоткрывал lost-wake при slot-reuse (wake
резолвит NEW ctx переиспользованного слота). Chunked (Option C) сохраняет slot-индексацию
→ fences не пере-доказываются. См. plan 83-study-go-c-mn §9.4.

### Followup
`[M-83.11-f1b-acquire-capacity]` — ✅ **RESOLVED 2026-06-11** (commit `98b4b05c6ae`). Добавлен
`nova_sched_cap_acq(st)` = ACQUIRE-load capacity; применён на всех ~14 accessor-guard сайтах
(nova_sched.h ×10, driver.c ×2, runtime.c ×2). Теперь guard `slot < cap_acq(st)` парится с
RELEASE-store capacity → на ARM accessor chunk-ptr не спекулируется вперёд guard'а → NULL-окно
закрыто. Диагностические dump-чтения + register_pending grow-trigger оставлены plain.
Verified x86: smoke + grow_vs_wake 40/40 (race stays closed). **Q28 ✅ закрыт.**

### Validation
grow_vs_wake_explicit 100/100 (MP=1) + 66/66 (MP=16); stress_iso_3e 66/66; semaphore_batch_n
30/30 armed; ring_overflow_drain 10/10 (5000 fibers overflow, exact-count); 1k 30/30, 10k 10/10;
concurrency suite 105/4. adversarial diff-review: fence_hazards VERIFIED CLEAN, verdict safe-to-commit.

## D244 — gopark/goready park/wake ordering (Plan 83-go-cmn Ф.2)

> **Создан:** 2026-06-11 (Plan 83-go-cmn Ф.2). Порт принципа Go runtime·gopark/goready.
> Заменяет pending_wake-счётчик + t1-t4 dance + TLS-deferred-unlock (D228-эры) единым
> lost-wakeup-free протоколом. Хранилище — D243 (chunked stable-address).

### Контракт

Любая блокирующая операция паркуется через `nova_gopark`, будится через `nova_goready`.
Состояние ожидания — **per-fiber 4-state latch `_nova_park_state`** на `NovaSpawnCtxBase`
(NIL=0/WAIT/READY/DISPATCHED; zero-init=NIL; адресуется **by-co** через
`mco_get_user_data(co)`, НЕ по (scope,slot)). Ортогонален `_nova_fiber_state`
(IDLE/RUNNING/PARKED/DEAD = resume-ownership): park_state = wait/ready handshake.

**`nova_gopark(unlock_fn, unlock_arg)` — строгий порядок (lost-wakeup-free):**
- **G0** `_nova_fiber_state` → PARKED (RELEASE; до G1, чтобы SEQ_CST G1 опубликовал и PARKED).
- **G1** `_nova_park_state` → WAIT (**SEQ_CST** = XCHG на x86, full fence). Несущая публикация:
  глобально видима до того, как любой waker возьмёт lock ресурса (который отпускает unlock_fn).
- **G2** stash unlock_fn/arg в TLS — scheduler сливает ПОСЛЕ yield'а (unlock, а значит
  reachability для peer-waker'а, строго после видимости WAIT).
- **G3** commit-recheck: CAS READY→DISPATCHED. Успех = ready-before-park (goready латчнул
  NIL→READY до G1 или в гонке) → **НЕ yield**: unlock_fn вызывается **inline** (через local
  copies) + TLS self-drain (scheduler не сливает на этом пути) + fiber_state IDLE + return.
- **G4** иначе `mco_yield`. После resume: TLS clear + reset park_state→NIL (end-of-wait;
  иначе поздний cross-thread NIL→READY заставил бы СЛЕДУЮЩИЙ gopark пропустить yield).

**`nova_goready(co)` — single-winner CAS-ladder (by-pointer):**
- CAS WAIT→DISPATCHED → **единственный** диспетчер: `dispatch_ready(co)` + fiber_state
  PARKED→IDLE + clear `parked[slot]`/`parked_co[slot]`.
- иначе CAS NIL→READY → латч (ready-before-park).
- иначе (READY/DISPATCHED/dead) → идемпотентный no-op. `mco_status != MCO_DEAD` гардит
  каждую мутацию/dispatch.

**Сосуществование cancel (by-slot) + примитив (by-pointer):** оба воронкуют re-queue через
ОДИН `nova_goready(co)` → single-winner (WAIT→DISPATCHED CAS) элек­тит ровно одного диспетчера
для любой пары wakers {sender,cancel}/{timer,cancel}/{cancel,cancel} → double-push невозможен
by construction. Cancel-walk резолвит `co = parked_co[slot]` (chunked, set@gopark/clear@goready),
**НИКОГДА `scope->fibers[slot]`** (мог быть NULL'нут/reused). `parked[]` демотирован до
cancel-reachability бита; alloc_slot skip-stale = `parked[i]` один (пинит slot пока parked).

### Удалено
pending_wake counter directory + accessor + t1-t4 barrier-dance + `_nova_park_unlock_fn`
как deferred-hack (TLS переиспользован как gopark unlock-carrier) + `nova_sched_is_parked`
gate в deferred-unlock. SEQ_CST на `pending_stop_cb` (register_pending, Plan 83.10.2 guard) —
СОХРАНЁН.

### Followup
`[M-83.11-f2-arm-tsan]` (P2): G0(RELEASE)/G1(SEQ_CST) x86-корректны (XCHG дренит store-buffer);
для ARM/weak-memory — валидировать под TSAN на Linux (gated на `[M-nova-linux-build]`).
Не регрессия (x86 — целевая платформа).

### Validation
adversarial diff-review (3 линзы + synth) verdict **safe-to-commit** (все fatal/high
опровергнуты построчно; fence не ослаблен; codegen ABI верифицирован). Independent stress:
grow_vs_wake 40/40, cross_channel 40/40, condvar_no_lost_wakeup 40/40, nested_cancel 30/30,
mutex_cancel 30/30; concurrency 105/4, plan103_4 25/25. grow-vs-wake остаётся CLOSED.

## D245 — worker-wakeup: uv_async IS the note primitive (Plan 83-go-cmn Ф.3 finding)

> **Создан:** 2026-06-11 (Plan 83-go-cmn Ф.3 design-finding). Решение: НЕ заменять
> worker-park примитив; nspinning-coalescing отложен. Без кода (de-risking-исход).

### Решение

Worker-thread park/wake в M:N-рантайме **остаётся на libuv `uv_async`** — он УЖЕ
удовлетворяет контракту Go-`note`:
- **notesleep** = `uv_run(&w->loop, UV_RUN_ONCE)` (worker блокируется до loop-события).
- **notewakeup** = `uv_async_send(&w->wake_handle)` (cross-thread, async-signal-safe, lock-free).
- **Idempotent + before/after ordering:** libuv хранит pending-async флаг в handle; send в
  ЛЮБОЙ момент (до входа в uv_run ИЛИ во время блокировки) даёт ровно один wake; множественные
  send'ы между двумя uv_run коалесцируются. Lost-wake невозможен (состояние в handle, не в transient).
- **Windows:** IOCP-backed (libuv). Nova НЕ владеет примитивом → весь класс Windows-note-багов
  (WaitForMultipleObjects / un-register corner) ВНЕ scope. Это главное снижение риска vs ручной lock_sema.

**Собственный `note.h` НЕ вводится** (`[M-83-gocmn-note-primitive-deferred]`) — понадобится
только если Ф.6 (timer-heap) / Ф.8 (netpoll) уберут libuv из worker-park.

### nspinning coalescing — отложен (gated на Ф.4)

Go-оптимизация «не слать wakep, если есть spinner» (`if nmspinning>0: skip`) **небезопасна в
текущей топологии Nova:** cross-thread работа публикуется в **per-worker `wake_pending`**
(dispatch_ready, spawn_global round-robin), но spinner сканит лишь runnext|local|global|steal →
НЕ дренит чужой `wake_pending` → coalesce-skip → **lost-wakeup**. Go-инвариант «spinner ⇒ вся
работа достижима» держится только для shared-global-runq топологии Go, не для Nova.

⇒ **Coalescing gated на Ф.4** (global-queue routing — spinner сканит global, поэтому cross-thread
работа через global станет coalesce-safe). Порядок ФЛИПается: **Ф.4 → Ф.3-coalesce**
(`[M-83-f3-coalesce-gated-on-f4]`). Безопасный subset (nspinning accounting + recheck-after-decr
без gate) — без value (uv_async уже корректен), поэтому не реализован.

**Ф.4 финальный (2026-06-12):** global-routing (cross-thread → global) ОТЛОЖЕН (review нашёл
stranding; home-affinity Nova уже корректен; `[M-83-f4-global-routing-gated-on-bench]`).
Реализован **безопасный subset БЕЗ routing** (только runtime.c find-work loop): steal random-victim
start (xorshift32, anti-herd) + post-steal global re-poll + 61-tick global fairness (anti-starve).
Subset **добавляет только global-DRAIN точки (consumers)**, не producers → stranding/lost-wakeup
невозможны. Suite 106/4 no-regression; grow_vs_wake 25/25; ring_overflow @MP=4 25/25. Ф.3
coalescing остаётся заблокирован (нужен routing). См. план §9.11.1.

## D270 — Loop codegen opt: preempt-check elision + copy-loop bulk-lowering (Plan 143 §2, [M-opt-preempt-strided-loop])

> **Создан:** 2026-06-14 (Plan 143 §2, merge `7c047a1b`). Две correctness-neutral codegen-оптимизации
> per-iteration loop-overhead. Обе семантически прозрачны — наблюдаемое поведение цикла не меняется.

### §A — Preempt-check elision на provably-short const-bound циклах

`nova_preempt_check()` (M:N cooperative safepoint, Plan 44.7) эмитится в back-edge КАЖДОГО цикла —
барьер-call, блокирующий clang-векторизацию/unroll. **Решение:** опускать его для **provably-short**
range-циклов `for i in lo..hi`, где ОБА bound'а — целочисленные литералы И iteration-count ∈ [0, 1024].

**Контракт вытеснения:** такой цикл **ограничен по построению** (≤1024 итер.) → не может
монополизировать worker → отсутствие safepoint starvation-safe. Variable/unbounded и large-const
(count>1024) циклы check **СОХРАНЯЮТ** (иначе возврат tight-loop-starvation — ровно то, что Plan 44.7
предотвращает). Порог 1024 консервативен; вложенный unbounded цикл имеет СВОЙ safepoint (элизия —
только внешнего const-small). Реализация: `emit_loop_body_inline_ex(skip_preempt)`.

### §B — Copy-loop → overlap-safe bulk copy (memmove)

`for i in lo..hi { dst[i] = src[i] }` на `Vec[T]` → одиночный bulk-copy вместо per-element цикла
(убирает preempt-check + per-element bounds-branch; clang векторизует). **Консервативный recognizer**
(иначе fallback на корректный цикл): single plain-assign без trailing; оба индекса = loop-var; dst/src
= plain Ident'ы (pure, single-eval); одинаковый `Vec[T]` flat-storage (не raw `*mut Vec`); flat-POD
элемент (`nova_*` / pointer `_p`).

**Инвариант overlap-семантики (полное соответствие циклу, НЕ упрощение):** восходящий per-element
цикл ПРОПАГИРУЕТ при destructive forward-overlap (dst строго внутри `[src, src+n)` — достижимо через
writable offset-overlap Vec-view'ы: `a=v[1..]; b=v[0..]; a[i]=b[i]`). Поэтому emit'ится runtime
overlap-guard: `if (dst ≤ src ∨ нет overlap) memmove; else forward-element-loop` (пропагация). Bounds
сохранены: highest-index `last` формируется БЕЗ `hi+1` (inclusive-overflow-safe при end==i64::MAX).

### Acceptance
plan143 12 кейсов PASS (preempt skip/keep/large; copy full/partial/self/empty/inclusive/offset-overlap-
propagation/non-destructive-overlap/extra-stmt-fallback); сгенерённый C проверен (skip + memmove с
overlap-guard + bounds); регрессия 0 new fail (Vec/collection слайс + concurrency smoke). Оба бага
(inclusive overflow, aliasing) найдены adversarial-review + эмпирическим baseline-vs-ветка probe.

### Long-term
General async-preemption (Go 1.14 SIGURG) уберёт per-iteration call для variable-bound циклов без
порога — SIGURG-часть `[M-opt-preempt-strided-loop]` (open). Cross-link **Plan 144 §7.4**: SIGURG =
ОБЩИЙ async-yield (preempt + async GC safe-point).

## D271 — Function-entry preempt-check elision на provably-leaf функциях (Plan 143 §2.B, [M-opt-leaf-preempt-entry-elision])

> **Создан:** 2026-06-14 (Plan 143 §2.B, ветка `plan-143-leaf-preempt-elision`). Correctness-neutral
> codegen-оптимизация: элидирует **function-prologue** `nova_preempt_check()` (Plan 44.7 — first statement
> КАЖДОЙ Nova-функции, [emit_c.rs:14925](../../compiler-codegen/src/codegen/emit_c.rs#L14925)). Sibling [D270 §A]
> элидит per-loop back-edge check; D271 — per-function entry check. Семантически прозрачна.

### Инвариант вытеснения (соундность)
Fiber бежит безгранично без yield'а **ТОЛЬКО** через (a) **цикл** — его back-edge `nova_preempt_check()`
СОХРАНЁН (D270 §A элидит лишь const-small finite); (b) **рекурсию** — цикл в call-графе. ⇒ Entry-check
безопасно опустить, если функция **провабельно НЕ может крутиться без safepoint'а**, т.е. каждый
рекурсивный цикл сохраняет ≥1 члена с safepoint'ом.

### Правило (conservative KEEP)
Функция `f` СОХРАНЯЕТ prologue entry-check ⟺ **любое** из:
1. `f` на call-граф **цикле** — self-recursive ИЛИ член SCC>1 (взаимная рекурсия);
2. `f` делает **indirect/closure/fn-ptr-call** (цель статически неизвестна → ацикличность не доказать);
3. `f` делает **FFI/extern-call** (C может вызвать Nova-callback → неизвестная цель);
4. `f` **address-taken** (используется как fn-value/callback → цель чьего-то indirect-вызова).

**ELIDE** иначе (non-recursive, статически-резолвимые callee, не-indirect, не-address-taken). `has_loop`
для entry-check **НЕ важен** (цикл держит свой back-edge check; bounded straight-line терминируется).

### Интеграция — source-level whole-program pre-pass
Call-граф над source-level `FnDecl`-символами (entry `module.items` + все импортируемые `peer_files`),
computed ДО эмита (`compute_preempt_keep_set` в `compiler-codegen/src/codegen/preempt_keep.rs`). Узлы =
non-external FnDecl (ключ `receiver::name::param-signature`); рёбра = direct resolved-call; per-fn флаги
indirect/ffi/address_taken; итеративный Tarjan SCC. `KEEP = cycle ∪ makes_indirect ∪ makes_ffi ∪
address_taken`. **Соундно над монтоморфизацией:** рекурсия/indirect/FFI/address-taken определяются
ИСХОДНИКОМ → KEEP-статус шаблона наследуется ВСЕМИ инстансами (over-approximation: лишние рёбра только
ДОБАВЛЯЮТ KEEP, ни один цикл не теряется). On-demand emit-пути (mono/erased-инстансы) консультируют тот же
KEEP-set по тому же ключу; closure/trailing-block/spawn-fiber-entry (без source-key) — unconditional KEEP.
**Conservative default:** любой неразрешённый/cross-module-неуверенный callee, named/spread-args, ambiguous
overload, любое сомнение → KEEP. Никогда не элидим под вопросом — соундность важнее агрессивности.

### Acceptance
plan143_2 7/7 PASS: positives (`pure_leaf`, `pure_branch`, `leaf_callee`, `forward_once`, `forward_chain`)
ЭЛИДИРОВАНЫ; negatives — self-recursion, mutual-recursion SCC, indirect/closure, FFI, address-taken — KEPT
(проверено в сгенерённом C). Probe: рекурсивный generic `gcount[T]` держит check в обоих моно-инстансах;
non-recursive `gid[T]` элидит. Регрессия 0 new fail (Vec/collection/concurrency слайс; pre-existing
подтверждены на main-бинаре). Adversarial-review (≥5 линз) закрыл интеграционные emit-дыры (рекурсивный
generic терял safepoint в моно-инстансе — реальная starvation-дыра).

### Open / long-term
Cross-module precision + minimal-SCC-cut (KEEP 1 члена на цикл вместо всех) — [Q-loop-opt-thresholds]
(open-questions.md). Compiler-synthesized conversions (`str.from(int/f64)` — нет `FnDecl`) держат
`StringBuilder.append(f32)` в KEEP (нет source-доказательства arg-type) — корректный conservative-KEEP, НЕ
shortcut. Частично снимается SIGURG'ом (Plan 144 §7.4, общий async-yield).

## D273 — may-GC effect lattice (Plan 144.0, closes precise-GC hole H4 / Q15)

> **Создан:** 2026-06-14 ([Plan 144.0](../../docs/plans/144.0-may-gc-effect-analysis.md), ветка
> `plan-144-may-gc-effect-analysis`). **Compile-time, EMIT-NOTHING** анализ: вычисляет для каждой
> функции внутренний эффект may-GC. Closes soundness-дыру **H4** ([Plan 144 §7.6](../../docs/plans/144-precise-gc-implementation.md#76))
> и вопрос **Q15** этого слайса. Sibling [D271]: тот же source-level whole-program call-graph
> pre-pass (`fn_key`/overload-резолюция/Tarjan SCC из `preempt_keep.rs`), но другая решётка и
> направление пропагации. **Ничего не эмитит** в генерируемый C — потребляется тиром **O1** позже
> (Plan 144 Ф.2: frame-elision / write-back-skip), отдельно и под гейтом.

### Решётка (двухточечная, дефолт = top)
```
        MayGC   (top, ⊤)   ← ДЕФОЛТ (soundness — H4)
          │
        NoGC    (bottom, ⊥) ← доказывается
```
`is_no_gc(f)` истинно **ТОЛЬКО** когда весь статический конус вызовов из `f` полностью разрешён по
именам И не содержит ни одной аллокации. **Любое сомнение → MayGC.** Ложный NoGC = пропущенный
GC safe-point / корень = use-after-free (как только Ф.2 обопрётся на набор), поэтому соундность
важнее агрессивности элизии.

### Seed `self_may_gc(node)` — узел сам MayGC, если ИСТИННО любое из
1. **Аллоцирует** — тело содержит аллоцирующее выражение (см. allowlist ниже).
2. **Indirect-вызов** — closure / fn-ptr / метод-на-не-self / trailing-block / `with` / `spawn` /
   `select` / `parallel-for`, а также **first-class method value** (`obj.@m` / `Type.@m` в
   value-позиции — codegen эмитит env+closure `nova_alloc`'и): цель/аллокация неизвестна → top.
3. **FFI/extern** — внешняя функция; её may-GC неизвестен → top (C может вызвать Nova-callback).
4. **Неразрешённый callee** — bare-ident / `Type.method` / `module.func`, не сматченный ни к одному
   known `FnDecl` (cross-module / closure-local) → top.

> NB (отличие от [D271]): `address_taken` сам по себе **НЕ** делает узел MayGC — быть first-class
> значением не значит аллоцировать; MayGC у КОЛЛЕРА через `makes_indirect`.

### Принцип allowlist'а аллокации (soundness)
**Allowlist provably-non-allocating, всё прочее → аллоцирует.** Не-аллоцирующими считаются ТОЛЬКО:
целочисленная/плавающая арифметика, сравнения, доступ к полю/индексу без копии, `as`-касты
скаляров, literal-скаляры, return/break/continue, чтение локала, интернированный str-литерал
(`static const u8[]`). Аллоцируют: `RecordLit`/sum-конструкторы/`ArrayLit`/`MapLit`,
интерполяция/конкатенация str (буфер), лямбды/замыкания (env), `spawn`/`detach`/`blocking`/
`supervised`/`parallel-for` (fiber/runtime), boxing escaping-значений, vec/StringBuilder/Map
конструкторы и `.clone()` на heap-типах. **Любой неизвестный AST-узел → аллоцирует (top).**

### Транзитивная пропагация (SCC-конденсация, обратный топо)
MayGC течёт **вверх по коллерам**: SCC помечается MayGC ⟺ любой его член `self_may_gc=true` ИЛИ
любое исходящее ребро (в ДРУГУЮ SCC) ведёт в MayGC-SCC; обработка SCC в обратном топологическом
порядке (callee → caller). `NoGC = {узлы вне MayGC-SCC}`. Рекурсивная SCC с аллокацией в любом
члене → вся SCC MayGC; чистая рекурсия без аллокаций и без MayGC-рёбер → NoGC.

### Соундность над мономорфизацией
Свойства seed (аллокация / indirect / FFI / unresolved) — **исходные** (source-level): вердикт,
вычисленный на шаблоне, наследуется КАЖДЫМ мономорфным/erased-инстансом. Over-approximation
рёбер/seed'ов ⇒ over-approximation MayGC ⇒ ни один реальный may-GC-инстанс не пропущен; spurious-
рёбра лишь теряют элизию (NoGC→MayGC), не ломают соундность. Conservative-gating как
`PreemptKeepSet`: при `populated==false` (анализ не прогонялся над непустой вселенной) — **никто не
NoGC** (всё MayGC).

### Реализация и introspection
`compiler-codegen/src/codegen/may_gc.rs` — `compute_may_gc_set` / `MayGcSet` / `is_no_gc`;
introspection-CLI `nova gc-effect-analyze <path> [--format json|text]` (зеркало `consume-analyze`,
в бинарь ничего не эмитит). `emit_c.rs` НЕ зовёт модуль во время эмиссии (emit-nothing инвариант).

### Acceptance
plan144_0: 3 позитивные (`pure_leaf`/`leaf_forwarder`/`pure_recursion` → NoGC) + 7 негативных
(`allocates_record`/`calls_allocator`/`ffi_call`/`indirect_call`/`method_value`/`recursive_alloc`/
`unknown_callee` → MayGC) фикстуры через релизный `nova gc-effect-analyze`; 19/19 may_gc unit-тестов
PASS; release-сборка чистая, генерируемый C не изменён (emit-nothing). Adversarial-review закрыл
дыру first-class-method-value (`Type.@m` в value-позиции эмитит env+closure alloc — был ложный NoGC,
ровно H4-форма) — `@`-префиксный `Member` теперь seed'ит аллокацию. **Без упрощений как для прода.**

### Open / long-term
Cross-module callee-резолюция, точность str-literal-interning, более тонкая классификация alloc-
сайтов — [Q-may-gc-precision] (open-questions.md). Все консервативны (теряют элизию, остаются
соундны). O1-потребление набора (frame-elision / write-back-skip) — Plan 144 Ф.2,
[M-144.0-may-gc-effect-analysis].


---

## D408. `supervised(deadline:/timeout:)` — областной срок → отмена → `TimeoutError` (Plan 174)

> Расширяет [D50](#d50) / [D75](#d75) (`supervised` / `supervised(cancel: tok)`).
> Нормативная база — план 173 §3a «Bounded-shutdown — дедлайн на SCOPE, не на
> cleanup». Owner sign-off 2026-07-06.

**Форма.** К именованным аргументам keyword-конструкции `supervised` добавлены
`deadline:` и `timeout:` (набор: `cancel:` / `deadline:` / `timeout:`, в любом
порядке, через запятую; каждый — не более одного раза; `deadline:` и `timeout:`
взаимно исключающи):

```nova
supervised(deadline: <Monotonic>) { тело с любыми эффектами }   // абсолютная точка (канон)
supervised(timeout:  <Duration>)  { тело с любыми эффектами }   // относительный сахар
supervised(cancel: tok, timeout: 5.seconds()) { ... }          // комбинируется с cancel:
```

- **`deadline: <Monotonic>`** — абсолютная точка на монотонных часах (D124);
  канон, потому что при пропагации во вложенные области складываются точки, а не
  относительные интервалы (иначе внутренний интервал «съезжал» бы на своё время
  входа). Codegen извлекает `i64 nanos` value-record'а.
- **`timeout: <Duration>`** — относительный сахар, равный
  `deadline: Monotonic.now() + d`. Codegen лоуэрит в
  `_nova_monotonic_ns() + (int64_t)d.nanos`.

Обе формы низводятся к абсолютному сроку в наносекундах (`NovaFiberQueue.deadline_ns`,
0 = нет срока).

**Структурная конструкция, НЕ функция-обёртка.** В отличие от ретрактируемого
`with_timeout[T](ms, fn() -> T)` (std/concurrency), который берёт **чистую**
`fn() -> T` и потому НЕ пропускает эффекты тела, `supervised(deadline:)` — часть
самой keyword-конструкции: тело выполняется с любым effect-row (`Http`, `Net`,
`Fail[E]`, …), эффекты протекают наружу естественно.

**Механика (таймер → отмена → TimeoutError).**
1. **Вход области.** `nova_scope_init` наследует срок enclosing-области
   (`_nova_active_scope->deadline_ns`); codegen ужимает его собственным
   `deadline:`/`timeout:` через `nova_deadline_combine` (минимум ненулевых —
   **внутренний срок может только УЖЕСТОЧИТЬ, никогда не продлить** внешний).
2. **Драйв.** `nova_supervised_run_impl` ограничивает свою idle-парковку сроком
   (`_nova_scope_deadline_run_once`: armed stack-timer + `uv_run(UV_RUN_ONCE)`),
   так что drain-loop просыпается в точке срока даже когда все волокна
   запаркованы на далёком `Time.sleep`/сетевом ожидании.
3. **Истечение.** В точке срока (и только если область ещё не отменена cancel-
   токеном — earliest-of-two) доставляется **кооперативная областная отмена**
   ровно тем же путём, что `cancel:` (`nova_scope_deliver_cancel`: cancel_requested
   + wake всех запаркованных волокон + worker-fibers + driver). Sleep/сетевой
   park прерывается РАНО (не досыпает до конца), cleanup'ы (defer/consume)
   добегают (completes-by-default, §3a).
4. **Наружу.** По завершении drain, если срок сработал, наружу летит
   **типизированный `TimeoutError { deadline_ns i64 }`** (prelude/errors) —
   ловится `with Fail[TimeoutError]` или `is TimeoutError` (D54/174.3). Таймер
   гасится при нормальном выходе (stack-timer — ноль утечки).

**USER-precedence.** Если внутри области случилась НАСТОЯЩАЯ user-ошибка И истёк
срок — наружу летит user-ошибка (не `TimeoutError`): срок лишь отменил siblings.
Только CANCEL-исход (или его отсутствие) при сработавшем сроке → `TimeoutError`.

**Нулевой / отрицательный / прошедший срок.** `timeout: 0` (или прошедший
`deadline:`) → срок истёк немедленно: дети отменяются на первом suspend, наружу
`TimeoutError`. Значение runtime'ное (выражение `Duration`/`Monotonic`) — не
compile-ошибка; мгновенный таймаут — принцип D317 (никогда silent-мусор).

**Вложенность (пропагация точки).** Плоский нижележащий `supervised {}` без
своего срока наследует ambient-срок через `nova_scope_init` (ноль изменений
codegen — byte-identical для не-deadline областей). Внутренний `supervised(timeout: 30s)`
внутри внешнего `supervised(timeout: 100ms)` эффективно ограничен 100ms.

**Longjmp-safety.** Throw (в т.ч. `TimeoutError`), пробивающий тело внешней
области, чей run-loop ещё не исполнялся, восстанавливает `_nova_active_scope`
(a) в `nova_supervised_run_impl` на всех путях выхода и (b) в `with Fail[...]`-
блоке при входе/выходе — иначе следующая область унаследовала бы `deadline_ns` из
освобождённого stack-фрейма (spurious immediate `TimeoutError`).

**Отличие от `CleanupTimeoutError` (D192).** Та была про cleanup-бюджет ОДНОГО
ресурса — РЕТРАКТИРОВАНА §3a и УДАЛЕНА из prelude (Plan 173 Ф.5 п.2:
watchdog-варн + `duration_ms`/`overrun` в ResourceTrace exit-событии);
эта — про срок ЦЕЛОЙ области (bounded-shutdown) и живёт.

**Ретракция `with_timeout` (§3a п.4) — UNBLOCKED, отложена маркером.** Теперь,
когда `supervised(timeout:)` приземлён, `with_timeout[T]`/`within[T]`
(std/concurrency/cancellation.nv) субсумированы и подлежат удалению вместе с
миграцией ~7 тест-ссылок. Не сделано в этом заходе (не «дёшево/безопасно» —
cancellation.nv уже независимо сломан retired-API-дрейфом) → маркер
`[M-174-retract-with-timeout]`. `race2` остаётся до общего `race` (173.1 §2a).

**Реализация:** parser `parse_supervised` (мультиаргумент); AST
`Supervised { body, cancel, deadline: Option<SupervisedDeadline{expr, relative} >}`;
codegen `emit_supervised` (`.nanos`-извлечение + `nova_deadline_combine`);
runtime `nova_rt/fibers.h` (`NovaFiberQueue.deadline_ns`/`saved_active_scope`,
`nova_scope_init` inherit, `nova_deadline_combine`/`nova_scope_deliver_cancel`/
`_nova_scope_deadline_run_once`/`nova_throw_scope_timeout` + run_impl deadline-gate);
typed-throw splice `_nova_throw_scope_timeout_impl` (по образцу ныне-удалённого CleanupTimeoutError-splice);
`with Fail[...]` active-scope restore. Тесты: `std/concurrency/supervised_deadline_test.nv`
(8/8: within-budget; timeout→TimeoutError+`is`; sleep interrupted early <2000ms для
sleep(5000); абсолютный deadline; вложенность inner-can't-extend; deadline+cancel
earliest-of-two ×2; zero→immediate).

## D449. `supervised(on_timeout:)` — срок как ЗНАЧЕНИЕ, а не как эффект

> Принято владельцем 2026-08-09 («`on_timeout: |_| Err(ReadFailed)` — ОК»).
> Расширяет [D408](#d408) (`deadline:`/`timeout:`). Записано ДО реализации:
> спека первой, окно реализует по решению.

### Что

К набору именованных аргументов `supervised` (`cancel:` / `deadline:` /
`timeout:`) добавляется **необязательный** `on_timeout:` — обработчик, который
вызывается вместо возбуждения `Fail[TimeoutError]`:

```nova
ro res = supervised(timeout: 10.to_millis(), on_timeout: |_e| Err(ReadFailed)) {
    read_headers(client, max_bytes)
}
```

**Тип конструкции всегда равен типу тела.** Обработчик обязан вернуть значение
того же типа; несовпадение — ошибка компиляции. Обработчик получает
`TimeoutError { deadline_ns i64 }`.

**Без `on_timeout:` поведение прежнее** — `Fail[TimeoutError]` летит наружу
(D408) и ловится где угодно выше через `with Fail[TimeoutError]`.

### Зачем

Сегодня «сколько ждём» стоит в `supervised`, а «что делать, если не дождались» —
уровнем выше, в обёртке `with Fail`, между ними уровень вложенности:

```nova
with Fail[TimeoutError] = |_e| Err(ReadFailed) {          // что делать
    supervised(timeout: 10.to_millis()) { read_headers(…) }   // сколько ждём
}
```

Выигрыш не в длине, а в **близости**: два решения об одном сроке стоят рядом.

### Почему обработчик, а не готовое значение ошибки

Рассматривалась форма `on_timeout_err: ReadFailed` (исходное предложение
владельца). Отвергнута по одной причине: она заставляет конструкцию **менять
свой тип** в зависимости от набора аргументов — без аргумента `supervised { … }`
даёт тип тела, с ним `Result[тело, E]`. Такую неоднородность трудно объяснить и
легко забыть; вдобавок форма требует, чтобы тело **уже** возвращало `Result`.

Обработчик свободен от обоих недостатков и покрывает исходную форму как частный
случай — `on_timeout: |_| Err(ReadFailed)`. Он же сохраняет `deadline_ns`,
который при передаче готового значения теряется.

### Прецеденты

[D43](03-syntax.md#d43) (функциональный параметр за скобками вызова),
[D102](03-syntax.md) (именованные аргументы), `with Fail[E] = |e| …`
(обработчик как значение — уже в языке).

### Открыто (не блокирует реализацию `timeout:`)

* нужен ли парный `on_cancel:` для внешней отмены (`cancel:`), или отмена и срок
  делят один обработчик;
* поведение, если обработчик сам падает либо сам не укладывается в срок;
* при `deadline:` — один обработчик на область или по одному на вид завершения.

### Порядок реализации

**Зависит от №457 (реестр 221.1)**: `supervised(timeout:)`
сегодня не достаёт до `.read()`. Строить форму поверх несрабатывающего механизма
нельзя — фикстура будет зелёной и при работающем, и при неработающем. Сперва
№457, затем D449. Решение владельца: делать **до тега** безусловно.

## D414. Structured error propagation — primary-selection precedence, detach-policy, channel closed-vs-value (Plan 173 Ф.3)

> Дом Ф.3-остатка плана [173](../../docs/plans/173-error-system-unify-harden.md)
> (structured-concurrency error handling), поверх субстрата [173.0](../../docs/plans/173.0-concurrency-runtime-substrate.md)
> (per-slot `child_error[]` + serialized decision-loop). Расширяет [D50](#d50)
> (`spawn`/`detach`), [D75](#d75) (`supervised`), [D94](#d94) (`select`);
> опирается на [D13](08-runtime.md#d13) (три уровня катастрофы). Owner sign-off
> 2026-06-21 (§3a/§3b); реализация 2026-07-09.

### §1. Primary-selection precedence: **PANIC > USER/USER_TYPED > CANCEL**

Когда `supervised`/`parallel for`-scope удержал **несколько** падений детей
**разных kind** (субстрат 173.0 хранит каждое в своём слоте `child_error[]`, не
схлопывает), при дефолтной эскалации (нет супервизора — [D50](#d50)/[173.2](../../docs/plans/173.2-supervision-as-effect.md))
**primary** (перевыбрасывается наружу) выбирается строгим рангом:

| kind | rank | смысл |
|---|---|---|
| `PANIC` | 3 | fiber-катастрофа (bug/abort-class, D13); **не деградирует до ловимого USER** |
| `USER` / `USER_TYPED` | 2 | управляемая ошибка (реальная throw-ошибка) |
| `CANCEL` | 1 | кооперативная отмена siblings (следствие чужого падения, не корневая причина) |

**Правило overwrite:** входящая ошибка становится primary ⇔ `rank(incoming) >
rank(current)` (строго больше → **ties keep-first**: first-PANIC-wins,
first-USER-wins, first-CANCEL-wins). Не-primary ошибки уходят в **suppressed**-
карман ([D158](02-types.md#d158) / Ф.4 — MultiError-агрегация).

**Инварианты и мотивация.**
- **D13-соундность:** panic ребёнка **всегда** становится primary, даже если
  реальная USER-ошибка была записана раньше — panic не должен «прятаться» за
  ловимой ошибкой (иначе `with Fail[E]` на call-site мог бы проглотить процесс-
  катастрофу). Порядок прибытия под M:N недетерминирован — ранг делает выбор
  детерминированным.
- **USER > CANCEL:** реальная ошибка приоритетнее отмены (Go `errgroup` делает
  first-wins и **теряет** реальную ошибку, если отмена случилась раньше — у Nova
  не теряется). CANCEL — производное состояние (siblings отменены из-за первого
  падения), оно не должно вытеснять корневую причину.

**Реализация.** `nova_throw_kind_precedence(NovaThrowKind)` (`nova_rt/fibers.h`) —
единый ранг; используется обеими report-точками: `nova_fiber_report_error_kinded`
(local/single-thread) и `nova_fiber_report_atomic_kinded` (M:N cross-worker
CAS-loop). Заменяет прежнюю 2-уровневую CANCEL→USER-таблицу (роняла входящий
PANIC на уже-записанном USER). Тест: `nova_tests/expected_runtime/supervised_precedence_panic_over_user.nv`
(10 USER + 10 PANIC детей → primary детерминированно PANIC, ×5 стабильно).
Стыкуется с catchability-инвариантом [Ф.4 п.6](../../docs/plans/173-error-system-unify-harden.md).

> **Амендмент (2026-07-13, волна «173 хвосты»): suppressed-агрегация РЕАЛИЗОВАНА.**
> Обещание «Не-primary ошибки уходят в suppressed-карман» теперь факт: re-throw
> хвост `nova_supervised_run_impl` (fibers.h) собирает все ПРОЧИЕ retained
> детские падения в suppressed-цепочку primary-броска (staging-слот
> `_nova_pending_suppressed` → транспорт через `nova_last_error_set`); после
> ловли primary они читаются **`suppressed() -> []any`** — та же поверхность
> [D158](02-types.md#d158) модели Б, что у cleanup-ошибок (`is T`/`try_as[T]`
> narrowing). Паритет Kotlin `coroutineScope` / Java `Joiner` (агрегация),
> строго лучше Swift TaskGroup first-wins.
> **В карман НЕ попадают:** CANCEL-производные падения siblings (следствие
> эскалации, не корневая причина) и Stop-решённые супервизором ошибки
> ([D416](#d416) — хендлер осознанно выкинул; retained в `child_error[]` для
> observability, наружу не текут). Primary исключается по идентичности
> (msg+payload/tid/kind — typed-броски делят литерал `msg_repr`); дубликаты
> схлопывает D193 identity-check. Видимый порядок = порядок слотов
> (spawn-порядок). Попутный фикс ABI: `nova_any_from_boxed` (typeid.h) для
> value-ABI примитивов (tid 1..7) кладёт heap-box значения в `data` напрямую —
> `try_as[int]` на suppressed-элементе возвращал адрес бокса вместо значения.
> Тесты: `nova_tests/err173_2/scope_multierror_test.nv` (детерминированный
> supervisor-барьер: оба слота retained; Stop не течёт; default-Escalate
> инвариант).

### §2. `detach` error-policy + enforcement эффекта `Detach`

**Enforcement (checker, `CapabilityCtx`).** `detach { … }` — fire-and-forget
задача, переживающая caller'а (orphan fiber, глобальный supervisor, не локальный
scope) — это **наблюдаемый capability** и **требует эффект `Detach`** в effect-row
enclosing-`fn` (норма [D50](#d50), ранее не проверялась — bootstrap-gap). Checker
отвергает `detach` без `Detach` чистым **`[E_DETACH_REQUIRES_EFFECT]`**. Разрешено
без объявления когда:
- **effect-root** — тело `test`-блока (нет сигнатуры; корень, как `main` для
  скрипта) → bare `detach` легален (гасится дефолт-handler'ом);
- **ambient handler** — `with Detach = …` в лексическом scope (эффект погашен на
  месте; `with_handler_stack`-проверка);
- **handler-op-литерал** — тело операции `effect X { op(){ detach … } }`
  эффект-полиморфно (эффект — обязанность use-site handler'а, не definition-site);
  capability-walker такие тела не обходит. Полная HOF-эффект-полиморфная
  проверка (Swift `rethrows`-аналог) — **вне периметра 173** (см. хаб НЕ-цели).

Тесты: neg `nova_tests/err173/neg/f3_detach_requires_effect` (fn без `Detach` →
`[E_DETACH_REQUIRES_EFFECT]`); pos `nova_tests/err173_2/detach_effect_ok_test`
(fn с `Detach` + test-root).

**Error-policy словарь (runtime).** Что происходит с ошибкой/паникой в detached-
fiber'е (у сироты нет call-site, некому вернуть `Result`):
- **`LogAndDrop` (default)** — throw из orphan-тела → `fprintf(stderr, …)` +
  fiber умирает чисто; caller НЕ abort'ится, другие orphans + main продолжают
  (`runtime.c`, [§3.1](#31-default-detach-semantic--asyncdetach-plan-83452-ф4-2026-05-23)).
  panic — critical-класс с D13-семантикой («fiber мёртв»). Паритет Go `go fn()`
  (паника горутины роняет процесс — Nova orphan-panic логируется, процесс жив по
  дефолту), tokio `JoinHandle`-drop.
- **`escalate-to-scope` (opt-in, design)** — детач привязывается к enclosing
  `supervised`-scope вместо глобального orphan-supervisor'а: ошибка сироты
  эскалируется в scope (участвует в §1-precedence как обычный child-fail). Это
  **opt-in**, т.к. противоречит fire-and-forget-семантике дефолта (сирота по
  определению переживает scope). Рычаг привязки — будущий `detach in <scope>` /
  атрибут; runtime-часть отслеживается `[M-173-detach-escalate-to-scope]` (нужен
  scope-handle у detach-примитива + участие в decision-loop). Дефолт остаётся
  `LogAndDrop` (изменение семантики orphan'а — owner-gated).

### §3. Channel closed-vs-value — `recv → Option` канон + select `None`-арм

**Канон (Ред. 2, sign-off).** `Channel[T].recv() -> Option[T]` **ОСТАЁТСЯ**
(D91/[D94](#d94)); Result-миграции **не будет**. `None` ⇔ канал closed **И** буфер
пуст (после close буфер сперва дренится, потом `None`). `try_recv()` различает
EMPTY (пусто, открыт) от closed через `is_closed()`. Это механизм, на котором
**стоит 173.1-десугар** `parallel for` (completion-order dense через Option-канал).
Прямое различение: `match rx.recv() { Some(v) => …; None => closed }`.

**Select `None`-арм (реализовано Plan 173 Ф.3 п.3).** Раньше select **не различал**
`Some(v)` от `None` в dispatch (bootstrap-упрощение — любой ready recv-арм
срабатывал). Теперь recv-арм несёт паттерн (`SelectSlot.want_none`, `channels.h`):
- `Some(v) = rx` — ready **только** когда есть значение (`count > 0`);
- `None = rx` — ready **только** когда closed **и** буфер пуст;
- `_ = rx` — ready на любой результат (value ИЛИ closed).

Readiness — в `nova_select_try_immediate` (централизованно; park-путь re-check'ает).
`close()` будит запаркованные select-recv-waiters → `None`-арм срабатывает при
переходе канала в closed. **Edge (документирован):** одинокий `None = rx` на
уже-closed канале с **непустым** буфером не срабатывает (ждёт пустоты; дренаж —
задача sibling `Some`-арма); без sibling'а → `panic "select: all channels closed"`
(чистый отказ, не hang) — идиоматично `None` пара́ется с `Some` на том же канале.

Тесты: `nova_tests/err173_2/{channel_closed_vs_value,select_none_arm}_test`
(recv-различение; select None/Some/wildcard на value/closed/multi-channel;
буфер-дренаж-до-None; wildcard-регресс).

### §4. `supervised` — value-expression; `parallel for → []T` — канальный сбор, completion-order dense (Plan 173.1, 2026-07-09)

**`supervised { … v }` — value-expression (Ф.1).** Возвращает trailing-выражение
тела, вычисленное **ПОСЛЕ join'а всех детей** (post-join — мутации детей видны в
`v`). Void-форма — unit. Bootstrap-заглушка «возвращает unit» снята. `parallel
for` = сахар над этой формой. Codegen: результат объявляется вне scope-C-блока;
unit-типизированный trailing остаётся eager/pre-join (байт-паритет со старым
поведением — типичный случай «`spawn {…}` последним стейтментом»).

**`parallel for x in xs { f(x) } → []T` (Ф.2) — для ЛЮБОГО `T` и ЛЮБОГО
итератора.** Сбор через канал, семантика §2.3 плана 173.1:

- **Клон-в-родителе:** `Sender`-клон создаётся в РОДИТЕЛЕ на момент `spawn`
  (`writer_count++` строго ДО закрытия родительского tx) и move'ится в ребёнка;
  ребёнок владеет и закрывает клон на **любом** выходе (success/throw/cancel).
- **Транспорт элемента:** int-скаляры ≤64 бит — прямо в слот канала;
  heap-указатели — через `intptr_t`; value-типы (f64/str/value-record/tuple/
  Option/Result) — **boxed-копией** (GC-бокс; drain разыменовывает и пушит
  копию). Политика одна на send/recv-стороны (`parfor_chan_repr`).
- **Дренаж внутри scope:** выделенный drain-fiber — единственный потребитель;
  `recv()` до `None` (= последний клон закрыт), `push` в `Vec[T]`. Плотный
  сбор, **completion order** — упавший ребёнок не шлёт (дыр нет); итерационный
  порядок НЕ гарантирован (нужен порядок — `xs.sort()`).
- **Back-pressure:** буфер `K = min(len, CAP)`, `CAP = 16` — память O(CAP), не
  O(N); ленивый итератор (Iter-protocol, без `len()`) → сразу CAP.
- **Escalate:** ошибка ребёнка → отмена siblings + re-throw после дренажа
  (субстрат 173.0) — накопленный массив НЕ возвращается. Stop-стратегия
  (супервизор) — [173.2](../../docs/plans/173.2-supervision-as-effect.md).
- Прежний примитив-whitelist `{int,bool,f64,str}` + slot-запись
  `result.data[idx]` + interim-guard `[E_PARFOR_RESULT_UNSUPPORTED]` —
  **УДАЛЕНЫ** (D71-amend; `[M-parfor-record-result-miscompile]` закрыт).

**Runtime-хардненинг канала (обнаружено канальным сбором при N≥1000, armed M:N):**
- `[M-chan-spurious-wake-retry]` — `send`/`recv` обязаны **ретраить** spurious
  wake (не fired, канал открыт): прежний `send` молча РОНЯЛ значение, прежний
  `recv` возвращал ложный `None` (Go `chansend`/`chanrecv` — та же петля).
- `[M-chan-close-phantom-zero]` — close-side wake **не** ставит `fired=1`
  (fired = «значение передано»): прежний CAS давал parked-recv фантомный
  `Some(0)` (len = N+1) и ложный «отправлено» parked-send'у.

Тесты: `nova_tests/err173_1/` — `parfor_elem_matrix` (вся матрица видов
элемента), `parfor_iter_edge` (Iter-протокол без len / пустой / один /
completion-плотность / N=2000 back-pressure / Escalate), `supervised_value_smoke`,
`neg/parfor_openended_range`; `nova_tests/concurrency/parallel_for_array.nv`
(set-equality канон для user-тестов).

## D415. Data-race freedom — `#share`-атрибут, capture-check, consume-в-spawn (Plan 173.3)

> Дом [Plan 173.3](../../docs/plans/173.3-data-race-freedom-share.md)
> (data-race-freedom: гонка → compile-error). Owner sign-off 2026-06-21
> (решения §2 плана); реализация 2026-07-09/10. Расширяет [D50](#d50)
> (`spawn`), [D75](#d75) (`supervised`), [D14](#d14-fiber-runtime--невидимая-инфраструктура);
> поглощает Plan 118.3 `#fiber_send`/`E_POINTER_CROSS_FIBER` (§5).
> Мировой контекст: Rust `Send`/`Sync` ≈ Swift 6 `Sendable` (тип-маркер,
> авто-вывод по полям, проверка на границе конкуренции, audited unsafe-escape)
> победили Pony-caps по эргономике; Go `-race` (рантайм) — анти-модель.

### §0. `#share` — АТРИБУТ типа, НЕ протокол

`#share` — bare-маркер на type-декларации (как `#zero_on_move`), парсится в
`TypeAttr::Share`. **Протоколом не является намеренно**: пустой протокол
(маркер без метода) был бы структурно истинен для всякого типа — систему
протоколов (`#impl`, `verify_impl_protocols`) не трогаем. Применим только к
kinds с собственной instance-идентичностью: record / named tuple / newtype /
opaque / sum; на effect/protocol/alias/type-set → **`[E_SHARE_INVALID_KIND]`**.

### §1. Одна ось: share. Два уровня доступа. Авто-вывод + ядовитая база

**Ось одна** — «можно ли алиасить значение из другого файбера». Send/move-оси
(как в Rust) НЕ нужно: под общим GC move всегда memory-safe — его покрывает
`consume` (§4). Чем алиас ОПАСЕН — зависит от доступа, который он даёт
(зеркало Rust-инварианта `T: Sync ⇔ &T: Send`):

- **read-alias-safe** (`is_alias_read_safe`) — безопасно алиасить для ЧТЕНИЯ
  (`ro`-захват / immutable-поле): примитивы, `()`, deep-immutable агрегаты
  memberwise, контейнеры/tuple/Option/Result из read-safe элементов.
- **mut-alias-safe** (`is_mut_alias_safe`) — безопасно алиасить для МУТАЦИИ
  (mut-биндинг, захваченный телом `spawn`): ТОЛЬКО внутренняя синхронизация —
  audited `#share`-vouch (§2), или агрегат, все пути мутации которого
  упираются в такой тип. Голый `mut int`-аккумулятор — мотивирующая гонка
  плана — read-safe, но **НЕ** mut-alias-safe.

**Авто-вывод (memberwise, зеркало auto-derive Plan 126):** обычный тип share
без единой аннотации — record share ⇔ каждое immutable-поле read-alias-safe
И каждое `mut`-поле mut-alias-safe; sum/named-tuple — по payload'ам (read).
Считается по запросу capture-check'а (`protocols/share_check.rs`), нигде не
кешируется и не публикуется.

**Ядовитая база:** сырой указатель `*T` (любой pointee-модификатор) — не-share
структурно, оба уровня (аналог Rust `UnsafeCell: !Sync`); его binding-близнец —
записываемая ячейка (`mut`-поле / mut-скаляр) без audited-синхронизации вокруг.
Тип, транзитивно содержащий ядовитую базу, не-share; **единственный выход** —
собственный `#share`-vouch типа.

**Консервативные направления (V1):** неразрешённое имя (generic-параметр `T`,
registry-miss) / fn-тип / anonymous-protocol / opaque-без-vouch / Effect →
не-share. `Share`-bound для дженериков — вне V1 (см. §7 Q3/Q7).

### §2. Capture-rule на границе `spawn` / `parallel for` / `detach` → `E_CONCURRENT_MUT_CAPTURE`

Тело `spawn`/`parallel for`/`detach` исполняется в НОВОМ файбере (M:N —
возможно на другом OS-потоке). Свободные переменные тела, резолвящиеся во
ВНЕШНИЕ биндинги (лексический скан `CapabilityCtx`, стек scope-фреймов),
могут пересечь границу только тремя способами:

| захват | правило |
|---|---|
| `consume`-move (§4) | отдал во владение ребёнка — виден в коде, всегда ок |
| `ro`-биндинг | deep-immutable view (D246) — всегда ок, share не требуется |
| `mut`-биндинг | ок ⇔ тип mut-alias-safe (§1); иначе **`[E_CONCURRENT_MUT_CAPTURE]`** |

**Audited vouch (`#share` в их `.nv`)** несут sync-примитивы std —
`Mutex`/`RwLock`/`ReentrantMutex`/`WaitGroup`/`Once`/`OnceCell`/`Lazy`/
`Barrier`/`Condvar`/`CountDownLatch`/`Semaphore`/все `Atomic*` (std/runtime/sync.nv):
«внутри настоящая синхронизация, авто-вывод её не видит через opaque-handle».
Это утверждение АВТОРА, аналог Rust `unsafe impl Sync` — компилятор верит без
проверки (потому «audited»). Пользовательский lock-free тип пишет тот же
`#share` — механизм один (§3).

**V1-границы скана (задокументированные, направление — недолов, не
false-positive):** биндинги замыканий не трекаются как внешние (имя не
найдено → не флагаем); типы биндингов — из аннотаций + синтаксического
эскиза init-выражения (`Type.new(..)`/`Type[T].new(..)`/`Type{..}`/литералы);
не-выводимый тип mut-биндинга → консервативно флагаем.

> **Амендмент ([M-detach-consume-escape-unchecked], владелец P1, 2026-07-22):
> `match`/`if let`/`while let` pattern-биндинги БОЛЬШЕ НЕ «не найдено».**
> Владелец нашёл во флагмане (`examples/flagship/aggregator/src/main.nv`)
> ровно ту дыру, что была здесь честно задокументирована: `match lst.accept()
> { Ok(consume stream) => { detach { …stream… } } }` — `stream` связан явным
> `consume`-sub-bind'ом (D157/D180, `Pattern::Ident{is_consume}`) в
> match-arm'е, ОБЪЕМЛЮЩЕМ относительно `detach`; `detach` захватывал его БЕЗ
> явного move — use-after-consume/use-after-free по выходе из match-arm'а,
> который к моменту исполнения orphan-файбера уже мог завершиться. `spawn
> consume` ловит СИММЕТРИЧНЫЙ случай (§4) — `detach` этого не делал, потому
> что `check_capture_boundary` в принципе не мог УВИДЕТЬ `stream`: ни один
> `match`/`if let`/`while let` arm не заводил scope-фрейм для своих
> pattern-биндингов (`state.scopes` в `types/mod.rs` — единственный
> механизм, которым capture-check резолвит имя до биндинга). Фикс —
> симметричный для `spawn`/`parallel for`/`detach` разом (общая точка входа,
> `check_capture_boundary`), в два слоя:
>
> 1. **`Match`/`IfLet`/`WhileLet` теперь заводят scope-фрейм** для bound-имён
>    своего pattern'а (мутабельность — из `Pattern::Ident.is_mut`, как и
>    везде) ПЕРЕД сканом guard/тела — то же, что уже делали `for`/
>    `parallel for` для loop-переменной. Побочный эффект (сознательно
>    принят, не половинчато): `mut`-бинд из match/if-let/while-let pattern'а,
>    захваченный в spawn/parallel-for/detach БЕЗ типовой аннотации, теперь
>    ТОЖЕ консервативно флагуется `E_CONCURRENT_MUT_CAPTURE` (тип неизвестен
>    на pattern-destructure сайте — тот же «не выводим ⇒ флагуем» путь, что
>    уже стоит для обычного `let`) — раньше это тоже было «имя не найдено →
>    не флагаем», симметричная дыра той же природы, закрыта тем же ходом.
> 2. **`ScopeBinding.linear_pattern`** — явный `consume`-sub-bind
>    (`Ok(consume x)`/`Some(consume x)`) не несёт статической type-аннотации
>    (`ty: None` — V1 не выводит тип pattern-destructure), поэтому обычный
>    `type_decls`-lookup по `ty` не может опознать линейность; сам факт, что
>    ИСХОДНЫЙ синтаксис пометил биндинг `consume`, уже достаточен и
>    авторитетен → флаг читается напрямую, независимо от `ty`.
>
> Диагностика — тот же код, **`E_LINEAR_CAPTURE_IN_FIBER`** (Plan 201/173.1,
> §4 ниже), текст различает «известный consume-тип» и «явный consume-pattern
> без известного типа». Требуемый фикс на месте ошибки — явный move:
> **`detach consume c [= expr] { … }`** — новая, симметричная `spawn consume`
> форма (парсер: `parse_detach`, тот же `Stmt::ConsumeScope`-десугар
> verbatim, минус лишняя `Expr::Block`-обёртка, которая `Spawn` требует, а
> `Detach` — нет, у него `body: Block` напрямую). Neg-фикстура: точный
> флагман-паттерн, `spec_tests/conformance/neg/detach_consume_escape_neg.nv`;
> pos-твин (обе формы `detach consume`):
> `spec_tests/conformance/detach_consume_move_ok.nv`. Флагман переписан на
> `detach consume stream { … }`.

> **Амендмент (owner P1, 2026-07-11): `detach` закрывает идентичный gap.**
> Первоначальная Ф.2-волна вкрутила capture-check только в `spawn`/
> `parallel for` — `detach { }` (D50: orphan-файбер на worker pool,
> fire-and-forget, НИКОГДА не join'ится с родителем) имеет РОВНО ту же
> race-поверхность, но проверка на неё не смотрела: `detach { mut_var = … }`
> над внешним не-`#share` `mut` компилировалась чисто, хотя orphan-тело
> могло мигрировать на другой OS-поток и писать в ту же ячейку конкурентно с
> родителем/siblings без синхронизации. Чекер теперь гоняет ТУ ЖЕ
> `check_capture_boundary` на теле `detach` (`ExprKind::Detach`,
> `types/mod.rs`), диагностика переименована в «`spawn`/`parallel for`/
> `detach` body». Neg-фикстура: `err173_3/neg/mut_capture_in_detach.nv`;
> pos-твин (ro/`#share` в `detach` легален): `share_capture_ok_test.nv`.
>
> **`blocking { }` — НЕ расширяется.** Рассмотрено и отклонено: блок-форма
> `blocking { }` ретрактирована Plan 113 (D172) — парсер сегодня ВСЕГДА
> отвергает `blocking { ... }` (`[D172-block-form-removed]`), конструктор
> `ExprKind::Blocking` больше не производится (dead AST arm, см.
> `[M-dead-exprkind-blocking-vestigial]`, backlog-followups.md). Живая
> замена — атрибут `#blocking` перед `fn`-декларацией (Plan 113): это
> top-level `fn`-item со своим параметрам-скоупом, НЕ closure/lambda-литерал
> — Nova `fn` не замыкает лексический скоуп вызывающего, поэтому у тела
> `#blocking fn` структурно нет capture-поверхности для этой проверки
> (аргументы передаются обычным by-value `fn`-вызовом, что вообще не
> «захват» в смысле §2). `realtime`/`nogc` — тот же класс fn-атрибутов, той
> же причине не касается.
>
> **Принцип (согласовано владельцем 2026-07-11), закрывающий асимметрию
> blocking/detach нормативно — тест для будущих конструктов той же оси:**
> конкурентные конструкты (`spawn` / `parallel for` / `detach`) — БЛОК-формы:
> inline-конкуренция требует захвата внешних биндингов лексическим скэном →
> покрыты capture-check'ом (этот §2); и если *unstructured* (`detach` — не
> joined ни с кем) — требуют capability-эффект (`Detach`, D50/D414 §2),
> тогда как *structured* `spawn` joined супервизором и эффекта не требует.
> Класс-планирования (`blocking` / `realtime`) — АТРИБУТЫ функции (свойство
> ОПЕРАЦИИ — где/как её тело исполняется, — не inline-блок с лексическим
> захватом) → захвата нет → capture-check неприменим по построению → эффекта
> тоже нет (это деталь планирования, не capability; реальную блокирующую
> I/O-операцию уже моделирует соответствующий I/O-эффект — `Net`/`Fs` и
> т.п. — паркующий файбер сам по себе; отдельный `Blocking`-эффект был бы
> дублем этой поверхности и правильно удалён Plan 113/D172).

### §3. §3-философия соблюдена by construction

Компилятор хардкодит только **правило** (авто-вывод + ядовитая база + чтение
`TypeAttr::Share`) — ни одного имени типа в Rust-коде предикатов. `Mutex` не
особый: его share-ность приходит из `#share` в `std/runtime/sync.nv`, ровно
как у пользовательского типа.

### §4. consume-в-spawn — явный move во владение ребёнка

```nova
spawn consume c = expr { body }   // связать и отдать ребёнку
spawn consume c { body }          // уже связанный c — переотдать ребёнку
```

Зеркало `consume c = expr { body }` (D188), но владеющий scope = **ребёнок**:
cleanup (`Cleanup[E]`, D194/D196) срабатывает на выходе тела РЕБЁНКА, не
лексического блока родителя. Десугар: `Spawn(Block[ConsumeScope])` —
переиспользует D188-машинерию verbatim (тело scope'а = тело spawn'а, значит
cleanup-на-выходе-тела = cleanup-на-child-exit «бесплатно», ни одного нового
runtime-примитива). Keyword'а `move` нет — используется `consume` (решение §2
п.8 плана: школа Rust `move ||`, анти-Go-неявный-захват).

**Use-after-consume закрыт by construction:** биндинг `c` существует ТОЛЬКО в
теле ребёнка; обращение после `spawn` — undefined identifier (V1/V2-грабли
173.1). Безопасные не-move захваты (`ro`, `#share`, by-value примитив в
`ro`) — неявные, но проверяемые (§2); полный capture-list
`spawn(consume c, ro cfg)` отвергнут (§7 Q9): обязательны явные только move'ы.

> **Амендмент ([M-detach-consume-escape-unchecked], владелец P1, 2026-07-22):
> `detach consume` — та же форма, симметрично `spawn consume`.**
>
> ```nova
> detach consume c = expr { body }   // связать и отдать orphan-файберу
> detach consume c { body }          // уже связанный c — переотдать orphan-файберу
> ```
>
> Мотивация — см. амендмент к §2 выше: `detach` — РОВНО такая же
> concurrency-граница, как `spawn` (fire-and-forget orphan-файбер, D50,
> может пережить лексический scope родителя), и до этого амендмента не имела
> явного move-эскейпа для consume-типов (`spawn consume` уже была). Десугар
> идентичен: `Detach(Block[ConsumeScope])` — тот же D188-механизм verbatim,
> с той разницей, что `ExprKind::Detach` уже хранит `body: Block` напрямую
> (не `Box<Expr>`, как `Spawn`), поэтому обёртка `Expr::Block` вокруг
> `ConsumeScope`, которую требует `Spawn`-десугар, здесь не нужна — парсер
> (`parse_detach`) строит `Block` со единственным `Stmt::ConsumeScope`
> напрямую. Cleanup срабатывает на выходе тела orphan-файбера, не
> лексического блока родителя — тем самым orphan переживает родителя БЕЗ
> use-after-consume (ownership уже внутри него). Capture-check
> (`check_capture_boundary`), codegen ctx-захват (`emit_detach` — «по
> образцу `emit_spawn`», уже было симметрично ДО этого амендмента) и
> consume-tracker (`consume_walk_isolated_block`, уже вызывался для `detach`
> с тем же телом) не потребовали отдельных изменений под эту форму — все три
> уже были написаны generic по `Block`/`Stmt::ConsumeScope`, не по
> containing-конструкту.
>
> **Коррекция (№379, 2026-08-06):** последнее утверждение об
> `consume_walk_isolated_block` было неточным для «already-bound»-формы
> (`detach consume c { … }`, без `= expr`): в отличие от `spawn`, у `detach`
> не было пост-обхода, повторно применяющего `Consumed`-состояние в
> ПЕРЕЖИВАЮЩЕМ (не изолированном) `ctx` после restore
> (`ExprKind::Spawn`-ветка в `types/mod.rs` это делала уже тогда, `ExprKind::
> Detach`-ветка — нет). На практике: `consume j = Job.new(1); detach consume
> j { … }; j.payload` компилировался чисто — use-after-consume после
> `detach` пропускался. Найдено и закрыто тем же слиянием, что ввело
> мульти-var форму ниже (см. следующий амендмент) — обе ветки теперь зовут
> общий `reapply_spawn_detach_consume_moves`.

> **Амендмент (№379, владелец, 2026-08-06): мульти-var форма
> `spawn`/`detach consume a, b, ... { body }`.**
>
> ```nova
> spawn consume a, b, ... { body }    // все перечисленные — реальный move в ребёнка
> detach consume a, b, ... { body }   // симметрично, в orphan-файбер
> ```
>
> Список идентов (≥2) вместо одного — мульти-var мирроринг single-var формы
> выше, **НЕ** D188-мультивар сахара `consume A, B, C { body }`
> (03-syntax.md, Plan 174-амендмент): та форма — ЧИСТЫЙ сахар над вложенным
> re-consume в ТОМ ЖЕ владеющем scope, тело получает RO-view на каждый
> идент (`E_CONSUME_BLOCK_MOVE_OUT` запрещает вынос владения изнутри). Эта
> форма — противоположность: КАЖДЫЙ перечисленный биндинг переходит в
> РЕАЛЬНОЕ владение ребёнка, move-out изнутри тела разрешён (как у
> single-var `spawn consume c { … }` — `re_consume: false`, «already-bound»
> ветка, move-out-запрет не вводился). Смешение со связывающей формой
> (`spawn consume a, b = expr { … }`) — парс-ошибка
> (`E_SPAWN_CONSUME_MULTIVAR_BINDING_MIX`): список — только re-consume уже
> существующих владеемых биндингов, `=`-инициализация остаётся
> одно-идентной (симметрия с D188-мультивар).
>
> **Десугар** (`parse_spawn_detach_consume_multivar`, `parser/mod.rs`):
> вложенные `Stmt::ConsumeScope` — по одному слою на биндинг, самый
> внутренний слой = настоящее тело пользователя (та же форма, что
> `parse_multi_reconsume_scope` строит для D188-мультивар), КАЖДЫЙ слой с
> `re_consume: false`. Момент перехода владения — НЕ порядок вложенности:
> checker'овский free-variable capture-scan (`capture_scan_stmt`'s
> `Stmt::ConsumeScope`-ветка, types/mod.rs) сканирует ВСЁ тело closure'а на
> owned/linear-ссылки независимо от глубины вложенности и освобождает от
> захвата `init`-идент каждого слоя — так что все перечисленные биндинги
> захватываются (move'ятся) в ребёнка в ОДНОЙ точке (сам `spawn`/`detach`
> statement), до входа в тело ребёнка. Вложенность — чисто parse-time
> сахар, не история последовательного move'а в рантайме.
>
> **Cleanup-порядок: LIFO** — последний перечисленный биндинг лежит в самом
> внутреннем слое (ближе к телу пользователя), поэтому его cleanup
> срабатывает ПЕРВЫМ на обратном пути наружу — то же правило, что у
> D188-мультивар (там оно следствие вложенности; здесь оно то же самое, но
> явно зафиксировано, а не просто «наблюдаемо»).
>
> **Use-after-consume** для ЛЮБОГО перечисленного биндинга закрыт ТЕМ ЖЕ
> механизмом, что single-var форма (D131, «использование потреблённой
> переменной») — обращение после `spawn`/`detach`-statement'а к любому из
> них падает с той же диагностикой. Чекер-фикс: `reapply_spawn_detach_
> consume_moves` (types/mod.rs) — обобщение прежнего однослойного матча
> (`[Stmt::ConsumeScope { .. }] = b.stmts.as_slice()`, ловил ТОЛЬКО первый
> биндинг) на всю цепочку вложенных слоёв; заодно закрыл gap у `detach`
> (см. коррекцию к амендменту 2026-07-22 выше).
>
> Мотивирующий носитель — двунаправленный relay (proxy/tunnel-канон): без
> этой формы файберу нельзя было отдать обе половинки соединения
> (`consume (ar, aw) = a.into_split()`, №378) — одна уходит move'ом, вторая
> становится захватом и (правильно) отвергается capture-check'ом. С формой:
> `spawn consume ar, bw { pump(ar, bw) }` / `spawn consume br, aw { pump(br,
> aw) }` — обе половинки каждого направления явно движутся в свой файбер.
> Фикстуры: `spec_tests/conformance/spawn_detach_consume_multivar_ok.nv`
> (pos, включая 3+-биндинг список, relay-эквивалент на независимых
> consume-локалах без деструктуризации, и regress single-var);
> `neg/spawn_consume_multivar_use_after_neg.nv`,
> `neg/spawn_consume_multivar_binding_mix_neg.nv`.

> **Амендмент (реестр 221.1 №456, `[M-consume-scope-cleanup-not-disarmable]`,
> 2026-08-08): ЯВНОЕ потребление биндинга внутри тела ДИЗАРМИТ его
> авто-`@cleanup`.**
>
> ```nova
> spawn consume r, w {          // обе половины во владении ребёнка
>     …
>     r.close()                 // явное потребление → cleanup r ДИЗАРМЛЕН
>     w.close()                 // то же для w
> }                             // на выходе тела cleanup НЕ запускается
> ```
>
> §4 — ЕДИНСТВЕННАЯ форма, где тело владеет биндингом по-настоящему и
> move-out изнутри РАЗРЕШЁН (см. амендмент №379 выше: «move-out-запрет не
> вводился»). Прежняя редакция §4 говорила, КОГДА cleanup срабатывает («на
> выходе тела ребёнка»), но молчала о том, что делать, если тело этим
> разрешением воспользовалось. Пробел закрывается прямым следствием
> exactly-once ([D131](02-types.md#d131)/[D133](02-types.md#d133)):
> **потреблённое значение не чистится повторно** — иначе одна фибра
> потребляет ресурс ДВАЖДЫ.
>
> **Правило — [D432](02-types.md#d432) §4/§5 verbatim,** без собственной
> механики: те же доказанно-безопасные дизарм-точки (голый `return X`;
> receiver-вызов ЗАРЕГИСТРИРОВАННОГО consume-метода типа `X`, включая
> `X.close()`; передача `X` аргументом на `consume`-позиции вызова) и тот же
> рантайм drop-флаг для ветвлений — потребление в ОДНОЙ ветке `if`/`match`
> не подавляет cleanup на ветке, где потребления не было. Порядок LIFO по
> слоям (амендмент №379) сохраняется: дизарм индивидуален для каждого
> перечисленного биндинга.
>
> **Почему у Plan-201 re-consume формы вопроса не возникало.** У
> `consume X { … }` ([D188](03-syntax.md#d188)-амендмент) ровно то же
> действие — ошибка `E_CONSUME_BLOCK_MOVE_OUT` (§«Правила формы» п.4): тело
> получает ro-view, а не владение, поэтому дизармить нечего. Именно поэтому
> «переиспользует D188-машинерию verbatim» (текст §4 выше) не давало ответа
> для ЭТОЙ формы — D188 запрещает то, что §4 разрешает.
>
> **Что было сломано до поправки.** Флаг `_active` взводился, но НИ ОДНА
> дизарм-точка не могла его найти: реестры имени → флаг наполнялись только
> для Plan-201 re-consume-скоупов и для bare consume-let'ов
> ([D432](02-types.md#d432) §2), а `re_consume=false`-скоуп не попадал ни в
> один. Итог — безусловный второй consume на выходе тела. На носителе
> (`TcpReadHalf`/`TcpWriteHalf` от `into_split()`) это давало
> use-after-free: одна фибра в одиночку гнала общий `split_refcount` 2→1→0 и
> запускала реальное закрытие, после чего вторая половина читала
> освобождённую память (`refcount` уходил в −1), куча портилась и процесс
> зависал внутри `GC_gcollect()`. **Рантайм этого различить не может** —
> «закрылась вторая половина» и «та же половина закрылась дважды» для него
> неотличимы; identity даёт только компилятор. Поэтому защитный флаг «уже
> закрыто» в `std/src/net/tcp.nv` был отвергнут как маскировка.
>
> Фикстуры: `spec_tests/conformance/m456_consume_scope_explicit_close_
> disarm.nv` — ДЕТЕРМИНИРОВАННЫЙ страж класса (счётчик
> `ResourceTrace.on_resource_exit`; обе формы потребления — receiver-вызов и
> consume-параметр хелпера; контроль анти-передизарма «тело не потребляет →
> cleanup обязан сработать»; контроль MaybeConsumed);
> `standalone/m456_crisscross_split_evloop_crash.nv` — исходный носитель на
> двух реальных TCP-соединениях (стохастический, ~1 % прогонов, гейтом
> класса служить не может).

### §5. Поглощение `#fiber_send` / `E_POINTER_CROSS_FIBER` (Plan 118.3) — Ф.4

Ad-hoc запрет Plan 118.3 «сырой `*T` через границу файбера»
(`E_POINTER_CROSS_FIBER` + opt-out `#fiber_send`) **поглощён** принципиальной
проверкой: `*T` = ядовитая база (§1), содержащий тип не-share, mut-захват →
`E_CONCURRENT_MUT_CAPTURE` (фикстура `err173_3/neg/pointer_cell_not_share`).
`#fiber_send`/`E_POINTER_CROSS_FIBER` в компиляторе никогда не существовали
(118.3 Ф.1 не был реализован) — ретайр чисто документальный: opt-out-маркер
per-binding не вводится (эскейп = `#share`-vouch на обёрточном ТИПЕ,
аудируемый в одном месте — декларации, не на каждом биндинге).

### §6. Конкуренция в module-level инициализаторах → `E_CONST_INIT_CONCURRENCY`

Дополнение волны ([M-const-init-concurrency-gate]; амендмент к D199-partition,
03-syntax.md): module-level `ro`/`const`-инициализаторы исполняются в
`nova_consts_init()` **ДО арминга M:N-воркеров** (eager-фикс
[M-lazy-const-init-race]). Конкуренция там лениво подняла бы воркеров
(`_auto_arm_if_needed()`) ПОСРЕДИ consts_init — хвост инициализации стал бы
multi-threaded, возврат гонки. Чекер отвергает в init-выражениях:
`spawn` / `detach` / `supervised` / `parallel for` / `select`, канальные
блокирующие вызовы `.send()`/`.recv()` (паркуют без воркеров = pre-main hang;
имя-эвристика по D91-поверхности; неблокирующие `try_send`/`try_recv`
легальны), вызовы свободных fn с эффектом `Detach` в сигнатуре →
**`[E_CONST_INIT_CONCURRENCY]`**. Обычные runtime/extern-вызовы и аллокация
в `ro`-init ОСТАЮТСЯ легальными (строже — только constexpr-правило `const`,
E_CONST_EFFECT_IN_INIT).

### §7. Резолюции под-вопросов плана (V1)

1. **Захваты хендлеров (Q12):** capability-walker не обходит тела
   handler-op-литералов (эффект-полиморфны, D414 §2) — handler-state вне
   периметра скана V1; эффект-row не несёт `#share`-обязательства. Полная
   HOF-эффект-полиморфная проверка — вне периметра 173 (хаб НЕ-цели).
2. **`#share`-nameability результата spawn/эффекта:** не нужен в V1 — spawn
   возвращает unit (D50), результаты через `#share`-примитивы/каналы; RTN-
   аналог не вводится.
3. **Opt-out `!share`:** НЕ вводится (Rust держал `!Send` нестабильным
   годами). Fiber-affine handle выражается ядовитой базой (сырое `*T`-поле)
   — тип автоматически не-share.
4. **Точная ядовитая база:** `*T`-указатель + записываемая ячейка (`mut`-поле
   вне audited-типа). Обходной путь внутренней мутации — `ref T` (алиас на
   storage): предикат прозрачно рекурсирует в referent с ТЕМ ЖЕ уровнем
   доступа — обхода нет.
5. **`ro`/D246 и `#share`:** derive кормит ось ДОСТУПА, не биндинга:
   immutable-поле — read-правило, `mut`-поле — mut-правило; `ro`-БИНДИНГ на
   границе — всегда ок (D246 deep-immutability). Известная V1-дырка: `ro`-
   алиас pointee, у которого ЖИВ второй mut-алиас у родителя, не ловится —
   зафиксировано как ограничение (направление — недолов).
6. **Половинки канала:** обе выражаются `#share`+`consume` без спец-кейса —
   `ro (tx, rx)`-биндинги захватываются как `ro` (send/recv не требуют mut);
   в десугаре сбора 173.1 Sender'ы clone+move (решение §2 п.9 плана).
7. **Region/`sending`-инференс (Swift SE-0414):** НЕ нужен — `consume`/move
   покрывает передачу уникального, `#share` — алиасинг; регионов нет.
8. **Диагностика leakage:** сообщение E_CONCURRENT_MUT_CAPTURE на use-site
   называет биндинг, лечения И путь первого отказа per-field (закрыто,
   `[M-173.3-share-leakage-explain]` 2026-07-10): предикаты share_check
   возвращают цепочку `Type.field.subfield` + rendered-тип + причину
   («writable cell» / «raw pointer» / «unresolved name» / …) — например
   `` `Outer` is poisoned at `Outer.inner.buf` (`[]u8`): a writable cell
   with no audited synchronization ``. Добавление не-share `mut`-поля вглубь
   типа больше НЕ снимает share-ность молча.
9. **Полнота явности захвата:** принята РЕКОМЕНДАЦИЯ плана — явные только
   move'ы (`spawn consume`); `ro`/`#share`/by-value — неявные-но-проверяемые.
   Полный capture-list отвергнут (многословен для каждого loop-var).

### Диагностики / тесты

| код | что |
|---|---|
| `E_CONCURRENT_MUT_CAPTURE` | mut-захват не-mut-alias-safe типа телом spawn/parallel-for/detach |
| `E_SHARE_INVALID_KIND` | `#share` на kind без instance-идентичности |
| `E_CONST_INIT_CONCURRENCY` | конкуренция в module-level ro/const-инициализаторе |
| `E_LINEAR_CAPTURE_IN_FIBER` | consume-тип (или явный `consume`-pattern-биндинг) захвачен в spawn/parallel-for/detach без move (Plan 201/173.1; [M-detach-consume-escape-unchecked]) |

Тесты: `nova_tests/err173_3/` — pos `share_capture_ok_test` (ro / `#share`-
примитивы / `spawn consume` / авто-share user-тип / user `#share`-vouch),
`const_init_runtime_ok_test`; neg `mut_capture_in_spawn`,
`pointer_cell_not_share`, `mut_field_nested_path` (вложенный 2-сегментный
путь отказа, §7 п.8), `share_invalid_kind`, `use_after_spawn_consume`
(by construction), `const_init_{spawn,supervised,detach_fn}`. Unit —
`protocols/share_check.rs::tests` (15, вкл. 6 на путь отказа). Миграция разом (решение §2 п.10):
std (`concurrency/cancellation.nv` `within`/`race2` — реальные TOCTOU-гонки,
переведены на каналы; `http/servernet` smoke — на каналы) + nova_tests
(~19 файлов — на `Atomic*`).

[M-detach-consume-escape-unchecked] (2026-07-22, §2/§4 амендменты выше) —
`detach consume`-форма + match/if-let/while-let scope-фрейм: neg
`spec_tests/conformance/neg/detach_consume_escape_neg.nv` (точный
флагман-паттерн), pos `spec_tests/conformance/detach_consume_move_ok.nv`
(обе `detach consume` формы). Флагман (`examples/flagship/aggregator/src/
main.nv`) переписан на явный move.

## D416. Supervision-as-effect — `Supervisor`/`on_child_fail(idx int, err any) -> Decision` (Plan 173.2)

> Дом [Plan 173.2](../../docs/plans/173.2-supervision-as-effect.md)
> (sign-off решений §2 2026-06-21; §3b-резолюция owner 2026-06-26; реализация
> 2026-07-10). Стоит на субстрате
> [Plan 173.0](../../docs/plans/173.0-concurrency-runtime-substrate.md)
> (per-slot `child_error[]` retention + serialized decision-loop + R1-guard)
> и [D414](#d414-structured-error-propagation--primary-selection-precedence-detach-policy-channel-closed-vs-value-plan-173-ф3)
> (precedence PANIC > USER > CANCEL). Erlang-style supervision = handler
> strategy (README), НЕ фиксированный набор стратегий-имён.

### §1. Эффект и словарь

`Supervisor` — настоящий эффект в `std/prelude/effects.nv` (§3 — не хардкод
в Rust; лоуэринг = обычный user-effect путь, как `Random`):

```nova
type Decision enum Escalate | Stop
type Supervisor effect {
    on_child_fail(idx int, err any) -> Decision
}
```

- `idx` — retention-слот упавшего ребёнка (spawn-порядок remote-детей,
  173.0 `child_error[]`); `err` — ошибка как `any`: typed-throw payload
  (сужается `err is T`), string-throw / panic — `str`-сообщение.
- **Стратегии = хендлеры** (значения эффекта): `with Supervisor = policy
  { … }`, как `Time`/`Fail`. Встроенные политики —
  `std.concurrency.supervisor.escalate()` / `.stop()`. Имён-стратегий как
  сущности нет.
- **Амендмент 2026-07-10 (решение владельца): `Restart`-семейство
  РЕТРАКТИРОВАНО из словаря** (изначальный §3b-MVP держал
  `Restart`/`RestartAll`/`RestartRest` в типе за компайл-гейтом
  `E_SUPERVISOR_RESTART_GATED`). Мотив: рестарт — идиома акторных систем
  (Erlang/OTP: долгоживущий изолированный процесс), а `supervised` —
  структурная конкуренция с лексическим scope'ом; эталоны класса (Kotlin
  `coroutineScope`, Swift `TaskGroup`, Java `StructuredTaskScope.Joiner`)
  рестарта не имеют. Повтор попытки идиоматично живёт ВНУТРИ тела ребёнка
  (`std/concurrency/retry`) — там нет проблемы изоляции полусостояния.
  Словарь `Escalate | Stop` — ПОЛНЫЙ, решение прод-реди; параметр `attempt`
  не нужен (был осмыслен только с Restart).

### §2. Семантика исполнения

- **Дефолт (нет хендлера) = сегодняшний all-or-throw, байт-паритет.**
  Supervision-ветка рантайма не активируется вовсе: codegen штампует
  `has_supervisor = (_nova_handler_Supervisor != NULL)` на входе scope'а
  (`supervised` / array-mode `parallel for`), false → все новые ветки
  мертвы, репорт-пути byte-идентичны до-173.2.
- **Хендлер установлен → deferred-decision режим scope'а:** падение ребёнка
  пишется ТОЛЬКО в его per-slot `child_error[idx]` (release-publish
  per-slot; ни CAS-выбора primary, ни cancel-бродкаста) — выбор реакции
  принадлежит супервизору, а не упавшему.
- **Сериализация:** `on_child_fail` исполняется на drive-потоке scope'а
  (drain-цикл `nova_supervised_run_impl` — решения принимаются ПОКА siblings
  ещё бегут + финальный catch-up после дрейна, строго до освобождения
  retained SpawnCtx), по одному падению за раз, в порядке слотов;
  ровно один вызов на падение.
- **`Escalate`** — ошибка ребёнка идёт в primary-machinery scope'а
  (тот же precedence-путь D414 PANIC > USER > CANCEL + cancel_requested,
  что и на дефолте) → siblings кооперативно отменяются, scope
  перевыбрасывает primary наружу.
- **`Stop`** — ребёнок «выкинут» (Erlang temporary): без эскалации, без
  отмены siblings, scope продолжает; ошибка остаётся retained per-slot
  (не теряется молча — D414 §критерий 3).
- **panic ребёнка доходит до решения** (kind PANIC — supervision на границе
  fiber'а санкционирована D13); **индуцированные отмены siblings (CANCEL)
  хендлеру не показываются** — следствие эскалации, не корневое падение.
- **Мост рантайм↔Nova:** `_nova_supervisor_decide_fn` — fn-pointer,
  назначаемый generated main() (паттерн `_nova_throw_scope_timeout_fn`);
  импл (`_nova_supervisor_decide_impl`, emit_c splice) читает ambient
  TLS-vtable, боксирует `NovaChildError` в `any`, маппит тег `Decision`
  в `NOVA_SUPERVISE_*`-коды.

### §3. Ограничения хендлера (closed handler effect-set, Q-блок)

- **`throw`/`Fail` разрешён = Escalate-with-handler-error:** вызов хендлера
  огорожен локальным fail-frame (guard от handler-fails-self / longjmp
  мимо drain-цикла); брошенная хендлером ошибка идёт в primary-machinery
  scope'а первой (precedence решает исход: PANIC ребёнка всё равно бьёт
  USER хендлера — D13).
- **`interrupt` запрещён** — `[E_SUPERVISOR_HANDLER_INTERRUPT]`: ранний
  выход из with-блока бросил бы drain с живыми детьми.
- **suspend-операции запрещены** — `[E_SUPERVISOR_HANDLER_SUSPEND]`
  (компилируемое приближение V1: прямой `Time.sleep` в теле хендлера;
  транзитивный вызов через функцию — followup эффект-row-анализом).

### §4. Restart — РЕТРАКТИРОВАН (амендмент 2026-07-10, superseded §3b-гейт)

Исходная §3b-резолюция держала `Restart`-варианты в словаре за компайл-гейтом
`[E_SUPERVISOR_RESTART_GATED]` до появления изоляции restartable-тела.
Решением владельца 2026-07-10 семейство УДАЛЕНО из словаря целиком (мотив —
§1): гейт-диагностика ретрактирована вместе с вариантами, ссылка на
`Decision.Restart` — обычный unknown-variant; runtime-мост маппит любой
не-Stop тег в Escalate (defensive). Маркер `[M-173.2-restart-all-rest]`
закрыт ретракцией. Повтор попытки — `std/concurrency/retry` внутри тела.

### §5. Периметр

Политика применяется к remote-детям armed M:N runtime — дефолтный путь
исполнения (auto-arm в main; источник `(idx, err)` = per-slot
`child_error[]` 173.0, который заполняет только remote-путь).
Bootstrap/single-thread (`NOVA_NO_AUTOARM=1`) и implicit main-scope
(top-level `detach`) остаются на дефолтном Escalate-all — честно
задокументированное ограничение субстрата, не тихий пропуск политики.

### Диагностики / тесты

| код | что |
|---|---|
| `E_SUPERVISOR_HANDLER_INTERRUPT` | `interrupt` в теле Supervisor-хендлера |
| `E_SUPERVISOR_HANDLER_SUSPEND` | `Time.sleep` в теле Supervisor-хендлера |

(`E_SUPERVISOR_RESTART_GATED` ретрактирован вместе с Restart-семейством — §4.)

Тесты: `nova_tests/err173_2/` — pos `supervisor_stop_test` (siblings живут,
scope продолжает; multi-fail; panic-до-решения), `supervisor_escalate_test`
(паритет с дефолтом на идентичном теле; custom-политика с mut-состоянием и
`err is int`-narrowing + `idx`-диапазон; Escalate-with-handler-error),
`supervisor_parfor_test` (Stop = «собери выживших», dense `[]T`); neg
`handler_interrupt_neg`, `handler_sleep_neg` (`restart_gated_neg` удалён
вместе с ретракцией §4 — unknown-variant покрыт общей диагностикой).
Модульные: `std/concurrency/supervisor_test.nv`.

---

## D425. CAS возвращает свидетеля провала: `compare_exchange` `bool` → `Result[(), T]` (Plan 207)

> **Статус:** ✅ landed (Plan 207, 2026-07-15). Amends [D168](#d168-sized-atomic-types--api-contract-plan-1032) §1 (матрица операций). Retag `[M-cas-return-witnessed-value]`.

### Мотив

`AtomicI*/U*/Isize/Usize/Ptr/Bool` (легаси `AtomicInt` тоже) `compare_exchange`/
`compare_exchange_weak` возвращали `bool` — при провале **выбрасывали
свидетеля** (актуально прочитанное значение). C-примитив
`__atomic_compare_exchange_n(&obj, &expected, desired, weak, ...)` УЖЕ пишет
фактически прочитанное значение в `expected` при провале (и оставляет
`expected` неизменным при успехе) — ровно то, что нужно в CAS-retry-цикле,
чтобы пересчитать без повторного `load()` (лишний барьер + окно гонки).
Обеднение было неосознанным — witness лежал в `expected` бесплатно (найдено
при дизайне [206](../plans/206-arithmetic-overflow-policy.md), тот же принцип
«примитив не теряет информацию»).

### Правило

**Новая сигнатура** (заменяет `bool` во всех строках D168 §1, отмеченных `*`):

```nova
fn AtomicX mut @compare_exchange(expected T, desired T) -> Result[(), T]
fn AtomicX mut @compare_exchange(expected T, desired T, success MemOrdering, failure MemOrdering) -> Result[(), T]
fn AtomicX mut @compare_exchange_weak(expected T, desired T) -> Result[(), T]
fn AtomicX mut @compare_exchange_weak(expected T, desired T, success MemOrdering, failure MemOrdering) -> Result[(), T]
```

- `Ok(())` — успех, значение заменено на `desired`.
- `Err(actual)` — провал; `actual` — фактически прочитанное значение
  (witness). Для `compare_exchange` (strong) провал гарантирует
  `actual != expected` (C11: strong CAS не фейлится spuriously). Для
  `compare_exchange_weak` witness корректен и в spurious-failure случае
  (`actual == expected`, но своп не произошёл — платформо-зависимая
  ARM-семантика; вызывающий код должен различать через `Err`, не через
  сравнение значений).
- Применяется ко **всем 11** CAS-методам (было 13 на момент landing 2026-07-15:
  [D426](#d426-atomic-семейство-консолидация-имён-atomicisizeatomicusize--atomicintatomicuint-легаси-atomicint-и-atomicptr-сняты-plan-207),
  2026-07-16, снял легаси `AtomicInt` и `AtomicPtr`, переименовал Isize/Usize
  → Int/Uint): `AtomicI8/I16/I32/I64`, `AtomicU8/U16/U32/U64`, `AtomicInt`,
  `AtomicUint`, `AtomicBool`.

**CAS-retry-цикл идиома** (после правки, мотивирующий пример):

```nova
mut cur = a.load()
loop {
    ro next = f(cur)
    match a.compare_exchange(cur, next) {
        Ok(_) => break
        Err(actual) => cur = actual   // без повторного load(), witness даром
    }
}
```

### Лоуэринг (codegen)

Публичный `compare_exchange`/`compare_exchange_weak` — **plain (non-extern)
`.nv` fn** (не intrinsic): вызывает private `@cmpxchg` intrinsic (module-
private, не exported), который возвращает raw `(ok bool, witness T)` пару из
ОДНОГО atomic op (C11 `__atomic_compare_exchange_n`, `weak`-флаг передан
явным параметром — strong и weak делят один intrinsic), затем строит
`Ok(())`/`Err(witness)` обычным Nova constructor-синтаксисом. Result[(), T]
монoморфизируется штатным generic-codegen (как `Vec.binary_search ->
Result[int,int]`) — hand-written C не участвует в сборке `Result`.

Raw-пара представлена named-tuple типом `CasRaw<W>(ok bool, witness T)`
(один на каждую witness-ширину: I8/I16/I32/I64/U8/U16/U32/U64/Int/Uint/Bool —
`CasRawInt` общий для `AtomicInt`; до [D426](#d426-atomic-семейство-консолидация-имён-atomicisizeatomicusize--atomicintatomicuint-легаси-atomicint-и-atomicptr-сняты-plan-207)
(2026-07-16) его также разделяли `AtomicPtr` и легаси `AtomicInt`, оба сняты).
C-структура
(`NovaTuple_CasRaw*`) — hand-written в `sync_primitives.h`, зарегистрирована в
`RUNTIME_DEFINED_TYPES` (emit_c.rs) — та же конвенция, что `MutexGuard`/
`MemOrdering`. Потребовалось расширение codegen (`emit_type_decl`,
RUNTIME_DEFINED_TYPES-ветка): для NamedTuple-типов в этом списке компилятор
теперь регистрирует field-schema/`NovaTuple_<Name>` type-alias БЕЗ повторной
эмиссии struct-body (раньше эта ветка обрабатывала только `Sum`/`Effect`
kind — NamedTuple падал в generic unknown-type fallback, `Nova_<Name>*`
pointer-record, что не совпадало с реальным value-struct'ом в хедере).

### Границы

Только форма возврата CAS (`bool`→`Result`). НЕ меняет memory-ordering, НЕ
трогает `fetch_*`/`load`/`store`. Закрывает backlog `[M-cas-return-witnessed-value]`.

### Амендмент (cmpxchg-lint волна B, 2026-07-16) — компиляторная диагностика success/failure ordering

> **Решение владельца:** compile-time варн на `failure`-ordering сильнее `success`-ordering — ОК.

Call-сайты `compare_exchange`/`compare_exchange_weak` (все `Atomic*`-типы, 4-арная
explicit-ordering overload) с **литеральными** `MemOrdering`-аргументами получают
две проверки. Не-литеральные (runtime-переменная) `success`/`failure` не
диагностируются — компилятору неизвестно значение.

1. **Hard-error `E_CAS_FAILURE_ORDER_INVALID`**: `failure ∈ {Release, AcqRel}`.
   Семантически невалидно — failure-путь CAS это чистый **load** (сравниваемое
   значение НЕ изменилось), у него нет release-семантики. C11/C++11
   `atomic_compare_exchange_*` явно запрещали `Release`/`AcqRel` как
   failure-memory-order (UB/ill-formed); это правило переносит запрет в
   compile-time диагностику Nova вместо тихого UB/undefined C-поведения.

2. **Warning `W_CAS_FAILURE_STRONGER`**: `strength(failure) > strength(success)`
   по тотальному порядку `Relaxed < Acquire ≈ Release < AcqRel < SeqCst`
   (`Acquire`/`Release` — ничья: разнонаправленная синхронизация, ни один не
   «сильнее»). С C++17 такая комбинация формально валидна (ограничение
   «`failure` не сильнее `success`» из C++11 снято), но почти всегда ошибка
   намерения — failure-путь (CAS не удался) обычно не должен требовать БОЛЬШЕ
   синхронизации, чем success-путь. Non-fatal — компиляция продолжается.

**Легальные (нет диагностики) пары** `(success, failure)`: `SeqCst/SeqCst`,
`AcqRel/Acquire`, `Release/Relaxed` — и любая пара с `strength(failure) <=
strength(success)` и `failure ∉ {Release, AcqRel}`.

Реализация: чекер (`compiler-codegen/src/types/mod.rs::check_cas_ordering`,
вызывается из `BoundCtx::walk_expr` — hard-error, `errors`-sink) + AST-lint
(`compiler-codegen/src/lints.rs::lint_cas_failure_stronger`, вызывается из
`walk_expr_lints` — non-fatal `LintWarning`-sink, тот же split что
`W_PRELUDE_SHADOW`). Receiver распознаётся по **префиксу** `Atomic` (не список
конкретных ширин) — новые sized-варианты семейства подхватываются без правки
диагностики.

**Замечание (после merge с [D426](#d426-atomic-семейство-консолидация-имён-atomicisizeatomicusize--atomicintatomicuint-легаси-atomicint-и-atomicptr-сняты-plan-207)):**
`compare_exchange`/`compare_exchange_weak` — ОДНА сигнатура с default-параметрами
(`success MemOrdering = MemOrdering.SeqCst, failure MemOrdering = MemOrdering.SeqCst`),
не отдельные 2-арг/4-арг overload'ы. Параметры с default — **keyword-only** (D102):
`success`/`failure`, если переданы, ОБЯЗАНЫ передаваться по имени
(`a.compare_exchange(cur, next, success: MemOrdering.Relaxed, failure:
MemOrdering.Acquire)`) — позиционная передача (даже полная, все 4 арг-та) — compile
error, не диагностика этого амендмента. Каждый из `success`/`failure` может быть
пропущен независимо (например только `failure: X` при пропущенном `success`) — тогда
его значение — известный литерал `MemOrdering.SeqCst` (тот же default, что в
сигнатуре), участвующий в обеих проверках наравне с explicit литералом, а не молча
пропускаемый.

---

## D426. Atomic-семейство: консолидация имён — `AtomicIsize`/`AtomicUsize` → `AtomicInt`/`AtomicUint`, легаси `AtomicInt` и `AtomicPtr` сняты (Plan 207)

> **Статус:** ✅ landed (Plan 207, 2026-07-16). Amends [D168](#d168-sized-atomic-types--api-contract-plan-1032) (таблица типов, §1 матрица, §4 AtomicPtr, Эволюция) и [D425](#d425-cas-возвращает-свидетеля-провала-compare_exchange-bool--resultt-plan-207) (список CAS-методов, CasRaw witness-width). Решение владельца 2026-07-16.

### Что

До этого амендмента atomic-семейство содержало имена, не соответствующие
типам Nova: `isize`/`usize` **не существуют** как типы языка (`int`/`uint` уже
address-sized, `intptr_t`/`uintptr_t` — Plan 133), а `AtomicPtr` был
нетипизированным int-proxy — дублем `AtomicInt` под другим именем, причём его
собственный doc-comment заявлял GC-root-регистрацию, которой в
`sync_primitives.h` не было (голый `__atomic_*` над `nova_int`, без единого
вызова root-API). Параллельно существовал отдельный legacy `AtomicInt`
(int32-precision, без `MemOrdering`-параметров, добавлен раньше Plan 103.2 и
никогда не обновлённый) — узкое подмножество API нового address-sized типа.

Три независимых изменения одним коммитом:

1. **`AtomicIsize` → `AtomicInt`.** Legacy `AtomicInt` (int32-backed, узкий
   API: `new/load/store/fetch_add/fetch_sub/compare_exchange` — все без
   `MemOrdering`) снят целиком; его вызовы покрываются 1:1 дефолтными
   SeqCst-перегрузками переименованного типа (все параметры с ordering у
   нового типа имеют `= MemOrdering.SeqCst` дефолт — Plan 207 cmpxchg-rename,
   тот же коммит-серия). Имя `AtomicInt` высвобождено под переименование.
2. **`AtomicUsize` → `AtomicUint`.** Прямое переименование, без конфликта
   имён (legacy `AtomicUint` не существовало).
3. **`AtomicPtr` — снят.** До появления typed generic `AtomicPtr[T]` (Plan
   103.7, требует GC-root integration в codegen — non-trivial) тип не
   существует в Nova. Потребители — только тесты (3 spec_tests файла);
   продакшн-кода, хранящего GC-managed адреса через `AtomicPtr`, не найдено.
   `AtomicInt` — прямая замена там, где нужен голый platform-word int-proxy
   (без каких-либо GC-гарантий, которых `AtomicPtr` и не давал).

### Правило

- `AtomicInt` (было `AtomicIsize`) — platform-word signed atomic, `int` =
  `nova_int` = `intptr_t`. Полный API: `new/load/store/swap/fetch_add/
  fetch_sub/fetch_or/fetch_and/fetch_xor/fetch_max/fetch_min/fetch_nand/
  compare_exchange/compare_exchange_weak`, все с default-SeqCst и
  explicit-`MemOrdering` перегрузками (2-арг / 3-арг с позиционным override
  `success` / 4-арг с явными `success`+`failure` — дефолт-параметры, не
  отдельные overload-декларации после Plan 207 cmpxchg-rename).
- `AtomicUint` (было `AtomicUsize`) — тот же API, `uint` = `nova_uint` =
  `uintptr_t`.
- `AtomicPtr` — **не существует**. Call-сайты, хранящие адрес как int-proxy —
  мигрировать на `AtomicInt`. Typed `AtomicPtr[T]` с GC-tracing — Plan 103.7
  (не переиспользует C-реализацию снятого V1; будет отдельный дизайн).
- `RUNTIME_DEFINED_TYPES`/`debt_is_runtime_backed_newtype`/
  `BUILTIN_RUNTIME_TYPES`/`RUNTIME_NATIVE_CONCRETE_TYPES` (4 независимых
  name-list'а в `emit_c.rs`, историческая accretion — не консолидированы в
  этом амендменте, вне scope) обновлены синхронно: `AtomicIsize`/`AtomicUsize`
  переименованы, `AtomicPtr` удалён, дубликат `AtomicInt`-записи (legacy +
  Isize) схлопнут в одну.

### Почему

1. **Имена типов Nova — `int`/`uint`, не `isize`/`usize`.** Rust-стиль
   именование (`Isize`/`Usize`) вводило читателя в заблуждение — предполагало
   существование отдельных типов `isize`/`usize`, которых в Nova нет (Plan
   133 сделал `int`/`uint` address-sized универсально). Имя atomic-типа
   обязано отражать имя wrapped-типа.
2. **`AtomicPtr` был дублем, не отдельной абстракцией.** Ни один production
   потребитель не зависел от заявленной (и не реализованной) GC-root
   семантики — удерживать нетипизированный int-proxy под отдельным именем
   до появления настоящего `AtomicPtr[T]` не давало преимущества над
   `AtomicInt` и создавало ложное впечатление GC-безопасности.
3. **Legacy `AtomicInt` (int32) был исторической случайностью, не
   дизайн-решением.** Добавлен до формализации Plan 103.2 sized-atomics,
   никогда не расширялся (нет `MemOrdering`, нет `swap`/`fetch_or/and/xor`),
   его собственный doc-comment уже помечал его как «deprecated alias»
   (ошибочно — не алиас `AtomicI64`, а отдельный int32 тип). Держать имя
   занятым под мёртвый API блокировало переиспользование под
   address-sized тип.

### Что отвергнуто

- **Оставить `AtomicPtr` как есть, просто задокументировав отсутствие
  GC-root.** Живой тип с ложной семантикой в имени (`Ptr` подразумевает
  указатель-специфичные гарантии) хуже, чем явное отсутствие типа до
  прихода корректной typed generic формы.
- **Переименовать legacy `AtomicInt` во что-то другое вместо снятия.**
  Узкий API (без ordering) не нёс уникальной ценности — 100% его
  вызовов покрывается дефолт-параметрами нового типа; держать две
  параллельные реализации одного и того же концепта (address-sized atomic
  int) — чистый долг.

### Связь

- [D168](#d168-sized-atomic-types--api-contract-plan-1032) — sized atomic
  types API contract (амендированная таблица типов).
- [D425](#d425-cas-возвращает-свидетеля-провала-compare_exchange-bool--resultt-plan-207)
  — CAS witness value (амендированный список методов/CasRaw).
- [reference-nova-int-intptr-not-i64.md] (проектная память) — `int` =
  `nova_int` = `intptr_t` (address-sized), НЕ `int64_t`; тот же принцип
  применён к `uint`/`nova_uint`/`uintptr_t`.
- Plan 103.7 — typed generic `AtomicPtr[T]` с GC-root integration (deferred;
  не переиспользует V1 C-реализацию, снятую этим амендментом).
- Plan 207 — cmpxchg-rename (тот же коммит-серия): `@__cas_raw` → `@cmpxchg`
  intrinsic rename + схлопывание ordering-перегрузок в default-параметры —
  предпосылка для п.1 (дефолты покрывают legacy 2-арг вызовы).

### Реализация

- `std/src/runtime/sync.nv`: legacy `AtomicInt`-секция и `AtomicPtr`-секция
  удалены; `AtomicIsize`/`AtomicUsize` → `AtomicInt`/`AtomicUint`
  (идентификатор + doc-comments + cross-ref «See Also» в `AtomicI64`/
  `AtomicBool`).
- `compiler-codegen/nova_rt/sync_primitives.h`: те же три изменения на
  C-уровне (`Nova_AtomicInt`/`Nova_AtomicPtr` legacy-структуры удалены,
  `Nova_AtomicIsize`/`Nova_AtomicUsize` → `Nova_AtomicInt`/`Nova_AtomicUint`).
- `compiler-codegen/src/codegen/emit_c.rs`: 4 name-list'а обновлены
  (`RUNTIME_DEFINED_TYPES`, `debt_is_runtime_backed_newtype`'s
  `RUNTIME_BACKED_NEWTYPES`, `BUILTIN_RUNTIME_TYPES`,
  `RUNTIME_NATIVE_CONCRETE_TYPES`).
- Тесты: `std/src/runtime/sync_test.nv`,
  `spec_tests/conformance/plan103_2_atomic_isize_usize.nv`,
  `spec_tests/conformance/neg/plan103_2_atomic_isize_no_such_method_neg.nv`
  мигрированы 1:1 (переименование идентификаторов, не ослабление).
  `spec_tests/conformance/plan103_2_atomic_ptr_basic.nv` и
  `spec_tests/conformance/neg/plan103_2_atomic_ptr_no_such_op_neg.nv`
  удалены вместе со снятым типом (не ослабление — тип снят решением
  владельца, тестировать нечего).

### Границы

Только переименование/снятие в atomic-семействе. НЕ меняет memory-ordering,
`fetch_*`/`load`/`store`/`compare_exchange` семантику сохранённых типов
(D425 остаётся в силе). НЕ вводит `AtomicPtr[T]` (Plan 103.7 — отдельная
работа). НЕ консолидирует 4 отдельных name-list'а в `emit_c.rs` в единый
источник (историческая accretion, вне scope этого амендмента).

---

## D439. `supervised{}` — прямая блокирующая операция в теле без `spawn` (Plan 221.1 №162)

> **Статус:** ✅ landed (реестр 221.1 №162, P1, 2026-07-30), **ограничение
> «cancel:/timeout: не защищают прямую блокирующую операцию тела» (Правило,
> третий пункт, и разбор в «Почему» ниже) ОТМЕНЕНО амендментом
> [D442](#d442) (реестр 221.1 №165, 2026-08-01) — `cancel:`/`timeout:`
> ОБЯЗАНЫ теперь покрывать и её. Текст этого D-блока ниже сохранён как
> исторический разбор корневой причины D439-бага (`_nova_active_scope`/
> `_nova_active_slot` не подменяются на время тела — это решение НЕ
> отменяется, D442 его не трогает); НЕ читать раздел «Правило» ниже как
> действующий — см. D442. Amends
> [D14](#d14-fiber-runtime--невидимая-инфраструктура) (невидимая
> приостановка — распространяет гарантию на `supervised`-тело без spawn),
> [D50](#d50-concurrency-model-spawn-detach-blocking) (`spawn`-only
> concurrency model — уточняет, что НЕ-spawn'нутый код в теле тоже
> легитимен), [D71](#d71-bootstrap-concurrency-runtime) (bootstrap
> scheduler) и [D75](#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном)
> (структурная отмена — уточняет её границу). Решение владельца
> 2026-07-30 (P1, срочно).

### Что

Ни один из D14/D50/D71/D75 не описывал явно, что происходит, когда
блокирующая операция (`Time.sleep`, `Channel.recv`, `select`, любой
sync-primitive wait) написана **непосредственно** в теле `supervised { ... }`
— то есть НЕ внутри вложенного `spawn { ... }`. На практике (до этого
амендмента) реализация ронял процесс на первой же такой операции:

```
nova: nova_sched_park: invalid scope/slot
```

`supervised { spawn { <блокирующая операция> } }` работал; `supervised { <блокирующая операция> }`
(без `spawn`) — падал; без ОБОИХ обёрток (голый top-level) — работал.
Это была недоработка рантайма, не намеренно неподдерживаемая форма —
владелец классифицировал это как P1-баг языка/рантайма, не «программист
должен обернуть в spawn».

### Правило

- Блокирующая операция, написанная непосредственно в теле `supervised{}`
  (без промежуточного `spawn`), **паркуется и резюмируется штатно** — так
  же, как если бы та же операция была написана вне `supervised` вообще.
  Она не аварийно завершает процесс и не требует оборачивания в `spawn`
  для простой корректности.
- **Join-семантика** этого `supervised`-scope (дождаться всех
  `spawn`'нутых детей) не меняется: прямая блокирующая операция в теле
  ничего не меняет в порядке/итоге join'а — она просто выполняется как
  обычный statement тела, до начала join-цикла.
- **Cancel/deadline-семантика** (`supervised(cancel: tok)` /
  `supervised(timeout:/deadline:)`, D75/D408) применяется **только к
  зарегистрированным детям** (`spawn { ... }`), **НЕ к прямой блокирующей
  операции тела**. Причина — структурная, не произвольное ограничение:
  enforcement дедлайна/токена (`nova_scope_deliver_cancel` на просрочку
  `deadline_ns`, forced-wake парковавшихся детей) выполняется
  ИСКЛЮЧИТЕЛЬНО изнутри join-цикла этого scope
  (`nova_supervised_run`/`nova_supervised_run_cancel`), который стартует
  **строго после** того, как все statement'ы тела (включая прямую
  блокирующую операцию, пока она паркуется) уже завершились. Пока тело
  выполняется, join-цикл ЭТОГО scope ещё не запущен — прервать парковку
  внутри тела попросту некому.
- Если нужна защита дедлайном/токеном именно для конкретной блокирующей
  операции — обернуть её в `spawn { ... }`: тогда она получает
  собственный слот в scope и участвует в join-цикле (и его
  cancel/deadline-enforcement) с первого же резюма, как любой другой
  ребёнок.

```nova
// Работает (эта редакция): паркуется/резюмируется штатно, join ждёт
// spawn'нутого ребёнка тоже.
supervised {
    spawn { child_work() }
    (5).to_millis().sleep()      // прямой блок в теле — НЕ падает
}

// timeout НЕ защищает прямой блок тела — он не является child'ом:
supervised(timeout: 10.to_millis()) {
    long_blocking_call()         // если реально блокирует >10мс — ждёт
                                  // ПОЛНОСТЬЮ, timeout проверяется только
                                  // в join-цикле (которого тут пока нет)
}
// Хотите, чтобы timeout мог прервать именно эту операцию — spawn:
supervised(timeout: 10.to_millis()) {
    spawn { long_blocking_call() }   // теперь под join-цикл + cancel
}
```

### Почему

Корень бага (найден инструментированием `nova_sched_park`/
`nova_scope_init`/`emit_supervised`): `nova_scope_init` всегда стартует
scope с `count == 0` (fibers.h) — слоты появляются только через
`nova_fiber_spawn_into`/`nova_scope_alloc_slot`, вызываемые для
по-настоящему `spawn`'нутых детей. Тело `supervised{}` выполняется
**inline**, на ТОЙ ЖЕ coroutine, что вошла в блок — кодогенерация
(`emit_supervised`) лишь подменяла TLS `_nova_active_scope` на новый
scope, не трогая `_nova_active_slot`; прямой блокирующий вызов затем
парковался на (новый_scope, старый_слот) — `nova_sched_park`'овская
проверка `slot < scope->count` проваливалась (`count` всё ещё 0), и
процесс падал.

Первая рассмотренная и **отвергнутая** идея (см. ниже) — завести
«self-slot» для тела в НОВОМ scope, зеркалируя то, как
`emit_nova_main_scoped_inner` заводит тело `main` как отдельный fiber
внутри `_nova_main_scope`. На практике это НЕ работает: единственный
корректный «водитель» (тот, кто реально вызывает `mco_resume` на этой
coroutine) — это ВНЕШНИЙ (родительский) scope или голый `main()`, чья
собственная bookkeeping-структура ничего не знает про парковку,
зарегистрированную во ВНУТРЕННЕМ scope. Внешний водитель, не увидев свой
собственный `parked`-бит выставленным, на следующей же итерации своего
цикла резюмирует ту же coroutine повторно — double-resume гонка поверх
single-winner park/wake протокола. Эмпирически: abort сменился на hang
(таймаут), не на исправление.

Настоящий фикс (`emit_supervised`, `emit_c.rs`) — НЕ подменять
`_nova_active_scope`/`_nova_active_slot` вообще на время выполнения
statement'ов тела. Эта пара TLS и так корректно указывает на (scope,
slot), под которым coroutine РЕАЛЬНО управляется прямо сейчас
(унаследовано от того, кто её резюмировал — внешний `nova_supervised_step`
либо `main()`'s собственный drive-loop) — то есть именно то, на чём
обязана парковаться прямая блокирующая операция, чтобы быть найденной и
разбуженной корректно. `spawn{}` внутри тела не страдает: он маршрутизуется
через compile-time-переменную `current_scope_queue` (никогда не через эту
runtime TLS), а `spawn`'нутый ребёнок получает СВОЮ корректную
(scope,slot) в первый момент, когда join-цикл (`nova_supervised_run` этого
же scope) его реально резюмирует.

Побочный эффект (документированное правило выше, не баг): раз
`_nova_active_scope`/`_nova_active_slot` в теле больше не указывают на
этот НОВЫЙ scope, прямая блокирующая операция физически не может быть
найдена/прервана enforcement'ом ЭТОГО scope (его `parked[]`/`parked_co[]`
— пустые массивы до первого `spawn`). Раз join-цикл этого scope — ЕДИНСТВЕННОЕ
место, где вообще проверяется его `deadline_ns`/`cancel_requested`, а
он стартует только после statement'ов тела — эта асимметрия существовала
бы при ЛЮБОМ решении, не завязанном на реальное создание второй coroutine
для тела (что стоило бы отдельного, гораздо большего архитектурного
изменения — вне P1-рамок этой правки).

### Что отвергнуто

- **Self-slot тела в новом scope** (`nova_scope_alloc_slot(new_scope,
  mco_running())` + подмена `_nova_active_slot`) — двойная регистрация
  одной coroutine в двух scope одновременно; конкурирующий «водитель»
  (внешний scope) резюмирует её повторно, не видя парковку во внутреннем
  scope → double-resume race → hang вместо фикса. Подтверждено
  эмпирически на минимальном репро, отброшено.
- **Статическая проверка в чекере**, запрещающая/предупреждающая о прямой
  блокирующей операции в теле `supervised{}` без `spawn` — не добавлена:
  после фикса эта форма СТАЛА валидной (не ошибочным состоянием, которое
  стоит ловить статически) — единственное её отличие от `spawn`-версии —
  отсутствие enforcement дедлайна/токена ЭТОГО scope, что не является
  синтаксической ошибкой и не поддаётся дешёвой статической проверке
  (компилятор не может знать, действительно ли конкретный вызов способен
  заблокироваться дольше дедлайна). Задокументировано в Правиле выше
  вместо диагностики.
- **Оставить `abort()` как есть** — противоречит P1-классификации
  владельца («это недоработка языка») и ломает валидный, распространённый
  паттерн (accept-loop прямо в `supervised` без обёртки в `spawn`).

### Связь

- [D14](#d14-fiber-runtime--невидимая-инфраструктура) — невидимая
  приостановка; это правило распространяет гарантию на `supervised`-тело.
- [D50](#d50-concurrency-model-spawn-detach-blocking) — `spawn`
  concurrency model; уточняет, что не-`spawn`'нутый блокирующий код в
  теле тоже поддерживается (не требует `spawn` для простой корректности).
- [D75](#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном) /
  D408 (`timeout:`/`deadline:`) — структурная отмена; уточняет границу
  (только зарегистрированные дети).
- Реестр 221.1 №162.

### Реализация

- `compiler-codegen/src/codegen/emit_c.rs::emit_supervised` — убрана
  безусловная подмена `_nova_active_scope = &queue` (и НЕ введена подмена
  `_nova_active_slot`) на время выполнения statement'ов тела;
  `_nova_active_scope`/`_nova_active_slot` не трогаются для этого окна
  вообще (доктрина зафиксирована развёрнутым doc-comment'ом в коде).
- `compiler-codegen/nova_rt/nova_sched.h::nova_sched_park`/
  `nova_sched_park_with_unlock` — диагностическое сообщение при
  оставшемся (защитном) abort-пути расширено: называет ситуацию явно
  («blocking operation attempted with no reserved fiber-slot in this
  scope») вместо голой bounds-check фразы, для остаточных
  runtime/codegen-багов, а не для этого (теперь легитимного) сценария.
- `compiler-codegen/nova_rt/fibers.h` — отвергнутый self-slot подход
  задокументирован комментарием на месте (не кодом), чтобы не
  переоткрывать тот же тупик.
- Регресс-фикстура:
  `spec_tests/conformance/standalone/m162_supervised_body_direct_park.nv`
  — 3 теста: прямой `sleep` без spawn; прямой блок, сосуществующий со
  `spawn`'нутым ребёнком в том же scope; прямой `Channel.recv` без spawn,
  дожидающийся `spawn`'нутого отправителя.

### Границы

- НЕ меняет поведение `spawn`-based `supervised` (join/cancel/deadline
  для зарегистрированных детей — без изменений, регресс-фикстуры D75/
  `supervised_drain_mn_guard`/`m107_supervised_spawn_option_fn_match_bind_call`
  проверены).
- ~~НЕ добавляет enforcement дедлайна/токена для НЕ-`spawn`'нутой прямой
  блокирующей операции тела~~ — **ОТМЕНЕНО амендментом [D442](#d442)**
  (реестр 221.1 №165, 2026-08-01): владелец классифицировал это как P1-баг
  (не сознательную границу), «скобки не врут» — `cancel:`/`timeout:`
  теперь покрывают ВЕСЬ блок, включая прямую блокирующую операцию тела.
- НЕ трогает `parallel for` (`emit_parallel_for`) — его тело никогда не
  блокирует напрямую (всегда `for x in iter { spawn { body } }` +
  выделенный drain-fiber), эта форма бага структурно недостижима там.

---

## D442. `supervised(cancel:/timeout:/deadline:)` покрывает ВЕСЬ блок, включая прямую блокирующую операцию тела (Plan 221.1 №165)

> **Статус:** ✅ landed (реестр 221.1 №165+№169, P1, 2026-08-01, окно
> M:N/Vela). Retracts the "НЕ добавляет enforcement... для прямой
> блокирующей операции тела" boundary of [D439](#d439-supervised--прямая-блокирующая-операция-в-теле-без-spawn-plan-2211-162)
> (owner: «ДА», D439's own limitation classified as a bug, not a design
> choice, once measured against the real serve()-shaped repro). Amends
> [D75](#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном)/
> [D408](#d408-superviseddeadlinetimeout--областной-срок--отмена--timeouterror-plan-174)
> (structural cancel / scope deadline — extends the covered surface).

### Что

Живое подтверждение (owner-verified, nova-polaris `src/net/serve.nv`'s own
header comment, defect 3): `supervised(cancel: tok) { ... listener.accept()
... }` with `accept()` called DIRECTLY in the block's own body (no inner
`spawn`) never actually got interrupted by `tok.cancel(...)` — a 15s test
timeout elapsed with the accept loop still parked. Wrapping the SAME call in
an inner `spawn { ... }` worked. `timeout:`/`deadline:` had the identical gap
(the join-loop's own deadline gate only starts AFTER every body statement,
including a direct blocking one, has already returned).

### Правило

`cancel:`/`timeout:`/`deadline:` on `supervised` now cover the WHOLE block —
a direct (non-`spawn`) blocking statement in the body is interrupted exactly
like a registered `spawn`'d child would be, no `spawn` wrapper required.

### Механика

- `NovaFiberQueue` gains `owner_scope`/`owner_slot` — the OWNER coroutine's
  REAL ambient `(_nova_active_scope, _nova_active_slot)`, snapshotted once
  at `nova_scope_init` time (plain-value copy, no cleanup needed on any exit
  path). D439's own finding stands unchanged: that TLS pair is deliberately
  NOT repointed at the new scope during body execution — a direct blocking
  op parks under the OWNER's real identity, not under the new scope. These
  two fields are how the new scope's cancel/deadline machinery reaches it.
- `nova_sched_cancel_pending_slot(scope, slot)` (`nova_sched.h`) — a
  single-slot sibling of `nova_sched_cancel_all_pending`: invokes whatever
  op is CURRENTLY, validly registered at `(scope, slot)` via the existing
  D93 register/stop_cb protocol, exactly as if that op's OWN real scope had
  been cancelled. Safe no-op if nothing (or something unrelated, later)
  occupies the slot — same envelope as `nova_sched_wake`'s existing
  `pco == NULL` guard (spurious-wake-tolerant by construction, not new
  tolerance introduced for this).
- `nova_scope_deliver_cancel` (the `cancel:` broadcast) additionally calls
  `nova_sched_cancel_pending_slot(q->owner_scope, q->owner_slot)`.
  `nova_cancel_token_cancel_reason`'s `bound_scope` branch — previously five
  lines DUPLICATING `nova_scope_deliver_cancel`'s own body verbatim (a
  maintenance hazard: the new targeted wake was almost added to only ONE of
  the two copies) — is now a single call to `nova_scope_deliver_cancel`
  itself (byte-identical behaviour, same five actions in the same order).
- `timeout:`/`deadline:` additionally arm a **self-cleaning libuv timer at
  scope ENTRY** (`nova_scope_arm_early_deadline`/`nova_scope_disarm_early_
  deadline`), independent of `nova_supervised_run_impl`'s own (later,
  join-loop) deadline gate. Its payload (`NovaEarlyDl`) is a plain
  `(owner_scope, owner_slot)` VALUE copy on a **collectable** (`nova_alloc`,
  not `nova_alloc_uncollectable`) heap block, freed by Boehm once
  unreachable — never a pointer into the new scope's own stack frame, and
  never manually freed (a first version manually freed it in the libuv
  close_cb and crashed with a genuine use-after-free: `uv_close`'s close_cb
  runs on a LATER loop iteration than the firing `timer_cb`, so the
  unconditional disarm call right after the body could — and, reproducibly,
  did — read already-freed memory).
- The cancel-token BIND moved EARLIER — right after scope entry, before the
  body runs, instead of right before `nova_supervised_run_cancel` (i.e.
  strictly after every body statement, including the direct blocking one,
  had already returned — which is exactly why `tok.cancel()` used to see
  `bound_scope == NULL` mid-park and do nothing). The original delayed-bind
  design existed to avoid a DIFFERENT hazard (a body that throws a genuine,
  unrelated error directly would leave `tok` dangling-bound past
  `nova_supervised_run_cancel`'s own unbind, which never runs). That hazard
  is now covered by a narrow, LOCAL `NovaFailFrame` try/finally wrapped
  around JUST the body-statement-execution window: unbind (if a cancel
  token) + disarm the early-deadline timer + re-throw the same error
  (`nova_rethrow_scope`, unchanged Plan 201 mechanism) on any escaping
  throw. Gated on `cancel:` OR `timeout:/deadline:` being present — a plain
  `supervised{}` emits neither the bind nor the guard, byte-identical to
  before.
- **№169 (nested deadline, same window):** `nova_scope_init`'s own deadline
  inheritance reads the RUNTIME `_nova_active_scope` chain, which (per the
  point above) does NOT see a directly (non-`spawn`) entered enclosing
  `supervised{}` block — a LEXICALLY nested `supervised { supervised
  (timeout:) { ... } }` therefore silently lost the outer block's tightened
  deadline. Fixed by an ADDITIONAL, explicit `nova_deadline_combine` against
  the LEXICALLY enclosing scope's own `deadline_ns`, known at codegen time
  (`current_scope_queue`, still the outer value at that point) — a
  compile-time channel, independent of the runtime TLS one. No-op (reads the
  same already-correct value twice) for the pre-existing working case (a
  `spawn`'d child's own nested `supervised{}`, where `_nova_active_scope`
  already correctly equals the parent).

### Гонко-анализ (STALE-slot-класс, mn-coding-conventions)

- **Identity, not index (§5):** `nova_sched_cancel_pending_slot` acts on
  WHATEVER is validly registered at `(owner_scope, owner_slot)` at the
  moment it runs — it does not itself re-derive or cache an "expected"
  fiber pointer. This is safe ONLY because it reuses the EXISTING D93
  register/stop_cb protocol's own atomics (ACQUIRE-load of `pending_stop_cb`
  paired with the RELEASE/SEQ_CST store in `nova_sched_register_pending`) —
  no new state-machine, no new CAS point was introduced by this change; the
  new caller is simply a second call-site into machinery `nova_sched_
  cancel_all_pending` already exercises for every existing scope-cancel.
- **Residual (documented, accepted) race:** if the OWNER coroutine's body
  throws a genuine, unrelated error DIRECTLY (not via the guarded window —
  impossible, since the guard now wraps exactly that) this class is closed;
  the remaining, narrower residual is the early-deadline timer firing LATE
  (its bounded, user-specified deadline) after the SAME coroutine has since
  died and `owner_slot` was reused by an unrelated LATER fiber under
  `owner_scope` — the stray `nova_sched_cancel_pending_slot` call then
  targets that unrelated fiber's current op. Memory-safe (§5's own
  wrong-fiber-but-safe envelope: the slot's OWN atomics gate any action),
  the worst outcome is a spurious interrupt of unrelated, later work — a
  functional (liveness) edge case, not memory corruption, and only reachable
  through the disarm call already being skipped (guard's own catch-arm
  disarms on every throw path it wraps).
- **Verified NOT racy by construction:** the two new runtime functions
  (`nova_sched_cancel_pending_slot`, `nova_scope_arm_early_deadline`/
  `nova_scope_disarm_early_deadline`) never repoint `_nova_active_scope`/
  `_nova_active_slot`, never allocate a scope slot, never touch `fibers[]`/
  `parked[]` bookkeeping directly — the REJECTED "self-slot" approach
  documented in D439 (double-registration → double-resume) is structurally
  unreachable here; this design was chosen specifically to avoid
  re-discovering that dead end.

### Приёмка

`std/src/net/supervised_cancel_accept_test.nv` — `cancel:`/`timeout:` each
interrupt a direct-body `TcpListener.accept()` (no inner `spawn`), matching
the serve()-shaped repro; ~~20 back-to-back runs, no flake~~ verified via a
standalone harness outside `std/`'s own folder-module (see Границы below —
`nova test std/src/net` is blocked by an UNRELATED pre-existing compiler bug,
`[M-io-write-all-tcpstream-mono-cc-fail]`, that predates this window and is
out of its scope). `std/src/concurrency/supervised_deadline_test.nv`
(includes the №169 nested-deadline case) — 20/20 clean via `nova test`.

### Границы

- **Амендмент 2026-08-07 (реестр 221.1 №398/№224 переоткрытие, окно
  p398-cancel-direct-body):** проба владельца (2026-08-06: `Time.sleep(3000)`
  прямо в теле `supervised(cancel: tok)`, отмена через 200мс → сон досидел
  3181мс) показала, что этот же gap бьёт не только `_nova_sleep_via_driver`
  как теоретический разбор ниже предсказывал, но что и `timeout:`/`deadline:`
  под M:N (driver уже запущен — т.е. в программе БЫЛ хотя бы один `spawn`
  раньше по времени) страдали ИДЕНТИЧНО: `_nova_early_dl_timer_cb`
  (early-armed timer, см. «Механика» выше) звал ТОЛЬКО
  `nova_sched_cancel_pending_slot`, тот же самый недостаточный вызов, что и
  `nova_scope_deliver_cancel`'s `_nova_cancel_via_driver(q)` — оба смотрят
  `q`'s (пустой) `armed_sleeps_head`, а driver-armed `Time.sleep` регистрирует
  себя под `owner_scope`'s списком (см. ниже). ПРОБА ИНТЕГРАТОРА, заявившая
  «timeout: 200 → прерван за 489мс, дедлайн-путь работает» — не была
  ошибочна, но её стенд не имел ни одного `spawn` до замера: без driver'а
  `Time.sleep` идёт ЛЕГАСИ `_nova_sleep_via_libuv`-путём, который
  `nova_sched_cancel_pending_slot` УЖЕ покрывал (fix №165) — с driver'ом
  запущенным (обычный случай в реальной M:N-программе) `timeout:` был
  сломан ТАК ЖЕ, как `cancel:`.

  **Фикс:** новый driver-job `NOVA_DRV_JOB_CANCEL_SLOT` (`driver.h`/
  `driver.c`, `_nova_driver_handle_cancel_slot`) — таргетированный по
  `(scope, slot)` аналог `CANCEL_SCOPE`: walk `armed_sleeps_head` того
  scope'а, но CAS/close только запись, чей `slot` совпадает — соседние
  операции ТОГО ЖЕ (возможно внешнего, долгоживущего) owner-scope не
  трогает. Submit-обёртка `_nova_cancel_via_driver_slot` (fibers.h) —
  вызывается из ОБОИХ прежних тупиков: `nova_scope_deliver_cancel`
  (`cancel:`) сразу после существующего
  `nova_sched_cancel_pending_slot(q->owner_scope, q->owner_slot)`, и
  `_nova_early_dl_timer_cb` (`timeout:`/`deadline:`) сразу после его
  собственного такого же вызова. Lifetime-контракт зеркалит
  `_nova_cancel_via_driver`: инкремент/декремент `owner_scope->pending_
  driver_jobs` — безопасно, потому что `owner_scope` есть ПРЕДОК текущего
  стека (его собственный `nova_supervised_run_impl` либо ещё крутит тот же
  spin-wait для СВОИХ jobs, либо это root/main scope, чей фрейм жив весь
  процесс).

  Матрица приёмки (позитив+негатив, `Time.sleep`×{прямая операция,
  `spawn`}×{`cancel:`,`timeout:`,`deadline:`,комбинация}) — `docs/plans/wip/
  PROGRESS-p398.md` (сохранена как справочная запись; не переносится в
  `std/` целиком — см. приёмку ниже).

- **Остаток, НЕ закрытый этим амендментом (сузился, но не исчез):**
  `Time.sleep()` теперь корректно БУДИТСЯ рано под ОБОИМИ путями
  (`_nova_sleep_via_libuv` — уже было верно; `_nova_sleep_via_driver` —
  фикс выше), но пост-wake проверка ОБОИХ путей по-прежнему консультирует
  только `cancel_scope->cancel_requested` (OWNER-scope — НЕ `q`) с нет
  per-op `cancelled`-латча, эквивалентного `channels.h`'s `w->cancelled`
  (уже добавлен и закрывает тот же класс для `Channel.recv`/`send`/
  `select`, см. «Механика» выше). Практическое следствие: прерванный
  direct-body `sleep` возвращается управление БЕЗ throw — код,
  синтаксически идущий сразу за ним в теле, ещё исполняется (внешний
  `supervised` тем не менее корректно и в срок ловит/бросает `cancel:`/
  `TimeoutError` — прикладной наблюдатель снаружи блока не замечает
  разницы, но код МЕЖДУ прерванной операцией и концом блока успевает
  выполниться, не будучи "должен был"). Маркер сужен и остаётся открытым:
  `[M-supervised-direct-sleep-timeout-silent]` (backlog-followups.md).

---

## D446. Свойство «безопасен для одновременного использования» и требование на границах

> **Решение владельца 2026-08-04.** Расширяет [D415](#d415) (`#share`-атрибут,
> capture-check) и [D441](#d441) (модель памяти между файберами) с той стороны,
> которую они не покрывали. Повод — вопрос владельца о `ratelimit` в
> nova-polaris и адверсарная разведка плана
> [238](../../docs/plans/238-fiber-memory-model.md) Ф.7.

### Что вскрыла разведка

Энфорс до этого решения был **конечным списком распознанных форм**, а не
свойством. Проверялись однохоповые случаи, подобранные под ранее найденные
регрессии; форма ловилась, только пока совпадала с закрытым классом
буквально. Шесть путей проходили молча, включая простейший — переприязку
имени (`ro b = a`) и возврат замыкания из функции.

Модель, выведенная из грамматики: значение может сменить владельца ровно
десятью способами связывания, из них девять живые (десятый — модуль-level
глобал — закрыт [D184](02-types.md#d184)).

### Утверждение, которое язык обязан выполнять

> Значение может быть достижимо более чем из одного файбера ТОЛЬКО если оно
> неизменяемо, либо единолично принадлежит одному владельцу (передано
> забиранием), либо его тип объявлен безопасным для одновременного
> использования и это объявление ПРОВЕРЕНО.

### Как это проверяется — три части, все локальные

**1. Свойство типа.** Тип может быть объявлен безопасным для одновременного
использования (сегодняшняя роль `#share`), но объявление перестаёт быть
ручательством автора и становится проверяемым: компилятор обязан убедиться,
что все изменяющие операции типа атомарны либо защищены.

**2. Правило для замыканий — считается ПО ЗАХВАТАМ, без обхода программы.**
Замыкание безопасно тогда и только тогда, когда безопасно всё, что оно
захватило.

**3. Границы объявляют требование В ПОДПИСИ.** Функция, принимающая значение,
которое ПОЗЖЕ будет исполняться параллельно, обязана сказать это в своей
подписи. Тогда неважно, кто и когда вызовет: небезопасное не проходит через
границу.

**Ключевое свойство алгоритма: анализ достижимости НЕ НУЖЕН.** Компилятор не
выясняет, откуда позовут; каждый вход в параллельность требует свойства от
того, что принимает. Решение локально и разрешимо.

### Отношение к прежнему решению

[Плана 238](../../docs/plans/238-fiber-memory-model.md) решение владельца от
2026-07-31 отвергло маркер ПЕРЕДАЧИ значения в другой поток («Send/Migrate —
YAGNI»). D446 его не отменяет: там речь о **переносе** значения, здесь — об
**одновременном использовании**. В мировой практике это два разных понятия
(Rust `Send` против `Sync`); отвергнут был аналог первого, вводится аналог
второго.

### Цена, названная явно

Подписи функций, принимающих исполняемое-позже-параллельно значение (в нашем
корпусе — установка middleware, регистрация обработчиков), обязаны обзавестись
требованием. Это видимое изменение публичного API std и пакетов.

По принципу «ужесточать до релиза, ослаблять после» (план 221) требование
вводится ДО тега: расширить принятое потом — не ломающее изменение, ужесточить
потом — ломающее.

## D441. Модель памяти между файберами — транзитивная линейность mut-захвата + белый список (Plan 238)

> **Статус:** ✅ landed (реестр 221.1 №150, P1 звучность конкурентности,
> блокер тега v0.1, 2026-07-30/31). Расширяет [D415](#d415-data-race-freedom--share-атрибут-capture-check-consume-в-spawn-plan-1733) (`§2` capture-
> check на СИНТАКСИЧЕСКОЙ границе `spawn`/`parallel for`/`detach`) и
> [D416 §2](#d416)
> (`Supervisor.on_child_fail` — сериализация на drive-файбере). Дом:
> [Plan 238](../../docs/plans/238-fiber-memory-model.md), измерение —
> `docs/plans/wip/scale150-migration-2026-07-30.md`, репро —
> ветка `p150-race-repro` (`race150/`, `RACE150_FINDINGS.md`).
> **A-V10-амендмент (2026-07-31):** обе честные границы §5 (№167, №168)
> закрыты — см. «Amend D441 §5» в конце этого раздела.
> **S1a-амендмент (2026-08-02):** класс «замыкание как ПОЛЕ структуры»
> (реестр 221.1 №117 + №242§3) закрыт пятой точкой пересечения §3(г) —
> см. «Amend D441 §5 — S1a» в конце этого раздела.

### §0. Почему D415 первого уровня не хватило (ретроспектива)

D415 §2 поставил capture-check РОВНО на синтаксическую границу
`spawn`/`parallel for`/`detach`: свободные переменные ТЕЛА этих конструкций
резолвятся против объемлющих scope'ов, и `mut`-биндинг небезопасного (не
`#share`) типа — ошибка. Это ловит ПРЯМОЙ захват. Владелец обобщил дыру ещё
в записи №150 (2026-07-28): «как и вызов любой функции из файбера, вызов
клоужры потоконебезопасен — компилятор ловит только обращения из `spawn` к
внешним переменным, а далее любая функция может иметь такой доступ».
Конкретно нашлись ДВА живых класса дыры, ни один из которых — обращение
ВНУТРИ синтаксической границы:

1. **Транзитивная передача значения.** Замыкание с `mut`-захватом,
   созданное СНАРУЖИ `spawn`, передаётся ВНУТРЬ границы как ДАННЫЕ —
   параметром функции, которая сама зовёт этот параметр из `spawn`
   (`race150/b_closure_bypass.nv`: `parallel_spawn(f fn()->(), ...) {
   supervised { spawn { f() } } }`, вызванная как `parallel_spawn(push, ...)`
   где `push = || { v.push(1) }`). Захват `v` внутри `push` НИКОГДА не
   пересекал синтаксическую границу `spawn` буквально — граница видит
   только имя `f`, не то, что оно закрывает.
2. **Обработчик как скрытая точка исполнения в файбере.** `with Fail[T] =
   |e| { mut_var = ... } { ...spawn... }` — обработчик формально стоит
   СНАРУЖИ `spawn`, синтаксически не внутри тела. Но измерение Ф.0 (см. §2
   ниже) доказало: обработчик исполняется В ФАЙБЕРЕ падающей операции, а не
   в файбере устанавливающего `with`. Мутация в обработчике — та же гонка,
   что и прямой захват, только D415 её не видел вовсе (capture-check не
   спускался в `handler`-выражение `with`-биндинга).

Оба класса — ОДНА и та же машина (D131 линейность / consume), просто на
НОВЫХ точках пересечения границы, которые D415 §2 не сканировал.

### §1. Измерение (Ф.0, факты, не гипотеза)

- **Обход границы (класс 1) — репро B, `race150/b_closure_bypass.nv`:**
  `nova check` молчал (0 errors). Рантайм: **60 прогонов из 60 — грязные**
  (0 чистых). Серия N=8×M=10000 (20 прогонов×2): 32 неверных `len()` + 8
  крашей / 40. Серия N=16×M=100000 (нагрузка, 20 прогонов): 2 неверных + 18
  крашей / 20 — под нагрузкой гонка деградирует в 90% крашей. Контроль
  (`race150/c_atomic_control.nv`, тот же код с `AtomicInt` вместо `Vec`):
  **20/20 чисто** — доказывает, что дело именно в отсутствии синхронизации,
  не в баге рантайма.
- **Обработчик как точка исполнения (класс 2):** 64 падающих ребёнка × 20
  раундов, 5 таких батчей; обработчик `with Fail[int]` делает неатомарный
  `cnt = cnt + 1` рядом с `AtomicInt`-контролем числа вызовов —
  **потерянные обновления в 2 батчах из 5** (2 и 1 потерянных апдейта на
  1280 вызовов в батче). Обработчик исполняется В ФАЙБЕРЕ падающего
  ребёнка, конкурентно с siblings — малая частота потерь + запись
  одинакового значения (`= true`) в 26 из 28 живых сайтов объясняют, почему
  идиом «казался рабочим» на практике.
- **`Supervisor.on_child_fail` — D416§2 обратное измерение, затем ОПРОВЕРЖЕНИЕ
  (приёмка A-V10 2026-07-31):** та же методика; в прогонах окон 238/A-V10
  счётчик был точен (0 потерь), НО приёмочный мега-CU дал RUN-FAIL, и
  изолированный перезамер показал **5 падений из 10** — гарантия D416§2
  рантаймом НЕ выполняется, ранним прогонам везло. Реестр №173 (P1, M:N-окно
  с №165/№169); репро запарковано в docs/plans/wip/d416-serialization-repro/.
  Урок: пин-гарантия должна перемеряться приёмкой, а не одним прогоном окна.
  **Уточнение диагноза (M:N-окно №165/№169/№173, 2026-08-01):** первоначальный
  диагноз («неатомарный счётчик теряет обновления» = конкурентный вызов
  `on_child_fail`) ОПРОВЕРГНУТ прямым измерением — инструментированная проба
  (`in_handler`-реентрантный счётчик на `AtomicInt`, busy-spin внутри
  хендлера вместо запрещённого `sleep`, 10 изолированных прогонов ×
  64 падения × 20 раундов) показала **`race_detected == 0` ВО ВСЕХ 10
  прогонах, включая 4 «упавших»** — `nova_supervised_process_decisions`
  (fibers.h) никогда не вызывается конкурентно для одного scope; это
  подтверждает чтение кода (drive-thread-only `_deciding`-latch, `_h->
  on_child_fail(...)` — обычный синхронный C-вызов, БЕЗ переключения
  корутины). **Настоящий баг — другого класса: ПОТЕРЯННОЕ падение ребёнка**
  (`total_calls` короче ожидаемых `n×rounds` на 1-2 в ~40% изолированных
  прогонов) — какой-то `throw` ребёнка НИКОГДА не доходит до decision-loop
  вовсе (не «два потока одновременно инкрементируют», а «одно падение
  потеряно ДО хендлера») — по мн-conventions §1/§2 классу (occupancy/
  `child_count`-видимость), НЕ по классу конкурентного вызова хендлера.
  Корень НЕ локализован в рамках этого окна (не найдено безопасного способа
  добавить трипваер в 173.0-субстрат — R1/R2-защищённый код — за оставшееся
  время); **РЕШЕНИЕ ОКНА: STOP, аргументированный** — carve-out НЕ
  возвращается (потерянное падение делает `on_child_fail` ненадёжным
  НЕЗАВИСИМО от вопроса сериализации), но и «неатомарный счётчик»-формулировка
  диагноза больше не верна — см. правку ниже. Пин-фикстура окна 238 (`docs/
  plans/wip/d416-serialization-repro/`) БОЛЬШЕ НЕ КОМПИЛИРУЕТСЯ (энфорс D441
  §3, ожидаемо — carve-out отозван) — репро для будущего окна:
  `_tmp/probe173/main.nv`-форма этого окна (Atomic-only, checker-чистая,
  не в реестре).

### §2. Правила (решение владельца, №150)

Три кирпича, БЕЗ нового языкового механизма (Send/Sync-аналог отклонён как
YAGNI для v1 — размер механизма не оправдан текущим масштабом):

1. **`ro`/`mut` — неизменяемое разделяемо по построению.** Читатели без
   писателей безопасны; это НЕ меняется этим окном.
2. **Линейность для `mut` — единый файбер.** Замыкание/значение с
   `mut`-захватом — линейный ресурс ОДНОГО файбера; пересечение границы
   файбера (§3 ниже — все точки пересечения, не только синтаксический
   `spawn`-блок) — ошибка той же машины D131/consume.
3. **Белый список синхронизированных, разделяемых ТРАНЗИТИВНО по составу**
   (уже реализовано `share_check.rs`/D415, этим окном НЕ менялось —
   инфраструктура уже была готова): share-типы (D415 `#share`-vouch),
   `Atomic*`, `Mutex`, концы каналов (`Sender`/`Receiver`). Запись разделяема
   ⇔ каждое поле — из (1) или из этого списка; ходьба по составу —
   `protocols::share_check::{is_mut_alias_safe, mut_alias_failure}`
   (существующий, уже транзитивный по `TypeDeclKind::Record/NamedTuple/Sum/
   Newtype/Alias` до этого окна — фикстуры `f2_pos_record_atomic.nv`/
   `f2_neg_record_bare_mut.nv` это лишь ПОДТВЕРДИЛИ, кода не понадобилось).

   Оговорка звучности: «`ro` ⇒ разделяемо» держится ТОЛЬКО вместе с (2) —
   запрет второго `mut`-алиаса гарантирует отсутствие писателя за спинами
   читателей. Есть ОДНО структурное исключение из «`ro` ⇒ безопасно»: тип
   `fn`/closure. `ro`-захваченное ЗНАЧЕНИЕ замыкания не «глубоко
   неизменяемо» — вызов замыкания может мутировать состояние, на которое оно
   закрылось, ГДЕ БЫ оно ни было объявлено. `share_check::share_rec`
   отказывает `TypeRef::Func` безусловно (`ShareReason::FnType`) — вот
   почему §3 п.(а)/(б) ниже смотрит НЕ на `ro`/`mut` самого захваченного
   имени, а на то, ЧТО оно закрывает.

### §3. Три точки закрытия транзитивного пути (реализация Ф.1-Ф.3)

**(а) Параметр, ведущий в `spawn`/`detach`/`parallel for`/`blocking`.**
Пре-пасс `spawn_tainted_params_of_fn` (по модулю, один раз): для каждой
именованной `fn` — какие из её СОБСТВЕННЫХ `fn`-типизированных параметров
вызываются внутри `spawn`/`detach`/`parallel for`/`blocking`-тела ГДЕ-ТО в
её определении (любая глубина вложенности). На КАЖДОМ вызове такой `fn`
(`check_transitive_closure_arg`) — если аргумент на этой позиции резолвится
к замыканию (буквальный литерал `|..| {..}` на месте вызова, ИЛИ простое
имя, снэпшот-зарегистрированное в `ScopeBinding.closure_free_vars` в
момент его `let`/`ro`-объявления), его свободные переменные проверяются
той же машиной share/линейности, что и прямой захват — на span'е
АРГУМЕНТА, с текстом «создано снаружи границы, передано параметром {p} в
вызов {f}, которая зовёт {p} внутри своего spawn/detach/parallel for».
Закрывает repro B буква-в-букву.

**(б) Отправка в канал.** `chan.send(value)` (имя-эвристика, тот же
паттерн, что уже использует `state.const_init`-гейт для `send`/`recv` в
этом же файле) — `value`, если резолвится к замыканию тем же путём, что
(а), проверяется идентично. Живых нарушителей класса (б) на момент замера
№150 не найдено (аудит scale150), но механизм универсален с (а).

**(в) Обработчик вокруг spawn-содержащего тела.** `with X = handler {
body }`: если `body` содержит (транзитивно, любая глубина) `spawn`/
`detach`/`parallel for`/`blocking` (`block_contains_fiber_boundary`) —
свободные переменные `handler`-выражения (замыкание-литерал ИЛИ методы
`effect X { ... }`-литерала) проверяются той же машиной. Диагностика —
НОВЫЙ код `E_HANDLER_MUT_CAPTURE_IN_FIBER` (текст объясняет: обработчик
исполняется В ФАЙБЕРЕ падающей операции, не устанавливающего scope'а, со
ссылкой на измерение Ф.0 §1). **Исключение (D416§2) ОТОЗВАНО (2026-07-31, №173):** пин-фикстура
сериализации упала 5/10 изолированных прогонов — гарантия D416§2 рантаймом
не выполняется; обработчики `Supervisor` проверяются как все остальные,
пока рантайм не начнёт реально сериализовать (после фикса №173 carve-out
можно вернуть вместе с возвратом пин-фикстуры в conformance).

### §4. Диагностики

| Код | Класс | Где ловит |
|---|---|---|
| `E_CONCURRENT_MUT_CAPTURE` | `mut`-небезопасный тип, прямой ИЛИ транзитивный (§3 а/б) захват | spawn/detach/parallel-for тело (D415, было); аргумент вызова в spawn-tainted параметр; значение, отправленное в канал (оба — новые, Plan 238) |
| `E_LINEAR_CAPTURE_IN_FIBER` | linear/consume-тип, та же транзитивность | как выше (D415 §4, расширено на транзитивные точки) |
| `E_HANDLER_MUT_CAPTURE_IN_FIBER` | `mut`-небезопасный тип, захвачен `with`-обработчиком вокруг fiber-содержащего тела | НОВЫЙ (Plan 238 Ф.3) — единственный класс, реально населённый живыми нарушителями (28 сайтов) |

### §5. Честные границы (НЕ городить сверх измеренного)

- **Точка (4) плана (именованный граф вызовов / `infer_effects`) — ЗАКРЫТО
  A-V10 (2026-07-31), реестр 221.1 №167.** Ф.3 (Plan 238) проверила:
  существующий `infer_effects` поднимает ТОЛЬКО объявленные эффекты
  сигнатуры, у него нет понятия «M:N-safe/unsafe leaf» — план предполагал
  НОВУЮ эффект-категорию, отдельное окно. A-V10 добавила её узко и честно,
  БЕЗ нового языкового эффекта: атрибут `#thread_affine` на `extern fn`
  (тот же parser-путь, что `#blocking` — `E_THREAD_AFFINE_NOT_EXTERN`,
  если не на `extern`) маркирует именно М:N-небезопасный лист. Транзитивный
  подъём — `thread_affine_closure` (`compiler-codegen/src/types/mod.rs`):
  fixed-point worklist над ИМЕНОВАННЫМ графом вызовов свободных fn
  (`own_fiber_call_names_of_fn`), а не через `infer_effects`/№131 (та
  машина принципиально НЕ транзитивна — по её собственному doc comment —
  и оперирует объявленными эффектами сигнатуры, не именами) и не через
  `#pure`-подъём (атрибут декларируется вручную, не выводится). Граница —
  `E_THREAD_AFFINE_IN_FIBER`: вызов листа или транзитивного носителя внутри
  тела `spawn`/`detach`/`parallel for` (та же `block_contains_fiber_
  boundary`-класса обхода, что и остальной D441 §3), с именем листа и
  цепочкой подъёма (минимум один промежуточный фрейм) в тексте
  диагностики. Ключевое структурное решение: граф ОСТАНАВЛИВАЕТСЯ на
  вложенных `spawn`/`detach`/`parallel for`/`blocking` под-телах — вызов
  листа ВНУТРИ собственного `spawn` обёртки НЕ делает саму обёртку
  «транзитивно небезопасной» (тот `spawn` заводит СВЕЖИЙ, развязанный от
  вызывающего файбер; вложенный вызов уже ловится там же, как прямое
  нарушение). `std` сегодня не маркирует НИ ОДНОГО живого extern
  (libuv fiber-aware) — механизм проверен тестовым extern в фикстуре
  (`neg/thread_affine_direct_in_fiber_neg.nv`,
  `neg/thread_affine_transitive_in_fiber_neg.nv`,
  `handler_thread_affine_pos.nv`); живая маркировка std-extern'ов —
  отдельное решение. **Остающаяся узкая граница:** граф именованный —
  вызов листа ЧЕРЕЗ closure-значение (параметр/поле, переданное как
  данные, не вызванное по имени напрямую) не отслеживается, тот же класс
  лимита, что у §3(а)/(в) ниже; методы (`obj.method()`) вне периметра —
  только свободные fn по `Ident`-имени.
- **Точка (6) плана (гейт на коэрсии fn→Handler, D52 стирание) — ЗАКРЫТО
  A-V10 (2026-07-31), реестр 221.1 №168.** §3(в) изначально ловил
  handler-ЛИТЕРАЛ, написанный ПРЯМО на месте `with X = …` (100% реальных
  28 сайтов на дату Ф.3), но не `with X = some_precomputed_handler` (имя,
  не литерал). A-V10 закрыла это МИНИМАЛЬНО, на месте — `check_handler_
  capture` резолвит bare-`Ident` handler-выражение через ТУ ЖЕ снэпшот-
  машину, что Ф.1 уже использует для transitive-параметров
  (`ScopeBinding.closure_free_vars`, записан на `let`/`ro`-time
  регистрации), без нового кода обхода. Тот же E-код
  `E_HANDLER_MUT_CAPTURE_IN_FIBER`, текст диагностики дополнен пометкой
  «PRECOMPUTED... обработчик, вычисленный заранее». Carve-out `Supervisor`
  (D416§2) на момент окна сохранялся; ОТОЗВАН приёмкой того же дня (№173 —
  пин сериализации опровергнут перезамером, см. правку §3 выше). **Транзитивная установка ЗАКРЫТА
  ПОЛНОСТЬЮ, не узкой строкой:** проверено, что Ф.1а-механизм
  (`spawn_tainted_params_of_fn`/`check_transitive_closure_arg`) НЕ покрывал
  install-позицию (handler-параметр, переданный в функцию, которая сама
  ставит `with` вокруг `spawn`) — `collect_fiber_boundary_frees_expr`'s
  `With`-ветка не трактовала handler-выражение install-сайта как
  boundary-use параметра. Закрыто расширением ТОГО ЖЕ пре-пасса: когда
  `with`-тело содержит fiber-границу (`block_contains_fiber_boundary`,
  тот же гейт, что уже использует сам handler-check), свободные переменные
  handler-выражения install-сайта добавляются в тот же boundary-set, что и
  прямой вызов параметра внутри `spawn` — `spawn_tainted_params_of_fn`
  подхватывает install-позицию БЕЗ отдельного нового пре-пасса. Измерено
  фикстурой `neg/handler_mut_capture_precomputed_install_transitive_neg.nv`
  (функция-параметр `h`, установленная как `with`-handler вокруг тела со
  `spawn`, ловится на call-сайте — `E_CONCURRENT_MUT_CAPTURE`, тот же путь,
  что `race150/b_closure_bypass.nv`).
- **Класс «замыкание как ПОЛЕ структуры»** (`BackgroundTasks.tasks
  []fn()->()`, `ServerResponse.upgrade`/`.background` и т.п.) — **ЗАКРЫТО
  S1a (2026-08-02), реестр 221.1 №117 + №242§3** — см. «Amend D441 §5 —
  S1a» в конце этого раздела (§3 получила пятую точку (г)).
- Ложные срабатывания на легальных паттернах (per-fiber аккумулятор
  внутри собственного `spawn`, `Sender`-отправка, `AtomicInt`) —
  ПРОВЕРЕНЫ отсутствующими: полный прогон `spec_tests/conformance`
  директорией (после миграции Ф.4) даёт диагностики нового класса ТОЛЬКО
  в уже известных `neg/`-фикстурах D415; ни одного нового ложного
  срабатывания на позитивных путях.

### Миграция (Ф.4)

26 бесспорных живых сайтов (замер scale150) + 2 дополнительных, найденных
точным AST-энфорсом там, где грепом-замером они были пропущены
(`m2217_15b_cancel_consume_shield_narrowed.nv`,
`d432_auto_cleanup_hybrid_c.nv`, `parfor_iter_edge.nv`,
`scope_multierror_test.nv` — 4 файла, не 2, если считать по файлам, а не
по записи scale150-документа) — все мигрированы `AtomicBool`/`AtomicInt`
вместо `mut`-флага, тот же наблюдаемый исход (проверено ассертами тестов,
НЕ ослаблены). `nova-polaris` (другая репа) — 1 сайт (`net/policy.nv:58`
`read_attempt`) остаётся немигрированным, вне периметра этого окна.

## Amend D441 §5 — A-V10: №168 (precomputed-обработчики) и №167
(M:N-небезопасный лист) закрыты (реестр 221.1, 2026-07-31)

> **Статус:** ✅ landed. Обе честные границы §5 «НЕ городить сверх
> измеренного» из блокеров тега (владелец «ДА» 2026-07-31, реестр 221.1
> №167/№168) закрыты одним окном, sonnet. §5 выше отредактирован на месте
> (bullet'ы «Точка (4)»/«Точка (6)» заменены описанием закрытия) — этот
> раздел суммирует изменение и фиксирует, что осталось честно открытым.

**№168 (precomputed-обработчики).** `check_handler_capture` в
`compiler-codegen/src/types/mod.rs` получил ветку `ExprKind::Ident(name)`:
резолвит handler-имя через `ScopeBinding.closure_free_vars` (та же
снэпшот-машина, что Ф.1 уже использует для spawn-tainted параметров —
никакого нового обхода). Тот же `E_HANDLER_MUT_CAPTURE_IN_FIBER`, текст
дополнен пометкой про precomputed-форму. Транзитивная УСТАНОВКА (handler
как параметр функции, которая сама ставит `with` вокруг `spawn`) —
проверено и закрыто ПОЛНОСТЬЮ (не узкой строкой): `collect_fiber_boundary_
frees_expr`'s `With`-ветка теперь трактует handler-выражение install-сайта
как boundary-use параметра ТОЧНО ТАК ЖЕ, как прямой вызов параметра внутри
`spawn` — существующий `spawn_tainted_params_of_fn`/`check_transitive_
closure_arg` подхватывают install-позицию без отдельного пре-пасса.

**№167 (M:N-небезопасный лист).** Новый атрибут `#thread_affine` на
`extern fn` (парсер: тот же путь, что `#blocking`, `E_THREAD_AFFINE_
NOT_EXTERN` если не на `extern`). Транзитивный подъём — `thread_affine_
closure` (новый, честно НЕ переиспользованный код: ни `infer_effects`/№131
(принципиально не транзитивна — её собственный doc comment), ни `#pure`
(вручную декларируемый атрибут, не выводимый) не были литеральным
транзитивным графом вызовов; структурно ближайший прецедент — `spawn_
tainted_params_of_fn`'s форма «once-computed pre-pass, keyed by fn name»,
воспроизведена для этого нового графа) — fixed-point worklist над
именованным графом свободных fn (`own_fiber_call_names_of_fn`), который
ОСТАНАВЛИВАЕТСЯ на вложенных `spawn`/`detach`/`parallel for`/`blocking`
под-телах (развязанный от вызывающего файбер — обёртка не наследует
небезопасность листа, вызванного ВНУТРИ её собственного `spawn`; проверено
вручную — единственная ошибка остаётся на месте вложенного `spawn`, без
второй/лишней у вызывающего). Граница — `E_THREAD_AFFINE_IN_FIBER` на
`spawn`/`detach`/`parallel for` (та же `block_contains_fiber_boundary`-
класса обхода, что и остальной D441 §3), с именем листа + цепочкой подъёма
(минимум один промежуточный фрейм) в тексте. `std` сегодня не маркирует ни
одного extern — механизм проверен тестовым `extern "C" fn abs` в
фикстурах; живые кандидаты (libuv fiber-aware C-вызовы) не выявлены при
этом окне — маркировка живых std-extern'ов остаётся отдельным решением.

**Осталось честно открытым (не молчаливый пробел):**
- №167: граф именованный — thread-affine лист, вызванный ЧЕРЕЗ closure-
  значение (параметр/поле, переданное как данные, а не вызванное по
  прямому имени), не отслеживается; методы (`obj.method()`) вне периметра.
- №168: `with`-handler в install-позиции resolvится через СНЭПШОТ, взятый
  на call-сайте передачи ПАРАМЕТРА; двух-и-более-хоповая передача
  (параметр передан ДАЛЬШЕ в третью функцию, которая уже ставит `with`) —
  тот же принципиальный лимит глубины, что у §3(а)'s исходного «2+ хопа не
  отслеживаются».
- Класс «замыкание как ПОЛЕ структуры» (§5 выше) НЕ был тронут этим окном
  (A-V10) — закрыт ПОЗЖЕ, окном S1a (2026-08-02) — см. следующий раздел.

Фикстуры: `neg/handler_mut_capture_precomputed_neg.nv`,
`neg/handler_mut_capture_precomputed_install_transitive_neg.nv`,
`handler_mut_capture_precomputed_pos.nv`,
`neg/thread_affine_direct_in_fiber_neg.nv`,
`neg/thread_affine_transitive_in_fiber_neg.nv`,
`handler_thread_affine_pos.nv` (все — `spec_tests/conformance/`).

## Amend D441 §5 — S1a: №117 + №242§3 закрыты — §3 получила точку (г)
«замыкание как ПОЛЕ структуры» (реестр 221.1, 2026-08-02)

> **Статус:** ✅ landed. Последняя из честных границ §5, оставленных A-V10
> («Класс «замыкание как ПОЛЕ структуры» … не тронут этим окном»),
> закрыта отдельным окном (S1a, sonnet) по личному контролю владельца
> (критерий: «частичное закрытие M:N-безопасности = невыполнение»).
> §3 выше отредактирована на месте (bullet «Класс «замыкание как ПОЛЕ
> структуры»» заменён ссылкой сюда); этот раздел — реализация и остаток.

**Механизм — пятая точка пересечения §3(г).** Симметрична (а)/(б)/(в):
замыкание с `mut`-захватом, ЗАПИСАННОЕ в поле записи/коллекции (вместо
прямой передачи параметром/каналом/handler'ом), чьё значение ВЫЗЫВАЕТСЯ
ГДЕ-ТО ещё внутри `spawn`/`detach`/`parallel for`/`blocking`-тела —
проверяется В МОМЕНТ ЗАПИСИ той же машиной (`flag_boundary_captures`),
что и остальные четыре точки. Два пре-пасса (`compiler-codegen/src/types/
mod.rs`, тот же файл/тайминг, что `spawn_tainted_params`/`thread_affine_
closure`):

1. `spawn_tainted_fields_of_module` — по всем методам (`Item::Fn` с
   `receiver`) находит `(TypeName, field)`-пары, чьё значение вызывается
   внутри границы: прямой `@field()`, ИЛИ через `for`/`parallel for`-петлю
   по прямому `@field`-чтению (петля/тело зовёт свою pattern-переменную
   внутри вложенной границы — `BackgroundTasks.drain()` форма), ИЛИ через
   `let`-биндинг того же поля (`ro x = @field`, вызванный дальше в том же
   блоке внутри границы — добавлено ПОСЛЕ находки codegen-гэпа ниже:
   единственная форма, которая реально СОБИРАЕТСЯ для случая bare-fn-поля).
2. `spawn_tainted_method_params_of_module` — один хоп ДАЛЬШЕ (НЕ
   fixed-point): какие СОБСТВЕННЫЕ параметры метод сам пишет в уже-
   тэйнтованное поле (`fn BackgroundTasks mut @add(f fn()->()) { @tasks.
   push(f) }` — параметр `f` тэйнтован для `(BackgroundTasks, "add")`) —
   закрывает буквальную форму вопроса владельца 2026-07-25
   (`bg.add(|| { log.push(...) } )`), которую (1) сама по себе не видит
   (значение поступает в `add` ПАРАМЕТРОМ, не литералом).

Гейт на WRITE-сайтах (три формы, все ведут к `flag_boundary_captures`):
`.push`/`.append`/`.add`/`.insert`/`.push_back`/`.push_front`/`.set`-
мутатор на тэйнтованном поле; присваивание `@field = v`/`obj.field = v`;
конструктор `Type { field: v, .. }`. Владелец поля резолвится через
`@field` (новое `CapState.current_receiver_type`, заполняется в
`walk_fn_body` из `FnDecl.receiver`) или через `ScopeBinding.ty` bare-
`Ident`-ресивера. Метод-call-сайт для пре-пасса (2) — новая ветка в
`ExprKind::Call` (twin уже существующей ветки для free-fn `Ident`-
вызовов): резолвит тип ресивера, матчит `self.sig.method_overloads` по
арности (D84-конвенция — та же, что уже применяется к free-fn overload'ам).

**№242§3 (escaping вообще).** Router-регистрация (`router.get(path,
handler)` ≡ `@routes.push((path, handler))`) — тот же write-сайт-класс.
Ответ на «как отличить убегающее от локального» (риск ложняков на
map/filter): escaping ⇔ значение ЗАПИСАНО в тэйнтованное поле; локальное
(map/filter/fold) ⇔ никогда не пишется в storage — течёт прямо в
синхронный вызов внутри тела HOF, которое само НЕ является границей. Ни
один пре-пасс не может пометить поле как тэйнтованное через синхронный
HOF-путь — структурная невозможность ложняка, не эвристика.

**Находка (codegen, НЕ чекер, filed НЕ fixed — channel-first):** прямой
вызов `@field()` bare fn-типизированного поля ВНУТРИ `spawn{}`-тела метода
— pre-existing emit_c CC-FAIL (`undeclared identifier 'nova_self'`,
спавненный closure не захватывает receiver в этом пути); родственный,
Vec[fn()->()]-поле через `for`-петлю — linker-ошибка `undefined symbol:
nova_fn_t` (родня уже известного `[M-vec-iter-fn-newtype-next-option-
mismatch]`). Оба — впервые вскрыты этим окном (шаблон «self-field вызван
внутри spawn метода», видимо, не встречался в корпусе раньше).
`[M-spawn-self-field-call-nova-self-undeclared]` в `backlog-followups.md`;
НЕ фикшено (легаси emit_c). Обходной путь — читать поле в `ro`-локаль
ДО `spawn` (`ro handler = @field; spawn { handler() }`) — codegen-safe
(обычный closure-capture-в-spawn, уже повсеместный в корпусе) И
ПОДХВАТЫВАЕТСЯ пре-пассом (1) наравне с for-петлёй — не костыль, а
идиоматичная форма.

**Осталось честно открытым (не молчаливый пробел):**
- Один хоп для метод-параметра (2-hop write→call); параметр, переданный
  ДАЛЬШЕ в третий метод — не отслеживается (тот же класс лимита, что уже
  принят для §3(а)/(в)).
- НЕ транзитивно между МЕТОДАМИ одного типа: поле, тэйнтованное вызовом
  ЧЕРЕЗ ДРУГОЙ метод того же типа (не напрямую внутри границы) — не
  подхватывается (симметрично тому, что `spawn_tainted_params_of_fn`
  тоже не транзитивен через цепочку вызовов разных fn).
- Ресивер неизвестного статического типа (generic-параметр/unresolved) —
  консервативный no-op.
- Возврат замыкания КАК РЕЗУЛЬТАТА функции (не через поле) — вне
  периметра V1 (не встречено как живой сайт при аудите фикстур этого окна).

Фикстуры (`spec_tests/conformance/`): `neg/field_sink_mut_capture_bare_
field_neg.nv` (bare fn-поле, двух-хоповая цепочка через wrapper-метод),
`neg/field_sink_mut_capture_background_tasks_neg.nv` (BackgroundTasks
дословно, `Vec[fn()->()]`-поле), `field_sink_mut_capture_pos.nv`
(#share-захват AtomicInt легален, map/filter НЕ ложнякует, Mutex-
SharedLog `#share`-vouch value-record легален).

Гейты: `nova check std/src` — 147/26/60 (байт-в-байт канон, без сдвига);
`nova-polaris` `nova test src --strict-effects` — 37/0/18 (байт-в-байт
канон); `cargo build`/`--release` — чисто.
