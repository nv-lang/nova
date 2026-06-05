// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 128 — `mut @method` receiver ABI: `recv.mutable` wiring + D215 NamedTuple pointer + `E_PRIMITIVE_MUT_METHOD`

> **Создан 2026-06-05.** Closure-doc: spec/codegen/diagnostic пакет
> для mut-receiver ABI gaps, обнаруженных в Plan 123 V7.6 V2 audit.
> **Status:** ✅ **CLOSED 2026-06-05.**
> **Trigger:** Plan 123 followups (`docs/plans/123-followups-2026-06-05.md`)
> spawned 3 markers — `[M-codegen-recv-mutable-flag-unwired]`,
> `[M-D215-mut-receiver-pointer-codegen]`, `[M-D26-primitive-mut-method-diag]`.
> **Branch:** `plan-128-mut-receiver-abi`.
> **Worktree:** `D:/Sources/nv-lang/nova-p128`.
> **Model:** Opus 4.7 + Thinking ON (codegen/ABI semantics).

---

## 1. Контекст

### 1.1 Что обнаружил Plan 123 V7.6 V2 audit

Plan 123 (chain-aware caching) V7.6 V2 refactor (2026-06-05) — field-cache
classifier `is_reference_type_ref` стал консультироваться с `AllocKind`
через `TypeKindRegistry`. В процессе работы выяснилось, что:

1. **`MethodCallInfo::recv.mutable`** существует в IR, но **не доходит**
   до `emit_c.rs` helpers (`emit_method_call_signature`, call-site
   emission). Result: type-checker знает, что receiver — `mut @`, codegen
   эту информацию теряет.

2. **D215 NamedTuple `mut @method`** — codegen бы получал `NovaTuple_X`
   by-value, мутация локальна, caller'у не видна. Spec D215 не explicit
   на этот счёт — только D32 «mutate-by-copy» implicitly применим, но
   не желателен для NamedTuple (zero-cost abstraction → unobservable
   mutation = silent bug).

3. **Primitives accept `fn int mut @method(...)`** — parser проглатывает,
   codegen генерирует by-value mutation в копию, programmer думает что
   мутирует receiver. Silent error, Nova-first idiom violation
   (примитивы должны pure-functionally return new value).

### 1.2 Что нужно

| Gap | Fix | Phase |
|---|---|---|
| `recv.mutable` flag не threaded в emit_c.rs | Wire через `prepare_method_recv` + `emit_method_call_signature` | Ф.1 |
| NamedTuple `mut @` → `NovaTuple_X*` | Codegen branch: emit `&v` для identifier, hoist+`&temp` для rvalue | Ф.2 |
| Primitives `mut @` accepted | Type-checker reject с `E_PRIMITIVE_MUT_METHOD` | Ф.3 |
| Regression coverage | 15 fixtures (NamedTuple mut + value-record mut + primitives neg) | Ф.4 |
| Spec amend + plan-doc + logs + merge | D215 §«Method receiver passing» + D228 cross-ref + D26 amend + D32 table column | Ф.5 |

---

## 2. Фазы

### Ф.1 — Wire `recv.mutable` через `emit_c.rs` helpers ✅

**Commit:** `2415644d24e feat(plan128 Ф.1): wire recv.mutable through emit_c.rs codegen helpers`

- `MethodCallInfo::recv.mutable` consumed в `prepare_method_recv` —
  identifier shortcut берёт `&local`, rvalue path hoist'ит в temp.
- `emit_method_call_signature` пробрасывает receiver pointer-ness в C
  parameter type emission (`NovaTuple_X*` vs `NovaTuple_X` by-value).
- Helper `is_value_type` распознаёт `NovaTuple_` / `NovaValue_` prefix
  для уже существующего value-record D228 path — Plan 128 экстендит
  такую же логику для NamedTuple.

### Ф.2 — D215 NamedTuple mut-receiver pointer ABI ✅

**Commit:** `50257797587 feat(plan128 Ф.2): D215 NamedTuple mut-receiver pointer ABI (NovaTuple_X*)`

