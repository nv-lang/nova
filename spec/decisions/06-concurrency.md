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
    let user = fetch_user(req.id)            // никаких .await
    let posts = fetch_posts(user.id)         // никаких .await
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
let t = ChanReader.close_after(Duration.from_secs_f64(5.0))
select {
    Some(msg) = channel_a.recv() => { process(msg) }
    Some(msg) = channel_b.recv() => { process(msg) }
    None      = t.recv()         => { default_value }
}

// supervised(cancel:) — структурная отмена с внешним токеном (D75)
let tok = CancelToken.new()
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
> (default SyncDetach handler — sync inline), `Time.sleep` как
> yield-point. Capture-by-value для immutable scalars. Не реализованы:
> `race`, `select`, `cancel_scope`, `with_timeout`, `blocking`, реальный
> async-detach (OS-thread + global supervisor), эффект `Detach` в
> effect-system, cancellation/error-propagation между fibers.

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
let r = spawn fetch_a()
```

Чтобы получить результат от concurrent-выполнения:

| Сценарий | Идиома |
|---|---|
| Нужен результат, можно подождать sequentially | прямой вызов: `let users = fetch_users()` (async прозрачный — D62) |
| Гомогенный fan-out с массивом результатов | `let xs = parallel for url in urls { fetch(url) }` |
| Гетерогенная параллельность с разными типами | `mut`-захваты внутри `supervised` |

Пример mut-захватов:
```nova
let mut a = 0
let mut b = 0
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
    let resp = process(req)
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

#### 4. `blocking { ... }` для синхронных C-вызовов

Синхронные C-функции (`read(2)` без `O_NONBLOCK`, `pthread_mutex_lock`,
тяжёлые computational библиотеки) **блокируют ОС-поток**. На M:N
scheduler'е это значит, что весь worker встал. Решение — отдельный
pool ОС-потоков для блокирующих задач:

```nova
fn read_file_sync(path str) Blocking Fail[IoError] -> []byte =>
    blocking {
        c_read_file(path)             // выполняется на blocking-pool потоке
    }
```

`blocking { ... }`:
- syntactic primitive языка,
- передаёт текущий fiber на отдельный ОС-поток из blocking-pool,
- worker scheduler'а возвращается в общий пул, обслуживает другие
  fiber'ы,
- когда C-код вернулся — fiber возвращается в обычный pool worker'а,
- requires эффект `Blocking` в сигнатуре.

