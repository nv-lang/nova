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
| 03 | [03-syntax.md](03-syntax.md) | Объявления, литералы, операторы, методы, парсинг, defer/errdefer, атрибуты `#name`, default generics, select | D16, D19, D20, D22, D23, D27, D30, D33, D34, D35, D37, D38, D40, D43, D44, D45, D46, D48, D49, D54, D58, D59, D60, D69, D82, D83, D88, D90, D94, D96 |
| 04 | [04-effects.md](04-effects.md) | Fail, Io, Db, handlers, with-блоки, interrupt, forbid, realtime, ?, `Handler[E, IRT]` | D2, D3, D4, D11, D12, D18, D25, D28, D31, D61, D62, D63, D64, D65, D67, D68, D85, D86, D87 |
| 05 | [05-memory.md](05-memory.md) | Managed GC, escape analysis, regions | D6, D21 (cancelled) |
| 06 | [06-concurrency.md](06-concurrency.md) | Fiber runtime, structured concurrency, spawn, detach, cancel_scope, channels (Channel revision capability-split), select, handler scoping, park/wake API, implicit main-scope | D14, D50, D71, D75, D79, D80, D91, D92, D93 |
| 07 | [07-modules.md](07-modules.md) | Модули, импорты (включая селективный `import X.{A, B}` и `export import` re-export), видимость, package tooling | D5, D29, D47, D78 |
| 08 | [08-runtime.md](08-runtime.md) | Panic, capability, deployment, prelude, From/Into, TryFrom, math, Mem, assert | D7, D13, D26, D41, D70 (replaced → D73), D73, D74, D76, D77, D81 |
| 09 | [09-tooling.md](09-tooling.md) | Тесты, контракты, форматирование, CLI, EXPECT-маркеры | D24, D89, D95 |
| 10 | [10-overloading.md](10-overloading.md) | Перегрузка функций и методов: четыре оси, резолв | D84 |

### Свежие D-решения (по нумерации)

| D# | Файл | Что |
|---|---|---|
| D85 | 04-effects.md | Операторы `?` и `!!` — унифицированное поведение для `Result` и `Option` |
| D86 | 04-effects.md | `??` coalesce-оператор — fallback для `Result`/`Option` без `Fail` |
| D87 | 04-effects.md | `Handler[E, IRT]` — параметризация handler типом interrupt'а |
| D88 | 03-syntax.md | Default-значения generic-параметров (`Handler[E]` ≡ `Handler[E, Never]`) |
| D89 | 09-tooling.md | Test-tooling конвенции — `EXPECT_*` маркеры для negative-тестов |
| D90 | 03-syntax.md | `defer` и `errdefer` — scope-level cleanup statements |
| D91 | 06-concurrency.md | Channel revision — capability-split на `ChanWriter` / `ChanReader` |
| D92 | 06-concurrency.md | Top-level `main` как implicit supervised scope |
| D93 | 06-concurrency.md | Park/wake — нормативный runtime primitive для блокирующих операций |
| D94 | 03-syntax.md | `select { ... }` — multiplexed channel operations |
| D95 | 09-tooling.md | CLI path конвенции — `nova check <path>` / `nova test <path>` |
| D96 | 03-syntax.md | Синтаксис атрибутов — `#name` без квадратных скобок (`#realtime`, `#pure`) |

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
