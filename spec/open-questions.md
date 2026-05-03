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
2. Куда отнести функции с несколькими эффектами (`Net Db Log Throws`)?
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

## Q4. Семантика `Alloc[Cycle]` collector'а

**D21 ввёл `~&T` и cycle collector.** Не уточнено:

1. **Когда запускается collector?** По таймеру? По threshold (например,
   1 MB новых `~&T` объектов)? По явному вызову `collect_cycles()`?
2. **На каком потоке?** Отдельный GC-поток? В произвольном
   application-потоке? Каждый поток сам собирает свои `~&T`?
3. **Скорость инкрементальности.** Bacon-Rajan делает шаги — но
   сколько работы за шаг? Это влияет на «паузы» (хоть и маленькие).
4. **Mark phase синхронизация.** Как читатели `~&T` ссылок
   синхронизируются с collector'ом? Atomic-доступ или barrier'ы?

Это **runtime-implementation**, не дизайн языка. Но влияет на
производительность.

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

**Q5.4. Assertion failures в debug.** Это `Panic` или обычные `Throws`?
Если `Panic` — нельзя поймать в обычном коде (только supervisor видит).
Если `Throws[AssertionError]` — нужно везде декларировать. Скорее
всего `Panic` (это «сбой инварианта», не бизнес-ошибка), но не
зафиксировано.

---

## Q6. Effect polymorphism — синтаксис

**Текущий пример:**
```nova
fn map_eff[T, U, E](xs [T], f (T) E -> U) E -> [U]
```

`E` — параметр-эффект. Не определён точный синтаксис:
- Можно ли `E1, E2`? `[E1, E2]`?
- Как ограничивать (bound) эффект-параметры?
- Как стирать (`erase`) для разнородных задач — D12 коснулся, но
  не всё детализировано.

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
- `mut self`, `:` в аннотациях типа, `throws` без `Throws[]` — устарели

**Что делать:** обновить под текущие D-решения. Это просто переписать
все примеры с новым синтаксисом, без изменения смысла.

`syntax.md`, `effects.md`, `decisions/01-philosophy.md` (D9) актуализированы. `audit.nv`
тоже актуален. `paradigm.md` помечен как устаревший до полной переписи.

---

## Q9. Стандартная библиотека

Не описана структура. Что есть в stdlib:
- `String`, `Vec`, `HashMap`, `Option`, `Result` — очевидно
- `LinkedList`, `Tree`, `Graph` — какие именно типы? Какие на `~T`,
  какие на `~&T` (D21)?
- `Json`, `Sql.builder` — упоминаются в `audit.nv`, не описаны
- `Time`, `Random`, `Net`, `Db` — стандартные эффекты, не определены
  их операции
- HTTP, WebSocket, gRPC — что в core, что в external?

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
pattern match с `Throws[InvalidVariant]`. Конфликт значений запрещён
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

## Q19. Trailing-block синтаксис `expr { x => body }` — общий механизм или фиксированные примитивы?

**Контекст.** В коде Nova используется паттерн «функция/конструкция +
блок в качестве последнего аргумента»:

```nova
race {
    body(),
    sleep(dur).then { throw Timeout }    // .then { ... } — trailing block
}

with_timeout(2.seconds) {                 // trailing block
    Db.exec(...)
}

supervised {                              // trailing block
    spawn handle_requests()
}

with Db = real_db {                       // блок после with
    transfer(alice, bob, 100)?
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
**ошибкой** — заменён на handler `with Throws[E] = (err) => ... { ... }`.

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

## Q20. Нужен ли `defer`?

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
   fn with_file[T](path str, body fn(File) Throws[IoError] -> T)
       Throws[IoError] -> T => {
       let f = File.open(path)?
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
  `cancel_scope` (срабатывает ли defer при отмене fiber'а?), с
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
    Db Net Log Trace Throws[OrderError] Async Time Random
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
alias StandardWeb = Db Net Log Trace Throws Async Time

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
  реально использует только `Db Throws` — компилятор разрешает
  (алиас как ≤-подмножество, но это ослабляет D28 «гарантия чистоты —
  проверенный факт») или требует все эффекты алиаса использовать
  (тогда алиас бесполезен)?
- **Параметризованные эффекты в алиасе** (`Throws[E]`): что значит
  `alias A = Throws[OrderError]`, `alias B = Throws[UserError]`,
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
export protocol StandardWeb : Db, Net, Log, Trace, Throws, Async, Time

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

## Q22. Унификация `type` / `protocol` в один keyword? ✅ ЗАКРЫТО ([D53](decisions/02-types.md#d53))

[D53](decisions/02-types.md#d53) принял унификацию: `protocol` стал
**kind-токеном** под единым `type`-keyword. Все объявления типов
(включая структурные контракты) идут через `type`:

```nova
type Hashable protocol { hash() -> u64 }
type Logger protocol { log(msg str) -> () }
type Db protocol { query(sql str, args []any) -> []Row }
type any protocol { }                      // top-type
```

Анонимный protocol-тип в позиции параметра — `protocol { ... }` (с
обязательным префиксом, симметрично `[]T`, `(A, B)`, `fn() -> T`).

D42 помечен revised → D53. Семантика структурной типизации,
generic-параметров и эффектов сохраняется — изменился только
синтаксис объявления.

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
    query(sql str, args []any) -> []Row
    exec(sql str, args []any) -> ()
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
- Параметризация `Throws[E]` — она у data-типа или у protocol'а?
  (Сейчас `protocol Throws[E]`, и это согласовано с другими
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
fn Account mut @withdraw(amount money) Throws[Overdraft] => ...
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

    fn mut @withdraw(amount money) Throws[Overdraft] =>
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
fn all[T FromRow]() Db Throws -> []T
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
   эффект-операции? (Эффекты — это `protocol`, [D18](decisions/04-effects.md#d18-эффекты-объявляются-через-protocol-не-type),
   так что технически да.)
3. Что бывает с уже принятыми решениями про anonymous structural
   type в позиции параметра ([D42](decisions/02-types.md#d42)):
   `fn f(x { show() -> str })` — можно ли это перенести в bound:
   `fn f[T { show() -> str }]()`?

**Статус.** Открыт как «предзафиксированная форма» — если bounds
будут, использовать `[T Bound]` без двоеточия. Целиком решение о
вводе bounds откладывается до post-MVP.

---

## Q-anon-effect. Анонимный эффект в позиции эффекта

**Контекст.** [D42](decisions/02-types.md#d42) разрешает анонимный
структурный тип в позиции параметра:

```nova
fn log_one(x { show() -> str }) Log -> () =>
    Log.info(x.show())
