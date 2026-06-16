<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 163 — Гигиена import/export: запрет glob-форм (named + alias only)

> **Создан:** 2026-06-16. **Статус:** ✅ CLOSED+AMENDED (2026-06-16). P3 (мелкий, шипится независимо).
> **Владеет:** `[M-import-glob-forbid]` (CLOSED).
> **Часть модульной концепции:** [Plan 42 — folder-modules](42-folder-modules.md) ([D29](../../spec/decisions/07-modules.md#d29-модули-и-импорты)) — уточняет **Rule C** «per-file imports remain per-file scope» и [42.09 re-export](42.09-re-export.md) / [42.04 per-file imports scope](42.04-per-file-imports-scope.md): запрещает «всё-подряд»-формы импорта/реэкспорта.
> **Координируется с:** [Plan 162](162-rust-model-module-resolution.md) (Rust-модель резолва — опция «бэр `import m` → qualified» из Ф.2 ниже согласуется с резолвером 162).
> **Research:** [docs/research/11-stdlib-method-resolution-reachability.md](../research/11-stdlib-method-resolution-reachability.md) (раздел про barrel/`pub use`).

## Проблема (по коду)

`import` у нас = **inline-merge** имён модуля в текущий. Отдельного токена `*` нет, но **две формы функционально являются glob/barrel**:

1. **glob-импорт:** `import m` (целый модуль, без `.{}` и без `as`) тащит **все публичные имена `m` без префикса** — функционально `use m::*`. Риск: «откуда это имя» (Go выкинул re-export именно за потерю этого).
2. **barrel-реэкспорт:** `export import m` (целый модуль, без `.{}`) — неконтролируемый публичный API наружу (скрытая поверхность, semver-ломкость, «откуда»).

Текущее состояние: в коде **все 39 `export import` — именованные** (`.{...}`), т.е. соглашение уже соблюдается, но **грамматика whole-форму допускает** → дыра латентна. Безопасные формы: `import m.{a,b}` (выбранные имена), `import m as x` (qualified `x.foo`).

Контекст из исследования: barrel/`export *` — известный антипаттерн (TS build-perf + tree-shaking; «откуда взялось»); хорошая практика — **явный именованный re-export + маленький курируемый prelude**. tree-shaking-аспект у нас уже закрыт [Plan 159](159-reachability-codegen.md) (DCE срезает неиспользуемое из бинаря); остаётся гигиена поверхности имён и компайл-тайма.

## Цель

Запретить **glob-формы**, оставить **named + alias**:

| Форма | Решение |
|---|---|
| `import m` (без `as`, без `.{}`) | ✅ **легален** (Ф.4 amend) — last segment = qualified name (`m.Foo`) |
| `import a.b.X as X` (alias = last seg) | **запрещён** `E_REDUNDANT_IMPORT_ALIAS` (Ф.4) |
| `import m as alias` (alias ≠ last seg) | ✅ оставить (non-redundant alias) |
| `import m.{a, b}` | ✅ оставить |
| `export import m` (всё) | **запрещён** `E_REEXPORT_GLOB` (Ф.1) |
| `export import m.{a, b}` | ✅ оставить (prelude/фасады живы) |

G0-инвариант: **не банить фичи целиком** (re-export нужен для prelude/фасадов; named-import нужен) — банить **только** «всё-подряд»-формы.

## Фазы

- **Ф.1 — `E_REEXPORT_GLOB`.** Запретить whole-module `export import m` (без `.{}`). **Нулевая миграция** (в коде таких нет — все 39 именованные). Дёшево, шипится первой. Закрывает главную barrel-дыру.
- **Ф.2 — `import m` (whole, unqualified): решение по Q-import-glob-hygiene.**
  - **(a) Запрет** `E_IMPORT_GLOB` → миграция ~60 whole-импортов (`import std.sort` → `import std.sort.{sort}` или `… as sort`).
  - **(b) Переопределить** в qualified namespace (Go/Python): `import m` легален, но даёт `m.foo` (с префиксом); `import m.{...}` — для выборочного без префикса. Эргономичнее, без `as`-шума; но это **семантическое изменение резолвера** → согласовать с [Plan 162](162-rust-model-module-resolution.md) Ф.1. Рекомендация: **(b)** (мейнстрим, меньше миграции «впихни alias»), если 162 уже в работе; иначе **(a)** как быстрый guard.
- **Ф.3 — Тесты + миграция std/nova_tests + close.**

## Тесты (через релизный nova & компилятор)

- **NEG:** `export import m` (без `.{}`) → `E_REEXPORT_GLOB` (span на import); `import m` (без `.{}`/`as`) → `E_IMPORT_GLOB` (вариант a) ИЛИ резолв в qualified `m.foo`, а `foo` без префикса = ошибка (вариант b).
- **POS:** `import m.{a,b}`, `import m as x`, `export import m.{a,b}` — компилируются; prelude (`std.prelude.*`, именованный re-export) не задет.
- **Регресс:** полный `nova test` (kill-switch-методология не нужна — это не codegen-изменение, чистый front-end gate) — zero NEW FAIL после миграции.

## Критерии приёмки

- **A1.** `export import m` (whole) → ошибка; именованный re-export работает; prelude цел.
- **A2.** `import m` (whole, unqualified) → ошибка (a) или qualified-семантика (b) по Q; named/alias не задеты.
- **A3.** std/nova_tests мигрированы; полный регресс zero NEW FAIL.
- **A4.** Грамматика/диагностика: ясное сообщение + хинт («используйте `import m.{...}` или `import m as x`»).
- **G0 (обязательный):** не запрещены полезные формы (named import, named re-export, alias, prelude-фасад); запрет точечный по glob-формам; миграция полная, не частичная.

## Спека (D — при реализации)

- Amend [D29](../../spec/decisions/07-modules.md#d29-модули-и-импорты) / [Plan 42](42-folder-modules.md) Rule C: «whole-module unqualified import и whole-module re-export запрещены; разрешены named (`.{}`) и alias (`as`)». Зафиксировать выбор Ф.2 (a vs b).

## Статус по завершении (2026-06-16)

Все фазы Ф.1-Ф.4 РЕАЛИЗОВАНЫ. Ключевые изменения:

- **Ф.1** (`compiler-codegen/src/types/mod.rs`): `E_REEXPORT_GLOB` — `export import m` без `.{}` → ошибка. Нулевая миграция (все 39 `export import` в коде уже именованные).
- **Ф.2** (`compiler-codegen/src/types/mod.rs`): `E_IMPORT_GLOB` V1 — `import m` без `.{}`/`as` → ошибка. Вариант **(a) запрет** (временный guard). Prelude auto-imports освобождены.
- **Ф.3** — миграция V1: ~100 файлов (bare `import X` → `import X as X`).
- **Ф.4 (D289 amend, 2026-06-16):** `E_IMPORT_GLOB` убран → `import m` легален, last segment = qualified name (`import vec_iter` = `vec_iter.Foo`). Добавлен `E_REDUNDANT_IMPORT_ALIAS` (`import a.b.X as X` запрещён — alias совпадает с default). ~123 файла мигрированы `import X as X` → `import X`.

**Тесты:** `nova_tests/plan163/` — 6 fixtures: `f1_reexport_glob_neg` ✅, `f2_named_reexport_pos` ✅, `f3_import_glob_neg` → переписан в позитивный (bare import работает) ✅, `f4_import_as_pos` ✅ (alias ≠ last seg разрешён), `f5_import_selective_pos` ✅, `f6_redundant_alias_neg` ✅ (E_REDUNDANT_IMPORT_ALIAS). plan163 PASS 6/0.

**Критерии приёмки (Ф.4):**
- **A5.** `import m` без `as` компилируется; `m.Foo` доступен (qualified). Prelude цел.
- **A6.** `import a.b.X as X` → `E_REDUNDANT_IMPORT_ALIAS` с hint «write `import a.b.X`».
- **A7.** `import a.b.X as alias` где `alias ≠ X` → легален (non-redundant alias остаётся).
- **A8.** ~123 файла мигрированы `as X` → убран суффикс; 0 новых FAIL.
- **G0 (обязательный, без упрощений как для прода):** полная миграция, не частичная; чекер применяется ко всем peer_files (entry + siblings); prelude освобождён; non-redundant alias (`as lib`) цел.

**D-блоки:** D288 (`E_REEXPORT_GLOB`), D289 amend (option b: last-segment qualified + `E_REDUNDANT_IMPORT_ALIAS`).

**Q-import-glob-hygiene:** RESOLVED (option b, amend от option a, 2026-06-16).

## Связь / отложенное

- **Plan 162** — принята параллельно. `imported_modules` в Plan 81 Ф.2 уже хранил `path.last()` — D289 amend формализует эту семантику.
- **Plan 42.09 / 42.04** — прямое продолжение (re-export / per-file import scope).
- tree-shaking-аспект barrel — закрыт Plan 159 (DCE); этот план — про поверхность имён.
