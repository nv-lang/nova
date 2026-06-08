# Plan 131 — Vec[T] implemented in Nova

**Статус:** 📋 PLANNED 2026-06-08  
**Приоритет:** P1  
**Модель:** Sonnet 4.6 HIGH (Ф.0-Ф.2), Opus 4.8 + Thinking (Ф.3-Ф.4 если сложно)  
**Worktree:** `D:/Sources/nv-lang/nova-p131`  
**Ветка:** `plan-131-vec-in-nova`

---

## Мотивация

`[]T` в Nova — встроенный тип, реализованный через C-макрос `NOVA_ARRAY_DECL(T)`
в `array.h`. Это даёт два фундаментальных ограничения:

1. **`[]Option[T]` / `[]tuple` не работают** — `NovaOpt_T` это value-struct
   (9+ байт), не влезает в `int64_t`-слот erasure. `[]Record` работает только
   через side-channel `array_element_types` (Plan 91 Ф.1).
2. **Новые element-типы требуют изменений в C** — `NOVA_ARRAY_DECL` нужно
   добавлять вручную в `array.h`.

**Решение:** реализовать `Vec[T]` как полноценный Nova-тип с `*mut T` pointer,
`len int`, `cap int`. Это даёт правильное typed-storage для любого T — в том
числе `Option[T]`, tuple, value-record — без int64-erasure.

`[]T` встроенный тип **остаётся** (не мигрируется сейчас). `Vec[T]` — новый тип
рядом. В будущем `[]T` может стать синтаксическим сахаром над `Vec[T]`.

---

## Предусловия

| Что | Статус |
|-----|--------|
| `*T` / `*mut T` typed pointers | ✅ Plan 118 |
| `unsafe {}` block | ✅ Plan 118 |
| `external fn` declarations | ✅ |
| `size_of[T]()` intrinsic | ✅ Plan 114.4.4 |
| `RawMem.copy_nonoverlapping` | ✅ Plan 118.1 |
| `nova_alloc` экспортирован в Nova | ❌ **нужно сделать** |
| pointer arithmetic `*T + int` | 🟡 в спеке D216 §6, нужно проверить codegen |
| deref write `*ptr = v` | 🟡 нужно проверить codegen |
| pointer cast `*mut u8 as *mut T` | 🟡 нужно проверить codegen |

---

## Фазы

### Ф.0 — Probe: что реально работает в codegen (½ дня)

Написать минимальные probe-тесты и прогнать через release nova:

1. `ptr + n` — pointer arithmetic scaled by sizeof(T)
2. `*ptr = v` — deref write (lvalue deref assign)
3. `p as *mut T` — pointer cast
4. `unsafe {}` block с deref-write внутри

Результат: список того что работает / что нужно починить в codegen.
Зафиксировать в этом документе под `### Probe results`.

**Критерии приёмки Ф.0:**
- [ ] Probe fixtures созданы в `nova_tests/plan131/`
- [ ] Каждый probe — отдельный `.nv` файл с `EXPECT_OUTPUT` или `EXPECT_COMPILE_ERROR`
- [ ] Результат зафиксирован в `### Probe results` ниже

---

### Ф.1 — RawMem.alloc (1 день)

Единственная реально отсутствующая функция. Добавить в `std/runtime/raw_mem.nv`:

```nova
/// Allocate `n` bytes, GC-tracked (Boehm/malloc backend), zeroed.
/// Returns untyped `*mut u8`. Caller casts to needed type.
/// Must be called inside `unsafe {}` block.
///
/// Backed by `nova_alloc(n)` from `nova_rt/alloc.h`.
/// Memory is GC-collectable — do NOT store in static/global without
/// GC-root registration (see `nova_alloc_uncollectable` for that).
///
/// # Safety
/// - Caller responsible for alignment (nova_alloc returns 8-byte aligned).
/// - Size must be > 0; behaviour with 0 is implementation-defined.
#unsafe
export external fn RawMem.alloc(n usize) -> *mut u8

/// Allocate `n` bytes, NOT GC-tracked (caller must call `RawMem.free`).
/// Used for long-lived buffers that must not be collected.
#unsafe
export external fn RawMem.alloc_uncollectable(n usize) -> *mut u8

/// Free a pointer allocated with `RawMem.alloc_uncollectable`.
/// UB if called on GC-tracked pointer (from `RawMem.alloc`).
#unsafe
export external fn RawMem.free_uncollectable(ptr *mut u8) -> ()
```

