# Plan 90 — примитивы доступа к памяти (`byte_at`, bulk slice-операции) + аудит FFI / `unsafe`

> **Статус:** 📋 proposed 2026-05-22 (production-grade), не начат
> **Приоритет:** P2 (enabler для миграции рантайма/stdlib на Nova и
> высокопроизводительного байт-кода; текущий обход — поэлементный цикл,
> лишние аллокации или `external fn` в C)
> **Оценка:** ~2–2.5 dev-day (включая investigation-фазу Ф.0)
> **Зависимости:** D82 `external fn` ✅; D126 `external type` ✅;
> Plan 70.x (primitive distinction в array element types) ✅;
> Plan 74 (`to_bits`/`from_bits`) ✅
> **Источник:** обсуждение self-hosting 2026-05-22 — «как переписать
> рантайм C на `.nv` без лишних аллокаций; нужны ли `memcpy`/`memmove`/
> `memcmp` и `unsafe`-режим, если в языке нет указателей».

## Зачем

Чтобы переписать рантайм/stdlib-алгоритмы с C на Nova, нужны
**примитивы доступа к памяти**, которых сейчас нет. Конкретный триггер —
str-методы (`starts_with`, `ends_with`, `eq`, `find`, …): их **алгоритм**
тривиально выражается на Nova, но без примитива доступа к байтам
единственный путь — `slice`/`bytes` (**лишние аллокации**) либо
`external fn` в C. Аналогично `WriteBuffer`/`ReadBuffer`, парсеры,
сериализация, хеши, crypto.

Второй вопрос — гипотеза «нужен `unsafe`-режим и сырые указатели». Её
надо **проверить с чистого листа**: язык с GC и без указателей (D6) не
должен вводить сырые указатели без доказанной необходимости.

Plan 90 закрывает оба: даёт минимальный набор **safe** примитивов
доступа к памяти **и** проводит нормативный аудит потребности в
`unsafe`/сырых указателях.

## Корректировка премиссы (результат разведки)

**Self-hosting *компилятора* не требует ни `memcpy`, ни `unsafe`, ни
указателей.** Компилятор — трансформация деревьев: байты → AST → строки;
всё это в Nova уже есть, GC-managed. Прецедент: компилятор Go — на safe
Go; OCaml / Java / C# / Haskell — в GC-языках без сырых указателей.

Где байтовые операции реально нужны:

| Где | Решение |
|---|---|
| Рантайм `nova_rt/*.c` (GC, аллокатор, `NovaArray`) | **остаётся C** — substrate, chicken-and-egg (так у всех: Go runtime, JVM) |
| stdlib-алгоритмы (str-методы, `WriteBuffer`/`ReadBuffer`) на Nova | **safe-примитивы доступа к памяти** (этот план) |
| Высокопроизводительный Nova-код (парсеры, сериализация, crypto) | то же |

Потребность — **не указатели, а примитивы доступа к байтам/срезам**,
выраженные безопасно.

## Три неустранимых примитива

«Максимально на Nova» означает: **алгоритм** — на Nova, в C остаётся
лишь неустранимый минимум. Этот минимум — три примитива:

1. **`fn str @byte_at(i int) -> u8`** — O(1) чтение одного байта.
   В C — одна строка (`return (u8)(unsigned char)s.ptr[i];`),
   bounds-checked `panic`. **Обязан быть inline** — иначе вызов на
   каждый байт убивает производительность. Для **последовательных /
   data-dependent** алгоритмов: лексер, `find`, `contains`, `trim` —
   там сравнение по своей природе побайтовое, и word/SIMD не помог бы.
2. **`compare` (memcmp-класс) — ОДИН примитив.** C-функция возвращает
   порядок (`int` `<0`/`0`/`>0`). **Равенство — частный случай:**
   `equal(a,b) ≡ compare(a,b) == 0`. Из одного примитива выводятся и
   `==`, и `lt`/`le`/`gt`/`ge`. Отдельного «`bytes_equal`» примитива
   **нет** — это zero-case `compare`. Под капотом — `memcmp`
   (word-at-a-time + SIMD). Для **bulk-сравнения**: `starts_with`,
   `ends_with`, `eq`, лексикографический порядок.