- NamedTuple `mut @method` параметр emit'ится как `NovaTuple_<X>*`.
- Call-site identifier path: `f(&v)` (требует `mut` binding — D33 + D215
  amend §«Binding-level mutability» enforced via type-checker).
- Call-site rvalue path: hoist в `NovaTuple_<X> __tmp_recv_<id> = expr;`
  + pass `&__tmp_recv_<id>`. Мутации видны только в expression chain
  (D32 «mutate-by-copy для rvalue» semantics).
- Symmetric с D228 value-record `NovaValue_X*` pattern.

### Ф.3 — `E_PRIMITIVE_MUT_METHOD` diagnostic ✅

**Commit:** `8b178873461 feat(plan128 Ф.3): E_PRIMITIVE_MUT_METHOD diagnostic rejects fn primitive mut @method`

- Type-checker `TypeCheckCtx::build` проверяет: если receiver type ∈
  `{int, i8-i64, u8-u64, f32, f64, bool, char, str, ()}` и method
  declared `mut @` → emit `E_PRIMITIVE_MUT_METHOD`.
- Diagnostic message: «primitive types pass by value — `mut @method`
  has no observable effect. Remove `mut` and return new value».
- Ro `@method` на примитивах остаётся allowed (positive regression
  fixture t10_primitive_ro_method_ok).

### Ф.4 — 15 regression fixtures ✅

**Commit:** `30f57d3a797 test(plan128 Ф.4): 15 regression fixtures для mut-receiver ABI`

| # | Fixture | Тип |
|---|---|---|
| t1 | `named_tuple_mut_observed_ok` | POS — mut @ на NamedTuple, мутация видна caller |
| t2 | `named_tuple_ro_unchanged_ok` | POS — ro @ → by-value, мутация не видна (regression) |
| t3 | `named_tuple_chained_mut_ok` | POS — `v.method1().method2()` chain |
| t4 | `value_record_mut_regression_ok` | POS — D228 path не сломан (symmetric pattern) |
| t5 | `value_record_mut_field_ok` | POS — value-record field mutation |
| t6 | `primitive_str_mut_method_neg` | NEG — E_PRIMITIVE_MUT_METHOD на `fn str mut @` |
| t7 | `primitive_int_mut_method_neg` | NEG — E_PRIMITIVE_MUT_METHOD на `fn int mut @` |
| t8 | `primitive_bool_mut_method_neg` | NEG — E_PRIMITIVE_MUT_METHOD на `fn bool mut @` |
| t9 | `primitive_f64_mut_method_neg` | NEG — E_PRIMITIVE_MUT_METHOD на `fn f64 mut @` |
| t10 | `primitive_ro_method_ok` | POS — ro `@` на примитивах разрешён |
| t11 | `mut_method_user_type_ok` | POS — heap record mut @ (sanity, не должен сломаться) |
| t12 | `named_tuple_field_mut_ok` | POS — mutate `.field` через mut @ method |
| t13 | `named_tuple_in_array_ok` | POS — `[]NamedTuple` element mutation pattern |
| t14 | `named_tuple_in_record_ok` | POS — heap-record { named_tuple_field } mutation |
| t15 | `value_record_in_named_tuple_ok` | POS — nested D228+D215 composition |

### Ф.5 — Spec amends + plan-doc + logs + commit + merge + push (this commit) ✅

- **Spec amend `spec/decisions/02-types.md`:**
  - D215 §«Method receiver passing» — `NovaTuple_X*` ABI, identifier/rvalue
    call-site rules, `recv.mutable` flag wiring note.
  - D228 cross-ref — pointing к D215 amend для symmetric pattern.
  - D32 таксономия table — new «Receiver mut-ABI» column.
- **Spec amend `spec/decisions/08-runtime.md`:**
  - D26 §«Plan 128 Ф.3 amend» — `E_PRIMITIVE_MUT_METHOD` rationale +
    enforcement + coverage.
- **Plan-doc:** этот файл.
- **Plans README:** new Plan 128 row.
- **Logs:** project-creation.txt + simplifications.md + nova-private discussion-log.md.
- **Commits:** atomic split — docs+spec, log, nova-private log.
- **Merge:** `git merge --no-ff plan-128-mut-receiver-abi` в main.
- **Push:** branch + main + nova-private.

