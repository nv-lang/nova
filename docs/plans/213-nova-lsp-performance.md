<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 213 — nova-lsp: диагностика и фикс жора CPU (27 CPU-часов/день)

**Статус:** ✅ ЗАКРЫТ 2026-07-17 — Ф.1 (диагностика: check_workspace тайпчекал ВСЕ 3074 файла
на каждый recheck, >300с CPU; поток/64МиБ-стек на файл; без debounce watch-событий) +
Ф.2 (фиксы: check_open_documents, watch-debounce 400мс, bounded threads cores/4 +
BELOW_NORMAL, общий фильтр-обход) влиты в main; Ф.3 ВКЛЮЧЁН интегратором 2026-07-17
~13:50 (свежий бинарь → nova-lsp-v14.exe, nova.lsp.enabled=true; нужен Reload Window).
Приёмка владельцем — по факту тихой работы в редакторе; при рецидиве жора — реоткрыть.
Ф.4 (протокольные идеи) — future-список вне объёма, не блокирует закрытие.

**Предшественники:** [План 104](104-ide-integration.md) (IDE-интеграция, закрыт 2026-06-17) и
[104.10](104.10-lsp-v2-production.md) (LSP V2, выполнен 2026-07-04); этот план чинит эксплуатационную
деградацию их результата (CPU-жор полной переиндексации на каждую правку).

**Проблема:** сервер `nova-lsp` (крейт `nova-lsp/`, LSP для VSCode-расширения nova-lang) сжигал
~27 CPU-часов за день непрерывной работы. Владелец выключил бинарь
(`nova-lsp-v14.exe` → `.disabled`, `nova.lsp.enabled=false` в `.vscode/settings.json`).
Гипотеза на входе: полная переиндексация workspace на каждое файловое событие, без debounce,
включая build-артефакты и соседние git-worktree.

## Ф.1 — диагностика по коду (найдено, не гипотеза)

Прочитан весь `nova-lsp/src/**`. Реальная причина оказалась ИНОЙ, чем входная гипотеза: debounce
уже существовал (`debouncer.rs`, 200ms per-URI, gopls-парити) и фильтр путей в watcher-регистрации
уже ограничивал события `**/*.nv` + `**/nova.toml`. Настоящий жор — архитектурный: **каждый**
debounced recheck пересчитывал **весь workspace**, а не только изменённый файл.

1. **Главный сжигатель CPU** — `nova-lsp/src/server.rs:150-201` (было), метод `schedule_recheck`:
   когда `workspace_root` установлен (обычный случай — открыт `d:/Sources/nv-lang/nova`), КАЖДЫЙ
   debounced recheck вызывал `check_workspace(&root)`
   (`nova-lsp/src/compiler.rs:89-123`, было) — полный `read_dir`-обход + парсинг + резолв
   импортов/прелюдии + тайпчек **каждого** `.nv`-файла под корнем воркспейса. В самом репо nova
   это **3074 файла** (std=271, examples=43, spec_tests=1562, nova_tests=1116) — т.е. на каждое
   кейстроук-событие (после 200мс дебаунса) сервер фактически гонял «nova check» по всему
   монорепо. Замер (см. Ф.1-регресс-тест ниже): полный `check_workspace` над реальным репо занял
   **>300 секунд** на одном прогоне — при активном редактировании в течение дня это и даёт
   заявленные ~27 CPU-часов.
   - Комментарий в самом коде уже фиксировал это как временное решение: «V1 strategy: full
     workspace recheck… Per-module incremental dep-graph is V2» (`compiler.rs:87-88`, было).
   - Усугубляющий фактор: `check_workspace`'s внутренний цикл вызывал `run_with_large_stack`
     (spawn нового OS-потока с резервацией стека 64 МиБ) **на каждый файл** отдельно
     (`compiler.rs:115-117`, было) — т.е. один recheck = **3074+ создания OS-потоков** поверх
     самой работы парсинга/тайпчека. На Windows создание потока с большим стеком заметно дороже,
     чем на Linux — это чистый паразитный оверхead, отдельный от собственно вычислительной работы.
2. **Debounce** — `nova-lsp/src/debouncer.rs` — реализация КОРРЕКТНА (per-URI cancellation
   token, 200ms), но не спасала от (1): дебаунс коалесцирует БЫСТРЫЕ правки в ОДИН recheck, но
   сам этот один recheck был `O(весь_workspace)`.
   Отдельная дыра: `workspace/didChangeWatchedFiles` (`server.rs:832-864`, было) НЕ имел
   собственного дебаунса — каждое уведомление (а при `git checkout`/переключении веток их может
   быть сотни-тысячи) обрабатывалось синхронно на async-таске (диск-чтение + парсинг для индекса
   — НЕ через `spawn_blocking`), и по завершении безусловно дёргало
   `invalidate_all_resolved()` + полный `check_workspace`-recheck.