3. **`copy_from` / `copy_within` / `fill`** — bulk copy/set для
   `[]T`/`[]u8` (safe-эквиваленты `memcpy`/`memmove`/`memset`).
   `copy_within` overlap-safe (= `memmove`).

Какой str-метод на каком примитиве:

| Метод(ы) | Примитив | Почему |
|---|---|---|
| `starts_with`, `ends_with`, `eq`, `lt/le/gt/ge` | `compare` | bulk-сравнение → word/SIMD-скорость |
| `find`, `contains`, `trim`, лексер | `byte_at` | data-dependent побайтовый скан; memcmp не применим, потери скорости нет |

⚠️ **Не делать bulk-сравнение через цикл по `byte_at`:** ранний `return`
при несовпадении делает цикл невекторизуемым → на длинных входах он в
разы медленнее `memcmp`. Bulk-сравнение — только через `compare`.

## Сравнение с Go / Rust / TS

### (a) Доступ к памяти / срезам

| Операция | Rust (safe) | Go (safe) | TS (safe) | Nova сейчас | Nova цель |
|---|---|---|---|---|---|
| байт по индексу | `s.as_bytes()[i]` | `s[i]` | `s.charCodeAt`/`buf[i]` | `str` — нет (только `char_at`, codepoint, O(i)) | `byte_at` O(1) |
| copy / move | `copy_from_slice`, `copy_within` | `copy(dst,src)` (overlap-safe) | `.set()`, `.copyWithin()` | ❌ | `copy_from`, `copy_within` |
| compare / equal | `==`, `.cmp()` (оба → memcmp) | `bytes.Equal`/`Compare` | loop / `Buffer.compare` | `eq`→`bool` (поэлементный C-цикл, не memcmp) | один `compare` (memcmp); `==` = zero-case |
| fill (memset) | `.fill(v)` | `clear` / loop | `.fill(v)` | ❌ | `fill(v)` |

Go вообще **не имеет `unsafe`** для `copy()`. Rust `[u8]::eq` и `::cmp`
оба сводятся к `memcmp`. `memcpy` и `memmove` схлопываются в один
overlap-safe `copy_within`.

### (b) Escape hatch для «низкого уровня» / FFI

| Язык | Механизм |
|---|---|
| **Rust** | `unsafe`-блок + `*const T`/`*mut T` + `ptr::*`; FFI — `extern "C"` |
| **Go** | `unsafe.Pointer` + `unsafe.Sizeof/Slice`; FFI — cgo; `copy()` сам safe |
| **TS** | `ArrayBuffer`/`DataView`/typed arrays — **сырых указателей нет вообще** |
| **Nova** | `external fn` (D82) + `external type` (D126) — сырой указатель живёт в C, в системе типов Nova **не появляется** |

TS — полноценный язык **без сырых указателей**. У Nova FFI-граница уже
закрыта `external fn` + `external type`. Ф.0.2 проверяет, остаётся ли
после них реальный gap.

## Scope

**Входит:**
- `str @byte_at` — O(1) доступ к байту.
- `compare` — **один** memcmp-класс примитив для `[]u8` и `str`
  (ranged); `==`/`eq` — его zero-case (существующий `nova_array_eq`
  переключается на этот примитив).
- `copy_from` / `copy_within` / `fill` для `[]T`/`[]u8`.
- Spec D-block; реализация в рантайме C; codegen + registry-диспетчеризация.
- Позитивные и негативные тесты.
- **Investigation-фаза Ф.0.2** — нормативный аудит `unsafe`/сырых
  указателей, решение A/B в spec.

**Не входит:**
- Переписывание GC/аллокатора на Nova (остаётся C).
- Переписывание **тел** str-методов и `WriteBuffer`/`ReadBuffer` на Nova
  (follow-up — разблокируется этим планом).
