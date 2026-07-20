# [M-imports-order-dependent-cycle] — заметки

Worktree: `nova-orderimp`, ветка `p-fix-order-imports`. Модель: sonnet.
Приоритет №1 владельца (найдено Ф4R-волной, план 208 §10R Ш2-стоп).

## Симптом (из `docs/plans/wip/208-f4r-notes.md`, секция Ш1/Ш2)

Двусторонний межмодульный import-цикл `A ↔ B` + ТРЕТИЙ файл `C`, импортирующий
имена из ОБОИХ модулей двумя ОТДЕЛЬНЫМИ top-level `import`-строками — итог
(PASS/CODEGEN-FAIL) зависит от ТЕКСТОВОГО ПОРЯДКА этих двух строк в `C`.
Реальный прецедент: `runtime.fmt_buf ↔ runtime.string_builder` (Ф.4R Ш1
v1-архитектура) — `import fmt_buf` перед `import string_builder` →
CODEGEN-FAIL (`undefined identifier int_fmt_into` внутри `string_builder.nv`);
swap → PASS.

## Шаг 1 — репро подтверждено

Минимальный 3-файловый кейс (asymmetric — только один модуль реально ЗОВЁТ
функцию другого, что и делает баг PASS/FAIL-переключаемым, а не двойным
провалом): `scratch/repro_asym{1,2}/{a,b,c}.nv` (временные, не в репо) +
постоянная фикстура `spec_tests/conformance/order_dependent_cycle/`
(`odc_a.nv`, `odc_b.nv`, `odc_c1.nv`/`odc_c2.nv` — тот же A/B, третий файл в
двух порядках).

Подтверждено НА НЕТРОНУТОМ бинаре (`d:\Sources\nv-lang\nova\nova-cli\target\
release\nova.exe`, main-репо, ДО фикса):
- `odc_c1.nv` (`odc_a` затем `odc_b`) → **CODEGEN-FAIL**:
  `odc_b.nv:17:36: undefined identifier 'odc_a_val'`.
- `odc_c2.nv` (`odc_b` затем `odc_a`) → **PASS**.

Тот же порядко-зависимый флип воспроизведён также симметричным repro
(`repro_order1`/`repro_order2` — оба направления реально зовут друг друга) —
там оба порядка ПАДАЮТ (просто на разных строках), что показывает: баг
общий (пустой `visible_acc` на «проигравшей» стороне цикла), а
PASS/FAIL-специфика конкретно от АСИММЕТРИИ реальной потребности (одна
сторона тянет только TYPE, резолвящийся глобально через `TypeMethodMap`/
type-table — не зависит от `visible_acc`; другая тянет FREE FUNCTION,
которая зависит).

## Шаг 2 — корень

`compiler-codegen/src/imports.rs`, `resolve_one`. DFS: `in_progress.insert
(module_key)` ставится ПЕРЕД циклом по `resolved_paths` (файлам модуля), но
`module_exports_cache` (итоговый список экспортов, который в конце кладётся
в `visited`) СОБИРАЕТСЯ только В КОНЦЕ — в merge-цикле `for item in
peer_module.items` (~строка 1927 ДО фикса), который выполняется ПОСЛЕ
рекурсивного резолва СОБСТВЕННЫХ импортов этого же peer'а (~строка 1849).
Итог: пока `resolve_one(M)` ещё не дошёл до merge-фазы, `M` числится
`in_progress`, но `visited` про него ничего не знает — хотя `peer_module.
items` (и, значит, полный список экспортируемых имён — они вычислимы БЕЗ
резолва импортов, `exported_names_from_items` уже существовала для
entry-фикса) был доступен СРАЗУ после парсинга, в начале итерации.

Если во время резолва СОБСТВЕННЫХ импортов `M` встречается обратная ссылка
на `M` (цикл) — `resolve_one`'s cycle-guard (`in_progress.contains`) стреляет
и возвращает `Ok(())` с ПУСТЫМ `visible_acc` для этого edge — даже если `M`'s
экспорты давно известны (просто не были сохранены в `visited` заранее).

Порядко-зависимость (в `C`, который делает ДВА отдельных top-level import):
какой из `A`/`B` DFS входит первым (текстовый порядок import-строк `C`)
решает, кто из двух окажется «снаружи» (root поддерева, где back-edge
цикла попадает на цикл-guard) — и получает пустой `visible_acc` для
СВОЕГО экспорта на СТОРОНЕ, который другой модуль реально использует.

