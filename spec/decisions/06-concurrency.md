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
[+ опц. `cancel:`], `select`, `parallel for`, `detach`, `blocking`),
`race`/`with_timeout` — stdlib поверх них, см. [D50](#d50), [D75](#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном).

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

// with_timeout — bound на время выполнения
with_timeout(2.seconds()) {
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
   Trio/structured-concurrency RFC — `parallel for`, `race`,
   `supervised(cancel:)` — часть языка. Это значительно безопаснее
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

> ⚠️ **REVISED → [D62](04-effects.md#d62), [D64](04-effects.md#d64).**
> Исходный D50 трактовал `Async` как эффект и упоминал «единый эффект
> `Async`». После D62 `Async` — ambient runtime-инфраструктура, не
> эффект. `Par` тоже не существует. Гарантия не-приостановки даётся
> блоком [`realtime`](04-effects.md#d64). `Detach`/`Blocking`
> остаются эффектами — у них есть видимый side-effect для caller'а
> (fire-and-forget семантика и блокировка ОС-потока соответственно),
> что делает их кандидатами на type-level декларацию.
>
> 📋 **PARTIALLY IMPLEMENTED IN [D71](#d71-bootstrap-concurrency-runtime).**
> Bootstrap'ом реализованы: `supervised`, `parallel for`, `detach`
> (Plan 83.4.5.2 Ф.4 amend — **default AsyncDetach** через
> `nova_runtime_spawn_orphan` primitive: armed runtime → worker pool
> fire-and-forget; bootstrap cooperative → global orphan scope drained
> on atexit либо через `runtime.drain_orphans()` explicit-sync API),
> `Time.sleep` как yield-point, `blocking { }` (Plan 83.3 — libuv-
> threadpool offload, см. §4 «Реализация»). Capture-by-value для
> immutable scalars. Не реализованы: `race`, `select`, `cancel_scope`,
> `with_timeout`, эффект `Detach` в effect-system (всё ещё runtime-
> primitive), cancellation/error-propagation между fibers (для
> non-orphan). Orphan errors → LogAndDrop в caller's stderr,
> non-propagate.

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

Suspension fiber'а **не пишется в сигнатуре**. `parallel for`, `race`,
`select` — синтаксические примитивы языка (D14), они работают на
уровне fiber-runtime'а, не type-system'ы.

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

Пример mut-захватов:
```nova
mut a = 0
mut b = 0
supervised {
    spawn { a = compute_a() }       // results через shared mut
    spawn { b = compute_b() }
}
use_both(a, b)
```

`spawn()` с пустыми скобками — **запрещено**: скобки не несут смысла и
создают иллюзию вызова функции. Подробно — [D43](03-syntax.md#d43).

`spawn` **запрещён** вне structured-блока. Допустимые скоупы:
`supervised` (в т.ч. `supervised(cancel:)`), `parallel for`, `select`;
а также stdlib-функции, построенные на них (`race`, `with_timeout`),
внутри своих тел. Вне такого скоупа `spawn foo()` — ошибка компиляции.

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
  statement; результаты через `mut`-захваты или `parallel for`
  (массив-результат).
- **`Handle[T]` / future-объект от spawn.** Aналог Rust `JoinHandle`
  или Kotlin `Deferred`. Отвергнуто: добавляет тип в систему,
  требует `.value()` синтаксиса (то же что implicit-await но явно
  в коде), не даёт ничего сверх mut-захватов.

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
- `Mutex`/`Atomic` — stdlib (D167-D173), не prelude; owner-actor pattern
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

**`supervised { body }` возвращает unit** (bootstrap, 2026-05-06). Trailing
expression body не пробрасывается caller'у — отбрасывается как `(void)`.
Это согласовано с «spawn возвращает unit» (см. п. 2): результаты от
concurrent-выполнения берутся через mut-захваты, не через возвращаемое
значение блока.

**`parallel for x in iter { body }`** — по spec D14 это **expression**
типа `[]T` (где `T` — тип `body`). Spec-семантика: parallel-fan-out
с собранными в порядке итерации результатами. Это **map**, не loop.

```nova
ro responses []Response = parallel for url in urls { fetch(url) }
//                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
//                          параллельный map: 1 element → 1 response
```

**Bootstrap-codegen (2026-05-06):** array-mode реализован.
Когда body имеет trailing-expression — форма возвращает `NovaArray_T*`
(T ∈ {nova_int, nova_bool, nova_f64, nova_str}); каждый fiber пишет
результат в `result.data[idx]` по своему индексу — порядок результатов
соответствует порядку `iter` независимо от порядка планирования fiber'ов.
Без trailing — старая semantic (statement, unit). Поддержанные
итераторы: `a..b`, `a..=b`, array literal. Spread в array literal
не поддержан (degrade to unit). См. `nova_tests/concurrency/parallel_for_array.nv`.

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

// (3) mut-захваты — гетерогенная параллельность.
mut a = 0
mut b = 0
supervised {
    spawn { a = compute_a() }
    spawn { b = compute_b() }
}
use_both(a, b)
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
  `parallel for` / mut-захваты.

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
- Эффект `Detach` **не объявлен** в effect-system. Compile-time проверка
  требования эффекта в сигнатуре не выполняется.
- Глобальный supervisor (для реального async-execution на отдельном OS-thread'е)
  — отложен до production-runtime.
- Panic-containment (`LogAndDrop`) — отложен.

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
4. **Closed channel** → `rx.recv()` возвращает `None` немедленно; arm
   считается ready. Матчится `None`-паттерном.
5. **Без default** и все каналы закрыты — panic "select: all channels closed".

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

#### `runtime.sync` — stdlib, не prelude (D167-D173)

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
// см. D173 decision tree — «когда что выбрать»
```

Детальное описание всех sync-примитивов:
- D167 — Memory ordering & `fence()` API
- D168 — Atomic типы (12 sized × 13 ops)
- D169 — `Mutex` / `RwLock` / `ReentrantMutex`
- D170 — `Semaphore` / `Barrier` / `CountDownLatch` / `Condvar`
- D171 — `Once` / `OnceCell` / `Lazy`
- D172 — `realtime { }` / `blocking { }` interaction matrix
- D173 — AI-first guidance: decision tree + canonical patterns

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
  не prelude (D167-D173).

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

### Правило

#### API — типы

```nova
// Writer capability:
type ChanWriter[T] protocol {
    send(v T) -> bool                             // true если послал; false если канал закрыт
    try_send(v T) -> bool                         // true если послал, false если полон или закрыт
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
- **Plan 83.12 `std/net`** ✅: `TcpStream.read_bytes` → `uv_read_start` + wake из `_tcp_read_cb`. See [D173](08-runtime.md#d173-stdnet--async-tcpudp-socket-stdlib-via-libuv).
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
- ✅ **std/net IO** (Plan 83.12) — ASYNC stop_cb, реализован [D173](08-runtime.md#d173-stdnet--async-tcpudp-socket-stdlib-via-libuv).
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

**Правило 6 — `_nova_active_slot = -1` означает main-flow.** Slot −1
не индексирует fiber-array (там `count >= 0` fiber'ов). Park/wake API
не работает с slot −1 (main-flow не может park'нуться через mco_yield —
нет coroutine'ы). Top-level `Time.sleep` остаётся через busy-yield
(supervised_step) либо через native sleep, не park-on-uv_timer.

**Правило 7 (future, не реализуется в Plan 22):** SIGINT/Ctrl+C через
`uv_signal_t` отменяет main-scope cancel-token, fiber'ы получают
cooperative cancel. Optional extension, отдельный план если потребуется.

### Семантика codegen

`emit_main_wrapper` эмитит:

```c
int main(void) {
    nova_gc_init();
    nova_evloop_init();
    /* effect-storage registration ... */

    /* D92: implicit main-scope. */
    NovaFiberQueue _nova_main_scope;
    nova_scope_init(&_nova_main_scope);
    _nova_active_scope = &_nova_main_scope;
    _nova_active_slot  = -1;

    nova_fn_main_impl();

    /* D92: drain detach'ов / pending fiber'ов до quiescence. */
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
- 🟡 **SIGINT handler** (Правило 7) — future extension.
- 🟡 **Top-level `Time.sleep` через uv_timer** (Правило 6 не работает) —
  всё ещё busy-yield / native sleep. Под D92 это OK потому что
  `_nova_active_slot = -1` детектируется в `_nova_time_default_sleep`
  как "main-flow, не fiber".

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

> **Status:** active. **Ред. 2** (Plan 82, 2026-05-22) — Windows
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

Плакируется ([std/runtime/fibers.nv](../../std/runtime/fibers.nv)):

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

- `std/time/duration.nv` — добавить `type Monotonic { readonly nanos i64 }`
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
- Plan 103.7 — финальная редакция D167; D173 AI-first guidance для паттернов ordering
- D173 — decision tree: когда нужен explicit ordering vs SeqCst-default

---

## D168. Sized atomic types — API contract (Plan 103.2)

> **Статус:** ✅ final (Plan 103.7, 2026-05-27). Реализован в Plan 103.2.

### Что

Nova предоставляет **12 sized atomic типов** с детерминированной шириной:

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
| `AtomicIsize` | платформенная | −2^(W−1)..2^(W−1)−1 | `nova_int` (= `intptr_t`) |
| `AtomicUsize` | платформенная | 0..2^W−1 | `nova_uint` (= `uintptr_t`) |
| `AtomicBool` | 1 логический бит | `false`/`true` | `bool` (8-битное хранение) |
| `AtomicPtr` | платформенная | адрес | `nova_int` (GC proxy) |

Все типы являются **value types в Nova**, копируются по значению при передаче в
функцию / возврате. Семантически — **ячейки с атомарным доступом**, не
разделяемые reference-типы. Для разделения между fiber'ами — передавать
mutable-ссылку или хранить в heap-структуре.

### Правило

#### 1. Матрица операций

Все 12 типов поддерживают следующие операции (обозначение: `T` — тип значения,
`Bool` — `bool`, `Int` — `int`):

| Операция | AtomicI*/U*/Isize/Usize | AtomicBool | AtomicPtr |
|---|---|---|---|
| `new(v T) → AtomicX` | ✓ | ✓ (`v bool`) | ✓ (`v int`) |
| `null() → AtomicPtr` | — | — | ✓ |
| `load() → T` | ✓ | ✓ | ✓ → `int` |
| `load(ord MemOrdering) → T` | ✓ | ✓ | ✓ |
| `store(v T)` | ✓ | ✓ | ✓ |
| `store(v T, ord MemOrdering)` | ✓ | ✓ | ✓ |
| `swap(v T) → T` | ✓ | ✓ | ✓ |
| `swap(v T, ord MemOrdering) → T` | ✓ | ✓ | ✓ |
| `compare_exchange(expected T, desired T) → bool` | ✓ | ✓ | ✓ |
| `compare_exchange(exp T, des T, ord MemOrdering) → bool` | ✓ | ✓ | ✓ |
| `compare_exchange_weak(exp T, des T) → bool` | ✓ | ✓ | — |
| `compare_exchange_weak(exp T, des T, ord MemOrdering) → bool` | ✓ | ✓ | — |
| `fetch_add(v T) → T` | ✓ (int/uint только) | — | — |
| `fetch_add(v T, ord MemOrdering) → T` | ✓ | — | — |
| `fetch_sub(v T) → T` | ✓ | — | — |
| `fetch_sub(v T, ord MemOrdering) → T` | ✓ | — | — |
| `fetch_or(v T) → T` | ✓ | ✓ (`v bool`) | — |
| `fetch_or(v T, ord MemOrdering) → T` | ✓ | ✓ | — |
| `fetch_and(v T) → T` | ✓ | ✓ | — |
| `fetch_and(v T, ord MemOrdering) → T` | ✓ | ✓ | — |
| `fetch_xor(v T) → T` | ✓ | ✓ | — |
| `fetch_xor(v T, ord MemOrdering) → T` | ✓ | ✓ | — |
| `fetch_nand(v T) → T` | ✓ | — | — |
| `fetch_nand(v T, ord MemOrdering) → T` | ✓ | — | — |
| `fetch_max(v T) → T` | ✓ | — | — |
| `fetch_max(v T, ord MemOrdering) → T` | ✓ | — | — |
| `fetch_min(v T) → T` | ✓ | — | — |
| `fetch_min(v T, ord MemOrdering) → T` | ✓ | — | — |

**Примечание по AtomicPtr:** хранит `int` (адрес GC-объекта как `intptr_t`).
Арифметика не поддерживается (`fetch_add` нет). Typed generic form `AtomicPtr[T]`
с GC-root integration — откладывается в Plan 103.9+.

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

#### 4. AtomicPtr — proxy для GC-адресов

`AtomicPtr` хранит `int` (= `intptr_t`) как GC-безопасный адрес. Это
**не typed generic** — Plan 103.7 вводит `AtomicPtr[T]` с proper GC-tracing.

V1 семантика (Plan 103.2):
- `AtomicPtr.new(v int)` — создать из raw int (адрес).
- `AtomicPtr.null()` — создать с значением 0 (null-адрес).
- `load()` → `int` — прочитать адрес как int.
- `store(v int)` — записать адрес.
- `swap(v int)` → `int` — атомарный обмен адресов.
- `compare_exchange(expected int, desired int)` → `bool` — CAS на адресах.
- Arithmetic (`fetch_add`) **не поддерживается** — AtomicPtr не счётчик.

**GC safety V1:** приложение несёт ответственность за то, что int-значение
в `AtomicPtr` остаётся живым GC-объектом. V2 (Plan 103.9+): `AtomicPtr[T]`
с типизированным GC-root — откладывается (typed generic в codegen non-trivial).

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

4. **AtomicPtr как `int` в V1.** Typed `AtomicPtr[T]` требует GC root integration
   в codegen — это non-trivial и откладывается в Plan 103.9+. `int`-proxy достаточен
   для lock-free pointer swapping где объект удерживается через другую ссылку.

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
- D173 — AI-first guidance: counter/swap/CAS паттерны выбора atomic типа.
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

**Backward compatibility note:** `AtomicInt` — deprecated alias на `AtomicI64`
(если существовал в pre-103.2 code). Все новые коды должны использовать
`AtomicI64` напрямую.

Отложено в Plan 103.9+:
- `AtomicPtr[T]` typed generic с GC root integration.
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
- D173 — AI-first guidance: выбор Once/OnceCell/Lazy по паттерну; decision tree.
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

D173 (этот plan) содержит AI-first guidance для init-pattern выбора (Once vs
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
export external fn Mutex mut @try_lock_for(timeout Duration) -> bool

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
export external fn RwLock mut @try_read_for(timeout Duration) -> bool

#stable(since = "0.1")
export external fn RwLock mut @write()
#stable(since = "0.1")
export external fn RwLock mut @write_unlock()
#stable(since = "0.1")
export external fn RwLock mut @try_write() -> bool
#stable(since = "0.1")
export external fn RwLock mut @try_write_for(timeout Duration) -> bool

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
export external fn ReentrantMutex mut @try_lock_for(timeout Duration) -> bool

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

#### 5. Realtime context ban

Методы `lock()` / `read()` / `write()` и `try_lock_for` / `try_read_for` /
`try_write_for` — **запрещены в `realtime { }` блоках** (Plan 103.6):
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

D173 (этот plan) содержит AI-first guidance: выбор Mutex vs RwLock vs
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
fn Semaphore mut @try_acquire() -> bool
fn Semaphore mut @try_acquire_for(timeout Duration) -> bool
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
- **`acquire`/`acquire_n`/`try_acquire_for`/`with_permit`** — park-ing methods,
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
fn CountDownLatch @try_await_for(timeout Duration) -> bool
fn CountDownLatch @current_count() -> int          /* best-effort */
```

**Семантика:**
- **Immutable initial count** (safer than WaitGroup, который позволяет `add()`
  после `wait()` → race risk).
- **`count_down()` saturating:** вызов когда `count == 0` — no-op (НЕ panic);
  Java parity.
- **`count_down_n(n)`:** `n <= 0` → no-op; `n > current count` → saturates на 0.
- **`CountDownLatch.new(0)`** → runtime panic (`count > 0`).
- **`await`/`try_await_for`** — park-ing, banned в `realtime { }`.

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
- **Semaphore (7):** bounded_concurrency, batch_n, try_acquire_for_timeout,
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

D173 (этот plan) содержит AI-first guidance: rate-limited workers (Semaphore),
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
- **`#blocking fn`** — runtime threadpool offload: вся fn выполняется на
  libuv threadpool worker, fiber паркуется до завершения.
  _До Plan 113: `blocking { }` block (D50 §4, Plan 83.3)._

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
| `Mutex` | `try_lock_for(d)` | `#realtime` ¹ | ⚠️ W_REALTIME_TRY_LOCK_FOR_TIMER | ✅ |
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

¹ `try_lock_for` является `#realtime` технически (не парков fiber),
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
| `W_REALTIME_TRY_LOCK_FOR_TIMER` | warning | `Mutex.try_lock_for` в `realtime { }` |
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

### Правило

1. **Annotate все external fn** в `.nv` stdlib с `#parks`/`#wakes`/`#realtime`.
2. **Compile-time enforcement**: `in_realtime` / `in_blocking` flags в CEmitter.
3. **Conservative default**: unannotated method inside realtime{} → E_REALTIME_SYNC_PARK.
4. **User fns**: explicit `#parks` annotation triggers E_REALTIME_NESTED_SYNC_VIA_FN.
5. **try_lock_for**: `#realtime` (no park) + W_REALTIME_TRY_LOCK_FOR_TIMER (timer overhead).

### Тесты

**Positive (10):** realtime_{atomic_load,atomic_fetch_add,mutex_try_lock,
semaphore_try_acquire,lazy_is_forced,oncecell_get,oncecell_set_first_call,fence}_ok,
blocking_{atomic_fetch_add,mutex_unlock}_ok

**Negative (14):** realtime_{mutex_lock,rwlock_read,rwlock_with_write,barrier_wait,
condvar_wait,countdown_await,semaphore_acquire,lazy_force,once_call_once,
oncecell_get_or_init}_neg, realtime_via_user_fn_neg,
blocking_{mutex_lock,condvar_wait}_neg,
realtime_{try_lock_for_zero_warn,mutex_try_lock_for_neg} (warnings)

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

---

## D173. AI-first guidance — sync-primitive decision tree (Plan 103.7)

> **Статус:** ✅ final (Plan 103.7, 2026-05-27). Новый D-блок; нет предшествующего draft.

### Зачем

Nova `runtime.sync` содержит 12+ sync-примитивов. Выбор правильного примитива
для конкретной задачи — типичный вопрос разработчика (и AI-агента, генерирующего
код). D173 формализует **decision tree** и **canonical patterns** — официальный
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
│       └── pointer publish (first-to-publish)    → AtomicPtr.compare_exchange(0, ptr, SeqCst)
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

D173 введён в Plan 103.7 как новый D-блок (нет предшествующего draft).
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
| `lock()` | `Mutex mut @lock() -> MutexGuard consume` | Parks; returns guard |
| `MutexGuard.unlock()` | `MutexGuard @unlock(consume self)` | Consumes guard; wakes next |
| `unlock()` (bare) | `Mutex mut @unlock()` | Deprecated V1; `W_BARE_UNLOCK_DEPRECATED` |
| `with_lock(fn)` | `Mutex mut @with_lock[R](body fn() -> R) -> R` | Thin wrapper; backward compat |

C mangling (Plan 100.6 D164):
- `MutexGuard.unlock(consume self)` → `Nova_MutexGuard_consume_unlock`
- `Mutex.lock()` → `Nova_Mutex_method_lock` (returns `Nova_MutexGuard*`)

#### RwLock V2

| Метод | Сигнатура | Примечание |
|---|---|---|
| `read()` | `RwLock mut @read() -> ReadGuard consume` | Parks; returns read guard |
| `write()` | `RwLock mut @write() -> WriteGuard consume` | Parks; returns write guard |
| `ReadGuard.unlock()` | `ReadGuard @unlock(consume self)` | Consumes guard; wakes if needed |
| `WriteGuard.unlock()` | `WriteGuard @unlock(consume self)` | Consumes guard; wakes next |
| `read_unlock()` (bare) | `RwLock mut @read_unlock()` | Deprecated V1 |
| `write_unlock()` (bare) | `RwLock mut @write_unlock()` | Deprecated V1 |
| `with_read(fn)` | `RwLock mut @with_read[R](...) -> R` | Thin wrapper; backward compat |
| `with_write(fn)` | `RwLock mut @with_write[R](...) -> R` | Thin wrapper; backward compat |

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
| `try_start()` | `Once mut @try_start() -> Option[OnceGuard consume]` | Nova body; Some = won race |
| `OnceGuard.commit()` | `OnceGuard @commit(consume self)` | Once → DONE; wakes waiters |
| `OnceGuard.abort()` | `OnceGuard @abort(consume self)` | Once → POISONED; wakes waiters (re-panic on resume) |
| `call_once(fn)` | `Once mut @call_once(body fn() -> ())` | V1 external; kept as-is |
| `run()` (bare) | `Once mut @run() -> bool` | Deprecated V1 |
| `done()` (bare) | `Once mut @done()` | Deprecated V1 |

`try_start()` is implemented as a Nova body:
```nova
export fn Once mut @try_start() -> Option[OnceGuard consume] {
    if self.try_start_won() {
        Some(self.make_guard())
    } else {
        None
    }
}
```
Where `try_start_won()` is the internal external fn (`Nova_Once_method_try_start_won` = alias to `run()`),
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
of `try_start()` / `call_once()` re-panic with `OncePoisoned`. Rationale: abort
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
- [D173](#d173-ai-first-guidance--sync-primitive-decision-tree-plan-1037) — Decision tree updated to reference D174.

### Эволюция

D174 введён в Plan 103.9 (2026-05-27) как финальный D-блок Plan 103 серии.
Закрывает «V2 consume guards migration» задачу из Plan 100.7 (stdlib migration playbook).

Предполагаемые follow-ups:
- Plan 103.9 V2.1: `try_lock() -> Option[MutexGuard consume]` (M-D174-4 follow-up).
- Plan 103.9 V2.2: Edition-gated removal of deprecated bare `unlock()/release()/done()`.
- Plan 100.8 LSP: quick-fix «wrap in consume guard» for deprecated bare-unlock sites.
