# Backlog Followups

This file tracks deferred items, known limitations, and future improvement tickets
referenced from plan docs and simplifications.md.

---

## Single-letter type names — E_TYPE_NAME_TOO_SHORT

- **[M-single-letter-type-ban]** CLOSED Plan 167. Запретить `type X { ... }` где имя типа длиной 1 символ.
  Мотивация: однобуквенные имена конфликтуют с generic-параметрами (`fn[S Iter[T]]` vs `type S`),
  вызывая E_PREFIX_SHADOWS_NAMED_TYPE. Haskell решает регистром (type vars строчные), Nova
  решает запретом однобуквенных типов — generic-параметры остаются однобуквенными по конвенции.
  Реализация: новый error E_TYPE_NAME_TOO_SHORT в checker (name.len() == 1 для TypeDecl).
  Sweep: grep `^type [A-Z] ` по nova_tests/ и std/ — исправить (~10 в nova_tests/plan118_1_addr_chains/).
  Priority: M.

---

## Name shadowing diagnostics

- **[M-prelude-name-shadow-hint]** Улучшить диагностику когда пользовательский тип называется так же как prelude-протокол.
  Сейчас: `type Iter { ... }` в модуле + использование в generic bound → `E_BOUND_NOT_PROTOCOL` (технически верно, но неясно почему).
  Хотим: hint «type name `Iter` shadows prelude protocol `Iter` — rename your type or use a qualified path».
  Реализация: в check_bound_ref, если bound-name резолвится в user TypeDecl (не Protocol) И в prelude есть Protocol с тем же именем — добавить hint к E_BOUND_NOT_PROTOCOL.
  Priority: M.

---

## Plan 118.6 — Safe &x model

- **[M-118.6-tuple-field-escape]** `&tuple.N` (tuple field by index) escape analysis chain-root tracking.
  Current: only named struct field chains are tracked. Tuple index access `&t.0` may not
  correctly promote the parent tuple. Verify and extend escape_analyze.rs if needed.
  Priority: M.


---

## D215 amend — Named tuple field defaults

- **[M-D215-defaults-handler-lambda-type]** `infer_handler_interrupt_ty` не может вывести тип
  lambda-параметра `e` в паттерне `with Fail[E] = |e| interrupt Some(e) { ... None }`.
  Корень: `infer_expr_c_type(Lambda(...))` не знает тип `e` без binding annotation или
  type-propagation от `Fail[E]` окружающего контекста. Следствие: `Some(e)` → `NovaOpt_nova_int`
  вместо `NovaOpt_ParseComplexError` → match на `Option[ParseComplexError]` падает.
  Тест в `std/_experimental/math/complex.nv` закомментирован.
  Fix: propagate Fail-binding type через context при выводе типа handler-lambda параметров.
  Priority: M (нужен для любого non-trivial Fail-bound error handler).

---

## Plan 147 — Three-axis mutability (D246)

- **[M-147-ro-binding-index-freeze]** `ro a []int` → `a[i] = x` должен давать ошибку по P7
  («голый `ro r` = freeze, весь owned-граф»), но сейчас **разрешается**.
  Корень: `check_target_readonly` ветка `ExprKind::Index` проверяет только `tr.is_readonly()`,
  но не `ro_binding_names`. Для `ExprKind::Member` `is_through_ro_binding` есть — для Index нет.
  ВАЖНО: `a[i]=x` для `[]T` codegen-inlined (`Stmt::Assign + ExprKind::Index`), НЕ диспатчится
  через `mut @index` метод (vec/access.nv:53-54) — поэтому `mut_methods` реестр не помогает.
  Баг актуален сейчас для `[]T` + после Plan 121 для `[N]T`.
  Fix: добавить `is_through_ro_binding(obj)` в Index-ветку `check_target_readonly` + oracle-тест.
  Priority: M.

