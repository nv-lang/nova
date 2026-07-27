<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# scripts/tools/ — разовые инструменты волн

В отличие от [`scripts/guards/`](../guards/README.md) (постоянные механизмы
принуждения, обязаны иметь самотест и быть подключёнными к `gate.sh`),
файлы здесь выполнили (или выполняют по требованию) миграцию/конвертацию
для КОНКРЕТНОГО плана и не обязаны иметь самотест или быть подключены
куда-либо — их можно запускать вручную, когда план того требует, и они
могут устареть, если корпус, который они мигрировали, уже весь мигрирован.
Не удалять не глядя — часть из них (`gen-plan-status.sh`,
`stress_bisect.sh`, `cdb_session.sh`) активно используется и сейчас.

| Файл | Что мигрировал / для чего | Волна |
|---|---|---|
| [`gen-plan-status.sh`](gen-plan-status.sh) | НЕ миграция — генератор `docs/plans/STATUS.md` из пофайловых `**Статус:**`-строк. Единственный не-разовый файл здесь (используется постоянно, но это генератор, не страж — поэтому не в `guards/`). | план 231 §0а / `docs/conventions-governance.md` |
| [`catb_convert.py`](catb_convert.py) | Конвертация `nova_tests/` на folder-module модель: переименование конфликтующих top-level имён внутри файла, `module X.stem` → `module nova_tests.X`, перенос `EXPECT_COMPILE_ERROR`-файлов в `neg/`. | Cat-B (Plan 169.1 — folder-module для 75 директорий с конфликтующими именами) |
| [`d78_audit_migrate.py`](d78_audit_migrate.py) | Аудит + миграция module-деклараций на форму rev-3 (`module = parent.target`, `internal/`-спецкейс `owner.internal.target`). | «D78 rev-3» (module-declaration rule, см. spec/decisions/07-modules.md) |
| [`migrate_modules_rev3.ps1`](migrate_modules_rev3.ps1) | PowerShell-миграция module-деклараций rev-1 → rev-3 для workspace-members `std`/`nova_tests` (более ранняя волна той же темы, что `d78_audit_migrate.py`; пути хардкожены — bootstrap-скрипт). | Plan 42 Sub-plan 42.6 (ЗАКРЫТ 2026-05-13) |
| [`demojibake.py`](demojibake.py) | Разовый/переисполняемый чинитель двойного mojibake (UTF-8 прочитан как cp1251) в русскоязычных комментариях/строках компилятора. | Повод — GitHub issue #1 (см. `docs/project-creation.txt`, `docs/simplifications.md`) |
| [`plan114_rewrite.py`](plan114_rewrite.py) | Массовый regex-рерайт `.nv`-корпуса, правила R1–R12 (`let`→`ro`/`mut`, `if let`/`while let` → без `let`, `readonly`→`ro`). | Plan 114 (keyword refresh ro/mut/no-let) |
| [`plan114_apply_md.py`](plan114_apply_md.py) | Применяет `plan114_rewrite.py` к `docs/**/*.md` + `spec/**/*.md`, исключая `history/`. | Plan 114 |
| [`plan114_rust_nova_body.py`](plan114_rust_nova_body.py) | Переписывает встроенные Nova-строки внутри `nova_body: Some("...")` в Rust-исходниках на новый синтаксис ключевых слов. | Plan 114 Ф.2 / D184 |
| [`plan114_4_stmt_const_arms.py`](plan114_4_stmt_const_arms.py) | Добавляет соседние `Stmt::Const(_)` match-руки рядом с `Stmt::Let(...)` в `compiler-codegen/src/types/mod.rs` по указанным номерам строк. | Plan 114.4 Ф.2 |
| [`cdb_session.sh`](cdb_session.sh) | Сессия Windows kernel debugger (`cdb.exe`) для локализации кадра-виновника стохастического crash в M:N-рантайме. | Plan 83.11 §12.30, `[M-83.11-supervised-spawn-cancel-memcpy-segv]` |
| [`stress_bisect.sh`](stress_bisect.sh) | `git bisect run`-совместимый stress-harness для стохастических SEGV/hang concurrency-тестов; переиспользуемый (не только для бага, на котором родился). | Plan 83.11 §12.27, ссылается `docs/debugging-races.md` |
| [`setup_worktree_p118.sh`](setup_worktree_p118.sh) | Копирует уже инициализированный `libuv`-submodule + prebuilt `libuv.lib` из главной репы в `nova-pNN`-worktree той же репы (экономит ~30с submodule-init на каждый worktree) + печатает `NOVA_GC_*`-env-переменные. | Plan 118 (typed pointers / unsafe) |

## Замечено при переносе (2026-07-27), не исправлялось (логику не менять)

- `cdb_session.sh` и `stress_bisect.sh` в собственных докстрингах называют
  себя `tools/cdb_session.sh` / `tools/stress_bisect.sh` — уже расходилось
  с фактическим путём ДО этой волны (файлы всегда лежали в `scripts/`, не
  в `tools/` на верхнем уровне репы); теперь, после переноса, путь
  `scripts/tools/...` почти совпал с докстрингом случайно. Не правилось —
  не в мандате этой волны (менять логику/докстринги скриптов запрещено
  правилами задания, кроме шапок стражей).
- `docs/plans/114.4-const-narrow-generalize-fn.md:295` ссылается на
  `scripts/plan114_4_const_rewrite.py` — файла с таким именем НЕТ ни здесь,
  ни раньше не было в репе (ближайший — `plan114_4_stmt_const_arms.py`).
  Похоже на разошедшееся упоминание из старой редакции плана, не связанное
  с этим переносом — оставлено как есть, не в мандате этой волны.

## Пути после переезда (2026-07-27)

Все файлы раньше лежали в `scripts/`. Скрипты, вычислявшие свой корень репы
от собственного расположения, обновлены на `.../../..` вместо `.../..`
(`gen-plan-status.sh`, `setup_worktree_p118.sh`, `migrate_modules_rev3.ps1`,
`catb_convert.py`). `plan114_apply_md.py` жёстко звал соседний
`plan114_rewrite.py` по относительному пути `scripts/plan114_rewrite.py` —
обновлено на `scripts/tools/plan114_rewrite.py`. Остальные файлы не
вычисляют собственное расположение (принимают пути аргументами или
рассчитывают на CWD = корень репы при запуске) — переезд их не затронул
функционально, только docstring-примеры использования.
