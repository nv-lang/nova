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

let root = Tree { value: 1, children: [], parent: ... }
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
        let scratch = region {
            let buf = []f32.with_capacity(1024)
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
дополнительно зануляют внутреннее состояние (`StringBuilder.@into()`
обнуляет `data`/`len`/`cap`); если статическая проверка обойдена,
следующий доступ fail-fast'ит через assert, а не молча портит данные.
Прежний runtime-флаг `consumed` удалён — его роль закрыта D131.

### Границы (bootstrap)

- **Без alias-tracking.** `let a = b` создаёт независимо отслеживаемую
  переменную `a`; consume `a` не помечает `b` (false-negative,
  permissive — не выдаёт ложных ошибок).
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

---

## D157. `view T` — read-only borrow без lifetime (scope-only)

> **Plan 100.3.** Принято 2026-05-23 (proposed; implementation pending).
> Closes deep-peek hole + closure capture analysis для consume-типов
> (Plan 100.1 [D133](02-types.md#d133)).

### Что

Квалификатор `view T` — **read-only borrow** на consume-typed value.
Источник остаётся `Live`; через `view`-binding только чтение полей и
не-mut не-consume методы. Borrow живёт в рамках лексического scope'а
(function body / pattern-arm / closure-body), **без lifetime'ов** —
sound через жёсткий запрет escape (return / store в record / capture-
and-return-closure).

```nova
fn print_id(tx view Transaction) {             // view-param — read-only borrow
    println(tx.id)                              // ✅ чтение поля
    tx.commit()                                 // ❌ consume через view — error
}

consume tx = begin()
print_id(view tx)                              // ✅ передача view; tx остаётся Live
tx.commit()                                    // ✅ tx Live после
```

Закрывает 2 hole'а D133 bootstrap'а:

1. **Deep peek в `Option[ConsumeType]`** — невозможен в D133 (`Some(f)`
   arm даёт Live linear `f`, обязан consumed). `match view @file`
   peek'ает inside Option без consume.
2. **Closure capture** consume-var — D133 bootstrap-permissive
   (closure считается «может consume», silent escape возможен). D157
   вводит explicit `view`/`consume`-capture taxonomy (FnMut/FnOnce
   analog).

### Зачем не Rust `&T`

`&T` (Rust-style) — отвергнуто:
- Путает с «raw pointer» из C (D6 — no pointers in user-facing).
- Требует lifetime annotations (`&'a T`) для escape-tracking — это
  borrow-checker, отвергнутый в D75.

`view T` — distinct keyword, **scope-only** semantics, без lifetimes.
Цена — borrow не может пересечь scope-boundary (return / store в struct).
Для resource-management use-cases (peek в Option, read-helper, closure-
callback) — достаточно.

### Правила доступа через view

| Действие | `view T` | regular `T` (D7 read-only) |
|---|---|---|
| `t.field` (read) | ✅ | ✅ |
| `t.regular_method()` (no mut/consume) | ✅ | ✅ |
| `t.@mut_method()` | ❌ E (D157-mut-via-view) | ❌ (D7) |
| `t.@consume_method()` | ❌ E (D157-consume-via-view) | ❌ (D7) |
| передача в `view`-param другой fn | ✅ | ✅ |
| передача в `consume`-param | ❌ | ❌ (D7) |
| store в поле | ❌ E (D157-view-escape-store) | ❌ (D7) |
| return | ❌ E (D157-view-escape-return) | ❌ (D7) |
| capture в closure, returned | ❌ E (D157-view-escape-closure) | ❌ (D7) |
| capture в closure, invoked в-scope | ✅ | ✅ |

`view` **строже** D7 read-only-param: запрещает даже mut-методы
(которые D7 разрешает). Логика: view — borrow на source, source может
быть consume-typed (Live), mut изменил бы invariant'ы consume-
обязательства.

### View propagation через scope

`view`-binding живёт в scope'е binding-site'а (function/closure/arm).
Source `Live` пока scope view'а активен; на scope exit — source
свободен (scope-mechanic, без runtime).

```nova
fn outer() {
    consume tx = begin()
    print_info(view tx)                         // ✅ view-передача
    tx.commit()                                 // ✅ tx Live после
}                                               // scope-exit OK
```

**Bootstrap:** `view` НЕ может bind'иться к локальной переменной
(`let v = view tx`). Live только в expression / pattern-arm / closure-
body. Это accepts мелкое ограничение ради zero-lifetime overhead.

(Future расширение: `let v = view tx` с scope-check — отложено в
`docs/plans/100.3-borrow-and-view.md` Ф.0 GATE.)

### `match view` для deep peek

```nova
type Service consume { consume file Option[File] }

fn Service @file_id() -> Option[int] {
    match view @file {                          // ← view-match
        Some(f) => Some(f.fd),                  // f: view File, read-only
        None => None,
    }
}
```

`match view <expr>` — destructive по форме, но binding'и в arm'ах —
**view, не linear**. После match — `@file` остаётся Live (не Consumed).

Это **закрывает deep-peek hole** D133 bootstrap'а.

### Closure capture analysis

Closure-body анализируется как функция:
- **consume операции** над captured `tx` → closure требует captured
  как `consume` → становится **consume-closure** (FnOnce-equivalent):
  invoke = consume; повторный invoke → error.
- **view операции** (read-only) → captured как `view` → **view-closure**
  (FnMut/Fn analog): multiple invokes OK.

```nova
fn outer() {
    consume tx = begin()
    let f = || println(tx.id)                   // view-capture, view-closure
    f()                                           // OK
    f()                                           // OK, multiple invokes
    tx.commit()                                   // ✅ tx Live после view-closure scope
}

fn outer2() {
    consume tx = begin()
    let commit_it = || tx.commit()              // consume-capture, consume-closure
    commit_it()                                  // ✅ tx Consumed, closure Consumed
    commit_it()                                  // ❌ use-after-consume на closure
}
```

**Escape detection:** closure capturing `view` не может escape scope'а
source'а (return / store) — error E (D157-view-escape-closure).

**Consume-closure (FnOnce)**: invoke ровно один раз; если не вызван —
closure Live на scope-exit, обязан consumed (invoked / passed). Иначе
error. Закрывает D133 closure-permissive hole.

### `mut`-borrow НЕ вводим

Соблазн добавить `mut view T` (Rust `&mut T`). Отвергнуто:
- `mut`-методы D131 + field-aware flow D133 D5 покрывают большинство
  use-case'ов.
- `mut view` усложнил бы aliasing rules (exclusive vs shared) — это
  borrow-checker, который мы избегаем.
- Если нужна mutable-borrow семантика — выражается через обычный
  mut-method на consume-record'е (D133 D5).

### Runtime cost

**Zero.** `view` — type-level only marker. Codegen эмитит обычный
pointer/value. Все проверки compile-time через `check_consume` pass
extension.

### Сравнение

| Capability | Rust | TS | Kotlin | Nova D157 |
|---|---|---|---|---|
| Read-only borrow | ✅ `&T` | ❌ | ❌ | ✅ **`view T`** |
| Mutable borrow | ✅ `&mut T` | n/a | n/a | ❌ **не вводим** |
| Borrow в pattern matching | ✅ | n/a | n/a | ✅ **`match view`** |
| Borrow в closure | ✅ | n/a | n/a | ✅ **`view`-capture** |
| Lifetime annotations | ❌ требуются | n/a | n/a | ✅ **не требуются** (scope-only) |
| Borrow checker cognitive cost | ❌ высокий | n/a | n/a | ✅ **низкий** (scope-only) |
| Compile-time soundness | ✅ | n/a | n/a | ✅ |

Nova **превосходит Rust** на одной оси — отсутствие lifetime annotations.

### Что отвергнуто

- **`&T` Rust-style** — путает с raw pointer; D6 «no pointers in
  user-facing».
- **`ref T` Kotlin/C++-style** — `ref` не keyword в Nova; inconsistent.
- **`borrow T`** — длиннее `view`.
- **Mutable borrow (`mut view T`)** — borrow-checker territory, D75
  policy.
- **`let v = view tx`** в bootstrap — отложено (scope-check
  complexity). Future plan.

### Связь

- [D131](#d131) — affine consume foundation.
- [D133](02-types.md#d133) — type-level consume; D157 — read-only access.
- [D156](02-types.md#d156) — generic `[T consume]` bound; `filter`-style
  HOF использует view.
- [D75](06-concurrency.md#d75) — почему borrow-checker отвергнут.

---