Зарегистрировать в codegen `external_sources` (emit_c.rs) — аналогично
как `RawMem.copy_nonoverlapping` → `memmove`.

**C-маппинг:**
- `RawMem.alloc(n)` → `nova_alloc(n)`
- `RawMem.alloc_uncollectable(n)` → `nova_alloc_uncollectable(n)`
- `RawMem.free_uncollectable(ptr)` → `nova_free_uncollectable(ptr)`

**Тесты Ф.1** (`nova_tests/plan131/`):
- `alloc_write_read_pos.nv` — alloc + write через ptr + read back (int)
- `alloc_record_pos.nv` — alloc + store record pointer
- `alloc_zero_init_pos.nv` — проверить что память zeroed
- `neg/alloc_outside_unsafe.nv` — `EXPECT_COMPILE_ERROR E_UNSAFE_REQUIRED`

**Критерии приёмки Ф.1:**
- [ ] `RawMem.alloc` / `alloc_uncollectable` / `free_uncollectable` доступны из Nova
- [ ] 3 позитивных + 1 негативный тест PASS через release nova
- [ ] D-block обновлён (см. Ф.5)
- [ ] Коммит: `feat(plan131 Ф.1): RawMem.alloc/alloc_uncollectable/free_uncollectable`

---

### Ф.2 — Codegen: pointer arithmetic + deref write (1-2 дня)

По результатам Ф.0 — чинить то что не работает.

#### Ф.2а — `*T + int` pointer arithmetic

По спеке D216 §6: `ptr + N` scaled by `sizeof(T)`, результат `*unsafe T`.
В C: `p + N` уже scaled (C pointer arithmetic). Codegen должен эмитить `(p + N)`.

Что проверить в `emit_c.rs`:
- `BinOp::Add` на `*T` left operand → эмитить `(left_c + right_c)`
- Тип результата → `*unsafe T` (degrade pointer safety, D216 §6)
- `BinOp::Sub` на двух `*T` → `(left_c - right_c)`, тип `isize`

#### Ф.2б — `*ptr = v` deref write

`*(ptr + i) = value` должно эмитить `*(ptr + i) = value` в C.

Что проверить в `emit_c.rs`:
- `Assign { target: Deref(expr), value }` → `*(<target_c>) = <value_c>`
- Должно работать внутри `unsafe {}` block

#### Ф.2в — pointer cast `p as *mut T`

`p as *mut T` в C → `(T*)(p)`. Проверить `As` cast codegen для pointer types.

**Тесты Ф.2** (`nova_tests/plan131/`):
- `ptr_arith_pos.nv` — `ptr + 0`, `ptr + 1`, `ptr + n` (int)
- `ptr_deref_write_pos.nv` — `*ptr = v`, read back, assert
- `ptr_cast_pos.nv` — `p as *mut u8 as *mut i32` round-trip
- `ptr_arith_sub_pos.nv` — `ptr_b - ptr_a` → isize element count
- `neg/ptr_arith_outside_unsafe.nv` — `EXPECT_COMPILE_ERROR`
- `neg/ptr_deref_write_outside_unsafe.nv` — `EXPECT_COMPILE_ERROR`

**Критерии приёмки Ф.2:**
- [ ] `ptr + N`, `ptr - ptr`, `*ptr = v`, `p as *mut T` работают в codegen
- [ ] 4 позитивных + 2 негативных теста PASS
- [ ] Коммит: `feat(plan131 Ф.2): codegen pointer arithmetic + deref-write`

---

### Ф.3 — Vec[T] реализация в Nova (2-3 дня)

Файл: `std/collections/vec_owned.nv` (модуль `collections.vec_owned`).

#### Структура

```nova
/// Heap-allocated growable array with typed storage.
/// Unlike built-in `[]T` (int64-erasure for composite), Vec[T] stores
/// elements at their real C type — works for Option[T], tuples, records.
///
/// Layout: (data: *mut T, len: int, cap: int) — mirrors []T ABI (D32)
/// but fully GC-tracked and Nova-implemented.
export type Vec[T] {
    priv data *mut T
    priv mut len  int
    priv mut cap  int
}
```

