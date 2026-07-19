# LSP write-after-destroyed — рабочие заметки (чекпоинт)

Ветка: `p-fix-lsp-write-destroyed`, worktree `d:/Sources/nv-lang/nova-lspwrite`.
Задача: фикс спама `Cannot call write after a stream was destroyed`
(vscode-jsonrpc messageWriter, при отправке `textDocument/didClose`) +
подготовка обновлённого бинаря nova-lsp.

## Симптом — уточнение направления

Лог владельца: `[Error] Sending document notification textDocument/didClose
failed. Error: Cannot call write after a stream was destroyed` — стек через
`ril.js WritableStreamWrapper.write`.

Это **клиентская** (vscode-languageclient) ошибка: клиент пытается ПОСЛАТЬ
`didClose` СЕРВЕРУ, но пишущий поток (stdin дочернего процесса nova-lsp со
стороны клиента) уже помечен как destroyed. Т.е. на момент попытки отправки
клиент уже считает транспорт закрытым (сервер завершился, или клиент уже
вызвал `stop()`/уничтожил соединение) — а закрытие документов (много вкладок
разом, например при закрытии окна VSCode) продолжает генерировать
`didClose` уже ПОСЛЕ этого момента → повтор на каждый документ ⇒ спам.

Значит первичный вопрос НЕ «сервер пишет в мёртвый поток», а «почему
транспорт/процесс сервера уже мёртв/закрыт в момент, когда клиент ещё
пытается его использовать» — нужно смотреть на обе стороны: (а) не роняет ли
сервер соединение раньше времени (краш/ранний exit фонового таска), (б) не
разрушает ли клиентский extension.ts транспорт до того как все pending
didClose улетели.

## Прочитано в nova-lsp/src (server.rs, state.rs, debouncer.rs, main.rs)

### `shutdown()` — server.rs:866-874
```rust
async fn shutdown(&self) -> Result<()> {
    tracing::info!("nova-lsp shutdown");
    self.shutdown_requested.store(true, Ordering::Relaxed);
    self.state.cancel_all();                    // debouncer + watch_debouncer only
    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(())
}
```
`state.cancel_all()` (state.rs:193-196) отменяет ТОЛЬКО два `Debouncer`
(`debouncer`, `watch_debouncer` — per-URI CancellationToken, cancel_all в
debouncer.rs:128-134). Уже неплохо гейтится через `token.is_cancelled()`
внутри `schedule_recheck_for` (server.rs) и watch-batch closure — обе
проверяют токен в нескольких точках перед `publish_diagnostics`/`send_notification`.

### НЕ гейтится вообще: `run_initial_scan_with_progress` (server.rs:389-618)
Вызывается напрямую (не через debouncer) из `initialized()`
(server.rs:826-864, awaited на строке 862). Это тяжёлый холодный скан
(~3090 файлов на основном репо) с явными паузами
(`tokio::task::yield_now()` + `sleep(10ms)` каждые 64 файла — REINDEX_SLEEP_EVERY,
server.rs:507-551) — то есть НАРОЧНО растянут во времени, чтобы не
монополизировать CPU. Внутри цикла и после — множество
`self.send_progress(...)` (→ `client.send_notification::<notification::Progress>`)
и `self.client.publish_diagnostics(...)` вызовов — НИ ОДИН не проверяет
`self.shutdown_requested` (приватное поле Backend, но у `run_initial_scan_with_progress`
есть `&self`, доступ тривиален).

Это ПЕРВЫЙ кандидат в корень: если клиент шлёт `shutdown`/`exit` пока
холодный скан ещё идёт (правдоподобно на большом воркспейсе — окно
может быть закрыто до того как скан закончится), таск продолжает слать
progress/diagnostics в `self.client` (tower-lsp `Client`) уже после
`shutdown_requested = true`, и потенциально после того как `exit` уже
пришёл и `Server::serve()` завершился (стдин/стдаут закрыты процессом).

