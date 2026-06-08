<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 133 — Удалить `usize`/`isize`; `int` = адресное целое везде

> **Создан:** 2026-06-09.  **Статус:** 📋 PLANNED.
> **Эстимат:** ~1 dev-day.  **Model:** Sonnet 4.6.

---

## Что

Убрать типы `usize` и `isize` из Nova. Заменить на `int` везде.

`int` объявляется адресным знаковым целым (аналог Go `int`, Rust `isize`):
на 64-bit платформах = i64. Nova строго 64-bit — размер фиксирован.

`uint` остаётся для битовых паттернов и беззнакового байт-уровня,
но **не** как замена `usize` — это отдельный тип без адресной семантики.

`u8`, `u32`, `u64`, `i32` и т.д. — остаются без изменений (фиксированные).

### До / После

```nova
// БЫЛО
export #unsafe external fn RawMem.alloc(n usize) -> mut * u8
RawMem.alloc((n as usize) * (size_of[T]() as usize))

// СТАЛО
export #unsafe external fn RawMem.alloc(n int) -> mut * u8
RawMem.alloc(n * size_of[T]())
```

---

## Почему

- Nova строго 64-bit: `int` (i64) = `isize` (i64) = `usize` (u64 в смысле диапазона) на целевой платформе.
  Различие `int` vs `usize` — источник трения без пользы.
- `as usize` касты везде в коде с памятью — шум, который скрывает логику.
- Go делает то же самое: `len()` → `int`, нет `usize`.
- `size_of[T]()` → `int` устраняет сигнатурный мусор при вычислении размеров буферов.

---

## Фазы

### Ф.0 — Audit (~30 min)

Scope зафиксирован:
- `spec/`: ~67 вхождений
- `nova_tests/`: ~44 вхождений
- `std/`: 10 (raw_mem.nv × 6, vec_owned.nv × 2, комментарий × 2)
- `compiler-codegen/src/`: ~10 значимых строк (type mapping)

Паттерны миграции:
- `n usize` → `n int`
- `n isize` → `n int`
- `expr as usize` → убрать каст (если тип уже `int`)
- `expr as isize` → убрать каст

### Ф.1 — Compiler: правильные C-типы для `int`/`uint` + убрать `usize`/`isize` (~2h)

#### Ф.1.0 — `nova_int` = `intptr_t`, `nova_uint` = `uintptr_t`

Сейчас `nova_int = int64_t` и `uint → uint64_t` (фиксированные). Нужно:

- **Ф.1.0.1** `compiler-codegen/nova_rt/nova_rt.h`:
  ```c
  // БЫЛО
  typedef int64_t nova_int;
  // СТАЛО
  typedef intptr_t nova_int;
  ```
  Добавить:
  ```c
  typedef uintptr_t nova_uint;
  ```

- **Ф.1.0.2** `external_registry.rs`: разделить `int` и `i64` маппинги:
  ```rust
  // БЫЛО
  "int" | "i64" => "nova_int".into(),
  "uint"        => "uint64_t".into(),
  // СТАЛО
  "int"  => "nova_int".into(),    // intptr_t
  "i64"  => "int64_t".into(),     // фиксированный
  "uint" => "nova_uint".into(),   // uintptr_t
  ```

- **Ф.1.0.3** `emit_c.rs`: аналогичное разделение в C-type lookup.

На 64-bit `intptr_t` = `int64_t` — поведение не меняется. Семантически корректно.

#### Ф.1.1 — Убрать `usize`/`isize`

- **Ф.1.1** `emit_c.rs` ~line 1827: убрать `"usize"` и `"isize"` из known primitive types.
  E_TYPE_UNKNOWN с hint: `type \`usize\` is removed — use \`int\` (Plan 133)`

- **Ф.1.2** `emit_c.rs` ~lines 4597-4598: убрать маппинги `"usize"`/`"isize"`.

- **Ф.1.3** `external_registry.rs` ~lines 344-345: убрать маппинги `"usize"`/`"isize"`.

- **Ф.1.4** `size_of[T]()` return type → `int` (не `usize`).

- **Ф.1.5** NEG fixture: `let x usize = 5` → error «type `usize` removed, use `int`»
- **Ф.1.6** NEG fixture: `let x isize = 5` → error «type `isize` removed, use `int`»

**Commit:** `feat(plan133 Ф.1): nova_int=intptr_t, nova_uint=uintptr_t; remove usize/isize`

### Ф.2 — stdlib: `std/runtime/raw_mem.nv` (~30 min)

