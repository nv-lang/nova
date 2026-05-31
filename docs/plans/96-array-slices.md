// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 96 — sub-slice views для `[]T` (production-grade, paritет/лучше Go/Rust/TS)

> **Статус:** ✅ **ЗАКРЫТ 2026-05-23** (Ф.1-Ф.7 на ветке `plan-96`;
> regression 1081+ PASS / 0 FAIL; 23 теста plan96). Lint
> `W_VIEW_PUSH_DETACH` отложен ([P-plan96-lint-deferred] в
> simplifications.md) — push-detach **поведение** работает корректно,
> lint — добавление warning без блокировки функционала.
> **Приоритет:** P2 (закрывает Q-array-slicing + D27 §1663 drift + D27
> bounds-check drift; разблокирует stdlib алгоритмы и убирает offset-
> параметры Plan 90)
> **Оценка:** ~7–8 dev-day (язык + рантайм + codegen + GC-verify + lint +
> ~35 тестов + spec)
> **Зависимости:** Range-литералы D58 ✅; Plan 90 bulk-ops ✅; D6
> non-moving GC ✅; `GC_set_all_interior_pointers(1)` runtime ✅;
> `Nova_RuntimeError_IndexOutOfBounds` runtime ✅ (но не wired в codegen — Ф.1);
> Plan 56 vtable / Plan 48 mono ✅ (понимание pipeline'а).
> **Закрывает:** `Q-array-slicing` (`spec/open-questions.md:2161` /
> `Q-array-api.5`), `D27 §1663` drift («Слайсинг отложен») —
> `spec/decisions/03-syntax.md:1663`, **D27 OOB-panic drift** —
> codegen `arr[i]` не эмитит panic (`emit_c.rs:12910-12974` vs spec
> `03-syntax.md:1632`).
> **Решения приняты ДО старта (не GATE-defer):** см. §Решения.
> **Источник:** обсуждение 2026-05-23 + аудит «с чистого листа».

## Зачем

У `[]T` **нет sub-slice views**. Это:
1. Заставляет ВСЕ bulk-операции тащить offset-параметры (`copy_from(src,
   from, to)` вместо `dst.copy_from(src[k..k+n])`) — Plan 90 чуть не
   принял эту форму, потому что альтернативы не было.
2. Заставляет алгоритмы аллоцировать копии или передавать пары `(arr,
   from, to)` через стек вызовов.
3. Делает Nova **строго беднее** Go/Rust/Python/TS — у всех четырёх
   есть дешёвые sub-slice операции.

### Real-world use cases (примеры, которые сейчас режутся)

```nova
\ (a) Network parser — header + payload
fn parse_packet(buf []u8) Fail[ParseError] -> Packet
    ro header = buf[0..20]                  \ view, no copy
    ro payload = buf[20..]                  \ open-ended view
    Packet { header: parse_header(header), payload: parse_body(payload) }

\ (b) HMAC inner-pad (RFC 2104)
fn hmac_init(key []u8) -> Hmac
    var ipad []u8 = with_capacity(64)
    ipad.copy_from(key[0..min(key.len(), 64)])    \ slice key prefix
    \ ... xor with 0x36 ...

\ (c) Two-pointer (quicksort partition)
fn partition[T Ord](arr mut []T, lo int, hi int) -> int
    ro pivot = arr[hi - 1]
    var i = lo
    for j in lo..hi-1
        if arr[j] <= pivot
            arr.swap(i, j)
            i = i + 1
    arr.swap(i, hi - 1)
    i

\ (d) Parser stack — remaining tokens
fn parse_block(tokens []Token, cursor int) -> (Ast, int)
    ro remaining = tokens[cursor..]         \ view хвоста
    ro (ast, consumed) = parse_expr(remaining)
    (ast, cursor + consumed)
```

Без slice — все четыре примера требуют либо offset-параметров (раздувают
API), либо копий (медленно).

## Production-rigor сравнение Go / Rust / TS / Swift / Python

### Header layout и тип-идентичность

| Язык | Тип view | Header (x64) | Owner ≠ view? | Aliasing model |
|---|---|---|---|---|
| **Go** | `[]T` | **24 байта**: `ptr (8) + len (8) + cap (8)` | НЕТ — один `[]T` | shared backing; `append` может молча писать в чужой backing если `len < cap` |
| **Rust** | `&[T]` / `&mut [T]` | **16 байт**: `ptr (8) + len (8)` (fat pointer) | ДА — `Vec<T>` ≠ `&[T]` ≠ `&mut [T]` | borrow checker compile-time: N immut XOR 1 mut |
| **TS TypedArray** | `Uint8Array` | ~24-32 (engine internal): `buffer + byteOffset + length` | `subarray()`→view, `slice()`→copy (разные методы!) | shared `ArrayBuffer`; `SharedArrayBuffer` для workers требует `Atomics` |
| **Swift** | `ArraySlice<T>` | ~16-24 (rc + bounds) | ДА — `Array<T>` ≠ `ArraySlice<T>` | CoW: mut view копирует backing если refcount > 1 → mutation **НЕ видна** в original |
| **Python** | `memoryview` (view) / `list[a:b]` (copy) | C struct ~192 байт (Py_buffer + strides + ndim) | ДА — `memoryview` ≠ `list` | mut через memoryview видна; list slice всегда copy |
| **Nova (текущее)** | НЕТ | — | — | — |
| **Nova (Plan 96)** | `[]T` тот же тип, view с `cap == len` | **24 байта** (тот же header) | НЕТ — один `[]T` | shared backing на read/write; `push` → realloc → silent detach |

### Open-ended формы диапазона

| Язык | `a..b` | `a..=b` | `a..` | `..b` | `..` |
|---|---|---|---|---|---|
| Rust (`RangeBounds` trait) | ✅ `Range` | ✅ `RangeInclusive` | ✅ `RangeFrom` | ✅ `RangeTo` | ✅ `RangeFull` |
| Go | ✅ `s[a:b]` | ❌ | ✅ `s[a:]` | ✅ `s[:b]` | ✅ `s[:]` |
| Swift | ✅ `a..<b` | ✅ `a...b` | ✅ `a...` | ✅ `...b` (suffix) | — |
| Python | ✅ `a:b` | ❌ (нет inclusive) | ✅ `a:` | ✅ `:b` | ✅ `:` |
| TS | ❌ (методы с числами) | — | — | — | — |
| Nova (сейчас) | ✅ | ✅ | ❌ | ❌ | ❌ |
| Nova (цель) | ✅ | ✅ | ✅ | ✅ | ✅ (для срезов; для for-loop spec'у не нужен) |

### Iterator invalidation contract

| Язык | `for x in slice { mutate parent }` | Гарантия |
|---|---|---|
| Go | компилируется; длина snapshot на момент start; новые элементы НЕ видны; реалок ломает backing-pointer для старых slice'ов | undefined visibility, defined no-crash (refcount-free) |
| Rust | **не компилируется** (borrow conflict E0499) | static prevention |
| TS | компилируется; behavior зависит от метода (`Array.prototype.push` invalidates, `Object.freeze` блокирует) | runtime unpredictable |
| Swift | компилируется; CoW копирует на mut → original НЕ виден slice'у | silently disconnected |
| Python | `for x in lst: lst.append(x)` — бесконечный цикл | defined (but surprising) |
| Nova (цель) | компилируется; for-loop snapshot len в начале (как Go); push на parent реаллокнёт parent, view продолжит держать **старый** backing через interior-ptr (Boehm); push на view → детач (см. §«Push-detach») | defined-and-documented |

### Production footguns (примеры)

**Go — `append`-aliasing** ([HN #9555472](https://news.ycombinator.com/item?id=9555472), [рекомендация 3-index slice](https://github.com/golang/go/issues/25638)):
```go
func bad(opts []string, extras ...string) []string {
    return append(opts, extras...)
    // Если cap(opts) > len(opts), append пишет в backing CALLER'а!
    // Fix: append(opts[:len(opts):len(opts)], extras...) — 3-index slice cuts cap
}
```

**TS — `subarray` vs `slice` confusion** ([Node.js #28087](https://github.com/nodejs/node/issues/28087)):
```js
const u8 = new Uint8Array([1,2,3,4]);
u8.subarray(1, 3)[0] = 99;  // u8[1] === 99 — view
u8.slice(1, 3)[0]    = 99;  // u8[1] === 2  — copy
// Node.js Buffer.prototype.slice исторически был view (legacy semantics)
// Uint8Array.prototype.slice — стандарт, copy
// → один и тот же `.slice()` имеет РАЗНУЮ семантику на разных типах
```

**Swift — CoW disconnect** ([Swift Forums #17158](https://forums.swift.org/t/17158)):
```swift
var arr = [1, 2, 3, 4]
var slice = arr[1...2]      // slice.startIndex == 1 (НЕ 0!)
slice[1] = 99               // CoW копирует; arr НЕ затронут
print(arr)                  // [1, 2, 3, 4]
print(slice[0])             // crash: Index out of range
```

**Python — `memoryview` не освобождён**:
```python
mv = memoryview(some_buffer)[1:4]
# забыли mv.release() → buffer protocol зависает; some_buffer не освобождается
```

### Nova-цель — позиция между языками

| Свойство | Nova (Plan 96) |
|---|---|
| Single type `[]T` (Go-стиль) | ✅ |
| View shares backing на R/W | ✅ |
| Mut view propagates to backing | ✅ (как Go) |
| **`append`/`push` НЕ молча пишет в чужой backing** (Go-footgun устранён) | ✅ через `cap == len` → push всегда реаллокает |
| Borrow checker для concurrent mut+immut | ❌ (нет в Nova; D79 disclaimer применяется) |
| `RangeBounds`-семейство (5 форм) | ✅ |
| O(1) slice creation | ✅ (24-byte header + ptr+arithmetic) |
| Iterator invalidation defined-and-documented | ✅ (Ф.0 §D-iter-snap) |

Nova: **≥ Go** (без append-footgun), **≥ TS** (нет двусмысленности `subarray`/`slice`), **≥ Swift** (нет CoW-disconnect), **≤ Rust** (нет статической aliasing-prevention).

## Привязка к коду (сверено аудитом 2026-05-23)

### Текущее представление `[]T`

- **Header** — [nova_rt/array.h:47-56](compiler-codegen/nova_rt/array.h#L47-L56):
  `NovaArray_##T { T* data; int64_t len; int64_t cap; }` = **24 байта**,
  alignment 8. Heap через `nova_alloc` (Boehm `GC_malloc`).
- **Mono-инстансы**: `NOVA_ARRAY_DECL` + `NOVA_ARRAY_IMPL` per element-type
  (line 122-176): `nova_int`, `nova_char`, `nova_byte`, `nova_bool`,
  `nova_f64`, `nova_f32`, `int{8,16,32}`, `uint{8,16,32,64}`, `nova_str`,
  `void_p`. Аллокация трекается в `emit_c.rs` через
  `array_element_types: HashMap<String, String>`.

### Текущая индексация (BUG: нет bounds-check)

[emit_c.rs:12910-12974](compiler-codegen/src/codegen/emit_c.rs#L12910-L12974):
```rust
ExprKind::Index { obj, index } => {
    let obj_ty = self.infer_expr_c_type(obj);
    let i = self.emit_expr(index)?;        // unconditional eval-as-int
    if obj_ty.starts_with("NovaArray_") {
        ...
        Ok(format!("({})->data[{}]", o, i))   // raw C indexing, no bounds-check
    }
}
```

- **Нет dispatch по типу index'а** (Range не отличается от int).
- **Нет runtime bounds-check** — controlled buffer overflow на запись,
  UB на чтение. **Противоречит D27 §1632** «panic при OOB».
- Runtime для panic'а уже есть: `Nova_RuntimeError_IndexOutOfBounds`
  ([array.h:836](compiler-codegen/nova_rt/array.h#L836)).
- → **Plan 96 Ф.1 чинит этот drift до того, как добавит slice**
  (иначе slice-OOB-panic будет асимметричен сильнее raw `[i]`).

### Range-литералы — only closed-form, нет open-ended

- Парсер: [parser/mod.rs:3777-3795](compiler-codegen/src/parser/mod.rs#L3777-L3795)
  — `parse_range` требует **оба** операнда; `a..` / `..b` / `..` не
  парсятся.
- AST: [ast/mod.rs:1360-1364](compiler-codegen/src/ast/mod.rs#L1360-L1364)
  — `ExprKind::Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool }`.
  Поля `Box<Expr>` (не `Option`) → AST-change для open-ended.
- Семантика D58: `Range { start int, end int, inclusive bool }`,
  закрытый record.
- Materialize: 90% usage через `for-in` inline (emit_c.rs:16882-16912);
  только при передаче в функцию материализуется (emit_c.rs:12251).

### `str.slice` — для контраста

- [nova_rt.h:126-154](compiler-codegen/nova_rt/nova_rt.h#L126-L154)
  `nova_str_slice` — **view, не копия** (`s.ptr + byte_from`).
- **Расходится с D27**: OOB → **clamp**, не panic. (`from < 0` → 0;
  `to > total` → total). **Inconsistency с slice-OOB Plan 96 (panic).**
  → Plan 96 фиксирует drift документально (см. §Не входит); реальное
  выравнивание `str.slice` → Plan 94 (`str` algorithms on Nova).

### GC — feasibility

- `GC_set_all_interior_pointers(1)` ВКЛЮЧЁН:
  [alloc_boehm.c:77](compiler-codegen/nova_rt/alloc_boehm.c#L77),
  [gc_test_helpers.c:63,102](compiler-codegen/nova_rt/gc_test_helpers.c#L63).
- Non-moving GC закреплён в [D6](spec/decisions/05-memory.md#d6)
  (line 32). **Spec не вербализует «interior pointers stable»** — Plan
  96 Ф.10 это amend'ит явно (необходимое условие slice-views).
- Slice-view: `data = backing->data + a` — Boehm держит backing alive
  по interior-ptr. ✅ feasible без runtime-changes.

### Codegen pipeline для индексации

- `ExprKind::Index`: главная точка emit_c.rs:12910-12974;
  ghost-detector 9734; ident-collector 5647.
- `infer_expr_c_type` для `Index`: **~10 точек** в emit_c.rs (line
  22344-22352, 22641 и др.) — все возвращают element-type `T`, нужно
  расширить до dispatch (index=int → `T`, index=Range → `NovaArray_T*`).
- Tracking maps требующие propagation:
  - `array_element_types: HashMap<String, String>` (var → element type) —
    slice-var должен унаследовать значение parent'а.
  - `str_box_arrays: HashSet<String>` — slice array-of-str должен унаследовать.
- Type-checker [types/mod.rs](compiler-codegen/src/types/mod.rs) —
  14 точек обработки `ExprKind::Range`; ни одна не возвращает
  `Range`-as-index для `Index`-узла.

### Stdlib методы `[]T` (актуально для §Interop)

| Метод | Где |
|---|---|
| `len()` / `capacity()` / `is_empty()` | inline (emit_c.rs:14926-14938) |
| `get(i)` → `Option[T]` | `nova_array_get_<T>` (array.h:78) |
| `push(v)` | `nova_array_push_<T>` (array.h:68) — **realloc, ключевая точка для detach** |
| `pop()` → `Option[T]` | `nova_array_pop_<T>` (array.h:84) |
| `copy_from`/`copy_within`/`fill` (Plan 90) | `nova_array_*_<T>` (array.h:95-108) |
| `compare` (Plan 90) | `nova_array_compare_nova_byte` (только `[]u8`) (array.h:138) |
| `map`/`filter`/`fold`/`any`/`all`/`first`/`last` | `std/collections/vec.nv` Nova-функции |

### M:N — D79 cross-ref (критично)

- [D79](spec/decisions/06-concurrency.md#d79) line 1316-1319: «shared `mut`
  через захваты — ⚠️ undefined behavior в preemptive runtime, разрешён
  только в D71 single-threaded bootstrap».
- **Slice-view = shared mut backing между fiber'ами** в M:N = формально
  UB по D79. Plan 96 явно cross-ref'ит D79 (вместо умолчания) и
  фиксирует правило: «view inherits D79 disclaimer; передача view через
  `Channel[]T]` или spawn-capture в M:N — UB» (см. §Решения D-fiber).
- В D71 single-thread bootstrap — OK по факту.

## Решения (приняты ДО старта, не GATE-defer)

| # | Решение | Обоснование |
|---|---|---|
| **D-single-type** | Single `[]T` для owner и view (НЕ Rust-стиль 2 типов) | Минимум новых концепций; матчит Go; вместе с `cap == len` (D-cap-len) избегает Go-footgun без введения borrow checker'а |
| **D-cap-len** | View имеет `cap == len == b - a` (без запаса) | Push на view → реаллок всегда → silent detach; родительский backing никогда не молча перезаписывается (Go-footgun устранён) |
| **D-open-ended** | Поддерживаем `a..b`, `a..=b`, `a..`, `..b`, `..` (5 форм Rust `RangeBounds`) | Реальный production-usage (HMAC ipad, parser tail, prefix/suffix); матчит Rust/Go/Python; AST требует `start: Option<Box<Expr>>` / `end: Option<Box<Expr>>` — breaking AST change, но additive для существующих программ (closed-form парсится без изменений) |
| **D-mut-rule** | `mut`-view только от `mut`-источника. Множественные `mut`-view одного backing'а **разрешены** (как Go) — caller responsibility | Borrow checker'а нет; static prevention сломала бы Plan 49 cancel-routing и др. использования; runtime safe (single-thread bootstrap); в M:N — D-fiber disclaimer |
| **D-fiber** | Передача view через `Channel`/spawn-capture в M:N — наследует D79 disclaimer (UB); в D71 single-thread — OK | Не вводим compile-time check (его нет ни для backing); enforcement = тот же, что у raw `[]T mut` |
| **D-iter-snap** | `for x in view` — snapshot `len` в начале (как Go); push на parent во время итерации не виден view'у (старый backing держит interior-ptr) | Defined behaviour; не silent UB; матчит Go iter-semantics |
| **D-push-detach** | `mut`-view `.push(x)` → realloc → детач от parent backing'а. Lint **`W_VIEW_PUSH_DETACH`** (warning) для типичных паттернов | Без footgun-silent-detach (lint предупреждает); error был бы breaking для legitimate use; warning приемлемо |
| **D-neg-panic** | Отрицательные индексы / `a > b` / `b > len` → `nv_panic` (D13). НЕТ Python-style wrap | Согласовано с Go/Rust; матчит D27 §1632 |
| **D-empty-ok** | `arr[a..a]` валиден (empty slice); `arr[len..len]` валиден; `arr[0..0]` валиден | Стандарт Go/Rust |
| **D-header** | Slice header — те же 24 байта (`ptr + len + cap`) что у owner | Single-type-design требует; 16-байтная оптимизация = два типа (отвергнуто D-single-type); зафиксировать как future-non-portable optimization |
| **D-bounds-check** | `arr[i]` raw indexing получает bounds-check как часть Plan 96 (Ф.1) — починка D27 §1632 drift | Иначе slice-OOB-panic асимметричен с raw-no-check; не имеет смысла добавлять safety только в slice |
| **D-str-slice** | `s[a..b]` для `str` (codepoint-indexed, как существующий `nova_str_slice`); **panic на OOB** (consistent с `arr[a..b]`). Старый `s.slice(a, b)` сохраняется (backwards-compat, clamp-семантика); align→panic откладывается в Plan 94 | Bracket-форма унифицирует idiom (`arr[a..b]` ≡ `str[a..b]`); runtime уже умеет codepoint-slice (line 153) — нужен только новый emit-path + wrapper с panic вместо clamp; убирает str↔arr inconsistency, ради которой раньше пришлось бы делать Plan 94 |
| **D-cap-pow2** | Подтверждено аудитом: `cap == len` НЕ нарушает invariant'ов кода. `NovaArray_T` стартует с `cap = 8` и удваивает (`cap * 2`), но это **happy default**, не required invariant. View с `cap = 7` → push → realloc до 14 — doubling сохраняется, amortized O(1) ОК. Power-of-2 enforce'ится только в `deque.h` (Chase-Lev work-stealing, Plan 44.5) — другая структура, slice её не касается | Проверено [array.h:63,70](compiler-codegen/nova_rt/array.h#L63) + [deque.h:43-44](compiler-codegen/nova_rt/deque.h#L43-L44) |
| **D-D-number** | Берём **D144** (D142+D143 за Plan 97) | Согласовано с авдитом max-D-block = D141 |

## Scope

**Входит:**

*Pre-existing fix (D-bounds-check):*
- Bounds-check в codegen `Index`-узле для int-индекса: эмитить
  `nv_panic` через `Nova_RuntimeError_IndexOutOfBounds` при `i < 0 ||
  i >= len`. Закрывает D27 §1632 drift.

*Range — open-ended (D-open-ended):*
- AST: `ExprKind::Range { start: Option<Box<Expr>>, end: Option<Box<Expr>>,
  inclusive: bool }`.
- Парсер: `..`, `a..`, `..b` валидны как Range-expressions.
- Type-checker: open-ended Range в slice-position подставляет границы
  (`a..` → `start=a, end=len`; `..b` → `start=0, end=b`; `..` →
  `start=0, end=len`).
- Validation: open-ended Range вне slice-position (`for-in`, materialize)
  — compile-error «open-ended Range requires bounded context (use in
  slice index or provide explicit bounds)».

*Slice для `str` (D-str-slice):*
- Синтаксис: `s[a..b]` / `s[a..=b]` / `s[a..]` / `s[..b]` / `s[..]` —
  codepoint-indexed view (same semantics как `nova_str_slice`).
- Runtime: `nova_str_slice_panic(s, from, to)` — то же что
  `nova_str_slice`, но panic на OOB вместо clamp.
- Type-checker dispatch: `str[int]` → `char` (если уже есть; иначе
  отдельный followup); `str[Range]` → `str`. Если char-indexing ещё
  не реализован — `str[int]` остаётся compile-error, slice добавляется
  как чисто additive фича.
- Старый `s.slice(a, b)` — **сохраняется** (clamp-семантика, backwards-
  compat); align на panic — Plan 94.

*Slice для `[]T` (D-single-type, D-cap-len, D-mut-rule, D-iter-snap,
D-empty-ok, D-neg-panic, D-header):*
- Синтаксис: `arr[range_expr]` где `range_expr : Range`.
- Type-checker: dispatch по типу index'а — `[]T[int]` → `T`,
  `[]T[Range]` → `[]T`. Slice от slice работает (`arr[a..b][c..d]`).
- Runtime: `nova_array_slice_<T>` — bounds-check, panic при OOB / a>b,
  возвращает new `NovaArray_##T` с `data = orig->data + from`,
  `len = cap = to - from`.
- Codegen: dispatch в emit-site `Index`; propagation `array_element_types`
  и `str_box_arrays` для slice-var.
- mut-vs-immut: `mut`-view только от `mut`-источника; mut-write идёт в
  shared backing.

*Push-detach + lint (D-push-detach):*
- `push` на view: runtime реаллоцирует (как обычно для `cap == len`)
  — view'а детачится, parent НЕ затронут.
- Lint `W_VIEW_PUSH_DETACH` в type-checker: при `var v = arr[a..b]; v.push(x)`
  — warning «mut view's push detaches from parent backing; parent NOT
  modified. Use parent directly to grow, or `Array.from(view)` to make
  the detach explicit».

*Interop:*
- `for x in view` — работает (D-iter-snap).
- `view.copy_from(src)` / `view.copy_within(...)` / `view.fill(...)` /
  `view.compare(other)` (Plan 90) — работают (view это `[]T`).
- `view.len()` / `view.capacity()` / `view.is_empty()` — работают
  (capacity == len для view).
- `view.iter()` → `Iter[T]` (D58 implicit-iter если для `[]T` уже есть;
  inherit). Для view inherits.
- Передача view в `fn f(arr []T)` — работает (same type).
- Передача view в `fn f(arr mut []T)` — работает; mut-write идёт в shared
  backing.

*Spec:*
- Новый **D144** (`02-types.md`): семантика slice-view для `[]T`
  (см. таблицу решений выше).
- Amend **D27** (`03-syntax.md`): убрать «Слайсинг отложен» (§1663);
  ссылка на D144. Подтвердить bounds-check на `arr[i]`.
- Amend **D58** (`03-syntax.md`): open-ended Range формы; правило
  «open-ended требует bounded context».
- Amend **D6** (`05-memory.md`): добавить нормативный текст «runtime
  гарантирует stable interior pointers — необходимое условие для
  slice-views (D144)».
- Закрытие `Q-array-slicing` / `Q-array-api.5`.
- Cross-ref **D79**: «slice-view inherits D79 disclaimer для shared mut
  между fiber'ами».

**Не входит (deferred / explicit):**

- **Align `str.slice` (метод) на panic-семантику** — отложено в Plan 94.
  Bracket-форма `s[a..b]` (новая, panic) добавлена в этом плане
  (D-str-slice); старый `s.slice(a, b)` (clamp) сохраняется.
- **Многомерные / strided срезы** (`arr[a..b, c..d]`, `arr[a..b:step]`)
  — научная сложность; не для bootstrap.
- **Отдельный тип `Slice[T]`** (Rust-модель) — отвергнуто (D-single-type).
- **Compile-time borrow checker** — отвергнуто (нет в Nova вообще; не
  меняем язык под slice'ы).
- **Cancellation-aware mut-view** (D75 supervised cancel посередине mut
  → partial write) — наследует общий D79 disclaimer; явный M:N-safe
  primitive — отдельная задача (Plan 18 std.sync / future).
- **3-index slice** (Go `s[a:b:c]` — задаёт cap отдельно) — НЕ нужен
  при `cap == len` модели (cap всегда == len для view).

## Декомпозиция (фазы и шаги)

### Ф.1 — Pre-existing fix: bounds-check для `arr[i]` (~0.5 д)

D-bounds-check (D27 §1632 drift). До добавления slice — выровнять
safety raw-indexing.

- **Ф.1.1** В [emit_c.rs:12910-12974](compiler-codegen/src/codegen/emit_c.rs#L12910)
  для `ExprKind::Index { obj, index }` если `obj_ty.starts_with("NovaArray_")`
  и index — int — эмитить:
  ```c
  ({
      NovaArray_T* _a = obj_eval;
      int64_t _i = index_eval;
      if (_i < 0 || _i >= _a->len) {
          nv_panic_index_oob(_i, _a->len);  // wraps Nova_RuntimeError_IndexOutOfBounds
      }
      _a->data[_i];
  })
  ```
- **Ф.1.2** Wrapper `nv_panic_index_oob(int64_t i, int64_t len)` в
  `array.h` — вызывает существующий `Nova_RuntimeError_IndexOutOfBounds`
  + `nv_panic`.
- **Ф.1.3** Тесты:
  - `oob_read_panic.nv` — negative.
  - `oob_write_panic.nv` — negative.
  - `oob_neg_index_panic.nv` — negative (`arr[-1]`).
  - `boundary_ok.nv` — positive (`arr[0]`, `arr[len-1]`).
- **Ф.1.4** Regression: full `nova test` — 0 новых FAIL.

**Acceptance:** D27 §1632 соответствует impl; raw `arr[i]` panic'ит на OOB.

### Ф.2 — Open-ended Range (`a..`, `..b`, `..`) (~1.0 д)

D-open-ended. AST + parser; type-checker валидация в slice-context;
блокировка в bounded-context.

- **Ф.2.1** AST: [ast/mod.rs:1360-1364](compiler-codegen/src/ast/mod.rs#L1360)
  — `ExprKind::Range { start: Option<Box<Expr>>, end: Option<Box<Expr>>,
  inclusive: bool }`. Breaking AST change — обновить **все** match-arms
  (grep `ExprKind::Range` — ~14 точек в types/mod.rs, ~5 в emit_c.rs).
- **Ф.2.2** Parser: [parser/mod.rs:3777-3795](compiler-codegen/src/parser/mod.rs#L3777)
  — `parse_range`:
  - `expr .. expr` → `{ start: Some(e1), end: Some(e2), inclusive: false }`
  - `expr ..= expr` → `{ start: Some(e1), end: Some(e2), inclusive: true }`
  - `expr ..` → `{ start: Some(e1), end: None, inclusive: false }`
  - `.. expr` → `{ start: None, end: Some(e2), inclusive: false }`
  - `..` → `{ start: None, end: None, inclusive: false }`
- **Ф.2.3** Disambiguation:
  - `..` в expression-position (не после operand) — OK.
  - `expr..` в end-of-statement / before `]` — OK.
  - Не путать с `.` accessor — Range всегда `..` (двойная точка).
- **Ф.2.4** Type-checker (types/mod.rs):
  - open-ended Range допустим **только** в slice-context (внутри `[]`).
  - Все остальные использования (`for-in`, materialize, передача в
    функцию) → compile-error «open-ended Range requires bounded context
    (provide both start and end, or use in slice index)».
- **Ф.2.5** Тесты:
  - `range_open_a_dotdot.nv` — pos: `arr[3..]`.
  - `range_open_dotdot_b.nv` — pos: `arr[..3]`.
  - `range_open_dotdot.nv` — pos: `arr[..]`.
  - `neg_range_open_in_for.nv` — neg: `for x in 3..` → error.
  - `neg_range_open_in_let.nv` — neg: `let r = 3..` → error.
  - `range_closed_unchanged.nv` — regression: `a..b` / `a..=b` без изменений.

**Acceptance:** open-ended парсится; type-checker блокирует вне slice;
существующие тесты Range проходят.

### Ф.3 — Type-checker: dispatch `[]T[int]` / `[]T[Range]` (~1.0 д)

D-single-type, D-empty-ok.

- **Ф.3.1** types/mod.rs `ExprKind::Index { obj, index }` —
  - infer `obj_ty`.
  - infer `index_ty`.
  - dispatch:
    - `obj_ty = []T && index_ty = int` → result `T` (текущее).
    - `obj_ty = []T && index_ty = Range` → result `[]T`.
    - `obj_ty = str && index_ty = Range` → result `str` (D-str-slice).
    - `obj_ty = str && index_ty = int` → unchanged (либо `char` если
      char-indexing уже есть, либо compile-error).
    - иначе error «cannot index <obj_ty> by <index_ty>».
- **Ф.3.2** Slice-of-slice: `arr[a..b][c..d]` — result `[]T` после
  первого `[a..b]`, dispatch второго `[c..d]` — тот же путь. Тест.
- **Ф.3.3** Mut-rule (D-mut-rule): `mut`-view только от `mut`-источника.
  Type-checker трекает mut-flag через index-expression.
- **Ф.3.4** Тесты pos/neg:
  - `slice_int_returns_elem.nv` — `arr[3]` тип `int`.
  - `slice_range_returns_array.nv` — `arr[1..3]` тип `[]int`.
  - `slice_of_slice.nv` — `arr[1..5][1..3]`.
  - `neg_index_by_string.nv` — `arr["foo"]` → error.
  - `mut_view_from_mut.nv` — pos: `var arr [...]; let mv = arr[1..3]`.
  - `neg_mut_view_from_immut.nv` — neg: `let arr [...]; let mv = arr[1..3]; mv[0] = 9` → error.

**Acceptance:** type-checker корректно различает int/Range index;
slice-of-slice работает; mut-правила enforce'нуты.

### Ф.4 — Runtime + Codegen: `nova_array_slice_<T>` (~2.0 д)

D-cap-len, D-header, D-neg-panic.

- **Ф.4.1** Runtime — array.h:
  ```c
  #define NOVA_ARRAY_SLICE_IMPL(T)                                         \
  static NovaArray_##T* nova_array_slice_##T(                              \
      NovaArray_##T* a, int64_t from, int64_t to                           \
  ) {                                                                       \
      if (from < 0 || to < from || to > a->len) {                          \
          nv_panic_slice_oob(from, to, a->len);                            \
      }                                                                     \
      NovaArray_##T* v = nova_alloc(sizeof(NovaArray_##T));                \
      v->data = a->data + from;          /* interior pointer */            \
      v->len  = to - from;                                                  \
      v->cap  = to - from;               /* D-cap-len */                   \
      return v;                                                             \
  }
  ```
  Добавить в `NOVA_ARRAY_IMPL` macro (auto для всех built-in element-types).
- **Ф.4.2** Wrapper `nv_panic_slice_oob(from, to, len)` — diagnostic
  message «slice [N..M] out of bounds for array of length L».
- **Ф.4.3** Codegen emit_c.rs:12910 — dispatch:
  ```rust
  ExprKind::Index { obj, index } => {
      let obj_ty = self.infer_expr_c_type(obj);
      let index_ty = self.infer_expr_c_type(index);
      ...
      if obj_ty.starts_with("NovaArray_") {
          let elem = &obj_ty["NovaArray_".len()..obj_ty.len() - 1];
          let o = self.emit_expr(obj)?;
          if index_ty == "Nova_Range*" {
              // slice path
              let (from, to) = self.emit_range_bounds(index, o)?;  // handles open-ended
              Ok(format!("nova_array_slice_{}({}, {}, {})", elem, o, from, to))
          } else {
              // int-index path with bounds-check (Ф.1)
              ...
          }
      }
  }
  ```
- **Ф.4.4** `emit_range_bounds(range_expr, array_var)` — substitution
  для open-ended:
  - `Some(a), Some(b), false` → `(a, b)`
  - `Some(a), Some(b), true` → `(a, b + 1)`
  - `Some(a), None, _` → `(a, array_var->len)`
  - `None, Some(b), false` → `(0, b)`
  - `None, Some(b), true` → `(0, b + 1)`
  - `None, None, _` → `(0, array_var->len)`
- **Ф.4.5** Propagation HashMaps (H3, H4 из аудита):
  - `array_element_types[slice_var] = array_element_types[parent_var]`.
  - `str_box_arrays.contains(parent) → str_box_arrays.insert(slice_var)`.
  - Hook в `emit_let_decl` где RHS = Index-with-Range-arg.
- **Ф.4.6** `infer_expr_c_type` — обновить все ~10 точек обработки
  `Index`, чтобы Range-index возвращал `NovaArray_T*`, не `T`.
- **Ф.4.7** GC verify: написать probe — `let v = make_array(); let s = v[2..5];
  v = nil; gc.collect(); /* s alive */ assert(s[0] == ...)`. Запустить;
  если падает — interior-ptr не работает; добавить explicit
  `GC_set_all_interior_pointers(1)` гарантию в spec amendment D6.
- **Ф.4.8** `str[Range]` emit-path (D-str-slice):
  - Runtime: `nova_str_slice_panic(nova_str s, nova_int from, nova_int to)`
    в `nova_rt.h` — same as existing `nova_str_slice` (codepoint walk →
    byte offset), но при `from < 0 || to < from || to > codepoint_count`
    → `nv_panic_str_slice_oob` вместо clamp.
  - Codegen: в `Index`-dispatch добавить ветку `obj_ty == "nova_str"
    && index_ty == "Nova_Range*"` → `nova_str_slice_panic(s, from, to)`
    (с `emit_range_bounds` Ф.4.4 для open-ended).

**Acceptance:** `nova_array_slice_<T>` работает для всех built-in
element-types; slice-of-slice работает; HashMaps propagate; GC-probe
проходит.

### Ф.5 — Interop + push-detach + lint (~1.0 д)

D-push-detach, D-iter-snap.

- **Ф.5.1** `for x in view` — emit_c.rs:16882 уже использует `_arr->len`,
  для view это OK (len = b-a). Тест: positive + parent.push-during-iter
  не виден.
- **Ф.5.2** Plan 90 методы на view: `view.copy_from`, `.copy_within`,
  `.fill`, `.compare`, `.len`, `.is_empty`, `.capacity` — работают
  автоматически (view это `[]T`). Тест: `slice_plan90_interop.nv`.
- **Ф.5.3** `view.iter()` — D58 implicit-iter inherit; тест.
- **Ф.5.4** Передача view в функцию `fn f(arr []T)` и `fn f(arr mut []T)`
  — работают. Тест: `slice_to_fn.nv`.
- **Ф.5.5** Push-detach: запустить тест `slice_push_detaches.nv`:
  ```nova
  var parent = [1, 2, 3, 4, 5]
  var view = parent[1..4]    \ view: [2, 3, 4], len=cap=3
  view.push(99)              \ realloc, view = [2, 3, 4, 99]
  assert(parent == [1, 2, 3, 4, 5])    \ parent UNCHANGED
  assert(view == [2, 3, 4, 99])
  ```
- **Ф.5.6** Lint `W_VIEW_PUSH_DETACH`: в type-checker'е (`compiler-codegen/src/diagnostics/`)
  — pattern «var X = Y[range]; X.push(...)» → warning «mut view's push
  detaches from parent backing; parent NOT modified. Use parent directly
  to grow, or `Array.from(view)` to make the detach explicit». Тест:
  `lint_view_push_detach.nv`.
- **Ф.5.7** Iterator invalidation: тест `slice_iter_after_parent_push.nv`:
  ```nova
  var parent = [1, 2, 3, 4, 5]
  ro view = parent[1..4]
  parent.push(99)            \ realloc parent backing
  \ view ещё указывает на старый backing (через interior-ptr, Boehm держит)
  assert(view.len() == 3)
  assert(view[0] == 2)       \ valid read
  ```

**Acceptance:** все Plan 90 методы работают на view; push-detach
доказан; lint warning эмитится; iter invalidation defined.

### Ф.6 — Тесты (~1.5 д)

~42 теста в `nova_tests/plan96/`. Категории:

*Basic (8):*
- `slice_basic.nv` — `arr[1..3]` read.
- `slice_full.nv` — `arr[..]` все элементы.
- `slice_prefix.nv` — `arr[..3]` префикс.
- `slice_suffix.nv` — `arr[2..]` суффикс.
- `slice_inclusive.nv` — `arr[1..=3]`.
- `slice_empty_start.nv` — `arr[0..0]`.
- `slice_empty_end.nv` — `arr[5..5]` (`len == 5`).
- `slice_of_empty.nv` — `[][0..0]`.

*Mut (4):*
- `slice_mut_write_visible.nv` — `mv[0] = 9` → `parent[from] == 9`.
- `slice_mut_read_through.nv` — `parent[i] = 9` → `mv[i-from] == 9`.
- `slice_mut_multiple_views.nv` — два mut-вью одного backing'а — OK,
  оба видят изменения.
- `slice_mut_view_overlap.nv` — два overlap'нутых mut-вью — OK по
  D-mut-rule.

*Open-ended (5):* — см. Ф.2.5.

*OOB / negative (6):*
- `neg_slice_a_negative.nv` — `arr[-1..3]` → panic.
- `neg_slice_b_too_large.nv` — `arr[0..100]` → panic.
- `neg_slice_a_gt_b.nv` — `arr[5..3]` → panic.
- `neg_slice_open_a_too_large.nv` — `arr[100..]` → panic.
- `neg_index_negative.nv` — `arr[-1]` → panic (Ф.1).
- `neg_index_too_large.nv` — `arr[100]` → panic (Ф.1).

*Push-detach (3):* — см. Ф.5.5–Ф.5.6.

*Slice-of-slice (2):*
- `slice_of_slice_basic.nv` — `arr[1..5][1..3]`.
- `slice_of_slice_offset.nv` — verify offset arithmetic
  (`arr[1..5][1..3][0]` == `arr[3]`).

*GC interop (2):*
- `slice_gc_parent_unreachable.nv` — backing достижим только через
  view; `gc.collect()` × 50; data intact.
- `slice_gc_stress.nv` — 1000 views, 50 collects; assert no segfault.

*Iter / for-in (3):* — см. Ф.5.1.

*Plan 90 interop (3):* — см. Ф.5.2.

*Function-passing (2):* — см. Ф.5.4.

*Lint (1):* — `lint_view_push_detach.nv` Ф.5.6.

*str-slice bracket (5, D-str-slice):*
- `str_slice_basic.nv` — pos: `s[1..3]` codepoint.
- `str_slice_open.nv` — pos: `s[..]` / `s[2..]` / `s[..5]`.
- `str_slice_utf8.nv` — pos: codepoint-correct для UTF-8 строки.
- `str_slice_oob_panic.nv` — neg: `s[0..100]` где `len < 100` → panic
  (НЕ clamp, в отличие от `s.slice`).
- `str_slice_method_still_clamps.nv` — regression: старый `s.slice(0, 100)`
  продолжает clamp'ить (backwards-compat).

*Edge case `[]Range` indexed by Range (1, G15):*
- `array_of_ranges_index.nv` — pos: `let rr []Range = [0..5, 10..20];
  let r0 = rr[0]; let sub = rr[0..1]` — dispatch корректен.

**Ф.6.43** Full `nova test .` — 0 новых FAIL.

**Acceptance:** ~35 тестов pos/neg PASS; полный регресс зелёный.

### Ф.7 — Spec + docs (~1.0 д)

- **Ф.7.1** Новый **D144** (`spec/decisions/02-types.md`) — slice-view
  семантика. Покрывает: single-type, cap==len, mut, push-detach,
  iter-snap, open-ended, GC-interior-ptr requirement, D79 disclaimer.
- **Ф.7.2** Amend **D27** (`03-syntax.md`): убрать §1663 «Слайсинг
  отложен»; cross-ref D144. Подтвердить §1632 bounds-check (Ф.1).
- **Ф.7.3** Amend **D58** (`03-syntax.md`): open-ended Range формы;
  правило «open-ended требует bounded context».
- **Ф.7.4** Amend **D6** (`05-memory.md`): «runtime гарантирует stable
  interior pointers — необходимое условие slice-views (D144). Любая
  замена GC-backend на moving GC требует одновременной замены slice-
  представления (separate header struct + ptr-update on move).»
- **Ф.7.5** Закрыть `Q-array-slicing` / `Q-array-api.5` в
  `spec/open-questions.md`.
- **Ф.7.6** `spec/decisions/README.md` — D-index с D144.
- **Ф.7.7** `docs/plans/README.md` — Plan 96 → ЗАКРЫТ.
- **Ф.7.8** `docs/simplifications.md`:
  - Снять `[P-array-slice-deferred]` (если есть).
  - Записать `[P-str-slice-clamp-vs-panic]` — `str.slice` оставлен с
    clamp; align с panic semantics — Plan 94.
- **Ф.7.9** `docs/project-creation.txt` — запись.
- **Ф.7.10** `nova-private/discussion-log.md` — запись с design decisions
  и cross-ref D79.

**Acceptance:** spec/docs consistent; D144 published; D27/D58/D6 amended;
Q-array-slicing closed.

## Acceptance criteria (full)

*Pre-existing fix:*
- [ ] `arr[i]` panic'ит при OOB / negative.
- [ ] D27 §1632 соответствует impl.

*Slice basics:*
- [ ] `arr[a..b]` / `arr[a..=b]` / `arr[a..]` / `arr[..b]` / `arr[..]`
      — все 5 форм работают.
- [ ] Result type `[]T` (тот же что у parent).
- [ ] Slice-of-slice (`arr[a..b][c..d]`) — offset arithmetic корректен.
- [ ] O(1) — slice creation = 1 alloc 24-byte + ptr arithmetic.

*Mut + aliasing:*
- [ ] `mv[i] = x` видно в `parent[from+i]`.
- [ ] `parent[i] = x` видно в `mv[i-from]`.
- [ ] Multiple `mut`-views — разрешены; все видят изменения.
- [ ] `mut`-view от `let`-источника — compile error.

*Push-detach:*
- [ ] `view.push(x)` НЕ модифицирует parent (cap==len → realloc → detach).
- [ ] Lint `W_VIEW_PUSH_DETACH` warning эмитится.

*Bounds:*
- [ ] OOB / `a > b` / `a < 0` / `b > len` → `nv_panic`.
- [ ] Empty slice (`arr[a..a]`, `arr[len..len]`, `arr[..0]`) — валиден.

*GC:*
- [ ] View держит backing alive через interior-ptr.
- [ ] Stress-test 1000 views + 50 collects — no segfault.
- [ ] Parent.push реалок'ает — view продолжает указывать на старый backing.

*Interop:*
- [ ] `for x in view` работает (D-iter-snap).
- [ ] Plan 90 methods (`copy_from`/`compare`/`fill`/...) работают на view.
- [ ] `view.iter()` → `Iter[T]` работает.
- [ ] `fn f(arr []T)` принимает view.
- [ ] `fn f(arr mut []T)` принимает mut-view и пишет в shared backing.

*Open-ended:*
- [ ] open-ended Range вне slice-context → compile error.
- [ ] AST migration (Option<Box<Expr>>) — все 14+ точек обновлены.

*str-slice bracket (D-str-slice):*
- [ ] `s[a..b]` / `s[a..=b]` / open-ended — все формы работают
      (codepoint-indexed).
- [ ] OOB → panic (НЕ clamp).
- [ ] Старый `s.slice(a, b)` неизменён (clamp, backwards-compat).
- [ ] UTF-8 corrrectness preserved (codepoint, не byte).

*Concurrency disclaimer:*
- [ ] D144 явно ссылается на D79 для shared mut между fiber'ами.
- [ ] В D71 single-thread bootstrap — нет UB (single-thread).

*Spec:*
- [ ] D144 в `02-types.md`.
- [ ] D27 §1663 убран («Слайсинг отложен»); §1632 bounds-check confirmed.
- [ ] D58 amend для open-ended.
- [ ] D6 amend для interior-ptr stability.
- [ ] Q-array-slicing / Q-array-api.5 закрыты.

*Регресс:*
- [ ] Полный `nova test .` — 0 новых FAIL.

## Risks

1. **AST breaking change для Range (Option<Box<Expr>>)** — 14+ точек в
   type-checker'е + 5 в codegen + парсер. **Средний** риск; mitigation
   — Ф.2 атомарный коммит, full test после миграции.
2. **`infer_expr_c_type` для Index — 10+ точек обновить.** Каждая
   точка должна dispatch'ить по index-type. **Средний** риск; mitigation
   — grep + checklist + per-point unit tests.
3. **HashMaps propagation (`array_element_types`, `str_box_arrays`)
   для slice-var.** Если пропустить — slice от `[]str` сломается на
   element access. **Низкий** риск (Ф.4.5 explicit hook в `emit_let_decl`).
4. **GC interior-ptr — теоретически работает (Boehm + `GC_set_all_interior_pointers(1)`)
   но без production-stress нет 100% guarantee.** Mitigation — Ф.4.7
   GC-probe + Ф.6 stress-тесты.
5. **Push-detach silent surprise** — D32 contract «mut visible to caller»
   не выполнен после detach. Mitigation — D-push-detach + `W_VIEW_PUSH_DETACH`
   lint (warning, не error).
6. **M:N + view sharing = D79 UB.** Не enforce-able в bootstrap; mitigation
   — explicit cross-ref D79 в D144 + явный disclaimer в spec.
7. **`str.slice` clamp vs slice-`[]T` panic inconsistency** — оставлено
   как known drift; mitigation — `[P-str-slice-clamp-vs-panic]` simplification
   маркер, fix в Plan 94.
8. **Edge case `[]Range` индексируется `Range`** — `let rr: []Range = [0..5,
   10..20]; rr[0..1]` — `Range` index → `[]Range` view; `rr[0]` → Range. Dispatch
   корректен по type, но **нужен explicit test**: `slice_array_of_ranges.nv`.

## Non-scope (deferred / explicit)

- Срезы `str` (align panic) — Plan 94.
- Многомерные / strided.
- Отдельный тип `Slice[T]` (Rust-модель).
- Compile-time borrow checker.
- Cancellation-aware mut-view посередине mut (D75 supervised).
- 3-index slice (Go `s[a:b:c]` — задаёт cap отдельно).
- Header optimization 24→16 байт (требует отдельного типа).
- Slice of `[N]T` (fixed-size) — отдельный path, отложен; D27 говорит
  fixed-size — отдельный API.

## Связь

- [D6](spec/decisions/05-memory.md#d6) non-moving GC + interior-ptr
  invariant (Ф.7.4 amend).
- [D27](spec/decisions/03-syntax.md#d27) — `[]T` API; §1632 bounds-check;
  §1663 «Слайсинг отложен» (Ф.7.2 amend).
- [D32](spec/decisions/02-types.md#d32) — managed reference; `mut`-видна-
  caller'у contract (cross-ref для push-detach footgun).
- [D58](spec/decisions/03-syntax.md#d58) — Range-литералы; open-ended
  (Ф.7.3 amend).
- [D71](spec/decisions/06-concurrency.md#d71) — single-thread bootstrap;
  slice-views safe в этой стадии.
- [D79](spec/decisions/06-concurrency.md#d79) — shared mut между
  fiber'ами = UB в M:N; slice inherits disclaimer (Ф.7.1 cross-ref).
- [D141](spec/decisions/08-runtime.md#d141) — Plan 90 bulk-ops; срезы
  делают offset-параметры ненужными (`copy_from(src[k..k+n])`).
- [Plan 90](90-memory-access-primitives.md) — bulk-операции `[]T`;
  работают на view автоматически.
- [Plan 94](94-str-methods-on-nova.md) — str-методы; align `str.slice`
  с panic-semantics.
- [Plan 56](56-vtable-dispatch-erased-generics.md) — vtable / mono для
  generic methods (фон).
- [Q-array-slicing](spec/open-questions.md#q-array-api) — закрывается
  (Ф.7.5).
- Ориентиры: Go `s[a:b]` (без append-footgun), Rust `&[T]`/`RangeBounds`,
  TS `subarray`, Swift `ArraySlice` (без CoW-disconnect).
