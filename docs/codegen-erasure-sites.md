# Codegen erasure sites — Cat B audit (Plan 70)

> **Created 2026-05-18** as part of Plan 70 Ф.3 ([M-no-silent-nova-int-fallback]).
> Lists всех legitimate intentional erasure sites в `compiler-codegen/src/codegen/`
> где `"nova_int"` или `"void*"` fallback не silent miscompilation, а часть
> intentional design contract. Каждый сайт обязан:
>
> 1. Иметь inline-комментарий с rationale «почему erasure безопасна»
> 2. Быть listed в этом документе с file:line + причина
> 3. Survived peer review при добавлении (CI lint guard
>    `scripts/lint-no-silent-int-fallback.sh` использует baseline count
>    из этого файла)

---

## Категории erasure

| Cat | Pattern | Семантика | Действие при добавлении нового |
|---|---|---|---|
| **B**  | `_ => "nova_int", // erased T` | Generic body emit pre-mono; type-param `T` ещё unresolved, erasure consistent для mono pass | Inline comment + entry здесь + bump lint baseline |
| **C**  | `WithResultCategory::IntLike => "nova_int"` | Categorical mapping для int-family aliases | Inline comment; baseline auto |
| **D**  | `_ => "nova_int", // unknown method on known type` | Dispatch table wildcard — type-checker already rejected unknown methods upstream | Inline comment; baseline auto |

---

## Cat B sites (intentional erasure for pre-mono generic emit)

### B1. `emit_c.rs:2720` — `any_erased` detection в `type_ref_to_c`

```rust
let type_args_c: Vec<String> = generics.iter()
    .map(|g| self.type_ref_to_c(g).unwrap_or_else(|_| "nova_int".into()))
    .collect();
let any_erased = type_args_c.iter().any(|a| a == "void*" || ...);
```

