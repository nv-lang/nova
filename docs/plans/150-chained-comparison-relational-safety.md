<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 150 — Reject chained comparison + ban bool relational operands (Rust-style)

> **Создан:** 2026-06-13.  **Статус:** 📋 PLANNED.  **Приоритет:** P1 (security: вакуумные контракты).
> **Эстимат:** ~1–1.5 dev-day.  **Model:** Opus + Thinking ON.
> **Решает:** Q35 + `[M-comparison-bool-operand-or-chaining]`.  **Разблокирует:** `[M-140-bounds-as-contract]`.
> **Решение автора (2026-06-13):** hard-error (option a). Chained comparison `a<b<c` **НЕ добавляем** —
> как Go/Rust/Kotlin/Java/Swift (только Python чейнит). Канонная форма диапазона — `a <= b && b < c`.

## Цель (в двух словах)
Починить footgun: `0 <= i < n` сейчас парсится как `(0<=i) < n` = `bool < n` и **молча вакуумно-истинно**
(проверка обходится). Сделать это **ошибкой компиляции** с подсказкой «пиши `0 <= i && i < n`» (как Rust),
+ запретить `bool` как операнд `<`/`<=`/`>`/`>=`. **Нового синтаксиса НЕ вводим.**

## Почему так (design-workflow + adversarial-judge 2026-06-13)
- **Status quo — SECURITY-дефект.** `requires 0 <= i < @len` — канонический bounds-контракт; сейчас
  вакуумен (checker не проверяет операнды, `bool < int` молча проходит, interp-ошибка глотается в
  vacuous-true assert/contract) → bounds-check тихо обходится. Nova **хуже всех peers** (даже untyped
  JS коэрсит в детерминированно-неверное; Nova нейтрализует предикат).
- **Прецеденты:** Go/Rust/Kotlin/Java/Swift — **hard-error** на `a<b<c` (bool не ordered, нет коэрции).
  Только Python чейнит. **Best-in-class = Rust:** дедикейтед «comparison operators cannot be chained» +
  machine-applicable fix-it. **Решение: матчим Rust** (hard-error + дедикейтед диагностика). Chaining
  (Python-only) НЕ добавляем — `a <= b && b < c` понятно, без нового синтаксиса/парсер-сложности.
- **Bool relational уже method-only:** `std/runtime/defaults.nv` делает `false<true` через `@compare`,
  НЕ через `<` → бан bool-relational-операндов консистентен с дизайном (`==`/`!=` на bool остаются).

## Изоляция / процесс
- Отдельный worktree `nova-p150` (ветка `plan-150` от main). **НЕ `git stash`** (repo-global,
  конкурентные worktree) — baseline через temp-worktree/commit-reset. git add конкретные файлы;
  commit `-s` без `Co-Authored-By`. **basics ОБЯЗАТЕЛЬНО в regression sweep** (урок Plan 139.2). Тесты —
  релизным nova, И C-codegen (`nova test`) И interp (`test-interp`).

---

## Ф.0 — Spec / D / Q (~0.25d, doc-only)
Новый D-блок (след. свободный, grep `^## D[0-9]` sort -V tail) в spec/decisions:
1. **Chained comparison ОТКЛОНЁН:** последовательность ≥2 relational/equality операторов одного
   precedence-уровня (`a OP1 b OP2 c`) → **hard error** `E_CMP_CHAIN_UNSUPPORTED` с fix-it «split into
   `a OP1 b && b OP2 c`». Rationale: только Python чейнит; `&&` явно + без нового синтаксиса.
2. **Relational-операнды требуют ordered-категорию:** `<`/`<=`/`>`/`>=` требуют mutually-ordered тип
   (int/float/str/char или `@compare`-несущие); `bool`/`ptr`/`unit` → `E_RELATIONAL_OPERAND_NOT_ORDERED`.
   `==`/`!=` на bool — **остаются легальны** (только relational получают бан). Консистентно с bool
   `@compare` method-only + Plan 115 ptr-ban (reuse `E_PTR_ARITHMETIC_BANNED`).
