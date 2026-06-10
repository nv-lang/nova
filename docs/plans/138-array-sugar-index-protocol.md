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
| `v[i] = val` | `()` | `mut @index(i, val)` — panic на OOB |
| `v.get(i)` | `Option[T]` | safe |
| `v[2..5]` | `Vec[T]` | `@index(Range)` — zero-copy view, panic OOB |
| `v.get(2..5)` | `Option[Vec[T]]` | safe zero-copy view |
| `s[i]` | `char` | `@index(i)` — panic на OOB |
| `s.get(i)` | `Option[char]` | safe (было `char_at`) |
| `s[2..5]` | `str` | уже работает, panic OOB |

---

## D-блоки

- **D238 NEW** — `Index[K, V]` protocol: `@index(key K) -> V` — magic для `a[key]`
- **D240 NEW** — `MutIndex[K, V]` protocol: `mut @index(key K, val V)` — magic для `a[key] = val`
- **D241 NEW** — `Next[T]` protocol: `mut @next() -> Option[T]` — замена `Iterable[T]`
- **D242 NEW** — `Iter[I]` protocol: `@iter() -> I` — источник итератора
- **D239 NEW** — `[]T` как сахар над `Vec[T]`: type alias, literal desugaring, migration
- **D27 AMEND** — `arr[i]` теперь через `@index` protocol (не compiler built-in)
- **D144 AMEND** — sugar migration выполнена (закрыть «future language version» пункт)
- **D27/08-runtime AMEND** — `str[i]` → `char` (panic), не `Option[char]` (spec fix)
- **D58 AMEND** — `Iterable[T]` → `Next[T]` + `Iter[I]`; iter-first rule

---

## Фазы

### Ф.0 — Prerequisite: `for x in c` двухфазный + D58 amend (~2h)

**Два связанных изменения (D58 amend, Plan 138):**

#### Ф.0.1 — D241+D242: `Next[T]` + `Iter[I]` протоколы + rename + iter-first sweep

**Два новых протокола (D241, D242) по конвенции «имя = магический метод»:**

```nova
// std/prelude/collections.nv

/// Протокол итератора. Тип реализует @next() → является итератором.
/// Конвенция: названо по магическому методу, как Index, Equal, Hash.
export type Next[T] protocol {
    mut @next() -> Option[T]
}

/// Протокол источника итератора. Тип реализует @iter() → может быть
/// передан в for-in или convert в итератор.
/// I — конкретный тип итератора (реализует Next[T]).
export type Iter[I] protocol {
    @iter() -> I
}
```

**Удалить** `Iterable[T]` — заменяется на `Next[T]` (D241).

Как складывается:

| Тип | Реализует |
|---|---|
| `RangeIter` | `Next[int]` + `Iter[RangeIter]` |
| `VecIter[T]` | `Next[T]` + `Iter[VecIter[T]]` |
| `Range` | `Iter[RangeIter]` только |
| `Vec[T]` | `Iter[VecIter[T]]` только |

Generic bound для «принять любой iterable»:
```nova
fn collect[C Iter[I], I Next[T], T](c C) -> Vec[T]
```

**`@iter() -> Self` sweep** — добавить на каждый итератор в stdlib
(итератор реализует `Iter[Self]` — trivial `=> self`):
```nova
export fn RangeIter @iter() -> RangeIter => self
export fn StepRangeIter @iter() -> StepRangeIter => self
export fn ReverseRangeIter @iter() -> ReverseRangeIter => self
// + VecIter, LinesIter, KeysIter, ValuesIter и др.
```

**Правило `for x in c`** (D58 amend, iter-first):
1. Если `c` имеет `@iter()` → `_it = c.iter()` → `_it.next()` в loop
2. Иначе если `c` имеет `@next()` напрямую → backward-compat (итератор без `@iter()`)
3. Иначе — compile error

**Commit:** `feat(D241+D242): Next[T] + Iter[I] protocols; Iterable[T] removed; iter-first D58`

#### Ф.0.2 — Codegen: `NovaTuple_` prefix fix в `emit_for` ✅ DONE

**Анализ C-типов по форме Range:**

