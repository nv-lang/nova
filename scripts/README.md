<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# scripts/ — карта каталога

Этот README — навигация по `scripts/`. Подкаталоги со своей документацией:
[`selftest/README.md`](selftest/README.md) (регресс-тесты самих стражей),
[`claude-hooks/README.md`](claude-hooks/README.md) (хуки Claude Code),
[`githooks/README.md`](githooks/README.md) (общий git pre-commit пяти реп).

Программа, частью которой является большинство файлов ниже — план
[231 «Выход из цикла точечных фиксов»](../docs/plans/231-bug-cycle-exit.md)
(трек Д «машинное принуждение норм», исполнительный дом —
[231.2](../docs/plans/231.2-enforcement-infra.md)) и реестр дефектов
[221.1](../docs/plans/221.1-bug-sweep.md).

## (а) Постоянные механизмы

Это машинные стражи/гейты — их нельзя удалять, они принуждают конвенции,
которые иначе живут только в тексте. **Важная оговорка, проверенная по
коду 2026-07-27**: не все они реально ВЫЗЫВАЮТСЯ откуда-либо автоматически
— см. колонку «Вызов».

| Скрипт | Что делает | Вызов | Самотест |
|---|---|---|---|
| `gate.sh` | Единый локальный гейт: arch-ratchet → check-no-runtime-copy → все `selftest/test-*.sh` → `cargo build --release` → мега-CU `spec_tests/conformance` (ассерт строки `PASS: N FAIL: 0`) → `nova check std/src` (байт-канон `142/27/1040`) → флагман `examples/flagship/aggregator --strict-effects` → уникальность D-номеров в `spec/decisions/*.md` | Вручную, `bash scripts/gate.sh`, перед push. **НЕ вызывается CI**: `.github/workflows/nova-gate.yml` реализует СХОЖУЮ, но не идентичную проверку (conformance + флагман + анти-гниль examples) отдельными inline-шагами, а не через этот файл — довязка в CI отмечена как открытый пункт в 231.2 §2.1 | Оркестратор сам себя не самотестирует; таблица план 231 §4в требует для него «синтетический красный кейс на каждый ассерт» — пока не реализовано |
| `arch-ratchet.sh` | Храповик: строки `compiler-codegen/src/codegen/emit_c.rs` и число вызовов `infer_expr_c_type` не растут относительно `arch-ratchet.baseline`; рост без правки baseline в том же коммите = красный | Из `gate.sh` (шаг «arch-ratchet») | Нет (план 231 §4в: «baseline+1 строка → красный» — не реализовано) |
| `check-no-runtime-copy.sh` | Не даёт копии `compiler-codegen/nova_rt` появиться в ПАКЕТНОЙ репе/worktree (не главной nova) — копия не под git, шадовит настоящий рантайм | Из `gate.sh` (отдельный шаг) | ✅ `selftest/test-check-no-runtime-copy.sh` — единственный существующий самотест |
| `check-no-manual-status-table.sh` | Греплет `docs/plans/README.md` на сигнатуру ручной сводной статус-таблицы (≥3 строк вида `\| N \| [файл](...) \| ... \| эмодзи \|`) — норма из `docs/conventions-governance.md` («статус плана — только пофайлово») | **НЕ вызывается из `gate.sh`** (гейт про код, не про `docs/plans/README.md`) — часть приёмки правок этого README | Нет |
| `gen-plan-status.sh` | Генерирует `docs/plans/STATUS.md` из пофайловых `**Статус:**`-строк всех `docs/plans/NNN-*.md` (натуральная сортировка подномеров, UTF-8-safe обрезка) | **НЕ вызывается из `gate.sh`** — запускать вручную после правки любого плана | Нет |
| `lint-no-silent-int-fallback.sh` | Тот же ratchet-паттерн, что arch-ratchet, но для двух категорий silent `nova_int`-fallback в `compiler-codegen/src` (Plan 70) | **НЕ вызывается ни из `gate.sh`, ни из CI** — запускать вручную при правках, задевающих `type_ref_to_c` | Нет |
| `hardcode-audit.sh` | Tripwire по 7 категориям хардкода имён типов/протоколов (Plan 196 §554) — счётчик, не абсолютный источник истины | **НЕ вызывается ни из `gate.sh`, ни из CI** — вручную на волнах Plan 196 | Нет |
| `strict_effects_smoke.sh` | Прогоняет `nova check` вручную (не через `nova test`, т.к. у D89 EXPECT_*-раннера нет per-файл CLI-флагов) на `spec_tests/strict_effects/pos_*`/`neg_*`, проверяя точный pass/fail-паттерн `--strict-effects` (Plan 197) | **НЕ вызывается ни из `gate.sh`, ни из CI** | Нет |
| `tsan_concurrency.sh` | ThreadSanitizer-гейт для concurrency-тестов (Plan 83.4.5.6 Ф.5), Linux-only (WSL2) | **НЕ вызывается ни из `gate.sh`(Windows-only), ни из CI** | Нет |

Дата-отметка (2026-07-27, при написании этого README): при реальном прогоне
`hardcode-audit.sh` и `lint-no-silent-int-fallback.sh` оказались КРАСНЫМИ
относительно своих текущих baseline (хардкод кат.B/E выросли, silent-fallback
кат.A1 = 21 при baseline 7) — это не следствие правок этой волны (только
шапки-комментарии), а факт, обнаруженный при верификации; актуализация
baseline или разбор дельты — отдельная задача, не эта.

