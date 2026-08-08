# Nova — вход для агентов

**Онбординг (читай в этом порядке):**
1. [docs/dev/promts/read-project.md](docs/dev/promts/read-project.md) — что за проект, текущее состояние, куда двигаться, команды.
2. [docs/dev/dev-workflow.md](docs/dev/dev-workflow.md) — процесс и жёсткие операционные правила.
3. [docs/plans/README.md](docs/plans/README.md) — навигация/приоритеты/очередь планов. **Статус плана — пофайлово** (строка `**Статус:**` в `docs/plans/NNN-*.md`, единственный source of truth); сводный обзор — сгенерированный [docs/plans/STATUS.md](docs/plans/STATUS.md) (`bash scripts/tools/gen-plan-status.sh`). Рукописная индекс-таблица статусов запрещена (conventions-governance).
4. [AGENTS.md](AGENTS.md) — build/test-справка (EN).

**Структура `docs/`** (карта — [docs/README.md](docs/README.md)): `docs/guide/` — пользовательские
гайды, **публикуются на nv-lang.org** (синк тянет ТОЛЬКО отсюда); `docs/dev/` — конвенции/процесс/
промпты для агентов, на сайт **не попадает никогда**; `docs/plans/` — планы (не трогать структуру).
Новый файл в `docs/` — сразу решай, гайд это или внутреннее, и клади в правильный подкаталог.

**Жёсткие правила (полные — в dev-workflow.md):**
- Интерпретатора НЕТ: только `nova build`/`check`/`test` (C-codegen). Release-сборка обязательна.
- Главный гейт: `spec_tests/conformance` ОДНИМ compile-unit'ом; красный = стоп. Для behavior-changing слияний авторитетный гейт ОБЯЗАН вдобавок собрать флагман-examples (`examples/flagship/aggregator` + затронутые) под `--strict-effects` — conformance app-регрессии не ловит (test-conventions.md, прецедент 206/splitmix64).
- Тест авторитетен: чинится компилятор в правильном месте; тесты не ослабляются/не удаляются.
- **Спека пишется ДО реализации.** Решение (D-блок в `spec/decisions/` плюс обзорный
  `spec/<тема>.md`) записывается КАК ТОЛЬКО владелец выбрал форму — не дожидаясь кода.
  Окно реализует ПО решению, а не приносит его задним числом. Требование «в том же
  слиянии» — это КРАЙНИЙ СРОК (позже нельзя), а НЕ «одновременно»: писать раньше не
  просто можно, а нужно. Язык-меняющее слияние без D-амендмента не пушится.
  Подробно — [dev-workflow.md](docs/dev/dev-workflow.md), раздел «Спека пишется ДО
  реализации».
- `git add` только по именам файлов; греп конфликт-маркеров ОДНОЙ командой с коммитом; без `git stash`; без AI-co-author-trailer'ов.
- Синтаксис Nova не выдумывать — `spec/decisions/` + `examples/`.
- `std/**` и `examples/**` обязаны собираться с `--strict-effects` (конвенция 2026-07-13).
- Код **Vela** (M:N-рантайм, `nova_rt/**` concurrency: spawn/cancel/scope/driver/GC) — по нормам [docs/dev/mn-coding-conventions.md](docs/dev/mn-coding-conventions.md) (проактив: как писать без гонок; реактив — [docs/dev/debugging-races.md](docs/dev/debugging-races.md)). Имя: [naming-conventions.md](docs/dev/naming-conventions.md) §1.2, план [224](docs/plans/224-vela-runtime-naming.md).