`Blocking`-эффект:
- виден в сигнатуре (caller знает «может заблокировать поток»),
- **запрещён внутри `realtime { }`-блока** ([D64](04-effects.md#d64)) —
  блок гарантирует не-suspension, а blocking-pool вызывает suspend
  на ОС-потоке,
- handler можно подменить (тесты, mock C-вызова).

Размер blocking-pool — runtime-конфиг (`NOVA_BLOCKING_POOL`,
default 64). Если пул заполнен — fiber ждёт в очереди.

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

- `Channel[T]` API — формализован в [D79](#d79). `Mutex`/`Atomic`
  отвергнуты ([D79 «Что отвергнуто»](#d79)) в пользу channel-only
  модели + owner-actor pattern.
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
let responses []Response = parallel for url in urls { fetch(url) }
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
let names []str = users.map(|u| u.name)
// или с trailing-fn (для длинных тел):
let names []str = users.map() fn(u) => u.name
```

**`parallel for` — expression** (тип `[]T`). Тело — функция от элемента
к результату:

```nova
let responses []Response = parallel for url in urls { fetch(url) }
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
let r = spawn { compute_x() }    // запускается СРАЗУ до завершения,
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
let users = fetch_users()       // тип []User; suspension случается сама

// (2) parallel for — массив гомогенных результатов.
let responses = parallel for url in urls { fetch(url) }   // []Response

// (3) mut-захваты — гетерогенная параллельность.
let mut a = 0
let mut b = 0
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
    let resp = process(req)
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
  (`int`, `bool`, `f64`, `f32`, `byte`). Значение **копируется** в ctx-struct
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
  yield (compatibility-режим). User override через `with Time = handler
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
    let mut results []Response = []
    supervised(cancel: cancel) {
        for url in urls {
            spawn { results.push(fetch(url)) }
        }
    }
    results
}

// caller-side:
let tok = CancelToken.new()
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
let tok = CancelToken.new()
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
    let tok = CancelToken.new()
    spawn { Time.sleep(dur); tok.cancel() }
    supervised(cancel: tok) { body() }
}

export fn race[T](competitors []fn() -> T) -> T {
    let ch  = Channel[T].new(capacity: competitors.len())
    let tok = CancelToken.new()
    supervised(cancel: tok) {
        for comp in competitors {
            let c = comp
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
    while let Some(req) = ch.recv() {
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
let timeout = ChanReader.close_after(Duration.from_secs_f64(5.0))
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
    let (processed_tx, processed_rx) = Channel.new(100)

    spawn {
        while let Some(req) = input.recv() {
            let resp = process(req)
            processed_tx.send(resp)
        }
        processed_tx.close()
    }

    spawn {
        while let Some(resp) = processed_rx.recv() {
            Db.exec(resp.persist_sql)
        }
    }
}
```

**Fan-out:**

```nova
let (work_tx, work_rx) = Channel.new(0)
for i in 0..10 {
    let rx = work_rx   // capture by value
    spawn {
        while let Some(task) = rx.recv() {
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
let (work_tx, work_rx) = Channel.new(0)
let (stop_tx, stop_rx) = Channel.new(1)

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

#### Mutex / Atomic — НЕ в spec

Channel — **достаточный** primitive для всех coordination patterns.
Mutex и atomic — нижнеуровневые, легко misuse'ить, не AI-friendly.

Если мутируемое разделяемое состояние действительно нужно, идиома:
**dedicated owner-fiber + channel** (Erlang-стиль). Owner владеет
данными, остальные шлют ему сообщения через channel.

```nova
fn counter_actor(input Channel[CounterMsg], output Channel[int]) {
    let mut value = 0
    while let Some(msg) = input.recv() {
        match msg {
            Increment => value += 1
            Get       => output.send(value)
            Reset     => value = 0
        }
    }
}
```

Это **safe by construction** — нет shared state, только message-passing.

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
  Если кому-то реально нужен Mutex — может реализовать через channel
  (token-channel вместимостью 1).

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
    with Time = handler Time { sleep(_) => () now() => 100 } {
        Time.now()                   // ВСЕГДА 100, независимо от других fiber'ов
    }
}

fn use_clock_200() -> int {
    with Time = handler Time { sleep(_) => () now() => 200 } {
        Time.now()                   // ВСЕГДА 200
    }
}

supervised {
    spawn { let a = use_clock_100() }   // a == 100, гарантированно
    spawn { let b = use_clock_200() }   // b == 200, гарантированно
}
```

Inheritance + override:

```nova
with Time = handler Time { ... now() => 42 } {
    supervised {
        spawn {
            assert(Time.now() == 42)         // наследовал outer

            with Time = handler Time { ... now() => 999 } {
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
- `Handler[X]` как first-class value (`fn make() -> Handler[X]`) сложнее —
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
let (tx, rx) = Channel[int].new(4)
tx.send(10)
let v = rx.recv()
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
    let (tx, rx) = Channel[Job].new(10)
    defer tx.close()                              // гарантированный close

    supervised {
        spawn { for j in jobs { tx.send(j) } }
        spawn { while let Some(j) = rx.recv() { process(j) } }
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
let (tx, rx) = Channel[Job].new(10)
let tx2 = tx.clone()
supervised {
    spawn { tx.send(1);  tx.close() }
    spawn { tx2.send(2); tx2.close() }
    spawn { while let Some(v) = rx.recv() { process(v) } }
}
```

**Семантика close с несколькими writers:** канал закрывается только
когда **все** writers вызвали `close()`. Внутри — ref-count
(`writer_count`): `Channel.new` инициализирует в 1, `clone()`
инкрементирует, `close()` декрементирует и закрывает при 0.

Идиома для spawn-fan-in:
```nova
let (tx, rx) = Channel[int].new(8)
supervised {
    for item in work_items {
        let worker_tx = tx.clone()
        spawn { worker_tx.send(process(item)); worker_tx.close() }
    }
    tx.close()                                    // close «корневого» writer'а
    spawn { while let Some(v) = rx.recv() { collect(v) } }
}
```

> **Managed heap и captures.** Без `clone()` два `spawn` могут захватить
> один `tx` через managed reference — оба могут слать. Но `close()`
> первого spawn'а закрыл бы канал для второго. `clone()` решает это:
> каждый spawn держит свою capability и закрывает её независимо.

#### `select` после revision

`select` работает через `ChanReader` (Plan 31, не реализован):

```nova
let (_, rx_a) = Channel[int].new(0)
let (_, rx_b) = Channel[int].new(0)

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
    let (tx, rx) = Channel[int].new(4)
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
    let (tx, rx) = Channel[Job].new(10)
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
let ch = Channel[int].new(4)
ch.send(10)
let v = ch.recv()
ch.close()
```

**Стало (D91):**

```nova
let (tx, rx) = Channel[int].new(4)
defer tx.close()
tx.send(10)
let v = rx.recv()
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
(Plan 44+ std.net/std.fs).

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
- **Plan 44+ `std.net`**: `TcpStream.read` → `uv_read_start` + `uv_read_stop` stop_cb.
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
- **Plan 44+ (std.net, std.fs):** socket-read, file-read — ASYNC stop_cb.
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
- 🟡 **std.net/std.fs IO** (Plan 44+) — ASYNC stop_cb, ждёт реализации.

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
- Будущие IO operations (Plan 44+ `std.net`) на top-level — не
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
event-loop driven scheduler. Под Plan 18 (std.net) все IO operations
требовали бы special-case для top-level. Нежелательно.

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
let t = ChanReader.close_after(Duration.from_secs(1))     // ChanReader[()] закрывается через 1 сек
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

> **Status:** active (Plan 44.2 Этапы 1-3 закрыты, 2026-05-12).
> Уточняет [D14](#d14-fiber-runtime--невидимая-инфраструктура) для
> bootstrap-runtime: где живут fiber stacks и как они видны GC.

### Что

Suspended fiber stacks **не на OS-стеке** — они лежат в пользовательской
памяти, выделяемой allocator'ом minicoro. Поскольку Boehm GC сканирует
только OS-стек активного потока + явно зарегистрированные roots, fiber
stacks нужно сделать видимыми GC явно. D97 фиксирует **гибридную
стратегию** allocation'а по платформам:

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

**Windows — calloc (как до D97):**

- Дефолтный minicoro allocator (`calloc(56 KB)`) per-fiber.
- GC safety обеспечивается **single-thread cooperative invariant**:
  Boehm не запускается между yield/resume, поэтому calloc'нутые stacks
  «логически live» в течение одного collect window.
- **Windows arena fundamentally blocked** (Plan 44.3, 4 failed attempts
  2026-05-13): Windows TIB (Thread Information Block) tracks current
  thread stack range (StackBase/StackLimit). minicoro `MCO_USE_ASM`
  backend (Windows x64 default) переключает только RSP — НЕ обновляет
  TIB. Любой код требующий TIB validation (SEH unwind, /GS canary,
  control flow guard) hangs на VirtualAlloc'нутом stack.
- Default calloc-based stacks работают «by luck» — HeapAlloc memory
  не enforces strict TIB validation в same paths.
- Решения (если когда-нибудь): либо переход на `MCO_USE_FIBERS`
  (CreateFiber API — proper TIB через kernel) ценой N independent
  fiber allocations (не arena), либо minicoro patch (нарушает
  «не патчить сторонние библиотеки»). Bootstrap calloc-путь остаётся.

### Зачем гибрид, а не unified path

- **Linux/macOS — primary production target** для Plan 44 M:N runtime
  (Docker, Kubernetes, server workloads). 8 GB virtual per thread — free
  на x86_64 (256 TB address space).
- **Windows VirtualAlloc** `MEM_RESERVE | MEM_COMMIT` (minicoro
  `MCO_USE_VMEM_ALLOCATOR`) commits all upfront. 256 MB × 16 threads =
  4 GB committed без real benefit для Windows.
- SEH-based growable stacks — отдельная инженерная задача с per-thread
  exception handler chains. Bootstrap path работает; production-Windows
  ждёт реального use case.

### Introspection — `std.runtime.fibers`

Плакируется ([std/runtime/fibers.nv](../../std/runtime/fibers.nv)):

```nova
import std.runtime.fibers

let virt    = fibers.virtual_reserved()  // bytes mmap'нуто, 0 на Windows
let total   = fibers.slot_count()         // 4096 на Linux/macOS, 0 Win
let active  = fibers.slots_active()       // running fibers сейчас
let peak    = fibers.high_water()         // peak concurrent
```

`slot_count() == 0` — honest sentinel «arena not active»; тесты могут
бранчиться по нему для cross-platform проверок.

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

### Bootstrap-status

- ✅ Arena infrastructure (Plan 44.2 Этап 1, commit `0b75bdcb06`)
- ✅ Wire-up в minicoro через `_NOVA_MCO_DESC_INIT` (Plan 44.2 Этап 1
  wire-up landing, commit `5ed208e84f`)
- ✅ Удаление `_NOVA_GC_DISABLE` (Plan 44.2 Этап 2, commit `810898de06`)
- ✅ `std.runtime.fibers` introspection (Plan 44.2 Этап 3, commit
  `f8d345e536`)
- ⏸ Linux Docker validation (Plan 44.2 Этап 4) — требует Docker daemon
- ⏸ SIGSEGV pretty handler (P41-6) — P2, отложено

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
        let mut i = 0
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

