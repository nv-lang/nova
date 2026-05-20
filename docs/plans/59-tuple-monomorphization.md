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
- [x] Phase 4 — spec D-block (D123 в `spec/decisions/02-types.md`).
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

### ✅ ЗАКРЫТО (2026-05-17) Plan 63 Fix E + workaround removal

Закрыто через Plan 63 Fix E (66113a8d2db, другой agent) — fix трёх
взаимосвязанных bug'ов в emit_array_lit / emit_monomorphized_method /
array_element_types tracking. После Fix E идиоматичный
`for (k, v) in other` в generic method body работает без cascade.

Workaround удалён в commit 36e215cce83 — HashMap.@merge_from/@filter
теперь идиоматичны. plan56 6/6 PASS.

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

### Out-of-scope (Plan 63+) — ✅ ЗАКРЫТО (2026-05-17)

**`Result[(T1, T2), E]`** — изначально deferred как
`[M-result-erased-no-mono]`. **Закрыто** через Plan 63 Fix F
(2ae78c7ae8d, другой agent) + Plan 63 Fix F+ (ca677dd2147, эта session):
- Fix F: `result_ok_inner_types` map + pending mechanism для let-bound
  + homogeneous + direct boxing case.
- Fix F+: per-fn registry `fn_result_ok_inner_types` + helper для
  inline match + heterogeneous case через function-call propagation
  + pending leak fix через save/restore на boundary fn body.

Test: `nova_tests/plan59/f19_tuple_in_result.nv` (5 sub-tests:
heterogeneous let+inline, homogeneous let+inline, Err branch).
plan59: 19/19 PASS.

Полный mono'd Result (NovaRes_<T>_<E> typedefs analogous к Option) —
future scope, не нужен для observable use-cases.

User sum-types с tuple payload работают если sum-type сам не generic.

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

### Acceptance Phase 5 (закрыто aa07fa0d545)

- [x] Новый `compute_mono_tuple_c_name` length-prefixed.
- [x] `parse_mono_tuple_elements` unambiguous для любого nesting depth.
- [x] 8+ callsites обновлены, ни одного `_NovaTuple____` literal.
- [x] Registry-lookup workaround в let-destructure удалён.
- [x] `f10_deeply_nested_tuple_mangle.nv` PASS (4-уровневый) + f11-f14
      (pointer elements, diverse types, 5-level nesting, out-of-bounds neg).
- [x] Full regression `nova test` — 594 PASS / 0 FAIL после Phase 5
      (vs 568 pre-Plan-59 baseline).
- [x] `docs/simplifications.md` — `[M-tuple-mangle-nested-collision]`
      ✅ ЗАКРЫТО + Plan 59 fix d73a892f27b запись.

### Estimate

~60-100 LOC + 1 test. Risk **medium-low** — изолированная замена
encoding с явным parser'ом + regression test guard.


---

## Phase 7 — Production polish (audit-driven, 2026-05-17 EOD+2)

### Контекст

После закрытия Phases 1-6 + 2 deferred follow-up'ов проведён audit
production-grade completeness (изолированно в worktree plan-59-audit).
Сравнение с Go/Rust/TS:
- Nova паритет с Rust по zero-cost mono tuples.
- Nova лучше TS (зеро runtime overhead vs erased Array).
- Nova лучше Go (first-class tuples vs тупо multiple returns).
- Nova отстаёт от Rust в: named tuple fields, subtyping, destructure
  в fn params, full mono'd Result.

User: "продолжай работать по плану сам по всем оставшимся пунктам
без упрощений как для прода".

### Ф.7.1 — Tuple arity mismatch diagnostics (M, ~30 LOC)

Сейчас destructure `let (a, b, c) = some_2_tuple` падает с C compiler
error "no member named 'f2'" — плохой UX. Pre-check в
`pattern_destructure_tuple` для Pattern::Tuple — если pattern arity
!= actual tuple arity, emit clear Nova-level diagnostic.

