<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 215 — персистентный кэш индекса nova-lsp (модель зрелых LSP)

**Статус:** ✅ ЗАКРЫТ 2026-07-19 (sonnet, ветка `p215-lsp-cache`, worktree `nova-lspcache`).
Персистентный кэш `workspace/symbol` + `references`-индекса, тёплый старт из кэша с фоновой
приоритизированной ре-валидацией; юнит- и интеграционные тесты зелёные (см. «Гейт» ниже).
**Продолжение:** [План 104](104-ide-integration.md) (IDE-интеграция, закрыт 2026-06-17) →
[104.10](104.10-lsp-v2-production.md) (LSP V2, выполнен 2026-07-04, завёл открытый маркер
`[M-104.10-persistent-index]`) → [План 213](213-nova-lsp-performance.md) (закрыт 2026-07-17,
починил per-edit CPU-жор полной переиндексацией; этот план — соседняя, ортогональная ось:
не «пересчитываем слишком много на каждую правку», а «пересчитываем всё с нуля на каждый СТАРТ
сервера»). Этот план закрывает `[M-104.10-persistent-index]`.

## Мотив

`nova-lsp` индексирует workspace (~3090 `.nv`-файлов на самом Nova-репо: std/examples/spec_tests/
nova_tests) **с нуля на каждый старт сервера** — `server.rs::run_initial_scan_with_progress`,
вызываемый из `initialized()`, читает и парсит КАЖДЫЙ `.nv`-файл под корнем workspace на
`workspace/symbol` + `textDocument/references` индексы, независимо от того, сколько файлов реально
изменилось со времени предыдущего запуска (обычно — единицы или ноль). Зрелые LSP (rust-analyzer,
gopls, clangd) вместо этого персистят индекс на диск и на следующем старте лишь ре-валидируют
изменившиеся файлы. Владелец явно поставил цель «делать, как в других языках» (2026-07-18).

Отдельно: владелец и раньше выключал `nova-lsp` из-за жора CPU (План 213) — любая новая фоновая
работа обязана быть CPU-скромной (батчи/троттлинг), не повторяя тот инцидент под новым именем.

## Дизайн

### Формат кэша

Один JSON-документ на workspace: `<workspace_root>/target/nova-lsp-cache/index-v1.json`.
`target/` — прецедент компилятора (`nova-cli/src/build_cache.rs::cache_dir` →
`target/.nova-cache/`) и уже покрыт корневым `.gitignore` (паттерн `target/` без якоря) — новой
записи не потребовалось. `serde_json` уже был зависимостью (JSON capability options) — переиспользован
как есть, новый бинарный формат не заводился (см. «Известный компромисс» ниже).

```rust
pub struct PersistedIndex {
    pub format_version: u32,               // CACHE_FORMAT_VERSION = 1
    pub files: HashMap<String, CachedFile>, // "file://..." -> запись
}
pub struct CachedFile {
    pub mtime_nanos: u64,
    pub size: u64,
    pub symbols: Vec<WorkspaceSymbolEntry>,        // workspace/symbol для этого файла
    pub refs: Vec<(String, Vec<Range>)>,           // references: имя -> диапазоны в файле
}
```

`WorkspaceSymbolEntry` (`symbols.rs`) получил `#[derive(Serialize, Deserialize)]` — все его поля
(`SymbolKind`, `Range`, `Url`) уже (де)сериализуемы в `lsp_types`/`url` (это же JSON-RPC wire-типы),
собственная кэш-схема не понадобилась. `serde` добавлен явной зависимостью `nova-lsp` (ранее был
только `serde_json`, `serde`-derive транзитивно тянулся через `tower-lsp`, но не был доступен для
собственных derive).

### Инвалидация — (mtime, size)

Per-file свежесть = `(mtime_nanos, size)`, не хэш содержимого. Компиляторный build-cache
(`nova-cli/src/build_cache.rs`) хэширует контент, потому что его ключ обязан быть стабилен между
машинами/чекаутами; этот кэш — чисто локальный, per-workspace акселератор, где `stat()` на файл
на порядки дешевле хэширования содержимого, и это ровно та же схема, что использует
rust-analyzer/gopls/clangd для того же назначения.

### Тёплый старт (`server.rs::run_initial_scan_with_progress`)

1. `index_cache::load(&root)` — читает+парсит JSON. Промах (нет файла / битый JSON /
   `format_version` не совпадает) → `PersistedIndex::default()`, ничем не отличимо от «кэша нет».
   **Никогда не паникует** — любая порча трактуется как холодный старт (юнит-тест ниже).
2. Путь-only обход `collect_nv_paths(root)` (без чтения содержимого — переиспользован фильтрованный
   обход Плана 213 Ф.1: `target/vcpkg_installed/node_modules`/вложенные git-корни пропускаются).
