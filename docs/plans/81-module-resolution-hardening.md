// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 81: Module-resolution hardening — закрытие оставшихся недоработок ресолва

> **Создан 2026-05-21.** Консолидирует все открытые недоработки
> резолва модулей, разбросанные по закрытым планам.
>
> **Статус:** 📋 proposed, не начат.
>
> **Источник:** аудит module-resolution 2026-05-21 — открытые пункты
> [Plan 35](35-cross-file-resolve.md) (sub-plans 35.B-E + R26),
> [Plan 70.1](70.1-module-alias-resolution.md) (known-limitation) и
> ряд маркеров `simplifications.md`. Эти планы закрыты как MVP —
> доработка фич переносится сюда.

---

## Зачем

Bootstrap-MVP резолва модулей закрыт (Plan 35 — cross-file resolve;
Plan 42 + 16 sub-plans — folder-modules; Plan 70.1 — alias codegen).
Но MVP оставил «честные пропуски»: часть из них — про **корректность**
(нарушение спеки), часть — про **качество кода** и **производительность**.
Этот план собирает их в один список и доводит до production-grade.

## Что НЕ входит (отклонено спекой — не недоработки)

- **Относительные пути** `import ../sibling` — D29: import всегда
  full path от корня. Дизайн-решение, остаётся.
- **Wildcard** `import X.Y.*` — R25 spec-rejected (D29/D5). Bare-name
  доступ — только через `import X as alias` + `alias.fn()`.
- **pub-гранулярность** (R28) — spec-rejected (D5).

## Что уже сделано (для контекста — НЕ задачи плана)

selective `import X.{A,B}`, `export import` re-export, prelude
auto-import, `#cfg` conditional compilation (Plan 42.12/42.16),
`nova test` cross-file parity (R31), folder-modules, alias codegen.

---

## Фазы

### Ф.1 — Visibility enforcement (P1 — корректность)

**Проблема:** флаг `is_export` сейчас чисто информационный.
Приватные (не-`export`) элементы импортированного модуля **доступны**
в импортирующем коде — нарушение спеки D29/D5.

**Задача:** type-checker обязан скрывать не-`export` элементы
импортированного модуля; обращение к ним → ошибка «undefined
identifier» (или отдельный E-код «private item»). Peer-файлы одного
folder-модуля видят приватные элементы друг друга (это правильно) —
граница только на границе модуля.

*(Было: Plan 35 R26 «post-bootstrap».)*

### Ф.2 — Cross-file generic bounds (P2 — функц. дыра)

**Проблема:** `[T Hashable]`, где `Hashable` объявлен в другом модуле,
type-checker не резолвит — bound не проверяется.

**Задача:** резолвить generic-bounds cross-file; убрать workaround
«inline-дублирование bound-протокола в каждом файле».

*(Было: Plan 35.C.)*

### Ф.3 — Symbol mangling v0 (P2 — безопасность codegen)

**Проблема:** импортированные элементы лежат в глобальном C-namespace
без стабильного mangling — риск коллизии линковки, если пользователь
переопределит имя stdlib-типа.

**Задача:** стабильная схема mangling v0 (module-path → C-префикс),
D-блок со спецификацией схемы (**резерв D134**; D133 занят Plan 80).

*(Было: Plan 35.D, часть 1.)*

### Ф.4 — Dead-code elimination (P2 — качество)

**Проблема:** все импортированные элементы эмитятся в C, даже
неиспользуемые (`import std.collections.range` тянет 20+ методов,
используются ~2) — раздувание бинарника и время компиляции.

**Задача:** tree-shaking — эмитить только достижимые из entry
импортированные элементы.

*(Было: Plan 35.D, часть 2.)*

### Ф.5 — FileId propagation в диагностику (P2 — UX)

**Проблема:** все `Span` в импортированных элементах имеют
`file_id = 0` (MAIN_FILE_ID) — ошибка в импортированном модуле
показывается против main-файла.

