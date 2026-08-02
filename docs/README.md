<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# docs/ — карта

`docs/` разделён по аудитории. Три раздела, три разных контракта:

| Раздел | Аудитория | Публикация на сайт |
|---|---|---|
| [`docs/guide/`](guide/) | Пользователи Nova — гайды по языку/CLI/stdlib | **публикуется на nv-lang.org** (синк тянет ТОЛЬКО отсюда) |
| [`docs/dev/`](dev/) | Контрибьюторы — конвенции, процесс, промпты для агентов | **никогда** не попадает на сайт |
| [`docs/plans/`](plans/) | Контрибьюторы — план-ориентированный процесс разработки | не публикуется; статус — по [`docs/plans/README.md`](plans/README.md) |

## `docs/guide/` — пользовательские гайды

Как пользоваться языком, CLI, stdlib: quickstart, language tour, CLI-справочник,
контракты, каналы, FFI, работа со строками/временем/IO и т.д. Часть файлов
синкается на сайт скриптом `www/site/scripts/sync-decisions.mjs`
(whitelist `DOC_SLUGS` там же + `src/data/docs.ts`) — сейчас это `channels`,
`contracts`, `nova-cli` (EN+RU). Остальные файлы `docs/guide/` на сайт пока
не тянутся, но по смыслу и адресату — пользовательские; кандидаты на
расширение whitelist решает агент сайта/владелец.

**Правило:** в `docs/guide/` кладём только то, что имеет смысл показать
пользователю Nova, который не читает исходники компилятора. Внутренние
design-rationale, «для контрибьюторов»/«для авторов stdlib»-гайды,
нормативные конвенции — в `docs/dev/`, даже если тема смежная (пример:
`docs/guide/strings.md` — публичный API строк, `docs/dev/strings-internals.md` —
как это устроено внутри, для контрибьюторов).

## `docs/dev/` — внутреннее

Конвенции разработки (нормативные — под управлением
[`docs/dev/conventions-governance.md`](dev/conventions-governance.md);
ведение самой документации — язык, нормативность, переводы/дрейф, публикация —
[`docs/dev/doc-conventions.md`](dev/doc-conventions.md)),
процесс (`dev-workflow.md`, `test-conventions.md`, `mn-coding-conventions.md`,
`naming-conventions.md`, ...), промпты для агентов —
[`docs/dev/promts/`](dev/promts/) (онбординг: `read-project.md`,
`read-toolchain.md`, `site-agent.md` и др.).

Внутренние подпапки:

- [`docs/dev/cases/`](dev/cases/) — разборы конкретных багов (case studies).
- [`docs/dev/idioms/`](dev/idioms/) — идиомы/паттерны и Q-блоки (design-Q&A);
  часть файлов — кандидаты на promotion в `docs/guide/`.
- [`docs/dev/migration/`](dev/migration/) — миграционные заметки волн.
- [`docs/dev/research/`](dev/research/) — исследования, сравнения, findings.

**Правило:** `docs/dev/` на сайт nv-lang.org не попадает НИКОГДА — это не
whitelist-фильтр, это структурное разделение (сайт синкает только
`docs/guide/`, см. выше).

## `docs/plans/` — планы

Пофайловый статус (строка `**Статус:**` в `docs/plans/NNN-*.md`) — источник
истины; сводка — сгенерированный [`docs/plans/STATUS.md`](plans/STATUS.md).
Навигация/очередь — [`docs/plans/README.md`](plans/README.md). Структура и
имена файлов этого раздела не меняются разделением `guide/`/`dev/`.

## Летопись (не переносится)

`docs/history/` (`simplifications-closed.md`) и `docs/project-creation.txt` —
исторические логи; не публикуются и не правятся в этой реструктуризации.
