# Планы Nova

В этой директории — только **планы** (что и когда делаем). Справочные
материалы (таблицы сравнений, research-заметки, бенчмарки) живут в
[docs/research/](../research/). Рабочие файлы живых волн (чекпоинты/progress/notes/карты/
verification) — в [wip/](wip/); при закрытии волны — удаляются (история в git), см.
[dev-workflow.md §4](../dev-workflow.md#4-как-организована-работа-план-ориентированная-разработка).

> **Открытые followup'ы** (`[M-…]`-маркеры): живой project-wide OPEN-view —
> [backlog-followups.md](backlog-followups.md) (только актуальное). Plan-bound детали — в
> Followups своего плана. [../simplifications.md](../simplifications.md) — **ТОЛЬКО
> действующие упрощения** (не история!); закрытые упрощения уезжают в
> [../history/simplifications-closed.md](../history/simplifications-closed.md).
> Конвенция: [AGENTS.md](../../AGENTS.md).

## Статусы планов

Источник правды статуса — **только** строка `**Статус:**` в самом файле плана
`NNN-*.md`. Сводный обзор — автогенерируемый [STATUS.md](STATUS.md)
(`bash scripts/gen-plan-status.sh`; git-копия протухает между перегенерациями —
дата генерации в его шапке).

**В этом README статусов, снапшотов и «текущих приоритетов» НЕТ намеренно**
(conventions-governance: рукописные сводки состояний запрещены — они протухают
и создают второй источник правды; прецедент вычистки — 2026-07-21, до этого
здесь лежал мёртвый «снапшот 2026-07-13»). Текущий приоритет и очередь — в
активных планах с их статус-строками (сейчас это план релиза
[221](221-release-v0-1.md) и зонтик [221.1](221.1-bug-sweep.md)).

## Схема нумерации

- `NN-…` / `NNN-…` — планы по порядку создания; `NNN.N-…` — подпланы.

## std-library (навигация модуль → план)

Сквозная конвенция над всеми — [177](177-fallible-result-everywhere.md)
(Result-everywhere, D325).

| Модуль | План |
|---|---|
| parse (str→примитив) | [174.1](174.1-primitive-parse-api.md) |
| time | [175](175-time-system-rework.md) + [175.1](175.1-civil-time.md) (civil) |
| io / fs / os | [176](176-io-fs-os.md) (umbrella) |
| nova lint | [185](185-nova-lint.md) |
| http | [178](178-std-http.md) (umbrella) + [222](222-http-framework.md) (зонтик: nova-http как веб-фреймворк — Router/extractors/middleware/run-loop hardening; волна A в работе) + [222.0](222.0-module-map.md) (карта модулей) + под-планы [222.3](222.3-extractors.md)/[222.4](222.4-middleware.md)/[222.5](222.5-respond.md)/[222.11](222.11-multipart.md)/[222.12](222.12-http-batteries.md)/[222.13](222.13-auth.md)/[222.14](222.14-websocket.md) |
| encoding/compress | [179](179-std-encoding-compress.md) |
| serde / typed-json | [180](180-serde-derive.md) + [222.2](222-http-framework.md) (field-атрибуты до Rust-паритета) |
| формат/Display | [208](208-unified-formatter.md) |
| коэрсии `#coerce` | [214](214-coerce-attribute.md) + [214.1](214.1-generic-coerce.md) (generic-образцы, снятие R14) |

## Связанные директории

- [docs/research/](../research/) — справочные материалы и сравнения.