3. **Q-блок (резолвит Q35):** зафиксировать решение «hard-error, НЕ chaining» + почему (peer-norm,
   one-canonical через `&&`). Cross-link Plan 85.4 @compare. Если в будущем будет спрос на chaining —
   отдельное предложение (не сейчас).
- **Commit:** `spec(plan150 Ф.0): reject chained comparison + relational-operand ordering D-block + Q35`

## Ф.1 — Parser + checker: hard-error + Rust-grade диагностика (~0.5d)
- **Parser** (`parser/mod.rs:5872` `parse_cmp` + `:5848` `parse_eq`): при разборе обнаружить
  **≥2 сравнения подряд** на одном уровне (`a OP1 b OP2 c`) → эмитить `E_CMP_CHAIN_UNSUPPORTED`
  («comparison operators cannot be chained») + machine-applicable fix-it `a OP1 b && b OP2 c`.
  (Не нужен новый AST-узел — детект в loop'е + диагностика + recovery.) **Grammar-тесты ПЕРВЫМИ**
  (lock precedence-boundary; риск mis-parse).
- **Checker** (`types/mod.rs:7533` Binary arm): для relational-оператора (`<`/`<=`/`>`/`>=`) проверять
  категорию обоих операндов; `bool`/`unit`/`ptr` или incompatible cross-category → диагностика
  (`E_RELATIONAL_OPERAND_NOT_ORDERED` для bool/unit; `E_PTR_ARITHMETIC_BANNED` для ptr — existing).
  Ловит одиночный `someBool < 5`. Permissive-на-Unknown/Other сохранить (не ломать generics —
  выверить `cat_of` generic-scope).
- Targeted-fixture per-fix; full nova test (+ test-interp) в конце фазы.
- **Commit:** `feat(plan150 Ф.1): reject chained comparison (E_CMP_CHAIN_UNSUPPORTED) + ban bool relational operand`

## Ф.2 — Тесты: позитивные + негативные (~0.25d, И C-codegen И interp)
**Позитивные** (`nova_tests/plan150/`):
- `0 <= i && i < n` — работает (канонная форма диапазона).
- `requires 0 <= i && i < @len` — реальный range-check (нарушающий вызов trap'ится, НЕ вакуумен) —
  **regression security-дефекта**.
- `a == b`, `a < b`, `bool == bool`, `bool != bool` — одиночные сравнения и bool-равенство OK.

**Негативные** (точные коды):
- `0 <= i < n` → `E_CMP_CHAIN_UNSUPPORTED` + проверить наличие fix-it «`0 <= i && i < n`».
- `a < b < c`, `a < b <= c`, `1 < 2 < 3` (даже ordered!) → `E_CMP_CHAIN_UNSUPPORTED`.
- `true < false`, `bool < int`, `someBool < 5` → `E_RELATIONAL_OPERAND_NOT_ORDERED`.
- `ptr` relational → `E_PTR_ARITHMETIC_BANNED` (existing) всё ещё fires.
- **Историч. footgun** `assert(0 <= 100 < 10)` → конвертировать в regression-тест: теперь **ошибка
  компиляции** `E_CMP_CHAIN_UNSUPPORTED` (раньше — вакуумно PASS).
- **Commit:** `test(plan150): chained-comparison reject + bool-relational ban pos/neg`

## Ф.3 — Docs, migration, close (~0.25d)
- spec → D implemented; language-reference + contracts-guide: канонная форма диапазона = `a <= b && b < c`
  (показать `requires 0 <= i && i < @len`). Error-code-индекс: оба кода + примеры + fix-it.
- Migration-note: НЕ breaking на практике (старый `(a<b)<c` уже был runtime-error/vacuous, не легитимен —
  checker-scan: 0 реального `bool<bool`/`bool<int`); flag поведенч. изменение для кода, опиравшегося на
  вакуумный контракт.
- project-creation.txt + simplifications.md + nova-private/discussion-log.md; закрыть Q35 +
  `[M-comparison-bool-operand-or-chaining]`; разгейтить `[M-140-bounds-as-contract]` (с `&&`-формой).
- **Commit:** `docs(plan150 Ф.3): relational-safety reference + migration + close`

---

## Критерии приёмки
- **A1** — `a OP1 b OP2 c` (≥2 relational/equality на одном уровне) → `E_CMP_CHAIN_UNSUPPORTED`
  (даже для ordered: `1 < 2 < 3`).
- **A2** — диагностика **Rust-grade**: «comparison operators cannot be chained» + machine-applicable
  fix-it `a OP1 b && b OP2 c`.
- **A3** — `bool` / `unit` как операнд `<`/`<=`/`>`/`>=` → `E_RELATIONAL_OPERAND_NOT_ORDERED`;
  `==`/`!=` на bool — **легально** (не трогаем).
- **A4** — `ptr`-relational → `E_PTR_ARITHMETIC_BANNED` (existing) сохраняется.
- **A5** — канонная форма `0 <= i && i < n` работает; `requires 0 <= i && i < @len` — **реальный**
  range-check (нарушение trap'ится, НЕ вакуумно) — regression security-дефекта.
- **A6** — `assert(0 <= 100 < 10)` теперь **ошибка компиляции** (раньше вакуумно PASS).
- **A7** — pos+neg фикстуры релизным nova (И C-codegen И interp); 0 регрессий (basics + широкий sweep);
  Permissive-на-generics сохранён (нет ложных срабатываний на Unknown-категориях).
- **A8** — spec D-блок + Q35 resolved + error-code-индекс + docs обновлены.

## Зависимости / связь
- **Разблокирует `[M-140-bounds-as-contract]`** — `requires 0 <= i && i < @len` валиден и НЕ вакуумен
  (после Ф.1 чекер гарантирует, что попытка вакуумного `0<=i<@len` — ошибка).
- **Резолвит Q35** + `[M-comparison-bool-operand-or-chaining]`.
- Plan 115 (ptr relational ban) — reuse `E_PTR_ARITHMETIC_BANNED`. Plan 85.4 (@compare protocol).

## Риски (из design-review)
| # | Риск | Mitigation |
|---|---|---|
| R1 | Parser: детект ≥2-сравнений ломает precedence (`a == b < c`) или loop | grammar-тесты ПЕРВЫМИ; lock boundary в Ф.0 spec |
| R2 | Bootstrap-permissive конфликт (checker «Permissive на Other») | check fires только на definitively-known (bool/ptr/unit + concrete mismatch); Unknown/Other permissive — verify cat_of generic-scope |
| R3 | Регрессия в корпусе (детект chain меняет parse/type-errors) | full regression (nova test + test-interp) в конце Ф.1; scan подтвердил 0 легитимного usage |
| R4 | Diagnostic UX слабее Rust | дедикейтед `E_CMP_CHAIN_UNSUPPORTED` (НЕ generic type-mismatch) + machine-applicable fix-it — A2 |

## Что НЕ делаем (сознательно)
- **Python-style chaining** (`a<b<c` ≡ `a<b && b<c`) — ОТКЛОНЁН. Только Python так делает; systems-языки
  (Go/Rust/Kotlin/Java/Swift) — нет. `&&` явно + без нового синтаксиса/парсер-сложности/single-eval-temps.
  Если будет спрос — отдельное предложение в будущем (зафиксировано в Q35).

## Связанные D-блоки
| D | Что |
|---|---|
| (новый, Ф.0) | Reject chained comparison (`E_CMP_CHAIN_UNSUPPORTED` + fix-it); relational-операнды требуют ordered-категорию (bool/unit → `E_RELATIONAL_OPERAND_NOT_ORDERED`; `==`/`!=` на bool легальны) |
| Plan 115 | reuse `E_PTR_ARITHMETIC_BANNED` (ptr relational) |

## Статус
📋 **PLANNED** — решение автора (2026-06-13): hard-error (option a, как Rust); chained comparison
НЕ добавляем. Готов к выполнению (~1–1.5 dev-day, Ф.0-Ф.3).