- **[M-147-ro-ro-redundant-binding]** Следующие формы должны давать `E_REDUNDANT_TYPE_MODIFIER`
  (D246 «Канон синтаксиса»), но сейчас принимаются без ошибки:
  - `ro a ro T` — явный `ro` на binding + явный `ro T` на типе
  - `func(a ro T)` — параметр ro по умолчанию (D176) + явный `ro T` на типе
  - `mut a mut T` — `mut` binding + явный `mut T` (тип без модификатора уже mutable)
  - `func(mut a mut T)` — то же для параметра
  Fix: в checker при let/param — если (binding ro явно или по умолчанию) И тип явно `ro T` →
  `E_REDUNDANT_TYPE_MODIFIER`; если binding mut И тип явно `mut T` → то же.
  Priority: M.

- **[M-147-param-index-freeze]** `func(a []int)` → параметр ro-binding по умолчанию (D176);
  `a[i] = x` внутри fn должен давать ошибку — codegen-inlined путь, не через `mut @index`.
  Связан с [M-147-ro-binding-index-freeze] — один и тот же фикс в Index-ветке `check_target_readonly`.
  Priority: M.

---

## Plan 138 — `[]T` sugar / Vec codegen

- **[M-138-vec-pointer-element-mono]** `Vec[*T]`/`Vec[*mut T]`: codegen монорфизация для pointer-element-type сломана — `Vec.new()` вызывает generic-заглушку `Nova_Vec_static_new()` → NULL вместо специализированного конструктора → SEGFAULT при push/index. Структура `Nova_Vec____int64_t_p` и методы push/index генерируются правильно; ломается только static constructor. `Option[*mut T]: Some(p)→*p=v` работает (другой путь). Воспроизводится: `mut v Vec[*mut i64] = Vec.new(); v.push(&a); unsafe{*v[0]=100}`. Priority: P2.

---

## Plan 168 — Vec generic fwd-decl (D300)

- **[M-168-resize-with-free-fn-shadow]** `plan153_1/resize_with_free_fn_shadow` — pre-existing CODEGEN-FAIL: `undefined identifier f` when a module-level free fn `f` clashes with closure param `f` inside Vec.resize_with/fill_with. Not caused by Plan 168. Requires fix in name resolution (closure param scope should shadow outer free fn). Priority: M.

- **[M-168-other-generic-fwd-decl]** Other generic types (HashMap[K,V], Set[T], etc.) may have similar body-only instantiation gaps if they're used in fn bodies but not in signatures/fields. The Plan 168 tuple-elem fwd-decl fix covers them too (via MONO_TUPLE_TYPEDEFS), but the pre-pass body-scan only scans Vec TurboFish. If HashMap[str, u32] appears body-only it may also fail. Monitor for CC-FAIL patterns and extend scan if needed. Priority: L.

---

## Plan 91.8b — operator-dispatch cleanup

- **[M-91.8b-precompiled-c-rebuild]** ✅ CLOSED (Plan 91.15, 2026-06-17) — plan91_8b 6/6 PASS.
- **[M-91.15-hashmap-precompiled-eq]** `std/collections/hashmap.c` (precompiled) still uses `k.eq(key)` struct-member syntax instead of `Nova_str_method_equal`. CC-FAIL on map_literals tests with str keys. Fix: regenerate hashmap.c via `nova build-std` after Plan 91.8b @eq→@equal rename. Priority: M.

---

## Plan 91.15 — std API tuning

- **[M-91.10-remove-needs-caps-field]** ✅ CLOSED (Plan 91.15 Ф.5, 2026-06-17) — FnDecl.needs_caps removed from AST.
- **[M-91.14-option-result-debug]** ✅ CLOSED (Plan 91.15 Ф.2, 2026-06-17) — Option/Result @debug work via DeclaredBody interp dispatch.
- **[M-91.14-derive-debug]** ✅ CLOSED (Plan 91.15 Ф.3, 2026-06-17) — `#impl(Debug)` auto-derive works for record types. known-limit: checker does not validate field Debug bounds at synthesis time.

---

## Plan 147 Ф.7 — D246 checker enforcement gaps