3. **Фильтры путей** — три независимые копии обхода директорий
   (`compiler.rs::collect_nv_files`, `symbols.rs::collect_nv_files`,
   `server.rs::collect_nv_files_for_rename`) пропускали только `target/` и dot-директории.
   Не пропускались: `vcpkg_installed/` (у `compiler-codegen/vcpkg_installed/` — большое
   vendored-дерево include/lib, ноль `.nv`-файлов, но глубокая структура директорий — каждый
   `read_dir` по ней — чистые потери), `node_modules/`, и, главное — **вложенные git-корни**:
   если владелец открывает в редакторе родительский каталог `d:/Sources/nv-lang/` (частый сценарий
   «флота» — 10-30 sibling-воркtree `nova-*` одновременно), обход рекурсивно спускался бы в КАЖДЫЙ
   soseдний воркtree как будто это часть текущего workspace, умножая 3074 файла на количество
   открытых воркtree.
4. **Приоритет/параллелизм** — выделенного thread-pool не было; вся работа шла через
   `tokio::task::spawn_blocking` (дефолт tokio — до 512 блокирующих потоков) плюс раздельный
   `run_with_large_stack`-поток НА КАЖДЫЙ вызов. Ограничения параллелизма/приоритета не было.

## Ф.2 — фиксы (влиты в ветку `fix-lsp-cpu`, коммиты раздельно по пунктам)

а) **Инкрементальность** — `nova-lsp/src/compiler.rs`: новая `check_open_documents(docs, root)`
   переиспользует ту же per-file resolve-механику, что и `check_file_with_root`
   (`resolve_for_check` → ближайший `nova.toml` / folder-module peers / прелюдия), но применяется
   ТОЛЬКО к открытым в редакторе документам (`state.docs`), а не ко всему дереву.
   `nova-lsp/src/server.rs` (`schedule_recheck_for`, бывший `schedule_recheck`): ветка
   «workspace_root установлен» теперь вызывает `check_open_documents` вместо `check_workspace`.
   Стоимость recheck'а — `O(открытых документов)` (обычно единицы) вместо `O(размера
   workspace)`. Полный `check_workspace` остался публичным API, но используется только для
   ОДНОРАЗОВОГО холодного скана при `initialized` (`run_initial_scan_with_progress`) и в тестах —
   там его стоимость приемлема (разовая, не на каждую правку).
   Заодно убран per-file `run_with_large_stack` внутри `check_workspace` (3074 spawn'а потоков
   → 0 — вызывающий уже внутри одного large-stack потока).
   Полный per-module dependency-graph (пересчёт только изменённого модуля + его
   reverse-dependents) остаётся будущей работой — см. Ф.4.

б) **Debounce burst'ов** — `nova-lsp/src/state.rs`: добавлено отдельное поле
   `watch_debouncer: Debouncer` (400мс, отдельно от 200мс интерактивного `debouncer` — разные
   профили нагрузки не должны тянуть друг друга) + `pending_watch_events: Mutex<Vec<FileEvent>>`.
   `nova-lsp/src/server.rs::did_change_watched_files`: события дёшево классифицируются (без I/O),
   складываются в `pending_watch_events`, и реальный apply-pass + recheck планируется через
   `watch_debouncer` с фиксированным ключом (`watch_batch_key()`) — пачка уведомлений внутри
   400мс окна схлопывается в ОДИН apply-pass + ОДИН recheck вместо одного на уведомление.
   Сам apply-pass (диск-чтение + парсинг для индексов) перенесён в `spawn_blocking` — больше не
   блокирует async-рантайм.

в) **Фильтры путей** — три копии обхода директорий схлопнуты в одну:
   `nova-lsp/src/compiler.rs::collect_nv_paths` (новая, `pub`), переиспользуемую
   `symbols.rs::collect_nv_files` и `server.rs::collect_nv_files_for_rename`. Добавлено:
   - список пропускаемых имён `SKIP_DIR_NAMES` = `target`, `target_alt`, `target_test`,
     `vcpkg_installed`, `node_modules` (кроме уже существовавшего dot-dir скипа);
   - guard вложенного репозитория: поддиректория, содержащая свой `.git` (файл-указатель
     worktree или директория), не обходится — она считается ГРАНИЦЕЙ отдельного
     репозитория/worktree, даже если физически достижима под открытой workspace-папкой. Это
     закрывает сценарий «открыт родительский каталог над несколькими sibling-worktree» без
     хардкода имён `nova-*`.

г) **Приоритет/параллелизм** — `nova-lsp/src/main.rs`: ручной `tokio::runtime::Builder` вместо
   `#[tokio::main]`, `max_blocking_threads = (cores/4).max(2)` — блокирующий пул больше не
   неограничен (дефолт tokio — 512). `nova-lsp/src/compiler.rs::run_with_large_stack`: поток,
   выполняющий чек, теперь best-effort понижает свой OS-приоритет
   (`THREAD_PRIORITY_BELOW_NORMAL` через прямой FFI к `kernel32` — без новой зависимости;
   no-op на не-Windows), чтобы фоновый тайпчек не соревновался с UI-потоком редактора за CPU при
   контеншне.

