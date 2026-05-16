# `nova doc` — user guide

`nova doc` генерирует документацию из Nova-исходников (D104-D107).

## Quick start

```bash
nova doc path/to/file.nv                # Markdown в stdout
nova doc path/to/file.nv --format json  # JSON в stdout
nova doc path/to/file.nv --test         # Запустить doc-tests
nova doc path/to/file.nv --check        # Проверить doc на ошибки (CI)
```

## Doc-comments

Используются два вида комментариев:

- `///` (outer) — относится к следующей декларации.
- `//!` (inner) — относится к module целиком.

```nova
//! Модульный doc — описание всего файла.

module my.module

/// Краткое summary в одно предложение.
///
/// Длинное описание ниже. Может занимать несколько строк.
///
/// # Examples
///
/// ```nova
/// let x = double(3)
/// assert(x == 6)
/// ```
export fn double(x int) -> int => x * 2
```

## Sections

Распознаются стандартные секции (D107, fixed order):

| Section | Назначение |
|---|---|
| `# Examples` | Примеры использования. ``` ```nova ``` блоки — doc-tests. |
| `# Errors` | Какие `Fail[X]` функция может вернуть. |
| `# Panics` | Условия runtime panic. |
| `# Safety` | Invariants для `unsafe`-вызывающих. |
| `# Effects` | Список effects (если не очевиден из signature). |
| `# Contracts` | Pre/post-conditions (см. Plan 33). |
| `# Since` | Версия, с которой fn появилась. |
| `# See also` | Cross-references. |
| `# Deprecated` | Причина + рекомендация замены. |

Другие `# Heading` сохраняются в текущей секции как часть text'а.

## Intra-doc links

```nova
/// Возвращает [Point] для координаты.
/// См. также [translate] и [std.math.abs].
```

Резолвинг:
- `[Name]` — short-match по item-id.
- `[Type.method]` — короткая форма метода.
- `[mod.path.Name]` — fully-qualified.

Broken links — `target_id: null` в JSON и сообщение в `nova doc --check`.

## Doc-tests

` ```nova `-блоки в doc'е — это doc-tests, исполняемые `nova doc --test`.

### Modifiers

```
```nova,no_run        — компилируется, не запускается
```nova,ignore        — не компилируется (только display)
```nova,compile_fail  — ожидается compile-error
```nova,should_panic  — ожидается runtime panic
```nova,must_verify   — ожидается successful SMT verify (Plan 33)
```

Можно комбинировать: ``` ```nova,no_run,ignore ```.

### Hidden lines

Строки `# code` скрыты в визуальном выводе, но включаются в компиляцию:

```nova
/// ```nova
/// # import std.io
/// let r = compute()
/// assert(r == 42)
/// ```
```

## Stability и deprecation

Два способа объявить:

**Через sections** (estable-via-version derivation):

```nova
/// API.
///
/// # Since
///
/// 1.0.0
export fn add(a int, b int) -> int => a + b
```

`# Since >= 1.0` → `stability.tier = "stable"`, иначе `unstable`.

**Через inline doc-attrs** (явный override):

```nova
/// Экспериментальное API.
///
/// #[experimental]
/// #[since("0.3.0")]
///
/// Подробности ниже.
export fn experimental_api() -> int => 0
```

Поддерживаемые attrs: `#[deprecated("note")]`, `#[since("version")]`,
`#[stable]`, `#[unstable]`, `#[experimental]`.

`#[deprecated]` приоритетнее, чем `# Deprecated` section.

## CLI flags

| Flag | Описание |
|---|---|
| `--format markdown\|json` | Output format (default `markdown`). |
| `--include-private` | Включить non-export items (default — только `export`). |
| `--test` | Запустить doc-tests вместо рендеринга. |
| `--check` | Проверить doc (broken links + missing summaries) → exit 1 при issue. |
| `--json-schema` | Напечатать embedded JSON Schema 2020-12 (D107). |

## JSON output

D107 schema v1. Ключи в алфавитном порядке, byte-for-byte deterministic.
Top-level fields:

- `format_version: 1`
- `nova_version: "0.1.0"`
- `generated_at` — опускается по умолчанию (определяется через
  `NOVA_DOC_GENERATED_AT` или `SOURCE_DATE_EPOCH`).
- `doc_tests[]` — извлечённые ```nova блоки.
- `links[]` — intra-doc-links (resolved + broken).
- `modules[]` — DocModule entries.
- `items[]` — DocItem entries (fn/type/const/effect/protocol).

См. `--json-schema` для полной спецификации.

## CI integration

```bash
# Проверить, что doc валиден (broken links, missing summaries).
nova doc src/api.nv --check

# Прогнать doc-tests (exit 1 на failure).
nova doc src/api.nv --test
```

Reproducible builds:

```bash
SOURCE_DATE_EPOCH=1700000000 nova doc src/api.nv --format json > api.json
```

## Style guide (§11.5)

1. Первое предложение — summary (`.` terminator).
2. Imperative mood: "Returns X" а не "This function returns X".
3. Fixed section order (см. таблицу выше).
4. Markdown subset: CommonMark + ` ```nova ` fenced blocks.
5. Examples обязательны для public fn.
6. Deprecation note содержит замену + `# Since`.
7. Stability tiers: stable / unstable / experimental.

## См. также

- `spec/decisions/03-syntax.md` — D104 doc-comment syntax.
- `spec/decisions/09-tooling.md` — D105/D106/D107.
- `docs/plans/45-nova-doc.md` — план реализации.