- **[M-147-ro-binding-index-freeze]** ✅ CLOSED (Plan 147 Ф.7, 2026-06-17) — `ro a = [...]; a[0] = x` now gives `E_READONLY_CONTENT`. `is_through_ro_binding` check added to `check_target_readonly` Index arm in `compiler-codegen/src/types/mod.rs`; entry-code guard avoids false positives in prelude/std imports.
- **[M-147-param-index-freeze]** ✅ CLOSED (Plan 147 Ф.7, 2026-06-17) — non-`mut` params are now registered in `ro_binding_names` at fn entry (snapshot/restore), so `v[i] = x` on a plain `v []int` param gives `E_READONLY_CONTENT`.
- **[M-147-ro-ro-redundant-binding]** ✅ CLOSED (Plan 147 Ф.7, 2026-06-17) — `ro a ro []int = [...]` gives `E_REDUNDANT_TYPE_MODIFIER`; handled at parser level (`parser/mod.rs` lines 5198–5205, already present); oracle test `f7_neg3` confirms.
- **[M-147-readonly-content-lsp-quickfix]** nova-lsp `E_READONLY_CONTENT` quick-fix (Plan 147 Ф.7, 2026-06-17) — базовый `fix_readonly_content` добавлен в `nova-lsp/src/code_actions.rs`: ищет `ro <name>` binding вверх по файлу и предлагает `ro → mut`, или добавляет `mut ` перед параметром. Priority: P2 (улучшить heuristic при необходимости).

- **[M-118.7-safe-addr-outside-fn-scope]** Plan 118.6/118.7 known limitation: `&ident` без `unsafe {}` как trailing expr в fn body даёт `undefined identifier` (checker ищет ident в другом контексте). Workaround: `unsafe { &ident }` — поведение идентично после 118.7. Priority: P3 (правильная fix requires full type-inference in escape sink).

---

## Plan 91.18 — str + unicode API audit & cleanup (followups)

