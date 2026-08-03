# `size_of[T]()` / `align_of[T]()` — compile-time type layout intrinsics

**English** | [Русский](size-of-align-of.ru.md)

> **Plan 114.4.4 Ф.5 V4 + V4.4 Ф.1.** Comptime built-in identifiers,
> replaced by an `int` literal in the rewriter pass. Only inside a `const`
> context (RHS of a `const` decl or a const fn body).

## What they return

```nova
const SIZE_INT  = size_of[int]()    // 8 — байт в памяти
const ALIGN_INT = align_of[int]()   // 8 — выравнивание (адрес кратен 8)
```

Both return `int` (i64). Evaluation happens **at compile time** — at
runtime it's just a constant.

## Why it's needed

The CPU doesn't read memory byte-by-byte, but in blocks. If an object
sits "crooked" (its address isn't a multiple of the block size), access
is slower (2 reads instead of 1) or crashes outright on some architectures.

**`size_of[T]`** — how many bytes a value of type `T` occupies in memory.
**`align_of[T]`** — on which byte-count boundary it must sit.

Rule: the address of an object of type `T` must be divisible by `align_of[T]()`.

## Type table (default x64 ABI)

| Type | `size_of` | `align_of` | Note |
|---|---|---|---|
| `i8` / `u8` / `bool` | 1 | 1 | a byte can sit anywhere |
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

## Padding in composite types

When you build a tuple/struct, the compiler **inserts invisible filler
bytes** between fields so that each field lands at the right alignment.

### Example 1: `(i8, i32)` — padding is needed in the middle

```
size_of[(i8, i32)]() == 8   // не 5! (1 + 4)
align_of[(i8, i32)]() == 4

Layout в памяти:
байты:   [0][1][2][3][4][5][6][7]
поле:    [i8][--padding--][i32        ]
         ^                ^
         offset 0         offset 4 (выровнен на 4)
```

i32 requires align 4 — after i8 (1 byte) 3 more padding bytes are needed,
then i32 lands on an address that's a multiple of 4.

### Example 2: `(i32, i8)` — order changes the trivial part

```
size_of[(i32, i8)]() == 8   // tail-pad до align 4
align_of[(i32, i8)]() == 4

Layout:
байты:   [0][1][2][3][4][5][6][7]
поле:    [i32        ][i8][tail-pad]
```

i32 lands at offset 0, then i8 at offset 4, and 3 bytes of tail-padding
so the total size is a multiple of the struct's max-align (4).

### Example 3: `(bool, int)` — a large gap

```
size_of[(bool, int)]() == 16
align_of[(bool, int)]() == 8

Layout:
байты:   [0][1][2][3][4][5][6][7][8][9]...[15]
поле:    [bool][----7 байт padding----][int          ]
```

int requires align 8 — after bool (1 byte) 7 padding bytes are needed.

### Example 4: `(i8, i8, i8)` — no padding

```
size_of[(i8, i8, i8)]() == 3   // ровно 3
align_of[(i8, i8, i8)]() == 1
```

Everything is align 1 — they sit back-to-back, no padding at all.

## Where this is needed in practice

1. **Layout-aware code** — when serializing a struct to a binary format,
   you need to know field offsets.
2. **FFI with C** — for compatibility with C struct layout you need to
   know `size_of` / `align_of` on both sides.
3. **Manual memory layout** — writing an allocator / memory pool,
   you need class sizes.
4. **Compile-time assertions** — `assert!(size_of[MyStruct]() == 32)`
   as a guarantee of ABI compatibility.

## Comparison with Rust

| Aspect | Rust | Nova |
|---|---|---|
| Name | `std::mem::size_of::<T>()` / `align_of::<T>()` | `size_of[T]()` / `align_of[T]()` |
| Where it lives | std + compiler intrinsic | Built-in identifier (parser-special) |
| Runtime? | ✅ Yes (as a const fn) | ❌ Comptime only |
| Generic | ✅ Fully | 🟡 Non-generic only in V4.4; generic — followup |
| Records | ✅ Any | 🟡 V4.4 — primitives + composite ABI; user records → V2 |

## V4.4 limitations

**Supported:** primitives, tuples (recursive), FixedArray, Array (slice), Unit, Readonly.

**Not supported (V2 followup `[M-114.4.4-trampoline-named-types]`):**
- Named user records: `type Point { x int, y int }` — requires a TypeDecl lookup.
- Sum types (tagged unions).
- Generic instantiations `Option[int]`.

The negative test `size_of_named_record_neg.nv` pins down current
behavior — it emits `E_CONST_FN_GENERIC_NEEDS_T_REFLECTION` for named
records.
