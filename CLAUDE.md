# Nova — вход для агентов

**Онбординг (читай в этом порядке):**
1. [docs/promts/read-project.md](docs/promts/read-project.md) — что за проект, текущее состояние, куда двигаться, команды.
2. [docs/dev-workflow.md](docs/dev-workflow.md) — процесс и жёсткие операционные правила.
3. [docs/plans/README.md](docs/plans/README.md) — статусы планов (единственный source of truth статусов).
4. [AGENTS.md](AGENTS.md) — build/test-справка (EN).

**Жёсткие правила (полные — в dev-workflow.md):**
- Интерпретатора НЕТ: только `nova build`/`check`/`test` (C-codegen). Release-сборка обязательна.
- Главный гейт: `spec_tests/conformance` ОДНИМ compile-unit'ом; красный = стоп.
- Тест авторитетен: чинится компилятор в правильном месте; тесты не ослабляются/не удаляются.
- Язык-меняющее слияние не пушится без D-амендмента в спеке в том же слиянии.
- `git add` только по именам файлов; греп конфликт-маркеров ОДНОЙ командой с коммитом; без `git stash`; без AI-co-author-trailer'ов.
- Синтаксис Nova не выдумывать — `spec/decisions/` + `examples/`.
- `std/**` и `examples/**` обязаны собираться с `--strict-effects` (конвенция 2026-07-13).
