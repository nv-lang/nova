// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 55: Codegen follow-ups от Plan 54

> **Создан 2026-05-16 EOD.** Собирает 3 codegen issues выявленные во
> время Plan 54 sprint'а. Каждый — изолированный bug ограниченного
> scope, не блокирует main acceptance criteria Plan 48/49/54, но
> накапливается как technical debt.
>
> **Приоритет:** P3 — local quality-of-life fixes. Решать когда есть
> spare bandwidth перед следующим major plan'ом.

---

## Контекст

Plan 54 sprint (post-Plan 48/49 audit) закрыл 7/8 items + 1 accepted-
as-is. В процессе вылезли 3 orthogonal codegen issues, которые
блокируют узкие, но реальные use cases:

1. `[M-array-of-func-mono]` — массив closures как параметр generic-fn.
2. `Ф.5b match-arm pattern_inner_type` — match-инференция через scrutinee.
3. `Nova_Duration_method_into` — stdlib Duration.into() invalid C.

Каждый зафиксирован [M-*] / Ф.* маркером в `docs/simplifications.md`,
но нужен дедикатеный план чтобы closed как-таковые.

---

## Ф.1 — `[M-array-of-func-mono]`: Array-of-Func type_ref_to_c

### Симптом

`[]fn() -> T` параметр в generic-fn даёт неправильный C-тип элемента
массива. Loop `for f in fns { f() }` пытается вызвать `f` как обычную
функцию (через `nova_fn_f()`), а не closure-call.

```nova
fn collect[T](fns []fn() -> T) -> []T {
    let mut out []T = []
    for f in fns {
        out.push(f())          // ← f() пытается nova_fn_f(), не closure
    }
    out
}

let xs = collect[int]([|| 1, || 2, || 3])
//          ↑ closures передаются — но collect видит их как nova_int
```

C-output:
```c
// fns параметр получает тип NovaArray_nova_int* (как массив int)
//                                                ↑ WRONG: closure-array
static NovaArray_nova_int* nova_fn_collect____nova_int(NovaArray_nova_int* fns) {
    for (i = 0; i < fns->len; i++) {
        nova_int f = fns->data[i];                 // f типа int не closure
        nova_array_push_nova_int(out, nova_fn_f()); // ← nova_fn_f undefined
    }
}
```

Error: `lld-link: error: undefined symbol: nova_fn_f`.

### Корень

В `compiler-codegen/src/codegen/emit_c.rs::type_ref_to_c` (примерно
строка 1860+, branch `TypeRef::Array(inner, _)`) — для `inner =
TypeRef::Func{...}` нет специального case'а, fallback'ит на default
element type `nova_int`. Результат — `NovaArray_nova_int*` (массив
int) вместо `NovaArray_void_p*` (массив closure pointers / NovaClosure).

Аналогично в Source 2c `resolve_mono_type_args` (line ~5158+) —
инференция T из `[]T` arg type strips `NovaArray_` prefix чтобы
извлечь T, но для array-of-func возвращает nova_int как element
type, → T inferred wrong.

### Fix

В `type_ref_to_c` для `TypeRef::Array(inner, _)`:
- Если `inner = TypeRef::Func {...}` — emit `NovaArray_void_p*` (или
  специальный `NovaArray_closure*` с правильным per-T mangling).
- Уже declared NovaOpt/NovaArray paired typedef нужно расширить для
  closure element types.

Альтернатива: использовать существующий closure-aware path —
закрытие хранится через `NovaClosBase*` (см. Plan 11 Ф.4). Array
of closures = `NovaArray_NovaClosBase_p*` или подобный mangled name.

### Acceptance

- `fn collect[T](fns []fn() -> T) -> []T` компилируется и работает.
- `for f in fns { f() }` корректно вызывает closure через
  `NOVA_CLOS_CALL_*` macro или fallback indirect-call.
- Test: `nova_tests/concurrency/fn_array_closure_test.nv` — `collect`
  pattern для T=int/str/bool.

### Estimate

~80-120 LOC. Risk medium — затрагивает type_ref_to_c + resolve_mono +
emit_for body. Зависимостей нет.

---

## Ф.2 — Ф.5b: match-arm `pattern_inner_type` из scrutinee

### Симптом

`match` expression inference смотрит на arm-bodies для определения
своего типа, но НЕ учитывает pattern-binding types из scrutinee. Это
особенно ломает `Some(v) => v` где `v` имеет тип scrutinee-inner.

```nova
let tok CancelToken[int] = CancelToken.new()
tok.cancel(42)
let r = tok.reason()
let got = match r {
    Some(v) => v       // v: nova_int (внутренний тип Option[int])
    None    => -1
}
// expected: got: nova_int
// actual:   got inferred сначала как nova_int default → ok случайно
// но если v leaked stale type (bool) → got: bool → ill-typed
```

После Ф.1 (var_types snapshot) leak между tests fixed, но WITHIN
одного match — pattern v ещё не registered к моменту infer_arm_body.

### Корень

В `compiler-codegen/src/codegen/emit_c.rs::infer_expr_c_type` для
`ExprKind::Match { arms, .. }` (line 15707-15721):

```rust
ExprKind::Match { arms, .. } => {
    for arm in arms {
        let t = match &arm.body {
            MatchArmBody::Expr(e) => self.infer_expr_c_type(e),
            // e может содержать Ident'ов pattern-bound (v) которых нет в var_types
            ...
        };
        if t != "nova_unit" && t != "nova_int" { return t; }
    }
    "nova_int".into()
}
```

