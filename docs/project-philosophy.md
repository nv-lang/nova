# Project philosophy — направляющие принципы для развития Nova

Этот документ — **директивный**, для всех контрибьюторов (включая AI-агентов).
Описывает не «как написано» (это в `spec/`), а **«как принимать решения»** при
расширении языка и runtime'а.

## 1. Nova не в проде — революционный язык важнее обратной совместимости

**Принцип (2026-05-06):** Nova находится в bootstrap-фазе. Никаких production-
зависимостей нет. Любые изменения допустимы — большие refactor'ы, breaking
syntax changes, переписывание тестов, ломка bootstrap-семантики — если они
оправданы:

- лучшими возможностями языка,
- лучшим user experience,
- движением к революционным целям из [`spec/revolutionary.md`](../spec/revolutionary.md) и [`spec/decisions/01-philosophy.md`](../spec/decisions/01-philosophy.md).

### Когда **минимальный** vs **правильный** вариант

При появлении дилеммы — **выбирать правильный**. Минимальные обходы (workaround,
type-erasure, builtin вместо handler-vtable, legacy code path для совместимости
тестов) копят техдолг и расходятся со spec'ой.

Примеры (из исторической работы 2026-05-06):

| Задача | Минимальный (плохой) выбор | Правильный (лучше) выбор |
|---|---|---|
| `Time.sleep` в codegen | builtin, минующий effect-system | объявить `effect Time` + handler-vtable |
| `Detach` ключевое слово | inline-block без `Detach` эффекта в сигнатуре | объявить эффект, проверять в сигнатуре |
| `spawn` вне `supervised` | оставить eager-blocking для 28 legacy-тестов | компилируется как error, переписать тесты |
| `throw` keyword | `abort()` стуб | `nova_throw` через fail-frame |
| spawn-результат | type-erased `nova_int` через ctx-поле | `unit` через mut-захваты (D50/D71) |
| fiber assert UB | оставить, документировать | redirect→throw в fiber-context |

### Trade-offs нужно проговаривать

Перед крупным изменением — **озвучить пользователю**:
- что приобретаем (новые возможности, корректная семантика, ближе к spec'е),
- что теряем (количество переписываемых тестов, временное падение test-rate),
- что меняется в spec'е (если затрагиваем decisions).

Но **не использовать trade-offs как повод выбрать упрощение**. Озвучили → начали
делать правильное.

### Записывать rationale в `simplifications.md`

Если задача всё-таки **отложена** (нет ресурса сделать правильно сейчас):

- Записать в `docs/simplifications.md` с чётким **rationale** (почему отложено) и
  **roadmap** (порядок шагов к правильной реализации).
- Не использовать «отложено» как тихое разрешение оставить tech-debt без плана.

## 2. Spec — живой документ

Spec в `spec/decisions/` — авторитетный, но **не неприкосновенный**. Если
реализация показывает что decision был неудачным или противоречит другим — **меняй
spec**, фиксируя эволюцию в `spec/decisions/history/evolution.md`.

Примеры эволюции, которые уже произошли:

- D14 (изначально `Async` как эффект) → REVISED по D62 (Async ambient).
- D50 (изначально подразумевал `let r = spawn ...`) → уточнено в D71 (spawn unit).

## 3. Рекордно-короткий путь к революционным фичам

Spec/revolutionary.md перечисляет ключевые revolutionary элементы Nova:
эффекты вместо async/throws, structured concurrency, single capability source,
no-magic-handlers, AI-first syntax. Любая работа должна **приближать** к этому
видению.

Если задача не приближает — спросить себя: **зачем она сейчас?** Иногда ответ
есть (закрыть тест который блокирует CI), иногда нет — и тогда отложить.

## 4. Сторонние библиотеки runtime — не патчить

**Принцип (2026-05-06):** библиотеки в `compiler-codegen/nova_rt/`, которые
являются сторонним кодом, **используются как есть**. Никаких правок в их
исходниках. Обёртки и расширения — только в **наших** runtime-файлах.

**Что относится к третьесторонним:**
- [`minicoro.h`](../compiler-codegen/nova_rt/minicoro.h) — stackful coroutines
  (assembly + C, single-header, [edubart/minicoro](https://github.com/edubart/minicoro)).
- Boehm GC, если/когда подключим (`alloc_boehm.c` использует public API).
- Любые third-party headers или sources, добавленные в `nova_rt/`.

**Что наше и можно править:**
- `effects.h` / `effects.c` — Fail-frames, test-frames, effect handler
  vtable infrastructure.
- `fibers.h` — обёртки над minicoro public API (mco_create, mco_resume,
  mco_yield, mco_status), supervised scope, scheduler.
- `array.h` — NovaArray\_T макросы.
- `alloc.{c,h}` (включая alloc_rc.c, alloc_boehm.c) — Nova allocator API
  поверх соответствующего GC backend'а.
- `nova_rt.h` — агрегатор includes.

**Why:**
- Сторонний код — black box. Патчинг ломает upgrade-path (нельзя обновить
  версию minicoro, не вмержив наши правки заново).
- Чужие баги в патченном коде — наша ответственность за поддержку, тогда
  как вне патчинга — багрепорт upstream и ждать fix.
- Архитектурно правильнее: если что-то не умеет minicoro — это ограничение
  его API, и Nova должна работать с этим (или сменить backend).

**How to apply:**
- Любая runtime-фича Nova → пишется в нашем коде, использует public API
  библиотеки.
- Если public API недостаточно — фиксировать как ограничение в
  `simplifications.md`, не лезть в патчинг.
- При сомнении («можно ли это решить только через patch minicoro?») —
  остановиться и обсудить с пользователем.

## 5. Тесты — first-class artifacts

Тесты — спецификация поведения, не техдолг. Поэтому:

- Тесты на корректность параллельности должны **доказывать параллельность**
  через observable interleave (см. `nova_tests/concurrency/parallel_for.nv`), а не
  только через итоговую сумму.
- Тесты на ошибки должны проверять **detection-механизм**, а не просто что
  программа не упала.
- При смене спеки/семантики тесты переписываются под новую семантику, не
  обходятся.