---

## 3. Acceptance criteria (A128.x)

- **A128.1 ✅** `MethodCallInfo::recv.mutable` reaches `prepare_method_recv`
  и `emit_method_call_signature` — verified Ф.1 commit.
- **A128.2 ✅** NamedTuple `mut @method` parameter emit'ится как
  `NovaTuple_<X>*` (Ф.2 fixtures t1, t3, t12-t15).
- **A128.3 ✅** Identifier call-site `f(v)` для mut @ → `f(&v)` (Ф.2 t1).
- **A128.4 ✅** Rvalue call-site hoist'ится в temp + `&temp` (Ф.2 t3 chained).
- **A128.5 ✅** Ro `@method` на NamedTuple → by-value (no pointer) — backward-compat (Ф.4 t2).
- **A128.6 ✅** D228 value-record path не сломан (Ф.4 t4, t5, t15).
- **A128.7 ✅** `fn <primitive> mut @method` → `E_PRIMITIVE_MUT_METHOD` (Ф.3 t6-t9).
- **A128.8 ✅** Ro `@method` на примитивах allowed (Ф.4 t10).
- **A128.9 ✅** Heap-record `mut @method` не затронут (Ф.4 t11).
- **A128.10 ✅** 15/15 plan128 fixtures PASS + lib tests clean (release build).

---

## 4. Markers closed

- 🟢 **`[M-codegen-recv-mutable-flag-unwired]`** — wired in Ф.1.
- 🟢 **`[M-D215-mut-receiver-pointer-codegen]`** — codegen branch landed в Ф.2.
- 🟢 **`[M-D26-primitive-mut-method-diag]`** — diagnostic landed в Ф.3.

Все три из Plan 123 V7.6 V2 followup spawn (`docs/plans/123-followups-2026-06-05.md`).

---

## 5. Verification

**Lib tests** (compiler-codegen): clean (zero regressions, smoke run в Ф.5 post-merge).

**Runtime fixtures** (`nova_tests/plan128/`): 15/15 PASS на release nova-cli
(rebuild required после merge — Plan 91.14 stale-binary lesson applied).

**Pre-existing test suites** не регрессируют — sanity'ные plan99/plan100_2/
plan100_6/plan108 FAIL counts идентичны main (отдельные known issues
непересекающихся sub-systems).

---

## 6. Спец-блоки

- **D215 amend** (Plan 128 Ф.2) — §«Method receiver passing» в `02-types.md`.
- **D228 cross-ref** (Plan 128 Ф.5) — параллельный pointer pattern note в `02-types.md`.
- **D26 amend** (Plan 128 Ф.3) — §«primitives reject `mut @method`» в `08-runtime.md`.
- **D32 amend** (Plan 128 Ф.5) — «Receiver mut-ABI» column в таксономии table в `02-types.md`.

Никаких новых D-блоков не вводится — все три fix'а — амендменты существующих.

---

## 7. Связь

- Plan 123 V7.6 V2 — `docs/plans/123-followups-2026-06-05.md` (spawn parent).
- Plan 120 — `docs/plans/120-named-tuples-and-allocation-contract.md` (D215 base).
- Plan 124.8 — `docs/plans/124.8-tuple-value-refine.md` (D228 base).
- Plan 91 §«Принцип: Nova-first» — обоснование `E_PRIMITIVE_MUT_METHOD`.

---

## 8. Lessons

1. **`MethodCallInfo` flag flow** — IR-уровневая информация (`recv.mutable`)
   легко теряется в emit_c.rs helpers, если signature не accept'ит её
   explicitly. Pattern: добавлять flag в helper signature, не в TLS.

2. **ABI symmetry pays** — D215 NamedTuple `NovaTuple_X*` mut-receiver
   wired через ту же `prepare_method_recv` helper, что и D228
   `NovaValue_X*`. Single path = single point of truth для escape /
   alignment / GC tracking.

3. **`E_PRIMITIVE_MUT_METHOD` early-rejection** > silent silent miscodegen.
   Better to catch at type-check (with suggestion) than emit valid but
   confusing C code.
