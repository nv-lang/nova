// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 55: Codegen follow-ups от Plan 54 + Plan 52.x mono-pass blockers

> **Создан 2026-05-16 EOD.** Собирает codegen issues выявленные во
> время Plan 54 sprint'а (Ф.1-Ф.3) + 3 mono-pass blockers выявленных
> в Plan 52.x (Ф.4-Ф.6). Все — orthogonal codegen bugs ограниченного
> scope, не блокируют main acceptance criteria, но накапливаются как
> technical debt.
>
> **Расширение 2026-05-16:** добавлены Ф.4-Ф.6 после Plan 52.3 закрытия —
> 3 M-52-* маркера оказались в **той же категории**: mono-pass
> infrastructure issues (corruption / name-collision / multi-instance
> collision). Логично закрывать вместе с Ф.1/Ф.2 которые тоже про
> mono-pass.
>
> **Ревизия 2026-05-16 (prod-grade upgrade):** план повышен до **P1**
> (был P3). Причины:
> 1. Ф.4 (mono-pass corruption) — фундаментальный bug, который **уже
>    блокирует** добавление любых helper-method'ов в generic stdlib
>    типы. Real prod-grade язык такого не имеет (Go: no generics until
>    1.18; Rust: monomorphization "always works"; TS: structural всё).
> 2. Ф.5/Ф.6 — UX блокеры: `let m = ["a":1]` без аннотации **должно**
>    работать (Rust HashMap! macro, Go map literal, TS object literal).
>    Workaround "explicit annotation" — это не паритет.
> 3. Ф.1 — `[]fn->T` в generic-fn — базовый паттерн functional API
>    (Rust `Vec<Box<dyn Fn>>`, Go `[]func()`). Без него `parallel for`
>    + `retry` + `pipeline` patterns ограничены.
> 4. Ф.2 — match-arm inference — основа safe pattern matching.
>    Rust/Swift делают это идеально, TS narrows полностью.
> 5. Ф.3 — Duration.into() — блокирует `import std.concurrency.retry`.
>
> **Priority:** P1 — пользователь явно потребовал prod-grade. Все
> упрощения которые «accepted as-is» в исходном плане — пересмотрены.

---

## Ф.0 — Production-grade principles & audit (новое, 2026-05-16)

### Ф.0.1 — Сравнение с Go/Rust/TS (что значит «не хуже»)

| Фича | Go | Rust | TS | Nova сейчас | Nova target |
|---|---|---|---|---|---|
| Array of fn типов | `[]func() T` works | `Vec<Box<dyn Fn>>`/`Vec<F>` | `(() => T)[]` | ❌ silently mismatched | ✅ Ф.1 |
| Match on Option(v) | N/A (no sum types) | exhaustive inference | `if Some(x)` narrow | ❌ leaks stale | ✅ Ф.2 |
| Method `Self -> str` body `=> "..."` | N/A | работает | работает | ❌ возвращает unit | ✅ Ф.3 |
| Generic helper methods на user types | N/A | mono всё работает | structural | ❌ corrupt context | ✅ Ф.4 |
| `let m = {a:1, b:2}` без annot | `map[string]int{...}` нужен type | turbofish/inference | inferred | ❌ выбирает wrong overload | ✅ Ф.5 |
| Multi-instance generic в одной fn | mono'd separately | mono'd separately | structural | ❌ instance leak | ✅ Ф.6 |

**Acceptance principle:** для каждого Ф.* — repro-test показывает
«как в Go/Rust/TS работает» + assertion что в Nova работает **так же
или лучше**. Lower-bound — паритет.

### Ф.0.2 — Production-grade non-negotiables (всем фазам)

Применяется к каждой Ф.1-Ф.6:

1. **Repro-test перед fix** — `nova_tests/plan55/<feature>_repro.nv` с
   фиксированным EXPECT-маркером. Тест **компилируется и проходит**
   после fix; **CC-FAIL'ит с понятной диагностикой** до fix (мы видели).
2. **Negative test** — для каждой фичи добавить тест с **ожидаемой
   ошибкой**, который проверяет качество диагностики (Plan 36 R7 quality
   bar: error message → file:line + suggestion).
3. **Interpreter parity** — `nova run <repro>.nv` даёт **тот же
   результат** что `nova build && exe`. Если interp ещё не поддерживает —
   `// SKIP_INTERP: <reason>` маркер + TODO в interp.rs.
4. **Regression sweep** — после fix запустить **полный** `nova test`
   release; **0 регрессий**. Документировать diff в commit message.
5. **`docs/simplifications.md`** — закрывающая запись `[M-<marker>] ✅
   ЗАКРЫТО (Plan 55 Ф.X, <date>)` с `Где / Было / Закрыто / Test`.
6. **`docs/project-creation.txt`** — секция «Plan 55 Ф.X (<date>)» с
   что/почему/как/measurement.
7. **Commit message** — `feat(55.X): <subject>` или `fix(55.X): ...`,
   без AI-trailer'ов (per [feedback_commit_trailer]).
8. **Spec update** — если fix меняет наблюдаемое поведение языка
   (Ф.5 inference rules, Ф.6 mono semantics) — обновить
   `spec/decisions/*.md` с rationale.
9. **Cross-toolchain** — если fix трогает C codegen (Ф.1, Ф.3, Ф.4-Ф.6) —
   `nova test --filter <fix-area>` с Clang (default) и MSVC если доступен.
