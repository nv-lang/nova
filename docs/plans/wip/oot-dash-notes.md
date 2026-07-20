# [M-oot-dash-module-name-e78] — фикс ложного E_D78 для oot-файла из каталога с дефисами

Статус: ЗАКРЫТ (заведён и закрыт этой же волной). Перенесено в
`docs/history/simplifications-closed.md` (запись у топа секции 1).

## Задача
Одиночный out-of-tree .nv из каталога с дефисами/двойными дефисами в пути → ложный
E_D78_MODULE_PATH_MISMATCH. Репро подтверждён интегратором.

## План
1. Мини-репро (temp с дефисом vs без) — подтвердить RED/PASS.
2. Найти место синтеза/сверки module-имени для anonymous standalone (D78/D29).
3. Фикс: нормализация дефисов в синтезированном имени ИЛИ анонимный singleton-модуль без D78-сверки для oot.
4. Rust-интеграционный тест по прецеденту oot_interp_stringbuilder.rs.
5. Гейт: repro RED->GREEN, no-dash PASS, oot_interp test PASS, D78-neg fixtures FAIL:0, standalone FAIL:0.

## Прогресс

### Шаг 1 — мини-репро
- Чистый `%TEMP%/oot-probe` (дефис, БЕЗ ancestor nova.toml) → PASS (control:
  `%TEMP%/ootprobe_control` тоже PASS). Дефис САМ ПО СЕБЕ репро НЕ даёт.
- Буквальный путь из тикета (`.../scratchpad/ootv/probe.nv`, тот же
  scratchpad, где лежит этот notes-файл) → RED, E_D78_MODULE_PATH_MISMATCH,
  `expected root peer: cmin` — т.е. `find_manifest` нашёл `scratchpad/nova.toml`
  (package "cmin", ЧУЖОЙ leftover-манифест от другой более ранней задачи в
  этом же scratchpad).
- Контрольный тест: та же структура (ancestor nova.toml прямо над файлом),
  путь БЕЗ единого дефиса (`%TEMP%/nodashpkg/ootv/probe.nv`) → ТОЖЕ RED,
  идентичная ошибка. **Дефис — red herring.** Настоящий триггер: ЛЮБОЙ
  ancestor `nova.toml`, найденный `find_manifest` вверх по ФС от файла,
  независимо от того, имеет ли он отношение к текущему запуску `nova test`.

### Шаг 2 — корень
`compiler-codegen/src/manifest.rs::find_manifest` (строка ~419) идёт вверх
по родительским директориям ФАЙЛА в поисках ЛЮБОГО `nova.toml` — без
привязки к тому, какой проект реально вызвал `nova`. `check_module_path_with_kind`
(манифест.rs ~1073) вызывается из `test_runner.rs::codegen_to_c` (~3740,
`nova test`) и из `nova-cli/src/main.rs` (`check_module_path` wrapper,
~1481: cmd_check/check_one_file, cmd_doc, cmd_build) БЕЗ учёта уже
резолвленного `repo` (CWD-based `find_repo_root()` — тот же root, что
threaded в `resolve_imports_inline*` по прецеденту
`[M-standalone-out-of-tree-interp-sb-typedef]`). Рассинхрон: imports
резолвятся против repo вызывающего проекта, а D78 — против ПЕРВОГО
найденного вверх по ФС manifest'а (может быть чужим).

Нет никакой "синтез module-имени из dash-сегментов пути" — package_name
берётся строго из `[package] name` в TOML, никакого dir-name-derivation
не существует (проверено грепом). Гипотеза интегратора про дефисы не
подтвердилась; настоящий баг — repo-agnostic ancestor-manifest walk.

### Шаг 3 — фикс (выбор: B, анонимный standalone, БЕЗ смены D78-нормы)
Добавлен `manifest::is_outside_repo(file, repo)` (manifest.rs, после
`check_module_path`): true если `file` резолвится ВНЕ `repo` (оба
canonicalized). Гейт применён в:
- `test_runner.rs::codegen_to_c` — если `is_outside_repo(path, repo)`,
  D78-проверка пропускается (Ok(Rev3)), иначе — прежнее поведение.