| Форма | C-тип | `strip_prefix("Nova_")` | iter_struct | Работает? |
|---|---|---|---|---|
| `type Range { ... }` (record) | `Nova_Range*` | `"Range*"` → `"Range"` | `"Range"` | ✅ |
| `type Range value { ... }` | `Nova_Range` | `"Range"` | `"Range"` | ✅ |
| `type Range(start, end)` (named tuple) | `NovaTuple_Range` | `None → ""` | `""` | ❌ |

**Value record уже работает** — `strip_prefix("Nova_")` корректно извлекает имя типа.
Compiler fix нужен **только для named tuples**.

**Проблема:** `"NovaTuple_Range".starts_with("Nova_")` = false (5-й символ `T`, не `_`)
→ `unwrap_or("")` → `iter_struct = ""` → `iter_struct.is_empty()` = true → весь
iter-dispatch пропускается → error.

**Применённый fix** (`compiler-codegen/src/codegen/emit_c.rs` ~24075):
```rust
// Plan 138 Ф.0.2: named tuples have "NovaTuple_" prefix, not "Nova_".
let iter_struct = if arr_ty.starts_with("NovaTuple_") {
    arr_ty.strip_prefix("NovaTuple_").unwrap_or("")
        .trim_end_matches('*').trim().to_string()
} else {
    arr_ty.strip_prefix("Nova_").unwrap_or("")
        .trim_end_matches('*').trim().to_string()
};
```

`all_methods` использует Nova-имена (`"Range"`, не `"NovaTuple_Range"`), поэтому
после извлечения `"Range"` lookup `all_methods.contains(("Range", "iter"))` работает ✓.

**Commit:** `fix(plan138 Ф.0.2): NovaTuple_ prefix в emit_for iter_struct extraction`

#### Ф.0.3 — Migrate `Range` → value record (~30min)

После того как Ф.0.1 sweep + Ф.0.2 fix применены и тесты PASS.

Value record выбран вместо named tuple — не требует дополнительных compiler изменений
(Ф.0.2 уже покрыл named tuples для будущих типов, но Range конкретно проще как value record).

**`std/collections/range.nv`:**
```nova
// было:
export type Range {
    ro start int
    ro end   int
}

// стало:
export type Range value {
    ro start int
    ro end   int      // half-open: end НЕ включён
}
```

Конструкторов **нет** — убрать `Range.exclusive` и `Range.inclusive` (уже удалены).
Синтаксис `a..b` и `a..=b` достаточен. `OverflowError` — убрать.

D58 spec-пример: убрать `inclusive bool` из описания Range.

**Commit:** `refactor(plan138 Ф.0.3): Range → value record; remove exclusive/inclusive ctors`

---

### Ф.1 — `Index[K, V]` + `MutIndex[K, V]` protocols + spec (~1h)

**`std/prelude/protocols.nv`** — добавить:

```nova
/// Индексный доступ `a[key]` — десугаринг `ExprKind::Index` для user-типов.
/// Паникует при invalid key (OOB, missing key etc).
///
/// Один протокол, два типичных варианта реализации:
///   `Index[int, T]`        — скалярный доступ `v[i]`
///   `Index[Range, Vec[T]]` — slice `v[2..5]` (zero-copy view)
///
/// Отдельного Slice-протокола нет: Range — просто другой тип ключа K.
/// Компилятор диспатчит по типу index-выражения.
///
/// Open-ended bounds (`arr[1..]`, `arr[..3]`, `arr[..]`) — компилятор
/// подставляет 0 / arr.len() до вызова @index; user-реализации
/// всегда получают полностью заполненный Range.
#stable(since = "0.1")
export type Index[K, V] protocol {
    @index(key K) -> V
}

/// Индексная запись `a[key] = val` — десугаринг assignment lhs.
/// Read-only типы реализуют только `Index`.
/// Мутабельные типы реализуют оба.
#stable(since = "0.1")
export type MutIndex[K, V] protocol {
    mut @index(key K, val V)
}
```

Протоколы раздельны: read-only slice реализует только `Index`,
`Vec[T]` реализует оба.

Spec D238 + D240 в `spec/decisions/03-syntax.md`.

