// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 70: Strict type propagation в codegen — no silent `nova_int` fallback

> **Создан 2026-05-18.** **Driver:** реальная боль пользователя — «куча
> ошибок из-за того, что всё неизвестное = int». Plan 67 закрыл одну
> точку (println overload), audit показал **155 аналогичных fallback
> sites** в codegen, из них ~120 потенциально дают silent miscompilation.
>
> **Приоритет:** **P0** (silent miscompilation = критика). Один
> неаннотированный generic-return → пользователь получает garbage в
> output без warning'а — невозможно отладить.
> **Трудоёмкость:** ~10-15 dev-days. Самый крупный hardening-план
> после Plan 65/62.

---

## Зачем

### Проблема (concrete user pain)

Codegen `emit_c.rs` (15+ тыс. строк) **155 раз** использует паттерн:

```rust
self.type_ref_to_c(&p.ty).unwrap_or_else(|_| "nova_int".into())
//                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
//                       Silent fallback: type translation failed →
//                       подставляем nova_int и идём дальше.
```

Если `type_ref_to_c` не смог разрешить тип (generic не mono'д, alias не
expanded, или просто bug в inference) — **молчаливо** эмитим C-код где
этот тип = `long long`. Результат:

- Pointer cast to int → garbage address как число
- Bool/char напечатается как code-point (Plan 67 как раз это закрыл)
- Record/Sum-type memcpy с неправильным sizeof
- Float → int truncation

**Симптомы:** wrong runtime output, иногда segfault, иногда **silent
miscompilation** — программа «работает», но возвращает мусор. Debug
невозможен — компилятор не сигналит ничего.

### Историческая cleanup-работа

Раньше много fallback'ов уже исправлены в plan-by-plan basis:
- Plan 48 (mono pass) — перевёл много generics с erasure на mono'д типы
- Plan 56 (vtable dispatch) — закрыл erased-method-call
- Plan 59 (tuple mono) — `_NovaTupleN.f*` slots → typed
- Plan 65 (timer API) — bonus fix `Nova_X* + Nova_X*` prefix bug
- Plan 67 (println overload) — последний для известного класса

**Но 155 sites осталось.** Не известно сколько из них дают реальные баги
сейчас. User знает что **«ещё много»** — empirically обнаруживается.

### Industry baseline

| Язык | Поведение при unknown type в codegen |
|---|---|
| **Rust** | Compile error на любой `unwrap()` over type-inference failure. Generic'и monomorphize'нутся до codegen — unresolved = error |
| **Swift** | Compile error «type of expression is ambiguous without more context» |
| **Go** | No generic erasure до Go 1.18; sinсе then — monomorphization mandatory, generic body type-checked при instantiation |
| **Kotlin/JVM** | Erased generics, но всё равно type через JVM bytecode signature — runtime ClassCastException **громкая** |
| **TypeScript** | Type erasure — но `any` явно opt-in, не silent default |
| **Nova (текущее)** | Silent `nova_int` fallback — **хуже всех baseline** |

Nova **должен** быть на уровне Rust/Swift — type-erased silent default
непозволителен. Это **regression vs language goals** (AI-first, type
safety, predictable codegen).

---

## Аудит результат (verified 2026-05-18)

### Категории fallback'ов

| Категория | Pattern | Количество (≈) | Семантика | Действие |
|---|---|---|---|---|
| **A. Unknown-fallback** | `type_ref_to_c(...).unwrap_or_else(\|_\| "nova_int")` | ~120 | Type translation failed — silent default | **Заменить на error** |
| **B. Explicit erasure** | `_ => "nova_int", // erased T` | ~20 | Generic до mono — intentionally erased | Document + assert + audit |
| **C. Categorical mapping** | `WithResultCategory::IntLike => "nova_int"` | ~10 | Mapping для int-family categories | OK, legit |
| **D. Int-family alias** | `"int"\|"i8"\|... => "nova_int"` | ~5 | Type alias normalization | OK, legit |

**Только Category A** — silent miscompilation. Остальные либо
documented intentional, либо корректные mappings.

### Распределение Category A по функциям