```

После [D18-revised](decisions/04-effects.md#d18-эффекты-объявляются-через-protocol-не-type)
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
   (например, `Throws[EncodeError]`) — генерируемая функция должна
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
        args: args
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
4. **`Db.query` сигнатура.** ✓ **Решено** (предварительно): через
   `Sql`-тег. `fn Db.query(q Sql) Throws[DbError] -> []Row`,
   `fn Db.exec(q Sql) Throws[DbError] -> int`. Прямой `(sql str, args
   []SqlValue)` остаётся **непубличным** для случаев, когда `sql`-тег
   не годится. Финализируется в D56.
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

### Q-array-api.5. Slicing — есть ли `xs[a..b]`

Range-индексирование — частая фича slice-семантики. В Go, Rust, Swift
есть. В Nova:
- `..` оператор range уже зафиксирован (`for i in 0..n`,
  [D38](decisions/03-syntax.md#d38)).
- Применим ли он в индексировании `xs[a..b]`?

Не зафиксировано. Предполагается **есть**, но D-решения нет.

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

## Q-embed-syntax. Синтаксис embed/delegation — `use Type` vs альтернативы

**Контекст.** [D39](decisions/02-types.md#d39) фиксирует embed через
keyword `use`:

```nova
type AuditedAccount {
    use Account                        // имя поля = "Account"
    audit_log []AuditEntry
}

type Wrapper {
    use w HashMapIter[K, V]            // имя поля = "w" (alias)
}
```

`use` — multi-purpose: и для embed (D39), и потенциально для
импортов/локальных алиасов (D29 использует `import`, но `use` тоже
рассматривался). Это создаёт перегрузку семантики.

Возникли альтернативные синтаксисы. Вариант B (Go-style голый тип)
предложил пользователь как более лаконичный.

### Альтернативы

#### A. Текущий D39 — `use Type`

```nova
type AuditedAccount {
    use Account
    audit_log []AuditEntry
}
type Wrapper { use w HashMapIter[K, V] }
```

**За:** keyword явный, AI видит embed с первого токена.
**Против:** `use` многозначен (embed, потенциально импорты).

#### B. Go-style — голый тип, без keyword

```nova
type AuditedAccount {
    Account                            // имя поля = "Account"
    audit_log []AuditEntry
}
type Wrapper {
    HashMapIter[K, V] as w             // alias через `as`
}
```

**Парсер** различает по case первого токена:
- PascalCase → embed (имя поля = имя типа).
- snake_case → имя поля + тип следующим токеном (обычное поле).

**За:**
- Самая краткая форма.
- Прецедент Go (известный паттерн).
- Нет нового keyword'а.
- Симметрично field punning (D17): «имя по умолчанию из контекста».

**Против:**
- Менее явное намерение — нет keyword'а, программист полагается на
  case-convention.
- Несогласованность alias-порядка: обычные поля `name type`,
  embed `Type as name` — обратный порядок.
- AI-locality чуть ниже (нужно знать правило case).

#### C. `embed Type` — отдельный keyword

```nova
type AuditedAccount {
    embed Account
    audit_log []AuditEntry
}
type Wrapper { embed w HashMapIter[K, V] }
// или с as:
type Wrapper { embed HashMapIter[K, V] as w }
```

**За:**
- Keyword **точно** описывает намерение («embed = встроить»). `use`
  более общий.
- Нет коллизий с другими решениями.
- AI-locality высокая.

**Против:**
- Ещё один keyword в языке.
- Очень похоже на A — выигрыш только в семантической точности слова.

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

## Финальное напоминание

Прежде чем продолжать **дизайн**, прочитай:
1. [README.md](../README.md) — главные тезисы
2. [decisions/](decisions/) — все принятые решения с обоснованием
3. discussion-log (личный, в отдельной репе) — путь к этим решениям

Прежде чем менять решение — **прочитай его обоснование**. Многие
решения поддерживают друг друга. Изменение одного может потребовать
пересмотра нескольких.
