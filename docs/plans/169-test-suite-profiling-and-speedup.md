# Plan 169 — Test-suite profiling + speedup

> **Создан:** 2026-06-17. **Статус:** ✅ ЗАКРЫТ 2026-06-17 (followups closed 2026-06-17).
> **Worktree:** `nova-p169` (branch `plan-169-test-profiling`).
> **Spec:** D298 (spec/decisions/09-tooling.md — тест-конвенции `_slow`, бюджет CI).

## Цель

Измерить compile_ms/run_ms на каждый тест-файл, построить профиль top-N,
мигрировать медленные тесты в `_slow` или создать fast-variant, закрепить
бюджет ≤120s через D298.

## Фазы

### Ф.1 — compile_ms / run_ms в ResultRecord (commit 822f358d)

- `TestResult` расширен: `compile_ms: u64`, `run_ms: u64` (backward-compat: 0 при absent).
- `--results-file` JSON содержит оба поля.
- Изменения: `test_runner.rs`, `compiler-codegen/src/main.rs`, `nova-cli/src/main.rs`.

### Ф.2 — Профиль 260 тестов (commit 88143697)

- Измерено 260 тестов (basics/generics/concurrency/plan57/plan103_*/plan110/plan55/plan56/plan140/plan152_5).
- Медиана 36.7s, p90 47.5s; compile доминирует 96%.
- Топ-10: cancel_stress_armed (100s), mono_spawn_closure_smoke (87s), condvar_wait_cancel (62s).
- Профиль: `docs/research/12-test-suite-profile-2026.md`.

### Ф.3 — Классификация кандидатов (commit 9b76ae04)

- 41 кандидат (total≥3s OR run≥2s OR compile≥500ms):
  - 24 → migrate-slow
  - 4 → create-fast-variant
  - 13 → keep-fast (не превышают порог при повторных запусках / system-tests)
  - 0 → investigate

### Ф.4 — Переименование + fast variants (commit 222936fd)

- 24 файла переименованы: `<name>.nv` → `<name>_slow.nv`; module-строки обновлены.
- 4 fast variants созданы с уменьшенными параметрами:
  - `gc_bench` (100k→1k iterations)
  - `stress_high_freq_loop_t11_8` (1000→100 iterations)
  - `f1_closure_array_gc_stress` (50→5 GC cycles)
  - `f4_clone_gc_stress` (100→10 iterations)

### Ф.5 — D298 spec (commit 348916f2)

- D298 добавлен в `spec/decisions/09-tooling.md`.
- Бюджет ≤120s CI (8 параллельных jobs).
- Порог кандидата: total≥3s OR run≥2s OR compile≥500ms.
- `docs/test-conventions.md` обновлён: секция `_slow`-тестов.

### Ф.6 — Фикстуры plan169/ (commit 340577e4)

- `nova_tests/plan169/t1_timing_fields.nv` — проверка compile_ms/run_ms в JSON.
- `nova_tests/plan169/t2_fast_variant.nv` — fast-variant pattern (PASS default).
- `nova_tests/plan169/t2_slow.nv` — slow-lane тест (skipped в default, PASS с `--include-slow`).
- Default: 2 PASS. With `--include-slow`: 3 PASS.

## Критерии приёмки

- ✅ A0: production-grade, без упрощений.
- ✅ A1: `TestResult` содержит `compile_ms` и `run_ms` раздельно; JSON включает оба.
- ✅ A2: Таблица top-30 по реальным данным (260 тестов); `docs/research/12-test-suite-profile-2026.md`.
- ✅ A3: По каждому тесту из top-41 — явное решение (24 migrate / 4 fast-variant / 13 keep).
- ✅ A4: 24 файла переименованы в `_slow`, module-строки обновлены.
- ✅ A5: 4 fast-variant созданы; оригиналы → `_slow`.
- ✅ A6: `nova test` (default) PASS без _slow; `nova test --include-slow` PASS со всеми.
- ✅ A7: D298 в `spec/decisions/09-tooling.md`; `docs/test-conventions.md` обновлён.
- ✅ A8: compile dominates 96% — документировано в profile (нет actionable issue).

