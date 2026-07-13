<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 203 — чекпоинт (вынос http из std в nv-lang/nova-http)

**Исполнитель:** sonnet, worktree `nova-p202` (ветка `plan-203`, база `8f848fe0e`
= main с планом 203). Репа-цель `nova-http` (новая, ветка `master`).

## Ф.1 — перенос (готово)

`std/src/http/**` (37 `.nv` файлов) скопирован в НОВУЮ репу
`d:/Sources/nv-lang/nova-http` (`git init`, ветка `master`), структура по
эталону `nova-tls` после 202-Ф.3:
- `nova.toml`: `[package] name = "http"`, `[lib] src = "src"`,
  `[dependencies] tls = { path = "../nova-tls" }` (форма — прецедент
  `std/nova.toml`/`examples/nova.toml`, Plan 202 Ф.2/Ф.3).
- Корневые файлы (`body`, `cookie`, `effect`, `error`, `header`, `message`,
  `method`, `mime`, `response_ext`, `status`, `url`, `version` + их
  `*_test.nv`) переведены на root peers (D78 rev-4): `module std.http` →
  `module http`. Подпапки (`client/`, `server/`, `servernet/`(+`rt/`),
  `serdejson/`, `transport/`, `neg/`) — decl-строки БЕЗ изменений (те же
  строки механически совпадают после переноса на уровень выше: parent —
  та же непосредственная папка, package name не участвует на этой глубине).
- Внутренние импорты `std.http.*` → `http.*` по всем файлам; `tls`/`std.*`
  импорты не тронуты.
- Найдены 8 внутрипакетных cross-submodule ссылок (`servernet.nv`,
  `transport/real_test.nv`, `serdejson/{serdejson,typed_json_test,
  typed_body_repro_test}.nv`, `neg/{response_not_consumed,
  server_header_setter_panic}.nv`, `servernet/rt/*_smoke.nv`) —
  абсолютная форма `http.server.{...}`/`http.client.{...}` не резолвится
  для НЕ-root-peer подмодуля (в отличие от однoсегментного `http.{...}`,
  который матчит root-peer правило); переведены на относительные импорты
  (`../server.{...}`, `../../servernet.{...}`) — D78/Plan 84 relative-import
  форма, уже используемая в `examples/flagship/aggregator`.
- README/LICENSE-MIT/LICENSE-APACHE/.gitignore — по образцу `nova-tls`.
  Баннер-комментарии файлов переписаны в стиле nova-tls (`nova-http: <file>
  — <desc>. Extracted from monorepo std/http (Plan 203 Ф.1)`).

Коммит: `d971228` (nova-http).

## Ф.2 — потребители (готово)