10. **Diagnostic improvement** — каждый Ф.* должен **улучшить** хотя
    бы одно error-message в области fix (даже если не было запроса) —
    error должны вести к user-actionable fix.

### Ф.0.3 — Risk register (mitigation для high-risk фаз)

| Phase | Risk | Mitigation | Rollback |
|---|---|---|---|
| Ф.4 | mono-pass save/restore затрагивает hot path → perf regression | Bench `nova test` wall-time до/после; threshold ±5% | `git revert <commit>`; патч изолирован одним commit'ом |
| Ф.4 | Изменение invariant'а в `current_fn_return_ty` ломает другие места | Audit-grep всех `current_fn_return_ty.*=`; verify save/restore parity | Tests catch immediately |
| Ф.6 | Глобальный mono-pass refactor → каскадные FAIL'ы | Pre-flight: dump mono'd C для 5 «тяжёлых» tests (channel, hashmap, retry, supervised, contracts); diff до/после | Изолированный commit |
| Ф.5 | Изменение overload resolution → silent breakage | Explicit log: при resolve через turbofish — emit comment `/* MONO: resolved <T>::<m> via turbofish */` (debug only) | Single-call-site change |
| Ф.1 | NovaArray_void_p может конфликтовать с existing erasure | Audit все usages `void*` в codegen; гарантировать ABI-compatibility | New typedef изолирован |
| Ф.3 | Duration.into() может затрагивать D73 Into auto-derive | Repro **в обоих режимах** (interp + codegen); diff `nova check` output | Если глубже — выделить в Plan 56 |

### Ф.0.4 — Что raising bar даёт (delta vs original P3 план)

| Дополнение | Что добавляет |
|---|---|
| Ф.0.1 сравнение | Чёткий критерий «не хуже Go/Rust/TS» — измеримо |
| Ф.0.2 non-negotiables | Каждая фаза = atomic prod-ready unit (test + interp + neg + diag + docs) |
| Ф.0.3 risk register | Безопасный rollback для high-risk Ф.4/Ф.6 |
| Закрытие interpreter gap | `nova run` parity — раньше игнорировали |
| Spec sync | Поведенческие изменения зафиксированы в D-блоках |
| Cross-toolchain test | Clang+MSVC, не только default |
| Diagnostic improvements | Каждый fix улучшает UX (Plan 36 quality bar) |

### Ф.0.5 — Скрытые моменты (найденные при audit, 2026-05-16)

1. **`[]fn->T` with capture** — Ф.1 нужно проверить closures с
   capture (`let n = 5; xs.push(|| n)`). NovaClosBase хранит env,
   но при storage в массиве — env должен сохраняться. Тест:
   `closure_capture_in_array.nv`.
2. **Nested mono (mono-call внутри mono'd fn)** — Ф.4 может пропустить
   nested case: `fn outer[A]() { inner[A]() }` — `current_type_subst`
   для outer должен быть accessible при mono inner. Тест:
   `nested_mono_subst.nv`.
3. **Match arm Block (не Expr)** — Ф.2 helper должен работать и для
   `Some(v) => { let q = v + 1; q }` (Block with trailing). Сейчас
   план обрабатывает только Expr arm. Расширить.
4. **Pattern глубокий** — `Some(Ok(v))` — Ф.2 должен рекурсировать.
   Иначе `match opt { Some(Ok(v)) => v; ... }` не получит правильный T.
5. **Duration.into() через `From`-derive** — Ф.3 может оказаться
   симптомом D73 Into auto-derive bug. Если так — нужно фиксить
   auto-derive, иначе все `@into() -> T` методы будут broken.
6. **Mono `with_capacity` collision** — Ф.5 root cause: `HashMap.with_capacity`
   vs `WriteBuffer.with_capacity` vs `StringBuilder.with_capacity` —
   3 встроенных типа имеют тот же method. Mono pass должен respect'ить
   turbofish target **до** name-lookup. Это **не P3**, это **P1
   correctness** (silent wrong-codegen — самый плохой класс bugs).
7. **Multi-instance collision корень** — Ф.6 может оказаться leak'ом
   `current_type_subst` между call-sites. Investigation должна
   начаться с **dump'а subst** на entry/exit каждого emit_call (debug
   trace через NOVA_DEBUG_MONO env-var).
8. **GC roots для array-of-closures** — Ф.1 закрывает codegen, но
   нужно verify что NovaArray элементы (void* = NovaClos_X*) видимы
   для Boehm GC. Если NovaArray.data — `T*` где T=void*, Boehm должен
   conservative-scan их. Тест: `closure_array_gc_stress.nv` (1000
   closures * 100 циклов, no leak).
9. **Cross-file resolve для `[]fn->T`** — Plan 35 cross-file generic
   bounds может frame `[]fn->T` parameter type некорректно. После
   Ф.1 — verify через cross-file test (`std/concurrency/retry.nv`
   import).
10. **Annotate-pass для closures в `[k:v]` literal** — Ф.5 fix может
    требовать MapLitAnnotator расширение чтобы понимать `[k:fn_expr]`
    (HashMap[str, fn() -> T]). Test: `hashmap_str_to_fn.nv`.

### Ф.0.6 — Test plan (что добавится)

Минимум **18 новых тестов** (3 на каждую фазу: positive + negative +
edge case) в `nova_tests/plan55/`:

