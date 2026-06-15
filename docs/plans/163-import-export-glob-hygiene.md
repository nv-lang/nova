<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 163 — Гигиена import/export: запрет glob-форм (named + alias only)

> **Создан:** 2026-06-16. **Статус:** 📋 PLANNED. P3 (мелкий, шипится независимо).
> **Владеет:** `[M-import-glob-forbid]` (новый).
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
| `import m` (всё, без префикса) | **запретить** `E_IMPORT_GLOB` (Ф.2) — ИЛИ переопределить в qualified `m.foo` (Q-import-glob-hygiene) |
| `import m.{a, b}` | ✅ оставить |
| `import m as x` | ✅ оставить (qualified) |
| `export import m` (всё) | **запретить** `E_REEXPORT_GLOB` (Ф.1) |
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

## Связь / отложенное

- **Plan 162** — если делается раньше/параллельно, вариант (b) (qualified `import m`) встраивается в новый резолвер естественно; иначе (a) — быстрый front-end guard.
- **Plan 42.09 / 42.04** — это их прямое продолжение (re-export / per-file import scope).
- tree-shaking-аспект barrel — **уже** закрыт Plan 159 (DCE); этот план — про **поверхность имён** и компайл-тайм, не про мёртвый код.
