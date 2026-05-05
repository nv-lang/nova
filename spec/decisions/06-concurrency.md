# Concurrency — параллелизм и асинхронность

Решения этой группы определяют модель параллельных вычислений Nova:
как fiber-runtime обеспечивает невидимую приостановку, какие
structured-concurrency примитивы есть в языке, и как параллелизм
выражается в коде.

| # | Решение |
|---|---|
| [D14](#d14-fiber-runtime--невидимая-инфраструктура) | Fiber runtime — невидимая инфраструктура |
| [D50](#d50-concurrency-model-spawn-detach-blocking) | Concurrency model: `spawn`, `detach`, `Blocking` |

---

## D14. Fiber runtime — невидимая инфраструктура

> ⚠️ **REVISED → [D62](04-effects.md#d62), [D64](04-effects.md#d64).**
> Изначально D14 объявлял `Async` как эффект. После D62 `Async` **не
> является эффектом** — это runtime-инфраструктура, ambient capability.
> В сигнатурах не пишется. Гарантия не-приостановки даётся блоком
> [`realtime`](04-effects.md#d64) как inverse-маркер, а не отсутствием
> `Async` в сигнатуре. Структурный параллелизм через [D50](#d50)
> (`spawn`, `parallel`, `race`, `cancel_scope`).

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

#### 2. `spawn` — только в structured-scope

`spawn` — keyword-конструкция (не функция). Синтаксис: `spawn expr`, где
`expr` — любое выражение: вызов функции, блок, и т.д.

```nova
spawn fetch_users()          // вызов функции
spawn { compute(x) }         // inline-блок
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