| Функция (line range) | Sites | Что translateит |
|---|---|---|
| `emit_fn_decl_with_return_type` | 4 | params + return types |
| `emit_handler_lit` | 6 | handler arms + ret type |
| `emit_call` (dispatch) | 25+ | call arg types для overload selection |
| `infer_expr_c_type` | 18+ | recursive type inference (worst — silent cascade) |
| `emit_record_lit` | 8 | field types |
| `emit_method_dispatch` | 12 | method arg + return |
| `emit_for_loop` | 6 | iterator element types |
| `emit_match` | 9 | scrutinee + arm types |
| `monomorphize_generic_fn` | 14 | substituted T → C-type translation |
| `emit_array_lit` / `emit_tuple_lit` | 8 | element types |
| ... остальные ~15 LOC chunks | ~10 | misc fallback paths |

---

## Архитектурные решения

### AD1. Replace `unwrap_or "nova_int"` → `Result` propagation

Категория A patterns менять механически:

```rust
// Before (silent miscompilation):
let ty = self.type_ref_to_c(&p.ty).unwrap_or_else(|_| "nova_int".into());

// After (strict — propagate error):
let ty = self.type_ref_to_c(&p.ty).map_err(|e| format!(
    "cannot infer C type for parameter `{}` at {}:{} — \
     {}. Add explicit type annotation or ensure generic is monomorphized.",
    p.name, p.span.file_id, p.span.start, e
))?;
```

Function signature changes: вместо `String` (ret) → `Result<String, String>`.
Propagation вверх по call chain. **Большой refactor**, но механический.

### AD2. Category B — explicit assert + audit

Места где `_ => "nova_int", // erased T` — **intentional** (pre-mono
generic body emit). Должны:
- Иметь **inline comment** объясняющий почему erasure безопасна (mono
  pass потом подставит, или body не использует тип конкретно)
- Иметь **debug-assert** что caller — generic-fn body context, не
  user-facing concrete code
- Записаться в audit-document `docs/codegen-erasure-sites.md` со списком
  всех legitimate erasure points

### AD3. Lint правило — internal compiler dev guard

Добавить custom `clippy.toml` rule (или separate Rust binary lint) что
`unwrap_or_else(_, "nova_int")` literal — **forbidden** в новом коде.
CI gate.

Цель: предотвратить новые fallback'ы. Existing — миграция.

### AD4. Phased migration — категория за категорией, не all-at-once

155 sites = много. Делать строго по одной категории за раз:

1. **Phase A1 — `infer_expr_c_type` 18+ sites** (worst-impact: cascades down)
2. **Phase A2 — `emit_call` 25+ sites** (high call-site density)
3. **Phase A3 — `monomorphize_generic_fn` 14 sites** (core mono path)
4. **Phase A4 — остальные ~63 sites** (per-function basis)

После каждой фазы — **полный `nova test`**, fix всех broken sites,
verify 0 регрессий vs предыдущей фазы.

### AD5. Migration strategy для broken sites

Когда unwrap → error меняется, broken sites упадут при compile:

| Type unresolved причина | Fix |
|---|---|
| Generic не mono'д | Force mono pass (Plan 48 reuse) |
| Type alias не expanded | Resolve alias до codegen |
| Missing annotation у user | Compile error с suggestion |
| Cascade от другого error | Fix root cause |

Per-site decision — может быть user-fix (нужна аннотация в .nv) или
internal-fix (compiler bug — unresolved alias / mono).

### AD6. Test coverage — per-Category negative tests

Каждая phase добавляет negative-фикстуры:
- `EXPECT_COMPILE_ERROR` для каждого паттерна где раньше silent fallback
- Сообщение об ошибке (substring match) с указанием call site

Existing positive tests должны pass без regression — это **acceptance
gate** per phase.

### AD7. Diagnostic infra reuse — D75/E5101 pattern

Использовать существующий structured Diagnostic format (Plan 36 R7,
Plan 65 E5101) для всех новых errors:

```
error[E7001]: cannot infer C type for expression
  --> file.nv:42:13
   |
42 |     let x = generic_fn(y)
   |             ^^^^^^^^^^^^^ type `T` not monomorphized at this point
   |
   = note: `generic_fn[T]` requires explicit type argument
   = note: this fallback would silently default to `nova_int` —
           output would be incorrect for non-int types
help: specify type argument
   |
42 |     let x = generic_fn[int](y)
   |                       ^^^^^
```

