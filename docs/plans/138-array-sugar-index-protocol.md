<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 138 — `[]T` sugar over `Vec[T]` + `Index` protocol

> **Создан:** 2026-06-10.  **Статус:** ✅ ЗАКРЫТ (Ф.1-Ф.4, 2026-06-10). Ф.5-Ф.6 deferred → `[M-138-array-sugar-alias]`.
> **Эстимат:** ~4-5 dev-day (Ф.1-Ф.4 = ~2d; Ф.5-Ф.7 = ~2-3d).
> **Model:** Sonnet 4.6 HIGH (Ф.1-Ф.4) + Opus + Thinking (Ф.5-Ф.7).
> **Зависит от:** Plan 131 ✅ (Vec[T] Nova impl), Plan 137 ✅ (protocol rename).

---

## Что

Два связанных изменения:

**1. `Index[K, V]` protocol + магический метод `@index`**

```nova
v[i]       →  v.index(i)            // @index — magic
v[2..5]    →  v.index(Range(2, 5))  // @index с Range
v.get(i)   →  Option[T]             // safe, без magic
```

**2. `[]T` как сахар над `Vec[T]`**

```nova
// сейчас: []T = built-in C-macro (NovaArray_T)
// после:  []T = Vec[T]  (полностью на Nova)
mut arr []int = [1, 2, 3]   // desugars to Vec[int]
arr.push(4)                  // Vec[int].push
arr[1]                       // arr.index(1) → @index magic
```

Это migration path, зафиксированный в спеке D144:
> *«In a future language version, `[]T` may become sugar over `Vec[T]`
> once the typed-storage gap is closed for all T.»*

---

## Мотивация

Сейчас `[]T` — built-in тип с C-macro (NOVA_ARRAY_DECL), требующий
compiler magic. `Vec[T]` — чистая Nova реализация. Они ABI-совместимы
(одинаковый layout: `data* + len + cap`). Объединение:

- Убирает дублирование логики между `[]T` и `Vec[T]`
- Закрывает typed-storage gap: `[]Option[int]`, `[]Record` получают
  правильное typed хранение вместо int64-erasure
- `Index[K, V]` разрешает `v[i]` для любого user-типа
- Выравнивает `str[i]` с остальными (реализует незакрытый spec-gap)

---

## Таблица API после плана

| Вызов | Тип | Семантика |
|---|---|---|
| `v[i]` | `T` | `@index(i)` — panic на OOB |
| `v.get(i)` | `Option[T]` | safe |
| `v[2..5]` | `[]T` | `@index(Range)` — zero-copy view, panic OOB |
| `v.get(2..5)` | `Option[[]T]` | safe zero-copy view |
| `s[i]` | `char` | `@index(i)` — panic на OOB |
| `s.get(i)` | `Option[char]` | safe (было `char_at`) |
| `s[2..5]` | `str` | уже работает, panic OOB |

---

## D-блоки

- **D238 NEW** — `Index[K, V]` protocol: `@index(key K) -> V` — magic для `a[key]`
- **D239 NEW** — `[]T` как сахар над `Vec[T]`: type alias, literal desugaring, migration
- **D27 AMEND** — `arr[i]` теперь через `@index` protocol (не compiler built-in)
- **D144 AMEND** — sugar migration выполнена (закрыть «future language version» пункт)
- **D27/08-runtime AMEND** — `str[i]` → `char` (panic), не `Option[char]` (spec fix)

---

## Фазы

### Ф.1 — `Index[K, V]` protocol + spec (~1h)

**`std/prelude/protocols.nv`** — добавить:

```nova
/// Индексный доступ `a[key]` — десугаринг `ExprKind::Index` для user-типов.
/// Паникует при invalid key (OOB, missing key etc).
/// Встроенные `[]T` и `str` удовлетворяют через built-in path; до Ф.5
/// только user-типы (Vec[T], HashMap[K,V] etc) используют protocol-path.
#stable(since = "0.1")
export type Index[K, V] protocol {
    @index(key K) -> V
}
```