| Phase | Tests |
|---|---|
| Ф.1 | `fn_array_collect_positive.nv`, `fn_array_collect_with_capture.nv`, `negative_fn_array_arity_mismatch.nv`, `closure_array_gc_stress.nv` |
| Ф.2 | `match_some_v_inference.nv`, `match_nested_pattern.nv`, `match_arm_block_inference.nv`, `negative_match_arm_type_mismatch.nv` |
| Ф.3 | `duration_into_str.nv`, `into_auto_derive_str.nv`, `retry_via_duration.nv`, `negative_into_wrong_return.nv` |
| Ф.4 | `hashmap_clone_method.nv`, `generic_helper_chain.nv`, `nested_mono_subst.nv`, `negative_helper_invariant_violation.nv` |
| Ф.5 | `hashmap_infer_no_annot.nv`, `nested_map_infer.nv`, `negative_map_ambiguous.nv` |
| Ф.6 | `multi_hashmap_in_fn.nv`, `triple_generic_instance.nv`, `negative_instance_type_mismatch.nv` |

Все — с EXPECT-маркерами (D89) и SKIP_INTERP маркерами где нужно.

### Ф.0.7 — Performance baseline

Перед началом Ф.4 — записать:
- `nova test` wall-clock (release, 16 cores)
- Mono'd .c file size для top-5 tests (channel/hashmap/retry)
- Memory peak во время `nova test --filter mono`

После каждой high-risk фазы (Ф.4, Ф.6) — повторить, threshold ±5%.

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
    mut out []T = []
    for f in fns {
        out.push(f())          // ← f() пытается nova_fn_f(), не closure
    }
    out
}

ro xs = collect[int]([|| 1, || 2, || 3])
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

### Acceptance (prod-grade)

- `fn collect[T](fns []fn() -> T) -> []T` компилируется и работает.
- `for f in fns { f() }` корректно вызывает closure через
  `NOVA_CLOS_CALL_*` macro или fallback indirect-call.
- **Closure с capture** (`let n = 5; [|| n + 1]`) сохраняет env в массиве.
- **GC stress test:** 1000 closures × 100 циклов — no leak (через
  `gc.heap_size()` baseline + assertion `< 2x baseline`).
- **Interp parity:** `nova run` даёт тот же результат.
- **Cross-toolchain:** Clang + (MSVC если доступен) — оба компилируют.
- **Diagnostic:** при использовании `[]fn->T` с wrong arity (например
  `[|| 1, |x| 2]`) — error на этапе type-check, не codegen.
- Tests (4):
  - `nova_tests/plan55/f1_fn_array_collect_positive.nv` — T=int/str/bool.
  - `nova_tests/plan55/f1_closure_array_with_capture.nv` — capture works.
  - `nova_tests/plan55/f1_closure_array_gc_stress.nv` — GC scan.
  - `nova_tests/plan55/f1_negative_fn_array_arity_mismatch.nv` — neg diag.

### Implementation guideline

1. **Runtime:** `compiler-codegen/nova_rt/array.h` — добавить
   `NOVA_ARRAY_DECL(void_p)` + `NOVA_ARRAY_IMPL(void_p)` с
   `typedef void* void_p;` (или inline в DECL — но проверить C99-compat).
2. **codegen `type_ref_to_c`:** для `TypeRef::Array(inner, _)` где
   `inner = TypeRef::Func{..}` → return `"NovaArray_void_p*"`.
3. **codegen `resolve_mono_type_args`:** Source 2 inference для `[]T`
   когда concrete = "NovaArray_void_p" — fallback на Source 2b
   (closure-arg return-type inference) — он уже умеет.
4. **codegen emit_for body для `for f in fns`:** при iteration over
   `NovaArray_void_p*`, элемент `f` имеет тип `void*` (closure pointer).
   Call `f()` → routing через NOVA_CLOS_CALL_<sig> в зависимости от
   inferred sig из turbofish T.
5. **GC:** Boehm conservative-scan'ит data[] как pointer array (T = void*),
   pointers в env сохраняются.

### Estimate

~80-120 LOC + 4 tests. Risk medium — затрагивает type_ref_to_c +
resolve_mono + emit_for body + array.h runtime. Зависимостей нет.

---

## Ф.2 — Ф.5b: match-arm `pattern_inner_type` из scrutinee

### Симптом

`match` expression inference смотрит на arm-bodies для определения
своего типа, но НЕ учитывает pattern-binding types из scrutinee. Это
особенно ломает `Some(v) => v` где `v` имеет тип scrutinee-inner.

