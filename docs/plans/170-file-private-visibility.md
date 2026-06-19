# Plan 170 — `priv(file)`: file-private видимость для peer-модулей

> **Создан:** 2026-06-19. **Статус:** ✅ **CLOSED 2026-06-19** (ветка `plan-170-priv-file`).
> **Реализовано полностью (Ф.1–Ф.4), без упрощений, production-grade.**
> `priv(file) fn`/`type`/`const` парсится; взаимоисключение с `export` (оба порядка);
> file-private символ невидим peer-файлам (`E_FILE_PRIV_LEAK`), виден в своём; два
> одноимённых `priv(file)` в peer-файлах сосуществуют (checker-дедуп + file-
> дискриминированный codegen для free fn И const — нет линк-коллизии); module-private
> (D281) / `export` без регрессий. **D-блок: D307** (план называл D304, но D304 занят
> Plan 169.1.1; D305/D306 зарезервированы Plan 104.10 → присвоен D307). Тесты
> `nova_tests/plan170`: 13/13 pos-блоков + 5/5 neg PASS. Регресс plan160/plan124*/
> modules/std — 0 новых FAIL (baseline-diffed против main). Followup
> `[M-170-priv-file-visibility]` закрыт. Коммиты: `feat(170)` impl, `docs(170)` spec
> D307, `test(170)` coverage, `docs(170)` logs + close.
>
> **Приоритет:** P1 — разблокирует чистую консолидацию тестов
> ([169.1.2](169.1.2-consolidate-by-theme.md)/[169.1.3](169.1.3-consolidate-partial-d.md)).
> **Зависит от:** Plan 160 / D281 (инфраструктура `priv`/`priv(type)` для полей).
> **Оценка:** ~1–1.5 dev-day.

---

## 0. Рабочая среда и правила (ОБЯЗАТЕЛЬНО прочитать)

**Worktree.** Создать постоянный изолированный worktree, не работать в основном
рабочем дереве пользователя:
```
git worktree add -b plan-170-priv-file ../nova-p170 main
```
Все пути ниже — относительно корня репозитория (в worktree — соответственно).

**Сборка** (Windows, Git Bash + PowerShell доступны):
- ⚠️ **Windows-гоча:** Edit-инструмент не всегда обновляет mtime так, чтобы cargo
  заметил. ПЕРЕД каждой сборкой принудительно обновить mtime изменённого .rs:
  PowerShell `(Get-Item "путь.rs").LastWriteTime = (Get-Date)`.
- Сборка CLI: `cd nova-cli && cargo build --release` (бинарь
  `nova-cli/target/release/nova.exe`). Одна пересборка ~1.5–2 мин.
- ⚠️ Изменения в `parser`/`ast`/`types`/`codegen` (крейт `nova-codegen`) требуют
  пересборки `nova-cli` (он зависит от крейта).

**Тестирование — ТОЛЬКО через C-codegen** (НЕ интерпретатор):
- `./nova-cli/target/release/nova.exe test nova_tests/plan170` — прогон папки.
- `./nova-cli/target/release/nova.exe test-build nova_tests/plan170/<file>.nv` —
  один файл с полным выводом ошибки (для отладки).
- bash cwd = корень репо; для worktree использовать cd-префикс в каждой команде.

**Правила коммитов / процесса:**
- НЕ трогать `nova-lsp` (параллельный агент).
- `git add` ТОЛЬКО конкретных файлов; НИКОГДА `git add -A`/`.` (рядом другие агенты).
- Перед `git commit` всегда `git diff --cached --stat` (в индексе м.б. чужое).
- НЕ добавлять trailer `Co-Authored-By: Claude`.
- Коммит по фазам (Ф.1–Ф.4 — отдельные коммиты).
- После закрытия: обновить логи — `docs/project-creation.txt` (одна строка-итог),
  `docs/plans/backlog-followups.md` (закрыть маркер), `docs/simplifications.md`
  (если есть упрощение синтаксиса).
- Один раз `nova test` с capture summary + FAIL details (не гонять в цикле).

**Регресс перед закрытием** (изменения видимости — чувствительны):
`nova test nova_tests/plan160` (D281), `nova test nova_tests/plan124*` (D220/D222),
`nova test nova_tests/modules`, `nova test std` — 0 новых FAIL.

---

## 1. Проблема

folder-module = один модуль из co-equal peer-файлов (D29/D78): все `.nv` в папке
с одинаковым `module X` — один compile unit, top-level имена видны всем
peer-файлам. При консолидации тестов одноимённые `fn helper`/`type Acc` в разных
файлах **конфликтуют** → лечится ordinal-rename (некрасиво).

Текущая лесенка видимости top-level (по факту бинарна — `is_export: bool`):
- `export` — виден снаружи модуля;
- (без `export`) — module-private (виден всем peer-файлам).

