---
name: project-spec-dblock-numbering
description: "Схема нумерации D-блоков; коллизии D109/D110/D111 (2026-05-18), cross-branch D245 (2026-06-12), two-live D278 (2026-06-15) + правило разрешения"
metadata: 
  node_type: memory
  type: project
  originSessionId: 1b171c1a-1ebe-41ac-8384-bdab1b0efdce
---

**Текущий максимум D ~ D279** (Plan 145.2 codegen emission determinism, `08-runtime.md`, 2026-06-15; D277 GC layout, D278 editors-highlighting). Раскладка ниже (D104-D145) — историческая; реальный максимум ВСЕГДА проверять: `grep -rh "^## D[0-9]" spec/decisions/ | sort -V | tail`.

**ПРАВИЛО cross-branch D-коллизии** (рекуррентно при параллельных worktree — нормальный режим работы): две ветки независимо присвоили ОДИН D-номер → при merge **ЖИВОЙ (active) блок УДЕРЖИВАЕТ номер; retracted/draft-блок УСТУПАЕТ и переименовывается в СЕМАНТИЧЕСКИЙ маркер** (не в новый номер). **Подвид «оба блока ЖИВЫЕ» (legitimate decisions, ни один не retracted): тот, что смёржился в main ПЕРВЫМ, удерживает номер; ПОЗЖЕ-мёржащийся переномеровывается в следующий свободный РЕАЛЬНЫЙ номер** (не семантический маркер — оба решения настоящие). Примеры:
- **2026-06-15 (D278, two-live):** main (`eb6ce348`) занял D278 = «Editor highlighting↔lexer» (09-tooling.md); моя ветка plan-145.2 независимо дала D278 = «Codegen emission determinism» (08-runtime.md). Оба живые → editors-D278 (смёржен раньше) удержал, мой переномерован **D278→D279** (heading + 2 cross-ref в 08-runtime.md + AC7 в плане 145.2 + логи + discussion-log). Детект: после `git merge main` — `grep -rn "## D278" spec/decisions/` показал 2 блока в разных файлах. Bump текущего максимума: D279.
- **2026-06-12 (D245):** plan-83-go-cmn (main) дал D245 живому M:N worker-wakeup (uv_async note, `06-concurrency.md`); Plan 147 (plan-138.1) кратко занял D245 под pointer flip-scan, который был RETRACTED→D246 → flip-scan уступил: все его ссылки «D245» в `02-types.md`+`README.md` заменены на «flip-scan-draft» (commit 1782f86; M:N D245 цел). Детект: после merge `grep -c "## D245\|## ~~D245~~" spec/decisions/*.md` показал 2 блока в разных файлах.
- **2026-05-18 (D109/D110/D111):** plan-45-doc (Plan 33.4) vs main (Plans 56/57/59) — независимо одни номера; переномерованы D120-D123 (см. раскладку ниже).

**Why:** Plan 33.4 (15 мая) создал D109/D110/D111 на ветке plan-45-doc,
пока Plans 56/57/59 независимо использовали те же номера на main.
При merge произошла коллизия — spec audit устранил её.

**Финальная раскладка D104-D125 (исторический фрагмент):**
- D104 (03-syntax.md) — doc-comment syntax (Plan 45)
- D105-D107 (09-tooling.md) — doc-attrs, doc-tests, JSON schema (Plan 45)
- D108 (03-syntax.md) — map-literal (Plan 52)
- D109 (08-runtime.md) — hash/eq/ord built-ins (Plan 48) — referenced в compiler source
- D110 (02-types.md) — ghost state (Plan 33.4)
- D111 (09-tooling.md) — assume/assert_static (Plan 33.4)
- D112-D116 (04-effects.md / 09-tooling.md) — contracts verifier (Plan 33.4)
- D117 (03-syntax.md) — size accessors call syntax (Plan 60)
- D118 (04-effects.md) — typed Fail[E] codegen (Plan 61)
- D119 (02-types.md) — method-level type params (Plan 48)
- D120 (04-effects.md) — #pure views + axioms [was D109, Plan 33.4]
- D121 (09-tooling.md) — Benchmark DSL [was D109, Plan 57]
- D122 (02-types.md) — Hybrid dispatch [was D110, Plan 56]
- D123 (02-types.md) — Tuple monomorphization [was D111, Plan 59]
- D124 (08-runtime.md) — Edition-versioned prelude resolver (Plan 62.F.bis Ф.1)
- D125 (08-runtime.md) — Prelude shadow warning lint (Plan 62.F.bis Ф.2)
- D126 (02-types.md) — Strict type propagation в codegen / no silent `nova_int` fallback (Plan 70 session 2, 2026-05-18)
- D127 (09-tooling.md) — Stability-tier enforcement scope (Plan 71, 2026-05-19)
- D128 (02-types.md) — `char` distinct from `int` в codegen mono'd generics (Plan 70.3, 2026-05-19)
- D129 (02-types.md, branch plan-70) — int as i64 alias + byte/u8 mangle (Plan 70.4 Ф.3+Ф.4, 2026-05-19) — НЕ merge'нут в main по состоянию 2026-05-19
- D130 (09-tooling.md) — Opaque/reveal/fuel controlled SMT unfolding (Plan 33.9 Ф.7, 2026-05-19)
- D138 коллизия: использован дважды (Plan 03.1 межпакетный импорт + Plan 83.4.5.6 Ф.3 draft Default-on M:N) — при активации 83.4.5.6 переномеровать. D139 version-диапазоны (03.2), D140 effect-aware deps (03.4), D141 byte_at primitives (90), D142 protocol/effect literal symmetry (97), D143 static-метод в protocol (97.1), D144 sub-slice views, D145 fn[T] префикс (101), … D246 three-axis (147).

**How to apply:** новый D-блок → следующий свободный (проверять grep'ом, НЕ полагаться на раскладку выше). При cross-branch merge — применять ПРАВИЛО коллизии выше. Связано с [[reference-mn-race-case-study]] (та же история параллельных веток).
Проверять текущий максимум: `grep -rh "^## D[0-9]" spec/decisions/ | sort -V | tail -1`.
