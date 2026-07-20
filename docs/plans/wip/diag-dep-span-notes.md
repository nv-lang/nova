# [M-diag-dep-file-span-misattribution] — фикс-notes (2026-07-20)

Статус: ✅ РЕШЕНО. Полная запись — `docs/plans/backlog-followups.md`
(`[M-diag-dep-file-span-misattribution]`, P1-таблица). Этот файл — рабочий
чекпоинт worktree'а `nova-diagspan2` (ветка `p-fix-diag-dep-span`), не
дублирует backlog-запись, только сырые детали расследования.

## Механизм span→файл (как есть, ДО и ПОСЛЕ фикса)

- `compiler-codegen/src/diag.rs`: `Span { start, end, file_id }`,
  `SourceMap { files: Vec<SourceFile> }` (file_id = index), `Diagnostic::
  render(src, path)` (single-file, ИГНОРИРУЕТ `span.file_id`) vs
  `Diagnostic::render_with_map(&SourceMap)` (резолвит per-span по
  `file_id`). Оба существовали ДО этой волны (Plan 35 Ф.0 / Plan 81 Ф.8.1).
- `imports.rs::resolve_imports_inline_ex` уже назначает каждому peer-файлу
  (folder-module peer ИЛИ файл из `import`, включая path/git-зависимость
  пакета) уникальный возрастающий `file_id`, пушит в `module.peer_files`.
  Это УЖЕ работало правильно — файлы deps РЕГИСТРИРУЮТСЯ с корректными
  file_id при резолве импортов.
- `nova-cli/src/main.rs::build_source_map(module, entry_src, entry_path)`
  — уже существовавший helper (Plan 81 Ф.8.1): строит `SourceMap` из
  `module.peer_files`, перечитывая non-entry файлы с диска.

## Корень бага

`cmd_check` (nova check) уже строил `SourceMap`+`render_with_map` для
type-check ошибок (main.rs:2398-2400, Plan 81 Ф.8.1). `cmd_build` (nova
build) — единственный путь, которым РЕАЛЬНО собираются `nova.toml`-пакеты
с path/git-зависимостями (флагман: `examples/nova.toml` →
`http = { path = "../../nova-http" }`) — этот фикс никогда не получал.
Три места в `cmd_build` рендерили диагностики через `d.render(&src,
&path_str)` (single-file, byte-offset dep-файла применяется к source
ВХОДНОГО файла):
1. Type-check error path (~main.rs:4978, ГЛАВНЫЙ — репортил E7320).
2. `embed_resolve` error path (~main.rs:4864) — тот же паттерн, редко
   бьёт (embed() в dep-файле — edge case), фикс за компанию.
3. `lint_module` warnings loop (~main.rs:5000) — то же самое для lint'ов
   на peer/dep-файлах, фикс за компанию.

Parse-error path (main.rs:4803) НЕ трогал — парсинг entry-файла ВСЕГДА
идёт до import-resolve, там span всегда file_id=0 (корректно и так).

## Фикс

`nova-cli/src/main.rs::cmd_build`: все три места переведены на
`build_source_map(&module, &src, &path)` + `render_with_map`/
`path_for`+`source_for` (тот же паттерн, что `cmd_check` уже использовал).
Диф — только `nova-cli/src/main.rs` (~55 insertions/13 deletions).
Зона `resolver/diag` не задета файлами `types/mod.rs`/`emit_c.rs` —
пересечений с protocol-Any/P3-агентами не было.

## Репро (воспроизведено И до, И после фикса)

Setup: `examples/nova.local.toml` (temp, НЕ коммитился) →
`[replace] http = { path = "../../nova-http-diagrepro" }` — detached-HEAD
git worktree `nova-http-diagrepro` на `nova-http@811197c` (ДО коммита
`250f4ab` — WriteBuffer `.into()`→`.into_bytes()` миграция, 8+ сайтов).

- **ДО фикса** (временно `git checkout -- nova-cli/src/main.rs`,
  пересобран): `nova build examples/flagship/aggregator/src/main.nv` →
  6× `[E7320] no method into on WriteBuffer`, все спаны
  `...\main.nv:47/377/111/139/144/159` — реально это строки-КОММЕНТАРИИ
  main.nv (напр. `47 | // \`ServerResponse\`; the closure's OWN exposed
  type...`). Точно повторяет диагноз владельца (`main.nv:46/110/137/142/
  157/376` — off-by-~1 из-за дрейфа main.nv между 2026-07-17 и текущим
  SHA, тот же симптом).
- **ПОСЛЕ фикса** (патч восстановлен, пересобран): ТЕ ЖЕ 6 ошибок →
  `D:\Sources\nv-lang\nova-http-diagrepro\src\header.nv:70:17`,
  `...\url.nv:564:17`, `...\server\wire.nv:162:5/203:5/215:5/243:5` —
  точные реальные строки, корректные сниппеты (`buf.into() //
  [M-lint-findings-writebuffer-into]`).

Cleanup: `nova.local.toml` удалён, `examples/nova.lock` откачен
(`git checkout --`, локальные touch-и от replace-override не коммитятся),
worktree `nova-http-diagrepro` удалён (`git worktree remove --force`).

## Regression-проверка

1. **Корневой пакет, синтетическая ошибка**: временно дописан
   `export fn root_span_probe() -> str { ro buf = WriteBuffer.new();
   buf.into() }` в `examples/flagship/aggregator/src/domain/domain.nv`
   (БЕЗ nova.local.toml override — реальный nova-http). `nova build` →
   `...\examples\flagship\aggregator\src\domain\domain.nv:147:5` — ТОЧНО
   та строка, где добавлена ошибка. Откачено (`git checkout --`).
2. **Флагман против РЕАЛЬНОГО текущего nova-http** (no override): `nova
   build examples/flagship/aggregator/src/main.nv` → **exit 0**, `built:
   .../agg_final.exe`. Lint-warnings из зависимостей показывают СВОИ
   реальные пути (`D:\Sources\nv-lang\nova-http\src\server\wire.nv:108:22
   [W_PARAM_TYPE_POS_MUT]`, `...\servernet\servernet.nv:123:17`,
   `D:\Sources\nv-lang\nova-diagspan2\std\src\...` для std-deps) — ни
   одного `main.nv`-спана среди warnings/errors о чужом коде.
3. Полный `spec_tests/conformance` мега-CU НЕ гонялся (задание явно
   исключило — изменение в `nova-cli` CLI-слое, не в чекере/резолвере;
   `cmd_check`/`test_runner.rs` — codepath conformance-гейта — НЕ
   менялись, только `cmd_build`).

## Хэши / окружение

- Worktree: `d:/Sources/nv-lang/nova-diagspan2`, ветка
  `p-fix-diag-dep-span`, base `main` @ `7f37f2645`.
- nova-http репро: worktree (удалён после теста) на `811197c` (до
  `250f4ab`); реальный nova-http @ `7becdcd` (master, чистый, не менялся).
- Модель: sonnet (данная волна).
- Diff: `git diff --stat` → `nova-cli/src/main.rs | 68 ++++...` (только
  этот файл; `examples/nova.lock`/`nova.local.toml` — репро-артефакты,
  откачены/удалены, НЕ часть коммита).