### main.rs (`Server::serve`)
`LspService::new` + `Server::new(stdin, stdout, socket).serve(service).await` —
без явного panic-hook / catch_unwind вокруг рантайма. Нужно уточнить (СЛЕДУЮЩИЙ
ШАГ): диспетчеризует ли tower-lsp каждый notification/request как отдельный
spawned task (конкурентно с продолжающимся чтением `shutdown`/`exit`), или
последовательно в одном task — от этого зависит, может ли `initialized()`
(долгий скан) заблокировать обработку `shutdown`/`exit`, либо наоборот —
скан продолжает жить как orphaned task после того как serve() уже вернулся
(и раннтайм после `runtime.block_on` в main() дропается — tokio Drop
форсированно абортит незавершённые таски, но это уже post-mortem, не
объясняет повторный спам с клиентской стороны один-в-один).

### `debouncer.rs` — cancel-модель корректна и протестирована
`schedule()` синхронно вставляет токен ДО возврата (race-free относительно
`cancel_all`), панику в `work` ловит `catch_unwind`
(`neg1_panicking_work_doesnt_crash_debouncer`). Это НЕ похоже на источник
бага — хорошо покрыто тестами (edge1/edge2/edge3).

### `state.rs` — WorkspaceState
Ничего похожего на источник краша/паники на фоновом пути; `cancel_all()`
затрагивает только 2 debouncer'а — см. выше. `resolved_cache`/`workspace_index`
и т.п. — не async, не пишут в transport напрямую.

## Ещё не прочитано / следующие шаги (актуально после обрыва)
1. Дочитать `server.rs` дальше (файл 2677 строк, прочитано 1-1318) —
   искать другие фоновые `tokio::spawn(...)` пути отправки
   notification/progress (например `did_change_watched_files` дальше по
   файлу, если там что-то за пределами уже виденного, и весь остаток
   `LanguageServer` impl).
2. `workspace_lifecycle.rs`, `perf.rs`, `code_lens.rs`, `symbols.rs`, `provenance.rs` —
   грепнуть `tokio::spawn`, `send_notification`, `publish_diagnostics`,
   `show_message`, `log_message` по всему `nova-lsp/src` одним проходом —
   свести полный список фоновых точек отправки.
3. Выяснить точную модель диспетчеризации tower-lsp (конкурентная ли
   обработка входящих сообщений) — версия из `nova-lsp/Cargo.toml`/`Cargo.lock`,
   при необходимости заглянуть в vendored исходники в `~/.cargo/registry`.
4. editors/vscode/src (extension.ts) — как устроены activate/deactivate,
   `client.stop()`, порядок относительно закрытия документов при закрытии
   окна. Это КЛИЕНТСКАЯ часть гипотезы (б) — пока не смотрел вообще.
5. Решение по гварду: добавить `Arc<AtomicBool>` "alive" (или переиспользовать
   существующий `shutdown_requested`, вынеся его на уровень, доступный
   фоновым тасками — сейчас он приватное поле Backend, `run_initial_scan_with_progress`
   имеет к нему доступ через `&self`, но НЕ проверяет) — вставить проверки
   перед каждым `send_progress`/`publish_diagnostics` в холодном скане,
   и досрочный выход из циклов (`to_reindex` loop) при `shutdown_requested`.
6. ro-warning навязчивость — НЕ проверено, отложено на потом.
7. Сборка релиза + путь бинаря в vscode-расширении — НЕ проверено.

## Вывод на данный момент (гипотеза, не факт)
Корень — вероятнее всего в `run_initial_scan_with_progress` (Plan 215):
единственный фоновый путь без cancellation-гейта вообще. Требуется
подтверждение через п.1-4 выше прежде чем фиксировать в отчёте как
окончательный вердикт.

---

## ПОДТВЕРЖДЕНО (финал)

