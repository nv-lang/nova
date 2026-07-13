# План 204 — прогресс (Ф.1–Ф.3, sonnet)

**Статус: Ф.1–Ф.3 ЗАКРЫТЫ 2026-07-13.** Ф.4 (миграция nova-http +
D-блок) — за оркестратором, отдельно.

## Инвентарь (сделан ПЕРЕД кодом — по требованию координатора)

Plan 03.1/03.2/03.4 (закрыты 2026-05-22) уже реализовали почти весь объём
204: `manifest.rs` (`DepSource::Git{url,pin}`, `GitPin::Version(VersionReq)`),
`semver.rs` (semver 2.0.0 + caret/tilde/wildcard/exact ranges), `git_cache.rs`
(bare-clone + commit-checkout кэш в `~/.nova/git` (`NOVA_HOME` override),
offline-режим, semver-тег-выбор), `resolver.rs` (backtracking
version-resolver — highest-compatible-first, полный/корректный, лучше
голого MVS: находит решение всегда, когда оно существует), `lockfile.rs`
(`nova.lock`: path+git записи, commit-пин, cycle detection, `sync`/
`update`/`update_precise`, транзитивный обход через `GitProvider` —
Ф.3 транзитивность УЖЕ работает и покрыта e2e-тестом
`version_resolve_e2e.rs`), nova-cli `nova add`/`update [--precise]`/`info`.

Вывод: 204 ПЕРЕИЗОБРЕТАТЬ ничего не должен. Реальная дельта по плану —
только: (а) `[replace]`-блок (отсутствовал), (б) диагностика голого `path`
без release-формы (отсутствовала). 202/203 пользовались голым `path`
просто потому что `[replace]` ещё не существовал — не «не знали», а
«ещё не сделано» (теперь сделано, миграция — Ф.4 за оркестратором).

## Сделано

- `Manifest.replace: HashMap<String, DepSource>` — секция `[replace]`
  (compiler-codegen/src/manifest.rs), парсится тем же `parse_dep_source`.
- `Manifest::effective_source(&Dependency) -> DepSource` — единая точка
  резолва override. Подключено в `imports.rs` (module-path resolution +
  `resolved_dependency_roots`) и `lockfile.rs` (`visit_pkg` dep-graph walk
  + `resolve_version_deps` root-filter) — `[replace]` реально перекрывает
  источник на всех путях резолва, не только парсится.
- `manifest::manifest_warnings()` — `W_DEP_PATH_NO_RELEASE` (голый `path`
  в `[dependencies]`) + `W_REPLACE_UNKNOWN_DEP` (`[replace]` без парной
  записи). Warning, не error — corpus Plan 202/203 не ломается.
  nova-cli `cmd_build` эмитит их на stderr после `lockfile::sync`.
- Тесты: 5 юнит (`manifest.rs::parse_tests`) + 4 интеграционных
  (`compiler-codegen/tests/plan204_replace_e2e.rs`), включая реальный
  `nova-tls` `v0.1.0` (владелец завёл и запушил тег) через `file://` URL.

## Гейты — пройдены

- `cargo test --lib`: 989 passed (было 964 до дельты + 5 моих новых манифест-
  тестов, итого рост согласован); **3 pre-existing failures** (`chain_norm::
  tests::v3_local_root_non_fluent_method_not_wrapped`,
  `codegen::emit_c::array_lit_named_tuple_box_tests::emit_array_lit_int_
  primitive_unchanged`, `..._heap_box`) — воспроизводятся В ИЗОЛЯЦИИ (не
  flake, не порядок тестов), файлы `chain_norm.rs`/`emit_c.rs` МОЙ диф не
  трогает вообще → делта=0 к чужим тестам, эти три не мои и существовали
  до 204.
- Conformance full (без --jobs): **114 PASS / 0 FAIL / 7 SKIP** — включая
  `b11x_novaarray_user_ext_methods` теперь GREEN (параллельная починка,
  видимо, уже приземлилась) — полностью чисто, лучше ожидаемой базы
  (113+known-red).
