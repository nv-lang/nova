# Plan 165 — Value-record iterator types + codegen generic-forward-decl fix

**Status:** ✅ CLOSED 2026-06-16  
**Commits:** `1f92f106` (codegen fix) · `3cec7a23` (stdlib value types) · `20d4ee8b` (docs + backlog)  
**Branch:** main (коммиты уже в main)  
**Зависит от:** Plan 153.2 ✅ (VecIter, lazy-iter layer), Plan 153 ✅ (Vec value-record), Plan 124.8 ✅ (value-record syntax `value`), Plan 162 ✅ (EnumerateIter)

---

## Мотивация

После Plan 153.2 (boxed-fluent lazy-адаптеры) итераторы `VecIter[T]`, `MapIter`, `FilterIter`
и аналоги жили как heap-record (обычные GC-объекты). Это означало:

1. Каждое `for x in v` — аллокация `VecIter[T]` на куче.
2. Каждый шаг цепочки `v.zmap(…).zfilter(…)` создавал boxed-объект адаптера.
3. Итераторные cursor'ы у Range/RangeIter не имели GC-pointer-полей → heap-аллокация
   была чистыми накладными расходами без какой-либо пользы.

Тип `value` (`type X value { … }`) уже был добавлен в Plan 124.8 и применён к `Vec[T]` в
Plan 153. Цель Plan 165 — применить его к итераторным типам, добившись **zero malloc
в adapter chain** для стандартных итерационных паттернов.

Параллельно в поле зрения попал баг codegen: при мономорфизации generic value-типа
(`type VecIter[T] value`) forward declaration эмитился без параметра типа
(`NovaValue_VecIter`), а определение — с полным mono-именем
(`NovaValue_VecIter____nova_int`). Несоответствие → CC-FAIL «incomplete return type».

---

## Ф.1 — Codegen fix: generic value-type mono C-name в forward declarations

**Коммит:** `1f92f106`

**Проблема.** `emit_forward_decl_for_generic_value_type` в `compiler-codegen/src/codegen/emit_c.rs`
эмитил `typedef struct NovaValue_VecIter NovaValue_VecIter;` (без type-param суффикса),
тогда как определение struct называлось `NovaValue_VecIter____nova_int`. Clang/MSVC
отказывается использовать неполный тип как return type функции → CC-FAIL.

**Исправление.** Forward declaration генерируется с полным мономорфным именем
(той же логикой, что и struct definition). Тип `"never"` добавлен в
`field_cache.rs` в предикат «примитивный лист» (ранее там было строчное `"never"`,
но регистр не совпадал — исправлено на нижний регистр).

**Acceptance criteria (A165.1.x):**

- A165.1.1: Компиляция `VecIter[int]` не выдаёт CC-FAIL «incomplete return type».
- A165.1.2: `field_cache` корректно распознаёт тип `never` как примитивный лист.
- A165.1.3: Существующие тесты plan162, plan153 не регрессируют.

---

## Ф.2 — Stdlib: VecIter[T] и Range*Iter — value-record

**Коммит:** `3cec7a23`

### VecIter[T]

`std/collections/vec/iter.nv` — добавлено ключевое слово `value`:

```nova
export type VecIter[T] value { ... }
```

`VecIter[T]` содержит GC-pointer-поля (`Vec[T]` = ссылка), поэтому переход на value
не нарушает GC — fiber arena автоматически корнирует value-record'ы со ссылочными
полями (D228). Аллокация курсора на стеке, ноль malloc при итерации.

### Range / RangeIter / StepRangeIter / ReverseRangeIter

`std/collections/range.nv` — все четыре типа объявлены value:

```nova
export type Range value { ... }
export type RangeIter value { ... }
export type StepRangeIter value { ... }
export type ReverseRangeIter value { ... }
```

