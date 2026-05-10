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
