# Memory — управление памятью

Решения этой группы определяют модель памяти Nova: как программист
взаимодействует с heap'ом, что делает компилятор, где живут циклы, и
как обеспечивается real-time производительность.

| # | Решение | Статус |
|---|---|---|
| [D6](#d6-память-managed-по-умолчанию-regions-opt-in-для-real-time) | Память: managed по умолчанию, regions opt-in для real-time | active |
| [D21](#d21-отменено-opt-in-cycle-collection) | Opt-in cycle collection | ⚠️ отменено, заменено D6 |

---

## D6. Память: managed по умолчанию, regions opt-in для real-time

### Что
Современный concurrent GC по умолчанию. Программист пишет код **не
думая о памяти** — циклы освобождаются автоматически, никаких
префиксов типов, никаких lifetime'ов. Real-time зоны (звук, торговля,
embedded) — через блок `realtime nogc { }` ([D64](04-effects.md#d64)),
GC внутри выключен. Явный `region { ... }` нужен для контроля над
аренами внутри realtime-блока.

### Правило

#### Два уровня памяти

```
┌─────────────────────────────────────────────────────────┐
│ Managed heap (default)                                  │
│   - Concurrent GC (паузы <1ms)                          │
│   - Generational, non-moving для FFI                    │
│   - Stable interior pointers (необходимо для D144 slice)│
│   - Escape analysis: что не утекает — на стеке/в арене  │
│   - Никаких префиксов в коде                            │
│   - Циклы освобождаются автоматически                   │
└─────────────────────────────────────────────────────────┘
              │
              │ opt-in для real-time
              ▼
┌─────────────────────────────────────────────────────────┐
│ realtime nogc { ... } блок (D64) + region { ... }       │
│   - GC выключен внутри блока                            │
│   - Аллокации в арену, освобождение en-masse на выходе  │
│   - Гарантированно нет GC pauses                        │
│   - Для звука, торговли, embedded                       │
└─────────────────────────────────────────────────────────┘
```

#### Базовое использование

```nova
type Tree {
    value int
    children []Tree         // обычная ссылка, GC управляет
    parent Tree              // циклы освобождаются автоматом
}

ro root = Tree { value: 1, children: [], parent: ... }
// освобождается автоматически когда становится недостижим
```

**Никаких `~T`, `~&T`, `~weak` префиксов.** Программист пишет логику,
GC делает остальное. **Никаких `&T` / `mut &T` borrow** — передача
объекта = передача указателя в managed heap, copy/move не нужны.

#### Real-time через блок `realtime { }` (D64)

> ⚠️ **REVISED → [D64](04-effects.md#d64).** Изначально D6 вводил
> эффект `Realtime` в системе типов с implicit-region обёрткой
> возвращаемого значения. После [D62](04-effects.md#d62)/[D64](04-effects.md#d64)
> Realtime — **runtime-блок**, не эффект. Гарантия не-GC-пауз
> даётся блоком `realtime nogc { }`, не сигнатурой функции.

Гарантия отсутствия GC pauses даётся блоком `realtime { body }`
(базовый, запрещает suspend) или `realtime nogc { body }` (жёсткий,
дополнительно запрещает аллокации в managed heap). Внутри
`realtime nogc` — только region-allocations и стек, см. [D64](04-effects.md#d64).

**Явный `region { ... }`** работает внутри `realtime nogc` для
arena-allocations:

```nova
fn map_audio(samples []f32, gain f32) -> []f32 =>
    realtime nogc {
        region {
            samples.map(|x| x * gain)
        }
    }

fn process_audio_block(samples []f32) -> []f32 {
    realtime nogc {
        ro scratch = region {
            ro buf = []f32.with_capacity(1024)
            // ... первая фаза, временные данные
            buf.to_owned()
        }
        region {
            // вторая фаза с другой ареной
            finalize(scratch)
        }
    }
}
```

Возвращаемое значение копируется в managed heap на границе
`realtime nogc { }` блока (компилятор делает сам через `to_owned()`).

`region { ... }` — примитив языка, как `parallel for`/`race`/`with_timeout`
([06-concurrency.md → D14](06-concurrency.md#d14)).

#### Escape analysis — фундамент производительности

Escape analysis делает большую часть perf-работы: значения, не
утекающие за пределы вызова, остаются **на стеке** или в **арене
вызова** — без аллокаций в managed heap, без GC pressure. Программист
пишет обычный код, компилятор сам решает.

Для случаев, где escape analysis не справляется (объект пересекает
границу fiber'а, сохраняется в долгоживущее место, возвращается из
функции), объект попадает в managed heap — **это нормально для 99%
случаев** для backend-кода.

#### Целевые характеристики GC

Конкретный движок — выбор реализации. Дизайн фиксирует **класс**:

- **Concurrent** (параллельно с приложением) — паузы <1ms p99.
- **Generational** (большинство объектов умирает молодыми).
- **Non-moving для FFI** или с pinning — указатели стабильны.
- **Throughput overhead** — целевой ~5-10% (как ZGC, Shenandoah).
- **Memory overhead** — целевой ~1.5x.

Кандидаты реализации: MMTk (фреймворк современных GC, используется
Java/Ruby/Julia), собственный concurrent collector, или адаптация
существующего. Выбор — на этапе реализации.

#### Эволюция реализации `region`

- **MVP (v0.5):** implicit region создаётся **всегда** для тела
  `realtime nogc { }` блока без явного `region { }`. Стоимость —
  одна арена на блок.
- **v0.7+:** escape analysis убирает арену там, где она не нужна
  (функция работает только на стеке).
- **v1.0+:** дальнейшие оптимизации — переиспользование арены
  вызывающего, стирание неиспользуемых регионов.

### Почему

#### Почему managed по умолчанию

1. **Целевая ниша Nova — backend + AI-кодинг**, не embedded/real-time.
   Kubernetes, Docker, etcd, Prometheus, CockroachDB на Go доказали,
   что современный GC **не мешает** инфраструктуре интернета.
2. **AI-first** ([D10](01-philosophy.md#d10)): LLM, читающая код, **не
   должна выбирать** `~T`/`~&T`/`~weak` для каждой структуры. Это
   трение, увеличивающее ошибки.
3. **Когнитивный налог** на программиста: ~80% случаев программист
   **не знает**, нужен ли real-time. Опт-ин по дефолту = угадывание.
4. **Прецедент антипаттерна**: Java/Swift/C++ сообщества жалуются на
   misuse weak-ссылок. Nova повторяла бы ту же ошибку.
5. **«Простота + огромные возможности»**: убрав префиксы памяти,
   упрощаем грамматику, освобождаем ментальный бюджет на
   effects/handlers/контракты.

#### Почему `&T` borrow отменён

В первоначальной версии после перехода на managed GC я предложил
оставить `&T` как «opt-in borrow для hot path». Пересмотрено по
аргументам:

1. **`&T` рефлекторно скопирован из Rust.** В Rust borrow нужен
   потому что нет GC. В Nova с GC передача объекта = передача
   указателя, никакого move/clone не требуется.
2. **Escape analysis закрывает большинство perf-кейсов.** Не утекающие
   значения остаются на стеке — это работает в Go, Java HotSpot, .NET.
3. **Slice уже передаётся эффективно.** `data []f64` — это
   `(ptr, len, cap)` структура, передача дешёвая. Не нужен отдельный
   `&[]T` borrow.
4. **Lifetime checker — research-уровень.** Стоит дорого реализовать,
   для прикладного языка с GC выгода низкая.
5. **Прецедент Go** — нет borrow, и язык успешно работает в backend-
   инфраструктуре.

Для real-time hot path остаётся `region { ... }` блок — **достаточный**
escape hatch.

### Что отвергнуто

- **Префиксы `~T`, `~&T`, `~weak`** — нет в языке.
- **`&T` / `mut &T` borrow** — нет в языке.
- **Cycle collector Bacon-Rajan** ([D21](#d21-отменено-opt-in-cycle-collection))
  — заменён на единый concurrent GC.
- **Эффект `Alloc[Cycle]`** — снят, аллокации в managed heap не
  отдельный эффект.
- **Compile-time анализ циклов через `~T`** — не нужен, GC справляется.
- **Тип `Weak[T]` в stdlib** — НЕ вводится. Use cases решаются иначе:
  - Кеш с auto-cleanup → `Cache[K, V]` с TTL/LRU из stdlib.
  - Observer pattern → handler-механизм Nova ([D10](01-philosophy.md#d10)).
  - GC-cycle оптимизация → не нужна для backend.

### Что сохранилось

- **Стек / escape analysis** — компилятор держит на стеке всё, что не
  утекает; для не-утекающих значений — без GC overhead.
- **Регионы** — явная opt-in фича через `region { }` для real-time.

### Цена

1. **Потеря дифференциации.** «Opt-in cycle collection» был кандидатом
   в третью уникальную заявку Nova ([D9](01-philosophy.md#d9)). Теперь
   Nova — «backend-язык с GC, как Go» — слабее, но честнее.
2. **Memory overhead ~1.5x** — цена GC.
3. **Tail-latency для p99.99** — современные concurrent GC дают pauses
   <1ms, для backend не проблема. Если столкнутся с GC pauses на
   high-load (как Discord Read States) — решается через `region` для
   критичных частей или профилирование allocation patterns.

### Связь

- [01-philosophy.md → D10](01-philosophy.md#d10) — обоснование AI-first
  обуславливает «без префиксов памяти».
- [04-effects.md → D64](04-effects.md#d64) — `realtime { }` как
  runtime-блок (заменяет эффект Realtime после D62).
- [06-concurrency.md → D14](06-concurrency.md#d14) — `region` рядом с
  `parallel for`, `race`, `with_timeout`.
- [09-tooling.md → D24](09-tooling.md#d24) — как и SMT-движок, конкретный
  GC-engine — выбор реализации, не дизайна.

### Эволюция

D6 в текущей форме **revised**. История:

1. **v0**: opt-in cycle collection, программист выбирает `~T`/`~&T`.
2. **v1**: пересмотрено — managed GC по умолчанию, regions opt-in.
   Старая версия → [D21](#d21-отменено-opt-in-cycle-collection).
3. **v2**: implicit region для тела Realtime-функций (через
   эффект `Realtime`), `&T` borrow окончательно отменён.
4. **v3 (текущая, после D62/D64):** `Realtime` как эффект отменён,
   гарантия не-GC-пауз даётся блоком `realtime nogc { }`. `region`
   используется внутри блока для arena-allocations.

Подробно — [history/evolution.md](history/evolution.md).

---

## D21. ОТМЕНЕНО — Opt-in cycle collection

> ⚠️ **ОТМЕНЕНО.** Заменено [D6](#d6-память-managed-по-умолчанию-regions-opt-in-для-real-time)
> (managed GC по умолчанию + regions opt-in).

### Что было

В ранней версии дизайна программист выбирал на уровне типа:
- `~T` — heap-аллокация без cycle collection (для acyclic-данных).
- `~&T` — heap с cycle collection.
- `~weak` — слабая ссылка для разрыва циклов.

Эффект `Alloc[Cycle]` помечал функции, использующие cycle collector.
Тип `Weak[T]` входил в stdlib.

### Почему отменено

См. раздел «Почему managed по умолчанию» в [D6](#d6-память-managed-по-умолчанию-regions-opt-in-для-real-time).
Кратко:

- **Когнитивная нагрузка** на программиста и LLM при выборе префикса
  для каждой структуры.
- **Backend-ниша** Nova не требует opt-in cycle control — современный
  concurrent GC справляется (Kubernetes, Docker, etc).
- **Прецеденты антипаттернов** — Java/Swift/C++ сообщества страдают от
  misuse weak-ссылок.

### Что переехало в D6

- **Регионы для real-time** — `region { ... }` блок остался, теперь как
  единственный механизм opt-in escape hatch.
- **Escape analysis** — стек для не-утекающих значений (входит в
  managed GC по умолчанию).

### Связь

- [D6](#d6-память-managed-по-умолчанию-regions-opt-in-для-real-time) —
  замещающее решение.
- [history/evolution.md](history/evolution.md) — детальная хронология
  пересмотра.

---

## D131. `consume` — квалификатор логической линейности

> **Plan 73.** Принято 2026-05-21.

### Что

Квалификатор `consume` на **receiver'е** метода или на **параметре**
функции. Помечает, что вызов **забирает значение целиком**: после
consume-вызова переменная-источник логически инвалидируется и больше
не может использоваться.

```nova
fn StringBuilder consume @into() -> str          // consuming receiver
fn drain(consume sb StringBuilder) -> str        // consuming параметр
```

Это **не** ownership в смысле Rust и **не** borrow checker. Памятью
по-прежнему управляет GC ([D6](#d6-память-managed-по-умолчанию-regions-opt-in-для-real-time));
`consume` проверяет **логический инвариант**: например, после
`sb.into()` буфер `StringBuilder` отдан в результирующий `str`,
поэтому дальнейшее использование `sb` — семантическая ошибка.

### Синтаксис

`consume` стоит на месте `mut` — между именем типа и `@` (receiver)
либо перед именем параметра:

```nova
fn Type consume @method(...) -> R       // receiver
fn f(consume name Type) -> R            // параметр
```

**Call-site неявный** — `sb.into()` / `f(sb)` без специального
синтаксиса (маркер `consume:` занят именованными аргументами с
дефолтами, [D102](03-syntax.md#d102-именованные-аргументы-и-значения-параметров-по-умолчанию)).

`consume` и `mut` на одном receiver — **взаимоисключающие** (parse
error): `consume` забирает значение целиком, `mut` мутирует его на
месте.

### Правило

Компилятор проводит **flow-sensitive** анализ. У каждой переменной —
состояние `VarState`:

- **`Live`** — значение доступно.
- **`Consumed`** — значение потреблено.
- **`MaybeConsumed`** — потреблено лишь на части путей выполнения.

Переходы:

- consume-вызов (`v.consume_method(...)` или `f(v)` в consume-позиции)
  переводит `v` в `Consumed`.
- Использование `v` в состоянии `Consumed` → **compile error**
  (use-after-consume).
- Использование `v` в состоянии `MaybeConsumed` → **compile error**
  (maybe-consumed: компилятор не гарантирует доступность).

**Слияние путей** (`if`/`match`/`??`/`select`): состояние объединяется
по-переменно — `(Live, Consumed) → MaybeConsumed`, `(Consumed,
Consumed) → Consumed`, `(Live, Live) → Live`.

**Циклы** (`for`/`while`/`loop`) — пессимистично: переменная,
потреблённая в теле, становится `MaybeConsumed` (на 2-й итерации
consume — уже use-after-consume).

`consume` на closure / handler / trailing-теле, которые исполняются
0+ раз, обрабатывается изолированно: use-after-consume внутри ловится,
но их собственные consume наружу не протекают.

### Runtime defense-in-depth

Compile-time проверка — основной механизм. В C-рантайме consume-методы
дополнительно зануляют внутреннее состояние (`StringBuilder.into()`
обнуляет `data`/`len`/`cap`); если статическая проверка обойдена,
следующий доступ fail-fast'ит через assert, а не молча портит данные.
Прежний runtime-флаг `consumed` удалён — его роль закрыта D131.

### Границы (bootstrap)

- ~~**Без alias-tracking.** `let a = b` создаёт независимо отслеживаемую
  переменную `a`; consume `a` не помечает `b` (false-negative,
  permissive — не выдаёт ложных ошибок).~~
  **→ amended by [D180](#d180-consume-binding-syntax-plan-731)** —
  `let a = consume_var` теперь запрещён в теле функции
  (E_VIEW_BINDING_FORBIDDEN); требуется явный `consume a = b`
  для move ownership ИЛИ передача в function-param для view-borrow.
- **Резолв consume-метода по типу receiver'а — best-effort.** Тип
  переменной выводится из аннотации / очевидного конструктора
  (`Type.new()`); если тип неизвестен, метод не трактуется как
  consuming (sound: false-negative, не false-positive).

### Связь

- [03-syntax.md → D30](03-syntax.md#d30) — `mut` как аналогичный
  receiver/param-квалификатор.
- `std/runtime/string_builder.nv` — `StringBuilder consume @into()`,
  первый потребитель D131.
- [02-types.md → D133](02-types.md#d133) — type-level `consume` (Plan
  100.1, proposed 2026-05-23) — расширение D131 с противоположной
  стороны: «инстансы обязаны быть consumed на каждом code-path'е».
  D131 = affine (≤1 раз; забыть OK); D133 = must-consume (≥1 раз;
  забыть → compile error). Foundation для Plan 100 family (D156-D166
  — generic propagation, borrow/view, defer/errdefer integration, FFI,
  cross-module, migration, IDE tooling).
- [D180](#d180-consume-binding-syntax-plan-731) — extension D131 на
  let-binding site (Plan 73.1, 2026-05-28).

---

## D180. `consume` binding syntax (Plan 73.1)

> **Plan 73.1.** Принято 2026-05-28. Расширяет [D131](#d131-consume--квалификатор-логической-линейности)
> с receiver/param на let-binding site.
>
> **Cross-ref Plan 114 D184 (2026-05-31):** `consume X = expr` теперь
> часть симметричной триады binding-statement keyword'ов `ro`/`mut`/
> `consume` — `consume` уже был standalone keyword без `let`-prefix'а;
> Plan 114 сделал другие две формы (`ro X = …` immutable, `mut X = …`
> mutable) тоже без `let`-prefix'а. См. [D184](03-syntax.md#d184).

### Что

`consume` квалификатор разрешён и **обязателен** на let-binding'е когда
RHS — consume-обязательный expression. Кроме того, **view-binding в теле
функции запрещён** — views существуют ТОЛЬКО как function params (D157
view-default carry-over).

```nova
type Token consume { val int }
fn Token.new(v int) -> Token => { val: v }

// ❌ ОШИБКА E_CONSUME_KEYWORD_MISSING
ro tok = Token.new(7)

// ✅ ownership-binding
consume tok = Token.new(7)
tok.release()                   // consume через метод D131
```

### Зачем

D131 ввёл `consume` keyword **только** на receivers/params:
```nova
fn StringBuilder consume @into() -> str       // receiver
fn drain(consume sb StringBuilder) -> str     // param
```

Но на let-binding consume-обязательство было **невидимо**:
```nova
ro sb = StringBuilder.new()    // ← неявно: sb имеет consume-obligation
sb.into()                        // ← consume happens silently
```

D180 закрывает 3 production-grade дыры:

1. **Невидимость ownership на binding-site.** Reviewer не видит на
   `let X = …` что переменная будет потреблена. Rust решает borrow
   checker + lifetimes; Nova lifetime-free → нужна syntactic visibility.

2. **Dangling view problem.** `let twin = sb; sb.into(); twin.append("…")`
   — `twin` после `sb.into()` указывает в "никуда". D131 не ловит этот
   pattern (см. «Границы — без alias-tracking»). D180 устраняет
   возможность by construction: alias-binding запрещён.

3. **Inconsistency с D157 view-default на params.** D157 говорит:
   «не-consume param = view-borrow». Что значит `let X = consume_var`
   в теле — move? alias? borrow без lifetime? D180 даёт чёткий ответ:
   запрещено; используй `consume X = sb` (move) или функцию (view).

### Синтаксис

`consume` стоит перед именем binding'а, после `let` опционально:

```nova
consume X = expr            // primary form
consume mut X = expr        // ❌ parse error (D131 §«взаимоисключающие»)
ro X = expr                // регулярный binding для не-consume RHS
```

**Type annotation** разрешён между pattern и `=`:
```nova
consume tok Token = Token.new(7)
```

**Destructuring patterns** — TBD (Plan 73.1 Ф.6 TODO; в V1 поддерживается
только simple ident pattern).

### Правило

**Rule 1 — `consume` keyword обязателен на binding consume-obligated RHS.**

`consume X = expr` требуется когда `expr` возвращает **consume-обязательный**
instance. Без keyword'а → `E_CONSUME_KEYWORD_MISSING`.

**Когда RHS считается consume-обязательным:**
- Constructor consume-type'а (D133): `Token.new(...)`, `File.open(...)`,
  any `Type.new(...)` где Type помечен `type Type consume { … }`
- Function returning consume-type: `fn open_file() -> File consume`
- Generic propagation per D156: `Option[T]` / `Result[T,E]` где T —
  consume-type → результат consume-обязательный
- Return-type consume-метода: TBD edge case

**Когда RHS НЕ consume-обязательный (regular `let`):**
- Primitive: `let n = 42`, `let s = "hi"`
- Regular record: `let p = Point { x: 1 }` (если Point не consume)
- Non-consume method: `let len = sb.len()` (len возвращает int)
- View-borrow результат внутри fn: не возникает (Rule 2)

**Rule 2 — view-binding в теле fn запрещён.**

`let X = consume_obligated_var` (alias-binding) → `E_VIEW_BINDING_FORBIDDEN`.

```nova
consume sb = StringBuilder.new()
sb.append("hi")

// ❌ E_VIEW_BINDING_FORBIDDEN
ro twin = sb

// ✅ move ownership
consume twin = sb
// sb теперь dead

// ✅ передать как view через function-param
fn read_len(view T) -> int => view.len()
ro n = read_len(sb)    // sb остаётся Live; read_len получает view-param
```

**Rationale safety:** views существуют ТОЛЬКО как function params. Param
lifetime ограничен временем вызова; owner переживает вызов by stack
semantics. Dangling view by construction **невозможен** — никакой
lifetime tracking не нужен (vs Rust).

**Rule 3 — `consume X = consume_var` = move.**

```nova
consume sb = StringBuilder.new()
consume sb2 = sb        // ← move: sb dead, sb2 owns
// sb.append("late")    // ❌ D131-use-after-consume
```

`consume`-binding исходного var в новый owner — explicit transfer.
Source становится `Consumed` после move (D131 VarState).

**Rule 4 — `consume mut X = expr` parse error (carry from D131).**

Сохраняется существующий reject ([parser/mod.rs:3311](../../compiler-codegen/src/parser/mod.rs#L3311)).

**Rule 5 — view как параметр сохраняет lifetime caller'а (D157 carry-over).**

```nova
fn read_len(view sb T) -> int => sb.len()

fn main() {
    consume sb = StringBuilder.new()
    ro n = read_len(sb)        // view-borrow на duration of call
    sb.append("more")            // OK — sb всё ещё Live (view returned)
    consume v = sb.into()        // OK — consume в конце
}
```

Safe by construction.

**Rule 6 — consume obligation в-scope check (carry from D131).**

`consume X = …` обязывает X быть consumed до конца scope'а. D131
flow-sensitive analysis применяется без изменений.

**Амендмент [D432](02-types.md#d432) (Plan 217, 2026-07-20):** Rule 6 НЕ
применяется, если тип `X` объявил эффект-чистый `@cleanup` — для таких
типов непотребление к концу скоупа не ошибка (компилятор авто-вставляет
`@cleanup(outcome)`, гибрид C). Rule 6 продолжает действовать БЕЗ
ИЗМЕНЕНИЙ для типов без `@cleanup` (строгая линейность).

### Error codes

| Код | Когда | Suggestion (machine-applicable) |
|---|---|---|
| `E_CONSUME_KEYWORD_MISSING` | `let X = consume-obligated-expr` | Insert `consume ` перед X |
| `E_VIEW_BINDING_FORBIDDEN` | `let X = consume_var` (alias) | Replace `let` с `consume` (move) ИЛИ перенести в function-param (view) |
| `W_CONSUME_KEYWORD_UNNECESSARY` | `consume X = non-consume-expr` | Delete `consume ` keyword |

Format Plan 50 D102 (header + code + span + note + suggestion).

### Type-checker (production-grade)

Не shortcut: flow analysis должна работать на all source kinds:
- Direct ctor (`Type.new(...)`)
- Fn return cross-fn (resolve return-type, check consume-status)
- Member-access consume-field
- Generic substitution (Option[T]/Result[T,E] D156 propagation)
- Branch joins (D131 VarState semantics carry over)

**Span precision** — error указывает на `let` keyword, не whole statement.

### Industry comparison

| Аспект | Nova D180 | Rust | Go | TypeScript | Swift |
|---|---|---|---|---|---|
| Explicit ownership на binding | ✅ `consume X = …` keyword | ⚠ implicit move (`let x = y`) | ❌ GC | ❌ GC | ⚠ implicit `consuming func` |
| View-binding в теле | ❌ запрещён by construction | ✅ `&x` + lifetime | n/a | n/a | ✅ borrow с lifetime |
| Lifetime аннотации требуются | ❌ нет | ✅ `<'a>` | n/a | n/a | частично implicit |
| Dangling view возможен | ❌ by construction | ⚠ требует borrow-checker | n/a | n/a | ⚠ Swift exclusivity |
| Visible move on assignment | ✅ keyword | ❌ silent | n/a | n/a | ⚠ implicit |

**Nova edge:** visible ownership transfer на каждом binding-site,
zero lifetime annotations, dangling-view-impossible by construction
(через restriction-based design вместо lifetime tracking).

### Связь

- [D131](#d131-consume--квалификатор-логической-линейности) — foundation
  (consume на receiver/param). D180 — extension на let-binding.
- [D133](02-types.md#d133) — type-level consume (`type T consume { … }`),
  source consume-обязательности.
- [D156](02-types.md#d156) — generic propagation Option/Result для
  consume-type'ов.
- [D157](#d157-implicit-view-default--closure-capture-analysis--match-consume)
  — view-default model; D180 cross-reference: views только как params.
- [D170](06-concurrency.md#d170-coordination-primitives--semaphore--barrier--countdownlatch--condvar-plan-1034)
  / [D174](06-concurrency.md#d174-sync-primitives-consume-integration-plan-1039)
  — Plan 103.9 consume guards (MutexGuard/ReadGuard/Permit/OnceGuard) —
  primary consumers D180 syntax в std/runtime/sync.nv.
- [Plan 50 D102](03-syntax.md#d102-именованные-аргументы-и-значения-параметров-по-умолчанию)
  — Diagnostic format для 3 error codes.

### Что отвергнуто

- **D131 amendment вместо нового D180.** Отвергнуто: новые правила
  (Rule 2 view-binding-forbidden, Rule 3 alias=move) — semantically
  distinct design decisions, не уточнения. Отдельный D-блок даёт
  чёткий historical record.
- **`view X = sb` keyword** для in-body view-binding. Отвергнуто: open
  problem dangling-view без lifetime tracking. Restriction-based
  design (views only as params) — простой safe выбор.
- **Lifetime annotations** (`<'a>` Rust-style). Отвергнуто: D157
  философия Nova — без lifetimes. Restriction в D180 — natural fit.
- **Auto-insert `consume` keyword при missing.** Отвергнуто: silent
  semantic change. Explicit error + machine-applicable suggestion даёт
  reviewable migration.

### Что отложено (honest defer)

- **Cross-module flow inference** — V1 conservative: external fn
  возврат-types помечены явно (D163 FFI consume); если нет — assumes
  non-consume. → followup `[M-73.1-cross-module-flow]`.

### Амендмент (consume-волна А, 2026-07-19) — Rule 2 подтверждена для pattern-bound значений; D156-пропагация enforced

> **Решение владельца 2026-07-18** (маркер
> `[M-d180-consume-propagation-match-payload-mut-rebind]`,
> `docs/plans/backlog-followups.md` §P1) — **вариант А: enforce по букве**.
> Кросс-ref: [D131](#d131-consume--квалификатор-логической-линейности),
> [D133](02-types.md#d133), [D156](02-types.md#d156),
> [D157-амендмент](#d157-implicit-view-default--closure-capture-analysis--match-consume),
> [D184](03-syntax.md#d184).

Найдена (и закрыта тем же слиянием) дыра: `mut stream = tcp` /
`ro stream = tcp`, где `tcp` — pattern-bound payload из
`match TcpStream.connect(...) { Ok(tcp) => … }`, компилировалось
вопреки Rule 2 (`E_VIEW_BINDING_FORBIDDEN`), потому что `tcp` никогда не
входил в `consume_obligations`/`var_types` с корректным типом — match/if-let
arm-биндинги регистрировались голым `ctx.declare(n, None)`.

**Уточнение — НЕ new rule.** Rule 2 её текст и код не менялись: как
только payload корректно входит в `consume_obligations` с известным
типом (см. [D157-амендмент](#d157-implicit-view-default--closure-capture-analysis--match-consume)
— `Ok(consume tcp)` / D156-пропагация через Option/Result), `mut stream
= tcp` детектируется СУЩЕСТВУЮЩИМ Rule 2 кодом БЕЗ единой строки
изменений в нём самом (`compiler-codegen/src/types/mod.rs`,
`consume_walk_stmt`/`Stmt::Let`, alias_obligated-проверка). Баг был
исключительно в upstream-пропагации (D157-амендмент), не в самом Rule 2
— зафиксировано здесь как явное историческое подтверждение (маркер
изначально держал это как отдельный, третий пункт разбора).

**Практический эффект:** `Ok(consume tcp)` payload-биндинг теперь имеет
идентичную дисциплину с `consume X = expr`: mut-capable без доп.
keyword'а (D180 Rule 4: `consume mut` избыточен — consume уже несёт
права мутации), must-consume-до-scope-exit (D133), alias запрещён без
явного `consume` (Rule 2, теперь фактически enforced и на этом пути),
double-consume → use-after (D131).

### Амендмент (2026-07-21) — match-tail passthrough как Rule 1 RHS; divergence-aware match join

> Находка владельца 2026-07-21 (чтение `examples/tls/echo_server.nv:42-45`),
> маркер `[M-mut-binding-accepts-must-consume]`,
> `docs/plans/backlog-followups.md`. Кросс-ref: [D131](#d131-consume--квалификатор-логической-линейности),
> [D133](02-types.md#d133), [D157-амендмент](#d157-implicit-view-default--closure-capture-analysis--match-consume)
> выше, [D184](03-syntax.md#d184).

**Дырка.** Канонический паттерн `Ok(consume l) => l` на match-arm'е (сам
payload-биндинг `l` — корректно consume-обязателен, D157-амендмент выше)
— НЕ гарантирует, что РЕЗУЛЬТАТ всего match-выражения тоже воспринимается
как consume-обязательный, когда этот результат тут же связывается новым
binding'ом:

```nova
// ❌ компилировался БЕЗ ошибки (пред-амендмент) — молчаливая дыра
mut lst = match TcpListener.bind(addr) {
    Ok(consume l)  => l
    Err(_) => panic("bind failed")
}
```

Корень — `infer_value_type` (D180 Rule 1's RHS-type inference,
`compiler-codegen/src/types/mod.rs`) распознавал только узкий набор форм
RHS (`Call` на конструктор/free-fn/метод с известным return-типом, `Ident`
alias, `RecordLit`, `Try`/`Bang`/`RefArg`/`Coalesce`-развёртку) — **но не
`ExprKind::Match`/`ExprKind::IfLet`**. Match-выражение как RHS целого
`let`/`mut`/`ro`-биндинга просто возвращало `None`, поэтому Rule 1
(`E_CONSUME_KEYWORD_MISSING`) не имел данных для срабатывания — молчал
даже когда каждая arm'а честно консьюмит свой payload через
`Ok(consume x) => x`. Это ОРТОГОНАЛЬНО [D157-амендменту](#d157-implicit-view-default--closure-capture-analysis--match-consume)
выше (тот закрыл alias arm-bound имени: `mut stream = tcp`, где `tcp` —
САМ payload pattern-биндинг); здесь же дыра на уровень дальше — результат
matcha, после того как arm уже корректно его вернул.

**Правило (нормативное, не new rule — расширение Rule 1's RHS-инференции).**
`infer_value_type` теперь рекурсирует в `Match`-arm'ы (`MatchArmBody::Expr`
tail либо `MatchArmBody::Block.trailing`) и в `IfLet`'s `then`/`else`
ветки, разрешая тип RHS через ПЕРВУЮ arm/ветку, чей tail сам resolve'ится
(best-effort — обычно единственная non-diverging arm, поскольку
well-typed match/if-let arm'ы согласуются в одном типе). Поскольку
`consume_walk_expr` над самим match'ем (который заполняет `var_types` для
arm-bound имён через `consume_declare_arm_pattern`) уже выполнен К МОМЕНТУ
вызова `infer_let_type` (см. `consume_walk_stmt`'s `Stmt::Let`: RHS
walk'ается ДО инференции типа), рекурсия в `Ident`-tail arm'ы (`Ok(consume
l) => l`) резолвится "бесплатно" через уже-существующий `Ident`-case.
Итог: `mut lst = match { Ok(consume l) => l, ... }` теперь корректно
триггерит `E_CONSUME_KEYWORD_MISSING` (Rule 1), требуя `consume lst = …`.

**Побочная находка (та же волна, тот же корень-класс дефектов) —
divergence-unaware match join.** Как только match-tail-passthrough
биндинги стали реально consume-obligated (выше), классический idiom
`Err(e) => { x.close(); return Err(e) }` начал ложно читаться как
`MaybeConsumed` на arm'ах, которые НЕ проходили через error-ветку.
Причина — `ExprKind::Match`'s state-join (`consume_walk_expr`,
`compiler-codegen/src/types/mod.rs`) сливал состояния ВСЕХ arm'ов через
`consume_join` безусловно, не проверяя, диverges ли arm (`return`/`throw`/
`panic(..)`/`exit(..)`/вложенный diverging if-или-match — существующие
`expr_diverges`/`block_diverges` helpers). Sibling-код `consume_walk_if`
УЖЕ имел divergence-aware merge (`then_diverges`/`else_diverges`,
исключающий diverging-ветку из join'а) — `Match` эту обработку никогда
не получал, чистое упущение симметрии, не самостоятельное решение.
Исправлено тем же слиянием: diverging arm пропускается из join'а
(`continue`), а ПЕРВАЯ non-diverging arm проходит через `consume_join`
(само-джойн с собой) вместо raw-присвоения — иначе arm-локальные
pattern-биндинги (`x` из `Ok(consume x) => x`) утекали бы в states-карту
ПОСЛЕ match'а (у `consume_join`'а есть задокументированное поведение
«ветка-локальные переменные отбрасываются» через домен ключей `saved`,
которое raw-присвоение обходило).

**Аудит + канонизация (та же волна).** Прогон нового Rule 1 по `std/**` +
`examples/**` (вне `_wip`) нашёл и канонизировал ~17 сайтов (`mut`/`ro X =
match { Ok(consume …) => … }` → `consume X = …`): `std/src/fs/fs.nv`
(`File.open`/`File.create`), `std/src/net/{byte_surface,
d302_neterror_iokind,split,write_all}_test.nv`,
`examples/{tls,net}/echo_server.nv`,
`examples/flagship/aggregator/{src/app/aggregate.nv,src/main.nv}`.
Каскад: где канонизация обнажала alias consume-обязательной переменной
(`mut st = stream` над уже-`consume stream`) — Rule 2 (`E_VIEW_BINDING_FORBIDDEN`)
срабатывал корректно; фикс — тот же `consume` на алиасе (Rule 3, move).
Отдельно — 3 сайта (`byte_surface_test.nv`'s `UdpSocket`,
`split_test.nv`'s `TcpListener`) обнажили ДОСРОЧНО-СУЩЕСТВУЮЩИЙ пробел
дисциплины в самих тестах: `panic(..)` — exit-point (D133 «Plan 100.1»),
все Live consume-obligations на panic-call должны быть закрыты; для типов
БЕЗ `@cleanup` (`UdpSocket`/`TcpListener`, в отличие от `TcpStream`/
`TlsStream`, D432) это не смягчается — тесты дозакрывали ресурс перед
panic-веткой явным `.close()`.

**Отдельно рассмотрено и НЕ сделано (см. цитату эмпирической проверки
ниже) — «убрать rebind + ручной `.close()`» для
`Ok(consume session) => { consume stream = session; …; stream.close() }`
идиомы в `examples/tls/net`.** Гипотеза (не подтвердилась): раз
`TcpStream`/`TlsStream` объявили `@cleanup` (D432), ручной `.close()` в
конце да и сам rebind — избыточны. Эмпирически (генерируемый C,
`compiler-codegen/src/codegen/emit_c.rs` `auto_cleanup_qualifies`)
подтверждено ДВОЯКО:
1. D432 §2 нормативно и буквально в реализации ограничивает авто-cleanup
   ТОЛЬКО именованным bare `consume X = e;` (`Stmt::Let`, `Pattern::Ident`)
   — arm-bound `Ok(consume stream) => { … }` БЕЗ явного rebind'а НЕ
   попадает в `auto_cleanup_qualifies`/`block_has_auto_cleanup_lets`
   вообще; убрать rebind → cleanup не вызывается НИГДЕ (проверено:
   сгенерированный C не содержит вызова `@cleanup` для такого биндинга).
   Rebind — LOAD-BEARING, не декоративный.
2. Даже сохранив rebind и убрав только ручной `.close()`, ветвление через
   `return`-до-конца-функции (не просто fallthrough нескольких match-arm'ов
   без `return`, как в реальных `echo_server`/`echo_client` файлах) в
   отдельном пробном репро вызвало runtime-fatal
   (`defer cleanup-fail with no outer handler:
   D188-on-exit-double-invocation`) — сигнал, что auto-cleanup под
   early-`return`-из-вложенного-match ещё не полностью надёжен на всех
   формах ветвления. Echo-файлы такого `return`-паттерна НЕ используют
   (только вложенные match/println без `return`), так что их текущий
   ручной `.close()` остаётся safe-by-construction и НЕ тронут этим
   слиянием — canonизация idiom'ы (drop rebind/close) отложена, честно,
   до отдельного разбора D432's early-return disarm-покрытия
   (`[M-d432-early-return-nested-match-disarm]`, вне периметра этой волны).

### Error codes (без изменений)

Коды та же таблица (`E_CONSUME_KEYWORD_MISSING`/`E_VIEW_BINDING_FORBIDDEN`/
`W_CONSUME_KEYWORD_UNNECESSARY`) — этот амендмент расширяет ТОЛЬКО
RHS-инференцию (какие expr-формы распознаются как consume-обязательные) и
join-механику match'а; новых кодов не вводит.

### Реализация

`infer_value_type` (`ExprKind::Match`/`ExprKind::IfLet` arms, best-effort
recursion в arm/branch tail), divergence-aware merge в `ExprKind::Match`'s
handler внутри `consume_walk_expr` (оба —
`compiler-codegen/src/types/mod.rs`). Фикстуры:
`spec_tests/conformance/neg/d180_match_tail_mut_binding_neg.nv` (RED,
`E_CONSUME_KEYWORD_MISSING`),
`spec_tests/conformance/d180_match_tail_consume_binding_ok.nv` (GREEN,
канонический фикс + divergence-join regression pin, оба test-блока
реально исполнены — не только type-check).

### Амендмент (№378, `[M-73.1-destructure]`, окно p378-consume-destructure,
2026-08-06) — destructuring patterns в consume-binding реализованы

Закрывает honest-defer пункт выше («Destructuring patterns … if запрос»);
запрос владельца поступил 2026-08-06 (мотивация — `TcpStream consume
@into_split() -> (TcpReadHalf, TcpWriteHalf)`, `std/src/net/tcp.nv`: пара
линейных значений, связать которую owned-биндингом было нечем — `ro (r,
w) = …` даёт ро-вью, ловится `E_CONSUME_BLOCK_NOT_OWNED` при попытке
re-consume).

**Обе формы теперь живые, СИММЕТРИЧНО `ro`/`mut`** — каждая по своему
канону (совпадает с амендментом 2026-08-05 «Конструирование и
деструктуризация именованного кортежа» под [D222](02-types.md#d222)):

| Форма | Семантика | На именованном кортеже/записи |
|---|---|---|
| `consume (a, b) = pair` | разбор ПО ПОЗИЦИИ, применима к анонимным (positional) кортежам | `E_NAMED_TUPLE_POSITIONAL_DESTRUCTURE` |
| `consume {a, b} = rec` | разбор ПО ИМЕНИ, применима к записям и именованным кортежам | единственная законная |

Частичный разбор фигурной формы — как у записей (D411): `consume {x, y,
..} = triple` требует явный `..`, если перечислены не все поля.

**Линейность — per-элемент, не паттерн целиком (D133).** Каждое имя,
которое `consume`-биндинг вводит через `Pattern::Tuple`/`Pattern::Record`,
получает СОБСТВЕННОЕ consume-обязательство: забытый элемент диагностируется
`D133-not-consumed` ПО ИМЕНИ этого элемента, не всего биндинга. Элемент
можно передать в consume-параметр функции (`pump(r, w)`) или re-consume
через блок-форму (`consume r { … }`) — оба места видят элемент как честный
owned-биндинг (`ctx.consume_obligations`), а не как ро-вью.

**Реализация — канал целиком существовал ДО этого окна, кроме одного
парсер-гейта:**

- Парсер: `parse_stmt_or_expr`'s `TokenKind::KwConsume` арм требовал
  lookahead `Ident`/`KwMut` после `consume` — `(`/`{` уходили в
  expression-парсер и давали «unexpected `consume` in expression».
  Единственная правка — расширить lookahead на `LParen`/`LBrace`;
  `parse_consume_decl_or_scope` уже вызывал общий `parse_pattern()`,
  который строит `Pattern::Tuple`/`Pattern::Record` для ЛЮБОГО
  вызывающего контекста (`ro`/`mut`/`consume` идентичны с этой точки).
- Чекер: `consume_walk_stmt`'s `Stmt::Let`-обработка уже итерирует ВСЕ
  имена паттерна (`consume_pattern_names`, tuple/record-агностична) и
  вызывает `declare_consume_binding` на каждое — механизм per-элементной
  линейности не писался заново, он уже обслуживал `ro`/`mut`-tuple/record
  destructure и просто заработал на `consume` в момент, когда парсер начал
  такие паттерны пропускать.
- Проверка «круглая форма на именованном кортеже — ошибка» переиспользует
  `check_positional_destructure_on_named_tuple` (№145,
  `E_NAMED_TUPLE_POSITIONAL_DESTRUCTURE`) без изменений — тот же код,
  что уже действует для `ro`/`mut`.
- Fiber-safety (`E_LINEAR_CAPTURE_IN_FIBER`, №364 precedent): capture-анализ
  уже читал `LetDecl.consume` как per-имени authoritative linear-сигнал
  (`linear_pattern: pat_consume || d.consume`, `types/mod.rs`) — написано
  ЗАРАНЕЕ, до того как парсер разрешил деструктуризацию, с явным
  комментарием, что это покрывает именно `consume (a, b) = expr`.

**НЕ покрыто этим окном (осознанно, за периметром):** мульти-var re-consume
блок для деструктурированных элементов (`spawn consume a, b { … }`,
`[M-consume-param-spawn-defer-active]`) — вложенная форма `spawn consume ar
{ consume bw { … } }` неработоспособна ПРИНЦИПИАЛЬНО (владение переносится
СИНХРОННО в момент `spawn`-statement'а, вложенный re-consume исполнялся бы
уже внутри ребёнка), не баг, не в объёме этого амендмента.

---

## D157. Implicit view default + closure capture analysis + `match consume`

> **Plan 100.3.** Принято 2026-05-23 (proposed; implementation pending).
> Финализированная модель (Ред. 2, 2026-05-24): **`view T` keyword
> отвергнут** — view это default mode без qualifier'а везде. D157
> формализует closure capture analysis + `match consume` syntax.

### Что

D133 финализирует общую модель «**view-default, consume-explicit**»
для param / for / match / if-let / binding. D157 покрывает:

1. **Closure capture analysis** — автоматическое определение consume-
   closure (FnOnce-equivalent) vs view-closure (FnMut/Fn analog).
2. **`match consume @expr`** explicit-consume pattern matching (D156
   collection-aware iteration sibling).
3. **`mut`-view rules** — что разрешено через `mut tx` qualifier
   (mut-методы + view-rules).

`view T` keyword **не существует** — попытка использовать = parse
error «view не keyword; use no-qualifier for view default».

### Зачем не Rust `&T` / explicit `view T` keyword

Initial design (Ред. 1) предполагал explicit `view T` keyword (Rust
`&T` analog). Финал (Ред. 2) — default-view везде, keyword избыточен.

Преимущества default-view:
- 🟢 Менее verbose — типичный case (read) без extra keyword'а.
- 🟢 Симметрично с D133 везде — «no qualifier = view, `consume` =
  transfer».
- 🟢 Меньше new syntax surface.
- 🟢 AI-first — explicit-ness только там где нужна (consume keyword).

Цена — нет explicit-marker для view (compensated через type-rule
mandatory `consume` keyword).

### `mut tx` qualifier — view + mut

Через `mut tx` qualifier (param / for / match / if-let / `let mut tx
= existing` alias) — добавляется разрешение на mut-методы:

```nova
fn print_id(tx Transaction) {                  // view (default)
    println(tx.id)                              // ✅ read
    tx.commit()                                 // ❌ consume через view
    tx.reopen()                                // ❌ mut через view
}

fn modify(mut tx Transaction) {                 // mut-view
    tx.reopen()                                // ✅ mut OK
    tx.commit()                                 // ❌ consume через mut-view
}

fn close(consume tx Transaction) {              // consume
    tx.commit()                                 // ✅
}
```

### Closure capture analysis

Closure-body анализируется как функция; capture-mode определяется
автоматически по operations:

| Operations в body над captured `tx` | Capture-mode | Аналог Rust |
|---|---|---|
| Только read fields, non-mut non-consume methods | **view-capture** | `Fn` |
| + mut methods | **mut-view-capture** | `FnMut` |
| consume-method или transfer | **consume-capture** | `FnOnce` |

```nova
consume tx = begin()

ro logger = || println(tx.id)                  // view-capture (только read)
logger()                                         // OK
logger()                                         // OK, multi-invoke
tx.commit()                                      // ✅ tx Live после

consume sb = StringBuilder.new()
ro appender = || sb.append("x")                // mut-view-capture
appender()                                       // OK
appender()                                       // OK, multi-invoke
sb.into()                                        // ✅ sb Live после mut-view

consume tx2 = begin()
ro commit_it = || tx2.commit()                 // consume-capture (FnOnce)
commit_it()                                      // ✅ tx2 Consumed, closure Consumed
commit_it()                                      // ❌ use-after-consume closure
tx2.commit()                                     // ❌ tx2 уже Consumed
```

### Closure escape rules

| Capture-mode | Escape (return / store) |
|---|---|
| view / mut-view | ❌ E (D157-borrow-escape-closure) — borrow не может outlive source |
| consume | ✅ — closure owns captured, может escape (becomes self-contained FnOnce) |

```nova
fn make_logger() -> ?? {
    consume tx = begin()
    ro f = || println(tx.id)                   // view-capture
    return f                                     // ❌ view-closure escape
}

fn make_committer() -> ?? {
    consume tx = begin()
    ro f = || tx.commit()                       // consume-capture (FnOnce)
    return f                                     // ✅ consume-closure owns tx; escape OK
}
```

Consume-closure escape allowed because closure carries ownership
(must be invoked exactly once anywhere it lives).

### `match consume @expr` для explicit-consume pattern

Default `match @expr` = view-match (D133): binding'и в arm'ах — view,
source Live после match. `match consume @expr` — explicit-consume:
binding'и в arm'ах carry ownership, source Consumed после match.

```nova
type Service consume { consume file Option[File] }

fn Service @file_id() -> Option[int] {
    match @file {                               // view-match (default)
        Some(f) => Some(f.fd),                  // f: view File
        None => None,
    }
    // @file Live ✅
}

fn Service consume @close_file() {
    match consume @file {                       // explicit-consume match
        Some(consume f) => f.close(),           // f: owns File, must consume in arm
        None => (),
    }
    // @file Consumed ✅
}
```

Симметрично D156 collection-aware: `for tx in vec` view, `for consume
tx in vec` consume. То же для `if let`:

```nova
if Some(t) = opt { println(t.id) }          // view, opt Live после
if ro consume Some(t) = opt { t.commit() }     // consume, opt Consumed после
```

### `mut`-borrow через `mut tx` qualifier (НЕ `&mut T`)

`mut`-view допускается в Nova (без Rust `&mut T` aliasing strictness),
потому что:
- Nova GC обрабатывает aliasing-memory-safety;
- D157 mut-view только about D131/D133 consume-invariant'ы (не data
  race protection — это Plan 47/49 concurrency layer);
- Single-thread (Plan 100 scope) multi-mut alias — sound;
- Multi-fiber concurrent mut через alias-class — addressed Plan 47
  supervised-cancel + Plan 49 cancel-routing (отдельный layer).

### Runtime cost

**Zero.** Default-view не вводит runtime overhead. Capture-mode
detection — compile-time через `check_consume` pass extension. Closure
representation — обычный `NovaClosBase` (Plan 56 D122).

### Сравнение

| Capability | Rust | TS | Kotlin | Nova D157 |
|---|---|---|---|---|
| Read-only borrow | ✅ `&T` (explicit) | ❌ | ❌ | ✅ **implicit view default** |
| Mutable borrow | ✅ `&mut T` (exclusive) | n/a | n/a | ✅ **`mut tx` (shared OK)** |
| Borrow в pattern matching | ✅ | n/a | n/a | ✅ **default view; `match consume` explicit** |
| Closure capture analysis | ✅ Fn/FnMut/FnOnce | ❌ | ❌ | ✅ **automatic view / mut-view / consume** |
| Lifetime annotations | ❌ требуются | n/a | n/a | ✅ **не требуются** (scope-only) |
| Borrow checker cognitive cost | ❌ высокий | n/a | n/a | ✅ **минимальный** (no keyword) |

Nova **превосходит Rust** на: (a) implicit view default — нет keyword;
(b) automatic closure capture-mode detection (Rust требует явный type-
annotation для closures); (c) нет lifetime annotations; (d) нет
borrow-checker exclusive-mut rules.

### Что отвергнуто

- **`view T` explicit keyword** — финал Ред. 2: default-view не нуждается
  в keyword'e.
- **`&T` Rust-style** — путает с raw pointer; D6 «no pointers».
- **Rust-style exclusive `&mut T`** — Nova GC справляется с aliasing-
  memory-safety; единственная concurrency-protection через Plan 47/49.
- **`let v = view tx`** (явный view-bind) — после dropping `view`
  keyword нет смысла. Alias через `let alias = tx` (default view-alias
  Plan 73).

### Амендмент (consume-волна А, 2026-07-19) — rvalue-скрутини pattern payload, `E_CONSUME_PATTERN_REQUIRED`

> **Решение владельца 2026-07-18** (маркер
> `[M-d180-consume-propagation-match-payload-mut-rebind]`,
> `docs/plans/backlog-followups.md` §P1) — **вариант А: enforce по букве**
> (философия D180 «visible ownership transfer на каждом binding-site»).
> Кросс-ref: [D131](#d131-consume--квалификатор-логической-линейности),
> [D133](02-types.md#d133), [D156](02-types.md#d156),
> [D180](#d180-consume-binding-syntax-plan-731), [D184](03-syntax.md#d184).

**Дырка (пред-амендмент).** D157 (выше) специфицировал только
**place-match** ownership — `match consume @expr` / `Some(consume f)` над
receiver-полем (`@file`). Владение pattern-биндингом payload при
**rvalue-скрутини** (`match TcpStream.connect(addr) { Ok(tcp) => … }`,
`if Some(x) = maybe_call() { … }`) нигде не было специфицировано, и
чекер (`ConsumeCtx`) не пропагировал consume-обязательство через
`Result[T,E]`/`Option[T]`-пейлоад ни для rvalue, ни фактически для
place — pattern-биндинги регистрировались без типа. Итог: `Ok(tcp)`
получал `tcp` живым, но НЕ consume-obligated → нижестоящий
`mut stream = tcp` тихо проходил мимо [D180](#d180-consume-binding-syntax-plan-731)
Rule 2, и double-close/use-after-close на этом пути не ловились
статически (исполнение оставалось корректным — реальный move, память
под GC — линейность лишь молчала).

**Правило (нормативное).** Payload-биндинг pattern'а `Ok(x)` / `Some(x)`
(single-arg tuple-variant над `Result[T,E]`/`Option[T]`, D156
generic-заразность) **обязан** нести явный `consume`-sub-pattern
(`Ok(consume x)` / `Some(consume x)`), когда `T` (Ok/Some-инвариант) —
must-consume тип (D133), **независимо** от того, rvalue скрутини
(`match f() { … }`) или place (именованная переменная типа
`Result[T,E]`/`Option[T]`, `match sess { … }`). Симметрично для
`if let`. Plain `Ok(x)` в этом случае — **ошибка**:

```nova
type TcpStream consume { fd i32 }
fn TcpStream consume @close() -> () { … }

fn TcpStream.connect(addr str) -> Result[TcpStream, IoErr]

// ❌ E_CONSUME_PATTERN_REQUIRED
match TcpStream.connect(addr) {
    Ok(tcp) => { tcp.close() },
    Err(e) => {},
}

// ✅ явный ownership transfer на pattern-site
match TcpStream.connect(addr) {
    Ok(consume tcp) => {
        tcp.close()                 // tcp — mut-capable (D180), must-consume (D133)
    },
    Err(e) => {},
}
```

`Ok(consume tcp)` / `Some(consume f)` вводят `tcp`/`f` в те же
`consume_obligations`, что и `consume X = expr` (D180): mut-capable
без доп. keyword'а (consume уже несёт права мутации — D180 Rule 4:
`consume mut` избыточен), must-be-consumed-до-scope-exit (D133),
alias без `consume` — [D180](#d180-consume-binding-syntax-plan-731)
Rule 2 `E_VIEW_BINDING_FORBIDDEN` (см. её амендмент ниже), double-consume
— use-after (D131).

**Для НЕ-must-consume пейлоада поведение НЕ меняется** — `Ok(x)`/`Some(x)`
остаётся legal view-default биндингом (пример: `Option[int]`), без
`consume`-keyword'а и без предупреждения.

**Amendment (Plan 216 tails, 2026-07-21) — Err-пейлоад + nested-tuple
payload.** Правило выше распространяется на Err-ветку и tuple-payload
симметрично (закрывает 2 из 3 bootstrap-honest-defer пунктов — см.
«Область» ниже):
- **Err-пейлоад.** `Err(x)` над `Result[T,E]`, где `E` (Err-инвариант) —
  must-consume тип, требует `consume`-sub-pattern (`Err(consume e)`) ТЕМ
  ЖЕ правилом, что Ok/Some для `T` — независимая ось (`T` и `E` могут быть
  обе, одна, или ни одна must-consume; правило проверяется на КАЖДОЙ
  match-arm независимо). `Option[T]` не участвует (нет Err-варианта).
- **Nested-tuple payload.** `Ok((a, b))` / `Some((a, b))` / `Err((a, b))` —
  single-arg tuple-variant, sub-pattern сам `(a, b)` (tuple-pattern) —
  КОГДА Ok/Some/Err-инвариант САМ tuple-тип `(A, B, …)`: каждый элемент
  проверяется НЕЗАВИСИМО тем же правилом (`A` must-consume без `consume`
  на `a` → ошибка; `B` не-must-consume → `b` legal plain).

**Amendment ([M-216-record-payload-consume], 2026-07-21) — record payload.**
Закрывает последний из трёх bootstrap-honest-defer пунктов (см. «Область»
ниже). `Ok({ a, b })` / `Some({ a, b })` / `Err({ a, b })` — single-arg
tuple-variant, sub-pattern сам record-паттерн (`{ .. }`) — КОГДА Ok/Some/Err-
инвариант САМ record-тип: каждое поле проверяется НЕЗАВИСИМО тем же
scalar-гейтом, что и tuple-payload элемент (`consume_require_pattern_binding`,
разделяемый helper): must-consume поле без `consume`-sub-pattern на его
binding'е (`{ a: consume x, b }` — explicit rename-форма; must-consume поле
в **shorthand**-форме `{ a }` — тоже ошибка, shorthand не может нести
`consume`) → `E_CONSUME_PATTERN_REQUIRED`; не-must-consume поле — legal
plain view-биндинг (shorthand и rename-форма обе). Per-field типы резолвятся
через новый `ConsumeRegistry::record_field_types` (record-тип → поле →
Named-тип-имя, `None`/absent — non-Named/nested-further поле, sound
false-negative) — companion `record_field_names`/`record_consume_fields`
(та же collect-логика, та же ТОЛЬКО-`module.items` область, без peer-file
merge). Глубже вложенное поле (не-`Ident` sub-pattern внутри record-поля) —
honest-defer для ЭТОГО поля (unchanged fallback).

**Синтаксис.** `consume` — новый sub-pattern qualifier на `Pattern::Ident`,
симметричный существующему `mut` (D36/Plan 108.3): взаимоисключающи на
одном биндинге (`consume mut x` / `mut consume x` — parse error
`E_PATTERN_CONSUME_MUT_CONFLICT`, зеркало D131 «consume и mut на одном
receiver'е»). Допустим ТОЛЬКО как элемент внутри single-arg
tuple-variant (`Ok(consume x)`); это ОРТОГОНАЛЬНО top-level `consume`
перед scrutinee (`match consume @expr`, разрешён; `if consume Pat = e`
— уже отдельно retracted, [D184](03-syntax.md#d184)
`E_CONSUME_IN_CONDITION`) — разные позиции грамматики, не конфликтуют.

**Error code.**

| Код | Когда | Suggestion (machine-applicable) |
|---|---|---|
| `E_CONSUME_PATTERN_REQUIRED` | `Ok(x)`/`Some(x)`/`Err(x)` payload (или per-element внутри `Ok((a,b))`/`Some((a,b))`/`Err((a,b))` tuple-payload, Plan 216 tails; или per-field внутри `Ok({a,b})`/`Some({a,b})`/`Err({a,b})` record-payload, [M-216-record-payload-consume]) — must-consume тип, sub-pattern без `consume` | Insert `consume ` перед именем биндинга |

Format Plan 50 D102 (header + code + span + note + suggestion), см.
[D102](03-syntax.md#d102-именованные-аргументы-и-значения-параметров-по-умолчанию).

**Область (bootstrap, honest defer).**
- ~~Только `Ok(..)`/`Some(..)` (успех-ветка) — Err-пейлоад НЕ покрыт~~ —
  **ЗАКРЫТО Plan 216 tails (2026-07-21).** `Err(x)` (`Result[T,E]`, `E`
  тоже must-consume — второй generic-аргумент) теперь симметричен Ok/Some:
  обязан нести `consume`-sub-pattern (`Err(consume e)`), та же
  `E_CONSUME_PATTERN_REQUIRED`-диагностика (текст `из \`Err(..)\`` вместо
  `Ok(..)`/`Some(..)`). Новые companion-карты `unwrapped_*_return_err_types`
  (`ConsumeRegistry`) / `var_unwrapped_err_types` (`ConsumeCtx`) +
  `infer_unwrapped_call_err_type` / `scrutinee_unwrapped_err_type` —
  зеркало Ok/Some-инфраструктуры, `Option` не участвует (нет Err-ветки).
  Closes `[M-73.2-err-payload-consume]`.
- Place-скрутини резолвится ТОЛЬКО для голого `Ident` (переменная с
  known `Result[T,E]`/`Option[T]`-аннотацией ИЛИ RHS с известным
  unwrapped-return-type). `@field`/`recv.field`-скрутини (Member) — не
  резолвится (нет field-type registry в `ConsumeCtx` для generic-типов
  сегодня) — sound false-negative. → followup `[M-73.2-field-scrutinee-unwrap]`
  (НЕ тронуто Plan 216 tails — отдельный, более узкий пробел).
- ~~Nested/record/tuple payload внутри `Ok(..)` — не покрыт~~ — **ЧАСТИЧНО
  ЗАКРЫТО Plan 216 tails (2026-07-21): tuple-форма.** `Ok((a, b))` /
  `Some((a, b))` / `Err((a, b))` (single-arg tuple-variant, sub-pattern —
  `Tuple` с известным per-element unwrapped-типом) — каждый элемент
  проходит ТОТ ЖЕ scalar-гейт независимо: must-consume элемент без
  `consume` → `E_CONSUME_PATTERN_REQUIRED` (текст `из \`Ok((..))\``/
  `Err((..))\``, отличает от scalar `Ok(..)`); не-must-consume элемент —
  unaffected view-биндинг. Arity-mismatch/unresolved shape — honest-defer
  fallback (unchanged). Новые companion-карты
  `unwrapped_*_return_tuple_types` / `unwrapped_*_return_err_tuple_types`
  (`ConsumeRegistry`) + `var_unwrapped_tuple_types` /
  `var_unwrapped_err_tuple_types` (`ConsumeCtx`) — per-element type names,
  `None` для non-Named/nested-further компонента (sound false-negative).
  **Record-форма — ЗАКРЫТО [M-216-record-payload-consume] (2026-07-21).**
  `Ok({ a, b })` / `Some({ a, b })` / `Err({ a, b })` (single-arg
  tuple-variant, sub-pattern — `Record` с известным per-field типом,
  резолвится через новый `ConsumeRegistry::record_field_types`) — каждое
  поле проходит ТОТ ЖЕ scalar-гейт независимо (shorthand `{ a }` И rename
  `{ a: x }` формы обе enforced; текст диагностики `из \`Ok({..})\``/
  `Err({..})\`` отличает от scalar/tuple вариантов). Глубже вложенное поле
  (не-`Ident` sub-pattern) — honest-defer для ЭТОГО поля (unchanged).
  Codegen-хвост, вскрывшийся при закрытии: `pattern_bind_typed`
  (`compiler-codegen/src/codegen/emit_c.rs`) не регистрировал plain
  struct-pointer inner-тип (`Nova_<Record>*`) в `var_types` на access-path
  ДО рекурсии в `Pattern::Record`-арм (только `_NovaTuple_`/`NovaOpt_`
  префиксы были покрыты, в mono-Result-ветке И в Option-`is_opt`-ветке) —
  рекурсивный `pattern_bind_typed` читал пустой `scr_ty`, `is_plain_record`
  ложно `false`, эмитил битый C (`->payload..a` двойная точка). Фикс —
  добавить ту же регистрацию для record-inner в обеих ветках (0
  существующих сайтов этого пути в кодовой базе на момент фикса — работа
  впрок, per followup-формулировку).

Реализация: `Pattern::Ident.is_consume` (`compiler-codegen/src/ast/mod.rs`),
`parse_pattern()` (`compiler-codegen/src/parser/mod.rs`),
`ConsumeCtx::var_unwrapped_types` / `scrutinee_unwrapped_type` /
`consume_declare_arm_pattern` (`compiler-codegen/src/types/mod.rs`).
Plan 216 tails (Err-payload + nested-tuple, 2026-07-21): `unwrap_result_err_name`
/ `unwrap_result_option_tuple_names` / `unwrap_result_err_tuple_names`,
`ConsumeCtx::scrutinee_unwrapped` (bundles ok/err/ok_tuple/err_tuple),
`consume_require_pattern_binding` (shared scalar-gate helper, both the
direct scalar arm AND per-tuple-element) — все в `compiler-codegen/src/types/mod.rs`.
[M-216-record-payload-consume] (2026-07-21): `ConsumeRegistry::record_field_types`
(companion of `record_field_names`/`record_consume_fields`, same
`module.items`-only collect scope) — record-type name → (field name → field's
Named-type single-segment name); `consume_declare_arm_pattern`'s
`Pattern::Record` arm reuses `scrut.ok`/`scrut.err` (already-resolved scalar
type name IS the record-type name when the Ok/Some/Err-inner T is itself a
record) as the lookup key into that map, then per-field
`consume_require_pattern_binding` (same shared helper, no new function) — all
in `compiler-codegen/src/types/mod.rs`. Codegen companion fix (record-inner
`var_types` registration gap) in `compiler-codegen/src/codegen/emit_c.rs`'s
`pattern_bind_typed` (mono-Result branch + Option `is_opt` branch).

**Amendment ([M-176-consume-through-result-match], 2026-07-24) — arm-exit
enforcement gap (checker soundness, no rule change).** Все амендменты выше
специфицировали, что `Ok(consume x)`/`Some(consume x)`/`Err(consume x)`
(и tuple-/record-payload варианты) вводят `x` в `consume_obligations`
must-be-consumed-до-scope-exit наравне с `consume X = expr` (D133) — но
чекер это правило НЕ проверял на самом arm/then-branch exit'е:
`consume_declare_arm_pattern` регистрирует обязательство ДО вызова
`consume_walk_block` для тела arm'а, поэтому блочный delta-scoped
exit-check (`consume_walk_block_inner`'s `obligations_before`, снятый
ПОСЛЕ declare) видел его как pre-existing OUTER-обязательство и пропускал,
рассчитывая на внешнюю проверку, которая никогда не наступала: пост-arm
join (`consume_join`, `Match`) отбрасывает state-ключи, отсутствовавшие в
pre-match `saved` (arm-локальные pattern-имена — ровно такие), запись
исчезает из `ctx.states`, но остаётся НАВСЕГДА в `ctx.consume_obligations`;
`check_obligations_at_exit` смотрит `None` (dropped by join) и трактует
это как `Consumed` — без диагностики. Итог: `Ok(consume x) => { /* x
никогда не закрыт */ }` молча проходило D133 — правило было specified,
но unenforced именно на этом pattern-site. Фикс — `check_and_clear_
arm_pattern_obligations` (`compiler-codegen/src/types/mod.rs`): exit-check
+ безусловная очистка (не гейтится на diverging arm — panic/return-path с
Live pattern-биндингом тоже D133, см. `consume_err_panic_path.nv`) arm'а
СВОЕГО pattern-обязательства на СВОЁМ exit'е, симметрично для `Match`
arms и `IfLet`'s `then`. Легитимный tail-passthrough (`Ok(consume l) => l`
без `{ }` — ownership transfer наружу, канонический `consume lst = match
{ .. }`) дозеркалил существующее bare-Ident-tail mark-consumed исключение
([M-mut-binding-accepts-must-consume]) на brace-less `MatchArmBody::Expr`
форму, которая раньше никогда не проходила через `consume_walk_block` и
потому не получала эту трактовку. Не новая грамматика/код ошибки — тот же
`D133-not-consumed`, просто теперь реально firing на pattern-bound
match-биндингах.

**Amendment ([M-consume-fn-value-call-arg-not-tracked], №55 221.1 registry,
2026-07-24) — call через первоклассное fn-значение.** Находка на реальном
коде (nova-http `ws/socket.nv`'s `@cleanup` workaround comment): передача
consume-обязательного значения через вызов, чей callee — первоклассное
fn-ЗНАЧЕНИЕ (`f(r)`, где статический тип `f` — голый `fn(T) -> U`), не
распознавалась чекером как consuming `r` — ложноположительный
`D133-not-consumed` на exit'е объемлющего scope'а (маскировался только для
типов с `@cleanup`-фолбэком, поскольку auto-cleanup-eligible типы не
ошибаются на scope-exit даже когда чекер (ошибочно) всё ещё считает их
Live). Корень — **не просто пробел чекера, а language-level дыра**:
грамматика `fn(T) -> U` в принципе НЕ несёт per-param `consume`-
квалификатор (`parse_fn_type_signature` парсит параметр как голый
`parse_type()`; иллюстративный `fn(consume T) -> U` из D156 HOF-раздела
выше по этому файлу — аспирационный design sketch, никогда не был проведён
в конкретный (non-generic) fn-type parser) — так что у чекера категорически
нет статической consume/view-сигнатуры для `h(ws)`-подобных вызовов, ЧЕМ
БЫ ни был реальный callee.

**Решение (checker-эвристика, БЕЗ новой грамматики).** Вместо расширения
грамматики (что потребовало бы parser + type-compat + ABI работы для
`fn(consume T) -> U` в конкретных, не-generic позициях — не сделано этим
фиксом, честно вне объёма) применена уже СУЩЕСТВУЮЩАЯ, задокументированная
выше в этом файле backward-compat политика для ИМЕННО этой неопределённости
("default = silent-ignore для generic-functions без bound", раздел выше):
когда callee вызова — голый `Ident`, который НЕ резолвится в
зарегистрированную top-level `fn` (т.е. её `consume_idxs` пуст) И не
consume-closure, но ЯВЛЯЕТСЯ известным ЛОКАЛЬНЫМ биндингом (параметр/`let`/
alias — `check_consume`'s param-loop `ctx.declare(&p.name, pty)`-ветка
регистрирует КАЖДЫЙ параметр, включая `fn(T) -> U`-типизированные, в
`ctx.states`/`ctx.var_types` независимо от типа) — bare-Ident
consume-обязательный аргумент трактуется как потреблённый этим вызовом.
Обоснование, почему это SOUND (не просто noise-suppression): codegen
УЖЕ единообразно передаёт consume-типизированное значение в ЛЮБОЙ call
(GC-backed, между "view"/"consume" передачей нет ABI-различия) — чекер
попросту не кредитовал это. Побочный эффект — реальное УЛУЧШЕНИЕ звучности:
до фикса `f(r); r.close()` (двойной close через fn-значение) молча
проходил (чекер думал `r` всё ещё Live после `f(r)`); после фикса второй
вызов корректно триггерит обычный `D131` use-after-consume (см.
`neg/consume_fn_value_call_arg_double_close_neg.nv`).

**Область (честный defer).** Это ЭВРИСТИКА, не полное решение: она НЕ
отличает "callee реально consume'ит этот параметр" от "callee лишь читает
его view-style" — ОБА трактуются как consumed (соответствует "default =
silent-ignore" секции выше — тот же trade-off, что уже принят для
generic HOF без `[T consume]` bound). Полное решение требует language-
уровневого расширения — `consume`-квалификатор в конкретных (non-generic)
`fn(...)` type-позициях (parser + type-compatibility между присваиваемым/
передаваемым closure/named-fn и объявленным fn-type + ABI-последствия) —
НЕ сделано этим фиксом, зафиксировано как followup
`[M-fn-type-consume-param-syntax]` (docs/plans/backlog-followups.md) для
отдельного design-цикла (новая грамматика ⇒ отдельный D-амендмент и
отдельное владельческое решение, не bundled в этот checker-фикс).

### Связь

- [D131](#d131) — affine consume foundation.
- [D133](02-types.md#d133) — type-level consume; D157 покрывает
  closure capture + match consume parts D133 model.
- [D156](02-types.md#d156) — generic + collection-aware iteration;
  D156 и D157 — sibling sub-plans для full D133 model.
- [D158](03-syntax.md#d158)-[D162](03-syntax.md#d162) — defer/errdefer
  family для cleanup-on-failure (Plan 100.4 family).
- [D75](06-concurrency.md#d75) — почему borrow-checker отвергнут.
- [D122](02-types.md#d122) — hybrid dispatch / NovaClosBase foundation.
- [D180 амендмент](#d180-consume-binding-syntax-plan-731) (consume-волна А) —
  Rule 2 подтверждена для pattern-bound consume-значений, введённых этим
  амендментом.

---
