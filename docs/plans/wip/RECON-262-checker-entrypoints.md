# Recon — Plan 262 часть А: инвентарь точек входа чекера

Working notes for window `p262-a-pipeline`. Not a plan file — checkpoint scratch,
per coordinator instruction (network drop risk today, checkpoint every
established fact).

## Метод

`grep -rn` по всему дереву (кроме `target/`) на:
- `check_module(`, `check_module_with_sig_table(`, `check_module_with_expr_types(`,
  `check_module_with_expr_types_ide(`, `check_module_impl(` — определены в
  `compiler-codegen/src/types/mod.rs`.
- Проходы, которые должна собрать unified-функция: `resolve_imports_inline[_ex](`,
  `embed_resolve::resolve_embeds(`, `desugar::desugar_module(`,
  `alpha_rename::alpha_rename(`, `number_exprs::number_exprs(`.

## Определения (types/mod.rs)

- `check_module_impl` (:1035) — реальный воркер; параметризован `Option<&SigTable>`
  и `record_expr_types: bool`.
- `check_module` (:966) → `check_module_impl(module, None, false)`
- `check_module_with_expr_types` (:978) → `check_module_impl(module, None, true)`
- `check_module_with_expr_types_ide` (:996) → `check_module_impl(module, None, true).0`
- `check_module_with_sig_table` (:2161) → `check_module_impl(module, Some(&sig_table), false)`

Эти четыре свободные функции — законный узкий контракт (какой sig-table,
нужны ли expr-types); НЕ то, что объединяет А.1-bis. Объединяется НАБОР
ПРОХОДОВ ДО них (resolve/embed/desugar/alpha_rename/number_exprs), а какую из
четырёх звать — остаётся решением каждой точки входа (сводится к «есть ли
sig_table» и «нужны ли expr_types для IDE hover/inlay»).

## Точки входа — вызовы check_module* напрямую (кроме types/mod.rs internal tests/impl)