## (б) Разовые инструменты волн (могут устареть, не удалять не глядя)

| Скрипт | Волна |
|---|---|
| `catb_convert.py` | Категория-B конвертация `nova_tests/` на folder-module модель: переименование конфликтующих top-level имён внутри файла, переписывание `module X.stem` → `module nova_tests.X`, перенос `EXPECT_COMPILE_ERROR`-файлов в `neg/`. Номер плана в самом файле не указан. |
| `d78_audit_migrate.py` | Аудит + миграция module-деклараций на форму rev-3 (`module = parent.target`, `internal/`-спецкейс `owner.internal.target`) — правило «D78 rev-3» согласно докстрингу файла. |
| `demojibake.py` | Разовый/переисполняемый чинитель двойного mojibake (UTF-8 прочитан как cp1251) в русскоязычных комментариях/строках компилятора — повод: GitHub issue #1 (см. `docs/project-creation.txt`, `docs/simplifications.md`). |
| `plan114_rewrite.py` | Plan 114: массовый regex-рерайт `.nv`-корпуса, правила R1–R12 (`let`→`ro`/`mut`, `if let`/`while let` → без `let`, `readonly`→`ro`). |
| `plan114_apply_md.py` | Plan 114: применяет `plan114_rewrite.py` к `docs/**/*.md` + `spec/**/*.md`, исключая `history/`. |
| `plan114_rust_nova_body.py` | Plan 114 Ф.2 / D184: переписывает встроенные Nova-строки внутри `nova_body: Some("...")` в Rust-исходниках на новый синтаксис ключевых слов. |
| `plan114_4_stmt_const_arms.py` | Plan 114.4 Ф.2: добавляет соседние `Stmt::Const(_)` match-руки рядом с `Stmt::Let(...)` в `compiler-codegen/src/types/mod.rs` по указанным номерам строк. |
| `cdb_session.sh` | Plan 83.11 §12.30, `[M-83.11-supervised-spawn-cancel-memcpy-segv]`: сессия Windows kernel debugger (`cdb.exe`) для локализации кадра-виновника конкретного стохастического crash. Подмечено: собственный докстринг файла именует себя `tools/cdb_session.sh`, хотя лежит в `scripts/` — расхождение в самом файле (не правилось, см. отчёт волны). |
| `stress_bisect.sh` | Plan 83.11 §12.27: `git bisect run`-совместимый stress-harness, нашедший коммит-виновник того же бага за 3 итерации/10 коммитов; докстринг заявляет инструмент переиспользуемым для ЛЮБОГО стохастического SEGV/hang-теста. Тот же путаный самоссылочный путь `tools/stress_bisect.sh` в докстринге. |
| `setup_worktree_p118.sh` | Plan 118: настройка **`nova-pNN`-worktree ГЛАВНОЙ репы nova** (не пакетной!) — копирует уже инициализированный `libuv`-submodule + prebuilt `libuv.lib` из главной репы (чтобы не гонять ~30с submodule-init на каждый worktree) и печатает нужные `NOVA_GC_*`-env-переменные. Не конфликтует с `check-no-runtime-copy.sh`: там речь о ПАКЕТНЫХ репах (nova-http и т.п.), где `compiler-codegen` — целиком чужеродная копия; здесь `compiler-codegen` — законная часть git-чекаута той же репы nova. |

## Прочее (не входит ни в одну из двух групп выше — упомянуто для полноты карты)

- `migrate_modules_rev3.ps1` — Plan 42 Sub-plan 42.6: PowerShell-миграция module-деклараций rev-1 → rev-3 для workspace-members `std`/`nova_tests` (пути хардкожены — bootstrap-скрипт, не general tool). Более ранняя волна той же темы, что `d78_audit_migrate.py`.
- `package-release.ps1` — Plan 221 Ф.2 (A-V2): собирает Windows-x64 zip-релиз (`nova.exe`+`nova-lsp.exe`+`std/`+урезанные `nova_rt/`+GC-библиотека+`setup-env.ps1`) для использования вне монорепы. Это релизный инструмент длительного действия, не гейт и не разовая миграция — просто вне двух категорий, которые просил разделить владелец.
- `tsan_suppressions.txt` — не скрипт, конфиг-файл подавлений для `tsan_concurrency.sh` (`TSAN_OPTIONS=... suppressions=...`).

## Куда класть новое

- **Постоянный механизм принуждения** (страж/гейт/линт, который должен жить
  долго и не может молча перестать работать) → кладётся сюда (`scripts/`) +
  шаг вызова добавляется в `gate.sh` (если это код-гейт) + **ОБЯЗАТЕЛЬНО**
  собственный самотест в `scripts/selftest/test-<имя>.sh` (см.
  [selftest/README.md](selftest/README.md) — «0 механизмов без самотеста»,
  план 231 §4в).
- **Разовый инструмент волны** (миграция/конвертер, который выполняется
  один раз для конкретного плана и может устареть) → тоже сюда, но с
  докстрингом, явно называющим план/волну — не в раздел (а) выше, и без
  требования самотеста/вызова из `gate.sh`.