infer не смотрит на scrutinee + pattern relationship. Для `Some(v) =>
v`, `infer_expr_c_type(v)` ищет `var_types[v]` (стэйл или default
nova_int).

### Fix

Добавить параметр `scrutinee` к match-branch:

```rust
ExprKind::Match { scrutinee, arms, .. } => {
    let scr_ty = self.infer_expr_c_type(scrutinee);
    for arm in arms {
        // Compute pattern-binding types from scrutinee.
        // Например, Some(v) для scr_ty="NovaOpt_T" → v: T.
        let pattern_bindings = pattern_inner_types(&arm.pattern, &scr_ty);
        // Temp-override var_types для arm body inference.
        let saved: Vec<_> = pattern_bindings.iter()
            .map(|(n, t)| (n.clone(), var_types.insert(n.clone(), t.clone())))
            .collect();
        let t = match &arm.body { ... };
        // Restore
        for (n, prev) in saved { ... }
        if t != "nova_unit" && t != "nova_int" { return t; }
    }
    "nova_int".into()
}
```

Helper `pattern_inner_types(pattern, scr_ty)`:
- `Some(v)`: scr_ty = "NovaOpt_T" → strip "NovaOpt_" → recover c_ty
  via novaopt_value_types → return [("v", T_c)].
- `Ok(v)`, `Err(e)`: analogous для Result.
- User sum-types: `Variant(a, b, c)` — lookup variants schema, type
  поле-binds.

### Acceptance

- `match r { Some(v) => v; None => default }` где `r: NovaOpt_<T>` —
  match-result правильно typed как T.
- Test: `nova_tests/concurrency/match_arm_inference_test.nv` —
  Some/None для int/str/bool/user-types.

### Estimate

~80 LOC. Risk medium — изменяет infer_expr_c_type для match (часто
вызываемый path), может ломать существующие тесты. Mitigation:
helper'ы сохраняют backward-compat когда `scr_ty` не Optional-like.

Зависимостей нет, но complement'ит Ф.1 (var_types cleanup).

---

## Ф.3 — `Nova_Duration_method_into` stdlib codegen issue

### Симптом

Любой код использующий `import std.time.duration` или `Duration.into()`
получает CC-FAIL:

```
nova_tests/.../X.c:Y: error: passing 'nova_unit' to parameter of incompatible
type 'nova_str' | ... Nova_Duration_method_into(...) ...
```

Это блокирует `std/concurrency/retry.nv` (использует Duration в backoff),
и любые tests что хотят use Time-effect-based durations.

### Корень

В `std/time/duration.nv` есть method:
```nova
export fn Duration @into() -> str => "..."   // → нужно вернуть str
```

Но codegen для этого method'а эмитит `nova_unit Nova_Duration_method_into(...)`
вместо `nova_str ...`. Где-то return type теряется.

Подозреваемые пути:
1. `emit_fn` для method'а с return `Self`/auto-derived типом, где return
   type теряется при path traversal.
2. `infer_expr_c_type` для тела `=> "..."` (expression body) возвращает
   wrong type.
3. D73 `Into` protocol auto-derivation путает с `from()`.

### Подход к fix

1. Repro isolated — simplest `fn Duration @into() -> str => "test"` —
   проверить эмитится ли правильный return type.
2. Если bug в emit_fn — найти где `Some(TypeRef)` → `String` (C-type)
   maps идёт, where falls into nova_unit default.
3. Если bug в Into-protocol auto-derive — отдельный path.

### Acceptance

- `Nova_Duration_method_into(d)` возвращает `nova_str`, не `nova_unit`.
- `std/time/duration.nv` self-test'ы (если есть) проходят.
- retry_test.nv с full `import std.concurrency.retry` (через duration.nv
  dependency) компилируется.

### Estimate

~30-80 LOC. Risk low (узкий fix), но требует repro + bisect debug.
Зависимостей нет.

---

## Acceptance criteria (Plan 55)

- [ ] Ф.1 — `[M-array-of-func-mono]` fixed; `collect[T](fns []fn->T)`
      pattern компилируется + работает.
- [ ] Ф.2 — Ф.5b match-arm pattern_inner_type implementation; Some/None
      match-результат правильно typed как inner T.
- [ ] Ф.3 — `Nova_Duration_method_into` returns nova_str; retry_test
      без isolation workaround'а проходит.
- [ ] Полный `nova test` (release) — без новых FAIL.

---

## Size estimate

| Фаза | LOC | Risk | Зависимости |
|---|---|---|---|
| Ф.1 — array-of-func-mono | ~80-120 | medium | нет |
| Ф.2 — match-arm pattern_inner_type | ~80 | medium | complement к Ф.1 (Plan 54) |
| Ф.3 — Duration.into() codegen | ~30-80 | low | нет |
| **Итого** | **~190-280** | mixed | independent |

---

## Связь

- Plan 48 — closures-in-generics; Ф.1 этого плана closure final Ф.7
  acceptance для `[]fn->T` (после Ф.1 — Plan 48 `[]fn()->T` полностью
  closed).
- Plan 49 — `Ф.2` касается reason()/match patterns для CancelToken[T];
  fix снимает workaround «уникальные pattern-var names».
- Plan 54 — этот план — direct followup от Plan 54 EOD findings.
- D73 `From`/`Into` protocols — Ф.3 касается Into auto-derive path.
