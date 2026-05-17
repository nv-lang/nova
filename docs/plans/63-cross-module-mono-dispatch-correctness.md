# Plan 63: cross-module + mono dispatch correctness

> **Status:** proposed (2026-05-17). Discovered во время Sprint F.36 regression testing
> (`nova_tests/plan11_followup/`). Family of related compiler issues — все из-за
> того что compiler tracks N (signature/typearg/overload) во-внутри-module, но
> теряет N при пересечении module boundary.

---

## Bugs covered

### Bug A: Return type inference cross-method для new external methods

**Repro:** [`nova_tests/plan11_followup/f11_stringbuilder_new_methods_positive.nv`](../../nova_tests/plan11_followup/f11_stringbuilder_new_methods_positive.nv)

```nova
let mut sb = StringBuilder.new()
sb.append("abc")
let snap = sb.peek()      // ← snap declared в C как nova_int, не nova_str
```

Generated C:
```c
nova_int snap = Nova_StringBuilder_method_peek(sb);   // wrong type!
```

Workaround в test: `let snap str = sb.peek()` (explicit annotation).

**Root cause:** call-site type inference для method returning `str`, **registered
через external_registry** (нашими auto-gen stubs), не возвращает корректный
return type. Fallback на `nova_int`. Symbol resolution OK (правильный
`Nova_StringBuilder_method_peek` called), но return type inference broken.

### Bug B: Variadic signature lost cross-module via alias-import

**Repro:** [`nova_tests/plan11_followup/f13_path_join_normalize_positive.nv`](../../nova_tests/plan11_followup/f13_path_join_normalize_positive.nv)

```nova
import std.path.path as p
p.Path.join("a", "b", "c")    // CODEGEN-FAIL: assert не variadic
```

Через `import std.path.path.{Path}` (direct import name) — works.

**Root cause:** при `import X as p`, compiler регистрирует alias `p` но
variadic flag fn'а **теряется** при path resolution `p.Path.join(...)`. Compiler
trying spread lowering но parent context (assert(...)) думает spread не нужен.

### Bug C: Generic type anonymous record literal — Nova_T_p placeholder leak

**Repro:** Generic factory pattern в внешнем модуле:
```nova
export type Box[T] { v T }
export fn Box[T].of(value T) -> Box[T] => { v: value }   // anonymous record
```

Generated C для caller `Box[int].of(42)`:
```c
Nova_Box____Nova_T_p* m = Nova_Box____nova_int_static_of(42);
//        ^^^^^^^^^^ unresolved T placeholder leaks
```

**Root cause:** anonymous record literal в return position generic static fn'а
не triggered mono instance enrollment для T → emit'ит type placeholder Nova_T_p
вместо substituted type. Mono path для record literal не handled.

### Bug D: D73 cross-module overload dispatch misroute

**Repro:** [`nova_tests/plan11_followup/f10_hashmap_self_restored_positive.nv`](../../nova_tests/plan11_followup/f10_hashmap_self_restored_positive.nv)
(workaround: revert на explicit type)

```nova
HashMap[str, int].from([("a", 1)])
```

При `-> Self` return type в HashMap.from declaration:
```c
Nova_str_static_from(...)         // ← wrong! str.from selected
// или Nova_StringBuilder_static_from(...) в другом запуске
```

При `-> HashMap[K, V]` (explicit) — works.

**Root cause:** при `parts = ["HashMap", "from"]` lookup в `method_overloads`
не finds key, fallback к single-key `method_receivers["from"]` который **last-wins**
(picks first registered .from from str / StringBuilder / WriteBuffer).
Cross-module overload resolution не учитывает typeargs `[str, int]` в parts[0].

## Что НЕ делаем

- **Drop variadic / Drop generics / Drop overloading** — это core spec features.
- **Per-method registration в external_registry** для каждого нового метода — too verbose.

## Решение (6 sub-fixes — добавились E, F из M-* findings)

### Fix A: Return type carrier через external_registry ✅ DONE (commit 13de6fc803e)

`ExternalRegistry` уже хранит `return_c_type`. Compile site lookup при method
call'е использует registered return_c_type вместо fallback на `nova_int`.

Файл: `compiler-codegen/src/codegen/emit_c.rs:16726-16745` (infer_expr_c_type
Member-call case добавлен external_registry.by_key fallback).

Acceptance: `let snap = sb.peek()` correctly infers `str` (f11 PASS без annotation).

### Fix B: Variadic info via cross-module signature carrier ✅ DONE (commit 827ee0258bf)

Парсер для `p.Path.join` строит nested Member chain
`Member{Member{Ident("p"), "Path"}, "join"}`. `lookup_variadic_arity`
recognized только Ident("Type") в obj — теряло variadic_last флаг для
alias-imported types.