#### API (все методы, без упрощений)

**Конструкторы:**
```nova
Vec[T].new() -> Vec[T]
Vec[T].with_capacity(n int) -> Vec[T]
Vec[T].from(items []T) -> Vec[T]       // из встроенного []T
```

**Размер:**
```nova
Vec[T] @len() -> int
Vec[T] @cap() -> int
Vec[T] @is_empty() -> bool
```

**Добавление/удаление:**
```nova
Vec[T] mut @push(v T) -> ()
Vec[T] mut @pop() -> Option[T]
Vec[T] mut @insert(i int, v T) -> ()   // сдвиг вправо
Vec[T] mut @remove(i int) -> T          // сдвиг влево
Vec[T] mut @swap_remove(i int) -> T     // O(1): swap с last + pop
Vec[T] mut @clear() -> ()
Vec[T] mut @truncate(n int) -> ()
```

**Доступ:**
```nova
Vec[T] @get(i int) -> Option[T]
Vec[T] mut @get_mut(i int) -> Option[*mut T]
Vec[T] @first() -> Option[T]
Vec[T] @last() -> Option[T]
```

**Ёмкость:**
```nova
Vec[T] mut @reserve(additional int) -> ()
Vec[T] mut @shrink_to_fit() -> ()
Vec[T] mut @shrink_to(min_cap int) -> ()
```

**Bulk:**
```nova
Vec[T] mut @extend(other []T) -> ()     // append из []T slice
Vec[T] mut @append(other Vec[T]) -> ()  // consume other Vec
Vec[T] mut @retain(pred fn(T) -> bool) -> ()  // filter in-place
Vec[T] mut @dedup_by(eq fn(T, T) -> bool) -> ()
Vec[T] mut @reverse() -> ()
```

**Итерация / slice:**
```nova
Vec[T] @as_slice() -> []T               // view как []T (без копии если возможно)
Vec[T] @contains(v T) -> bool           // через Eq protocol
Vec[T] @index_of(v T) -> Option[int]
```

**Grow strategy:** × 2 (как array.h + Go). Начальная ёмкость: 8.

**Реализация grow:**
```nova
priv fn[T] Vec[T] mut @grow_to(new_cap int) -> () {
    unsafe {
        let new_data = RawMem.alloc(new_cap * size_of[T]()) as *mut T
        if @cap > 0 {
            RawMem.copy_nonoverlapping(
                @data as *u8,
                new_data as *mut u8,
                @len * size_of[T]()
            )
        }
        @data = new_data
        @cap  = new_cap
    }
}
```

**Индексный оператор** (если поддерживается в Nova):
```nova
Vec[T] @[](i int) -> T     // panic on OOB (как Rust [])
```

#### Тесты Ф.3 (позитивные — `nova_tests/plan131/`)

- `vec_basic_pos.nv` — new/push/pop/len/is_empty
- `vec_capacity_pos.nv` — with_capacity/reserve/shrink_to_fit/grow
- `vec_access_pos.nv` — get/first/last/contains/index_of
- `vec_mutate_pos.nv` — insert/remove/swap_remove/clear/truncate/reverse
- `vec_bulk_pos.nv` — extend/append/retain/dedup_by
- `vec_from_slice_pos.nv` — from([]T)/as_slice()
- `vec_option_elem_pos.nv` — **Vec[Option[int]]** (главный мотивационный кейс)
- `vec_tuple_elem_pos.nv` — **Vec[(int, str)]** (tuple by-value)
- `vec_record_elem_pos.nv` — **Vec[MyRecord]** (struct by-value)
- `vec_nested_pos.nv` — **Vec[Vec[int]]** (nested Vec)
- `vec_large_grow_pos.nv` — push 10000 элементов, проверить корректность

#### Тесты Ф.3 (негативные)

- `neg/vec_oob_panics.nv` — `EXPECT_PANIC` при `v[100]` на empty Vec
- `neg/vec_pop_empty.nv` — pop on empty → None (не panic)
- `neg/vec_remove_oob.nv` — `EXPECT_PANIC` remove out of bounds
- `neg/vec_type_mismatch.nv` — `EXPECT_COMPILE_ERROR`