- `nova-cli/src/main.rs::check_module_path` wrapper — новый параметр
  `repo: Option<&Path>`; если `Some(repo)` и файл вне него — skip.
  Вызовы: `check_one_file` (repo резолвится РАНЬШЕ, переиспользуется),
  `cmd_doc`, `cmd_build` (repo уже был в скоупе).
`compiler-codegen/src/main.rs` (вторичный dev-бинарь `nova-codegen`) НЕ
трогал — там repo для check ужЕ = `find_repo_root_from(path)` (тот же
file-based walk, что и find_manifest) — гейтить против самого себя
бессмысленно; это отдельная архитектурная особенность вне зоны задачи.

Спека: НЕ норма-амендмент — D78 rev-3 правило (`parent.target`) не
менялось; поменялся только SCOPE enforcement (какие файлы вообще
подлежат D78), причём этот scope нигде не был нормативно заф, - как и
прошлый прецедент `[M-standalone-out-of-tree-interp-sb-typedef]`
(тоже implementation-only, без D-амендмента).

### Шаг 4 — сборка/верификация
- release-сборка `nova-cli` в worktree (NOVA_GC_LIB_DIR/INCLUDE_DIR на
  main repo; libuv скопирован + .git удалён; libuv-cache скопирован).
- RED→GREEN на буквальном репро; control (no ancestor manifest) — PASS
  как был; ancestor-manifest-без-дефиса — тоже теперь PASS (доказывает,
  что фикс адресует настоящую причину, не дефис).
- **Airtight δ0-протокол** (урок `[M-172.13-cross-repo-c-diff-noise-not-regression]`
  — старый бинарь ПРОТИВ ТОГО ЖЕ дерева, не другого репо): `git checkout --`
  трёх изменённых файлов В ТОМ ЖЕ worktree → пересборка = чистый pre-fix
  бинарь на pre-fix ИСТОЧНИКЕ этого же дерева. На нём: буквальный репро —
  RED (совпадает с самым первым замером); `std/src/runtime/string_builder_test.nv`
  `--keep-artifacts` → `.c` MD5 `bcefb77645973789c8687d8a1e6ee8e5`. Восстановил
  fix из бэкапа, пересобрал — тот же файл даёт ИДЕНТИЧНЫЙ MD5
  `bcefb77645973789c8687d8a1e6ee8e5`. **δ0 подтверждён строго** (не через
  сравнение с main repo, который тем временем ушёл вперёд на 208-Ф4R
  str/char/bool rich-spec — ловушка, которая дала ложный 6-строчный diff
  при первой, неправильной попытке через main-репо-бинарь).
- Rust: `oot_ancestor_manifest_module_path.rs` (новый, 2 теста —
  dash-in-path + no-dash control) + `oot_interp_stringbuilder.rs`
  (прецедент, не менялся) — оба 3/3 PASS на финальном (fix) бинаре.
- In-tree D78-neg: `spec_tests/conformance/d78_root_peers/bad_root_decl_neg.nv`
  (`nova test`) → `PASS (negative)` (ошибка E_D78 корректно поймана раннером
  как ожидаемый негатив); `nova_tests/negative_capability/folder_inconsistent_decl/{alpha,beta}.nv`
  (`nova check`, по прецеденту 42.08-audit-closing.md — «N/A folder-level
  тест, каждый peer отдельно») → оба корректно FAIL с E_D78 (per-file, не
  тронуты). Директория `folder_inconsistent_decl/` целиком (`nova test <dir>`)
  даёт `PASS:0 FAIL:0` (не содержит `test`-блоков — это compile-error-only
  neg fixtures, задуманные для per-file `nova check`, см. `docs/plans/42.08-audit-closing.md:39`).
- `nova check std` (in-tree, ~1200 файлов): фикс-бинарь → `PASS:141 FAIL:18
  WARN:1033`; тот же счёт на main-репо бинаре (доп. sanity, до анализа
  cross-repo-noise ловушки) — совпадает, хотя это слабее чем δ0-протокол
  выше (типчек нечувствителен к display_spec-codegen дрейфу).
- Модель: sonnet. Worktree `nova-ootdash`, ветка `p-fix-oot-dash`. В main НЕ
  мёржил, не пушил.