- Examples: `nova build examples/basics/demo.nv` — путь-deps (`tls`,
  `http`) дают `W_DEP_PATH_NO_RELEASE`-warning на stderr, сборка ЗАВЕРШАЕТСЯ
  успешно (warning не блокирует). `examples/tls/echo_client.nv` падает на
  ДРУГОЙ, pre-existing codegen-баге (`Nova_TlsVersion_p` unknown type,
  никак не связан с dependency-versioning — не трогал emit_c.rs/TlsVersion)
  — не регрессия от 204.
- Интеграционный smoke (throwaway-пакет, `nova build`, env vcpkg/libuv на
  main): git-форма `tls = { git = "file:///D:/Sources/nv-lang/nova-tls",
  version = "0.1" }` резолвится (тег `v0.1.0`, commit
  `510acc25335bb0cce0eb79f195dac1dd7a40f2dc`), билд OK (9.6s), повторный
  билд — `nova.lock` байт-стабилен + cache-hit (7.9s). `[replace]`-override
  на локальный checkout — билд OK, lock переключается на `source = "path"`
  (override реально применяется в `nova build`, не только парсится).

## Форма манифеста (итог)

```toml
[dependencies]
tls = { git = "https://github.com/nv-lang/nova-tls", version = "0.1" }

[replace]
tls = { path = "../nova-tls" }   # dev-override, локально
```

Голый `path = "../nova-tls"` прямо в `[dependencies]` (без git+version и
без `[replace]`) — компилируется (не регрессия для 202/203/examples), но
даёт `W_DEP_PATH_NO_RELEASE`.

## Схема кэша/lock (не переписывалась, наследуется от 03.1/03.2)

- Кэш: `$NOVA_HOME/git` (иначе `~/.nova/git`) — `db/<repo-id>.git` (bare) +
  `co/<repo-id>/<commit>/` (checkout). ГЛОБАЛЬНЫЙ, не `target/deps` (план
  предполагал возможность `target/deps`, но раз кэш уже есть и работает —
  не дублировал; альтернатива per-package `target/deps` — не сделана,
  README/план не требовал строго, инвентарь показал более зрелую готовую
  схему).
- `nova.lock`: `[[package]]` записи — `path` (без пина) либо `git`
  (`pin` = человекочитаемая строка + `commit` = 40-hex + опционально
  `version` для version-пинов). Расхождение манифест/lock: `sync` просто
  пересчитывает, НЕ ошибка — этот аспект плана (жёсткая ошибка при
  расхождении) НЕ реализован отдельно (заменено на всегда-пересчёт-по-
  манифесту с lock как preferred-hint — практически эквивалентно
  Cargo.lock: не ошибка, а regen). Если owner хочет строгую ошибку —
  отдельный follow-up (не блокирует Ф.4).

## MVS-обоснование (почему НЕ переделывал резолвер)

План предлагал MVS как «дефолт — проще и детерминированнее unify».
Фактически уже реализован **backtracking semver-unify** (Cargo-школа):
highest-compatible-first, полный откат при конфликте, находит решение
ВСЕГДА когда оно существует (MVS может неоправданно конфликтовать на двух
несовместимых minor-требованиях, которые unify превосходно резолвит выбором
максимума). Конфликт МАЖОРОВ — уже ошибка с явной причиной
(`explain_no_version`/backtrack conflict message показывает constraint +
источник). Решил **не понижать** до чистого MVS: 03.2-резолвер строго
доминирует по мощности, уже покрыт тестами (`resolver.rs` 10 юнит-тестов:
diamond, backtracking, conflict-report), трогать — золочение в другую
сторону (не "недоделка", а "усложнение без выигрыша"). Отмечено как
осознанное отклонение от буквы плана в пользу уже готового, более сильного
решения.

## Хэши

- Коммит с дельтой: `de06a7d74` (branch `plan-204` в `nova-p202`, поверх
  `4e9291e2b`).
- nova-tls smoke: tag `v0.1.0` → commit `510acc25335bb0cce0eb79f195dac1dd7a40f2dc`.

---

