<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 140 — Contracts enforced in release (Z3-proven elided, unproven checked)

> **Создан:** 2026-06-10.  **Статус:** 📋 PLANNED.
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

- `[M-140-bounds-as-contract]` — Vec `@index` OOB как `requires 0 <= i < @len` (после 140; Plan 138.x/139).
- `[M-140-invariant-release]` — type invariants (если есть) — та же enforce-in-release семантика.
- `[M-140-contract-levels]` — Eiffel-style гранулярность (pre/post/invariant раздельно) если потребуется.

---

## Связанные D-блоки

| D | Что |
|---|---|
| D24 | AMEND — contracts enforced in release (proven-elided, unproven-checked); «стираются» retracted |
| D106 | verify_status / verify_mode — proven feeds release elision |