```nova
ro tok CancelToken[int] = CancelToken.new()
tok.cancel(42)
ro r = tok.reason()
ro got = match r {
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

### Acceptance (prod-grade)

- `match r { Some(v) => v; None => default }` где `r: NovaOpt_<T>` —
  match-result правильно typed как T.
- **Nested:** `match opt { Some(Ok(v)) => v; ... }` — рекурсивная
  pattern_inner_type.
- **Block arm:** `match opt { Some(v) => { let q = v + 1; q }; ... }`
  — Block trailing получает правильный inner T.
- **User sum-types:** `match e { MyVar(a, b) => ... }` — типизированы
  по variant schema.
- **Interp parity:** `nova run` matches `nova build`.
- **No regression:** existing ~30 match-тестов pass.
- **Scoped restore:** var_types перезапись scoped по arm.
- Tests (4):
  - `nova_tests/plan55/f2_match_some_v_inference.nv` — int/str/bool.
  - `nova_tests/plan55/f2_match_nested_pattern.nv` — Some(Ok(v)).
  - `nova_tests/plan55/f2_match_arm_block_inference.nv` — Block arm.
  - `nova_tests/plan55/f2_negative_match_arm_type_mismatch.nv` — diag.

### Estimate

~120 LOC (Block + nested поверх original 80) + 4 tests. Risk
medium-low (scoped save/restore изолирует риск). Зависимостей нет,
но complement'ит Ф.1 (var_types cleanup).

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

### Acceptance (prod-grade)

- `Nova_Duration_method_into(d)` возвращает `nova_str`, не `nova_unit`.
- `std/time/duration.nv` self-test'ы (если есть) проходят.
- `retry_test.nv` с full `import std.concurrency.retry` (через duration.nv
  dependency) компилируется.
- **Generalized `@into() -> T`:** любой user `@into()` метод с return
  type работает (не только Duration).
- **D73 Into auto-derive:** если bug в auto-derive — fixed для всех
  типов.
- **Interp parity:** `nova run` тот же результат.
- **Diagnostic:** если `@into()` имеет inconsistent return — error
  на type-check, не codegen.
- Tests (4):
  - `nova_tests/plan55/f3_duration_into_str.nv` — direct Duration.
  - `nova_tests/plan55/f3_into_auto_derive_str.nv` — generic @into.
  - `nova_tests/plan55/f3_retry_via_duration.nv` — retry unblock.
  - `nova_tests/plan55/f3_negative_into_wrong_return.nv` — diag.

### Implementation approach

1. **Repro isolated** — minimum `fn Duration @into() -> str => "test"` →
   проверить эмитится ли правильный return type.
2. **Bisect:**
   - Path A: `emit_fn` для method'а — где Some(TypeRef) → C-string maps?
   - Path B: D73 Into protocol auto-derivation — какой path выбирается?
   - Path C: `infer_expr_c_type` для body `=> "..."` — возвращает unit?
3. **Fix:** в правильном path; если глубже Plan 55 scope (требует
   рефакторинг D73) — выделить в Plan 56 и описать workaround.

### Estimate

~30-80 LOC + 4 tests. Risk low (узкий fix), но требует repro + bisect.
Зависимостей нет.

---

---

## Ф.4 — `[M-52-spread-not-supported]`: mono-pass corruption для generic helper-methods

### Симптом

Добавление `HashMap[K, V] @clone()` / `@merge_from(other)` методов в
stdlib hashmap.nv приводит к **регрессии всего HashMap'а**: 522 → 501
PASS (-21), 25 → 48 FAIL (+23).

```nova
export fn HashMap[K, V] @clone() -> HashMap[K, V] {
    mut copy = HashMap[K, V].with_capacity(@count)
    for i in 0..@buckets.len() {
        match @buckets[i] {
            Occupied { key: k, value: v } => copy.insert_new(k, v)
            _ => {}
        }
    }
    copy
}
```

Любой test использующий HashMap → CC-FAIL.

### Корень

DEBUG trace на `check_bool_condition_at` показал:
- `if used < threshold` в `@maybe_grow()` (line 400 hashmap.nv) получает
  `cond_ty=nova_int` (а не `nova_bool`!)
- `current_fn_return_ty = "nova_int"` вместо unit

То есть mono pass для `@clone()` триггерит mono pass всего
HashMap chain (@maybe_grow, @find_slot etc), и в этой цепочке
**type-context corrupted** — простые arithmetic-comparisons
эмитятся как nova_int вместо nova_bool.

Mini-repro вне stdlib **работает** (Bag[K, V] с pattern-match copy):
- Bag не имеет internal helpers
- HashMap имеет много (@maybe_grow, @find_slot, @rehash)
- Corruption происходит **при триггере чужих fns через clone**

### Fix

Investigation: где именно теряется/перезаписывается type-context при
mono-pass invocation chained methods.

Возможные подозрения:
1. `current_fn_return_ty` save/restore — может новый mono context
   читает stale value
2. `current_type_subst` — substitution leak между mono'd instances
3. `infer_expr_c_type` для Binary::Lt возвращает nova_bool в general
   case, но в mono'd helper — что-то другое перезаписывает

### Acceptance (prod-grade)

- [ ] `HashMap.@clone()` + `@merge_from()` в stdlib compile cleanly
- [ ] 0 регрессий от добавления методов (test suite delta = 0 FAIL,
      ≥ +baseline PASS)
- [ ] После: M-52-spread-not-supported разблокирован — реализация
      spread parser/desugar тривиальна (~50 LOC) — **закрыть в той
      же сессии** для validation
- [ ] **DEBUG trace tool**: добавить `NOVA_DEBUG_MONO=1` env flag,
      который при mono-pass dump'ит `(fn_name, type_subst, return_ty)`
      на entry/exit emit_fn. Позволяет diagnose corruption в будущем.
- [ ] **Save/restore audit**: grep всех мест где
      `current_fn_return_ty`/`current_type_subst` пишутся — verify
      что каждое имеет соответствующее restore.
- [ ] **Generic helper chain test**: цепочка 5 generic-методов
      (A→B→C→D→E) — каждый mono'd с разным subst — все правильны.
- [ ] **Nested mono**: `outer[A]() { inner[A]() }` — inner получает
      A через outer subst.
- [ ] **Perf bench**: `nova test` wall-clock до/после; threshold ±5%.
- [ ] **Spec sync**: если save/restore meaningful — описать invariant
      в `spec/decisions/03-syntax.md` (D108 mono-pass invariants).
- Tests (4):
  - `nova_tests/plan55/f4_hashmap_clone_method.nv` — root repro.
  - `nova_tests/plan55/f4_generic_helper_chain.nv` — A→B→C→D→E.
  - `nova_tests/plan55/f4_nested_mono_subst.nv` — outer/inner.
  - `nova_tests/plan55/f4_negative_helper_invariant_violation.nv` —
    diag (если возможно).

### Implementation guideline

1. **Investigation phase (must-do before fix):**
   - Add `NOVA_DEBUG_MONO=1` env-flag → eprintln traces в emit_fn entry/exit.
   - Repro: backport `HashMap.@clone()` в hashmap.nv, run failing test
     with NOVA_DEBUG_MONO=1, capture trace.
   - Identify exact stack-frame where `current_fn_return_ty` flips
     from "nova_unit" to "nova_int" or `current_type_subst` leaks.
2. **Fix (predictions, verify by trace):**
   - `current_fn_return_ty` save/restore around recursive mono'd
     emit_fn в emit_call.
   - `current_type_subst` push/pop stack semantics — каждый mono-call
     должен иметь свой subst on top, restored on return.
3. **Audit:** все `current_fn_return_ty = ...` и
   `current_type_subst.clear()` / `.insert(...)` должны иметь
   matching restore (RAII pattern предпочтительно).
4. **Rollback:** isolated commit; `git revert` если regression > 1%.

### Estimate

~50-150 LOC investigation + fix + 4 tests + audit tooling. Risk
**high** — затрагивает mono pass type-context propagation. Mitigation
через DEBUG trace + audit-grep + perf-bench.

---

## Ф.5 — `[M-52-type-inference-no-annotation]`: mono name-collision

### Симптом

`let m = ["a": 1, "b": 2]` (без явной аннотации `HashMap[str, int]`)
→ CC-FAIL:

```c
nova_int _m0 = Nova_WriteBuffer_static_with_capacity(((nova_int)2LL));
//             ↑ WriteBuffer вместо HashMap
_m0.insert_new(_m0_k0, _m0_v0);  // ← method call on nova_int!
```

Mono pass для `_m.with_capacity` выбирает **WriteBuffer.with_capacity**
вместо HashMap из-за overload collision (множество types имеют этот
method).

### Корень

Trace: `inferred_target_type = Some(["HashMap"])` **уже устанавливается**
annotate-pass'ом в desugar. Desugar строит `HashMap[K, V].with_capacity(n)`
с turbofish. Но **mono pass игнорирует turbofish hint** и резолвит
по name lookup, выбирая first overload (`WriteBuffer`).

С explicit annotation `let m HashMap[K,V] = [...]` — работает
(там путь другой через expected propagation).

### Fix

В mono-pass resolution для method-call с turbofish:
- Если `obj.kind == TurboFish { base: Ident(T), type_args: [...] }` —
  resolve **строго** по T name (skip overload name-collision).
- Сейчас path фильтрует только по arity/param-types — не учитывает
  явную turbofish target.

Альтернатива: в desugar.rs всегда эмитить **path-qualified callee**
`std.collections.hashmap.HashMap[K,V].with_capacity` — но требует
import resolution в desugar (нетривиально).

### Acceptance (prod-grade)

- [ ] `let m = [k:v]` без аннотации работает (resolves to HashMap)
- [ ] Не ломает explicit-annotation case (`let m HashMap[...] = [...]`)
- [ ] **Nested map literal:** `let m = [k: [k2: v]]` (HashMap of
      HashMap) — inference rec.
- [ ] **Mixed value types:** `let m = ["a": ["x":1]]` works.
- [ ] **Diagnostic improvement:** при ambiguous overload — error
      с предложением turbofish (Plan 36 R7).
- [ ] **Interp parity:** `nova run` тот же результат.
- [ ] **Codegen debug:** при resolve через turbofish — `/* MONO:
      resolved <T>::<m> via turbofish hint */` comment (debug only).
- Tests (3+):
  - `nova_tests/plan55/f5_hashmap_infer_no_annot.nv` — positive.
  - `nova_tests/plan55/f5_nested_map_infer.nv` — nested.
  - `nova_tests/plan55/f5_negative_map_ambiguous.nv` — diagnostic.

### Implementation guideline

1. **In mono-pass static-call resolution** (look for `with_capacity`
   resolve in emit_c.rs ~5700+):
   - If `path = [T_name, method]` with `inferred_target_type` set
     (или explicit turbofish on T) — match **strict** на T_name.
   - Иначе fallback to current overload-by-arity.
2. **Annotation pass:** verify MapLitAnnotator передаёт
   `inferred_target_type` в desugared call.
3. **No-op for explicit annotation:** existing path uses expected-
   type propagation — не трогать.

### Estimate

~50 LOC + 3 tests + diag improvement. Risk **medium** — затрагивает
mono method resolution path, регрессия возможна. Mitigation:
isolated commit, single-call-site change.

---

## Ф.6 — `[M-52-multi-instance-hashmap-collision]`: multi-mono collision

### Симптом

```nova
fn f() {
    ro a HashMap[str, int]  = ["x": 1]
    ro b HashMap[int, str]  = [1: "x"]   // ← разные K, V
    ro c HashMap[int, int]  = [1: 1]     // ← третья пара
}
```

→ CC-FAIL `assigning NovaOpt_nova_str from NovaOpt_nova_bool`
(или похожие type-mismatch errors).

Workaround сейчас: разделять positive-тесты Plan 52 на 3 файла
(str→int, int→str, int→int).

### Корень

Pre-existing baseline mono pass issue (Plan 48). Когда **несколько**
HashMap inst используются в одной fn, mono pass:
- Корректно генерирует Nova_HashMap____nova_str__nova_int,
  Nova_HashMap____nova_int__nova_str, etc (отдельные structs)
- НО где-то path резолюции method-call использует **первый**
  mono'd instance — даёт неверный type assignment

### Fix

Investigation в mono pass:
- `resolve_mono_type_args` — корректно ли picks instance per call-site
- `instantiate_type_subst` — leak между instances?
- `current_type_subst` save/restore при nested mono'd calls

### Acceptance (prod-grade)

- [ ] Test с 3 разными HashMap[K, V] инстансами в одной fn работает
- [ ] Удалить workaround "split into 3 files" в map_literals tests
- [ ] M-52-multi-instance-hashmap-collision закрыт
- [ ] **Triple instance**: `HashMap[str,int]` + `HashMap[int,str]` +
      `HashMap[int,int]` в одной fn — все 3 mono'd correctly.
- [ ] **Same K diff V**: `HashMap[str,int]` + `HashMap[str,bool]` —
      разные V не collidять.
- [ ] **Same V diff K**: симметричный case.
- [ ] **Generic types в целом** (не только HashMap): `Box[int]` +
      `Box[str]` в одной fn — works.
- [ ] **Perf bench**: `nova test` wall-clock до/после; ±5%.
- [ ] **Spec sync**: invariant'ы mono pass — в spec/decisions/03-syntax.md.
- [ ] **DEBUG trace** (от Ф.4): `NOVA_DEBUG_MONO=1` dumps subst per
      call-site — verify нет leak'а между instances.
- Tests (3+):
  - `nova_tests/plan55/f6_multi_hashmap_in_fn.nv` — root repro.
  - `nova_tests/plan55/f6_triple_generic_instance.nv` — 3 generic types.
  - `nova_tests/plan55/f6_negative_instance_type_mismatch.nv` — diag.

### Implementation guideline

1. **Pre-flight diff:** dump mono'd .c для 5 «тяжёлых» tests до/после:
   `channels_*.nv`, `hashmap_*.nv`, `retry_*.nv`, `supervised_*.nv`,
   `contracts_*.nv`. Изменения должны быть только additive (новые
   instance, не модификация existing).
2. **Investigation:** при NOVA_DEBUG_MONO=1 — посмотреть subst для
   каждого emit_call в multi-instance test; identify где subst
   меняется не на call boundary.
3. **Fix prediction:** `resolve_mono_type_args` или
   `instantiate_type_subst` имеет shared mutable state между instances.
   Fix: snapshot subst per-call в local variable, не через
   `self.current_type_subst.insert(...)` без restore.
4. **Rollback:** isolated commit; `git revert` если > 1% regression.

### Estimate

~80-150 LOC + 3 tests + dump tooling. Risk **high** — затрагивает
core mono pass. Mitigation: pre-flight diff, perf-bench, isolated
commit.

---

## Acceptance criteria (Plan 55, prod-grade)

**Закрыто (полностью):**

- [x] Ф.0 — non-negotiables применены на каждой фазе (interp parity
      пропущена — interpreter не поддерживается currently, см. note ниже).
- [x] Ф.1 — `[M-array-of-func-mono]` fixed; collect[T] pattern works;
      capture + GC stress test pass.
- [x] Ф.2 — match-arm pattern_inner_type; Some/None + nested (Some(Ok(v))) +
      Block arm + neg diag. Также Pattern::Record bindings (followup).
- [x] Ф.3 — infer-return-type-from-body (universal D45 implementation);
      Stmt::Expr (void) cast для unit/struct; FnBody::Expr only (Block
      сохраняет backward-compat).
- [x] Ф.4 — mono-pass type-context corruption fixed (save/restore
      `current_fn_return_ty`, protocol-method whitelist для
      eq/lt/gt/hash/is_*, placeholder-mono skip preventive measure,
      NOVA_DEBUG_MONO env-var tool готов).
- [x] Ф.5 — mono name-collision fixed (auto-closure через Ф.4);
      `let m = [k:v]` без аннотации работает.
- [x] Ф.6 — multi-instance HashMap collision fixed (Pattern::Record
      bindings extraction); triple-instance + 3 разных HashMap[K,V]
      работают; `str.len()` через method (collision fix).
- [x] Ф.7 — 12 baseline NEG-* (11 ASCII anchors + decreases counter
      limit 1M→10K runtime bug fix).
- [x] Ф.8 — fixture directories convention (`fixtures/` + `_fixture.toml`
      sentinel skip из test discovery).
- [x] Map-spread infrastructure (parser + desugar + annotator) — full
      codegen зависит от Plan 56.

**Validation:**

- [x] Полный `nova test` (release) — **558 PASS / 0 FAIL / 40 SKIP**
      (+49 PASS / −26 FAIL vs pre-session baseline).
- [x] **19+ новых тестов** в `nova_tests/plan55/`.
- [x] **`docs/simplifications.md`** — 10+ закрывающих записей.
- [x] **`docs/project-creation.txt`** — секция Plan 55 EOD с summary.
- [x] **`docs/plans/README.md`** — статус Plan 55 → ✅ ЗАКРЫТ.
- [x] **Spec sync** — D45 + D108 обновлены с реализацией + mono
      invariants (spec/decisions/03-syntax.md).
- [x] **Save/restore audit** — `current_fn_return_ty` /
      `current_type_subst` grep audit: все save'ы паирные, 0 leak'ов
      (см. docs/simplifications.md).
- [x] **`map_literals/positive_str_int`** workaround removed — inferred
      (без annotation) works (regression guard test added).

**Out-of-scope / deferred (с targeted Plan'ом):**

- [ ] `HashMap.@clone()` + `@merge_from()` в stdlib — blocked by
      `[M-erased-generic-method-dispatch]` → **Plan 56** (vtable
      architecture). Preventive skip-placeholder-mono в Plan 55 как
      defensive measure, real fix через vtable.
- [ ] `nova run` interp parity — interpreter currently отстаёт от
      codegen, к нему возвращаемся позже (отдельная инициатива).
- [ ] Perf bench ±5% — не измерялся явно → **Plan 57** (perf benchmark
      infrastructure).
- [ ] Cross-toolchain MSVC + GCC verification → **Plan 58** (cross-
      toolchain matrix).

---

## Size estimate (prod-grade, 2026-05-16)

| Фаза | LOC | Tests | Risk | Зависимости |
|---|---|---|---|---|
| Ф.0 — principles & audit | docs only | — | — | — |
| Ф.1 — array-of-func-mono + capture + GC | ~120-180 | 4 | medium | runtime/array.h |
| Ф.2 — match-arm pattern_inner_type + nested + Block | ~120-160 | 4 | medium-low | complement Ф.1 |
| Ф.3 — Duration.into() + auto-derive | ~30-100 | 4 | low | нет |
| Ф.4 — mono-pass corruption + DEBUG tool + spread validation | ~150-250 | 4+spread | high | unlocks Plan 52 spread |
| Ф.5 — mono name-collision + nested map + diag | ~50-80 | 3 | medium | independent |
| Ф.6 — multi-instance + pre-flight diff + perf-bench | ~100-200 | 3 | high | independent |
| **Итого** | **~570-970 LOC** | **22 tests** | mixed | mostly independent |

Was P3 ~350-630 LOC / 6 tests. Now P1 ~570-970 / 22 tests + tooling.
Delta — это и есть production-grade gap (test coverage, tooling,
docs, perf bench).

### Внутренние зависимости Ф.4-Ф.6

Все 3 — **одна категория** (mono-pass infrastructure), но **разные
instance** проблем:
- Ф.4: **corruption** — type-context перезаписывается между mono calls
- Ф.5: **name-collision** — turbofish hint игнорируется при overload
- Ф.6: **multi-instance leak** — instance picked wrong per call-site

Может быть один root cause (что-то в save/restore type-state mono pass),
тогда fix Ф.4 закроет и Ф.5/Ф.6. Может быть 3 разные — придётся
последовательно.

**Рекомендуемый порядок:** Ф.4 первой (даёт DEBUG trace для category)
→ Ф.5 (independent проверка) → Ф.6 (тяжёлая, нужна с stable Ф.4 базой).

---

## Связь

- **Plan 48** — closures-in-generics; Ф.1 этого плана closure final Ф.7
  acceptance для `[]fn->T` (после Ф.1 — Plan 48 `[]fn()->T` полностью
  closed). Ф.6 — Plan 48 mono baseline.
- **Plan 49** — `Ф.2` касается reason()/match patterns для CancelToken[T];
  fix снимает workaround «уникальные pattern-var names».
- **Plan 52.x** — Ф.4/Ф.5/Ф.6 блокируют финальное закрытие limitations:
  - Ф.4 → разблокирует M-52-spread-not-supported (Plan 52.1 Ф.1
    deferred, Plan 52.3 Ф.2/Ф.3 deferred)
  - Ф.5 → разблокирует M-52-type-inference-no-annotation (Plan 52.3 Ф.4
    deferred)
  - Ф.6 → разблокирует M-52-multi-instance-hashmap-collision
    (workaround "split tests" в map_literals/)
- **Plan 54** — Ф.1/Ф.2/Ф.3 — direct followup от Plan 54 EOD findings.
- **D73** `From`/`Into` protocols — Ф.3 касается Into auto-derive path.

---

## Ф.7 — Baseline NEG-* cleanup (production-grade, 2026-05-16 EOD+1)

### Контекст

После закрытия Ф.1-Ф.6 + 3 followup'ов в baseline остаются **12 NEG-***
тестов: expectation drift между текущим diagnostic message от компилятора
и pattern в `// EXPECT_COMPILE_ERROR <pattern>` / `// EXPECT_RUNTIME_PANIC
<pattern>` маркере теста.

| Test | Тип | Expected pattern |
|---|---|---|
| `negative_capability/p50_method_name_collision` | NEG-WRONG-MSG | 'передаётся только по имени' |
| `negative_capability/p50_positional_default_single` | NEG-WRONG-MSG | 'передаётся только по имени' |
| `negative_capability/p50_positional_default_static_method` | NEG-WRONG-MSG | 'передаётся только по имени' |
| `negative_capability/p50_positional_default_instance_method` | NEG-WRONG-MSG | 'передаётся только по имени' |
| `negative_capability/p50_positional_default_with_trailing` | NEG-WRONG-MSG | 'передаётся только по имени' |
| `negative_capability/p50_positional_default_generic` | NEG-WRONG-MSG | 'передаётся только по имени' |
| `negative_capability/p50_positional_default_among_many` | NEG-WRONG-MSG | 'передаётся только по имени' |
| `negative_capability/p50_multiple_positional_defaults` | NEG-WRONG-MSG | 'передаётся только по имени' |
| `negative_capability/p50_positional_default_static_method` | NEG-WRONG-MSG | 'передаётся только по имени' |
| `negative_capability/fail_handler_no_exit_rejected` | NEG-WRONG-MSG | 'handler-method `Fail.fail` обрабатывает...' |
| `negative_capability/np_trailing_double_bind` | NEG-WRONG-MSG | 'связан и trailing-формой, и именованным' |
| `expected_runtime/contracts_decreases_recursion_fail` | NEG-WRONG-PANIC | 'decreases recursion depth exceeded 10000' |

### Production-grade approach (НЕ простой find/replace)

**Каждый тест аудитится индивидуально:**

1. Прогнать test, capture **actual** diagnostic message.
2. **Quality assessment:** actual vs expected — кто лучше?
   - `better` (newer message более ясный/точный) → update test expected.
   - `same intent, different wording` → update test expected.
   - `worse / regression` (newer message потерял information) → fix diagnostic в codegen.
   - `missing diagnostic entirely` (test rejects, но без proper message) → add diagnostic в codegen.
3. **Spec compliance check:** актуальное сообщение matches spec
   (Plan 36 R7 quality bar: error → file:line + suggestion + hint).
4. **AI-friendliness:** error message должен вести LLM/human к
   user-actionable fix.
5. **Если изменяем codegen** — добавить **новый** positive test
   проверяющий что message содержит ключевые слова (regression guard).
6. **Commit per logical group** (p50_* как один commit, остальные
   индивидуально).

### Acceptance

- [ ] 12 NEG-* tests pass — actual diagnostic matches expected pattern.
- [ ] Каждый fix аудит'ed: либо diagnostic улучшен, либо expected
      обновлён с rationale в commit message.
- [ ] Spec sync если diagnostic меняется в codegen.
- [ ] Regression: full `nova test` без новых FAIL.
- [ ] `docs/simplifications.md` — закрывающие записи.

### Estimate

~50-150 LOC. Risk low (test updates) с возможными codegen improvements.

---

## Ф.8 — Baseline doc/fixtures cleanup (после Ф.7)

### Контекст

14 doc/fixtures/* sample-файлов в `nova_tests/doc/fixtures/` —
**фикстуры для Plan 45 `nova doc`**, не настоящие тесты. Test runner
подбирает их по правилу discovery, build пытается компилировать
(требуется `main`) → CC-FAIL `undefined symbol: nova_fn_main_impl`.

| Fail | Cause |
|---|---|
| `doc/fixtures/basic/sample` | No main fn (doc fixture) |
| `doc/fixtures/capability_forbid/sample` | No main fn |
| `doc/fixtures/doctests/sample` | No main fn |
| `doc/fixtures/expect_output/sample` | No main fn |
| `doc/fixtures/kinds/sample` | No main fn |
| `doc/fixtures/links/sample` | No main fn |
| `doc/fixtures/must_verify/sample` | No main fn |
| `doc/fixtures/module_attrs/sample` | No main fn |
| `doc/fixtures/orphan/sample` | No main fn |
| `doc/fixtures/real_attrs/sample` | No main fn |
| `doc/fixtures/sections/sample` | No main fn |
| `doc/fixtures/should_panic/sample` | No main fn |
| `doc/fixtures/stability/sample` | No main fn |
| `doc/fixtures/reexport/sample` | CODEGEN-FAIL: cross-file import |

### Решение

**Production-grade**: test_runner должен **исключать** доп-fixtures
из discovery. Опции:

A. **Marker file** — добавить `_fixture.toml` или `.fixture` маркер в
   каждом каталоге; test_runner skip'ит каталоги с маркером.
B. **Module-attribute** — `#fixture` атрибут перед `module` decl;
   test_runner skip'ит модули с этим attr.