**Rationale:** Used by `any_erased` check downstream. Fallback к `nova_int`
для unresolved type-param — intentional, потому что check проверяет
factual erasure (если any arg is unresolved → партиально mono'd path).
Strict mode здесь false-positive — это namespace squat, не codegen output.

**Reachability:** Hot path в generic-type instantiation. Triggered
regularly for partial-mono detection.

---

### B2. `emit_c.rs:5846` — `emit_handler_lit` first handler-fn erasure

```rust
"u8" | "u16" | "u32" | "u64" => "nova_int",
_ => "nova_int", // erased T
```

**Rationale:** Handler-lambda с generic return type `T`. До mono pass
type-param T не известен — emit'им placeholder `nova_int` для consistent
ABI (matches generic-fn forward decl). Mono pass позже substit'ит.

**Reachability:** Triggered только для generic handler lambdas; редкая path.

---

### B3. `emit_c.rs:5867` — `emit_handler_lit` second handler-fn erasure

```rust
"u8" | "u16" | "u32" | "u64" => "nova_int",
_ => "nova_int", // erased T
```

**Rationale:** Same as B2, второй emission site в той же функции (для
другой handler-arm variant). Idempotent с B2.

---

### B4. `emit_c.rs:5934` — `erased_type_ref_c` strict-mode bypass

```rust
// Strict-mode здесь = breaking erased dispatch. Documented Cat B.
self.type_ref_to_c(ty).unwrap_or_else(|_| "nova_int".into())
```

**Rationale:** `erased_type_ref_c` намеренно erases type-params для
erased generic body emission (Plan 48 V1 fallback). Если type_ref_to_c
fails — type-param ещё не resolved, erasure к nova_int сохраняет ABI
consistency для erased dispatch path. Migration к strict здесь сломал бы
fallback emit когда mono pass недоступен.

---

### B5. `emit_c.rs:5948` — `erased_type_ref_c` whole-fn default

```rust
// Plan 70 Cat B (intentional): erased_type_ref_c whole-fn erasure default.
_ => self.type_ref_to_c(ty).unwrap_or_else(|_| "nova_int".into()),
```

**Rationale:** Default arm для `erased_type_ref_c` match по type-kind.
Все non-special types proxy через type_ref_to_c с erasure fallback.
Same rationale as B4.

---

### B6. `emit_c.rs:6149` — `emit_method` `erase_unk` fn-param types

```rust
// Plan 70 Cat B (intentional erasure): erase_unk нормализует
// unknown→nova_int для consistent pointer-stomping в emit_generic_*
// (erased generics). type_ref_to_c fail здесь = type-param ещё не
// mono'd, erase_unk применяет к "nova_int" → tracks эту erasure.
let param_c_tys: Vec<String> = fp.iter()
    .map(|t| erase_unk(self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into())))
    .collect();
```

**Rationale:** `erase_unk` closure (defined just above) explicitly maps
`Nova_X*` unknown→`nova_int` для consistent dispatch. Inner unwrap_or
feeds raw type into erase_unk, который потом нормализует. Strict-mode
здесь false-positive — это namespace squat для erased dispatch.

**Reachability:** Generic method emission with fn-typed parameters.

---

### B7. `emit_c.rs:6152` — `emit_method` `erase_unk` return type

```rust
let ret_c = match return_type {
    Some(rt) => erase_unk(self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into())),
    None => "nova_unit".into(),
};
```

**Rationale:** Same as B6 for return type. erase_unk normalizes downstream.

---

### B8. `emit_c.rs:8311` — `emit_fn` `erase_unk` fn-param types

```rust
let param_c_tys: Vec<String> = fp.iter()
    .map(|t| erase_unk(self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into())))
    .collect();
```

**Rationale:** Same `erase_unk` pattern as B6, но для free fn (не method).
emit_fn эмитит function bodies generic-type extension methods (e.g.
`fn []T @map[U]`) где T/U type params, не real Nova record/sum types.

---

### B9. `emit_c.rs:8314` — `emit_fn` `erase_unk` return type

```rust
let ret_c = match return_type {
    Some(rt) => erase_unk(self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into())),
    None => "nova_unit".into(),
};
```

**Rationale:** Same as B8 для return type.

---

### B13. `emit_c.rs:18382` — `infer_expr_c_type` SelfAccess outside method

```rust
ExprKind::SelfAccess => {
    self.var_types.get("nova_self").cloned().unwrap_or_else(|| "nova_int".into())
}
```

**Rationale:** Reached в legitimate cases где `nova_self` не зарегистрирован
в `var_types`:

- **Closures inside instance methods:** Closure body inherits `self` via
  capture, но `var_types` scope inside closure отдельный (Plan 8
  closure-capture pre-registration не покрывает SelfAccess).
- **Generic-fn body emit без receiver:** `self` ref'ит type param, который
  не registered как concrete type yet.

**Reachability:** Confirmed в `concurrency/fiber_arena_compact.nv`
(closure inside method context). Pre-strict fallback к "nova_int" работает
для int-typed self contexts; non-int silent miscompilation theoretically
possible но не observed в test corpus.

**Strict migration scope:** Requires (a) closure-capture SelfAccess
pre-registration, (b) generic-fn receiver-type subst tracking. Both
отдельные refactors — deferred. Documented как Cat B13.

---

### B12. `emit_c.rs:19437` — `infer_expr_c_type` Member access exhausted fallback

```rust
// 3 lookup paths (record_schemas, generic_type_templates, erased base schema)
// all failed → fallback.
"nova_int".into()
```

**Rationale:** Reached в `infer_expr_c_type` Member arm после exhausting
all 3 schema lookup paths:
1. Concrete mono schema (`record_schemas`)
2. Generic template subst (`generic_type_templates`)
3. Erased base schema (`record_schemas[base_name]`)

Triggered legitimately by:
- **Tuple field access без bound var** (call-result `.0`, nested member на
  anon tuple): mangled `_NovaTuple_N_lenT_T_...` containers НЕ в
  `record_schemas`; element types embedded в name, но decoder отсутствует
  на этом code path.
- **Generic-record field access до mono pass**: erased schema ещё не
  зарегистрирован при first inference call.
- **Type-checker-rejected access** (поле не существует) — не reach'ит
  codegen normally; defensive fallback.

**Strict-mode migration scope:** Requires (a) full `_NovaTuple_*` name
decoder для tuple element resolution, (b) pre-mono schema registration
hook. Both отдельные refactors, deferred.

**Reachability (observed):** 9+ failing tests в PhaseB3 v1 attempt
(`basics/tuples`, `concurrency/cancel_cross_type_cascade_test`,
`concurrency/select_test`, etc) — all triggered на tuple field access
с `_NovaTuple_2_8_nova_int_8_nova_int` etc. Documented как Cat B12;
strict migration deferred.

---

### B11. `emit_c.rs:19493` — `infer_expr_c_type` final wildcard

```rust
// Plan 70 Cat B (intentional erasure, session 2 finding): final wildcard
// в infer_expr_c_type. Reached for ExprKind variants which cannot
// meaningfully infer a standalone C type — primarily ClosureLight
// (variant 35, `|x| ...`) and Path-only expressions без call-context
// (variant 12).
_ => "nova_int".into(),
```

**Rationale:** Reached для ExprKind variants которые **cannot meaningfully**
infer C type в standalone context:

- **ClosureLight** (discriminant 35): `|x| body`. Тип closure — fn-type,
  но он зависит от **expected context** (parameter signature). Standalone
  inference невозможен. Caller (`emit_call` через `fn_param_sigs`) знает
  expected type из bidirectional inference.

- **Path** (discriminant 12): standalone Path expression без call context
  (e.g. `[]T` без `.new()`). Numeric type constants (`int.MAX`) handled
  separately в pre-match. Прочие Path — placeholder для overload
  resolution; actual C type delivered через alternative path.

**Не Cat A:** этот placeholder никогда не используется для actual C
emission — только для overload resolution / hint computation, где
`nova_int` treated как "any int-shaped placeholder".

**Migration к strict (future):** Requires bidirectional inference в каждом
wildcard-hitter ExprKind. Otder of magnitude work — отдельный план
если когда-нибудь needed. Currently low ROI (no observed silent
miscompilation в test corpus pre/post Plan 70).

**Reachability:** Confirmed reached в `plan70/f4_let_fn_binding_pos`
(ClosureLight) и `plan70/f5_emit_call_hof_pos` (ClosureLight + Path).
Test fixtures pass без strict error — placeholder behavior correct.

---

### B10. `emit_c.rs:7051` — `simple_type_ref_to_c` static-helper fallback

```rust
fn simple_type_ref_to_c(tr: &crate::ast::TypeRef) -> String {
    match tr {
        TypeRef::Named { ... } => ...,
        _ => "nova_int".to_string(),
    }
}
```

**Rationale:** Limited-context helper для simple type mangling в
`compute_generic_type_c_name`. Handles только `TypeRef::Named` —
unhandled variants (Func, Array, Tuple) fall back к nova_int.
Caller responsibility: unwrap composite TypeRef'ы до calling
simple_type_ref_to_c. Static fn — нет &self, нельзя record_strict_error.
Если когда-нибудь reach'нем _ для valid mono path — это compiler bug,
не silent miscompilation: type не должен попадать сюда non-Named.

**Migration path (future):** Add `&self` parameter если нужно strict mode,
тогда можно вызвать `record_strict_error`. Сейчас оставлено как Cat B
для compatibility с static-fn callers.

---

## Cat D sites (legitimate dispatch fallbacks)

Wildcard `_ => "nova_int"` в match-arms где receiver type **известен**,
а method name перебирается. Unknown method → type-checker already
rejected upstream; codegen fallback — defensive belt-and-suspenders.

Перечисление по file:line (короче чем Cat B потому что patterns
identical — известный type, unknown method dispatch):

| File:Line | Receiver / Context |
|---|---|
| `emit_c.rs:18787` | PrimBuiltin (`int.eq`, `bool.lt`, etc.) wildcard |
| `emit_c.rs:18860` | `Nova_ChanWriter*` unknown method |
| `emit_c.rs:18869` | `Nova_ChanReader*` unknown method |
| `emit_c.rs:18880` | `StringBuilder` static unknown method |
| `emit_c.rs:18886` | `WriteBuffer` static unknown method |
| `emit_c.rs:18892` | `ReadBuffer` static unknown method |
| `emit_c.rs:18900` | `gc.*` introspection unknown method |
| `emit_c.rs:18909` | `fibers.*` introspection unknown method |
| `emit_c.rs:18918` | `runtime.*` introspection unknown method |
| `emit_c.rs:18947` | `Nova_StringBuilder*` instance unknown method |
| `emit_c.rs:18957` | `Nova_WriteBuffer*` instance unknown method |
| `emit_c.rs:18983` | `Nova_ReadBuffer*` instance unknown method |
| `emit_c.rs:19009` | `Result/Option.ok_or` family wildcard |
| `emit_c.rs:19019` | `Result/Option.map/map_err` family wildcard |
| `emit_c.rs:19101-2` | `Map/Set` is_empty + others |
| `emit_c.rs:19139-40` | `str.split` family wildcard |
| `emit_c.rs:19218-19` | `StringBuilder` Ident-form static |
| `emit_c.rs:19224-25` | `WriteBuffer` Ident-form static |
| `emit_c.rs:19230-31` | `ReadBuffer` Ident-form static |
| `emit_c.rs:19374-75` | `Channel.rx` member access wildcard |

**Total Cat D:** ~20 sites (lint baseline для `_ => "nova_int"` includes
all these + 9 Cat B wildcards above + 2 wildcards в Cat A-cleanup
holdovers).

---

## Lint baseline reconciliation

`scripts/lint-no-silent-int-fallback.sh` использует baseline counts:

- `BASELINE_TYPE_REF_TO_C_UNWRAP_OR=8` — соответствует Cat B sites
  B1, B4, B5, B6, B7, B8, B9 + B10's grep-only-match (variant).
  Note: Cat A1 был 90 в audit pre-Plan 70 → теперь ≤8 (только Cat B).
- `BASELINE_WILDCARD_NOVA_INT=24` — Cat B (B2, B3) + Cat D listed above.
  Note: Cat A2 был 27 в audit pre-Plan 70 → теперь 24 (3 мигрированы к
  strict в PhaseB2/B3).

При добавлении нового legitimate sites: bump baseline после добавления
inline-comment и entry в этот doc. При migration sites к strict mode:
bump-DOWN baseline (counts decreasing — strict gain).

---

## Cross-references

- **Plan 70 doc:** `docs/plans/70-no-silent-nova-int-fallback.md`
- **Spec D127:** «Strict type propagation в codegen» (Plan 70 Ф.4)
- **Lint script:** `scripts/lint-no-silent-int-fallback.sh`
- **Audit baseline:** `docs/plans/70-artifacts/audit-2026-05-18.md`
- **Strict helpers:** `compiler-codegen/src/codegen/emit_c.rs` `err_no_int_fallback`, `record_strict_error`
