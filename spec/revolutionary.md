# Nova — революционные возможности

Этот документ описывает фичи, которые делают Nova не «ещё одним хорошим
языком», а языком с уникальной заявкой. Все они следуют из одной
центральной идеи (см. [decisions/01-philosophy.md#d10](decisions/01-philosophy.md#d10)):

> **Всё — эффект. Handler — функция первого класса. Killer use-case —
> AI-first программирование.**

---

## R1. Алгебраические эффекты + handler'ы

### Идея

Сеть, диск, время, случайность, лог, ошибка, мутация — это всё
эффекты. Эффект объявляется через `effect`, имеет операции, и
**handler** перехватывает операции и решает, что с ними делать.

Это обобщение `try/catch`, `async/await`, dependency injection и
моков в одну штуку: тест-mock, transaction wrapper, retry,
distributed tracing — всё пишется через один и тот же механизм
handler'а, не через четыре разные библиотеки.

### Базовый синтаксис

```nova
// объявление эффекта
type Logger effect {
    log(msg str) -> ()
}

// функция, использующая эффект
fn process(x int) Logger -> int {
    Logger.log("processing ${x}")
    x * 2
}

// handler — обычное значение через `handler` keyword
let console = effect Logger {
    log(msg) => println("[LOG] ${msg}")
}

// применение handler'а
fn main() Io -> () =>
    with Logger = console {
        process(42)   // напечатает [LOG] processing 42
    }
```

`return value` (или финальное выражение) в handler-method'е —
продолжение вычисления с возвращённым значением. Для досрочного
завершения всего `with`-блока используется `interrupt v` (так
работает `Fail`).

**Особый случай `Fail[E]`.** Операция `Fail[E].fail` имеет тип
возврата `never` — возвращать в точку `throw` нечего. Поэтому у
handler'а `Fail[E]` всего два исхода: `interrupt v` (завершить
with-блок) или новый `throw` (перебросить дальше). Форма «return
value» для `Fail` запрещена.

Роли в обработке ошибок:

- **`throw err`** — синтаксис языка, запускает ошибку. После
  `throw` управление в эту точку не возвращается.
- **`Fail[E]`** — эффект-контракт для перехвата и обработки
  ошибки. У эффекта нет полей, только сигнатуры операций.
- **handler `Fail[E]`** — то, что перехватывает ошибку. Своих
  полей нет, но он захватывает переменные из окружения (как
  обычное замыкание).

### Что из этого следует автоматически

**Тестирование без моков:**

```nova
test "process logs correctly" {
    let mut buf = []
    let collect = effect Logger {
        log(msg) { buf.push(msg); return () }
    }
    with Logger = collect {
        process(42)
    }
    assert(buf == ["processing 42"])
}
```

Никакой mock-библиотеки. Никакого DI-фреймворка. Это **просто handler**.

**Транзакции:**

```nova
type Db effect {
    query(q Sql) -> []DbRow
    exec(q Sql)  -> ()
}

fn transactional(real Effect[Db]) -> Effect[Db] => effect Db {
    query(q) => return real.query(q)
    exec(q)  { staged.push(q); return () }
}

with Db = transactional(real_db) {
    transfer(1, 2, 100)
    transfer(2, 3, 50)
}  // обе операции в одной транзакции, при ошибке — откат
```

Транзакция — handler. Вложенные транзакции — вложенные handler'ы.

**Capability security:**

```nova
fn untrusted_plugin(input str) Logger -> str {
    // плагин может только логировать; Net/Db/Fs недоступны
    Logger.log("plugin called")
    input.reverse()
}
```

Если плагин попытается использовать `Net.get`, **компилятор не
пропустит** — эффект `Net` отсутствует в сигнатуре. Это capability
security в типах, не в рантайме.

---

## R2. Стандартный набор эффектов

В отличие от Koka, Nova поставляется с **готовым набором эффектов
для прикладного программирования**. Их не надо изобретать каждый проект.

| Эффект | Что описывает | Пример handler'а |
|---|---|---|
| `Fail[E]` | Контракт для перехвата и обработки ошибки типа E | catch, retry, log-and-continue |
| `Io` | stdin/stdout/stderr | capture-stdout, mock-stdin |
| `Fs` | Файловая система | virtual filesystem |
| `Net` | Сетевые запросы | record/replay, fault injection |
| `Db` | База данных | транзакция, in-memory storage |
| `Time` | Часы, таймеры, задержки | virtual clock, fast-forward |
| `Random` | RNG | seeded RNG для тестов |
| `Log` | Структурированный лог | JSON, человекочитаемый, capture |
| `Trace` | Распределённая трассировка | OpenTelemetry, off |
| `Ask[T]` | Чтение из контекста (как Reader) | подмена конфига |
| `Alloc[R]` | Аллокация в регионе R | арена, GC, pool |

**Async, Mut, Par не входят** в стандартный набор эффектов
([D62](decisions/04-effects.md#d62)):

- `Async` — ambient capability, не часть type system'ы. Программист
  никогда не пишет в сигнатурах. Fiber-runtime под капотом (см. R7).
- `Mut` — реальные сценарии state-машин покрываются специализированными
  эффектами с понятными именами (Counter, Cache, IdGen, etc.); generic
  Mut[T] провоцировал бы анти-паттерн «безымянное shared state».
- `Par` — runtime-keyword `parallel for` / `spawn`, не эффект.

Цвет функции **отсутствует** — нет деления на «sync» и «async», есть
«какие у функции эффекты». Async никогда не появляется в типах.

---

## R3. Детерминированный режим тестирования

Из эффектов автоматически следует: **любую программу можно запустить
полностью детерминированно**, если все эффекты заменены на
детерминированные handler'ы.

```nova
test "complex flow is deterministic" {
    with Time = fixed(2026-04-28T10:00:00),
         Random = seed(42),
         Net = record_or_replay("testdata/flow.json"),
         Db = in_memory() {
        let result = run_complex_flow()
        assert(result.snapshot() == expected_snapshot)
    }
}
```

Это не требует никаких mock-библиотек — **подмена эффекта часть языка**.
Snapshot-тесты, property-based, time-travel — всё строится из этого.

---

## R4. Контракты в сигнатуре (requires/ensures/invariant)

Эффекты дают видимость **что** делает функция. Контракты — видимость
**при каких условиях это работает**:

```nova
fn withdraw(mut acc Account, amount money) Fail -> ()
    requires amount > 0
    requires acc.balance >= amount
    ensures acc.balance == old(acc.balance) - amount
    ensures result.is_ok || acc.balance == old(acc.balance)
=
    acc.balance -= amount
```

Контракты — **необязательные**. Без них код работает как обычно.
С ними компилятор пытается доказать их статически (как F* / Dafny),
а что не может доказать — превращает в runtime-проверку в debug-режиме
и убирает в release.

Это даёт **градиент**: пишешь как в Go (без контрактов), хочешь
сильнее — добавляешь `requires`, хочешь полную верификацию —
добавляешь `ensures` и `invariant`. Один и тот же язык покрывает
спектр от скрипта до критичного к корректности кода.

---

## R5. AI-first дизайн как явная цель

### R5.1. Локальность контекста

Ни одной фичи, требующей чтения нескольких файлов для понимания одной
функции:

- **Нет неявных импортов** — каждый идентификатор виден откуда пришёл
- **Нет DI через рефлексию** — зависимости в параметрах или эффектах
- **Нет аннотаций-хуков невидимок** (типа `@Autowired`, `@Inject`)
- **Нет глобального изменяемого состояния** — мутируемое состояние
  только через `mut` поля/параметры (локально) или через специализи-
  рованные effects (`Counter`, `Cache` — имена видны в сигнатуре).
  Generic эффект `Mut` удалён в [D62](decisions/04-effects.md#d62).
- **Нет операторного оверлоадинга на произвольные типы** — только
  для стандартных traits
- **Нет macro-перезаписи синтаксиса** — comptime только над типами
  и значениями, не над AST

LLM, которому дали одну функцию, **видит всё, что нужно
для её понимания**.

### R5.2. Сигнатура = прямые эффекты + полная throw-картина

Уточнено в [D62](decisions/04-effects.md#d62): сигнатура показывает
**прямые** эффекты функции (которые она использует сама) и **полную
картину throw** через транзитивность `Fail`. Транзитивные side-эффекты
через вложенные вызовы — warning'ом подсвечиваются, не обязательно
объявляются.

```nova
type TransferError | InsufficientFunds | InvalidAccount

fn transfer(from AccountId, to AccountId, amount money)
    Fail[TransferError]
    Db Time Log
    requires amount > 0
    ensures from != to
    -> TransferReceipt
```

(Несколько типов ошибки — sum-type или multi-Fail в row
`Fail[A] Fail[B]`, [D65](decisions/04-effects.md#d65). Multi-параметры
`Fail[A, B]` отвергнуты [D25](decisions/04-effects.md#d25).)

По этой сигнатуре LLM (и человек) знает:
- что принимает и возвращает
- какие ошибки бросает (`Fail` транзитивен — это **полная**
  throw-картина включая через вложенные вызовы)
- какие эффекты функция использует **напрямую** (БД, время, лог)
- какие входные ограничения
- какие гарантии на выходе

То что **не** в сигнатуре:
- Эффекты, которые функция получает только через вложенные вызовы
  (компилятор-warning при их обнаружении, можно подавить через
  `@allow_transit` или Nova.toml).
- `Async` — невидимая инфраструктура, никогда в сигнатуре.

Это **компромисс**, принятый D62: полная транзитивность всех эффектов
делает реальные сигнатуры backend-кода нечитаемыми (8-10 эффектов
накопляется на 5 уровнях вызова). Прямые + Fail-strict — баланс
между «сигнатура говорит правду» и «сигнатура читаемая».

В Java/Python/Go этой информации **нет в сигнатуре**, она в коде или
её нет вообще. LLM приходится читать тело и угадывать. Nova остаётся
**впереди мейнстрима** в плане видимости throw + прямых эффектов,
просто не идёт до полной транзитивной видимости side-effects.

### R5.3. Ошибки компилятора как обучающий сигнал

Каждое сообщение об ошибке имеет структуру, оптимизированную под LLM:

```
error E0142: missing effect `Net`

  in function `fetch_user` at src/users.nv:34
  ┌─ src/users.nv:34:5
  │
  34 │     http.get(url)
  │     ^^^^^^^^^^^^^ this call requires effect `Net`
  │
  function signature is:
    fn fetch_user(id u64) -> User

  function should be:
    fn fetch_user(id u64) Net -> User
                          ^^^

  why: `http.get` performs network I/O. Functions that perform I/O
       must declare it in their signature so callers can decide
       whether to allow it.

  fix-suggestion: add `Net` to the effect list before `->`

  see also: docs/effects/Net.md
```

Формат: место → причина → как исправить → **готовый патч** →
ссылка на документацию. LLM применяет патч за одну итерацию.

### R5.4. Стабильность синтаксиса

Явное обязательство в дизайне: **никаких breaking changes синтаксиса
после v1.0**. Новые фичи — только аддитивно. Это гарантия для LLM,
обученных на старых данных, что их код останется валидным.

Цена — ошибки дизайна нельзя будет починить. Поэтому v1.0 выпускается
поздно, после долгого preview-периода.

### R5.5. Проверяемость по фрагменту

Возможность типечекнуть **одну функцию** без всего проекта:

```bash
nova check --fragment 'fn double(x int) -> int = x * 2'
# → ok

nova check --fragment 'fn double(x) = x * 2' --infer
# → fn double[T Mul[T, int]](x T) -> T  (выведенная сигнатура)
```

LLM может генерировать функции и проверять их по одной, без
context'а всего проекта. Это меняет петлю обратной связи кардинально.

### R5.6. Self-describing API

Стандартная библиотека пишется так, чтобы каждая функция описывала
себя через сигнатуру + структурированный doc-комментарий. По
[D62](decisions/04-effects.md#d62) сигнатура содержит прямые эффекты
+ полную throw-картину; транзитивные side-effects дополнительно
указываются в doc-комментарии для ясности.

```nova
/// Sends an HTTP GET request.
///
/// effect.Net: makes an outgoing request
/// effect.Time: waits up to `timeout` ms
/// effect.Fail[NetError]: on connection failure, timeout, non-2xx
///
/// example:
///     let body = http.get("https://api.example.com/users/1")
///
/// see also: http.post, http.client
fn http.get(url str, timeout ms = 30000) Net Time Fail[NetError] -> Response
```

Doc-комментарий имеет **структуру**, парсится компилятором,
проверяется на согласованность с сигнатурой. LLM использует его
как контекст — структурированный, не свободный текст.

### R5.7. Обратимость spec ↔ impl

Это **тулинг-возможность**, а не фича языка. Никаких новых синтаксических
конструкций — только описание workflow, который становится возможным
благодаря [R4](revolutionary.md) (контракты в сигнатуре),
[R5.2](revolutionary.md) (сигнатура = полное описание) и
[R5.3](revolutionary.md) (структурированные ошибки).

LSP/IDE Nova поддерживает **два направления генерации** между контрактом
и реализацией.

#### Направление 1: impl → spec

Программист пишет реализацию. LSP запрашивает у LLM сгенерировать
`requires`/`ensures` по коду. Программист подтверждает или редактирует
предложенные контракты. Принятые контракты становятся **частью кода** и
проверяются компилятором (статически где может, runtime в debug).

```nova
// программист написал:
fn withdraw(mut acc Account, amount money) Fail[Overdraft] -> () {
    if amount > acc.balance { throw Overdraft }
    acc.balance -= amount
}

// LSP предлагает дополнить:
fn withdraw(mut acc Account, amount money) Fail[Overdraft] -> ()
    requires amount > 0
    ensures result.is_ok || acc.balance == old(acc.balance)
    ensures result.is_ok ==> acc.balance == old(acc.balance) - amount
{
    if amount > acc.balance { throw Overdraft }
    acc.balance -= amount
}
```

Программист видит контракты, оценивает корректность, принимает или
правит. Это **review**, не доверие LLM на слово — но дешевле, чем
писать контракты с нуля.

#### Направление 2: spec → impl

Программист пишет **только** сигнатуру и контракты. Тело генерируется
LLM (через IDE-команду «Generate body»), компилятор проверяет
соответствие контракту. Цикл идёт до сходимости или ручного
вмешательства.

```nova
fn withdraw(mut acc Account, amount money) Fail[Overdraft] -> ()
    requires amount > 0
    ensures result.is_ok ==> acc.balance == old(acc.balance) - amount
    ensures result.is_err ==> acc.balance == old(acc.balance)
=>
    // [генерируется LSP]
```

LSP вызывает LLM, получает тело, **компилятор проверяет контракт**:

- Если контракт держится (статически или в debug-runtime) — ок.
- Если нарушен — ошибка возвращается LLM как обучающий сигнал ([R5.3](revolutionary.md)),
  итерация повторяется.

#### Никаких изменений в языке

Это **полностью** живёт в LSP/IDE. Нет директивы `@ai-impl`, нет
генерации в момент компиляции, нет зависимости билдов от LLM.
Воспроизводимость билдов сохраняется.

Что нужно от языка для работы этого workflow — **уже есть**:

- Контракты в сигнатуре ([R4](revolutionary.md))
- Структурированные ошибки компилятора ([R5.3](revolutionary.md))
- Локальность контекста ([R5.1](revolutionary.md)) — функция
  типечекается без всего проекта (`nova check --fragment`)
- Эффекты в сигнатуре ([R5.2](revolutionary.md)) — LLM знает, какие
  побочные действия разрешены

#### Что это меняет в экономике

Сейчас в индустрии написать функцию **с инвариантами** дороже, чем
**без**. Контракты пишут только для критичного кода. R5.7 переворачивает
экономику: **контракт пишется быстрее, чем тело**, потому что человек
описывает «что должно быть истиной», а LLM делает скучную часть.

Это смещает программирование от «писать код» к «описывать инварианты».
Близко к Dafny / F* / TLA+, но без специального языка спецификаций —
тот же Nova.

#### Где это работает и где нет

**Хорошо работает:**
- Чистые функции с понятным контрактом (parsing, validation,
  arithmetic)
- Функции с эффектами, где контракт описан в терминах входов/выходов
- Малые функции (< 50 строк)
- Функции с известным паттерном (CRUD, маршрутизация, форматирование)

**Плохо работает:**
- Большие stateful-функции с тонкими инвариантами над несколькими
  типами
- Функции с распределёнными эффектами, где контракт требует
  global reasoning (см. [R12](revolutionary.md))
- Функции, для которых SMT-проверка контракта не сходится за разумное
  время (см. ограничения SMT в [decisions/09-tooling.md#d24](decisions/09-tooling.md#d24))

#### Ограничения

1. **Нужна качественная LSP-интеграция.** Не каждый редактор её даст;
   стандартизация — вне языка.
2. **Контракт может быть неполным.** LLM сгенерирует тело, проходящее
   контракт, но делающее не то, что хотел программист. Защита — code
   review человеком, как обычно.
3. **Семантика контрактов через handler-state** — открытый вопрос.
   `ensures Db.balance(acc) == ...` — может ли SMT это проверить?
   См. [decisions/09-tooling.md#d24](decisions/09-tooling.md#d24).

#### Связь с другими решениями

- **Развивает [R4](revolutionary.md)** — контракты становятся
  утилитарным инструментом, а не теоретической надстройкой.
- **Использует [R5.3](revolutionary.md)** — структурированные ошибки
  как обучающий сигнал для LLM.
- **Опирается на [decisions/09-tooling.md#d24](decisions/09-tooling.md#d24)** — стратегия
  SMT-проверки контрактов.

---

## R6. Capability-режим для безопасной композиции

Функция может **запретить** определённые эффекты в своём скоупе:

```nova
fn run_user_script(code str) Fail -> Result =>
    forbid Net, Fs, Db {
        // внутри этого блока компилятор не позволит
        // вызвать ни одну функцию с эффектами Net, Fs, Db
        eval(code)
    }
```

Compile-time проверка работает на **прямых** эффектах вызываемых
функций. Если функция объявляет `Net` — её вызов внутри `forbid Net`
запрещён. Транзитивные эффекты ловятся не строго ([D62](decisions/04-effects.md#d62)) —
функция без `Net` в сигнатуре, но вызывающая `helper()` с `Net`,
не блокируется compile-time. Полная capability-sandbox-гарантия
достигается через **closure-границы** с явной декларацией allowed
эффектов и через project-level whitelist в `Nova.toml`.

Полезно для:
- Плагинов (со closure-параметрами фиксированной capability)
- Пользовательских скриптов (через project-whitelist)
- LLM-сгенерированного кода (закрепление эффектов на closure-границе)
- Детерминированных вычислений (запретить `Time`, `Random`, `Io`)

`Async` не запрещаемо — это ambient capability, не часть type
system'ы ([D62](decisions/04-effects.md#d62)). Если нужна
гарантия «функция не приостанавливается» — это runtime-флаг fiber-
runtime'а, не type-check.

---

## R7. Async — невидимая инфраструктура

В Nova функции могут приостанавливаться (network roundtrip, sleep,
channel.recv, async-Db) — но это **никак не выражается в типах**.
Цвета функции нет; нет деления на «sync» и «async». `await` keyword'а
тоже нет.

```nova
fn fetch(url str) Net -> Response => ...

fn handler(req Request) Net Db -> Response {
    let user = fetch_user(req.id)        // suspendable, но не в типах
    let posts = fetch_posts(user.id)
    Response.json(posts)
}
```

Тип возврата `Response`, не `Future<Response>`. В сигнатуре — только
эффекты которые программист **видит** как доступы к внешнему миру
(`Net`, `Db`); приостановка — деталь реализации.

Под капотом — **fiber-based scheduler** (как Go/Erlang/OCaml 5).
Когда операция эффекта приостанавливается, fiber кладётся в очередь
ожидания, scheduler берёт другой fiber. Программист не пишет ни
`async`, ни `await`, ни `Async`-эффекта в сигнатурах.

### Решение D62 — Async ambient capability

[D62](decisions/04-effects.md#d62) явно фиксирует: `Async` **не
является эффектом** в Nova. Не часть type system'ы. Это позволяет
backend-коду оставаться компактным — в реальном backend почти каждая
функция «может приостановиться», и явный эффект `Async` был бы шумом
без информативности.

### Сравнение с другими языками

|  | Rust async | Nova |
|---|---|---|
| Цвет функции | да (`async fn`) | нет |
| `await` нужен | да | нет |
| Тип возврата меняется | `Future<T>` | нет |
| Async в сигнатуре | да | **никогда** |
| Стоимость задачи | ~64 байта | ~4–8 KB (fiber stack) |
| Cancellation | ручная | structured |
| C-interop blocking | нет проблем | требует `detach to OS thread` |

Nova ближе к **Erlang/Go** по runtime: горутины/fiber'ы могут
вытесняться в любой точке, программист не пишет `async`. Платит
**памятью** (fiber stacks) ради **простоты кода**.

### Structured concurrency — отдельные примитивы языка

`spawn`, `supervised` (+ опц. `cancel:`), `select`, `parallel for`,
`detach`, `blocking` — **runtime-keyword'ы**; `race`, `with_timeout`
— **library-функции** поверх них. Не эффекты:

```nova
fn fetch_all(urls []str) Net -> []Response =>
    parallel for url in urls {
        fetch(url)
    }  // ждёт всех, отменяет хвост при ошибке

fn with_timeout[T](dur Duration, body fn() -> T) Fail -> T =>
    race {
        body(),
        sleep(dur).then { throw Timeout }
    }
```

Подробно — [decisions/06-concurrency.md#d14](decisions/06-concurrency.md#d14).

---

## R8. Time-travel debugging из коробки

Поскольку все эффекты проходят через handler'ы, **запись и повтор любого
запуска** — стандартная фича:

```bash
nova run --record trace.nrec ./server
# ... ловим баг

nova replay trace.nrec --step
# пошаговый repro с возможностью вернуться назад
```

Это даёт Erlang-уровень observability в любом приложении, без
специальной инструментации кода.

---

## R9. Compile-time supervision (Erlang-style)

Из эффектов следует встроенная structured concurrency с supervision:

```nova
fn server() Net Fail -> () =>
    supervised {
        spawn handle_requests()      // если упадёт — рестарт
        spawn periodic_cleanup()     // если упадёт — рестарт
        spawn metrics_reporter()     // если упадёт — рестарт стратегии one_for_one
    } strategy = one_for_one, max_restarts = 3
```

Erlang/OTP supervision — встроена в язык, без отдельного фреймворка.

---

## R10. Эффекты на границах: типизация, стирание, динамика

Статическая типизация эффектов **передаётся** в очереди, каналы и
планировщики. Это хорошо для типизированных пайплайнов и плохо для
разнородных задач. Решение — **три уровня**, программист выбирает:

**Уровень 1 — типизированный планировщик** (дефолт):
```nova
let order_queue Queue[fn(OrderId) Db Log Fail -> ()]
```

**Уровень 2 — явное стирание** (когда нужна разнородность):
```nova
fn erase[E](task fn() E -> ()) E -> fn() -> () {
    let captured = capture_handlers[E]()
    || with captured { task() }
}

universal_queue.enqueue(erase(send_email_task))
universal_queue.enqueue(erase(cleanup_db_task))
```

**Уровень 3 — динамические эффекты** (плагины, сериализация):
runtime-структура `EffectSet`, тип `DynFn`. Используется редко.

Подробно — [decisions/04-effects.md#d12](decisions/04-effects.md#d12).

---

## R11. Panic — что НЕ эффект

Не каждое прерывание вычисления — эффект. **Аппаратные/математические
сбои** (деление на ноль, переполнение, выход за границы массива, OOM,
переполнение стека) **не указываются в сигнатуре**:

```nova
// никакого Fail[DivByZero]
fn mean(xs []int) -> int =>
    xs.sum() / xs.len()
```

Они образуют категорию `Panic`. Программист **не ловит panic в коде** —
panic означает смерть текущего fiber'а, runtime обрабатывает на границе:

```nova
fn handle_request(r Request) Db Log -> Response =>
    process(r)             // panic → fiber умирает, runtime вернёт 500

fn server() Net Fail -> () =>
    supervised {
        spawn handle_requests()
    } strategy = one_for_one
    // supervisor рестартует упавшие fiber'ы
```

Иначе `Fail[DivByZero]` оказался бы в каждой второй сигнатуре —
информативность исчезла бы. Это **сознательный компромисс**, граница
проводится явно: «обработать никак нельзя, надо умереть» → Panic;
«обработать можно и нужно» → Fail.

Опциональный `@strict_total` — для критичного кода, превращает
функцию в тотальную (компилятор требует обработать все возможные
panic-источники). Подробно — [decisions/08-runtime.md#d13](decisions/08-runtime.md#d13).

---

## R12. Распределённые системы как handler-композиция

Это **не новая фича языка**. Это иллюстрация, что центральный тезис
[D10](decisions/01-philosophy.md#d10) («всё handler») масштабируется до распределённых
систем — без новых синтаксических конструкций. Retry, идемпотентность,
репликация, exactly-once, distributed tracing — всё получается как
**стек handler'ов** над эффектами `Db`, `Net`, `Fail`.

### Бизнес-логика не знает про распределёнку

Программист пишет обычную функцию с эффектами:

```nova
type TransferError | AccountNotFound(AccountId) | InsufficientFunds

fn transfer(from AccountId, to AccountId, amount money)
    Db Fail[TransferError] -> Receipt
{
    let src = Db.find(from) ?? throw AccountNotFound(from)
    let dst = Db.find(to)   ?? throw AccountNotFound(to)
    if src.balance < amount { throw InsufficientFunds }
    Db.exec(sql`UPDATE accounts SET balance = balance - ${amount} WHERE id = ${from}`)
    Db.exec(sql`UPDATE accounts SET balance = balance + ${amount} WHERE id = ${to}`)
    Receipt { from, to, amount, ts: Time.now() }
}
```

В сигнатуре — `Db` и `Fail`. **Никаких** `@Idempotent`, `@Replicated`,
`@Retry`, `@Trace`. Это бизнес-функция, и она такой остаётся.

### Распределённые свойства добавляются handler'ами

Каждое распределённое свойство — handler `Db`/`Net`, перехватывающий
операции и решающий, что с ними делать:

**1. Repaglication.** Handler `Db`, который рассылает write на N узлов
и читает локально:

```nova
fn replicated(nodes [Node], quorum int, real Effect[Db]) -> Effect[Db] => effect Db {
    query(q) => return real.query(q)    // чтения локальны
    exec(q) {                             // записи на все узлы
        let acks = parallel for node in nodes {
            node.exec(q)
        }
        if acks.count(Ok) < quorum { throw QuorumLost }
        return ()
    }
}
```

**2. Идемпотентность.** Handler, кеширующий результат по ключу:

```nova
fn idempotent_by(tx_id str, real Effect[Db]) -> Effect[Db] => effect Db {
    query(q) => return real.query(q)
    exec(q)  => match Cache.get(tx_id) {
        Some(cached) => return cached         // повтор — вернуть кеш
        None => {
            let result = real.exec(q)
            Cache.put(tx_id, result)
            return result
        }
    }
}
```

Второй вызов с тем же `tx_id` не выполнит SQL — отдаст кеш.

**3. Retry с backoff.** Handler `Net`, перехватывающий `Fail[NetError]`
и повторяющий вызов:

```nova
fn retry(max_attempts int, real Effect[Net]) -> Effect[Net, Response] => effect Net {
    get(url) {
        let mut attempt = 0
        loop {
            match try_fail[NetError] { real.get(url) } {
                Ok(resp) => interrupt resp           // IRT = Response
                Err(_) if attempt < max_attempts => {
                    Time.sleep(backoff(attempt))
                    attempt += 1
                }
                Err(e) => throw e
            }
        }
    }
    post(url, body) => /* аналогично */
}
```

**4. Exactly-once = idempotent + persistent log.** Композиция двух
handler'ов:

```nova
fn exactly_once(tx_id str, log PersistentLog, real Effect[Db]) -> Effect[Db] {
    let logged = with_log(log, real)              // пишет в WAL до Db
    idempotent_by(tx_id, logged)                  // и кеширует результат
}
```

WAL гарантирует, что операция не потеряется при падении; idempotent
гарантирует, что повтор не выполнит её дважды. **Композиция**, не
монолитная фича.

**5. Distributed tracing.** Handler `Trace` (уже в [R2](revolutionary.md)
стандартном наборе), оборачивающий каждую операцию в span:

```nova
fn traced(real Effect[Db]) -> Effect[Db] => effect Db {
    query(sql, args) => Trace.span("db.query", { "sql": sql }) {
        return real.query(sql, args)
    }
    exec(sql, args)  => Trace.span("db.exec", { "sql": sql }) {
        return real.exec(sql, args)
    }
}
```

### Композиция через `with`

Распределённые свойства **компонуются стеком** handler'ов:

```nova
with Db = traced(idempotent_by(tx_id, retry(replicated(nodes, 2, real_db)))) {
    transfer(alice, bob, 100)
}
```

Читается изнутри наружу: `real_db` → реплицируется → ретраится → делается
идемпотентным → трассируется. **Программист не пишет распределённую
логику — он её конфигурирует.** Один и тот же `transfer` работает
с любым набором handler'ов.

Замени `real_db` на `in_memory()` для теста — distributed свойства не
нужны, тест-handler даёт детерминированную БД. Замени `replicated` на
`single_node()` для dev-режима — без репликации, но retry и трассировка
остались. **Каждое свойство выключаемо независимо.**

### Сравнение с обычным стеком

| Свойство | Go + K8s + Istio + Temporal | Nova |
|---|---|---|
| Репликация | StatefulSet + Raft-библиотека + конфиг YAML | `replicated(nodes, 2, ...)` |
| Идемпотентность | Temporal workflow с idempotent activities | `idempotent_by(tx_id, ...)` |
| Retry с backoff | Istio retry policy + envoy конфиг | `retry(max_attempts, ...)` |
| Distributed tracing | OpenTelemetry SDK + Jaeger sidecar + сэмплинг конфиг | `Trace` handler |
| Circuit breaker | Hystrix-библиотека + конфиг | handler с `Fail[Tripped]` |
| Канарейный деплой | Istio VirtualService + traffic split YAML | handler, маршрутизирующий по `Random` |
| Exactly-once | Kafka transactional producer + Temporal | композиция `idempotent` + `persistent_log` |
| Тестирование без БД | testcontainers / mocks | `with Db = in_memory() { ... }` |

В обычном стеке распределёнка живёт **снаружи кода** — в YAML, sidecars,
конфигах CI/CD. Бизнес-логика связана с инфраструктурой через тонкие
неявные контракты (порядок вызовов, headers, request IDs). LLM, читая
функцию, **не видит**, какие свойства гарантированы. Программист, читая
YAML, не видит, какой код этим управляется.

В Nova распределёнка — это **`with`-блок**, видный в коде. LLM, читая
сигнатуру `transfer`, видит `Db Fail` — обычные эффекты. Читая
вызывающий код, видит handler-стек — все распределённые гарантии. Граница
между бизнес-логикой и инфраструктурой проходит **по handler'у**, не по
YAML-файлу.

### Что это даёт по AI-first тезису

LLM пишет `transfer` без знания, в каком окружении он запустится.
Один и тот же код работает в:

- **Тесте** — `with Db = in_memory() { transfer(...)? }`
- **Локальной разработке** — `with Db = postgres(local) { transfer(...)? }`
- **Staging** — `with Db = retry(traced(postgres(staging))) { transfer(...)? }`
- **Production** — полный стек

Бизнес-логика **не зависит** от среды. Это противоположность тому, что
требует Spring/FastAPI/Temporal — там бизнес-функция аннотирована
средой через декораторы и контейнеры, и LLM-сгенерированный код может
случайно «попасть в production-handler» из-за невидимой ассоциации.

### Пределы абстракции

Не всё распределёнка тривиально handler'ом. Открытые сложности:

- **Распределённый консенсус (Raft/Paxos).** `replicated` handler выше
  показан упрощённо — реальный консенсус требует state machine,
  логи, выборы. Это **stdlib-уровень**, не язык. Handler даёт **точку
  внедрения**, не саму реализацию.
- **Кросс-handler состояние.** `idempotent_by` хранит кеш — где он
  живёт? Это вопрос Q12 (модель concurrency и shared state). Без её
  решения handler'ы можно описать на уровне семантики, но не реализовать
  поверх многопоточного runtime.
- **Транзакции через границы handler'ов.** Если `Db` обернут в
  репликацию, а вокруг ещё `with Fail[NetError] = retry`, корректное
  взаимодействие транзакции и retry — нетривиально. Это known issue
  в Erlang/OTP supervision и в Temporal — не уникальная проблема Nova.

См. также [Q12 в open-questions.md](open-questions.md) — concurrency
model влияет на полноту реализации этих handler'ов.

### Связь с другими решениями

- **Развивает [D10](decisions/01-philosophy.md#d10)** — это иллюстрация центрального
  тезиса, не новая фича.
- **Использует [R1](revolutionary.md) handler-механизм** — без него
  ничего из этого невозможно.
- **Опирается на [R2](revolutionary.md) стандартные эффекты** — `Db`,
  `Net`, `Trace` — все уже определены.
- **Поддерживает [R5](revolutionary.md) AI-first** — видимость
  распределённых свойств в коде, а не в YAML.

---

## Что вместе делает Nova революционной

Каждая отдельная идея существует в каком-то языке. Уникально:

1. **Все они следуют из одной центральной абстракции** — алгебраических
   эффектов с handler'ами. Не «коллекция фич», а **одна идея с
   развёртыванием**.
2. **Заявка на killer use-case** — AI-first программирование с
   верифицируемым кодом от LLM. Этого не делает никто целенаправленно.
3. **Эффекты делают LLM-генерируемый код безопасным**, потому что
   побочные действия видны в типе, а capability-режим даёт compile-time
   песочницу.
4. **Один язык покрывает спектр** от скрипта до верифицированного кода
   через градиент контрактов.
5. **Time-travel debugging, supervision, тесты без моков, async без
   вируса** — следствия, а не отдельные фреймворки.

Главный тезис, заменяющий прежний:

> **Nova — это язык, в котором LLM может писать код, который человек
> может доверять, потому что эффекты делают всё видимым, контракты
> делают всё проверяемым, а handler'ы делают всё тестируемым.**

---

## Главные риски (повторено из [decisions/01-philosophy.md → D10](decisions/01-philosophy.md#d10))

1. Algebraic effects — задача переднего края PL. Реализация сложна.
2. Сообщения компилятора про эффекты должны быть понятны Java-программисту
   за день, иначе язык мёртв.
3. Performance overhead эффектов нужно прибить агрессивной оптимизацией.
4. Ставка на AI-кодинг как доминирующий тренд — статистически вероятна,
   но не гарантирована.
5. Fiber-runtime платит памятью — миллиарды задач не работают, миллион
   работает.
6. 9 из 10 таких проектов проваливаются.
