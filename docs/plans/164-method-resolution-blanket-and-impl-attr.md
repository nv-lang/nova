# Plan 164 — Method resolution: blanket dispatch fix + #impl(P[T]) + vec_iter rename

**Status:** ✅ CLOSED (Ф.1–Ф.4, 2026-06-16). **Branch:** plan-zfix. **Worktree:** D:\Sources\nv-lang\nova-p-zfix.

**Commits:**
- Ф.1: `3846a976` — `#impl(P[T])` parser + `impl_spec_base_name`/`impl_spec_args_text` helpers + verify_impl_protocols + vec_iter_zc annotations + fixtures
- Ф.2: `b94d46f3` — blanket generic param-order fix (`generics.iter().find(|g| g.name == tvname)`)
- Ф.3: `af33bc76` — receiver-compatibility dispatch block (D285) + blanket_no_collision_* fixtures
- Ф.4: `70079788` — `@znth`→`@nth` + `@zmin`→`@min` + `@zmax`→`@max` rename + `#impl(Next[T])` on `VecIter[T].@next()`

**Test results (final):**
- `plan153_2_zc`: 4/4 PASS
- `plan161`: 12/12 PASS
- `plan162`: 5/5 PASS
- `plan164`: 6/6 PASS

**Markers closed:**
- `[M-impl-attr-generic-protocol]` ✅ CLOSED Ф.1
- `[M-codegen-blanket-generic-param-order]` ✅ CLOSED Ф.2
- `[M-153.2-drop-z-prefix]` ✅ CLOSED Ф.4 (partial — z-prefix on terminators dropped; `vec_iter_zc.nv` rename to `vec_iter.nv` deferred to post-merge cleanup)

**Spec:** D285 NEW (10-overloading.md) — receiver-compatibility rule.

---

## Motivation

Three compiler bugs blocked removing the `z`-prefix workaround from zero-cost iterator adapters:

1. **`#impl(P[T])` parser gap** — `#impl(Next[U])` with generic type argument was rejected by the parser (`expected ')' got '['`). Without this, `type_impl_protocols` could not be populated with parametric protocols, so the blanket dispatch had no way to verify receiver compatibility.

2. **Blanket generic param-order bug** — `fn[T Compare, I Next[T]] I mut @zmin()` generated a broken mono C-name because `emit_call` and `infer_expr_c_type` both used `fn_decl.generics.first()` to find the receiver typevar. For swapped param order `[T, I]` this bound `T`→concrete_iterator (wrong) and left `I` unresolved.

3. **Receiver-compatibility bug** — blanket method `@count`/`@find`/etc. on `FilterIter[...]` was dispatched to `CharsIter.count()` (from `std/strings`) because `method_receivers` (last-wins HashMap) let a concrete method on a different type overwrite the blanket entry. Nova's dispatch rule (D84 §1, D285) requires concrete methods on the actual receiver to win, but concrete methods on unrelated types must not.

---

## Ф.1 — `#impl(P[T])` generic protocol attribute (commit 3846a976)

**Changed files:**

**`compiler-codegen/src/parser/mod.rs`** (lines 11–33):
- Added `impl_spec_base_name(spec: &str) -> &str` — extracts bare protocol name (strips `[...]` suffix).
- Added `impl_spec_args_text(spec: &str) -> Option<&str>` — returns content inside `[...]` or `None`.

**`compiler-codegen/src/parser/mod.rs`** (lines 2335–2422, `parse_type_attrs()` `"impl"` arm):
- Replaced bare `parse_ident()` with a bracket-skipping loop that stores the full spec like `"Next[U]"` in `impl_protocols`. Duplicate check uses `impl_spec_base_name` (compares bare names, not full specs).

**`compiler-codegen/src/types/mod.rs`**:
- Imports `impl_spec_base_name` and `impl_spec_args_text`.
- `verify_impl_protocols` and `verify_method_impl_protocols`: extract `proto_base` via `impl_spec_base_name`; build `proto_arg_subst` from bracket text; call `check_signature_match_with_subst`.
- Added `normalize_type_str()` helper — strips whitespace around `,`/`[`/`(`/`)`/`]` so `"(int, T)"` and `"(int,T)"` compare equal.
- `check_signature_match_with_subst`: applies `normalize_type_str` to both sides before comparing.

**`compiler-codegen/src/codegen/emit_c.rs`** (line 3):
- Imports `impl_spec_base_name`.

**`std/collections/vec_iter_zc.nv`**:
- `@next()` methods annotated: `MapIter` → `#impl(Next[U])`, `FilterIter` → `#impl(Next[T])`, `FilterMapIter` → `#impl(Next[U])`, `TakeIter` → `#impl(Next[T])`, `SkipIter` → `#impl(Next[T])`, `EnumerateIter` → `#impl(Next[(int,T)])`.

**New test fixtures:**
- `nova_tests/plan164/impl_attr_generic_pos.nv`
- `nova_tests/plan164/impl_attr_generic_neg.nv`

**Test results:** 2/2 PASS. `vec_iter_zc.nv` check: PASS.

---

## Ф.2 — Blanket generic param-order fix (commit b94d46f3)