**Замер (до/после), `nova-lsp/tests/perf.rs::check_open_documents_much_cheaper_than_check_workspace_on_real_repo`
(`#[ignore]`, ручной запуск `cargo test --release --test perf -- --ignored --nocapture`):**
воспроизводит СТАРУЮ стратегию (`check_workspace` над реальным репо, 3074 файла) и НОВУЮ
(`check_open_documents` над 2 «открытыми» документами, тот же workspace_root) в одном
процессе и утверждает ≥3× разрыв. `check_workspace` на реальном репо — счёт на многие десятки
секунд/минуты за один проход (эквивалент одного пре-фикс recheck'а на любую правку); `nova build
--release` + `cargo test --release` (юнит 406/408 + интеграционные наборы) — зелёные, кроме двух
ЗАРАНЕЕ существовавших несвязанных провалов из-за дрейфа `std/io` (не тронуто этой волной —
`completion.rs`/`stdlib_index.rs` не менялись).

**Коммиты (ветка `fix-lsp-cpu`, `d:/Sources/nv-lang/nova-lspfix`):**
- фикс (а) инкрементальность — `check_open_documents` + удаление per-file thread-spawn
- фикс (б) debounce watch-burst'ов
- фикс (в) общий фильтрованный обход директорий (dedup 3 копий + vendor/nested-repo guard)
- фикс (г) bounded blocking pool + thread-priority
- этот план-документ

## Ф.3 — включение (ЖДЁТ владельца/интегратора)

Эта волна НЕ включает сервер — только диагностика+фикс+сборка. Процедура включения (после
приёмки диффа):
1. `cargo build --release` в `nova-lsp/` → `nova-lsp/target/release/nova-lsp.exe`.
2. Скопировать собранный бинарь на место текущего `.disabled`-бинаря (путь/имя — по факту
   установки расширения; исторически `nova-lsp-v14.exe`), сохранив старый как бэкап на время
   обкатки.
3. В `.vscode/settings.json` (main-репо) выставить `"nova.lsp.enabled": true`.
4. Обкатка: понаблюдать CPU процесса (`Get-Process nova-lsp*`) в реальной сессии редактирования
   +в «дни флота» (несколько открытых sibling-worktree) — это ключевой сценарий, который
   спровоцировал исходную проблему.

## Ф.4 — будущее (НЕ делать в этой волне; идеи одной строкой)

- Настоящий per-module dependency-graph: recheck только изменённого модуля + его
  reverse-dependents (а не всех открытых документов) — `[M-104.10-dependent-invalidation]`.
- go-to-definition/hover через тот же resolved-module кэш каналы чекера (переиспользование
  `expr_types`/`resolved_callees` вместо отдельного текстового резолва) для более точной
  семантики за меньшую цену.
- Инкрементальный (не whole-cache-clear) `invalidate_resolved` при внешних изменениях —
  точный reverse-dependency walk вместо текущего консервативного clear-all
  (`[M-104.10-watch-reverse-deps]`).
- Уважение `.gitignore` в обходе директорий (сейчас — только явный skip-list имён), если
  дёшево завести парсер `.gitignore` без новой тяжёлой зависимости.
- Кэш результатов резолва прелюдии/стдлиба между отдельными `check_source`-вызовами (сейчас
  каждый файл в `check_workspace`/`check_open_documents` резолвит прелюдию заново).
- Телеметрия: серверный счётчик «recheck'ов в минуту» + логирование, если он превышает разумный
  порог (ранняя сигнализация о будущем регрессе этого фикса).