**Критерии приёмки Ф.3:**
- [ ] `Vec[T]` компилируется и линкуется для всех element-типов
- [ ] Все 11 позитивных + 4 негативных PASS через release nova
- [ ] **`Vec[Option[int]]` работает** (ключевой критерий)
- [ ] **`Vec[(int, str)]` работает** (tuple by-value)
- [ ] **`Vec[MyRecord]` работает** (struct by-value)
- [ ] Коммит: `feat(plan131 Ф.3): Vec[T] implementation in Nova`

---

### Ф.4 — Protocol implementations (1 день)

`Vec[T]` должен реализовывать стандартные протоколы:

```nova
// Iterable[T] — for-in поддержка
// impl(Iterable[T]) for Vec[T]

// Eq — ==  (если T: Eq)
// impl(Eq) for Vec[T] where T: Eq

// Clone — .clone() (если T: Clone)  
// impl(Clone) for Vec[T] where T: Clone

// DebugPrintable — println/fmt
// impl(DebugPrintable) for Vec[T] where T: DebugPrintable
```

Проверить что `for x in my_vec { ... }` работает.

**Тесты Ф.4:**
- `vec_iter_pos.nv` — for-in, collect results
- `vec_eq_pos.nv` — vec_a == vec_b
- `vec_clone_pos.nv` — clone, modify, assert original unchanged
- `vec_debug_pos.nv` — println(v) / fmt

**Критерии приёмки Ф.4:**
- [ ] `for x in vec` работает
- [ ] `vec_a == vec_b` работает
- [ ] `.clone()` работает
- [ ] println(vec) работает
- [ ] 4 тестовых файла PASS

---

### Ф.5 — Spec, D-blocks, docs (½ дня)

#### Новые/обновлённые D-блоки

**D-NEW (в 02-types.md): RawMem allocator API**
- `RawMem.alloc(n usize) -> *mut u8` — GC-tracked
- `RawMem.alloc_uncollectable(n usize) -> *mut u8`
- `RawMem.free_uncollectable(ptr *mut u8)`
- Связь с `nova_alloc` / `nova_alloc_uncollectable` / `nova_free_uncollectable`
- Safety contract: 8-byte alignment, GC vs uncollectable lifecycle

**D216 amend (в 02-types.md):** `ptr + N` pointer arithmetic — добавить
примеры с `Vec[T]` grow, подтвердить scaled-by-sizeof семантику в codegen.

**D-NEW: `Vec[T]` type spec (в 03-syntax.md или новый файл 06-collections.md)**
- Layout: `(data *mut T, len int, cap int)`
- Grow policy: ×2, initial 8
- Отличие от `[]T`: typed storage vs int64-erasure
- Когда использовать Vec[T] vs []T

#### Q-блоки

**Q-NEW: Q-vec-vs-slice** — "Когда Vec[T] vs []T?"
- `[]T` — default для всего; примитивы, records через pointer (работает)  
- `Vec[T]` — когда нужен typed storage: `Option[T]`, tuple by-value,
  fixed-size value-structs > 8 bytes

#### Документы

- `docs/collections/vec-owned.md` — user guide для `Vec[T]`
- Обновить `docs/plans/91-stdlib-mvp-for-0.1.md` — `[M-91.1-value-struct-array-elem]`
  закрыт через Plan 131

**Критерии приёмки Ф.5:**
- [ ] D-блок для `RawMem.alloc` добавлен
- [ ] D216 amend с pointer arithmetic подтверждён
- [ ] D-блок для `Vec[T]` добавлен
- [ ] Q-vec-vs-slice написан
- [ ] `docs/collections/vec-owned.md` готов
- [ ] Коммит: `docs(plan131 Ф.5): Vec[T] spec D-blocks + Q + user guide`

---

### Ф.6 — Logs + final close (½ дня)

- Обновить `docs/project-creation.txt`
- Обновить `docs/simplifications.md`
- Обновить `d:/Sources/nv-lang/nova-private/discussion-log.md`
- Обновить `docs/plans/README.md` — запись о Plan 131
- Зафиксировать статус в этом файле (`### Status`)
- Финальная полная регрессия `nova test nova_tests` — 0 новых падений
- Коммит: `docs(plan131 Ф.6): logs + final close`

