# D216 V3 — pointer/type modifier rules migration

## Что изменилось (V2 → V3)

Plan 118.5 V3 amend (closure 2026-06-05) ввёл 4 design rules + 1 binding-context relaxation:
1. **§V3.1** storage-class-aware ro+mut ban (E_MUTABILITY_CONFLICT_VALUE_TYPE)
2. **§V3.2** modifier ordering safety-outer/mutability-inner (E_MODIFIER_ORDER)
3. **§V3.3** right-binding propagation semantics (extended V2)
4. **§V3.4** `safe` keyword + extended E_REDUNDANT_TYPE_MODIFIER
5. **Ф.6** binding-context relaxation (`ro x mut T` allowed)

## Migration patterns (что переписать)

### 1. Type-level chain ro+mut на value-T
```nova
// V2 (валидно)
fn f(p ro mut int)  // или return type-form

// V3 — ERROR E_MUTABILITY_CONFLICT_VALUE_TYPE
// int — value type, type-form ro+mut запрещён

// V3 fix варианты:
fn f(ro x mut int)         // binding-form (Ф.6 allowed)
fn f(p ro mut Acc)         // если T — heap record (ref type), оставить как есть
```

### 2. Modifier ordering reversed
```nova
// V2 (валидно)
fn f(p ro unsafe T)
fn f(p mut unsafe T)

// V3 — ERROR E_MODIFIER_ORDER
// Safety outer, mutability inner.

// V3 fix:
fn f(p unsafe ro T)
fn f(p unsafe mut T)
```

### 3. Same-class в chain (redundancy)
```nova
// V2 (валидно)
fn f(p ro * ro Acc)
fn f(p mut * mut Acc)
fn f(p unsafe * unsafe T)

// V3 — ERROR E_REDUNDANT_TYPE_MODIFIER
// Outer modifier propagates через chain automatically.

// V3 fix варианты:
fn f(p ro * Acc)              // simplest — drop redundant inner
fn f(p ro * safe ro Acc)      // safe escape — intentional fresh inner layer
```

### 4. `safe` keyword conflict
```nova
// V2 (валидно — `safe` был identifier)
fn safe(x int) -> int { x }
ro safe = 42

// V3 — ERROR (parse error / reserved keyword)
// `safe` теперь keyword.

// V3 fix: rename identifier
fn accept_safe(x int) -> int { x }
ro safe_value = 42
```

## Files affected (audit per §V3.6)

- `nova_tests/plan108_1/readonly_mut_conflict_neg.nv` + `mut_readonly_conflict_neg.nv` — already NEG tests, error code transition
- `nova_tests/plan118/t1_9_chain_modifiers_ok.nv` — V2 chains rewritten
- `nova_tests/plan118/t1_3_chain_multi_level_ok.nv` — triple chain → double (V3 propagation)
- `nova_tests/plan118_5/t4_neg_unsafe_arg_to_safe_param.nv` — `fn safe` → `fn accept_safe`
- stdlib `std/runtime/raw_mem.nv` — V3-compliant без changes (используется `mut * u8` canonical)

## Error code reference

| Code | Кратко | Spec | Fix |
|------|--------|------|-----|
| E_MUTABILITY_CONFLICT_VALUE_TYPE | type-form ro+mut на value-T | §V3.1 | use binding-form OR ref-type T |
| E_MODIFIER_ORDER | mut/ro wrap unsafe (wrong order) | §V3.2 | swap к unsafe-outer |
| E_REDUNDANT_TYPE_MODIFIER (extended) | same-class в chain | §V3.4 | drop redundant inner OR use `safe` escape |
| E_PARAM_MOD_CONFLICT | param ro+mut (binding-level) | D6/D176 | LIFTED для `ro x mut T` (Ф.6); preserved для consume+mut и т.д. |

## Cross-refs

- Spec: `spec/decisions/02-types.md` § D216 V3 amend (§V3.1-§V3.7)
- Plan-doc: `docs/plans/118.5-right-binding-rule-migration.md` (V3 + Ф.6 sections)
- Tests: `nova_tests/plan118_5_v3/` (17 fixtures POS+NEG, Ф.1-Ф.6)