Родственный баг: `[M-imports-entry-folder-module-self-cycle-empty-exports]`
(закрыт 2026-07-20) чинил ИМЕННО ЭТУ схему, но только для CU **entry**
(сеял `entry_key`'s экспорты в `visited` ДО резолва импортов). Этот баг —
тот же класс для ЛЮБОГО (не-entry) модуля.

## Шаг 3 — решение по спеке (ключевой выбор волны)

`spec/decisions/07-modules.md` → **D291** («Module resolution —
collect-signatures-first, lazy bodies; cross-module cycles allowed»,
СТАТУС: принято/реализовано, Plan 162): «Cross-module cycles разрешены
(как peer-циклы в Rule D). Amended Plan 42 Rule A (cycle-detection заменена
cycle-guard)». Это ЯВНО амендит более старый текст D29 rev-1 («Циклические
импорты — запрещены», строки 549-563/615/624-625 того же файла — СТАРЫЙ,
не обновлённый после D291 текст-остаток, НЕ актуальная норма). D291 также
явно фиксирует АРХИТЕКТУРНЫЙ принцип — «collect-signatures-first, lazy
bodies» — то есть СИГНАТУРЫ (а значит и экспорт-имена) обязаны быть
известны ДО того, как тела/импорты резолвятся лениво. Фактическая
реализация ДО этой волны нарушала собственный принцип D291: экспорт-имена
собирались НЕ «first», а только post-recursion.

**Выбор: вариант «отдавать exports»** (не вводить `E_IMPORT_CYCLE`).
Причины: (1) D291 прямо разрешает циклы — вводить новую хард-ошибку было бы
язык-меняющей РЕТРАКЦИЕЙ уже принятого решения, не тем, что просили; (2)
фикс делает компилятор фактически соответствующим уже продекларированной
архитектуре («collect-signatures-first»), а не новой семантикой.

## Шаг 4 — фикс

`compiler-codegen/src/imports.rs`:
1. `exported_names_from_items` (раньше — locale-fn внутри
   `resolve_imports_inline_ex`, использовалась только для entry-фикса)
   поднята на top-level (`pub(crate) fn`, перед `resolve_one`) — переиспользуется.
2. В `resolve_one`, в цикле `for peer_path in &resolved_paths` (peer-файлы
   ОДНОГО резолвимого модуля), СРАЗУ после парсинга `peer_module` (и
   `cfg_active`-фильтра) и ДО рекурсивного резолва этого peer'а
   собственных импортов — новая вставка:
   ```rust
   let peer_export_names = exported_names_from_items(&peer_module.items);
   visited.entry(module_key.clone())
       .or_insert_with(Vec::new)
       .extend(peer_export_names);
   ```
   Это ПРОДОЛЖАЕТ расти по мере обработки peer'ов того же (folder-)модуля;
   финальная запись `visited.insert(module_key, module_exports_cache)` в
   конце `resolve_one` (без изменений) полностью ЗАМЕЩАЕТ провизорную —
   контент идентичен (та же логика `is_export`/`module_has_exports`), так
   что никакого рассинхрона не остаётся. `module_key` уже в `in_progress`
   в этот момент — прецедент «в обоих множествах одновременно» уже
   установлен entry-фиксом (visited-check стоит ПЕРЕД in_progress-check).
3. Известное ограничение (не хуже, чем ДО фикса; НЕ регрессия): для
   MULTI-PEER folder-модуля, если цикл замыкается ДО того, как все peers
   модуля распарсены (peer 1 ещё резолвит СВОИ импорты, peer 2+ ещё не
   парсились) — провизорный `visited`-список НЕПОЛНЫЙ (только peer'ы,
   обработанные к этому моменту). Строго монотонное улучшение (раньше было
   ПУСТО всегда), не полный фикс для этого более узкого вложенного случая.

## Побочный эффект — ПРЕДВИДЕННЫЙ, обсуждён с координатором

Existing neg-фикстура `spec_tests/conformance/entry_self_cycle/
{cyc_a,cyc_b,cycle_test}.nv` (закрыта `[M-imports-entry-folder-module-
self-cycle-empty-exports]`, 2026-07-20) кодировала ИМЕННО структурно
идентичный кейс (`cyc_a` → `cyc_b` → `cyc_a`-back-edge, `cyc_b.b_calls_a`
зовёт `cyc_a.a_val`) как НЕГАТИВНЫЙ (`EXPECT_COMPILE_ERROR`) — то есть тот
самый баг, зафиксированный как «защищённое поведение», хотя D291 говорит,
что цикл должен работать. Структурно неотличимо от нашего repro (ни `cyc_a`,
ни модуль-виновник в fmt_buf-кейсе не entry) — общий фикс НЕ может починить
один кейс, оставив другой сломанным (это ОДНА и та же ветка кода). После
обсуждения с координатором (подтверждено: «cycle_test позитивным PASS —
значит выбор «отдавать exports участникам цикла»») фикстура
**ПЕРЕВЕДЕНА В ПОЗИТИВНУЮ**: `EXPECT_COMPILE_ERROR`-маркер снят, тест
теперь `assert(a_val() == 3); assert(b_calls_a() == 3)` — оба PASS.
Комментарии во всех трёх файлах обновлены с explicit историей находки.

## Верификация

- `nova test` (собственный релизный бинарь worktree) на repro (asym1/asym2,
  order1/order2) — ОБА порядка → PASS на фикс-бинаре; на нетронутом
  main-бинаре — order-dependent PASS/FAIL флип подтверждён (baseline).
- `spec_tests/conformance/order_dependent_cycle/` (новая фикстура,
  `odc_c1`/`odc_c2`) — PASS 2/0 на фикс-бинаре; на baseline-бинаре —
  `odc_c1` CODEGEN-FAIL / `odc_c2` PASS (тот же флип, подтверждает фикстура
  действительно ловит баг).
- `spec_tests/conformance/entry_self_cycle/cycle_test` — теперь PASS
  (позитивный); `--compile-error` лейн для этой папки — пусто (нет больше
  EXPECT_COMPILE_ERROR маркеров там).
- `std/src/checksums` — PASS 3/0.
- `std/src/collections` — PASS 13/0.
- Флагман `examples/flagship/aggregator/src/main.nv --strict-effects` —
  built чисто (51.9s), только предсущ. unused-import warnings.
- Folder-CU `spec_tests/conformance` (главный риск-гейт, ~169 файлов) —
  см. финальный отчёт (гоняется в фоне из-за длительности >10 мин на
  разделяемой CPU с параллельной сессией другого агента в main-репо).

## Коммиты

См. финальный отчёт волны.
