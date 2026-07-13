<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 202 — прогресс-ледер

**Родитель:** [202-module-registry-path-keyed-and-root-module.md](202-module-registry-path-keyed-and-root-module.md).
Исполнитель: sonnet, worktree `nova-p202` (ветка `plan-202`), в main не мёржить.

## Ф.1 ШАГ-0 — грep-инвентарь потребителей module-ключа/decl-строки

Метод: греп `read_module_decl`/`module_name`/`module_key`/`ModuleSigTable`/
`fn_module_map`/`file_priv_fn_c_names`/`colliding_type_names`/`type_defining_modules`
по `compiler-codegen/src`, `nova-cli/src`, `nova-lsp/src` + чтение каждого сайта.
Вердикт per-потребитель — не «на глаз», по факту чтения кода ключа.

| # | Потребитель | Файл | Ключ сегодня | Вердикт |
|---|---|---|---|---|
| 1 | **`resolve_one`/`resolve_imports_inline_ex`** — `visited`/`in_progress` (diamond-dedup + cycle-guard) | `compiler-codegen/src/imports.rs` (~1129-1817) | `module_key = read_module_decl(first_path)` — **decl**, canonical-path только как fallback при отсутствии decl | **ГЛАВНАЯ ЦЕЛЬ Ф.1.** Дубль decl из разных физических модулей → второй `visited.get(&module_key)`-дедупится как «уже resolved» → экспорты второго тихо не мержатся (репро §2а). Переводится на canonical-path key. |
| 2 | **`collect_all_signatures`/`collect_sigs_one`/`ModuleSigTable::insert`** — sig pre-pass (D292/D293) | `compiler-codegen/src/imports.rs` (~281-445) | `visited`/`in_progress: HashSet<Vec<String>>` по decl; `ModuleSigTable.table: HashMap<Vec<String>, ModuleSignatures>` — key = `sigs.module_name` (decl) | **ВТОРОЙ реестр с идентичным дефектом.** `table.insert` перезатирает сигнатуры первого same-decl модуля сигнатурами второго (silent overwrite). Обязан переводиться СИНХРОННО с #1 — иначе резолвер чинится, а sig_table (is_known_fn/is_known_type, cross-module fn-резолв) продолжает жить по decl → второе окно идентичности (ровно риск, которого опасается карта плана). |
| 3 | **`SigRegistry`/`method_table`** (checker+codegen unified sig registry, Plan 172.1 U.2) | `compiler-codegen/src/sig_registry.rs` | `by_key: HashMap<(Option<String> receiver, String name), Vec<SigEntry>>` — построен ИЗ УЖЕ СМЕРЖЕННЫХ `module.items`; никакого module-identity ключа нет вообще | **Подтверждён безвреден сам по себе** (не хранит module-key). Корректность данных, которые в него попадают, зависит от #1 (резолвер не должен терять items) и от #4/#5 (codegen-mangling не должен путать одноимённые items из разных физических модулей). Не требует правки в Ф.1; входит в периметр Ф.1b верификации. |
| 4 | **D307 file-discriminated free-fn mangling** — `fn_module_map`, `file_priv_fn_c_names`, `colliding_fn_names`-детектор | `compiler-codegen/src/codegen/emit_c.rs` (~4227-4370, ~16855-16888) | `name_modules: HashMap<String, BTreeSet<Vec<String>>>` — множество РАЗЛИЧНЫХ decl, объявляющих имя; `mangle_free_fn(&pf.module_name, &f.name)` — суффикс мангла = decl | **ТРЕБУЕТ Ф.1b-фикса.** Два физически разных same-decl модуля вставляют ОДИН и тот же `Vec<String>` в `BTreeSet` → `mods.len()==1` → коллизия НЕ детектится (хотя после Ф.1 обе физические копии реально мержатся в `merged_items`) → одинаковый mangled C-символ для двух разных приватных fn с одинаковым именем → C redefinition / потеря одного тела. Расширение D381-прецедента: группировка обязана учитывать физическую идентичность (canonical path), не только decl-строку. |
| 5 | **D381 colliding-type qualification** — `type_def_modules`, `colliding_type_names`, `qualify_type_base`/`def_type_base`/`ref_type_base`, `file_type_module` | `compiler-codegen/src/codegen/emit_c.rs` (~4383-4470, ~3579-3620, и ~15 читающих сайтов ниже) | `type_def_modules: HashMap<String, BTreeSet<Vec<String>>>` — множество decl, объявляющих тип; qualified C-base = `format!("{}_{}", module.join("_"), name)` | **ТРЕБУЕТ Ф.1b-фикса, идентичная ось.** Same-decl дубль → `mods.len()==1` → НЕ коллидирует по детектору → ОБА типа эмиттятся под одним bare `Nova_<Name>` C-именем (структурная коллизия/переопределение). Плюс: даже если детектор починен на «физическую» коллизию, `qualify_type_base` сам по себе даёт ОДИНАКОВЫЙ суффикс для двух физических модулей с одинаковым decl — нужен вторичный дизамбигьюатор. |
| 6 | **`nova-lsp` module index** (автокомплит импортов) | `nova-lsp/src/stdlib_index.rs` (~55-108) | `module_path` строится ПРЯМЫМ обходом ФС (`rel_segments` = полный путь от source root), decl вообще не читается | **Path-keyed уже.** Не подвержен дефекту — каждый файл получает уникальный ключ по построению (обход дерева, не парсинг decl). Изменений не требует. |
| 7 | **`nova-lsp` прочие модули** (state/symbol/compiler/goto_definition/…) | `nova-lsp/src/*.rs` | Нет отдельного module-key реестра — используют `nova_codegen::imports`/`manifest` API как есть | **Безвреден косвенно.** Потребитель через общий resolve-pipeline — наследует фикс #1 автоматически, без отдельной правки. |
| 8 | **`build_cache::compute_c_key`** (content-addressed кэш `.c`) | `nova-cli/src/build_cache.rs` | Хэш по `&[PeerFile]` (canonical `path` + содержимое файла), decl не участвует в ключе | **Path-keyed уже.** Инвалидация кэша идёт по физическим файлам — безвреден. |
| 9 | **`TypeCheckCtx.type_defining_modules` / `type_method_map`** (D281/D286 — priv-boundary + inherent/extension) | `compiler-codegen/src/types/mod.rs` (~3839-3917, ~16617-16653) | `type_defining_modules: HashMap<String, Vec<String>>` — «first wins» по decl; `type_method_map` дедупит modules-list по `.contains(&pf.module_name)` (decl-equality) | **Декл-кеинг подтверждён, но ОСТАТОЧНЫЙ риск ниже acceptance-порога Ф.1b.** При дубле decl с одноимённым `priv`-типом в РАЗНЫХ физических модулях: `type_defining_modules` укажет на ПЕРВЫЙ физический модуль для обоих → priv-boundary enforcement станет over-permissive (файл физ.модуля-B формально «пройдёт» как свой для типа физ.модуля-A). Это НЕ путает VALUES и не ломает компиляцию (Ф.1b acceptance = «компилируется, значения не смешаны») — заведено как остаточный backlog-риск (см. §5 итог), вне обязательного объёма Ф.1b. |
| 10 | **`nova doc` JSON/MD рендер сигнатур** | `compiler-codegen/src/doc/{collector,render_json,render_md}.rs` | Строится из `SigRegistry`/module.items постфактум, атрибуция по имени/модулю для отображения | **Вне объёма гейтов Plan 202** (`nova doc` не входит в приёмочные гейты §2 плана). Не проверялось прицельно; при обнаружении визуальной путаницы атрибуции в будущем — отдельный тикет. |
| 11 | **`manifest::check_module_path[_with_kind]`** (E_D78_MODULE_PATH_MISMATCH identity-check) | `compiler-codegen/src/manifest.rs`, вызывается из `nova-cli/src/main.rs`, `compiler-codegen/src/main.rs`, `test_runner.rs` | Per-entry-file проверка декларации против ЕЁ ЖЕ пути (не реестр, не агрегирует несколько модулей) | **Ортогонален Ф.1, без изменений.** Остаётся identity-check файла (по плану: «decl = identity-check, не routing key») — это и есть механизм, который Ф.1 СОХРАНЯЕТ. |
| 12 | **`embed_resolve.rs`** упоминание `build_cache` | `compiler-codegen/src/embed_resolve.rs:27` | Комментарий-ссылка на #8, самостоятельного реестра нет | **Н/о.** |
| 13 | **`sig_registry.rs`-потребители** (`external_registry.rs`, `runtime_registry.rs`, `protocols/auto_derive.rs`, `strict_effects.rs`) | `compiler-codegen/src/codegen/*`, `compiler-codegen/src/protocols/*`, `compiler-codegen/src/strict_effects.rs` | Читают `SigRegistry`/`method_table` API как есть, не строят отдельный module-keyed реестр | **Наследуют #3** — безвредны при условии корректности #1/#2/#4/#5. |