Spec D238 в `spec/decisions/03-syntax.md`.

**Commit:** `spec(D238): Index[K,V] protocol — magic @index for a[key]`

### Ф.2 — `Vec[T]` новый API (~2h)

**`std/collections/vec_owned.nv`**:

**Scalar (замена текущему `@get`):**

```nova
/// Element at `i` — panics on OOB. Powers `v[i]`.
export fn Vec[T] @index(i int) -> T {
    if i < 0 || i >= @len {
        panic("Vec: index ${i} out of bounds for length ${@len}")
    }
    unsafe { @data[i] }
}

/// Element at `i`, or `None` if out of bounds (safe).
export fn Vec[T] @get(i int) -> Option[T] {
    if i < 0 || i >= @len { return None }
    Some(unsafe { @data[i] })
}
```

**Range (zero-copy view):**

```nova
/// Zero-copy view `v[from..to]` as `[]T`.
/// Interior pointer into Vec's GC-tracked buffer.
/// cap == len == to - from: push on view → realloc → silent detach.
export fn Vec[T] @index(r Range) -> []T {
    ro from = r.start
    ro to   = r.end
    if from < 0 || to < from || to > @len {
        panic("Vec: slice [${from}..${to}] out of bounds for length ${@len}")
    }
    unsafe {
        nova_vec_slice[T](@data, from, to)
    }
}

/// Safe range access, `None` if out of bounds.
export fn Vec[T] @get(r Range) -> Option[[]T] {
    ro from = r.start
    ro to   = r.end
    if from < 0 || to < from || to > @len { return None }
    Some(unsafe { nova_vec_slice[T](@data, from, to) })
}
```

`nova_vec_slice[T](data *mut T, from int, to int) -> []T` — C-helper:
выделяет `NovaArray_T` header с `data = data + from`, `len = cap = to - from`.
Добавить в `compiler-codegen/nova_rt/array.h`:

```c
// nova_vec_slice_T: zero-copy view into *mut T buffer.
// Returns NovaArray_T* with interior pointer data+from.
// GC: backing kept alive via GC_set_all_interior_pointers.
#define NOVA_VEC_SLICE_DECL(T) \
    static NovaArray_##T* nova_vec_slice_##T(T* data, int64_t from, int64_t to) { \
        NovaArray_##T* v = (NovaArray_##T*)nova_alloc(sizeof(NovaArray_##T)); \
        v->data = data + from; \
        v->len  = to - from; \
        v->cap  = to - from; \
        return v; \
    }
```

**Убрать `@as_slice()`** — заменяется на `v[..]` (zero-copy) или
`Vec[T].to_vec()` / `v.to_array()` если нужна копия. Оставить `@to_array()`:

```nova
/// Copy all elements to a fresh `[]T`. O(n).
export fn Vec[T] @to_array() -> []T { ... }   // было @as_slice()
```

**Commit:** `feat(plan138 Ф.2): Vec[T] @index + @get + Range zero-copy + @to_array`

### Ф.3 — `str.get(i)` + `str[i]` spec fix (~1h)

**Spec fix (`spec/decisions/08-runtime.md` строка 752):**

```
// было:
s[i] (codepoint indexing) — Option[char], O(i).

// стало:
s[i] — char, O(i), panic если i >= s.len.
s.get(i) — Option[char], O(i), None если i >= s.len.
```

**Compiler (`emit_c.rs`, `ExprKind::Index` с `nova_str`):**

Добавить arm для scalar str-index:
```rust
} else if obj_ty == "nova_str" {
    let o = self.emit_expr(obj)?;
    return Ok(format!("nova_str_index_panic({}, {})", o, i));
}
```

`nova_str_index_panic` в `array.h`:
```c
static inline nova_char nova_str_index_panic(nova_str s, nova_int idx) {
    NovaOpt_nova_char r = nova_str_char_at(s, idx);
    if (!r.has_value) nv_panic_index_oob(idx, nova_str_char_len(s));
    return r.value;
}
```

