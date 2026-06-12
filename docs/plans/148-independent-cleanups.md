# Plan 148 — Independent compiler cleanups

**Создан:** 2026-06-12 · **Ветка:** `plan-138.1` · **Worktree:** `D:\Sources\nv-lang\nova-p138`
**Приоритет:** P2 (самодостаточные backlog-маркеры, НЕ гейтят другие планы) · **Модель:** Opus 4.8, production-grade, без упрощений

## Цель (в двух словах)
Промоутнуть 4 самодостаточных backlog-маркера (не зависят от других модулей/планов) в реализованные фичи + doc-hygiene. Каждый — production-grade, с позитивными И негативными тестами (через **релизный** nova), spec/D/Q/doc-обновлением и критериями приёмки. Коммит на задачу.

## Порядок исполнения (escalating risk)
**Ф.1** parser → **Ф.2** parser → **Ф.3** checker → **Ф.4** codegen → **Ф.5** docs/close.
Исполняется **последовательно** (общий worktree + ветка + parser/mod.rs у Ф.1/Ф.2 → нельзя параллельно; последовательность также предотвращает D-номер коллизии).

## Изоляция / процесс (для ВСЕХ фаз)
- Worktree `D:\Sources\nv-lang\nova-p138`, ветка `plan-138.1`. Все Read/Edit/Write — абсолютные пути **только** под этой папкой. git через `git -C D:\Sources\nv-lang\nova-p138`. НЕ трогать `D:\Sources\nv-lang\nova` (main).
- **НЕ использовать `git stash`** — `.git` repo-global, рядом конкурентные worktree (stash collision → потеря изменений). Для baseline — temp-worktree / commit-reset.
- Сборка: `cd D:\Sources\nv-lang\nova-p138\compiler-codegen; cargo build --release` (~30s incr.). Зелёная перед коммитом.
- Тесты — **релизный** бинарь `compiler-codegen\target\release\nova-codegen.exe` (`test-build <FILE>` / `test-all --tests-dir nova_tests\<dir>`), env: `NOVA_GC_LIB_DIR`=`d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib`, `NOVA_GC_INCLUDE_DIR`=`…/include`.
- `git add` — только конкретные файлы (никогда `-A`/`.`); commit `-s`; БЕЗ `Co-Authored-By` trailer; перед commit — `git diff --cached --stat`.
- D-номер: следующий свободный (`grep "^## D[0-9]" spec/decisions/*.md | sort -V | tail`) или amend существующего. Q-блок — **только** при реальном открытом вопросе.
- Один коммит на задачу (код + тесты + spec/D + закрытие backlog-маркера). На провал — откатить свои правки (конкретные пути), дерево чистым, status=ERROR; НЕ коммить сломанное.

---

## Ф.1 — `[M-138-canonical-modifier-order]` (parser)
**Что:** единый канон порядка type-decl модификаторов = **`value priv`** (НЕ `priv value`). Order-independence сейчас намеренный (`plan124_8 modifier_order_independence_ok`) — противоречит «one canonical syntax» Nova.
**Реализация:** (a) parser детектит out-of-canon порядок → `E_MODIFIER_ORDER` + fix-it «переставь в `value priv`»; (b) обобщить на ВСЕ type-modifier'ы (фиксированный канонический порядок — определить полный набор модификаторов и их порядок); (c) флип `plan124_8 modifier_order_independence_ok` в negative; (d) мигрировать редкие out-of-canon в std/examples/tests; (e) D-блок (canonical-modifier-order / amend D124).
**Критерии приёмки:**
- A1: `type T priv value {...}` → `E_MODIFIER_ORDER` + fix-it.
- A2: `type T value priv {...}` → OK.
- A3: `plan124_8` order-independence-тест → negative (`EXPECT_COMPILE_ERROR`).
- A4: все out-of-canon в репо мигрированы; build зелёный.
- A5: pos+neg фикстуры `nova_tests/plan148/` (`mo_*`); D-блок записан; 0 регрессий.

## Ф.2 — `[M-138-unsafe-block-postfix-stmt]` (parser)
**Что:** постфикс (`.method()`/`[i]`/`.field`) должен цепляться к `unsafe {}` block-expr в **statement-позиции**: `unsafe { @data[i] }.display(sb)` без скобок. Сейчас leading `unsafe {}` в начале stmt = блок-заявление → нужны `(…)`.
**Реализация:** `parser/mod.rs::parse_stmt_or_expr` (~8650) — ветка leading-`unsafe` парсит полное выражение с постфиксом; обобщить на `if`/`match` block-expr в stmt-позиции. **Bare `{}` block остаётся statement** (не сломать). Cleanup: убрать скобки `vec_owned.nv:863/875`.
**Критерии приёмки:**
- A1: `unsafe { @data[i] }.display(sb)` в stmt-позиции парсится без скобок и работает.
- A2: bare `{}` в stmt-позиции остаётся statement.
- A3: `vec_owned.nv:863/875` — скобки убраны, build зелёный.
- A4: pos+neg фикстуры (`up_*`); 0 регрессий.

