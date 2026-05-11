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
| [D75](#d75-cancel_scope--ручная-структурная-отмена) | `cancel_scope { tok => ... }` — ручная структурная отмена (реализовано) |
| [D79](#d79-channels--coordination-между-fiber-ами) | ⚠️ Уточнено [D91](#d91): `Channel[T]` (старая Go-style модель — один объект, send+recv на нём) |
| [D80](#d80-handler-scoping-per-fiber) | Handler scoping per-fiber — `with X = handler` локален для fiber'а, наследуется через spawn |
| [D80](#d80-handler-scoping-per-fiber) | Handler scoping per-fiber — `with X = h` биндинги изолированы между fibers |
| [D91](#d91-channel-revision--capability-split-на-sender--receiver) | `Channel` revision: capability-split на `Sender[T]` / `Receiver[T]` (Rust mpsc-style) |

---

## D14. Fiber runtime — невидимая инфраструктура

> ⚠️ **REVISED → [D62](04-effects.md#d62), [D64](04-effects.md#d64).**
> Изначально D14 объявлял `Async` как эффект. После D62 `Async` **не
> является эффектом** — это runtime-инфраструктура, ambient capability.
> В сигнатурах не пишется. Гарантия не-приостановки даётся блоком
> [`realtime`](04-effects.md#d64) как inverse-маркер, а не отсутствием
> `Async` в сигнатуре. Структурный параллелизм через [D50](#d50)
> (`spawn`, `parallel`, `race`, `cancel_scope`).
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

Structured concurrency (`parallel for`, `race`, `select`,
`cancel_scope`, `with_timeout`, `spawn`) — **примитивы языка**, см.
[D50](#d50).

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

// select — ожидание любого из событий (полная семантика — D79)
select {
    msg <- channel_a => process(msg),
    msg <- channel_b => process(msg),
    timeout(5.seconds()) => default_value,
}

// cancel_scope — ручное управление отменой
cancel_scope { tok =>
    spawn do_thing(tok)
    spawn do_other(tok)
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
   `cancel_scope` — часть языка. Это значительно безопаснее для
   AI-генерации (нет утечек fiber'ов).

### Сравнение с Rust async

| | Rust async | Nova |
|---|---|---|
| Цвет функции | да (`async fn`) | нет |
| `await` нужен | да | нет |
| Тип возврата меняется | `Future<T>` | `T` (не меняется) |
| Стоимость задачи | ~64 байта (state machine) | ~4-8 KB (fiber stack) |
| Cancellation | ручная (Drop) | structured (`cancel_scope`) |
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
`supervised`, `parallel for`, `race`, `select`, `cancel_scope`,
`with_timeout`. Вне такого скоупа `spawn foo()` — ошибка компиляции.

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
- **Plan 23 (M:N, milestone v1.0+):** scheduler становится work-stealing
  per-worker, park/wake API D93 расширяется на cross-worker wake через
  `uv_async_t`.

---

## D75. `cancel_scope { tok => ... }` — ручная структурная отмена

> **Status:** active. **Реализовано** в bootstrap'е (2026-05-06).
> Тесты: `nova_tests/concurrency/cancel_scope_test.nv` (5 тестов).

### Что

`cancel_scope { tok => body }` — это `supervised`-scope, которому
**снаружи** можно сообщить «отмени всех fiber'ов внутри». Связь
снаружи/внутри идёт через токен `tok` — first-class значение,
которое замыкается в spawn'ах body и которое **caller текущего
scope'а** может удерживать и вызвать `tok.cancel()` на нём извне
(например, из другого fiber'а).

```nova
fn fetch_with_kill_switch(urls []str, kill ?CancelToken) -> []Response {
    let mut results []Response = []
    cancel_scope { tok =>
        // если caller дал нам внешний kill — связываем его с tok
        if let Some(k) = kill { k.bind(tok) }
        for url in urls {
            spawn {
                if !tok.is_cancelled() {
                    results.push(fetch(url))
                }
            }
        }
    }
    results
}

// caller-side:
let tok = CancelToken.new()
spawn {
    Time.sleep(5_000)
    tok.cancel()        // через 5s принудительно валим scope
}
fetch_with_kill_switch(urls, Some(tok))
```

### Семантика

1. **`cancel_scope { tok => body }`** — синтаксис аналогичен
   `supervised { ... }`, но вводит `tok` (тип `CancelToken` —
   pre-registered protocol/struct в prelude) как биндинг в body-scope.
2. **Token capabilities:**
   - `tok.cancel()` — пометить scope как cancelled. Все fiber'ы
     scope'а на следующем yield-point бросят `"scope cancelled"`
     (тот же механизм что `cancel_requested` в D71). Idempotent.
   - `tok.is_cancelled() -> bool` — проверка флага без yield.
     Не throws.
   - `tok.bind(other CancelToken)` — связать токен с другим:
     при отмене `other.cancel()` вызывает и `tok.cancel()`. Это
     даёт композицию (включение scope-токена в более широкий
     родительский kill-switch).
3. **Ручная отмена изнутри scope'а** (`tok.cancel()` внутри
   spawn-body) — допустима. Эффект: остальные spawn'ы в том же
   scope'е тоже получают cancel-сигнал на следующем yield.
4. **Auto-уборка fiber'ов:** на выходе из `cancel_scope { ... }`
   гарантируется, что все spawn'ы scope'а завершились (как в
   `supervised`), независимо от того, сработала отмена или нет.
5. **Throw + cancel:** если внутри scope'а `throw`, scope сначала
   ставит `cancel_requested = true` (как в supervised), потом
   re-throw'ит на main flow. Token остаётся cancelled.

### Отличие от `supervised`

| | `supervised { body }` | `cancel_scope { tok => body }` |
|---|---|---|
| Wait для всех fiber'ов | да | да |
| Cancel изнутри (через throw) | да | да |
| **Cancel снаружи** | **нет** | **да** (через `tok.cancel()`) |
| Token-binding (родительский kill-switch) | нет | да |

### Реализация (план)

В bootstrap'е уже есть `NovaFiberQueue.cancel_requested` (D71).
Реализация D75 — это:

1. **Lexer/parser/AST.** Новый keyword `cancel_scope`,
   AST `ExprKind::CancelScope { token_name, body }`.
2. **`CancelToken` тип в runtime.** Структура с `cancelled bool` и
   опциональным указателем на чужую очередь:
   ```c
   typedef struct CancelToken {
       NovaFiberQueue* scope;       // own scope
       struct CancelToken** linked; // bound parents
       int linked_count;
   } CancelToken;
   ```
   Методы: `cancel()` ставит `scope->cancel_requested = true` +
   walks `linked[]` и cancel'ит их; `is_cancelled()` читает
   `scope->cancel_requested`; `bind(other)` пушит `&self.scope`
   в `other.linked`.
3. **Codegen.** `emit_cancel_scope` — как `emit_supervised`, но
   объявляет локальный `CancelToken` биндинг, чей `scope` указывает
   на queue scope'а. Token капчится в spawn-body как обычная
   immutable scalar (by-value pointer).
4. **Tests.**
   - manual `tok.cancel()` внутри spawn → peer fiber отменяется на yield
   - manual `tok.cancel()` снаружи (из другого fiber'а в outer scope)
   - `bind` каскадная отмена
   - повторный `cancel()` — idempotent, no panic

### Почему отдельный примитив

`supervised` намеренно «закрытый» — нет способа извне принудительно
свалить его (кроме panic'а изнутри). Это безопасное умолчание для
большинства serial-кода. `cancel_scope` — escape hatch для случаев
когда нужен kill-switch (timeout-обёртка, user cancel button,
fail-fast при внешнем сигнале). Разделение делает код самодокументирующимся:
если видно `cancel_scope`, значит scope намеренно отменяемый.

### Что отвергнуто

- **Передача `tok` через goroutine-channel** — это паттерн Go (через
  ctx.Done()). В Nova предпочли явный `bind` метод: композиция
  токенов происходит compile-time видимо, без аллокации канала.
- **Auto-cancel через Drop** — Nova не имеет Drop. Cancellation —
  явная операция через `cancel()`, не побочный эффект scope-exit.
  Это согласовано с D7-style explicit-resource-management.

### Связь

- [D14](#d14-fiber-runtime--невидимая-инфраструктура) — fiber-runtime.
- [D50](#d50-concurrency-model-spawn-detach-blocking) — concurrency model.
- [D71](#d71-bootstrap-concurrency-runtime) — `cancel_requested` flag,
  cooperative cancellation propagation. D75 надстраивается над ним.
- [D93](#d93-park-wake--нормативный-runtime-primitive-для-блокирующих-операций)
  — park/wake API. `cancel()` через `nova_sched_cancel_all_pending`
  пробуждает parked-fiber'ов **immediate** через generic stop_cb
  mechanism (Plan 22 Ф.4). Раньше cancel ждал следующего yield-point'а
  внутри `nova_fiber_yield`; теперь fiber может быть park'нут на
  `uv_timer_t` без yield-point'ов до срабатывания timer'а, и cancel
  всё равно срабатывает сразу.

### Реализация (2026-05-06)

- `compiler-codegen/nova_rt/fibers.h`: `NovaCancelToken` struct +
  `nova_cancel_token_init/cancel/is_cancelled/bind`.
- `compiler-codegen/src/lexer/`: keyword `cancel_scope` (`KwCancelScope`).
- `compiler-codegen/src/ast/`: variant `CancelScope { token_name, body }`.
- `compiler-codegen/src/parser/`: `parse_cancel_scope`.
- `compiler-codegen/src/codegen/emit_c.rs`: `emit_cancel_scope` +
  built-in dispatch для `tok.cancel()` / `is_cancelled()` / `bind()`
  на receiver-типе `NovaCancelToken*`.
- `nova_tests/concurrency/cancel_scope_test.nv`: 5 тестов (без cancel ≡ supervised,
  is_cancelled false по умолчанию, internal cancel + peer-non-execute,
  double-cancel idempotent, is_cancelled() reflects state, bind cascade).

### Известные ограничения bootstrap-реализации

1. **Cancel-throw на main flow приходит как plain `nova_throw`**, не как
   `Nova_Fail_fail` через handler-vtable. Это значит user `with Fail`
   handler **не вызывается** (handler-method не запускается). Top-level
   `_nova_fail_top` ловит longjmp, control возвращается в `with`-блок
   через else-ветку. Различить cancel-throw от любого другого fiber-
   error через caught-msg сейчас нельзя. Тесты в 52 обходят это
   проверкой side-effects (peer не выполнился).

   *Причина:* если supervised_run роутил бы re-throw через
   `Nova_Fail_fail`, для thrown-в-fiber через `throw "msg"` (D25) handler
   вызывался бы дважды (раз в fiber-Nova_Fail_fail, раз в re-throw),
   что ломает тест `45_fail_handler.nv` "handler invoked once per
   throwing fiber". Корректный фикс требует различать source: fiber-
   throw-from-handler vs cooperative-cancel-throw — отдельная задача.

2. **NOVA_CANCEL_LINKED_CAP=8** — token может быть привязан к не более
   чем 8 родительским токенам. Production-runtime — динамический список.

3. **Token не survives scope-exit.** Token хранит указатель на
   queue-frame. После выхода из cancel_scope queue уничтожен; token
   становится dangling. По дизайну: токен — scope-bound handle.

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
| `send(v)` | panic ("send on closed channel") | panic |
| `try_send(v)` | false | false |
| `recv()` | None | Some(item) — дренаж |
| `try_recv()` | None | Some(item) — дренаж |

**`send` на closed channel — panic, не throw.** Это programming error
(как двойной free), не recoverable runtime condition. Закрывать channel
должен **producer** (или координирующая сторона), и после close никто
не должен отправлять — это invariant программы. По D13 — panic.

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

```nova
select {
    msg <- ch_a       => process_a(msg)
    msg <- ch_b       => process_b(msg)
    timeout(5.seconds()) => default_action()
}
```

**Грамматика:**

```
select-expr   = 'select' '{' select-arm+ '}'
select-arm    = recv-arm | timeout-arm
recv-arm      = pattern '<-' expr '=>' arm-body
timeout-arm   = 'timeout' '(' expr ')' '=>' arm-body
```

**`<-`** — recv-операция в pattern-position select-арма (только там).
Не general operator.

**Семантика:**

1. Запускается **все** recv-операции одновременно. Блокирует пока
   ≥ 1 готов (есть значение в буфере или закрыт), либо timeout
   истёк.
2. Если **несколько** готовы одновременно — выбор **non-deterministic**
   (runtime может round-robin / random / FIFO). Программист **не
   должен** полагаться на конкретный порядок.
3. **Closed channel в recv-арме** → возвращает None, арм-pattern
   match'ится (например, `None => break`).
4. **Без timeout-case** — `select` может блокировать бесконечно
   (если все каналы пусты и не закрываются).
5. **Один timeout-case** — обязательное-уникальное ограничение.
   Несколько timeout — compile error.

Pattern в recv-арме — **обычный pattern** на `Option[T]` (channel
type). Programmer обычно пишет `Some(msg)` или `None`:

```nova
select {
    Some(msg) <- ch_a => process(msg)
    None      <- ch_a => break               // ch_a закрылся
    Some(req) <- ch_b => handle(req)
    timeout(1.second()) => log_idle()
}
```

Сокращение `msg <- ch_a` (без `Some(...)`) валидно если все ветки
ждут только Some-вариантов:

```nova
select {
    msg <- ch_a => process(msg)              // подразумевает Some(msg); None игнорируется
    msg <- ch_b => process(msg)
}
```

Если все каналы закрылись и нет timeout-case — `select` panic
("all channels closed in select without timeout"). Программист либо
обрабатывает None явно, либо ставит timeout.

#### Канонические patterns

**Producer/consumer:**

```nova
fn pipeline(input Channel[Request]) Db -> () {
    let processed = Channel[Response].new(100)

    spawn {
        while let Some(req) = input.recv() {
            let resp = process(req)
            processed.send(resp)
        }
        processed.close()
    }

    spawn {
        while let Some(resp) = processed.recv() {
            Db.exec(resp.persist_sql)
        }
    }
}
```

**Fan-out:**

```nova
let work = Channel[Task].new(0)
for i in 0..10 {
    spawn {
        while let Some(task) = work.recv() {
            task.run()
        }
    }
}
for t in tasks {
    work.send(t)
}
work.close()
```

**Worker pool с graceful shutdown:**

```nova
let work = Channel[Task].new(0)
let shutdown = Channel[()].new(1)

spawn {
    select {
        Some(task) <- work        => task.run()
        Some(_)    <- shutdown    => return ()
    }
}
```

#### Bootstrap-семантика (D71)

В D71 bootstrap-runtime (single-threaded cooperative):
- `send` на полный буфер — yield, продолжается когда recv освобождает место
- `recv` на пустой — yield, продолжается когда send добавит
- Memory ordering тривиальна (single thread)
- Round-robin между select-armами

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

1. **Закрывает реальный пробел spec'и.** D14/D50 упоминали `select { msg
   <- ch_a => ... }` как пример с подразумеваемым Channel[T], но без
   формальной декларации. D79 формализует.

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

- **`<-` как general operator.** `recv` через method `.recv()` для
  consistency с другими method-based API. `<-` только в pattern-position
  select-арма — синтаксический сахар, не expression.

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

2. **`select` в parser.** Новая конструкция: `select { pattern <- expr
   => body, timeout(d) => body }`. Compiler-codegen агент займётся
   когда D79 будет принят.

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
- [D75](#d75) — `cancel_scope`; channels часто используются с
  cancellation tokens.
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
- [D75](#d75) — `cancel_scope` использует тот же per-scope state pattern.

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

## D91. Channel revision — capability-split на `Sender` / `Receiver`

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

- **`Sender[T]`** — capability «отправлять в канал». Методы: `send`,
  `try_send`, `close`.
- **`Receiver[T]`** — capability «получать из канала». Методы: `recv`,
  `try_recv`.

Внутренний state (buffer, sync) **скрыт** — не доступен напрямую,
только через capabilities.

### Правило

#### API — типы

```nova
// Sender capability:
type Sender[T] protocol {
    send(v T) -> ()                              // blocking если буфер полон
    try_send(v T) -> bool                         // true если отправлено, false если буфер полон
    close() -> ()                                  // закрыть канал (idempotent)
}

// Receiver capability:
type Receiver[T] protocol {
    recv() -> Option[T]                            // blocking; None = closed+drained
    try_recv() -> Option[T]                        // None = пусто (НЕ означает closed)
}
```

`Sender` и `Receiver` — **protocols**. Конкретная реализация скрыта
внутри `Channel.new`. Это аналогично Rust `Sender`/`Receiver` как
конкретным типам — Nova предпочла protocol-подход для абстракции
(хочешь свой `Sender` impl — реализуй protocol).

#### Factory

```nova
fn Channel[T].new(capacity int) -> (Sender[T], Receiver[T])
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
- `tx.send(v)` — panic (нельзя слать в closed canal). Strict для
  defensive programming.
- `tx.try_send(v)` — возвращает `false`.
- `rx.recv()` — возвращает `Some(v)` пока буфер не пуст; потом `None`.
- `rx.try_recv()` — то же.

#### Sender clone — не нужен

В Rust `Sender` cloneable: `let tx2 = tx.clone()` (mpsc). В Nova
**через managed heap** все captures share один `tx`:

```nova
let (tx, rx) = Channel[Job].new(10)
supervised {
    spawn { tx.send(...) }                        // оба захватывают тот же tx
    spawn { tx.send(...) }                        // через managed reference
}
defer tx.close()
```

Это работает потому что `Sender` (как любой managed object) — shared
по умолчанию. **`clone()` не нужен** — добавим если появится
performance use-case (per-thread inbox для cache-locality).

#### `select` после revision

Текущий `select` D79 работает через Channel-объект; в D91 — через
Receiver:

```nova
let (_, rx_a) = Channel[int].new(0)
let (_, rx_b) = Channel[int].new(0)

select {
    msg <- rx_a       => process_a(msg)
    msg <- rx_b       => process_b(msg)
    timeout(5.seconds()) => default_action()
}
```

`<-` оператор читает из `Receiver[T]`. Семантика без изменений
(non-deterministic выбор при множественной готовности; полная
семантика — D79 раздел «`select`»).

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
1. `Channel.new(N)` возвращает `(Sender, Receiver)`, не `Channel`.
2. `send` через `tx`, `recv` через `rx`.
3. `close` через `tx.close()` (или `defer tx.close()`).
4. `Channel[T]` как type-аннотация **не используется** в коде — есть
   только `Sender[T]` и `Receiver[T]`.

**Что нужно мигрировать:**
- `std/` — нет существующих `Channel`-API.
- `nova_tests/runtime/channels.nv` — переписать все тесты.
- Bootstrap `nova_rt/channels.h` — переделать API: state-struct
  + sender/receiver wrappers.
- `select { msg <- ch => ... }` — поменять `ch` на `rx`.

Реализация — отдельный план (Plan 22+).

### Что отвергнуто

- **Auto-on-drop (Rust-style).** Не работает в managed heap без
  deterministic destruction. См. «Почему close — explicit».
- **`recv() Fail[Closed] -> T`.** Closed — не ошибка, валидный исход.
  `Option[T]` композируется чище.
- **Sender.clone() в bootstrap.** В Nova managed-heap делает sender
  shared by default; clone не нужен. Если появится use-case
  (per-thread cache-locality) — добавим.
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

- 🟡 **Spec фиксирует семантику.** Реализация — отдельный план
  (Plan 22+), после D90 (defer) поддержки.
- Текущий nova_rt/channels.h реализует **старую** D79-модель. После
  Plan 22 — переделка под D91.
- Существующие тесты `nova_tests/runtime/channels.nv` мигрируются
  в рамках Plan 22.


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
(Plan 23+ std.net/std.fs).

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
обеспечивает это естественно. Под M:N ([Plan 23](../../docs/plans/23-mn-runtime-roadmap.md))
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
- **Plan 23+ `std.net`**: `TcpStream.read` → `uv_read_start` + `uv_read_stop` stop_cb.
- **Plan 23+ `std.fs`**: `File.read` → `uv_fs_t` + `uv_cancel` stop_cb.

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
- [D75](#d75-cancel_scope--tok--ручная-структурная-отмена) —
  cancel_scope. D93 описывает как cancel прерывает blocking-операции
  через generic stop_cb mechanism (вместо cooperative yield-check).
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
- **Plan 23+ (std.net, std.fs):** socket-read, file-read — ASYNC stop_cb.
- **Plan 23 (M:N):** park/wake становится cross-worker. Wake может
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
- 🟡 **std.net/std.fs IO** (Plan 23+) — ASYNC stop_cb, ждёт реализации.

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
- Будущие IO operations (Plan 23+ `std.net`) на top-level — не
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
- [D75](#d75-cancel_scope--tok--ручная-структурная-отмена) — cancel_scope.
  D92 Правило 7 (future): SIGINT через main-scope cancel.
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