### Итог ШАГ-0

- **Обязательные правки Ф.1:** #1, #2 (резолвер + sig-table, СИНХРОННО, иначе второе окно идентичности).
- **Обязательные правки Ф.1b:** #4, #5 (mangling-ось, D381-прецедент расширяется на физическую идентичность
  вместо decl-строки — и в детекторе коллизий, И в самом дизамбигьюаторе суффикса).
- **Подтверждены безвредны без изменений:** #3, #6, #7, #8, #11, #12, #13.
- **Остаточный риск вне обязательного объёма (задокументирован, не блокирует Ф.1b-гейт):** #9 (priv-boundary
  over-permissive при decl-дубле — не путает значения, не ломает компиляцию).
- **Вне объёма гейтов Plan 202:** #10 (`nova doc`).

Commit этой таблицы — ДО правки резолвера (план требует commit-first).

## Ф.1 — реализация (path-keyed реестр)

**Код:** `compiler-codegen/src/imports.rs` — новая `pub(crate) fn canonical_module_key(resolved_paths:
&[PathBuf]) -> Vec<String>` (identity = canonical filesystem path: parent-директория для
folder-module-пира, сам файл для single-file — не зависит от алфавитного порядка peers, см.
doc-comment у функции). Заменяет decl-based `module_key`/`entry_key` в:
- `resolve_one`/`resolve_imports_inline_ex` (`visited`/`in_progress`, главный резолвер);
- `collect_all_signatures`/`collect_sigs_one`/`ModuleSigTable::insert` (D292/D293 sig pre-pass —
  переведён СИНХРОННО, `insert` теперь принимает explicit `key: Vec<String>` вместо `sigs.module_name`).

