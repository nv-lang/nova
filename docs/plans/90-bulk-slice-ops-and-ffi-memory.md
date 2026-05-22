# Plan 90 — bulk-операции над срезами + аудит FFI-памяти / `unsafe`

> **Статус:** 📋 proposed 2026-05-22 (production-grade), не начат
> **Приоритет:** P2 (enabler для self-hosting stdlib и высокопроизводительного
> байт-кода; текущий обход — поэлементный цикл или `external fn` в C)
> **Оценка:** ~2–2.5 dev-day (включая investigation-фазу Ф.0)
> **Зависимости:** D82 `external fn` ✅; D126 `external type` ✅;
> Plan 70.x (primitive distinction в array element types) ✅;
> Plan 74 (`to_bits`/`from_bits`) ✅
> **Источник:** обсуждение self-hosting 2026-05-22 — «нужны ли
> `memcpy`/`memmove`/`memcmp` и `unsafe`-режим, если в языке нет указателей».

## Зачем

Два связанных вопроса, поднятых при обсуждении self-hosting:

1. **Нет bulk-операций над `[]T`/`[]u8`.** Скопировать срез в срез —
   только поэлементный цикл либо `external fn` в C. Сравнить два `[]u8`
   лексикографически — нечем (есть лишь поэлементный `eq` → `bool`).
   Заполнить срез значением — снова цикл. Для stdlib-буферов
   (`WriteBuffer`/`ReadBuffer`), парсеров, сериализации, хешей и crypto
   это означает либо медленный код, либо уход в C.

2. **Гипотеза «нужен `unsafe` и сырые указатели».** Её надо **проверить
   с чистого листа**, а не принять на веру: язык с GC и без указателей
   (D6) не должен вводить сырые указатели без доказанной необходимости.

Plan 90 закрывает оба: даёт safe bulk-операции **и** проводит полный
нормативный аудит потребности в `unsafe`/сырых указателях — без
хэндвейва, с фиксацией решения в spec.

## Корректировка премиссы (важно — результат разведки)