Эти типы содержат только `int`-поля (без GC-pointer'ов) → stack-аллокация абсолютно
безопасна и даёт чистый выигрыш: cursor `0..n` — стековая структура из двух `int64_t`.

**Acceptance criteria (A165.2.x):**

- A165.2.1: `for _ in 0..100 { }` компилируется и работает без malloc (вся цепочка на стеке).
- A165.2.2: `for x in v { }` (VecIter) компилируется без CC-FAIL, работает корректно.
- A165.2.3: `for (i, x) in v.zenumerate() { }` (EnumerateIter.src = VecIter value) работает.
- A165.2.4: Тесты plan153_*, plan162, plan164 не регрессируют.

---

## Docs / backlog

**Коммит:** `20d4ee8b`

- Добавлена comparison table `VecIter[T] value` vs heap аналоги в docs/vec_iter.md.
- Маркер `[M-codegen-value-type-generic-forward-decl]` зарегистрирован в
  `docs/plans/backlog-followups.md` как P2 (forward-decl мисматч для *других*
  generic value-типов, которые могут появиться в будущем; для текущих типов уже
  исправлен в Ф.1).
- Тесты с голым `import X` мигрированы на `import X as X` (nova_tests/).

---

## Ф.3 — step_by: Fail[OverflowError] → requires контракт + copy-тесты

**Коммиты:** `f957d018` · `4d1ef9df`

### Мотивация

`Range.step_by(step int)` изначально объявлен с `Fail[OverflowError]` — семантически
неверно: `step <= 0` это нарушение контракта (программная ошибка), а не recoverable
runtime-ошибка. `Fail` предназначен для ожидаемых ошибок (IO, parse), которые caller
должен обработать. Невалидный аргумент — нарушение precondition.

**Проблема с Fail:** `_nova_throw_typed_void` — символ runtime, нужный линкеру при
любом вызове Fail-функции, даже если Fail не срабатывает. В standalone тест-харнессе
этот символ недоступен → тесты с `step_by` не линкуются.

### Изменения

`std/collections/range.nv`:

```nova
// До:
export fn Range @step_by(step int) Fail[OverflowError] -> StepRangeIter {
    if step <= 0 { throw OverflowError { msg: "..." } }
    StepRangeIter { @end, step, _cur: @start }
}

// После:
export fn Range @step_by(step int) -> StepRangeIter
    requires step > 0
    => { @end, step, _cur: @start }
```

`requires step > 0` — compile-time контракт (D24); при Z3 backend проверяется
статически; без Z3 — runtime panic при нарушении (D81 always-on assertion).

### Copy-тесты StepRangeIter/ReverseRangeIter

После снятия Fail с `step_by` стало возможным добавить copy-тесты прямо в `range.nv`
(standalone тест `step_by(2)` теперь линкуется):

```nova
test "StepRangeIter copy by value — independent cursors" { ... }
test "ReverseRangeIter copy by value — independent cursors" { ... }
```

Тесты проверяют что value-copy StepRangeIter/ReverseRangeIter даёт независимые курсоры.

**Acceptance criteria (A165.3.x):**

- A165.3.1: `step_by` не имеет `Fail` эффекта — вызов без `with Fail` в standalone тесте компилируется и линкуется.
- A165.3.2: `requires step > 0` присутствует как compile-time контракт (D24).
- A165.3.3: StepRangeIter и ReverseRangeIter copy-тесты в `range.nv` проходят в полном `nova test`.
- A165.3.4: «без упрощений как для прода» — нет `Fail` там где нужен контракт.

---

## Ф.4 — Fix: external fn → extern "nova" fn в stdlib (D282)

**Коммит:** `a5d4a30b`

D282 (Plan 91.12) убрал синтаксис `external fn` — нужно `extern "nova" fn` или
`extern "C" fn`. `std/runtime/raw_mem.nv` и `std/ffi/cstr.nv` содержали старый синтаксис
→ CC-FAIL внутри embedded stdlib (internal panic компилятора при попытке парсить
`raw_mem.nv` из embedded ресурса).

`std/runtime/raw_mem.nv` — 7 функций (`RawMem.copy`, `copy_nonoverlapping`, `fill`,
`compare`, `alloc`, `alloc_uncollectable`, `free_uncollectable`).
`std/ffi/cstr.nv` — 1 функция (`nova_str_terminated_ptr`).

**Acceptance criteria (A165.4.x):**

- A165.4.1: `nova test nova_tests/plan_value_iter` — 4/4 PASS (raw_mem не ломает prelude import chain).
- A165.4.2: `nova check std/runtime/raw_mem.nv` не выдаёт `E_EXTERNAL_FN_RETRACTED`.

---

## Followups

| Маркер | Описание | Приоритет |
|---|---|---|
| `[M-codegen-value-type-generic-forward-decl]` | Generic value-type forward-decl мисматч — закрыт для `VecIter`/`Range*Iter`; проверить новые value-типы при добавлении | P2 closed for this plan |
| `[M-153.2-tuple-elem-adapter]` | Chained adapter после enumerate (`enumerate().filter(…)`) гейтнут на closure-type propagation | P2 open |

---

## Spec

**D290** — добавлен в `spec/decisions/02-types.md`:
Iterator types объявлены как value-record.

**D282** (Plan 91.12) — `external fn` синтаксис убран; `extern "nova" fn` / `extern "C" fn`.