- Атомики / lock-free структуры на Nova.
- Ключевое слово `unsafe` — вводится **только если** Ф.0.3 = вариант B.

## Декомпозиция (фазы и шаги)

### Ф.0 — Аудит потребности (~0.5 д) — GATE

- **Ф.0.1** Инвентарь примитивов. Probe-фикстурами подтвердить
  минимальный набор: `byte_at`, `compare` (один, equality = zero-case),
  `copy_from`, `copy_within`, `fill`. Зафиксировать, что bulk-сравнение
  идёт через `compare`, а не через цикл по `byte_at`.
- **Ф.0.2** **Аудит `unsafe`/FFI-ptr — без упрощений.** Для **каждого**
  гипотетического кейса сырого указателя — маппинг на существующий
  механизм либо фиксация реального gap:
  1. C-функция возвращает `void*` / `FILE*` / opaque handle →
     `external type` (D126).
  2. Передача Nova-буфера в C, пишущий в него → `external fn` (D82)
     получает `NovaArray_u8*` (ptr+len).
  3. Pointer arithmetic → `[]u8`/`str` + индекс/`byte_at`.
  4. Reinterpret-cast → `to_bits`/`from_bits` (D74) + `WriteBuffer`.
  5. Unchecked indexing ради perf → bounds-check elimination на стороне
     компилятора (модель Go), **не** `unsafe`-keyword.
  6. Ручной `malloc`/`free` → противоречит D6, не нужен.
  Сверка с Go `unsafe.Pointer` / Rust `unsafe` / TS (указателей нет).
- **Ф.0.3** **Decision point:** A (safe-only: примитивы + `external
  fn` + `external type` достаточно, `unsafe`/указатели не вводятся) /
  B (выявлен реальный gap → минимальный точечный escape hatch).
  Зафиксировать в «Итог Ф.0».
- **Ф.0.4** Контракт: OOB → `panic` или эффект `Fail`? `byte_at` —
  bounds-checked + inline-требование. `compare` возвращает `int`
  (-1/0/1) или sum `Less|Equal|Greater` (есть протокол `Comparable`,
  Plan 85.4)? Длина при mismatch у `copy_*` — `panic` (Rust) или
  min-копирование (Go)?

### Ф.1 — Spec (~0.3 д)

- **Ф.1.1** D-block в `spec/decisions/08-runtime.md` (рядом с D117):
  `str @byte_at`; bulk slice-операции `[]T`; **один** примитив
  `compare` с явной фиксацией «equality = zero-case». Семантика
  overlap, границы, допустимые `T`.
- **Ф.1.2** Нормативно зафиксировать решение Ф.0.3 про
  `unsafe`/указатели.
- **Ф.1.3** При необходимости — аменд D82/D126 (контракт передачи
  буферов через FFI).

### Ф.2 — Реализация в рантайме C (~0.4 д)

- `str @byte_at` — `static inline`, одна строка, bounds-checked.
- **Один** `nova_bytes_cmp(a, b, len) -> int` (memcmp-класс). На нём:
  `compare` (знак), `==`/`eq` (`== 0`), `lt/le/gt/ge` (знак).
  Существующий `nova_array_eq` для `[]u8` переключается на него.
- `nova_array_copy_from` / `copy_within` / `fill` в
  `compiler-codegen/nova_rt/array.h` (`memcpy`/`memmove`/`memset` под
  капотом), через `NOVA_ARRAY_IMPL`-macro + специализации.
- Bounds-check по контракту Ф.0.4.

### Ф.3 — Codegen + registry (~0.3 д)

- Завести примитивы в реестр (`runtime_registry.rs` для `byte_at` на
  `str`; `prim_builtin_method`/array-dispatch для `[]T`).
- Mangling C-имён + Nova→C type-mapping.

### Ф.4 — Тесты pos/neg (~0.4 д)

