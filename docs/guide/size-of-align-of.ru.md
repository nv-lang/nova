---
source_rev: 27d5dd055
source_date: 2026-08-02
---

# `size_of[T]()` / `align_of[T]()` — интринсики раскладки типов на этапе компиляции

[English](size-of-align-of.md) | **Русский**

> **Plan 114.4.4.** Встроенные идентификаторы компилятора
> (comptime built-in), заменяются литералом `int` в rewriter-проходе. Только
> в `const`-контексте (правая часть `const`-объявления или тело const fn).

## Что они возвращают

```nova
const SIZE_INT  = size_of[int]()    // 8 — bytes in memory
const ALIGN_INT = align_of[int]()   // 8 — alignment (address is a multiple of 8)
```

Оба возвращают `int` (i64). Оценка происходит **на этапе компиляции** —
в рантайме это просто константа.

## Зачем нужно

CPU читает память не побайтно, а блоками. Если объект лежит "криво"
(адрес не кратен размеру блока), доступ медленнее (2 чтения вместо 1)
или crash на некоторых архитектурах.

**`size_of[T]`** — сколько байт занимает значение типа `T` в памяти.
**`align_of[T]`** — на границе какого числа байт оно должно лежать.

Правило: адрес объекта `T` должен делиться на `align_of[T]()`.

## Таблица типов (дефолтный x64 ABI)

| Тип | `size_of` | `align_of` | Заметка |
|---|---|---|---|
| `i8` / `u8` / `bool` | 1 | 1 | байт может лежать где угодно |
| `i16` / `u16` | 2 | 2 | |
| `i32` / `u32` / `f32` | 4 | 4 | |
| `char` | 4 | 4 | кодовая точка u32 |
| `int` / `i64` / `u64` / `f64` | 8 | 8 | естественное выравнивание |
| `str` | 16 | 8 | slice-ABI: указатель (8) + длина (8) |
| `()` Unit | 0 | 1 | zero-sized тип |
| `(T1, T2, ..)` Tuple | sum + padding | max(elem aligns) | раскладка C struct |
| `[N]T` FixedArray | `N * size_of(T)` | `align_of(T)` | |
| `[]T` Array (slice) | 16 | 8 | указатель + длина |
| `readonly T` | `size_of(T)` | `align_of(T)` | прозрачная обёртка |

## Заполнение (padding) в составных типах

Когда вы делаете tuple/struct, компилятор **добавляет невидимые байты-заполнители**
между полями чтобы каждое поле выровнялось правильно.

### Пример 1: `(i8, i32)` — нужен padding посередине

```
size_of[(i8, i32)]() == 8   // not 5! (1 + 4)
align_of[(i8, i32)]() == 4

Memory layout:
bytes:   [0][1][2][3][4][5][6][7]
field:   [i8][--padding--][i32        ]
         ^                ^
         offset 0         offset 4 (aligned to 4)
```

i32 требует align 4 — после i8 (1 байт) нужно ещё 3 байта padding,
потом i32 ложится на адрес кратный 4.

### Пример 2: `(i32, i8)` — порядок меняет тривиальную часть

```
size_of[(i32, i8)]() == 8   // tail-pad up to align 4
align_of[(i32, i8)]() == 4

Layout:
bytes:   [0][1][2][3][4][5][6][7]
field:   [i32        ][i8][tail-pad]
```

i32 ложится с offset 0, потом i8 на offset 4, и tail-padding 3 байта
чтобы общий размер был кратен max-align'у структуры (4).

### Пример 3: `(bool, int)` — большой gap

```
size_of[(bool, int)]() == 16
align_of[(bool, int)]() == 8

Layout:
bytes:   [0][1][2][3][4][5][6][7][8][9]...[15]
field:   [bool][----7 bytes padding-----][int          ]
```

int требует align 8 — после bool (1 байт) нужно 7 байт padding.

### Пример 4: `(i8, i8, i8)` — нет padding

```
size_of[(i8, i8, i8)]() == 3   // exactly 3
align_of[(i8, i8, i8)]() == 1
```

Всё align 1 — лежат подряд, никакого padding.

## Где это нужно на практике

1. **Layout-aware code** — когда сериализуешь struct в бинарный формат,
   нужно знать смещения полей.
2. **FFI с C** — для совместимости C struct layout нужно знать
   `size_of` / `align_of` обеих сторон.
3. **Manual memory layout** — пишешь allocator / memory pool,
   нужны размеры классов.
4. **Compile-time assertions** — `assert!(size_of[MyStruct]() == 32)`
   как гарантия ABI-совместимости.

## Сравнение с Rust

| Аспект | Rust | Nova |
|---|---|---|
| Имя | `std::mem::size_of::<T>()` / `align_of::<T>()` | `size_of[T]()` / `align_of[T]()` |
| Где живёт | std + интринсик компилятора | Встроенный идентификатор (специально обрабатывается парсером) |
| Runtime? | ✅ Да (как const fn) | ❌ Только на этапе компиляции |
| Generic | ✅ Полностью | 🟡 Только non-generic V4.4; generic — followup |
| Records | ✅ Любые | 🟡 V4.4 — примитивы + составной ABI; пользовательские records → V2 |

## V4.4 ограничения

**Поддержано:** примитивы, кортежи (рекурсивно), FixedArray, Array (slice),
Unit, Readonly.

**Не поддержано (V2 followup `[M-114.4.4-trampoline-named-types]`):**
- Именованные пользовательские records: `type Point { x int, y int }` —
  требует TypeDecl-lookup.
- Типы-суммы (tagged unions).
- Обобщённые инстанциации `Option[int]`.

Негативный тест `size_of_named_record_neg.nv` фиксирует текущее поведение —
эмитит `E_CONST_FN_GENERIC_NEEDS_T_REFLECTION` для именованных records.
