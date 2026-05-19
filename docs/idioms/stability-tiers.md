# Stability tiers — когда нужны, когда не нужны

> **Что это:** `#stable` / `#unstable` / `#experimental` атрибуты на exported
> items. Определены в [D105](../../spec/decisions/09-tooling.md#d105-doc-атрибуты),
> scope enforcement'а — в [D127](../../spec/decisions/09-tooling.md#d127-stability-tier-enforcement-scope).
>
> **TL;DR:** обязательны только в library crates с явным opt-in
> (`enforce-stability = true`). User-application code — convention.
> Test fixtures / examples / benchmarks — exempt.

---

## Кто что обязан

| Контекст | `#stable` обязателен? | Поведение `nova doc --check` |
|---|---|---|
| **Library crate с `enforce-stability = true`** (stdlib, public-API libraries) | Да, на каждом `export` | `error` → exit 1 |
| **User application** (binaries, internal tools) | Нет | `warning` (учит conventions, не блокирует CI) |
| **Test fixtures** (`nova_tests/`, `tests/`) | Нет, auto-exempt | silent skip |
| **Examples** (`examples/`) | Нет, auto-exempt | silent skip |
| **Benchmarks** (`bench/`) | Нет, auto-exempt | silent skip |

## Включить enforcement для library crate

```toml
# nova.toml
[package]
name = "mylib"

[lib]
src = "."
enforce-stability = true   # ← каждый export должен иметь tier
```

После этого `nova doc --check` на любом файле под `[lib].src` → exit 1
если export без `#stable` / `#unstable` / `#experimental`.

---

## Когда какой tier

| Tier | Когда использовать |
|---|---|
| `#stable(since = "1.0.0")` | API готов к long-term commitment. Breaking change → semver major bump. `since` рекомендован — используется changelog tooling'ом (Plan 45 Ф.12 `--since` filter). |
| `#unstable(feature = "name")` | API в работе. Caller должен явно opt-in через `#cfg(feature = "name")` (D105 §unstable). Use-сайт вне `#cfg`-scope'а → hard error. |
| `#experimental(note = "...")` | Proof-of-concept. Может исчезнуть в любой момент. Use-site emit'ит warning. `note` обязан описывать ожидаемые изменения. |

## Module-level propagation

D105 §propagation: tier, заданный на module через inner-doc (`//! #stable`)
или module-level attribute, **пропагируется на все items без явного override**.

```nova
//! Сжатие данных через DEFLATE.
//!
//! #stable(since = "1.0.0")

module compression.deflate

// Auto-inherits `#stable(since = "1.0.0")` — не нужно повторять.
export fn compress(data []u8) -> []u8 { ... }

// Явный override — этот item остаётся experimental.
#experimental(note = "API будет переработан под streaming")
export fn compress_with_window(data []u8, window u32) -> []u8 { ... }
```

Если 90% items одного tier'а — задайте на module, override на исключениях.

---

## Best practices

### ✅ Делайте

- **`#stable(since = "X.Y.Z")` с явной версией.** Changelog tooling
  использует `since` для генерации.
- **Module-level tier** если большинство items одного класса. Override
  на исключениях.
- **`#experimental` для новых API в первом release cycle.** Затем
  promote до `#unstable` (с feature flag) или `#stable` после soak.
- **`#deprecated(since, until, note)` вместе с `#stable`/`#unstable`** —
  они orthogonal: stable item может быть deprecated.

### ❌ Не делайте

- **Не ставьте `#stable` на experimental API "чтобы скрыть warning".**
  Это обещание перед users — breaking change станет major release event.
- **Не оборачивайте все exports в `#experimental` "на всякий случай".**
  Lint `experimental-overuse` (Plan 45 future) поймает этот pattern.
- **Не миксуйте `#stable` без `since` и `#stable(since = "X")` в одном
  module.** Inconsistency.

---

## Industry comparison

| Язык | Подход |
|---|---|
| **Rust** | `#[stable(feature, since)]` / `#[unstable(feature, issue)]`. Enforce'ится только в stdlib под `#![feature(staged_api)]`. User crates — никогда. |
| **Swift** | `@available(macOS 10.15, *)` — implicit availability от target OS. Не on-each-item. |
| **Kotlin** | `@RequiresOptIn` — для opt-in API. `@Deprecated`. Не на каждом export. |
| **Scala 3** | `@experimental` / `@apiStatus.Stable` опционально. |
| **C# / .NET** | `[Obsolete]`. Stability — convention. |
| **Go / Python / OCaml / Haskell** | Convention в docstring. Без enforcement. |

**Nova:** конфигурируемое. По умолчанию — convention (warning). Opt-in
strict — для library crates обязующихся документировать API. Test
fixtures — exempt.

---

## См. также

- [D105 doc-атрибуты](../../spec/decisions/09-tooling.md#d105-doc-атрибуты)
- [D127 stability-tier enforcement scope](../../spec/decisions/09-tooling.md#d127-stability-tier-enforcement-scope)
- [Plan 45 nova doc](../plans/45-nova-doc.md) §11.5 №7
- [Plan 71 doc-stability-scope](../plans/71-doc-stability-scope.md)
