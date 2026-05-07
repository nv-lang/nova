# Решения по дизайну Nova

Журнал принятых решений по дизайну языка с обоснованиями.

> **Структура.** Раньше всё лежало в одном файле `decisions.md` (~6000
> строк, 48 D-решений по хронологии). Теперь разбито по темам — каждый
> файл описывает одну область, решения внутри расположены логически
> (от общего к частному), а не хронологически.
>
> История эволюции (что менялось, что отменялось) — в `history/`.

## Тематические разделы

| # | Файл | Что внутри | D-решения |
|---|---|---|---|
| 01 | [01-philosophy.md](01-philosophy.md) | Цели, парадигма, AI-first | D1, D9, D10 |
| 02 | [02-types.md](02-types.md) | Record, sum-type, protocol, generic, поля, bounds | D15, D17, D32, D36, D39, D42, D52, D53, D55, D66, D72 |
| 03 | [03-syntax.md](03-syntax.md) | Объявления, литералы, операторы, методы, парсинг | D16, D19, D20, D22, D23, D27, D30, D33, D34, D35, D37, D38, D40, D43, D44, D45, D46, D48, D49, D54, D58, D59, D60, D69 |
| 04 | [04-effects.md](04-effects.md) | Fail, Io, Db, handlers, with-блоки, interrupt, forbid, realtime, ? | D2, D3, D4, D11, D12, D18, D25, D28, D31, D61, D62, D63, D64, D65, D67, D68 |
| 05 | [05-memory.md](05-memory.md) | Managed GC, escape analysis, regions | D6, D21 (cancelled) |
| 06 | [06-concurrency.md](06-concurrency.md) | Fiber runtime, structured concurrency, spawn, detach, cancel_scope, channels | D14, D50, D71, D75, D79 |
| 07 | [07-modules.md](07-modules.md) | Модули, импорты, видимость, package tooling | D5, D29, D47, D78 |
| 08 | [08-runtime.md](08-runtime.md) | Panic, capability, deployment, prelude, From/Into, TryFrom, math, Mem | D7, D13, D26, D41, D70 (replaced → D73), D73, D74, D76, D77 |
| 09 | [09-tooling.md](09-tooling.md) | Тесты, контракты, форматирование | D24 |

## История

- [history/rejected.md](history/rejected.md) — все отвергнутые альтернативы с причинами.
- [history/evolution.md](history/evolution.md) — как менялись решения по ходу разработки.

## Шаблон D-решения

Каждое D-решение в новых файлах следует единому формату:

```markdown
## DXX. Название

### Что
Одно предложение — суть решения.

### Правило
Подробные правила и примеры current syntax.

### Почему
Обоснование с прецедентами / trade-offs.

### Что отвергнуто
Краткий список альтернатив с причинами отказа.

### Связь
- DXX (зависит / уточняется)
- DYY (родственное)

### Эволюция (если применимо)
Краткая хронология изменений с указанием прежних формулировок.
```

Если решение **отменено** — в начале блока пометка `> ⚠️ ОТМЕНЕНО, см. DZZ`.

## Принципы записи

1. **Только current state** в основном тексте. Ссылки на устаревшие
   формулировки — только через раздел «Эволюция».
2. **Все примеры синтаксически валидны** по текущим правилам. Никаких
   `trait`/`impl`, lowercase эффектов, `:` в типах.
3. **Перекрёстные ссылки внутри `spec/decisions/`** — относительные
   пути (`02-types.md#d17`).
4. **Внешние ссылки** на `syntax.md`, `effects.md` etc. — относительные
   `../syntax.md` (они теперь рядом, в `spec/`).

## Миграция

Старый `decisions.md` в корне репозитория **удалён** после переноса
всех решений в эту директорию (`spec/decisions/`). Все cross-references
в живых документах (`spec/*.md`, `examples/`, `docs/articles/`,
`docs/plans/`, `docs/research/`, `README.md`, `CONTRIBUTING.md`,
`editors/vscode/README.md`) обновлены на новые пути.

См. процесс миграции — [history/evolution.md](history/evolution.md).