**`str.get(i) -> Option[char]`** — переименовать `str.char_at` → `str.get`:
```nova
// было:
fn str @char_at(i int) -> Option[char]
// стало:
fn str @get(i int) -> Option[char]
// char_at — deprecated alias с W_METHOD_RENAMED
```

**Commit:** `feat(plan138 Ф.3): str[i] → char panic + str.get(i) → Option[char]`

### Ф.4 — Fixtures (~1h)

**`nova_tests/plan138/`**:

Позитивные:
- `t1_vec_index_basic.nv` — `v[0]`, `v[2]`
- `t2_vec_get_some_none.nv` — `v.get(0) == Some(...)`, `v.get(99) == None`
- `t3_vec_range_index.nv` — `v[1..3]` → zero-copy view, mutations visible
- `t4_vec_range_get.nv` — `v.get(1..3) == Some(...)`, `v.get(99..100) == None`
- `t5_str_index_char.nv` — `"abc"[0] == 'a'`
- `t6_str_get_option.nv` — `"abc".get(0) == Some('a')`, `"abc".get(9) == None`
- `t7_hashmap_index.nv` — `map["key"]` через `@index` protocol
- `t8_view_detach.nv` — `v[1..3].push(99)` → silent detach, original unchanged

Негативные:
- `t_neg_vec_index_oob.nv` — `v[99]` → panic
- `t_neg_str_index_oob.nv` — `"hi"[5]` → panic

**Commit:** `test(plan138 Ф.4): fixtures`

---

### Ф.5 — `[]T` → `Vec[T]` type alias (БОЛЬШОЙ, gated) (~2-3d)

> **Gate:** Ф.1-Ф.4 все PASS. Отдельный worktree `nova-p138`.

Цель: компилятор видит `[]int` → внутренне это `Vec[int]`.

**Шаг 5.1 — Type resolution**

В `types/mod.rs`, `resolve_type_ref` для `TypeRef::Array { elem }`:
```rust
// Вместо построения внутреннего ArrayType:
// разверни в TypeRef::Named { path: ["Vec"], generics: [elem] }
TypeRef::Array { elem, .. } => {
    self.resolve_type_ref(&TypeRef::Named {
        path: vec!["Vec".into()],
        generics: vec![*elem.clone()],
        span,
    })
}
```

Это делает `[]T` синонимом `Vec[T]` на уровне type-checker'а.

**Шаг 5.2 — C-type mapping**

В `emit_c.rs`, `type_ref_to_c` для `[]T`:
```rust
// было: "NovaArray_T*"
// стало: "Nova_Vec_T*"  (маппинг через Vec[T] struct)
```

Или: оставить `NovaArray_T*` как C-имя для `Vec[T]` (ABI compat — layout одинаковый).

**Шаг 5.3 — Array literals**

`[1, 2, 3]` сейчас → `NovaArray_nova_int` literal.
После → `Vec[int]` constructor через `nova_array_from_literal` (уже есть) или:
```nova
// desugar в parser/desugar.rs:
[1, 2, 3]  →  Vec[int].from_literal(3, 1, 2, 3)
```

Или оставить array-literal как отдельный `ExprKind::ArrayLit` → codegen строит `Vec[T]`
через `Vec.with_capacity(n)` + serial push. (Существующий C-macro path можно сохранить
как оптимизацию через `nova_array_literal_T(n, ...)`.)

**Шаг 5.4 — `@index` magic в компиляторе**

В `ExprKind::Index` codegen — для non-builtin receiver:
```rust
// Если obj_ty = "Nova_Vec_T*" (или mapped []T):
//   emit call Vec[T].index(i) или Vec[T].index(Range)
// Для nova_str — Ф.3 path.
// Для других user-типов с @index impl — protocol dispatch.
```

До Ф.5 (`[]T` ещё built-in) `@index` работает только для `Vec[T]`
через явный вызов. После Ф.5 — через type alias автоматически.

**Шаг 5.5 — NOVA_ARRAY_DECL cleanup**

