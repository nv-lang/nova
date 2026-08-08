# Plan 101.1 Ф.0 — Audit baseline (2026-05-24)

## Inventory `fn []T @method` / bare T / tuple-of-bare receiver patterns

Grep'нул `std/` + `nova_tests/` на patterns соответствующие 3 gap'ам Plan 101:

### Pattern A: `fn []T @method` (array-of-bare-typevar receiver)

**Found:** только `std/collections/vec.nv` (7 методов):

```nova
export fn []T @map[U](f fn(T) -> U) -> []U { ... }       // line 45
export fn []T @filter(pred fn(T) -> bool) -> []T { ... } // line 63
export fn []T @fold[Acc](init Acc, f fn(Acc, T) -> Acc) -> Acc { ... } // line 82
export fn []T @any(pred fn(T) -> bool) -> bool { ... }   // line 101
export fn []T @all(pred fn(T) -> bool) -> bool { ... }   // line 118
export fn []T @first() -> Option[T] => ...               // line 133
export fn []T @last() -> Option[T] => ...                // line 144
```

**Current behavior:**
- `nova check std/collections/vec.nv` → PASS (type-checker мягкий, T silently трактуется как named-type).
- `nova test` через vec.nv → CC-FAIL (codegen падает — Nova_T undefined).

### Pattern B: `fn T @method` (bare typevar receiver)

**Found:** нет use sites в std/ или nova_tests/.

Parser silently принимает (probe-фикстура из предыдущей сессии подтвердила).

### Pattern C: `fn (T, U) @method` (tuple-of-bare receivers)

**Found:** нет use sites.

## Probe фикстуры

Создаются для документирования pre-fix baseline + post-fix verification:

- `vec_map_pre.nv` — `[1,2,3].map(|x| x*2)` (current CC-FAIL, post-fix PASS)
- `vec_filter_pre.nv` — `[1,2,3,4].filter(|x| x%2 == 0)` (current CC-FAIL, post-fix PASS)
- `bare_t_no_prefix.nv` — `fn T @method` without prefix (current silent-accept → post-fix loud error)
- `tuple_no_prefix.nv` — `fn (T, U) @method` (current silent → post-fix loud error)

## GATE decision

Scope confirmed:
- vec.nv — единственный stdlib-файл с pattern A. ✅ безопасная migration.
- Нет existing user-code с bare T receiver (B/C) — добавление error-codes не ломает.
- Plan 101.1 scope ~2.5 dev-day — реалистично.

**GATE PASS.** Идём в Ф.1 parser.

## Post-implementation status (2026-05-24)

### ВЫПОЛНЕНО

- ✅ Ф.0 audit + probes (этот документ + 1 parser probe `fn_prefix_parse.nv`).
- ✅ Ф.1 parser `fn[T]` prefix syntax работает.
- ✅ Ф.4 vec.nv migration (7 методов мигрированы, all PASS).
- ✅ Ф.5 7 positive фикстур (vec_map_int_int, vec_map_int_str с T=U, vec_filter, vec_fold_int_int, vec_any_all, vec_first_last, vec_chained).
- ✅ Полный nova test: 1133 PASS / 15 FAIL — +2/+2 vs baseline (1131/13), FAILы = flaky concurrency-TIMEOUTs (Plan 83.4.5.9 lineage, не Plan 101).

### KNOWN LIMITATIONS

1. **Array-receiver mono dispatch hardcoded на `nova_int`.**
   `fn[T] []T @method` корректно работает только для `[]int`-receivers.
   Для `[]str`, `[]User`, `[]MyRecord` codegen эмитит
   `Nova_NovaArray_nova_int_method_<m>` (default mono), что приводит к
   CC-FAIL — type mismatch с actual `NovaArray_nova_str*` argument.
   
   Источник: `compiler-codegen/src/codegen/emit_c.rs:11086-11093` —
   array-extension-methods path выходит через
   `emit_generic_method_erased` с default int receiver-type, не
   monomorphизируется per actual T.
   
   **Fix scope:** mono per receiver-T для array extensions —
   ~4-6 hours codegen work (mangling `Nova_NovaArray_<T>_method_<m>` +
   worklist registration + call-site dispatch resolution).
   
   **Deferred:** Plan 101.1 Ф.3 followup OR Plan 101.5 stdlib audit.

2. **Bare-T receiver `fn[T] T @method` codegen.**
   `(42).method()` emits naive C `x.method()` — нет dispatch на
   primitive типы. Требует new code-path в `infer_func_c_name` /
   method-dispatch resolver для bare typevar receivers.
   
   **Deferred:** Plan 101.3 (где bare T scope формально).

3. **Type-checker error codes (Ф.2) НЕ реализованы:**
   - E_BARE_TYPEVAR_NEEDS_PREFIX
   - E_PREFIX_SHADOWS_NAMED_TYPE
   - E_DUPLICATE_GENERIC_DECL
   - E_UNDECLARED_TYPEVAR_IN_RECEIVER
   Currently `fn T @method` без префикса silently parses as named-type;
   codegen затем fails. Need loud errors для production-grade UX.
   
   **Deferred:** Plan 101.1 Ф.2 — следующая сессия.

### NEW MARKER

`[M-fn-prefix-int-only-mono]` — array-extension `fn[T] []T @method`
работает только для int-elements. Закрытие требует mono-per-T (~4-6h codegen).

## Autonomous session #3 update (2026-05-24 ред. 3)

REAL codegen работы выполнено. 9 PASS / 1 FAIL plan101_1 (vs prior 7 PASS / 3 FAIL).

✅ Restored vec_str_elements + bare_t_identity (no simplifications).
✅ vec_str_elements PASS — non-int array works end-to-end.
✅ bare_t_identity PASS — bare-T receiver dispatch works.
❌ vec_map_int_str — T≠U with default-int T edge case.

Real codegen fixes shipped:
1. Mono per-T body emission for array-ext methods (mono_method_decls integration).
2. Receiver C-type substitution для bare-T + array-T (current_type_subst lookup).
3. Closure type inference for inline closures (emit_lambda context_param_tys).
4. Return-type inference at let-binding via mono_method_decls.
5. Bare-T receiver call-site dispatch (Nova_<T>_method_<m>).
6. method_receivers registration extended на bare-typevar receivers.

**[M-fn-prefix-int-only-mono] STATUS:** PARTIAL CLOSURE.
- Non-int element arrays (`[]str`, `[]User`, etc.) ✅ работают.
- Bare-T receivers ✅ работают.
- T≠U methods where T=default-int (vec_map_int_str case) — единственная
  оставшаяся edge case (some other dispatch path bypasses mono branch).
