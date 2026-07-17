# [M-d376-slow-suffix-folder-module-peer-merge] — рабочие заметки

Задача: peer-merge folder-модуля (imports.rs) не уважает `_slow`-суффикс
(D376), хотя discovery-walker (test_runner.rs::walk_nv_filtered_ex) его уже
уважает. Следствие: `_slow.nv`-peer компилируется и ГОНЯЕТСЯ на каждом
обычном `nova test` (эмпирически на nova-tls).

Worktree: d:/Sources/nv-lang/nova-d376fix (branch p-fix-d376-peer-slow, из main).

## Разведка (готово)

Канонический предикат уже есть в test_runner.rs:
- `is_slow_file_stem(stem: &str) -> bool { stem.ends_with("_slow") }` (публичная, ~L4776)
- `walk_nv_filtered_ex` (~L4828+) peels `_slow` ПЕРВЫМ (outermost), потом `_test`,
  потом OS-suffix. Канон: `<core>[_<os>][_test][_slow]`.

В imports.rs грепом «_test» найдено ПЯТЬ мест с дублированной строковой
suffix-фильтрацией (`stem.strip_suffix("_test")` + `peer_active_for_target`),
ни одно не знает про `_slow`:

1. **L787-830** (`resolve_imports_inline_ex`, SiblingPeer collection) — сканит
   `entry_dir` на файлы, объявляющие ТОТ ЖЕ `module X` что entry, мёржит их
   items (**включая Item::Test**, комментарий L775-777: "entry folder-module's
   own tests must run, unlike imported peers whose tests are skipped").
   ЭТО и есть фактическая точка бага для nova-tls: nova-tls/src — root-peers
   (D78 rev-4, все файлы `module tls` прямо в source_root), `cert_modes_test.nv`
   как entry подтягивает `stream_leak_test_slow.nv` как SiblingPeer (тот же
   `module tls`) → его Item::Test мёржится в CU entry'я → slow-тест
   компилируется и ГОНЯЕТСЯ при обычном прогоне cert_modes_test.
2. **L2069-2074** (`is_folder_module_peer`) — детектор folder-module (только
   `_test`-strip для сверки деклараций, `_slow` не трогает — но детектор не
   мёржит контент, риск ниже, чисто классификация).
3. **L2171-2179** (`collect_root_peers`, D78 rev-4 root-peers коллектор,
   используется при resolve ИМПОРТА чужого пакета через root-peer форму).
4. **L3044-3055** (orphan-detection scan, `resolve_module_paths`, single_file
   ветка) — диагностика "file+peer orphan", не мёрж, но не должен звать
   `_slow`-peer'ы orphan'ами лишний раз (не критично, но предикат общий).
5. **L3091-3106** (`resolve_module_paths`, folder_exists ветка — ИМЕННО эти
   координаты называл TLS-агент "~3073-3110") — коллектор peer'ов при resolve
   ИМПОРТИРУЕМОГО folder-module (`import std.unicode` style). Item::Test
   отсюда обычно фильтруется ниже по пайплайну для ИМПОРТИРУЕМЫХ модулей
   (не entry) — но сами `_slow`-файлы всё равно тащатся в CU (лишняя
   компиляция +契 риск, раз они не отфильтрованы для build-mode тоже).

Вызов `resolve_imports_inline_ex(path, &mut module, &repo, &stdlib_dir, true)`
(test_runner.rs:3602) — ЕДИНСТВЕННАЯ точка сборки CU test-раннера, ВСЕГДА
передаёт `include_test_peers=true` НЕЗАВИСИМО от SlowLane/TestSelection —
слоулейн туда вообще не прокинут. Нужно прокинуть info "разрешён ли _slow
в этом CU" (по факту: entry сам _slow ИЛИ include_test_peers... нет — по
заданию: default НЕ ГОНЯЕТ _slow НИКОГДА как peer чужого entry; когда сам
_slow — entry — его собственные peers мёржатся как обычно, т.е. simple rule:
"exclude candidate iff candidate is _slow AND candidate != entry_path").

## План фикса

- Вынести общий хелпер в imports.rs (либо переиспользовать
  `is_slow_file_stem` из test_runner.rs — сделать её видимой из imports.rs,
  раз она уже `pub`) + новый маленький предикат
  `peer_slow_excluded(candidate_stem, is_entry) -> bool` который заменяет
  повторяющийся паттерн ИЛИ просто инлайн-проверка в каждом из 5 мест:
  "если candidate stem (после strip `_test`) ends_with `_slow` И candidate
  != entry_path → skip" (симметрично _test).
- Приоритет фикса — место (1) L787-830 (доказанный баг). Остальные 4 —
  тот же предикат ради консистентности (§0, не третье место с своей
  логикой) + чтобы build-mode/import-mode тоже не тащили _slow зря.
- НЕ трогать `_windows`-семейство (peer_active_for_target) — оно отдельно
  и уже корректно работает (независимая ОС-фильтрация).

## Тесты

- Plan 156 precedent: test_runner.rs `mod plan156_slow_lane_tests` (~L6385+),
  особенно `walk_nv_filtered_slow_lanes` (~L6419) — образец для симметричного
  peer-merge теста в imports.rs (`mod entry_folder_module_tests` уже есть
  ~L3217, куда добавить кейс с `_slow`-peer).

## Верификация (TODO)
- nova-tls worktree `nova-tls-d376` (branch fix-d376-move-back) — перенести
  stream_leak_test_slow.nv / stream_leak_live_test_slow.nv из src/slow/ ОБРАТНО
  в src/ (module tls), поправить баннеры.
- Дефолтный `nova test src` → slow не гоняются (PASS 1, только cert_modes_test).
- `--slow-only` → гоняются (live — ОДИН раз, не в цикле).
- spec_tests/conformance/standalone --jobs 4 = PASS 69/0.
- std/src/checksums смоук (обычный folder-module с _test, peer-merge не сломан).

## Статус: фикс ЕЩЁ НЕ НАПИСАН (разведка завершена, приступаю к правке imports.rs).
