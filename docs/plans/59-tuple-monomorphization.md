// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 59: Tuple monomorphization (mono'd `_NovaTuple_<K>_<V>` структуры)

> **Создан 2026-05-16 EOD.** Закрывает **bootstrap limitation** выявленный
> Plan 56 followup: идиоматический `for (k, v) in collection` (implicit
> Iter + tuple destructure) не работает потому что Nova bootstrap не
> имеет monomorphized tuple types.

---

## Контекст и motivation

### Симптом

Идиоматический Nova:
```nova
for (k, v) in some_hashmap {
    process(k, v)
}
```

Compiler:
1. ✅ Implicit Iter (D58): `for ... in coll` → `for ... in coll.iter()`.
2. ✅ Mono'd `HashMapIter[K, V].next()` returns `Option[(K, V)]`.
3. ✅ Tuple destructure pattern `(k, v)` — parser + AST work.
4. ❌ **Tuple storage**: `_NovaTuple2` имеет `f0: nova_int, f1: nova_int`
   — generic int slots. `nova_str` (struct ptr+len, 16 bytes) **не
   fit'ит** в nova_int (8 bytes).

Result: `nova_str k = _NovaTuple2.f0` — CC-FAIL `initializing nova_str
from nova_int`.

### Root cause

`_NovaTuple<N>` в `compiler-codegen/nova_rt/` — **single generic
struct** per arity:

```c
typedef struct _NovaTuple2 {
    nova_int f0;
    nova_int f1;
} _NovaTuple2;
```

Это **type erasure** для tuples — все элементы хранятся как nova_int
(8-byte slot). Для primitives что fit (int/bool/byte/f64) — works через
cast. Для **struct value types** (nova_str = 16 bytes; user records с
fields) — **breaks**.

Альтернатива bootstrap'a сейчас:
- Pointer boxing: store nova_str* в slot, deref на read.
- Это **silently** работает только для known specific paths (codegen
  inserts cast). Для general tuple destructure — не работает.

### Параллель в других языках

| Язык | Tuple representation | Cost |
|---|---|---|
| **Rust** | Mono per (T1, T2, ...) — concrete struct `(T1, T2)` | Zero (mono) |
| **C++** | `std::tuple<T1, T2>` — template, mono per instantiation | Zero (mono) |
| **Go** | No tuples — multiple return values + pseudo-structs | N/A |
| **TypeScript** | Erasure to JS Array | N/A |
| **Swift** | Mono per tuple type | Zero |
| **Nova (current)** | Single `_NovaTupleN` с int slots | **Broken для struct elements** |
| **Nova (target)** | Mono per concrete tuple `_NovaTuple_<T1>_<T2>` | Zero (mono) |

Nova should be **на уровне** Rust/C++/Swift — proper mono per tuple
type. Это **не optimization**, это **correctness** — без mono'd tuples
struct elements broken.

---

## Scope

### Phase 1 — Mono'd tuple structs

- **Codegen**: для каждого concrete tuple type `(T1, T2, ..., TN)`,
  generate mono'd struct:
  ```c
  typedef struct _NovaTuple____nova_str__nova_int {
      nova_str f0;
      nova_int f1;
  } _NovaTuple____nova_str__nova_int;
  ```
- **Mangling**: `_NovaTuple____<T1_c>__<T2_c>__...` — параллель с
  generic record mono mangling.
- **Registration**: при type-checker / codegen pass discover tuple
  types used (return types of mono'd methods, struct fields, let
  bindings, etc.) → enqueue tuple mono.
- **Worklist**: similar к `generic_type_worklist` (Plan 48).

### Phase 2 — Codegen integration

- **emit tuple lit**: `(a, b)` где a: T1, b: T2 — emit `_NovaTuple____<T1>__<T2>`.
- **emit tuple destructure**: `let (k, v) = ...` — direct field access
  на mono'd struct, no cast.
- **emit_for tuple in iter**: `for (k, v) in coll` — pattern_destructure_tuple
  uses mono'd field types directly.
- **Backwards compat**: legacy `_NovaTupleN` (generic int slots)
  оставляется для cases where mono unavailable (e.g. truly erased
  generic context). Auto-fallback на legacy boxing.

### Phase 3 — Stdlib unlock

- **`for (k, v) in hashmap`** — idiomatic syntax работает (без обходов
  на keys()+get() или direct .buckets access).
- **HashMap.@iter() / @keys() / @values()** идиоматично используются.
- **General tuple usage** — `let pair: (str, int) = ("a", 1); let (k, v) = pair`
  работает для **любых** T1, T2.

### Phase 4 — Spec

- D27 (или новый D-block) — описать tuple monomorphization rule.
- D58 (Iter protocol) — note that `for (k, v) in coll` работает через
  mono'd tuples.

---

## Acceptance criteria

- [ ] Phase 1 — mono'd `_NovaTuple____<...>` struct generation работает.
- [ ] Phase 2 — `for (k, v) in coll` для **любых** K, V (включая
      struct types like nova_str + user records) работает.
- [ ] Phase 3 — HashMap stdlib usable через идиоматический iter syntax.
      Direct `.buckets` access → optional implementation detail (можно
      убрать workaround в @merge_from/@filter).
- [ ] Phase 4 — spec D-block.
- [ ] Полный `nova test` (release) — **0 регрессий** vs Plan 56 baseline.
- [ ] **~10 новых тестов** в `nova_tests/plan59/` (tuple lit, destructure
      в let, destructure в for, tuple as fn param/return, mixed K/V types,
      tuple of tuples, tuple of records).

---

## Estimate

| Phase | LOC | Risk | Зависимости |
|---|---|---|---|
| Phase 1 (mono struct gen + worklist) | ~250-400 | medium | Plan 48 (mono infra) |
| Phase 2 (codegen tuple lit + destructure) | ~200-300 | medium | Phase 1 |
| Phase 3 (stdlib idiomatic + tests) | ~50 | low | Phase 1+2 |
| Phase 4 (spec) | docs | low | Phase 2 |
| **Total** | **~500-750 LOC** | medium | self-contained |

**Estimate:** ~2-3 dev-days production-grade.

---

## Closed-by Plan 59

| Marker | Origin | What this plan closes |
|---|---|---|
| `[M-mono-tuple-element-types]` | Plan 55 followup | Direct fix |
| `[M-tuple-storage-int-slots]` | Plan 59 (этот) | Direct fix — proper struct storage |
| Spread `[...src, k:v]` codegen full | Plan 55 spread infrastructure | Phase 3 unlock (iter+tuple) |

---

## Связь

- **Plan 48** (closures-in-generics / monomorphization) — Phase 1
  reuses mono infrastructure.
- **Plan 55** spread feature — `[...src]` codegen require iter+tuple
  destructure mono'd.
- **Plan 56** vtable dispatch — orthogonal; vtable не нужен для
  tuple storage, но complementary для bound K methods.

---

## Priority

**P1** — закрывает фундаментальную bootstrap limitation. Идиоматический
`for (k, v)` — базовое ожидание любого Nova programmer (параллель
Rust/Swift). Без Plan 59 stdlib design вынужден использовать
workarounds (direct field access на private state).

---

## Что НЕ в Plan 59

- **Tuple subtyping** (`(int, str) <: (any, any)`) — не нужно в
  bootstrap.
- **Variadic tuples** (`(T...)`) — отдельный план.
- **Named tuple fields** (`(name: str, age: int)`) — это record-territory.