### Ф.7.2 — Stdlib iter cleanup BTreeMap/HashSet (M, ~20 LOC)

Аудит std/collections/*.nv на legacy `for i in 0..@_field.len()`
workaround'ы, замена на идиоматичный `for x in @iter()` после
Plan 63 Fix E.

### Ф.7.3 — sizeof validation для больших tuples (M, ~40 LOC)

Большой tuple (>5 elems, >128 bytes) может вызвать cache-line
stuffing. Emit Plan 36 W-warning suggesting record/struct.

### Ф.7.4 — Named tuple fields (L, ~150-200 LOC)

`(name: T1, name: T2)` syntax → desugar в positional. Spec amend
D27 / D123.

### Ф.7.5 — Full mono'd Result NovaRes_<T>_<E> (L, ~300-400 LOC)

Аналогично NovaOpt — register Result variants per (T, E) combo.

### Ф.7.6 — Tuple subtyping / variance (L, ~200+ LOC)

Design only — нет immediate use case для bootstrap.

### Acceptance Phase 7

- [ ] Ф.7.1: arity diagnostics + test PASS.
- [ ] Ф.7.2: stdlib iter cleanup + regression 0 fail.
- [ ] Ф.7.3: sizeof warning + test PASS.
- [ ] Ф.7.4: named tuple fields parser+codegen+spec+test.
- [ ] Ф.7.5: mono'd Result + spec + test.
- [ ] Ф.7.6: tuple subtyping — design only.
- [ ] Full regression — 0 регрессий.
- [ ] simplifications.md + project-creation.txt + discussion-log updates.

### Working environment

Working dir: `d:\Sources\nova-lang\.claude\worktrees\plan-59-audit`
(isolated worktree). Merge в main после complete acceptance.


---

## Ф.7.4-7.6 — Deferred до dedicated planning sessions (2026-05-17 EOD+2)

### Rationale: L-priority items требуют design decisions

После закрытия M-priority items (Ф.7.1-7.3) re-assessed L-priority items:

**Ф.7.4 Named tuple fields** (~200 LOC + design):
- Parser/AST extension straightforward (~50 LOC).
- Open design questions:
  - Литерал syntax: `(x: 1, y: 2)` collides с record literal `{x: 1, y: 2}`.
    Nova должен явно differentiate.
  - Mixed named+positional `(x: 1, 2)` — allow? Rust не позволяет.
  - Type-equivalence: `(x: int, y: int)` ≡ `(int, int)` или separate?
- Decisions impact spec (D27, D123) + may need extra D-block.
- **Defer:** требует dedicated plan (Plan 64?) с design pre-discussion.

**Ф.7.5 Full mono'd Result** (~300-400 LOC):
- Plan 63 Fix F+ (ca677dd2147) уже покрывает наблюдаемые cases через
  targeted boxed-pointer tracking — все 4 case'а Result[(T, U), E]
  работают (5 sub-tests в f19).
- Full mono refactor (NovaRes_<T>_<E> typedefs analogous Option):
  - Расширение sum-type mono infrastructure.
  - Migration всех Result usage в stdlib + tests.
  - **Не блокирует prod use** — Fix F+ покрытие достаточно.
- **Defer:** Plan 65? только когда появится use case с arbitrary T
  в Result Ok payload (не tuple/struct).

**Ф.7.6 Tuple subtyping / variance** (~200+ LOC):
- Требует variance system в type-checker (covariance/contravariance).
- Нет immediate use case в bootstrap; structural typing Nova не имеет.
- **Design only**, не реализуется до появления requirements.

### Что закрыто Phase 7

- [x] Ф.7.1: tuple arity mismatch diagnostics (commit 12ac69b9700).
- [x] Ф.7.2: stdlib HashMap.@clone() idiomatic (commit 4a6532ccea5).
- [x] Ф.7.3: sizeof warning для больших tuples (commit a27e1968040).
- [-] Ф.7.4: named tuple fields — deferred до dedicated plan.
- [-] Ф.7.5: full mono'd Result — deferred (Fix F+ покрывает наблюдаемое).
- [-] Ф.7.6: tuple subtyping — design only, не реализуется без use case.

### Plan 59 финальное состояние

- 7 phases: 6 закрыты полностью, Phase 7 — M-priority items закрыты,
  L-priority deferred с rationale.
- 25 regression tests (f1-f25) в nova_tests/plan59/, все PASS.
- 2 dependent M-markers закрыты (Plan 63 Fix E + Fix F + Fix F+).
- spec D123 + D-block amendments.

**Validation:** `nova test plan59/` → 25 PASS / 0 FAIL.


---

## Ф.7.5 — Full mono'd Result: executable implementation plan (2026-05-20)

> **Контекст.** User: «продолжай работать по плану 59 Ф.7.5 сам по всем
> оставшимся пунктам без упрощений как для прода». Снят defer Ф.7.5.
> Работа в worktree `nova-p59`, ветка `plan-59-f75`.

### Дизайн-решение (зафиксировано)

`Result[T, E]` → mono'd **`NovaRes_<okC>_<errC>*`** — heap-pointer (как
legacy `Nova_Result*`), но per-(T,E) **типизированный payload union**
`{ <T> ok; <E> err; }`. Pointer (а не value как `NovaOpt`) выбран
сознательно: даёт корректность Ф.7.5 (типизированные Ok/Err, отсутствие
int-slot truncation, корректная inference) при **нулевом
calling-convention churn** vs legacy → максимальный шанс довести до
зелёного. Value-репрезентация ортогональна фиче и осталась бы возможной
future-микрооптимизацией.

- **Erased-generic** `Result[T,E]` (T/E — type-param) → fallback
  `NovaRes_nova_int_nova_str*` (int/str instance как ABI-compat erased
  repr — прямой аналог fallback'а `NovaOpt_nova_int` у Option).
- Mangling: `NovaRes_<sanitize(okC)>_<sanitize(errC)>` через
  `sanitize_for_novaopt` — параллель NovaOpt.
- Legacy `Nova_Result` typedef в `array.h` остаётся (безвреден), codegen
  перестаёт его эмитить.

### Инкремент 1 — ✅ ЗАКРЫТ (commit `9463cf1d83a`)

Аддитивная инфраструктура (build green, поведение не изменено):
- `register_novares_decl(ok_c, err_c)` (emit_c.rs) — lazy-эмитит typedef
  `NovaRes_<n>` + heap-конструкторы `nova_make_NovaRes_<n>_Ok/_Err` +
  trampoline-методы `Nova_Result_method_{is_ok,is_err,unwrap_or,ok,err}_<n>`.
- Поля `novares_typedefs_buf` / `novares_decls_seen`; splice-маркер
  `/*__NOVARES_TYPEDEFS__*/` (после `/*__NOVAOPT_TYPEDEFS__*/`).
- `result_mono_c_pair` / `novares_name` helpers.
- `type_ref_to_c[Result]` регистрирует mono typedef для concrete (T,E).

### Инкремент 2 — ядро (переключение представления)

**Природа задачи:** атомарная замена представления. ~40 сайтов
codegen используют **string-based dispatch** (`obj_ty == "Nova_Result*"`)
— Rust-компилятор НЕ ловит рассинхрон (строковое сравнение просто
перестаёт матчиться); единственный safety net — падение тестов.
Поэтому ядро — test-driven, focused-сессия.

Регион за регионом (file:line — карта на момент 2026-05-20):

1. **`register_novares_decl`** → pointer-форма (конструкторы heap-alloc
   + return `NovaRes_<n>*`; методы принимают `NovaRes_<n>*`, `->`).
2. **`type_ref_to_c[Result]`** (emit_c.rs ~3282) → возвращать
   `NovaRes_<n>*` (concrete) / `NovaRes_nova_int_nova_str*` (erased),
   через `result_mono_c_pair`.
3. **Конструирование `Ok(v)`/`Err(e)`** (~12711-12753) → эмитить
   `nova_make_NovaRes_<n>_Ok/_Err`; (T,E) из инференции арг-типа +
   контекста (current_fn_return_ty / let-аннотация). Удалить ветку
   `nova_make_Result_Err_typed` (hybrid больше не нужен — Err типизован).
4. **9 методов** (~13372-13612) — per-(T,E) c_name + `->payload.ok` /
   `->payload.err` (было `->payload.Ok._0` / `->payload.Err._0`):
   - trampoline: `is_ok`/`is_err`/`unwrap_or`/`ok`/`err` → `_<n>` suffix;
   - inline: `unwrap`/`map`/`map_err`/`unwrap_or_else`.
5. **Pattern-match** (~16447-18049) — `collect_pattern_inner_bindings`,
   `pattern_cond`, `pattern_bind_typed`: `NovaRes_<n>*`, `->tag`,
   `->payload.ok`/`.err`. Recover (T,E) из имени типа `NovaRes_<ok>_<err>*`.
6. **`?` / `!!`** (~11733-11826) — `ExprKind::Try`/`Bang` для Result:
   `->payload.ok`/`.err`; propagate `nova_make_NovaRes_<n>_Err`; `!!` →
   `Nova_Fail_fail` / `nova_throw_typed` напрямую по типизованному Err.
7. **`infer_expr_c_type`** (~20914) — method return-types на базе
   разобранного `NovaRes_<ok>_<err>*` (распарсить mangled имя).
8. **`sum_schema_registry`** (~375-439) — Result schema: payload-поля
   `ok`/`err`; method routing — per-(T,E) `is_per_t`-аналог.
9. **Все `== "Nova_Result*"` строковые проверки** (~41 шт в emit_c.rs)
   → helper `is_novares_c(ty) -> Option<(okC, errC)>` (распознаёт
   `NovaRes_<ok>_<err>*`), заменить точечные сравнения.

**Миграция stdlib + tests:** `.nv`-исходники Result НЕ ссылаются на
C-типы (всё на Nova-уровне: `Result[T,E]`, `Ok`/`Err`, `match`, `?`).
Поэтому миграция = «codegen корректен для всех паттернов» — проверяется
прогоном, **без правок `.nv`** (если не всплывёт stdlib-workaround).

**Тест-стратегия:** plan72 (Result-heavy) → runtime → plan62 → полный
прогон 868. Erased-generic кейсы — особое внимание (ABI-fallback).

**Риск:** нет compiler safety net (string-dispatch); средний период
red-codegen; erased↔concrete boundary — потенциальные cast-mismatch'и
(как у Option, [C2]-уровень).

### Ф.7.5-lite — прицельный фикс инференса (2026-05-20) — ✅ ЗАКРЫТ

> User: «запиши в план работ и делай». Полное переключение представления
> (инкремент 2) — отдельная focused-сессия. Прицельно закрываем САМ
> блокер `[M-result-method-named-var-only]` без рефактора представления.

**Проблема.** `infer_expr_c_type` и emit Result-методов выводят (T,E)
только когда receiver — именованная переменная (`result_type_params`
keyed by var name). Inline-цепочки (`parse_x().unwrap_or(...)`,
`parse_x().map(f).unwrap_or(...)`) → fallback `(nova_int, nova_str)` →
для pointer-Ok-типов wrong result, для int/bool «случайно» проходит.

**Фикс.** Helper `infer_result_type_params(expr) -> Option<(ok_c,err_c)>`:
- `Ident` → `result_type_params`;
- `Call(fn)` где fn возвращает `Result[T,E]` → `fn_result_type_params`
  (инфра Plan 72 P2-A, `call_result_type_params_key`);
- `.map`/`.map_err` цепочки → рекурсия + closure return-type
  (`typed_closure_c_sig`).
Применён в 5 точках (inference Result-методов + emit `unwrap_or` /
`unwrap_or_else` / `map` / `unwrap`) вместо паттерна «Ident-или-default».

**Закрывает:** inline fn-call Result-цепочки — основной кейс блокера.
Остаток (Result из field-access / user-method) — узкий edge вне
documented-блокера. **Не меняет представление** — `Nova_Result*`
остаётся; инкремент 2 (полная mono) — отдельно.

**Результат:** helper `infer_result_type_params` + 5 точек применения +
тест `plan59/f29_result_inline_inference` (8 inline-кейсов). Полный
прогон **869 PASS / 0 FAIL / 52 SKIP**, 0 регрессий. Блокер
`[M-result-method-named-var-only]` для основного кейса снят.

### Ф.7.5 ядро (инкремент 2) — декомпозированный план A-E (2026-05-20)

> Прежний «executable план ядра» трактовал переключение представления
> как один атомарный блок (~50 сайтов). Это и было главным риском.
> Здесь — декомпозиция, где A/B/C/E **independently green**, а
> атомарный риск сжат до меньшего шага D.

**Ключевое дизайн-решение:** mono-тип `NovaRes_<n>` использует **ту же
payload-схему**, что legacy `Nova_Result`:
`union { struct { <T> _0; } Ok; struct { <E> _0; } Err; }` — доступ
`payload.Ok._0` / `payload.Err._0`. Следствие: **generic pattern-match
codegen работает для `NovaRes_<n>` без изменений** (он эмитит
`payload.<Variant>._<idx>`), и ~20 сайтов field-access **не трогаем**.
Отличие mono от legacy — только: (1) типы полей payload (реальные T/E
вместо `nova_int`/`nova_str`), (2) имя типа.

**Шаги:**

- **A** — `register_novares_decl`: pointer-форма, payload-схема
  `Ok._0`/`Err._0` (как legacy), + helpers (`result_mono_c_pair`,
  `novares_name`, `novares_ok_err`, поле `novares_value_types`).
  Аддитивно — `type_ref_to_c` пока возвращает `Nova_Result*`, новое
  ничем не используется → **green, коммит**.
- **B** — helper `is_result_like(ty)` принимает И `Nova_Result*`, И
  `NovaRes_<n>*`. Все `obj_ty == "Nova_Result*"` проверки (~15) → через
  него. Аддитивно (поведение не меняется — `NovaRes_<n>*` ещё не течёт)
  → **green, коммит**.
- **C** — inference (`infer_expr_c_type`) + emit Result-методов
  восстанавливают (T,E) из обоих типов: `Nova_Result*` → дефолт
  `(nova_int, nova_str)`; `NovaRes_<n>*` → `novares_ok_err`. Аддитивно
  → **green, коммит**.
- **D** — **флип** (атомарное ядро, но меньшее): `type_ref_to_c[Result]`
  → `NovaRes_<n>*`; construction `Ok`/`Err` → `nova_make_NovaRes_<n>_*`;
  method-dispatch → суффикс `_<n>`. Теперь `NovaRes_<n>*` течёт везде —
  B обрабатывает dispatch, C — инференс, pattern-match (generic,
  `Ok._0`) работает т.к. payload-схема совпадает. Прогон → fix → green.
- **E** — удалить legacy `Nova_Result` + `nova_make_Result_*` +
  `Nova_Result_method_*` из `array.h` (после D ничего не ссылается) →
  **green, коммит**. ← разблокирует Plan 62.A.bis Ф.4 (удаление
  legacy `sum_schemas` — `[M-legacy-sum-schemas-retained]`).

**Закрывает:** `[M-legacy-sum-schemas-retained]` (через E + 62.A.bis
Ф.4), `[M-result-record-payload-match]` (D — Ok-payload типизируется
реальным T), остаток `[M-result-method-named-var-only]`.

**Тест-стратегия:** после каждого A/B/C/E — полный прогон 0 регрессий;
D — итеративно (plan72 → runtime → plan62 → full). Erased-generic
кейсы — особое внимание (fallback `NovaRes_nova_int_nova_str*`).