| # | Файл:строка | Контекст | Проходы ДО check_module* | Вердикт |
|---|---|---|---|---|
| 1 | `nova-cli/src/main.rs:2503-2504` | `check_one_file` = `nova check` (`cmd_check`) | `check_module_path`(D78) → `resolve_imports_inline_ex(.., true)` + `collect_all_signatures` → `resolve_embeds` → `alpha_rename` → `number_exprs` → **эталон**, потом `infer_effects`+`lint_module` (после check, не входит в prepare) | ЭТАЛОН — эту последовательность резолва копирует prepare-функция |
| 2 | `nova-cli/src/main.rs:5190` | `cmd_build` (`nova build`, cache-miss ветка) | `resolve_imports_inline` (test_peers=**false**) → `inject_synthesized_methods_filtered(Serialize/Deserialize)` → `alpha_rename` → `number_exprs` → `check_module` (БЕЗ sig_table — весь граф уже инлайнен, sig_table не нужен) | ЗАКОНЕН другой набор: (а) test_peers=false — production-код, тестовые peer-файлы не часть сборки; (б) embed_resolve вызывается РАНЬШЕ (:5058, до auto-derive-inject), а не между resolve/alpha_rename как в check — порядок относительно auto-derive не совпадает 1:1, нужно решить куда встраивать auto-derive-inject в prepare или оставить его отдельным вызовом ПОСЛЕ prepare (см. ниже); (в) НЕТ sig_table (`check_module`, не `_with_sig_table`) — корректно, single-CU build видит все символы напрямую |
| 3 | `compiler-codegen/src/test_runner.rs:4320-4321` | `codegen_to_c` = ядро `nova test` / `nova build` через test-раннер | `resolve_imports_inline_ex(.., true)` (:4213) → `resolve_embeds` (:4273) → `alpha_rename` (:4300) → `number_exprs` (:4308) → `check_module_with_sig_table`/`check_module` (:4320) → `desugar_module` (:4406, ПОСЛЕ check!) | ВАЖНО: здесь `desugar_module` вызывается ПОСЛЕ type-check, не до — порядок в `test_runner.rs` расходится с `cmd_build`, где desugar тоже после check (см. cmd_build :5273, тоже после). Значит desugar_module ВЕЗДЕ, где смотрел, идёт ПОСЛЕ check_module, не до. Гипотеза плана про «desugar_module не запускается вовсе» у LSP нужно перепроверить: возможно LSP краснеет не из-за отсутствия desugar ДО check, а из-за того что checker сам обязан понимать `MapLit` без десугаринга (десугар — это pre-codegen шаг, не pre-check). ТРЕБУЕТ ДОПРОВЕРКИ прямо на файле-носителе перед тем как класть desugar в prepare. |
| 4 | `nova-lsp/src/compiler.rs:251-252` | `check_source_inner` — ЕДИНСТВЕННЫЙ путь LSP (`check_file`/`check_workspace`/`check_open_documents` все идут через него) | `resolve_imports_inline` (test_peers=**false**, ЖЁСТКО — не параметризовано) → (НЕТ resolve_embeds) → `alpha_rename` → `number_exprs` → check | БАГ (подтверждён №531): test_peers всегда false → `*_test.nv` не видит свои `_test.nv` соседей → `must_io`/`must`/`build_snapshot`/`sse_event` undefined. `resolve_embeds` не вызывается вовсе → `embed(...)` остаётся вызовом функции, не HexBlobLit → undefined identifier embed. Нужно чинить: test_peers по признаку пути (`_test.nv` suffix) или всегда true (cmd_check тоже всегда true и это безопасно per коммент в main.rs) + добавить resolve_embeds. |
| 5 | `nova-lsp/src/provenance.rs:219-221` | `check_module_guarded` — hover/inlay-types provenance (`check_module_with_expr_types_ide`) | `resolve_imports_inline_guarded`(:192, wraps `resolve_imports_inline`, test_peers ЗАФИКСИРОВАН где-то внутри — проверить) → `alpha_rename`(:142) → (нет number_exprs? нет embed_resolve? — ДОПРОВЕРИТЬ построчно) → `check_module_with_expr_types_ide`/`check_module` | ТРЕБУЕТ ДОЧТЕНИЯ файла целиком — не дочитан из-за обрыва сессии |
| 6 | `nova-lsp/src/semantic_tokens.rs:1178-1179,1184` | semantic-token classification (уточнение подсветки) | `alpha_rename`(:1178) → `number_exprs`? (не найден в этом файле — ДОПРОВЕРИТЬ) → `desugar_module`(:1184, ПОСЛЕ `check_module.is_err()` на :1179!) → | Похоже на ТОТ ЖЕ класс бага: desugar здесь вызывается уже ПОСЛЕ check_module — то есть либо намеренно (semantic tokens не для диагностики, только для покраски токенов после успешного check), либо баг. ДОПРОВЕРИТЬ. |
| 7 | `nova-lsp/src/server.rs:2276-2277, 2382-2386, 2504-2508, 2576-2580` | 4 отдельных вызова (какие фичи? — hover/completion/signature-help/code-action? ДОПРОВЕРИТЬ имена функций) | `alpha_rename` → `desugar_module` → `check_module(...).is_err() { return None }` — ПОРЯДОК ТУТ desugar ДО check (в отличие от semantic_tokens.rs) | Непоследовательность порядка desugar/check МЕЖДУ файлами nova-lsp — сам по себе довод для prepare-функции: 4 места, 2 разных порядка. ДОПРОВЕРИТЬ какие это фичи и не расходятся ли они законно. |
| 8 | `compiler-codegen/src/doc/test_runner.rs:163` | доктест-раннер (`nova doc --run-doc-tests`, `nova doc watch`) | `alpha_rename`(:159) → `number_exprs`(:160) → `check_module`(:163) — уже почищено окном `p-novadoc` тем же утром (по брифу) | ПРОВЕРИТЬ живьём: содержит ли текущий код `resolve_imports_inline`/`resolve_embeds`/`desugar_module` тоже, или только alpha_rename+number_exprs были дофиксены (бриф говорил только про эти два). Если resolve/embed/desugar ещё отсутствуют — тоже носитель класса. |
| 9 | `compiler-codegen/src/doc/watch_cache.rs:85` | doc watch cache invalidation probe (`let _ = check_module(...)`) | ДОПРОВЕРИТЬ проходы перед этим вызовом | результат отбрасывается (`let _ =`) — вероятно используется только как probe "компилируется ли", не как источник диагностики; уточнить, важна ли точность здесь |
| 10 | `compiler-codegen/src/main.rs:299-300,433-434,511` | `nova-codegen` binary (bin target ИЗ `compiler-codegen/Cargo.toml`, НЕ `nova` из `nova-cli`) | СВОЙ pipeline, параллельный nova-cli — ДОПРОВЕРИТЬ, используется ли ещё (найден только 1 живой потребитель: `scripts/tools/setup_worktree_p118.sh`, старый per-plan скрипт) | Легаси/дублирующий бинарь. Решить: включать в unified pipeline или считать нерелевантным (его единственный известный потребитель — исторический скрипт plan118). Указать в отчёте владельцу/интегратору явно — не решать самому, входит ли устаревший бинарь в объём. |
| 11 | `nova-cli/src/bench/run.rs:103,396` + `field_cache_wallclock.rs:319` | `nova bench` (перфоманс-бенчи) | `resolve_imports_inline`(test_peers?) → `desugar_module` → `alpha_rename` → `number_exprs` → `check_module` — порядок ОТЛИЧАЕТСЯ (desugar ДО alpha_rename/number_exprs, а не после check как везде выше) | ЕЩЁ ОДИН порядок desugar — четвёртая вариация после (после-check в test_runner/cmd_build, после-check в semantic_tokens, до-check в server.rs). ДОПРОВЕРИТЬ точный код bench/run.rs. Возможно bench намеренно не идёт в prepare (не диагностический путь, а перфоманс-замер, где важна СКОРОСТЬ конкретной последовательности) — если так, законное расхождение, но нужно решить осознанно. |
| 12 | `nova-cli/src/main.rs:1364,1596,1713,3065,3520,3587,4546,5315` | разные под-команды: `cmd_check_explain_cache`(1528), `cmd_check_telemetry_cache`(1627), doc-related probes(3065,3520,3587), `cmd_consume_analyze`?(4546), другое(5315) | ДОПРОВЕРИТЬ каждую по отдельности — список длинный, не весь дочитан | ОБЪЁМ РАБОТЫ ещё не закрыт полностью |
| 13 | `nova-lsp/tests/diagnostic_pipeline.rs:195-198` | тест САМОГО nova-lsp, дублирует pipeline `compiler.rs` вручную (а НЕ вызывает `compiler::check_source_inner`!) | ручной дубль того же списка проходов, что и compiler.rs (:251-252 идентичны) | Это САМ ПО СЕБЕ носитель класса: тест дублирует пайплайн вместо вызова `check_source_inner`/prepare-функции напрямую → тест не поймает регресс в реальном коде, только в своей копии. Нужно переписать тест на вызов реальной prepare-функции (или хотя бы `compiler::check_source_inner`), не дублировать проходы. |

