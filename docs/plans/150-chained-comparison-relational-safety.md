<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 150 — Chained comparison + relational-operand safety

> **Создан:** 2026-06-13.  **Статус:** 📋 PLANNED.  **Приоритет:** P1 (security: вакуумные контракты).
> **Эстимат:** ~2–3 dev-day (Ф.0-Ф.1 ≈ 1d shippable сам по себе; Ф.2+ ≈ 1–2d).  **Model:** Opus + Thinking ON.
> **Решает:** Q35 + `[M-comparison-bool-operand-or-chaining]`.  **Разблокирует:** `[M-140-bounds-as-contract]`.

## Цель (в двух словах)
Починить footgun `0 <= i < @len` (сейчас парсится как `(0<=i) < @len` = `bool < @len`, молча
**вакуумно-истинно**) и дать настоящий chained comparison. Итог: **безопасность+диагностика Rust
ПЛЮС эргономика Python**, без минусов обоих.

## Почему (design-workflow 2026-06-13, adversarial-judge)
- **Status quo НЕДОПУСТИМ + это SECURITY-дефект.** `requires 0 <= i < @len` — канонический
  bounds-контракт; сейчас он **вакуумен** (checker не проверяет операнды, `bool < int` молча
  проходит, interp-ошибка глотается в vacuous-true assert/contract) → bounds-check тихо обходится.
  Nova сейчас **хуже всех peers**, включая untyped JS (JS хотя бы ToNumber-коэрсит в детерминированно-
  неверное; Nova тихо нейтрализует предикат).
- **Прецеденты:** только Python чейнит (`a<b<c` ≡ `a<b and b<c`, `b` раз). Go/Rust/Kotlin/Java/Swift —
  **hard-error** (bool не ordered, нет коэрции). TS/JS — silent-coerce. **Best-in-class = Rust:**
  hard-error + ДЕДИКЕЙТЕД «comparison operators cannot be chained» + machine-applicable fix-it.
- **Контракт-корпус** (`nova_tests/contracts/f14_*`, f47): range-bounds пишут verbose `lo<=mid && mid<=hi`;
  chained `a<=x<b` встречается только в КОММЕНТАХ (math) → юзеры хотят, но не могут доверять.
- **Bool relational уже method-only:** `std/runtime/defaults.nv` делает `false<true` через `==`/`@compare`,
  НЕ через оператор `<` → бан bool/non-ordered relational-операндов консистентен с дизайном.

## Решение — hard-error-plus-chained, ФАЗИРОВАННО
Ф.0-Ф.1 доставляют **option (1)** (hard-error + Rust-диагностика = peer-parity, **shippable сам по себе**);
Ф.2+ накладывают настоящий chaining (**option 3**). Если Ф.2 поскользнётся — у Nova уже peer-parity.

**Семантика (нормативно):** `a OP1 b OP2 c` ≡ `tmp_b = b; (a OP1 tmp_b) && (tmp_b OP2 c)` —
средние операнды вычисляются **РОВНО ОДИН раз**, слева-направо, конъюнкция **short-circuit**.

## Изоляция / процесс (когда выполняется)
- Отдельный worktree `nova-p150` (ветка `plan-150` от main). **НЕ `git stash`** (repo-global,
  конкурентные worktree) — baseline через temp-worktree/commit-reset. git add конкретные файлы;
  commit `-s` без `Co-Authored-By`. **basics ОБЯЗАТЕЛЬНО в regression sweep** (урок Plan 139.2) +
  NOVA_DIAG на любой RUN-FAIL. Тесты — релизным nova, **И C-codegen (`nova test`) И interp (`test-interp`)**.

---

## Ф.0 — Spec / D / Q (~0.25d, doc-only)
Новый D-блок (след. свободный, grep `^## D[0-9]` sort -V tail) в spec/decisions:
1. **Грамматика:** последовательность ≥2 relational (`<`/`<=`/`>`/`>=`) и equality (`==`/`!=`) операторов
   одного precedence-уровня → chained.
2. **Семантика (нормативно):** desugar в left-to-right конъюнкцию, средние операнды — РОВНО ОДИН раз;
   short-circuit (если `a OP1 b` false — дальше НЕ вычисляется). Явно — это user-visible с side-effects.
3. **Запрет non-ordered операндов:** relational требует mutually-ordered категорию (int/float/str/char
   или `@compare`-несущие типы); `bool`/`ptr`/`unit` → hard error (консистентно с bool `@compare`
   method-only + Plan 115 ptr-ban).
4. **Mixed-direction (`a < b > c`):** рекомендую ALLOW (per-pair, как Python) — но **Q-блок**: если
   ревьюер хочет Rust-strict, gate через lint, не parse-error.
5. **Equality-in-chain (`a < b == c`):** рекомендую ALLOW per-pair — Q-блок на подтверждение.
6. **Q-блок:** `bool == bool` остаётся легальным (только RELATIONAL получают bool-бан). Cross-link
   Plan 85.4 @compare. Резолвит Q35.
