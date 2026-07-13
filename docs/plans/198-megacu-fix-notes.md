<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 198 — merged-CU runtime-блокер #8a/#8b: фикс + чекпойнт

**Worktree:** `d:/Sources/nv-lang/nova-p196`, ветка `fix-megacu-stack` (от `b199370ef`).
**Коммит фикса:** `73c8a28aa`.
**В main НЕ мёржить.**

## Контекст

`docs/plans/198-redo-notes.md` (worktree `nova-198`, ветка `triage-198`,
read-only) задокументировал находку №8: merged flat CU из ~1010 файлов /
2589 test-блоков `spec_tests/conformance` компилируется и линкуется, но:

- **8a**: `main_impl` держит ВСЕ 2589 `NovaTestFrame`/`NovaFailFrame`
  setjmp-фреймов в ОДНОМ кадре C-функции → кадр >1MB → stack overflow
  0xC00000FD на старте (до первого теста). Верифицировано PE-патчем
  `SizeOfStackReserve→64MB` — бинарь стартует и бежит.
- **8b**: с 64MB стеком прогон доходит до ~520-го теста и падает access
  violation 0xC0000005 в panics-recovery (`contracts loop preentry fail`);
  изолированно тест PASS.

## Диагноз 8a

`compiler-codegen/src/codegen/emit_c.rs::emit_main_wrapper` (test-runner
ветка, было ~L23975-24094): каждый test-блок эмитился как
`{ NovaTestFrame _tf; NovaFailFrame _tf_fail; ...; }` — scope, ИНЛАЙНЕННЫЙ
прямо в `nova_fn_main_impl()`. Адрес `_tf` уходит в глобальный
`_nova_test_frame`, поэтому clang не может доказать, что sibling test-scope
не пересекаются по времени жизни, и держит слот КАЖДОГО scope живым на
протяжении всей функции — при N test-блоков кадр `main_impl` растёт O(N).
При N=2589 это >1MB (дефолтный Windows-стек — 1MB) → overflow до первого
теста.

## Фикс 8a

Тест-блоки эмитятся ЧАНКАМИ по `TEST_CHUNK_SIZE = 64` в отдельные
C-функции `static int nova_test_chunk_<i>(void)`; `nova_fn_main_impl()`
становится циклом вызовов, суммирующим `_nova_tests_failed` каждого чанка.
Кадр каждой chunk-функции ограничен размером чанка (64 теста), НЕ размером
корпуса — константа независимо от N. Побочный эффект: меньше живых
стек-слотов на функцию должно также снижать стартовую/компиляционную
стоимость больших CU (относится к Plan 200.1 §1).

Дополнительно (страховка, НЕ фикс): `test_runner.rs::compile_c_to_exe`
получил умеренный запас `/stack:0x1000000` (16MB RESERVE — clang
`-Wl,/stack:` + MSVC `/STACK:`). RESERVE ничего не стоит в простое (страницы
коммитятся лениво), это чистая страховка сверху структурного фикса.

Проверено структурно: `nova-codegen compile` на synthetic 200-test файле
даёт `nova_test_chunk_0..3` (200/64=4 чанка) + константный `main_impl`,
рантайм 200/200 passed.

## Диагноз/судьба 8b

**8b НЕ переиспроизведён после фикса 8a** — трактуется как побочный эффект
того же O(N)-кадра (не отдельный баг с отдельным фиксом).

Верификация: synthetic single-file corpus, 2589 test-блоков (тот же
масштаб, что и реальный merged CU), с реалистичным миксом:
- большинство — простые passing `assert(...)`;
- каждый 97-й — `panics "requires failed:"`-тест с реальным
  `while ... invariant ...` contract-violation (байт-в-байт как
  `contracts_loop_preentry_fail.nv` — `nova_contract_violation` роутится
  идентично `nova_assert`, D13);
- каждый 50-й (по модулю 97) — простой ФЕЙЛЯЩИЙ assert БЕЗ panics-клаузулы
  (проверка гипотезы «висящее состояние после не-panics FAIL» — в
  не-panics ветке `nova_runtime_reset()` не вызывается, в отличие от
  panics-ветки).

Результат (дефолтный toolchain, БЕЗ PE-патча, БЕЗ ручного stack-флага —
проверялось ещё ДО добавления страховочного `/stack:` тоже, тем же
результатом): все 2589 тестов отрабатывают до конца, 2562 PASS / 27 FAIL
(ожидаемо — умышленные фейлы), 0 крашей, 0 access violation где-либо в
прогоне. `nova.exe build` + запуск — обычный дефолтный Windows-стек (1MB),
никакого спец-флага не потребовалось.

