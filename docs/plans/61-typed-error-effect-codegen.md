# Plan 61: Typed-error effect codegen (user-defined errors через Fail[E])

> **Status:** proposed (2026-05-17). Architectural change в effect-codegen.

---

## Problem

`Fail[E]` declared с **generic E** в Nova spec, но в codegen runtime
`Nova_Fail_fail` hard-coded на `nova_str msg`:

```c
// compiler-codegen/nova_rt/effects.h:428
static inline nova_unit Nova_Fail_fail(nova_str msg) {
    ...
    current->fail(current->ctx, msg);
    ...
}
```

Handler arm signature тоже:

```c
static nova_unit _nova_handler_lit_0_impl_Fail_fail(void* _ctx, nova_str e);
//                                                                ^^^^^^^^^ ALWAYS nova_str
```

То есть **типаргумент `E` теряется**.

### Симптомы

**Symptom 1**: throw user-error → C type error.
```nova
export type AError | A
fn raise_a() Fail[AError] -> int => throw A
```
Lowers в `Nova_Fail_fail(nova_make_AError_A())` — passes `Nova_AError*` к
`Nova_Fail_fail(nova_str)`. C compile error.

**Symptom 2**: cross-effect throw в handler arm.
```nova
with Fail[AError] = |e| throw B { reason: "x" } {
    raise_a()
}
```
В handler body, `Nova_Fail_fail((BError*))` — wrong type cast.

**Symptom 3**: handler arm uses `e` parameter.
```nova
with Fail[AError] = |e| {
    println(e.detail)   // ← e treated как nova_str, .detail undefined
}
```

### Текущий "working" workaround

Pattern в stdlib (e.g. semver_range.nv, snowflake.nv):

```nova
let r = with Fail[E] = |e| interrupt Err(e) {
    Ok(operation())
}
match r {
    Ok(v)  => v
    Err(_) => throw NewErr { ... }
}
```

Работает **только** когда:
- Handler uses `interrupt` (не `throw`).
- Handler body НЕ переadressует `e` (просто захватывает через closure).
- Error type **passes as pointer** в `interrupt_ptr(void*)` — runtime treat'ит как opaque ptr.

Это **не idiomatic** Nova — verbose Ok/Err wrapping вокруг каждой operation.

## Что НЕ делаем

- **String-based errors only** — оставить `Fail[str]` единственным вариантом. Это
  ломает существующие `Fail[CustomError]` API во всём stdlib (snowflake, semver,
  json и т.п.).

## Решение: per-E specialization

### Codegen changes

1. **Per-E runtime fail-fns**:
   ```c
   static inline nova_unit Nova_Fail_AError_fail(Nova_AError* err);
   static inline nova_unit Nova_Fail_BError_fail(Nova_BError* err);
   ```
   Generated per concrete E used in `Fail[E]`.

2. **Per-E vtables**:
   ```c
   typedef struct {
       void* ctx;
       void (*fail)(void* ctx, Nova_AError* err);  // typed!
       ...
   } NovaVtable_Fail_AError;
   ```
   Plus per-E thread-local `_nova_handler_Fail_AError`.

3. **Handler arm signature** — типизирован per-E:
   ```c
   static nova_unit _nova_handler_lit_0_impl_Fail_AError_fail(
       void* _ctx, Nova_AError* e);
   ```

4. **Cross-effect throw в handler arm**: handler `|e| throw NewErr {...}`
   inside `with Fail[AError]` — emit как call `Nova_Fail_BError_fail(...)` (NOT
   `Nova_Fail_fail`). Requires tracking outer effect-context.

### Alternative: void* boxing

- Все error types passed как `void*` (boxed pointer). Cast'ить в handler body.
- Pros: меньше generated code, no per-E specialization.
- Cons: type safety теряется на C-уровне, runtime overhead на boxing.

**Recommendation**: per-E specialization (option 1). Type safety + zero-cost.

## Scope / Estimate

- Codegen (`emit_c.rs`): ~400-600 LOC изменений (Fail-specific paths).
- Runtime (`effects.h`): ~200 LOC dedicated per-E plumbing + macro infrastructure.
- Tests: ~30 regression tests (positive throw/handle + cross-effect rethrow).
- Migration: stdlib usages **уже** work через Ok/Err workaround — после fix'а
  можно переписать на idiomatic form, но не блокер.

**Total estimate**: 5-8 dev-days.

## Связь с другими планами

- **Plan 11** (method values): закрыт. Бug found во время Plan 11 followup
  (см. "Open issue: cross-effect throw" в `docs/plans/11-method-values-and-overload.md`).
- **Plan 45.B** (stdlib doc-pass): не блокер. Stdlib docs пишутся с
  текущим Ok/Err workaround.
- **Plan 49** (kinded throws): спецификация типизации throws — Plan 61
  имплементирует codegen для типизированной части.

## Acceptance criteria

- ✅ `throw UserError { ... }` для `Fail[UserError]` compiles + работает.
- ✅ Handler arm `|e| ...` имеет `e: UserError` type, можно access fields.
- ✅ Cross-effect rethrow `with Fail[A] = |e| throw B {...}` compiles + работает.
- ✅ Существующие stdlib usages (snowflake, semver, json, ...) — не регрессят.
- ✅ Documented в `spec/decisions/04-effects.md` (новый D-block или amend).

## Post-fix migration checklist

После закрытия Plan 61 — пройти по workaround-местам и переписать на idiomatic:

- [ ] **`std/data/semver_range.nv` `parse_version()`** — текущий код:
  ```nova
  fn parse_version(s str) Fail[ParseRangeError] -> Version {
      let r = with Fail[ParseVersionError] = |e| interrupt Err(e) {
          Ok(Version.from(s))
      }
      match r {
          Ok(v)  => v
          Err(_) => throw InvalidVersion { value: s }
      }
  }
  ```
  Должно стать:
  ```nova
  fn parse_version(s str) Fail[ParseRangeError] -> Version {
      with Fail[ParseVersionError] = |e| throw InvalidVersion { value: s } {
          return Version.from(s)
      }
  }
  ```

- [ ] **Grep по stdlib + nova_tests** для других мест с pattern
  `with Fail[X] = |e| interrupt Err(e)` + outer match — все они кандидаты
  на переход к direct handler-rethrow:
  ```bash
  grep -rn "with Fail\[.*\] = |e| interrupt Err" std/ nova_tests/
  ```

- [ ] **Документировать idiom** в `docs/idioms/error-rewriting.md` (создать).
  Сейчас effect-handler rethrow — не self-evident; пользователи будут писать
  Ok/Err wrapping по привычке. Idiom-док должен явно рекламировать
  cross-effect rethrow как short form.

## Ссылки

- `compiler-codegen/nova_rt/effects.h:428` — `Nova_Fail_fail` runtime.
- `compiler-codegen/src/codegen/emit_c.rs` — handler emit, throw lowering.
- [Plan 11 followup](11-method-values-and-overload.md) — где bug discovered.
- `/tmp/test_handler_rethrow.nv` — minimal repro (CC-FAIL).