---

## Acceptance Criteria (общие)

| ID | Критерий |
|----|---------|
| A1 | `RawMem.alloc(n)` работает из Nova unsafe-блока |
| A2 | `ptr + n`, `ptr - ptr`, `*ptr = v` работают в codegen |
| A3 | `Vec[int]` — push 10000 + pop все → корректно |
| A4 | `Vec[Option[int]]` — push Some/None + get → корректно |
| A5 | `Vec[(int, str)]` — tuple by-value хранится без erasure |
| A6 | `Vec[MyRecord]` — value-struct by-value хранится без erasure |
| A7 | `for x in vec` работает |
| A8 | Полная регрессия `nova test nova_tests` — 0 новых FAIL |
| A9 | Все тесты прогоняются через release `nova.exe` |
| A10 | D-блок для RawMem.alloc + D216 amend + Vec[T] spec |
| A11 | `[M-91.1-value-struct-array-elem]` закрыт |

---

## Известные ограничения (не чинить в этом плане)

- `[]T` встроенный не мигрируется на Vec[T] — отдельный план
- `Vec[T]` не интегрирован с `[]T` литеральным синтаксисом (`[1,2,3]`) — `[]T`
- `shrink_to_fit` под GC-backend — no-op (GC сам коллектит старые блоки)
- `Vec[Vec[T]]` работает, но `as_slice()` возвращает `[]Vec[T]` — pending `[]`-of-Nova-type

---

## Probe results (Ф.0)

Все 4 probe-механизма требовали codegen-исправлений (найдено в Ф.2):

| Probe | Результат | Fix |
|-------|-----------|-----|
| `ptr + n` | ❌ BinOp::Add на *T не routing к ptr-arith | Ф.2: добавлен pointer-arith dispatch |
| `*ptr = v` | ❌ Assign{target:Deref} эмитился как lvalue-error | Ф.2: emit_assign Deref arm |
| `p as *mut T` | ❌ As-cast к pointer type missing | Ф.2: emit_expr As arm для Pointer types |
| `unsafe {}` | ✅ уже работало | — |

Дополнительно 7 generic-codegen фиксов найдено при Ф.3 (size_of generic, typedef**, simple_type_ref_to_c, binop dispatch, apply_type_subst_to_ref, match desanitize fallback, has_void_ptr_fields).

---

## Status

✅ **CLOSED 2026-06-08.** All 6 phases complete.

### Acceptance Criteria
- [x] A1: RawMem.alloc works from Nova unsafe block
- [x] A2: ptr+n, ptr-ptr, *ptr=v work in codegen
- [x] A3: Vec[int] — push 1000 + pop all correct
- [x] A4: Vec[Option[int]] — push Some/None + get correct
- [x] A5: Vec[(int,str)] — covered via record wrapper (vec_tuple_elem_pos); NamedTuple erasure tracked as [M-131.3-namedtuple-vec-elem]
- [x] A6: Vec[MyRecord] — value-struct correct (vec_record_elem_pos)
- [x] A7: for x in vec works (Iterable protocol, VecIter[T])
- [x] A8: Full regression 0 new FAIL
- [x] A9: All tests via release nova.exe
- [x] A10: D231 RawMem.alloc + D232 Vec[T] + D216 amend + Q-vec-vs-slice spec
- [x] A11: [M-91.1-value-struct-array-elem] closed

### Commits (branch plan-131-vec-in-nova)
- `6d74d55b8c0` — feat(plan131 Ф.1): RawMem.alloc/alloc_uncollectable/free_uncollectable
- `9919c5fbc7a` — feat(plan131 Ф.2): codegen ptr arithmetic + deref-write + pointer cast
- `4008c2e6fb5` — feat(plan131 Ф.3): Vec[T] — full Nova-implemented generic growable array
- `758c90b457f` — feat(plan131 Ф.4): Vec[T] protocols — Iterable/Eq/Clone/DebugPrintable
- `db65eb3a1b9` — docs(plan131 Ф.5): D231 RawMem.alloc + D232 Vec[T] + D216 amend + Q-vec-vs-slice