Декларация (`module_name`) остаётся ТОЛЬКО identity-check (`E_D78_MODULE_PATH_MISMATCH`,
`manifest.rs`, без изменений) — по плану «decl = identity-check, не routing key».

**Репро §2а → PASS.** Ручной репро (`d/nova202repro`, вне репо) с двумя `neg.x`-модулями в разных
поддеревьях: `nova check` — PASS (раньше `who_b` — «undefined identifier», якорь на module-decl).
Побочно найден и обойдён НЕ связанный с Plan 202 баг: файл с basename `x` коллидирует с
внутренним синтезированным идентификатором дериватора (`E7401 no function 'compare' in module
'x'` при `nova build`, не при `nova check`) — переименование в `helper.nv` убирает симптом;
отмечено ниже как отдельный backlog-маркер, вне объёма Plan 202.

**Pos-фикстура:** `spec_tests/conformance/d78_dup_decl_registry/{a/neg/helper.nv, b/neg/helper.nv,
entry_d78_dup_decl.nv}` — оба `neg.helper` (разные поддеревья `a/`, `b/`), оба экспорта (`who_a`,
`who_b`) живы, значения не смешаны (`assert(who_a()==1)`, `assert(who_b()==2)`). `nova test` →
**PASS**.

## Ф.1b — mangling-ось (расширение D381-прецедента)