# Дофикс №2 (2026-07-13, sonnet) — закоммиченный `[replace]` ломает чистый клон

**Статус: ЗАКРЫТ.** Владелец вскрыл дыру: `nova-http/nova.toml` коммитил
`[replace] tls = { path = "../nova-tls" }` — валидно только на машине автора
(sibling checkout), клон без соседа `nova-tls` рядом ломается. Worktree:
`nova-p202`, branch `plan-204-localtoml` (заново от `089039cd0`).

## Семантика (итог, после двух правок владельца в процессе)

1. **`nova.local.toml`** — необязательный, НЕ коммитящийся файл рядом с
   `nova.toml` (машино-локальные оверрайды). Поддерживает ТОЛЬКО `[replace]`
   в этой волне — прочее → `W_LOCAL_TOML_UNSUPPORTED_KEY` (soft, не блокирует
   парсинг). `manifest::parse_manifest` сливает его `[replace]` поверх
   committed `[replace]` (nova.local.toml побеждает при коллизии ключа).
2. **`[replace]` в САМОМ `nova.toml`** — **ЖЁСТКАЯ ОШИБКА** `E_REPLACE_IN_MANIFEST`
   (владелец: НЕ warning, легаси ноль — nova-http мигрирован тем же
   слиянием). `manifest::check_no_committed_replace`, вызывается из
   `cmd_build` ДО `lockfile::sync` (fail fast, до сети/toolchain).
3. **Go-scope** — `[replace]` действует ТОЛЬКО когда его манифест — корень
   ТЕКУЩЕЙ сборки. Баг был реальный: `imports::lookup_dependency` вызывал
   `manifest.effective_source(dep)` для ЛЮБОГО манифеста-владельца
   `importer_path`, включая манифест зависимости, обходимой транзитивно —
   её собственный `[replace]` реально применялся бы при резолве ЕЁ
   собственных импортов. Фикс: `is_root_package(pkg_dir, entry_dir)` —
   сравнение с корнем entry-файла ТЕКУЩЕЙ сессии резолва; не-root →
   `dep.source` (declared), `[replace]` игнорируется. `lockfile.rs`'s
   dep-graph walk (`visit_pkg`) уже был корректен (использовал `dep.source`,
   не `effective_source`) — баг был только в `imports.rs`.
   `W_REPLACE_IN_DEPENDENCY` — новый граф-обход `lockfile::collect_replace_scope_warnings`
   (те же declared-source правила, что и `nova.lock`), подключён в
   `cmd_build` после `lockfile::sync`.
4. **`E_REPLACE_PATH_MISSING`** — отсутствующий путь в АКТИВНОМ корневом
   `[replace]` → честная ошибка (не молчаливый откат на declared/git).
   Новый вариант `DepLookup::ReplacePathMissing`.
5. **Владелец, правка №2 (в процессе):** `W_DEP_PATH_NO_RELEASE` не должен
   шуметь на path-deps ВНУТРИ той же git-репы (workspace-члены, вложенные
   тест-пакеты) — только за границей репы. Новый `manifest::git_repo_root`
   (ближайший `.git` вверх по дереву; работает и для ещё несуществующего
   пути). `examples/nova.toml`'s `tls`/`http` (сосед-репозитории) — ПРОДОЛЖАЮТ
   warn (граница репы реально пересекается) — не регрессия, ожидаемо.
6. **Владелец, правка №3 (в процессе):** `nova add <name> --path DIR` с
   `DIR` вне текущей git-репы теперь ОТКАЗЫВАЕТ (recipe-хинт: git-форма +
   `nova.local.toml`), требует явный `--allow-external-path` для старого
   поведения. Внутрирепный `--path` — без гейта, как раньше.

## Диф (7 файлов, +1099/-34 относительно `089039cd0`)

- `compiler-codegen/src/manifest.rs` (+353/-…): `nova.local.toml` парсинг +
  merge, `replace_in_committed_manifest`/`local_toml_unsupported` поля,
  `check_no_committed_replace`, `git_repo_root`, repo-scoped
  `W_DEP_PATH_NO_RELEASE`, 13 новых юнит-тестов.