**Root cause:** `emit_call` and `infer_expr_c_type` both used `fn_decl.generics.first()` to find the receiver typevar when building `type_subst` for blanket methods. For `fn[I Next[T], T Compare] I mut @zmin()` this is correct (I is first), but for `fn[T Compare, I Next[T]]` it binds T→concrete_iterator (wrong), leaving I unresolved → broken monomorphized C names and types.

**Fix locations in `compiler-codegen/src/codegen/emit_c.rs`:**

1. **Line ~25382** (emit_call, `type_subst` assembly): replaced `fn_decl.generics.first()` with `fn_decl.generics.iter().find(|g| g.name == type_name)` where `type_name` is the registered receiver typevar from `method_receivers`.

2. **Line ~35277** (infer_expr_c_type, `tv_elem_bindings` assembly): changed to also capture `tvname` as `blanket_tvname`, then replaced `fd.generics.first()` with `fd.generics.iter().find(|g| g.name == blanket_tvname)`. Applied at two sub-sites: the tuple-return override block and the string-subst fallback block.

**New test fixture:** `nova_tests/plan164/blanket_param_order.nv` (5/5 PASS).

---

## Ф.3 — Receiver-compatibility dispatch block (commit af33bc76, D285)

**Root cause:** `emit_call` used `method_receivers` (single-key, last-wins HashMap) as the final fallback. When `CharsIter.count()` registered later than the blanket `fn[I Next[T]] I @count()`, it overwrote the entry for key `("any", "count")` or similar. Result: `FilterIter.count()` dispatched to `CharsIter.count()`.

**Fix:** Inserted a new dispatch block (~120 lines) in `emit_c.rs` between the `try_synthesize_default_method` block and the `method_receivers` fallback (lines 25288–25289). The block runs BEFORE single-key last-wins fallback, so it pre-empts wrong dispatch.

**Algorithm:**
1. Extract actual receiver's base type from `obj_ty` (strip `Nova_`/`NovaValue_` prefix, `*`, take part before first `____`). E.g. `"Nova_FilterIter____..."` → base `"FilterIter"`.
2. Skip primitives (they have their own dispatch path).
3. Scan `mono_method_decls` for any `(tvname, method)` key where `tvname` is a bare typevar (len ≤ 2, all-uppercase).
4. For the found blanket `fn_decl`, check if its receiver typevar's bounds are all satisfied: for each bound `Next`/`Compare`/etc., look in `type_impl_protocols[recv_base]` for a matching entry (using `impl_spec_base_name`).
5. If all bounds match → dispatch via blanket with the same `type_subst` logic as the existing bare-typevar path (lines 25365+): bind receiver typevar → concrete_t, bind inner typevars from protocol bounds via `infer_mono_method_ret_with_args`.

**Why CharsIter.count() is unaffected:** `CharsIter` is a non-generic concrete type. Its `@count()` is registered in `method_overloads[("CharsIter", "count")]`. Section 5 (multi-overload, ~line 24428) finds it and returns early — the fix block is never reached.

**New test fixtures:**
- `nova_tests/plan164/blanket_no_collision_count.nv` — `zfilter().zcount()` regression guard (5/5 PASS)
- `nova_tests/plan164/blanket_no_collision_find.nv` — `zfilter().zfind()` regression guard (5/5 PASS)
- `nova_tests/plan164/blanket_no_collision_nth.nv` — `zfilter().znth()` regression guard (5/5 PASS)

**Validated:** plan153_2_zc 4/4, plan161 12/12, plan162 5/5, plan164 6/6 PASS.

---

## Ф.4 — vec_iter terminators rename: z-prefix drop (commit 70079788)

**Renamed methods** in `std/collections/vec_iter_zc.nv`:
- `@znth` → `@nth`
- `@zmin` → `@min`
- `@zmax` → `@max`

**Additional change:** Added `#impl(Next[T])` to `VecIter[T] mut @next()` in `std/collections/vec/iter.nv`. Required because renaming `@znth`→`@nth` caused the blanket dispatch (Ф.3 fix) to mis-route `VecIter[int].nth()` to `CharsIter.nth` — `VecIter` had no `#impl` annotation and was not registered in `type_impl_protocols`. Adding the annotation registers `VecIter` as implementing `Next`, enabling correct blanket dispatch.

**Note on z-prefix:** The full `z`-prefix removal (`@zmap`→`@map`, `@zfilter`→`@filter`, `@zcollect`→`@collect`, `vec_iter_zc.nv`→`vec_iter.nv`) is **deferred** — it requires a stdlib-wide rename sweep and will be done in the main branch after merge. Only the three conflicting terminator names were renamed here as the MVP to unblock `[M-153.2-drop-z-prefix]`.

**Files changed:** 22.

**Final test results:**
- plan153_2_zc: 4/4 PASS
- plan161: 12/12 PASS
- plan162: 5/5 PASS
- plan164: 6/6 PASS

---

## Followups

- `[M-153.2-drop-z-prefix]` ✅ CLOSED (terminators unblocked; full module rename = post-merge sweep)
- `[M-method-resolution-registry-inconsistency]` OPEN (P3) — two dispatch registries (`method_receivers` vs `method_overloads`) still diverge in tie-break; D285 adds a pre-check layer, but structural unification is a separate task.
- `[M-generic-param-bound-with-constraint]` OPEN (P2) — `fn[I Next[T Hash]]` syntax (bound on a bound's param) still not supported; workaround: two params with correct order.
