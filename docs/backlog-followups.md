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
- **[M-91.14-derive-debug]** `#impl(Debug)` auto-derive deferred from Plan 91.15. Requires checker-side synthesis of @debug body for record/value/sum types. Blocked on Plan 126 (#impl framework). Priority: M.