- **[M-91.18-to-words-array]** `str @to_words() -> []str` — eager materialization of word segments (mirrors `to_chars`). Priority: P2.
- **[M-91.18-eq-u8-slice]** `Equal` for `ro []u8` — would simplify `string_builder.nv @starts_with/@ends_with` (`.compare(b)==0` → `==b`). Priority: P2.
- **[M-91.18-from-bytes-lossy-slice]** `str.from_bytes_lossy` valid-sequence push optimization: `out.append(bytes[i..i+seq])` instead of per-byte push. Priority: P2.
- **[M-91.18-validate-utf8-dedup]** Shared `utf8_seq_len()` helper to de-duplicate utf8 sequence-length calculation between `from_bytes_lossy` and `chars.nv` decode. Priority: P3.
- **[M-91.18-stringbuilder-len-naming]** Consider `@len` → `@byte_len`, `@capacity` → `@cap` on StringBuilder (aligns with str convention; WriteBuffer family naming context). Priority: P3.
- **[M-91.18-unicode-cat-enum]** `GCB_*` / `WB_*` / `GC_*` / `SB_*` constants as real enums (requires codegen enum-from-int support). Priority: P3.
- **[M-91.18-import-gated-str-methods]** `str @to_upper()` / `str @to_lower()` extension methods currently resolve without `import std.unicode` (str ext-methods bypass import gating). Fix would require per-module method visibility tracking in the resolver. Priority: P2.
- ~~**[M-152.5-collation-conformance-u32-overflow]**~~ ✅ **FIXED 2026-06-19.** `nova_tests/plan152_5/collation_conformance.nv` RUN-FAIL `array: index 12884901890 out of bounds for length 4` (= 3·2³²+2). Root cause: in `collate.nv` `s21_match`, the consumed-index list (`Vec[int]`) was pushed through `cp_seq_push(src Vec[u32], x u32)` — the `(hi<<32)|lo` garbage came from reinterpreting 64-bit ints as 32-bit u32 words. Triggered only on the DUCET **S2.1 discontiguous** contraction path (Tibetan U+0FB2+U+0F71+U+0F80). Fix: added `idx_seq_push(src Vec[int], x int)` and routed both `cur_consumed` pushes through it. Regression-guard added to `collation.nv`.
- ~~**[M-vec-elem-type-mismatch-silent]**~~ ✅ **FIXED 2026-06-19** (generalized to **[M-generic-arg-type-mismatch-silent]**, commit `a9726e91`). The checker accepted passing a whole generic value with a different concrete-primitive type-argument (`Vec[int]`→`Vec[u32]`, user `Stack[int]`→`Stack[u32]`, `Option[f32]`→`Option[f64]`, …) — a pointer reinterpretation that surfaced only as a runtime OOB or a late C-stage CC-FAIL. Root cause: `cat_of`/`TyCat` folds all int widths into one `TyCat::Int` AND drops a named type's generic arguments. Fix (general, NOT Vec-specific): `f1_check_call` compares each type-argument of matching generic types at raw-TypeRef granularity and emits `[E_ARG_ELEM_TYPE_MISMATCH]`. (Scalar `int`→`u32` coercion outside a generic is NOT touched by this check — but per spec it should require explicit `as`; the current lenient behavior is a SEPARATE gap, see `[M-scalar-nonliteral-narrowing-not-enforced]`.) Supporting: `cat_of` lowers named `Vec[T]`→`Array` (D239 `[]T ≡ Vec[T]`); `infer_expr_type` resolves `Type[T].{new,with_capacity,from,default,filled}(…)` to carry element types into scope. Tests: `nova_tests/vec_elem_type/` + `plan70_4/neg/`.
- ~~**[M-scalar-nonliteral-narrowing-not-enforced]**~~ 🟡 **MOSTLY DONE 2026-06-19** (commit `f96016e6`). Per spec D54+D227 a non-literal wider int narrowing into a narrower / value-range-unsafe int position now requires explicit `as` → `[E_IMPLICIT_NARROWING]`. Enforced at: **bindings** (`ro a u8 = int_var`), **free-fn / static-method arguments** (`take_u8(int_var)`), and **reassignment** (`a = int_var`). Rule: value-range-preserving widening stays implicit (signed→wider-signed, unsigned→wider-unsigned, unsigned→strictly-wider-signed, `int`≡`i64`, `uint`≡`u64`); narrowing + signed→unsigned + value-unsafe cross (u32→i32, u64→int) need `as`. Literals keep their D227 range-check; `as`-casts auto-exempt. **Blast radius was ZERO** (no std migration needed) — see the remaining gap below. Tests: `nova_tests/narrowing/`. Spec amend pending (D54/conversions.md — gated on the other session's in-flight spec edits to `03-syntax.md`).
- **[M-instance-method-arg-narrowing]** The narrowing check (and ALL argument type-checking) does NOT cover **instance-method** calls `obj.method(arg)` — `f1_check_call` returns early for them (`_ => return`), because instance methods are resolved in codegen, not the checker. So `vec_u32.push(int_var)` (the std hotspot, ~375 sites) still silently narrows int→u32. Closing this needs instance-method receiver-type resolution + arg-checking in the checker (a sizeable feature, not narrowing-specific — it would also surface arg-type mismatches for every method call), THEN a std migration to add `as` at the push sites. Priority: P1 (soundness), but a dedicated plan. NOT the same as the now-enforced binding/fn-arg/reassign narrowing.
- ~~**[M-generic-arg-mismatch-records-followup]**~~ ✅ **DONE 2026-06-19** (commit `4e5533ff`). The generic-argument mismatch check now flags concrete **record/sum/newtype** type-args too (`Box[Dog]`→`Box[Cat]`) and **nested** generics (`Vec[Vec[int]]`→`Vec[Vec[u32]]`) via a recursive `generic_arg_mismatch()`. Alias-safe (resolved via `cat_of`, so `Box[Meters alias int]`→`Box[int]` does not false-flag); permissive on generic type-params / protocols / unknowns. Zero false positives across the corpus.