3. Для каждого файла: `fs::metadata` → сравнение `(mtime, size)` с записью в кэше.
   - Совпало → `WorkspaceIndex::install_file` / `ReferencesIndex::install_file` — установка
     ГОТОВЫХ записей напрямую, БЕЗ парсинга.
   - Не совпало / нет записи → в очередь `to_reindex`.
4. `references_index.mark_primed()` сразу после шага 3 — сервер отвечает на `workspace/symbol` /
   `references` немедленно по (возможно частично устаревшему) кэшу, не дожидаясь довалидации.
5. Реиндексация `to_reindex`: файлы, УЖЕ открытые в редакторе (`state.docs`), **пропускаются** —
   `didOpen`/`didChange` уже переиндексировали их из живого буфера (диск мог не совпадать с
   несохранёнными правками — тот же принцип, что уже задокументирован в
   `workspace_lifecycle::apply_watched_event` про watch-события). **Это и есть механизм «открытые
   документы первыми»**: они не ждут своей очереди в фоновом цикле вообще, независимо от того, где
   цикл сейчас находится.
6. После прохода — `index_cache::save(&root, &new_persisted)`, атомарная запись (temp-файл +
   rename, best-effort — ошибка записи ничего не роняет, просто следующий старт снова холодный).
7. Лог (`tracing::info!`, уровень INFO — виден по умолчанию): `total_files`, `warm_hits`, `stale`,
   `had_cache`, `fingerprint_pass_ms`, затем `reindexed`, `total_elapsed_ms`.

### CPU-скромность фона

Цикл реиндексации — `tokio::task::yield_now().await` после КАЖДОГО файла (кооперативная уступка
tokio-рантайму, чтобы параллельные `didOpen`/hover/etc. не голодали за диспетчером) + активный
`sleep(10мс)` каждые 64 файла (константы `REINDEX_SLEEP_EVERY`/`REINDEX_SLEEP`). Это НЕ то же самое,
что троттлинг Плана 213 Ф.2 (`worker_priority::lower_current_thread_priority`, применим к выделенным
OS-потокам тайпчека под `spawn_blocking`) — здесь работа лёгкая (parse-only, без тайпчека) и идёт
прямо в async-таске на общем tokio-рантайме, так что нужный рычаг — темп, а не приоритет потока.
`debounce` (200мс interactive / 400мс watch-events, Планы 104.1/213) не тронут.

### Явный scope-out

`check_workspace` (полный тайпчек ВСЕХ файлов для initial diagnostics) — **не кэшируется** в этой
волне. Корректно кэшировать diagnostics per-file нельзя без графа обратных зависимостей
(изменение файла-пира может изменить диагностику импортера) — это отдельный уже открытый маркер
`[M-104.10-dependent-invalidation]`, не эта волна. Эта волна = «индекс символов»
(`workspace/symbol` + `references`), ровно то, что было OPEN-промаркировано
`[M-104.10-persistent-index]`.

## Найденный по ходу баг (не гипотеза — пойман замером)

Первая версия `ReferencesIndex::export_file` (нужен для чтения per-file вклада перед записью в
кэш) реконструировал диапазоны файла ФИЛЬТРОМ по общему `by_name`-бакету (все вхождения имени по
ВСЕМУ workspace). Токенизатор индекса ссылок — чисто лексический (не AST-aware), поэтому
идентификаторы, совпадающие с ключевыми словами (`fn`, `module`, `type`, `import`, ...), попадают
в общий бакет из ПОЧТИ КАЖДОГО файла — на 3093-файловом workspace бакет `fn` держит десятки тысяч
записей. Фильтрация такого бакета на каждый файл на старте = экспорт всего скана становится
`O(workspace²)` — тот же класс проблемы, что чинил План 213 (посчитать один файл дороже, чем
нужно, помноженное на N файлов). Замер холодного старта поймал это напрямую: 57.7с. Фикс: `by_file`
теперь хранит `(name, ranges)` целиком (не только имена) — ровно то, что нужно
`install_file`/`index_file` для восстановления общего индекса, и ровно то, что нужно `export_file`
для отдачи, без обратного скана `by_name`. `export_file`/`remove_file` стали `O(имён в этом
файле)`, независимо от размера общего бакета. +3 юнит-теста, включая прямой регресс-guard
(`p215_edge_export_file_cost_independent_of_shared_bucket_size`: 4000 файлов с общим
идентификатором, экспорт ОДНОГО файла обязан уложиться в 50мс).

## Замеры (release-бинарь, workspace = worktree `nova-lspcache`, ~3093 `.nv`-файла)

