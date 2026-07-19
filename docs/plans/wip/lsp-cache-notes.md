# Plan 215 — рабочий чекпоинт (LSP index cache)

Волна: `p215-lsp-cache`, worktree `d:/Sources/nv-lang/nova-lspcache`. Модель sonnet.

## Разведка (сделано)

- `nova-lsp/src/server.rs::run_initial_scan_with_progress` (вызывается из `initialized()`):
  холодный скан = `collect_nv_files(root)` (читает содержимое ВСЕХ `.nv`, ~3090 на этом
  workspace) → `workspace_index.index_file` + `references_index.index_file` (parse-only,
  no typecheck) для каждого → `references_index.mark_primed()` → затем ОТДЕЛЬНО
  `check_workspace(root)` (полный тайпчек всех файлов, только для initial diagnostics —
  ВНЕ объёма этой волны, см. «Явный scope-out» ниже).
- `nova-lsp/src/symbols.rs`: `WorkspaceIndex` (`DashMap<Url, Vec<WorkspaceSymbolEntry>>`) и
  `ReferencesIndex` (`by_name: DashMap<String, Vec<RefOccurrence>>` +
  `by_file: DashMap<Url, Vec<String>>`, `primed: AtomicBool`). Уже есть открытый маркер
  `[M-104.10-persistent-index]` (backlog-followups.md) — ИМЕННО эта задача его закрывает.
- `nova-lsp/src/compiler.rs::collect_nv_paths` — путь-only обход (без чтения содержимого),
  с фильтром `target/vcpkg_installed/node_modules/.git-вложенные-корни` (Plan 213 Ф.1). Это
  то, что нужно переиспользовать для тёплого старта (stat, не read).
- Прецедент кэша компилятора: `nova-cli/src/build_cache.rs` — `target/.nova-cache/<key>.c`,
  версия схемы в самом ключе (`"nova-c-cache-v2".hash(...)`), atomic write (temp+rename),
  best-effort (ошибки проглатываются, никогда не роняют сборку). Беру эту же модель.
- `target/` уже в корневом `.gitignore` (паттерн `target/` без якоря) → `target/nova-lsp-cache/`
  не требует новой записи в `.gitignore`.
- `lsp-types` 0.94.1 (vendored в cargo registry): `SymbolKind`, `Range`, `Position` уже
  `#[derive(Serialize, Deserialize)]` — можно напрямую сериализовать `WorkspaceSymbolEntry`
  добавив derive, БЕЗ ручной схемы. `Url` (crate `url`) тоже Serialize/Deserialize (иначе
  весь LSP JSON-RPC не работал бы). serde САМ не объявлен явной зависимостью в
  `nova-lsp/Cargo.toml` (только `serde_json`) — добавляю `serde = { version = "1", features
  = ["derive"] }` явно.
- Plan 213 Ф.2 уже даёт прецедент троттлинга: `worker_priority::lower_current_thread_priority()`
  (Windows `THREAD_PRIORITY_BELOW_NORMAL`) для выделенных OS-потоков тяжёлого тайпчека.
  Для моего фон-скана (parse-only, БЕЗ отдельного потока — работает прямо в async-таске
  на tokio) буду троттлить через `yield_now().await` + мелкий `sleep` каждые N файлов —
  не приоритет потока (там нет отдельного потока), а темп.

## Дизайн (решено)

- Кэш-файл: `<workspace_root>/target/nova-lsp-cache/index-v1.json` (serde_json — уже
  зависимость, новых крейтов не добавляю; bincode не нужен для этого масштаба).
- Формат: `PersistedIndex { format_version: u32, files: HashMap<uri_string, CachedFile> }`,
  `CachedFile { mtime_nanos: u64, size: u64, symbols: Vec<WorkspaceSymbolEntry>, refs:
  Vec<(String, Vec<Range>)> }`. `format_version` расхождение ИЛИ любая ошибка
  parse/read → трактуется как холодный кэш (полная фон-переиндексация), НЕ падение.
- Тёплый старт: `collect_nv_paths(root)` (путь-only) → для каждого файла читаем
  `fs::metadata` (mtime+size), сверяем с кэш-записью по URI; совпало → `install_file` в
  `WorkspaceIndex`/`ReferencesIndex` НАПРЯМУЮ (без парсинга) + переносим запись в
  new_persisted; не совпало/нет в кэше → в `to_reindex`.
- `references_index.mark_primed()` вызывается СРАЗУ после тёплой установки (не ждём
  дореиндексацию stale) — LSP отвечает по (возможно частично устаревшему) кэшу немедленно;
  фон досчитывает.
- Открытые документы: файлы, УЖЕ в `state.docs` на момент скана, ПРОПУСКАЮТСЯ в
  to_reindex-цикле (не читаем их с диска — `didOpen`/`didChange` уже их переиндексировали
  из живого буфера, чтение с диска затёрло бы несохранённые правки — тот же принцип, что
  уже задокументирован в `workspace_lifecycle.rs` про watch-события). Это И ЕСТЬ механизм
  «открытые документы первыми»: они не ждут своей очереди в фон-цикле вообще — `didOpen`
  переиндексирует их немедленно синхронно в своём хендлере независимо от прогресса фона.