При первом прогоне pos-фикстуры Ф.1 всплыл ОБЯЗАТЕЛЬНЫЙ (не гипотетический) баг: `CC-FAIL
redefinition of 'nova_fn_3neg6helper6secret'` — обе одноимённые приватные `secret()` манглились в
ОДИН C-символ, потому что D307-детектор коллизий (`name_modules: HashMap<String,
BTreeSet<Vec<String>>>`, `emit_c.rs`) группировал по **decl** (`pf.module_name`): два физически
разных модуля с одинаковым decl схлопывались в ОДНУ запись множества (`mods.len()==1` — коллизия
НЕ детектится), а сам `mangle_free_fn`/`qualify_type_base` манглит по decl — даже задетекченная
коллизия дала бы идентичный C-символ.

**Код:** `compiler-codegen/src/codegen/emit_c.rs`, `emit_module` — общий блок ПЕРЕД fn- и
type-коллизионными под-блоками:
```
let mut phys_key_of: HashMap<u32, Vec<String>> = ...;        // file_id → canonical_module_key
let mut decl_phys_groups: HashMap<Vec<String>, BTreeSet<Vec<String>>> = ...; // decl → {physical keys}
let effective_modpath = |pf: &PeerFile| -> Vec<String> { ... };  // decl, либо decl+"dupN" при
                                                                   // decl_phys_groups[decl].len()>=2
```
`effective_modpath(pf) == pf.module_name` для ЛЮБОГО decl, отображающегося РОВНО в один физический
модуль (весь существующий корпус, 0 деклараций сегодня расшарены ≥2 физическими модулями — раньше
такое глоталось резолвером, а не существовало легально) — byte-identical гарантия сохранена.
Применено к: `name_modules`/`module_file_ids`/`mangle_free_fn`/`fn_module_map` (fn-ось) и
`type_def_modules`/`emit_file_module`/`file_type_module`-branch-(1) (type-ось, D381).

**Известный остаточный пробел (вне обязательного объёма Ф.1b):** branch (2) `file_type_module`
(суффикс-матчинг import-пути к DEFINING module) не различает decl-дубль при импорте типа
СЕЛЕКТИВНО извне (не из своего же модуля) — деградирует к «ambiguous, оставить неквалифицированным»
(существующий safe fallback, не путает значения, но может вернуть C-коллизию в этом узком
сценарии). Обе Ф.1b-фикстуры этот путь не используют (тип используется только ВНУТРИ
объявляющего модуля, экспортируются только функции-аксессоры) — задокументировано, не блокирует
гейт. Кандидат-маркер: `[M-d78-dup-decl-type-cross-import-ambiguous]`.

**Фикстуры (обе PASS через `nova test`, debug-бинарь):**
- fn-ось: переиспользует Ф.1 pos-фикстуру (`d78_dup_decl_registry/`) — `secret()` линкуется, значения
  разные (1/2).
- type-ось: `spec_tests/conformance/d78_dup_decl_type_axis/{a/neg/kind.nv, b/neg/kind.nv,
  entry_d78_dup_decl_type.nv}` — оба `neg.kind`, разные варианты sum-типа `Kind` (`Alpha|Beta` vs
  `Alpha|Gamma`), конструкция+match внутри каждого модуля, наружу — только функции-аксессоры.
  `assert(describe_a(1)=="a-alpha")`, `assert(describe_b(1)=="b-gamma")` и т.д. — значения не
  смешаны.

## Ф.2 — root peers (D78 rev-4)

**Код:**
- `compiler-codegen/src/manifest.rs` — новая `pub fn expected_root_peer_decl(file, m) ->
  Option<Vec<String>>` (Some(`[package_name]`) для файлов, чей родитель == `source_root`, иначе
  None); wired как ДОПОЛНИТЕЛЬНАЯ acceptance-ветка в `check_module_path_with_kind` (после rev-3,
  до финального Err) — легальна ОДНОВРЕМЕННО с независимой `<package>.<stem>` формой (смешанный
  корень).
