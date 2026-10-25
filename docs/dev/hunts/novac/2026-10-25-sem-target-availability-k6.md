<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Охота: применимость неявно выбранной callable row — sem × К6

КЛЕТКА | sem | К6

## 1. Находки

Охота назначена интегратором по разрешению владельца 2026-10-25 на isolated
gate repair 092. Предмет — sem candidate на task `dfd30c69e` + main
`a2c52edd8` до исправления №1910. Только source-query и ограниченная алгебра:
Nova/Oracle, emit/native/runtime не запускались. Три пробы повторены интегратором,
все exit 0; этот ноль подтверждает свидетелей дефекта, не исправность языка.

**Один корень, три формы потребления, реестр №1910.** D495
(`spec/decisions/07-modules.md:2220–2229`) требует, чтобы неактивная callable row
не участвовала в разрешении и эмиссии. `FnIndex.fill` (`resolve.nv:267–279`)
исключает её, но прежний `sem/coerce.nv:73–77` возвращает `via_row` только по
двум TyId; регистрация пары (`:401–409`, own collect `:632–643`, handed harvest
`:212–220`) не фильтрует target. `compiles_body` — только own-boundary.

НАХОДКА | К6 | sem | coerce-return | return consumer записывает own inactive row как coercion/wanted при валидных TyId и допустимом receiver mode (`check/return_rules.nv:342–365`); та же row отсутствует в обычном FnIndex
НАХОДКА | К6 | sem | coerce-argument | active free callee получает inactive conversion в SlotBind (`check/calls.nv:320–326,385–387,417`); тот же корень, не второй дефект
НАХОДКА | К6 | sem | forms-consumers | typed field/payload принимает Some(inactive row) как отсутствие mismatch (`check/numeric_positions.nv:87–95`); известная отдельная missing-emission запись №1755 новой находкой не считается

**Зазор:** `pipeline/cfg_fn_test.nv` проверяет прямые функции, методы, static,
export, generic instance и own/imported shadowing, но не `#coerce`.
`check-novac-row-fields.py` держит поля, `check-novac-arch-class-proofs.py` —
текстовые proof markers; ни одно не устанавливает applicability inserted row.

Пробы: `probes/2026-10-25-sem-target-availability-k6/<имя>/cmd.sh <корень>`.
Каждая самостоятельна, читает только относительные пути; потеря якоря краснит
assertion, не превращается в зелёный ноль. Исходниковые hashes до исправления
записаны в `source-pins.json`. После class-fix пробы исходного bypass должны
перестать подтверждаться: не ослаблять их ради зелени. Новый поведенческий
контроль — `spec_tests/conformance/standalone/target_os_coerce_class_1910.nv`.

## 2. Что обошёл и почему

* Free/method/static/generic/handed/export сохраняют общую маску и обычный индекс;
  нового обхода на этих обычных путях не найдено, полнота компилятора не заявлена.
* Aliases сводятся к underlying declaration (`collect.nv:852–864`), import renames
  отдельно отвергаются. Extern и native ABI не проверялись.
* Readonly protocol satisfaction использует FnIndex. Handed overload counters
  оставлены следом: target/ABI совместимость не измерена, находка не присвоена.
* Нет исполняемого Nova carrier, miscompile/link/runtime вывода; Python-модели
  не парсят Nova и не заменяют end-to-end fixture. №1899, F110/reg1900 и известные
  долги не переоткрывались. Protected W не читался и не менялся охотником.

## 3. Противоречия

Отдельных нормативных противоречий не установлено. Расхождение требования
D495 и двух исходниковых дверей названо выше; другую норму не выбирали.
