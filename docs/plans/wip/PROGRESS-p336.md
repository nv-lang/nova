<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# PROGRESS p336-lockpin — лок не держит версию (№336)

**Модель:** sonnet. Worktree: `d:/Sources/nv-lang/nova-p336`, ветка `p336-lockpin` (не
вливается, не пушится). Периметр: резолвер зависимостей + `nova-cli`; в
`compiler-codegen/src/types/mod.rs` не заходил (не потребовалось).

## Разведка (ДО кода)

### 1. Где происходит пере-резолв и почему

`lockfile::sync_ex` (compiler-codegen/src/lockfile.rs) устроен КОРРЕКТНО:

1. `load(entry_pkg_dir)` читает существующий `nova.lock.toml`.
2. `git_cache::install_lock_entries(ex.git_pins())` — сразу засеивает
   ГЛОБАЛЬНУЮ (per-process, `static OnceLock<Mutex<HashMap<url,commit>>>`)
   таблицу `git_cache::lock_table()` парой `(url, commit)` для **каждой**
   git-записи старого лока — вне зависимости от типа пина (rev/tag/branch/
   version).
3. `resolve_version_deps` — согласованный backtracking-резолв ТОЛЬКО
   версионных git-зависимостей ИЗ `[dependencies]` САМОГО entry-пакета
   (не транзитивных), с `preferred`-картой из старого лока (§03.2 Ф.4):
   если предпочтённая версия проходит ограничения — резолвер берёт ИМЕННО
   её, а не максимум (`resolver.rs::solve`, строки 148-156).
4. `collect_dep_graph_ex` — обход графа; для `GitPin::Version`-зависимостей
   зовёт `git_cache::resolve_git_dep(url, pin, None)` — ОБЁРТКА, которая
   ВСЕГДА сверяется с `lock_table()` (шаг 2) ПЕРЕД тем как резолвить пин
   «вживую»; если URL там есть — `resolve_git_dep_in` берёт `if let
   Some(locked) = locked_commit` ветку и живой резолв (fetch тегов, выбор
   максимума) даже не запускается.

**Эмпирически подтверждено ТРЕМЯ прогонами** (все ✅ PASS), что при вызове
`lockfile::sync` НАПРЯМУЮ механизм держит зафиксированный коммит против
новых тегов:
- `version_lock_repro.rs` (уже был в дереве, Plan 03.2 Ф.4) — синтетический
  локальный git-репозиторий, прямая git+version-зависимость.
- НОВЫЙ `version_lock_transitive_via_path_repro.rs` (написан в разведке) —
  зависимость `deepdep` достижима ТОЛЬКО транзитивно через `path`-пакет
  (топология `examples/` → `polaris`(path) → `http`(git+version)):
  держит commit после добавления нового тега upstream.
- НОВЫЙ `version_lock_real_tls_repro.rs` (написан в разведке, реальная сеть
  `github.com/nv-lang/nova-tls`) — лок с искусственно СТАРЫМ `commit=`
  (v0.1.4) держится при наличии более нового v0.1.5 тега upstream.

**Вывод: сам `lockfile::sync` (и, транзитивно, `nova build`, который его
вызывает) УЖЕ РАБОТАЕТ ПРАВИЛЬНО.** Гипотеза «резолвер каждый раз лезет в
сеть и берёт максимум» для `nova build` эмпирически ОПРОВЕРГНУТА.

### Настоящий дефект: `nova check` и `nova test` НИКОГДА не загружают лок

Грep всего `compiler-codegen/src/` на `lockfile::` дал ДВА совпадения, оба
комментарии — ни один реальный код резолва импортов (`imports.rs`,
`git_cache.rs`) не вызывает `lockfile::load_pins`/`sync` вообще. В
`nova-cli/src/main.rs` вызовы `lockfile::sync`/`load_pins` есть ТОЛЬКО в
`cmd_build` (строки ~4180, ~4989/4993) и в `cmd_update`/`cmd_add`. У
`cmd_check` (1949) → `check_one_file` (2286) и у `cmd_test` (5596) /
`cmd_test_build` (5823) — **ноль** вызовов `lockfile::` где-либо в их телах
(проверено grep по диапазонам строк функций).