- **Commit:** `spec(plan150 Ф.0): chained comparison + relational-operand safety D-block + Q`

## Ф.1 — Parser + checker: hard-error + Rust-grade диагностика (= option 1, shippable) (~0.75d)
- **Parser** (`parser/mod.rs:5872` `parse_cmp` + `:5848` `parse_eq`): из nested-Binary-accumulation →
  собирать `first` + `Vec<(BinOp, Expr)>`. `len()==1` → Binary как сейчас (back-compat, ноль AST-churn).
  `len()>=2` → новый `ExprKind::ChainedComparison{first, comparisons}`. parse_eq+parse_cmp кооперируют
  (mixed `==`/`<` → ОДИН ChainedComparison). **Grammar-тесты ПЕРВЫМИ** (lock precedence-boundary —
  риск mis-parse/infinite-loop).
- **Checker** (`types/mod.rs:7533` Binary arm + новый ChainedComparison arm): для КАЖДОЙ relational-пары
  infer обе категории операндов, требовать mutually-ordered; reject `bool`/`unit`/`ptr`/incompatible
  cross-category. Диагностики: **`E_CMP_CHAIN_UNSUPPORTED`** («comparison operators cannot be chained»
  + fix-it на `&&`-split — fires ТОЛЬКО пока Ф.2 не активна, interim peer-parity gate);
  **`E_RELATIONAL_OPERAND_NOT_ORDERED`** для bool/ptr/unit (fix-it: «`==`/`!=` для равенства; ordering
  через `@compare`»). Добавить per-pair check и в plain Binary arm (одиночный `bool < int` ловится).
  Permissive-на-Unknown/Other сохранить (не ломать generics — выверить `cat_of` generic-scope).
- **Конец Ф.1: Nova уже = peers + Rust-диагностика.** Targeted-fixture per-fix; full nova test в конце фазы.
- **Commit:** `feat(plan150 Ф.1): chained-comparison hard-error + Rust-grade diagnostic + relational-operand check`