Error codes E7001-E7099 reserved для Plan 70.

---

## Requirements (R1-R20)

### Core strictness

**R1.** Category A sites (`unwrap_or "nova_int"`) → strict error
propagation. 0 silent fallbacks в category A после Plan 70 closure.

**R2.** Category B sites (intentional erasure) — documented audit в
`docs/codegen-erasure-sites.md`. Each site:
- Inline comment с rationale
- `debug_assert!` что context legitimate (generic body, не concrete)
- Listed в audit doc с file:line + reason

**R3.** Category C/D — оставить, verify в tests что mapping корректен.

**R4.** Лекsemantic invariant: **`nova_int` в emitted C-code только если
тип фактически int-family** (int/u8/i8/.../i64/u64/char→via nova_char/...).
Никаких pointer/struct/union cast'ов к `nova_int`.

### Diagnostic UX

**R5.** Каждый бывший silent-fallback site → structured diagnostic
E7001-E7099 (Plan 36 R7 format):
- File + line + column
- Variable name / expression context
- Rationale «why this is now error»
- Machine-applicable fix suggestion (annotation / mono hint / type cast)

**R6.** Diagnostic group code `E70xx` reserved. Cross-ref в Plan 36
diagnostic table.

### Migration

**R7.** Per-phase migration: `infer_expr_c_type` → `emit_call` →
`monomorphize_generic_fn` → остальные. **0 regressions** между фазами.

**R8.** Broken user code (если найдено) → migration tool с auto-fix
для thumb cases (`let x = f(y)` без аннотации → `let x int = f(y)` если
infer возвращает int).

**R9.** Migration commits per-phase, atomic. Pattern `feat(70 PhaseA1):
strict infer_expr_c_type — N sites migrated`.

### Lint

**R10.** Custom Rust lint (или `clippy.toml` rule) — запретить новые
`unwrap_or "nova_int"` patterns в codegen source. CI fails на violation.

**R11.** `cargo check` для compiler-codegen — clean (warnings allowed
существующие, новые — no).

### Tests

**R12.** Negative fixtures `nova_tests/plan70/`:
- f1-f30 для Category A — каждый pattern даёт E7001-E7030 при попытке
  compile.