## Проходы-строители сигнатур (embed/desugar/alpha/number) — где их вообще нет

- `embed_resolve::resolve_embeds` — ПОЛНОСТЬЮ ОТСУТСТВУЕТ в `nova-lsp/**` (ни одного вызова во всём каталоге, проверено грепом). Это системный пробел LSP, не единичный.
- `desugar_module` — вызывается в РАЗНЫХ ПОРЯДКАХ относительно check_module в разных местах (см. таблицу) — нужно установить единственный правильный порядок эталоном (похоже check-до-desugar везде, кроме test_runner/cmd_build, где desugar идёт ПОСЛЕ check, ПЕРЕД codegen). Гипотеза: desugar НЕ обязан быть до check_module с точки зрения корректности типов (MapLit имеет свой путь в checker), а обязан быть до CODEGEN. Если так — «desugar_module не запускается» в LSP может быть НЕ причиной ложной красноты типа (LSP не делает codegen), и симптом «unexpected consume in expression» из плана нужно ПЕРЕВОСПРОИЗВЕСТИ на реальном файле, а не считать доказанным.

## Статус: НЕ ЗАВЕРШЕНО

Ещё не дочитаны: `provenance.rs` целиком, `server.rs` 4 call site (какие фичи),
`semantic_tokens.rs` полностью, `doc/test_runner.rs` текущее состояние (после
фикса `p-novadoc` тем же утром), `bench/run.rs`, `main.rs` (compiler-codegen)
259-520, `nova-cli/main.rs` оставшиеся 8 call site из строки 1364-5315.

Следующий шаг: дочитать по одному, обновляя таблицу, коммитить после каждого
факта.
