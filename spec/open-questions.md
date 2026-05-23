# Nova — открытые вопросы дизайна

Что обсуждали, но не зафиксировали как решение. Когда вернёшься
к работе над языком — сначала закрой эти вопросы.

---

## Q1. Унификация методов типов и эффектов

**Контекст.** Сейчас (после [D35](decisions/03-syntax.md#d35)):

```nova
fn User @greet() -> str => ...       // метод инстанса: неявный self через @
fn list_users() Db -> []User => ...  // функция с эффектом: handler в скоупе
```

D35 ввёл `@`-методы (неявный `self`), но **только для типов данных**.
Для эффектов остался отдельный синтаксис «обычная функция + эффект в
сигнатуре». Это два разных способа объявить «функцию, ассоциированную
с типом/эффектом».

**Предложение.** Распространить `@` на эффекты:

```nova
fn User @greet() -> str => ...
fn Db @list_users() -> []User => ...   // self = активный handler в скоупе
```

Один синтаксис, два способа доступа к `self`:
- Для data-типа — экземпляр через `u.greet()`
- Для эффект-роли — активный handler через `Db.list_users()` (или
  `db.list_users()` если есть локальное имя)

Эффект в сигнатуре автоматически — через `@` (не нужно дублировать).

**Статус.** D35 закрыл часть про data-типы. На эффекты не распространён —
вопрос остаётся открытым.

**Тонкие места:**
1. Неявный `self` для эффектов — это «магия» или унификация?
2. Куда отнести функции с несколькими эффектами (`Net Db Log Fail`)?
   `fn (Net, Db, Log) @method` — некрасиво, и `self` не один.
3. Переписывание stdlib — но stdlib ещё нет, дешёвое время.

---

## Q2. Tuple-namespace `(Db, Log).method`

**Контекст.** Был вопрос: «можно ли `(Db, Log).list_users()` как
сокращённый `with`?»

**Решение в обсуждении.** Отвергнуто (не даёт ничего сверх `with`-блока,
дублирует информацию из сигнатуры).

**Не зафиксировано** в decisions/ явно. Если возникнет снова —
сослаться на это обсуждение.

---

## Q3. Реализация fiber stack для `Async`

> **Bootstrap-status (2026-05-06):** временное решение — minicoro
> (mco) с **fixed-size growable stacks**. Размер по умолчанию — то,
> что minicoro даёт ас default'ом. Этого достаточно для bootstrap-
> сценариев (тесты до 64 fiber'ов в `NOVA_SCOPE_CAP`).
> **Production-выбор остаётся открытым** — список ниже.

**D14 говорит «открытый вопрос»:**
- Segmented stacks (как старый Go)?
- Cactus stacks (как Cilk)?
- On-demand growable (как новый Go)?

Каждый имеет цену:
- **Segmented:** дешёвый старт, hot-spot на границе сегмента
- **Cactus:** хороший для work-stealing, сложнее реализация
- **Growable:** выделяет много vsmem заранее, копирование при росте

**Дефолтный размер fiber stack** тоже не определён. Erlang начинает
с 233 слов (~2KB), Go — с 8KB. Для Nova нужно решить, ориентируясь
на основной use-case (серверы? embedded? AI-генерация?).

---

## Q4. Семантика `Alloc[Cycle]` collector'а — ✅ ЗАКРЫТО

**Закрыто [D6](decisions/05-memory.md#d6).** D21 (`~T`/`~&T` opt-in
cycle collection) отменён в пользу tracing GC по умолчанию. Эффект
`Alloc[Cycle]` снят, префиксы `~T`/`~&T` удалены из языка.
Управление collector'ом — runtime-параметр, не часть языка.

---

## Q5. Точная граница `Panic` (частично закрыто)

**D13 определил Panic как «аппаратные/математические сбои»:**
- Деление на ноль ✓
- Целочисленное переполнение (debug — panic, release — wrapping) ✓ закрыто D13
- Выход за границы массива ✓
- Переполнение стека — открыт (Q5.2 ниже)
- OOM ✓ закрыто D13 (Panic, fiber умирает; supervisor может рестартовать)

**Остаются открытыми:**

**Q5.2. Stack overflow recoverable?** Может ли быть не-panic,
обрабатываемая ошибка? Или всегда смерть fiber'а? Erlang restart,
Java `StackOverflowError`. Nova должна заявить позицию — скорее всего
«fiber умирает, supervisor рестартует» по аналогии с OOM, но не
зафиксировано явно.

**Q5.4. Assertion failures в debug.** Это `Panic` или обычные `Fail`?
Если `Panic` — нельзя поймать в обычном коде (только supervisor видит).
Если `Fail[AssertionError]` — нужно везде декларировать. Скорее
всего `Panic` (это «сбой инварианта», не бизнес-ошибка), но не
зафиксировано.

---

## Q6. Effect polymorphism — синтаксис + rank-2 семантика

**Текущий пример:**
```nova
fn map_eff[T, U, E](xs [T], f (T) E -> U) E -> [U]
```

`E` — параметр-эффект. Не определён точный синтаксис:
- Можно ли `E1, E2`? `[E1, E2]`?
- Как ограничивать (bound) эффект-параметры?
- Как стирать (`erase`) для разнородных задач — D12 коснулся, но
  не всё детализировано.

### Rank-2 polymorphism в handler-method'ах

[D42 модель B](decisions/02-types.md#d42) (через D53) допускает
generic в методах protocol/effect:

```nova
type Db effect {
    in_transaction[T](body fn() Db Fail -> T) Fail -> T
}
```

Здесь `T` — generic метода (rank-2): один и тот же handler `Db`
вызывает `in_transaction` с разными `T` для каждого вызова. После
[D61](decisions/04-effects.md#d61) (tail-only семантика, без resume)
**не специфицировано**, как handler управляет произвольным `T`:

- `body` возвращает `T` — handler-method получает `T` через вызов
  `body()`. Может ли handler решить «вернуть T или другое»?
- Если handler делает `interrupt v` — типы `v` и `T` должны быть
  совместимы. Как это проверяется при rank-2?
- SMT-проверка контрактов на `body()` — возможна ли она на rank-2-T?

Использовано в [orm_demo.nv](../examples/orm_demo.nv) и
[stdlib_sql.nv](../examples/stdlib_sql.nv) — компилятор должен
поддержать раньше чем self-hosted production.

**Связь:** [D42 (REVISED)](decisions/02-types.md#d42),
[D53](decisions/02-types.md#d53), [D61](decisions/04-effects.md#d61),
[D12](decisions/04-effects.md#d12), Q-bounds.

---

## Q7. Macros / comptime

В overview.md сказано: «typed compile-time функции (как Zig comptime)».
Но конкретный синтаксис, мощь, ограничения — не описаны.

**Вопросы:**
- Comptime-функции имеют доступ к типам как первый класс?
- Можно ли генерировать код во время компиляции?
- Reflection во время компиляции — да или нет?
- Custom DSL через comptime — допускается?

---

## Q8. Обновить `paradigm.md` и `revolutionary.md`

**Технический долг.** Эти файлы содержат **устаревший** синтаксис из
ранних этапов:
- `trait` / `impl Trait for Type` — отменено в D15
- `type X = { ... }` с `=` для record — отменено в D17
- `type X = { методы }` для интерфейсов — заменено на `protocol X { методы }` в D42
- `effect X { ... }` — отменено в D18
- match с `->` — отменено в D19
- `mut self`, `:` в аннотациях типа, `throws` без `Fail[]` — устарели

**Что делать:** обновить под текущие D-решения. Это просто переписать
все примеры с новым синтаксисом, без изменения смысла.

`syntax.md`, `effects.md`, `decisions/01-philosophy.md` (D9) актуализированы. `audit.nv`
тоже актуален. `paradigm.md` помечен как устаревший до полной переписи.

---

## Q9. Стандартная библиотека

Не описана структура. Что есть в stdlib:
- `String`, `HashMap`, `HashSet`, `Option`, `Result` — очевидно
  (Vec нет — `[]T` встроенный, см. D58)
- `LinkedList`, `Tree`, `Graph` — какие именно типы?
- `Json`, `Sql.builder` — упоминаются в `audit.nv`, не описаны
- `Time`, `Random`, `Net`, `Db` — стандартные эффекты, не определены
  их операции
- HTTP, WebSocket, gRPC — что в core, что в external?

### Конкретные пробелы методов

**String API** не зафиксирован. В `nova_tests/` и `examples/`
используются `s.len`, `s.contains(sub)`, `s.starts_with(p)`,
`s.ends_with(p)`, `s.to_lower()`, `s.to_upper()`, `s.trim()` — все
работают в `compiler-codegen` runtime, но это **implementation-факт**,
не часть спеки. Production-компилятор должен ориентироваться на
формальный список.

**Escape sequences** в string literals (`"\n"`, `"\t"`, `"\""`,
`"\\"`) — в bootstrap'е поддержаны, в спеке упоминаются только
для tagged templates ([D48](decisions/03-syntax.md#d48):1510),
не для обычных `"..."`-литералов. Нужно явно зафиксировать список
поддерживаемых escapes для обычных string-литералов.

**Array API** — `xs.len`, `xs.push(v)`, `xs.pop()`, `xs.get(i)`
работают в bootstrap'е, не описаны в спеке. `xs.map(f)`,
`xs.filter(p)`, `xs.reduce(...)`, `xs.find(p)` — НЕ работают
(в bootstrap'е) и НЕ описаны.

Это **большая** работа на отдельный документ.

---

## Q10. Tooling детали

Заявлены в overview.md:
- `nova run`, `nova build`, `nova fmt`, `nova lint`, `nova test`
- `nova check --fragment`
- `nova run --record` / `nova replay` (time-travel)

Но не описаны:
- Формат `.nrec` файлов трасс
- Структура package manager (content-addressed как Deno)
- Hot reload — как именно работает?
- LSP протокол — расширения для эффектов? Для контрактов?

---

## Q11. Имя языка

«Nova» — рабочее имя. Конфликтует с:
- Battle.net Nova (игровой движок Activision)
- Various JS/Python/Ruby библиотеки
- Команда Linux `nova` (OpenStack compute)
- Nova Networks (компания)

Если язык будет реально публиковаться — нужно другое имя.

---

## Q12. Модель concurrency — ЗАКРЫТО ([D50](decisions/06-concurrency.md#d50))

[D50](decisions/06-concurrency.md#d50) закрыл основные пункты Q12:

- **Q12.1 (spawn-семантика)** → `spawn` только в structured-scope;
  fire-and-forget — через `detach { ... }` с эффектом `Detach`.
- **Q12.2 (Async vs Par)** → один эффект `Async`, `Par` не вводится.
- **Q12.6 (C interop)** → `blocking { ... }` примитив + эффект
  `Blocking` (несовместим с `Realtime`).
- **`await`/маркер на call site** → нет, эффект `Async` в сигнатуре
  единственная декларация (подтверждение D14).

Двухэтапный план реализации (Go-style v1.0, Erlang-style v2.0+) —
там же.

**Остаётся открытым в Q9 (stdlib):**

- Точные API `Channel[T]`, `Mutex`, `RwLock`, `Atomic[T]`.
- Размер blocking-pool по умолчанию (runtime-конфиг).
- Q12.7 — `Domain`-примитив (явная граница ОС-потока) для real-time
  embedded use-case, отложено до user-feedback.

Исторический контекст подвопросов сохранён ниже на случай возврата.

### Q12.1. Семантика `spawn`

В `examples/audit.nv:256` используется `spawn write_audit(...)` как
fire-and-forget вне `supervised`/`parallel` блока. Это противоречит
structured concurrency: задача переживает родителя, отмена не
прорастает. Варианты:

1. **`spawn` всегда внутри scope'а** (`supervised`, `parallel`,
   `nursery`-блок). Unstructured нет вообще.
2. **`spawn` структурный, `detach` отдельный** для долгоживущих задач.
   `detach` требует явного эффекта `Detach` или capability.
3. **Текущее поведение `audit.nv`** — `spawn` fire-and-forget, scope
   неявно «модуль/процесс». Это удобно, но ломает structured concurrency.

Нужно решение. Влияет на cancellation, на семантику ошибок,
на supervision.

### Q12.2. Граница `Async` vs `Par`

Сейчас оба эффекта в стандартном наборе, но границы не описаны:

- `Async` = «функция может уступить fiber-scheduler'у»?
- `Par` = «функция запускает несколько fiber'ов параллельно»?
- Должны ли они комбинироваться (`Net Async Par` для `parallel for`
  с сетью)?
- `parallel for` требует `Par` или достаточно `Async`?
- Может ли быть `Par` без `Async` (например, чисто CPU-bound параллелизм)?

Текущие примеры противоречат: `audit.nv:222` пишет `Async` без `Par`
для middleware, который spawn'ит задачу — то есть эффект параллелизма
не виден в сигнатуре. Это дыра в «сигнатура = полное описание».

### Q12.3. Multithreading vs concurrency

Не зафиксировано: fiber'ы работают на **одном ОС-потоке**, на
**M-потоках с work-stealing** (Go/Tokio), или на **отдельных Domain'ах**
(OCaml 5)? Это влияет на:

- Производительность CPU-bound кода (один поток — bottleneck)
- Сложность реализации (M:N сложнее)
- Семантику shared state (см. Q12.4)

OCaml 5 разделяет `Domain` (ОС-поток с собственной кучей) и `Fiber`
(легковесная задача внутри Domain). Это явное двухуровневое разделение.
Nova могла бы:

1. **Один scheduler на процесс, M-потоков** (Go-style) — простая модель,
   но shared state требует синхронизации.
2. **`Domain` как явный примитив** — изоляция кучи, передача данных
   через каналы. Сложнее, но безопаснее.
3. **Single-threaded по умолчанию, multi-thread opt-in** — упрощает
   `~T` и `~&T`, но ограничивает параллелизм.

### Q12.4. Shared state между fiber'ами

Совсем не описано. Если два fiber'а должны делить состояние:

- **Channels** (Go, Erlang-style)? Тип `Channel[T]`, операции
  `send`/`recv` как эффекты?
- **Actor mailbox** (Erlang)? Каждый fiber имеет inbox?
- **Mutex/RwLock как обычные типы**? Тогда чем они лучше Rust?
- **Atomic-операции**? Какой тип `Atomic[T]`?
- **Software Transactional Memory** (Haskell/Clojure)?

В контексте D10 «всё нечистое — эффект» естественный ответ — **channels
как handler эффекта**, но это не зафиксировано. Без этого решения
нельзя описать `Mut` для multi-fiber случаев.

### Q12.5. ~~Thread-safety `~T` и `~&T`~~ — СНЯТО

**Статус: снято.** [D21](decisions/05-memory.md#d21) отменён в пользу managed memory
([D6 пересмотрен](decisions/05-memory.md#d6)). Префиксов `~T`/`~&T` нет, atomic
refcount не нужен. GC сам обеспечивает thread-safe управление памятью.

Заменяющий вопрос — **thread-safety GC**: как concurrent collector
синхронизируется с приложением? Это **runtime-implementation deal**,
не language design. Современные решения (ZGC, Shenandoah, MMTk) уже
дают ответы — выбирается на этапе реализации.

### Q12.6. C interop для блокирующих вызовов

D14:665 упоминает «механизм `detach to OS thread`», но не описывает.
Если fiber вызывает синхронный C-код (например, `read(2)` без
O_NONBLOCK), он блокирует весь scheduler. Нужен механизм:

- Явный `detach { c_call() }` блок, runtime передаёт fiber на отдельный
  ОС-поток?
- Эффект `Blocking` в сигнатуре C-обёртки?
- Pool блокирующих потоков для всех `detach`?

### Что блокируется без Q12

- **Q9 (stdlib).** Нельзя описать `Channel`, `Mutex`, `Atomic`,
  `Mailbox` без модели shared state.
- **AI-first тезис.** Без `Par` в сигнатуре `audit_middleware` LLM не
  видит, что функция параллельна. Сигнатура не полна.
- **Реализация GC ([D6](decisions/05-memory.md#d6)).** Concurrent collector работает
  на отдельном потоке параллельно с приложением — выбор реализации
  зависит от Q12.3.
- **Эффекты на границах (D12).** Очереди и планировщики типизированы,
  но «передача задачи между потоками» — это та же граница, что
  «передача через процесс»? Не описано.

### Приоритет

**Высокий.** Это структурный вопрос уровня D6/D14, не «деталь runtime».
Влияет на сигнатуры stdlib, AI-first тезис и память.

### Прагматичный план: Go-style v1.0, Erlang-style v2.0

После обсуждения — выбран **двухэтапный план**:

**v1.0 — Go-style fibers + shared memory:**
- Fiber'ы как goroutine: shared heap, передача данных по указателю
- Каналы как stdlib (`Channel[T]` с `send`/`recv`)
- `Mutex`, `RwLock`, `Atomic[T]` как stdlib-типы (Q12.4)
- Один scheduler на процесс, M ОС-потоков work-stealing (Q12.3.1)
- Atomic refcount для `~T`/`~&T` всегда (Q12.5.1) — простая модель,
  цена ~10ns
- Preemptive scheduling — runtime прерывает fiber'ы по таймеру или
  через compiler-вставленные точки (как Go 1.14+)
- Supervisor как библиотечный паттерн поверх panic = exit fiber (D13)

**v2.0+ — Erlang-style isolation (опционально, если докажет ценность):**
- Per-fiber heap (изолированная куча, как Erlang process)
- Per-fiber GC (микросекундные паузы локально)
- Передача между fiber'ами только через каналы с копированием
- Полная изоляция падений
- Hot reload, distributed-by-default

Erlang-модель **сильнее** (изоляция, supervision, distributed бесплатно),
но **сложнее реализовать** (per-process heap + per-process GC). Для v1.0
Nova не берёт эту планку — Go-модель достаточна для backend-серверов и
CLI-приложений, и хорошо сочетается с [D6](decisions/05-memory.md#d6) (managed memory
с concurrent GC).

### Что зафиксировано из v1.0-плана

- **Preemptive fiber-runtime** — обязательно (исключает «cycle in plugin
  останавливает весь сервер»)
- **Shared heap с concurrent GC** — единая куча на процесс, GC снимает
  вопрос refcount/atomicity для shared ownership ([D6 пересмотрен](decisions/05-memory.md#d6))
- **Channels как stdlib** — описание в Q9 (stdlib)
- **C interop через `detach`** — Q12.6, фиксируется как stdlib-функция
  с эффектом `Blocking` в сигнатуре
- **Supervisor — библиотечный паттерн** — поверх runtime-границы fiber'а

### Что остаётся открытым

- Точные API `Channel[T]`, `Mutex`, `Atomic[T]` — Q9
- Граница `Async` vs `Par` (Q12.2) — Q9 stdlib опишет
- Семантика `spawn` структурный/unstructured (Q12.1) — Q9 определит
- Q12.7 (новый): следует ли вводить `Domain`-примитив (явная граница
  ОС-потока) для real-time embedded use-case — отложено до user-feedback

---

## Q13. Версионирование типов данных как stdlib-паттерн

**Идея.** Эволюция типов через sum-type вариантов + методы преобразования
+ handler `Db`/`Fs`, знающий о версиях:

```nova
type Account
    | V1 { id u64, balance money }
    | V2 { id u64, balance money, frozen bool }
    | V3 { id u64, balance money, frozen bool, currency Currency }

fn Account.to_latest(self) -> Account => match self {
    V1 { id, balance }                 => V2 { id, balance, frozen: false }.to_latest()
    V2 { id, balance, frozen }         => V3 { id, balance, frozen, currency: USD }
    v3                                 => v3
}
```

Handler `Db` при чтении применяет миграцию прозрачно, возвращая latest
версию.

**Почему не D-решение.** Реальная сложность миграций (DDL, блокировки,
конкурентные writers, big-rollback) — снаружи кода. Любой язык с sum-types
даёт ту же выразительность. Ввод `evolution` как ключевого слова
противоречил бы D18 («не плодить специальные сущности») и не давал бы
ничего сверх библиотечного решения.

**Когда вернуться.** При работе над Q9 (stdlib) — описать как рекомендуемый
паттерн для типов, читаемых из persistent storage, вместе с операциями
`Db` handler'а.

**Связь.** Q9, D10, D18.

---

## Q14. Cost-types — resource bounds в сигнатуре

**Идея.** Опциональный контракт о сложности:

```nova
fn sort[T](xs [T]) -> [T]
    requires bounded(time: O(n log n), space: O(1))
=> ...
```

Проверяется статически где можно (RAML / AARA подходы).

**Почему отложить.** Research-уровень. Nova и так признаёт высокую планку
реализации эффектов ([decisions/01-philosophy.md → D10](decisions/01-philosophy.md#d10)). Брать ещё одну
рискованную ставку до v1.0 — превышение допустимого риска.

**Когда вернуться.** После стабилизации D10/D14/D21 и реализации R5
в проде.

**Связь.** R4 (контракты), R5 (AI-first), D10.

---

## Q15. Enum с числовыми значениями ✅ ЗАКРЫТО ([D52](decisions/02-types.md#d52))

[D52](decisions/02-types.md#d52) ввёл sum-варианты с числовыми
discriminants и auto-increment:

```nova
type ExitStatus | Ok | Failure | Critical                  // 0, 1, 2 (auto)
type FileMode | Read = 1 | Write | Execute                 // 1, 2, 3
type ErrorCode
    | NotFound       = 404
    | Unauthorized   = 401
    | InternalError  = 500
type Bit u8 | Off = 0 | On = 1                             // явный базовый тип
```

Cast `Sum → int` безопасный (`c as int`); cast `int → Sum` через
pattern match с `Fail[InvalidVariant]`. Конфликт значений запрещён
компилятором.

Закрывает все use-case'ы исходного Q15:
- Привязка численного значения — через `= n`.
- Автонумерация — через auto-increment от первого варианта.
- Сериализация в wire-формат — через `as int` (стабильный
  discriminant).

---

## Q16. Bitflags ✅ ЗАКРЫТО (D46)

> С введением [D46 (operator overloading)](decisions/03-syntax.md#d46) вопрос
> закрывается **Вариантом C** (newtype над int с методами `@or`, `@and`):
>
> ```nova
> type Permission(int)
> const READ    Permission = Permission(1)
> const WRITE   Permission = Permission(2)
> const EXECUTE Permission = Permission(4)
>
> fn Permission @or(other Permission) -> Permission =>
>     Permission(@.0 | other.0)
>
> fn Permission @and(other Permission) -> Permission =>
>     Permission(@.0 & other.0)
>
> fn Permission @contains(flag Permission) =>
>     (@ & flag).0 != 0
>
> let p = READ | WRITE
> if p.contains(READ) { ... }
> ```
>
> Типобезопасность сохранена, оператор `|` работает через `@or`.
> Stdlib `Bitflags[T]` (Вариант A) **не нужен**.

**Контекст.** Permissions, capabilities, set-of-options — частый паттерн:

```
Read | Write | Execute
HTTP_GET | HTTP_POST
INTR_HOLD | INTR_LATCH
```

Это **не sum-type** (sum-type = один из вариантов, bitflags = комбинация).
Sum-type для них не подходит. Нужен либо `int` с константами и битовыми
операциями (как в C), либо специальный тип `Bitflags[T]`.

В Nova **никак** — нет ни `int`-констант с битовыми операторами как
идиомы, ни `Bitflags`-типа.

### Варианты

**A. Stdlib `Bitflags[T]`** — generic-тип над enum-подобным sum-type:

```nova
type Permission | Read | Write | Execute
let p Bitflags[Permission] = Permission.Read | Permission.Write
if p.contains(Permission.Read) { ... }
```

Требует перегрузки операторов `|`, `&` для конкретного типа. См. [D1](decisions/01-philosophy.md#d1)
— перегрузка операторов «только для стандартных traits» (намёк, что для
custom-типов нельзя). Нужно явно расширить.

**B. Goлевой стиль `int` + константы**:

```nova
let PERM_READ    = 1
let PERM_WRITE   = 2
let PERM_EXECUTE = 4
let p = PERM_READ | PERM_WRITE
```

Работает уже сейчас (int + битовые операторы), но теряется типобезопасность —
`p` имеет тип `int`, не `Permission`.

**C. Newtype над int** — `type Permission(int)`, методы `.has(...)`,
`.with(...)`. Безопасно, но многословно, и `|` не работает без
operator overloading.

### Приоритет

**Средний.** Нужно для backend (HTTP, БД, ОС-вызовы). Решение зависит
от того, есть ли в Nova operator overloading для custom-типов.

**Связь.** Q15 (если оба решаются через derive-макросы), D1 (перегрузка
операторов).

---

## Q17. Bootstrap-язык компилятора — Rust

**Решение из обсуждения:** первый компилятор Nova (v0.1–v1.0) пишется
на **Rust**. После self-hosting (v2.0+) переписывается на Nova.

**Почему Rust:**
- Лучшая экосистема для компиляторов (LLVM через `inkwell`, парсеры
  через `chumsky`/`logos`, AST через native sum-types)
- Pattern matching, sum-types, traits — естественны для PL-кода
- LLM знает Rust очень хорошо — AI-codegen качество максимальное
- Прецедент массовый: Roc, Gleam, Carbon, Mojo, Grain — все на Rust
- Концептуальная близость к Nova (`~T`/`&T`/мономорфизация/sum-types
  идейно срисованы с Rust)

**Почему не C:**
- AST на C через `enum + union` — в 3-5 раз больше кода + ручная память
- Нет exhaustiveness check для switch
- LLM хуже на C-компиляторных задачах
- Memory bugs съедят 30% времени разработки

**Почему не OCaml:**
- В 1.5x короче Rust для компилятора, но LLM знает хуже
- LLVM bindings слабее
- Меньше потенциальных контрибьюторов

**Это не D-решение.** Это **выбор реализации**, как SMT-движок в
[D24](decisions/09-tooling.md#d24). В дизайн-документе языка не фиксируется — может
измениться в зависимости от инструментов и команды.

**Связь.** D24 (по аналогии — выбор реализации, не дизайна).

---

## Q19. ЗАКРЫТО (2026-05-10) — Trailing-block синтаксис `expr { x => body }`

**Статус: закрыто.** Решено ревизией Closure-rev (Plan 19,
[D43 rev](decisions/03-syntax.md#d43-trailing-block--без-params-fnp-body-с-params)):

- **Trailing-block** (`f(args) { block }`) — разрешён **только** для
  callback'ов **без параметров** (DSL-форма: `with_timeout`, `retry`,
  `transaction`, `region`, `supervised`).
- **Trailing-fn** (`f(args) fn(p) body`) — для callback'ов с params,
  идентично closure-full без имени.
- Старая форма `f(args) { x => body }` (с параметрами через `=>`)
  **отменена**.
- Ответ на исходный вопрос Q19: «общий механизм», но в двух чётких
  формах для разных случаев — `{ block }` для no-params, `fn(p) body`
  для with-params.

Вопрос остаётся в файле как **исторический контекст** — показывает
эволюцию от Kotlin-style `{ x => body }` к Rust-style разделению.

---

## Q19 (исторический контекст). Trailing-block синтаксис `expr { x => body }` — общий механизм или фиксированные примитивы?

**Контекст.** В коде Nova используется паттерн «функция/конструкция +
блок в качестве последнего аргумента»:

```nova
race {
    body(),
    sleep(dur).then { throw Timeout }    // .then { ... } — trailing block
}

with_timeout(2.seconds) {                 // trailing block
    Db.exec(sql`UPDATE counters SET v = v + 1`)
}

supervised {                              // trailing block
    spawn handle_requests()
}

with Db = real_db {                       // блок после with
    transfer(alice, bob, 100)
}

region {                                  // блок region (D6)
    let buf = []f32.with_capacity(1024)
    buf.to_owned()
}
```

**Не зафиксировано:** это **специальный синтаксис языка** для structured
concurrency / scope primitives, или **общий механизм** «вызов функции
с блоком как последним аргументом» (как Swift/Kotlin trailing closure)?

### Варианты

**A. Только зафиксированные примитивы языка.** Список конструкций с
блоком фиксирован: `with`, `supervised`, `region`, `parallel for`,
`race`, `select`, `with_timeout`, `try_panic` (отменён) — каждая
описана в D-решениях. Программист **не может** делать `expr { x =>
body }` для своих функций.

**Плюсы:**
- Парсер однозначен — `{` после имени = блок только для известных
  конструкций
- AI-first: LLM видит конкретные конструкции, не путает с лямбдами
- Минимум синтаксической поверхности

**Минусы:**
- Расширение языка требует D-решения
- Меньше гибкости для библиотек

**B. Общий trailing-closure механизм** (Swift/Kotlin/Ruby стиль).
Любая функция, последний параметр которой — функция, может быть вызвана
с блоком вместо круглых скобок:

```nova
fn with_lock[T](lock Mutex, body fn() -> T) -> T => ...

with_lock(my_mutex) {                     // trailing block
    do_work()
}
```

**Плюсы:**
- Унифицирует язык: `parallel for`, `with_timeout`, custom `with_lock` —
  всё одна форма
- Библиотеки могут создавать DSL-подобные конструкции
- Прецедент Swift, Kotlin, Ruby

**Минусы:**
- Парсер сложнее — `{` после идентификатора может быть и блок-литерал,
  и trailing closure
- AI-first: дублирование с обычной лямбдой `f((x) => ...)`
- Скрытое поведение: что значит `obj.method { x => ... }`?

**C. Гибрид — ключевые слова и whitelist'ed функции.** Зафиксированные
примитивы (вариант A) + явный список stdlib-функций, которые принимают
trailing block (`with_lock`, `with_resource`, `with_timeout`).
Пользовательские функции — только через обычный вызов с лямбдой.

**Плюсы:**
- Гибкость для stdlib без обобщения
- Парсер всё ещё знает, что является блоком

**Минусы:**
- Список нужно поддерживать
- Грамматика менее однородна

### Моё предложение

**Вариант A — только зафиксированные примитивы языка.** Причины:

1. **AI-first**: один способ передать closure — обычный аргумент
   `f((x) => body)`. Не два способа делать одно.
2. **Парсер однозначен**: `{` после `with`/`supervised`/`region`/`race`/
   `parallel`/`select`/`with_timeout`/`try` — это блок, не record/
   handler-литерал. Иначе грамматика становится контекстно-зависимой.
3. **Принцип «не плодить специальные сущности»** ([D17](decisions/02-types.md#d17),
   [D18](decisions/04-effects.md#d18), [D22](decisions/03-syntax.md#d22)) — trailing-closure это
   ещё одна синтаксическая роль `{`, которая увеличивает поверхность.

`.recover { err => ... }` в [examples/audit.nv](examples/audit.nv) был
**ошибкой** — заменён на handler `with Fail[E] = |err| ... { ... }`.

### Приоритет

**Низкий.** Сейчас все нужные конструкции (`with`, `supervised`,
`region`, `parallel for`, `race`, `select`, `with_timeout`) зафиксированы
как **примитивы языка**. Если возникнет реальный use-case для общего
trailing-closure — пересмотреть. Пока — нет.

**Связь.** D14 (structured concurrency primitives), D22 (лямбды).

---

## Q18. ЗАКРЫТО — Cycle-detection больше не актуален

**Статус: закрыто.** [D21](decisions/05-memory.md#d21) отменён в пользу managed memory
([D6 пересмотрен](decisions/05-memory.md#d6)). Циклы освобождаются автоматически
concurrent GC, никаких compile-time ошибок цикла, никаких suggestion'ов
о weak-направлении не нужно.

Вопрос остаётся в файле как **исторический контекст** — показывает,
почему мы отказались от opt-in cycle collection. Дальнейшее тело
сохранено, но **не актуально** для текущего дизайна.

См. [D6 (новая версия)](decisions/05-memory.md#d6) и discussion-log этап 13.

---

### Историческое тело (не актуально)

**~~Контекст.~~** [D21](decisions/05-memory.md#d21) фиксирует: цикл через `~T` без
`~weak` — compile error с suggestion. Это **уже работает** на уровне
дизайна. Открытый вопрос — **качество** этих сообщений и наличие
lint-режима для поиска циклов в большом проекте.

### Зачем нужно

Программисты регулярно создают потенциальные циклы — особенно при
рефакторинге. Сейчас в [D21 (отменено)](decisions/05-memory.md#d21-отменено-opt-in-cycle-collection)
зафиксирован формат ошибки:

```
error: cycle possible in `~T` references between Node and Edge
   suggestion: use `~weak` for one direction, or `~&T` to enable cycle collection
```

**Этого недостаточно** для AI-first языка. LLM получает короткое
сообщение, не видит **варианты**, не знает **какую сторону цикла**
сделать слабой. Человек тоже теряется.

### Что улучшить

**1. Расширенный формат ошибки с тремя вариантами:**

```
error: cycle in `~T` references: Tree → children → Tree → parent → Tree

  options:
    1. Make `parent` weak (typical for trees, leaves owned by root):
         parent ~weak Tree
    2. Make `children` weak (rare, used when leaves outlive parent):
         children []~weak Tree
    3. Use `~&T` for both (enables cycle collection, runtime cost):
         children []~&Tree, parent ~&Tree

  see: docs/memory/cycles.md#trees
```

LLM или человек видит **все варианты с пояснением**, делает осознанный
выбор. Это AI-first ([R5.3](revolutionary.md)) — обучающий сигнал.

**2. Lint-режим `nova lint --memory-graph`:**

Анализирует граф типов всего проекта, находит возможные циклы и
неоптимальные паттерны. Полезно при рефакторинге крупных систем.

**3. Документация `docs/memory/cycles.md`:**

Каталог типичных паттернов с готовыми решениями:

- Деревья (parent → child) — `parent ~weak`
- Doubly linked list — `tail ~weak` или `prev ~weak`
- Observer / Subscription (publisher ↔ subscriber) — `subscribers []~weak`
- DOM-like (parent ↔ children + listeners) — `~&T` (подходит для
  cycle collector)
- Graph (произвольные циклы) — `~&T`

Каждый паттерн со ссылкой на ошибку компилятора, чтобы LLM могла
переходить от ошибки к docs автоматически.

**4. Stdlib-defaults с явным комментарием:**

```nova
// в stdlib
type Tree[T] {
    value T
    children []~Tree[T]
    parent ~weak Tree[T]    // weak: tree owned top-down, leaves don't outlive root
}
```

Комментарий объясняет **почему** именно эта сторона weak — для
обучения программиста, использующего stdlib.

### Что отвергнуто

**Авто-вставка `~weak`.** Компилятор не может выбрать сторону цикла
без знания домена. Контрпример: дерево vs subscription/publisher —
weak'ом помечается противоположная сторона. Авто-выбор приведёт к
тихим багам с жизнью объектов.

**Авто-конверсия `~T` → `~&T` при цикле.** Скрывает performance-импликацию
(cycle collector работает в этой зоне), нарушает real-time гарантии
тихо. D21 опт-ин, не автомат.

### Приоритет

**Средний.** Не блокирует язык, но критично для UX. Без хороших
сообщений compile-time проверка циклов превратится в раздражитель,
а не помощника. Делается одновременно с реализацией type checker'а
(этап 2 [roadmap](../docs/plans/01-roadmap-v0.1.md)).

**Связь.** D21, R5.3, [roadmap](../docs/plans/01-roadmap-v0.1.md) этап 2.

---

## Приоритет

Если возвращаться к работе:

**Сначала** (закрыть, чтобы продолжать):
- Q1 (унификация методов) — структурный вопрос
- Q8 (обновить документы) — технический долг
- Q12 (concurrency model) — блокирует Q9 и ломает AI-first тезис

**Потом** (важно для v0.1):
- Q5 (граница Panic)
- Q6 (effect polymorphism)
- Q9 (stdlib) — зависит от Q12, Q13
- Q15 (enum с числами) — частая боль для wire-протоколов
- Q16 (bitflags) — нужно для permissions/options

**Можно отложить** (детали реализации):
- Q3, Q4 (runtime детали)
- Q7 (macros) — но блокирует Q15 D-вариант
- Q10 (tooling)
- Q11 (имя)
- Q13 (schema evolution как stdlib-паттерн) — описать вместе с Q9
- Q14 (cost-types) — после v1.0

---

## Q20. Нужен ли `defer`? ✅ ЗАКРЫТО (2026-05-10) [D90](decisions/03-syntax.md#d90)

> **Закрыт D90** (2026-05-10): добавлены `defer` и `errdefer` как
> scope-level cleanup statement'ы. Семантика — Zig-style:
> scope-level (не function-level), LIFO, eager arguments, infallible
> body, no-suspend. `errdefer` запускается только на throw/panic-exit.
> Решение мотивировано отсутствием RAII в Nova ([D6](decisions/05-memory.md#d6)
> managed heap) — без `defer` resource cleanup пишется через
> handler-блоки, что многословно.

**Контекст.** Слово `defer` присутствует в подсветке VSCode-расширения
([editors/vscode/syntaxes/nova.tmLanguage.json](../editors/vscode/syntaxes/nova.tmLanguage.json),
[editors/vscode/README.md](../editors/vscode/README.md)) как зарезервированное
ключевое слово, но семантика в `spec/decisions/` **не определена**. До
формального решения `defer` использовать нельзя.

Семантика, обсуждавшаяся ранее (Zig/Swift-style):

- **Block-scoped** (а не function-scoped как Go).
- **Срабатывает при `throw`, не при `panic`** (D13/D25).
- **LIFO**.
- Две формы: `defer expr` и `defer { ... }`.
- Эффекты внутри тела defer должны быть в сигнатуре enclosing-функции.

**Главный вопрос — нужен ли `defer` в Nova вообще.**

В Nova уже есть **два механизма cleanup'а** без отдельного `defer`:

1. **Handler-обёртки.** Защита ресурса оформляется как функция,
   принимающая lambda-тело. Cleanup — на выходе из обёртки:

   ```nova
   fn with_file[T](path str, body fn(File) Fail[IoError] -> T)
       Fail[IoError] -> T {
       let f = File.open(path)!!       // Result[File, IoError] → throw IoError
       let result = body(f)            // если throw — handler ловит выше
       f.close()                        // обычное закрытие
       result
   }

   with_file("data.txt") { f =>
       f.write(data)
   }
   ```

   Это **более общий механизм** — handler видит throw, может откатить
   транзакцию, обработать как угодно. Согласовано с D10 «всё — handler».

2. **`region { ... }`** ([D6](decisions/05-memory.md#d6)) — арена
   освобождается en-masse на выходе из блока, без `defer`.

**Открытые подвопросы:**

- **Q20.1.** Покрывают ли handler-обёртки реальные use-case'ы, или
  остаются практичные сценарии, где `defer x.close()` рядом с
  открытием — заметно эргономичнее? Нужны примеры из реальных программ
  (придёт с MVP-stdlib и первыми пользователями).
- **Q20.2.** Если `defer` всё-таки вводится — взаимодействие с
  `supervised(cancel:)` (срабатывает ли defer при отмене fiber'а?), с
  region (порядок: defer'ы сначала, арена потом, или наоборот?), с
  «двойным throw» (defer в процессе throw сам делает throw — что
  выигрывает?).
- **Q20.3.** Альтернатива — RAII через protocol-метод `@drop()` на
  типе ресурса. Тип `File` определяет `fn File @drop() Io -> ()`,
  компилятор вставляет вызов на выходе из скоупа, как в Rust/C++. Это
  **встроеннее** в систему типов, чем `defer`, и не требует ручного
  вызова. Рассмотреть как третий вариант.

**Статус.** До решения — `defer` **не используется** в коде/документации
языка. Подсветка VSCode оставлена как форвард-резерв (если решение в
пользу defer'а — менять не придётся; если против — будет небольшое
изменение в syntaxes/).

**Связь:** [D10](decisions/01-philosophy.md#d10) (всё — handler),
[D13](decisions/08-runtime.md#d13) (panic), [D25](decisions/04-effects.md#d25)
(throw), [D6](decisions/05-memory.md#d6) (region).

---

## Q21. Управление proliferation эффектов в публичных сигнатурах

**Контекст.** Типичная backend-функция после нескольких слоёв
архитектуры (router → middleware → controller → service → repository)
накапливает 5-8 эффектов в публичной сигнатуре:

```nova
export fn create_order(req CreateOrderReq)
    Db Net Log Trace Fail[OrderError] Time Random
    -> OrderResponse
```

Любое расширение в нижнем слое (например, добавили `Cache` в
репозиторий) поднимается через все public-функции вверх. Без механизма
группировки это работает как «N-вирусов одновременно» — хуже одиночного
`async`-вируса в Rust. **Автор языка считает это критическим вопросом**:
ожидается, что эффектов в реальных программах будут десятки или сотни.

С другой стороны — D28 («public — обязательно явно») и D10 (AI-first,
«сигнатура = полное описание») явно требуют, чтобы ничего не пряталось
от читателя. Любой механизм группировки балансирует на этом ребре.

**Три альтернативы.**

### Вариант A. Effect aliases через `alias`-keyword

```nova
alias StandardWeb = Db Net Log Trace Fail Time

export fn create_order(req Req) StandardWeb Random -> Resp
```

`StandardWeb` — синтаксический сахар, раскрывается компилятором в union
эффектов. Видимость через `export alias`/private как у других деклараций
(D47). Может ссылаться на другие алиасы.

**Плюсы:**
- Лаконично в сигнатуре.
- Локально для проекта (каждый проект свои алиасы).
- Расширяется обычным redeclaration алиаса.

**Минусы / открытые вопросы:**
- **Семантика «контракта»:** если функция объявила `StandardWeb`, но
  реально использует только `Db Fail` — компилятор разрешает
  (алиас как ≤-подмножество, но это ослабляет D28 «гарантия чистоты —
  проверенный факт») или требует все эффекты алиаса использовать
  (тогда алиас бесполезен)?
- **Параметризованные эффекты в алиасе** (`Fail[E]`): что значит
  `alias A = Fail[OrderError]`, `alias B = Fail[UserError]`,
  использование `A B` — конфликт, объединение, или ошибка?
- **AI-first риск:** LLM, видя `StandardWeb`, не знает её состав без
  чтения определения — это «−1 файл к локальности контекста». Для
  пользовательского кода допустимо, для prelude-алиасов опасно.
- **Стандартные алиасы в prelude (`Web`, `Cli`, `BatchJob`)** —
  привлекательно, но: устаревает (нужно `Cache`?), не угадать
  «правильный» набор (REST vs GraphQL vs WebSocket — разные), все
  программы Nova зависят от выбора. Лучше **не делать prelude-алиасов**;
  алиасы — проектный механизм.

### Вариант B. Алиас как `protocol` с композицией

После D18 (эффект = `protocol`) естественно использовать тот же
механизм:

```nova
export protocol StandardWeb : Db, Net, Log, Trace, Fail, Async, Time

export fn create_order(req Req) StandardWeb Random -> Resp
```

`StandardWeb` — обычный протокол, расширяющий 7 других. Composition
(`A : B, C`) — единый механизм для эффектов и для структурных
интерфейсов.

**Плюсы:**
- **Один механизм** с D18, не отдельная фича. Меньше концепций.
- Не вводит новой грамматики (только composition между протоколами).
- Hover/doc показывают композицию естественно.

**Минусы / открытые вопросы:**
- Требует решить **семантику protocol composition** — это отдельный D,
  не покрытый D18 в текущей форме. Inheritance vs flat union vs mixin —
  выбор не сделан.
- Композиция протоколов влияет и на **data-протоколы** (структурные
  контракты для типов), не только на эффекты. Большая семантическая
  поверхность.
- Те же AI-first и параметризационные вопросы, что в варианте A.

### Вариант C. Effect rows / row polymorphism

```nova
export fn create_order[E](req Req) Db Net | E -> Resp
```

`E` — переменная для «остальных эффектов». Caller подставляет конкретные
эффекты под `E`. Прецедент — Koka, Effekt (где это академически
проверено).

**Плюсы:**
- **Не пакует имена в группы** — каждый эффект остаётся явным в той
  функции, которая его реально использует.
- Решает **полиморфные** случаи: `map_eff[T, U, E](xs [T], f (T) E -> U) E -> [U]`
  — функция-комбинатор работает с любым эффектом вызываемой лямбды.
- AI-first сохраняется: `E` — это «остальное», не магическое имя группы.

**Минусы / открытые вопросы:**
- **Сложнее** — это полноценный полиморфизм. Реализация дороже.
- Уже частично есть в Q6 «Effect polymorphism — синтаксис». Нужно
  свести Q6 + Q21.C в одно решение.
- Не решает **именования** в публичных API — там 7 эффектов всё равно
  пишутся явно. Подходит больше для библиотечных комбинаторов, чем
  для backend-сигнатур.

### Что не делается (для всех вариантов)

- **Subtraction** (`Alias \ Effect`) — сложная row-typing семантика, не
  для MVP.
- **Default-эффекты на модуль** — нарушает «сигнатура = полное
  описание» (D10).
- **Effect categories** в стиле Helium — сложнее, чем алиасы, без
  выигрыша.
- **Опт-ин на effect inference в public** — D28 остаётся, public явный.

### Когда решать

**Откладываю до первой стадии stdlib (Q9) и первых реальных программ.**
Сейчас «proliferation» — это **прогноз**, не измеренная проблема. До
MVP неясно:

- Сколько эффектов реально в типичной сигнатуре?
- Какие пакуются в стандартные группы?
- Будут ли row-polymorphic комбинаторы доминировать в stdlib?

Принимать решение в текущем виде — риск ввести неправильную семантику
(см. открытые вопросы каждого варианта) или избыточный механизм. Когда
начнёт писаться реальный код на Nova, картина прояснится: либо
proliferation действительно болит и алиас/protocol-композиция —
очевидная победа, либо row-polymorphism в stdlib + 5-7 эффектов в
public-сигнатуре оказываются нормой.

**Действие сейчас:** ничего в спеке не вводить, не использовать ни в
примерах, ни в подсветке. Если автор хочет «лёгкое решение прямо
сейчас» — рекомендуемый вариант **B (protocol composition)**: дешевле
по новой грамматике, опирается на уже принятый D18.

**Связь:** [D18](decisions/04-effects.md#d18) (эффект = protocol),
[D28](decisions/04-effects.md#d28) (public явно), [D10](decisions/01-philosophy.md#d10)
(AI-first), [D47](decisions/07-modules.md#d47) (видимость), Q6
(effect polymorphism — пересекается с вариантом C), Q9 (stdlib —
проявит реальную картину proliferation).

---

## Q22. Унификация `type` / `protocol` в один keyword? ✅ ЗАКРЫТО ([D53](decisions/02-types.md#d53), [D61](decisions/04-effects.md#d61))

**Финальное решение** — гибрид:

- **Declaration syntax** объединён под единым `type`-keyword'ом по
  [D53](decisions/02-types.md#d53). Все объявления идут через `type`,
  с kind-токеном для категории.
- **Семантика расщеплена** по [D61](decisions/04-effects.md#d61):
  `effect` и `protocol` — **разные kind-токены** с разным поведением.
  - `effect` — поддерживает `with`-substitution (mock в тестах) и
    continuation-capture (`interrupt`, throw).
  - `protocol` — структурный контракт, без `with`-substitution.

```nova
type Hashable protocol { hash() -> u64 }    // структурный контракт
type Logger effect { log(msg str) -> () }   // эффект (with-substitution)
type Db effect { query(q Sql) -> []DbRow }  // эффект
type any protocol { }                        // top-type
```

Анонимный protocol-тип в позиции параметра — `protocol { ... }` (с
обязательным префиксом, симметрично `[]T`, `(A, B)`, `fn() -> T`).

D42 помечен revised → D53. Выбор между `effect` и `protocol` —
программистский, через два sniff-вопроса (см. [D62 правило 4](decisions/04-effects.md#d62)).

Контекст ниже сохранён как историческая справка.

---

### Исходный контекст Q22

**Контекст (до D53).** После D18-revised и [D42](decisions/02-types.md#d42)
в Nova два keyword'а:

- `type` — данные (record, sum-type, alias).
- `protocol` — поведение (эффекты + структурные контракты).

Различаются позицией в репо и формой литералов: у `type` литерал —
`Name { field: value }`, у `protocol` — handler-литерал
`Name { op(args) => body }`.

**Гипотеза.** Возможно, достаточно одного `type`, разрешающего
**либо поля, либо методы** (но не одновременно). Парсер уже различает
содержимое `{...}` для литералов (двоеточие vs стрелка) — то же самое
правило могло бы работать и для объявлений.

```nova
// data — type с полями
type User { id u64, name str }

// behavior — type только с сигнатурами методов
type Logger { log(msg str) -> () }
type Db {
    query(q Sql) -> []DbRow
    exec(q Sql)  -> ()
}
```

**Аргументы за единый `type`:**
- Один keyword вместо двух — проще грамматика.
- Прецеденты: TypeScript (`interface` и для полей и для методов),
  Common Lisp CLOS (`defclass`).
- Содержимое `{...}` уже различимо парсером, можно поднять это правило
  на уровень объявления.

**Аргументы против (как сейчас в D18-revised):**
- **Декларация категории заранее.** Данные и поведение —
  разные категории (значение существует/нет, сериализуемо/нет,
  расширение = +поле или +метод, подтипирование = по форме или по
  контракту). Keyword фиксирует категорию явно.
- **Прецеденты статически типизированных языков** идут в обратную
  сторону: Rust `struct`/`trait`, Swift `struct`/`protocol`,
  Go `struct`/`interface`. Единый keyword — у структурных/динамических
  языков (TypeScript runtime — duck typing).
- **Handler-литералы.** Сейчас форма литерала однозначно связана с
  keyword объявления: у `type X` — `X { field: value }`, у
  `protocol X` — `X { op(args) => body }`. С единым `type` LLM может
  смешать формы в одном литерале (синтаксически корректно для разных
  имён, семантически бессмыслица).
- **D42 уже принят** — концептуально разделил данные и поведение.
  Откат к единому keyword требует пересмотра D42.

**Тонкие места при унификации:**
- Запретить ли смешивание полей и методов в одном `{...}`? (Иначе
  получится «класс» — нежелательно, противоречит «protocols + data».)
- Что с пустым `type X { }` — данные без полей или поведение без
  операций? Сейчас `type X = ()` для unit, `protocol X { }` теоретически
  пустой контракт.
- Параметризация `Fail[E]` — она у data-типа или у protocol'а?
  (Сейчас `protocol Fail[E]`, и это согласовано с другими
  параметрическими протоколами.)

**Решение пока:** не трогать. D18-revised только что прошёл по spec/
и примерам, аргументы в пользу унификации риторические («одного
keyword'а хватит»), без измеренной болевой точки. Оставить как
открытый вопрос — если в реальном коде Nova появится систематическое
неудобство от двух keyword'ов, вернуться.

**Связь:** [D18](decisions/04-effects.md#d18) (эффект через `protocol`),
[D42](decisions/02-types.md#d42) (разделение данные/поведение),
[D17](decisions/02-types.md#d17) (объявление типов без `=`).

---

## Q23. Группировка методов: `methods Type { ... }`-блок (Rust `impl`-style)

**Контекст.** Сейчас методы типа объявляются через
[D35](decisions/03-syntax.md#d35) — отдельные `fn Type @method(...)`
декларации:

```nova
type Account { readonly id u64, balance money, closed bool }

fn Account.new(owner str) -> Account => ...
fn Account @balance_pct(of money) -> f64 => @balance / of * 100.0
fn Account @is_solvent() -> bool => !@closed && @balance > 0
fn Account mut @deposit(amount money) => @balance += amount
fn Account mut @withdraw(amount money) Fail[Overdraft] => ...
```

Имя типа повторяется на каждом методе. Для типа с 15 методами — 16
повторений. Локальность теряется: тип и его поведение визуально
разнесены, особенно когда методы рассеяны по файлу.

**Предложение.** Ввести `methods Type { ... }`-блок (как `impl Type`
в Rust):

```nova
type Account { readonly id u64, balance money, closed bool }

methods Account {
    fn new(owner str) -> Account =>
        Account { id: ids.next(), balance: 0, closed: false }

    fn @balance_pct(of money) -> f64 =>
        @balance / of * 100.0

    fn @is_solvent() -> bool =>
        !@closed && @balance > 0

    fn mut @deposit(amount money) =>
        @balance += amount

    fn mut @withdraw(amount money) Fail[Overdraft] =>
        if amount > @balance { throw Overdraft }
        @balance -= amount
}
```

Имя типа задаётся блоком. Внутри — `fn name(...)` для static-функций,
`fn @name(...)` для методов инстанса, `fn mut @name(...)` для
мутирующих. Это **тот же `@`-синтаксис D35**, просто сгруппирован.

**Альтернативы (расширенный контекст).**

- **(a) Оставить как сейчас** — раздельные `fn Type @method`. Минимум
  концепций, но плохая локальность.
- **(b) `methods Type { ... }`-блок** — это предложение.
- **(c) Методы внутри `type` блока** (Java/Kotlin/Swift-стиль) —
  сильнее всего ломает текущую модель: `type` становится «данные +
  методы» (полу-класс), grow-pressure на наследование, путаница
  `type`/`protocol` для эффектов.

Вариант (c) явно отвергнут: возврат методов в `type` размывает D1
(«protocols + data, без классов»), создаёт два механизма для
поведения (`type` с методами и `protocol`), и порождает семантические
дыры (что значит «эффект внутри метода `type`?»).

Вариант (b) — компромисс, дающий локальность без слома модели.

**Что даёт (b):**

1. **Локальность.** Тип и методы в одном месте файла.
2. **Группировка по теме.** Несколько `methods`-блоков для одного
   типа — конструкторы отдельно, базовые операции отдельно,
   conditional methods (если введут bounds) отдельно. Прецедент Rust.
3. **Extension methods из чужого модуля видны явно.**

   ```nova
   import HashMap from std

   methods HashMap[K, V] where K: Json, V: Json {
       fn @to_json() -> str => ...
   }
   ```

   Сейчас в Nova расширение чужого типа делается через
   `fn ForeignType @method` где-нибудь в своём модуле — визуально
   неотличимо от «своих» методов. С блоком — заявка явная.
4. **Совместимость с `protocol`.** `type`/`protocol` остаются
   раздельными. Структурная совместимость работает как сейчас:
   компилятор смотрит, какие методы определены у типа (через
   `methods` или старый стиль), и сравнивает с протоколом.
5. **D1, D17, D42 не меняются.** `type` остаётся чистым (только
   данные), `protocol` — чистым контрактом. Меняется только D35:
   методы переезжают в `methods`-блок.

**Тонкие места:**

1. **Один способ или два?** Текущий стиль `fn Type @method` остаётся
   валидным или нет?
   - **Только блок** — чище, но breaking change для существующих
     spec/examples (кода мало, миграция дешёвая).
   - **Оба разрешены** — совместимо, но «два способа одного» нарушает
     AI-first.

   Рекомендация: **только `methods`-блок**, миграция один раз.
2. **Static vs instance в блоке.** Та же разметка: `fn name(...)` —
   static (конструктор/factory), `fn @name(...)` — instance,
   `fn mut @name(...)` — mutating instance. Без новых правил.
3. **Множественные блоки для одного типа.** Разрешены (как Rust
   многократный `impl`). Программист сам группирует по теме.
4. **`where`-clauses** для conditional methods — зависит от Q-bounds.
   Если bounds в MVP нет, `where` тоже нет, conditional откладывается.
5. **Visibility.** Каждый метод сам декларирует `export`/private
   ([D47](decisions/07-modules.md#d47)). Блок `methods` свою
   видимость не имеет.
6. **Embed/delegation** ([D39](decisions/02-types.md#d39)). Прокси-
   методы при `use Type` генерируются на основе всех `methods`-блоков
   типа. Override метода — отдельный метод во внешнем `methods`-блоке
   обёртки, явный вызов `@Inner.method()` для делегации.

**Прецеденты:**

- **Rust** `impl Type { ... }` — ровно эта идиома, 10+ лет, любят.
- **Swift** `extension Type { ... }` — то же для своих и чужих типов.
- **Kotlin** — методы внутри `class` + extension functions снаружи
  (два способа, что у нас неприемлемо).
- **Go** — `func (r Type) method()` отдельно, как Nova сейчас.
  Жалоба сообщества — нет визуальной группировки.

**Цена:**

1. Новый keyword `methods`. Короткий, читаемый, семантически точен.
2. Миграция всех существующих `fn Type @method` в `methods`-блоки —
   делается раз, кода пока мало.
3. Семантика множественных блоков для одного типа должна быть
   зафиксирована.

**Решение пока:** не трогать. Текущий стиль (D35) рабочий, breaking
change без измеренного болевого опыта рискован. Когда появится первый
средний по размеру тип (например, `Vec[T]` или `HashMap[K, V]` в
stdlib) с 20+ методами, локальность станет реальной проблемой —
тогда вернуться к выбору (b) vs текущее.

**Связь:** [D1](decisions/01-philosophy.md#d1) (protocols + data, без
классов — Q23 это сохраняет), [D17](decisions/02-types.md#d17)
(`type` для данных), [D35](decisions/03-syntax.md#d35) (методы через
`@` — Q23 переселяет их в блок), [D39](decisions/02-types.md#d39)
(embed/delegation на основе методов), [D42](decisions/02-types.md#d42)
(`protocol` остаётся раздельным), [D47](decisions/07-modules.md#d47)
(видимость), Q22 (унификация type/protocol — связанный, но
ортогональный).

---

## Q-bounds. Синтаксис bounds на дженериках (если будут)

> ✅ **CLOSED by [D72](decisions/02-types.md#d72).** Bounds приняты,
> синтаксис `[T Hashable]` без двоеточия, единое правило «name type».
> Текст ниже — историческая справка.

**Контекст.** В MVP bounds на дженерики **отвергнуты**
([02-types.md → D42](decisions/02-types.md#d42), open-questions D42:
«сейчас параметр без bound, компилятор полагается на структурное
соответствие при использовании»; [history/rejected.md](decisions/history/rejected.md):
«[T: Bound] отвергнут в MVP»). Если/когда bounds будут вводиться,
нужно зафиксировать синтаксис.

**Главное правило, которое уже есть в языке.** Двоеточие в Nova —
**только** разделитель ключ-значение (record-литералы, dict-литералы,
[02-types.md → D17](decisions/02-types.md#d17)). В аннотациях типов
двоеточия нет: `u User`, не `u: User`. Параметр `T` с указанным
контрактом — это аннотация типа, не key-value.

**Рекомендуемый синтаксис, если bounds появятся.**

```nova
fn all[T FromRow]() Db Fail -> []T
fn map[K Hashable, V](m HashMap[K, V]) -> ...
```

Без двоеточия, по правилу «имя тип» — единый стиль с параметрами
функции (`x int`), полями record (`id u64`), let-bindings
(`let x int = 5`), for-loops (`for x int in xs`), embed
(`use w HashMapIter[K, V]`).

**Что отвергается заранее:**
- `[T: FromRow]` (Rust/Scala/Kotlin) — конфликтует с D17.
- `[T where FromRow]` (C#-style) — многословно.
- `[T impl FromRow]` (Swift `some`-style) — нестандартно.

**Тонкие места:**
1. Несколько bounds на один параметр — `[T FromRow & Hashable]`?
   `[T (FromRow, Hashable)]`? Лучше — анонимный structural type или
   композиция протоколов.
2. Связь с эффектами в bounds — может ли protocol-bound включать
   эффект-операции? (Эффекты — это `protocol`, [D18](decisions/04-effects.md#d18-эффекты-объявляются-через-kind-токен-не-голый-type),
   так что технически да.)
3. Что бывает с уже принятыми решениями про anonymous structural
   type в позиции параметра ([D42](decisions/02-types.md#d42)):
   `fn f(x { show() -> str })` — можно ли это перенести в bound:
   `fn f[T { show() -> str }]()`?

**Статус.** Открыт как «предзафиксированная форма» — если bounds
будут, использовать `[T Bound]` без двоеточия. Целиком решение о
вводе bounds откладывается до post-MVP.

---

## Q-self-mandatory. Обязательное использование `Self` где валидно

**Контекст.** [D66](decisions/02-types.md#d66) разрешает `Self` в любом
type-контексте (методы, static-функции, protocol, effect). Сейчас
программист **может** писать либо `Self`, либо явное имя типа:

```nova
fn Box[T].of(v T) -> Self => ...           // Self
fn Box[T].of(v T) -> Box[T] => ...         // явное имя — тоже валидно
```

**Предложение.** Сделать `Self` **обязательным** там где он валиден.
Использование явного имени типа — compile error или линт-warning.

**Аргументы за:**

1. **DRY жёстче.** Имя типа никогда не повторяется → переименование
   `Box → Container` точечное.
2. **AI-консистентность.** LLM не «забывает» дописать generic-
   параметры (`Box[T]` vs `Box`).
3. **Один способ** ([D40](decisions/03-syntax.md#d40)).

**Аргументы против:**

1. **Имя типа читается лучше для коротких типов.** `User`, `Box` —
   нагляднее чем `Self`. Self экономит для generic'ов с параметрами.
2. **Прецедентов нет.** Rust, Swift, Scala — `Self` всегда
   опционален. Strict-Self нет ни в одном языке.
3. **Конфликт с существующим стилем.** Большая часть spec/examples
   написана с явными именами типов. Миграция значительная.
4. **AI-training data.** LLM обучен на языках без обязательного
   `Self` — генерирует имена типов. Hard-rule вызовет постоянные
   compile errors при first generation.
5. **Гибче через линтер.** Если хочется единообразия — это
   linter-warning, не language-rule. Программист отключает локально
   при необходимости.

**Варианты решения:**

A. **Hard-rule в языке.** `fn Box.of(v T) -> Box[T]` — compile error
   «use Self». Жёстко, но единообразно.

B. **Линт-warning по умолчанию.** Линтер предупреждает «здесь можно
   `Self`», программист игнорирует/исправляет. Гибко.

C. **Style-guide рекомендация.** Прописать в convention'ах: «для
   методов с явным receiver'ом и static-конструкторов используй
   `Self`», без linter enforcement.

D. **Оставить как есть.** Обе формы валидны без рекомендации.

**Тонкие места:**

1. **Свободные функции с generic-параметрами не имеют receiver'а** —
   `Self` запрещён по D66. Hard-rule не применим в свободных функциях.
2. **Bound `[T From[Self]]` в свободной функции** — `Self` запрещён,
   нужно явное имя или другой generic. Это уже исключение.
3. **Sum-варианты** — `fn Tree @clone() -> Self` валиден, но в теле
   `match @ { Leaf => Leaf, ... }` нельзя `Leaf => Self.Leaf`
   (конструктор). Self применим только в типовых позициях.
4. **Миграция существующих файлов** — большая часть spec/decisions/,
   examples/, nova_tests/ написаны с явными именами. Hard-rule
   потребует масштабного sweep.

**Статус.** Не зафиксировано. Склонность — **(B) линт-warning**:
сохраняет гибкость, даёт DRY-win опционально, не ломает существующий
код. Решение откладывается до появления линтера в toolchain.

**Связь:** [D66](decisions/02-types.md#d66) (`Self` universal),
[D40](decisions/03-syntax.md#d40) (один способ), [D72](decisions/02-types.md#d72)
(bounds, где `Self` запрещён в свободных функциях).

---

## Q-anon-effect. Анонимный эффект в позиции эффекта

**Контекст.** [D42](decisions/02-types.md#d42) разрешает анонимный
структурный тип в позиции параметра:

```nova
fn log_one(x { show() -> str }) Log -> () =>
    Log.info(x.show())
```

После [D18-revised](decisions/04-effects.md#d18-эффекты-объявляются-через-kind-токен-не-голый-type)
эффект — это `protocol`. По симметрии:

```nova
fn log_one(s str) { log(msg str) -> () } -> () =>
    log("...")            // анонимный эффект между ) и ->
```

Допускать ли это? Удобно при «одноразовом» эффекте без отдельного
объявления. Но границы между параметрами и анонимным эффектом могут
читаться плохо: `fn f(x { ... }) { ... } -> ()` — два `{...}` подряд,
парсер должен различить «структурный параметр» и «анонимный эффект».

**Статус.** Решение отложено. На MVP — **анонимные эффекты запрещены**,
эффект всегда именованный `protocol`.

---

## Q-field-tags. Метаданные на полях типов (аналог Go struct tags)

**Контекст.** В Go теги вида `` `json:"id"` `` решают связку «имя поля
в коде ↔ имя в wire-формате» для JSON, БД, валидации, и доступны через
runtime reflection. В Nova сейчас такого механизма **нет** — маппинг
делается ручными функциями (`fn User.from_row(r) -> User => ...`,
`fn User @to_json() -> Json => ...`) или handler-литералами.

Для record с 5-10 полями ручной маппинг — приемлемая цена и AI-friendly
прозрачность. Для record с 30+ полями (типичный backend domain-model) —
заметный boilerplate, который накапливается на каждом формате
(Json/Sql/Validate/Yaml/Protobuf).

**Что в других языках:**

| Язык | Механизм | Где живёт |
|---|---|---|
| Go | `` `json:"id"` `` после поля | runtime reflection |
| Rust | `#[serde(rename = "id")]` | compile-time macros (derive) |
| Swift | `enum CodingKeys: String, CodingKey` | compile-time через protocol |
| Kotlin | `@SerialName("id")` + serialization plugin | compile-time |
| OCaml | `[@@deriving yojson]` | compile-time PPX |
| C# / Java | `[JsonPropertyName("id")]` / `@JsonProperty` | runtime reflection |

Современные типизированные языки в подавляющем большинстве идут по
пути compile-time derive, не runtime reflection. Это согласовано с
направлением Nova (всё видно в типах, никакой невидимой runtime-магии).

**Варианты:**

**A. Не вводить, оставить ручной маппинг.** Прозрачно, никакой магии,
полный контроль. Цена — boilerplate для больших record'ов и
дублирование при многоформатной сериализации.

**B. Compile-time атрибуты + derive-макросы** (Rust serde-style).
Атрибут на поле и/или типе:

```nova
@derive(Json, FromRow)
type User {
    readonly id u64       @json("id")          @sql("user_id")
    name str              @json("name")        @sql("full_name")
    email str             @json("email")       @sql("email")
    internal_token str    @json(skip)          @sql(skip)
}
```

Атрибуты — compile-time литералы, доступные только comptime-функциям.
Сами по себе ничего не делают; `@derive(Json)` запускает
comptime-функцию, которая читает атрибуты и генерирует
`fn User @to_json() -> Json`, `fn User.from_json(j Json) -> User`.

Раскрытие генерации обязано быть видимым через тулинг
(`nova check --show-derive User`) — это сохраняет AI-first прозрачность,
LLM может посмотреть «что на самом деле вызывается».

Зависит от **Q7 (macros/comptime)** — без него вариант B нереализуем.

**C. Schema-объект как first-class value.** Программист руками
объявляет схему рядом с типом:

```nova
type User {
    readonly id u64
    name str
    email str
}

let user_json_schema = JsonSchema[User] {
    field("id",    (u) => u.id,    (u, v) => User { ..u, id: v })
    field("name",  (u) => u.name,  (u, v) => User { ..u, name: v })
    field("email", (u) => u.email, (u, v) => User { ..u, email: v })
}
```

Никаких атрибутов на полях. Schema — обычный Nova-объект, читается
функциями `Json.encode_with(user, user_json_schema)`. Цена —
multiline-объявление вместо одного-двух тегов на поле, дублирование
имён полей в schema.

Не зависит от других open-questions, можно зафиксировать сейчас.

**D. Runtime reflection** (как Go, Java, C#).
**Отвергается заранее:**
- Противоречит AI-first: тег `json:"id"` ничего не делает «сам по
  себе», нужно знать какую-то библиотеку «снаружи кода». LLM, читая
  поле, не видит активную конструкцию.
- Несовместимо с принципом «всё видно в типах» — теги это
  out-of-band метаданные, которые компилятор не валидирует.
- Несовместимо с capability-режимом и effect-видимостью —
  reflection обходит эффект-систему.

**Тонкие места:**

1. **Несколько потребителей одного поля** (`json` + `sql` + `validate`).
   В B — несколько атрибутов рядом с полем, шумно но локально. В C —
   несколько отдельных schema-объектов, поле повторяется по числу
   форматов. Что лучше — открыто.

2. **Skip-семантика.** Поля, которые не сериализуем: отдельный atom
   `@json(skip)`, отсутствие атрибута → не включать, или маркер
   `_prefix` (D47 convention) → не включать?

3. **Default-имя.** Если 95% полей сохраняют имя один-к-одному
   (snake_case в коде ↔ snake_case в wire), не нужно ли по умолчанию
   маппить, отмечать атрибутом только исключения? Это сильно срезает
   boilerplate.

4. **Имена derive'ов.** `@derive(Json)` или `@derive(Encoder[Json])`
   или `@derive(SerializableTo[Json])`? Упирается в дизайн stdlib
   (Q9).

5. **Composability с эффектами.** Если `Json.encode` имеет эффекты
   (например, `Fail[EncodeError]`) — генерируемая функция должна
   правильно их пробрасывать. Это согласуется с D28 (вывод эффектов в
   private), но требует, чтобы comptime-генератор корректно вычислял
   эффект-сигнатуру.

6. **Совместимость с `readonly`/`mut`-полями (D36).** При decode'е из
   wire-формата нужно создавать новый `User` (потому что `id` —
   `readonly`), не мутировать существующий. Генератор должен это
   учитывать.

**Связь.**
- Q7 (macros/comptime) — блокирует вариант B.
- Q9 (стандартная библиотека) — определяет `Json`, `FromRow`,
  `Validate` и т.п.
- Q15 (representation tags для enum'ов) — смежная задача, тоже про
  сериализацию, может решаться тем же механизмом.

**Статус.** Открыт. Рекомендуемый путь:
1. **Сейчас** — оставить вариант A (ручной маппинг). Это работает,
   у Nova нет реализации, спешить некуда.
2. **После Q7** — вернуться, рассмотреть B+C: атрибуты для частых
   случаев, schema для сложных. Это не взаимоисключающие варианты.
3. **Никогда** — вариант D (runtime reflection).

---

## Q-anonymous-union. Anonymous unions (TS-style `string | number`) без обёрток

**Контекст.** Сейчас sum-тип в Nova ([D52](decisions/02-types.md#d52))
требует **именованные конструкторы**:

```nova
type StrOrInt | S(str) | I(int)
let x StrOrInt = S("alice")
```

С [D55](decisions/02-types.md#d55) literal coercion программист пишет
просто `let x StrOrInt = "alice"` (компилятор оборачивает в `S`). Но
тип всё равно остаётся **sum-обёрткой**, а не «string или number».

TypeScript/Scala 3 имеют **anonymous unions** без обёрток:

```typescript
type StrOrInt = string | number;     // tip = string ИЛИ number
let x: StrOrInt = "alice";            // тип x — string, не обёртка
```

Здесь `string` — **подтип** `string | number`. Это **subtyping**,
которого в Nova сейчас нет (только структурная типизация для
protocol'ов).

**Альтернативы синтаксиса для Nova (если когда-то введём):**

**A. С маркером `type` для existing types:**

```nova
type IntOrStr | type int | type str
type Maybe[T] | type T | None
```

Парсер однозначен — `type X` = «existing type», без `type` = «новый
конструктор». Не ломает текущий синтаксис sum'ов.

**B. Со скобками:**

```nova
type IntOrStr | (int) | (str)
```

Двусмысленно с tuple-конструкторами одного поля.

**C. Не вводить.** Использовать D55 coercion + named sum'ы как
сейчас (`type StrOrInt | S(str) | I(int)`). Громоздко при объявлении,
но coercion убирает шум при использовании.

**Главные минусы введения:**

1. **Subtyping — серьёзное расширение системы типов.** Нужно решить
   variance, type inference, exhaustiveness, dispatch. Сейчас Nova
   эти концепции не имеет.
2. **Runtime-cost.** Каждое значение `IntOrStr` несёт runtime-tag
   (иначе `is`-проверка не работает). Boxing на границах. Для
   статически типизированного языка — реальная цена.
3. **Прецедентов мало.** TS (бесплатно в JS-runtime), Scala 3 (с
   cost). Большинство строго типизированных языков (Rust, Swift,
   Kotlin, F#, OCaml, Haskell) **не вводят** anonymous unions —
   используют named variants.

**Решение пока:** не вводить. D55 coercion + named sum покрывают
большинство use-case'ов. Если в реальном Nova-коде накопится
измеренная боль от обёрток — вернуться.

**Связь:** [D52](decisions/02-types.md#d52),
[D55](decisions/02-types.md#d55), [D54](decisions/03-syntax.md#d54)
(`is`-pattern уже даёт runtime type-check для `any`).

---

## Q-stdlib-data-types. `SqlValue`, `JsonValue`, `Sql`, теги `sql`/`json` в stdlib

**SQL-часть — эталонная реализация в [`examples/stdlib_sql.nv`](../examples/stdlib_sql.nv)**
и применение в [`examples/orm_demo.nv`](../examples/orm_demo.nv).
Окончательная фиксация в prelude (D26) — отдельным D-блоком после
v1.0-stdlib.

**JSON-часть — открыта.** Number representation и Object representation
не зафиксированы (см. подвопросы ниже).

**Контекст.** [D48](decisions/03-syntax.md#d48) фиксирует tagged
template literals и стандартные теги `json`, `sql`, `regex`, `bytes`.
Но **возвращаемые типы** этих тегов и их структура — не определены.

С введением [D55](decisions/02-types.md#d55) (literal coercion)
типизация SQL-аргументов через closed sum становится практичной:

```nova
// Кандидат для prelude:
type SqlValue
    | I(i64)
    | F(f64)
    | S(str)
    | B(bool)
    | Bytes([]byte)
    | Null

type Sql {
    template str           // "SELECT * FROM users WHERE id = ?"
    args []SqlValue        // [I(42)]
}

// Tag-функция:
fn sql(parts []str, args []SqlValue) -> Sql =>
    Sql {
        template: parts.join("?"),
        args
    }

// Использование (через D55 coercion):
let q = sql`SELECT * FROM users WHERE id = ${user_id}`
let users = Db.query(q)
Db.query(sql`... ${42} ... ${"alice"}`)   // безопасно, без injection
```

Аналогично для JSON:

```nova
type JsonValue
    | Null
    | Bool(bool)
    | Number(...)            // f64? i64+f64? opaque Number?
    | String(str)
    | Array([]JsonValue)
    | Object(HashMap[str, JsonValue])

let j JsonValue = json`{"name": "${user}", "age": ${age}}`
```

**Открытые вопросы:**

1. **Number representation в JsonValue.** `Number(f64)` теряет
   int-precision. `Int(i64) | Float(f64)` точнее, но coercion `42`
   ambiguous (i64 или f64?). `Number(NumberKind)` где `NumberKind |
   I(i64) | F(f64)` — двухуровневое, гибко но громоздко. Прецеденты:
   Rust serde_json — opaque `Number` с методами `as_i64()`/`as_f64()`.
2. **Object representation.** `HashMap[str, JsonValue]` теряет
   порядок ключей. Альтернатива — `[]Field`. Большинство JSON-парсеров
   используют HashMap.
3. **Compile-time JSON-парсинг через `json\`...\``.** Нужен
   [Q7 (macros/comptime)](#q7-macros--comptime), без него — runtime.
4. **`Db.query` сигнатура.** ✓ **Решено**: через `Sql`-тег.
   `fn Db.query(q Sql) Fail[DbError] -> []DbRow`,
   `fn Db.exec(q Sql) Fail[DbError] -> int`. Это единая публичная
   сигнатура — все usage'и через `sql\`...\`` или `Sql.builder().build()`
   (динамические запросы). Прямого пути с raw-string'ом и отдельным
   `[]SqlValue`-массивом не осталось. Эталон — `examples/stdlib_sql.nv`.
5. **Где разместить.** В prelude ([D26](decisions/08-runtime.md#d26))
   или в stdlib-модулях (`std.sql`, `std.json`)? Гибрид:
   `Option`/`Result` — prelude, `SqlValue`/`JsonValue` — модули?

**Решение пока:** не вводить в prelude. Зафиксировать как часть Q9
(stdlib). При работе над Q9 решить все 5 пунктов.

**Связь:** [D48](decisions/03-syntax.md#d48) (tagged templates),
[D55](decisions/02-types.md#d55) (coercion делает это эргономичным),
[D26](decisions/08-runtime.md#d26) (prelude),
[Q9](#q9-стандартная-библиотека).

---

## Q-numeric-coercion. Coercion числовых литералов через D55

> ⏸ **DEFERRED — ждёт JsonValue** (Plan 17 Ф.3, 2026-05-08).
> **Rationale:** сквозная coercion (D44 numeric + D55 sum) полезна
> для `JsonValue | Number(f64) | ...` и подобных, но без
> зафиксированного `JsonValue`-типа в stdlib главного use-case'а
> нет. Текущий exact-match строже, не ломает existing код.
> **Trigger:** добавление `JsonValue` или `Configvalue` в stdlib
> (Q-stdlib-data-types) — первый случай, где coerce даст реальное
> упрощение (`{ "k": 42 }` без `Number(42.0)`).

**Контекст.** [D55](decisions/02-types.md#d55) ввёл literal coercion
для sum-конструкторов с **exact match** типа значения. Тонкость
возникает с **числовыми литералами**:

```nova
type Wrapper | F(f64)

let w Wrapper = 1.5         // ok — F(1.5), exact match (f64)
let w Wrapper = 42           // ??? 42 это int, не f64
```

[D44](decisions/03-syntax.md#d44) разрешает literal coercion для
числовых типов в позиции с явной аннотацией:

```nova
let x u8 = 200               // 200 как u8 (D44)
let y f64 = 42                // 42 как f64 (D44)
```

**Вопрос:** работает ли эта литеральная coercion **сквозь D55**?

```nova
let w Wrapper = 42           // ⇒ w = F(42 as f64)? или ОШИБКА?
```

**Альтернативы:**

**A. Сквозная coercion** (D44 + D55 комбинируются). Литерал `42`
подгоняется под `f64` в позиции `F(f64)`-параметра. Эргономично,
но добавляет **цепочку** конверсий — D55 говорит «exact match».

**B. Только exact match.** Программист пишет `42 as f64` или
`42.0`:

```nova
let w Wrapper = 42 as f64    // явно
let w Wrapper = 42.0          // float-литерал
```

Строже, без неожиданностей. Но громоздче.

**C. Только для числовых литералов.** Literal coercion (D44)
работает для int↔int, int↔float; D55 видит уже «адаптированный»
литерал. Это **частный случай** A, но ограниченный литералами (не
переменными).

**Проблема для JsonValue:**

```nova
type JsonValue | ... | Number(f64) | ...

let j JsonValue = 42         // что: Number(42 as f64) или ОШИБКА?
```

Без сквозной coercion `JsonValue` неэргономичен — каждое целое
требует `42.0` или `42 as f64`. С coercion — `42` работает.

**Решение пока:** отложено до решения по `JsonValue` (Q-stdlib-data-types).
Сейчас D55 строго требует exact match. Если JSON/SQL покажет реальную
боль — расширить D55 до варианта C (literal-only через D44).

**Связь:** [D44](decisions/03-syntax.md#d44),
[D55](decisions/02-types.md#d55), Q-stdlib-data-types.

---

## Q-style-coercion. Когда применять D55 coercion, когда писать явно?

> ✅ **CLOSED by [D55 → «Style-guide»](decisions/02-types.md#d55)**
> (Plan 17 Ф.1, 2026-05-08): permissive (вариант **A**) с
> формализованными рекомендациями (вариант **B** для линтера).
> `nova fmt` не переписывает между формами — выбор стилистический.
> Сводка-таблица «coerce / явный» добавлена в D55.

**Контекст.** [D55](decisions/02-types.md#d55) ввёл literal coercion в
позиции с явным целевым типом. Это **opt-in эргономика** — старая
форма (явные конструкторы и имена типов) **тоже валидна** и работает.
Получаются **два равнозначных написания** одного и того же:

```nova
// Sum-coercion
let m Maybe[int] = 42                    // ✓ coercion
let m Maybe[int] = Just(42)               // ✓ явный, тоже валидно

// Record-coercion
let u User = { id: 2, name: "Bob" }       // ✓ coercion
let u User = User { id: 2, name: "Bob" }   // ✓ явный, тоже валидно

fn make() -> Duration => { nanos: 100 }    // ✓ coercion
fn make() -> Duration => Duration { nanos: 100 }    // ✓ явный
```

Это **classical style-vs-mandate** вопрос, не зафиксированный в D55.

**Реальные case'ы из миграции `examples/`:**

1. **Однозначно лучше с coercion:**
   ```nova
   export fn Duration.from_secs(n i64) -> Duration => { nanos: n * 1e9 }
   //                                                ^^^ имя из аннотации, чище
   ```

2. **Явный конструктор лучше из-за визуальной симметрии в match:**
   ```nova
   match @buckets[idx] {
       Occupied { value } => Some(value)        // лучше Some(value)
       _                  => None               // ← unit, не coerce'ится
   }
   // С coercion: `value` слева, `None` справа — асимметрично, читать сложнее.
   ```

3. **Явный конструктор лучше в `let` без аннотации:**
   ```nova
   let ip_value = if e.ip != "" { Some(e.ip) } else { None }
   // нет аннотации — coercion не работает; нужны явные Some/None.
   ```

4. **Спорно — `{ {...} }` после else:**
   ```nova
   else { Money { amount: a + b, currency: c } }    // явный
   else { { amount: a + b, currency: c } }           // coerce — `{ {...}}` шумно
   ```

**Альтернативы политики:**

**A. Permissive (текущее).** Программист сам выбирает — coercion или
явно. D55 разрешает оба.
- Плюс: гибкость, читаемость per-case.
- Минус: **inconsistency** — в кодовой базе одно и то же пишется
  по-разному. Code review устаёт. LLM генерирует то так, то так.

**B. Style guide (рекомендация).** D55 разрешает оба, но **стиль
рекомендует** одну форму:
- expression-body return: предпочитать coercion (короче).
- match-веточки с unit-альтернативой (Some/None): явные конструкторы
  (visual symmetry).
- `let` без аннотации: явный конструктор (других вариантов нет).
- Сложные nested-литералы (`{ {...} }`): явный для ясности.

Это **не правило компилятора**, а guideline для `nova fmt`/линтера и
code review.

**C. Mandatory coercion.** Запретить явный конструктор там, где
coercion применим — компилятор ругается «излишняя обёртка». **Жёсткая
форма**, единая.
- Плюс: zero ambiguity, единый стиль везде.
- Минус: ломает практичность (case 3 выше — нет аннотации, нельзя
  без Some), требует разрешения для `let` без аннотации.

**D. Mandatory explicit.** Запретить coercion — программист всегда
пишет имя.
- Плюс: explicit always.
- Минус: убивает D55 целиком, теряем эргономику prelude-типов.

**Решение пока:** A (permissive). При работе над `nova fmt`/style
guide вернуться к B — формализовать рекомендации. C/D — слишком
жёстко, ограничивает практический код.

**Тонкости для guideline (если введём B):**

- **expression-body** с явным `-> T`: предпочитать coercion (короче).
- **`let x T = ...`** с аннотацией: предпочитать coercion.
- **`let x = ...`** без аннотации: явный конструктор обязателен.
- **match-arms**: unit-варианты (None, Empty) **не coerce'ятся**, для
  визуальной симметрии писать **все** ветки с явным конструктором.
- **`{ {...} }` (block + record-литерал)**: писать явный имя для
  ясности (избегать визуально шумного `{ {...}}`).
- **call-site аргументы коллекций**: `[42, "alice"]` для `[]SqlValue`
  — coercion лучше (нет `[I(42), S("alice")]`-шума).
- **nested coercion**: `let r Result[User, str] = { id: 2, name:
  "Bob" }` — двойная coercion (record + sum), явный был бы
  `Ok(User { ... })`. Coercion **значительно** короче.

**Связь:** [D55](decisions/02-types.md#d55), Q-anonymous-union,
Q-numeric-coercion (связаны), Q9 (style guide как часть tooling в
v1.0).

---

## Q-array-api. Формальный API `[]T` — что встроено, что расширяется

> ✅ **CLOSED by [D38 → «Built-in API для `[]T`»](decisions/03-syntax.md#d38)**
> (Plan 17 Ф.1, 2026-05-08): зафиксирован минимальный built-in API
> (`len`/`cap`/`is_empty`, `[i]`/`get`, `push`/`pop`, `iter`/`for`,
> static-конструкторы `new`/`with_capacity`/`filled`) и список
> текущих stdlib extensions (`map`/`filter`/`fold`/`any`/`all`/
> `first`/`last`). Slicing `xs[a..b]` остаётся отложенным
> (Q-array-slicing). Embed `use []T` — разрешён по D39.

**Контекст.** `[]T` — встроенная конструкция языка ([D27](decisions/03-syntax.md#d27)).
По [D32](decisions/02-types.md#d32) runtime-представление —
`(ptr, len, cap)`-структура. В примерах spec/ и examples/
используются: `xs.len`, `xs.push(x)`, `[]T.with_capacity(n)`,
`xs.iter()`, и т.д. Но **формального D-решения** про API `[]T`
нет — это используется «по умолчанию», без зафиксированного списка.

Вопросы:

### Q-array-api.1. Что входит в API `[]T`

Из практики и примеров видны следующие операции — нужно зафиксировать
полный список:

**Геттеры:**
- `xs.len` — количество элементов (поле или метод? сейчас как поле).
- `xs.cap` — capacity (выделенная память).
- `xs.is_empty` — `len == 0` (для удобства).

**Конструкторы (static-функции на типе `[]T`):**
- `[]T.with_capacity(n int) -> []T` — выделить с capacity n, len 0.
- `[]T.alloc(n int) -> []T` — выделить с len n (заполнено default-T).
  *Не уверен, что зафиксировано. См. Q-array-api.4.*
- `[]T.from(other []T) -> []T` — копия (shallow clone).

**Мутирующие:**
- `mut xs.push(item T) -> ()` — добавить в конец, grow при
  переполнении.
- `mut xs.pop() -> Option[T]` — удалить с конца.
- `mut xs.clear() -> ()` — обнулить len, capacity сохранить.
- `mut xs.insert(i int, item T) -> ()` — вставить по индексу.
- `mut xs.remove(i int) -> Option[T]` — удалить по индексу.

**Итерация:**
- `xs.iter() -> Iter[T]` — итератор по элементам.
- `for x in xs { ... }` — синтаксический сахар над `iter()`.

**Доступ:**
- `xs[i]` — индексирование, panic при out-of-bounds (D13).
- `xs.get(i int) -> Option[T]` — безопасный доступ.

**Slicing (если есть):**
- `xs[a..b]` — slice. Возвращает `[]T` без копирования (zero-cost).
  Не зафиксировано.

### Q-array-api.2. Можно ли расширять `[]T` методами через `fn []T @custom()`

Да — программист может объявить собственный метод на `[]T`, как на
любом типе:

```nova
fn []T @sum_int() -> int where T = int =>     // bound пока нет, см. Q-bounds
    @fold(0) { (acc, x) => acc + x }

fn []f64 @average() -> f64 =>
    @fold(0.0) { (a, x) => a + x } / (@len as f64)
```

**Это валидно по [D35](decisions/03-syntax.md#d35)** — методы на типе
через `fn Type @method`. `[]T` — тип, расширение работает. Нужно
зафиксировать формально, что **встроенные конструкции** (массивы,
tuples) подлежат расширению так же, как именованные типы.

### Q-array-api.3. `use []T` в record (D39 на встроенные типы)

Может ли record-тип использовать `use []T` для прокси-делегации?

```nova
type Holder[T] {
    use data []T
    extra str
}

let h = Holder[int] { data: [1, 2, 3], extra: "info" }
let n = h.len             // прокси к data.len через D39
h.push(42)                 // прокси к data.push
```

[D39](decisions/02-types.md#d39) написан под именованные типы
(`use Account`). Распространение на встроенные конструкции (`[]T`,
tuples) — естественное расширение, но не зафиксировано формально.

См. clarification в D39.

### Q-array-api.4. `[]T.alloc(n)` vs `[]T.with_capacity(n)` — разница

Из текущих примеров используются оба:

- `[]Slot[K, V].with_capacity(cap)` ([decisions/03-syntax.md → D38](decisions/03-syntax.md#d38)).
- `[]T.alloc(n)` (мой usage в `examples/stdlib_vec.nv`).

Если оба существуют:
- `with_capacity(n)` — len=0, cap=n. Push не реаллокирует, пока len < cap.
- `alloc(n)` — len=n, cap=n. Все элементы инициализированы default-T.

Для default-T нужен механизм default-значения. Либо `Default`-protocol,
либо требование bound'а на T. **Не зафиксировано.**

Возможно, `alloc(n)` вообще не нужен — пользовательские структуры
(HashMap, Vec) делают `with_capacity` и заполняют сами.

### Q-array-api.5. Slicing — есть ли `xs[a..b]` ✅ ЗАКРЫТО Plan 96 / D144

Range-индексирование `xs[a..b]` реализовано Plan 96 (2026-05-23) —
sub-slice view, без копии backing. 5 форм Range (Rust `RangeBounds`
parity): `a..b`/`a..=b`/`a..`/`..b`/`..`. Также `str[a..b]`
(codepoint-indexed, panic при OOB). Полная семантика —
[D144](decisions/02-types.md#d144).

### Решение пока

Не вводить отдельный D — это часть **Q9 (stdlib)**. При работе над
stdlib уточнить полный API `[]T` и зафиксировать одним D-блоком.

Сейчас: считать API `[]T` де-факто включающим
`len`/`cap`/`push`/`pop`/`with_capacity`/`iter`/`get`/`[i]` (по
текущим примерам). Расширение через `fn []T @method` разрешено по
D35. Embed `use []T` в record — clarification в D39.

**Связь:** [D27](decisions/03-syntax.md#d27) (синтаксис массивов),
[D32](decisions/02-types.md#d32) (runtime-представление),
[D35](decisions/03-syntax.md#d35) (методы на типе),
[D38](decisions/03-syntax.md#d38) (turbofish для конструкторов),
[D39](decisions/02-types.md#d39) (use-delegation), Q9 (stdlib),
Q-bounds (где `[]T` методы используют T-constraint'ы).

---

## Q-embed-syntax. Embed-keyword — `use` vs альтернативы

**Контекст.** [D39](decisions/02-types.md#d39) (revised) фиксирует
embed через `use name Type` — alias-имя поля **обязательно**:

```nova
type AuditedAccount {
    use account Account                // имя поля = "account"
    audit_log []AuditEntry
}
```

Этот вопрос — про **выбор keyword'а** (`use`), не про обязательность
имени (она зафиксирована в D39).

`use` — multi-purpose: и для embed здесь, и потенциально для
импортов/локальных алиасов в будущем (D29 использует `import`, но
`use` тоже рассматривался — например, как Rust `use std::io`). Это
создаёт **перегрузку семантики keyword'а**.

### Альтернативы

#### A. Текущий D39 — `use name Type`

```nova
type AuditedAccount {
    use account Account
    audit_log []AuditEntry
}
type Wrapper { use w HashMapIter[K, V] }
```

**За:** проверенный, кода уже написано.
**Против:** `use` многозначен (embed, потенциально импорты, scope-
локальные aliases).

#### B. Go-style — голый тип без keyword (с обязательным alias)

После нового D39 имя поля обязательно везде, поэтому Go-style без
keyword'а выглядел бы так:

```nova
type AuditedAccount {
    account Account                    // обычная запись поля!
    audit_log []AuditEntry
}
```

Но это **превращает embed в обычное поле** — синтаксически
неотличимо. Для активации delegation нужен **специальный токен**.
Голый тип (Go-style без keyword'а и без alias'а) **несовместим** с
обязательным alias'ом из D39 — теряется единственный синтаксический
маркер «это embed, а не обычное поле».

Чтобы спасти этот вариант, нужен какой-то маркер:

```nova
type AuditedAccount {
    account = Account                  // `=` как маркер embed?
    audit_log []AuditEntry
}
```

Но `=` уже занят (присваивание в `let`, alias в `type X alias Y`).
Нет хорошего символа.

**Против:** обязательность alias'а из D39 сделала Go-style не
применимым без явного keyword'а. Голый embed теряет различие с
обычным полем.

#### C. `embed name Type` — отдельный keyword

```nova
type AuditedAccount {
    embed account Account
    audit_log []AuditEntry
}
type Wrapper { embed w HashMapIter[K, V] }
```

**За:**
- Keyword **точно** описывает намерение. `embed` однозначен,
  `use` — общий.
- Освобождает `use` для других целей (scope-aliases, импорты в
  блоке).
- AI-locality высокая.

**Против:**
- Ещё один keyword в языке.
- Очень похоже на A синтаксически — выигрыш только в семантической
  точности слова (одна роль вместо потенциально нескольких).

### Отвергнутые альтернативы

- **`name : Type`** через `:` — конфликт с [D17](decisions/02-types.md#d17)
  (Nova явно отвергла `:` в type annotations).
- **`Type + {...}` (intersection)** — конфликт с [D46](decisions/03-syntax.md#d46)
  operator overloading (`+` = `@plus`).
- **`extends Type`** — обещает наследование (Java/C# семантика), а
  D39 — delegation, не наследование. Вводит в заблуждение.
- **`~Type`** — конфликт с removed memory prefix (`~T`/`~&T` из
  отменённого D21), путает ветеранов.
- **`@embed Type`** — `@` уже значит self-method/field в [D35](decisions/03-syntax.md#d35),
  перегрузка значения.
- **`+Type`** — конфликт с унарным `+`, не принято в mainstream.

### Сравнение топ-3

| Аспект | A. `use Type` | B. голый `Type` | C. `embed Type` |
|---|---|---|---|
| Keyword | `use` (multi-purpose) | (нет) | `embed` (специфичный) |
| Длина | средняя | короткая | средняя |
| Прецедент | D `mixin`, partial Rust | Go | OCaml `include` (схожая идея) |
| AI-locality | высокая | средняя | высокая |
| Парсер | прямолинейный | lookahead по case | прямолинейный |
| `use` нужен для импортов? | **да, занят** | свободен | свободен |

### Решение пока

Не менять. D39 принят, кода с `use` написано (примеры, decisions).
Менять синтаксис без сильного триггера — лишний breaking change.

**Update 2026-05-08:** D39 расширен формой `use _ Type` (anonymous
embed) для simple wrappers где явный alias бессмысленный. Это
**снимает часть давления** на keyword `use` — программист не
вынужден придумывать имя поля каждый раз. Q-embed-syntax по-прежнему
открыт про выбор `use` vs `embed` keyword'а, но anonymous form
закрыла главный pain-point обязательного alias'а в simple cases.

Реализация anonymous embed — Plan 11 Ф.9 (через override-precedence
в общем overload-resolution, lazy mechanism).

**Если возвращаться** — мой собственный голос за **C (`embed Type`)**:

- Точная семантика keyword'а («embed» однозначно говорит «встроить»,
  тогда как `use` это и многое другое).
- Освобождает `use` для других целей (потенциально — local-aliases
  типов, импорты в скоупе функции).
- Совместимо с alias-формой через `as` в Go-style — при желании
  программиста.

Триггеры для пересмотра:
- Если в реальном Nova-коде накопится боль от перегрузки `use`
  (программист путает embed и импорт).
- Если `use` потребуется для другой семантики (например,
  using-statement из C# для эффект-handler'ов в `with`-альтернативе).

**Связь:** [D39](decisions/02-types.md#d39),
[D29](decisions/07-modules.md#d29) (импорты — потенциальный второй
пользователь `use`), [D17](decisions/02-types.md#d17) (record-форма),
[D52](decisions/02-types.md#d52) (kind-токены — `embed` встал бы
наряду с `alias`).

---

## Q-positional-partial-pattern. `..` для позиционных конструкторов sum ✅ ЗАКРЫТО ([D59](decisions/03-syntax.md#d59))

[D59](decisions/03-syntax.md#d59) формализовал partial-pattern `..`
для **трёх контекстов одновременно** — record (`{ field, .. }`,
наследие D17/D52), позиционные конструкторы sum (`Cons(..)`,
`Move(x, ..)`) и массивы (`[head, ..]`, `[a, .., z]`). Единый `..`
маркер «остальные элементы игнорируются».

Также формализованы array-patterns (`[]`, `[r]`, `[a, b]`, slice-
bind `[head, ..rest]`) и tuple-patterns (`(a, b)`, `(a, _, c)`,
destructuring `let`).

Контекст ниже сохранён как историческая справка.

---

### Исходный контекст Q-positional-partial-pattern

**Контекст.** [D17](decisions/02-types.md#d17) фиксирует partial
pattern matching **только для record-формы**:

```nova
type Shape | Circle { radius f64 } | Square { side f64 }

match shape {
    Circle { radius, .. } => 3.14 * radius * radius      // .. — остальные поля
    Circle { radius }     => 3.14 * radius * radius      // эквивалент
}
```

Для **позиционных** конструкторов (`Cons(T, LinkedList[T])`,
`Click(int, int)`, etc.) текущий синтаксис требует placeholder для
каждого поля:

```nova
type LinkedList[T] | Empty | Cons(T, LinkedList[T])

match list {
    Empty       => true
    Cons(_, _)  => false              // два `_` для двух полей
}
```

При большем числе полей растёт шум: `Click(int, int) | Move(int,
int, int) | Scroll(int)` — `Click(_, _)`, `Move(_, _, _)`,
`Scroll(_)`. Программист пишет «не интересуют поля» N раз.

### Предложение

Расширить `..` partial-pattern на позиционные конструкторы:

```nova
match list {
    Empty     => true
    Cons(..)  => false              // partial: все поля игнорируются
}

match event {
    Click(..)   => "click"           // не важны координаты
    Move(x, ..) => "move at ${x}"    // важна только первая
    _           => "other"
}
```

**Правила (предлагаемые):**

1. `Cons(..)` — все поля игнорируются (как `Cons(_, _)` сейчас).
2. `Move(x, ..)` — первое поле в bind, остальные игнорируются.
3. `Move(.., z)` — последнее в bind, начальные игнорируются.
4. `Move(x, .., z)` — первое и последнее, среднее игнорируется.

### Прецеденты

- **Rust:** `..` работает в обеих формах (`Variant(_, _)` и
  `Variant(..)`), `Variant(x, ..)`/`Variant(.., x)` тоже.
- **Swift:** `case .variant(_, _, _)` явно, `..` нет — все
  поля прописываются.
- **OCaml:** `Cons (_, _)` явный wildcard, нет `..` для tuple.
- **Haskell:** wildcard `_` для каждого поля.

**Rust — единственный** mainstream-прецедент. Но Rust-сообщество
любит `..` — частая идиома.

### Цена

1. **Новая форма pattern.** Парсер должен различать `Variant(..)`,
   `Variant(x, ..)`, `Variant(.., x)`, `Variant(x, .., y)`.
2. **Конфликт с record-формой `..`.** В `{ field, .. }` `..` стоит
   после `,`. В позиционной `(x, ..)` тоже после `,`. Парсер
   различает по виду внешних скобок (`{}` vs `()`), что согласовано
   с D17.
3. **Тонкость с одним полем:** `Variant(..)` для конструктора с
   одним полем эквивалентно `Variant(_)`. Скорее всего разрешено.

### Решение пока

Не вводить формально. **Текущий код использует `Cons(..)` идиому
неформально** ([examples/stdlib_linkedlist.nv](../examples/stdlib_linkedlist.nv))
— ожидая, что D17/D52 будет расширен. До формализации `Cons(..)`
**фактически** работает по интуитивному правилу «`..` означает
"остальное игнорируется"», но компилятор может потребовать
строгую форму D17. При работе над парсером — зафиксировать в
revision к D17 или D52.

**Связь:** [D17](decisions/02-types.md#d17) (partial pattern для
record), [D52](decisions/02-types.md#d52) (sum-варианты), [D19](decisions/03-syntax.md#d19)
(match-arms через `=>`).

---

## Q-static-method-protocol. Static-методы в protocol через `.name()`-префикс

> ✅ **РЕШЕНО 2026-05-22 (Plan 97).** Принято предложение из этого
> вопроса: static-методы в `type X protocol { ... }` маркируются
> **точка-префиксом** `.name()`, по симметрии с
> [D35](decisions/03-syntax.md#d35) (`fn Type.name(...)` в реализации).
> Формализовано в [D143](decisions/03-syntax.md#d143) (parse rule,
> matching rules, backwards-compat: bare имя — instance, как было).
> Реализовано в Plan 97 Ф.1 (parser + AST `EffectMethod.is_static`).
> Применено в prelude: `From[T] { .from(t T) }`, `TryFrom[T, E]
> { .try_from(t T) -> Result[Self, E] }` (см. `std/prelude/protocols.nv`).
>
> **Note on hard runtime-enforcement:** парсер принимает `.name()`,
> AST хранит `is_static: bool`. Type-checker строгое сопоставление
> static↔instance при satisfaction-проверке — **deferred** (Plan 15
> Ф.5+ или отдельный follow-up). См.
> `docs/simplifications.md#m-protocol-static-enforcement-deferred`.
>
> Историческое DEFER (предыдущее) — снято: Plan 15 закрыт, Plan 59
> мономорфизировал Result, что покрывает основные use-case'ы
> (`From`/`Into`/`TryFrom`/`TryInto`).

**Контекст.** [D42](decisions/02-types.md#d42)/[D53](decisions/02-types.md#d53)
описывают protocol с **instance-методами** (без префикса):

```nova
type Hashable protocol {
    hash() -> u64
    eq(other Self) -> bool
}
```

Реализация — через [D35](decisions/03-syntax.md#d35) `@`-методы.

Для **static-функций** (конструкторов, factory-функций) protocol
сейчас не предусмотрен. Это блокирует, например, generic `collect`:

```nova
type FromIter[T] protocol {
    .from_iter(it Iter[T]) -> Self      // static-функция-конструктор
}

fn Iter[T] @collect[Out: FromIter[T]]() -> Out =>
    Out.from_iter(@)
```

### Предложение

Расширить protocol-синтаксис: **точка-префикс** (`.name()`) маркирует
static-функцию (по симметрии с [D35](decisions/03-syntax.md#d35)
`fn Type.name(...)` — точка в реализации).

```nova
type FromIter[T] protocol {
    .from_iter(it Iter[T]) -> Self      // static — через точку
    @count() -> int                      // instance (если нужен @ symmetry,
                                          //  Q-protocol-method-prefix)
    method() -> bool                      // instance (текущее, без префикса)
}
```

Реализация (структурно):

```nova
type Vec[T] { data []T }
fn Vec[T].from_iter(it Iter[T]) -> Vec[T] => ...

// Vec[T] автоматически удовлетворяет FromIter[T]
```

### Минусы

- Тонкость грамматики: точка в protocol-блоке как маркер.
- Связано с Q-collect-mechanism — без bound'ов на дженериках
  (Q-bounds) generic-collect не работает даже со static-protocol.
- `Self` в protocol — концепция уже есть, но в static-контексте
  означает «конкретный реализующий тип» (как Swift `Self` в
  protocol).

### Решение пока

Не вводить. Когда понадобится `collect`/`from_iter`-style generic-
конструкторы — вернуться. Связано с Q-bounds, Q-collect-mechanism.

**Связь:** [D35](decisions/03-syntax.md#d35) (точка для static),
[D42](decisions/02-types.md#d42) (protocol),
[D53](decisions/02-types.md#d53), Q-bounds, Q-collect-mechanism,
Q-protocol-method-prefix.

---

## Q-protocol-method-prefix. `@method()` vs голое `method()` в protocol-объявлении

> ✅ **CLOSED by [D53 → «Method-prefix в protocol-блоке»](decisions/02-types.md#d53)**
> (Plan 17 Ф.1, 2026-05-08): **обе формы валидны и эквивалентны**.
> `@method()` для визуальной симметрии с реализацией; голое `method()`
> для краткости. `mut @method()` обязательно с `@`. Bootstrap парсит
> обе формы; std/testing/property.nv использует голую.

**Контекст.** Сейчас в protocol-блоке instance-методы пишутся **без
префикса**:

```nova
type Hashable protocol {
    hash() -> u64                    // instance, без префикса
    eq(other Self) -> bool
}
```

В реализации — **с `@`**:

```nova
fn User @hash() -> u64 => ...
```

**Асимметрия:** declaration без `@`, definition с `@`. Программист
мысленно сопоставляет.

### Предложение

`@` обязателен и в protocol-объявлении — для **полной симметрии**:

```nova
type Hashable protocol {
    @hash() -> u64                   // instance — @, как в реализации
    @eq(other Self) -> bool
    .new() -> Self                    // static — точка (Q-static-method-protocol)
    mut @push(item T) -> ()           // mut instance
}
```

### За

- **Полная симметрия** declaration ↔ definition.
- **Меньше неявности** — программист не помнит, что без префикса в
  protocol = instance.
- **AI-friendly** — точно как реализация.

### Против

- **Breaking change** — все 16+ protocol-объявлений переписать.
- **Шум** — `@hash()` чуть длиннее `hash()`.
- В существующих языках (Swift, Rust, Kotlin) protocol/trait не
  использует self-маркер в declaration — convention.

### Решение пока

Не менять. Текущая асимметрия живёт. Программист привыкает.
Возможен пересмотр вместе с Q-static-method-protocol — если вводим
точку для static, можно добавить `@` для instance ради консистентности.

**Связь:** [D42](decisions/02-types.md#d42),
[D35](decisions/03-syntax.md#d35), Q-static-method-protocol.

---

## Q-collect-mechanism. Generic collection construction

**Контекст.** В Rust:

```rust
let v: Vec<i32> = (0..5).collect();
let s: HashSet<i32> = (0..5).collect();
```

Один метод `collect`, целевой тип выводится из контекста или
передаётся через turbofish (`collect::<Vec<_>>()`). Универсальный
collection-builder.

В Nova через D58 `Iter[T]` есть, но **универсальный `collect` не
работает** без:

1. **Bound'ов на дженериках** — `Out: FromIter[T]` (Q-bounds, отвергнуты
   в MVP).
2. **Static-method в protocol** — `FromIter[T] { .from_iter(...) -> Self }`
   (Q-static-method-protocol).
3. **Type-as-value** или turbofish для передачи целевого типа.

### Альтернативы

#### A. Конкретные методы — без collect

```nova
fn Range @to_vec() -> []int
fn Range @to_set() -> Set[int]
fn Range @to_linked_list() -> LinkedList[int]
```

N методов для N целей. Простой, рабочий, без bound'ов.

#### B. Turbofish + bound (Rust-style)

```nova
fn Iter[T] @collect[Out]() -> Out where Out: FromIter[T] => ...
let v = (0..5).collect[[]int]()
```

Требует Q-bounds + Q-static-method-protocol.

#### C. Type-as-value (Swift-style)

```nova
let v = (0..5).collect([]int)        // []int как «type-callable»
```

Тип в позиции аргумента вызывает type's `from_iter`. Требует Q-type-as-value.

#### D. Передача функции явно

```nova
let v = (0..5).collect(([]int).from_iter)
```

Длинно, но без новых концепций.

### Решение пока

A в MVP — конкретные `to_vec`, `to_set`, etc. на каждом типе-
итераторе. B/C/D — после Q-bounds/Q-static-method-protocol/Q-type-
as-value.

**Связь:** Q-bounds, Q-static-method-protocol, Q-type-as-value,
[D58](decisions/03-syntax.md#d58).

---

## Q-type-as-value. Передача типа как значения (`xs.collect([]int)`)

**Контекст.** В Swift:

```swift
let v = Array(0..<5)              // Array — «type как callable»
```

Тип-имя в позиции функции — вызывает соответствующий `init`.

В Nova сейчас типы — **compile-time сущности**. Передавать `[]int`
как значение в `()`-аргументе **не работает**:

```nova
fn collect[Out](ctor SomeProtocol) -> Out => ...
collect([]int)                    // []int это тип, не значение — ошибка
```

Turbofish работает (`collect[[]int]()`), но **передача в
`()`-аргументе** требует механизма «type as callable».

### Предложение

`Type` в позиции выражения вызывает соответствующий конструктор по
convention:

- `[]int` = type-callable, эквивалентно `[]int.from_iter` или
  `[]int.new` (выбор по сигнатуре).
- `User` = type-callable, эквивалентно `User.new` или общему
  конструктору.

### Минусы

- **Type-resolution полнее.** Какой конструктор выбирается — `from_iter`?
  `new`? Зависит от target-типа в позиции? Сложно.
- **Прецеденты ограничены** — Swift, Python (`list(...)`), но не Rust/
  Kotlin/Go.
- **Type-as-value в runtime** — требует runtime-tag типа (как Swift
  Mirror, Java reflection).

### Решение пока

Не вводить. Если когда-то понадобится для эргономики `collect` —
вернуться вместе с Q-collect-mechanism.

**Связь:** Q-collect-mechanism, [D38](decisions/03-syntax.md#d38)
(turbofish — текущая альтернатива).

---

## Q-range-extras. Reverse и step для Range

**Контекст.** [D58](decisions/03-syntax.md#d58) ввёл базовый Range
(`a..b`, `a..=b`). Не зафиксировано:

1. **Reverse range** — `5..0` (start > end). Что значит:
   - Пустой range (Rust-style — для прямого направления)?
   - Идущий назад (5, 4, 3, 2, 1, 0)?
2. **Step** — итерация с шагом, `(0..100).step(10)` или `0..100..10`?

### Прецеденты

- **Rust:** `5..0` — пустой; reverse через `(0..5).rev()`. Step через
  `step_by(n)`.
- **Python:** `range(5, 0)` — пустой. `range(5, 0, -1)` — обратный.
  Step через третий аргумент.
- **Kotlin:** `5 downTo 0` — отдельный keyword. Step через `step(n)`.
- **Swift:** `stride(from: 0, to: 100, by: 10)` — отдельная функция.

### Решение пока

Не зафиксировано. Реализуется в `examples/stdlib_range.nv` как
методы:

```nova
fn Range @reverse() -> Range
fn Range @step(n int) -> StepIter
```

Конкретный синтаксис — после первой версии Range (см. examples).

**Связь:** [D58](decisions/03-syntax.md#d58).

---

## Q-resume-semantics. Семантика `resume` в handler-method'е ✅ ЗАКРЫТО ([D61](decisions/04-effects.md#d61))

> Закрыт через [D61](decisions/04-effects.md#d61) в варианте **(II)
> tail-only**: `resume` как keyword **отвергнут**, заменён на
> комбинацию `return v` (нормальное завершение, continuation
> возобновляется) + `interrupt v` (досрочное прерывание with-блока).
> Линейность — one-shot. Multi-shot отложен под Q-multishot-resume
> (если когда-нибудь потребуется backtracking-эффект). Для never-операций
> разрешён только `interrupt`. См. D61 «Алгоритм компиляции/интерпретации
> эффектов» — пошаговое тех-задание имплементатору.
>
> Оригинальный текст ниже сохранён для истории.



**Контекст.** Все handler-литералы в spec'е и examples массово
используют `resume(value)` для возобновления континуации операции
(`query(q) => resume(real.query(q))`, `log(msg) { ... ; resume(()) }`).
Но **формального D-блока про `resume`** не существует — только
фрагментарные упоминания в [D10](decisions/01-philosophy.md#d10) и
[D31](decisions/04-effects.md#d31). Программист, читающий spec, не
знает:

1. Что **формально означает** `resume(v)`? Возвращает ли он что-то?
2. **One-shot или multi-shot?** Можно ли вызвать `resume` дважды?
   Что произойдёт?
3. **Тип `resume`.** `fn(R) -> ()` или `fn(R) -> T_with_block`?
4. Что если `resume` **не вызван**? handler возвращает за весь
   `with`-блок? Какое значение?
5. **Запрещён ли `resume`** для `never`-операций (`Fail.throw`)?
6. **`resume()` без аргумента** для unit-операций — сахар или
   обязательная форма?

### Ключевые design choices

#### One-shot vs multi-shot

| Модель | Прецедент | Стоимость | Use-cases |
|---|---|---|---|
| **One-shot** | Koka, OCaml 5, Eff | дёшево, stack-based | Fail, Db, Log, Time, Random — **95%** реальных эффектов |
| **Multi-shot** | Multicore-OCaml лаб. | дорого, копирование континуации | backtracking, недетерминизм, choose-effect |

Nova — backend-язык, фокус на надёжности и производительности.
Склонность — **one-shot**: вызвал `resume` дважды → runtime panic
(или compile-time error, если static-анализ позволит).

#### Тип `resume`

(I) **`fn(R) -> ()`** — handler-method заканчивается, значение `v`
становится результатом операции в бизнес-коде.
(II) **`fn(R) -> T_with_block`** — `resume` возвращает финальное
значение всего with-блока, позволяя писать «после resume» (например,
log время выполнения).

(II) мощнее, (I) проще. Koka использует (II).

#### Без resume

Если handler-method **не вызвал** `resume` — handler возвращает за
весь `with`-блок. Это типичный паттерн для `Fail`:

```nova
fn try_parse(s str) -> Option[int] =>
    with Fail[ParseError] = |_| interrupt None {
        Some(parse(s)!!)
    }
```

Здесь `|_| interrupt None` — handler-лямбда. Если бизнес-код
бросает — handler возвращает `None`, и **весь with-блок** даёт
`None`.

#### never-операции

`Fail.throw` имеет тип `never` (нет возвратного значения). `resume`
для неё **запрещён** — нечего возобновлять. Линтер/тайпчекер должен
запретить.

#### Unit-аргумент

Для операции `() -> ()` (`log(msg) -> ()`) `resume` принимает `()`:

```nova
log(msg) { println(msg); resume(()) }
```

Можно сделать `resume()` без аргумента — синтаксический сахар. Решение
зависит от того, насколько часто такое встречается (`Log.log`,
`Time.sleep` — частые).

### Что нужно зафиксировать в D-блоке

- Линейность (one-shot recommended).
- Точный тип `resume`.
- Поведение если не вызван.
- Запрет для never.
- Поведение для unit-операций.
- Что происходит при второй попытке вызова (panic vs compile-error).
- Пример каждой формы.

### Решение пока

**Не зафиксировано.** Все примеры используют `resume(...)` интуитивно
по принципу «возвращает значение в место операции». До D-блока — это
имплицитная семантика, нужная для понимания handler'ов. Обсудить и
записать в виде D-блока (вероятно, D61 после D60).

**Связь:** [D10](decisions/01-philosophy.md#d10),
[D31](decisions/04-effects.md#d31), [04-effects.md](decisions/04-effects.md).

---

## Q-handler-method-param-inference. Тип параметра handler-method'а ✅ ЗАКРЫТО ([D61](decisions/04-effects.md#d61))

> Закрыт через [D61](decisions/04-effects.md#d61) в варианте **(A)
> inference обязателен по умолчанию, явные типы разрешены опционально**.
> Параметры handler-method'а биндятся по позиции к параметрам декларации
> операции; типы автоматически выводятся из effect-декларации. Можно
> писать `query(q Sql) => ...` для документации, но это избыточно.
>
> Оригинальный текст ниже сохранён для истории.



**Контекст.** Сейчас в handler-литералах параметры пишутся **без
типа**:

```nova
with Db = Db {
    query(q) => resume(real.query(q))     // q: Sql выводится из protocol
    exec(q)  { staged.push(q); resume(()) }
}
```

Тип `q` (а также `sql, args` в старой форме `(sql, args)`) **выводится
из сигнатуры protocol'а** `Db.query(q Sql) -> []DbRow`. То же самое
делает лямбда: `(req) => handle(req)` получает тип `req` из
контекста-параметра.

Вопрос: должна ли спека **разрешать инференс**, или **требовать
явный тип** в handler-method'е?

### Аргументы

**За инференс** (текущая практика во всех ~20 примерах):
- handler-method всегда вызывается через protocol — типы фиксированы.
  Дублировать в каждом литерале — шум.
- Симметрия с лямбдой.
- Все примеры в spec/examples написаны без типов; требование явных
  типов — большой sweep.

**Против**:
- Локальное чтение хуже: `query(q) { use(q) }` — непонятно `q : ?`.
- AI-first: LLM проще генерирует с явными типами (меньше неочевидного
  контекста).
- D45 inferred return type — там inference только для return,
  параметры всегда явные. Handler-method был бы исключением.

### Возможные варианты

(A) **Инференс обязателен** — типы из protocol-сигнатуры всегда
выводятся, явные типы запрещены (избыточны).
(B) **Инференс опционален** — `query(q)` и `query(q Sql)` оба валидны.
Линтер может предлагать опускать.
(C) **Явные типы обязательны** — `query(q Sql)` всегда. Sweep всех
примеров.

(B) самый гибкий, но создаёт «два пути»; (A) самый компактный; (C)
самый локально-читаемый.

### Решение пока

**Не зафиксировано.** Все примеры используют (A)-форму неявно. До
формального решения работает «инференс из protocol-сигнатуры», но
это нужно явно зафиксировать в D-блоке (вероятно, в составе D31 или
отдельным расширением).

**Связь:** [D31](decisions/04-effects.md#d31),
[D45](decisions/03-syntax.md#d45) (inferred return type — прецедент,
но только для return).

---

## Q-fail-coercion. Auto-coercion типов ошибок при `?`-операторе

> ⏸ **DEFERRED — нужен дизайн** (Plan 17 Ф.3, 2026-05-08).
> **Rationale:** проблема **known** (без auto-coerce каждое
> разнотипное `?` требует ручной `.map_err(AppError.Variant)?`),
> но дизайн нетривиален: вариант через unary-конструктор «один
> подходящий wrap» — близкий аналог Rust `From<E>` через D73, но
> взаимодействие с D55 sum-coercion требует точной формализации
> (что приоритетнее: явный `.map_err` или auto-wrap?).
> **Trigger:** реальная боль в stdlib — когда ≥5 функций требуют
> `.map_err(...)?` boilerplate и pattern регулярен (один wrap-конструктор).
> **Варианты решения** перечислены ниже (auto-derive `From` через
> `#[from]`-маркер; явный `?.into()`; sum-type AppError с `Or<A, B>`).

**Контекст.** [D65](decisions/04-effects.md#d65) фиксирует семантику
`Fail[E]`. При транзитивном пробросе через `?` если callee бросает
`E'`, а caller декларировал `Fail[E]` (E ≠ E') — программист обязан
явно использовать `.map_err(...)` или multi-Fail в row.

```nova
type AppError
    | Parse(ParseError)
    | Db(DbError)

fn process(s str) Fail[AppError] -> int {
    let n = parse(s).map_err(AppError.Parse)?      // явный wrap
    Db.query(...).map_err(AppError.Db)?            // явный wrap
}
```

В Rust есть `From<E>`-trait, через который `?` автоматически конвертирует
тип ошибки если есть имплементация. Для Nova аналогичное правило могло
бы быть:

> Если `E` (caller's Fail) имеет ровно один sum-вариант с типом `E'`
> (callee's Fail), `?` автоматически вызывает этот вариант-конструктор:
>
> ```nova
> parse(s)?     // вместо parse(s).map_err(AppError.Parse)?
> ```
>
> compile-time проверка: вариантов с типом E' должно быть **ровно один**.
> Если несколько — ambiguous, compile error.

### За

- Убирает boilerplate `.map_err(...)` для типичных wrap'ов.
- Прецедент Rust — программисты знают.
- Compile-time проверка остаётся (один вариант — однозначно, иначе ошибка).

### Против

- **Магия**. По месту вызова `parse(s)?` неочевидно что происходит wrap.
- **AI-friendly?** LLM может не знать про auto-coercion и путаться.
- **D40-style «один способ»**. Auto-coercion + явный `.map_err` —
  два способа, неоднозначность.
- **Локальное reasoning**. С явным `.map_err(AppError.Parse)` сразу
  видно как ошибка маппится. С `?` — нужно смотреть на тип `AppError`.

### Решение пока

Не зафиксировано. Оставляется как потенциальная будущая фича.
В текущем D65 — всегда явный `.map_err(...)?` или multi-Fail в row.

**Связь:** [D65](decisions/04-effects.md#d65), [D4](decisions/04-effects.md#d4),
[D25](decisions/04-effects.md#d25).

---

## Q-pipe-operator. Pipe-оператор `|>`

> ⏸ **DEFERRED to v0.5+** (Plan 17 Ф.3, 2026-05-08).
> **Rationale:** trailing-block (D43) + method-chain через `@`-методы
> (D35) покрывают паттерн «data flow» без `|>`. Добавление оператора
> требует решения о приоритете и ассоциативности, конкуренции с
> `bitwise OR`, и effect-row inference на partial-application —
> complexity без ясного выигрыша.
> **Trigger для пересмотра:** ≥3 use-case'а в реальной кодовой базе,
> где method chain или free-function call дают объективно худший
> читаемость, и где pipe был бы естественнее.

**Контекст.** Во многих функциональных языках (Elixir, F#, Elm,
Hack, OCaml) есть pipe-оператор `x |> f |> g`, эквивалент
`g(f(x))`. Делает цепочки трансформаций линейными слева-направо.

```nova
let result = users
    |> filter(active)
    |> map(format_name)
    |> join(", ")
```

vs. без pipe:

```nova
let result = join(map(filter(users, active), format_name), ", ")
```

или через method chaining (если каждая функция — метод):

```nova
let result = users.filter(active).map(format_name).join(", ")
```

**Статус.** В bootstrap-парсере токена `|>` нет. В спеке тоже не
упомянут. Был ли намеренно отвергнут или просто не дошли руки —
не зафиксировано.

### За

- Линейная читаемость для трансформаций («data flow»).
- Не требует чтобы функция была методом типа.
- Прецедент в FP-сообществе.

### Против

- **D40 «один способ».** Уже есть method chaining — pipe это
  альтернатива, дублирующая тот же data-flow паттерн.
- **AI-friendly?** Method chaining `x.f().g()` гораздо более распространён
  в LLM-обучении (Java/Python/JS/Rust). `|>` редкий, повышает
  cognitive load.
- **Эффекты + pipe.** `users |> get_users() |> ...` — где effect-row?
  Pipe скрывает callee, осложняет вывод эффектов.
- **Партикулярно для Nova.** Все типы могут иметь `@`-методы (D35),
  поэтому method chaining — universal pattern. Pipe не добавляет
  выразительности.

### Решение пока

Не зафиксировано. Скорее всего **не добавлять** — есть method
chaining через `@`-методы, а D40 призывает не дублировать паттерны.
Если решим зафиксировать как «no» — отдельный D-блок «No pipe».

**Связь:** [D35](decisions/03-syntax.md#d35) (`@`-методы),
[D40](decisions/01-philosophy.md#d40) (один способ).

---

## Q-string-interpolation. Интерполяция строк `"hello ${name}"`

> ✅ **CLOSED by [D44 → «Строковые литералы и интерполяция»](decisions/03-syntax.md#d44)**
> (Plan 17 Ф.4, 2026-05-08): синтаксис `${expr}` (JS-style),
> escape `\${` для буквального `${`. Codegen эмитит **StringBuilder
> цепочку** с pre-size estimate (одна аллокация, без O(N²) от `+`).
> Реализован весь стек: lexer (sentinel \x01$ для escape), parser
> (sub-lex/parse каждого `${expr}`), AST (`ExprKind::InterpolatedStr
> { parts }`), codegen (StringBuilder.append + into), interp.
>
> Тесты: `nova_tests/types/string_interpolation.nv` (13 тестов:
> int / negative / str / bool / f64 / char literal / multi /
> expression in `${}` / escape / большие строки).
>
> Const-инициализатор: интерполяция **запрещена** (требует runtime
> StringBuilder); compile error.

**Контекст.** Многие современные языки имеют интерполяцию строк:

**Контекст.** Многие современные языки имеют интерполяцию строк:
```
JS: `Hello ${name}, you are ${age}`
Python f"Hello {name}, you are {age}"
Kotlin "Hello $name, you are $age"
Swift "Hello \(name)"
Rust println!("{name}")
```

В bootstrap-парсере Nova — конкатенация через `+`:
```nova
let s = "Hello " + name + ", you are " + str.from(age)
```

**Статус.** Не зафиксировано в спеке. Bootstrap не парсит интерполяцию.

### За

- Читаемость, особенно для длинных строк.
- Меньше boilerplate (`+` и `str.from(...)`).
- Универсальная фича — все программисты ожидают.

### Против

- **Сложность парсера/лексера.** Интерполяция — это string-mode +
  embedded expression mode. Усложняет грамматику.
- **Tagged templates (D-?)** — есть планы на ` `tag`...` ` с runtime'ом.
  Интерполяция и tagged templates — пересекаются (sql tag = Sql.eval
  с интерполированными частями?).
- **AI-friendly формат.** `${...}` — популярно (JS), но `\(...)` (Swift),
  `{...}` (Python) — все разные. Нужно выбрать один синтаксис.
- **Type coercion.** `"${n}"` для int — implicit `str.from(n)` (D73)?
  Или требовать явный? (Plan 17 закрыл это: `${expr}` — sugar над `str.from(expr)`.)

### Альтернативы синтаксиса

1. `"${expr}"` — JS-style (привычно большинству)
2. `"\(expr)"` — Swift (без $-конфликта со shell)
3. `"{expr}"` — Python f-string (без префикса) или `f"{expr}"`

### Решение пока

Не зафиксировано. **Скорее всего нужно** — современный язык без
интерполяции выглядит архаично. Вопрос — какой синтаксис и как
взаимодействует с tagged templates.

**Связь:** Tagged templates (нет D-блока пока), [D40](decisions/01-philosophy.md#d40).

---

## Q-coercion-order. Порядок применения coercion: sum vs record vs spread vs punning

**Контекст.** В Nova есть несколько форм неявных трансформаций
литералов в позиции с явным типом:

- **D55 literal coercion** — sum-конструкторы (`E.Variant(...)`) и
  record-литералы.
- **D60 spread** — `[...arr, x]` для массивов и `{ ...rec, k: v }`
  для записей.
- **D52 field punning** — `{ x, y }` ≡ `{ x: x, y: y }` когда
  идентификаторы совпадают с именами полей.

Композиция этих правил в одном литерале **может давать неоднозначность**
порядка применения:

```nova
let p = { ...base, x }        // что сделать первым:
                              // (a) spread → record { ...base.fields, x: x }?
                              // (b) punning x → x: x → record { ...base.fields, x: x }?
                              // обычно эквивалентно, но не всегда
```

```nova
let r RecordType = { ...base, value: 5 + 10 }
// 1. coerce 5 + 10 в тип поля value (sum-coercion если value: SomeOption)?
// 2. spread base?
// 3. record-construct?
```

### Открытые пункты

1. **Формальный порядок** — нужно зафиксировать sequence:
   spread → punning → field-coercion → sum-coercion (или другой).
2. **Многошаговая coercion**: `let r SomeRecord = { x: 5 }` где
   `SomeRecord.x: SomeSumType`. Двушаговая: int → SomeVariant → field.
   Зафиксировано ли «не более одного уровня coercion»?
3. **Type checker order**: bottom-up vs top-down — на какой стадии
   применяется coercion?

### За

- Без формального порядка LLM может генерировать «работает в одном
  направлении, не в другом».
- D55 в большом примере (`audit.nv`, `orm_demo.nv`) активно
  использует comlex coercion — нужно правило.

### Против

- Может оказаться что в реальном коде эти кейсы редки.
- Решение требует expert-внимания к type-checker дизайну (production
  компилятор).

### Решение пока

Не зафиксировано. Bootstrap применяет правила «по одному за раз»
(сначала spread, потом field-resolution, потом coercion внутри полей)
— это implementation-факт. Production должен дать явное правило.

**Связь:** [D55](decisions/02-types.md#d55), [D60](decisions/03-syntax.md#d60),
[D52](decisions/02-types.md#d52), Q-numeric-coercion, Q-style-coercion.

---

## Q-pure-view. Семантика `pure_view` для handler-state в контрактах

**Контекст.** [D24](decisions/09-tooling.md#d24) упоминает `pure_view`
как механизм ссылки на handler-state в контрактах:

```nova
fn transfer(...) Db -> ()
    ensures Db.balance(to) == old(Db.balance(to)) + amount
=> ...
```

«В v1.0 поддержка частичная — только для эффектов с явным
`pure_view` (чистая проекция состояния handler'а). Полная поддержка —
research, отдельный D-пункт после v1.0.»

Но **что такое `pure_view`** формально нигде не зафиксировано:
- Декларация: `pure_view` — это атрибут метода эффекта? Отдельная
  декларация? Свойство handler'а?
- Семантика: какие операции можно использовать в pure_view? Только
  чтение — нельзя `Db.exec(...)`?
- SMT-кодировка: как решатель переводит `Db.balance(...)` в
  uninterpreted function + axioms?
- Проверка: handler обязан реализовать `pure_view` соответствующим
  методом?

### Используется в

- [revolutionary.md R5.7](revolutionary.md) — обратимость spec ↔ impl,
  ссылается на handler-state в `ensures`.
- [revolutionary.md R4](revolutionary.md) — пример с `Db.balance` в
  ensures.
- [09-tooling.md D24](decisions/09-tooling.md#d24) — упоминание.

### За

- Без этого ключевая фича spec ↔ impl не работает на effect-методах.
- Db/Net/Time/Random — типичные handler'ы, контракты на них —
  естественны.

### Против

- Большой scope: формализация SMT-кодировки + axioms + проверка
  pure'ности.
- Связь с D62: handler — обычное значение, `Db.balance(x)` это
  вызов через handler-стек, который может меняться. Что значит
  «pure» для такого вызова?

### Решение пока

Открыто. До формализации контракты с `Db.X(...)` принимаются
грамматикой, но SMT их не доказывает → ошибка `@must_verify` или
runtime check. Production-компилятор должен дать формальное
определение.

**Связь:** [D24](decisions/09-tooling.md#d24), Q-contract-dsl,
[R5.7](revolutionary.md), [D62](decisions/04-effects.md#d62).

---

## Q-contract-dsl. Формальный contract-DSL: `result`, `old(...)`, `.is_ok`, `.is_err`

**Контекст.** В D24 «Контракты как обычная часть языка» приведены
примеры `requires`/`ensures` с использованием:
- `result` — выражение «значение, которое функция возвращает»
- `old(expr)` — значение `expr` до вызова функции
- `result.is_ok`, `result.is_err` — для функций с `Fail` эффектом
- `result.value`, `result.error` — для Result-типов?

Используется в [revolutionary.md:175,365-366,386-388](revolutionary.md)
и [09-tooling.md → D24](decisions/09-tooling.md#d24), но **формальная
семантика этих ключевых слов не зафиксирована**.

### Открытые вопросы

1. **Что такое `result` для функции с `Fail`?** В сценарии
   `fn withdraw() Fail[Overdraft] -> ()` функция формально возвращает
   `()`. `result.is_ok` означает «функция завершилась без throw».
   Но тогда `result.is_ok` для `() -> ()` всегда true (ничего не
   бросает) — что отличает от `Fail[E] -> ()` где результат может
   быть `is_err`?

2. **Семантика `result` для Result-типа.** Если функция возвращает
   `Result[T, E]`, `result.is_ok` это вызов method'а на возвращённом
   Result, а `result.value`/`result.error` — извлечение payload'а?
   Тогда два механизма (Fail и Result) дают разные значения `result.is_ok`.

3. **`old(expr)` — глубина копии.** `old(acc.balance)` копирует поле,
   `old(arr)` копирует массив или ссылку?

4. **Композиция контрактов.** Можно ли использовать другие функции
   в `requires`/`ensures` (`requires is_valid(input)`)? Если да — что
   с эффектами этой функции (она должна быть `pure`)?

### За

- Контракты — ключевая фича spec ↔ impl ([R5.7](revolutionary.md)).
- Без формализации SMT-checker и LLM-генератор не могут проверять.

### Против

- Большой scope: D24 + новый D-блок про contract DSL.
- Связь с handler-state (Q-pure-view): `Db.balance` в `ensures` —
  как handler-зависимое значение проверяется?

### Решение пока

Не зафиксировано. В примерах используется неформально. До формализации
контракты являются **рекомендацией для LLM**, не проверяемой
гарантией.

**Связь:** [D24](decisions/09-tooling.md#d24), [R5.7](revolutionary.md),
Q-pure-view (handler-state в контрактах).

---

## Q-alloc-region. Полная семантика `Alloc[R]` и связь с `region { }`

**Контекст.** В spec упомянут эффект `Alloc[R]` — аллокация в named-
региона `R` ([overview.md](overview.md), [effects.md](effects.md),
[D26](decisions/08-runtime.md#d26) prelude, [04-effects.md → D2](decisions/04-effects.md#d2)).
Параллельно [05-memory.md → D6](decisions/05-memory.md#d6) и
[06-concurrency.md → D14](decisions/06-concurrency.md#d14) объявляют
`region { ... }` как **примитив языка** (как `parallel for` / `race`).
Связь между `Alloc[R]` (эффект) и `region { }` (блок-примитив) явно
не зафиксирована.

**Что есть сейчас:**

```nova
fn alloc_in(buf []u8) Alloc[r] -> Buffer    // r — имя региона
                                              // (как параметр?)

fn map_audio(samples []f32, gain f32) -> []f32 =>
    realtime nogc {
        region {                             // примитив языка
            samples.map() { x => x * gain }
        }
    }
```

**Открытые подвопросы:**

1. **Объявление `Alloc[R]`.** Эффект параметризуется именем региона.
   Откуда берётся `R`? Это compile-time имя (как lifetime в Rust),
   тип-параметр функции, или identifier из enclosing `region { }`?
2. **Handler для `Alloc[R]`.** Кто его ставит? Блок `region { }`
   автоматически? Или программист пишет
   `with Alloc[r] = arena_handler { ... }`?
3. **Сигнатура `region { body }`.** Что body может делать с
   эффектом? Body — лямбда `fn() Alloc[r] -> T`? Тип `T` уезжает
   из региона как — копируется, references запрещены?
4. **Multi-region.** Можно ли вложить `region { region { ... } }`?
   Получится `Alloc[outer]`, `Alloc[inner]` — две арены. Как escape
   между ними? Сейчас D6 показывает sequential `let scratch = region
   { ... }; region { finalize(scratch) }`, но не вложенный случай.
5. **Связь с `realtime nogc { }`.** D64 говорит «внутри `realtime
   nogc` — только region-allocations и стек». То есть `Alloc[R]`
   в сигнатуре функции = «эта функция совместима с `realtime
   nogc { }` контекстом». Нужна формальная связь.
6. **Coercion / inference.** Если функция `f() Alloc[r] -> T`
   вызывается внутри `region { }`, должен ли компилятор автоматически
   связать `r` с регионом блока? Или программист пишет явно?
7. **Сравнение с прецедентами:**
   - **Rust** — lifetimes `'a` через borrow checker.
   - **Koka** — `<alloc<r>>` эффект в effect row.
   - **Encore** — capabilities + parallel regions.
   - **Cyclone** — region-based memory с явными аннотациями.

**Статус.** `Alloc[R]` оставлен в prelude как **зарезервированное
имя**, концептуально упомянут в декларациях, но **полная семантика
откладывается до v1.0+**. Реализация regions — пост-MVP вместе с
`realtime nogc` enforcement.

**Что делаем сейчас (MVP):**
- `Alloc[R]` остаётся в prelude-списке как заявленный эффект.
- `region { ... }` в spec упомянут, но в bootstrap/codegen не
  имплементирован (managed GC покрывает 99% backend-сценариев).
- `realtime { }` блок (D64) парсится, без compile-time enforcement.

**Решение об активации:** когда появится первый real-time use case
(audio-обработка, embedded), или когда будет реализован concurrent
GC и понадобится strict no-GC escape hatch.

**Связь:** [D6](decisions/05-memory.md#d6) (managed GC + region opt-in),
[D14](decisions/06-concurrency.md#d14) (region как примитив языка),
[D62](decisions/04-effects.md#d62) (эффекты прямые, ambient runtime),
[D64](decisions/04-effects.md#d64) (`realtime { }` / `realtime nogc { }`),
[D26](decisions/08-runtime.md#d26) (prelude — `Alloc[R]` зарезервирован).

---

## Q-record-spread-args. Spread record-литерала в позиции аргументов функции

**Контекст.** Сейчас named arguments в Nova нет. Опциональные параметры
выражаются через паттерн «опции-record + spread» ([syntax.md →
«Опциональные параметры»](syntax.md)). Но это требует **отдельного
record-типа** для каждого набора опций, что иногда избыточно.

**Предложение.** Разрешить `f(...{field1: v1, field2: v2})` —
spread record-литерала **в позиции аргументов**. Компилятор
раскладывает поля по именам параметров функции:

```nova
fn name(x int, s str) -> ()

name(...{x: 2, s: "test"})            // эквивалент name(2, "test")
name(...{s: "test", x: 2})            // порядок полей не важен (spread по именам)

let opts = { x: 2, s: "test" }
name(...opts)                          // spread существующего record'а

name(2, ...{s: "test"})               // позиционный + spread остальных
```

**Семантика:**
- `...record-expr` в позиции аргумента раскладывается по именам
  параметров функции.
- Несоответствие имён поля и параметра — compile error.
- Непокрытые параметры — compile error («missing field»).
- Можно комбинировать с обычным spread (D60) внутри:
  `name(...{ ...base, s: "override" })`.

**Преимущества:**

1. **`...` — явный маркер.** Парсер однозначен без type-directed
   parsing (в отличие от `name({...})` который мог бы быть либо
   одним record-аргументом, либо named-form).
2. **Согласовано с D60.** Spread уже расширяет литералы — теперь
   расширяет и call-site.
3. **Закрывает named arguments** без отдельной фичи.
4. **Refactoring безопасен** — добавил параметр, spread-вызовы
   получают compile error «missing field».

**Тонкости:**

1. **Конфликт с variadic spread (D69).** `f(...xs)` для variadic =
   «развернуть массив `[T]`». `f(...rec)` для record =
   «развернуть по именам». Различимы по типу spread-выражения
   (массив vs record), но требует type-check для resolution —
   мягкий type-directed step.
2. **Дублирует паттерн опций-record** в простых случаях. Если уже
   есть `fn connect(opts ConnArgs)`, то `connect(...opts_record)` ≈
   `connect(opts_record)`. Преимущество только когда не хочется
   заводить отдельный record-тип под опции (т.е. для разнородных
   параметров).
3. **Композиция с D60:** `name(...{ ...defaults, x: 9 })` —
   nested spread внутри record-литерала, потом spread record'а
   в аргументы. Двухуровневый spread.
4. **Парсер vs type-checker.** Сейчас D60 spread — чисто
   синтаксический. Здесь — **семантический**: раскладка на
   этапе type-check'а. Шаг к усложнению, но не критичный.
5. **`mut`-параметры.** `fn deposit(mut acc Account, amount money)`
   — spread `...{acc, amount}` должен сохранять `mut`-семантику.
   Технически тот же self-resolution, что и при обычном вызове.
6. **Default-значения** (если когда-то будут — отвергнуты сейчас,
   см. [history/rejected.md](decisions/history/rejected.md)). Spread
   мог бы пропускать поля без override — но без default'ов
   compile error.

**Альтернативы рассмотрены:**

- **`f({x: 1, s: "test"})` без `...`** (named arguments через D55).
  Отвергнуто: требует type-directed parsing для различения «один
  record-аргумент vs named-form».
- **Python-style `f(x=1, s="test")`** — отдельный синтаксис
  named-аргументов. Новая грамматика, конфликтует с lambda-параметрами.
- **Не вводить ничего** — паттерн опций-record уже работает. Цена —
  отдельный record-тип под каждый набор опций.

**Прецеденты:**

- **JavaScript** — `f(...obj)` spread для **массивов**, для объектов —
  только в литералах (`{...obj}`). Разворачивания в args нет.
- **Python** — `f(**kwargs)` разворачивает dict в named-args. Прямой
  прецедент семантики.
- **Ruby** — `f(**hash)` аналогично.
- **OCaml** — labeled arguments `f ~x:1 ~s:"test"` — отдельный
  механизм.

**Статус.** Не зафиксировано. Склонность — **принять** (хорошо
ложится на существующие D60/D69, явный синтаксис, закрывает named
arguments без отдельной фичи). Решение откладывается до появления
конкретного use case (когда D55+D60 паттерн опций-record окажется
многословным в реальном коде).

**Связь:** [D55](decisions/02-types.md#d55) (record-coercion),
[D60](decisions/03-syntax.md#d60) (spread в литералах),
[D69](decisions/03-syntax.md#d69) (variadic spread на call-site,
для массивов), [history/rejected.md «Default-значения параметров»](decisions/history/rejected.md).

---

## Q-math-protocol. `Float` / `Numeric` protocol для generic числового кода

**Контекст.** [D74](decisions/08-runtime.md#d74) объявляет
математические операции (`@sqrt`, `@sin`, `@cos`, `@atan2`, `@hypot`,
...) как **instance-методы** на конкретных числовых типах
(`f64`, `f32`, `int`). Generic-код, желающий работать с «любым
числом» (например, `Complex[T]`, `Vector[T]`, `Matrix[T]`), упирается
в отсутствие protocol-bound — без него `theta.cos()` на абстрактном
`T` не скомпилируется.

В D74 было прямо отвергнуто:
> **Trait-style `Float` protocol с `sin/cos/...`** (Haskell `Floating`,
> Rust `num_traits::Float`). Лишняя indirection, generics с bounds
> для каждой математической функции усложняют сигнатуры.

Решение принято для **stdlib**: математика на конкретных типах,
не через protocol. Но это блокирует **пользовательский generic-код**.

**Вопрос:** ввести ли `Float` (и `Numeric`) protocol для generic-кода?

**Варианты:**

**A. Не вводить (текущее).** Generic числовой код — невозможен без
дублирования `ComplexF32` / `ComplexF64`, `VectorF32` / `VectorF64`.
Цена — ~2x кода для каждого generic числового типа. Для stdlib
терпимо (сделано один раз). Для прикладного — программист пишет
`type ComplexF32 { ... }` отдельно.

```nova
// Текущий подход: дублирование
type Complex { re f64, im f64 }
type ComplexF32 { re f32, im f32 }      // отдельный тип
// Дублированная алгебра, дублированные методы
```

**B. Ввести только `Float` protocol для generic-кода.** Stdlib-реализация
на f32/f64 остаётся как в D74 (instance-методы), но **дополнительно**
объявляется protocol с теми же сигнатурами. Generic-код использует
protocol-bound:

```nova
type Float protocol {
    @sqrt() -> Self
    @sin() -> Self
    @cos() -> Self
    @atan2(other Self) -> Self
    @hypot(other Self) -> Self
    @abs() -> Self
    @is_finite() -> bool
    @is_nan() -> bool
    // ...
}

// Теперь generic Complex работает:
type Complex[T Float] { re T, im T }

export fn Complex[T].from_polar(r T, theta T) -> Self =>
    { re: r * theta.cos(), im: r * theta.sin() }
```

Структурно `f32` и `f64` автоматически удовлетворяют `Float` —
никаких `impl` не нужно (D53). Бесплатно для stdlib, разблокирует
пользовательский generic-код.

**C. Ввести иерархию `Numeric` ⊂ `Float` ⊂ ...** (как Haskell `Num` /
`Floating` / `Real`). Гранулярные bounds — функция использующая
только `+`/`*` требует `Numeric`, использующая `sin`/`cos` — `Float`.
Лучше типобезопасность, дороже сложность.

```nova
type Numeric protocol {
    @plus(other Self) -> Self
    @times(other Self) -> Self
    @neg() -> Self
    // ... только арифметика
}

type Float protocol {
    @sqrt() -> Self
    @sin() -> Self
    // ... тригонометрия
    // Float наследует Numeric? — открытый подвопрос protocol-наследования
}
```

**Тонкости:**

1. **Конфликт с D74 «отвергнут Float protocol».** Формулировка D74
   касалась **stdlib-реализации**. Можно уточнить: stdlib пишет
   instance-методы напрямую (никакой indirection), а **дополнительно**
   объявленный protocol существует только для **типизации generic-
   bound'ов** — без runtime-overhead через мономорфизацию.
2. **Какие методы включить в `Float`?** Полный набор D74 (~25 методов)
   делает protocol тяжёлым. Минимум: `@sqrt`, `@sin`, `@cos`, `@abs`,
   `@hypot`, `@atan2` — что нужно для `Complex` / `Vector`. Остальное
   — расширения (`FloatExtra`, `Hyperbolic`).
3. **Protocol-наследование** (Q-protocol-inheritance) — если хочется
   `Float : Numeric`, нужно решение про composition protocol'ов
   (открытый вопрос D42).
4. **`Numeric` для `int`?** int не имеет sqrt/sin, но имеет +/*/-.
   Generic-функция `sum[T Numeric](xs []T) -> T` хочет работать и
   с int, и с f64. Нужна отдельная иерархия.
5. **Прецеденты:**
   - **Rust** — `num_traits::Float`, `num_traits::Num`, целая иерархия.
     Работает, но сложно для новичков.
   - **Haskell** — `Num`, `Floating`, `Real` — каноническая иерархия.
     Известная сложность для обучения.
   - **Swift** — `BinaryFloatingPoint`, `BinaryInteger`, `Numeric` —
     протоколы в stdlib. Используется в обобщённой математике.
   - **Go** — нет (generics с bounds появились поздно). Дублирование.
   - **Julia** — `AbstractFloat`, через duck-typing. Просто, но
     слабо проверяемо.

**Связанные открытые вопросы:**

- [Q-bounds](#q-bounds) — закрыто [D72](decisions/02-types.md#d72),
  bounds разрешены.
- Q-default-generic ниже — для `Complex[T = f64]`, чтобы старый
  не-generic вызов `Complex.from(2.0)` остался работать.
- Q-protocol-inheritance — для `Float : Numeric`.

**Статус.** Не зафиксировано. Текущее (A) работает для stdlib через
дублирование. Если/когда понадобится generic числовой код в проде —
ввести **B** (минимальный `Float` protocol с 6-8 ключевыми методами).
**C** (иерархия) — отложено до накопления реальных use-case'ов.

**Связь:** [D72](decisions/02-types.md#d72) (bounds),
[D74](decisions/08-runtime.md#d74) (math на числовых типах),
[D53](decisions/02-types.md#d53) (protocol = тип),
[D42](decisions/02-types.md#d42) (protocol-наследование — открытый
подвопрос).

---

## Q-default-generic. Default-значения generic-параметров ✅ ЗАКРЫТО (2026-05-10)

> ✅ **ЗАКРЫТО → [D88](decisions/03-syntax.md#d88-default-значения-generic-параметров)** (2026-05-10).
> Триггер — [D87](decisions/04-effects.md#d87-handlere-irt--параметризация-handler-типом-interruptа)
> `Effect[E, IRT = never]`: тип handler'а должен сообщать о
> возможности `interrupt`, но обратная совместимость требует
> `Effect[E] ≡ Effect[E, never]` через default. Это и есть real
> consumer, которого ждали.
>
> Принят синтаксис из текущего раздела as-is: `[T = f64]`,
> `[T Bound = Default]`, обязательные параметры до опциональных.
> Содержимое ниже — историческое описание перед закрытием.

> ⏸ **DEFERRED — nice-to-have** (Plan 17 Ф.3, 2026-05-08).
> **Rationale:** прецеденты есть (Rust, C++, TS), синтаксис чистый
> (`[T = f64]`), но **в Nova сейчас нет generic Complex / Vector /
> Matrix** — главного use-case'а. Без real consumer'а добавлять
> сложность вывода типов (default vs inference из аргументов) —
> overengineering.
> **Trigger:** появление generic math-типов (Q-math-protocol) или
> первый кейс «добавить параметр к существующему типу не ломая
> caller'ов» в stdlib.

**Контекст.** Сейчас generic-параметры объявляются без default-значений
(D16: `[T]`). Если добавить generic к существующему типу
(`Complex` → `Complex[T]`), все существующие вызовы ломаются —
программист обязан указать `[T]` явно.

**Предложение.** Разрешить default-значение в generic-объявлении,
чтобы можно было параметризовать тип/функцию без breaking change'а
для существующих использований:

```nova
type Complex[T = f64] {
    re T
    im T
}

// Старые вызовы продолжают работать без [T]:
let z = Complex.from(2.0)             // T выводится как f64 из default
let z Complex = Complex.new(1.0, 2.0)  // тип Complex (без скобок) → Complex[f64]

// Новые — с явным параметром:
let z32 Complex[f32] = Complex.new(1.0_f32, 2.0_f32)
```

**Синтаксис:** `[T = f64]` — без двоеточия (стиль Nova).
Альтернатива `[T default f64]` — длиннее, без выгоды.

**Семантика:**
- `Complex` без скобок ≡ `Complex[f64]` (default подставляется).
- `Complex[f32]` — явная инстанциация.
- Default — **тип**, должен быть уже определён (никаких forward references).

**Использование с алиасами:**

```nova
type Complex32 alias Complex[f32]
type Complex64 alias Complex[f64]      // эквивалент `Complex` без скобок
```

**Тонкости:**

1. **Парсер.** `[T = f64]` — после имени параметра идёт `=` потом
   тип. Грамматически чисто (нет конфликтов с другими `=` в Nova,
   потому что generic-список окружён `[]`).
2. **Несколько параметров с default.** `[K = str, V = int]` —
   все опциональны, можно не указывать ни одного. Если только часть
   с default — обязательные **должны идти до** опциональных
   (`[T, U = f64]` ✅, `[T = f64, U]` ❌).
3. **Default через bound.** `[T Float = f64]` — bound `Float` +
   default `f64`. Парсер: `name bound = default`.
4. **Inference vs default.** Если компилятор может вывести `T` из
   аргументов — default не нужен:
   ```nova
   fn first[T = int](xs []T) -> Option[T]
   first([1, 2, 3])             // T = int (вывод из []int, не default)
   first[]([])                   // []? — пусто, default не помогает
                                  // (тип []T неизвестен)
   ```
   Default — это «когда не выводится и не указан».
5. **Прецеденты:**
   - **Rust** — `struct Vec<T, A: Allocator = Global>`. Работает.
   - **C++** — `template<typename T = int>`. Работает.
   - **TypeScript** — `interface Foo<T = string>`. Работает.
   - **Swift, Kotlin** — нет default-параметров для generic'ов.
   - **Java** — нет.

**Конфликт с D9 «один очевидный путь»:**

Default делает `Complex` и `Complex[f64]` эквивалентами — это нарушает
«один путь». Но это **не выбор для программиста** (как с default
arguments — там программист может опустить или передать), это
**сокращённая запись**. Аналогично D58 implicit iter в `for`-loop
(`for x in xs` ≡ `for x in xs.iter()`) — формально два пути, но
семантически тот же.

**Решает реальную проблему:**

Без default — добавление generic к существующему типу = breaking
change. С default — backward-compatible эволюция API. Это важно
для долгоживущей stdlib.

**Тонкость с D52 (newtype):**

```nova
type Complex Complex[f64]      // ОШИБКА: парсер не знает, это newtype
                                // или alias-без-keyword
type Complex alias Complex[f64]  // ok: явный alias
```

Default-параметры **дополняют** alias-механику, не заменяют. Алиасы
полезны для **конкретных** инстанций (`Complex32`, `ResultStr`),
default — для **самой частой** инстанции «по умолчанию».

**Статус.** Не зафиксировано. Полезно для:
- Backward-compat при добавлении generic к существующим типам.
- `Complex[T = f64]`, `HashMap[K, V, S = DefaultHasher]` (если будет
  hasher-параметр), `Result[T, E = Error]` (если хочется упростить
  частый случай).

**Решение** — отложено до появления конкретного use case (например,
generic Complex / Vector / Matrix через [Q-math-protocol](#q-math-protocol)).

**Связь:** [D16](decisions/03-syntax.md#d16) (generic `[T]`),
[D52](decisions/02-types.md#d52) (newtype, alias),
[D72](decisions/02-types.md#d72) (bounds — комбинация
`[T Bound = Default]`),
[Q-math-protocol](#q-math-protocol) — главный use case.

---

## Q-char-literals. Синтаксис char-литералов ✅ ЗАКРЫТО (2026-05-07)

> Реализовано предложенное в Q-char-literals: lexer recognizes
> `'a'` / `'\n'` / `'\u{HEX}'` → TokenKind::Char(u32) (codepoint).
> AST: ExprKind::CharLit + Literal::Char (для match-pattern'ов).
> Codegen: char как nova_int в bootstrap (codepoint напрямую). См.
> nova_tests/types/char_literals.nv (16 тестов).
>
> Оригинальный текст ниже сохранён для истории.

**Контекст.** В prelude есть тип `char` ([D26](decisions/08-runtime.md#d26)),
он используется в сигнатурах: `fn s @chars() -> Iter[char]`,
`fn s @char_at(i int) -> Option[char]`. Однако **синтаксис
char-литерала** (`'a'`, `'\n'`, `'é'`) в спеке не описан.

Это пробел: код вида `match c { '0'..='9' => ... }` или
`if c == '"'` нужен в практических парсерах ([std/encoding/json.nv](../std/encoding/json.nv),
[std/math/complex.nv](../std/math/complex.nv) и т.д.),
но формально не определён.

**Предложение.** Добавить char-литералы стандартного вида:

```nova
let c char = 'a'
let nl = '\n'
let backslash = '\\'
let quote = '\''
let unicode = 'é'                // é
let emoji = '\u{1F600}'              // 😀, escape с {} для codepoint > 0xFFFF
```

**Грамматика:**

```
char-literal = "'" ( raw-char | escape ) "'"
escape       = '\\' ( "'" | '"' | '\\' | 'n' | 'r' | 't' | 'b' | 'f' | '0'
                    | 'u' hex4
                    | 'u{' hex+ '}' )
hex4         = hex hex hex hex
```

**Тонкости:**

1. **Конфликт с tuple-индексом?** `t.0`, `t.1` (D37) использует точку
   и цифру. Char-литерал начинается с `'` — нет конфликта.
2. **Конфликт со str-литералом?** `"a"` это `str`, `'a'` это `char`.
   Чёткое разделение: одинарные = char, двойные = string.
3. **Single-quote string как в Python?** Нет — Python разрешает оба
   варианта для строк. Nova использует одинарные **только** для
   char.
4. **`char` это codepoint или byte?** Codepoint (как Rust, Swift).
   Размер 4 байта (Unicode scalar). Не байт-char как в C.
5. **Range patterns** (`'0'..='9'` в match) — отдельный вопрос
   (Q-range-patterns), часто связан с char-литералами.

**Прецеденты:**

| Язык | Char-литерал | Тип |
|---|---|---|
| Rust | `'a'` | `char` (Unicode scalar, 4 байта) |
| Swift | `"a" as Character` или `Character("a")` | `Character` |
| Go | `'a'` | `rune` (int32) |
| C/C++ | `'a'` | `char` (byte) или `int` |
| Java | `'a'` | `char` (UTF-16 code unit) |
| Python | нет (всё — `str`) | — |
| OCaml | `'a'` | `char` (byte) |

Большинство языков используют одинарные кавычки. Nova следует этому
прецеденту.

**Статус.** Не зафиксировано. Текущая нужда — парсеры в stdlib
(json, complex, sql) используют char-литералы в кодекс-стиле. Bootstrap
parser char-литералы **не поддерживает** — это блокирует прогон таких
файлов. Предложенный синтаксис согласован с прецедентами и Nova-style
(escape через `\\`, `\u{...}` для extended codepoint'ов).

**Связь:** [D44](decisions/03-syntax.md#d44) (числовые литералы —
другой класс литералов), [D26](decisions/08-runtime.md#d26)
(`char` в prelude как тип), [D48](decisions/03-syntax.md#d48)
(tagged template literals — соседняя категория), Q-range-patterns
(`'0'..='9'`).

---

## Q-string-indexing. Семантика `s[i]` для `str` ✅ ЗАКРЫТО (2026-05-07)

> **Решение: вариант B (Codepoint).** В соответствии с школой B
> codepoint-indexed API (D26 пересмотрен 2026-05-07), `s[i]` —
> codepoint at index, `Option[char]`. O(n) cost — это явная цена
> школы B; для hot-path есть explicit `s.bytes()` → byte-level access.

**Контекст.** D26 фиксирует `str` как UTF-8 byte slice внутри, но
**все public operations работают на codepoint-уровне**. Что означает
`s[i]`? Три варианта были рассмотрены:

| Вариант | Семантика | Cost | Прецедент |
|---|---|---|---|
| A. Byte | `s[i]: byte` (`u8`) | O(1) | Go |
| **B. Codepoint** ✅ | `s[i]: Option[char]` | O(i) | Python |
| C. Запрещено | Только `s.bytes()[i]` / `s.chars().nth(i)` | n/a | Rust |

**Принят вариант B** — consistent со всем остальным D26 API
(s.len, s.slice, s.find, etc — всё codepoint-indexed). См. D26
«Почему codepoint-indexing (школа B) выбрана для Nova».

**Связь:** [D26](decisions/08-runtime.md#d26), Q-char-literals,
[D27](decisions/03-syntax.md#d27).

---

## Q-cstring. Гарантия nul-termination для `nova_str.ptr`

**Контекст.** Bootstrap-runtime сейчас:
- `nova_str_concat` — аллоцирует `len + 1`, кладёт `\0` после данных.
- Литералы (`(nova_str){.ptr="...", .len=N}`) — nul-terminated (C `.rodata`).
- `nova_str_slice` — НЕ добавляет `\0`, просто view.

Это **полу-гарантия**: пользователь не может надёжно передать
`s.ptr` в C-функцию без копирования.

**Варианты:**

1. **Always nul-terminated.** `slice` копирует с `\0`. Простой
   C-interop ценой O(n) на slice. Прецедент: Java Native Interface
   (через `GetStringUTFChars`).
2. **Никогда не гарантировано.** Удалить `\0` из concat, литералы —
   честный slice указатель. Любой C-interop — через явный
   `s.as_cstr() -> *const c_char` (копирует если нужно). Прецедент:
   Rust (`&str` → `CString` — явная аллокация).
3. **Текущее inconsistent.** Документировать что `\0` есть после
   литералов и concat, но не slice. Программист сам знает контекст.
   *Не рекомендуется* — путает.

**Решение (2026-05-07): вариант 2** — Rust-style. Согласуется с
принципом «нет скрытых аллокаций» и упрощает `nova_str_slice` (zero-
copy). C-interop через явный `Buffer.from(s).add_byte(0).into()` (или
будущий `s.as_cstring() -> []byte` если станет частым use-case).
Текущий bootstrap всё ещё inconsistent (concat/литералы — terminated,
slice — нет); fix — отдельная задача рантайма.

**Связь:** [D26](decisions/08-runtime.md#d26), Q-ffi (FFI-механизм
ещё не зафиксирован), Q-buffer (Buffer.add_byte для явного `\0`).

---

## Q-string-interning. Опциональное interning через Atom-тип

**Контекст.** D26 фиксирует `str` как **не интернируемый**. Это даёт
предсказуемый perf и совпадает с Rust/Go. Но кейсы AST identifiers,
JSON keys, log fields — это **много дубликатов одной строки**, где
interning экономит память и даёт O(1) `==`.

**Варианты:**

1. **Auto-intern для коротких строк.** Python-style — runtime
   автоматически интернирует строки до N байт. Скрытая cost.
2. **Явный `Atom` тип.** Erlang-style — `Atom` это always-interned
   immutable identifier. Создаётся через `Atom("foo")`, сравнение
   O(1) pointer equality.
3. **`Sym[T]` newtype.** Тоньше — пользователь сам объявляет какие
   строки интернируются (`type UserId = Sym[str]`). Управление
   pool'ом через runtime API.
4. **Просто `Rc[str]`/`Arc[str]`.** Ручная дедупликация через
   reference counting + хеш-таблицу. Самый простой вариант — нет
   нового типа, но программист сам управляет pool.

**Предложение:** отложить, по умолчанию **2 (Atom-type)** как наиболее
expressive (Erlang/Elixir опыт показывает что Atoms полезны вне
строк — для tag-types, error-codes). До решения — Nova-программист
использует `HashMap` если нужна явная дедупликация.

**Связь:** [D26](decisions/08-runtime.md#d26), [D6](decisions/05-memory.md#d6)
(GC managed heap), Q-stdlib-data-types.

---

## Q-buffer. `Buffer` — mutable byte accumulator ❌ REMOVED (2026-05-08)

> **Buffer удалён из языка полностью** в Plan 04 Этап 6 (2026-05-08).
> Заменён split'ом на StringBuilder/WriteBuffer/ReadBuffer. Никакой
> backward compatibility — Nova не в production, революционный язык
> важнее обратной совместимости.
>
> **Удаление одним коммитом:**
> - codegen dispatch удалён (record_schemas + 5 групп special-case'ов
>   в emit_call/infer).
> - `nova_rt/buffer.h` удалён.
> - `nova_tests/runtime/buffer.nv` удалён.
> - `nova_rt/nova_rt.h` — `#include "buffer.h"` удалён.
> - 14 std/ файлов мигрированы на StringBuilder (text-only sweep);
>   url.nv decode_query — на WriteBuffer + `str.try_from([]byte)?`.
>
> **Замены:**
> - text accumulation → `StringBuilder` (Q-string-builder)
> - binary accumulation → `WriteBuffer` (Q-write-buffer)
> - binary reading → `ReadBuffer` (Q-read-buffer)
> - mixed text+binary → `WriteBuffer` + `str.try_from([]byte)?`
>   (D77 — UTF-8 validate + конверсия). `WriteBuffer @write_char(c)` /
>   `@write_str(s)` добавлены для UTF-8 encode chars/strings в byte
>   buffer (Plan 04 Этап 6.1).
>
> **Buffer — неудачное решение** (попытка унифицировать text+binary
> в одном типе). Правильно заменено split'ом со специализированной
> семантикой. История unified `Buffer` (Q-buffer 2026-05-07 → ⚠️
> REPLACED 2026-05-08 → ❌ REMOVED 2026-05-08) сохранена ниже для
> понимания эволюции; **не использовать ни при каких условиях** —
> компилятор Buffer не знает.
>
> См. также: [D82](decisions/08-runtime.md#d82) (`external` keyword),
> [D26](decisions/08-runtime.md#d26) (prelude добавляет три новых типа),
> Plan 04 Этап 6 (`docs/plans/04-buffer-split-and-external.md`).

**Контекст.** D26 фиксирует что `s1 + s2` — O(a+b) per call, новая
аллокация. В hot loop `s = s + x` × N → O(N²). Это норма для
immutable strings, но требует builder для production кода.

Также для бинарных протоколов (network, serialization) нужен
аккумулятор `[]byte` с capacity-grow. Это **та же задача**: растущий
байт-буфер. Различие только в финализации: с UTF-8 валидацией → `str`,
без проверки → `[]byte`.

**Решение (2026-05-07):** один тип `Buffer` для обоих случаев. Это
шаг вперёд относительно прецедентов (которые имеют **два** типа —
`bytes.Buffer` + `strings.Builder` в Go, `Vec<u8>` + `String` в Rust).
Унификация даёт меньше API surface и одну mental model для AI-генерации.

**Реализовано (2026-05-07):** runtime `nova_rt/buffer.h`, codegen
special-case dispatch для `Buffer.new()` / `Buffer.with_capacity(n)` /
`Buffer.from(s/b)` (Path-form static) и для `buf.add_*()` / `buf.into()` /
`buf.try_into()` / `buf.into_str_unchecked()` / `buf.len()` / `buf.capacity()`
/ `buf.clone()` (Member-form instance методов на receiver-type
`Nova_Buffer*`). Tests: `nova_tests/runtime/buffer.nv` (16 passing) — basic
ops, capacity-grow, clone independence, UTF-8 add_char (1/2/4-byte),
hot-loop 1000-add accumulation. UTF-8 валидация в `try_into`
реализована вручную (overlong/surrogate detection). После consume
mutating-method даёт `nova_assert("buffer consumed: ...")` panic.

### API (финализирован, не реализован)

```nova
// Создание
Buffer.new() -> Buffer
Buffer.with_capacity(n int) -> Buffer
Buffer.from(s str) -> Buffer            // copy UTF-8 bytes
Buffer.from(b []byte) -> Buffer         // copy bytes

// Аккумуляция (mutating, @-методы; bootstrap-limit: разные имена,
// см. ниже про overload)
fn Buffer mut @add_str(s str) -> ()      // O(s.len) memcpy + grow
fn Buffer mut @add_bytes(b []byte) -> () // O(b.len) memcpy + grow
fn Buffer mut @add_byte(b byte) -> ()    // одна byte
fn Buffer mut @add_char(c char) -> ()    // UTF-8 encode 1-4 bytes

// Финализация (consume — после неё mutating-методы → runtime panic
// "buffer consumed")
fn Buffer @into() -> []byte                              // infallible — через D73
fn Buffer @into() Fail[Utf8Error] -> str                 // fallible — D73 + Fail (target str)
fn Buffer @try_into() -> Result[str, Utf8Error]          // D77 sugar — equivalent
fn Buffer @into_str_unchecked() -> str                   // escape hatch без проверки

// Note: D73 уточнение (2026-05-07) — fallible from/into через Fail-effect
// в сигнатуре. То есть `buf.into()` для target `str` декларирует
// Fail[Utf8Error] и throw'ает на невалидный UTF-8; для target `[]byte` —
// infallible. Compiler разрешает overload по target-type через D73 dispatch.
// `try_into()` остаётся как D77 convenience-sugar (Result-стиль).

// Read (без consume)
fn Buffer @len() -> int
fn Buffer @capacity() -> int
fn Buffer @clone() -> Buffer                      // для snapshot'ов
```

### Семантика

- **`@add_*`** копируют контент в internal `[]byte` (heap-allocated, 2x
  capacity grow).
- **`@into() -> []byte`** через D73 (`[]byte.from(buf Buffer)` авто-
  выводится из `Buffer @into`). Перевод ownership: после `@into()`
  любой mutating-метод → runtime panic.
- **`@try_into() -> Result[str, Utf8Error]`** через D77. Walks buffer,
  валидирует UTF-8, на успехе — zero-copy переход в `str`. Тоже consume.
- **`@into_str_unchecked()`** — escape hatch для случаев когда buffer
  **доказуемо валидный UTF-8** (например, аккумуляция только через
  `@add_str` и `@add_char` без `@add_byte`/`@add_bytes`). Без runtime
  check, дешевле.
- **`@clone()`** возвращает независимую копию buffer; для snapshot'а
  без consume.

### Bootstrap-limitation: overload по типу аргумента

Текущий codegen использует `method_receivers: HashMap<name, (recv_ty,
is_instance)>` — ключ это **только имя метода**. Это значит overload
`@add(s str)` и `@add(b []byte)` на одном receiver `Buffer` дадут
last-wins (второй перепишет первого). Поэтому в API **разные имена**:
`@add_str` / `@add_bytes` / `@add_byte` / `@add_char`.

Когда Q-overloading закроется (overload by argument type) — можно
будет уплотнить в `@add(...)`. Текущий API forward-compatible: добавить
overload-ed `@add` потом не сломает существующие `@add_str` etc.

### Конверсии str ↔ []byte (через D73)

Без `Buffer` есть простая граница:

```nova
fn []byte.from(s str) -> []byte =>
    // copy s.ptr..s.ptr+s.len
fn str.try_from(b []byte) -> Result[str, Utf8Error] =>
    // validate UTF-8, return Ok(str) или Err(Utf8Error)
```

D73/D77 авто-синтезируют `s.into()` / `b.try_into()`. Это для
**одноразовой** конверсии. `Buffer` нужен когда конкатенаций много.

### Прецеденты

| Язык | Builder/Buffer | Финализация | Особенности |
|---|---|---|---|
| Go | `bytes.Buffer` + `strings.Builder` | разные методы | разделены: bytes vs UTF-8 |
| Rust | `Vec<u8>` + `String` | `String::from_utf8` | Vec<u8> for raw, String для UTF-8 |
| Java | `ByteArrayOutputStream` + `StringBuilder` | `.toString()` | разделены |
| Python | `bytearray` + `''.join` | manual | разделены |

Nova **унифицирует** в один `Buffer` — меньше API surface, единая
mental model. Это design-выбор согласованный с principle «одна идиома
для одной задачи» (D9).

**Связь:** [D26](decisions/08-runtime.md#d26), [D73](decisions/08-runtime.md#d73)
(`From`/`Into` для финализации), [D77](decisions/08-runtime.md#d77)
(`TryFrom`/`TryInto` для UTF-8 валидации), Q-overloading (overload
по типу аргумента в `@add`), Q-array-api (`[]T.from`/`@push` general API),
Q-clone-semantics (`@clone()` deep vs shallow), Q-readonly-types
(TS-style `Readonly<T>` / `DeepReadonly<T>`).

---

## Q-string-builder. `StringBuilder` — UTF-8 string accumulator ✅ ЗАКРЫТО (2026-05-08)

**Контекст.** Replaces text-side унифицированного `Buffer` (Q-buffer).
`s1 + s2` — O(a+b) per call; в hot loop O(N²). Требуется builder.
Split на StringBuilder (text-only) + WriteBuffer (binary-only)
выявился при добавлении endianness-методов в Q-buffer — text+binary
смешение ломает API.

**Решение (2026-05-08):** отдельный тип `StringBuilder` с **append-only
text-семантикой**. `@into() -> str` **infallible** (UTF-8 invariant
поддерживается каждым `@append`). Декларации API — через
`external fn` (D82), реализация — `nova_rt/string_builder.h`.

### API (Plan 04 Этап 3)

```nova
export external fn StringBuilder.new() -> Self
export external fn StringBuilder.with_capacity(n int) -> Self
export external fn StringBuilder.from(s str)  -> Self
export external fn StringBuilder.from(c char) -> Self

export external fn StringBuilder mut @append(s str)  -> ()
export external fn StringBuilder mut @append(c char) -> ()

export external fn StringBuilder @len()      -> int
export external fn StringBuilder @capacity() -> int
export external fn StringBuilder @clone()    -> Self
export external fn StringBuilder @into()     -> str    // infallible
```

### Семантика

- **Append-only.** `@append(s)` копирует UTF-8-байты; `@append(c)` —
  encode codepoint в 1-4 байта.
- **`@into()` infallible.** UTF-8 валиден по построению (`str`/`char`
  входы) — invariant держится без runtime-check на финализации.
- **Consume.** После `@into()` любой mutating-метод → runtime panic.
- **`@clone()`** — deep копия internal byte storage.
- **2x capacity grow** — стандартное удвоение.

### Что отвергнуто

- **Объединение с `WriteBuffer` обратно** (унифицированный Buffer).
  Q-buffer закрылся как REPLACED — split лучше: type-safety +
  infallible `@into() -> str`.
- **`@append(b []byte)`** — нарушит UTF-8 invariant. Сырые байты
  → WriteBuffer.
- **`@into_str_unchecked()` escape hatch** (был в Q-buffer). Не
  нужен — построение через `@append(s|c)` уже гарантирует UTF-8.

### Прецеденты

| Язык | Тип | Финализация |
|---|---|---|
| Java | `StringBuilder` | `.toString()` infallible |
| Go | `strings.Builder` | `.String()` infallible |
| Rust | `String` (с `push_str`/`push`) | identity (уже String) |

Все three разделяют builder для строк и для байтов. Nova согласована
с этим mainstream'ом.

**Связь:** [D26](decisions/08-runtime.md#d26) (prelude),
[D82](decisions/08-runtime.md#d82) (`external` keyword),
[D73](decisions/08-runtime.md#d73) (`from(c char)` через D73),
Q-buffer (REPLACED), Q-write-buffer, Q-read-buffer.

---

## Q-write-buffer. `WriteBuffer` — binary serialization buffer ✅ ЗАКРЫТО (2026-05-08)

**Контекст.** Replaces binary-side унифицированного `Buffer` (Q-buffer).
Бинарные протоколы (network, serialization) требуют **endianness-методы**
(`write_u32_le`, `write_i64_be`, ...) — 18 числовых типов × LE/BE.
В унифицированном Buffer'е такие методы не вписывались рядом с text
(`add_str`, `add_char`).

**Решение (2026-05-08):** отдельный тип `WriteBuffer` с **endianness-aware
write-методами**. `@into() -> []byte` infallible. Декларации API —
через `external fn` (D82), реализация — `nova_rt/write_buffer.h`.

### API (Plan 04 Этап 3)

```nova
export external fn WriteBuffer.new() -> Self
export external fn WriteBuffer.with_capacity(n int) -> Self
export external fn WriteBuffer.from(b []byte) -> Self

// Bytes
export external fn WriteBuffer mut @write_byte(v byte)      -> ()
export external fn WriteBuffer mut @write_bytes(src []byte) -> ()

// 18 числовых × LE/BE (write_u8/i8 без endianness — 1 byte):
export external fn WriteBuffer mut @write_u8(v u8)           -> ()
export external fn WriteBuffer mut @write_i8(v i8)           -> ()
export external fn WriteBuffer mut @write_u16_le(v u16)      -> ()
export external fn WriteBuffer mut @write_u16_be(v u16)      -> ()
export external fn WriteBuffer mut @write_u32_le(v u32)      -> ()
export external fn WriteBuffer mut @write_u32_be(v u32)      -> ()
export external fn WriteBuffer mut @write_u64_le(v u64)      -> ()
export external fn WriteBuffer mut @write_u64_be(v u64)      -> ()
export external fn WriteBuffer mut @write_i16_le(v i16)      -> ()
export external fn WriteBuffer mut @write_i16_be(v i16)      -> ()
export external fn WriteBuffer mut @write_i32_le(v i32)      -> ()
export external fn WriteBuffer mut @write_i32_be(v i32)      -> ()
export external fn WriteBuffer mut @write_i64_le(v i64)      -> ()
export external fn WriteBuffer mut @write_i64_be(v i64)      -> ()
export external fn WriteBuffer mut @write_f32_le(v f32)      -> ()
export external fn WriteBuffer mut @write_f32_be(v f32)      -> ()
export external fn WriteBuffer mut @write_f64_le(v f64)      -> ()
export external fn WriteBuffer mut @write_f64_be(v f64)      -> ()

export external fn WriteBuffer @len()      -> int
export external fn WriteBuffer @capacity() -> int
export external fn WriteBuffer @clone()    -> Self
export external fn WriteBuffer @into()     -> []byte
```

### Семантика

- **`@write_uN_le/be`** — endianness-explicit. Программист **обязан**
  выбрать LE/BE; нет «default-endian». Это безопасно: bug-class «забыл
  endianness» исключён на API-уровне.
- **`@write_u8`/`@write_i8`** — 1 byte, endianness не нужен.
- **`@into() -> []byte`** infallible (любые байты валидны как `[]byte`).
- **Consume.** После `@into()` mutating → runtime panic.
- **2x capacity grow** — как StringBuilder.

### Что отвергнуто

- **`WriteBuffer.from(s str)`** — не вводим в MVP. Программист пишет
  `wb.write_bytes(s.bytes())` или `WriteBuffer.from(s.bytes())`. Future
  вопрос: добавить как convenience.
- **Default-endian (`write_u32` без суффикса).** Bug-class. Программист
  забывает что default может быть LE на одной системе и BE на другой;
  network-protocol code тогда ломается тихо.
- **Объединение с `StringBuilder` обратно** — split победил, см.
  Q-string-builder.
- **`write_str` метод** — нарушает binary-only семантику. Программист
  пишет `wb.write_bytes(s.bytes())` — explicit конверсия str→bytes.

### Прецеденты

| Язык | Тип | Endianness |
|---|---|---|
| Rust `byteorder` crate | `WriteBytesExt` trait | LE/BE explicit |
| Go `encoding/binary` | `binary.LittleEndian.PutUint32` | namespace per endian |
| Java `ByteBuffer` | `.order(ByteOrder.LITTLE_ENDIAN)` | mode-per-buffer |

Nova выбирает Rust-style explicit per-method — самый безопасный
(нет hidden state).

**Связь:** [D26](decisions/08-runtime.md#d26),
[D82](decisions/08-runtime.md#d82), Q-buffer (REPLACED),
Q-string-builder, Q-read-buffer, Q-overloading.

---

## Q-read-buffer. `ReadBuffer` — cursor-style binary reader ✅ ЗАКРЫТО (2026-05-08)

**Контекст.** Pair к WriteBuffer для **читающей** стороны бинарных
протоколов. View над `[]byte` с position-cursor; `@read_*` advance'ит
position. Pair `@read_*` (Fail-form, throw на end-of-buffer) /
`@try_read_*` (Result-form) — auto-derive на C-runtime уровне.

**Решение (2026-05-08):** отдельный тип `ReadBuffer`. View, не value
(нет `@into()` — явный throw блокирует D73 auto-derive). Декларации —
через `external fn` (D82), реализация — `nova_rt/read_buffer.h`.

### API (Plan 04 Этап 3)

```nova
export external fn ReadBuffer.from(b []byte) -> Self    // view, no copy

export external fn ReadBuffer @position()           -> int
export external fn ReadBuffer @remaining()          -> int
export external fn ReadBuffer @has_remaining(n int) -> bool
export external fn ReadBuffer @remaining_bytes()    -> []byte    // copy of remaining

// Throwing form (Fail[ReadBufferError])
export external fn ReadBuffer mut @read_byte()       Fail[ReadBufferError] -> byte
export external fn ReadBuffer mut @read_bytes(n int) Fail[ReadBufferError] -> []byte
export external fn ReadBuffer mut @read_u8()         Fail[ReadBufferError] -> u8
export external fn ReadBuffer mut @read_i8()         Fail[ReadBufferError] -> i8
export external fn ReadBuffer mut @read_u16_le()     Fail[ReadBufferError] -> u16
export external fn ReadBuffer mut @read_u16_be()     Fail[ReadBufferError] -> u16
// ... все 18 числовых × LE/BE

// Try form (Result[T, ReadBufferError]) — auto-derived на C-runtime уровне
export external fn ReadBuffer mut @try_read_byte()       -> Result[byte, ReadBufferError]
export external fn ReadBuffer mut @try_read_bytes(n int) -> Result[[]byte, ReadBufferError]
export external fn ReadBuffer mut @try_read_u8()         -> Result[u8, ReadBufferError]
// ... все 18 числовых × LE/BE

// Block D73 auto-derive of @into() — ReadBuffer is a view, not a value
fn ReadBuffer @into() Fail[Error] -> () =>
    throw Error.new("ReadBuffer.@into() is not supported; use @remaining_bytes()")
```

### ReadBufferError

```nova
export type ReadBufferError
    | UnexpectedEnd { wanted int, available int }
```

Будущие варианты (`InvalidFormat`, `InvalidUtf8`) — добавлять по мере
появления read-методов с этими failure modes.

### Auto-derive read/try_read на C-runtime уровне

Программист stdlib **не пишет** `@read_*` и `@try_read_*` отдельно.
**Одна C-функция** на каждый числовой × LE/BE возвращает
result-структуру:

```c
typedef struct ReadResult_u32 {
    nova_bool ok;       // 1 = success, 0 = UnexpectedEnd
    uint32_t  value;
    int64_t   wanted;   // для error
    int64_t   available;
} ReadResult_u32;
```

Codegen эмитит **обе Nova-сигнатуры** на одну C-функцию:
- `@read_u32_be` (Fail-form): проверяет `ok`, throw'ит через
  `Nova_Fail_fail` с `ReadBufferError.UnexpectedEnd { wanted, available }`.
- `@try_read_u32_be` (Result-form): wrapper упаковывает в
  `Result.Ok(value)` / `Result.Err(UnexpectedEnd {wanted, available})`.

**Минимизирует C-код в 2x** (~18 functions вместо 36) и поддерживает
D77 «программист пишет одну форму, обе доступны».

### Семантика

- **View, no copy** — `ReadBuffer.from(b)` хранит указатель + len + pos,
  не копирует input. Срок жизни `[]byte` должен пережить `ReadBuffer`
  (managed heap GC обеспечивает).
- **`@position`/`@remaining`/`@has_remaining`** — read-only cursor metadata.
- **`@remaining_bytes()` — копирует** оставшиеся байты в новый `[]byte`.
  Это compromise: zero-copy view над slice потребовал бы Q-readonly-types.
- **`@into()` явный throw** — блокирует D73 auto-derive `@into()` для
  ReadBuffer. ReadBuffer — view, не value-to-convert.

### Что отвергнуто

- **Только `@try_read_*` (без Fail-формы).** Программист часто хочет
  `try` — early-exit через `?` operator. Но Fail-форма короче для
  «just read it, throw on error» паттерна. Обе нужны.
- **Только `@read_*` (Fail-only).** Result-форма необходима для
  graceful-recovery в protocol parser'ах.
- **`ReadBuffer.@into() -> []byte`** — нарушает view-семантику. Какой
  bytes возвращать — все? Только remaining? Двусмысленно. Лучше явный
  `@remaining_bytes()`.
- **Default-endian** — bug-class, как и в WriteBuffer.

### Прецеденты

| Язык | Тип | Read API |
|---|---|---|
| Rust `byteorder` | `ReadBytesExt` trait | `read_u32::<LE>(...)` Result |
| Go `binary.Read` | `binary.LittleEndian.Uint32(b)` | panic on short read |
| Java `ByteBuffer` | `.getInt()` | BufferUnderflowException |

Nova auto-derive read/try_read — оригинальная фича (закрепляется
плотно D77 pattern).

**Связь:** [D26](decisions/08-runtime.md#d26),
[D77](decisions/08-runtime.md#d77) (`TryFrom`/`TryInto` параллель),
[D82](decisions/08-runtime.md#d82), Q-buffer (REPLACED), Q-write-buffer.

---

## Q-codegen-builtins-cleanup. Удаление hard-coded external-таблиц из codegen ✅ CLOSED (2026-05-08)

> **Plan 12 закрыт (2026-05-08).** Ф.1-Ф.5 + Ф.7 acceptance.
> `std/runtime/builtins.nv` — single source of truth; codegen
> читает AST через `ExternalRegistry` (`include_str!`-embedded
> в binary). Hard-coded dispatch удалён в emit_call (Member-form
> instance + Member-form static + Path-form static). Acceptance:
> добавление `WriteBuffer @write_zero(n int)` в builtins.nv +
> runtime impl работает БЕЗ правки Rust-codegen'а.
>
> **Не сделано (отложено):** Ф.6 type-checker gate для unknown
> methods на opaque types. Сейчас unknown method даёт linker error
> (late-stage); idealем early-stage type error. Отдельный refactor
> `types/mod.rs`, не блокер для main goal'а.

**Контекст.** [D82](decisions/08-runtime.md#d82) (расширен 2026-05-08)
фиксирует: `std/runtime/builtins.nv` — единственный источник истины
для сигнатур external-функций. Codegen знает только правила mangling
и Nova→C type mapping, но не хранит список самих функций.

Сейчас codegen этому ещё не соответствует. В `compiler-codegen/` есть:

- `record_schemas.insert("StringBuilder", ...)` / `"WriteBuffer"` /
  `"ReadBuffer"` — hard-coded layout/method tables.
- Method dispatch таблицы — special-case `Nova_StringBuilder_method_*`
  emit'ы в emit_c.rs.
- Старый `record_schemas.insert("Buffer", ...)` (Plan 04 Этап 6
  удалит).

**Проблема.** Любое расхождение между builtins.nv и Rust-таблицей —
silent. Если в builtins.nv:
```nova
export external fn WriteBuffer mut @write_u32_be(v u32) -> ()
```
а в codegen Rust hard-coded `Nova_WriteBuffer_method_write_u32_be(buf,
v)` где `v` имеет тип `int` (не `uint32_t`) — компилируется, но
runtime UB: codegen эмитит call с `nova_int` (64-bit), runtime ждёт
`uint32_t`, ABI ломается.

**Что нужно сделать.** Codegen читает AST builtins.nv (как обычный
Nova-модуль) и для каждой `external fn` декларации:
1. Применяет mangling rules → C-name.
2. Применяет Nova→C type mapping → C-prototype.
3. Эмитит prototype в сгенерированный header.
4. При встрече вызова `wb.@write_u32_be(v)` — lookup'ит декларацию
   в builtins.nv AST, не в Rust-таблице.

После этой миграции hard-coded таблицы удаляются. Расхождение между
.nv-декларацией и runtime-реализацией ловится **линкером** (undefined
reference / type mismatch при включённом `-Wstrict-prototypes`).

**Объём работы.**
- AST-walker для builtins.nv (можно переиспользовать существующий
  parser).
- Mangling rules вынесены в один модуль (сейчас разбросано).
- Type mapper Nova→C централизован.
- Удаление `record_schemas.insert(...)` для StringBuilder/WriteBuffer/
  ReadBuffer (после Plan 04 Этап 6 — и для Buffer, до — оставить).

**Зависимости.**
- Plan 04 Этапы 1-5 (runtime типы реализованы) — закрыты.
- Plan 04 Этап 6 (удаление Buffer) — pending.
- Этот cleanup — после Этапа 6 или параллельно.

**Связь:** [D82](decisions/08-runtime.md#d82) (single source of truth
правило), [Plan 12](../docs/plans/12-builtins-driven-codegen.md)
(этот cleanup — план), Plan 04 Этап 6 (предшествует), Q-overloading
(overload-resolution тоже читает из AST builtins.nv).

---

## Q-match-unit-arms-in-expr. Bootstrap-codegen: `match` в expression-position с unit-arms

**Контекст.** Когда `match`-выражение стоит в expression-position
(`let r = match expr { ... }`), а тело какой-то arm'ы содержит
**только unit-возвращающие statements** (например `assert(...)`,
`println(...)`), bootstrap-codegen эмитит C-код с **void mismatch** —
функция `nova_assert` возвращает unit (`void`), но codegen ожидает
`nova_int` или `nova_str` value.

**Симптом.** При компиляции:
```
error C2440: невозможно преобразование "void" в "nova_int"
```

**Где встречается.** Обнаружено реальной работой со stdlib:
- `nova_tests/runtime/error_runtime_error.nv` — 4 места
  (IndexOutOfBounds / TypeMismatch / AssertFailed / NoHandler).
- `std/collections/hashmap.nv` — 4 места (проверки Occupied/Some).

**Workaround.** Переписать на `if let Pattern = expr { stmts;
assert(...) }` — `if let` это **statement-form**, нет mismatch.

**Что нужно для закрытия.** Codegen должен распознавать unit-result
match-arms и:
- Либо завернуть в block-expr с явным `()` return.
- Либо detect'ить void в C и эмитить как statement, а не expression.
- Либо в type-checker'е требовать всем arm'ам совпадающий не-unit
  тип, если match в expr-position (более строгая семантика).

**Status.** Не закрыто. Workaround достаточен для bootstrap, но
ограничивает идиоматический Nova-код. После полного codegen rewrite
(Plan 02) — закрыть.

**Связь:** Plan 02 (codegen-c-backend), [D19](decisions/03-syntax.md#d19)
(match-arms `=>`), Q-pattern-mut (ниже — связанное ограничение).

---

## Q-pattern-mut. Bootstrap-codegen: `mut` в pattern не парсится

**Контекст.** В match/let-pattern'ах нельзя использовать `mut`-модификатор:

```nova
match result.get("section") {
    Some(mut section_map) => {       // ✗ parse error
        section_map.insert("k", "v")
    }
    None => ()
}
```

Парсер не распознаёт `Some(mut x)` — ожидает identifier, а не keyword.

**Workaround.** Переписать на `if let` с отдельным `let mut`:

```nova
if let Some(section_map_immut) = result.get("section") {
    let mut section_map = section_map_immut
    section_map.insert("k", "v")
}
```

**Где встречается.** Реально нужно при работе с Option/Result
обёртками над mutable коллекциями. В `std/encoding/ini.nv`
переписано workaround'ом при миграции.

**Что нужно для закрытия.** Расширить parser pattern-grammar:
```
pattern = ['mut'] (identifier | constructor-pattern | record-pattern
                  | tuple-pattern | wildcard | literal)
```

`mut` в pattern должен создавать mutable binding (как `let mut x =
expr`) — это compile-time annotation, runtime-поведение не меняется.

**Status.** Не закрыто. Workaround (отдельный `let mut`) рабочий, но
многословный. Низкий приоритет — после Plan 02 (codegen rewrite) или
при type-checker rewrite.

**Связь:** Q-match-unit-arms-in-expr (родственное bootstrap-
ограничение), [D33](decisions/03-syntax.md#d33) (let/const/mut/readonly),
Plan 02.

---

## Q-overloading. Перегрузка функций / методов по типу аргументов ✅ CLOSED by [D84](decisions/10-overloading.md#d84)

> **Закрыт D84** (2026-05-10). Полная семантика: четыре оси
> (receiver-тип, типы аргументов, тип результата, арность); правила
> резолва — самый специфичный матч, concrete > generic, non-variadic >
> variadic, args-фильтр перед result-фильтром, ambiguity → compile
> error с hint'ом. Распространяется на свободные функции, методы и
> static-функции на типе.
>
> **Реализация в bootstrap-codegen:**
> - Methods (с receiver'ом) — ✅ работает (Plan 11): multi-overload
>   registry + strict resolution + C-name mangling.
> - Free-functions (без receiver'а) — ✅ разрешено по D84, codegen
>   будет расширен через тот же mangling-механизм.
> - Method values + disambiguation через `as fn(...)` — ✅ Plan 11 Ф.4/Ф.5.
>
> **Variant 4 (protocol-based dispatch)** — параллельный путь, не
> отменяющий D84: используется когда расширяемость через protocol
> предпочтительнее ad-hoc перегрузки. Описан как идиоматичный путь в
> D84 «Что отвергнуто».



**Контекст.** D46 фиксирует **operator overloading** через `@plus`/
`@times` etc — это перегрузка по operator-кейсу. Но ad-hoc overload
обычных функций / методов по типу аргументов — **не описан**.

Текущее состояние bootstrap-codegen:

| Ось перегрузки | Bootstrap | Прецеденты |
|---|---|---|
| **По receiver-типу** (`fn int @m()` vs `fn str @m()`) | ✅ Работает | Rust impl блоки |
| **По типу результата** (через D73/D77 dispatch) | ✅ Работает | Haskell type classes |
| **По типу аргумента** на одном receiver (`fn T @m(s str)` vs `fn T @m(b []byte)`) | ❌ Last-wins | Java, C++, Swift |
| **По arity** (разное число аргументов) | ❌ Last-wins | C# optional params |

Причина: `method_receivers: HashMap<name, (recv_ty, is_instance)>` —
ключ это только имя метода. Insert по тому же имени переписывает.

**Use-cases требующие arg-type overload:**
- `Buffer.add(s str)` / `Buffer.add(b []byte)` — Q-buffer.
- `Logger.log(msg str)` / `Logger.log(level int, msg str)`.
- Coercive constructors: `Money.from(int)` / `Money.from(f64)` /
  `Money.from(str)` — частично решается D73 (несколько `from`
  с разными типами параметра).

**Варианты:**

1. **Полная ad-hoc overload (Java/Swift-style).** Компилятор резолвит
   по статическим типам аргументов. Требует переделки `method_receivers`
   в `HashMap<name, Vec<Sig>>` + dispatch-логику.
2. **Только D73-style dispatch.** Программист пишет несколько
   `T.from(V1)`, `T.from(V2)` — это уже работает (D73 specifically
   для `from`). Расширить same-name multiple-defines на любые
   статические методы — но с явной семантикой "разные параметры
   значит разные dispatch-ключи".
3. **Запретить overload, требовать разные имена.** Текущее состояние
   bootstrap. `add_str` / `add_bytes` etc. Lower expressiveness, но
   очень предсказуемо.
4. **Generic functions (`fn add[T](v T)` с trait-bound).** Если `Buffer
   @add[T Encodable](v T)` — один метод, dispatch через protocol.
   Требует Encodable protocol с методом encode_to_buffer. Сложнее
   объявить, но extensible (новые типы могут implement Encodable).

**Предложение:** на bootstrap-уровне **3** (разные имена — что и
делается в Q-buffer). На production-уровне — **4 (protocol-based)**
как идиоматичный Nova-путь, с **fallback на 1** для редких случаев
где protocol не подходит (overload по числу аргументов).

**Связь:** [D46](decisions/03-syntax.md#d46) (operator overloading,
specific case), [D53](decisions/02-types.md#d53) (protocols как
основной механизм абстракции), [D73](decisions/08-runtime.md#d73)
(`From` уже допускает multiple `T.from(V1)`/`T.from(V2)` — частный
случай), Q-buffer (motivating use-case).

---

## Q-overload-result-type. Result-type overload (ось 3 D84) — отложено

> ⏸ **DEFERRED** (2026-05-10). Производная от
> [D84](decisions/10-overloading.md#d84-перегрузка-функций-и-методов-четыре-оси-резолв-по-самому-специфичному-матчу) —
> ось 3 (по типу результата) частично реализована: type-checker
> регистрирует overloads с разным return-type, но codegen на call-site
> **не делает** expected-type propagation.
> **Trigger:** реальный use-case в stdlib, где single-target `Into[T]`
> через D73 + ось 1 не покрывает (например `T.@into() -> X` vs
> `T.@into() -> Y` — multi-target конверсии для одного receiver'а).

**Контекст.** [D84](decisions/10-overloading.md#d84-перегрузка-функций-и-методов-четыре-оси-резолв-по-самому-специфичному-матчу)
заявляет четыре оси перегрузки. Оси 1 (receiver-type), 2 (arg-types),
4 (arity) — реализованы в bootstrap-codegen (Plan 11 + 2026-05-10
free-fn extension). **Ось 3 (result-type) — частично:**

```nova
fn Celsius @into() -> Fahrenheit => ...
fn Celsius @into() -> Kelvin     => ...

let f Fahrenheit = c.into()       // должно резолвиться в первый
let k Kelvin     = c.into()       // должно резолвиться во второй
```

**Что работает:**
- Type-checker допускает обе декларации (overload по возврату — валидно
  по D84).
- Mangling даёт уникальные C-имена.

**Что не работает:**
- При `c.into()` codegen не смотрит на ожидаемый тип из контекста
  (let-аннотация, return-position, тип параметра, поле record-литерала).
- Если кандидатов несколько с одинаковыми arg-types и разными return-
  type — **ambiguity error**, даже когда контекст однозначно задаёт тип.

**Что нужно для реализации.**

Codegen на каждом call-site должен:
1. Вытащить **expected type** из контекста выражения (let-annotation,
   return-position, argument-type вызывающей функции, поле
   record-литерала).
2. Применить как **фильтр 3** в D84 resolve: отбросить кандидатов с
   несовместимым return-type.
3. Если после фильтра остался один — выбрать его.
4. Если несколько / ноль — fallback на текущую ambiguity error.

Это требует **bidirectional type inference** через выражения: типы
текут не только bottom-up (из аргументов), но и top-down (из контекста).

**Workaround сейчас.** Вместо instance-method overload по возврату —
static-функции с разными именами:

```nova
fn Fahrenheit.from(c Celsius) -> Self => ...
fn Kelvin.from(c Celsius) -> Self => ...

// Вместо `c.into()`:
let f = Fahrenheit.from(c)
let k = Kelvin.from(c)
```

Это работает потому что `T.from(...)` overload'ится по **receiver-типу**
(ось 1), которая полностью реализована.

**Альтернативно** — `Into[T]` ([D73](decisions/08-runtime.md#d73))
работает в bootstrap'е через **single-target** конверсию + контекст из
let-аннотации. Multi-target Into — пока не покрывается.

**Когда разморозить.** Реальный use-case в stdlib, где обходной путь
(static-функции / single-target Into) не работает или требует много
дублирования. Например:
- `Vec[T] @into() -> List[T]` vs `Vec[T] @into() -> Set[T]`.
- `Json @into() -> User` vs `Json @into() -> Order` (но это уже
  из плохого дизайна — лучше `User.from_json(j)`).

**Связь:** [D84](decisions/10-overloading.md#d84-перегрузка-функций-и-методов-четыре-оси-резолв-по-самому-специфичному-матчу)
(основное определение четырёх осей), [D73](decisions/08-runtime.md#d73)
(`Into[T]` — частный случай через single-target), Plan 11 (bootstrap
для осей 1, 2, 4).

---

## Q-clone-semantics. `@clone()` — shallow или deep / рекурсивно?

> ✅ **CLOSED by [D26 → «`@clone()` — shallow по умолчанию»](decisions/08-runtime.md#d26)**
> (Plan 17 Ф.1, 2026-05-08): **`@clone()` — shallow** для record и
> коллекций (поля копируются, managed-references share'ятся). Для
> deep-копии программист пишет вручную (`@deep_clone()` не в prelude).
> Исключение — opaque accumulator-типы (`StringBuilder`, `WriteBuffer`),
> для которых `@clone()` deep по семантике типа (mutable internal
> buffer не должен share'иться между копиями).
>
> Прецедент Rust (Clone shallow, DeepClone руками), Java
> (Object.clone shallow), Go (slice/map share по assign).
>
> Регрессия: `nova_tests/runtime/clone_semantics.nv`.

**Контекст.** В Nova нет `&` borrowing (D6 — managed heap), поэтому
все ссылки value-shared через GC. Это значит `let b = a` копирует
указатель, не контент. Когда нужна **независимая копия**, программист
вызывает `a.clone()`.

Вопрос: что именно делает `@clone()`?

**Прецеденты:**

| Язык | Default | Расширения |
|---|---|---|
| Rust | per-type (Clone trait) | `derive(Clone)` рекурсивно поля |
| Java | shallow (Object.clone) | override для deep |
| Go | value-types (assignment копирует поля) | references — share |
| Python | `copy.copy` shallow, `copy.deepcopy` recursive | разные функции |
| JS/TS | `Object.assign({}, x)` shallow | `structuredClone(x)` deep |

**Варианты для Nova:**

1. **Auto-derived deep clone (Rust-style).** Компилятор синтезирует
   `@clone()` для record/sum-типа: для каждого поля вызывает `field.clone()`.
   Программист может override для специальных случаев.
   - `int @clone()` → value copy (тривиально)
   - `str @clone()` → тот же ptr (immutable, нет смысла копировать)
   - `Buffer @clone()` → копия internal `[]byte` (mutable, требует
     независимости)
   - `Cache @clone()` → пустой cache (override — зависит от business
     semantics)
   - **Циклы:** runtime-detection (set уже-клонированных) или запрет.
2. **Shallow по умолчанию, явный `@deep_clone()`.** Дешевле default,
   но программист должен помнить про share-mut между clone'ами.
3. **Не вводить `@clone()` в prelude.** Каждый тип сам определяет
   что клонировать значит. Минимум surprise, максимум ad-hoc work.

**Предложение:** **1 (auto-derived deep)** — Rust-style. Это:
- Безопасный default (после clone независимы).
- Auto-derive снимает boilerplate для типичных типов.
- Override доступен где нужна другая семантика.
- Циклы — отдельная задача (Q-cycle-detection); пока Nova не имеет
  явных reference-циклов в data-types (D6: GC может collect cycles
  но user-code их не создаёт идиоматически).

**Тонкости:**
- `Buffer @clone()` — deep копия `[]byte` (vital для buffer'а; shared
  buffer между clone'ами = data races).
- `str @clone()` — тот же ptr (str immutable, копия эквивалентна).
- `[]T @clone()` — auto-derived: новый array, для каждого элемента
  вызывается `element.clone()`. O(n).
- Записи с handler-фунциями / closures — closure clone что значит?
  Открытый sub-вопрос.

**Связь:** [D6](decisions/05-memory.md#d6) (managed heap), Q-buffer
(`Buffer @clone()` — конкретный mutable use-case), Q-cycle-detection
(когда это станет актуально).

---

## Q-readonly-types. TypeScript-style `Readonly<T>` / `DeepReadonly<T>`

**Контекст.** TS позволяет помечать тип как иммутабельный на любой
глубине через mapped types:

```typescript
type Readonly<T> = { readonly [K in keyof T]: T[K] }
type DeepReadonly<T> = T extends object
  ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
  : T
```

В Nova сейчас:
- D36 даёт `readonly` modifier на отдельных **полях** record.
- НЕТ `readonly T` как type-modifier для целого типа.
- НЕТ `keyof T` / mapped types.

**Use-cases:**
- `s.bytes()` хочет вернуть `[]byte` без mutate-возможности — сейчас
  нет способа. Workaround: копировать (что и делает D26).
- API возвращает «config», который не должен меняться вызывающим — сейчас
  только конвенция «не меняй».
- Snapshot vs live view — сейчас выражается копированием.

**Варианты:**

1. **Полный TS-style.** `keyof T`, mapped types, `readonly T`,
   `DeepReadonly<T>`. Большая type-system фича. Compile-time only
   (нет runtime enforcement в managed heap без borrow-checker).
2. **`Read[T]` effect-marker.** Маркируем функции «читает только» через
   эффект — runtime может проверять (через GC marker?) или просто
   compile-time hint. Согласовано с Nova effect-system.
3. **`const T` newtype.** Простой type-flag. `const []byte` — это
   отдельный тип, `[]byte` → `const []byte` через `as`, mutate methods
   compile-time error.
4. **Не вводить.** Полагаться на конвенцию + копирование там где надо
   независимость.

**Тонкость:** D6 (managed heap, без borrow-checker) ограничивает
**runtime enforcement**. Любая readonly-проверка может быть только
compile-time (как в TS). Это значит Nova-код может через `as` cast'ом
обойти readonly — это **soft guarantee**, не **hard**. TS живёт с
этим, но это тонкость.

**Предложение:** отложить до созревания. Сейчас — **4 (конвенция +
копия)** через `Buffer.clone()`, `s.into() -> []byte` (копия) и т.д.
Когда будет 5+ конкретных use-cases где readonly нужен — выбрать
вариант 2 (`Read[T]` как effect) или 3 (`const T` newtype) в
зависимости от того, нужно ли это compile-time only или runtime.

**Связь:** [D6](decisions/05-memory.md#d6), [D36](decisions/03-syntax.md#d36)
(`readonly` поля), [D62](decisions/04-effects.md#d62) (effects как
runtime-marker pattern), Q-effect-polymorphism.

---

## Q-keywords-as-fields. Можно ли использовать keyword как имя поля?

> ✅ **CLOSED by [D83](decisions/03-syntax.md#d83)** (2026-05-08)
> вариантом 1 — keywords строго запрещены как identifier'ы.
> Без escape-механизма (Rust `r#`, Swift backticks отвергнуты как
> overkill для bootstrap'а; могут быть добавлены после v1.0 если
> накопится FFI-боль).
>
> Sweep задача: `std/collections/queue.nv` — поле `in []T`
> переименовать в `input` или `inputs`.

**Контекст.** std/collections/queue.nv использует `in` как имя поля:

```nova
export type Queue[T] {
    mut in  []T            // ⛔ in — keyword (for x in iter)
    mut out []T            // ✅ out — обычный ident
}
```

Bootstrap-парсер падает на `in` field-declaration:
`expected identifier, got 'in'`.

**Варианты (на момент обсуждения):**
1. **Запретить keywords как identifiers вообще.** Все keywords —
   зарезервированы. Программист переименовывает (`input`, `inq`).
   Самый простой, согласован с большинством языков (Rust/Go/Java).
2. **Контекстно-чувствительные keywords.** `in` keyword только в
   `for x in iter`-конструкции, везде ещё — обычный ident. Сложнее
   парсер, но эргономичнее. Прецедент: Swift, C# (contextual keywords).
3. **Raw-identifier escape.** `r#in` — keyword как ident через префикс.
   Прецедент: Rust `r#fn`, `r#move`.

**Принятое решение:** Вариант 1, зафиксирован в
[D83](decisions/03-syntax.md#d83).

**Связь:** [D30](decisions/03-syntax.md#d30) (naming convention),
[D83](decisions/03-syntax.md#d83) (closing decision).

---

## Q-effect-type-anonymous. Anonymous effect types в позиции type

> ✅ **CLOSED by Variant 3** (2026-05-08): использовать `Iter[T]`
> protocol из prelude. Нет нужды в anonymous effect types в позиции
> типа — D58 даёт `Iter[T]` явно, D53 protocol-as-type работает для
> структурного match'а.
>
> Sweep done: `std/collections/linkedlist.nv:from_iter` и
> `std/collections/set.nv:from_iter` мигрированы с
> `it effect { mut next() -> Option[T] }` (некорректный синтаксис) →
> `it Iter[T]` (корректный, prelude).

**Контекст.** std/collections/linkedlist.nv использовал `effect { ... }`
inline в параметре функции:

```nova
fn LinkedList[T].from_iter(it effect { mut next() -> Option[T] }) -> Self {
    while let Some(x) = it.next() { ... }
}
```

Это **anonymous effect type** — структурный effect, объявленный в
позиции type-аннотации. Bootstrap-парсер не поддерживал: `expected
type, got 'effect'`. Также синтаксис **некорректен** по двум
причинам:

1. `effect` — kind-token при declaration of named type (D18/D61),
   не используется в позиции типа значения inline.
2. `mut` на operation в effect-declaration не описан spec'ом —
   effects описывают operations без `mut` (D61).

**Варианты (на момент обсуждения):**
1. **Поддержать anonymous effect types.** Парсер видит `effect {...}`
   в type-position, парсит method-block, создаёт anonymous effect.
   Согласовано с D53 (protocol как type).
2. **Только named effects.** Программист объявляет `effect Iter[T] {
   mut next() -> Option[T] }` отдельно, потом использует имя.
   Простой, less expressive.
3. **`Iter[T]` protocol в prelude.** Стандартный protocol для
   итераторов; пользователь принимает `it Iter[T]` без объявления.
   Прецедент: Rust `IntoIterator`, Swift `IteratorProtocol`.

**Принятое решение:** **Variant 3** — `Iter[T]` protocol уже есть
в D26 prelude (см. D58/08-runtime.md строки 332-336):

```nova
type Iter[T] protocol {
    mut next() -> Option[T]
}
```

Программист пишет `it Iter[T]` для drain-параметра. Структурно любой
тип с `mut @next() -> Option[T]` удовлетворяет. Anonymous effects
в позиции типа **не нужны** — D53 protocol-as-type решает то же самое.

**Связь:** [D58](decisions/03-syntax.md#d58) (Iter[T] protocol),
[D53](decisions/02-types.md#d53), [D26](decisions/08-runtime.md#d26)
(prelude содержит Iter[T]).

---

## Q-generic-receiver-method. `fn []T @method[U](...)` — generic methods на slice

> ✅ **ЧАСТИЧНО ЗАКРЫТО (2026-05-17)** для **user-defined generic типов**
> через [D119](decisions/02-types.md#d119-method-level-type-parameters-в-generic-methods)
> + [Plan 48 Ф.9](../docs/plans/48-closures-in-generics.md#-9--method-param-mono).
>
> Generic method с method-level type-param теперь работает на user
> generic types: `Wrapper[T] @map[U](f fn(T) -> U) -> Wrapper[U]` —
> compiler emit'ит mono'd instance per (T, U) pair, bidirectional
> inference из closure-typed args, return type корректно substituted.
>
> Остаётся **OPEN** для **built-in `[]T`** (slice receiver) — требует
> отдельной parser-side работы (`[]T` в receiver position).
> Q-array-api всё ещё open.

**Контекст.** std/collections/vec.nv хочет writing extension methods
на встроенный `[]T`:

```nova
export fn []T @map[U](f fn(T) -> U) -> []U { ... }
export fn []T @filter(pred fn(T) -> bool) -> []T { ... }
```

Bootstrap не парсит `[]T` как receiver type. Это требует:
1. Парсер: `[]T` в receiver position — type с inferred type-parameter.
2. Codegen: ✅ **DONE** (D119) — generation специализированных функций
   для каждой комбинации `(T, U)` через mono pass.

**Варианты:**
1. **Полная поддержка generic methods на built-in типах.** Codegen ✅ готов
   (D119). Остался parser-side работа: `[]T` в receiver position.
2. **Free functions с TYpe parameters.** `fn map[T, U](xs []T, f fn(T) -> U) -> []U`.
   Просто, но теряется method-syntax (`xs.map(f)`).
3. **Prelude-методы только.** Compiler знает фиксированный набор
   `[]T.map/.filter/.fold` и т.п., user не расширяет. Простой
   bootstrap-уровень.

**Предложение:** **3** на bootstrap (text status), **1** на production
(codegen уже готов через D119 — нужен только parser pass для `[]T`
receiver). User generic types — уже работают.

**Связь:** [D27](decisions/03-syntax.md#d27), [D35](decisions/03-syntax.md#d35),
[D72](decisions/02-types.md#d72),
[D119](decisions/02-types.md#d119-method-level-type-parameters-в-generic-methods),
Q-array-api.

---

## Q-assert-without-parens. `assert cond` без parentheses?

**Контекст.** std/data/sql.nv писал `assert n == 42` как
keyword-style assert (без скобок), но Nova `assert` это обычная
функция, требует `assert(n == 42)`.

**Варианты:**
1. **`assert` — функция (текущее).** `assert(cond)` обязательны
   parens. Nova-консистентно (`println("...")`, `Mem.live()` —
   все функции с parens).
2. **`assert` — keyword.** `assert cond` — отдельный statement.
   Прецедент: Rust `assert!(cond)` (макрос); Java `assert cond` (statement).
3. **Trailing-block style.** `assert { cond }`? Нелепо для assertions.

**Предложение:** **1** — keep current. Nova не имеет macros, и
выделять `assert` как special-form нет причин. Обновить файлы где
было `assert ...` без скобок.

**Связь:** [D40](decisions/03-syntax.md#d40) (function call syntax).

---

## Q-source-annotations. CLI `--no-annotate-source` (default-on) ✅ ЗАКРЫТО (2026-05-07)

> Реализованы annotations `/* SRC: <Nova-исходник> */` перед каждым
> statement'ом / fn-body / trailing-expr сгенерированного `.c` файла.
> **По умолчанию включены** (user-driven решение 2026-05-07): полезно
> настолько часто (отладка, code-review, понимание codegen), что
> должно быть default-on. Off — через явный `--no-annotate-source`
> для CI-friendly стабильных diff'ов.
>
> Прецеденты: Cython (по умолчанию вставляет Python-исходник),
> Crystal (`--emit-line-numbers`), Rust LLVM (`#line` директивы).
> Nova выбрал opt-in — потому что (a) Nova-исходник может содержать
> non-ASCII (UTF-8 в строках/идентификаторах), MSVC иногда хочет
> `/utf-8` flag; (b) тестовая сюита диффает `.c`, аннотации дают
> разные diffs.
>
> Реализация: `CEmitter::set_source_for_annotations(src: String)`,
> `emit_source_annotation_for_stmt(stmt)` hook в начале `emit_stmt`.
> Snippet берётся из span'а statement'а, первая строка, escaped `*//*`,
> truncated до 120 символов.

---

## Q-stdlib-minimal-api. Минимальный stdlib API surface, выявленный из practical libraries

**Контекст.** Реализация пяти практических stdlib-либ
([math/complex.nv](../std/math/complex.nv), [data/semver.nv](../std/data/semver.nv),
[encoding/json.nv](../std/encoding/json.nv), [identifiers/uuid.nv](../std/identifiers/uuid.nv),
[encoding/base64.nv](../std/encoding/base64.nv), [encoding/url.nv](../std/encoding/url.nv),
[checksums/crc32.nv](../std/checksums/crc32.nv), [identifiers/ulid.nv](../std/identifiers/ulid.nv))
выявила **минимальный набор API**, без которого парсеры и
сериализаторы не пишутся. Этот набор — ориентир для bootstrap stdlib
implementation.

Каждая API ниже **используется по крайней мере в двух** из перечисленных
файлов. Это не пожелания, а измеренные требования.

### Эффекты

```nova
// Random — детерминизм через handler-substitution в тестах
type Random effect {
    u64() -> u64                        // 64 случайных бита
    bytes(n int) -> []byte              // массовая генерация
}

// Pre-defined handlers
fn seeded(seed u64) -> Effect[Random]      // PRNG с фиксированным seed
fn secure() -> Effect[Random]               // CSPRNG для production

// Time — Unix-timestamp + sleep
type Time effect {
    now_ms() -> u64                     // Unix timestamp в миллисекундах
    now_ns() -> u64                     // наносекунды (для high-precision)
    sleep(d Duration) -> ()
}

// Pre-defined handlers
fn fixed_ms(ms u64) -> Effect[Time]         // время заморожено
fn system_clock() -> Effect[Time]            // реальные часы OS
```

**Use cases:**
- `Random` — uuid.nv (v4), ulid.nv, любая криптография
- `Time` — uuid.nv (v7), ulid.nv, expiration, retry backoff

### Парсинг чисел из строки

```nova
fn int.try_from(s str) -> Result[int, ParseIntError]
fn u64.try_from(s str) -> Result[u64, ParseIntError]
fn i64.try_from(s str) -> Result[i64, ParseIntError]
fn f64.try_from(s str) -> Result[f64, ParseFloatError]
// ... для всех числовых типов
```

D77 даёт обе формы: `int.from(s) Fail[ParseIntError] -> int` синтезируется.

`ParseIntError` / `ParseFloatError` — отдельные типы (D30 convention).

### Базовые str-методы (предполагаются в prelude)

```nova
fn str @len() -> int                        // длина в байтах (или codepoint'ах? — Q-string-len)
fn str @char_at(i int) -> Option[char]      // codepoint на позиции i
fn str @chars() -> Iter[char]               // итератор codepoint'ов
fn str @bytes() -> []byte                   // UTF-8 байты
fn str @slice(from int, to int) -> str      // подстрока (D78 — открытый Q что значит i)
fn str @starts_with(prefix str) -> bool
fn str @ends_with(suffix str) -> bool
fn str @contains(needle str) -> bool
fn str @find(needle str) -> Option[int]     // позиция или None
fn str @replace(from str, to str) -> str
fn str @to_lower() -> str
fn str @to_upper() -> str
fn str @trim() -> str
fn str @split(sep str) -> []str
fn str @strip_prefix(p str) -> Option[str]  // None если не starts_with
fn str @strip_suffix(s str) -> Option[str]
```

### Static-методы str (конструкторы)

```nova
fn str.from(c char) -> str                  // 1-char string
fn str.from(n int) -> str                   // через D74 conversion
fn str.from(b bool) -> str                  // "true" / "false"
fn str.from(f f64) -> str                   // лучше через format spec
fn str.from_codepoint(code int) Fail[InvalidCodepoint] -> str  // 1 codepoint → str
fn str.from_bytes(b []byte) Fail[Utf8Error] -> str             // UTF-8 validate
fn str.from_bytes_unchecked(b []byte) -> str                    // escape hatch
```

### `[]T` API

```nova
fn []T.new() -> []T                          // empty с capacity 0
fn []T.with_capacity(n int) -> []T           // empty с зарезервированной памятью
fn []T mut @push(item T)
fn []T @len() -> int
fn []T @is_empty() -> bool
fn []T @get(i int) -> Option[T]              // safe indexing
// arr[i] — panic on bounds (D13), arr.get(i) — Option
fn []T mut @clear()
fn []T mut @remove(i int) -> Option[T]
fn []T @first() -> Option[T]
fn []T @last() -> Option[T]
fn []T @contains(value T) -> bool             // требует @eq на T
fn []T @iter() -> Iter[T]
```

### `Buffer` API (Q-buffer закрыто, реализовано)

```nova
fn Buffer.new() -> Buffer
fn Buffer.with_capacity(n int) -> Buffer
fn Buffer.from(s str) -> Buffer
fn Buffer.from(b []byte) -> Buffer

fn Buffer mut @add_str(s str)
fn Buffer mut @add_bytes(b []byte)
fn Buffer mut @add_byte(b byte)
fn Buffer mut @add_char(c char)              // UTF-8 encode 1-4 bytes

fn Buffer @into() -> []byte                   // consume → bytes (infallible)
fn Buffer @into() Fail[Utf8Error] -> str      // consume → str (UTF-8 validate)
fn Buffer @try_into() -> Result[str, Utf8Error]
fn Buffer @into_str_unchecked() -> str        // escape hatch
fn Buffer @len() -> int
fn Buffer @capacity() -> int
fn Buffer @clone() -> Buffer
```

### `Option[T]` методы

```nova
fn Option[T] @is_some() -> bool
fn Option[T] @is_none() -> bool
fn Option[T] @unwrap() -> T                   // panic on None
fn Option[T] @unwrap_or(default T) -> T
fn Option[T] @unwrap_or_else(f fn() -> T) -> T
fn Option[T] @map[U](f fn(T) -> U) -> Option[U]
fn Option[T] @and_then[U](f fn(T) -> Option[U]) -> Option[U]
fn Option[T] @ok_or[E](err E) -> Result[T, E]
```

### `Result[T, E]` методы

```nova
fn Result[T, E] @is_ok() -> bool
fn Result[T, E] @is_err() -> bool
fn Result[T, E] @unwrap() -> T                // panic on Err
fn Result[T, E] @unwrap_err() -> E             // panic on Ok
fn Result[T, E] @ok() -> Option[T]             // Result → Option (D77)
fn Result[T, E] @err() -> Option[E]
fn Result[T, E] @map[U](f fn(T) -> U) -> Result[U, E]
fn Result[T, E] @map_err[F](f fn(E) -> F) -> Result[T, F]
fn Result[T, E] @and_then[U](f fn(T) -> Result[U, E]) -> Result[U, E]
```

### `HashMap[K, V]` API (для json.nv)

```nova
fn HashMap[K Hashable, V].new() -> HashMap[K, V]
fn HashMap[K, V].with_capacity(n int) -> HashMap[K, V]
fn HashMap[K, V] mut @insert(key K, value V) -> Option[V]
fn HashMap[K, V] @get(key K) -> Option[V]
fn HashMap[K, V] @contains(key K) -> bool
fn HashMap[K, V] mut @remove(key K) -> Option[V]
fn HashMap[K, V] @len() -> int
fn HashMap[K, V] @entries() -> Iter[(K, V)]
fn HashMap[K, V] @keys() -> Iter[K]
fn HashMap[K, V] @values() -> Iter[V]
```

### Iter[T] composers

```nova
fn Iter[T] @map[U](f fn(T) -> U) -> Iter[U]
fn Iter[T] @filter(pred fn(T) -> bool) -> Iter[T]
fn Iter[T] @fold[Acc](init Acc, f fn(Acc, T) -> Acc) -> Acc
fn Iter[T] @count() -> int
fn Iter[T] @collect() -> []T                  // в массив; collect[Out]() — Q-collect-mechanism
```

### Числовые операции (D74 instance methods)

Уже зафиксировано в D74. Минимум для парсеров и математики:

```nova
fn f64 @sqrt() / @cbrt() / @sqr()
fn f64 @sin() / @cos() / @atan2(other) / @hypot(other)
fn f64 @abs()
fn f64 @is_finite() / @is_nan() / @is_infinite()
fn f64 @floor() / @ceil() / @round() / @trunc()
fn f64 @min(other) / @max(other)

fn int @abs()
fn int @pow(n int)
fn int @signum()
fn int @min(other) / @max(other)
```

Static константы:
```nova
f64.PI / f64.E / f64.NAN / f64.INFINITY / f64.MAX / f64.EPSILON
int.MAX / int.MIN
```

### Ошибки парсинга — стандартные типы в prelude

По D30 convention `Parse<TypeName>Error`:

```nova
type ParseIntError { value str, reason str }
type ParseFloatError { value str, reason str }
type Utf8Error { position int, byte byte }
type InvalidCodepoint { value int }
```

### Что отсутствует (намеренно — не для MVP)

- **Regex** — отдельная либа, не prelude. Q-regex.
- **Date/Time formatting** — кроме `Time.now_ms()`, format/parse сложен.
- **JSON parser** — пользовательская либа (`std.json`), не prelude.
- **Crypto primitives** — отдельная либа.
- **Async I/O** — через эффекты Net/Fs, не prelude API.

### Приоритеты реализации

**Tier 1 (без них stdlib не пишется):**
- `int.try_from(s)` / `u64.try_from(s)` / `f64.try_from(s)`
- `str` методы (`@len`, `@char_at`, `@chars`, `@slice`, `@find`, `@contains`, `@starts_with`)
- `[]T` базовые (`new`, `with_capacity`, `push`, `len`, `get`, `iter`)
- `Buffer` (уже реализован 2026-05-07)
- `Option`/`Result` основные методы

**Tier 2 (для production):**
- `Random` / `Time` handler'ы (включая `seeded` / `fixed_ms`)
- `HashMap`
- Iter composers
- str manipulation (`@to_lower`, `@to_upper`, `@trim`, `@replace`)

**Tier 3 (nice-to-have):**
- Regex
- Format spec
- Расширенные числовые (`f64 @atan2`, etc.)

### Что уже реализовано в bootstrap (по состоянию на 2026-05-07)

После раундов 4–5 codegen+runtime закрыли часть Tier 1/2:

- **str**: `@bytes()` / `@chars()` / `@split(sep)` — раунд 4 (eager, []byte / []int / []str)
- **Pattern alternation** `Some(A) | Some(B) => body` в match-arms — раунд 4
- **Buffer** API (Q-buffer закрыто) — реализован
- **Channel[T]** base API (D79): `Channel.new(cap)`, `@send/@recv/@try_send/@try_recv/@close/@is_closed/@len/@capacity`, drain-семантика — раунд 5
  - Tier 1+ (для concurrent stdlib); `select { ... }` — pending до spawn-block fix
- **D28 effect inference** — private fn с throw авто-получает Fail
- **char-литералы** (Q-char-literals закрыто) — `'a' / '\n' / '\u{...}'`

Pending Tier 1/2:
- `int.try_from(s)` / `u64.try_from(s)` / `f64.try_from(s)` — D77 spec есть, runtime нет
- `Random` / `Time` handler'ы — нужны для AI-first тестов через handler-substitution
- `HashMap[K, V]` — base нет в runtime
- `Iter[T]` composers (`@map`, `@filter`, `@fold`, `@count`, `@collect`)

### Связь

- [D26](decisions/08-runtime.md#d26) — prelude содержит часть этого
  набора; D26 нужно расширить под этот список.
- [D74](decisions/08-runtime.md#d74) — math instance methods.
- [D77](decisions/08-runtime.md#d77) — TryFrom для парсинга.
- [Q-buffer](#q-buffer) — закрыто.
- [Q-char-literals](#q-char-literals) — закрыто.
- [Q-string-indexing](#q-string-indexing) — open: что значит `i` в
  `str @char_at(i int)` (байты или codepoint'ы).
- [Q-collect-mechanism](#q-collect-mechanism) — open: `collect[Out]()`.
- [std/](../std/) — все либы используют этот набор.

**Статус.** Open question — не decision потому что это **накопительный
список**, не финальный. Каждая новая stdlib-либа может выявить что-то
ещё. Но текущий набор уже **измерен** на 8 практических файлах и
является обязательным минимумом для bootstrap stdlib implementation.

---

## Q-parallel-tuple. `parallel { ... }` блок с typed tuple-result

**Контекст.** [D50](decisions/06-concurrency.md#d50) рекомендует
`mut`-захваты для гетерогенного fan-out:

```nova
let mut a = 0
let mut b = 0
spawn { a = compute_a() }
spawn { b = compute_b() }
```

Это **race-prone** в production-runtime (D14 с preemption) и допустимо
только в D71 single-threaded bootstrap. После принятия [D79
(channels)](decisions/06-concurrency.md#d79) есть safe-альтернатива
для streaming/pipelines, но для **2-N** разнородных задач channel
тяжеловат.

**Предложение.** `parallel { ... }` блок с typed tuple-result:

```nova
let (a, b) = parallel {
    compute_a(),    // → A
    compute_b()     // → B
}

let (users, posts, count) = parallel {
    fetch_users(),     // []User
    fetch_posts(),     // []Post
    count_active()     // int
}
```

Семантика:
- Каждое выражение запускается в отдельном fiber'е параллельно.
- Блок ждёт завершения **всех**.
- Результат — tuple типов выражений в порядке объявления.
- При throw в любом fiber — отмена остальных через cancel-propagation
  (как `parallel for` сегодня).
- Никакого shared `mut` — программист не пишет race-prone захваты.

**Преимущества:**

1. **Safe by construction.** Нет shared state, только структурное
   агрегирование результатов.
2. **Типизировано.** Compiler знает типы каждого выражения, собирает
   правильный tuple-тип.
3. **AI-friendly.** Один паттерн вместо двух (`mut`-захват vs
   channels) для типичного fan-out.
4. **Композиция с D75.** Если нужен kill-switch снаружи — используем
   `supervised(cancel: tok)`.

### Implementation hint — overload-семья (bootstrap)

В Nova **нет variadic generics** (есть только variadic для одного
типа `[]T` через [D69](decisions/03-syntax.md#d69)). Для bootstrap-
времени реализация — **explicit overload-семья N=2..8**:

```nova
fn parallel[A, B](
    a fn() -> A,
    b fn() -> B,
) -> (A, B)

fn parallel[A, B, C](
    a fn() -> A,
    b fn() -> B,
    c fn() -> C,
) -> (A, B, C)

// ...до N=8 (стандартный лимит, как Rust tuple impls)
```

**Особенности:**

- `parallel` — **library function**, не language keyword. Это упрощает
  парсер.
- Overloading по **arity** (число параметров) — [D46](decisions/03-syntax.md#d46)
  разрешает overloading по типу аргумента, по arity тоже работает.
- Generic-параметры выводятся из типов lambda-выражений в позиции
  аргумента.

**Использование как блок-выражение** возможно благодаря
trailing-block-стилю ([D43](decisions/03-syntax.md#d43)):

```nova
let (a, b) = parallel(
    || compute_a(),
    || compute_b(),
)
```

### Долгосрочная цель — variadic generics

В будущем (отдельный Q-variadic-generics) вместо overload-семьи —
один generic:

```nova
fn parallel[T...](fns ...fn() -> T) -> (T...)
```

Где `T...` — variadic generic-параметр (как Rust `tuple[T...]` или
TypeScript `[...T]`). Это **отдельный Q**, не блокер для
parallel-tuple сейчас.

### Тонкости

1. **Cancellation на throw.** Если первая задача throw'ит, остальные
   должны отмениться. Реализация через `supervised`-style scope под
   капотом. Если нужен «keep going on error» — программист пишет
   `Result[T, E]` в каждой ветке явно.

2. **Empty / 1-arg parallel.** `parallel()` или `parallel(f)` —
   тривиальные случаи. `parallel(f)` ≡ `(f(),)` (single-element
   tuple) — runtime overhead не оправдан, лучше compile-warning.

3. **Effect-row.** `parallel(f, g)` имеет union эффектов f и g. В
   bootstrap при overload-семье — статическая union. С variadic
   generics — динамическая.

4. **Async-context.** `parallel` использует suspension (ambient,
   [D62](decisions/04-effects.md#d62)) — сигнатуры fn() -> T чистые,
   suspension implicit.

### Прецеденты

| Язык | Конструкция | Notes |
|---|---|---|
| Rust | `tokio::join!(f, g)` | macro, возвращает tuple |
| OCaml 5 | `Domain.spawn` + manual sync | без tuple-builder |
| Erlang | `rpc:multicall` | для distributed |
| Go | `errgroup.Group{}.Go(...)` | через group, не tuple |
| Swift | `async let a = ...` | per-binding async |
| Kotlin | `awaitAll(deferred1, deferred2)` | возвращает list |

Nova `parallel(...) -> (T1, T2, ...)` — **гетерогенный typed tuple**,
самая близкая аналогия — Rust `tokio::join!`.

### Статус

**Не зафиксировано.** После принятия [D79](decisions/06-concurrency.md#d79)
(channels) parallel-tuple — естественное дополнение для гетерогенного
fan-out. Решение:
- **Bootstrap path:** overload-семья `parallel[A,B]`, `parallel[A,B,C]`,
  …, `parallel[A,B,…,H]` в prelude.
- **v2 path:** variadic generics + единая `parallel[T...]` функция.

**Связь:** [D14](decisions/06-concurrency.md#d14) (suspension ambient),
[D50](decisions/06-concurrency.md#d50) (concurrency model),
[D69](decisions/03-syntax.md#d69) (variadic для одного типа),
[D79](decisions/06-concurrency.md#d79) (channels — solution для
streaming, parallel-tuple — для fan-out 2..N).

---

## Q-build-pgo. Profile-Guided Optimization для C-backend

**Контекст.** Nova C-backend через `compiler-codegen` сейчас не
интегрирован с PGO. Прирост от PGO в production-компиляторах
обычно 15-30% на hot path'ах (rustc bootstrap +12-14%, Chrome
core rendering +15-25%). Это **самая большая** «бесплатная»
оптимизация после LTO для backend-программ.

**Зависит от:**

- **Plan 09 (Clang migration)** должен быть завершён до PGO
  работы. На MSVC PGO существует, но слабее: Clang/LLVM имеет
  IR-based профили (более точные), instrumentation flags
  (`-fprofile-generate`, `-fprofile-use`), tooling (`llvm-profdata`).

**Открытые вопросы:**

1. **AutoFDO vs обычный PGO?**
   - Обычный PGO (`-fprofile-generate`) — инструментирует binary
     счётчиками, training run медленнее обычного, но точнее.
   - AutoFDO (`-fprofile-sample-use`) — использует `perf` sampling,
     training run без overhead'а, но менее точные профили
     (sampling на ~100kHz).
   AutoFDO проще для CI (не нужен инструментированный binary),
   обычный PGO даёт чуть больше прироста. **Не решено.**

2. **Profile в репо или нет?**
   - **За хранение:** training run может занимать минуты, удобно
     закоммитить готовый профиль.
   - **Против:** профили платформо-специфичные (x86-64-v3 vs ARM),
     устаревают при изменении кода, blow up репозиторий.
   - **Cargo (Rust)** — рекомендует **не коммитить**. Программист
     сам делает training run.
   - Решается при написании полного плана.

3. **PGO как часть `nova build` или отдельный workflow?**
   - **Integrated:** `nova build --pgo` делает three-step pipeline
     автоматически (instrument → user training run → use). Удобно,
     но скрывает магию.
   - **Manual:** программист сам пишет три команды
     (`--pgo-instrument`, run, `--pgo-use`). Гибче для CI.
   - Скорее всего **оба** (default — manual, `--pgo` shorthand для
     стандартного workflow).

4. **Каков канонический training workload?**
   - User-defined — программист передаёт свой workload
     (`nova build --pgo-train="./bench/representative.sh"`).
   - Auto-generated — Nova prepare'ит из tests/benchmarks
     автоматически.
   - **Рекомендация:** оба пути; для stdlib-разработки используем
     `bench/` suite (план 09 Ф.6).

5. **PGO для stdlib и user-кода — раздельно или один профиль?**
   - **Один профиль** — проще, но если stdlib обновляется чаще
     user-кода, профиль устаревает.
   - **Раздельно** — stdlib имеет свой PGO профиль (один раз
     обновляется при release), user-код имеет свой.
   - Скорее всего сначала **один профиль** (простота),
     `cargo pgo`-style refinement позже.

**Не open question:** само решение «использовать PGO» — да,
очевидно полезно. Open это **как** интегрировать.

**Связь:**
- [Plan 09](../docs/plans/09-clang-migration.md) — Clang migration,
  prerequisite.
- [Plan 10](../docs/plans/10-pgo-integration.md) — stub для PGO
  работы. Полный план будет написан после плана 09.
- [docs/simplifications.md] → `[P-no-pgo-integration]` — пометка
  про текущее отсутствие.

**Когда закроется:** после реализации Plan 10 (PGO integration)
с benchmark'ами показывающими ≥10% прирост vs `--release` без PGO.

---

## Q-keyword-symmetry. Симметрия keyword'ов в declaration и literal: `effect`/`protocol` vs `handler`

> ✅ **РЕШЕНО 2026-05-22 (Plan 97).** Вариант **4** (полная симметрия)
> принят и зафиксирован в [D142](decisions/02-types.md#d142):
>
> 1. **(B)** keyword `handler` снят, литерал эффекта пишется через
>    `effect X { ops }`. Builtin тип `Effect[E, IRT]` переименован
>    в `Effect[E, IRT]` (см. [D87](decisions/04-effects.md#d87)
>    «Plan 97 amendment»).
> 2. **(D)** анонимный protocol-литерал введён — `protocol X { ops }`
>    в expression-position для one-off implementations (Channel-style
>    capability-split factory pattern). Type-position также получил
>    `protocol { sig* }` (анонимный protocol-тип в bound'ах /
>    параметрах), см. [D53 §628](decisions/02-types.md#d53) и Plan 15
>    `[P-15-anon-protocol-bound]` (снят).
>
> Реализовано в Plan 97 Ф.2 (anon-protocol type-position), Ф.3
> (handler→effect rename, lexer/parser/prelude/sweep), Ф.4
> (protocol-literal expression). Clean break — backwards-compat
> намеренно не сохраняется.
>
> **Решающий аргумент:** capability-split factory pattern окупает
> вариант 4, а симметрия declaration↔literal согласована с D52/D53
> (kind-token система) и D61/D87 (effect позиционная dispatch'ация).
>
> Историческое обсуждение оставлено ниже как справка.

---

**Контекст.** Сейчас Nova использует **разные keyword'ы** для
declaration и literal-формы одной сущности:

```nova
// Declaration:
type Cron effect   { run() -> () }
type Fan  protocol { run() -> () }

// Literal (только для effect):
let h = effect Cron { run() => () }       // keyword `handler`, не `effect`
let p = ???                                 // для protocol — нет literal-формы вообще
```

Возникает вопрос: **унифицировать ли** keyword'ы — использовать
`effect`/`protocol` и в declaration, и в literal-position?

```nova
// Предложение:
let h = effect Cron   { run() => () }      // keyword `effect`
let p = protocol Fan  { run() => () }      // новый — anonymous protocol-литерал
```

**Развилка 1 — `effect` vs `handler` для литерала эффекта:**

- **(A) Оставить `handler` (текущее).** Точнее в expression-position:
  чтение `let h = effect Logger { ... }` сразу говорит «это
  **обработчик** эффекта, не сам эффект». `effect Logger { ... }`
  может вводить в заблуждение: «это значение типа эффекта Logger?»
- **(B) Переименовать на `effect`.** Симметрия с declaration
  (`effect` ≡ `effect`). Breaking change, нужен migration sweep по
  всем тестам и spec'у.

**Развилка 2 — anonymous protocol-литералы:**

- **(C) Не делать (текущее).** Protocol реализуется через **типы с
  методами**. Идиома Rust/Go/Swift. Аргумент: для **reusable**
  протоколов (Hashable, Iter) named-форма естественна; anonymous-
  форма экономит мало в этих случаях.
- **(D) Делать `protocol Fan { run() => () }`.** Аналог Kotlin
  `object : Runnable { ... }` / Java anonymous classes / TS
  object-literal. Удобно для **one-off** реализаций без объявления
  отдельного типа.

**Уточнение:** protocols в Nova **бывают двух типов** use-case:
- **Reusable** (Hashable, Iter, From, Into) — лучше named-форма.
  Тип нужен в bound'ах, generic-сигнатурах, документации.
- **One-off** (Channel-style factory results, see use-case ниже) —
  выигрывают anonymous-форму. Тип нужен только как return-type
  factory-функции.

Old assumption «protocols обычно reusable» — **частично верна**.
Для большинства protocols (~80%) да. Но **structural-pattern**
«factory возвращает interface-implementations» делает one-off
случай **частым** в concurrency- и I/O-API.

**Важная аналогия:** Nova **уже** имеет anonymous protocol-impl —
это `effect Logger { ... }` для эффектов. Эффект структурно тот же
контракт (набор методов с сигнатурами) что и protocol. Различия:

| | Effect | Protocol |
|---|---|---|
| Структура контракта | методы (operations) | методы |
| Anonymous literal | `effect X { ... }` ✅ есть | нет (текущее) |
| Применяется в | `with X = h { ... }` | parameter / generic-bound |
| Типичный use-case | **one-off** (mock в тесте, transaction) | **reusable** (Hashable, Iter) |

Реальная причина разной идиомы — **частота one-off vs reusable
использования**, не философское различие. Для эффектов anonymous-
форма окупается, потому что handler'ы почти всегда одноразовые.
Для протоколов экономия меньше — программист один раз пишет
`type MyIter` и переиспользует.

**Слабые аргументы (отвергнутые при анализе):**

- ~~«Реализации спрятаны, не находятся grep'ом»~~ — найдутся через
  `grep "protocol Fan"`. То же что для `handler X`.
- ~~«Размывает AI-first locality»~~ — closures уже приняты в Nova
  (D22 closure-light/full); anonymous protocol — обобщение closure
  на multi-method, та же категория.
- ~~«Captures complexity»~~ — managed heap (D6) разрешает капчуры
  для closures, для protocol-литералов та же семантика.

**Реальный аргумент против (D):**

- **D40 «один очевидный путь».** Если protocol чаще reusable
  (named-type идиома лучше), anonymous-форма добавляет **второй**
  путь без существенной новой выразительности — анти-паттерн Nova.
- **Прецедент Swift.** Языки с **extension**-системой (Swift)
  обходятся без anonymous-impl. Nova-методы (`fn Type @method`)
  работают как extensions — Swift-подобная модель.

**Реальный аргумент за (D):**

- **Симметрия с handler-литералом** — оба «inline implementation
  of a method-contract». Текущая асимметрия неестественна.
- **Multi-method ad-hoc** удобен для случаев когда нужно реализовать
  protocol с 2-3 методами разово (closures покрывают только
  single-method case).
- **Прецеденты Kotlin/Java/TS** — устоявшийся паттерн.

**Прецеденты:**

| Язык | Effect-literal/handler | Anonymous protocol/interface |
|---|---|---|
| Nova (current) | `effect X { ... }` | нет |
| Koka | `with effect X { ... }` | нет |
| Eff | `handler { ... }` | нет |
| Java | — (нет effect system) | `new Runnable() { ... }` ✓ |
| Kotlin | — (нет effect system) | `object : Runnable { ... }` ✓ |
| Rust | — (нет effect system) | **нет** (только `impl Trait for Type`) |
| Go | — (нет effect system) | **нет** (только конкретные типы) |
| Swift | — (нет effect system) | **нет** (только `extension Type: Protocol`) |
| TypeScript | — | object-literal удовлетворяет interface структурно ✓ |
| OCaml | — | **нет** (только functor/module) |

Для **effect-литералов** прецеденты не помогают — Koka/Eff используют
свой keyword `handler`, как Nova сейчас. Для **anonymous protocol**
картина расколота: Kotlin/Java/TS — за, Rust/Go/Swift/OCaml — против.

**Почему мейнстрим без anonymous protocol-impl** — разные причины,
не один отвергнутый аргумент:

- **Rust** — невозможно из-за ownership/borrow-checker (нужен
  concrete type на стадии анализа). К Nova **не применимо** (нет
  ownership).
- **Go** — методы требуют named receiver-типа (Go-специфика). Можно
  через `var r Runner = (myStruct{}).func()` обходные пути. К Nova
  применимо частично (метод привязан к типу через `fn Type @m`).
- **Swift** — `extension Type: Protocol { ... }` достаточно
  идиоматичен, нет потребности в anonymous. Nova близка к Swift
  (extension-style методы).
- **OCaml** — functor/module system покрывает похожие use-cases.

**Почему мейнстрим с anonymous** (Kotlin/Java):

- **Java** — исторически (до 1.8 нет lambdas, anonymous classes
  были **единственным** способом передать callback).
- **Kotlin** — унаследовал, оставил для multi-method контрактов.
- **TypeScript** — структурная типизация делает любой object-литерал
  потенциальной impl интерфейса автоматически.

**Аргументы в Nova-контексте:**

1. **AI-first locality (R5.1).** Anonymous protocol-impl затрудняет
   поиск реализаций — программист (или LLM) не может grep'ом найти
   все impls protocol'а если часть из них в expression-position.
2. **Цена в symbols.** Anonymous-impl экономит ~2-3 строки vs
   `type X {} + fn X @m()`. Это малая выгода.
3. **Симметрия в declaration↔literal — слабый аргумент.** В Rust
   `struct X { f: int }` объявление и `X { f: 42 }` литерал тоже
   не имеют symmetric keyword'ов (нет `struct X { f: 42 }` в
   expression-position). Так делают **большинство** языков.
4. **`handler` keyword — узкий и точный.** Не пересекается ни с чем,
   парсер прост.

**Объём работ:**

- (B) переименование `handler` → `effect` в literal: правка lexer,
  парсер, ~30+ тестов в `nova_tests/`, ~10 spec-документов, AST-узел,
  codegen, interp. **Среднее изменение.**
- (D) добавление `protocol X { ... }` литерала: новый AST-узел
  `ProtocolLit`, парсер, type-checker (структурная проверка
  соответствия protocol'у), codegen (синтез anonymous-типа +
  методов). **Большое изменение.**

**Варианты комбинаций:**

| # | Effect-literal | Anon protocol | Объём | Net |
|---|---|---|---|---|
| 1 | `handler` (A) | нет (C) | 0 | статус-кво |
| 2 | `effect` (B) | нет (C) | средний | симметрия без новой фичи |
| 3 | `handler` (A) | `protocol` (D) | большой | новая фича без переименования |
| 4 | `effect` (B) | `protocol` (D) | большой+ | полная симметрия |

**Конкретный use-case — capability-split factory pattern (обнаружен 2026-05-10):**

Канонический паттерн «factory возвращает несколько связанных
interface'ов с общим скрытым state», каждый interface — отдельный
**capability** на одну сущность.

**Пример (гипотетический):**

```nova
// Гипотетический Lock с capability-split:
type Locker   protocol { lock() -> () }
type Unlocker protocol { unlock() -> () }

fn Lock.new() -> (Locker, Unlocker) {
    let state = MutexState { ... }
    let l = protocol Locker {
        lock() -> () => state.lock()
    }
    let u = protocol Unlocker {
        unlock() -> () => state.unlock()
    }
    (l, u)
}
```

Без anonymous protocol-литерала нужно объявить **два named-типа**
(`LockerImpl`, `UnlockerImpl`) с явными методами + обернуть. Цена —
**~3-4 лишних строки** и два типа в namespace которые **больше нигде
не используются** (полностью one-off).

**Сравнение с другими языками для этого use-case:**

| Язык | Boilerplate | Эквивалент |
|---|---|---|
| Nova-named (текущее) | средний | два named-типа + методы + constructor |
| Nova-anonymous (D) | **минимальный** | как в примере выше |
| Kotlin | минимальный | `object : Locker { override fun lock() = ... }` |
| TypeScript | минимальный | object-literal удовлетворяет structurally |
| Rust | большой | внутренние `struct LockerImpl` + `impl Trait` |
| Go | большой | named-типы `lockerImpl`, `unlockerImpl` |
| Swift | большой | type-erasing wrapper или внешние structs |

Use-case — **прямое противоречие** аргументу «D40 один путь»:
named-path работает, но **дороже на каждый capability-split API**.

**Замечание о текущем Channel в Nova:** Nova уже имеет `Channel[T]`
([D79](decisions/06-concurrency.md#d79)), но по **Go-модели** — один
объект, у которого есть и `send`, и `recv`. Это **другая** модель,
не capability-split.

**Capability-split** (вторая модель из Rust/Python/TS) — отдельная
дизайн-задача. Если когда-нибудь в Nova появится отдельный
`split_channel()` API (по образцу `tokio::sync::mpsc`,
`MessageChannel` JS, `multiprocessing.Pipe` Python) — anon protocol
будет идиомой.

**Реалистичные кандидаты в Plan 18 stdlib:**

- `Process.spawn(cmd) -> (Stdin, Stdout, Stderr)` — child-process с
  тремя capabilities.
- `HttpServer.bind() -> (Acceptor, ShutdownHandle)` — слушатель +
  capability для graceful shutdown.
- `Db.transaction() -> (TxReader, TxWriter, Commit)` — три role'а в
  транзакции.

Эти API **точно** появятся в зрелой stdlib. Тогда anon protocol —
естественная идиома.

**Предложение (обновлено).** Use-case есть, не «когда-нибудь
появится». Текущая дилемма:

1. **Если приоритет — минимальный bootstrap** — статус-кво
   (named-типы), документировать через guide «как писать Channel-
   style API в Nova». Стоимость в каждом stdlib-API — 3-4 строки.
2. **Если приоритет — идиоматический stdlib** — реализовать (D)
   до начала Plan 18 (stdlib roadmap). Channel-API и другие
   sync-primitives получают чистый идиом.

Решение между (1) и (2) зависит от того, **когда** начнётся
реальная stdlib работа. Если она через 2-3 сессии — (2) разумно
сделать **сейчас**. Если откладывается — статус-кво до v1.0-аудита.

До решения — статус-кво (вариант 1: `handler` + no anon protocol).
Anonymous-форма для протоколов **не запрещена принципиально** —
просто пока не реализована. Use-case Channel-style зафиксирован
как сильный аргумент для приоритезации.

**Связь:**
- [D42](decisions/02-types.md#d42), [D53](decisions/02-types.md#d53)
  — protocol как структурный контракт.
- [D61](decisions/04-effects.md#d61) — `handler` keyword в литерале.
- [D10](decisions/01-philosophy.md#d10), R5.1 — AI-first locality.
- Q23 — группировка методов (`methods Type { ... }`-блок) — другая
  related фича про синтаксис методов.

---

## Q-mn-*. M:N runtime — открытые вопросы

> **Источник:** [Plan 23 — M:N runtime roadmap](../docs/plans/23-mn-runtime-roadmap.md).
> Эти вопросы выявлены при проработке архитектуры перехода с N:1
> (single-thread bootstrap) на M:N (work-stealing scheduler на пуле
> OS-thread'ов). Закрываются D-блоками **до старта реализации M:N**.

### Q-mn-1. Memory model для shared mut при M:N

Что происходит когда fiber A на worker'е 1 и fiber B на worker'е 2
оба пишут в одно managed-heap поле без synchronization?

Варианты:
- **(a) UB (Rust-style):** запрещено компилятором через ownership analysis.
- **(b) Atomic-required:** shared mut между fiber'ами требует
  `Atomic[T]` обёртки. Type-checker enforce'ит.
- **(c) Channel-only:** shared mut между fiber'ами в принципе запрещён;
  общение только через `Channel`. Owner-actor pattern obligatorily.

Влияет на: type-checker rules, std.sync API surface, generic-bounds.

**Связь:** [D6](decisions/05-memory.md#d6), [D79](decisions/06-concurrency.md#d79),
[D91](decisions/06-concurrency.md#d91), Plan 18 std.sync.

### Q-mn-2. Fiber-migration boundary для `realtime nogc`

`realtime nogc { body }` ([D64](decisions/04-effects.md#d64)) обещает
«no GC pauses, no suspension». При M:N — fiber может мигрировать
между worker'ами; миграция через GC safepoint = pause. Решения:

- **(a)** Pin fiber'а к worker'у на время блока.
- **(b)** Запрет миграции через атрибут fiber'а (`no_migrate`).
- **(c)** Запрет M:N для fiber'ов, прошедших через realtime-блок
  (downgrade в N:1 на время).

**Связь:** [D64](decisions/04-effects.md#d64).

### Q-mn-3. Concurrent GC choice

BDW-GC (`libgc`) drop-in vs свой Go-style concurrent mark-and-sweep
с write-barrier'ами. Trade-off в Plan 23 «Слой 6».

**Рекомендация Plan 23:** BDW-GC сначала (быстрый путь к работающему
M:N), свой GC — отдельный milestone после v1.0 при необходимости.

Решение фиксируется отдельным D-блоком (D-mn-gc) до старта реализации.

**Связь:** [D6](decisions/05-memory.md#d6).

### Q-mn-4. Worker count auto-tuning

По умолчанию `nproc`? Через `NOVA_THREADS` env? Через `nova.toml`?
Configurable runtime через `Runtime.set_workers(n)` API?

Прецеденты: Go — `GOMAXPROCS` env + runtime API; Tokio — `worker_threads`
в build'е; Erlang — `+S N` флаг.

### Q-mn-5. `Blocking`-effect — pool size

Plan M:N добавляет honest blocking-pool thread'ов для `Blocking`
operations ([D50](decisions/06-concurrency.md#d50)). Размер пула —
fixed, auto-grow, либо bounded?

Прецеденты: Go's blocking-pool unbounded (один thread per blocking
call); Tokio `spawn_blocking` — bounded (default 512); JVM
`ManagedBlocker` — fork-join адаптивный.

### Q-mn-6. Effect handler stack — concurrent access

`with X = h { spawn { ... } }` — после spawn fiber имеет snapshot
handler-stack'а ([D80](decisions/06-concurrency.md#d80)). Под M:N —
handler-объект может быть **shared** между worker'ами. Если handler
stateful (captures `let mut` через closure) — UB.

Spec должен явно описать:
- Handler — immutable после capture? Mutable но требует internal
  synchronization?
- Если handler-method вызывается одновременно на двух worker'ах —
  это race? Или sequential consistency гарантируется?

**Связь:** [D80](decisions/06-concurrency.md#d80), [D61](decisions/04-effects.md#d61).

### Q-mn-7. SIGINT / signal handling в multi-thread runtime

Какой thread получает SIGINT при `Ctrl+C`? libuv даёт `uv_signal_t`
— на какой `uv_loop_t` его прибивать?

Сценарии:
- (a) Dedicated signal-thread, шлёт `uv_async_send` на main-worker.
- (b) Signal handler на main-worker'е напрямую (libuv позволяет на
  одном thread'е).
- (c) Каждый worker регистрирует — broadcast, идемпотентность через
  `nova_cancel_token_cancel(main_scope_token)`.

**Связь:** D92 (implicit main-scope, Plan 22 Ф.5).

---

## Q-cancel_scope-lambda-syntax. ЗАКРЫТО (2026-05-14) — `cancel_scope` keyword удалён, отмена через `supervised(cancel:)`

> ✅ **ЗАКРЫТО (2026-05-14).** Решение: **не** превращать `cancel_scope`
> (и прочие keyword-scope'ы) в trailing-fn функции. Вместо этого keyword
> `cancel_scope` **удалён полностью**, а внешняя отмена выражается
> именованным аргументом `cancel:` у `supervised`:
> `supervised(cancel: tok) { body }` (ревизия [D75](decisions/06-concurrency.md#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном),
> зависит от [D102](decisions/03-syntax.md#d102-именованные-аргументы-и-значения-параметров-по-умолчанию)
> «именованные аргументы»).
>
> **Почему не trailing-fn-функция:** `supervised` — неустранимый
> keyword (точка регистрации `spawn`-fiber'ов, D14/D50). Делать его
> функцией не выгодно — компилятор всё равно спецкейсит. А `cancel:`
> как именованный аргумент keyword-конструкции не требует ни нового
> keyword'а, ни scope-introduced `tok =>` binding.
>
> **Token-scope enforcement:** проблема не escape, а aliasing. Токен
> теперь caller-owned by construction (создаётся вне scope'а), поэтому
> «no escape» нечего защищать. Защищается double-bind — **runtime
> bind-check** «один токен → один живой scope», без affine/linear-типов.
> `tok.cancel()` на завершённом scope'е — безвредный no-op.
>
> Реализация / миграция bootstrap'а — [Plan 47](../docs/plans/47-supervised-cancel.md).
> Ниже — исходный анализ вопроса (сохранён для контекста).

**Контекст.** Сейчас `cancel_scope` — keyword-конструкция со специальным
синтаксисом ([D75](decisions/06-concurrency.md#d75)):

```nova
cancel_scope { tok =>
    spawn { do_thing(tok) }
    spawn { do_other(tok) }
}
```

Здесь `tok =>` — это не lambda-arrow, а **scope-introduced token binding**.
Token `tok` имеет тип `CancelToken` и доступен внутри блока для
передачи в spawn / для последующего `tok.cancel()` снаружи.

Это согласовано с другими keyword-scope конструкциями в Nova:
- `supervised { ... }` — keyword-block без token
- `parallel for x in iter { ... }` — keyword-for
- `forbid X { ... }` — keyword-capability
- `with_timeout(5.s) { ... }` — keyword-block с param
- `cancel_scope { tok => ... }` — keyword-block с token

### Предложение: унификация на trailing-fn (D43)

`cancel_scope` мог бы быть **обычной функцией prelude** с handler-param:

```nova
fn cancel_scope[T](body fn(CancelToken) Fail -> T) Fail -> T
```

Вызов через trailing-fn (D43):
```nova
cancel_scope() fn(tok) {
    spawn { do_thing(tok) }
    spawn { do_other(tok) }
}
```

### Плюсы

- **Единообразие.** Один pattern «trailing-fn с params» вместо
  special-cased keyword-syntax для `cancel_scope`.
- **AI-генерация safer.** LLM знает trailing-fn pattern из `list.map(...) fn(x) { ... }`,
  не нужно учить ещё один edge-case.
- **First-class.** `cancel_scope` можно передавать как value
  (`let cs = cancel_scope`), что невозможно с keyword'ом.
- **Парсер проще.** Нет special-casing для `cancel_scope` token —
  обычный fn-call.

### Минусы

- **Token leak.** `tok` теперь lambda-параметр, не scope-binding —
  его можно сохранить в outer `let mut t = ...; cancel_scope() fn(tok) { t = tok }`
  и использовать после exit'а из scope'а. Compile-time enforce
  «tok доступен только внутри scope'а» теряется. Mitigation —
  либо runtime-check (cancel'нуть уже мёртвый scope = error), либо
  ownership-rules (token не Copyable, потребитель owns).
- **Codegen overhead.** Сейчас `cancel_scope` — keyword с custom
  codegen (token-init + scope-bind за один шаг). При обычной fn-call
  нужно либо inline'ить body (compiler optimization), либо overhead
  от dynamic dispatch closure'а.
- **Асимметрия со `supervised` / `parallel for` / `forbid`.** Если
  унифицировать **только** `cancel_scope` — оно станет outlier'ом
  среди keyword'ов. Если унифицировать **все** — это **большой
  refactor structured concurrency** (отдельный D-блок).

### Связанные конструкции для унификации

Если идти этим путём, тот же rationale применяется к:

```nova
// Текущий → trailing-fn:
supervised { ... }              → supervised() fn() { ... }
parallel for x in iter { ... }  → parallel_for(iter) fn(x) { ... }
forbid Net { ... }              → forbid([Net]) fn() { ... }   // принимает list of effects?
with_timeout(5.s) { ... }       → with_timeout(5.s) fn() { ... }  // уже совместимо!
race { a, b }                   → race([a, b])                     // values, не block
```

`with_timeout` уже **близок** к trailing-block paradigm — он принимает
duration param. `cancel_scope` идёт следующим natural candidate'ом
для унификации.

### Что не решено

1. **Compile-time token-scope enforcement** — есть ли способ
   сохранить «tok недоступен вне scope'а» без keyword-syntax? Возможно
   через linear type / borrow checker аналог.
2. **Codegen efficiency** — компилятор должен inline'ить trailing-fn
   тело чтобы избежать closure overhead. Это требует guarantee от
   спеки.
3. **Breaking change для existing code.** Текущий `cancel_scope { tok => ... }`
   используется в `nova_tests/concurrency/cancel_scope_test.nv` и
   `cancel_stress_test.nv` — нужна миграция либо backward-compat
   period.
4. **Все keyword-scope конструкции одновременно или только `cancel_scope`?**
   Симметрия требует **все**, но это significant breaking change
   для всей structured-concurrency surface.

### Прецеденты

- **Kotlin** — `synchronized(lock) { body }` это **обычная функция**
  `inline fun synchronized(lock: Any, block: () -> R): R`. Trailing
  lambda is the norm.
- **Swift** — `withCheckedContinuation { continuation in ... }` —
  обычная функция, trailing closure with param.
- **Scala** — `Using.resource(r) { res => ... }` — function call.
- **Rust** — `thread::scope(|s| { ... })` — function call с closure-param.
- **Go** — нет analog'а (нет structured concurrency primitives).

Все прецеденты используют **function + closure-param**, не keyword.
Это аргумент в пользу унификации Nova'ы.

### Статус

**ЗАКРЫТО (2026-05-14).** Итоговое решение — в выноске вверху секции.
Кратко: ни `cancel_scope`, ни остальные keyword-scope'ы не становятся
trailing-fn функциями. `cancel_scope` удаляется, отмена выражается
`supervised(cancel: tok)` (именованный аргумент, D102). Унификации
*всех* keyword-scope'ов в функции **не происходит** — `supervised`
остаётся keyword'ом из-за `spawn`-registration магии.

Что из исходных «что не решено» как разрешилось:
1. **Compile-time token-scope enforcement** — признано ненужным:
   токен caller-owned, escape = no-op; защищается только aliasing,
   через runtime bind-check.
2. **Codegen efficiency** — `supervised(cancel:)` остаётся keyword'ом
   с custom codegen, closure-overhead вопрос снят.
3. **Breaking change для existing code** — `cancel_scope_test.nv` и
   `cancel_stress_test.nv` мигрируются в [Plan 47](../docs/plans/47-supervised-cancel.md).
4. **Все keyword-scope'ы или только `cancel_scope`** — только
   `cancel_scope` (удаляется); `supervised`/`parallel for`/`select`
   остаются keyword'ами. Асимметрии нет — `cancel_scope` не
   «становится функцией», а схлопывается в аргумент существующего
   keyword'а.

**Связь:**
- [D43](decisions/03-syntax.md#d43) — trailing-fn syntax.
- [D102](decisions/03-syntax.md#d102-именованные-аргументы-и-значения-параметров-по-умолчанию)
  — именованные аргументы; `supervised(cancel:)` опирается на D102.
- [D75](decisions/06-concurrency.md#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном)
  — ревизованный: `cancel_scope` удалён, отмена через `supervised(cancel:)`.
- [D50](decisions/06-concurrency.md#d50) — `supervised`/`parallel for`/`spawn`.
- [Plan 47](../docs/plans/47-supervised-cancel.md) — реализация / миграция.
  keyword'ы (одна team — либо все унифицируются, либо все остаются).
- [Q-keyword-symmetry](#q-keyword-symmetry) — related concern про
  symmetry keyword'ов declaration/literal.

---

## Q-D93-sync-async-stop. Sync-vs-async stop_cb contract в D93 API ✅ ЗАКРЫТО

> **Закрыто:** Plan 22 Ф.8 (2026-05-11) — `NovaStopMode` enum
> `{SYNC, ASYNC}` добавлен в D93 API. `nova_sched_cancel_all_pending`
> различает: SYNC → unpark immediate, ASYNC → ждёт backend wake.
> Sleep stop_cb стал ASYNC, close-wait busy-loop удалён из
> `_nova_sleep_via_libuv`. Подробно — [D93](decisions/06-concurrency.md#d93)
> + Plan 22 Ф.8 секция. Историческая запись сохранена ниже.



**Контекст.** D93 park/wake API определяет `NovaCancelStopCb` —
callback, который вызывается из `nova_cancel_token_cancel` через
`nova_sched_cancel_all_pending`. После stop_cb idempotent loop
делает `parked[i] = false` (synchronous unpark) — fiber resumes
immediate, видит `cancel_requested = true`, throw'ает.

**Текущее предположение:** stop_cb выполняется **synchronously** —
после возврата из stop_cb всё необходимое для cleanup'а handle'а
уже сделано. Под этим предположением `parked[i] = false`
сразу после stop_cb безопасен.

Это **верно для sleep handle с текущей Ф.4/Ф.6 реализацией**:
stop_cb делает `uv_timer_stop + uv_close(close_cb)`, но fiber всё
равно делает busy-wait через `uv_run NOWAIT` пока close_cb не
выполнится — поэтому handle уже фактически освобождён к моменту
возврата stop_cb.

**Проблема — Plan 22 Ф.8 (close-cb state machine):**

Production-grade refactor хотел убрать busy-loop wait и сделать
park ждать close_cb напрямую (без второго park'а). Архитектура:
timer_cb инициирует close (НЕ wake), close_cb делает wake. Один
park на весь lifecycle. Stop_cb (для cancel) тоже только initiates
close — не делает synchronous wait.

Это сломало контракт с `cancel_all_pending`: после stop_cb он
делает `parked[i] = false`, fiber resume'ится **до** close_cb, и
sanity-check (`stage == CLOSED`) abort'ит. Откат к Ф.6 версии.

**Что нужно:** D93 должен **формализовать** sync-vs-async stop_cb
semantic, чтобы cancel_all_pending мог различать:

```c
typedef enum {
    NOVA_STOP_SYNC,    /* handle полностью freed после stop_cb return */
    NOVA_STOP_ASYNC,   /* stop_cb лишь инициировал close; ждём wake'а от backend */
} NovaStopMode;

typedef NovaStopMode (*NovaCancelStopCb)(void* handle);
```

`cancel_all_pending` для `SYNC` делает `parked[i] = false` сразу;
для `ASYNC` — оставляет parked, полагается на backend wake (uv close_cb).

**Use-cases:**

- **Sleep handle** (Plan 22 Ф.8) — ASYNC: stop_cb инициирует
  `uv_close`, wake придёт из close_cb.
- **Channel waitlist** (Plan 21 Ф.1) — SYNC: stop_cb отвязывает
  waitlist node, handle (waitlist node) полностью убран immediate.
- **Socket read** (Plan 23+ `std.net`) — ASYNC: stop_cb делает
  `uv_read_stop` + `uv_close`, wake из close_cb.
- **File read** (Plan 23+ `std.fs`) — ASYNC: stop_cb делает
  `uv_cancel` на in-flight `uv_fs_t`, wake из request callback.

**Status (2026-05-11).** Plan 22 Ф.8 — DEFERRED. Прототип
выявил проблему, откат к Ф.6 семантике. Текущая ms-busy-loop на
close_cb (через `uv_run NOWAIT`) — pragmatically acceptable (1-2
iterations typical), не блокер production deployment.

**Когда фиксировать.** Перед Plan 21 (Channel) реализацией. Каналы
требуют чёткого SYNC контракта; sleep и socket — ASYNC. Без
формального enum смешать оба в одном API — UB.

**Связь:** Plan 22 Ф.8 (deferred), Plan 21 channel waitlist,
Plan 23 socket-read/file-read, D93 spec.

---

## Q-axiom-binder-type. Тип binder в axiom: `Option<TypeRef>` vs отдельный enum

**Контекст.** `EffectAxiom.binders: Vec<(String, Option<TypeRef>)>` — `None` означает
"тип не указан явно". В SMT encoding при `None` делается inference из usage в формуле,
а если inference не находит — дефолт `SortRef::Int`.

**Проблема.** `None` читается как "отсутствует/нет значения", хотя семантически это
"untyped" — совсем другое намерение. Скрывает смысл на call-сайтах.

**Предлагаемое именование:**

```rust
pub enum BinderType {
    Untyped,           // axiom name(id) => ...      — inference + дефолт Int
    Typed(TypeRef),    // axiom name(id int) => ...  — явный sort
    Generic,           // axiom name[T](id T) => ... — T из generics (V2)
}
```

Тогда `match` на call-сайтах читается как документация, а не загадка `None`.

**Когда менять.** При добавлении третьего варианта (Generic как отдельный BinderType,
а не флаг `is_generic` на уровне аксиомы) — тогда рефактор окупится. Пока два варианта
(`Option<TypeRef>`) работает корректно, это техдолг читаемости.

**Связь:** Plan 33.3 Ф.9 (contracts), `compiler-codegen/src/ast/mod.rs` `EffectAxiom`,
`verify/pipeline.rs` `encode_axiom`.

---

## Q-with-deadline-vs-within. `with_deadline[T](deadline_ms, body)` — отмена по точке времени

**Контекст.** В stdlib `std/concurrency/cancellation.nv` есть `within[T](timeout_ms, body)`
— отмена через duration от вызова. В distributed системах (Go context, gRPC, Tower)
принято передавать **deadline** (абсолютный timestamp) между сервисами, чтобы каждый
hop подсчитывал свой timeout как `min(local_timeout, deadline - now)`.

**Предложение.** Добавить `with_deadline[T](deadline_ms_unix, body fn() -> T) -> Option[T]`:
```nova
let deadline = parent_request_deadline_ms()   // unix-ms, например, 1716470000000
let r = with_deadline(deadline, || fetch_data())
```
Реализация — тривиальная обёртка над `within`:
```nova
fn with_deadline[T](deadline_ms int, body fn() -> T) -> Option[T] {
    let now = Time.now()                  // unix-ms (через Time.now_ms когда будет)
    let remaining = if deadline_ms > now { deadline_ms - now } else { 0 }
    within(remaining, body)
}
```
Зависимости: `Time.now()` возвращает unix-ms (или новый `Time.now_ms()` с правильной
semantics — runtime сейчас `now()` returns monotonic ms, не unix).

**Trade-off.** `with_deadline` удобен для RPC-цепочек, но Time.now_ms() vs Time.monotonic
— два разных concept'а (wall vs monotonic clock). within работает на monotonic; deadline
обычно на wall. Нужна decision про какие часы используем.

**Связь:** `std/concurrency/cancellation.nv`, `Time` effect (`std/time/duration.nv` §289 comment).

---

## Q-tok-checked. `tok.checked()` — cooperative yield + cancel-check одной операцией

**Контекст.** Сейчас в CPU-bound loop'е без `Time.sleep` / `Channel.recv` fiber может
не yield'нуть scheduler'у долго → отмена «не успевает»:
```nova
for i in 0..10_000_000 {
    if tok.is_cancelled() { return }      // флаг проверяем, scheduler не дёргаем
    heavy_computation(i)
}
```

Если `tok.cancel()` из другого fiber'а — этот fiber может НИКОГДА не yield'нуть,
и cancel не сработает до конца loop'а.

**Предложение.** Метод `tok.checked()` — explicit yield + cancel-throw одной операцией:
```nova
for i in 0..10_000_000 {
    tok.checked()                  // 1) yield; 2) if cancel → throw CANCEL
    heavy_computation(i)
}
```

Аналоги: Go `runtime.Gosched()` + `ctx.Err()`, Rust `tokio::task::yield_now()`.

Реализация: `nova_cancel_token_checked` — wrapper над `nova_fiber_yield()` +
`nova_throw_cancel_reason(scope.cancel_reason_ptr)` если cancelled.

**Trade-off.** Может быть путаница с `is_cancelled()` (non-throwing bool). API surface
ширится. Зато явный pattern для CPU-loops.

**Связь:** Plan 49 Ф.2 (cooperative cancel), `nova_fiber_yield` (`fibers.h`).

---

## Q-cancel-token-with-timeout. `CancelToken.with_timeout(ms)` factory — auto-cancelling token

**Контекст.** Хочется factory который создаёт CancelToken и сам отменяет его через
заданное время — как `AbortSignal.timeout(5000)` в TC39 (WHATWG DOM standard).

**Желаемое API:**
```nova
let tok = CancelToken.with_timeout(5000)   // через 5 сек авто-cancel
do_long_work(tok)
```

**Проблема — design choice.** Кто-то должен в фоне ждать 5 секунд и вызвать `cancel()`.
Варианты:

1. **Background fiber outside structured scope** — нарушает structured concurrency
   (Plan 47 D50/D75: spawn только внутри supervised). Token живёт каллер-side, fiber'у
   нужен parent-scope для drop'а. Сложно без leak'а.

2. **OS timer callback (libuv `uv_timer_t`)** — callback в event-loop thread вызывает
   `nova_cancel_token_cancel()`. Обходит fiber-runtime, нужен thread-safe path.

3. **Lazy timer queue** — separate fire-and-forget queue для таких timer'ов с GC
   ownership. Новая infrastructure.

**Текущий workaround** (works today, чуть более многословно):
```nova
let tok = CancelToken.new()
supervised(cancel: tok) {
    spawn { Time.sleep(5000); tok.cancel("timeout") }
    spawn { do_long_work(tok) }
}
```
Это уже `within[T]` pattern в stdlib (`std/concurrency/cancellation.nv`).

**Trade-off.** Factory удобнее (один-liner для async patterns), но требует или нарушения
structured-concurrency (background fiber outside scope), или новой timer-queue
infrastructure. Решение влияет на rest of cancellation design.

**Связь:** Plan 49 (cancellation), Plan 22 (libuv timers), TC39 AbortSignal.timeout proposal.

---

## Q-context-value-equivalent. Go `context.Value` (typed request-scoped values) для Nova

**Контекст.** В distributed системах request-scoped values (trace ID, user ID, locale,
deadline) пробрасываются через всё дерево вызовов. Передавать каждый параметром →
много boilerplate; глобальные variables → не thread-safe / не fiber-scoped.

**Go-style (минусы):**
```go
ctx := context.WithValue(parent, traceKey, "abc-123")
trace := ctx.Value(traceKey).(string)   // type-assert, no compile-time safety
```
Key — `interface{}`, value — `interface{}`. Нет type safety. Конвенция использовать
private types для key чтобы избежать collision'ов — ad hoc.

**Желаемое API (typed):**
```nova
context.set[TraceId]("abc-123")
context.set[UserId](42)

let trace = context.get[TraceId]()      // -> Option[TraceId], typed
let user  = context.get[UserId]()       // -> Option[UserId]
```

**Альтернатива — Nova effects (уже есть):**
```nova
type Trace effect { current() -> str }
with Trace = trace_handler("abc-123") {
    do_work()    // внутри: Trace.current() → "abc-123"
}
```
Effects дают тот же use-case с typed dispatch + handler swap.

**Trade-off.** Два пути:
1. Использовать existing effects (не вводить новый API) — паритет с Go context,
   но handler-ceremony более многословно.
2. Ввести typed `context.set/get[T]()` API — короче на use-site, но new infrastructure
   (где живут values: TLS / fiber-locals / scope-stack?), propagation rules через spawn
   / handler boundary не определены.

**Решение:** Нужен полноценный design plan (Plan 51 tentative). Use-cases собрать,
сравнить effects-based vs typed-context API, решить storage model. Не в текущем sprint.

**Связь:** Plan 47/49 (cancellation), spec/decisions/04-effects.md (effects design),
Go `context` package, Rust `tokio::task_local!`, TC39 `AsyncContext`.

---

## Q-multi-bound. Intersection-multi-bound syntax `[T A + B]`

> 🟡 **PROPOSED — Plan 101.3 (closes this).** Закрывается через `+`-syntax
> в `[T Bound1 + Bound2 + ...]`. Параллель Rust `<T: A + B>`.

**Контекст.** [D72](decisions/02-types.md#d72) §«Multiple bounds»
сейчас требует anonymous-protocol или named-composition:

```nova
fn cache[T protocol { hash() -> u64, eq(other Self) -> bool }](xs []T) -> ...
// или
type HashableEq protocol { hash() -> u64, eq(other Self) -> bool }
fn cache[T HashableEq](xs []T) -> ...
```

[Q-bounds](#q-bounds-синтаксис-bounds-на-дженериках-если-будут)
§«Тонкие места 1» оставило открытым inline-syntax для multi-bound.

**Решение (Plan 101.3 proposal):** `[T A + B]` — intersection-bounds
через `+`, параллель Rust. **`+` выбран** vs `&` (TS) потому что:
- Familiar для Rust-программистов (Nova target audience overlaps).
- Не конфликтует с `,` (multi-param separator) и `&` (bitwise).
- `[…]` — pure type context, no arithmetic possible → unambiguous.

Применим везде где D72 bound допустим: free fn (`fn dedup[T A + B]`),
type-decl (`type Cache[K A + B, V]`), `fn[T]` prefix (`fn[T A + B] []T @method`).

**Параллель индустрии:** Rust `+`, TS `&`, Kotlin `where` clause, Go
embedded composition. Nova `+` совпадает с Rust.

**Status:** **proposed** в Plan 101.3 (P3, ~1 dev-day). Закроется как
часть Plan 101 closure.

**См. также:** [D72](decisions/02-types.md#d72),
[D145](decisions/02-types.md#d145), [Plan 101.3](../docs/plans/101.3-multi-bound.md).

---

## Q-representation-bound. Concrete-type bounds (`fn[T int]`, `fn[T User]`)

> 🟡 **PROPOSED — Plan 102 future (out of scope Plan 101).**

**Контекст.** [D72](decisions/02-types.md#d72) фиксирует «bound — это
protocol-тип». Concrete types — newtype'ы (`type UserId u64` per D52),
records (`type User { ... }`) — **не могут** быть bound'ами.

```nova
type UserId u64        // D52 newtype
fn[T u64] []T @method   // currently ❌ — u64 не protocol

type User { id u64, name str }
type Profile { use user User, avatar Url }   // D39 embed (not subtype)
fn[T User] T @method                          // currently ❌
```

**Use cases:**
1. **Newtype-aware bounds:** UserId/SessionId/OrderId все — `u64`
   newtype'ы; хочется generic method для всех «u64-representable»:
   `fn[T : repr u64] []T @sum() -> u64`.
2. **Embed-aware bounds:** Profile embeds User (D39); хочется generic
   method для всех «User-embedding» record'ов:
   `fn[T : has User] T @greet() -> str`.

**Дизайн options:**

1. **Auto-derived protocol from concrete-type shape:**
   - Record `type User { id u64, name str }` auto-derives protocol
     `User-shape { id() -> u64, name() -> str }`.
   - `fn[T User]` ≡ `fn[T User-shape]` (structural conformance).
   - Profile (via D39 embed) auto-satisfies через delegated accessors.
2. **Explicit representation-bound:** `fn[T : repr u64]` —
   T's runtime representation = u64.
3. **Explicit embed-bound:** `fn[T : has User]` — T contains User
   field (record-only).
4. **Combination:** все три.

**Cross-language precedent:** Nova-уникальная — Rust/Go/TS/Kotlin/Scala
все требуют explicit trait/protocol/interface declaration. Auto-derive
from record-shape — это **Nova edge** opportunity.

**Risks:**
- Compiler must walk type-tree для shape-matching.
- Может ввести silent matches («user думал что не satisfies, а satisfies»).
- Дизайн compatibility с D17 «no inheritance».

**Status:** **proposed для Plan 102** (post Plan 101). Отдельная design
phase Ф.0 нужна — это значительное расширение semantic model. P3
(polish over correctness).

**Источник:** обсуждение 2026-05-24 во время Plan 101 design.

**См. также:** [D72](decisions/02-types.md#d72),
[D145](decisions/02-types.md#d145), [D52](decisions/02-types.md#d52)
(newtype), [D39](decisions/02-types.md#d39) (record-embed).

---

## Финальное напоминание

Прежде чем продолжать **дизайн**, прочитай:
1. [README.md](../README.md) — главные тезисы
2. [decisions/](decisions/) — все принятые решения с обоснованием
3. discussion-log (личный, в отдельной репе) — путь к этим решениям

Прежде чем менять решение — **прочитай его обоснование**. Многие
решения поддерживают друг друга. Изменение одного может потребовать
пересмотра нескольких.