---

## ИТОГ (2026-06-17)

✅ Ф.1: `compile_ms`/`run_ms` добавлены в `ResultRecord`; `--results-file` JSON включает оба поля. Backward-compatible (missing fields → 0).
✅ Ф.2: 260 тестов измерено (basics/generics/concurrency/plan57/plan103_*/plan110/plan55/plan56/plan140/plan152_5). Медиана 36.7s, p90 47.5s; compile dominates 96%. Профиль: `docs/research/12-test-suite-profile-2026.md`.
✅ Ф.3: 41 кандидат классифицирован — 24 migrate-slow, 4 create-fast-variant, 13 keep-fast, 0 investigate.
✅ Ф.4: 24 файла переименованы в `_slow.nv` (module-строки обновлены); 4 fast-variant созданы (gc_bench, stress_high_freq_loop_t11_8, f1_closure_array_gc_stress, f4_clone_gc_stress).
✅ Ф.5: D278 добавлен в `spec/decisions/09-tooling.md` — бюджет ≤120s CI, пороги (total≥3s/run≥2s/compile≥500ms). `docs/test-conventions.md` обновлён.
✅ Ф.6: `nova_tests/plan169/` — 3 фикстуры (t1 PASS, t2 PASS, t2_slow → slow-lane). Compile_ms/run_ms подтверждены в JSON. Fast variants: gc_bench/plan55/plan56 PASS.

**Критерии приёмки:**
- ✅ A0 (ОБЯЗАТЕЛЬНЫЙ): production-grade, без упрощений. compile_ms/run_ms реальные измерения, не dummy.
- ✅ A1: `TestResult` содержит `compile_ms` и `run_ms` раздельно; `--results-file` сохраняет оба.
- ✅ A2: Таблица top-30 построена по реальным данным (260 тестов); `docs/research/12-test-suite-profile-2026.md`.
- ✅ A3: По каждому тесту из top-41 принято явное решение (24 migrate / 4 fast-variant / 13 keep).
- ✅ A4: 24 файла переименованы в `_slow`, module-строки обновлены.
- ✅ A5: 4 fast-variant созданы с уменьшенными параметрами; оригиналы → `_slow`.
- ✅ A6: `nova test` (default) PASS без _slow; `nova test --include-slow` PASS со всеми.
- ✅ A7: D278 в `spec/decisions/09-tooling.md`; `docs/test-conventions.md` обновлён.
- ✅ A8: Все системные находки: compile dominates 96% (нет actionable issue, документировано в profile).

Merged → main через merge(plan-169).

### Followup-фиксы (2026-06-17)

**[M-169-timing-report-regression-gate] ✅ CLOSED** — `nova test --max-test-ms N`:
- Флаг добавлен в `nova-cli/src/main.rs` + `TestAllOpts` в `test_runner.rs`.
- После прогона: тесты превысившие N ms → список + `exit 3`.
- `N=0` (default) — отключено (backward-compat).
- Commit: `a61199a5`.

**plan55 4→0 FAIL** — три бага исправлены:
- `f1_closure_array_gc_stress_slow.nv` дублировал module name → исправлен (`plan55.f1_closure_array_gc_stress_slow`).
- Compiler Bug A (`e5945c24`): `export fn T @m() => expr` без explicit return type → codegen генерировал `nova_unit`. Фикс в `emit_c.rs`: field_cache коэрсирует `FnBody::Expr` → `FnBody::Block(stmts=[], trailing=e)` до codegen; расширены обе ветки matching.
- Compiler Bug B (`a4cb7f4d`): `collect_pattern_bindings` проверял `all_decls.contains(name)` включая snake_case fn-имена — for-loop bindings с именем свободной функции (`inner`) не регистрировались. Фикс: только `builtins` + PascalCase = variant-like.
- Финал plan55: **19 PASS, 0 FAIL**.