`check_one_file` вызывает `imports::resolve_imports_inline_ex` НАПРЯМУЮ, БЕЗ
предварительного `load_pins`. Для git+version-зависимости это означает:
`git_cache::resolve_git_dep(url, pin, None)` → `locked_commit_for(url)` =
`None` (таблица никем не засеяна за весь процесс `nova check`/`nova test`)
→ ветка `else if let GitPin::Version(req) = pin` в `resolve_git_dep_in` →
`fetch --tags` + `select_version_tag` (МАКСИМАЛЬНЫЙ подходящий тег) — **на
каждый запуск, безусловно**, независимо от содержимого `nova.lock.toml`.

Это и есть находка №336, если её сформулировать точно: **не «резолвер
каждый раз лезет в сеть», а «`nova check`/`nova test` вообще не знают о
существовании `nova.lock.toml`»** — `nova build` уже честен.

### 2. Точный пин vs диапазон

Не имеет отношения к корню: разница в поведении НЕ между точным пином и
диапазоном внутри `sync`, а между `cmd_build` (зовёт `sync`/`load_pins`) и
`cmd_check`/`cmd_test` (не зовут ничего). И `rev`/`tag`/`branch`-пины
ТОЖЕ никогда не консультируют лок под `check`/`test` — им просто в
типичном случае всё равно (branch и так «плывёт» по дизайну, tag/rev
детерминированы сами по себе), поэтому расхождение незаметно НИГДЕ, КРОМЕ
`version`-пинов, где живой резолв выбирает МАКСИМУМ, а не то же самое.

### 3. Поведение без сети

`ensure_db`/`list_versions_in`/`resolve_git_dep_in`: если объекта нет в
локальном bare-клоне и `NOVA_OFFLINE` не установлен — сеть используется
безусловно (fetch тегов на КАЖДЫЙ `GitPin::Version`-резолв, если db уже
существовала). `NOVA_OFFLINE=1` полностью запрещает clone (`bail`) и,
т.к. `run_git(fetch)` в `Result` не пробрасывается (`let _ =` в двух
местах: List/`resolve_git_dep_in` version-ветка), молча продолжает на
уже закэшированных локально тегах — **это отдельный, более мелкий гэп**
(offline + version-пин без лока может использовать ЛЮБОЙ ранее увиденный
тег из локального кэша, не обязательно тот, что в отсутствующем/старом
локе) — но с фиксом ниже он не проявляется на `check`/`test`, т.к. лок
теперь консультируется ПЕРВЫМ.

### 4. Есть ли уже флаг офлайн/строгого лока

`NOVA_OFFLINE` существует (git_cache.rs) — управляет только сетью, не
строгостью лока. Отдельного «strict lock» флага нет и не требуется:
корректная семантика — лок ВСЕГДА диктует commit, если он есть; отличие
`build`/`update` в том, что `update` явно СНИМАЕТ пины
(`drop_git_locks`) перед пере-резолвом — это уже правильно реализовано и
не трогается.

## План фикса

Симметричный `nova build`-у, но БЕЗ дорогого `resolve_version_deps`
(незачем пере-резолвить/переписывать лок на `check`/`test` — только
читать). Хук — единая точка входа резолва зависимости в
`compiler-codegen/src/imports.rs::lookup_dependency` (там уже вычисляется
`root_dir` — корневой пакет всей сборки, ИМЕННО туда указывает
`nova.lock.toml`) + `resolved_dependency_roots` (уже документированно
вызывается ТОЛЬКО с корневым `pkg_dir`). Новая мемоизированная функция
`lockfile::ensure_pins_loaded(dir)` — читает и засеивает `git_cache`
lock-таблицу один раз за процесс на директорию, безопасно вызывать
многократно (per-lookup). `nova build` не меняется (уже вызывает
`sync`/`load_pins` раньше и полнее).