- f31-f40 для Category B — verify intentional erasure всё ещё работает
  (positive tests что generic body emit'ит корректно).

**R13.** Existing `nova test` (release) — **0 regressions** после full
Plan 70 closure.

**R14.** Cross-toolchain (Plan 58 когда будет) — strict mode на all
platforms.

### Audit / docs

**R15.** `docs/codegen-erasure-sites.md` — full list Category B sites.

**R16.** Spec new D-block: D126 «Strict type propagation в codegen».

**R17.** Plan-doc closure with per-phase commit list.

### Performance

**R18.** Per-phase bench (Plan 57) — compile time не должен расти > 5%
от baseline. Strict mode = больше Result-propagation, но `Result<String,
String>` zero-cost при success.

**R19.** Bench `compile_corpus_full` — gate.

### Backwards compatibility

**R20.** **Plan 70 — breaking change для user code** который полагался
на silent int default. Bootstrap convention: clean break с machine-
applicable migration suggestions. Document в release notes (когда
будут).

---

## Phases (10 phases, ~10-15 dev-days)

### Ф.0 — Audit baseline (1 day)

- [ ] Inventory всех 155 sites — script `nova-cli/src/bin/audit_nova_int_fallbacks.rs`
- [ ] Категоризация per-site (A/B/C/D) с rationale
- [ ] Output: `docs/plans/70-artifacts/audit-2026-05-XX.md`
- [ ] baseline `nova test` PASS count

### Ф.1 — Diagnostic infra E7001-E7099 (½ day)

- [ ] Reserve error codes E7001-E7099 в diagnostic registry
- [ ] Helper `Self::err_no_int_fallback(span, expr_name, reason) -> String`
      для unified error format
- [ ] Test: dummy site → produces E7001 formatted correctly

### Ф.2 — Internal lint guard (½ day)

- [ ] Custom Rust lint (или regex CI check) — forbidden new
      `unwrap_or "nova_int"` patterns
- [ ] CI gate (Plan 58 если будет, иначе shell script)
- [ ] Verify trips on dummy test PR

### PhaseA1 — `infer_expr_c_type` (~18 sites, 2 days)

- [ ] Convert signature `fn infer_expr_c_type(&self, e: &Expr) -> String`
      → `Result<String, String>`
- [ ] Migrate каждый `unwrap_or "nova_int"` → `err_no_int_fallback`
- [ ] **Propagate `?`** в caller chain (большой refactor, могут падать
      сотни call sites — fix one-by-one)
- [ ] `nova test` после каждого batch — 0 regressions
- [ ] Negative fixtures `nova_tests/plan70/f1_infer_*_neg.nv` (5-8 tests)

### PhaseA2 — `emit_call` (~25 sites, 2 days)

- [ ] Same pattern для emit_call dispatch
- [ ] Migrate per overload resolution path
- [ ] Negative tests f9-f15

### PhaseA3 — `monomorphize_generic_fn` (~14 sites, 1.5 days)

- [ ] Strict mode в mono pass
- [ ] User-facing migration cases — auto-fix tool?
- [ ] Negative tests f16-f20

### PhaseA4 — остальные ~63 sites (3 days)

- [ ] Per-function migration (emit_handler_lit, emit_record_lit,
      emit_for_loop, emit_match, ...)
- [ ] Negative tests f21-f30

### Ф.3 — Category B audit + docs (1 day)

- [ ] Document каждый legitimate erasure site в
      `docs/codegen-erasure-sites.md`
- [ ] Add `debug_assert!` на context legitimacy
- [ ] Positive tests f31-f40 что generic body emit OK

### Ф.4 — Spec + project docs (½ day)

- [ ] Spec D126 «Strict type propagation»
- [ ] Update `docs/simplifications.md` — RESOLVE markers
- [ ] Update `docs/project-creation.txt`
- [ ] Update `docs/plans/README.md` Plan 70 row

### Ф.5 — Final audit + perf gate (½ day)

- [ ] Re-run audit script — должно показать 0 Category A sites
- [ ] `nova test` — 0 regressions vs Ф.0 baseline
- [ ] Bench `compile_corpus_full` — ≤ 5% delta
- [ ] Cross-toolchain (если Plan 58 ready) — full PASS
- [ ] 25-point final audit checklist (Plan 60/65 pattern)

---

## Acceptance criteria (production-grade)

- [ ] `grep -rE 'unwrap_or.*"nova_int"|or_else.*"nova_int"' compiler-codegen/src/` → **0 matches** (категория A)
- [ ] `docs/codegen-erasure-sites.md` — full list Category B (~20 sites) с rationale
- [ ] Все existing `nova test` PASS (0 regressions) после full Plan 70
- [ ] 30+ negative fixtures `nova_tests/plan70/f1-f30` PASS (each catches one Category A site)
- [ ] Internal lint trips on `unwrap_or "nova_int"` в new PRs
- [ ] Diagnostic E7001-E7099 used для всех strict errors
- [ ] Spec D126 published
- [ ] Bench compile-time ≤ 5% regression
- [ ] Plan 36 R7 structured diagnostic format используется
- [ ] Cross-toolchain (если Plan 58) — full matrix PASS

---

## Risks

| Risk | Mitigation |
|---|---|
| **Cascade refactor** — `infer_expr_c_type` ret type change ломает сотни callers | Per-phase migration с verify; one function at a time |
| **User code breaks** при strict mode | Diagnostic с machine-applicable suggestion + migration tool |
| **False positives** — legit Category B mis-classified как A | Audit doc + debug_assert + tests f31-f40 |
| **Bench regression** от Result propagation overhead | Inline `Result<String, String>` — zero-cost; verify в Ф.5 |
| **Не все 155 sites покроет phasing** — missed Category A | Audit script automation в Ф.0 + Ф.5 re-verification |

---

## Связь

- **Plan 67** — sibling fix (println overload — самый видимый частный случай этого класса)
- **Plan 48** — monomorphization (упрощает Category B → меньше erasure)
- **Plan 56** — vtable dispatch (closed похожий class)
- **Plan 59** — tuple mono (closed похожий class)
- **Plan 36** — diagnostic infra (R7 structured format)
- **Plan 58** — cross-toolchain CI (gate для strict mode validation)
- **Plan 70+1** — codegen warning channel (для opt-in W67xx-class warnings — отдельный план если нужно)
- **Spec D126** — нормативно фиксирует strict invariant

---

## Конкретные known bugs закрываемые Plan 70

### Set.or / Set.and / Set.minus — silent wrong output

`Set.or(other)`, `Set.and(other)`, `Set.minus(other)` возвращают
generic `Nova_Set*` без mono'd типа. Последующий вызов `.contains(x)`
на результате диспатчится в `Nova_Set_method_contains` — stub,
всегда возвращающий `false`. Это Category A silent miscompilation.

**Воспроизведение:** тест `nova_tests/modules/set.nv` — тесты
`or()`/`and()`/`minus()` удалены из-за этого бага (покрыты в stdlib
inline-тестах до фикса). После Plan 70 Phase A2/A3 эти методы
должны работать корректно — тесты вернуть.

**Acceptance:** после Plan 70 написать/вернуть тесты Set.or/and/minus
в `nova_tests/modules/set.nv` и убедиться что PASS.

---

## Sub-планы

- **[Plan 70.1](70.1-module-alias-resolution.md)** — `import X as th` + `th.func()` не раскрывается в C codegen
- **[Plan 70.2](70.2-linkedlist-sum-type-mono.md)** — LinkedList sum-type monomorphization broken

---

## Эволюция плана

- **2026-05-18 created**: после Plan 67 closure user раскрыл systemic
  problem — «всё неизвестное = int, куча багов в истории». Audit показал
  155 fallback sites (120 silent miscompilation Category A). Plan 70 —
  production-grade strict mode, 10-15 dev-days.
- **2026-05-18**: добавлен concrete known bug Set.or/and/minus (Category A).
  Sub-планы 70.1 (module alias) и 70.2 (LinkedList mono) зафиксированы.
- **2026-05-18 PhaseA partial session 1** (worktree `plan-70`,
  commits `1b353895908`...`36c32b8edd0`):
  - Ф.0 audit refined: 117 Cat A → 64 real Cat A1 (precise pattern
    `type_ref_to_c().unwrap_or "nova_int"`).
  - Ф.1 diagnostic helper `err_no_int_fallback` (E7001 reserved).
  - Baseline: **761 PASS / 0 FAIL / 44 SKIP**.
  - **~40 Cat A sites migrated** в Result-returning fns (no cascade):
    PhaseA1.1 emit_type_decl (1), A1.2 emit_fn_forward_decl (5),
    A1.3 emit_module overload (4), A1.4 emit_stmt let-fn-binding (11),
    A2.1 emit_call HOF (5), A2.2 emit_fn + emit_lambda (6),
    A2.3 emit_record_type (2), A3 mono'd method/fn/type/expr (7).
  - 5 positive test fixtures `nova_tests/plan70/f1-f5_*.nv` — все PASS.
  - **Final regression: 766 PASS / 0 FAIL / 44 SKIP** (+5 fixtures,
    **0 regressions** vs baseline).

  **Remaining ~24 sites следующей сессии:**
  - **8 cascade-blocked** (no Result return — register_mono_instance,
    register_mono_method_instance, infer_expr_c_type,
    infer_mono_method_ret_with_args) — нужен signature refactor.
  - **6 Cat B intentional erasure** (erased_type_ref_c,
    emit_generic_method_erased, emit_fn erase_unk wrapped,
    type_ref_to_c any_erased detection) — leave с inline doc rationale.
  - **~10 misc smaller patterns** — sweep.

  **Honest negative-test gap:** все migrated sites unreachable в practice
  (type-checker pre-rejects pre-codegen). Migration = dead-code hardening
  + diagnostic guard. Negative .nv тесты не реализуемы — documented
  в `nova_tests/plan70/README.md` per-site.
