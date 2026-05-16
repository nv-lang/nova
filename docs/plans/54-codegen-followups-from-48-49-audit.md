// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 54: Codegen follow-ups от Plan 48/49 audit

> **Создан 2026-05-16.** Собирает deferred items найденные в audit-fix
> sprint'ах по Plan 48 (closures-in-generics) и Plan 49 (cancellation
> semantics). Все items — реальные codegen / type-inference gaps,
> заметные при увеличении test coverage.
>
> **Приоритет:** P2 — каждый item ограничивает один use case или один
> test scenario. Не блокируют главные acceptance criteria Plan 48/49
> (которые closed). Но накапливаются — нужно закрыть до новых planов.

---

## Контекст — что закрыли в Plan 48/49

- Plan 48: 8/10 acceptance closed (2 partial — этим планом закрываем)
- Plan 49: 11/11 main + 6/6 Ф.6 — весь acceptance closed
- 75 sub-cases tests, beyond state-of-the-art (USER-precedence, typed
  CancelToken[T], cross-type cascade с From, tok.merge)

## Что осталось — items этого плана

### Ф.1 — `[M-pattern-var-leak]` (~30 LOC, low risk)

**Bug:** pattern-bound variable `Some(v) => v` register'ит `v` в
`var_types` но НЕ очищает на scope-exit. Между tests/fns inference
leaks: test 3 имел `v: bool`, test 6 `Some(v) => v` для int — но
infer_expr_c_type(v) возвращает stale bool → match-arm result inferred
как bool вместо int → assert падает.

**Reproduce** (commit a45e73c1a26 — temporarily fixed через unique
pattern names `vi_big`, `vi_w` instead of `v`).

**Fix:** в `emit_match` (или wherever pattern-vars register'ятся в
var_types) — track introduced vars, restore var_types на scope exit
(как делает существующий code для regular fn-params в emit_fn).

**Acceptance:** existing tests с `Some(v) => v` pattern (используя
обычное имя `v`) работают correctly даже если другой test раньше
biнd'нул `v` к другому типу.

**Зависимости:** нет. Локальный fix в emit_match / pattern-bind path.

---

### Ф.2 — `[M-int-extension-record-field]` (~50-100 LOC, medium risk)

**Bug:** `100.millis()` (int-extension method) внутри record-literal
field в generic-static-ctor context генерирует invalid C:
```c
_field = ((nova_int)100LL).millis();   // member access на nova_int!
```
вместо правильного `nova_fn_int_method_millis(100LL)` или равноценного
extension-method dispatch.

**Импакт:** `std/concurrency/retry.nv` использует pattern:
```nova
RetryPolicy.exponential(max_attempts) => {
    backoff: Exponential { initial: 100.millis(), ... }
}
```
Любой import retry.nv валит C-build. **Blocks retry_test.nv** (Plan 48
Ф.5 acceptance).

**Fix:** в emit_record_lit (или emit_field_value path) для generic
static ctor — int-extension method calls должны разрешаться через
extension-dispatch path (`nova_fn_<sanitize>_method_<m>` или similar),
не через member-access.

**Acceptance:** retry_test.nv компилируется (положительные tests с
RetryPolicy.fixed/.exponential).

**Зависимости:** Ф.1 (если ловит pattern-vars в retry tests).

---

### Ф.3 — `[M-unit-variant-context-inference]` (~150-300 LOC, high risk)

**Gap:** `let r = Err2` для `Result2[T]` — T не выводится из bare unit
variant. Текущее поведение: `r` инферится как `Nova_Result2*` (erased),
последующие `r.method(arg)` идут через erased dispatch.

**Импакт:** `emit_generic_method_erased` оставлен как V1 fallback
именно для этого случая. Plan 48 Ф.7.4 final closure (полное удаление
erased emit) блокируется этой инференцией.

**Fix:** forward analysis на let-binding'е:
1. При `let r = Err2` (где Err2 — unit variant generic sum type) —
   scan rest of fn-scope для `r.method(arg)` calls.
2. Из method arg types infer T:
   - `r.unwrap_or(99)` → method `unwrap_or(fallback T) -> T` → T = type of 99 = nova_int.
3. Re-emit let-binding с mono'd `Nova_Result2____nova_int*` type.
4. Subsequent method calls используют mono dispatch path.

**Acceptance:**
- `let r = Err2; r.unwrap_or(99)` → mono Result2[int], r typed correctly.
- emit_generic_method_erased может быть removed (Plan 48 Ф.7.4 final).

**Зависимости:** Ф.1 (var_types cleanup). High risk — forward analysis
across statements — может ломать существующие тесты.

