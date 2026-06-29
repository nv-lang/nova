<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# int-collapse: pending discriminating tests (7 РЕАЛЬНЫХ багов) — roadmap к celevой форме

> Источник: ultracode-workflow `wf_4bfb1f93-809` (13 surface-агентов, grounded чтением codegen,
> 2026-06-30). 6 поверхностей оказались int-collapse-SAFE → залочены в conformance (commit 9536f5e3,
> D403/404/406/407/409/411). ОСТАЛЬНЫЕ 7 — РЕАЛЬНЫЕ int-collapse баги (тесты ЗДЕСЬ, НЕ в conformance —
> они RUN-FAIL/CC-FAIL и сломали бы зелёный CU). Восстановить как локи ПОСЛЕ фикса.

Эти тесты ПОДТВЕРЖДАЮТ директиву владельца «примитивные числа схлопываются в int — в корне неверно»:
int-collapse **ПОВСЕМЕСТЕН**, не «почти обработан». named-priority (uint≠int, u8≠i8≠i16≠int).

## 7 багов с КОРНЯМИ (file:line из grounded-анализа агентов)

| D | поверхность | исход | КОРЕНЬ |
|---|---|---|---|
| **D401** | `match`-как-значение | RUN-FAIL ✓ | **МИС-ФРЕЙМ агента + ИСТИННЫЙ корень (spec-уточнено):** тест-литерал `0x8000000000000000` БЕЗ uint-контекста = `int` сверх i64.MAX → по спеке (02-types.md:969 + 03-syntax.md:9396) ДОЛЖЕН быть **`E_LIT_OUT_OF_RANGE`** compile error, НЕ «preserve uint». Текущее молчаливое заворачивание в i64::MIN = §1/§4-баг (нарушает спеку). **Фикс = checker-диагностика** на wide-default int-литерал > i64.MAX (range-check сейчас fires только при sized_int_name=Some). ШИРОКИЙ blast-radius → ПОЛНЫЙ регресс (§7). Тест переделать в NEG (EXPECT_COMPILE_ERROR) ЛИБО дать uint-контекст. ОТДЕЛЬНО: match/if-value-С-контекстом coercion (Some/tuple-класс) — не покрыто этим тестом. |
| **D413** | `if`-как-значение | RUN-FAIL | ТОТ ЖЕ (мис-фрейм + overflow-not-caught). Фикс = тот же checker range-check на wide-default int > i64.MAX. |
| ~~D410~~ | ~~tuple-destructure~~ | ✅ **ЗАКРЫТО** | ~~аннотация `(uint,uint)` отброшена~~ → ФИКС [M-172.1-tuple-destructure-annot]: `emit_tuple_destructure` принимает `decl.ty`, эмитит элементы через `emit_expr_with_target_type` (коэрсия литерала к declared-элементу, НЕ fallback). Lock в conformance. |
| ~~D412~~ | ~~default-arg + const~~ | ✅ **ЗАКРЫТО** (4 кейса) | ФИКС [M-172.1-default-arg-typed]: callnorm desugar default-arg → `let_stmt_typed(param.ty)` (Let-путь коэрсит через target-type) + [M-172.1-const-binary-typed]: `emit_const_expr_typed` Binary-арм пропагирует target в арифм. операнды. Lock в conformance. |
| **D405** | mixed-width compare | RUN-FAIL ✓ (скомпилён+прогнан) | Binary lt-wins (emit_c.rs:36883 «оба typed — берём lt») усекает к ЛЕВОМУ операнду → `u8+u16`≠`u16+u8` (СЛОМАНА КОММУТАТИВНОСТЬ). Корень = arith-result-width берёт left, не wider. |
| **D402** | closure-return | MIXED | light-closure (без аннотации) param/ret дефолтят `nova_int` (fn_param_sigs default `["nova_int"]`, :18935). |
| **D408** | option-chain | MIXED (CC/RUN-FAIL) | adapter-chain receiver (MethodCall) НЕ имеет resolved_types-аннотации → degraded `infer_method_level_return_for_sum` (:34917) → `U` биндит nova_int → внешний адаптер коллапсит. method-chain receiver inference gap. |

## МЕТА-КОРЕНЬ и ЦЕЛЕВАЯ ФОРМА

5 из 7 (D401/D413/D410/D412 + часть D402) = **ОДИН мета-корень**: bare integer-литерал дефолтит в
signed `nova_int`, а declared/target sized-тип НЕ пропагируется в value-позицию (match/if-как-значение,
tuple-destructure, default-arg, const-binary). Это **literal-coercion канал** (§0 целевая форма):
чекер материализует COERCED-тип литерала (D55) в `resolved_types` во ВСЕХ позициях, codegen IntLit
ЧИТАЕТ канал. СУБСУМИРУЕТ tactical Some/Coalesce-фиксы + грызёт ≈103 `nova_int`-fallback долг (HARD-AC).

D405 (width-pick lt→wider) и D408 (chain-receiver inference) — ОТДЕЛЬНЫЕ корни, но тоже в legacy-infer.

**SPEC-вопрос РЕШЁН (D401/D413, no-context):** свериться со спекой дало ответ — `0x8000000000000000`
БЕЗ uint-контекста = `int`-литерал сверх i64.MAX → **`E_LIT_OUT_OF_RANGE` compile error**
(02-types.md:969 «Выход за диапазон — compile error E_LIT_OUT_OF_RANGE»; 03-syntax.md:9396-9398
примеры). Текущее молчаливое заворачивание (i64::MIN) НАРУШАЕТ спеку (§1/§4). Фикс = checker range-check
на wide-default int-литерал > i64.MAX (сейчас :9594 fires только при sized_int_name=Some). ШИРОКИЙ
blast-radius → §7 полный регресс; не marathon-end. Эти 2 теста переделать в NEG или дать uint-контекст.

## Прогресс закрытия (2026-06-30)
Из 7 багов: **2 ЗАКРЫТО** — D410 (tuple-destructure, 4bfa4ce8), D412 (default-arg+const, da934613).
**ОСТАЛОСЬ 5:** D401/D413 (overflow-not-caught — checker range-check, §7-широкий), D405 (mixed-width
lt-wins — checker promote_arith_rt pick-wider), D402 (light-closure defaults), D408 (option-chain
receiver inference). + ранее (вне 13): Some-literal (a0a2a2dd), Coalesce-Result (62c0f57f). Итого
**4 int-collapse фикса** эту сессию + 6 локов. Остаток = checker-изменения (регрессо-опасны, §7) +
complex (closure/chain) → fresh-focus.

## Off-topic находки (НЕ int-collapse, отдельно)
- **C-keyword-as-identifier**: `ro inline = …` → codegen эмитит `nova_uint inline = …` = невалидный C
  (`inline` — C-keyword). Codegen НЕ экранирует C-keywords в идентификаторах. (Обойдено в D407: `inl`.)
- **D411 latent**: `[N]uint` литерал строит `Nova_Vec____nova_int` (hint_overrides_int :32025 ОМИТит
  `nova_uint`), bit-recovered на read → PASS, но латентный долг.
