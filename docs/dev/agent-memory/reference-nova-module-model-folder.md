---
name: reference-nova-module-model-folder
description: Nova module model — folder = ONE module from co-equal files; file+folder same-name forbidden; str method resolution is .nv-driven not registry
metadata: 
  node_type: memory
  type: reference
  originSessionId: 11d1f9f8-5c3b-4afc-a301-fe6093d13686
  modified: 2026-08-01T11:15:49.159Z
---

Модель модулей Nova (подтверждено автором + эмпирически, Plan 152.0 Ф.0.0, 2026-06-13):

- **Папка = ОДИН модуль из равноправных (co-equal) файлов:** много `.nv` в папке `X/`,
  все объявляют **один** `module ...X` → сливаются в один модуль. Прецедент в std:
  `std/runtime/sync.nv` + `sync_test.nv` — оба `module runtime.sync`. Подтверждено для
  папки: `string/core.nv`+`search.nv`, оба `module runtime.string`, методы из обоих
  резолвятся (`nova test` PASS). Module-private (не-`export`) хелперы видны всем co-equal
  файлам модуля, но не снаружи → так делается «internal» без keyword.
- **Файл `X.nv` и папка `X/` с одним именем — ЗАПРЕЩЕНО:** резолвер даёт
  `ambiguous module: both single-file and folder-module exist`. → facade-файл рядом с
  папкой невозможен; вместо facade — co-equal файлы. `_module.nv` — только носитель
  prelude-атрибутов (Plan 107/D174), контент не несёт.
- **Резолв методов типа (str и т.п.) — из распарсенного `.nv`, НЕ из реестра компилятора.**
  `std/runtime/string.nv` — hand-maintained форк, НЕ регенерируется (`emit-runtime-stubs
  --check` падает; метод `get`, 0 записей в `runtime_registry.rs`, резолвится без импорта).
  Реестровые str-Nova-body записи (`find`/`len`/…) вестигиальны: `types/mod.rs:12233`
  читает их только ради `is_consume`/`is_mut` (у str-методов нет). Компилятор реально
  хардкодит про str лишь: операторы `==`/`+`/`<`/… → C `nova_str_eq`/`concat`/`lt`
  (`emit_c.rs:17302`) + `@hash` (DoS-seed). Интенция автора: всё в `.nv`.
- **Доступность str без импорта:** prelude грузит модуль `runtime.string`
  (`export import std.runtime.string.{…}` парсит файл), type-directed resolution находит
  ВСЕ методы типа. Folder-сплит модуль не меняет → доступность не меняется.

- **Канон conformance-фикстур: `module spec_tests.conformance` — у ВСЕХ co-equal файлов
  папки одинаково.** Урок 2026-08-01: новый файл с выдуманным подмодулем
  (`module conformance.plan200_...`) сломал D78-резолв корня ВСЕМУ слитому CU — каскад
  1079×E_D78 на невиновных фикстурах; standalone-прогон при этом «чинился» неверной
  формой. При E_D78 на новом файле — образец в ЛЮБОМ соседнем файле, не изобретать.

Связано: [[project-worktree-nova-test-setup]] (env для nova test в worktree),
[[project-conformance-single-cu-run]].
