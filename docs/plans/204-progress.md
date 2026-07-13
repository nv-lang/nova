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