**Задача:** прокинуть реальный `file_id` через резолв импортов;
cross-file диагностика указывает на настоящий файл и строку.
Может потребовать предварительной FileId-инфраструктуры — оценить
в начале фазы.

*(Было: маркер `simplifications.md` «FileId propagation».)*

### Ф.6 — Build cache + incremental (P3 — производительность)

**Проблема:** каждый `nova build` заново парсит все импорты; нет
dependency-based пересборки.

**Задача:** on-disk кэш разобранных модулей + инкрементальная
пересборка по графу зависимостей.

*(Было: Plan 35.B.)*

### Ф.7 — Alias member-call type-check + негативные тесты (P2)

**Проблема:** `import X as a; a.unknown()` даёт link-error
(undefined symbol), а не compile-error — `EXPECT_COMPILE_ERROR`
не ловит. Type-checker не валидирует Member-call против
alias-резолвленной сигнатуры.

**Задача:** type-checker проверяет `alias.func(args)` против
сигнатуры функции модуля; неизвестный метод / неверные аргументы →
compile-error. Негативные фикстуры для `nova_tests/plan70_1/`.

*(Было: Plan 70.1 known-limitation.)*

### Ф.8 — Entry-folder-module peer-isolation (P3)

**Проблема:** per-peer import isolation не активна, если **сам
entry-модуль** — folder-module (entry парсится как один файл,
MAIN_FILE_ID).

**Задача:** активировать per-peer резолв и для entry-folder-module.

*(Было: `[M-entry-folder-module]`, designed-defer Plan 42.17 Ф.8.)*

### Ф.9 — Cross-module mutual recursion (P3 — edge-case)

**Проблема:** single-pass typecheck merged AST — взаимная рекурсия
через границы модулей может ломаться (flat-зависимости работают).

**Задача:** 2-pass typecheck (сигнатуры → тела), чтобы mutual
recursion через модули резолвилась.

*(Было: маркер `simplifications.md` «AD3 sig/body 2-pass».)*

### Ф.10 — spec sync + чистка simplifications.md + README

- Обновить `spec/decisions/07-modules.md` (visibility, mangling D134).
- Почистить `simplifications.md`: MVP-таблица Plan 35 (~стр. 4595-4612)
  и секция wildcard (~стр. 4405) устарели — отметить сделанное /
  spec-rejected, закрыть маркеры, перенесённые в этот план.
- Обновить `docs/plans/README.md`.

---

## Приоритеты

| Фаза | Приоритет | Природа |
|---|---|---|
| Ф.1 visibility | **P1** | корректность (нарушение спеки) |
| Ф.2 generic bounds | P2 | функц. дыра |
| Ф.3 mangling | P2 | безопасность codegen |
| Ф.4 DCE | P2 | качество |
| Ф.5 FileId | P2 | диагностика / UX |
| Ф.7 alias type-check | P2 | корректность ошибок |
| Ф.6 cache | P3 | производительность |
| Ф.8 entry-folder | P3 | edge-case |
| Ф.9 mutual recursion | P3 | edge-case |

Рекомендованный порядок: Ф.1 → Ф.7 → Ф.2 → Ф.3 → Ф.4 → Ф.5 → Ф.9 →
Ф.8 → Ф.6 → Ф.10. Фазы независимы — можно закрывать по одной
отдельными коммитами.

## Зависимости

- Опирается на закрытые Plan 35 / Plan 42.x / Plan 70.1.
- Ф.5 может потребовать FileId-инфраструктуры (оценить в начале фазы).

## Ссылки

- [Plan 35](35-cross-file-resolve.md) — cross-file resolve MVP; sub-plans
  35.B-E и R26 перенесены сюда.
- [Plan 42](42-folder-modules.md) — folder-modules; `[M-entry-folder-module]`
  → Ф.8.
- [Plan 70.1](70.1-module-alias-resolution.md) — alias codegen;
  known-limitation → Ф.7.
- `docs/simplifications.md` — маркеры FileId / AD3 / MVP-таблица.
- `spec/decisions/07-modules.md` — D29 (модули), D5 (видимость).