**Self-hosting *компилятора* не требует ни `memcpy`, ни `unsafe`, ни
указателей.** Компилятор — трансформация деревьев: байты → AST
(sum-типы + record'ы + массивы) → type-check (деревья + `HashMap`) →
строки. Всё это в Nova уже есть, GC-managed. Прецедент: компилятор Go —
на safe Go; компиляторы OCaml / Java / C# / Haskell — в GC-языках без
сырых указателей в логике компилятора.

Где байтовые операции реально нужны:

| Где | Решение |
|---|---|
| Рантайм `nova_rt/*.c` (GC, аллокатор, `NovaArray`) | **остаётся C** — substrate, chicken-and-egg (так у всех: Go runtime, JVM) |
| stdlib-низкоуровневые типы (`WriteBuffer`/`ReadBuffer`), если их тела переписывать на Nova | **safe `[]u8` API** (этот план) |
| Высокопроизводительный Nova-код (парсеры, сериализация, crypto) | то же — safe bulk-операции |

Потребность — **не указатели, а bulk-операции на `[]u8`/`[]T`**.

## Сравнение с Go / Rust / TS

### (a) Bulk-операции над срезами

| Операция | Rust (safe) | Go (safe) | TS (safe) | Nova сейчас | Nova цель |
|---|---|---|---|---|---|
| copy / move | `copy_from_slice`, `copy_within` | `copy(dst,src)` builtin (overlap-safe) | `.set()`, `.copyWithin()` | ❌ | `copy_from`, `copy_within` |
| compare | `==`, `.cmp()` | `bytes.Equal`/`Compare` | loop / `Buffer.compare` | `eq` → `bool` только | `==` (есть `nova_array_eq`) + `compare` |
| fill (memset) | `.fill(v)` | `clear` / loop | `.fill(v)` | ❌ | `fill(v)` |

Go вообще **не имеет `unsafe`** для `copy()`. Rust `unsafe ptr::copy` —
только нишевый путь; обычный — safe-срезы. `memcpy` и `memmove`
схлопываются в один overlap-safe `copy_within` (как `copy()` в Go).

### (b) Escape hatch для «низкого уровня» / FFI

| Язык | Механизм |
|---|---|
| **Rust** | `unsafe`-блок + `*const T`/`*mut T` + `ptr::*`; FFI — `extern "C"` |
| **Go** | `unsafe.Pointer` + `unsafe.Sizeof/Slice`; FFI — cgo; `copy()` сам safe |
| **TS** | `ArrayBuffer`/`DataView`/typed arrays — managed, **сырых указателей нет вообще** |
| **Nova** | `external fn` (D82) + `external type` (D126) — сырой указатель живёт в C, в системе типов Nova **не появляется** |

Ключевое: TS — полноценный язык **без сырых указателей**. Go
`unsafe.Pointer` нужен в основном для cgo и reflection-трюков. У Nova
FFI-граница уже закрыта двумя механизмами: `external fn` (вызвать C) и
`external type` (держать C-handle непрозрачно). Раздел Ф.0.2 проверяет,
остаётся ли после них реальный gap.

## Scope

**Входит:**
- Safe bulk-операции `copy_from` / `copy_within` / `fill` / `compare`
  для `[]T` и `[]u8` (`==` через `nova_array_eq` уже есть).
- Spec D-block; реализация в рантайме C; codegen + registry-диспетчеризация.
- Позитивные и негативные тесты.
- **Investigation-фаза Ф.0.2** — нормативный аудит потребности в
  `unsafe`/сырых указателях, с фиксацией решения A/B в spec.

**Не входит:**
- Переписывание GC/аллокатора на Nova (остаётся C).
- Переписывание тел `WriteBuffer`/`ReadBuffer` на Nova (follow-up,
  разблокируется этим планом).
- Атомики / lock-free структуры на Nova.
- Ключевое слово `unsafe` — вводится **только если** Ф.0.3 = вариант B.

## Декомпозиция (фазы и шаги)

### Ф.0 — Аудит потребности (~0.5 д) — GATE

- **Ф.0.1** Инвентарь bulk-операций. Probe-фикстурами зафиксировать,
  каких операций не хватает для (a) тел stdlib-буферов на Nova,
  (b) типового hot-path кода (парсер байт, сериализация). Подтвердить
  минимальный набор: `copy_from`, `copy_within`, `fill`, `compare`.
- **Ф.0.2** **Аудит `unsafe`/FFI-ptr — без упрощений.** Для **каждого**
  гипотетического кейса сырого указателя — замаппить на существующий
  механизм либо зафиксировать реальный gap:
  1. C-функция возвращает `void*` / `FILE*` / opaque handle →
     `external type` (D126) — указатель спрятан в непрозрачном типе.
  2. Передача Nova-буфера в C, который туда пишет → `external fn` (D82)
     получает `NovaArray_u8*` (ptr+len) на C-стороне.
  3. Pointer arithmetic (обход буфера) → `[]u8` + индекс.
  4. Reinterpret-cast → `to_bits`/`from_bits` (D74) + `WriteBuffer`.
  5. Unchecked indexing ради perf → bounds-check elimination на стороне
     компилятора (модель Go), **не** `unsafe`-keyword.
  6. Ручной `malloc`/`free` в Nova → противоречит D6, не нужен.
  Сверка с Go `unsafe.Pointer` / Rust `unsafe` / TS (указателей нет).
- **Ф.0.3** **Decision point:**
  - **A (ожидаемо рекомендуется):** safe bulk-операции + `external fn` +
    `external type` покрывают всё; ключевое слово `unsafe` и сырые
    указатели **не вводятся**.
  - **B:** Ф.0.2 выявил реальный gap → спроектировать **минимальный**
    escape hatch (точечный, не общий `unsafe`-блок).
  Зафиксировать выбор и обоснование в «Итог Ф.0».
- **Ф.0.4** Контракт границ: OOB → `panic` или эффект `Fail`?
  Длина при mismatch — `panic` (модель Rust) или min-копирование
  (модель Go)? Какие `T` — numeric безусловно; `[]T` с GC-ссылками
  (копирование ссылок sound при non-moving GC, D6) — сразу или
  follow-up. Зафиксировать.

### Ф.1 — Spec (~0.3 д)

- **Ф.1.1** Новый D-block в `spec/decisions/08-runtime.md` (рядом с
  D117 size-accessors): bulk slice-операции для `[]T` — сигнатуры,
  семантика overlap (`copy_within` overlap-safe), контракт границ
  (из Ф.0.4), допустимые `T`.
- **Ф.1.2** Зафиксировать **нормативно** решение Ф.0.3 про
  `unsafe`/указатели: либо «escape hatch — `external fn` + `external
  type`, сырых указателей в языке нет» (вариант A), либо D-block на
  спроектированный минимальный hatch (вариант B).
- **Ф.1.3** При необходимости — аменд D82/D126, если Ф.0.2 уточнил
  контракт передачи буферов через FFI-границу.

### Ф.2 — Реализация в рантайме C (~0.4 д)

- `nova_array_copy_from` / `copy_within` / `fill` / `compare` в
  `compiler-codegen/nova_rt/array.h` — через `NOVA_ARRAY_IMPL`-macro
  (generic по `T`) + явные специализации для primitive `T`.
- Под капотом: `memcpy` (`copy_from`, distinct), `memmove`
  (`copy_within`, overlap-safe), `memset`/цикл (`fill`),
  `memcmp`+длина (`compare`).
- Bounds-check по контракту Ф.0.4.

### Ф.3 — Codegen + registry (~0.3 д)

- Завести методы в реестр (по образцу `runtime_registry.rs` либо
  прямой `prim_builtin_method`-dispatch для `[]T`).
- Mangling C-имён + Nova→C type-mapping для аргументов-срезов.

### Ф.4 — Тесты pos/neg (~0.4 д)

- **Ф.4.1** `copy_from` — distinct-срезы, разные `T` (`[]u8`, `[]int`).
- **Ф.4.2** `copy_within` — overlap forward и backward (ключевой —
  корректность `memmove`).
- **Ф.4.3** `fill` — заполнение, пустой срез.
- **Ф.4.4** `compare` — `Less`/`Equal`/`Greater`, срезы разной длины,
  префиксное отношение.
- **Ф.4.5** Негатив — OOB → детерминированная `panic` (или `Fail`);
  length mismatch по контракту Ф.0.4.
- **Ф.4.6** `[]u8` round-trip: `copy_from` → `compare` → `==`.
- **Ф.4.7** FFI-buffer probe, если Ф.0.2 уточнил контракт.
- **Ф.4.8** Полный `nova test` — 0 новых FAIL.

### Ф.5 — Spec sync + docs (~0.2 д)

- **Ф.5.1** `docs/plans/README.md` — статус Plan 90.
- **Ф.5.2** `docs/simplifications.md` — если аудит Ф.0.2 породил
  отложенные пункты (напр. bounds-check elimination) — занести маркеры.
- **Ф.5.3** `docs/project-creation.txt` + `nova-private/discussion-log.md`
  — записи.
- **Ф.5.4** Idiom-doc по bulk slice-операциям (краткий), если требуется.

## Итог Ф.0

> Заполняется по результатам аудита: таблица «кейс сырого указателя →
> существующий механизм / реальный gap», выбор варианта A/B по Ф.0.3 +
> обоснование, контракт границ Ф.0.4. До аудита раздел пуст.

## Acceptance criteria

- [ ] `copy_from` / `copy_within` / `fill` / `compare` работают для
      `[]u8` и `[]int`, проверены позитивно и негативно.
- [ ] `copy_within` корректен при overlap (forward и backward) —
      эквивалент `memmove`.
- [ ] OOB → детерминированная `panic` (или `Fail` — по контракту Ф.0.4).
- [ ] «Итог Ф.0» содержит **полный** аудит `unsafe`/FFI-ptr (6 кейсов)
      и нормативное решение A/B.
- [ ] Spec D-block опубликован; решение про `unsafe`/указатели
      зафиксировано нормативно.
- [ ] Полный `nova test` — 0 новых FAIL относительно baseline.

## Non-scope

- **GC / аллокатор на Nova** — остаётся C (substrate).
- **Тела `WriteBuffer`/`ReadBuffer` на Nova** — follow-up, этот план
  лишь даёт под него фундамент.
- **Атомики / lock-free** — не относится.
- **Общий `unsafe`-блок** — вводится исключительно если Ф.0.3 = B; по
  умолчанию Nova остаётся языком без сырых указателей (D6).

## Открытые вопросы для Ф.0

- Имена: `copy_from` vs `copy_from_slice`; `copy_within(src_from,
  dst_from, len)` — сигнатура.
- `compare` возвращает `int` (-1/0/1, модель Go) или sum
  `Ordering`/`Less|Equal|Greater` (есть протокол `Comparable`, Plan 85.4)?
- `[]T` для произвольного `T` сразу или сначала `[]u8` + numeric
  (`[]T` с GC-ссылками — копирование ссылок sound при non-moving GC)?
- Границы: `panic` (Rust) vs min-копирование (Go) при length mismatch.

## Связь

- [D6](../../spec/decisions/05-memory.md#d6) — managed GC, нет
  указателей/borrow; Plan 90 не нарушает.
- [D82](../../spec/decisions/08-runtime.md) — `external fn` (FFI).
- [D126](../../spec/decisions/03-syntax.md) — `external type` (opaque
  C-handle).
- [D117](../../spec/decisions/08-runtime.md) — size-accessors `[]T`;
  D-block Plan 90 встаёт рядом.
- [Plan 74](74-primitive-bitcast.md) — `to_bits`/`from_bits`.
- [Plan 01](01-roadmap-v0.1.md) — self-hosting (v2.0+); Plan 90 —
  один из enabler'ов stdlib-на-Nova.
- Ориентиры: Go `copy()`/`bytes`, Rust `slice::copy_*`, TS typed arrays.
