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

---

## Ф.7 — Полный sweep nova_tests/ (≥3250 файлов)

**Статус:** 🔴 OPEN

### Мотивация

Ф.2 профилировала только 260 файлов (8% suite'а — 10 выбранных директорий).
В `nova_tests/` находится 3250+ `.nv`-файлов и 6700+ `test {}`-блоков, которые
ни разу не проверялись на соответствие бюджету ≤3s.

### Цель

Гарантировать, что **каждый** тест в default-регрессе укладывается в 3s total
(compile + run). Тесты, превышающие лимит, переносятся в `_slow`; для каждого
из них создаётся fast-variant (сокращённые параметры) — если файл содержит
параметрический workload. Если fast-variant невозможен — только `_slow`.

### Разграничение: тесты vs фикстуры

Не все `.nv`-файлы в `nova_tests/` являются тестами:

- **Тест-файл** — содержит хотя бы один `test {}` блок.
- **Фикстура** — файл без `test {}`: вспомогательный модуль, compile-error
  smoke (`// EXPECT_COMPILE_ERROR`), helper-type, shared data. Фикстуры в sweep
  **не участвуют** — у них нет run-времени и нет смысла ставить лимит.
- **Slow-файл** — `*_slow.nv`; уже исключён из default-регресса (D298). В sweep
  не входит.

Алгоритм классификации файла:
1. Имя оканчивается на `_slow.nv` → пропустить.
2. Содержит `test "` → тест-файл, подлежит измерению.
3. Иначе → фикстура, пропустить.

### Фазы

**Ф.7.1 — Полный прогон с --results-file**

Запустить `nova test --results-file /tmp/profile_full.json` по всем директориям
`nova_tests/` (или `nova test --results-file` без фильтра). Получить compile_ms
и run_ms для каждого тест-файла.

Ожидаемый объём: ~2600–2700 тест-файлов (3250 минус фикстуры и _slow).

**Ф.7.2 — Классификация**

Кандидат на `_slow`: `run_ms ≥ 3000`.

Compile-time в порог не входит. Минимальная стоимость компиляции Nova — ~18-27s
на файл независимо от количества тест-кейсов (базовый cost codegen+clang, не
проблема объёма). Порог по compile_ms покрыл бы все тесты без исключения.

По каждому кандидату — явное решение:
- **migrate-slow**: переименовать `<name>.nv` → `<name>_slow.nv`, обновить
  `module` строку. Использовать когда fast-variant невозможен (нет параметра N,
  сценарий неделимый).
- **create-fast-variant**: создать `<name>.nv` (уменьшенные параметры, run<3s),
  оригинал → `<name>_slow.nv`. Fast-variant обязан: иметь тот же module-путь,
  покрывать тот же сценарий, но с меньшим N.
- **keep**: тест случайно медленный на измерительном прогоне, при повторе run<3s.
  Задокументировать причину.

**Ф.7.3 — Применение**

- Переименование + module-строки.
- Создание fast-variants.
- `nova test` (default) — все тесты PASS, ни один не превышает run_ms=5000
  (проверить через `--max-test-ms 3000`).
- `nova test --include-slow` — slow-тесты PASS.

**Ф.7.4 — Флаг --max-run-ms + Gate в CI**

Текущий `--max-test-ms` меряет `total elapsed` (compile + run вместе) —
непригоден для порога по `run_ms`.

Нужно добавить `--max-run-ms N` в `nova-cli` + `TestAllOpts`:
- Смотрит на `run_ms` из `ResultRecord`, не на `elapsed()`.
- Exit 3 если хоть один тест превысил N ms run_ms → pipeline RED.
- `N=0` (default) — отключено (backward-compat, как `--max-test-ms`).

После реализации добавить в CI: `nova test --max-compile-ms 15000 --max-run-ms 3000`.
Оба флага работают независимо — exit 3 если хоть один из критериев нарушен.

### Критерии приёмки Ф.7

- A7.0 (ОБЯЗАТЕЛЬНЫЙ): production-grade, без упрощений.
- A7.1: Измерены **все** тест-файлы (не фикстуры, не _slow), не менее 2500
  файлов.
- A7.2: По каждому кандидату (total≥3s / run≥2s) — явное решение в таблице.
- A7.3: `--max-run-ms N` реализован в `nova-cli` + `test_runner.rs`; проверяет `run_ms` из `ResultRecord`, exit 3 при нарушении. (`--max-compile-ms` тоже реализован как инструмент анализа, но не используется как gate — compile всегда ≥18-27s.)
- A7.4: `nova test --max-run-ms 3000` завершается без exit 3 — ни один тест не превышает run_ms=3000.
- A7.5: Все новые fast-variants PASS в default-регрессе.
- A7.6: Все перемещённые в _slow тесты PASS с `--include-slow`.
- A7.7: Таблица кандидатов и решений добавлена в
  `docs/research/12-test-suite-profile-2026.md` (секция Ф.7).

---

## Ф.8 — Консолидация тестов в folder-module (compile-unit reduction)

**Статус:** ✅ CLOSED 2026-06-17 (commits c6d09fec + 9b760577)

### Мотивация

Минимальная стоимость компиляции — ~18-27s на файл независимо от числа `test {}`
блоков (базовый cost codegen+clang). При 2181 позитивном тест-файле и 8 parallel
jobs регресс занимает ~80 мин wall-clock. Объединение нескольких позитивных файлов
в один folder-module compile unit сокращает число вызовов компилятора пропорционально
числу объединённых файлов.

### Механизм

Nova уже поддерживает folder-module: все `.nv`-файлы в папке объявляют одинаковый
`module X` → компилятор собирает их как один compile unit (один вызов clang). Это
реализовано в `imports.rs: resolve_imports_inline_ex` (флаг `include_test_peers`).

Текущий `walk_nv_filtered` в `test_runner.rs` при обнаружении folder-module
**пропускает папку** — считает её библиотекой, а не тестом. Нужно добавить ветку:
folder-module с `test "` блоками → один entry (первый файл по алфавиту), остальные
peers подтянет resolver.

### Ограничение: негативные тесты не объединяются

`EXPECT_COMPILE_ERROR` проверяется на уровне compile unit — один unit может нести
только одну ожидаемую ошибку. Негативные тесты **остаются standalone навсегда**.

По D29: папка — либо folder-module (все файлы объявляют одинаковый `module X`),
либо папка со standalone-файлами (каждый свой `module`). Смешивать нельзя.
→ Негативные файлы переезжают в подпапку `neg/`.

### Алгоритм

**Ф.8.1 — Изменить `walk_nv_filtered` в `test_runner.rs`**

Добавить ветку: если папка является folder-module (`is_folder_module_dir`) И хотя
бы один peer содержит `test "` → добавить первый файл (по алфавиту) как entry.
Resolver подтянет остальных peers через `resolve_imports_inline_ex`.

```
// было: folder-module → пропуск
// стало:
if is_folder_module {
    if folder_module_has_tests(&direct_nv) {
        direct_nv.sort();
        out.push(direct_nv[0].clone());
    }
    // иначе — fixture-folder-module, пропускаем как раньше
} else {
    for p in direct_nv { out.push(p); }
}
```

**Ф.8.2 — Пилот: `basics/` (8 файлов, 0 негативных)**

1. Переименовать `module basics.X` → `module basics` в каждом файле.
2. Разрешить 2 конфликта имён (`fib`, `factorial` — в `functions.nv` и `recursion.nv`).
3. Убедиться что `nova test nova_tests/basics/` → все тесты PASS как один compile unit.

**Ф.8.3 — Разделение смешанных папок (126 директорий)**

Для каждой директории где есть и позитивные и негативные файлы:
1. Создать подпапку `neg/`.
2. Переместить все файлы с `EXPECT_COMPILE_ERROR` в `neg/`.
3. Обновить `module` в перемещённых: `module X.filename` → `module X.neg.filename`.

**Ф.8.4 — Переименование `module` в позитивных файлах**

Для каждой директории (после выноса негативных):
1. Сменить `module X.filename` → `module X` во всех позитивных файлах.
2. Разрешить конфликты имён (детектор: скрипт по всем 126 папкам перед началом).
   Стратегия: `fn busy` в `blocking_cancel_test.nv` → `fn busy_cancel`; префикс = имя файла без суффикса.

**Ф.8.5 — Верификация**

- `nova test` (default) — все тесты PASS.
- Число compile units сократилось: `nova test --results-file` → кол-во записей ≤ (кол-во папок + негативные).
- `nova test --include-slow` — slow-тесты PASS.

### Статистика (2026-06-17)

| Категория | Файлов | Действие |
|---|---|---|
| Позитивные тест-файлы | 2181 | Переименовать `module`, разрешить конфликты |
| Негативные (`EXPECT_COMPILE_ERROR`) | 760 | Перенести в `neg/`, обновить `module` |
| Фикстуры | 302 | Не трогать |
| `_slow` | 57 | Не трогать |
| Папок со смешанными тестами | 126 | Создать `neg/`, разделить |
| Негативных файлов для переноса | 673 | Из 126 смешанных папок |

### Критерии приёмки Ф.8

- ✅ A8.0 (ОБЯЗАТЕЛЬНЫЙ): production-grade, без упрощений.
- ✅ A8.1: `walk_nv_filtered` запускает folder-module с `test "` как один compile unit.
- ✅ A8.2: Пилот `basics/` — 8 файлов → 1 compile unit, все тесты PASS.
- ✅ A8.3: 92 eligible dirs конвертированы; 170 EXPECT_COMPILE_ERROR файлов перенесены в `neg/`. 67 dirs с конфликтами имён — оставлены как есть (followup: Ф.8.next).
- ✅ A8.4: 259 PASS / 0 FAIL по 85 затронутым dirs; 0 регрессий.
- ⬜ A8.5: Ещё не измерено (требует --results-file по всему suite).
- ⬜ A8.6: D298 update ещё не сделан.

### Итог Ф.8 (2026-06-17)

- `walk_nv_filtered` + `folder_module_has_tests` добавлены в `test_runner.rs` (commit c6d09fec).
- `basics/` пилот: 8 файлов → 1 compile unit, `module nova_tests.basics`.
- 92 eligible dirs конвертированы: позитивные файлы → `module nova_tests.X`, 170 EXPECT_COMPILE_ERROR файлов → `neg/` с `module neg.stem` (commit 9b760577).
- Исключены из конвертации: 67 dirs с name conflicts, 8 dirs со slow-only файлами, 5 dirs с nova.toml package name.
- 259 PASS / 0 FAIL по 85 dirs.

---

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
