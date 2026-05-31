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

- **Destructuring patterns** в consume-binding (`consume (a, b) = pair`)
  — V1 поддерживает только simple ident pattern. → followup `[M-73.1-
  destructure]` if запрос.
- **Cross-module flow inference** — V1 conservative: external fn
  возврат-types помечены явно (D163 FFI consume); если нет — assumes
  non-consume. → followup `[M-73.1-cross-module-flow]`.

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

---
