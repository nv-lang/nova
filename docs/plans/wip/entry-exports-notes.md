# [M-imports-entry-folder-module-self-cycle-empty-exports] — заметки

Worktree: nova-entryfix, ветка p-fix-entry-exports. Модель: sonnet.

## Шаг 1 — репро подтверждено (ДО фикса не гонялось отдельно — диагноз
из маркера уже был repro-verified другим агентом; после фикса — зелено,
см. ниже "После").

## Шаг 2 — фикс применён (compiler-codegen/src/imports.rs)
Выбран вариант **(б)** (cycle-guard отдаёт exports entry), реализован как:
1. Seed `visited[entry_key] = entry_export_names` СРАЗУ после сбора
   siblings (до import_work-цикла) — экспорты entry вычисляются из уже
   распарсенных `module.items` + `siblings[].module.items`, тем же
   правилом видимости (`module_has_exports`/`is_export`), что и
   `resolve_one`'s peer-loop (новая fn `exported_names_from_items`).
2. В `resolve_one` порядок двух guard'ов ПЕРЕСТАВЛЕН: `visited`-check
   теперь ПЕРЕД `in_progress`-check. Для ЛЮБОГО НЕ-entry модуля это
   no-op (инвариант: `in_progress`/`visited` взаимно исключающие — pop+
   insert атомарны в конце `resolve_one`), для entry — даёт спец-путь.
3. Финальный `visited.insert(entry_key, vec![])` (затирал кэш пустым
   вектором) — УДАЛЁН; ложный комментарий-инвариант "entry's exports
   not cached... never dedup'd" — переписан.

Почему не (а) (просто снять entry_key из in_progress перед
pending_peer_preludes-drain): чинит только ОДИН путь манифестации (через
drain), не общий класс (тот же цикл теоретически может замкнуться и
внутри основного top-level import_work-цикла, не только в drain). (б)
закрывает архитектурно — работает вне зависимости от ТОГО, на какой фазе
resolve случился обратный импорт на entry.

## Шаг 1 (после фикса) — репро
`nova test std/src/runtime/fmt_buf.nv` → PASS 1/0, 8/8 внутренних тестов
(int_fmt/bool_fmt/char_fmt/f64_fmt_into/fmt_f64) — undefined-каскад в
string_builder.nv ушёл.

## Осталось по гейту
- nova check std/src/runtime
- neg-фикстура настоящего A↔B цикла (создать/найти — тестов на cycles
  в репо НЕ найдено вообще, ни .nv, ни Rust unit)
- standalone CU: std/src/checksums, std/src/collections
- флагман src --strict-effects
- folder-CU conformance (главный риск-гейт)