- CPU-скромность фона: батчи (константа, `~32` файлов) + `tokio::task::yield_now().await`
  после каждого файла + короткий `sleep` каждые N файлов — кооперативно уступаем tokio-рантайму,
  не блокируем обработку параллельных LSP-запросов (didOpen/hover/etc.) длинным синхронным
  циклом. НЕ трогаю `check_workspace` (диагностика/тайпчек) — сознательный scope-out (ниже).
- После фон-реиндексации — атомарная запись нового `PersistedIndex` (temp+rename,
  best-effort, как в `build_cache.rs`).
- Порча/несовместимость кэша → юнит-тест на молчаливый фолбэк (не паника).

## Явный scope-out (не путать с недоработкой)

- `check_workspace` (полный тайпчек ВСЕХ файлов для initial diagnostics) — НЕ кэшируется в
  этой волне. Кэшировать diagnostics per-file корректно нельзя без графа обратных
  зависимостей (transitive peers/imports) — это отдельный открытый маркер
  `[M-104.10-dependent-invalidation]`, не эта волна. Эта волна = «индекс символов»
  (workspace/symbol + references), как и было явно OPEN промаркировано
  `[M-104.10-persistent-index]`.

## Статус выполнения (обновляю по ходу)

- [x] Cargo.toml: добавить `serde` explicit dep
- [x] symbols.rs: derive Serialize/Deserialize на WorkspaceSymbolEntry + install_file/export_file
      на WorkspaceIndex и ReferencesIndex
- [x] новый модуль index_cache.rs (load/save/fingerprint, версия, атомарная запись) — 9 тестов
- [x] server.rs: переписать run_initial_scan_with_progress под тёплый/холодный путь + throttle
- [x] lib.rs: зарегистрировать модуль
- [x] юнит-тесты: round-trip, mtime/size invalidation, corrupt/version-mismatch → fallback
- [x] **найден и зафикшен реальный баг во время замера**: `ReferencesIndex::export_file`
      исходно фильтровал ОБЩИЙ `by_name`-бакет (записи всех файлов workspace) на каждый файл —
      для широко общих идентификаторов (`fn`, `module`, ...) это O(workspace²). Первый холодный
      замер после фикса: 57.7с → 42.4с (фикс), но заодно доказал что сам scan (без бага) даёт
      разумный порядок величины. Фикс: `by_file` теперь хранит `(name, ranges)` целиком
      (не только имена) — export_file/remove_file больше не трогают by_name сверх необходимого.
      +3 юнит-теста (round-trip, install≡index_file по ответам find(), регресс-guard на сам баг
      — 4000 файлов с общим идентификатором, export одного файла <50мс).
- [x] лог холодный/тёплый + замер на реальном workspace (3093-3094 файлов, см. ниже)
- [x] cargo test --lib зелёные (RUST_MIN_STACK=67108864 — обходит ПРЕДСУЩЕСТВУЮЩИЙ
      Windows-default-stack overflow у `code_lens::tests::non_test_fn_has_no_run_lens`,
      воспроизведён идентично на main — НЕ регрессия этой волны). 415/417 + 1 pre-existing
      overflow + 2 pre-existing "std.io does not exist" (тоже воспроизведены на main).
- [x] cargo build --release — собирается чисто
- [ ] docs/plans/215-lsp-index-cache.md
- [ ] финальный коммит(ы) + очистка временных тестовых артефактов

### Замеры (после фикса O(N²), release-бинарь, workspace = сам worktree nova-lspcache, ~3093 .nv)

| Прогон | total_files | warm_hits | stale | total_elapsed_ms | initialized→ready |
|---|---|---|---|---|---|
| cold (кэша нет) | 3093 | 0 | 3093 | 42438 | 42.9с |
| warm (ничего не менялось) | 3093 | 3093 | 0 | 6995 | 7.5с |
| warm + 1 намеренная правка (+1 случайный stray-файл задет моими же ручными тестами) | 3094 | 3092 | 2 | 1078 | 1.6с |

Тёплый старт без изменений: **~6x быстрее холодного** (7.0с vs 42.4с), доминанта — чтение+парсинг
42МБ JSON-кэша (`fingerprint_pass_ms=6806` из 6995мс). Точечная инвалидация подтверждена: тронул
РОВНО 1 файл (`examples/basics/hello.nv`, впоследствии `git checkout --` откачен) — лог показал
`stale=2` (второй — случайно задетый мной же ранее пустой stray-файл `std/prelude.nv`, НЕ
относящийся к тесту и не отслеживаемый git; удалён) при `warm_hits=3092`: реиндексация не тронула
ни одного из 3092 неизменённых файлов. total_elapsed при 2 stale = 1.08с (доминанта та же — чтение
кэша, реиндексация 2 файлов пренебрежимо мала).

Артефакты замера очищены: `git checkout -- examples/basics/hello.nv`, удалён stray
`std/prelude.nv`, `target/nova-lsp-cache/` — disposable (в .gitignore через `target/`).

### Известный компромисс v1 (не блокер, для плана)

Кэш-файл сериализован как ОДИН JSON-документ (`serde_json`, уже зависимость) — на этом workspace
получилось ~42МБ. Работает и укладывается в цель «тёплый = секунды», но компактный бинарный формат
(bincode/postcard) уменьшил бы и размер на диске, и время чтения/парсинга ещё в разы — оставляю как
задел на будущее (не требуется для закрытия этой волны, JSON достаточен и не требует новой
зависимости).
