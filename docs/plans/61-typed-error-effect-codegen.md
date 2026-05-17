# Plan 61: Typed `Fail[E]` codegen — hybrid (per-E mono + erased `Fail[any]` fallback)

> **Status:** proposed (2026-05-17, revised). Architectural change. Закрывает три silent UB в effect-codegen + два открытых open-issue (Plan 11 cross-effect throw, Plan 49 «typed Fail-канал — future work»). Реализует [D65](../../spec/decisions/04-effects.md#d65) на C-уровне.

---

## Цель в одной фразе

`throw err` для `Fail[E]` должен **передавать типизированное значение `E`** через runtime (а не silently кастовать его pointer к `nova_int`), handler arm `|e| ...` должен видеть `e: E` с полным набором полей, и одновременно `Fail` (= `Fail[any]`) — catch-all sugar по D65 правило 1 — должен продолжать работать через erased fallback. Желаемое состояние = **Rust `Result<T, E>` + `Box<dyn Error>` parity** (zero-cost typed когда возможно, erased когда нужно), что строго лучше Go (`error` interface, single untyped path), TS (`throw any`), Java (checked exceptions без generic), и параллельно с Swift 6.0 typed throws `func f() throws(E)` + untyped `throws`.

---

## Problem — verified silent UB stack

### Silent UB #1: `throw user-error` — pointer-to-int type pun

[`compiler-codegen/src/codegen/emit_c.rs:8503-8517`](../../compiler-codegen/src/codegen/emit_c.rs#L8503):

```rust
let val_ty = self.infer_expr_c_type(value);
let val = self.emit_expr(value)?;
if val_ty == "nova_str" {
    self.line(&format!("Nova_Fail_fail({});", val));
} else {
    self.line(&format!("Nova_Fail_fail(nova_int_to_str((nova_int)({})));", val));
}
```

Для `throw ParseError.Bad`: value type — `Nova_ParseError*` (pointer); код кастует pointer к `nova_int` и форматирует как число. **Payload теряется полностью**, handler получает строку `"140726384521232"`. План v1 описывал это как "C compile error" — на самом деле **silent data loss + runtime garbage message**.

### Silent UB #2: `nova_throw_value` для `expr!!` — placeholder

[`compiler-codegen/nova_rt/effects.h:140-150`](../../compiler-codegen/nova_rt/effects.h#L140):

```c
void nova_throw_value(void* v) {
    /* TODO: structured throw — see Plan 61. Bootstrap fallback: */
    nova_throw(nova_str_from_literal("Result::Err"));
}
```

`expr!!` через `Err(e)` → `nova_throw_value(e)` → каждый раз бросается одна и та же строка `"Result::Err"`. Тип `e` потерян ещё до runtime. План v1 этого даже не упомянул.

### Silent UB #3: handler arm `e` parameter — wrong C type

[`effects.h:405-409`](../../compiler-codegen/nova_rt/effects.h#L405) — vtable hardcoded:

```c
typedef struct NovaVtable_Fail {
    void*               ctx;
    nova_unit          (*fail)(void* _ctx, nova_str msg);  // ALWAYS nova_str
    struct NovaVtable_Fail* prev;
} NovaVtable_Fail;
```

Handler arm `with Fail[ParseError] = |e| println(e.detail)` lowers так, что `e: nova_str` на C-уровне. `e.detail` либо не компилируется (Symptom 3 плана v1), либо silently выбирает offset в `nova_str` byte representation. Type safety теряется на handler boundary.

### Текущий workaround — широко используется, scope ≠ 2

`grep "interrupt" std/ nova_tests/` показывает **15+ мест** (план v1 нашёл только 2 с `interrupt Err(e)`):

| Pattern | Count | Файлы (выборка) |
|---|---|---|
| `interrupt Err(e)` | 2 | `std/data/semver_range.nv:134`, `std/concurrency/retry.nv:113` |
| `interrupt Some(e)` / `interrupt None` | 11+ | `uuid.nv:431/442/453`, `range.nv:262/311/319`, `ulid.nv:458/470`, `jwt.nv:184/195/211/227`, `snowflake.nv:127/140`, `bcrypt.nv:291/304`, `statistics.nv:261` |
| `interrupt -1` / `interrupt fallback` | 6 | `nova_tests/types/handler_flow_inference.nv:25,32,45,53,65,73` |
| `interrupt e` (rethrow) | 1 | `orm_demo.nv:592` |
| `interrupt to_http_error(e)` (cross-effect convert) | 5 | `http.nv:36,54`, `oxsar_port.nv:51`, `orm_demo:663,691`, `audit.nv:307` |

Эти места **работают** только потому что используют **`interrupt`** через `void* value_ptr` slot в `NovaInterruptFrame` (Plan 39 Issue A), который не парсится как структурированный тип на C-стороне — handler-arm получает opaque pointer. То есть «работает» путём упаковки данных через побочный канал. Это **не idiomatic** D65 (D65 §«Re-throw в handler» требует именно `throw expr`, не `interrupt`).

### Silent взаимодействие с Plan 49 (cancel)

[Plan 49](49-cancel-throw-routing.md) уже сделал **прямой precedent для типизированного payload через эффект**: `CancelToken[T]` с typed `reason() -> Option[T]`, runtime infra `nova_cancel_token_reason_<T_c>`, compile-time tracking `cancel_token_t_map: HashMap<var, T_c>`. Plan 61 — это **зеркальное применение того же подхода** для USER-канала (Fail-effect). Plan 49 line 543-545 явно говорит:

> Plan 49 типизирует *причину отмены* (`CancelToken[T]`); типизация *Fail-канала* — future work с D65.

Plan 61 — эта future work.

---

## Что НЕ делаем (rejected alternatives)

| Alternative | Почему отвергнут |
|---|---|
| **String-based only** (`Fail[str]` единственный вариант) | Ломает D65 правило 1 (`Fail[E]` явно spec'нут), весь stdlib переписывается, паритет с Rust/Swift теряется. |
| **Pure per-E mono без erased fallback** | Ломает D65 правило 1 (`Fail` ≡ `Fail[any]` catch-all). `Fail[any]` не может быть мономорфизирован (top-type). Также: TLS slot per E → registry overflow при множестве типов ошибок в большом проекте. |
| **Pure erased + RTTI tag (Go-style `error`)** | Type safety теряется на C-уровне; runtime overhead на boxing каждого throw; player получает worse-than-Rust ergonomics. |
| **`void*` boxing + cast в handler arm** | Похоже на текущий workaround. Не закрывает Symptom 3 (handler `e.field` requires real type, not opaque ptr); runtime overhead на бокс на каждый throw. |
| **Дублирование Plan 48 + Plan 49 инфры** (как в v1 плана) | 5-8 dev-days вместо 3-4. Plan 48 уже даёт worklist + mangle; Plan 49 уже даёт `cancel_token_t_map` pattern. |

---

## Решение: **Hybrid** (per-E mono fast path + erased `Fail[any]` fallback)

Архитектура, прямо соответствующая D65 (правила 1-5) и переиспользующая существующую инфраструктуру Plans 48 + 49.

### Принцип

1. Для каждого concrete `E`, используемого в `Fail[E]` (получаемого из Plan 48 worklist'а), генерируется **per-E specialization**: `Nova_Fail_<E>_fail(E* err)`, vtable `NovaVtable_Fail_<E>`, TLS slot `_nova_handler_Fail_<E>`. Это zero-cost typed path.
2. Параллельно сохраняется **erased path** — текущая `Nova_Fail_fail(void* boxed, NovaTypeId tid)` + TLS slot `_nova_handler_Fail_any`. Catch-all для `Fail[any]` (alias `Fail`), и fallback когда per-E handler не установлен.
3. **Throw lookup** (D65 правило 2):
   ```
   throw e: E
     → if _nova_handler_Fail_<E> != NULL: per-E fast path
     → else if _nova_handler_Fail_any != NULL: erased path with (e, TypeId<E>)
     → else: nova_throw_value(e, TypeId<E>) — longjmp to nearest fail-frame
   ```
4. **Re-throw** (D65 правило 3) — `prev` link отдельно в per-E slot и в erased slot. Codegen install'а handler swap'ает оба нужных slot'а во время handler-body invocation.
5. **`Fail[any]` handler** (catch-all) видит `e: any` — реализуется как Nova-side protocol-method dispatch (`any.is[T]() -> bool`, `any.as[T]() -> Option[T]`) поверх runtime `TypeId`.

### Lookup precedence — формализация D65 правила 2

D65 говорит «точный E → catch-all `Fail` → runtime panic». Hybrid реализует это так:

```c
// Каждый throw E codegen эмитит:
static inline void _nova_throw_typed_E(E* err) {
    NovaVtable_Fail_E* h_typed = _nova_handler_Fail_E;
    if (h_typed) {
        NovaVtable_Fail_E* current = h_typed;
        _nova_handler_Fail_E = current->prev;  // re-throw swap
        current->fail(current->ctx, err);
        _nova_handler_Fail_E = current;        // restore
        nova_throw_value_typed(err, NOVA_TID_E);  // unhandled → unwind
        return;
    }
    NovaVtable_Fail_any* h_erased = _nova_handler_Fail_any;
    if (h_erased) {
        NovaVtable_Fail_any* current = h_erased;
        _nova_handler_Fail_any = current->prev;
        current->fail(current->ctx, (void*)err, NOVA_TID_E);  // erased
        _nova_handler_Fail_any = current;
        nova_throw_value_typed(err, NOVA_TID_E);
        return;
    }
    nova_throw_value_typed(err, NOVA_TID_E);  // no handler → fail-frame longjmp
}
```

Это **strict superset** текущей семантики `Nova_Fail_fail(nova_str)`: для legacy `Fail[str]` всё работает через per-E specialization at E=str — идентичный быстрый путь, но больше нет silent cast.

### Type-erased handler `Fail[any] = |e| ...`

Внутри handler-body `e: any` — Nova-side, через D54 anonymous-protocol `any`. Pattern-match через `e is ParseError ? ... : e is LookupError ? ... : ...` использует runtime `NovaTypeId` сравнение. Это явно spec'нуто в D54 — на C-уровне realising через `nova_any_is_typeid(payload, expected_tid) -> bool`.

### Cross-effect throw в handler arm (Plan 11 open issue)

`with Fail[A] = |e| throw NewErr {...}` — handler arm бросает `B`, не `A`. Codegen:
1. Determine outer effect-context: какие handlers активны во время handler-body invocation.
2. `throw NewErr {...}` лоwers по same lookup precedence: per-B fast path → erased → unwind.
3. Critical: handler arm execution временно swap'ает `_nova_handler_Fail_A = prev` (D65 правило 3), но `_nova_handler_Fail_B` и `_nova_handler_Fail_any` остаются intact — outer handler видит `B`.

Это автоматически работает как только D65 правило 2 реализовано — никакой отдельный «cross-effect» path не нужен.

### `expr!!` на `Err(e)` — закрытие Silent UB #2

[`effects.h:140-150`](../../compiler-codegen/nova_rt/effects.h#L140) `nova_throw_value` стрипается; codegen для `expr!!` на `Result[T, E]`:
```nova
let v = result!!   // если Err(e) — throw e
```
lowers в:
```c
NovaResult_T_E r = expr;
if (r.tag == NOVA_RESULT_ERR) {
    _nova_throw_typed_E(&r.err);
}
T v = r.ok;
```

То есть `_nova_throw_typed_E` это **тот же путь, что и `throw e`** — единая инфраструктура.

### Закрытие D85 `RuntimeNoneError`

`Option!!` на `None` бросает `RuntimeNoneError` (D85). Это per-E specialization at E=RuntimeNoneError. Existing emit_c.rs path сохраняется, просто через новую инфраструктуру.

---

## Codegen ↔ Plan 48 ↔ Plan 49 reuse map

Чтобы избежать duplication: что переиспользуется, что новое.

| Component | Источник | Plan 61 переиспользует / расширяет |
|---|---|---|
| Worklist для concrete E типов | Plan 48 (`MonoKey`, instantiation queue) | **Reuse 1-to-1**: `Fail[E]` instances добавляются в тот же worklist; emit_c обрабатывает их через тот же loop. |
| Type-name mangling | Plan 48 (`mangle_c_name`) | **Reuse**: `Nova_Fail_<mangled_E>_fail`. |
| Var-level type tracking | Plan 49 (`cancel_token_t_map: HashMap<var, T_c>`) | **Mirror as `fail_e_map`** для tracking `with Fail[E] = ...` bindings и `throw expr` source type. Идентичный паттерн. |
| Per-T runtime helper generation | Plan 49 (`nova_cancel_token_reason_<T_c>`) | **Mirror as `Nova_Fail_<E>_fail`** + vtable + TLS slot. Идентичный шаблон. |
| `From`-conversion для cross-type | Plan 49 (`linked_converters` + `nova_cancel_token_bind_cascade_typed`) | **Optional reuse**: для idiom `with Fail[A] = |a| throw B.from(a)` — explicit user-side, не runtime-injected. |
| `TypeId` runtime tagging | **New** (Plan 61 adds) | `enum NovaTypeId` per-mono'd type; emit'тся одновременно с per-E vtable. Используется `Fail[any]` erased path и Nova `any.is[T]()`. |
| RuntimeError vs Error vs RuntimeNoneError | D26 / D85 — already prelude types | **Reuse**: эти типы proходят тот же per-E path; not special-cased. |

**Bottom line:** Plan 61 это **«применить Plan 48 worklist + Plan 49 cancel_token_t_map pattern к built-in Fail-эффекту»**, плюс **новый TypeId runtime layer** для erased `Fail[any]` fallback. Минимизирует дублирование.

---

## Sub-tasks (фазы)

### Ф.0 — Audit + baseline (½ day)

- [ ] Зафиксировать baseline: `nova test` count на main.
- [ ] Repro minimal: `nova_tests/plan61/silent_ub_throw_typed.nv` — throw `ParseError.Bad`, handler-arm checks `e.detail`. Сейчас должен demonstrate corruption. **Этот тест станет positive regression после Ф.3.**
- [ ] Repro для `expr!!` Silent UB #2: `nova_tests/plan61/result_bang_err_carries.nv` — `result!!` на `Err(custom)` → outer handler видит `custom`, не строку «Result::Err».
- [ ] Audit `nova throw_value` placeholder usage — grep `nova_throw_value` в emit_c.rs.

### Ф.1 — TypeId runtime infrastructure (1 day)

- [ ] Add `compiler-codegen/nova_rt/typeid.h`: `typedef uint32_t NovaTypeId;` + monotonic ID allocator (compile-time generated, не runtime — каждый mono'd type получает unique constant).
- [ ] Codegen: при monomorphization Plan 48 worklist'а emit'ит `#define NOVA_TID_<mangled> <N>` в `nova_typeids.h` (auto-gen, splice in pre-compile pass).
- [ ] Runtime helpers: `nova_typeid_to_name(NovaTypeId) -> const char*` (для diagnostic / debug panics).
- [ ] No behaviour change yet — infra only. `nova test` PASS.

### Ф.2 — `Fail[any]` erased path (1.5 days)

- [ ] Add to `effects.h`: `NovaVtable_Fail_any` (vtable with `(*fail)(void*, void* err, NovaTypeId tid)` signature) + TLS slot `_nova_handler_Fail_any`.
- [ ] Codegen: `with Fail = |e| body` (без `[E]`) emit'ит install в `_nova_handler_Fail_any` slot.
- [ ] Codegen: `throw expr` где outer context = `Fail[any]` — emit'ит call с `(value_ptr, NOVA_TID_<E>)`.
- [ ] Nova-side `any.is[T]() -> bool` / `any.as[T]() -> Option[T]` — registered via runtime_registry.rs, lowers в `nova_any_is_typeid(payload, NOVA_TID_<T>)`.
- [ ] Regression: existing `Fail` (без E) tests PASS.

### Ф.3 — Per-E specialization via Plan 48 worklist (2 days)

- [ ] Add `fail_e_map: HashMap<Var, CType>` в codegen state (mirror `cancel_token_t_map`).
- [ ] При `with Fail[E] = ...` bind site — записать `(var, E_c)` в map.
- [ ] При `throw expr` — infer expr type, lookup в `fail_e_map` для текущего effect-binding, emit per-E `_nova_throw_typed_<E>(value_ptr)`.
- [ ] Add per-E typedef + vtable + TLS slot emission в Plan 48 worklist hook (callback при инстанциации generic `Fail[E]`).
- [ ] Emit `_nova_throw_typed_<E>` static-inline в effects-per-E section.
- [ ] Regression: existing tests PASS; **new tests pass:**
  - [ ] `plan61/throw_typed_user_error.nv` — `Fail[ParseError]`, handler-arm reads `e.detail`.
  - [ ] `plan61/cross_effect_throw.nv` — `with Fail[A] = |e| throw B {...}`. Closes Plan 11 open issue.
  - [ ] `plan61/throw_typed_record_variant.nv` — `throw RuntimeError.IndexOutOfBounds { index: 5, length: 3 }`, handler reads fields.

### Ф.4 — `expr!!` typed throw + `nova_throw_value` removal (1 day)

- [ ] Codegen: `result!!` где `Err(e: E)` → `_nova_throw_typed_<E>(&r.err)` (вместо `nova_throw_value(...)`).
- [ ] Same для `option!!` → `_nova_throw_typed_RuntimeNoneError(&NOVA_NONE_ERR)`.
- [ ] Remove `nova_throw_value` placeholder из `effects.h`. Add migration shim only если есть downstream usage (vendored bdwgc/minicoro не должны зависеть — verified).
- [ ] Regression: `plan61/result_bang_err_carries.nv` PASS; `plan61/option_bang_none_typed.nv` PASS.

### Ф.5 — D65 правило 1 — `Fail` ≡ `Fail[any]` corner cases (½ day)

- [ ] Verify D65 правило 4 — re-throw в `Fail[any]` handler через `throw e`: erased handler делает swap для `_nova_handler_Fail_any`, не для `_nova_handler_Fail_E`. Test: `plan61/erased_handler_rethrow.nv`.
- [ ] Verify D65 правило 5 — sequence handler chains: `with Fail[A] { with Fail[B] { with Fail = ... { ... }}}` — каждый install'ает в свой slot. Lookup precedence работает.

### Ф.6 — Spec D-block + diagnostic (½ day)

- [ ] **Amend D65** в `spec/decisions/04-effects.md` — добавить §«Codegen representation» (hybrid: per-E + erased), §«Lookup precedence» (формализация правила 2), §«Re-throw mechanism» (per-slot prev-link).
- [ ] **Amend D85** — `RuntimeNoneError` идёт через typed path.
- [ ] **Amend D26** — `Fail[E]` requires monomorphization (link to Plan 48).
- [ ] Diagnostic: при throw value неподходящего типа в `Fail[E]` context — clear error «type mismatch in throw: expected `E`, found `F`». Сейчас silent cast.
- [ ] Diagnostic: handler arm type mismatch — «handler for `Fail[E]` expects `|e: E| ...`, found `|e: F|`».

### Ф.7 — Stdlib idiomatic migration (1 day)

Post-Ф.4 — переписать 15+ workaround мест на idiomatic D65 form:

- [ ] `std/data/semver_range.nv:134` — `with Fail[A] = |e| interrupt Err(e)` + outer match → `with Fail[A] = |e| throw NewErr {...} { ... }`.
- [ ] `std/concurrency/retry.nv:113` — same pattern.
- [ ] `std/concurrency/http.nv:36,54`, `oxsar_port.nv:51`, `orm_demo.nv:663,691`, `audit.nv:307` — `interrupt to_http_error(e)` → `throw to_http_error(e)`. Эта transformation требует, чтобы `to_http_error` accepted типизированный `e: SourceErr`, не `nova_str`.
- [ ] **NOT migrating:** `interrupt Some(e)` / `interrupt None` cases — это другой паттерн (early-return через handler, не error-rewrite). Оставляем как есть.
- [ ] Add `docs/idioms/error-handling.md` (new) — canonical patterns: typed throw, catch-all, cross-effect rewrite, `expr!!`, when to use each.

### Ф.8 — Performance + cross-toolchain (½ day)

- [ ] `nova bench` (Plan 57) для typed throw vs erased throw vs untyped baseline — overhead должен быть ≤ 5% vs baseline (vtable dispatch + 1 conditional).
- [ ] Code-size: per-E specialization add'ит ~50 LOC C per unique E. Worst case в std/* — ~10 уникальных E типов → 500 LOC. Acceptable.
- [ ] Cross-toolchain (Plan 58): PASS на Clang/MSVC/GCC.
- [ ] Memory: per-E TLS slot — 1 pointer per E. Если в проекте 50 уникальных E типов — 400 bytes TLS. Acceptable.

**Total: 7-8 dev-days** (v1 заявлял 5-8 без учёта Silent UB #2, без stdlib migration, без TypeId infra, без cross-effect throw).

---

## Acceptance criteria (production-grade)

### Корректность — закрытие Silent UB

- [x] `throw UserError { ... }` для `Fail[UserError]` — handler видит `e: UserError` с правильными полями. **Silent UB #1 закрыт.**
- [x] `result!!` на `Err(custom_payload)` — outer handler ловит `custom_payload`, не строку «Result::Err». **Silent UB #2 закрыт.**
- [x] Handler arm `|e| println(e.detail)` для `Fail[E]` — `e` имеет правильный C-тип, `e.detail` компилируется и читает правильное поле. **Silent UB #3 закрыт.**
- [x] `nova_throw_value` placeholder удалён из runtime.

### D65 spec compliance

- [x] Правило 1: `Fail` ≡ `Fail[any]` — alias через erased path. PASS test `plan61/fail_alias_any.nv`.
- [x] Правило 2: lookup precedence (точный E → any → unwind). PASS test `plan61/lookup_precedence.nv` (3 sub-cases).
- [x] Правило 3: re-throw skips current frame. PASS existing Plan 20 Ф.8 tests + new `plan61/rethrow_per_e.nv`.
- [x] Правило 4: handler-body не resumes (Fail-strict). PASS existing tests.
- [x] Правило 5: nested handlers — каждый own slot. PASS new `plan61/nested_handlers_per_e.nv`.

### Plan 11 / Plan 49 unblock

- [x] Plan 11 open issue (cross-effect throw в handler arm) — закрыт. `plan61/cross_effect_throw.nv` PASS, repro `/tmp/test_handler_rethrow.nv` (если ещё существует) — PASS.
- [x] Plan 49 «typed Fail-канал — future work» — закрыто. CancelToken[T] и Fail[E] оба используют тот же шаблон.

### Stdlib regression

- [x] Все 15+ workaround мест либо переписаны на idiomatic form (Ф.7), либо явно сохранены с comment'ом «kept on interrupt-pattern — non-error early-return semantic».
- [x] `nova test` — 0 regressions vs baseline. `nova check std/` — clean.

### Performance

- [x] Throw typed `Fail[E]` overhead vs baseline (string-only) ≤ 5%. `nova bench plan61/throw_micro.nv`.
- [x] Code-size growth ≤ 1% от total emit'тся stdlib binary.
- [x] TLS slot memory ≤ 64 bytes per unique E in workspace.

### Cross-toolchain (Plan 58)

- [x] PASS на Clang (default), MSVC, GCC.

### Diagnostic quality

- [x] Type mismatch в `throw` — diagnostic с expected/actual + suggestion.
- [x] Type mismatch в handler arm — diagnostic с expected `|e: E| ...` shape.

---

## Open questions

1. **`Fail[any]` syntax: `Fail` vs `Fail[any]`** — оба легальны по D65. **Decision:** оба эмитятся в erased path identically; никакой semantic difference. Lint warning только для `Fail` без `[E]` в public API (D65 convention table).

2. **Boxing для small typed errors** — `Fail[bool]` или `Fail[int]` пройдёт через `(int*)` pointer-passing → требует stack-alloc. **Decision:** для типов sizeof ≤ pointer — pass-by-value через `union { int as_int; void* as_ptr }`. Codegen знает тип, выбирает правильный путь. Zero-cost.

3. **`From`-based cross-type cascade** (Plan 49 P1 Ф.6) — нужен ли для Fail? `with Fail[A] = |a| throw B.from(a)` — это **user-explicit conversion**, не runtime-magic. **Decision:** не вводить runtime auto-convert в Plan 61. User пишет explicit `B.from(a)` — это idiomatic D73/D77. Если в будущем понадобится auto-cascade (как Rust `?` с `From`), это **Plan 61.A followup**, не блокер.

4. **`Fail[never]`** — функция helps типизировать unreachable. **Decision:** N/A для bootstrap; `Never` per-E specialization тривиален (vtable никогда не invoke'нется), no special-case.

5. **Поддержка polymorphic `throw` в generic-функции**: `fn raise[E](e E) Fail[E] -> Never => throw e` — требует, чтобы Plan 48 worklist знал, что E в этой fn используется как Fail-parameter. **Decision:** yes, worklist расширяется на «Fail-touched E» как ещё один источник concrete instantiation. Test: `plan61/generic_raise.nv`.

6. **Interaction с Plan 49 CANCEL channel**: Plan 49 уже изолировал CANCEL от USER. **Decision:** Plan 61 не трогает CANCEL. `throw e` всегда идёт в USER channel; `tok.cancel(reason)` — в CANCEL. Полная орthogональность. Verified via existing Plan 49 tests + new `plan61/cancel_vs_throw_isolated.nv`.

7. **D85 `expr!!` через `Err(e)`: какой E?** — `Result[T, E]!!` бросает `Fail[E]`. **Decision:** да, это natural. Codegen знает E из Result's type-args. Test: `plan61/result_bang_typed_err.nv`.

---

## Связь с другими планами

- **[Plan 11](11-method-values-and-overload.md)** — закрыт. Plan 61 закрывает «Open issue: cross-effect throw в handler arm (discovered 2026-05-17)» (lines 791-814 in Plan 11).
- **[Plan 20](20-defer-implementation.md)** — D65 правило 3 re-throw уже частично реализовано через `prev` link в legacy `NovaVtable_Fail`. Plan 61 расширяет mechanism на per-E + erased slots.
- **[Plan 39](39-range-stdlib-fixes.md)** — Issue A `NovaInterruptFrame.value_ptr` для типизированного interrupt уже сделан. Plan 61 решает аналог для Fail-throw.
- **[Plan 45.B](45-nova-doc.md)** — после Plan 61 stdlib doc-comments переписываются на idiomatic typed throw. `nova doc` extractor должен рекламировать typed form.
- **[Plan 48](48-closures-in-generics.md)** — **жёсткий blocker для Ф.3**. Per-E specialization использует Plan 48 worklist. Ф.0/Ф.1/Ф.2 независимы (TypeId infra + erased path).
- **[Plan 49](49-cancel-throw-routing.md)** — **direct precedent**: `cancel_token_t_map` → `fail_e_map` pattern; per-T runtime helper generation → per-E. Plan 61 закрывает Plan 49 line 543-545 «typed Fail-канал — future work».
- **[Plan 57](57-perf-benchmark-infrastructure.md)** — Ф.8 perf gate.
- **[Plan 58](58-cross-toolchain-msvc-verification.md)** — Ф.8 cross-toolchain gate.

---

## Сравнение с state-of-the-art

| Language | Throw mechanism | Typed errors | Catch-all | Re-throw |
|---|---|---|---|---|
| **Rust** | `Result<T, E>` + `?` | per-E mono, zero-cost | `Box<dyn Error>` (erased) | `?` propagates, manual conversion via `From` |
| **Swift 6.0** | `func f() throws(E)` | typed throws (per-E) | `throws` без `(E)` (= `any Error`) | `try` propagates |
| **Koka** | `effect ctl raise<a>(e: a) : b` | typed effect rows | `effect amb` (= any) | re-raise через `resume` |
| **Effekt** | typed effects + capabilities | typed | wildcard handler | controlled re-raise |
| **Go** | `error` interface | **none** (single untyped) | implicit (everything is `error`) | `return err` |
| **Java** | checked exceptions `throws E1, E2` | listed types, не generic | `throws Throwable` | `throw e` |
| **TypeScript** | `throw any` | **none** | implicit | `throw err` |
| **Nova до Plan 61** | `Fail[E]` declared, hardcoded на str | **silently broken** | `Fail` works но через бажный путь | works через workaround |
| **Nova после Plan 61** | `Fail[E]` per-E mono + `Fail[any]` erased | **production-grade typed, zero-cost** | `Fail` ≡ `Fail[any]` erased path с RTTI | per-slot prev-link, D65 правило 3 |

**Nova после Plan 61:**
- **=** Rust (`Result<T,E>` + `Box<dyn Error>` parity, no syntactic noise of `?` because Nova uses effect rows instead).
- **=** Swift 6 typed throws.
- **=** Koka (Nova's `Fail[E]` ≡ Koka `effect raise<E>`).
- **>** Go (Go нет typed errors вообще).
- **>** Java (Java checked exceptions не generic — нельзя `throws T`).
- **>** TypeScript (TS нет typed throws).

**Уникальное преимущество Nova:** hybrid в одном языке, без syntactic split (Rust split: `Result` vs `panic!`; Swift split: `throws(E)` vs `throws`; Koka split: effect rows). Nova: всегда `Fail[E]` или `Fail`, единая семантика, разный backend.

---

## Ссылки

- [compiler-codegen/nova_rt/effects.h:38-145](../../compiler-codegen/nova_rt/effects.h#L38) — runtime Fail + готовая typed-cancel infra (Plan 49).
- [compiler-codegen/nova_rt/effects.h:140-150](../../compiler-codegen/nova_rt/effects.h#L140) — `nova_throw_value` placeholder (будет удалён).
- [compiler-codegen/nova_rt/effects.h:405-442](../../compiler-codegen/nova_rt/effects.h#L405) — `NovaVtable_Fail` hardcoded на str (будет расширено).
- [compiler-codegen/src/codegen/emit_c.rs:8503-8517](../../compiler-codegen/src/codegen/emit_c.rs#L8503) — throw lowering silent type-pun (Silent UB #1).
- [compiler-codegen/src/codegen/emit_c.rs:2480-2522](../../compiler-codegen/src/codegen/emit_c.rs#L2480) — with-binding handler install (D65 prev-link).
- [compiler-codegen/src/codegen/emit_c.rs:828-832](../../compiler-codegen/src/codegen/emit_c.rs#L828) — Fail schema registration (hardcoded `nova_str`).
- [docs/plans/11-method-values-and-overload.md:791-814](11-method-values-and-overload.md#L791) — origin open-issue для cross-effect throw.
- [docs/plans/48-closures-in-generics.md](48-closures-in-generics.md) — mono worklist reuse.
- [docs/plans/49-cancel-throw-routing.md:467-527](49-cancel-throw-routing.md#L467) — direct precedent: per-T mono pattern.
- [docs/plans/49-cancel-throw-routing.md:543-545](49-cancel-throw-routing.md#L543) — explicit «typed Fail-канал — future work» (= Plan 61).
- [spec/decisions/04-effects.md#d65](../../spec/decisions/04-effects.md#d65) — Fail full semantics (rules 1-5).
- [spec/decisions/04-effects.md#d25](../../spec/decisions/04-effects.md#d25) — Fail effect origin.
- [spec/decisions/04-effects.md#d85](../../spec/decisions/04-effects.md#d85) — `expr!!` semantics.
- [spec/decisions/04-effects.md#d75](../../spec/decisions/04-effects.md#d75) — Plan 49 cancel typing (mirror).
- [spec/decisions/08-runtime.md#d26](../../spec/decisions/08-runtime.md#d26) — `RuntimeError` / `Error` / `RuntimeNoneError` prelude types.
- [spec/decisions/03-syntax.md#d54](../../spec/decisions/03-syntax.md#d54) — `any` top-type via empty protocol.
