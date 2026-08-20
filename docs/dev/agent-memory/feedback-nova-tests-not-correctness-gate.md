---
name: feedback-nova-tests-not-correctness-gate
description: nova_tests СЛОМАН — его pass/fail НЕ гейт корректности; только byte-identical baseline; свои тесты в detect172/
metadata: 
  node_type: memory
  type: feedback
  originSessionId: b6f9282a-da61-4413-ac88-f82ea6a9f472
---

`nova_tests/` помечен **сломанным** (pre-existing CC-FAIL/CODEGEN-FAIL). Его absolute pass/fail — **НЕ источник правды по корректности** и НЕ гейт. Использовать ТОЛЬКО как byte-identical baseline (снять `.c` чистым бинарём → рефакторить → сравнить line-multiset `.c` + pass/fail-delta на ТОМ ЖЕ бинаре). Свои pos/neg-тесты каждой задачи — в `detect172/` (на ветке plan-172; neg — отдельный `neg/` folder-module, `--compile-error`).

**Корректировка владельца (2026-06-20):** в 172.1 U.3.1 я подал «~78 dirs nova_tests pass/fail delta=0» как ГЕЙТ behavior-change — это рискованный неверный framing (corpus сломан). Поломки фактически не было (изменение монотонно: только +код ошибки; звучно по построению), но опираться на сломанный corpus как на correctness-гейт нельзя.

**Why:** сломанный corpus даёт шумный pass/fail; count-сравнение может маскировать регрессию (если не монотонно). Owner явно маркировал его «не чинить, не доверять».

**How to apply:**
- byte-identical правка → nova_tests годится как baseline (multiset-`.c`=0 + pass/fail-delta=0 vs чистый бинарь, см. [[feedback-codegen-dce-verification]]).
- behavior-change (не byte-identical) → ГЕЙТ = свои `detect172/` pos+neg + аргумент звучности (напр. монотонность / construction); nova_tests максимум supporting-выборка, НЕ гейт.
- Не утверждать «0 регрессий» по сломанному corpus; полный регресс для claim'а — по §6.6, но на сломанном corpus его pass/fail интерпретировать осторожно. См. [[feedback-large-tests-stored-not-in-regress]].

**Доп. урок (2026-06-27, U.4.5): byte-identical-to-legacy — НЕВЕРНЫЙ гейт; legacy багует.** Владелец дважды поправил: «не думаешь, что в легаси были ошибки?» + указал docs/plan-169.2-compiler-fixes.md. Там фиксы #5/7/8/9/10/18 — ВЕСЬ класс багов legacy `infer_expr_c_type`: name-keyed side-tables (`fn_ret_<name>`/`array_element_types`/`tuple_element_types`/`var_types`) last-wins/протекают между peer-файлами folder-модуля; чинились по-одному (band-aid на re-derive = §0-анти). Канал (`resolved_types` per-ExprId) убивает класс by construction. Баг #23 (пустой `[]fn`→nova_int element→SEGV) ОТКРЫТ, гейт U.4 `[M-172-nova-int-fallback-audit]`. **Урок:** я переусердствовал с byte-identical-to-legacy как гейтом U.4.5 — это (а) консервировало бы баги legacy (б) over-reject'ило звучные channel-фиксы. Правильный гейт = SOUNDNESS (detect172 + 0 CC-FAIL + тесты + clean-baseline-DELTA по §7.5), НЕ byte-identical. Он ловит регрессии (gap#2 lru=CC-FAIL) и принимает benign/fix-различия. Конвенция §0/§1 это и говорит (legacy re-derive + `_=>nova_int` fallback = soundness-дыра к удалению). **How to apply:** при флипе арма на канал — гейт soundness, НЕ «.c совпал с legacy»; дивергенции от legacy могут быть ФИКСАМИ (проверь, не предполагай регрессию).

**Доп. урок (2026-06-20, U.3.1):** аргумент «звучно по построению» / прогон корпуса НЕ заменяют тщательные edge-тесты в `detect172/`. В U.3.1 helper считал arity-mismatch type-mismatch'ем → false-positive на multi-param/0-arg overload; «звучность по построению» была НЕПОЛНА (пропускала arity), корпус ~78 dirs тоже пропустил (нет такой формы), поймал ТОЛЬКО multi-param тест в detect172 (по запросу владельца). Урок: для overload/arg-проверок ОБЯЗАТЕЛЬНО покрывать multi-param, разную арность, частичный матч (1-й арг ок, 2-й Bad), не только single-param. См. [[feedback-test-conventions-strict]].
