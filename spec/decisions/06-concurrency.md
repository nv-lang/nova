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

// select — ожидание любого из событий
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
   - Гетерогенная параллельность → `mut`-захваты или channels.

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

- Точные API `Channel[T]`, `Mutex`, `Atomic[T]` — Q9 (stdlib).
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

В bootstrap-codegen **сейчас возвращает unit** — упрощение. Полная
реализация требует:
- Сбор результатов body каждой итерации в массив.
- Heap-allocated `NovaArray_T*` нужного типа (тип берётся из body).
- Гарантия порядка результатов — соответствует порядку `iter`.

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
последовательно) — использовать `iter.map((x) => body)`:

```nova
let names []str = users.map((u) => u.name)
// или с trailing-block:
let names []str = users.map() { u => u.name }
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
| `iter.map((x) => body)` | `[]T` | sequential map |
| `parallel for x in iter { body }` | `[]T` | parallel map (fan-out) |

Это **намеренное** различие — `for` для side-effects (большинство
случаев), `parallel for` для structured fan-out. Sequential map
выражается через method-chain, не через `for`-form, чтобы избежать
аллокации `[]unit` для side-effect-циклов и сохранить привычную
семантику `for` из Go/Rust/Java.

— но это **не работает в bootstrap** (нет array-index-mutation для
`NovaArray*` в captured контексте). Open-question D71.

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

В bootstrap'е **`ms` игнорируется** — нет timer-wheel'а. `sleep(0)` и `sleep(100)`
неотличимы. Любое значение даёт **один cooperative yield**:

| Контекст вызова | Поведение |
|---|---|
| Внутри fiber-body (spawn) | `nova_fiber_yield()` — coroutine suspended, scheduler resumes others |
| Вне fiber, внутри `supervised` body | `nova_supervised_step(&queue)` — main-flow прокручивает queue один раз |
| Полностью вне любого scope | no-op (нет scheduler'а) |

Это **spec-faithful по D62** (Async — ambient): `Time.sleep` — обычная функция
без эффект-окраски, callable откуда угодно. Поведение зависит от ambient
runtime-окружения в точке вызова.

В production-runtime'е добавится timer-wheel: `sleep(N>0)` поставит fiber в
sleep-list с deadline, scheduler пропускает sleeping fibers до момента их
пробуждения.

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
  effect-dispatch путь (Nova_Time_sleep / Nova_Time_now). Default
  handlers в runtime: sleep — context-sensitive cooperative yield
  (fiber → mco_yield, supervised body → step queue, top-level → no-op);
  now — возвращает 0 (timer-wheel не реализован). User override через
  `with Time = handler Time { sleep(ms) {...} now() {...} } { body }`
  — работает (тесты `46_time_handler.nv`). Что НЕ закрыто: реальный
  timer-wheel (sleep с задержкой не делает реальной задержки).
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
- **Channels (`Channel[T]`).** Spec-mention в D50 secondary. Без них
  producer-consumer — через shared mut + yields (что и тестируется).

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
- Тесты: `tests-nova/38_deep_spawn.nv` (section 10, 9 interleave-тестов),
  `40_detach.nv` (13), `41_parallel_for.nv` (12), `42_main_yield.nv` (11).
  Полный suite — 42/42 passing.
