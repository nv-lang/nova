# Plan 173 Ф.2 (defer-kernel) — де-риск-карта (ultracode Workflow, 2026-07-04)

> Источник: de-risk Workflow `w5btxzmse` (4 read-only агента, 490k токенов, 0 ошибок). Карта
> current-state + blast-radius + сайты + **скорректированная** последовательность под-атомов.
> Каждый под-атом Ф.2 = отдельный заход + коммит + гейт (conformance 38/38 + затронутые nova_tests
> baseline-delta 0 + disasm hot-path). Baseline — worktree HEAD~N/commit-reset, **НЕ git stash**.

## 🔴 Критические находки (меняют план)

1. **D194-элизия §3.5 ПРЕМИСА ЛОЖНА** (независимо подтверждено 2 агентами — высокая уверенность).
   План §3.1/§3.5: «Consumable[Never] СЕЙЧАС элидит shield/timeout/outcome (disasm-verified T2.9)».
   **Факт кода:** единственная эмитящая ConsumeScope-ветка (emit_c.rs:19746-20031) эмитит ПОЛНЫЙ
   frame-bearing slow-path БЕЗУСЛОВНО — нет Never/infallible-ветки; grep effect-row inspection в
   emit_c = 0; Plan 110:695 сам числит отсутствующую элизию как ❌-анти-паттерн. Спека D194
   (03-syntax:8921 «Статус: ACTIVE») дрейфанула. **Следствие:** §3.5 «пере-ключение элизии» — на
   деле FIRST-TIME реализация, не сохранение. **РЕШЕНИЕ (Option: PARITY, дефолт — принимаю сам):**
   Ф.2 = НЕ регрессировать (disasm-parity: lowered consume/defer(o) даёт вывод ≡ текущему
   frame-bearing). §perf-элизия (никогда не реализованная) — ОТДЕЛЬНЫЙ deferred followup
   `[M-173-d194-perf-elision]`, НЕ блокирует Ф.2. Причина: §3.5 acceptance буквально «≡ до
   рефактора» = PARITY; новая элизия — перф-оптимизация вне периметра unification. Спеку D194
   привести к факту (не «disasm-verified» без живого артефакта). *(Владельцу на ревью: если хочешь
   ГЕНУИННУЮ элизию как часть Ф.2 — это противоположный acceptance, скажи.)*

