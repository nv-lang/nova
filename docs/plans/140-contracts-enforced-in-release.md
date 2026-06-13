<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 140 — Contracts enforced in release (Z3-proven elided, unproven checked)

> **Создан:** 2026-06-10.  **Статус:** ✅ CLOSED Ф.0-Ф.5 (2026-06-12, branch `plan-140`, НЕ merged).
> **Эстимат:** ~2-3 dev-day.  **Model:** Opus + Thinking ON.
> **Зависит от:** Plan 33.1 Ф.4 / D24 (contracts foundation), Plan 45/D106 (verify pipeline),
> Z3 backend (`--features z3-backend`, vcpkg libz3).
> **Разблокирует:** bounds-check-as-contract на Vec `@index` (Plan 138.x/139).

---

## Проблема

Контракты `requires`/`ensures` (D24) сейчас **стираются в release**. Доказательство:
`compiler-codegen/src/test_runner.rs:829-843` — `Mode::Dev` передаёт
`-DNOVA_CONTRACTS_RUNTIME=1` (проверки включены), `Mode::Release` — НЕТ, с явным комментом
«**В release контракты стираются (zero-cost)**» (модель C `assert`/`NDEBUG`).

**Дыра:** распруфленность контракта (Z3) учитывается ОТДЕЛЬНО от release-стирания.
В release контракт стирается **независимо от того, доказан он или нет**. Итог:
- где Z3 **доказал** — проверка не нужна (и так safe) — ОК;
- где Z3 **не смог доказать** — в release страховка **молча выключается** → потенциальный
  silent UB / corruption ровно там, где статически безопасность НЕ подтверждена.

Это худший выбор: защита снимается именно в недоказанных (= рискованных) местах.

---

## Решение — «enforce with static elision»

| Контракт | Release-поведение | Стоимость |
|---|---|---|
| Z3-**доказан** (`proven_contracts`) | **элидируется** (не эмитится на codegen) | zero |
| Z3 **не доказал** | runtime-проверка **остаётся** → `nova_contract_violation` (fail-fast abort) | проверка |
| `#unchecked` (явный per-fn opt-out) ИЛИ build-flag | элидируется под ответственность разработчика | zero |

Модель Dafny/Verus + runtime-fallback; ровно как bounds-checks в Rust (оптимизатор снимает
доказуемые, прочие проверяются всегда). «Always-on» ≠ «always pay» — платишь только за недоказанное.

**Graceful degrade без Z3:** если собрано без `--features z3-backend`, `proven` пуст → все
контракты недоказаны → все проверяются в release (safe, медленнее). С Z3 — доказанные элидируются.

---

## Архитектура (verified anchors)

| Что | Где | Деталь |
|---|---|---|
| proven set | emit_c.rs:468 | `proven_contracts: HashSet<(fn_name, span.start)>` |
| populate | emit_c.rs:1319 `set_proven_contracts` ← main.rs:470 ← types/mod.rs:712 `env.proven_contracts = report.proven` (VerificationPipeline) |
| codegen elision (proven) | emit_c.rs:13595, 13966 | `if proven_contracts.contains((fn,span)) { continue }` — proven НЕ эмитится (zero-cost даже в debug) |
| release gate (unproven) | emit_c.rs:13588/13947/13960/16003/16021/17423 | `#ifdef NOVA_CONTRACTS_RUNTIME` — оборачивает только недоказанные |
| flag wiring | test_runner.rs:836 (Dev: `-DNOVA_CONTRACTS_RUNTIME=1`), :838-843 (Release: отсутствует) |
| violation rt | nova_rt/contracts.h | `nova_contract_violation(PRE/POST, fn, src, file, line)` структурное сообщение |
| Z3 backend | verify/crosscheck.rs:251-254 | `CrossCheckBackend` `#[cfg(feature = "z3-backend")]` |
| verify mode | parser/mod.rs:2748 | `verify_mode` / `verify_timeout_ms` per-fn (D106) |
| quantified skip | emit_c.rs:13599 | `forall`/`exists` не runtime-checkable — пропускаются (verify-only) |

**Ключевое наблюдение:** proven/unproven распознаётся уже на codegen-уровне (`proven_contracts`
→ `continue`). `#ifdef` оборачивает ТОЛЬКО недоказанные. Значит фикс минимален: эмитить
недоказанные проверки и в release (доказанные и так не эмитятся).

---

## Фазы

### Ф.0 — Spec D24 amend + Q-block (~0.5d)

- **D24 amend**: контракты — НЕ debug-only. Release-eval = «enforce-with-elision»:
  Z3-proven элидируются; unproven — runtime-checked (abort на violation); `#unchecked` / build-opt-out
  снимает проверки явно. (Прежняя формулировка «в release стираются» — retract.)
