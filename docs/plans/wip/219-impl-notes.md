# План 219 — заметки реализации (build-демон)

**Модель:** sonnet. **Worktree:** `nova-219`, ветка `p219-build-daemon`, база `main` (b7d842168).
**Зона:** `nova-cli/src/**` только — compiler-codegen НЕ трогается (только читаем `pub`
API `test_runner`/`lockfile`, которые уже существуют).

## Разведка (что уже есть, что реально стоит кэшировать)

- `detect_or_build_libuv` (`test_runner.rs:4257`) — при готовом `libuv.lib` в кэше это
  3 `is_file()` — уже дёшево (<1мс). НЕ приоритет.
- `detect_or_build_rt_archive` (Plan 218, `test_runner.rs:4931`) — уже ДИСКОВЫЙ кэш
  (`libnova_rt.a`/`.lib`), плюс process-wide memo (`RT_ARCHIVE_MEMO`) для `nova test`
  (много файлов в одном процессе). `nova build` вызывает его 1 раз за процесс — memo
  бесполезен в рамках одного short-lived процесса. Хеширование ~1-1.5МБ `nova_rt/*.{c,h}`
  — не измерено отдельно, вероятно единицы мс. НЕ первый приоритет, но даром достаётся
  бесплатно если демон когда-нибудь станет in-process (Ф.3, не в этой волне).
- **`detect_toolchain`** (`test_runner.rs:568`) — на Windows с `clang`/`msvc` toolchain
  вызывает `capture_vcvars_env` — `cmd /c "call vcvars64.bat > nul && set"` — READS
  реестр/VS-инсталляцию, доля секунды-единицы секунд (комментарий в коде: «avoids the
  ~7-second call vcvars64.bat overhead» — относится к старому поведению ДО кэша env,
  но сам capture ещё вызывается КАЖДЫЙ `nova build`-процесс). Замер: разница wall −
  сумма именованных PerfTimer-фаз в разведке (~11с − ~8.7с ≈ 2.3с) — вероятный источник:
  detect_toolchain (vcvars) + detect_or_build_libuv + процесс-старт/выход.
- **dep-lock** (`nova_codegen::lockfile::sync`, `main.rs:4822-4824`) — измерено ~987мс
  на манифесте с git+path зависимостями (`examples/nova.toml`: `tls` git,
  `http` path). Это ЯВНО названная в плане цель №1.

## Архитектура (Ф.1 — эта волна)

**Не** «демон гоняет весь `cmd_build` in-process» (это потребовало бы редиректа
stdout/stderr демона к клиенту — инвазивно, риск для гейта byte-identical). Вместо
этого: **демон — резидентный cache/config-сервис**, клиент (`nova build`, обычный
short-lived процесс, КОД БЕЗ ИЗМЕНЕНИЙ в остальном) перед дорогими шагами спрашивает
демон «дай готовое» вместо «вычисли заново». Дешёвый round-trip (localhost TCP,
JSON) вместо секунд пересчёта. Сам `cmd_build` печатает как раньше (тот же процесс,
тот же stdout) — byte-identical тривиально (не другой код-путь для вывода).

### IPC
- `std::net::TcpListener` на `127.0.0.1:0` (OS выдаёт порт) — НЕ named pipe (кроссплатформенно
  без unsafe/WinAPI, без новых crate-зависимостей). Discovery-файл
  `<repo_root>/target/.nova-daemon/daemon.json`: `{pid, port, token, started_at}`.
  `target/` уже в `.gitignore` (паттерн без якоря, прецедент — Plan 215 LSP-кэш).
- `token` — случайный 128-бит hex, генерируется демоном при старте, обязателен в
  каждом запросе (минимальная защита от чужого процесса на том же localhost).
- Протокол — по одному JSON-объекту на соединение (запрос → ответ → close). serde
  добавлен как явная зависимость `nova-cli` (`serde = { version = "1", features =
  ["derive"] }`) — тот же паттерн, что Plan 215 добавил `nova-lsp`.

### Резидентное состояние демона
- `toolchain_cache: Mutex<Option<(key, test_runner::Toolchain)>>` — key = pref +
  explicit_clang + explicit_vcvars + env_fingerprint (хеш PATH+NOVA_CLANG+NOVA_VCVARS+
  ProgramFiles(x86)). Смена env → новый key → miss → демон зовёт
  `detect_toolchain` заново (та же функция, что клиент звал бы сам).
- `libuv_cache: Mutex<HashMap<key, test_runner::LibuvConfig>>` — key = rt_dir+vcvars.
- `dep_lock_ledger: Mutex<HashMap<pkg_dir, combined_hash>>` — combined_hash =
  hash(entry nova.toml content) + hash(nova.lock content, если есть). Приход
  `Prime`-запроса с тем же combined_hash что уже в ledger → `skip_dep_lock=true`.
  Иначе → `false`, клиент зовёт РЕАЛЬНЫЙ `lockfile::sync` (код не меняется), затем
  шлёт демону `Commit` с новым combined_hash (fire-and-forget).