Недостаёт самого узкого уровня — **file-private**.

## 2. Решение: `priv(file)`

```nova
priv(file) type Acc { … }      // виден только в этом файле
priv(file) fn helper() -> int  // не виден peer-файлам модуля
priv(file) const K = 42        // file-local константа
// без модификатора            // module-private (как сейчас)
export     fn api() …          // публичный (как сейчас)
```

Лесенка: `priv(file)` ⊂ (module-default) ⊂ `export`.

**Концепция:** `priv(file)` — visibility-hint, НЕ смена module-резолва. Модуль
остаётся один (D29 не нарушается); символ помечен «не виден peer-файлам». Аналог
Rust `pub(self)`. Нейминг `priv(file)` (не `local`): единая ось видимости под
`priv` + scope-квалификатор (как `priv(type)`); `local` двусмысленно (вложенные
функции) и требует нового KW (риск коллизии идентификаторов).

---

## 3. Фазы реализации

### Ф.1 — AST + Parser

**Факты о текущем коде:**
- AST top-level структуры с `is_export: bool`:
  `FnDecl` (`ast/mod.rs:348`), `TypeDecl` (`:938`), `ConstDecl` (`:1342`).
- Парсинг top-level: `is_export = self.eat(&TokenKind::KwExport)` в общем
  parse-item (`parser/mod.rs:1295`), передаётся в `parse_fn`/`parse_type_decl`/
  `parse_const_decl` (`:1495-1547`).
- Токены есть: `KwPriv`, `KwModule`, `KwType`. **`KwFile` НЕТ** → `file` внутри
  `priv(...)` парсить как `Ident("file")`.
- Образец парсинга `priv(...)` (для полей): `parser/mod.rs:3583-3615`
  (`priv` → bare; `priv(type)` → type-private; `priv(module)`/прочее → ошибка
  `E_PRIV_QUALIFIER`).

**Сделать:**
1. AST: добавить в `FnDecl`/`TypeDecl`/`ConstDecl` поле
   `pub file_private: bool` (default `false`; обновить все конструкторы/`Default`).
   *(Минимальный путь — bool. Не делать enum-рефакторинг is_export — лишний риск.)*
2. Parser, общий parse-item (зона `:1295`, ДО `KwExport`-eat): распознать
   `priv` `(` `file` `)`:
   - `priv` без `(` на top-level → пока ошибка `[E_PRIV_QUALIFIER] bare priv on
     top-level item not supported; use priv(file) or omit for module-private`
     (bare top-level priv не вводим в этом плане);
   - `priv(file)` → выставить флаг, пробросить в `parse_fn`/`parse_type_decl`/
     `parse_const_decl` (добавить параметр `file_private: bool`);
   - `priv(<other>)` на top-level → `E_PRIV_QUALIFIER` (как в образце 3608).
   - `priv(file)` и `export` вместе → ошибка (взаимоисключающи).
3. `test`/`bench`/`lemma` — `priv(file)` к ним неприменим (ошибка или игнор;
   проще — синтаксически не допускать перед `test`).

### Ф.2 — Checker (резолв + enforcement)

**Факты:** резолв top-level в `types/mod.rs` через `env.fns` / `env.types`
(заполняются ~`:374-407`). Файл-источник символа — в `decl.span.file_id`.

**Сделать:**
1. При построении `env.fns`/`env.types` сохранять для каждого символа
   `(file_private, file_id)`.
2. При резолве имени с use-site в файле `F`: если найденный символ
   `file_private == true` И `symbol.file_id != F` → символ НЕ виден (трактовать
   как «не найден» для этого файла) → диагностика
   `[E_FILE_PRIV_LEAK] `<name>` is file-private to <other_file>; not visible from
   <this_file>` с подсказкой «remove priv(file) or move the symbol».
3. **Дедупликация:** два `priv(file)` символа с ОДИНАКОВЫМ именем в РАЗНЫХ
   peer-файлах — НЕ конфликт (разные file-scope). Снять для них проверку
   «duplicate top-level name» (которая сейчас в зоне `:363-407`). Module-private
   и export одноимённые между файлами — конфликт как раньше.
4. `priv` (module, D281) и `export` — поведение БЕЗ изменений.

### Ф.3 — Codegen (манглинг)

**Факт (готовый образец!):** в `codegen/emit_c.rs:1010` уже есть
`private_const_c_names: HashMap<(FileId, String), String>` — per-file
резолв `(file_id, source_name) → mangled C name` (Plan 160, mangled
`Nova_const_<modpath>_<NAME>`, заполняется ~`:2001-2054`, читается ~`:4554`).
Это **точный паттерн для priv(file)** — он УЖЕ keyed по `file_id`. Переиспользовать
ту же схему для fn/type: завести аналог `file_priv_c_names: HashMap<(FileId,
String), String>`, заполнять для `file_private` items, резолвить call-site по
`(file_id, name)`.