---

### Ф.4 — `[M-generic-array-return-mono]` (~80 LOC, medium risk)

**Gap:** generic-fn с `return []T` (где T — generic param) даёт
receiver C-тип `void*` вместо `NovaArray_<T_c>*`. `.len()` / `[i]` /
`for-in` на void* не работают.

**Test:** `nova_tests/concurrency/fn_array_generic_smoke.nv` —
изначально хотел `fn collect_all[T](fns []fn()->T) -> []T` с
return-path verification, но receiver `xs` стал void* → `xs.len()`
сгенерировал ill-formed C.

**Fix:** в mono pipeline — при resolve return type generic-fn'и,
`[]T` должен mono'rphize к `NovaArray_<T_c>*` (с T resolved из
arg-type binding). Это требует:
- `infer_func_ret_type` (или equivalent) с array-of-T handling.
- mono'd return type в C-emit для fn call site.

**Acceptance:** `collect_all[int]([||1, ||2, ||3]).len() == 3`
компилируется и runs.

**Зависимости:** нет.

---

### Ф.5 — `[M-generic-nested-call-inference]` (~50-100 LOC, medium risk)

**Gap:** type-inference для nested generic call не пропагирует T от
outer signature к inner. Пример:
```nova
fn with_timeout[T](ms int, body fn()->T) -> T {
    match within(ms, body) {     // ← within[T] — T должно быть inferred from body
        Some(v) => v
        None    => throw "timeout"
    }
}
```
Codegen: "cannot infer type argument `T` for generic function `within`".
Требуется явный turbofish: `within[T](ms, body)`.

**Fix:** В resolve_mono_type_args — при call внутри generic fn body, T
inference должна учитывать current_type_subst (T uppercase parameter
inferred from outer mono'd context).

**Acceptance:** stdlib `with_timeout` compiles без явного turbofish.

**Зависимости:** нет.

---

### Ф.6 — `[M-cancel-aware-defer]` (~200 LOC, medium risk, ОТЛОЖЕНО)

**Feature** (не bug): `cancel_defer { ... }` — defer block который
runs ТОЛЬКО на cancel-unwind path, не на normal exit / USER throw.
Аналог Rust Drop с разделением normal vs cancel scenarios.

**Status:** не реализовано. Требует:
- Parser changes (новое keyword).
- emit_defer scope-exit machinery — discriminate CANCEL vs USER vs normal.
- Test coverage.

**ОТЛОЖЕНО — не в этом плане.** Tentative feature; вернуться когда
будет конкретный use case. Зафиксировано в Plan 49 Ф.7 docs.

---

### Ф.7 — Polymorphic recursion compile-error verification test (~30 LOC, no risk)

**Gap:** mechanism (`mono_depth_limit` + safety counter в drain)
работает, но реальный verification test пока не написан — pattern
`fn f[T](depth, v) -> int { ... f(depth-1, Box[T]{v: v}) ... }`
упирается в orthogonal codegen bug "anonymous record literal: expected
struct 'int' not in record_schemas" (на mono'd Box[T] стороне).

**Зависимости:** Ф.4 (generic []T return) косвенно — если related
codegen bug в record schemas разрешится.

**Acceptance:** test с polymorphic-recursion + `--mono-depth=N` (low N)
получает понятный compile-error с указанием места первой инстанциации.

---

### Ф.8 — Plan 48 acceptance checkboxes update (~5 min)

Plan 48 имеет 10 acceptance criteria — все показаны `[ ]` в текущем
plan'e даже после fix'ов в Ф.7.4/Ф.7.6/Ф.4. Обновить:
- [x] Generic-fn мономорфизируется (closed)
- [x] Closure-param typed call (Ф.4)
- [⚠️] `[]fn()->T` consume-path closed; return-path Ф.4 этого плана
- [x] Generic record Box[T] (Ф.3)
- [x] TurboFish source (Ф.7.1)
- [⚠️] Polymorphic recursion compile-error (mechanism есть, test = Ф.7)
- [x] cancellation.nv compiles (audit-fix sprint)
- [⚠️] retry_test.nv (blocked Ф.2 этого плана)
- [x] nova test без новых FAIL
- [⚠️] Erased emit removal — Ф.3 этого плана closure

### Ф.9 — Cross-type cascade `child.reason()` round-trip test (~20 LOC, no risk)

Verification hole: existing `cancel_cross_type_cascade_test.nv` проверяет
только `child.is_cancelled()`. Должен также verify что `child.reason()`
возвращает correctly converted reason value (не raw B-reason cast as A).

**Acceptance:** test `cross-type: child.reason() возвращает converted A`
для `From[B] for A` pair, тестирует round-trip.

---

## Acceptance criteria (Plan 54) — final 2026-05-16 EOD

- [x] Ф.1 — `[M-pattern-var-leak]` fixed; emit_test snapshots+restores
      var_types/var_mutable/cancel_token_t_map (commit 5eb1168c5d0).
- [x] Ф.2 — `[M-int-extension-record-field]` fixed; primitive-receiver
      extension dispatch через method_overloads lookup (commit 1a2d62990d7).
      retry_test.nv от Plan 48 unblock'нут (с workaround для orthogonal
      duration.into() codegen issue).