- **Ф.4.1** `byte_at` — ASCII/UTF-8 байты, граница, OOB → `panic`.
- **Ф.4.2** `compare` — `Less`/`Equal`/`Greater`, разная длина,
  префиксное отношение; `==` через zero-case.
- **Ф.4.3** `copy_from` — distinct-срезы, разные `T`.
- **Ф.4.4** `copy_within` — overlap forward и backward (корректность
  `memmove`).
- **Ф.4.5** `fill` — заполнение, пустой срез.
- **Ф.4.6** Негатив — OOB → детерминированная `panic` (или `Fail`);
  length mismatch по контракту Ф.0.4.
- **Ф.4.7** `[]u8` round-trip: `copy_from` → `compare` → `==`.
- **Ф.4.8** Полный `nova test` — 0 новых FAIL.

### Ф.5 — Spec sync + docs (~0.2 д)

- `docs/plans/README.md` — статус Plan 90.
- `docs/simplifications.md` — маркеры отложенного (напр. bounds-check
  elimination), если аудит Ф.0.2 их породил.
- `docs/project-creation.txt` + `nova-private/discussion-log.md` — записи.

**Follow-up (вне scope Plan 90):** переписать тела str-методов
(`starts_with`/`ends_with`/`eq`/`lt..ge` через `compare`;
`find`/`contains`/`trim` через `byte_at`) и `WriteBuffer`/`ReadBuffer`
на Nova поверх этих примитивов.

## Итог Ф.0

> Заполняется по результатам аудита: подтверждённый набор примитивов;
> таблица «кейс сырого указателя → механизм / реальный gap»; выбор
> A/B по Ф.0.3 + обоснование; контракт границ Ф.0.4. До аудита пусто.

## Acceptance criteria

- [ ] `str @byte_at` — O(1), inline, bounds-checked; работает на
      ASCII/UTF-8, OOB → `panic`.
- [ ] `compare` — **один** примитив; `==`/`eq` работает как его
      zero-case; `lt/le/gt/ge` выводятся из него.
- [ ] `copy_from` / `copy_within` / `fill` работают для `[]u8` и
      `[]int`; `copy_within` корректен при overlap (= `memmove`).
- [ ] OOB → детерминированная `panic` (или `Fail` — по Ф.0.4).
- [ ] «Итог Ф.0» содержит полный аудит `unsafe`/FFI-ptr (6 кейсов) и
      нормативное решение A/B.
- [ ] Spec D-block опубликован; решение про `unsafe`/указатели
      зафиксировано нормативно.
- [ ] Полный `nova test` — 0 новых FAIL.

## Non-scope

- **GC / аллокатор на Nova** — остаётся C (substrate).
- **Тела str-методов / `WriteBuffer` / `ReadBuffer` на Nova** —
  follow-up; Plan 90 даёт под них фундамент.
- **Отдельный примитив `bytes_equal`** — не вводится; равенство — это
  zero-case `compare`. (Может появиться позже как fast-path, если
  профайл покажет необходимость — модель Go `bytes.Equal`.)
- **Атомики / lock-free** — не относится.
- **Общий `unsafe`-блок** — только если Ф.0.3 = B; по умолчанию Nova
  остаётся языком без сырых указателей (D6).

## Открытые вопросы для Ф.0

- `byte_at` — `-> u8` с `panic` на OOB (без per-call Option-оверхеда)
  vs `-> Option[u8]`. Рекомендация: `-> u8` + `panic` (как индексация).
- `compare` возвращает `int` (-1/0/1, модель Go) или sum
  `Less|Equal|Greater` (модель Rust; есть `Comparable`, Plan 85.4)?
- `[]T` для произвольного `T` сразу или сначала `[]u8` + numeric
  (`[]T` с GC-ссылками — копирование ссылок sound при non-moving GC)?
- `copy_*` границы: `panic` (Rust) vs min-копирование (Go).

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
  enabler миграции рантайма/stdlib на Nova.
- Ориентиры: Go `copy()`/`bytes`, Rust `slice::copy_*`/`[u8]::cmp`,
  TS typed arrays.
