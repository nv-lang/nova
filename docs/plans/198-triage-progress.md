<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 198 Ф.1 — census-таблица (read-only, БЕЗ удаления/переноса)

**Статус:** 📋 ГОТОВ К РЕВЬЮ владельца. Ф.1 = только классификация; удаление/миграция = Ф.2
после ревью (см. [198-nova-tests-triage.md](198-nova-tests-triage.md)).

## Методология

1. `ls nova_tests/**/*.nv` → **3450 файлов** в **252** директориях верхнего уровня.
2. Ключевое устройство корпуса (подтверждено эмпирически, см.
   [[reference-nova-module-model-folder]]): **папка = один модуль** — все `.nv`-файлы
   в директории, объявляющие одинаковый `module X.Y`, компилируются **ОДНИМ** compile-unit
   и в `nova test` репортятся ОДНОЙ строкой под именем алфавитно-первого файла. Поддиректории
   (`neg/`, вложенные пакеты) обычно объявляют СВОЙ уникальный `module`, поэтому
   компилируются отдельно. Итог: 3450 файлов → **1382 реальных compile-unit**.
3. Прогнал `nova test <dir> --full` батчами (nova.exe = релизный бинарник главного репо,
   env `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → vcpkg главного репо) на ВСЕХ 252
   директориях. Батчи ~600-650 файлов, дробил дальше бисекцией при ICE-крэше процесса
   (см. ниже). Логи: `target/triage-logs/*.log` (в worktree, не коммитятся — build-артефакт).
4. Для каждого файла: вердикт по правилам ниже. Полная построчная таблица (3450 строк, file ·
   verdict · status · note) **регенерируется** прогоном `nova test <dir> --full` по `nova_tests/` —
   698КБ TSV-дамп в `docs/plans` НЕ коммитим (doc-hygiene). Здесь — агрегированная сводка + особые списки.

### Правила вердикта

- **KEEP-SPECIAL** — файл с меткой `SOUNDNESS_REGRESSION` (soundness-ratchet,
  `contracts-z3.yml` MIN=12 — **18 файлов**, включая явные deferred-решения Ф.3(a)/(b)
  из плана (`trivial_string_len_*`, `f26_newtype_*`). Не трогать до Ф.2/Ф.3.
- **blocked-by-nova-build-ICE** — компиляция валит `nova` целым процессом
  (`[P67-LEGACY] ... checker must annotate`, compiler-conventions.md §0) — **13 файлов**,
  известный класс багов codegen-checker (не дефект файла, чинит Ф.4c). Список ниже.
- **N/A-support** — файл без собственного test-unit (библиотека/хелпер, импортируется
  тестом-соседом в том же поддереве, например `plan03_1/path_dep/mathlib/calc.nv`) —
  **111 файлов**. Наследует судьбу потребляющего теста, отдельно не классифицирован.
- **MIGRATE** — юнит компилируется и проходит (`PASS`) ИЛИ валиден, но требует z3-backend
  (`SKIP` под trivial-backend прогоном) — **1905 файлов**. Ценно, но не на своём месте:
  цель по умолчанию `spec_tests/conformance/` (это в основном language-feature тесты, а
  не std-модули — см. касту ниже про DUPLICATE). Точную цель (conformance vs
  `std/<module>/*_test.nv`) — решать per-directory в Ф.2.
- **STALE** — юнит не компилируется/не проходит (`CODEGEN-FAIL`/`CC-FAIL`/`RUN-FAIL`/
  `NEG-NO-ERROR`/`NEG-WRONG-MSG`/`TIMEOUT`) и НЕ входит в load-bearing/ICE-списки —
  **1403 файла**. Причина — в TSV-note (конкретная ошибка компилятора); превалируют
  retired-API маркеры (`E_STR_NO_LEN`, `E_POINTER_OP_USE_METHOD`, `E_D78_MODULE_PATH_MISMATCH`,
  `E_UNKNOWN_STATIC_METHOD`, ретировавшийся `.capacity()` accessor и т.п.), но есть и
  «прочий дрейф» — конкретика в note каждой строки, финальный root-cause за Ф.2/Ф.3.

### ⚠️ Про DUPLICATE (важная находка)

Задание просило 4-й вердикт **DUPLICATE** (уже есть эквивалент рядом со std-модулем).
Проверил: `nova_tests/` почти целиком тестирует **языковые фичи** (control-flow,
generics, atomics, closures, effects/concurrency-примитивы, str/Vec core-methods,
pattern-matching) — а не std-библиотечные модули. `std/**/*_test.nv` (122 файла)
покрывает другое (crypto/json/csv/http/fs/os/net/time/tls) — **пересечения по темам
почти нет**. Автоматически DUPLICATE не обнаружен ни в одной директории с уверенностью.

Единственный проверенный вручную кандидат — **`nova_tests/sync`** (39 файлов, Mutex/
RwLock/Semaphore/Lazy/Once/OnceCell) vs `std/runtime/sync_test.nv` (88 строк, 6 тестов).
Итог: **НЕ дубликат** — `std/runtime/sync_test.nv` заметно у́же по покрытию; `nova_tests/sync`
скорее *расширяет* существующий `*_test.nv`, чем дублирует. Вердикт для него — MIGRATE
(consolidate into `std/runtime/sync_test.nv`), не DUPLICATE.

**Вывод:** true-DUPLICATE detection требует контентного ревью per-directory (сравнение
покрытия тестов), это не автоматизировалось надёжно за один проход по 3450 файлам.
Рекомендация: в Ф.2 при миграции каждой директории — быстрая проверка «есть ли уже
`std/<module>/*_test.nv` с тем же именем модуля» перед созданием нового файла; если да —
слить (merge), не дублировать.

## Сводка по вердиктам (3450 файлов)

| Вердикт | Файлов | % |
|---|---|---|
| MIGRATE | 1905 | 55.2% |
| STALE | 1403 | 40.7% |
| N/A-support | 111 | 3.2% |
| KEEP-SPECIAL | 18 | 0.5% |
| blocked-by-nova-build-ICE | 13 | 0.4% |

## KEEP-SPECIAL — 18 файлов (не трогать до Ф.2/Ф.3)

- `nova_tests/contracts/assert_static_unverified_warn.nv` — SOUNDNESS_REGRESSION
- `nova_tests/contracts/assume_trust_introduced_warn.nv` — SOUNDNESS_REGRESSION
- `nova_tests/contracts/int_arith_no_overflow_positive.nv` — SOUNDNESS_REGRESSION
- `nova_tests/contracts/loop_compound_assign_w2402.nv` — SOUNDNESS_REGRESSION
- `nova_tests/contracts/loop_cond_assign_w2402.nv` — SOUNDNESS_REGRESSION
- `nova_tests/contracts/loop_in_let_w2402.nv` — SOUNDNESS_REGRESSION
- `nova_tests/contracts/recursive_no_decreases_warn.nv` — SOUNDNESS_REGRESSION
- `nova_tests/contracts/trivial_string_len_positive.nv` — unique repro (deletion forbidden
  by file header); Ф.3(a) deferred: `byte_len()` в TrivialBackend allow-list
- `nova_tests/contracts/neg/f60_bv_nooverflow_overflow_fail.nv` — SOUNDNESS_REGRESSION
- `nova_tests/contracts/neg/f61_bv_signed_overflow_fail.nv` — SOUNDNESS_REGRESSION
- `nova_tests/contracts/neg/int_overflow_add_panic.nv` — SOUNDNESS_REGRESSION
- `nova_tests/contracts/neg/int_overflow_compound_panic.nv` — SOUNDNESS_REGRESSION
- `nova_tests/contracts/neg/int_overflow_mul_panic.nv` — SOUNDNESS_REGRESSION
- `nova_tests/contracts/neg/trivial_string_len_fail.nv` — пара к trivial_string_len_positive
- `nova_tests/doc/f26_newtype_positive.nv` — Ф.3(b) deferred: newtype↔int только явная
  конверсия (владелец решил 2026-07-11) — фикстуру нужно поправить под решение
- `nova_tests/doc/neg/f26_newtype_negative.nv` — пара к f26_newtype_positive
- `nova_tests/plan140_4/ovf_unbounded_panic_neg.nv` — SOUNDNESS_REGRESSION
- `nova_tests/plan140_4/neg/ovf_contracts_off_panic_neg.nv` — SOUNDNESS_REGRESSION

## blocked-by-nova-build-ICE — 13 файлов (известный класс багов, чинит Ф.4c)

Все — `[P67-LEGACY] ... checker must annotate (compiler-conventions.md §0)`: компилятор
падает ЦЕЛЫМ ПРОЦЕССОМ (`nova: internal error`, exit 101), не просто CODEGEN-FAIL одного
теста. Найдено методом бисекции (delta-debugging directory-групп до единичного файла/
директории). Для co-equal-module директорий (все файлы = один compile-unit) нельзя
исключить один файл флагом `--skip` — крашится вся директория, т.к. `--skip` фильтрует
только репортинг, а компиляция юнита включает все файлы модуля целиком:

| Файл | Механизм ICE |
|---|---|
| `nova_tests/plan65/neg/f2_time_after_removed.nv` | emit_c.rs:48876, method=after |
| `nova_tests/plan83_10_4/*` (3 файла, весь merged-модуль) | emit_c.rs:48876, method=now |
| `nova_tests/plan153_4/chunks_windows.nv` + `views.nv` (merged) | emit_c.rs:49366, Index elem type unknown |
| `nova_tests/plan154_1/neg/neg_unknown_static_method.nv` | emit_c.rs:48876, method=from_debug |
| `nova_tests/plan48_mpm/neg/f5_cannot_infer_u_negative.nv` | emit_c.rs:48630, method=.produce |
| `nova_tests/plan48_mpm/neg/f6_method_param_only_in_return_negative.nv` | emit_c.rs:48630, method=.transform |
| `nova_tests/plan60/f1_methods_zero_cost.nv` | emit_c.rs:48630, method=.capacity |
| `nova_tests/plan60/f5_cap_legacy_rejected.nv` | emit_c.rs:48876, method=now (соседний файл в merged-модуле) |
| `nova_tests/plan60/f6_array_capacity_method.nv` | emit_c.rs:48630, method=.capacity |
| `nova_tests/plan99/neg/option_map_wrong_closure_arity_neg.nv` | emit_c.rs:49553, Ident `y` not in var_types |

Побочная находка: `plan60/f1` и `f6` тестируют retired `.capacity()`-accessor (сам по себе
уже вытеснен `.cap()`/`.cap(n)` fluent-методом, D372) — т.е. даже после починки ICE эти два,
скорее всего, окажутся STALE, а не MIGRATE.

## Сводка по директориям (252 директории верхнего уровня)

| Директория | Файлов | MIGRATE | STALE | N/A-support | KEEP-SPECIAL | ICE-blocked |
|---|---|---|---|---|---|---|
| `nova_tests/any_is` | 3 | 3 | 0 | 0 | 0 | 0 |
| `nova_tests/atomics` | 24 | 24 | 0 | 0 | 0 | 0 |
| `nova_tests/basics` | 8 | 0 | 8 | 0 | 0 | 0 |
| `nova_tests/buffers` | 11 | 11 | 0 | 0 | 0 | 0 |
| `nova_tests/cfg` | 26 | 8 | 0 | 18 | 0 | 0 |
| `nova_tests/cgfix_fluent_tail_if` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/concurrency` | 117 | 2 | 115 | 0 | 0 | 0 |
| `nova_tests/contracts` | 308 | 76 | 218 | 0 | 14 | 0 |
| `nova_tests/doc` | 46 | 9 | 21 | 14 | 2 | 0 |
| `nova_tests/effect_registry` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/effects` | 13 | 2 | 11 | 0 | 0 | 0 |
| `nova_tests/err173` | 22 | 22 | 0 | 0 | 0 | 0 |
| `nova_tests/err173_0` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/err173_1` | 6 | 6 | 0 | 0 | 0 | 0 |
| `nova_tests/err173_2` | 8 | 8 | 0 | 0 | 0 | 0 |
| `nova_tests/err173_3` | 11 | 11 | 0 | 0 | 0 | 0 |
| `nova_tests/err177_collectors` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/expected_runtime` | 10 | 10 | 0 | 0 | 0 | 0 |
| `nova_tests/ffi` | 11 | 11 | 0 | 0 | 0 | 0 |
| `nova_tests/fixed_array` | 7 | 7 | 0 | 0 | 0 | 0 |
| `nova_tests/gc` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/generics` | 41 | 38 | 3 | 0 | 0 | 0 |
| `nova_tests/lint` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/map_literals` | 29 | 8 | 21 | 0 | 0 | 0 |
| `nova_tests/modules` | 59 | 27 | 4 | 28 | 0 | 0 |
| `nova_tests/named_params` | 10 | 8 | 0 | 2 | 0 | 0 |
| `nova_tests/narrowing` | 5 | 5 | 0 | 0 | 0 | 0 |
| `nova_tests/negative_capability` | 20 | 9 | 0 | 11 | 0 | 0 |
| `nova_tests/p176repro` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan03_1` | 19 | 12 | 0 | 7 | 0 | 0 |
| `nova_tests/plan100_1` | 23 | 23 | 0 | 0 | 0 | 0 |
| `nova_tests/plan100_2` | 17 | 17 | 0 | 0 | 0 | 0 |
| `nova_tests/plan100_3` | 10 | 10 | 0 | 0 | 0 | 0 |
| `nova_tests/plan100_4_1` | 16 | 16 | 0 | 0 | 0 | 0 |
| `nova_tests/plan100_4_2` | 9 | 9 | 0 | 0 | 0 | 0 |
| `nova_tests/plan100_4_4` | 13 | 13 | 0 | 0 | 0 | 0 |
| `nova_tests/plan100_4_5` | 4 | 4 | 0 | 0 | 0 | 0 |
| `nova_tests/plan100_6` | 15 | 15 | 0 | 0 | 0 | 0 |
| `nova_tests/plan100_7` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan100_8` | 6 | 6 | 0 | 0 | 0 | 0 |
| `nova_tests/plan101_1` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan103_3` | 12 | 0 | 12 | 0 | 0 | 0 |
| `nova_tests/plan103_4` | 17 | 0 | 17 | 0 | 0 | 0 |
| `nova_tests/plan103_5` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan103_6` | 26 | 13 | 13 | 0 | 0 | 0 |
| `nova_tests/plan103_8` | 14 | 0 | 14 | 0 | 0 | 0 |
| `nova_tests/plan103_9` | 20 | 20 | 0 | 0 | 0 | 0 |
| `nova_tests/plan104_7_grammar` | 5 | 1 | 4 | 0 | 0 | 0 |
| `nova_tests/plan104_9` | 12 | 12 | 0 | 0 | 0 | 0 |
| `nova_tests/plan106` | 13 | 12 | 1 | 0 | 0 | 0 |
| `nova_tests/plan107` | 7 | 4 | 2 | 1 | 0 | 0 |
| `nova_tests/plan108` | 48 | 15 | 33 | 0 | 0 | 0 |
| `nova_tests/plan108_4` | 13 | 6 | 7 | 0 | 0 | 0 |
| `nova_tests/plan110` | 48 | 48 | 0 | 0 | 0 | 0 |
| `nova_tests/plan110_9_np` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan114` | 95 | 33 | 62 | 0 | 0 | 0 |
| `nova_tests/plan115` | 11 | 11 | 0 | 0 | 0 | 0 |
| `nova_tests/plan118` | 139 | 49 | 90 | 0 | 0 | 0 |
| `nova_tests/plan118_1_7` | 5 | 2 | 3 | 0 | 0 | 0 |
| `nova_tests/plan118_1_cstr_nul` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan118_1_unsafe_extern_fn` | 2 | 0 | 2 | 0 | 0 | 0 |
| `nova_tests/plan11_followup` | 20 | 3 | 17 | 0 | 0 | 0 |
| `nova_tests/plan120` | 12 | 12 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123` | 105 | 105 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_1` | 18 | 18 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_1_1` | 3 | 3 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_1_2` | 5 | 0 | 5 | 0 | 0 | 0 |
| `nova_tests/plan123_2` | 14 | 14 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_2_1` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_3` | 12 | 12 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_3_1` | 4 | 0 | 4 | 0 | 0 | 0 |
| `nova_tests/plan123_3_2` | 3 | 3 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_4` | 10 | 10 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_4_2` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_4_3` | 3 | 3 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_4_4` | 1 | 0 | 1 | 0 | 0 | 0 |
| `nova_tests/plan123_5` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_5_4` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_7` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_7_1` | 10 | 10 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_7_2` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_7_5` | 3 | 3 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_7_6` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_7_7` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_chain_elem` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan123_followups_2026_06_05` | 11 | 11 | 0 | 0 | 0 | 0 |
| `nova_tests/plan124` | 102 | 24 | 78 | 0 | 0 | 0 |
| `nova_tests/plan125` | 22 | 21 | 1 | 0 | 0 | 0 |
| `nova_tests/plan125_1` | 15 | 14 | 1 | 0 | 0 | 0 |
| `nova_tests/plan125_2` | 15 | 9 | 6 | 0 | 0 | 0 |
| `nova_tests/plan125_followups` | 9 | 9 | 0 | 0 | 0 | 0 |
| `nova_tests/plan126` | 21 | 6 | 15 | 0 | 0 | 0 |
| `nova_tests/plan126_2` | 10 | 2 | 8 | 0 | 0 | 0 |
| `nova_tests/plan127` | 17 | 4 | 13 | 0 | 0 | 0 |
| `nova_tests/plan127_1` | 3 | 0 | 3 | 0 | 0 | 0 |
| `nova_tests/plan128` | 19 | 4 | 15 | 0 | 0 | 0 |
| `nova_tests/plan128_2` | 8 | 8 | 0 | 0 | 0 | 0 |
| `nova_tests/plan132` | 6 | 6 | 0 | 0 | 0 | 0 |
| `nova_tests/plan133` | 4 | 4 | 0 | 0 | 0 | 0 |
| `nova_tests/plan134` | 4 | 4 | 0 | 0 | 0 | 0 |
| `nova_tests/plan135` | 11 | 1 | 10 | 0 | 0 | 0 |
| `nova_tests/plan136` | 11 | 11 | 0 | 0 | 0 | 0 |
| `nova_tests/plan136_1` | 7 | 7 | 0 | 0 | 0 | 0 |
| `nova_tests/plan138` | 10 | 10 | 0 | 0 | 0 | 0 |
| `nova_tests/plan138_1` | 10 | 10 | 0 | 0 | 0 | 0 |
| `nova_tests/plan138_2` | 19 | 2 | 17 | 0 | 0 | 0 |
| `nova_tests/plan138_3` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan138_5` | 15 | 7 | 8 | 0 | 0 | 0 |
| `nova_tests/plan140` | 10 | 10 | 0 | 0 | 0 | 0 |
| `nova_tests/plan140_1` | 15 | 15 | 0 | 0 | 0 | 0 |
| `nova_tests/plan140_2` | 12 | 12 | 0 | 0 | 0 | 0 |
| `nova_tests/plan140_3` | 7 | 7 | 0 | 0 | 0 | 0 |
| `nova_tests/plan140_4` | 5 | 3 | 0 | 0 | 2 | 0 |
| `nova_tests/plan141` | 8 | 0 | 8 | 0 | 0 | 0 |
| `nova_tests/plan142` | 10 | 10 | 0 | 0 | 0 | 0 |
| `nova_tests/plan143` | 3 | 3 | 0 | 0 | 0 | 0 |
| `nova_tests/plan143_2` | 7 | 7 | 0 | 0 | 0 | 0 |
| `nova_tests/plan144_0` | 10 | 8 | 2 | 0 | 0 | 0 |
| `nova_tests/plan144_1` | 6 | 6 | 0 | 0 | 0 | 0 |
| `nova_tests/plan144_checker` | 2 | 1 | 1 | 0 | 0 | 0 |
| `nova_tests/plan144_inftype` | 11 | 9 | 2 | 0 | 0 | 0 |
| `nova_tests/plan145` | 6 | 6 | 0 | 0 | 0 | 0 |
| `nova_tests/plan145_2` | 4 | 0 | 4 | 0 | 0 | 0 |
| `nova_tests/plan147` | 38 | 30 | 8 | 0 | 0 | 0 |
| `nova_tests/plan148` | 18 | 8 | 10 | 0 | 0 | 0 |
| `nova_tests/plan149` | 10 | 10 | 0 | 0 | 0 | 0 |
| `nova_tests/plan149_toml` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan150` | 13 | 13 | 0 | 0 | 0 | 0 |
| `nova_tests/plan153_0` | 4 | 3 | 1 | 0 | 0 | 0 |
| `nova_tests/plan153_1` | 9 | 9 | 0 | 0 | 0 | 0 |
| `nova_tests/plan153_2` | 11 | 1 | 10 | 0 | 0 | 0 |
| `nova_tests/plan153_2_zc` | 6 | 6 | 0 | 0 | 0 | 0 |
| `nova_tests/plan153_3` | 8 | 8 | 0 | 0 | 0 | 0 |
| `nova_tests/plan153_3_1` | 8 | 1 | 7 | 0 | 0 | 0 |
| `nova_tests/plan153_4` | 7 | 5 | 0 | 0 | 0 | 2 |
| `nova_tests/plan153_5` | 7 | 4 | 3 | 0 | 0 | 0 |
| `nova_tests/plan153_5_nested` | 4 | 0 | 4 | 0 | 0 | 0 |
| `nova_tests/plan153_6` | 3 | 3 | 0 | 0 | 0 | 0 |
| `nova_tests/plan154` | 5 | 5 | 0 | 0 | 0 | 0 |
| `nova_tests/plan154_1` | 9 | 8 | 0 | 0 | 0 | 1 |
| `nova_tests/plan156` | 4 | 4 | 0 | 0 | 0 | 0 |
| `nova_tests/plan159` | 19 | 19 | 0 | 0 | 0 | 0 |
| `nova_tests/plan160` | 8 | 5 | 1 | 2 | 0 | 0 |
| `nova_tests/plan161` | 12 | 12 | 0 | 0 | 0 | 0 |
| `nova_tests/plan162` | 14 | 8 | 0 | 6 | 0 | 0 |
| `nova_tests/plan162_1` | 5 | 3 | 0 | 2 | 0 | 0 |
| `nova_tests/plan162_2` | 4 | 3 | 0 | 1 | 0 | 0 |
| `nova_tests/plan163` | 6 | 4 | 0 | 2 | 0 | 0 |
| `nova_tests/plan164` | 6 | 6 | 0 | 0 | 0 | 0 |
| `nova_tests/plan167` | 7 | 7 | 0 | 0 | 0 | 0 |
| `nova_tests/plan168` | 4 | 2 | 2 | 0 | 0 | 0 |
| `nova_tests/plan169` | 6 | 6 | 0 | 0 | 0 | 0 |
| `nova_tests/plan169_2_blanket` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan170` | 11 | 11 | 0 | 0 | 0 | 0 |
| `nova_tests/plan172` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan172_14` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan172_boxiter_width` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan172_composition` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan172_neg_member_not_concrete` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan172_neg_mixed_signedness` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan172_neg_multiple_sets` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan172_neg_not_in_set` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan172_showcase` | 1 | 0 | 1 | 0 | 0 | 0 |
| `nova_tests/plan172_stdlib_neg` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan172_stdlib_use` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan175_handler_annot` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan176_holes` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan180_f1` | 1 | 0 | 1 | 0 | 0 | 0 |
| `nova_tests/plan183_f4` | 3 | 3 | 0 | 0 | 0 | 0 |
| `nova_tests/plan34` | 8 | 8 | 0 | 0 | 0 | 0 |
| `nova_tests/plan36_d1` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan48_1` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan48_mpm` | 7 | 0 | 5 | 0 | 0 | 2 |
| `nova_tests/plan55` | 20 | 2 | 18 | 0 | 0 | 0 |
| `nova_tests/plan56` | 7 | 7 | 0 | 0 | 0 | 0 |
| `nova_tests/plan57` | 11 | 11 | 0 | 0 | 0 | 0 |
| `nova_tests/plan59` | 40 | 38 | 2 | 0 | 0 | 0 |
| `nova_tests/plan60` | 6 | 0 | 3 | 0 | 0 | 3 |
| `nova_tests/plan61` | 7 | 7 | 0 | 0 | 0 | 0 |
| `nova_tests/plan62` | 34 | 3 | 31 | 0 | 0 | 0 |
| `nova_tests/plan63` | 4 | 2 | 0 | 2 | 0 | 0 |
| `nova_tests/plan65` | 19 | 2 | 16 | 0 | 0 | 1 |
| `nova_tests/plan67` | 14 | 14 | 0 | 0 | 0 | 0 |
| `nova_tests/plan70` | 24 | 5 | 19 | 0 | 0 | 0 |
| `nova_tests/plan70_1` | 5 | 5 | 0 | 0 | 0 | 0 |
| `nova_tests/plan70_2` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan72` | 16 | 15 | 1 | 0 | 0 | 0 |
| `nova_tests/plan73` | 25 | 11 | 14 | 0 | 0 | 0 |
| `nova_tests/plan74` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan76` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan77` | 7 | 7 | 0 | 0 | 0 | 0 |
| `nova_tests/plan79` | 25 | 20 | 5 | 0 | 0 | 0 |
| `nova_tests/plan81` | 14 | 10 | 0 | 4 | 0 | 0 |
| `nova_tests/plan82_2` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan83_10` | 20 | 9 | 0 | 11 | 0 | 0 |
| `nova_tests/plan83_10_3` | 3 | 3 | 0 | 0 | 0 | 0 |
| `nova_tests/plan83_10_4` | 3 | 0 | 0 | 0 | 0 | 3 |
| `nova_tests/plan83_11` | 4 | 0 | 4 | 0 | 0 | 0 |
| `nova_tests/plan83_12` | 10 | 0 | 10 | 0 | 0 | 0 |
| `nova_tests/plan83_4_5_6_stress` | 3 | 0 | 3 | 0 | 0 | 0 |
| `nova_tests/plan83_6` | 3 | 3 | 0 | 0 | 0 | 0 |
| `nova_tests/plan83_7` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan83_stress_armed` | 5 | 0 | 5 | 0 | 0 | 0 |
| `nova_tests/plan84` | 14 | 14 | 0 | 0 | 0 | 0 |
| `nova_tests/plan87` | 6 | 6 | 0 | 0 | 0 | 0 |
| `nova_tests/plan88` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/plan89` | 7 | 7 | 0 | 0 | 0 | 0 |
| `nova_tests/plan90` | 9 | 9 | 0 | 0 | 0 | 0 |
| `nova_tests/plan90_1` | 21 | 21 | 0 | 0 | 0 | 0 |
| `nova_tests/plan91` | 2 | 0 | 2 | 0 | 0 | 0 |
| `nova_tests/plan91_10` | 1 | 1 | 0 | 0 | 0 | 0 |
| `nova_tests/plan91_12` | 13 | 13 | 0 | 0 | 0 | 0 |
| `nova_tests/plan91_13` | 9 | 9 | 0 | 0 | 0 | 0 |
| `nova_tests/plan91_14` | 21 | 20 | 1 | 0 | 0 | 0 |
| `nova_tests/plan91_15` | 4 | 4 | 0 | 0 | 0 | 0 |
| `nova_tests/plan91_7` | 5 | 5 | 0 | 0 | 0 | 0 |
| `nova_tests/plan91_8a` | 2 | 0 | 2 | 0 | 0 | 0 |
| `nova_tests/plan91_8a_2` | 26 | 4 | 22 | 0 | 0 | 0 |
| `nova_tests/plan91_8b` | 6 | 6 | 0 | 0 | 0 | 0 |
| `nova_tests/plan91_8c` | 14 | 1 | 13 | 0 | 0 | 0 |
| `nova_tests/plan91_8c_direct` | 5 | 1 | 4 | 0 | 0 | 0 |
| `nova_tests/plan91_fe1` | 10 | 1 | 9 | 0 | 0 | 0 |
| `nova_tests/plan91_fe2` | 10 | 10 | 0 | 0 | 0 | 0 |
| `nova_tests/plan91_fe4` | 10 | 0 | 10 | 0 | 0 | 0 |
| `nova_tests/plan91_fe5` | 5 | 5 | 0 | 0 | 0 | 0 |
| `nova_tests/plan95` | 6 | 6 | 0 | 0 | 0 | 0 |
| `nova_tests/plan95bis` | 5 | 1 | 4 | 0 | 0 | 0 |
| `nova_tests/plan96` | 23 | 23 | 0 | 0 | 0 | 0 |
| `nova_tests/plan96_1` | 3 | 3 | 0 | 0 | 0 | 0 |
| `nova_tests/plan97` | 23 | 23 | 0 | 0 | 0 | 0 |
| `nova_tests/plan98` | 5 | 5 | 0 | 0 | 0 | 0 |
| `nova_tests/plan99` | 12 | 11 | 0 | 0 | 0 | 1 |
| `nova_tests/plan_parser_recsep` | 4 | 4 | 0 | 0 | 0 | 0 |
| `nova_tests/plan_value_iter` | 4 | 4 | 0 | 0 | 0 | 0 |
| `nova_tests/protocols` | 20 | 17 | 3 | 0 | 0 | 0 |
| `nova_tests/rebind` | 4 | 4 | 0 | 0 | 0 | 0 |
| `nova_tests/recursive_mono` | 3 | 3 | 0 | 0 | 0 | 0 |
| `nova_tests/runtime` | 18 | 0 | 18 | 0 | 0 | 0 |
| `nova_tests/runtime_panics` | 14 | 14 | 0 | 0 | 0 | 0 |
| `nova_tests/self_nested` | 6 | 6 | 0 | 0 | 0 | 0 |
| `nova_tests/std_hygiene` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/str` | 14 | 2 | 12 | 0 | 0 | 0 |
| `nova_tests/strings` | 122 | 28 | 94 | 0 | 0 | 0 |
| `nova_tests/sync` | 39 | 15 | 24 | 0 | 0 | 0 |
| `nova_tests/syntax` | 54 | 0 | 54 | 0 | 0 | 0 |
| `nova_tests/syntax_probe` | 2 | 2 | 0 | 0 | 0 | 0 |
| `nova_tests/types` | 21 | 0 | 21 | 0 | 0 | 0 |
| `nova_tests/unicode` | 54 | 10 | 44 | 0 | 0 | 0 |
| `nova_tests/vec_elem_type` | 4 | 4 | 0 | 0 | 0 | 0 |
| `nova_tests/xmodule_struct_variant_ctor_a` | 1 | 0 | 1 | 0 | 0 | 0 |
| `nova_tests/xmodule_struct_variant_ctor_b` | 1 | 0 | 1 | 0 | 0 | 0 |
| `nova_tests/xmodule_struct_variant_ctor_test.nv` | 1 | 1 | 0 | 0 | 0 | 0 |

## Заметки по крупным/примечательным директориям

- **`nova_tests/contracts`** (308 файлов, крупнейшая) — 14 KEEP-SPECIAL
  (soundness-ratchet), 76 MIGRATE, 218 STALE. Большая доля STALE — это НЕ обязательно
  «протухший тест»: часть FAIL внутри contracts — z3-зависимые доказательства, упавшие
  под **trivial-backend** прогоном (`SKIP requires NOVA_SMT_BACKEND=z3` встречается тоже,
  такие уже в MIGRATE), часть — настоящий дрейф (retired API). Перед Ф.2-сносом
  **обязательно перепрогнать contracts под `NOVA_SMT_BACKEND=z3`** — цифры STALE тут
  вероятно завышены относительно z3-прогона.
- **`nova_tests/basics`** — ВСЕ 8 файлов STALE: единый merged-модуль не C-компилируется
  (`error: use of undeclared identifier 'apply'` в сгенерированном .c). Не ICE (процесс не
  падает), обычный CC-FAIL — но воспроизводится стабильно. Похоже на реальный codegen-баг
  (не просто retired API) — стоит отдельно завести в Ф.4c-очередь или разобрать в Ф.2.
- **`nova_tests/syntax`**, **`nova_tests/types`**, **`nova_tests/runtime`** — 0% MIGRATE
  (все юниты FAIL). Не ICE — обычные CODEGEN-FAIL/CC-FAIL с разными причинами (см. TSV);
  выглядит как систематический дрейф этих директорий на retired/changed API.
- **`nova_tests/sync`** — см. касту про DUPLICATE выше: MIGRATE+consolidate в
  `std/runtime/sync_test.nv`, не отдельный DUPLICATE-снос.
- **`nova_tests/modules`**, **`nova_tests/cfg`**, **`nova_tests/doc`**,
  **`nova_tests/negative_capability`**, **`nova_tests/plan03_1`**, **`nova_tests/plan83_10`**,
  **`nova_tests/plan162*`** — высокая доля N/A-support: это намеренно-мультимодульные
  тест-сценарии (module-system / capability / path-dependency фичи), где «файл» и
  «тестируемый юнит» не совпадают 1:1 по конструкции. Мигрировать их нужно ЦЕЛЫМИ
  поддеревьями, не файл-за-файлом.

## Известные ограничения этого прохода

1. DUPLICATE-детект не автоматизирован надёжно (см. касту выше) — потенциально часть
   MIGRATE на самом деле DUPLICATE/merge-candidate относительно `std/**/*_test.nv`;
   решать per-directory в Ф.2.
2. `contracts/` прогнан под **trivial GC/SMT backend** (дефолт), НЕ под z3 — часть STALE
   там может оказаться MIGRATE/PASS под `NOVA_SMT_BACKEND=z3`. Требуется z3-перепрогон
   перед Ф.2-решениями по contracts/.
3. 13 файлов не скомпилированы вообще (ICE) — их реальный вердикт (STALE vs MIGRATE)
   неизвестен до починки Ф.4c.
4. STALE-причины не были все вручную root-caused — TSV-note содержит точный текст ошибки
   компилятора для каждого файла, финальная классификация (retired-API vs codegen-баг)
   — задача Ф.2/Ф.3.
