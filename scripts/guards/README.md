<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# scripts/guards/ — постоянные механизмы принуждения

Машинные стражи/гейты/ratchet'ы — нельзя удалять, они принуждают
конвенции, которые иначе живут только в тексте. **Единственный источник
правды о состоянии каждого механизма** (документирован / подключён /
покрыт самотестом) — реестр [231 §0а](../../docs/plans/231-bug-cycle-exit.md)
(план [231 «Выход из цикла точечных фиксов»](../../docs/plans/231-bug-cycle-exit.md),
исполнительный дом [231.2](../../docs/plans/231.2-enforcement-infra.md)).
Таблица ниже — карта файлов этой папки, а не дубликат реестра; за точным
текущим статусом смотри план, не сюда.

| Файл | Что делает |
|---|---|
| [`check-guard-wiring.sh`](check-guard-wiring.sh) | **Мета-страж**: проверяет, что каждый `check-*.sh` в этой папке документирован (содержательная шапка + ссылка на план), подключён (вызывается `gate.sh` напрямую или через цикл самотестов) и покрыт самотестом. Страж, который проверяет остальных стражей — правило владельца «в скрипте нет толку, если не подключён к автопроверке». |
| [`arch-ratchet.sh`](arch-ratchet.sh) + [`arch-ratchet.baseline`](arch-ratchet.baseline) | Храповик: строки `compiler-codegen/src/codegen/emit_c.rs` и число вызовов `infer_expr_c_type` не растут относительно baseline; рост без письменного обоснования В ТОМ ЖЕ коммите — красный. Baseline-файл co-located, переехал вместе со скриптом. |
| [`check-no-runtime-copy.sh`](check-no-runtime-copy.sh) | Не даёт копии `compiler-codegen/nova_rt` появиться в пакетной репе/worktree — копия не под git, шадовит настоящий рантайм (реестр 221.1 №138). |
| [`check-no-manual-status-table.sh`](check-no-manual-status-table.sh) | Греплет `docs/plans/README.md` на сигнатуру ручной сводной статус-таблицы (норма — `docs/conventions-governance.md`). |
| [`install-guards.sh`](install-guards.sh) | Установщик всех механизмов: `core.hooksPath` во всех репах семьи, права на исполнение, проверка объявления хуков Claude Code, финальный прогон мета-стража и всех самотестов. `--check` — только проверить, ничего не менять. |
| [`lint-no-silent-int-fallback.sh`](lint-no-silent-int-fallback.sh) | Ratchet против тихого `nova_int`-fallback в кодогене (Plan 70). |
| [`hardcode-audit.sh`](hardcode-audit.sh) | Tripwire по 7 категориям хардкода имён типов/протоколов вместо .nv-источника (Plan 196 §554). |
| [`strict_effects_smoke.sh`](strict_effects_smoke.sh) | Прогоняет `nova check` на `spec_tests/strict_effects/pos_*`/`neg_*`, проверяя точный pass/fail-паттерн `--strict-effects` (Plan 197). |
| [`tsan_concurrency.sh`](tsan_concurrency.sh) + [`tsan_suppressions.txt`](tsan_suppressions.txt) | ThreadSanitizer-гейт для concurrency-тестов (Plan 83.4.5.6 Ф.5), Linux-only. Suppressions-файл co-located, переехал вместе со скриптом. |
| [`selftest/`](selftest/README.md) | Регресс-тесты самих стражей — см. отдельный README. |

## Кто реально подключён (проверено по коду 2026-07-27)

Не путай «есть в этой папке» с «работает автоматически» — таблица
[231 §0а](../../docs/plans/231-bug-cycle-exit.md) размечает это колонкой
«Подкл», но вот что это значит конкретно на уровне вызовов:

- **Вызываются `scripts/gate.sh`:** `arch-ratchet.sh`, `check-no-runtime-copy.sh`,
  весь цикл `selftest/test-*.sh` (значит — транзитивно и
  `check-guard-wiring.sh`/`check-no-manual-status-table.sh`/
  `check-no-runtime-copy.sh`, у которых есть самотесты).
- **НЕ вызываются НИ `gate.sh`, НИ CI** (`.github/workflows/nova-gate.yml`
  реализует свою, отдельную версию похожего гейта inline, а не зовёт этот
  `gate.sh`): `lint-no-silent-int-fallback.sh`, `hardcode-audit.sh`,
  `strict_effects_smoke.sh`, `tsan_concurrency.sh` (плюс `tsan_concurrency.sh`
  всё равно Linux-only — `gate.sh` пишется под Windows). Запускать их
  нужно вручную на соответствующих волнах (тексты в шапках — когда
  именно). Это НЕ баг реорганизации — так было и до неё.
- **`check-guard-wiring.sh` смотрит только на `check-*.sh`** (соглашение
  имён стражей) — `arch-ratchet.sh`, `install-guards.sh`,
  `lint-no-silent-int-fallback.sh`, `hardcode-audit.sh`,
  `strict_effects_smoke.sh`, `tsan_concurrency.sh` под именование не
  подпадают и мета-стражом не проверяются.
- **Дата-отметка 2026-07-27:** реальный прогон `hardcode-audit.sh` и
  `lint-no-silent-int-fallback.sh` против текущего кода показал КРАСНЫЙ
  результат (хардкод кат.B/E выросли сверх baseline; silent-fallback
  кат.A1 = 21 при baseline 7) — обнаружено при верификации этой волны,
  не вызвано ей (правки этой волны — только пути и комментарии). Разбор
  дельты — отдельная задача.

## Пути после переезда (2026-07-27)

Все файлы этой папки раньше лежали в `scripts/` (на уровень выше). Каждый
скрипт, вычисляющий свой `REPO_ROOT` от собственного расположения, обновлён
на `.../../..` вместо `.../..` (был на один уровень мельче). `arch-ratchet.sh`
и `tsan_concurrency.sh` путей не меняли — их co-located файлы
(`arch-ratchet.baseline`, `tsan_suppressions.txt`) переехали вместе с ними,
а `EMIT`/`NOVA_BIN` в них считаются от CWD запуска (репо-корень), не от
расположения самого скрипта.