- **Ф.2.1** 6 параметров `n usize` → `n int` в RawMem внешних fn
- **Ф.2.2** Убрать комментарий `// usize для byte count (D226 alias, C size_t ABI)`
- **Ф.2.3** `std/collections/vec_owned.nv`: убрать `as usize` касты (2 места)

```nova
// vec_owned.nv ДО:
RawMem.alloc((n as usize) * (size_of[T]() as usize)) as *mut T
RawMem.alloc(size_of[T]() as usize) as *mut T

// ПОСЛЕ:
RawMem.alloc(n * size_of[T]()) as *mut T
RawMem.alloc(size_of[T]()) as *mut T
```

**Commit:** `refactor(plan133 Ф.2): std — usize → int in raw_mem.nv + vec_owned.nv`

### Ф.3 — nova_tests sweep (~2h)

44 вхождения. Паттерны:

1. `n as usize` → убрать каст (переменная уже `int`)
2. `x usize` → `x int` (тип параметра/переменной)
3. `-> usize` → `-> int`
4. `size_of[T]() as usize` → `size_of[T]()`

Выполнить bulk-sed:
```
sed -i 's/ as usize//g; s/: usize/: int/g; s/ usize)/) /g; ...'
```
Затем `nova test` — исправить остатки.

**Commit:** `refactor(plan133 Ф.3): nova_tests — usize/isize → int (44 sites)`

### Ф.4 — Spec (~1h)

- **Ф.4.1** D226 (где `usize` описан как alias) — пометить REMOVED / SUPERSEDED by Plan 133.
  Добавить: «`int` = address-sized signed integer; replaces `usize`/`isize`».
- **Ф.4.2** `spec/decisions/02-types.md` — таблица примитивных типов:
  убрать строки `usize`/`isize`, добавить примечание к `int`:
  «address-sized (64-bit); use for sizes, indices, counts».
- **Ф.4.3** `spec/decisions/03-syntax.md` — примеры с `usize` → `int`.
- **Ф.4.4** Все остальные вхождения в spec (67 total) — bulk замена + ручная правка контекста.

**Commit:** `docs(plan133 Ф.4): spec — remove usize/isize, document int as address-sized`

### Ф.5 — Logs (~15 min)

- **Ф.5.1** `docs/simplifications.md` — добавить запись о Plan 133.
- **Ф.5.2** `project-creation.txt` + nova-private `discussion-log.md`.

**Commit:** `docs(plan133 Ф.5): logs — plan133 usize removal`

---

## Acceptance criteria

- **A-133.a** — `usize` в Nova-коде → compile error с hint «use `int`»
- **A-133.b** — `isize` в Nova-коде → compile error с hint «use `int`»
- **A-133.c** — `RawMem.alloc(n int)` — принимает `int`, C-codegen кастит в `size_t`
- **A-133.d** — `size_of[T]()` → тип `int`
- **A-133.e** — `vec_owned.nv`: `n * size_of[T]()` без кастов — компилируется
- **A-133.f** — 0 regressions в full `nova test`
- **A-133.g** — `uint` не затронут (остаётся отдельным типом)

---

## C-кодогенерация

После Plan 133:
- `int` (Nova) → `nova_int` = `intptr_t` (адресный, 64-bit на 64-bit таргете)
- `i64` (Nova) → `int64_t` (фиксированный)
- `uint` (Nova) → `nova_uint` = `uintptr_t`
- `u64` (Nova) → `uint64_t` (фиксированный)
- `RawMem.alloc(n int)` → C-call `nova_alloc((size_t)n)` — `intptr_t`→`size_t` cast внутри

Это безопасно: `n ≥ 0` инвариант вызывающей стороны; Nova не добавляет runtime-проверок
(та же политика что у Go с `len()`).

---

## D-block changes

**D226 RETIRED (Plan 133):** `usize` alias удалён. `int` = address-sized signed integer
на 64-bit Nova target. Используй `int` для размеров, индексов, счётчиков байт.

**D-type-primitives amend:** таблица примитивов — строки `usize`/`isize` удалены.
`int` получает пометку «address-sized (= i64 on 64-bit)».

---

## Что НЕ меняется

- `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64` — фиксированные типы, остаются
- `uint` — остаётся (беззнаковый, не address-alias)
- Rust-код компилятора: `usize` там — это Rust-тип, не Nova-тип, не трогаем

## Followups

- `[M-133-uint-clarify]` — уточнить роль `uint` в spec: когда использовать `uint` vs `int`.
  Кандидаты: битовые маски, хеш-значения, raw байтовые операции.
- `[M-133-negative-size-lint]` — опциональный lint W_NEGATIVE_SIZE_ARG если
  передаётся `int` в позицию «размер буфера» и значение может быть отрицательным
  (Plan 114.4.x const fn flow — future).