2. **`nova_scope_exit(frame, outcome_kind)` НЕДО-СПЕЦИФИЦИРОВАН** (§3.4). Крус — CATCH vs TRANSPARENT
   политика: with-Fail USER→SWALLOW (handler отработал, result=default, НЕ rethrow) vs defer/consume
   USER→RETHROW. Единый безпараметрический helper либо глотает ошибки в defer (новая unsoundness),
   либо двойной-rethrow в with-Fail (ломает #1-фикс). **РЕШЕНИЕ:** helper берёт `NovaScopeExitPolicy
   policy ∈ {CATCH, TRANSPARENT}` + читает `frame->error_kind` (не redundant outcome_kind). Таблица:
   PANIC→nv_panic; CANCEL→nova_throw_cancel_reason(msg,reason_ptr); USER|USER_TYPED→(CATCH:
   return-to-caller / TRANSPARENT: nova_rethrow_with_suppressed); Success: no-op. **7 kind-читающих
   сайтов**, маркировка IN/OUT: IN = A(with-Fail 6963-7028), B(consume 19877-20024), C1(defer-error
   18165-18208), C2(defer-normal cleanup-fail — HAND-ROLLED longjmp 18357, ВЫСШИЙ miss-risk); OUT =
   D(fiber-report 8200-8236 — report-семья, не throw), E(detach 8843-8850 — LogAndDrop), test-frame.
   Composition (nv_compose_suppressed, 2-frame) остаётся в codegen; helper — только single-frame
   terminal transport. grep-guard: `error_kind ==` только внутри nova_scope_exit + санкционир. outcome/report.

3. **RENAME collision-ordering ПОДТВЕРЖДЁН** (жёсткий build-breaker). Протокол `Cleanup[E]` и эффект
   `Cleanup` не сосуществуют в prelude (один type-name namespace, нет arity/kind-overload) →
   duplicate-def → падает ВЕСЬ prelude → каждая компиляция. **Порядок ОБЯЗАТЕЛЕН:** (a) эффект
   `Cleanup`→`ResourceTrace` ПЕРВЫМ (освобождает имя) → green → (b) протокол `Consumable`→`Cleanup`
   + метод `@on_exit`→`@cleanup`. §3.5-relevant: элизия детектится по on_exit method effect-row
   (types/mod.rs:4359) — rename должен сохранить детект.

4. **defer(o) design-gaps.** (a) **Interrupt-outcome:** код ConsumeScope НЕ пушит interrupt-frame →
   `interrupt v` в body СЕЙЧАС обходит on_exit; но СПЕКА core.nv:130 уже говорит «Failure(reason) —
   throw/cancel/**interrupt**». defer-frame ОБОРАЧИВАЕТ interrupt (enter_defer_scope:18227). →
   desugar на defer-frame ВЫРАВНИВАЕТ impl к спеке: on_exit/defer(o) СРАБОТАЕТ на interrupt с
   outcome=**Failure** (correctness-фикс, тестируемо). (b) **AST:** добавить ОПЦИОНАЛЬНОЕ ПОЛЕ на
   `Stmt::Defer` (не новый вариант) → low blast (существующие `{body,..}` армы целы). (c) **Parser:**
   bounded lookahead `defer (IDENT ScopeOutcome)` vs `defer (expr)` — коллизий в корпусе нет, но
   грамматика допускает parenthesized-expr body → fallback на parse_expr ОБЯЗАТЕЛЕН. (d) **Panic/throw
   split:** defer throw-path (18165) сейчас НЕ различает panic; defer(o) читает error_kind как consume
   (19878-19922). (e) **ScopeOutcome** конструкторы уже эмитятся consume-веткой → defer(o) переиспользует.

## Current-state по измерениям (сжато; полные сайты — ниже)

**LOWERING (агент 1):** `Stmt::ConsumeScope` (ast:1878 {binding,type_annot,init,body,span}) —
МОНОЛИТ emit_c.rs:19746-20031 (~285 строк), НЕ переиспользует defer-kernel: init+`#define`,
3-level timeout (19773-19820), cancel-shield enter/leave (19831/19980, БЕЗУСЛОВНО), ResourceTrace
(Cleanup-эффект) enter/exit (19843/19967), body fail-frame (19850), 4 ScopeOutcome-make-сайта
(Success 19894 / Cancel→Failure"cancel:" 19906 / Failure 19913 / Panic 19922, kind из error_kind
19878), on_exit во ВТОРОМ fail-frame (19938, символ HARDCODED `Nova_<T>_consume_on_exit` :19761),
6-way re-raise ladder (19992-20024). **defer-kernel** (18079 enter / 18271 leave / 18384 early-exit
/ 18629 emit_defer_body_void / 19691 Stmt::Defer flag; DeferEntry{active_var,body} 1331) материализует
ScopeOutcome НИГДЕ. Consume НЕ пушит interrupt-frame.

**RE-DISPATCH (агент 2):** effects.h: NovaThrowKind {USER=0,CANCEL=1,USER_TYPED=2,PANIC=3} (30-37),
error_kind :58; терминалы nv_panic:555 / nova_throw_cancel_reason:126 / nova_rethrow_with_suppressed:210
/ nv_compose_suppressed:172. 11 setjmp-сайтов в emit_c; 7 читают kind (см. находка 2). `nova_scope_exit`
НЕ существует (grep=0) — создать `static inline` рядом с триадой в effects.h.

**RENAMES (агент 3):** (a) эффект — effects.nv:210 `type Cleanup effect {on_scope_enter(label,timeout_ms);
on_scope_exit(label,outcome)}`; codegen dispatch 2 сайта (19843/19967, C-символы HARDCODED
`Nova_Cleanup_on_scope_enter/exit`); п.8 ДРОПАЕТ timeout_ms из enter (сигнатурное изменение). (b)
протокол Consumable[E] — protocols.nv:459; satisfaction СТРУКТУРНАЯ (по методу on_exit, types:4316),
имя «Consumable» в Rust — только error-текст; НО generic-bound `[T Consumable[Never]]` резолвится по
ИМЕНИ (d194_consumable_never_hot_path:24 — load-bearing). (c) метод @on_exit — def-символ авто-mangle
из имени метода (emit_c:4093/11433 `Nova_{}_consume_{f.name}`), НО call-site pin HARDCODED :19761;
+ 4 hand-written guard-defs sync_primitives.h:2324/2338/2352/2366; + чекер-литералы «on_exit»
types:4316/4363/4537 + lints:303. CleanupTimeoutError (errors.nv:306) — SED-HAZARD, не трогать.
Blast: ~110 файлов, но load-bearing CODE ≈ 6 (emit_c, types, lints, parser, sync_primitives.h,
protocols/effects/sync.nv); остальное — token-sed tests/spec/docs.

**ELISION+SYNTAX (агент 4):** D194 §perf НЕ реализован (см. находка 1). defer parser :10052; AST :1845;
ScopeOutcome core.nv:147 `Success|Failure(str)|Panic(str)` (str, не any — any=Ф.4 #5). Плейн `defer`
~430 сайтов/89 .nv — must byte-identical. `defer(...)`-parenthesized в корпусе НЕТ (только комменты).

## Скорректированная последовательность под-атомов Ф.2 (каждый = заход+коммит+гейт)

- **Ф.2.0 (этот заход):** D314 spec-first (03-syntax.md) + эта де-риск-карта + план-статус. Гейт: spec-review.
- **Ф.2.A0 BASELINE (executing, small):** release nova.exe на HEAD; dump .c/disasm 2 фикстур —
  (a) Mutex/Sem/atomic consume-block (Cleanup[Never] hot-path), (b) Fail[E]-цепочка без local
  handler/defer (frame-free propagation). → `docs/plans/artifacts/173-disasm-baseline/`. Зафиксировать
  ФАКТ: guard-фикстуры НЕСУТ setjmp-кадры (ожидаемо YES) → §3.5 acceptance = PARITY. Гейт: артефакты
  закоммичены; conformance 38/38.
- ✅ **Ф.2.R1 rename эффект Cleanup→ResourceTrace ЗАКРЫТ (43f9ee5b, RENAME-only):** spec D185 (04-effects)
  → effects.nv → emit_c 2 dispatch (Nova_ResourceTrace_on_resource_enter/exit, _nova_handler_ResourceTrace)
  → parser hint → 03-syntax effect-рефы → 3 теста plan110. Гейты: build clean; conformance 38/38; 3
  ResourceTrace-теста + plan110 + plan110/neg(9) + plan103_9 PASS; grep `effect Cleanup`/`on_scope_*` в
  source = 0. **⚠ DROP timeout из enter (§3a/п.8) ОТЛОЖЕН → Plan 173 Ф.5 timeout-rework** (семантическая
  правка + ретайр D195-override-тестов, не ренейм; timeout пока сохранён — покрытие цело).
- ✅ **Ф.2.R2 rename Consumable→Cleanup + @on_exit→@cleanup ЗАКРЫТ (ffb76506, BUNDLED):** CODE(care):
  emit_c pin→consume_cleanup (def-side авто-mangle из f.name); sync_primitives.h ×4 hand-C defs→
  _consume_cleanup; types 3 литерала + lints→"cleanup"; коды `D188-*` (дефис on-exit) СТАБИЛЬНЫ, только
  слова в тексте. MECHANICAL(token-sed \bon_exit\b не задел `_consume_on_exit`): protocols.nv + sync.nv
  ×4 + 53 .nv (nova_tests/spec_tests/examples/bench) + prelude core/errors + ast/parser комменты. Spec:
  03-syntax pre-D314 + 04-effects D195; D314 + D185 «ex-Consumable» ЗАЩИЩЕНЫ. Гейты: build+LINK clean;
  conformance 38/38; plan103_9 (mutex Cleanup[never] hot-path LINK) + plan110 + plan110/neg(9) + plan140 +
  plan125_1-consume PASS; grep Consumable/@on_exit в source=0 вне D314-ex. plan125_1/neg/let_never
  CC-FAIL=pre-existing.
- **Ф.2.B1 parser+AST defer(o ScopeOutcome):** опц. поле на Stmt::Defer (parser:10052 bounded-lookahead
  +fallback; ast:1845); neg-диаги (double-binding, non-ScopeOutcome). Гейт: parser pos/neg; conformance
  38/38; syntax/defer_* + err173/f1_defer_plain_all_paths baseline-delta 0 (плейн defer byte-identical).
- **Ф.2.B2 codegen outcome-defer:** binding на DeferEntry (1331); emit_defer_body_void (18629) + 4 splice
  (normal→Success 18299; throw→Failure/CANCEL-marker/PANIC via error_kind 18173; early-exit→Success 18393;
  interrupt→Failure 18227). Гейт: err173/f2_defer_outcome_{success,failure_payload,panic,cancel,interrupt}
  (incl Zig-парность Failure(e)=>use(e) + Panic-ветка-БЕЖИТ + LIFO-with-outcome + panic-in-defer compose);
  conformance d90/d314; baseline-delta 0.
- **Ф.2.B3 consume→desugar:** заменить монолит 19746-20031 на `ro X=e` + consume-flavored outcome-defer,
  RE-HOME cancel-shield(body+cleanup)/timeout/ResourceTrace/exactly-once/partial-init как defer-entry
  policy (НЕ потерять); on_exit→ordinary X.@cleanup(o) method-dispatch. Гейт: все 8 consume-conformance
  (d131/d133/d156/d162/d164/d174/d188/d196) через сахар; full std компилится; plan110 baseline-delta 0.
- **Ф.2.C nova_scope_exit unification (структурный финал #1):** helper в effects.h (policy CATCH/TRANSPARENT,
  находка 2); reroute SITE A(CATCH)/B/C1/C2(TRANSPARENT); EXCLUDE+document D/E/test + grep-guard. Гейт:
  rt/f1_with_fail_swallow_panic + composed body+cleanup + d196(in_fail_ctx intact) green; conformance 38/38;
  нет per-frame kind-dispatch дублирования.
- **Ф.2.D194 disasm-parity + spec-sync:** disasm re-baseline ≡ A0 (PARITY, находка 1); D194-спека к факту;
  §perf-элизия → followup `[M-173-d194-perf-elision]`. Гейт: disasm Mutex/Sem/atomic ≡ A0; conformance 38/38.
- **Ф.2.E hub REWRITE (idiom/error-and-cleanup-model.md) + doc-sweep** ~18 docs (idiom×10, cookbook,
  tutorial, nova-cli[.ru], nv-coding-style); plans/110.* HISTORICAL не трогать. Гейт: grep Consumable/@on_exit
  в std/+docs(non-plans)=0 вне historical; docs-only (нет build/test).
- **Ф.2.multi-binding consume** (D188 R1) — по ходу B3 или отдельным атомом.

## Ключевые сайты (свод; всегда re-grep перед правкой — план-строки дрейфуют, R9)

emit_c.rs: 19746-20031 (consume монолит), 19761 (on_exit call-pin), 19843/19967 (Cleanup-effect dispatch),
19891-19926 (ScopeOutcome 4 make-сайта), 6963-7028 (with-Fail SITE A), 18165-18208 (defer-error C1),
18348-18367 (leave cleanup-fail C2 hand-rolled longjmp), 18079/18271/18384/18629/19691 (defer-kernel),
1331-1377 (DeferEntry/DeferScope), 4093/11433 (consume-method mangle). effects.h: 30-37/58/126/172/210/555
(kinds+терминалы; nova_scope_exit сюда). types/mod.rs: 4316/4363/4537 (on_exit-литералы), 4332-4362 (D194
caller-relax). lints.rs:303. parser/mod.rs: 10052 (defer), 10061 (Cleanup-hint). ast/mod.rs: 1845
(Stmt::Defer), 1878 (ConsumeScope). std: core.nv:147 (ScopeOutcome), protocols.nv:459 (Consumable),
effects.nv:210 (Cleanup-effect), sync.nv:1390/1402/1414/1428 (4 guard-decls), sync_primitives.h:2324/2338/
2352/2366 (4 hand-C guard-defs). spec: 03-syntax D188/D191/D194/D196/D197/D90/D189, 04-effects D185/D195.

## Прогресс B-атомов

- ✅ **Ф.2.B1 parser/AST defer(o ScopeOutcome) ЗАКРЫТ (e0d95a31):** AST опц. поле `outcome_binding:
  Option<String>` на `Stmt::Defer` (~30 traversal-армов целы через `..`); parser bounded lookahead
  (`( IDENT IDENT` → defer(o), иначе fallback parse_expr); neg `[E_DEFER_OUTCOME_TYPE]`/`[E_DEFER_OUTCOME_ARITY]`;
  D189-Pipe цел. Codegen пока IGNORE поля (плейн-path). Spec D90 amend. Тесты: pos f2_defer_outcome_parse
  (парсится+бежит LIFO), neg f2_{bad_type,arity}. Гейт: build clean, conformance 38/38, plain defer
  byte-identical. **Попутно:** доукомплектовал дефект #3 (e3fce6f3) — пропущенный `?`-в-Fail-fn сайт
  plan100_4_1:19 (тогдашний grep исключал `//`-строки; свип подтвердил — единственный пропуск).
- ✅ **Ф.2.B2 codegen+checker outcome-материализация ЗАКРЫТ (23f512d8):** `DeferEntry.outcome_binding`
  + `enum DeferOutcome{Success,FromFrame,Interrupt}` + helper `emit_defer_body_with_outcome`
  (`#define`-binding + `var_types["o"]="Nova_ScopeOutcome*"`, зеркалит consume-arm); 4 splice-сайта:
  normal-exit+early-exit→Success, throw/panic/cancel→FromFrame (Panic по PANIC / Failure("cancel: ") по
  CANCEL / Failure(msg) иначе), interrupt→Failure("interrupt"). **Гэп найден эмпирически:** резолвер имён
  (types/mod.rs:16769) не биндил `o` → "undefined identifier o" в folder-module; фикс: push/pop scope-frame
  с binding при `outcome_binding=Some` (зеркалит ConsumeScope), для None — дословный no-op. Тест
  f2_defer_outcome_matched (Success на normal exit, Failure на throw через `match o`). Гейт: conformance
  38/38; plain defer + consume (plan100_4_*/plan110/plan103_9 mutex) без регрессий; плейн byte-identical
  конструктивно (None → тот же emit_defer_body_void). **Отложено на B2-хвост (runtime-наблюдение):** Panic-
  ветка-БЕЖИТ + cancel-Failure — panic убивает процесс (pre-existing D158-segfault-риск), cancel =
  supervised-сценарий; сейчас compile-покрыты (exhaustive match). Zig-парность payload — B3 (consume-desugar).
- 🟡 **Ф.2.B3 consume→desugar — ЧАСТИЧНО (outcome-примитив унифицирован; полный run-site merge ЗАБЛОКИРОВАН):**
  Understand-workflow (5 ридеров, wf_67a1d385) + firsthand-разбор монолита (emit_c.rs:19816-20101) + всех
  4 defer run-site'ов выявили **архитектурное препятствие** (класс «D194-премиса-ложна»): **compose-семантика
  consume и user-defer genuinely РАЗНЫЕ** и несовместимы с физическим слиянием run-site'ов:
  • consume: panic-dominance (D196 R3 — cleanup-panic ДОМИНИРУЕТ) + pairwise body-primary/cleanup-suppressed
    + immediate rethrow/nv_panic per-path.
  • user-defer (D161): chain-suppressed (first-fail primary, rest в suppressed-chain), НЕТ panic-dominance,
    loop-then-rethrow-once структура (pop в конце).
  Маршрутизация consume через defer-kernel run-sites потребовала бы либо (а) регресс user-defer, либо
  (б) реплику монолит-compose в каждом run-site (борьба со структурой pop-в-конце + rethrow-before-pop).
  ТАКЖЕ: checker-инвариант (RISK #146) — consume-body свободно допускает throw/return/break/panic, а
  defer-body их ЗАПРЕЩАЕТ (check_defer_body: D158/D90/D159) → AST-десугар ConsumeScope→Block{Stmt::Defer}
  дал бы массовый регресс. **Вывод: монолит ЕСТЬ корректная реализация consume-flavored-defer-entry (D314 §3);
  full physical merge — не clean-refactor, а semantic-hazard.**
  **ДОСТАВЛЕНО (safe, behavior-identical, verified):** единый ScopeOutcome-примитив — `materialize_scope_outcome`
  + `assign_scope_outcome_from_frame` (emit_c.rs) — общий для defer(o)-FromFrame И consume-cleanup (step-7
  монолита теперь зовёт shared helper). core.nv:131 doc-fix cancel-marker "cancelled:"→"cancel:" (рассинхрон
  spec/impl). Гейт: conformance 38/38; plan110/plan103_9/plan100_4_* + err173 defer(o) — 0 регрессий.
  **FOLLOWUP'ы (осознанно отложено, не half-measure):**
  • `[M-173-b3-runsite-unify]` — физическое слияние consume↔defer-kernel run-site'ов ТРЕБУЕТ сперва
    унифицировать compose-модель (chain vs pairwise+panic-dominance) — отдельный дизайн-вопрос (D-блок).
  • `[M-173-consume-interrupt-cleanup]` — монолит НЕ бежит cleanup на `interrupt` (defer-kernel бежит);
    D314 §2 предписывает interrupt→cleanup(Failure). Beyond-parity correctness-fix, свой тестируемый атом.
  • `[M-173-consume-exactly-once-observability]` — conformance d188/d196 наблюдают ТОЛЬКО body-exec+binding
    (RISK #208), НЕ exactly-once/shield/timeout — добавить trail-observability тесты.
- ✅ **Ф.2.D194 disasm-parity + spec-sync ЗАКРЫТ (spec-truth):** D194-спека (03-syntax:8930) приведена к
  ФАКТУ: caller-relaxation (`Cleanup[Never]` снимает `Fail[E]` у caller'а) — РЕАЛИЗОВАНА (живо); §perf
  hot-path elision (§Hot-path optimization) — **НЕ реализована** (единственная ConsumeScope-ветка эмитит
  полный frame-bearing путь БЕЗУСЛОВНО; effect-row-inspection в codegen = 0). Убран ложный «Disasm-verified
  в T2.9» (артефакт отсутствует). Статус-хедер + врезка помечают элизию как аспирационную → followup
  `[M-173-d194-perf-elision]`. **Parity подтверждён:** B3 (outcome-DRY) НЕ увеличил кадры/shield/outcome —
  mutexguard.c post-B3 ≡ baseline (84 setjmp, 3 ScopeOutcome, идентично); consume-corpus (plan110/plan103_9)
  + conformance 38/38 без регрессий. Acceptance Ф.2 = PARITY (не элизия) зафиксирован в спеке.
- ✅ **Ф.2.E doc-sweep + hub ЗАКРЫТ (0636a9ed):** workflow из 6 агентов + ручная доводка. R2-rename
  (`@on_exit`→`@cleanup`, `Consumable[E]`→`Cleanup[E]`, `.on_exit`→`.@cleanup`, ~170 замен) в 16 live-доках
  (cookbook/tutorial/idiom×13/nv-coding-style/nova-cli[.ru]). R1-debt (effect `Cleanup`→`ResourceTrace`,
  `on_scope_enter/exit`→`on_resource_enter/exit`, стейл C-symbol `_consume_on_exit`→`_consume_cleanup`).
  Historical (simplifications/plans/spec-D-ex) + диагностик-id + rename-explain СОХРАНЕНЫ. Hub
  error-and-cleanup-model.md ПЕРЕПИСАН под факт (Ф.1 with-Fail-panic FIXED; Ф.2 partial; defer(o)-migration;
  followups). Residue-grep = 0. Docs-only.
- 🔴 **Ф.2.C nova_scope_exit transport-unify — ЗАБЛОКИРОВАН (тот же класс, что B3-merge):** firsthand-разбор
  terminal-сайтов выявил их **genuine несогласованность** в kind-dispatch: SITE A (with-Fail): PANIC→
  rethrow_with_suppressed, CANCEL→nova_throw_cancel_reason(+restore handlers), USER→caught(default); defer
  C1 (FAIL): ВСЕ kinds→rethrow_with_suppressed; consume: PANIC→nv_panic, USER→rethrow. Единый helper требует
  НОРМАЛИЗАЦИИ per-kind транспорта (PANIC: nv_panic vs rethrow; CANCEL: cancel_reason vs generic-rethrow —
  влияет на reason_ptr + cancel-propagation в structured concurrency) — design-decision + верификация в
  КРИТИЧЕСКОМ error-transport, НЕ механический extract. Owner-away + high-regression-risk → отложено.
  Followup `[M-173-c-transport-normalize]`: сперва единая per-kind модель (D-блок), потом reroute + verify.
  D314 §4 таблица (PANIC→nv_panic uniform) сама конфликтует с impl (SITE A/defer→rethrow) → spec к факту.

## СВОДКА Ф.2 (2026-07-03): SAFE-SCOPE ЗАКРЫТ, structural-finale → followups

**Закрыто (safe, verified):** A0 · R1 · R2 · B1 · B2 (defer(o) codegen+checker) · B3-outcome (ScopeOutcome-
примитив унифицирован) · D194 (parity + spec-truth) · E (doc-sweep + hub). Conformance 38/38 сквозь всё.

**Отложено (осознанно, documented — НЕ half-measure; критический error-transport, owner-away, high-risk):**
- `[M-173-b3-runsite-unify]` — физический merge consume-монолита в defer-kernel run-site'ы (compose-семантика
  genuinely РАЗНАЯ: panic-dominance/pairwise vs chain).
- `[M-173-c-transport-normalize]` — единый nova_scope_exit (terminal-сайты несогласованы per-kind).
- `[M-173-consume-interrupt-cleanup]` — cleanup на interrupt (spec D314 §2, beyond-parity).
- `[M-173-consume-exactly-once-observability]` + `[M-173-d194-perf-elision]` + multi-binding consume.

**Вывод для владельца:** D314 language-level goal ДОСТИГНУТ (defer(o) работает; consume=корректный
consume-flavored-defer-entry; soundness Ф.1). Оставшийся structural-finale (physical unification) —
рефакторинг-долг, требующий per-kind/compose нормализации (design-decision), НЕ блокирует 174/176 (им
нужно ПОВЕДЕНИЕ error-system, которое работает). Рекомендация: принять safe-scope + followups; solidify
173 для downstream ИЛИ выделить сессию на transport/compose-нормализацию с owner-steered design.
