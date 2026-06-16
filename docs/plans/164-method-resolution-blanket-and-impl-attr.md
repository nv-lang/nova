# Plan 164 — Method-resolution blanket fix + `#impl(P[T])` + z-prefix rename

**Status:** 🔵 IN PROGRESS  
**Branch:** `plan-zfix` · **Worktree:** `D:\Sources\nv-lang\nova-p-zfix`  
**Closes:** `[M-153.2-drop-z-prefix]`, `[M-codegen-blanket-generic-param-order]`, `[M-impl-attr-generic-protocol]`

---

## Motivation

Three linked compiler gaps block removing the `z`-prefix from `vec_iter_zc` iterator methods:

1. **`[M-impl-attr-generic-protocol]`** — `#impl(Next[T])` rejected by parser (only bare names accepted).
2. **`[M-codegen-blanket-generic-param-order]`** — mono-name depends on generic param order in `fn[...]`; `fn[T Compare, I Next[T]]` generates broken `Nova_VecIter____nova_int` (double `__`).
3. **`[M-153.2-drop-z-prefix]`** — `@count`/`@find`/`@nth` on `FilterIter[...]` resolves to `CharsIter.count()` (wrong concrete method) instead of the blanket `fn[I Next[T]] I mut @count()`. Root cause: method-resolution picks by name only, ignoring receiver-type compatibility.

The z-prefix (`@zmap`, `@zfilter`, …) is the current workaround for (3). Fixing (3) makes rename possible; (1)+(2) are independent improvements.

---

## Phases

### Ф.1 — Parser: `#impl(P[T])` generic protocol argument

**File:** `compiler-codegen/src/parser/mod.rs`

The `#impl(...)` attribute parser currently reads a bare identifier. Extend it to call `parse_type()` so any `TypeRef` is accepted — `#impl(Next[T])`, `#impl(Next[(int,T)])`, `#impl(Equal + Hash)` all become valid.

Find the impl-attr parse site (search `impl_protocols` or `KwHash`/attribute parse). Replace bare-ident read with `parse_type()`, store the string representation (or the TypeRef itself) in `Item::Fn.impl_protocols`.

**Acceptance:**
- `#impl(Next[T])` on `fn MapIter[I,T,U] mut @next()` → no parse error
- `#impl(Next[(int,T)])` on `fn EnumerateIter[I,T] mut @next()` → no parse error
- Checker validates: method name ∈ protocol methods, signature matches
- Negative: `#impl(NonExistentProtocol[T])` → `E_IMPL_UNKNOWN_PROTOCOL`
- 0 regressions on existing `#impl(Display)` / `#impl(Debug)` usages

### Ф.2 — Codegen: fix mono-name param-order sensitivity

**File:** `compiler-codegen/src/codegen/emit_c.rs`

Root cause: the mono-name builder iterates `fn.generics` in declaration order and uses them to build the C type name. When `T` appears before `I`, the name for `I` gets an extra `_` separator.

Find `emit_blanket_method` / mono-name generation for blanket fns. Make the name canonical regardless of param order — sort params by name, or derive the name only from the concrete receiver type (which is `I` — the first type-param that is the receiver), not the full param list.

**Acceptance:**
- `fn[T Compare, I Next[T]] I mut @zmin()` and `fn[I Next[T], T Compare] I mut @zmin()` generate identical C names
- `nova_tests/plan153_2_zc` 4/4 PASS with both param orders
- Add fixture `nova_tests/plan164/blanket_param_order.nv` testing `zmin`/`zmax` work with either order

### Ф.3 — Method-resolution: prefer receiver-exact over wrong-type concrete

**File:** `compiler-codegen/src/codegen/emit_c.rs` (method dispatch / `resolve_method` / `emit_method_call`)

Root cause: when resolving `expr.method()`, the resolver finds the first method named `method` in `method_receivers` (last-wins registry). If `CharsIter.count` was registered after the blanket `fn[I Next[T]] I mut @count()`, it wins — even though `expr` has type `FilterIter[...]`, not `CharsIter`.

