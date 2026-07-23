<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 223 — src-прозрачность в entry-режиме («src/ невидим всегда», D78 rev-5)

**Статус:** ✅ РЕАЛИЗОВАН 2026-07-23 (ОКНО-2, sonnet, ветка `p-okno2-derive-seed-223`) —
Ф.0 (D78 rev-5 амендмент в 07-modules.md §Source root) + Ф.1 (`apply_src_transparency` +
`expected_module_path_rev3`/`expected_module_path` bare-режим + `E_MODULE_DIR_SRC_RESERVED`
в `compiler-codegen/src/manifest.rs`) + Ф.2 (10 файлов агрегатора мигрированы на
`module {main,api,app,domain}`) + Ф.3 (4 фикстуры pos/neg в
`spec_tests/conformance/d78_src_transparency/`, RED→GREEN). Таргетный гейт агента зелёный
(см. §3); авторитетный гейт (conformance folder-CU целиком + флагман-CI) — за интегратором.
**Приоритет:** P2, объём малый. **Очередь:** ДО тегов v0.1 — `module src.main` виден прямо в витрине-флагмане;
язык-меняющее (резолюция модулей) → D-амендмент ОБЯЗАН ехать в слиянии реализации.

## 0. Мотив (одной фразой)

`src/`-канон (Plan 195, D78) прозрачен в манифест-режиме (`[lib] src` — nova-http объявляет
`module http.server`, слова «src» в путях нет), но в entry-режиме (приложение без манифеста:
`nova build <root>/src/main.nv`) корень модулей выводится = `<root>`, и `src` протекает в
декларации: флагман-агрегатор пишет `module src.main`/`src.app`/`src.api`/`src.domain` —
асимметрия-полурешение, видимая цена src/-канона. Решение владельца: **одно правило без
исключений — «src/ невидим ВЕЗДЕ», слово src остаётся только на диске (как в Rust)**.

## 1. Норма (D78 rev-5, амендмент §«Source root»)

1. **Entry-режим:** если entry-файл лежит под каталогом `src/` (ближайший предок с этим
   именем на пути от выведенного корня), module root = этот `src/`; сегмент `src` НИКОГДА
   не входит в module path. Флагман: `<aggregator>/src/main.nv` → `module main`;
   `src/app/*.nv` → `module app`; `src/api/*.nv` → `module api`; `src/domain/domain.nv` →
   `module domain`.
2. **Entry вне `src/`** (например `regressions/<name>/<file>.nv`, `examples/*.nv`) —
   поведение НЕ меняется (вывод корня как сегодня).
3. **`src` — зарезервированное имя каталога:** папка-МОДУЛЬ с именем `src` внутри source
   root — compile error `E_MODULE_DIR_SRC_RESERVED` (иначе `src/src/` делает правило 1
   неоднозначным). Симметрично обоим режимам.
4. **Манифест-режим не меняется** (`[lib] src` уже отрезает src; root peers D78 rev-4 без
   изменений).
5. **Path/module enforcement** (D78 §): suggestion-ветка «rename module to: …» не должна
   предлагать имена с сегментом `src` (обновить генерацию подсказки).

## 2. Фазы (одна sonnet-волна, worktree)

- **Ф.0 Спека:** D78 rev-5 амендмент в `spec/decisions/07-modules.md` §«Source root»
  (датированный, в стиле амендмента Plan 195 там же) + правка suggestion-примера в
  §«Path/module enforcement» при необходимости. ЕДЕТ В ТОМ ЖЕ СЛИЯНИИ, что реализация.
- **Ф.1 Резолвер:** entry-режим вывода корня (зона: matching декларации к пути /
  «module declaration does not match file path» — `compiler-codegen/src/types/mod.rs`,
  `main.rs`; агент локализует точную функцию по строке ошибки) + `E_MODULE_DIR_SRC_RESERVED`.
- **Ф.2 Миграция:** РОВНО 10 файлов агрегатора (полный грепом подтверждённый список
  `^module src\.` в дереве): `src/main.nv`, `src/api/{report_json,report_json_test}.nv`,
  `src/app/{aggregate,aggregate_test,live,live_test,scenarios,emit}.nv`,
  `src/domain/domain.nv` → декларации без `src.`-префикса (main/api/app/domain).
  Относительные импорты (`./domain`, `./app`) — не трогаются; README упоминания
  `src/main.nv` (пути на диске) — не трогаются, это файловые пути.
- **Ф.3 Фикстуры (standalone, конвенция §116):** pos — entry в `src/`: модульные пути без
  src (мини-копия раскладки `<r>/src/{main.nv,app/x.nv}`); pos — entry ВНЕ src не задет;
  neg — `module src.main` при entry в src → path/module-mismatch error, suggestion БЕЗ
  `src`-сегмента (пин текста); neg — папка-модуль `src` внутри source root →
  `E_MODULE_DIR_SRC_RESERVED`.

## 3. Гейты

Таргетно (агент): фикстуры Ф.3 + `nova build examples/flagship/aggregator/src/main.nv
--strict-effects` зелёный + smoke (`/`, `/api/run` → 200) + все `regressions/*` фикстуры
компилятся (root вне src не задет) + `nova build examples/mini_aggregator.nv` (entry вне
src, регресс). Авторитетный гейт (интегратор): conformance folder-CU + флагман-CI
(резолюция модулей — язык-меняющее).

## Связи

D78 (07-modules.md §Source root, rev-4 root peers) · Plan 195 (src-канонизация) ·
Plan 202 (path-keyed реестр, ЗАКРЫТ) · Plan 203 (эталон nova-http) · обсуждение-решение
владельца 2026-07-22 (асимметрия «библиотека src-невидим / приложение src-виден» снята).