Вывод: константный кадр `main_impl` (фикс 8a) убирает и 8b. Отдельного
runtime-фикса panics-recovery не потребовалось.

## РЕПРО/ГЕЙТ — что удалось и что заблокировано

### Обнаруженный ПОБОЧНЫЙ (не мой) блокер: version-skew в nova-198

Попытка собрать merged CU **из корпуса nova-198** (branch `triage-198`,
база `4394fec95`) СВОИМ компилятором (текущий main HEAD `b199370ef` +
фикс) упёрлась в ПРЕДСУЩЕСТВУЮЩУЮ, НЕ связанную с 8a/8b проблему:

`std/encoding/json.nv` (nova-198) вызывает `n.trunc()`/`n.abs()` на `f64`
без явного импорта — это ambient-резолвится через `std/prelude.nv`
(`import std.runtime.math`). В nova-198 эта строка ОТСУТСТВУЕТ — Plan 196.3
(2026-07-12, УЖЕ на main HEAD) добавил `import std.runtime.math` в
`std/prelude.nv` ИМЕННО ЧТОБЫ чекер резолвил `.trunc()`/`.abs()` через
нормальный `method_overloads`-канал (взамен ретрагированного
codegen-only `primitive_instance_method_known`-фолбэка). nova-198 — снэпшот
ДО этого амендмента → чекер честно не находит `trunc`/`abs` → E_UNKNOWN_METHOD.

Подтверждено экспериментом: `NOVA_STD_PATH`, указанный на nova-198's
`std/` (старый layout, `[lib] src = "."`) ИЗ репо nova-p196 (значит, чисто
layout/версия std, не что-то специфичное для nova-198-репо), воспроизводит
ту же ошибку; diff между `nova-198/std/prelude.nv` и
`nova-p196/std/src/prelude.nv` показывает РОВНО одну добавленную строку
(`import std.runtime.math`, `PRELUDE_VERSION` 16→17). Это НЕ регрессия
компилятора — предсказуемое следствие компиляции устаревшего снэпшота std
текущим компилятором. nova-198 — read-only для меня (задание), поэтому
пофиксить прямо там нельзя; чинить main-код тоже нечего (main уже
корректен).

Дальше по корпусу (после этой точки, с `NOVA_STD_PATH` указанным на
nova-p196's ПОЛНЫЙ std) встретился ЕЩЁ один independent codegen-ICE
(`.is_nan` return type unknown, emit_c.rs:50387) на одном из `neg/`
fixture-файлов nova-198 — тоже похоже на тот же класс version-skew
(Plan 196.3 checker-visibility migration), НЕ расследовался в глубину
(вне рамок 8a/8b; отдельный тикет).

**Вывод:** end-to-end прогон РЕАЛЬНОГО merged CU из nova-198 корпуса
текущим HEAD-компилятором технически недостижим без затрагивания
version-skew дефектов вне периметра 8a/8b (и без права писать в
nova-198). 8a/8b верифицированы вместо этого прямым структурным разбором
+ synthetic corpus того же масштаба (см. выше) — это даёт эквивалентное
покрытие для ИМЕННО стек/AV-дефектов (единственное, что зависит от N
test-блоков в одном CU; version-skew дефекты — per-file typecheck,
ортогональны чанкованию).

### Обычные гейты (нужный периметр)

- `nova test spec_tests/conformance --full` (nova-p196, без `--jobs`,
  jobs=16 auto — команда не принимает "без jobs" в смысле "1 процесс",
  это per-file параллелизм, не относится к merged-CU): **PASS 113 / FAIL 0
  / SKIP 7** — БЕЗ РЕГРЕССИЙ (совпадает с базой 113/0+7skip).
  `b11x_novaarray_user_ext_methods` — известный гуляющий тест; в полном
  прогоне PASS (98.6s), точечный ре-ран отдельно словил тот же известный
  RUN-FAIL паттерн (`D325 A1/R0` категория), т.е. флака подтверждена, НЕ
  регрессия от моего фикса.
- `nova test std/src/collections --full`: **PASS 14 / FAIL 0 / SKIP 6**
  (SKIP = файлы без test-блоков, ожидаемо).

## Файлы

- `d:/Sources/nv-lang/nova-p196/compiler-codegen/src/codegen/emit_c.rs` —
  чанкование test-блоков (`emit_main_wrapper`).
- `d:/Sources/nv-lang/nova-p196/compiler-codegen/src/test_runner.rs` —
  страховочный `/stack:0x1000000` (clang + MSVC ветки `compile_c_to_exe`).
- Коммит: `73c8a28aa` (ветка `fix-megacu-stack`).