| Прогон | total_files | warm_hits | stale | total_elapsed | доминанта |
|---|---|---|---|---|---|
| холодный (кэша нет) | 3093 | 0 | 3093 | 42.4с | парсинг 3093 файлов |
| тёплый (ничего не менялось) | 3093 | 3093 | 0 | 7.0с | чтение+парсинг ~42МБ JSON-кэша |
| тёплый + 1 намеренно изменённый файл | 3094 | 3092 | 2¹ | 1.1с | чтение кэша; реиндекс 2 файлов — пренебрежимо |

¹ Один из двух «stale»-файлов в третьем замере — случайно задетый мной же в ходе ручного тестирования
stray-файл вне git (`std/prelude.nv`, пустой, untracked — не относится к тесту), не баг. Ключевой
факт подтверждён однозначно: **1 намеренно изменённый файл (`examples/basics/hello.nv`, после теста
`git checkout --`) → реиндексирован ровно он**, `warm_hits=3092` — ни один из 3092 неизменённых
файлов не тронут.

Тёплый старт **≈6x быстрее** холодного и укладывается в цель «секунды» (не минуты).

### Известный компромисс v1 (не блокер)

Кэш сериализован ОДНИМ JSON-документом — на этом workspace вышло ~42МБ на диске, и его
чтение+разбор доминируют тёплый прогон (6.8с из 7.0с). Компактный бинарный формат
(bincode/postcard) уменьшил бы и объём на диске, и время загрузки в разы — не требуется для
закрытия этой волны (цель «секунды» уже достигнута, JSON уже был зависимостью, новой не
понадобилось), но зафиксировано как задел «если понадобится ещё быстрее».

## Гейт

- `cargo build --release` (`nova-lsp/Cargo.toml`) — собирается чисто.
- `cargo test` (весь nova-lsp: lib + 15 интеграционных бинарей в `tests/`) — **не внесено ни одной
  новой красной строки**. Базовая линия (сверена побайтово с `main`, идентична и там):
  - `code_lens::tests::non_test_fn_has_no_run_lens` — pre-existing Windows-default-stack overflow
    под тестовым раннером (не эта волна; воспроизведён идентично на `main`, обходится
    `RUST_MIN_STACK=67108864` для локального прогона).
  - `completion::tests::imp_pos2_std_prefix_returns_submodules`,
    `stdlib_index::tests::pos_top_level_modules_real_not_stale`,
    `tests/completion.rs::{ipos3_method_dot_int, method_str_detail_present}` — pre-existing
    `"std.io does not exist"` / устаревшая stdlib-сигнатура (`str.len` → `byte_len`), идентично на
    `main`, не связано с этой волной.
  - Все 15 интеграционных `tests/*.rs`-бинарей (включая `e2e_smoke.rs`'s
    `f18_pos_initial_scan_emits_progress`, который напрямую гоняет
    `run_initial_scan_with_progress` через реальный JSON-RPC-хендшейк) — ✅ зелёные.
  - +12 новых юнит-тестов этой волны (9 в `index_cache.rs`, 3 в `symbols.rs`) — ✅ зелёные,
    включая обязательный по приёмке тест «кэш повреждён/несовместим → молчаливый фолбэк, не крэш»
    (`neg_corrupt_json_is_none`, `neg_wrong_version_is_none`, `neg_empty_file_is_none`).
- Ручной замер холодный/тёплый/точечная-инвалидация на реальном workspace — см. таблицу выше.

## Затронутые файлы

- `nova-lsp/Cargo.toml` — явная зависимость `serde` (derive).
- `nova-lsp/src/index_cache.rs` — новый модуль: `PersistedIndex`/`CachedFile`, `load`/`save`/
  `file_fingerprint`, `CACHE_FORMAT_VERSION`, 9 юнит-тестов.
- `nova-lsp/src/symbols.rs` — `WorkspaceSymbolEntry` derive Serialize/Deserialize;
  `WorkspaceIndex::{install_file, export_file}`; `ReferencesIndex` — `by_file` хранит
  `(name, ranges)` (не только имена) + `{install_file, export_file}`; 3 новых теста.
- `nova-lsp/src/server.rs` — `run_initial_scan_with_progress` переписан под тёплый/холодный путь +
  троттлинг фона.
- `nova-lsp/src/lib.rs` — регистрация модуля `index_cache`.

## Followups

- `[M-104.10-persistent-index]` (`docs/plans/backlog-followups.md`) — ЗАКРЫТ этим планом.
- Компактный бинарный формат кэша вместо JSON (быстрее чтение/меньше на диске) — P3, не блокер,
  см. «Известный компромисс v1» выше.
- Кэширование `check_workspace`-диагностики (не только индекса) требует графа обратных
  зависимостей — остаётся за `[M-104.10-dependent-invalidation]`, вне объёма этого плана.
