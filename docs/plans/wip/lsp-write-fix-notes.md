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
