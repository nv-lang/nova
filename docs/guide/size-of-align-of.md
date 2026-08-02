# `size_of[T]()` / `align_of[T]()` — compile-time type layout intrinsics

> **Plan 114.4.4 Ф.5 V4 + V4.4 Ф.1.** Comptime built-in identifiers,
> заменяются литералом `int` в rewriter pass. Только в `const`
> context (RHS of `const` decl или const fn body).

## Что они возвращают

```nova
const SIZE_INT  = size_of[int]()    // 8 — байт в памяти
const ALIGN_INT = align_of[int]()   // 8 — выравнивание (адрес кратен 8)
```

Оба возвращают `int` (i64). Оценка происходит **на этапе компиляции** —
в runtime это просто константа.

## Зачем нужно

CPU читает память не побайтно, а блоками. Если объект лежит "криво"
(адрес не кратен размеру блока), доступ медленнее (2 чтения вместо 1)
или crash на некоторых архитектурах.

**`size_of[T]`** — сколько байт занимает значение типа `T` в памяти.
**`align_of[T]`** — на границе какого числа байт оно должно лежать.

Правило: адрес объекта `T` должен делиться на `align_of[T]()`.

## Таблица типов (default x64 ABI)

| Тип | `size_of` | `align_of` | Заметка |
|---|---|---|---|
| `i8` / `u8` / `bool` | 1 | 1 | байт может лежать где угодно |
| `i16` / `u16` | 2 | 2 | |
| `i32` / `u32` / `f32` | 4 | 4 | |
| `char` | 4 | 4 | u32 codepoint |
| `int` / `i64` / `u64` / `f64` | 8 | 8 | natural alignment |
| `str` | 16 | 8 | slice ABI: pointer (8) + length (8) |
| `()` Unit | 0 | 1 | zero-sized type |
| `(T1, T2, ..)` Tuple | sum + padding | max(elem aligns) | C struct layout |
| `[N]T` FixedArray | `N * size_of(T)` | `align_of(T)` | |
| `[]T` Array (slice) | 16 | 8 | pointer + length |
| `readonly T` | `size_of(T)` | `align_of(T)` | transparent wrapper |

## Padding в композитных типах

Когда вы делаете tuple/struct, компилятор **добавляет невидимые байты-заполнители**
между полями чтобы каждое поле выровнялось правильно.

### Пример 1: `(i8, i32)` — нужен padding посередине

```
size_of[(i8, i32)]() == 8   // не 5! (1 + 4)
align_of[(i8, i32)]() == 4

Layout в памяти:
байты:   [0][1][2][3][4][5][6][7]
поле:    [i8][--padding--][i32        ]
         ^                ^
         offset 0         offset 4 (выровнен на 4)
```

i32 требует align 4 — после i8 (1 байт) нужно ещё 3 байта padding,
потом i32 ложится на адрес кратный 4.

### Пример 2: `(i32, i8)` — порядок меняет тривиальную часть

```
size_of[(i32, i8)]() == 8   // tail-pad до align 4
align_of[(i32, i8)]() == 4

Layout:
байты:   [0][1][2][3][4][5][6][7]
поле:    [i32        ][i8][tail-pad]
```

i32 ложится с offset 0, потом i8 на offset 4, и tail-padding 3 байта
чтобы общий размер был кратен max-align'у структуры (4).

### Пример 3: `(bool, int)` — большой gap

```
size_of[(bool, int)]() == 16
align_of[(bool, int)]() == 8

Layout:
байты:   [0][1][2][3][4][5][6][7][8][9]...[15]
поле:    [bool][----7 байт padding----][int          ]
```

int требует align 8 — после bool (1 байт) нужно 7 байт padding.

### Пример 4: `(i8, i8, i8)` — нет padding

```
size_of[(i8, i8, i8)]() == 3   // ровно 3
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
   как гарантия ABI compatibility.

## Сравнение с Rust

| Аспект | Rust | Nova |
|---|---|---|
| Имя | `std::mem::size_of::<T>()` / `align_of::<T>()` | `size_of[T]()` / `align_of[T]()` |
| Где живёт | std + compiler intrinsic | Built-in identifier (parser-special) |
| Runtime? | ✅ Да (как const fn) | ❌ Только comptime |
| Generic | ✅ Полностью | 🟡 Только non-generic V4.4; generic — followup |
| Records | ✅ Любые | 🟡 V4.4 — primitives + composite ABI; user records → V2 |

## V4.4 ограничения

**Поддержано:** primitives, tuples (recursive), FixedArray, Array (slice), Unit, Readonly.

**Не поддержано (V2 followup `[M-114.4.4-trampoline-named-types]`):**
- Named user records: `type Point { x int, y int }` — требует TypeDecl lookup.
- Sum types (tagged unions).
- Generic instantiations `Option[int]`.

Negative test `size_of_named_record_neg.nv` фиксирует current behavior —
emits `E_CONST_FN_GENERIC_NEEDS_T_REFLECTION` для named-records.
