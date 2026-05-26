// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 100.4.2 — Ф.0 GATE: design probe + scope decision

> Дата: 2026-05-26. Worktree: `nova-p100-4-2`. Branch: `plan-100-4-2-async-cleanup`.
> HEAD baseline: `8c397698814` (post-100.4.4 closure).

## 1. Design D1-D7 locked
Per plan-doc: D1 suspend allowed; D2 cancel-safe; D3 Time.timeout integration;
D4 shielded keyword (отвергнут для bootstrap); D5 spawn в defer запрещён;
D6 Channel.recv allowed с caveat; D7 await preservation.

## 2. Current checker audit
`compiler-codegen/src/types/mod.rs::check_defer_body_inner`:
- `SUSPEND_EFFECT_NAMES = &["Net", "Fs", "Db", "Time"]` (line ~6085).
- Call arm: callee.effects ∩ SUSPEND_EFFECT_NAMES → error D90 «defer must be fast cleanup».
- Member call (Net.x, Time.x, ...): same error.
- AST-level Spawn / Supervised / Detach / Blocking / ParallelFor → error (D5 keep).

## 3. План реализации (Option B — minimal viable D159 for bootstrap)

**Что делаем:**
1. Spec D90 §5 amend: «suspend allowed»; spawn/parallel for keep banned (D5).
2. Checker: remove suspend-call error в Call arm; keep AST-level Spawn/Supervised/Detach/Blocking/ParallelFor bans.
3. Cleanup может теперь содержать `Time.sleep`, `Net.*`, `Fs.*`, `Db.*` — но эти effects будут declared в fn-sig (как и любые другие effects).
4. 5-7 POS fixtures (verify suspend allowed); 4-5 NEG fixtures (verify spawn/parallel for still error).

**Что НЕ делаем (followup'ы):**
- `[M-100.4.2-cancel-shielding]` — D2 cancel-safe semantics (cleanup completes before cancel-propagation): runtime feature, требует значительной работы в Plan 49 cancel-routing + defer scope wrap. Production-grade важно, но в bootstrap defer тоже выполняется после throw (cancel — это throw kind), так что cleanup runs regardless. **Cancel-shielding строго гарантирует завершение даже при повторном cancel — это followup.**
- `[M-100.4.2-time-timeout-integration]` — `with Time.timeout(d) { cleanup() }` уже работает (Plan 22), но verification fixture отложен.
- `[M-100.4.2-await-preservation]` — Plan 49 cancel-routing уже обеспечивает; verification fixture.

## 4. Acceptance Ф.0
- [x] D1-D7 locked.
- [x] Checker audit complete.
- [x] Scope decided (Option B): checker change + minimal fixtures.
- [x] Followup markers enumerated.

**GATE Ф.0: PASS.**