Прочитан vendored `tower-lsp-0.20.0` (`src/transport.rs`, `src/service.rs`,
`src/service/client.rs`, `src/service/state.rs`) — механизм подтверждён
буквально по коду, не по догадке:

`Server::serve()` (`transport.rs`) держит
`join!(print_output, read_input, process_server_tasks)`, где
`process_server_tasks = server_tasks_rx.buffer_unordered(max_concurrency=4)`
— пул уже ЗАПУЩЕННЫХ futures хендлеров. `shutdown`/`exit` отвечают быстро
(отдельная futura внутри того же пула), но НЕ отменяют уже
диспетчеризованный `initialized()`-таск — `run_initial_scan_with_progress`
awaited НАПРЯМУЮ внутри `initialized()`, без единой проверки отмены. Пока
скан не доработает сам (растянут REINDEX_SLEEP_EVERY-паузами под CPU-щадящий
режим Plan 213/215 — на большом воркспейсе реально десятки секунд/минуты),
`process_server_tasks` не осушается → весь `join!` не резолвится → `serve()`
не возвращается → `main()` не завершается → OS-процесс физически жив дольше,
чем клиент готов ждать.

`vscode-languageclient`: `client.stop()` шлёт `shutdown`→`exit`, затем (по
стандартному поведению `LanguageClient`) форс-килляет child-процесс, если
тот не завершился за grace-период. Форс-килл рвёт СТОРОНУ КЛИЕНТА (Node
`ChildProcess.stdin` writer уничтожается в ответ на смерть child'а) — и
КАЖДАЯ последующая попытка клиента отправить notification (напр.
`textDocument/didClose` по каждому ещё открытому документу при закрытии
окна VSCode) валится с "Cannot call write after a stream was destroyed" —
повтор на документ ⇒ спам из лога владельца. Это СЕРВЕР-ОБУСЛОВЛЕННАЯ
клиентская ошибка: корень на нашей стороне (никогда не отпускает процесс
вовремя), симптом — на стороне клиента.

### Фикс (закоммичен 0c12dc70e)
`WorkspaceState::shutting_down: AtomicBool` (+ `mark_shutting_down()` /
`is_shutting_down()`) — единая точка правды, видимая и `Backend`-методам, и
свободным функциям без `&Backend` (`schedule_recheck_for`,
`recheck_open_documents_for`, `refresh_client_hints_for`, watch-batch
closure). `Backend::shutdown()` выставляет флаг ДО `cancel_all()`. Гейт
вставлен в 7 точек `run_initial_scan_with_progress` + все `token.is_cancelled()`
точки debounce-путей + начало `recheck_open_documents_for`/
`refresh_client_hints_for` + orphan-таск регистрации watcher'ов в
`initialized()`. `Backend.shutdown_requested` (писался, но НИГДЕ не
читался — сам был дырой) удалён в пользу общего флага.

### ro/let doc-comment warning — навязчивость: ДА, но вне scope write-спама
`compiler-codegen/src/parser/mod.rs:1636-1644` — `eprintln!` (НЕ
`Diagnostic`, не идёт в LSP `publishDiagnostics`) на каждый `ro`/`let` с
предшествующим `///`. Летит в STDERR процесса nova-lsp (не в JSON-RPC
stdout-транспорт) → VSCode "Output > Nova LSP". Т.к. `check_open_documents`
(Ф.1-стратегия `schedule_recheck_for`) перепарсивает ВСЕ открытые документы
на КАЖДЫЙ debounced edit ЛЮБОГО документа — при наличии такого паттерна в
любом открытом файле предупреждение печатается повторно на каждое
нажатие клавиши в любом открытом документе. Навязчиво — да. Но структурно
НЕ может быть причиной "Cannot call write after a stream was destroyed"
(другой канал/другой механизм — readable-stream форвардинг stderr, не
vscode-jsonrpc message writer). Сам warning не тронут (дизайн Plan 45,
задание явно просило не трогать).

### Тесты (добавлены, зелёные)
`nova-lsp/src/server.rs::shutdown_gate_tests` — 3 теста на РЕАЛЬНОМ
`tower_lsp::ClientSocket`-транспорте (не мок): `neg_refresh_client_hints_
sends_nothing_after_shutdown`, `neg_recheck_open_documents_sends_nothing_
after_shutdown` (оба проводят сервис через настоящий `initialize`-хендшейк,
дренируют loopback-канал и считают реально отправленные сообщения — до/после
`mark_shutting_down()`), `pos_mark_shutting_down_flips_flag`. `cargo test
--lib shutdown_gate_tests` → 3/3 ok.

Полный `cargo test --lib` (с `RUST_MIN_STACK=64MiB` — без этого несколько
НЕСВЯЗАННЫХ тестов валятся stack-overflow ОДИНАКОВО что в этом worktree,
что на немодифицированном main, т.е. предсуществующий инфраструктурный
момент): 421 passed, 2 failed — оба сбоя («std.io does not exist» в
`completion::imp_pos2_std_prefix_returns_submodules` и
`stdlib_index::pos_top_level_modules_real_not_stale`) воспроизведены
identично на немодифицированном main → предсуществующий дефект, НЕ
регрессия от этой правки, вне scope.

### Сборка
`cargo build --release` в `nova-lsp/` (worktree) — чисто, 0 ошибок/новых
warning. Бинарь: `nova-lsp/target/release/nova-lsp.exe`
(sha256 `162b100a8d35ba30fb21f0f4f0280d1108185176674361dc32f002654ca6c1db`),
против старого живого (`d:/Sources/nv-lang/nova/nova-lsp/target/release/
nova-lsp.exe`, sha256 `1a79c4c81ce42194c67202d2e543919b80b7e21c1c3c7c5a91
ccb5e74713a63f`, собран 2026-07-17).

### Путь бинаря в расширении (nova-lang-local 0.2.0)
`editors/vscode/client/extension.ts::findNovaLsp()` — приоритет:
(1) настройка `nova.lsp.path` → (2) PATH (`where`/`which`) → (3)
`<workspaceFolder>/target/release|debug/nova-lsp[.exe]`. `editors/vscode/
package.json` НЕ бандлит сам бинарь nova-lsp в vsix (`.vscodeignore`
исключает только `client/`/TS-исходники и dev-конфиги; `node_modules`
СОХРАНЯЕТСЯ — рантайм-зависимость `vscode-languageclient`, как уже
задокументировано в самом `.vscodeignore`).

`d:/Sources/nv-lang/nova/.vscode/settings.json` держит явно:
`"nova.lsp.path": "D:/Sources/nv-lang/nova/nova-lsp/target/release/
nova-lsp.exe"` — т.е. текущий живой LSP грузится СТРОГО по этому пути
(вариант (1), выше остальных в приоритете). Значит переустановка — это
ЗАМЕНА ФАЙЛА по этому пути, БЕЗ переустановки vsix (расширение
код/логика не менялись — только серверный бинарь).

### Инструкция переустановки (интегратору/владельцу)
1. Закрыть ВСЕ окна VSCode, где открыт nova-репозиторий (снимает файловую
   блокировку Windows на запущенном `nova-lsp.exe` — иначе copy = os error 5).
2. Скопировать `d:/Sources/nv-lang/nova-lspwrite/nova-lsp/target/release/
   nova-lsp.exe` → `d:/Sources/nv-lang/nova/nova-lsp/target/release/
   nova-lsp.exe` (перезапись).
3. Открыть VSCode заново на nova-репо — `nova.lsp.path` в `.vscode/
   settings.json` уже указывает туда же, расширение подхватит новый бинарь
   автоматически. Переустановка `.vsix` НЕ требуется (extension.ts/
   package.json не менялись).
