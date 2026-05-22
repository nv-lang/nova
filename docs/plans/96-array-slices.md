// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 96 — срезы `[]T` (`arr[a..b]` — sub-slice views)

> **Статус:** 📋 proposed 2026-05-23, не начат
> **Приоритет:** P2 (реальный пробел в выразительности языка: у `[]T`
> нет под-диапазонов вообще; разблокирует множество алгоритмов stdlib и
> упрощает bulk-операции Plan 90)
> **Оценка:** ~5–7 dev-day (языковая фича: парсер + type-checker +
> codegen + рантайм + GC-взаимодействие + spec)
> **Зависимости:** Range-литералы `a..b`/`a..=b` (D — `03-syntax.md`,
> `Range { start, end, inclusive }`) ✅; [Plan 90](90-memory-access-primitives.md)
> (bulk-операции `[]T`) ✅; [D6](../../spec/decisions/05-memory.md#d6)
> non-moving GC ✅.
> **Источник:** обсуждение 2026-05-23 — при проектировании `copy_from`
> выяснилось, что у `[]T` **нет срезов вообще** (ни `arr[a..b]`, ни
> `.slice` — только `str.slice`).

## Зачем

У `[]T` **нет под-диапазонов**. Чтобы передать «часть массива» в
функцию, сравнить хвосты, скопировать кусок — приходится либо
аллоцировать копию, либо тащить пары offset-параметров в каждый API
(как чуть не случилось с `copy_from`). Go/Rust/Python/TS все имеют
дешёвые срезы; Nova здесь **строго беднее**.

Срез `arr[a..b]` — это **вью** (`(ptr+a, len)` без копии данных,
разделяет backing). Он:
- разблокирует bulk-операции Plan 90 в естественной форме —
  `dst[..n].copy_from(src[k..k+n])` вместо offset-параметров;
- даёт под-диапазоны всем алгоритмам stdlib (парсеры, crypto, encoding,
  коллекции) без лишних аллокаций;
- закрывает пробел выразительности — паритет с Go/Rust/Python/TS.

## Сравнение с Go / Rust / TS

| Язык | Срез |
|---|---|
| **Rust** | `&s[a..b]` — вью, без копии. `Vec<T>` (владеет/растёт) и `&[T]`/`&mut [T]` (вью) — **разные типы** → нет footgun'а с `push` по вью. `a..b` exclusive, `a..=b` inclusive. |
| **Go** | `s[a:b]` — вью + `cap` до конца backing. Один тип `[]T` и для роста, и для вью → знаменитый footgun: `append` по вью может молча писать в родительский backing. |
| **TS** | `TypedArray.subarray(a,b)` — вью; `Array.slice(a,b)` — копия. `b` exclusive. |
| **Nova (сейчас)** | срезов `[]T` нет вообще. Беднее всех трёх. |
| **Nova (цель)** | `arr[a..b]` — вью; `b` exclusive (`a..=b` inclusive) — как существующий Range. Модель ближе к Rust (без Go-footgun'а — см. §«Напряжение»). |

## Привязка к коду (сверено 2026-05-23)

- **Range уже есть:** `a..b` / `a..=b` — литералы `Range { start, end,
  inclusive }` в любой expression-позиции (`spec/decisions/03-syntax.md`
  ~3075-3090). Срезам не нужен новый синтаксис диапазона — только
  индексация массива диапазоном.
- **`[]T` представление:** `NovaArray_T { T* data; int64_t len;
  int64_t cap; }` (`nova_rt/array.h`), передаётся как `NovaArray_T*`.
  Вью — новый `NovaArray_T` с `data = orig->data + a`.
- **Индексация:** codegen `arr[i]` (целочисленный индекс) — `emit_c.rs`;
  срез = тот же оператор `[]`, но аргумент — `Range`.
- **GC:** Boehm, non-moving (D6), `GC_all_interior_pointers` — `data`
  вью указывает ВНУТРЬ backing'а; Boehm держит backing живым по
  interior-указателю. ✅ feasible.
- **`[]T`-методы:** `push`/`pop`/`get`/`len`/`capacity`/`is_empty` +
  Plan 90 (`copy_from`/`copy_within`/`fill`/`compare`).

## Ключевое напряжение — `[]T` это И вектор, И срез

Nova имеет **один** тип `[]T` — и растущий вектор, и (будущий) вью.
`push` по вью опасен (вью разделяет backing). Развязки:
- **Go-модель:** вью + `cap` до конца backing; `append` пишет в общий
  backing если влезает. ⚠️ footgun.
- **Rust-модель:** разные типы (`Vec` / `&[T]`). Чисто, но Nova —
  один `[]T`.
- **Рекомендация (для Ф.0):** вью `arr[a..b]` имеет **`cap == len ==
  b-a`** (нет запаса). Тогда:
  - чтение/запись элемента (`view[i]`, `view[i] = x`) → идут в общий
    backing — это и есть смысл вью;
  - `push` по вью → `cap` исчерпан **всегда** → realloc → вью
    **отсоединяется** в независимый массив, родитель не затронут.
  → footgun'а Go нет (append по вью никогда молча не пишет в чужой
  backing), отдельный тип не нужен. Это **≥ Go** и близко к Rust.

## Scope

**Входит:**
- Синтаксис `arr[range]` где `range` — `Range` → sub-slice вью.
- Формы диапазона: `a..b`, `a..=b`, и open-ended `a..` / `..b` / `..`
  (набор — решение Ф.0).
- Вью (без копии данных), `cap == len` (см. §«Напряжение»).
- mut / immutable вью: запись через `mut`-вью видна в backing; без
  `mut` — read-only.
- Bounds-check → `panic` (D13).
- Type-checker, codegen, рантайм, GC-корректность.
- spec D-block.

**Не входит:**
- Срезы `str` — уже есть `str.slice` (отдельная семантика, codepoint).
- Многомерные / strided срезы (`arr[a..b, c..d]`) — не для bootstrap.
- Отдельный тип `Slice[T]` ≠ `[]T` (Rust-модель раздельных типов) —
  если Ф.0 выберет рекомендованную `cap==len`-модель, не нужен.

## Декомпозиция (фазы и шаги)

### Ф.0 — Design-аудит + decision points (~1 д) — GATE

- **Ф.0.1** Модель вью vs владелец (§«Напряжение»): подтвердить
  `cap == len` + push-отсоединение, либо обосновать иную. Probe:
  поведение `push`/`pop`/`copy_from` по вью.
- **Ф.0.2** GC-soundness: вью держит backing живым по
  interior-указателю (Boehm `GC_all_interior_pointers`). Проба —
  backing недостижим напрямую, достижим только через вью; стресс-GC
  не освобождает. Зафиксировать, что non-moving GC (D6) — необходимое
  условие (moving GC сломал бы interior-ptr).
- **Ф.0.3** Набор форм диапазона: `a..b` / `a..=b` обязательно;
  `a..` / `..b` / `..` — решить (нужен ли open-ended Range или sugar).
- **Ф.0.4** mut-семантика: `mut`-вью пишет в backing; кто может взять
  `mut`-вью (только от `mut`-источника). Взаимодействие с D6
  «immutable view без mut».
- **Ф.0.5** Контракт границ: `a > b`, `b > len`, отрицательные →
  `panic`. Пустой срез `arr[a..a]` — валиден.
- **Ф.0.6** Bootstrap-объём: вью только для чтения/записи элементов
  (без роста) достаточен для всех известных потребителей? Если да —
  push-отсоединение можно даже отложить (push по вью → ошибка
  компиляции/паника), зафиксировать.

### Ф.1 — Парсер + AST (~0.7 д)

- `arr[range_expr]` — индексация диапазоном. AST: `Index { obj, index }`
  где `index` может быть `Range`-выражением (отличить от int-индекса —
  по типу/форме).
- Open-ended формы по Ф.0.3.

### Ф.2 — Type-checker (~0.8 д)

- `[]T` индексированный `Range` → тип `[]T` (срез того же элемента).
- `[]T` индексированный `int` → `T` (как сейчас).
- mut-правила (Ф.0.4): `mut`-вью только от `mut`-источника.

### Ф.3 — Рантайм + codegen (~1.5 д)

- Рантайм: `nova_array_slice_##T(NovaArray_##T* a, int64_t from,
  int64_t to)` → новый `NovaArray_##T` с `data = a->data + from`,
  `len = cap = to - from`; bounds-check → `nv_panic`.
- Codegen: `arr[a..b]` → вызов `nova_array_slice_<T>`; диспетч
  оператора `[]` по типу аргумента (int vs Range).
- GC: убедиться, что вью-struct сканируется и interior-ptr держит
  backing (как правило — автоматически Boehm'ом; проверить).

### Ф.4 — Взаимодействие (~0.8 д)

- `for x in arr[a..b]` — итерация по вью.
- `arr[a..b].copy_from(...)` / `.compare(...)` / `.fill(...)` /
  `.len()` — bulk-операции Plan 90 по вью.
- `push` по вью — поведение из Ф.0.1/Ф.0.6.
- Передача вью в функцию, ждущую `[]T`.

### Ф.5 — Тесты (~0.7 д)

- `nova_tests/plan96/`: базовый срез; `a..b`/`a..=b`/open-ended;
  запись через `mut`-вью видна в backing; чтение; пустой срез;
  OOB → `panic` (негатив); вью переживает недостижимость backing'а
  (GC); `for-in` по вью; `copy_from`/`compare` по вью; push-поведение.
- Полный `nova test` — 0 новых FAIL.

### Ф.6 — Spec + docs (~0.5 д)

- D-block в `spec/decisions/02-types.md` (или `05-memory.md`): срезы
  `[]T` — семантика вью, `cap==len`, mut, границы, GC-условие.
- `docs/plans/README.md`, `docs/simplifications.md` (если Ф.0.6 что-то
  отложил), `project-creation.txt`, `nova-private/discussion-log.md`.

## Итог Ф.0

> Заполняется по результатам аудита: модель вью (Ф.0.1) + обоснование;
> GC-проба (Ф.0.2); набор форм диапазона (Ф.0.3); mut-правила (Ф.0.4);
> контракт границ (Ф.0.5); bootstrap-объём (Ф.0.6). До аудита пусто.

## Acceptance criteria

- [ ] `arr[a..b]` / `arr[a..=b]` (+ open-ended по Ф.0.3) — sub-slice
      **вью**, без копирования данных backing'а.
- [ ] Запись через `mut`-вью видна в исходном массиве; чтение — тоже
      из общего backing'а.
- [ ] `b` — exclusive (`a..b`), inclusive (`a..=b`) — консистентно с
      существующим Range.
- [ ] OOB / `a > b` → детерминированный `panic`; пустой срез валиден.
- [ ] Вью держит backing живым (GC interior-pointer) — проба на
      недостижимом-напрямую backing'е зелёная.
- [ ] `for-in`, `copy_from`/`compare`/`fill`/`len` Plan 90 — работают
      по вью.
- [ ] Полный `nova test` — 0 новых FAIL.
- [ ] spec D-block опубликован.

## Non-scope

- Срезы `str` — `str.slice` уже есть.
- Многомерные / strided срезы.
- Moving GC — D6 фиксирует non-moving; срезы-вью это условие
  закрепляют (interior-указатели).
- Полная Rust-модель раздельных типов `Vec`/`&[T]` — при
  рекомендованной `cap==len`-модели не нужна.

## Связь

- [Plan 90](90-memory-access-primitives.md) — bulk-операции `[]T`;
  срезы делают их offset-параметры ненужными (`copy_from` остаётся
  односложным).
- [D6](../../spec/decisions/05-memory.md#d6) — non-moving GC —
  необходимое условие срезов-вью.
- [03-syntax.md](../../spec/decisions/03-syntax.md) — Range-литералы
  `a..b`/`a..=b` (фундамент синтаксиса срезов).
- Ориентиры: Rust `&[T]`/`a..b`, Go `s[a:b]` (но без footgun'а
  `append`), TS `subarray`.
