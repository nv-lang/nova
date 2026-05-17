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

- [x] Phase 1 — mono'd struct generation работает (commit 5b9f317452e).
- [x] Phase 2 — `for (k, v) in coll` для **любых** K, V (включая
      `nova_str` и user records) работает в user-code (f1, f7, f10 PASS).
- [x] Phase 3 — HashMap stdlib **usable** через идиоматический iter syntax
      из user-кода (validated [`f1_for_tuple_in_hashmap.nv`](../../nova_tests/plan59/f1_for_tuple_in_hashmap.nv)).
      Direct `.buckets` workaround в `@merge_from`/`@filter` оставлен —
      см. [`[M-stdlib-iter-in-generic-method-body]`](#m-stdlib-iter-in-generic-method-body)
      ниже (отдельный followup, не блокер Plan 59).
- [x] Phase 4 — spec D-block (D111 в `spec/decisions/02-types.md`).
- [x] Phase 5 — length-prefixed mangle (aa07fa0d545); root-cause fix
      для nested tuple collision вместо workaround'а.
- [x] Phase 6 — match-pattern variant + mono'd tuple payload
      (Option Some((k, v)) heterogeneous). Result deferred — sum-type
      mono — отдельный план (см. ниже Phase 6 section).
- [x] Полный `nova test` — **594 PASS / 0 FAIL / 42 SKIP** после Phase 5
      (vs 568 pre-Plan-59 baseline — net **+26 unlocked**).
- [x] **10 новых тестов** в `nova_tests/plan59/` (f1-f10): tuple lit,
      destructure в let, destructure в for, tuple as fn param/return,
      mixed types, tuple of tuples (до 4 уровней), tuple of arrays,
      tuple of user records, closure-in-tuple, custom iter.

---

## [M-stdlib-iter-in-generic-method-body] — deferred followup

### Симптом

Замена workaround'а `for i in 0..@_buckets.len { match @_buckets[i] {...}}`
на идиоматичный `for (k, v) in other` в **generic method body**
(`HashMap[K, V] mut @merge_from(other HashMap[K, V])`) при mono'd
HashMap[str, int] вызывает CC-FAIL в неизменённом `@clone` и других
методах. Cascade — encoding/dispatch issue в mono'd path.

### Hypothesis

В erased generic body, `for (k, v) in other` требует мono'd
`HashMapIter[K, V]` + tuple destructure. Mono pass registers
HashMapIter instance, который затем conflict'ит с другими мono'd
методами того же HashMap[K, V] — возможно `var_types`/`current_type_subst`
state leak между mono'd method bodies.

### Что подтверждено в Plan 59

- User-code иdiomatic `for (k, v) in m` где m уже concrete HashMap[K, V]
  — **работает** (f1, f7, f10 PASS).
- Stdlib internal idiomatic, где iter call происходит в generic method
  body — нет (cascading CC-FAIL).

### Scope (отдельный план / Plan 60+)

Diagnose mono-pass state isolation для `for-tuple-in-iter` внутри
generic method body. Реальные кандидаты: leak в
`tuple_element_types`/`var_types`/`current_type_subst` при rotation
между sibling method bodies одного и того же типа.

Workaround `for i in 0..@_buckets.len` в stdlib безопасен и
производителен — не блокирует prod use.

---

## Phase 6 — match-pattern variant + mono'd tuple payload (2026-05-17 EOD+2)

### Контекст

После Phase 5 обнаружен gap: `match Some((k, v)) { ... }` где
`Some` payload — heterogeneous mono'd tuple `Option[(str, int)]`,
не работал. Inner tuple-destructure после Variant pattern
обращался к `_nv_scr.value.f0/f1` но bind'ил k, v как `nova_int`
default → CC-FAIL при str element.

### Root cause

`pattern_destructure_tuple` lookup'ит `tuple_element_types[scr]`,
но для variant-payload access (`scr_var.value`) ключ не был
зарегистрирован. Variant pattern handler (Pattern::Variant в
`pattern_bind_typed`) знает payload type (через `novaopt_value_types`
для Option), но не propagate'ил mono'd tuple element types в
registry для inner Tuple pattern.

### Fix

При recurse'е в inner Tuple pattern из Variant handler, если
payload type starts_with("_NovaTuple_") (mono'd) — parse elements
через `parse_mono_tuple_elements` + insert в `tuple_element_types`
для access string. Это покрывает обе branches Variant handler:
- Option Some: t_from_scr path (NovaOpt typed payload).
- User sum-type Ok/custom: sum_schemas path (variant-field types).

### Acceptance Phase 6

- [x] `Option[(str, int)]` match Some((k, v)) destructure works.
- [x] `Option[(int, int)]` (homogeneous) continues working.
- [x] f17_tuple_in_option PASS (4 sub-tests с разными K, V).

### Out-of-scope (Plan 63+)

**`Result[(T1, T2), E]`** не работает — Result generic типы НЕ
monomorphized (всегда `Nova_Result*` erased, payload slot — `nova_int`).
Это **отдельная** ограничение: mono pass для Result-как-sum-type, не
для tuple. Требует архитектуры sum-type monomorphization (≈Plan 56
vtable scope расширенный на variants). Documented как
`[M-result-erased-no-mono]` — отдельный план, не блокер Plan 59.

User sum-types с tuple payload (`Custom { Ok((k, v)) }`) work
если sum-type сам **не generic** (mono path не trigger'ится).

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

---

## Working environment & autonomous start (2026-05-17 EOD+1)

> **Working directory:** `d:\Sources\nova-lang`
> **Branch:** `main`
> **Build:** release (`cargo build --release`).
> **Tests:** `nova-cli/target/release/nova test` (без `--jobs N`).
> **Commits:** без AI-trailer'ов.

### Pre-start baseline

- **568 PASS / 0 FAIL / 42 SKIP** — clean baseline (post Plan 55 +
  Plan 56 + Plan 33.6 Ф.15.2 bonus fix).
- Plan 55 ✅, Plan 56 ✅ закрыты.

### Autonomous start

User explicitly: "продолжай работать по плану сам по всем оставшимся
пунктам без упрощений как для прода". Plan 59 — наиболее impactful
из deferred (P1, unlocks idiomatic `for (k, v) in coll`).

---

## Working environment & autonomous progress log

> **Working directory:** `d:\Sources\nova-lang`
> **Branch:** `main`
> **Build:** release (`cargo build --release` — обязательно per memory
> `feedback_release_builds`).
> **Tests:** `nova-cli/target/release/nova test` (без `--jobs N` per
> `feedback_no_jobs_flag`).
> **Commits:** без AI-trailer'ов per `feedback_commit_trailer`.

### Pre-start baseline (2026-05-17 EOD)

- **568 PASS / 0 FAIL / 42 SKIP** — clean baseline.
- Plan 55 ✅, Plan 56 ✅ закрыты в предыдущей session.
- Plan 33.6 Ф.15.2 bonus fix — apply_bounds_propagation standalone.

### Autonomous start

User explicitly requested: "продолжай работать по плану сам по всем
оставшимся пунктам без упрощений как для прода". Plan 59 — наиболее
impactful из deferred (P1, unlocks idiomatic `for (k, v) in coll`).

---

## Phase 5 — Mangle scheme: length-prefixed encoding (2026-05-17 follow-up)

### Контекст

Изначальная mangle scheme Plan 59: `_NovaTuple____<T1>__<T2>__...` —
prefix `____` (4 underscores), separator `__` (2 underscores).

**Bug** выявлен после регрессий: когда element type — это сам mono'd
tuple (`_NovaTuple____...`), его внутренние `____` collide с outer
separator. Парсер `split("__")` распадается на garbage. Симптом:
- `f2_nested_tuple_typedef_order.nv` падал CC-FAIL потому что
  `let (left, right) = nested_pair` использовал legacy `_NovaTuple2`
  вместо реального mono'd struct.
- `let_destructure` general case парсил `_NovaTuple_____NovaTuple____...`
  как ≠ arity 2.

**Workaround в d73a892f27b:** registry lookup для Ident RHS
(`tuple_element_types.get(name)`). Закрывал symptom, не root cause.

### Root cause

Mangle scheme не self-describing: невозможно однозначно разделить
elements когда element name содержит separator. Это classic ambiguity
flat string concatenation без length-prefix.

### Production-grade решение: length-prefixed mangle

**Аналог:** Itanium C++ ABI (`_Z3foo`), Rust v0 mangle (`N4nameE`).
Length-prefix делает encoding **unambiguous** для любой глубины nesting.

**Новый формат:**
```
_NovaTuple_<arity>_<L1>_<T1>_<L2>_<T2>_..._<LN>_<TN>
```

Где `<Ln>` — десятичная длина (no leading zeros) `<Tn>` в bytes.

**Примеры:**
- `(nova_int, nova_int)` → `_NovaTuple_2_8_nova_int_8_nova_int`
- `(nova_str, nova_bool)` → `_NovaTuple_2_8_nova_str_9_nova_bool`
- `((int, int), int)` outer:
  - inner = `_NovaTuple_2_8_nova_int_8_nova_int` (34 chars)
  - outer = `_NovaTuple_2_34__NovaTuple_2_8_nova_int_8_nova_int_8_nova_int`
  - Parser читает L=34, берёт следующие 34 chars как T1, продолжает.

**Distinguishing от legacy `_NovaTupleN`:**
- Legacy: `_NovaTuple1`, `_NovaTuple2`, ..., `_NovaTuple8` — digit
  напрямую после `NovaTuple`, без `_`.
- New: `_NovaTuple_<arity>_...` — underscore после `NovaTuple`.
- `starts_with("_NovaTuple_")` точно идентифицирует mono'd новый формат.

### Scope

1. **`compute_mono_tuple_c_name`** — переписать на length-prefix.
2. **`parse_mono_tuple_elements`** — новый helper, unambiguous parser.
3. **Callsites** обновить: `_NovaTuple____` → `_NovaTuple_` в
   `starts_with`/`strip_prefix` (8 мест).
4. **`desanitize_c_from_ident`** оставить — element types
   sanitize'ятся независимо (e.g. `Nova_Foo*` → `Nova_Foo_p`),
   reverse `_p → *` нужен.
5. **Удалить workaround** registry-lookup в `let_destructure`
   general case — parser теперь точен.
6. **Regression test** `f10_deeply_nested_tuple_mangle.nv` — 4-уровневый
   nested tuple который ломался бы под старой схемой.
7. **simplifications.md** запись `[M-tuple-mangle-nested-collision]` ✅
   закрыт + d73a892f27b summary.

### Acceptance Phase 5

- [ ] Новый `compute_mono_tuple_c_name` length-prefixed.
- [ ] `parse_mono_tuple_elements` unambiguous для любого nesting depth.
- [ ] 8+ callsites обновлены, ни одного `_NovaTuple____` literal.
- [ ] Registry-lookup workaround в let-destructure удалён.
- [ ] `f10_deeply_nested_tuple_mangle.nv` PASS (4-уровневый).
- [ ] Full regression `nova test` — 0 регрессий.
- [ ] `docs/simplifications.md` — `[M-tuple-mangle-nested-collision]`
      ✅ ЗАКРЫТО + Plan 59 fix d73a892f27b запись.

### Estimate

~60-100 LOC + 1 test. Risk **medium-low** — изолированная замена
encoding с явным parser'ом + regression test guard.
