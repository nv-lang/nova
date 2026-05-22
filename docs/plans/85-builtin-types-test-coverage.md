# Plan 85 — Полное тестовое покрытие встроенных типов и протоколов

> **Статус:** ✅ ЗАКРЫТ 2026-05-22 (85.1–85.5)
> **Приоритет:** P1
> **Оценка:** ~2 недели (5 sub-plans параллельно или последовательно)
> **Зависимости:** Plan 75 (str coverage — паттерны тестирования) ✅
>
> **Итог:** все 5 sub-plans закрыты. 47 новых тест-файлов в
> `nova_tests/{str_builder,buffers,protocols/{conversion,comparison,iter}}/`.
> Найдено и **исправлено 4 codegen-бага**:
> 1. D73 `.into()` auto-derive игнорировал overload-резолюцию `from`.
> 2. `char.hash()` mis-dispatch на user-метод → segfault.
> 3. `==`/`!=` на типе с user `@eq` эмитил `->tag` вместо dispatch.
> 4. generic-функция с concrete non-primitive return (`-> Ordering`)
>    инферилась как `void*` на call-site.
>
> Documented limitations: `[M-generic-static-method-on-typevar]`,
> `[M-for-in-explicit-elem-type]`. Регрессий — 0.

## Зачем

После Plan 75 видно: многие встроенные типы покрыты только smoke-тестами или вообще не покрыты.
Каждый метод каждого типа должен иметь ≥1 позитивный тест + ≥1 негативный (Err/None/panic/compile-error).
Особый фокус — поиск codegen-багов (аналог Plan 75: char_at, str.from(char), интерполяция char-var).

## Scope

| Sub-plan | Тип | Тест-каталог |
|---|---|---|
| [85.1](85.1-stringbuilder-coverage.md) | StringBuilder | `nova_tests/str_builder/` |
| [85.2](85.2-buffers-coverage.md) | ReadBuffer + WriteBuffer | `nova_tests/buffers/` |
| [85.3](85.3-conversion-protocols.md) | From / Into / TryFrom / TryInto | `nova_tests/protocols/conversion/` |
| [85.4](85.4-comparison-protocols.md) | Hashable / Equatable / Comparable / Display | `nova_tests/protocols/comparison/` |
| [85.5](85.5-iter-protocol.md) | Iter[T] | `nova_tests/protocols/iter/` |

## Методология (из Plan 75)

- Каждый API-метод → ≥1 позитивный тест
- Каждый Err/None/OOB-путь → отдельный негативный тест
- Compile-error ожидания → `EXPECT_COMPILE_ERROR <pattern>`
- Runtime panic ожидания → `EXPECT_RUNTIME_PANIC <pattern>`
- Unicode (кириллица, emoji) во всех str/char методах
- Если найден баг → фиксируется в рамках sub-plan; нетривиальный → documented known limitation

## Acceptance criteria

- [x] Каждый метод каждого типа — ≥1 тест
- [x] Каждый Err/None-путь — отдельный тест
- [x] Все найденные codegen-баги — 4 исправлены; 2 ограничения documented
- [x] `nova test` — 0 новых FAIL относительно baseline (958 → 1005)

## Non-scope

- `Option[T]` / `Result[T,E]` — покрыты в `nova_tests/plan62/` и `nova_tests/plan72/`
- `HashMap` / `Vec` / остальные коллекции — Plan 86+
- `char` отдельные методы — уже покрыты планом 75 (char_at, str.from(char))
- Производительность / бенчмарки — Plan 57
- Unicode collation — будущее

## Порядок выполнения

Рекомендуется последовательно: 85.1 → 85.2 → 85.3 → 85.4 → 85.5.
85.3 зависит от базового From/Into codegen; 85.4 и 85.5 независимы.
