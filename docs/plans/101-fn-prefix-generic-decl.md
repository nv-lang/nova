// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 101 — `fn[T]` префикс для bare-typevar receiver'ов

> **Создан:** 2026-05-24. Дизайн-decisions финализированы в обсуждении
> 2026-05-24 (см. ниже §«Дизайн»). Spec: [D145](../../spec/decisions/02-types.md#d145-fnt-префикс--generic-declaration-для-bare-typevar-receiverов).
>
> **Статус:** 📋 proposed, не начат, **P3** (grammar extension, не
> блокер 0.1). ~2 dev-day.
> **Приоритет:** P3 (de-magic / completeness — позволяет писать generic
> methods на `[]T`, bare `T`, tuple-receiver'ах; сейчас grammar gap).
> **Зависимости:** [Plan 48](48-closures-in-generics.md) ✅ (D119
> method-level generics); [Plan 88](88-static-method-on-typevar.md) ✅
> (static methods on typevars — закрыт ранее, lineage).
> **Источник:** обсуждение 2026-05-24 — закрытие grammar gap, обнаружен
> probe-тестом `fn T @func[U](a U) -> (T, U) => (@, a)` (парсер
> принимал, codegen падал).

---

## 1. Зачем

Сейчас generic-параметры функции вводятся через **скобки `[…]` на
named generic-типе** в receiver-позиции:

```nova
fn Option[T] @map[U](f fn(T) -> U) -> Option[U] => ...  // T введён через Option[T]
fn HashMap[K, V] @keys() -> []K => ...                  // K, V через HashMap[K, V]
fn Result[T, E] @ok() -> Option[T] => ...               // T, E через Result[T, E]
```

Это work'ает для named generic types. Но **не работает** когда receiver:
- **Bare typevar** — `fn T @func[U]` (T нигде не декларирован).
- **Array of typevar** — `fn []T @append(a T)` (у `[]` нет скобок).
- **Tuple of bare typevars** — `fn (T, U) @swap()` (tuple-конструктор
  без скобок дженериков).

Парсер сейчас **принимает** такой синтаксис (трактует `T` как имя
конкретного типа), а codegen падает с CC-FAIL — `Nova_T*` undefined.
Probe-фикстура:

```nova
fn T @func[U](a U) -> (T, U) => (@, a)
let r = (42).func("hi")  // → CC-FAIL: passing 'nova_int' as 'Nova_T*'
```

**Решение (D145):** narrow `fn[T]` префикс — **только** для случаев,
где existing bracket-mechanism не справляется.

## 2. Дизайн

См. полное описание в [D145](../../spec/decisions/02-types.md#d145).
Краткое summary:

### Три позиции для `[…]`-decl

| # | Где | Существует | Пример |
|---|---|---|---|
| 1 | На named generic-типе receiver'а | ✅ Plan 48 / D119 | `fn Option[T] @map` |
| 2 | На имени метода | ✅ Plan 48 / D119 | `@map[U]`, `@map_err[F]` |
| 3 | **`fn[…]` префикс** | 🆕 D145 / Plan 101 | `fn[T] []T @append` |

### Правило (3) narrow-use

`fn[X1, ..., Xn]` префикс декларирует **только те** typevars, которые
не покрываются (1) в receiver-position. Дублирование с (1) — compile
error.

### Conflict + naming rules

1. **Дублирование `fn[…]` + receiver-`[…]`** — error.  
   `fn[T] Option[T] @map` → «`T` уже введён через `Option[T]`, удалите
   из `fn[…]`».
2. **Лишний в `fn[…]`** — error.  
   `fn[T, X] []T @append(a T)` → «`X` объявлен, но не используется».
3. **Bare typevar без `fn[…]`** — error.  
   `fn T @func() -> T => @` → «`T` не объявлен; добавьте `fn[T]`».
4. **Одно имя — один generic** (existing convention из D119).  
   `fn (T, Option[T]) @method` — оба T один; T вводится в первом
   eligible месте (`Option[T]`), второе use — голая ссылка.
5. **Опция A для mix-cases:** `fn[T] (T, Option[U]) @method` —
   T bare через `fn[T]`, U через `Option[U]`. **Никакой передекларации**.

### Backward-compat: 100%

Existing `fn Option[T] @map` и `fn HashMap[K, V] @keys` — **не меняются**.
D145 строго аддитивно. Migration не требуется.

## 3. Фазы

### Ф.0 — GATE: parser probe (current state)

**Acceptance:** доказать парсер-текущее поведение через regression-фикстуры.

- Test 1: `fn T @func[U](a U) -> ... => ...` — `nova check` PASS
  (парсер принимает `T` как имя типа), `nova test` CC-FAIL (codegen
  не диспатчит). Документирует pre-D145 baseline.
- Test 2: `fn []T @append(a T) { ... }` — то же.
- Test 3: `fn (T, U) @swap() -> (U, T) => ...` — то же.

Артефакт: `nova_tests/plan101/probe/` с 3 negative-фикстурами +
expected diagnostic.

### Ф.1 — Parser: grammar extension

**Acceptance:** парсер принимает `fn[X, Y] ...` префикс и сохраняет
список typevars в `FnDecl.generics` (или новое поле — TBD по структуре
AST).

Файлы:
- `compiler-codegen/src/parser/...` — добавить `parse_fn_prefix_generics`
  hook между `fn` keyword и receiver-type.
- AST: расширить `FnDecl` если нужно (или переиспользовать `generics`
  поле, если уже есть; см. Plan 48 — там method-level в отдельном поле).

Negative parser tests:
- `fn[T, T] T @func` — duplicate typevar names → parse error.
- `fn[ ] T @func` — empty generic list → parse error.
- `fn[T,] T @func` — trailing comma — allowed (как в др. формах).

### Ф.2 — Type-checker: conflict + usage rules

**Acceptance:** все 5 правил из §«Conflict + naming rules» enforced
с loud diagnostics.

Файлы: `compiler-codegen/src/check/` (или где сейчас type-checker
для method declarations).

Логика:
1. Собрать generic-set из `fn[…]` — `prefix_decls`.
2. Пройти receiver-type — собрать typevars, найденные в (1)-eligible
   позициях — `receiver_decls`.
3. Conflict: `prefix_decls ∩ receiver_decls` — error.
4. Усиление: typevar в params/return/body, не покрытый ни (1) ни (3) —
   error.
5. Лишний `fn[…]` typevar (не используется в receiver) — error.

### Ф.3 — Codegen: dispatch on bare typevar receiver

**Acceptance:** для `fn[T] []T @append(a T)`-методов codegen эмитит
правильно mangled `Nova___Array____<T>_method_append(self, a)` (или
аналог по существующей mangling-convention для array methods).

Файлы: `compiler-codegen/src/codegen/emit_c.rs`. Likely changes в:
- Receiver-type resolution для array-of-typevar (новый case в
  `receiver_c_type`).
- Mono-instance registration — generic'и из `fn[…]` подаются как
  receiver-level subst, не method-level.

### Ф.4 — Tests: positive + negative

**Acceptance:** все три use-case'а проходят `nova test`. 8-10 фикстур.

`nova_tests/plan101/`:
- `bare_typevar_identity.nv` — `fn[T] T @identity() -> T => @`
- `array_append.nv` — `fn[T] []T @append(a T)` (probe из probe_T_receiver)
- `tuple_swap.nv` — `fn[T, U] (T, U) @swap() -> (U, T)`
- `mix_bare_and_named.nv` — `fn[T] (T, Option[U]) @pair() -> (T, U)`
  (Опция A confirmation)
- `nested_array.nv` — `fn[T] [][]T @flatten() -> []T`
- `existing_form_still_works.nv` — `fn Option[T] @map[U]` (regression)
- **Negative:**
  - `duplicate_decl_neg.nv` — `fn[T] Option[T] @map` → error
  - `unused_prefix_decl_neg.nv` — `fn[T, X] []T @append` → error
  - `undeclared_bare_neg.nv` — `fn T @func` → error
  - `name_collision_neg.nv` — `fn[T, T]` → parse error

### Ф.5 — Spec + docs + close

**Acceptance:**
- D145 в spec — ✅ закрыт (создан в Ф.0).
- README plan статус — ✅ ЗАКРЫТ + merge hash.
- simplifications.md: marker `[M-bare-typevar-receiver-grammar-gap]` ✅
  ЗАКРЫТО Plan 101 (новый marker; создать в Ф.5).
- project-creation.txt + discussion-log.md — финальный entry.
- Plan 88 cross-ref — D-block updated с D145-ref.

## 4. Risks + mitigations

| Risk | Mitigation |
|---|---|
| Существующий `fn T @func` (parser принимает, codegen падает) уже мог использоваться где-то в std/tests как dead code — изменение grammar превратит это в parse error | Ф.0 probe + `grep -rE "^fn [A-Z] @" std/ nova_tests/` — найти и исправить заранее. |
| Conflict с возможным будущим `fn[Effect]`-синтаксисом для effect-generic'ов | Effect-generics сейчас не запланированы; если появятся — другой synтаксис (например, `fn{E}` braces). D145 фиксирует `[…]` для type-generics. |
| Multi-typevar в complex tuple-of-named — поведение Опция A может удивлять (`fn (T, Option[U])` — U только через Option[U]) | Документировать в D145 + Plan 101 §2 (уже сделано). 4-я Ф фикстура `mix_bare_and_named.nv` подтверждает Опция A. |

## 5. Acceptance overall

- Все 8-10 фикстур из Ф.4 PASS.
- Полный `nova test` зелёный (regression-gate).
- D145 в spec ✅.
- Plan 101 статус в README ✅ ЗАКРЫТ.
- Marker ✅ ЗАКРЫТО.

## 6. Не входит (future work)

- **Effect-generics** — `fn[E] @method() E -> ()` — отдельный план,
  если эффекты станут параметризуемыми (не запланировано).
- **Higher-kinded receiver** — `fn[F[_]] F[T] @method` (functor-like) —
  отдельный план, требует HK-type-checker (P3+, не P3).
- **Implicit `fn[T]`-inference** — автоматическое introduction T без
  явного `fn[…]` префикса для bare-typevar. Отклонено: loud-decl
  предпочтительнее magic (D119 lineage).