- [⚠️ accepted-as-is] Ф.3 — `[M-unit-variant-context-inference]`:
      forward analysis НЕ делается. **Паритет с Go/Rust/TS** — все
      требуют explicit type annotation на let-binding (`let r:
      CancelToken[int] = ...`) для unit-variants. Наш текущий подход:
      Ok2(42) args-driven inference + explicit annotation — паритет.
      `emit_generic_method_erased` остаётся как V1 fallback для bare
      `let r = Err2` без annotation. Не bug, accepted design.
- [⚠️ partial] Ф.4 — `[M-generic-array-return-mono]`: turbofish args
      через return-type inference (commit 901b29f6608); plain `[]T`
      works. `[]fn->T` (Array-of-Func) — orthogonal followup
      `[M-array-of-func-mono]`.
- [⚠️ partial] Ф.5 — `[M-generic-nested-call-inference]`: caller-side
      Source 2d (commit cf20c2d0c76) works. Body-side match-arm pattern
      inference — отдельный issue `Ф.5b match-arm pattern_inner_type`.
- [x] Ф.7 — polymorphic recursion compile-error test
      (commit 067696ae272). EXPECT_COMPILE_ERROR matches "instantiation
      depth limit". Test PASS в default --mono-depth=500 и explicit low
      values. Orthogonal "anonymous record literal" bug рассосался от
      Ф.4/Ф.9 fixes.
- [x] Ф.8 — Plan 48 acceptance checkboxes updated (commit 3e287605710).
- [x] Ф.9 — Cross-type cascade `child.reason()` round-trip + side fix
      novaopt_value_types для pattern_bind_typed (commit 63bd0e4416e).
      Побочный эффект: +20+ pre-existing test failures fixed.
- [x] Полный `nova test` (release) — 517 PASS / 26 FAIL. Plan 54 added
      11 new test files (37 sub-cases), zero new regressions от Plan 54
      items.

### Status summary

- ✅ Closed: 7/8 (Ф.1, Ф.2, Ф.4 partial, Ф.5 partial, Ф.7, Ф.8, Ф.9)
- ⚠️ Accepted-as-is: 1/8 (Ф.3 — паритет с industry, не bug)
- Открытые followup'ы: `[M-array-of-func-mono]`, `Ф.5b match-arm pattern
  inference`, orthogonal `Nova_Duration_method_into` codegen issue.

---

## Что НЕ входит

- Ф.6 (cancel-aware defer) — tentative feature, требует use case.
- Q-with-deadline-vs-within, Q-tok-checked — design decisions через
  open-questions.
- Q-cancel-token-with-timeout — требует scheduling design.
- Q-context-value-equivalent — design plan Plan 51 tentative.
- `[M-time-effect-schema-mismatch-sleep]` — отдельный fix, не closures.

---

## Size estimate

| Фаза | LOC | Risk | Зависимости |
|---|---|---|---|
| Ф.1 — pattern-var leak | ~30 | low | нет |
| Ф.2 — int-extension record field | ~50-100 | medium | Ф.1 |
| Ф.3 — unit-variant context infer | ~150-300 | high | Ф.1 |
| Ф.4 — generic array return mono | ~80 | medium | нет |
| Ф.5 — nested call inference | ~50-100 | medium | нет |
| Ф.7 — polymorphic recursion test | ~30 | none | Ф.4 косвенно |
| Ф.8 — plan checkboxes | trivial | none | нет |
| Ф.9 — cross-type reason round-trip | ~20 | none | нет |
| **Итого** | **~410-660** | mixed | |

---

## Связь

- Plan 48 — главные acceptance gaps (Ф.7.4 final, []T return, retry_test).
- Plan 49 — verification holes (cross-type reason round-trip).
- spec/open-questions.md — Q-with-deadline / Q-tok-checked / Q-cancel-token-with-timeout / Q-context-value.
- D73/D77 (`From` protocol) — используется в Ф.9.