**Commit:** `spec(D238+D240): Index[K,V] + MutIndex[K,V] protocols — @index magic`

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
/// Zero-copy view `v[from..to]` — реализует `Index[Range, Vec[T]]`.
/// Interior pointer в GC-tracked буфере Vec.
/// cap == len == to - from: push на view → realloc → silent detach от родителя.
export fn Vec[T] @index(r Range) -> Vec[T] {
    if r.start < 0 || r.end < r.start || r.end > @len {
        panic("Vec: slice [${r.start}..${r.end}] out of bounds for length ${@len}")
    }
    ro len = r.end - r.start
    // pointer arithmetic — unsafe: @data + r.start сдвигает *mut T
    unsafe { Vec[T] { data: @data + r.start, len, cap: len } }
}

/// Safe range access, `None` if out of bounds.
export fn Vec[T] @get(r Range) -> Option[Vec[T]] {
    if r.start < 0 || r.end < r.start || r.end > @len { return None }
    ro len = r.end - r.start
    Some(unsafe { Vec[T] { data: @data + r.start, len, cap: len } })
}
```

Чистая Nova-реализация: никакого C-хелпера. `unsafe` нужен только для
pointer arithmetic (`*mut T + offset`). GC видит interior pointer через
`GC_set_all_interior_pointers` — backing-буфер не собирается.

**Убрать `@as_slice()`** — заменяется на `v[..]` (zero-copy) или
`Vec[T].to_vec()` / `v.to_array()` если нужна копия. Оставить `@to_array()`:

```nova
/// Copy all elements to a fresh `[]T`. O(n).
export fn Vec[T] @to_array() -> []T { ... }   // было @as_slice()
```

**Write magic (`MutIndex`):**

```nova
/// Write `v[i] = val` — panics on OOB.
export fn Vec[T] mut @index(i int, val T) {
    if i < 0 || i >= @len {
        panic("Vec: index ${i} out of bounds for length ${@len}")
    }
    unsafe { @data[i] = val }
}
```

`str` не реализует `MutIndex` — строки immutable.

**Commit:** `feat(plan138 Ф.2): Vec[T] @index + @get + Range zero-copy + @to_array + mut @index`

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

**Ф.1-Ф.4 (CLOSED 2026-06-10, 10/10 PASS):**

- **A1** ✅ — `v[i]` на `Vec[T]` компилируется → `T`, panic на OOB (t1, neg/t_neg_vec_index_oob)
- **A1b** ✅ — `v[i] = val` на `Vec[T]` компилируется → запись, panic на OOB (t_vec_write_index)
- **A2** ✅ — `v.get(i)` → `Option[T]` (t2_vec_get_some_none)
- **A3** ✅ — `v[2..5]` → `Vec[T]` zero-copy view; push на view → silent detach, original unchanged (t3, t8)
- **A4** ✅ — `v.get(2..5)` → `Option[Vec[T]]` (t4_vec_range_get)
- **A5** ✅ — `"abc"[0]` → `'a'` (char, panic OOB) (t5, neg/t_neg_str_index_oob)
- **A6** ✅ — `"abc".get(0)` → `Option[char]` (t6_str_get_option)
- **A7** — `[T Index[int, T]]` bound принимает `Vec[T]` и `[]T` — deferred (Ф.5)
- **A8** — `[]T` = `Vec[T]` на уровне типов: `fn f(a []int)` принимает `Vec[int]` — deferred (Ф.5, `[M-138-array-sugar-alias]`)
- **A9** — 0 новых FAIL в `nova test` — targeted plan138 tests: 10/10 PASS
- **A10** ✅ — `Next[T]`/`Iter[I]` протоколы объявлены; `for x in c` через `iter()` (D58 amend, t7_for_iter_first)
- **A11** ✅ — `Iterable[T]` удалён из prelude; заменён на `Next[T]` + `Iter[I]` (D241+D242)

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
- `[M-138-mutindex-range]` — `Vec[T] mut @index(r Range, val []T)` — write по range (bulk replace)
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
| D240 NEW | `MutIndex[K,V]` protocol |
| D241 NEW | `Next[T]` protocol — замена `Iterable[T]` |
| D242 NEW | `Iter[I]` protocol — источник итератора |
| D58 AMEND | iter-first rule + `Iterable[T]` → `Next[T]` + `Iter[I]` |