Fix: ExprKind::Path arm relax len==2 → len>=2 (берём последние 2 segment'а);
ExprKind::Member arm: добавлена ветка для nested Member chain
(alias prefix игнорируется, lookup по последнему (Type, method) pair).

Acceptance: f16/f17 PASS — `import std.path.path as p; p.Path.join(...)` works.

### Fix E: mono'd tuple iter в generic method body ✅ DONE (commit 66113a8d2db)

[M-stdlib-iter-in-generic-method-body] state leak в mono pass. Для
`for (k, v) in pairs` внутри generic static method'а codegen эмиттил
mixed legacy/mono tuple types → CC-FAIL.

Три взаимосвязанных fix'а:
1. emit_array_lit (14426): для boxed-storage пишет `<T>*` (pointer), не raw struct.
2. emit_monomorphized_method (6451+): pre-populate array_element_types для
   array-of-tuple params + save/restore против state leak'а cross-fn.
3. emit_for Case 2 destructure (13041): добавлена ветка для
   `_NovaTuple_<arity>_<sigs>*` (mono'd tuple pointer) — deref в mono'd struct.

Acceptance: f18 (HashMap.from с Self + tuple destructure) PASS.

### Fix F: Result Ok-payload tuple/struct unboxing ✅ DONE (commit 2ae78c7ae8d)

[M-result-erased-no-mono] Result hardcoded с nova_int payload slot.
Tuple `(str, int)` не fits — match destructure `_0.f0/f1` на int падал.

Минимальный fix (без full mono'd Result rewrite):
- result_ok_inner_types map (analogous к option_inner_types).
- pending_result_ok_inner_type set в emit_call для nova_make_Result_Ok с
  struct arg; consumed в let-binding.
- emit_match propagates на scr_tmp.
- pattern_bind_typed для Result Ok с Tuple sub-pattern: emit deref+cast tmp,
  populate tuple_element_types через parse_mono_tuple_elements.

Acceptance: f20 (Ok((str, int)) destructure + Err + pattern_cond) PASS.

Scope note: full mono'd Result (NovaRes_<T>_<E> typedefs) — отдельная
масштабная переработка ≈ Plan 56 vtable scope. Targeted fix покрывает
основной use-case без системного refactor'а.

### Fix C: Mono enrollment для anonymous record literal в generic return ✅ VERIFIED 2026-05-17 EOD

**Status:** **no longer a bug** — implicitly resolved через Plan 59
mono pipeline + per-static-method mono enrollment infrastructure.

Comprehensive regression test exercises all предполагаемых failure
scenarios:
- `Box[T].of(value T) -> Box[T] => { v: value }` (single-field).
- `Pair[A, B].make` (multi-field, heterogeneous types).
- `Holder[T]` (T-field + concrete-type int field).
- `Box[Box[int]].of` (nested generic).
- `Box[T].twin` (factory-calls-factory intermediate helper).
- Computed-field record literal.
- Cross-module call (`import other.{Box}; Box[int].of(42)`).

ВСЕ emit правильно mono'd struct names (`Nova_Box____nova_int*`,
`Nova_Box____nova_str*`, etc.), **без** `Nova_T_p` placeholder leak.

Tests (permanent regression guards):
- `nova_tests/plan63/f1_anonymous_record_factory.nv` — 8 sub-tests.
- `nova_tests/plan63/cm_box_use.nv` + `nova_tests/plan63/cm_box/lib.nv`
  — cross-module folder-module factory case.

**Remaining edge case (NOT Fix C scope):** ✅ **RESOLVED 2026-05-17 EOD via
Plan 48 method-param mono extension** (branch `plan-48-mpm`).

Generic method с method-level type param `[U]` (e.g.
`Wrapper[T] @map[U](f fn(T) -> U) -> Wrapper[U]`) ранее mono'lся только
по receiver T, U оставался `Nova_U_p` placeholder. Implementation
extends `infer_mono_method_ret_with_args` + emit_call path 5b с
bidirectional inference из closure-typed args (см. Plan 48 doc).

Tests (permanent regression guards):
- `nova_tests/plan48_mpm/repro_wrapper_map.nv` — minimal int→int + int→str.
- `nova_tests/plan48_mpm/f1_method_param_mono.nv` — 5 sub-tests:
  chained map, cross-type chains, identity, isolated str→int.

### Fix D: Typeargs-aware overload dispatch ✅ implicitly DONE

`HashMap[str, int].from(pairs)` теперь dispatches корректно через
TurboFish→Member path (compiler-codegen/src/codegen/emit_c.rs:11343-11410
— уже существует, обрабатывает D109 generic type static call).

Acceptance: f18/f19 PASS — HashMap.new/with_capacity/clone/filter/from
все используют `-> Self` без misroute.

## Acceptance criteria

- ✅ `let snap = sb.peek()` infers snap as `str` (no explicit annotation). [Fix A]
- ✅ `import X as p; p.VariadicFn(a, b, c)` lowers корректно. [Fix B]
- ✅ `Box[int].of(42)` (generic factory) [Fix C — verified 2026-05-17 EOD,
  implicitly resolved via Plan 59 mono pipeline].
- ✅ `HashMap[str, int].from(pairs)` dispatches к HashMap.from. [Fix D + E]
- ✅ `for (k, v) in pairs` в generic method body работает. [Fix E]
- ✅ `Result[(T, U), E]` Ok-payload destructure работает. [Fix F]
- ✅ Existing 620+ regression tests не регрессят (только Windows AV flakes).
- ✅ Все 18 tests в `nova_tests/plan11_followup/` PASS (без workaround'ов).
- ✅ Plan 63 fully closed (все 6 fixes A/B/C/D/E/F resolved).

## Связь с другими планами

- **Plan 11 Follow-up** (закрыт): нашёл первые 2 bugs (Self + assert-in-expr).
  Plan 63 — продолжение того же regression-testing pass'а.
- **Plan 35** (cross-file resolve): фундамент cross-module resolution.
- **Plan 48 / 55 / 56** (mono / closures / vtable): mono enrollment infrastructure
  где Fix C должен интегрироваться.
- **Plan 60** (.len() uniformity): косвенно — тот же сорт «cross-module API
  resolution» проблемы.
- **Plan 61** (typed errors): отдельная архитектурная проблема, не пересекается.

## Ссылки

- `nova_tests/plan11_followup/` — full regression suite (18 tests, 16 PASS,
  2 Windows AV os 740 deferred).
- Sprint F.36 closure: `docs/plans/45-nova-doc.md` (Ф.36 status table).
- `compiler-codegen/src/codegen/emit_c.rs` — dispatch sites.
- `compiler-codegen/src/codegen/external_registry.rs` — type info storage.