- **Корректность при skip:** клиент ВСЕГДА зовёт `lockfile::load_pins(&pkg_dir)`
  (дешёвая — просто читает существующий `nova.lock`, устанавливает git-пины в
  process-local `git_cache`-таблицу) — это ТО ЖЕ первое действие, что делает
  `sync()` изнутри. Пропускается только дорогая часть (`resolve_version_deps` +
  `collect_dep_graph_ex` + перезапись lock — там git tag listing/резолв).

### Известный OPEN (честно, не блокер Ф.1)
`combined_hash` покрывает: entry `nova.toml` + `nova.lock`. НЕ покрывает транзитивные
`nova.toml` path/git-зависимостей (напр. правка `nova-http/nova.toml`), т.к. полный
обход графа — код `lockfile.rs` (compiler-codegen, вне зоны). Правка entry-манифеста
ловится (entry-hash меняется). Правка транзитивного манифеста БЕЗ правки entry —
редкий кейс (в основном при разработке самих зависимостей одновременно с потребителем)
— может дать stale skip до следующего изменения lock/manifest. Смягчение: `NOVA_DAEMON=0`
глушит демон целиком; `nova daemon stop` сбрасывает ledger.

### Lifecycle
- **Auto-start:** ТОЛЬКО если `NOVA_DAEMON=1` (opt-in, см. ниже почему НЕ default-on).
  Первый `nova build`, не находящий живого демона — спавнит его в фоне (detached,
  `CREATE_NEW_PROCESS_GROUP|DETACHED_PROCESS` на Windows) и продолжает ТЕКУЩИЙ билд
  холодным путём (не ждёт демона — второй билд уже тёплый).
- `nova daemon start` — явный старт (работает ВСЕГДА, независимо от `NOVA_DAEMON`env —
  явная команда владельца).
- `nova daemon stop` — коннект к живому демону, `Shutdown`-запрос, демон завершается,
  подчищает discovery-файл.
- `nova daemon status` — коннект (или отсутствие), печатает uptime/served/cache-состояние.
- Idle-timeout: демон — фоновый поток проверяет `last_activity`; > `NOVA_DAEMON_IDLE_SECS`
  (default 1800) без запросов → самозавершение + чистка discovery-файла.
- Один демон на workspace: discovery-файл keyed путём `repo_root` (хеш пути в имени
  файла внутри `target/.nova-daemon/`, а не глобальный singleton).

### Почему opt-in (`NOVA_DAEMON=1`), а не default-on
Это ЕДИНСТВЕННОЕ отклонение от «default-on как 218» в этой волне — осознанное решение,
не забывчивость. Демон — это ФОНОВЫЙ ПРОЦЕСС, переживающий родительский `nova build`;
default-on означало бы, что ЛЮБОЙ `nova build` (включая CI/conformance/автономные
агенты в песочницах) молча плодит detached-процесс без явного согласия — риск зомби
в CI, конфликт с существующим CI-monitoring-протоколом (сторож только по явной
команде владельца). `NOVA_RT_ARCHIVE`(218)/`NOVA_MULTI_TU`(209) — чисто in-process
кэши, тот же процесс, нет фонового state. Демон — другая категория риска. Опция
явно задокументирована, `nova daemon start` доступен явной командой без env.

### Fallback
Любая ошибка IPC (нет discovery-файла, коннект не удался, timeout, битый ответ) →
`None` из клиентского helper'а → `cmd_build` продолжает ТЕМ ЖЕ кодом, что без демона
вообще (детект/sync как раньше). Никогда не паникует, никогда не блокирует билд.

## Фазы этой волны
- Ф.1: `nova-cli/src/daemon.rs` (протокол+сервер+клиент+lifecycle) — DONE ниже.
- Ф.2: wiring в `cmd_build` (dep-lock skip + toolchain/libuv cache) — DONE ниже.
- Ф.3 (слияние с LSP, Plan 215): **НЕ делаем** — LSP держит ДРУГОЙ ресурс (индекс
  символов workspace, tower-lsp/stdio к редактору) и ДРУГОЙ lifecycle (стартует
  редактором, живёт весь editor-session). Общий процесс связал бы lifecycle
  build-cli (короткие, частые, из терминала/скриптов) с LSP (долгий, из IDE) без
  выигрыша — build-демон и так резидентен по своему собственному lifecycle.
  Развилка решена: **отдельный демон** (см. отчёт).
- Ф.4: замер + гейты — DONE ниже (см. отчёт).
