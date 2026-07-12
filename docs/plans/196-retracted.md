<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196-retracted — тупики и мисдиагнозы миграции «одного окна»

**Родитель:** [196](196-one-truth-closeout.md). **Назначение:** архив РЕТРАКТИРОВАННЫХ заходов + промежуточных
МИСДИАГНОЗОВ, чтобы активные планы (196/196.2/196.3) оставались чистыми и forward-looking, а ошибки не
повторялись. **НЕ рабочий план** — только запись «что пробовали и почему отбросили».

## 1. Ф.4a/4b/4c — co-authority solver (~месяц, ТУПИК, ретрактирован)

Построен `constraint_solver.rs` (Join/Project/Resolve) в режиме **verify-and-discard** («проверь legacy →
выброси свой результат», `let _ = channel`). **Провал:** ничего не кладёт в `resolved_types` → **0 армов снято**;
solver оказался подмножеством-верификатором (резолвит лёгкое, `None` на class-C). Корень ошибки процесса: гнал
byte-parity-зелёные волны как «фундамент», НЕ меряя реальное удаление legacy. **Закрыто амендментом конвенции
§0/§7** (гейт-прогресса + спайк-на-авторитет, `b7a45bf7a`). **Салвэдж:** примитивы `constraint_solver.rs`
переиспользуемы ВНУТРИ class-C резолвера чекера при ResolvedType-native. Урок: verify-and-discard ≠
materialize-and-delete.

## 2. «ExprId-across-mono = блокер» (МИСДИАГНОЗ, снят A-спайком 2026-07-12)

B07-спайк заключил, что iterator-adapter-residual требует ExprId-стабильности сквозь mono (тяжёлая инфра, почти
MIR). **A-спайк ОПРОВЕРГ:** mono УЖЕ сохраняет template-ExprId (`body.clone()`, ExprId=Copy, renumber нет).
Настоящий root iter/map-residual: `[]T.of` не в списке резолва slice-ctor (`mod.rs:13077`) + латентный
`NovaOpt from int` consumption-баг. **MIR НЕ нужен.** Оба реально починены (of + `infer_lambda_return_type_with_params`
protяжка блок-let) → первое gate-1 снятие −17.

## 3. «B07 = первый-атом / carrier» (ПЕРЕ-СКОУПЛЕН)

План изначально скоупил B07 как несущий carrier / первый W1-атом. B07-спайк нашёл: B07 — chained-residual
CONSUMER; несущая chained-протяжка УЖЕ построена (`infer_method_call_channel_type` рекурсивно несёт generic
ResolvedType). Далее уточнено: снятие iter/map упирается не в carrier, а в `of`/`NovaOpt-from-int` (см. п.2).
Gate-1 достигнут через отделяемый B11w, а не через «B07-carrier».

## 4. «Координаты cb устарели на current-main» (ОПАСЕНИЕ, снято P0)

Пред-W1 боялись, что cb-координаты 114-реестра не совпадут с current-main (нужен remap). **P0 подтвердил:**
`infer_call_ret_c` region-diff cb↔current-main = ТОЛЬКО trace-строки, `main-строка == cb-строка` для ВСЕХ 114 →
координаты валидны, remap не нужен.

## 5. «3 тяжёлых куска неизвестной осуществимости» (пред-W1 гейтинг, СНЯТО спайками)

Пред-W1 P2/P3 framing: mono-registration emit-пасс + class-C ResolvedType-native + B07-спайк — как «тяжёлые куски
неизвестной осуществимости». **Спайки сняли:** mono-registration УЖЕ есть (`resolved_type_to_c→resolved_named_to_c`);
class-C механизм работает (0 lowering-err на корпусе); TypeParam-протяжка на месте. Осуществимость доказана —
осталась инженерия снятия (волна-1) + миграция сиблингов (волна-2).
