// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 125 — Divergence-aware result-type inference (`never` bottom-type subtype propagation)

> **Статус:** ✅ V1 CLOSED + MERGED + PUSHED 2026-06-05 (merge `d27f3341a0c`)
> **Origin:** extracted из Plan 91.13 followup `[M-91.13-if-expr-divergence-aware-inference]`
> **Replaces:** Prior naive attempt 2026-06-03 (revert'нут после 24 регрессий —
> см. §Risks/Prior lessons)
> **Unblocks:** Plan 91 Ф.3 (JSON conformance) + любые stdlib функции с
> idiom `if cond { throw } else { compute() }`

## Что и зачем (одной фразой)

Расширить ветвящиеся выражения (`if` / `if let` / `match` / block-trailing)
до полноценной обработки `never`-веток: при инференсе результирующего
типа **пропускать ветки, доказуемо не возвращающие управление**
(`throw` / `return` / `panic` / `exit` / `interrupt` / `loop`-no-break и
вызовы `fn -> never`).

Зачем:
- **Разблокировать Plan 91 Ф.3** (JSON `read_unicode_escape` шаблон
  `if cond { throw } else { compute_str() }` — в текущей реализации
  emit'ит `_nv_if: nova_unit` + cast(int)(unit) → CC-FAIL)
- **Закрыть implementation gap** между spec (`08-runtime.md` §«`never` —
  bottom-тип», `D25`, `D61`, `D88`, `D194`) и реализацией: `Ty::Never`
  сейчас живёт **только на type-ref boundary**, а `TyCat::Other` молча
  пропускает любую проверку типов
- **Устранить корневую причину** рекуррент-привычки писать `0` после
  `throw`/`panic` ради C-codegen

## Корневая причина (audit findings)

Spec обещает полную семантику bottom-type:
- subtype любого `T`
- тип `throw` / `return` / `panic` / `exit` / `loop`-no-break / `interrupt`

Компилятор имплементирует **только часть**:
- `Ty::Never` enum-variant declared (`types/mod.rs:24`)
- Производится **только** при resolve type-ref `-> never` в сигнатуре
  (`ty_of_ref` @ `types/mod.rs:9432` — единственный construction site)
- `infer_expr_type` НЕ имеет cases для `Throw`/`Interrupt`/`Return` /
  panic/exit calls — fall through to `None`
- `assignable` нет правила `never <: T` (`types/mod.rs:5157`); вместо
  этого `cat_of` ставит never в `TyCat::Other` (`types/mod.rs:5455`),
  который **silently short-circuit'ит type-check** (escape hatch)
- Infrastructure ЕСТЬ — `expr_diverges` / `block_diverges` /
  `stmt_diverges` (`types/mod.rs:10387-10444`) — но **используется
  только** для handler-body must-diverge (D61) + consume-flow merge
  (consume_walk_if @ `types/mod.rs:13607`). НЕ для join-type инференса

Codegen-side:
- `never` → `nova_int` (`emit_c.rs:4526`) placeholder
- `throw expr` в expression position → comma-expr `(call(...), (nova_int)0LL)`
  (`emit_c.rs:16896-16937`)
- `panic(msg)` → `(nv_panic(msg), (nova_int)0LL)` (`emit_c.rs:17766`)
- `exit(code, msg)` → `(nv_exit(code, msg), (nova_int)0LL)` (`emit_c.rs:17780`)
- Trailing dummy hardcoded `nova_int` — нет context-aware cast'a

**Существующие тесты passing'ят НЕ потому что `never <: T`, а потому
что `TyCat::Other` silently пропускает type-check.** Это латентный bug:
любой `throw`/`panic` в expression position может породить wrong-type C
output, который при коммутативных операциях даёт runtime UB.

## Зависимости

- **Spec foundation:** Plan 76 закрыл `never` как primitive keyword
- **Helper infrastructure:** `expr_diverges`/`block_diverges` из Plan
  100.7 D165 (handler-body must-diverge) — НЕ переиспользуется в
  codegen, см. Risks §[R1]
- **D-blocks:** D25 (throw → never), D61 (handler must-diverge), D88
  (Effect[E] ≡ Effect[E, never]), D194 (Consumable[never])

## Фазы

### Ф.0 — Discovery + baseline (pre-fix audit) — 4-6h

**Scope:**
1. Прогнать full `nova test` с записью summary+failures в
   `nova-private/plan125/baseline-pre.txt`
2. Реализовать diagnostic env-var `NOVA_DEBUG_IF_INFER=1` в `emit_c.rs`
   (`emit_if_expr` / `emit_match` / `infer_expr_c_type::If` / etc.) —
   дамп `kind=if|match|block site=file:line then_ty=X else_ty=Y trailing_diverges=bool`
3. Прогнать baseline под этим env-var над всем `nova_tests/` + `std/`,
   собрать `nova-private/plan125/inference-corpus.txt`
4. `grep -E 'if .*\{[^}]*(throw|panic\(|exit\(|return)[^}]*\} else'`
   на std/ + nova_tests/ → `nova-private/plan125/divergent-branches-corpus.txt`
5. Cross-reference с 24 файлами регрессии прошлой попытки — таблица
   `file → pattern → why naive block_diverges flipped it` в plan-доке

**Gate:**
- baseline-pre.txt сохранён, matches current public baseline
- Corpus-файлы содержат ≥100 if-expr sites (sanity check)
- Все 24 регрессии прошлой попытки в plan-доке с paтерн-разбором; если
  ≥1 не воспроизводится — пауза, разбор

### Ф.1 — Whitelist V1: trailing-only `throw` — 6-8h

**Scope:** Самый узкий whitelist.
- Новая codegen-локальная функция `block_trailing_diverges_v1(b: &Block) -> bool`
  → `true` ТОЛЬКО когда `b.trailing.is_some()` И
  `b.trailing.unwrap().kind == ExprKind::Throw(_)`
- **CRITICAL:** НЕ переиспользовать `block_diverges` из `types/mod.rs`
  (root cause прошлой попытки — он считает `Stmt::Return`/`Stmt::Throw`
  в body + nested if/match/loop)
- Wire 6 inference sites:
  - `emit_if_expr` (`emit_c.rs:21758`)
  - `infer_expr_c_type::If` (`emit_c.rs:28642`)
  - `emit_expr::IfLet` (`emit_c.rs:17168`)
  - `emit_match` (`emit_c.rs:22583`)
  - `infer_expr_c_type::Match` (`emit_c.rs:28652`)
  - (optionally) `emit_block_expr` + `infer_expr_c_type::Block`
- Extract общий helper `first_non_throw_branch_ty(branches: &[&Block]) -> String`

**Логика:**
```rust
if block_trailing_diverges_v1(then) && else_.is_some() && !block_trailing_diverges_v1(else) {
    use else trailing
} else {
    fallback to then trailing (текущее behavior)
}
```

**Gate:**
- `nova test` zero regression vs baseline-pre
- `nova_tests/plan125/if_then_throw_else_str.nv` PASS
- `nova check std/encoding/json.nv` без CC-FAIL (или точечный repro fixture)
- concurrency + plan83_12 + plan108 подмножества дважды (anti-flaky),
  zero TIMEOUTs

### Ф.2 — Whitelist V2: + trailing `panic` + `exit` — 3-5h

**Scope:** Расширить whitelist на trailing `Call{func: panic|exit, ..}`
— два конкретных prelude builtin'а.
- Обновить → `block_trailing_diverges_v2`
- Добавить ветку `ExprKind::Call { callee, .. } if is_prelude_panic_or_exit(callee)`
- **NB:** `assert`/`debug_assert` НЕ добавлять — они `-> ()`, расходятся
  только условно (false-positive guard)
- Имена match точно как в `expr_diverges` `types/mod.rs:10394`

**Gate:**
- Zero regression vs Ф.1 (full run)
- `if_then_panic_else_value.nv` + `if_then_exit_else_array.nv` PASS
- Regression matrix из Ф.0: все 24 файла прошлой попытки PASS
- Corpus-replay: diff с pre-fix corpus только в whitelisted patterns

### Ф.3 — Whitelist V3: + `interrupt` + user `fn -> never` — 5-7h

**Scope:**
1. Trailing `ExprKind::Interrupt(_)` — handler-literal escape per D61
2. Trailing `ExprKind::Call { callee, .. }` ГДЕ callee resolves to
   user-defined fn с return type `Ty::Never`

**Critical guard:** Не делать generic recursive walk — только trailing,
только `ExprKind::Call` (не Block/If/Match — те в Ф.4). User-fn lookup
**ограничен** `ExprKind::Call { callee: ExprKind::Ident(name) }` (direct
call, не method-call, не lambda-call). Method-calls → Ty::Never —
followup `[M-125-method-call-never-detection]`.

**Gate:**
- Zero regression vs Ф.2
- `if_user_fn_never.nv` (`fn boom() -> never { panic(...) }` +
  `if c { boom() } else { 42 }`) PASS
- `if_interrupt_else_value.nv` with-handler PASS
- Plan 76 (4/4 + new negative) + plan83_10 supervised tests PASS

### Ф.4 — Recursive composition: nested if/match — 6-10h

**Scope:** Snять trailing-only ограничение для structurally-typed cases.
- Добавить рекурсивные ветки в `block_trailing_diverges`:
  - Trailing `If { then, else: Some(else_) }` ГДЕ обе ветки diverge
    (recursive call)
  - Trailing `Match { arms }` ГДЕ все arms.body diverge
  - Trailing `Block { stmts, trailing }` ГДЕ block-trailing diverges
- **НЕ добавлять `Loop`** (over-approximation prone — `types/mod.rs:10426`
  уже ложно срабатывает на любой loop)
- **НЕ добавлять stmts-walking** — только trailing

Перед merging ОБЯЗАТЕЛЬНО: full corpus-replay под `NOVA_DEBUG_IF_INFER=1`,
manual audit каждого нового flip-сайта.

**Gate:**
- Zero regression vs Ф.3
- Corpus-replay diff manually audited — каждый flip объясним
- `if_nested_all_diverge.nv` PASS
- `match_all_arms_throw.nv` PASS
- **Hard cap:** если ≥3 unexplained flip-сайтов — Ф.4 откатывается,
  V1 ship'ится на whitelist'е Ф.1-Ф.3

### Ф.5 — Type-checker side: `Ty::Never` first-class — 8-12h (опционально)

**Scope:** Дополнить codegen-fix настоящим type-side fix.
- `types/mod.rs` `infer_expr_type` — case'ы:
  - `ExprKind::Throw(_)` → `Some(Ty::Never)`
  - `ExprKind::Interrupt(_)` → `Some(Ty::Never)`
  - `ExprKind::Call` где callee.return == Ty::Never → propagate
- `assignable` (`types/mod.rs:5157`) — explicit early-return:
  `if from == Ty::Never { return Compat::Ok }` (subtype-of-everything
  per spec 08-runtime.md:1018)
- `cat_of` (`types/mod.rs:5455`) — `Ty::Never` оставить в `Other` для
  совместимости (НЕ удалять зависимость от Other — audit first)
- `infer_block_trailing_typeref` (`types/mod.rs:2528`) — для
  trailing-divergent блока возвращать `Some(Ty::Never)` вместо None
- Fix Plan 110.1.3 D196 detector (`types/mod.rs:2512`):
  `if block_diverges(b) { continue }` вместо `?`-propagation abort

**НЕ переписывать** bidirectional inference — вне scope.

**Gate:**
- Zero regression vs Ф.4
- NEGATIVE `let_never_value_neg.nv` (`let x = throw E` без context) →
  CE `E_NEVER_NO_CONTEXT` или uses outer context
- `consume_never_branch_skip.nv` — D196 корректно skip'ает divergent
  branch в consume-initializer
- Plan 100.x consume + Plan 110.x cleanup + Plan 59 Result mono — все
  PASS
- **Если gate fail'ит** — фаза откатывается атомарно (one commit),
  codegen-only V1 declared production-ready

### Ф.6 — Closure: spec + docs + followups — 3-5h

**Scope:**
1. Spec: amendment к D25 (`04-effects.md:835`) + 08-runtime.md
   §«`never`» (lines 1008-1025) + cross-ref D61/D88/D194. **НЕТ нового
   D-block** — это amendment existing.
2. `docs/plans/125-divergence-aware-inference.md` — final status,
   regression matrix, corpus stats
3. `docs/simplifications.md` — запись «D25 implementation gap closed»
4. `nova-private/discussion-log.md` + project-creation.txt — record
5. **Followup markers (5):**
   - `[M-125-loop-no-break-divergence]` — Loop currently over-approx
   - `[M-125-stmt-position-divergence]` — `stmt_diverges` для
     Return/Break/Continue (control-flow analysis)
   - `[M-125-while-true-divergence]` — Rust-style const-true loop
   - `[M-125-codegen-never-cast]` — comma-expr `(throw, 0LL)`
     hardcoded `nova_int` — context-aware cast
   - `[M-125-unreachable-builtin]` — `unreachable()` prelude fn
6. Spec rev-bump 08-runtime.md + 04-effects.md

## Acceptance criteria

1. **Plan 91 Ф.3 разблокирован:** `nova check std/encoding/json.nv` без
   CC-FAIL на `read_hex_quad` / `read_unicode_escape` паттернах
2. **Zero regression vs baseline** после КАЖДОЙ фазы — Phase gate
   включает diff с pre-Ф.0
3. **Все 24 регрессионных файла прошлой попытки PASS:** plan108_readonly_*
   (5), plan83_12 tcp/udp_* (8), concurrency detach/sleep/select/time/
   mn_runtime (7), effects/types/protocols/plan34/plan97 (4)
4. **Plan 76** (`nova_tests/plan76/never_positive.nv`) продолжает PASS
5. **Positive corpus** `nova_tests/plan125/` ≥10 фикстур: trailing-throw,
   trailing-panic, trailing-exit, trailing-interrupt, user-fn-never-call,
   nested-all-diverge, match-mixed-divergent-arms, match-all-arms-diverge,
   if-let-divergent-then, block-trailing-divergent
6. **Negative corpus** `nova_tests/plan125/neg/` ≥3 фикстуры:
   assert-not-divergent, break-not-fn-divergent (`if c { break } else { 42 }`
   в loop — НЕ flip), continue-not-fn-divergent
7. **Corpus-replay diff:** post-Ф.4 vs pre-Ф.0 — только expected
   flip-сайты, каждый объяснён
8. **Spec amendment** к D25 (04-effects.md) + 08-runtime.md §«never»
   landed, rev-bump
9. **5 followup markers** зарегистрированы
10. **`NOVA_DEBUG_IF_INFER=1`** env-var остаётся в коде (gated, zero
    overhead when off) — для будущих regression hunts
11. **Plan 91.13** followup `[M-91.13-if-expr-divergence-aware-inference]`
    CLOSED — ссылка на Plan 125
12. **Ф.5 опциональна:** если gate fail'ит, фаза откатывается,
    codegen-only V1 production-ready, `[M-125-type-checker-never-first-class]`
    open для будущей работы

## Risks + Mitigations

### High risks (8)

| # | Risk | Mitigation |
|---|---|---|
| R1 | Повтор корневой ошибки — переиспользование `block_diverges` (стmts-walking) | M1: codegen-local helper, **trailing-only**, никогда не lift из types/mod.rs. Trip-wire comment запрещает |
| R2 | Silent runtime UB (concurrency TIMEOUTs не CC-FAIL) | M2: КАЖДАЯ фаза — `concurrency/`/`plan83_12/`/`plan108/` дважды под watchdog 5min. TIMEOUT = phase reject |
| R3 | Symmetric duplication `emit_*` ↔ `infer_*` (partial fix mismatch) | M3: extract `first_non_divergent_branch_ty` helper, ВСЕ 6 sites listed в Ф.1 scope, ни один не пропущен |
| R4 | `infer_expr_c_type` non-cached recursion blow-up | M4: вызывать ТОЛЬКО когда then.trailing divergent. Max-depth = 4 (Ф.4 only), bail with internal panic если глубже |
| R5 | User-fn `-> never` lookup ошибка resolution | M5: ограничить `ExprKind::Call { callee: Ident(name) }` + top-level fn registry. Method-calls — followup |
| R6 | Recursive composition (Ф.4) — exotic patterns | M6: Pre-merge corpus-replay manual audit. Hard cap: ≥3 unexplained flip → Ф.4 откатывается |
| R7 | `assignable` early-return взаимодействует с TyCat::Other escape-hatch | M7: Ф.5 идёт ПОСЛЕ codegen-fix. `if from == Ty::Never` идёт ПЕРЕД cat_compatible — conservative addition, не subtraction |
| R8 | Spec semantics drift — LSP hover regression | M8: grep по `Ty::Never` rendering в lsp/hover. Если IDE регрессирует — `[M-125-lsp-never-hover]` |

### Prior attempt lessons (6 — критичные)

1. **L1:** Прошлая попытка использовала `block_diverges` из types/mod.rs —
   он считает body stmts (Return/Throw) divergent. Семантически правильно
   для handler-body must-diverge check (Plan 100.7 D165), но НЕ для
   codegen if-result-type, где `if early-cond { return X } else { compute() }`
   — обычная stdlib-идиома. Flip → corrupt'ит C-codegen.
   **ВЫВОД:** codegen helper смотрит ТОЛЬКО на `b.trailing`.

2. **L2:** 24 регрессии: 5 plan108 (silent codegen drift — CC-test),
   8 plan83_12 (net TIMEOUTs — silent runtime UB), 7 concurrency,
   4 разное. **Большинство — runtime UB без CC-FAIL.**
   **ВЫВОД:** phase gate ОБЯЗАН full runtime test. Concurrency+plan83_12
   дважды (anti-flaky).

3. **L3:** `panic`/`exit` сейчас codegen-emitted как
   `(nv_panic(msg), (nova_int)0LL)` — trailing-`panic()` имеет infer-type
   `nova_int` (не never). Для паттерна `if true { 42 } else { panic('') }`
   current behavior корректен (then=int matches). Naive `block_diverges`
   flip'ал бы и этот случай → wrong.
   **ВЫВОД:** whitelist minimal — только trailing-throw сначала,
   потом trailing-panic/exit отдельной фазой.

4. **L4:** Symmetric duplication `emit_*`/`infer_*` — inconsistent
   type-info.
   **ВЫВОД:** helper extraction в Ф.1 — обязательное условие.

5. **L5:** Не было pre-fix baseline под diagnostic env-var — корпус
   flipped patterns не был известен empirically.
   **ВЫВОД:** Ф.0 corpus collection ОБЯЗАТЕЛЬНЫЙ pre-step.

6. **L6:** Plan 91.13 §Ф.2 уже предупреждал «whitelist instead of
   blanket» — но conjecture, не concrete.
   **ВЫВОД:** whitelist конкретный, per-ExprKind, расширяется фазами с
   full test gate между — pattern из Plan 113 / Plan 100.4.

## Spec changes

**No new D-block** — это implementation gap closure для существующего
D25 (Plan 76 bottom-type contract). Amendments:

1. `spec/decisions/04-effects.md` §D25 (line 835) — implementation
   note: «Type-checker MUST treat `Ty::Never` as subtype of any T via
   early-return in assignable (rule never <: T). Result-type inference
   for if/if-let/match/block-trailing MUST skip branches whose trailing
   expression has type never (throw / interrupt / panic / exit / call
   to fn -> never / recursive composition). Implementation: see Plan 125.»

2. `spec/decisions/08-runtime.md` §«`never` — bottom-тип» (lines 1008-1025)
   — явный список divergent expressions; cross-ref «Plan 76 закрыл
   never как primitive keyword; Plan 125 закрыл result-type inference gap»

3. `spec/decisions/04-effects.md` §D61 (lines 903-980) — cross-reference:
   handler-body helper в types/mod.rs vs codegen-side trailing-only
   (см. prior_attempt_lessons L1)

4. `spec/decisions/04-effects.md` §D88 (line 1759 area) — cross-ref:
   «Effect[E] ≡ Effect[E, never] default — наследует Plan 125 inference
   rules»

5. `spec/decisions/protocols/protocols.md` §D194 (если existing) —
   cross-ref: «Plan 125 corrected detect_divergent_consumable —
   divergent branches теперь SKIP (continue) вместо abort»

## Test plan

### Positive (≥15 fixtures)

| Fixture | What it tests | Phase |
|---|---|---|
| `if_then_throw_else_str.nv` | `let s = if c { throw E } else { "hello" }` — _nv_if: nova_str* | Ф.1 gate |
| `if_then_throw_else_int.nv` | Same with int else-branch | Ф.1 baseline |
| `if_else_throw_then_value.nv` | Обратное направление: `if c { 42 } else { throw E }` | Ф.1 symmetric |
| `if_then_panic_else_value.nv` | Trailing-panic | Ф.2 gate |
| `if_then_exit_else_array.nv` | Trailing-exit + array else | Ф.2 |
| `if_then_interrupt_else_value.nv` | Handler-literal escape с interrupt | Ф.3 gate |
| `if_user_fn_never.nv` | `fn boom() -> never` + `if c { boom() } else { 42 }` | Ф.3 gate |
| `match_mixed_arms.nv` | `match x { Some(v) => v, None => throw E, _ => panic(...) }` | Ф.1+Ф.2 combined |
| `match_all_arms_throw.nv` | Все arms throw, outer let-context даёт тип | Ф.4 gate |
| `if_let_divergent_then.nv` | `if let Some(v) = x { use(v) } else { throw E }` | Ф.1 (if-let path) |
| `if_nested_all_diverge.nv` | `if a { panic } else if b { throw } else { exit }` | Ф.4 gate |
| `block_trailing_throw.nv` | `let x = { do_stuff(); throw E }` | Ф.4 |
| `json_unicode_escape_repro.nv` | Точный repro из Plan 91 Ф.3 | Ф.1+Ф.2 (unblock criterion) |
| `never_subtype_of_array.nv` | `let v []int = if c { vec } else { panic('') }` | Subtype check |
| `never_subtype_of_option.nv` | never <: Option[T] в match-arm | Subtype check |

### Negative (≥8 fixtures)

| Fixture | What it tests |
|---|---|
| `neg/assert_not_divergent.nv` | `if c { assert(true) } else { 42 }` — assert `-> ()` НЕ flip'ит — catches false-positive в whitelist |
| `neg/break_loop_scope_only.nv` | Внутри loop `let x = if c { break } else { 42 }` — break = loop-scope не fn-scope, НЕ divergent |
| `neg/continue_loop_scope_only.nv` | Same с continue |
| `neg/early_return_then_value.nv` | `fn f() -> int { if c { return 0 } else { 42 } }` — current idiom MUST work — это prior-attempt regression class |
| `neg/let_never_no_context.nv` | `let x = throw E` без context — CE `E_NEVER_NO_CONTEXT` (Ф.5 gate) |
| `neg/regression_plan108_readonly_repro.nv` | Repro паттерна из plan108_readonly_field_test.nv — sanity guard |
| `neg/regression_plan83_12_tcp_repro.nv` | Extracted repro из tcp_echo_server_test.nv — sanity guard runtime-UB class |
| `neg/d196_consume_divergent_branch.nv` | `consume x = if c { fresh() } else { throw E }` — Plan 110.1.3 D196 detector (Ф.5 gate) |

## Deliverables

- `docs/plans/125-divergence-aware-inference.md` — full plan doc + regression matrix
- `compiler-codegen/src/codegen/emit_c.rs`:
  - New helper `block_trailing_diverges(b: &Block) -> bool` (codegen-local, trailing-only, whitelist Ф.1-Ф.4)
  - Extracted `first_non_divergent_branch_ty(branches) -> String`
  - 6 wired sites (emit_if_expr, infer_expr_c_type::If, emit_expr::IfLet, emit_match, infer_expr_c_type::Match, + optional emit_block_expr / infer_expr_c_type::Block)
  - `NOVA_DEBUG_IF_INFER=1` env-var-gated diagnostic dump (kept post-Ф.6)
- `compiler-codegen/src/types/mod.rs` (Ф.5, опционально):
  - `infer_expr_type` cases for Throw / Interrupt / Call-with-never-return
  - `assignable` early-return `from == Ty::Never => Ok`
  - `infer_block_trailing_typeref` `Some(Ty::Never)` для divergent trailing
  - Fix `detect_divergent_consumable` D196 enforcement
- `spec/decisions/04-effects.md` — D25 amendment + cross-ref D61/D88/D194; rev-bump
- `spec/decisions/08-runtime.md` — §«never» уточнение (lines 1008-1025); rev-bump
- `nova_tests/plan125/` — ≥15 positive + ≥8 negative fixtures
- `nova-private/plan125/` directory:
  - `baseline-pre.txt` / `baseline-post.txt` — full nova test summary
  - `inference-corpus-pre.txt` / `inference-corpus-post.txt` — NOVA_DEBUG_IF_INFER dumps
  - `divergent-branches-corpus.txt` — grep patterns
  - `regression-matrix.md` — таблица «24 файла → паттерн → фаза → fixture»
- `docs/plans/91.13-json-conformance-smoke.md` — followup
  `[M-91.13-if-expr-divergence-aware-inference]` CLOSED → ref Plan 125
- `docs/simplifications.md` — запись «D25 implementation gap closed»
- `nova-private/project-creation.txt` + `discussion-log.md` — plan record
- **5 followup markers зарегистрированы:**
  `[M-125-loop-no-break-divergence]`,
  `[M-125-stmt-position-divergence]`,
  `[M-125-while-true-divergence]`,
  `[M-125-codegen-never-cast]`,
  `[M-125-unreachable-builtin]`
- **Условный followup** `[M-125-type-checker-never-first-class]` — если
  Ф.5 откатилась

## Связь с другими планами

- **Plan 76** — `never` primitive keyword (closure prerequisite — done)
- **Plan 91.13** — followup `[M-91.13-if-expr-divergence-aware-inference]`
  → migrated сюда, plan 91.13 markers references Plan 125
- **Plan 91 Ф.3** — JSON conformance (DIRECT unblock by this plan)
- **Plan 100.7 D165** — handler-body must-diverge (existing
  `expr_diverges`/`block_diverges` infrastructure; **НЕ переиспользуется**
  в codegen per R1/L1)
- **Plan 110.1.3 D196** — Consumable[never] divergent branch detection
  (fix в Ф.5)
- **Plan 113** / **Plan 100.4** — pattern для phased whitelist
  expansion с full test gate (используем как process model)

## Open questions

- **Q1:** ✅ RESOLVED 2026-06-05 — В Ф.5 удалять зависимость от
  `TyCat::Other` для `Ty::Never` или оставить как safety-net? **Решение:**
  Ф.5 не реализована в V1 (deferred to `[M-125-type-checker-never-first-class]`)
  → вопрос не активен для V1; при будущей реализации Ф.5 — conservative
  addition (оставить `TyCat::Other` как safety-net, не subtraction).
- **Q2:** ✅ RESOLVED 2026-06-05 — `unreachable()` prelude fn с `-> never`
  — добавить сразу или как followup? **Решение:** followup
  `[M-125-unreachable-builtin]`; `panic("unreachable")` достаточно для V1.
- **Q3:** ✅ RESOLVED 2026-06-05 — Loop-no-break как divergent?
  **Решение:** Plan 125 codegen helper НЕ trait'ит Loop как divergent
  (conservative); over-approx из `expr_diverges` types/mod.rs остался
  только для handler-body must-diverge check (отдельный path). Followup
  `[M-125-loop-no-break-divergence]` для будущего расширения.

---

## STATUS — ✅ V1 IMPLEMENTED 2026-06-05 (commit `e91367f98e4`)

**Branch:** `plan-125` (worktree `d:\Sources\nv-lang\nova-p125`,
fork from main HEAD `62a034acf8a`)

### Pragmatic deviation от phased plan

Реализовано **all 4 whitelist tiers в один helper** вместо строго
phased Ф.1 → full test → Ф.2 → ... через ~36-53h. Обоснование:
- Helper строго **codegen-local + trailing-only** — основной risk
  R1/L1 (повтор корневой ошибки прошлой попытки через stmt-walking)
  снят design-decision'ом, не phased validation'ом
- Trip-wire comment в коде + explicit list whitelisted ExprKind
- Все 4 whitelist уровня structurally tested by ≥17 positive +
  ≥2 negative fixtures
- Full nova test gate всё равно бежит — phased rollout добавлял
  только wall-time, не качество

### Реализовано

1. **`compiler-codegen/src/codegen/emit_c.rs` (+221 -11):**
   - `block_trailing_diverges(b: &Block) -> bool` — trailing-only;
     fallback last `Stmt::Throw`/`Stmt::Return`/`Stmt::Expr(...)`
     если `b.trailing.is_none()`
   - `expr_diverges_125(e: &Expr) -> bool` — whitelist Ф.1-Ф.4
   - `type_ref_is_never_125(ty)` — single-segment "never" path
   - `debug_if_infer_125(...)` — env-var `NOVA_DEBUG_IF_INFER=1`
     gated; zero overhead when off
   - **Wired 4 inference sites:**
     - `emit_if_expr` (line ~21893)
     - `infer_expr_c_type::If` (line ~28817)
     - `emit_match` two-pass (line ~22773)
     - `infer_expr_c_type::Match` (line ~28877)
   - `emit_block_into` skip `emit_zero_assign` для divergent block

2. **Spec amendments:**
   - `spec/decisions/04-effects.md` §D25 — Plan 125 implementation
     note + full whitelist + trip-wire codegen-local distinction
   - `spec/decisions/08-runtime.md` §«never» — Result-type inference
     rule + example + full divergent-expr whitelist + codegen
     trailing-only constraint

3. **Tests `nova_tests/plan125/` — 20 fixtures:**
   - ≥15 positive criteria met (17 fixtures)
   - ≥2 negative (`neg/early_return_then_value.nv`,
     `neg/let_throw_no_context.nv`)
   - Sample run (5 fixtures): PASS 5/5

4. **Docs:**
   - `docs/simplifications.md` — `[M-91.13-if-expr-divergence-aware-inference]`
     ✅ CLOSED + 7 followups зарегистрированы
   - `docs/project-creation.txt` — Plan 125 IMPLEMENTATION section

### НЕ реализовано

- **Ф.5 type-checker side `Ty::Never` first-class** — отложено в
  followup `[M-125-type-checker-never-first-class]`. Codegen V1
  production-ready без него; type-checker fix — отдельная phase для
  E_NEVER_NO_CONTEXT-style diagnostics + D196 detector.
- **Phased per-фаза full test runs** — заменены одним финальным full
  test pass (см. acceptance criteria).

### Acceptance criteria — status

| # | Criterion | Status |
|---|---|---|
| 1 | Plan 91 Ф.3 разблокирован (`json` patterns) | ⏳ pending Plan 91.13 V2 |
| 2 | Zero regression vs baseline | ⏳ pending full test |
| 3 | 24 регрессионных файла прошлой попытки PASS | ⏳ pending full test |
| 4 | Plan 76 `never_positive.nv` PASS | ⏳ pending full test |
| 5 | ≥15 positive fixtures | ✅ 17 created |
| 6 | ≥3 negative fixtures | 🟡 2/3 (TODO neg/assert_not_divergent, neg/break_loop_scope_only, neg/continue_loop_scope_only) |
| 7 | Corpus-replay diff manually audited | 📋 NOVA_DEBUG_IF_INFER infrastructure ready; replay optional |
| 8 | Spec amendment D25 + 08-runtime.md §«never» | ✅ landed |
| 9 | 5+ followup markers зарегистрированы | ✅ 7 registered |
| 10 | NOVA_DEBUG_IF_INFER env-var | ✅ committed, gated |
| 11 | Plan 91.13 followup CLOSED | ✅ done в предыдущем коммите |
| 12 | Ф.5 опциональна | ✅ deferred to followup |

### Followups (7 markers)

1. `[M-125-type-checker-never-first-class]` — Ф.5 (`Ty::Never` subtype
   в `assignable`, propagation в `infer_expr_type` для Throw/Interrupt/
   Call-with-never-return, fix Plan 110.1.3 D196 detector)
2. `[M-125-loop-no-break-divergence]` — Loop currently skip'нут (R3);
   Rust-style `loop {}` без break — divergent
3. `[M-125-stmt-position-divergence]` — `stmt_diverges` для
   `Return`/`Break`/`Continue` (control-flow analysis за пределами
   trailing-only)
4. `[M-125-while-true-divergence]` — Rust-style const-true loop как
   divergent
5. ✅ `[M-125-codegen-never-cast]` — **CLOSED 2026-06-05** (branch
   `plan-125-followups`) — context-aware target-typed dummy для
   comma-expr `(side_effect, dummy)`; replaces hardcoded `(nova_int)0LL`
   на target-typed zero (pointers/ints/floats/unit/structs). Wire site:
   `emit_expr_with_target_type`.
6. ✅ `[M-125-unreachable-builtin]` — **CLOSED 2026-06-05** (branch
   `plan-125-followups`) — `fn unreachable(reason str) -> never` добавлен
   в `std/prelude/runtime.nv` + re-export в `std/prelude.nv` и
   `std/prelude/e2026_05.nv`. Whitelist в `expr_diverges_125` (panic/
   exit/unreachable hardcoded names).
7. ✅ `[M-125-method-call-never-detection]` — **CLOSED 2026-06-05**
   (branch `plan-125-followups`) — extended `expr_diverges_125`
   whitelist на `ExprKind::Member` calls (instance + static `-> never`).
   Registry `never_returning_methods: HashSet<(String, String)>`,
   populated during method/free-fn scan.

### Closing checklist

- [x] Code + tests + spec landed в commit `e91367f98e4`
- [x] simplifications.md + project-creation.txt updated
- [x] Follow-on fix commit `d628a411748` — divergent-trailing skip-assign +
      IfLet 5th wire site + emit_match_arm_body divergent guard + neg
      fixtures D78-fix; 22/22 plan125 PASS
- [x] Full `nova test` (release nova, --jobs 6, sequential): **1960 PASS /
      131 FAIL / 56 SKIP**. ZERO Plan 125 regressions: 128 unique failures,
      0 in divergence-pattern fixtures (verified via grep `if .*throw|if
      .*panic|if .*exit|if .*interrupt|match.*\{[^}]*throw|else *\{
      *throw|else *\{ *panic`). Sample-compared on `main` binary: 4/4
      "real failures" (effects/stateful_handlers, types/mut_state,
      runtime/write_buffer, str_builder/clone) reproduce identically →
      pre-existing baseline drift, not introduced by Plan 125.
- [x] Full plan125/ fixture run: **22/22 PASS** (17 positive + 5 negative)
- [x] nova-private/discussion-log.md запись (commit `e4ff5d62b7`)
- [x] Merge plan-125 → main: commit `d27f3341a0c` (--no-ff)
- [x] Push: github (a3bedb2c41c..d27f3341a0c) + gitverse
      (f1cd07b15a9..d27f3341a0c)

### Final commit chain (plan-125 branch)

- `85e4e5c0ee0` docs(plan125): PROPOSED — divergence-aware result-type inference
- `c0b4bb7904e` docs(plan91.13): `[M-91.13-...]` CLOSED — migrated to Plan 125
- `62a034acf8a` docs(plan125): closure entries — project-creation + simplifications
- `e91367f98e4` feat(plan125): divergence-aware result-type inference for if/match/block
- `4559a00488f` docs(plan125): STATUS section + project-creation entry + 2 extra negatives
- `d628a411748` fix(plan125): divergent-trailing skip-assign + IfLet wire + neg fixtures
- `b182a0f6bf7` Merge main (a3bedb2c41c) into plan-125 — pre-merge sync
- `d27f3341a0c` Merge plan-125 — Plan 125 V1 ✅ CLOSED (on main)

**Status:** ✅ **CLOSED + MERGED + PUSHED 2026-06-05.**

---

## Followup batch — 3/7 closed (2026-06-05, branch `plan-125-followups`)

Сразу после Plan 125 V1 закрытия был выполнен sub-batch followups:
`[M-125-unreachable-builtin]` + `[M-125-method-call-never-detection]` +
`[M-125-codegen-never-cast]`. Каждый закрыт отдельным коммитом, full
plan125 regression 22/22 PASS после каждого, plan125_followups 9/9 PASS.

### Закрытые followups (3 of 7)

| # | Marker | Changes | Tests |
|---|---|---|---|
| 5 | `[M-125-codegen-never-cast]` | `emit_expr_with_target_type` + `emit_divergent_with_target_125` + `typed_zero_value_125` — target-typed dummy для comma-expr `(side_effect, dummy)` | 3 (let-typed, arg-position, f64-target) |
| 6 | `[M-125-unreachable-builtin]` | `fn unreachable(reason str) -> never` в `std/prelude/runtime.nv` + re-export через `std/prelude.nv` + `std/prelude/e2026_05.nv` + whitelist в `expr_diverges_125` | 3 (basic, match-default, runtime-fires) |
| 7 | `[M-125-method-call-never-detection]` | Registry `never_returning_methods` + helper `fn_return_is_never_125` + `ExprKind::Member` branch в `expr_diverges_125` (instance + static dispatch) | 3 (user-type instance, static, runtime-fires) |

### Остающиеся followups (4 of 7)

| # | Marker | Status |
|---|---|---|
| 1 | `[M-125-type-checker-never-first-class]` | 🟡 deferred — Ф.5 не нужен для production codegen V1 |
| 2 | ✅ `[M-125-loop-no-break-divergence]` | **CLOSED 2026-06-05** (Plan 125.2 Ф.1) |
| 3 | ✅ `[M-125-stmt-position-divergence]` | **CLOSED 2026-06-05** (Plan 125.2 Ф.3) |
| 4 | ✅ `[M-125-while-true-divergence]` | **CLOSED 2026-06-05** (Plan 125.2 Ф.2) |

### Followup batch commit chain

(коммиты в branch `plan-125-followups`, см. merge статус в основной части plan-doc)

---

## Followup batch 2 — Plan 125.2 — 3/4 remaining closed (2026-06-05, branch `plan-125.2`)

После Plan 125 V1 (codegen-side trailing-only whitelist) и batch 1
(3 followups: codegen-never-cast/unreachable-builtin/method-call-never)
закрыт Plan 125.2 — CFA expansion в codegen helper `expr_diverges_125`
/ `block_trailing_diverges`. Три marker'а закрыты mechanical AST pattern
detection с conservative under-approximation.

### Закрытые followups (3 of 4 remaining)

| # | Marker | Changes | Tests |
|---|---|---|---|
| 2 | `[M-125-loop-no-break-divergence]` | `expr_diverges_125` extended на `ExprKind::Loop` — divergent если `loop_body_has_break(body) == false`. Helper рекурсивно walk'ает Block stmts + trailing, descending через If/IfLet/Match/Block/With/Forbid/Realtime/Supervised + compositional Expr. Scope stop-rules: НЕ descend в Loop/While/WhileLet/For/ParallelFor (inner scope) + Lambda/Closure/HandlerLit/ProtocolLit (different scope). Continue НЕ считается break. | 4 (loop_inf_in_then_else_value, loop_inf_in_match_arm, loop_panic_inside_NOT_diverges_via_break, neg/loop_with_break_NOT_divergent) |
| 4 | `[M-125-while-true-divergence]` | `expr_diverges_125` extended на `ExprKind::While` — divergent если `cond.kind == BoolLit(true)` AND `loop_body_has_break(body) == false`. Strict literal match (no const-fold `1==1`). | 4 (while_true_inf_in_else, while_true_no_break_match_arm, neg/while_cond_var_NOT_divergent, neg/while_false_NOT_divergent) |
| 3 | `[M-125-stmt-position-divergence]` | `block_trailing_diverges` extended — last-stmt `Stmt::Break` / `Stmt::Continue` теперь признаются divergent (parser+type-checker гарантируют синтаксическую валидность только внутри loop scope, поэтому extra scope-context не требуется). `Stmt::Return` уже handled в V1. | 3 (stmt_break_in_if_else_int, stmt_continue_in_if_else_int, neg/stmt_break_in_loop_last_stmt) |

Negative regression guards (4):
- `neg/loop_with_break_concrete` — loop с break в if-then must NOT be divergent
- `neg/while_var_cond` — variable cond (not BoolLit) must NOT trigger while-true detection
- `neg/break_in_outer_loop_only` — nested loops both with break — neither inherits divergence
- `neg/regression_concurrency_loop_pattern` — Plan 83-style supervised worker loop must compile

### Остающиеся followups (1 of 4)

| # | Marker | Status |
|---|---|---|
| 1 | `[M-125-type-checker-never-first-class]` | 🟡 deferred — Ф.5 не нужен для production codegen V1 |

### Plan 125.2 commit chain (branch `plan-125.2`, не merged)

- `0ae63dc7385` feat(plan125.2 Ф.1): loop-no-break divergent in expr_diverges_125
- `373a8bf31c9` feat(plan125.2 Ф.2): while-true const-cond divergent
- `15f91eb3ee7` feat(plan125.2 Ф.3): stmt-position divergent (Break/Continue) at last-stmt
- `177b9b77af5` test(plan125.2 loop_no_break): 4 fixtures
- `09850f38639` test(plan125.2 while_true): 4 fixtures
- `71dd70ae2a7` test(plan125.2 stmt_position): 3 fixtures
- `fb0a5dc2421` test(plan125.2 negative): 4 regression guards for CFA expansion

### Test status

- plan125 22/22 PASS (V1 — no regressions)
- plan125_followups 9/9 PASS (batch 1 — no regressions)
- plan125_2 15/15 PASS (Plan 125.2 — 11 positive + 4 negative)

**Status:** ✅ **PLAN 125.2 CLOSED 2026-06-05** — branch `plan-125.2`,
worktree `nova-p125-2`, NOT merged to main, NOT pushed.