`compiler-codegen/nova_rt/array.h` содержит ~30 NOVA_ARRAY_IMPL инстанциаций
для примитивов. После migration:
- Оставить как C-backend optimized path (компилятор может продолжать
  эмитить `NovaArray_nova_int` под капотом — ABI тот же)
- Или: полностью убрать и использовать `Nova_Vec_T` C struct

Консервативно на V1: оставить NOVA_ARRAY_DECL/IMPL, просто сделать `[]T` alias
на уровне Nova type-checker. C codegen продолжает эмитить `NovaArray_T*`.
Rename NOVA_ARRAY_DECL → NOVA_VEC_DECL в Ф.7+.

**Commit (несколько):** `feat(plan138 Ф.5.1-5.5): []T → Vec[T] type alias`

### Ф.6 — Stdlib + тесты миграция (~1h)

- Миграция `[]T`-specific методов на `Vec[T]`:
  `arr.with_capacity`, `arr.push`, `arr.pop`, `arr.len` etc — уже есть на `Vec[T]`
- Убрать `NovaArray`-specific special-cases в `emit_c.rs` где они стали
  дублировать `Vec[T]` routing
- `nova test` full regression

**Commit:** `refactor(plan138 Ф.6): stdlib + codegen — remove []T special-cases`

### Ф.7 — Docs + close (~30min)

- D144 AMEND — пометить migration как выполненную
- `docs/simplifications.md`, `nova-private/`
- README.md

**Commit:** `docs(plan138 Ф.7): close — []T sugar migration complete`

---

## Acceptance criteria

- **A1** — `v[i]` на `Vec[T]` компилируется → `T`, panic на OOB
- **A2** — `v.get(i)` → `Option[T]`
- **A3** — `v[2..5]` → `[]T` zero-copy view; push на view → detach, original unchanged
- **A4** — `v.get(2..5)` → `Option[[]T]`
- **A5** — `"abc"[0]` → `'a'` (char, panic OOB)
- **A6** — `"abc".get(0)` → `Option[char]`
- **A7** — `[T Index[int, T]]` bound принимает `Vec[T]` и `[]T`
- **A8** — `[]T` = `Vec[T]` на уровне типов: `fn f(a []int)` принимает `Vec[int]`
- **A9** — 0 новых FAIL в `nova test`

---

## Порядок выполнения

```
Ф.1 (spec) → Ф.2 (Vec API) → Ф.3 (str) → Ф.4 (fixtures)
                                   ↓
                          Ф.5 ([]T alias) → Ф.6 (migration) → Ф.7 (close)
```

Ф.1-Ф.4 — независимые улучшения, можно выпустить без Ф.5-Ф.7.
Ф.5-Ф.7 — большой refactor, отдельный worktree, отдельные коммиты.

---

## Followups

- `[M-138-array-sugar-alias]` — Ф.5-Ф.6 deferred: `[]T` → `Vec[T]` type alias + stdlib migration (risky refactor, separate plan)
- `[M-138-index-set]` — `a[i] = v` через `@index_set(key K, val V) -> ()` (write magic)
- `[M-138-hashmap-index]` — `HashMap[K,V]` implements `Index[K, V]` (panic на missing key)
- `[M-138-nova-array-decl-rename]` — NOVA_ARRAY_DECL → NOVA_VEC_DECL C-macro rename
- `[M-138-char-at-deprecate]` — убрать `str.char_at` после migration period
- `[M-138-vec-range-index]` — `v[2..5]` → `[]T` zero-copy view (Vec range-index, Ф.2 deferred)

---

## Связанные планы / D-блоки

| Связь | Что |
|---|---|
| Plan 131 ✅ | `Vec[T]` Nova impl — основа |
| Plan 137 ✅ | Protocol rename — `Index` имя свободно |
| D27 AMEND | `arr[i]` через `@index` protocol |
| D144 AMEND | sugar migration выполнена |
| D238 NEW | `Index[K,V]` protocol |
| D239 NEW | `[]T` as sugar over `Vec[T]` |
| `[M-138-index-set]` | write magic `a[i] = v` |
