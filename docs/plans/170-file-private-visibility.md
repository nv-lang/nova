# Plan 170 — `priv(file)`: file-private видимость для peer-модулей

> **Создан:** 2026-06-19. **Статус:** 📋 proposed — execution-ready.
> **Приоритет:** P1 — разблокирует/упрощает консолидацию тестов
> ([Plan 169.1.2](169.1.2-consolidate-by-theme.md)/[169.1.3](169.1.3-consolidate-partial-d.md)):
> `priv(file)` на helper'ах вместо ordinal-rename конфликтующих имён.
> **Зависит от:** Plan 160 / D281 (инфраструктура `priv`/`priv(type)`).
> **Spec:** новый D-блок (amend D281/D220).

## Проблема

folder-module = один модуль из co-equal peer-файлов (D29/D78). Top-level имена
видны всем peer-файлам модуля. При консолидации тестов (по плану или по теме)
одноимённые `fn helper`/`type Acc` в разных файлах **конфликтуют** → сейчас
лечится ordinal-suffix rename (Cat-B, `scripts/catb_convert.py`) — некрасиво,
теряет читаемость имён.

Нет уровня видимости «только в этом файле». Лесенка сейчас:
- `export` — виден снаружи модуля,
- `priv` (голый) — module-private (виден peer-файлам, D281),
- `priv(type)` — type-private (только свой тип).

Недостаёт **самого узкого** уровня — file-private.

## Решение: `priv(file)`

Расширить существующий квалификатор-синтаксис `priv(<scope>)`:
```nova
priv(file) type Acc { … }      // виден только в этом файле
priv(file) fn helper() -> int  // не виден peer-файлам модуля
priv(file) const K = 42        // file-local константа
priv          type Job { … }   // module-private (без изменений, D281)
priv(type)    field …          // type-private (без изменений, D220)
export        fn api() …        // публичный (без изменений)
```

Лесенка scope (от узкого к широкому):
`priv(file)` ⊂ `priv` (module) ⊂ `export`.

**Концепция:** `priv(file)` — **visibility-hint**, НЕ смена module-резолва.
Модуль остаётся один (D29 не нарушается); символ просто помечен «не виден
peer-файлам». Аналог Rust `pub(self)` внутри модуля.

## Фазы

### Ф.1 — Parser + AST
- `parser/mod.rs`: расширить разбор `priv(…)` на top-level items (`fn`/`type`/
  `const`) — добавить `priv(file)` (сейчас `priv(...)`-парсинг есть для полей,
  ~строка 3586; обобщить на item-level visibility).
- `ast/mod.rs`: top-level visibility получает уровень `File` (сейчас по сути
  export-vs-нет; ввести enum `ItemVisibility { Export, Module, File }`, default
  = Module для не-export top-level). Хранить `file_id` объявления (уже есть в
  `Span`).

### Ф.2 — Checker (резолв + enforcement)
- При резолве top-level имени из ДРУГОГО файла того же folder-module: если
  символ `priv(file)` и `decl.span.file_id != use.span.file_id` → имя НЕ видимо
  (как будто не существует в этом файле) → обычная ошибка «unknown name» ИЛИ
  спец-диагностика `E_FILE_PRIV_LEAK` с подсказкой.
- Cross-module (export) и module-private (`priv`) — без изменений.
- Дедупликация: два `priv(file) fn helper` в разных peer-файлах НЕ конфликтуют
  (разные file-scope) — снимает причину ordinal-rename.

### Ф.3 — Codegen
- `priv(file)` символы мангли́ть с file-discriminator (напр. суффикс file_id
  или stem), чтобы два одноимённых `priv(file) fn helper` из разных файлов
  давали разные C-символы без коллизии линковки. (Аналогично тому, как
  module-private уже мангли́тся.)

### Ф.4 — Spec + тесты
- D-блок (amend D281/D220): добавить `priv(file)` в privacy-лесенку; таблица
  scope; `E_FILE_PRIV_LEAK`.
- Тесты `nova_tests/plan170/`:
  - pos: `priv(file) fn` виден в своём файле;
  - pos: два одноимённых `priv(file)` в peer-файлах сосуществуют (один module);
  - neg: peer-файл ссылается на чужой `priv(file)` → `E_FILE_PRIV_LEAK`;
  - pos: `priv` (module) по-прежнему виден peer-файлам (регрессия D281).

## Применение (зачем сейчас)

После Plan 170 консолидация ([169.1.2](169.1.2-consolidate-by-theme.md)/
[169.1.3](169.1.3-consolidate-partial-d.md)) упрощается: вместо ordinal-rename
конфликтующих helper'ов — пометить их `priv(file)`. Ноль rename, читаемые имена,
происхождение не размывается. → **делать Plan 170 ДО агрессивной консолидации**
(Уровень 2 по теме особенно выигрывает).

Польза шире тестов: любой folder-module (`std/collections/vec/` из co-equal
`core`/`access`/`mutate`/…) сможет иметь приватные file-local helper'ы.

## Нейминг — почему `priv(file)`, не `local`

Единая ось видимости под одним ключевым словом `priv` (+ scope-квалификатор),
консистентно с `priv(type)`. `local fn` двусмысленно (путается с вложенными
локальными функциями). `priv` уже зарезервировано; `local` — новое KW с риском
коллизии идентификаторов. (Обсуждение 2026-06-19.)

## Followup-маркер

`[M-170-priv-file-visibility]`.