## Ф.2 — Desugar + interp + codegen: настоящий chaining (= option 3) (~1d)
- **Desugar** (`desugar.rs`, ChainedComparison arm): lower в `&&`-конъюнкцию с **hoisted temps** для
  КАЖДОГО среднего операнда (один eval): `let _cmp_b = <b>; let _cmp_c = <c>; (a OP1 _cmp_b) && (_cmp_b OP2 _cmp_c) && …`.
  Recurse desugar на операнды ПЕРЕД сборкой; assert: lowered-форма НЕ содержит ChainedComparison
  (guard desugar-loop). **Ownership/consume:** temps — readonly by-value reads (chain только сравнивает,
  не move'ит); desugar-unit-test: consume-set неизменен.
- **Interp** (`interp/mod.rs`): ChainedComparison eval = зеркало desugar (eval first, каждый средний —
  раз в слот, short-circuit `&&`). ИЛИ defensive `unreachable!` если desugar до interp в pipeline.
- **Codegen** (`emit_c.rs`): ожидается БЕЗ изменений (видит plain `&&` of bool-comparisons); defensive
  «ChainedComparison must be desugared before codegen». Pretty-print (`ast/pretty.rs`): arm на precedence-6.
- **FLIP Ф.1 gate:** ChainedComparison теперь ПРИНИМАЕТСЯ → `E_CMP_CHAIN_UNSUPPORTED` больше не fires на
  валидных chains; `E_RELATIONAL_OPERAND_NOT_ORDERED` ОСТАЁТСЯ.
- **Commit:** `feat(plan150 Ф.2): true chained comparison — single-eval desugar + interp + codegen accept`

## Ф.2-tests — Позитивные + негативные (~0.5d, И C-codegen И interp)
**Позитивные** (`nova_tests/plan150/`):
- `0 <= i < n`; mixed `a < b <= c`; chain в `requires`/`ensures` контракте.
- **SINGLE-EVAL** (load-bearing): `a < side_effect_counter() < b` → counter инкрементится РОВНО раз.
- **SHORT-CIRCUIT**: средний операнд с side-effect НЕ вычисляется когда первая пара false.
- chain + `&&`/`||` precedence (`a<b<c && d<e`).
- **REGRESSION security-дефекта:** chain в `requires 0 <= i < @len` РЕАЛЬНО ограничивает — нарушающий
  вызов trap'ится/reject'ится, НЕ проходит молча.

**Негативные** (точные коды):
- `true < false` → `E_RELATIONAL_OPERAND_NOT_ORDERED`; `bool < int` → то же.
- `"hello" < 1 < 5` → type-mismatch на провинившейся паре, **ВСЕ пары проверены** (не stop-on-first).
- `ptr`-relational → `E_PTR_ARITHMETIC_BANNED` (existing) всё ещё fires.
- **Историч. footgun** `assert(0 <= 100 < 10)` → конвертировать в regression-тест, который теперь
  корректно ПРОВАЛИВАЕТ assert (не вакуумен).
- **Commit:** `test(plan150): chained-comparison pos/neg (single-eval, short-circuit, contract-non-vacuous)`

## Ф.3 — Docs, migration, close (~0.25d)
- spec → D implemented; language-reference + contracts-guide: `requires 0 <= i < @len` как **канонический**
  bounds-формат (headline-выигрыш). Error-code-индекс: оба кода + примеры + fix-it.
- Migration-note: НЕ breaking на практике (старый `(a<b)<c` уже был runtime-error/vacuous, не легитимен —
  checker-scan подтвердил 0 реального `bool<bool`/`bool<int`); flag поведенч. изменение для кода,
  опиравшегося на вакуумный контракт. Опц. мигрировать verbose `lo<=mid && mid<=hi` → chained (low-pri, small diffs).
- project-creation.txt + simplifications.md + nova-private/discussion-log.md; закрыть Q35 +
  `[M-comparison-bool-operand-or-chaining]`; разгейтить `[M-140-bounds-as-contract]`.
- **Commit:** `docs(plan150 Ф.3): chained comparison reference + migration + close`

---

## Критерии приёмки
- **A1** — `a OP1 b OP2 c` (≥2 relational) desugars в short-circuit `&&` со средними операндами,
  вычисленными **РОВНО ОДИН раз** (тест с side-effect-counter).
- **A2** — `requires 0 <= i < @len` — **реальный** range-check (НЕ вакуумный): нарушающий вызов
  trap'ится/reject'ится (regression security-дефекта).
- **A3** — `bool` / `unit` / `ptr` как операнд `<`/`<=`/`>`/`>=` → `E_RELATIONAL_OPERAND_NOT_ORDERED`
  (а `==`/`!=` на bool — легально).
- **A4** — interim (до Ф.2): `a<b<c` → `E_CMP_CHAIN_UNSUPPORTED` + fix-it (peer-parity). После Ф.2: принимается.
- **A5** — `ptr`-relational → `E_PTR_ARITHMETIC_BANNED` (existing) сохраняется.
- **A6** — short-circuit: при false первой пары средний операнд (side-effect) НЕ вычисляется.
- **A7** — работает И в C-codegen (`nova test`) И в interp (`test-interp`); evaluation-order нормативен
  и протестирован на ОБОИХ путях.
- **A8** — pos+neg фикстуры релизным nova; 0 регрессий (basics + широкий sweep); `assert(0<=100<10)`
  теперь корректно FAIL.
- **A9** — spec D-блок + Q (resolved Q35) + error-code-индекс + docs обновлены.

## Зависимости / связь
- **Разблокирует `[M-140-bounds-as-contract]`** — после Plan 150 `requires 0 <= i < @len` валиден И
  не вакуумен → bounds-as-contract можно писать chained (или `&&`).
- **Резолвит Q35** + `[M-comparison-bool-operand-or-chaining]`.
- Plan 115 (ptr relational ban) — reuse `E_PTR_ARITHMETIC_BANNED`. Plan 85.4 (@compare protocol).

## Риски (из design-review)
| # | Риск | Mitigation |
|---|---|---|
| R1 | Parser precedence / infinite-loop (`a == b < c` mis-parse) | grammar-тесты ПЕРВЫМИ; lock boundary в Ф.0 spec |
| R2 | Scope-creep (2 фичи в 1) | фазирование: Ф.0-Ф.1 = shippable option-1; Ф.2 — отдельный gate/commit |
| R3 | Consume/ownership-регрессия в desugar-temps | desugar-unit-test: consume-set неизменен; инспекция emit_c temp-lifetimes |
| R4 | Bootstrap-permissive конфликт (checker «Permissive на Other») | check fires только на definitively-known (bool/ptr/unit + concrete mismatch); Unknown/Other permissive |
| R5 | Evaluation-order нормативность (interp vs C divergence) | Ф.0 делает нормативным; Ф.2-tests на ОБОИХ путях |
| R6 | Direction-mixed (`a<b>c`) | решить в Ф.0 (рекоменд. allow per-pair); Rust-strict → lint, не parse-error |
| R7 | Регрессия в корпусе (смена parse-tree для ≥2 сравнений) | full regression (nova test + test-interp) в конце Ф.1 и Ф.2; scan подтвердил 0 легитимного usage |

## Связанные D-блоки
| D | Что |
|---|---|
| (новый, Ф.0) | Chained comparison grammar + semantics (single-eval, short-circuit) + relational-operand ordering requirement |
| D? | AMEND — relational-операнды: bool/ptr/unit запрещены (`==`/`!=` на bool остаётся) |
| Plan 115 | reuse `E_PTR_ARITHMETIC_BANNED` (ptr relational) |

## Статус
📋 **PLANNED** — решение принято (design-workflow + adversarial-judge 2026-06-13: hard-error-plus-chained,
фазированно). Готов к выполнению. Ф.0-Ф.1 — shippable peer-parity; Ф.2+ — эргономика.