Fix strategy:
1. When dispatching `recv_type.method_name()`, collect ALL candidates with that name.
2. Filter to candidates whose declared receiver type is compatible with `recv_type` (either: receiver IS `recv_type`, or receiver is a blanket `fn[I Next[T]]` and `recv_type` implements `Next`).
3. Prefer exact match over blanket; among blanket matches prefer the one registered for `recv_type`'s module.
4. Only fall through to a mismatched-type concrete if no compatible candidate exists.

Check: how does `type_impl_protocols` interact? After Ф.1, `#impl(Next[T])` on `@next()` registers `MapIter`/`FilterIter`/etc. in `type_impl_protocols` as implementing `Next`. The resolver can use this to confirm that `FilterIter` is a valid receiver for the blanket `@count`.

**Acceptance:**
- `FilterIter[VecIter[int], int].count()` resolves to blanket `fn[I Next[T]] I mut @count()`, not `CharsIter.count()`
- `FilterIter[...].find(pred)` resolves correctly
- `FilterIter[...].nth(n)` resolves correctly
- Add fixtures `nova_tests/plan164/blanket_no_collision_count.nv`, `blanket_no_collision_find.nv`, `blanket_no_collision_nth.nv`
- Existing `CharsIter` tests still pass (concrete method still resolved correctly for `CharsIter`)
- `str.find()` still resolves correctly

### Ф.4 — Rename `vec_iter_zc` → `vec_iter`, drop z-prefix

Now that Ф.3 is fixed:

1. `git mv std/collections/vec_iter_zc.nv std/collections/vec_iter.nv`
2. `module collections.vec_iter_zc` → `module collections.vec_iter`
3. Remove `@ziter()` bridge (or rename to `@iter()` — but `Vec[T] @iter()` already exists, so just delete)
4. All `@z*` → `@*` in `vec_iter.nv` and all test files
5. All `import std.collections.vec_iter_zc` → `import std.collections.vec_iter`
6. Update doc comments in the file header

Add `#impl(Next[U])` / `#impl(Next[T])` / `#impl(Next[(int,T)])` annotations on all `@next()` methods (now valid after Ф.1).

**Acceptance:**
- `nova_tests/plan153_2_zc` 4/4 PASS
- `nova_tests/plan161` 12/12 PASS
- `nova_tests/plan162` 5/5 PASS
- All `@z*` names gone from `std/`
- `import std.collections.vec_iter` works

### Ф.5 — Spec, D-blocks, docs, logs

- D-block for method-resolution receiver-compatibility rule (new D-number, reserve after D281)
- Update `docs/plans/backlog-followups.md`: close `[M-153.2-drop-z-prefix]`, `[M-codegen-blanket-generic-param-order]`, `[M-impl-attr-generic-protocol]`
- Update `docs/simplifications.md` (append entries for each fix)
- Update `project-creation.txt`
- Update `nova-private/discussion-log.md`
- Update Plan 164 status → ✅ CLOSED

---

## Acceptance criteria (umbrella)

1. **No z-prefix** in `std/collections/vec_iter.nv` or any test importing it
2. **`#impl(Next[T])`** parses and is validated by checker
3. **Mono-name** identical regardless of `fn[I Next[T], T Compare]` vs `fn[T Compare, I Next[T]]`
4. **Blanket dispatch** correct: `FilterIter.count()` → blanket, not `CharsIter.count()`
5. **No regressions**: full `nova_tests/plan153_2_zc` + `plan161` + `plan162` pass; broad regression 0 new FAIL
6. **Production quality**: positive + negative fixtures for every fix; no simplifications
7. **Logs updated**: simplifications.md + project-creation.txt + discussion-log.md + backlog

---

## Commit plan

| Commit | Content |
|---|---|
| `fix(parser): #impl(P[T]) — generic protocol in impl attribute` | Ф.1 |
| `fix(codegen): blanket mono-name param-order independence` | Ф.2 |
| `fix(codegen): method-resolution prefer receiver-compatible over wrong-type concrete` | Ф.3 |
| `refactor(vec_iter): drop z-prefix, rename vec_iter_zc→vec_iter` | Ф.4 |
| `docs(plan164): spec D-block, backlog close, logs` | Ф.5 |
