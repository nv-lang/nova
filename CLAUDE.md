# Nova — вход для агентов

**Онбординг (читай в этом порядке):**
1. [docs/dev/promts/read-project.md](docs/dev/promts/read-project.md) — что за проект, текущее состояние, куда двигаться, команды.
2. [docs/dev/dev-workflow.md](docs/dev/dev-workflow.md) — процесс и жёсткие операционные правила.
3. [docs/plans/README.md](docs/plans/README.md) — навигация/приоритеты/очередь планов. **Статус плана — пофайлово** (строка `**Статус:**` в `docs/plans/NNN-*.md`, единственный source of truth); сводный обзор — сгенерированный [docs/plans/STATUS.md](docs/plans/STATUS.md) (`bash scripts/tools/gen-plan-status.sh`). Рукописная индекс-таблица статусов запрещена (conventions-governance).
4. [AGENTS.md](AGENTS.md) — build/test-справка (EN).

**Жёсткие правила (полные — в dev-workflow.md):**
- Интерпретатора НЕТ: только `nova build`/`check`/`test` (C-codegen). Release-сборка обязательна.
- Главный гейт: `spec_tests/conformance` ОДНИМ compile-unit'ом; красный = стоп. Для behavior-changing слияний авторитетный гейт ОБЯЗАН вдобавок собрать флагман-examples (`examples/flagship/aggregator` + затронутые) под `--strict-effects` — conformance app-регрессии не ловит (test-conventions.md, прецедент 206/splitmix64).
- Тест авторитетен: чинится компилятор в правильном месте; тесты не ослабляются/не удаляются.
- Язык-меняющее слияние не пушится без D-амендмента в спеке в том же слиянии.
- `git add` только по именам файлов; греп конфликт-маркеров ОДНОЙ командой с коммитом; без `git stash`; без AI-co-author-trailer'ов.
- Синтаксис Nova не выдумывать — `spec/decisions/` + `examples/`.
- `std/**` и `examples/**` обязаны собираться с `--strict-effects` (конвенция 2026-07-13).
- Код **Vela** (M:N-рантайм, `nova_rt/**` concurrency: spawn/cancel/scope/driver/GC) — по нормам [docs/dev/mn-coding-conventions.md](docs/dev/mn-coding-conventions.md) (проактив: как писать без гонок; реактив — [docs/dev/debugging-races.md](docs/dev/debugging-races.md)). Имя: [naming-conventions.md](docs/dev/naming-conventions.md) §1.2, план [224](docs/plans/224-vela-runtime-naming.md).