Инвентарь: `grep -rn "import http" std/ examples/ spec_tests/ nova_tests/`
— единственный внешний потребитель (кроме самого http-модуля) —
`examples/flagship/aggregator` (`server.nv` + `server_test.nv`).
`examples/nova.toml`: добавлена `[dependencies] http = { path =
"../../nova-http" }` рядом с `tls`. Импорты aggregator'а: `std.http.server.
{...}` → `http.server.{...}`.

**Обратные зависимости std → http: НЕ найдены** (в std ничего кроме самого
http-модуля не импортировало `http.*`) — Ф.3 без разрывов, СТОП-условие
плана не сработало.

Коммит: `86eb7def6` (nova-p202).

## Ф.3 — вычистка std (готово)

`std/src/http/**` удалён целиком. `std/nova.toml`: убрана `[dependencies]
tls` (единственный потребитель внутри std был `http/transport/real.nv` +
`http/error.nv` — обоих больше нет) — **std снова самодостаточен, 0 внешних
зависимостей** (мотив плана достигнут).

Коммит: `071771335` (nova-p202).

## Побочная находка — 2 компилятор-бага в резолвере (закрыто тем же заходом)

Cross-package smoke (Ф.3 гейт) вскрыл ДВА реальных дефекта в
`compiler-codegen/src/imports.rs`, не связанных с tls-веткой Plan 178:

1. **Own-package root-peer self-reference не резолвился транзитивно.**
   `http.server`'s `import http.{Method, ...}` (root-peer self-reference,
   1 сегмент) работал ВНУТРИ nova-http (entry_dir/repo естественно = свои),
   но ломался при потреблении ЧЕРЕЗ внешнего консьюмера
   (`examples/flagship/aggregator` → `[dependencies] http`) — `entry_dir`/
   `repo` в `resolve_one` зафиксированы на ВНЕШНЕМ entry на всю сессию
   резолва, root-peer detection в `resolve_module_paths` проверял только
   их. `tls` не ловил этот баг раньше (весь surface — root peers, нет
   подпапок, самоссылка не нужна); `std` ловил через отдельный hardcoded
   `"std"`-спецкейс (`stdlib_dir`, не зависящий от entry).
   **Фикс:** `lookup_dependency` — self-reference (`imp.path[0] ==`
   собственный `package_name` файла) резолвится как reflexive `dep_root`
   (`PathDep(own source_root)`), переиспользуя уже рабочий external-dep
   путь.
2. **Регрессия от фикса №1, найдена и закрыта тем же заходом.** Generic
   `dep_root`+single-segment путь в `resolve_module_paths` расширял
   `local_rel` до ПУСТОГО (`parts[1..]` для 1-сегментного пути) и листил
   ВСЕ `.nv` файлы `source_root` неотфильтрованными по module-декларации —
   для «смешанного корня» (root peers + независимые single-file модули в
   одном source_root, D78 rev-4 §7) это протащило независимый модуль в
   root-peer группу и дало `redefinition` при codegen
   (`spec_tests/conformance/d78_root_peers/entry_root_peers`, CC-FAIL).
   **Фикс:** явный `collect_root_peers(root, ...)`-путь для
   `dep_root+len==1` (тот же фильтрованный механизм, что и non-dep_root
   ветка).

Коммит: `522b11382` (nova-p202, `compiler-codegen/src/imports.rs`, +48
строк).

## Гейты

| Гейт | Результат |
|---|---|
| `nova check std` (дельта) | 18 FAIL — байт-идентичны baseline (86eb7def6, до Ф.3), все pre-existing neg-фикстуры вне http. Дельта = ровно 11 исчезнувших http-строк (проверено детач-worktree на baseline-коммите тем же бинарём). |
| `spec_tests/conformance` полный, без `--jobs` | **PASS: 111 / FAIL: 0 / SKIP: 7** — совпадает с базой из задания, 0 регрессий (после обоих резолвер-фиксов; включает `d78_root_peers`/`d78_dup_decl*` фикстуры, которые ловят фикс №2). |
| `nova test src` (nova-http) | Import resolution ЧИСТ (0 ошибок резолва). Остаточные 6 CODEGEN-FAIL — **ортогональный pre-existing блокер**: `nova-tls` (внешняя зависимость, path-dep, live checkout) требует компилятор-фикс `D133-амендмент` (Plan 178, `M-178-consume-field-ctor-from-var`, в разработке в `nova-p201`/ветка `m178-consume-field`, ещё НЕ влит в main). Подтверждено независимо от Plan 203: (а) `nova test src` В САМОЙ nova-tls тем же биналём даёт идентичную ошибку; (б) `nova check` на baseline `std/src/http/{body,transport/real}.nv` (commit 86eb7def6, ДО Ф.3) даёт ИДЕНТИЧНУЮ ошибку — т.е. блокер существовал ДО выноса http, не регрессия этого плана. |
| Сборка/чек флагмана (`examples/flagship/aggregator`) | Import resolution чист. Тот же остаточный tls/D133-блокер (см. выше). |
| Cross-package smoke (независимый пакет, throwaway, `[dependencies] http = {path=...}`, `import http.{Method, StatusCode, HeaderMap}` + `import http.client.{HttpClient}`) | Import resolution чист. Тот же остаточный tls/D133-блокер. |

**Итог:** миграция http (структура пакета, module-decl, импорты, потребители,
вычистка std) — **полностью корректна и зелена** по ВСЕМ import-resolution-
уровневым проверкам (check/build без codegen через tls-цепочку не доходят
до этой строки). Единственный незелёный кусок гейтов — orthogonal
pre-existing блокер в `nova-tls`, НЕ вызванный и НЕ усугублённый этим
планом; повторный прогон ожидается автоматически зелёным после слияния
Plan 178's `m178-consume-field` в main (тот же паттерн, что и Plan 202's
собственный gate-3 environment-skew, `202-progress.md`: «дельта исчезает
при слиянии в текущий main»).

## Вне объёма

- Ф.4 (публикация на github) — за оркестратором.
- Fix Plan 178's D133/consume-field gap — чужой, уже идущий заход
  (`nova-p201`), не трогал.