## Ф.3 — `[M-114.4-strict-partition]` (checker)
**Что:** ro-vs-const partition — добавить в checker `E_RO_FOR_CONSTEXPR_PREFER_CONST` (обратное к существующему `E_CONST_NOT_CONSTEXPR`; правило сейчас spec-only).
**Реализация:** изучить где живёт `E_CONST_NOT_CONSTEXPR` + правило partition (Plan 114.4, spec/decisions const/constexpr/ro); реализовать forward-направление ТОЧНО по правилу: ro-объявление в constexpr-контексте (compile-time constant) → подсказка предпочесть `const`.
**Критерии приёмки:**
- A1: ro-binding, используемый как constexpr где предпочтительнее const → `E_RO_FOR_CONSTEXPR_PREFER_CONST`.
- A2: корректный `const` — без ошибки; корректный runtime-`ro` — без ошибки.
- A3: pos+neg фикстуры (`rc_*`); spec partition-правило: spec-only → implemented; 0 регрессий.

## Ф.4 — `[M-codegen-unify-tuple-repr]` (codegen, крупная)
**Что:** унифицировать кортежи на on-demand mono'd **typed** C-структуры (реальные C-типы полей, эмит только использованных форм). Ретайр blanket all-int `_NovaTuple1..8` pre-decl + int-boxing не-int элементов + `(intptr_t)`-касты.
**Реализация:** ~15+ codegen-сайтов в `emit_c.rs` (build/field-read/pass/eq/debug). Каждая использованная комбинация типов → своя mangled C-структура, эмит при первом использовании (как mono'd generics). Связь D52/D123/D215/D232.
**Критерии приёмки:**
- A1: tuple с не-int полями (f64/str/record/bool) хранит реальные типы (нет int-boxing/`intptr_t`-каста).
- A2: эмитятся только реально использованные tuple-формы (on-demand), нет blanket `_NovaTuple1..8`.
- A3: tuple eq/debug корректны по типам; pos+neg фикстуры (`tr_*`).
- A4: **полная** регрессия зелёная (tuple широко используется); D-блок/amend записан.

## Ф.5 — docs + close
- README plans-index: проверить ведётся ли индекс; если да и 147/139.1/148 отсутствуют — добавить (NB: строки «Plan 147 NOT main» в README **не существует** — ошибка верификации, не искать).
- `project-creation.txt` + `simplifications.md` + `nova-private/discussion-log.md` (append, не переписывать) — записи по факту.
- `backlog-followups.md`: закрыть/обновить 4 маркера.
- Финальный статус-раздел этого плана.

---

## Статус
✅ **CLOSED Ф.1–Ф.5** (2026-06-12, branch `plan-138.1`, НЕ merged в main). Все 4 backlog-маркера промоутнуты в реализованные фичи production-grade; 0 новых регрессий vs baseline. Исполнено последовательно фоновым workflow.

Сводка по фазам (в порядке исполнения):

- **Ф.1** ✅ DONE (commit `98f0b48050f`) — `[M-138-canonical-modifier-order]`. Канон порядка type-decl модификаторов = `value priv`. Enforcement в `parser/mod.rs::parse_type_decl` modifier-loop: каждому модификатору присвоен canonical rank по scope (`value`=0 type-level representation → `consume`=1 type-level ownership → `priv`=2 field-default visibility); после парсинга ранги обязаны строго возрастать в source order, иначе `E_MODIFIER_ORDER` на merged modifier-region span + MachineApplicable fix-it (переписывает регион в rank-канон, напр. `priv value` → `value priv`). Обобщено на ВСЕ будущие type-модификаторы (новый модификатор = rank по scope, авто-входит в monotonicity-check); 0/1 модификатора всегда канон (проверка ≥2). `plan124_8/modifier_order_independence_ok.nv` флипнут positive→negative. Миграция: единственная out-of-canon type decl в корпусе. D241 (03-syntax.md) DECIDED→IMPLEMENTED с полными rank-rules + amend D124/D220 (02-types.md): stale «parser order-independent; W_NON_CANONICAL V2 lint» → hard error. Фикстуры `mo_*` (3 pos + 5 neg). A1✅ A2✅ A3✅ A4✅ A5✅. plan124_8 40/0, plan148 7/0; 0 регрессий (1 pre-existing CC-FAIL plan100_1 `consume_ok_record_field` — Vec-of-pointer C-mangling, идентичен на HEAD).
- **Ф.2** ✅ DONE (commit `2d24a38c288`) — `[M-138-unsafe-block-postfix-stmt]`. Расследование: парсер УЖЕ корректен (нет отдельной leading-`unsafe`/block-statement ветки; `parse_stmt_or_expr` `_`-arm зовёт полную `parse_expr`→`parse_postfix`), block-формы (`unsafe {}`/`if`/`match`/bare `{}`) в statement-позиции уже принимают постфикс (`.method()`/`[i]`/`.field`) без `(…)` — проверено value-discarded формой (true `Stmt::Expr`). Изменения parser-кода НЕ потребовалось. Работа: (a) cleanup лишних скобок `vec_owned.nv:862/874` (`(unsafe { @data[i] }).display/debug` → без скобок); (b) regression-guard фикстуры `up_unsafe_postfix_stmt_ok` (8 tests) + `up_bare_block_stmt_ok` (A2, 3 tests — bare `{}` остаётся statement); (c) D49 (03-syntax.md) amend — единая postfix-on-block-expr-in-stmt семантика + граница bare `{}`. A1✅ A2✅ A3✅ A4✅. plan148 9/0; 0 новых регрессий (vec_debug_pos + plan118 NPO — pre-existing, идентичны на HEAD).
- **Ф.3** ✅ DONE (commit `b6d3687108`) — `[M-114.4-strict-partition]`. Forward-направление partition `E_RO_FOR_CONSTEXPR_PREFER_CONST` реализовано в checker'е: `types/mod.rs::check_ro_module_partition` (зеркало `E_CONST_NOT_CONSTEXPR`, тот же предикат `check_const_constexpr_ex` — две оси не могут разойтись на «constexpr»), вызывается из `TypeCheckCtx::check_module` по новому `Item::Let`-арму; `known_consts`/`const_fn_names` хойстнуты перед циклом (shared обоими направлениями). Семантика: только module-level single-name `ro` (UPPER_CASE binder парсится как single-segment unit `Pattern::Variant` → имя извлекается из обоих `Pattern::Ident` И unit-variant), только полностью constexpr-eligible RHS; scope-local `ro`, destructuring, `ghost`, runtime-RHS — не затрагиваются (проверено `check`-probe'ами: arith/record/const-ref → fire; runtime-call/ro-ref/tuple-pattern/scope-local → silent). Фикстуры `rc_*`: `rc_ro_for_constexpr_neg` + `rc_ro_const_ref_neg` (NEG), `rc_const_ok` + `rc_runtime_ro_ok` (POS, scope-local silence). Spec: D199 amend + partition-table «spec-only → implemented» (03-syntax.md). A1✅ A2✅ A3✅. plan148 13/0; 0 регрессий. CAVEAT (документировано в D199 + out of scope): module-level RUNTIME `ro X = expr()` codegen — pre-existing unimplemented gap (binding не lowered, runnable POS невозможен; checker корректно принимает, verified via `check`); const-ref-const codegen-баг (`const_ref_const_ok` CC-FAIL) — pre-existing.
- **Ф.4** ✅ DONE (commit `982dfd90153`) — `[M-codegen-unify-tuple-repr]`. Кортежи унифицированы на on-demand mono'd typed структуры (D123 amend). (A1) concrete tuples (f64/str/record/bool) хранят реальные C-типы полей — без int-boxing/`(intptr_t)`-каста (уже был default через mono'd путь; подтверждено фикстурой `tr_tuple_typed_fields_ok`). (A2) blanket `_NovaTuple1..8` pre-decl РЕТАЙРНУТ → on-demand `register_legacy_tuple` + `/*__LEGACY_TUPLE_TYPEDEFS__*/` splice (idempotent `#ifndef`); на практике только arity 2 (erased HashMap/Set `(K,V)`). (A3) tuple eq/debug по типам корректны (per-element compare через `emit_field_eq`). КЛЮЧЕВОЙ ФИКС: field-read (`emit_expr` Member-digit) + `infer_expr_c_type` декодируют элемент-тип напрямую из mono'd struct name в `obj_ty` (`parse_mono_tuple_elements`), не только из per-Ident `tuple_element_types` side-table — чинит field access на fn-параметрах, call-result кортежах и вложенных `t.0.1` цепочках; закрыло 5 pre-existing CC-FAIL plan59 (f2/f10/f13/f15/f16). Новый код `[E_TUPLE_DESTRUCTURE_ARITY]` (3 codegen-сайта). (A4) полная регрессия зелёная — ~30 директорий сверены с baseline-бинарём (temp-worktree `nova-p148-base` @ `b6d3687`, удалён после): 0 новых FAIL, +5 fixed. Фикстуры `tr_*` (2 pos + 2 neg), plan148 17/0. SCOPE-NOTE (честно, не блокирует): legacy all-int `_NovaTuple2` не удалён полностью — erased-generic HashMap/Set lowers `(K,V)` через него; полное удаление требует mono'д HashMap/Set (большой отдельный effort). Остаток: mono'd-tuple-of-Vec forward-decl ordering (plan59 f5 / types arrays — pre-existing, отдельная ось); OOB tuple field index `t.5` на 2-tuple не reject'ится (pre-existing checker gap).
- **Ф.5** ✅ DONE (2026-06-12, этот commit) — docs + close. README plans-index: добавлены строки 139/139.1/147/148 (отсутствовали). `project-creation.txt` + `simplifications.md` + `nova-private/discussion-log.md` (append) — записи по факту. 4 backlog-маркера подтверждены closed в `backlog-followups.md`. Финальный статус-раздел (этот).