- **Q-block** (open-questions): default build-policy (предлагается enforce-by-default + opt-out);
  поведение violation в release (abort vs panic-unwind); гранулярность opt-out (per-fn / per-module / build).

**Commit:** `spec(plan140 D24 amend): contracts enforced in release (proven-elided, unproven-checked)`

### Ф.1 — Codegen: unproven-checks в release (~0.5d)

- Эмитить недоказанные контракт-проверки в release. Минимальный путь: убрать
  `#ifdef NOVA_CONTRACTS_RUNTIME` обёртку для **недоказанных** (proven уже `continue`'ятся),
  ИЛИ инвертировать флаг в opt-out (default ON). Рекомендуется: эмитить безусловно
  (proven elision на codegen уже даёт zero-cost), флаг → opt-out (Ф.2).
- `test_runner.rs Mode::Release`: убрать «стирание» — release чекает недоказанное.
- Verify `nova_contract_violation` release-safe (abort/flush под `-DNDEBUG`/LTO).

**Commit:** `feat(plan140 Ф.1): emit unproven contract checks in release builds`

### Ф.2 — `#unchecked` opt-out + build policy (~0.5d)

- Per-fn `#unchecked` (или `#[contracts(off)]`) — элидирует даже недоказанные для hot-path.
  Parser + AST flag + codegen skip.
- Build-level: `nova build --contracts=enforce|off` (default enforce). `off` восстанавливает
  legacy zero-cost целиком.

**Commit:** `feat(plan140 Ф.2): #unchecked opt-out + --contracts build policy`

### Ф.3 — Verify pipeline на build-пути (~0.5d)

- Убедиться, что `VerificationPipeline` прогоняется и `proven_contracts` заполняется на build-пути
  (`nova build`/`nova test` release), не только в `nova doc`/verify. Иначе proven пуст → всё чекается.
- Z3 (`--features z3-backend`, vcpkg libz3 готов — project-z3-backend-ready) — задокументировать
  build с Z3 для элидирования. Без Z3 — safe degrade.

**Commit:** `feat(plan140 Ф.3): run verification pipeline on build path for proven-elision`

### Ф.4 — Тесты (release-build!) (~0.5d)

`nova_tests/plan140/` — собирать в **release** (проверяем именно release-поведение):
- `t1_unproven_requires_aborts` — недоказуемый `requires` нарушен → abort (should-abort).
- `t2_proven_requires_elided` — доказуемый `requires` (Z3 build) → проверки нет (inspect .c / perf).
- `t3_ensures_release` — `ensures` нарушен → abort.
- `t4_unchecked_optout` — `#unchecked` fn → проверка снята даже при нарушении (no abort).
- `t5_build_policy_off` — `--contracts=off` → все сняты (legacy).
- `t6_no_z3_degrade` — без Z3 все чекаются, abort на violation.

**Commit:** `test(plan140 Ф.4): release-build contract enforcement fixtures`

### Ф.5 — Perf + docs/close (~0.5d)

- Perf: contract-heavy fixture в release с Z3 (proven-элидировано) vs без Z3 (всё чекается) vs `off`.
  Подтвердить, что proven-элидирование zero-cost; задокументировать overhead.
- D24 amend финал; simplifications.md; nova-private/{project-creation.txt,discussion-log.md}; README; memory.

**Commit:** `docs(plan140 Ф.5 D24): contracts-in-release complete + perf notes`

---

## Связь: разблокирует bounds-as-contract

После Plan 140 `@index(i) requires 0 <= i < @len` становится валидной заменой ручного panic:
- `for i in 0..v.len() { v[i] }` → Z3 доказывает → проверка элидируется (fast);
- индекс из неизвестного источника → недоказан → runtime abort в release (safe).
Rust-модель bounds-check. Применение к Vec — followup Plan 138.x/139 (`[M-140-bounds-as-contract]`), gated на 140.

---

## Risk register

| # | Риск | Sev | Mitigation |
|---|---|---|---|
| R1 | Без Z3 все контракты чекаются → release perf-overhead | 🟡 MED | `--contracts=off` / `#unchecked`; Z3 build; Ф.5 измеряет |
| R2 | Behavior change: release-программы с латентным violation теперь **abort'ят** | 🟡 MED | это цель (fail-fast vs silent UB); задокументировать как breaking + migration note |
| R3 | `nova_contract_violation` release-safety (abort/flush под NDEBUG/LTO) | 🟢 LOW | Ф.1 verify |
| R4 | VerificationPipeline не прогоняется на build-пути → proven пуст → всё чекается | 🟡 MED | Ф.3 wire; degrade безопасен (медленнее, не unsafe) |
| R5 | Quantified (`forall`/`exists`) не runtime-checkable | 🟢 LOW | уже пропускаются (emit_c.rs:13599); verify-only |

---

## Acceptance criteria

- **F1** — release-сборка чекает **недоказанные** `requires`/`ensures` (abort на violation).
- **F2** — Z3-**proven** контракты в release **элидированы** (zero-cost; verified via .c / perf).
- **F3** — `#unchecked` (per-fn) снимает проверки даже у недоказанных.
- **F4** — `nova build --contracts=off` восстанавливает legacy zero-cost.
- **F5** — без `--features z3-backend`: все контракты чекаются в release (safe degrade), abort на violation.
- **F6** — D24 amended: release-eval = enforce-with-elision; «стираются в release» retracted.
- **F7** — 0 регрессий в существующих contract-тестах (debug + release).

---

## Followups

- `[M-140-bounds-as-contract]` — **READY (ungated 2026-06-12 Ф.5).** Vec `@index` OOB как
  `requires 0 <= i < @len`. Был gated на Plan 140 (нужна была enforce-in-release семантика, чтобы
  `requires`-форма реально страховала в release). Plan 140 закрыт → enforce-with-elision активна:
  доказуемый индекс (`for i in 0..v.len()`) элидируется (fast), недоказанный → runtime abort (safe) —
  ровно Rust bounds-check. Реализация на Vec — Plan 138.x/139.
- `[M-140-invariant-release]` — **N/A на момент закрытия (нет type-invariant фичи).** Type invariants
  как отдельная конструкция в языке ещё не существуют (есть только record-invariant в контракт-codegen,
  который УЖЕ покрыт enforce-in-release — см. Ф.1: record-invariant `#ifdef` снят наравне с requires/ensures).
  Когда/если появятся декларативные type invariants — они автоматически наследуют ту же enforce-with-elision
  семантику (общий codegen-путь). Маркер оставлен как напоминание-якорь, не блокер.
- `[M-140-contract-levels]` — **OPEN (deferred, не блокер).** Per-module opt-out
  (`#unchecked_module` / манифест) + Eiffel-style раздельная гранулярность (pre / post / invariant
  по отдельности). V1 (Ф.2) даёт per-fn `#unchecked` + build `--contracts=off` — достаточно для hot-path.
  Per-module/per-clause — по запросу. Home: backlog (Q34 §3 cross-ref).
- `[M-140-contract-panic-unwind]` — **OPEN (deferred, не блокер).** Release violation как
  panic-unwind (recoverable, ловится supervised-scope/`okrescue`) вместо текущего `abort` (Ф.1).
  Default = abort (контракт-violation = баг программы, fail-fast). Делать если понадобится graceful
  degrade сервиса. Home: backlog (Q34 §2 cross-ref).
- `[M-140-contract-message]` — **OPEN (pre-existing, ортогонален).** Опциональное
  `, "message"` на `requires`/`ensures`/`invariant`. Home: backlog (Planned-указатель).

---

## Связанные D-блоки

| D | Что |
|---|---|
| D24 | AMEND — contracts enforced in release (proven-elided, unproven-checked); «стираются» retracted |
| D106 | verify_status / verify_mode — proven feeds release elision |

---

## СТАТУС — ✅ CLOSED Ф.0-Ф.5 (2026-06-12, branch `plan-140`, НЕ merged)

| Фаза | Что сделано | Commit |
|---|---|---|
| Ф.0 spec+Q | D24 amend (release-eval = enforce-with-elision; «стираются» RETRACT) + новый Q34 (build-policy/abort-vs-panic/granularity) | `c7453118fdb` (a.k.a. `b058fb53`) |
| Ф.1 codegen-release | Сняты все 6 `#ifdef NOVA_CONTRACTS_RUNTIME` обёрток в `emit_c.rs` (requires/ensures/decreases/assert_static/assume/record-invariant) → недоказанные эмитятся и в release; proven по-прежнему `continue` (zero-cost). Убран dead `-DNOVA_CONTRACTS_RUNTIME=1` define из `test_runner.rs`. `nova_contract_violation` release-safe (R3). | `691917e9cd0` (`7abfc491`) |
| Ф.2 unchecked+policy | `#unchecked` per-fn opt-out (parser `parse_contract_attrs` → `ContractAttrs.contracts_unchecked` → `FnDecl.contracts_unchecked` → codegen elide ВСЕ checks fn) + `--contracts=enforce\|off` build-policy (codegen `set_contracts_off`, wired через `nova-codegen` + `nova build` + build-cache key). `// CONTRACTS off\|enforce` per-fixture directive. 2 POS fixtures (t4/t5). | `61fb7175eb3` (`52b527f2`) |
| Ф.3 verify-pipeline | Захват `ModuleEnv` + `set_proven_contracts(&env.proven_contracts)` на ВСЕХ build/codegen call-sites (test_runner `codegen_to_c`, `nova-cli cmd_build`, bench `run`+`compile_for_profile`) — раньше proven-set доходил до codegen только на `nova-codegen compile` (R4: элизии не было на build-пути). SMT-backend selection note. 1 POS fixture (t2). | `0e864af8b1a` (`98add912`) |
| Ф.4 release-tests | 3 should-abort release fixtures (t1 unproven-requires, t3 ensures, t6 no-Z3-degrade). Матрица plan140 = 6 fixtures, все PASS в release под Z3/Trivial/no-z3-binary. | `31332a41b24` |
| Ф.5 perf+docs/close | Perf-микробенч `perf_contract_hot_loop` (proven-элидирование zero-cost: Z3-elide 0.197s ≈ off-baseline 0.191s; Trivial-checked 0.214s ≈ +12% на contract-saturated loop). D24 amend Perf-блок + Q34 closure + project-creation/simplifications/discussion-log/backlog/README. | _(этот коммит)_ |

### Acceptance verdict (F1-F7)

| | Критерий | Verdict | Доказательство |
|---|---|---|---|
| **F1** | release чекает недоказанные `requires`/`ensures` (abort) | ✅ | t1/t3 + 8/8 `contracts_*` abort в release (Ф.1) |
| **F2** | Z3-proven элидируется в release (zero-cost) | ✅ | t2 + perf: POST `result >= x` отсутствует в `.c` под Z3/Trivial; Z3-elide ≈ baseline |
| **F3** | `#unchecked` (per-fn) снимает проверки у недоказанных | ✅ | t4 — `#unchecked fn bad`, violated requires+ensures, exit 0 |
| **F4** | `--contracts=off` восстанавливает legacy zero-cost | ✅ | t5 — 0 `nova_contract_violation` в whole-module `.c` |
| **F5** | без Z3 все чекаются (safe degrade), abort на violation | ✅ | t6 (no-z3-binary + Trivial); perf Trivial-config |
| **F6** | D24 amended (enforce-with-elision; «стираются» retracted) | ✅ | Ф.0 D24 amend + RETRACT-callout (09-tooling.md) |
| **F7** | 0 регрессий в contract-тестах (debug + release) | ✅ | contracts 295/0/11 release (Z3), 250/0/56 (Trivial); 0 new FAIL vs baseline |

**Z3-статус:** компилятор собран `--features z3-backend` (vcpkg `libz3` скопирован в worktree
`vcpkg_installed`, untracked via `.git/info/exclude`). Default-бэкенд остаётся `Trivial` даже при
скомпилированном Z3 (детерминизм verify-suite Plan 33); полный Z3 — `NOVA_SMT_BACKEND=z3`. Safe degrade
без Z3 верифицирован (proven меньше → больше runtime-checks, никогда не unsafe).

## Followup — Plan 140.3 (2026-06-13, ветка `plan-140.3-followups`)

- `[M-140-contract-panic-unwind]` ✅ **CLOSED** (commit `60e909a0`). Уточнение Q34 §2:
  не «abort→unwind» (в файбере assert/контракт уже разматываются к fail-frame —
  не abort), а **унификация классификации**. `nv_panic` тегал fail-frame
  `error_kind = NOVA_THROW_PANIC` (D188), а `nova_assert_loc`/`nova_contract_violation`
  ставили только `error_msg` (kind = NOVA_THROW_USER) → пойманный `consume`-scope'ом
  assert/контракт классифицировался как recoverable **Failure**, не **Panic** —
  хотя spec D13 говорит «assert failure = panic». Фикс: обе fail-frame ветки теперь
  ставят `error_kind = NOVA_THROW_PANIC` → assert + контракт + panic классифицируются
  ОДИНАКОВО (формат сообщения уже был унифицирован Plan 140.1). 2 строки в хедерах,
  без ABI/codegen-изменений. Тест `plan140/consume_assert_contract_panic_class`
  (compile+link, зеркало `codegen_consume_panic_caught`). Верифицировано: plan140
  32/0, plan110 50/0, contracts 251/0, plan100_4 42/0, plan125_1 15/0 — 0 регрессий
  от Failure→Panic-реклассификации.
- `[M-140.1-message-interpolation]` ✅ **CLOSED** (commit `1d6d2ca5`) — interp-сообщения
  контрактов `requires x>0, "got ${x}"`. Полное acceptance — в
  [140.1-contract-custom-message.md](140.1-contract-custom-message.md) §Followup.
- `[M-140-contract-levels]` (P3) — остаётся deferred (per-module opt-out + Eiffel-гранулярность).
  По именованию: module-opt-out = голый `#unchecked` перед `module X` (как `#stable`/
  `#no_prelude`), НЕ `#unchecked_module` (суффикса `_module` в Nova нет).