**Сделать:** `priv(file)` символы мангли́ть с **file-discriminator**, чтобы два
одноимённых `priv(file) fn helper` из разных файлов давали РАЗНЫЕ C-символы (без
коллизии линковки). Дискриминатор — стабильный (file stem или file_id), напр.
`nova_fn_<module>_<filestem>__helper`. Call-site внутри файла резолвит в свой
вариант (через тот же (name, file_id) ключ, что и checker Ф.2).

### Ф.4 — Spec + тесты

1. **Spec:** новый D-блок **D304** в `spec/decisions/02-types.md` (рядом с D281).
   Заголовок: «D304. File-private visibility — `priv(file)` (Plan 170)».
   Содержимое: лесенка `priv(file)` ⊂ module ⊂ export; таблица scope;
   `E_FILE_PRIV_LEAK`; нейминг-обоснование; amend-ссылка на D281/D220.
   Зарегистрировать D304 в таблице решений (верх файла) + README/index если есть.
2. **Тесты** `nova_tests/plan170/` (folder-module `module nova_tests.plan170`,
   negative — в `nova_tests/plan170/neg/` с `module neg.<stem>`):
   - `pos_file_priv_visible_in_own_file.nv` — `priv(file) fn h()` вызывается в
     своём файле → PASS.
   - Пара peer-файлов: `peer_a.nv` + `peer_b.nv`, оба `module nova_tests.plan170`,
     оба объявляют `priv(file) fn helper()` с РАЗНЫМ телом, каждый вызывает свой
     → PASS (сосуществуют, нет конфликта, нет линк-коллизии).
   - `neg/file_priv_leak.nv` — ссылка из peer на чужой `priv(file)` →
     `EXPECT_COMPILE_ERROR E_FILE_PRIV_LEAK`.
   - `pos_module_priv_still_peer_visible.nv` (пара) — `priv`/module-default
     символ из одного peer ВИДЕН другому (регрессия D281) → PASS.
   - `neg/priv_file_on_export.nv` — `export priv(file) fn` → `EXPECT_COMPILE_ERROR`.

---

## 4. Acceptance (критерии готовности)

- [x] **Без упрощений, как для прода** (ОБЯЗАТЕЛЬНЫЙ критерий) — все фазы Ф.1–Ф.4
      реализованы production-grade: parser обрабатывает оба порядка `priv(file)`/
      `export`, все 4 формы E_PRIV_QUALIFIER; checker — реальный per-file leak-table
      + dedup; codegen — file-дискриминация free fn **И** const (не заглушка).
- [x] `priv(file) fn`/`type`/`const` парсится; `priv(file)`+`export` = ошибка
      (оба порядка — `neg/priv_file_on_export_neg` + `neg/priv_file_export_order_neg`).
- [x] file-private символ НЕ виден peer-файлам (`E_FILE_PRIV_LEAK` —
      `neg/file_priv_leak/`), виден в своём (`pos_file_priv_visible_in_own_file`).
- [x] Два одноимённых `priv(file)` в peer-файлах компилируются (codegen
      file-discriminator, нет линк-коллизии) и работают — `peer_a`/`peer_b`
      `compute()` == 200 / 400 доказывает корректную per-file резолвцию.
- [x] module-private (`priv`/default) и `export` — без регрессий (D281/D220):
      `pos_module_priv_peer_a`/`_b` (cross-file module-default visible) + plan160 6/6.
- [x] `nova_tests/plan170` — 13/13 pos-блоков PASS; 5/5 negative ловят нужные ошибки.
- [x] Регресс: plan160 / plan124* / modules / std — 0 новых FAIL (baseline-diffed
      против main HEAD `1b3b11f3`; все остаточные FAIL идентичны baseline).
- [x] **D307** в spec (02-types.md + README index); логи обновлены
      (project-creation, simplifications, nova-private/discussion-log); маркер
      `[M-170-priv-file-visibility]` закрыт. *(Номер: план называл D304 — занят
      Plan 169.1.1; D305/D306 → Plan 104.10; присвоен свободный D307.)*

## 5. Применение после закрытия

Консолидация ([169.1.2](169.1.2-consolidate-by-theme.md)/
[169.1.3](169.1.3-consolidate-partial-d.md)): конфликтующие helper'ы помечать
`priv(file)` вместо ordinal-rename → чище, имена читаемы. Польза шире тестов:
любой folder-module (`std/collections/vec/` и т.п.) получает file-local helper'ы.

## Followup-маркер

`[M-170-priv-file-visibility]`.
