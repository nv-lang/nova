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

## Гейт — ВСЁ ЗЕЛЕНО, задача закрыта
- `nova check std/src/runtime` → PASS 18/0 WARN 121, 0 undefined int_fmt_into.
- neg-фикстура настоящего A↔B цикла: тестов на cycles в репо не было
  вообще (ни .nv, ни Rust unit) — создана
  `spec_tests/conformance/entry_self_cycle/{cyc_a,cyc_b,cycle_test}.nv`.
  `cyc_a`↔`cyc_b` — генуинный двусторонний цикл, НИ ОДИН не entry
  (`cycle_test.nv` — третья сторона/entry). `cyc_b.b_calls_a` ссылается
  на `cyc_a.a_val` на ветке, где cycle-guard (in_progress, НЕ моя правка)
  по-прежнему отдаёт пустой visible_acc → честный `undefined identifier`.
  `nova test --compile-error entry_self_cycle` → PASS (negative) —
  защита цикла НЕ сломана.
- standalone: `std/src/checksums` PASS 3/0, `std/src/collections`
  PASS 13/0.
- Флагман: `nova build examples/flagship/aggregator/src/main.nv
  --strict-effects` — built чисто (только предсущ. unused-import warn).
- Folder-CU conformance (главный риск-гейт): `nova test
  spec_tests/conformance --jobs 4` → PASS 126/0 SKIP 16; `--compile-error`
  лейн → PASS 385/0 (включая новую фикстуру). Оба FAIL:0 — known-red не
  встретилось (лучше ожидавшегося "0/1 known-red").

Коммиты: `160789715` (фикс imports.rs) + `b8385f4cb` (neg-фикстура).
Маркер `[M-imports-entry-folder-module-self-cycle-empty-exports]` закрыт
в backlog-followups.md; `[M-p200-17-remaining-1-fmtbuf]` разблокирован
(split fmt_buf/{core,core_test} не переприменён — вне scope этой волны).
В main НЕ мёржено, push не делан — ждёт интегратора.