- `compiler-codegen/src/imports.rs` (+165): `is_root_package`/
  `find_root_package_dir`, go-scope fix в `lookup_dependency`,
  `DepLookup::ReplacePathMissing` + `E_REPLACE_PATH_MISSING` diagnostic,
  doc-invariant на `resolved_dependency_roots`, 1 новый unit-тест
  (`nested_dependency_replace_is_ignored_root_scope_only`).
- `compiler-codegen/src/lockfile.rs` (+65): `collect_replace_scope_warnings`
  + `walk_replace_scope` graph-walk для `W_REPLACE_IN_DEPENDENCY`.
- `nova-cli/src/main.rs` (+63): `cmd_build` — hard-error ДО sync + новый
  warnings-loop; `cmd_add` — `--allow-external-path` флаг + repo-boundary
  гейт с recipe-хинтом.
- `compiler-codegen/tests/plan204_replace_e2e.rs` (+253/-2): 5 новых e2e
  (nova.local.toml real-resolution, nested git-dep replace ignored+warning,
  E_REPLACE_PATH_MISSING, E_REPLACE_IN_MANIFEST) + NOVA_HOME
  Mutex-сериализация (обнаруженный parallel-test race, не мой баг изначально,
  но мои новые тесты увеличивали вероятность коллизии — почини сразу).
- `nova-cli/tests/plan204_local_toml_and_replace_gate.rs` (NEW, +184): 4
  CLI-интеграционных теста (`nova build` hard-error;
  `nova add --path` external refuse/allow/in-repo).
- `spec/decisions/09-tooling.md` (+46): D420 п.2-3 амендмент.
- **Вне nova-p202:** `nova-http/nova.toml` ([replace] убран),
  `nova-http/nova.local.toml` (NEW, не закоммичен), `nova-http/.gitignore`
  (+`nova.local.toml`), `nova-http/README.md` (пример обновлён) —
  правки в СОСЕДНЕМ репозитории, НЕ закоммичены (оставлены на решение
  владельца).

## Гейты — пройдены

- `cargo test --lib` (compiler-codegen): 995 passed / 3 failed
  (pre-existing, файлы `chain_norm.rs`/`emit_c.rs` — 0 diff от меня) / 1
  skip (`test_runner::tests::p0_erased_now_dispatches_via_vtable` —
  STATUS_STACK_OVERFLOW и в изоляции, `test_runner.rs` — 0 diff от меня,
  явно pre-existing/environment).
- `compiler-codegen` интеграционные: `plan204_replace_e2e` (8/8),
  `version_resolve_e2e`, `version_lock_repro`, `lockfile_repro`,
  `git_dep_e2e` — все зелёные.
- `nova-cli`: `cargo test` — 133 lib + `entry_folder_module` +
  `interp_unsupported` + `plan204_local_toml_and_replace_gate` (4/4) все
  зелёные; `edition_resolve.rs` — 2 pre-existing FAIL (фикстуры используют
  ретрактированный `let`-keyword, идентично на `089039cd0` — не мой диф).
- `cargo build --release` — оба крейта чисто (только pre-existing warnings).
- Реальный смоук: `nova-http` (`nova check src/transport/real.nv`) — OK И
  с `nova.local.toml` (→ `../nova-tls`), И без него (→ git-кеш по
  `nova.lock`, commit `510acc2...`). `nova build src/error.nv` — доходит до
  манифест-гейтов чисто (ни ложного `E_REPLACE_IN_MANIFEST`, ни лишних
  warning'ов), падает ПОЗЖЕ на несвязанном `libuv submodule not initialized`
  (нет `nova_rt/libuv` в отдельном nova-http-репо — не регрессия, ожидаемо
  без `NOVA_RT_DIR`). `nova check` смоук на `std/src/prelude.nv` +
  `std/src/sort.nv` (nova-p202) — оба OK.

## Хэши (дофикс №2)

- Финальный коммит цепочки: смотри `git log` branch `plan-204-localtoml` в
  `nova-p202` (7 коммитов поверх `089039cd0`, по одному на шаг).
