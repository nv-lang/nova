\ SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 105 — `type X u8 | A = 0 | B = 1` парсер + codegen для явного базового типа sum'ов

> **Создан:** 2026-05-27. **Статус:** 📋 proposed, **P2**.
> **Источник:** drift discovery 2026-05-27 — spec задокументировал
> синтаксис в [D-блоке без номера, 02-types.md:270-277](../../spec/decisions/02-types.md#L270-L277),
> но парсер падает с `expected fn / type / let / const / test, got '|'`.
> **Spec:** [02-types.md §«Явный базовый тип»](../../spec/decisions/02-types.md#L270),
> [syntax.md:550](../../spec/syntax.md#L550),
> [open-questions.md:508](../../spec/open-questions.md#L508).
> **Зависит от:** ничего (изолированный фикс).

---

## 1. Проблема

Спека утверждает, что у sum-типа с дискриминантами можно опционально
указать базовый числовой тип:

```nova
type Bit u8 | Off = 0 | On = 1                      \ ✗ FAIL parse
type HttpCode i32 | Ok = 200 | NotFound = 404       \ ✗ FAIL parse
```

Парсер реально принимает только дефолтную форму без базового типа:

```nova
type Bit | Off = 0 | On = 1                         \ ✓ PASS (implicit int)
```

**Probe (2026-05-27, `nova check`):**

```
tmp_probe.nv:3:13: error: expected fn / type / let / const / test, got `|`
  3 | type Bit u8 | Off = 0 | On = 1
    |             ^
```

`type_decl` ловит идентификатор `u8` как начало следующего top-level
определения, а `|` дальше становится мусором. Никакого ветвления на
«опциональный базовый тип» в парсере не существует.

## 2. Решение

Минимальное расширение grammar — после имени типа и опциональных
generic-параметров парсер пытается распознать **один primitive type
token** (`u8 | u16 | u32 | u64 | i8 | i16 | i32 | i64 | int | uint`),
сразу за которым обязан идти `|`. Если за идентификатором НЕ `|` —
back-off (это не base type, а что-то другое), parse продолжается как
сейчас.

AST: к `TypeDeclKind::Sum` добавить `base_type: Option<PrimitiveType>`.
Codegen: вместо `typedef enum { … }` (implicit `int`) эмитить
`typedef <c_type> Nova_<Name>_Tag;` + ручное присвоение discriminant'ов
по AST (инфраструктура для discriminant-значений уже есть в
[SumVariant.discriminant](../../compiler-codegen/src/ast/mod.rs#L726)).

## 3. Декомпозиция (фазы)

| Ф. | Что | Acceptance |
|---|---|---|
| **Ф.0** | GATE probe-фикстура: `nova_tests/types/sum_base_type_probe.nv` повторяет 3 формы (`u8`, `i32`, `int` явный) — на момент Ф.0 все ❌ FAIL parse. Фиксирует baseline. | 3/3 FAIL зафиксировано в expected_*.txt |
| **Ф.1** | Парсер: расширить `parse_type_decl()` ([parser/mod.rs:2273](../../compiler-codegen/src/parser/mod.rs#L2273)) — после имени+generic'ов + lookahead на primitive-token + следующий `\|`. Принять `base_type` опционально. | Probe Ф.0 → 3/3 PASS parse; existing sum-tests без регрессий |
| **Ф.2** | AST: `TypeDeclKind::Sum { variants, base_type: Option<PrimitiveType> }` ([ast/mod.rs:660](../../compiler-codegen/src/ast/mod.rs#L660)). Все call-site обновлены. | `cargo check` чистый |
| **Ф.3** | Type-checker: bounds-валидация discriminant'ов — если `base_type = u8`, все discriminant'ы должны помещаться в `0..=255`; для `i8` — `-128..=127` и т.д. Loud error `E_SUM_DISCRIMINANT_OUT_OF_RANGE` с указанием конкретного варианта и допустимого диапазона. | Negative-фикстура `type X u8 \| A = 256` → ERR с правильным message |
| **Ф.4** | Codegen ([emit_c.rs:7448](../../compiler-codegen/src/codegen/emit_c.rs#L7448)): если `base_type.is_some()` — эмитить `typedef <c_type> Nova_Tag_<Name>;` + `#define NOVA_TAG_<Name>_<Variant> ((<c_type>)<discriminant>)` вместо C-enum'а. Constructor ([emit_c.rs:7548](../../compiler-codegen/src/codegen/emit_c.rs#L7548)) присваивает напрямую. Для `base_type = None` — без изменений (regression 0). | `nova test` зелёный; `sizeof(Nova_Bit)` для `u8`-base = 1, для default-int = 4 (проверяется runtime-фикстурой через `as int`) |
| **Ф.5** | Positive-фикстуры: 8 файлов в `nova_tests/types/sum_base_type_*.nv` — `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `u64`, `i64`. Каждый делает `let x = Variant; let n = x as int; println(...)` и проверяет числовое значение через `expected_stdout`. | 8/8 PASS под обоими backend'ами (clang + MSVC) |
| **Ф.6** | Spec close + closure: пометить marker `[M-sum-explicit-base-type-parser-gap]` в [docs/simplifications.md](../simplifications.md) как ✅ ЗАКРЫТО (создаётся в Ф.0). Обновить [02-types.md:276-277](../../spec/decisions/02-types.md#L276-L277) — добавить sub-section «Bounds validation» c precise error message. Обновить project-creation.txt + discussion-log.md. | Marker ✅; spec amend ссылается на Plan 105 |

**Total:** ~1.5 dev-day (изолированный, без зависимостей).

## 4. Acceptance (master)

Plan 105 = ✅ ЗАКРЫТ когда:

- [ ] Все 3 формы из spec'а компилируются и работают: `type Bit u8 | …`,
      `type HttpCode i32 | …`, `type X int | …` (явный default).
- [ ] Bounds-validation loud при выходе за диапазон базового типа.
- [ ] `sizeof(Tag)` совпадает с базовым типом (`u8` → 1 байт, etc.).
- [ ] Regression 0: `nova test` зелёный, все существующие sum-тесты
      без `base_type` работают как раньше.
- [ ] Spec ↔ impl drift ликвидирован: marker `[M-sum-explicit-base-type-parser-gap]` ✅.
- [ ] Memory: `project-plan105-status.md` создан.

## 5. Что вне scope (явно отложено)

- **`type X u8` без `|`** (newtype с явным representation) — это другой
  use-case, относится к Q-representation-bound / Plan 102. Plan 105 — только
  sum-with-discriminants.
- **Auto-derive `Into[int]`** для sum'ов с базовым типом — отдельное
  обсуждение в контексте D73/D131.
- **Bit-packing нескольких discriminant'ов в один байт** (как Rust
  `#[repr(u8)]` + niche-optimization) — оптимизация runtime layout,
  не grammar-фикс.

## 6. Связь

- [D-spec 02-types.md §«Discriminants»](../../spec/decisions/02-types.md#L270) — формализует.
- [Q-representation-bound](../../spec/open-questions.md) → Plan 102 (newtype с явным
  representation для **не-sum** типов; complementary).
- [Plan 101](101-receiver-generic-prefix.md) — параллель «spec задокументировал,
  parser silently бьёт» (тот же class drift'а).