- `compiler-codegen/src/imports.rs`:
  - `is_peer_group_member(path)` — новый helper (существующий `is_folder_module_peer` ИЛИ decl ==
    single-segment package_name И parent == source_root); используется ТОЛЬКО в
    `canonical_module_key`, не трогает `is_folder_module_peer`'ов контракт (D78 validation в
    manifest.rs использует старую функцию как прежде).
  - `collect_root_peers(source_root, package_name, include_test_peers)` — сканирует прямых детей
    source_root, фильтрует по декларации `[package_name]`, теми же test/target-фильтрами что и
    обычный folder-peer сбор.
  - `resolve_module_paths`: новая ветка ПЕРЕД generic single-file/folder поиском — для
    single-segment import, совпавшего с package_name корневого кандидата (через `nova.toml` того
    root), возвращает `collect_root_peers`. Для cross-package `[dependencies]`-импорта (`dep_root`)
    — тот же путь, но package_name берётся из `imp.path[0]` (уже провалидирован
    `lookup_dependency`'s `NameMismatch`-веткой), без re-parse манифеста.
  - Ослаблена жёсткая ошибка «голое имя зависимости требует путь к модулю»: bare `import <dep>`
    (1 сегмент) теперь легален, ЕСЛИ у зависимости есть root peers (иначе прежняя ошибка).
- Sibling-коллекция entry-группы (`resolve_imports_inline_ex`, decl-based, без явной
  `is_folder_module_peer`-проверки) уже была ОБЩЕЙ и заработала для root peers без изменений
  (verified — см. ниже).

**Проверено вручную (`d/nova202rootpeers` + `d/nova202rootpeers_consumer`, вне репо):**
- pos: `client.nv`/`server.nv` (`module tls`) — `nova check` PASS, sibling-коллекция подтянула оба
  без import (entry-group namespace sharing).
- pos cross-package: consumer-пакет с `[dependencies] tls = { path = "../..." }`,
  `import tls.{hello_client, hello_server}` — PASS через `nova test` (`assert` на оба значения).
- pos смешанный корень: `util.nv` (`module tls.util`, независимый) + root peers в одном
  source_root — `import util.{util_value}` (внутрипакетный, БЕЗ префикса имени пакета — абсолютные
  внутрипакетные импорты не содержат имя пакета, только `import <package_name>.{...}` явно для
  ССЫЛКИ НА root peers из независимого файла того же пакета) — PASS.
- neg: файл прямо в source_root с decl `module wrongname` (ни rev-3, ни root-peer форма) —
  `E_D78_MODULE_PATH_MISMATCH`, сообщение перечисляет все три ожидаемые формы.

**Fixtures (в репо, `nova test` verified):**
`spec_tests/conformance/d78_root_peers/` — СОБСТВЕННЫЙ `nova.toml` (package `d78_root_peers`,
отдельно от `spec_tests`, чтобы легально сработало правило package-name-matching):
`client.nv`+`server.nv` (root peers), `util.nv` (независимый, смешанный корень), `entry_root_peers.nv`
(explicit `import d78_root_peers.{...}` + `import util.{...}`, test-блок с assert), `bad_root_decl_neg.nv`
(`EXPECT_COMPILE_ERROR E_D78_MODULE_PATH_MISMATCH`). Прогон
`nova test --compile-error --positive spec_tests/conformance/d78_root_peers`: **PASS: 2, FAIL: 0,
SKIP: 3** (client/server/util — compiled OK, no test blocks, норма).

**Спек-амендмент D78 rev-4** — `spec/decisions/07-modules.md`: два блока — (1) keying-семантика
(D29 «Свойства правила» п.4, дубль decl из разных физических модулей легален и безвреден после
Ф.1) + (2) root peers (новая секция под D78 «Path / module enforcement»). В том же слиянии, что
код (lang-change-needs-spec).