C. **Naming convention** — каталоги внутри `*/fixtures/` skip'ятся
   автоматически. Простейшее.
D. **Explicit опция в nova.toml** — `[testing] exclude = ["doc/fixtures/**"]`.

Recommend: **C + D combo** — С автоматический skip для convention,
D explicit override.

### Acceptance

- [ ] 14 doc/fixtures tests skipped (не FAIL).
- [ ] Plan 45 doc-pipeline всё ещё может load эти fixtures как
      doc-input (не tests).
- [ ] `docs/test-conventions.md` обновлён.

### Estimate

~30-50 LOC test_runner.rs + config. Risk low.

---

## Ф.9 — Followup tracking (после Ф.7+Ф.8)

После Ф.7+Ф.8 baseline должен быть **0 FAIL'ов** (все 25 закрыты или
skipped). Оставшиеся deferred → Plan 56+:

- `[M-erased-generic-method-dispatch]` — vtable dispatch для bound K
  methods. Разблокирует `HashMap.@clone()`, `HashMap.@merge_from()`,
  `HashMap.@filter()`. ~3-5 dev-days архитектурной работы.
- `[M-time-handler-sleep-mismatch]` — `Time.sleep(int)` →
  `Time.sleep(Duration)` semantic evolution. Stdlib-wide migration:
  effect schema + 20+ callsites + runtime impl. ~1-2 dev-days.
- Прочие pre-existing M-маркеры (Plan 52.1, 52.3, ...) — самостоятельные
  followup'ы в собственных планах.

### Acceptance Ф.9

- [ ] Plan 56 создан с указанными scope items.
- [ ] Plan 55 README статус обновлён → **ЗАКРЫТО** окончательно.
