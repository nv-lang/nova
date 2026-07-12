<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — «Одна правда»: удалить второе окно `infer_expr_c_type`

**Статус:** 🔥 IN PROGRESS, **КУРС-КОРРЕКТИРОВАН 2026-07-12**. **Приоритет:** P0 (ключевая
идея 172-186). **Умбрелла над:** 172.1 (U-хвосты), 172.12, 172.13. Координирует, НЕ дублирует.

---

## ⚑ Курс-коррекция (2026-07-12) — читать ПЕРВЫМ

Честная запись, чтобы ошибка не повторилась.

**Что пошло не так (~месяц):** заход Ф.4a/4b/4c (**co-authority solver**) — ТУПИК, ретрактирован.
- Построил `constraint_solver.rs` (Join/Project/Resolve), работающий в режиме
  **«проверь legacy → выброси свой результат»** (`let _ = channel`).
- Почему провал: verify-and-discard **ничего не кладёт в `resolved_types` → 0 армов снято**.
  Solver оказался **подмножеством-верификатором** (Ф.4c negative): резолвит
  только лёгкое, воздерживается (`None`) на class-C. Флип на авторитет удаляет ~0 (доказано).
- **Корень ошибки процесса:** гнал byte-parity-зелёные волны как «фундамент», НЕ меряя РЕАЛЬНОЕ
  удаление legacy. Закрыто амендментом конвенции §0/§7 (**гейт прогресса** + **спайк-на-авторитет**,
  коммит `b7a45bf7a`).

**Что было и остаётся ВЕРНЫМ:** направление §0 не менялось — свести всё в `resolved_types`
(одно окно, D315) → удалить `infer_expr_c_type`. **Ф.1/Ф.2 сделали это правильно (21 арм).**
Продолжаем ИМ, а не co-authority.

**MIR вынесен из 196.** Полноценный typed-IR/MIR — ОТДЕЛЬНАЯ будущая цель (borrow-check/оптимизации).
§0 его **НЕ требует**: `ResolvedType` уже C-lossless (см. «Грунтовка» ниже),
достаточно **полноты** `resolved_types`. MIR = Стадия 2, отдельный план, открывается ПОСЛЕ §0.

---

## Проблема — два консолидированных окна (не «каша»)

- **Окно 1 — `resolved_types`** (`HashMap<ExprId, ResolvedType>`, чекер, D315): намеченное одно
  окно, но **НЕПОЛНОЕ** (дыры `[M-104.10-expr-types-coverage]`: generic method-chain returns,
  non-primitive TupleLit/RecordLit, UNSET-desugar-узлы).
- **Окно 2 — `infer_expr_c_type`** (`emit_c.rs:48885`, диспетчер, **249 консумеров**) + его 6z-делегат
  `infer_call_ret_c` (`emit_c.rs:46293-48883`, **2591 стр — ОСНОВНАЯ масса**; ОТДЕЛЬНАЯ сиблинг-функция,
  НЕ тело `infer_expr_c_type`):
  codegen ПЕРЕвыводит C-тип там, где канал пуст. Каналы 1-6 читают чекер; **Канал 6z** (44 арма +
  `infer_call_ret_c` 2591 стр) = legacy-перевывод, срабатывает как **fallback** когда `resolved_types`
  пуст. Главная масса 6z-кода — class-C generic-mono.

Расхождение окон → §0-баги (мис-диспатч, `nova_int`-затычка, тот самый nova build ICE). Цель:
**дополнить `resolved_types` → удалить `infer_expr_c_type`.**

## ★ Матрица «одного окна правды» — полный §0-scope (директива владельца 2026-07-12)

«Одно окно» = для КАЖДОЙ клетки (форма-функции × аспект) ОДИН механизм резолвит в чекере, codegen лоуэрит.
Второго окна нет, дрейфа нет. **114 веток `infer_call_ret_c` = ТОЛЬКО строка «возврат»; матрица ШИРЕ 196.2.**

**Формы функции (столбцы):** static · generic-static · static+кросс-модуль · generic-static+кросс-модуль ·
приватные методы · методы+кросс-модуль. (Кросс-модуль и generic — ортогональные усложнители; хардест-клетка
= **generic-static + кросс-модуль**.)

**Аспекты (строки; каждая = одно окно ПО ВСЕМ формам):**

| # | Аспект | Окно (механизм) | В 114? | Статус |
|---|---|---|---|---|
| 1 | Резолв функции → `FnDecl` | чекер-резолверы + `external_registry` (кросс-мод) | нет | ⚠ generic-static+кросс-мод неполон |
| 2 | Тип ВОЗВРАТА | `resolved_types` ← 114 веток | **ДА = 114 (196.2)** | 🔄 W1 идёт |
| 3 | Аргументы (arg↔param) | `callnorm`/`argbind` | нет | ⚠ зависит от (1) |
| 4 | Generic-аргументы (вывод type-arg) | generic-инференс чекера + `callnorm` | нет | ⚠ gaps (generic-static не пробрасывает type-arg; `[M-153-vec-of-variadic]`) |
| 5 | Default-арги | `callnorm` backfill (`:485`) | нет | 🔴 generic-static+кросс-мод (`[M-vec-new-cap-default-arg-backfill]`, чинится) |
| 6 | Generic default-арги | пересечение (4)+(5) | нет | 🔴 `Vec.new(cap int=0)` — чинится + регресс-тест на пересечение |

**Строки 3-6 (args/generic-args/defaults/generic-defaults) = сиблинг-окно `callnorm`/`argbind`, НЕ в 114**, и
ВСЕ зависят от строки 1 (резолв): без `FnDecl` нет ни аргументов, ни default'ов. **Приватные (форма)** —
checker-контроль доступа (D267/D281), не codegen-ветка.

**Программа «одного окна» = ЧЕТЫРЕ сходимости, не одна:**
- **A. Возврат** → **196.2** (114 → `resolved_types`). Идёт (W1).
- **B. Резолв→`FnDecl`** для generic-static/кросс-модуль → сходимость на ОБЩИЙ резолвер (`external_registry`
  ≠ отдельный путь). Vec.new-баг вскрыл начало: резолв generic-static кросс-мод не доходит до `FnDecl`.
- **C. Args / default / generic-default** → сходимость `callnorm`/`argbind` по ВСЕМ формам. Vec-фикс = первая
  клетка (default × generic-static × кросс-мод).
- **D. Приватные** → checker-доступ (D267/D281), отдельно.

**Приёмка «одного окна» = вся сетка (форма × аспект) зелёная:** каждая клетка резолвится ОДНИМ окном; на
КАЖДОЕ тяжёлое пересечение — conformance-тест red-до/green-после (особенно generic-static+кросс-модуль ×
{args, generic-args, default, generic-default}). **196.2 (114) — строка 2 (одна из шести). B/C/D — НЕ отдельные планы, а ФАСЕТЫ 196** (директива владельца
2026-07-12: **`callnorm.rs`/`argbind.rs` ОБЯЗАНЫ быть частью 196**). 196 = «одно окно» по ВСЕЙ матрице, не
только возврат:
- **Ф.A — Возврат** = 196.2 (114 → `resolved_types`). Идёт (W1).
- **Ф.C — `callnorm`/`argbind`-сходимость** (args / generic-args / default / generic-default по ВСЕМ формам)
  — В SCOPE 196. Первый экземпляр = generic-static default-backfill фикс (`[M-vec-new-cap-default-arg-backfill]`,
  идёт [sonnet, nova-p200], root-резолв, не заплатка); `Vec.new(cap)` API (план 200) — лишь ПОТРЕБИТЕЛЬ фикса,
  сам фикс = 196 Ф.C.
- **Ф.B — Резолв→`FnDecl`** (generic-static/кросс-модуль на ОБЩИЙ резолвер, `external_registry` ≠ отдельный
  путь) — В SCOPE 196.
- **Ф.D — Приватные** (checker-доступ D267/D281) — В SCOPE 196.
Матрица зелёная (A+B+C+D) = §0 «одно окно» выполнено. 196 — НЕ только «удалить `infer_expr_c_type`», а вся сетка.

### ★ Инвентарь «второго окна» (греп 2026-07-12) — это СЕМЕЙСТВО, не функция

**Корень:** типизированного IR НЕТ (AST единственный) → у чекера нет канала донести резолв/типы до codegen →
codegen ПЕРЕвыводит, каждый по-своему → дублирование путей + дрейф (§0-баги) + **LSP слеп** (инференс codegen-only).
В `emit_c.rs`: **~20+ `infer`/`resolve`-функций + 56 `_c_type`/`_to_c`-сайтов.** Цели удаления (логика → ЧЕКЕР;
codegen только лоуэрит через `resolved_type_to_c`; LSP читает ТЕ ЖЕ каналы → hover/completion получают типы):
- **Возврат/тип (Ф.A):** `infer_expr_c_type`(48885), `infer_call_ret_c`(46293, 114), `infer_expr_c_type_str`,
  `infer_mono_method_ret(_with_args)`, `infer_method_level_return_for_sum`, `infer_static_method_ret`,
  `infer_generic_static_ctor_ret`, `infer_lambda_return_type_with_params`, `infer_trailing_block_sig`,
  `resolve_result_option_ret`, `resolve_result_te(_strict)` + 56 `_c_type`.
- **Generic/type-param (Ф.B/C):** `infer_type_param_binding`×3, `infer_protocol_structural_binding`,
  `infer_result_type_params`, `resolve_method_level_subst`, `resolve_mono_type_args`, `compute_array_elem_type_for_obj`.
- **Args/default (Ф.C):** `callnorm.rs`, `argbind.rs`.
- **Резолв/хардкод (Ф.B):** `external_registry`, `primitive_instance_method_known` (§3 хардкод-зеркало), method/static-резолверы.
- **Legacy-лоуеринг:** `type_ref_to_c` (ретайр D315, дублирует `resolved_type_to_c`).

**Финал 196** = это семейство удалено/сведено к `resolved_type_to_c(resolved_types[id])` + каналы кормят LSP
(hover/completion). Это НЕ «пара веток», а систематическая переархитектура потока типов/резолва — БЕЗ полного
MIR (mono остаётся lazy в codegen; каналы лишь дотягиваются до mono-копий, см. Ф.A / A-спайк).

## Зачем `resolved_types` лучше `infer_call_ret_c` (мотивация — не «оно и так работает»)

**История:** `infer_call_ret_c` (codegen-перевывод) ИСТОРИЧЕСКИ был ЕДИНСТВЕННЫМ окном — до §0/172 не было
checker-канала типов, codegen сам всё выводил. Но это одно окно в **НЕПРАВИЛЬНОМ слое** (codegen — слишком
поздно, пер-инстанс, LSP-слеп). `resolved_types` (172/D315) добавлен как **правильно-слойная ЗАМЕНА** — правда
переехала в ЧЕКЕР. Сейчас оба существуют (переходный §0-долг); миграция ДОДЕЛЫВАЕТ переезд + удаляет старое.
**Улучшать `infer_call_ret_c` на месте §0 НЕ решает** (останется в codegen). Почему ЧЕКЕР (`resolved_types`)
лучше codegen-перевывода:
1. **Одна правда, не две** → окна не расходятся → нет §0-багов (nova build ICE, 13 P67-ICE в nova_tests,
   мис-диспатч, `nova_int`-затычка). Два окна ДРЕЙФУЮТ: фикс в одном живёт багом в другом.
2. **Резолв ОДИН раз** (чекер, generic-шаблон), не пер-инстанс на КАЖДОЙ mono в codegen → меньше работы (§2).
3. **Чекер кормит И codegen, И LSP/IDE** → hover/completion видят типы. `infer_call_ret_c` codegen-only → LSP слеп.
4. **Чекер — авторитет:** резолвит ИЛИ чистая диагностика (§4). `infer_call_ret_c` гадает (`nova_int`) или
   паникует (P67-LEGACY) на непокрытом.
5. **Чистые диагностики** (§6) vs утёкший CC-FAIL/паника пользователю.
6. **Поддерживаемость** (§0/§10): одно место vs два (дрейф = баги).

Итог: `infer_call_ret_c` работает, но как второе окно — ценой §0-багов, слепого LSP, двойной работы и гаданий.
`resolved_types` убирает это, будучи единственным авторитетом. **Это оправдание всей 196-работы.**

## Стратегия — strangler-fig (перенос со страховкой), НЕ verify-and-discard

Структура УЖЕ правильная: `resolved_types` (Ch2) читается ПЕРВЫМ, `infer_call_ret_c`/6z — fallback.
Работа по одному случаю:
1. **ПЕРЕНЕСТИ** резолюцию из `infer_call_ret_c`/6z в чекер → **МАТЕРИАЛИЗОВАТЬ** в `resolved_types`.
2. **УДАЛИТЬ** обработку этого случая из 6z. Сжимающийся `infer_call_ret_c` держит непере­несённое рабочим.
3. **Проверка встроена:** удалил случай → вывод byte-identical (conformance 95/0) → канал корректно
   заменил legacy. Разошёлся → чинишь резолюцию чекера (fallback ловит).
4. 6z опустел → **удалить `infer_expr_c_type`.** Одно окно.

Отличие от co-authority ровно одно: **материализуем в канал и удаляем**, а не проверяем-и-выбрасываем.

## Фазы

**Сделано:**
- **Ф.1/Ф.2 ✅** — 21 арм снят (материализация + delete): литералы, As, empty-sum, Match/RecordLit-дубли,
  TupleLit-элементы (concrete_value_named). Byte-parity, conformance 95/0.
- **Ф.4a/4b/4c ❌ РЕТРАКТИРОВАНО** (co-authority, 0 снято — см. курс-коррекцию). **Салвэдж:** примитивы
  `constraint_solver.rs` — частично переиспользуемы ВНУТРИ class-C резолвера чекера, при условии
  **ResolvedType-native** (обойти лоссовый round-trip `Ty↔TypeRef`, P1 арх-карты). Обвязка co-authority мертва.

**Стадия 1 — добить `resolved_types` (§0, byte-parity, strangler-fig):**
- **Ф.S1a — лёгкое (class-A/B, ~36 армов).** Где чекер умеет/почти умеет — материализовать в
  `resolved_types` (`f1_expr_inner`, `resolved_types_buf.insert`) → удалить арм: TupleLit/RecordLit
  non-primitive, As, Is, IfLet, SelfAccess, Unary, Match, Coalesce, Ident. **Граница с 196.2:** тут `Ident`
  = простой expr (variable/const-ref); `Ident`-как-Call-callee (free-fn/variant/closure — E-группа) =
  собственность 196.2 (W2/W4), НЕ дублировать. **[ИДЁТ — worktree `nova-p196`, гейт-1 на убывание строк].**
  Может выделиться в подплан `196.1`.
- **Ф.S1b — тяжёлое (class-C; `infer_call_ret_c` = основная масса второго окна).** `infer_call_ret_c`
  (2591 стр, ОТДЕЛЬНАЯ функция, вызывается из 6z-ветки `infer_expr_c_type`) = **class-C резолвер,
  живущий в codegen.** РЕЛОКАЦИЯ его логики в чекер: чекер резолвит generic-method-chain returns
  **ДО mono** → `ResolvedType` (с `TypeParam`) → материализует; mono подставляет позже. По одному
  под-случаю, fallback-страховка на каждом. **Подплан `196.2`.** Долго, но стратегия та же безопасная.
- **Ф.S1-финал — удалить `infer_expr_c_type`.** 6z опустел → функция схлопывается в
  `resolved_type_to_c(ir.type_of(expr))` либо call-sites зовут напрямую и функция сносится. Второго окна нет.
- **Хвосты внутри Стадии 1:** CI-линт raw-decode invariant (было «0 raw `Nova_`/`____`-decode вне
  `debt_`», дрейфануло до 12 вне debt — восстановить + под CI); снос `ResolvedType::Raw`; wildcard →
  ГРОМКИЙ panic (D368) — возможен ТОЛЬКО когда class-C достроен и wildcard недостижим.

**Стадия 2 — MIR (в-full, ОТДЕЛЬНЫЙ будущий план, вне 196):**
- Только для borrow-check / SSA / DCE / оптимизаций. НЕ §0. Извлечь mono в пре-пасс + построить CFG +
  переселить control-flow и concurrency-lowering на MIR. Раскладка по 129-структуре (`codegen/c/`,
  `mir/` — ≤5k строк/модуль). Открывается ПОСЛЕ закрытия §0.

## Гейты (КАЖДАЯ волна)

- ★ **ГЕЙТ-1 ПРОГРЕССА (конвенция §0, ОБЯЗАТЕЛЕН):** `wc -l` тела `infer_expr_c_type` ДО/ПОСЛЕ —
  **строго убыло** (в КОД-строках, не сырой счёт). **NB: во время 196.2 убывает `infer_call_ret_c`
  (делегат), а НЕ `infer_expr_c_type` (диспетчер почти неподвижен до W6-финала) — гейт-1 меряет
  `infer_call_ret_c`; сам `infer_expr_c_type` схлопывается на финале.** 0 снято = **КРАСНАЯ** волна →
  стоп + переоценка. Никакого «фундамента».
- ★ **ГЕЙТ-2 спайк-на-авторитет (§7.14):** для ДОКАЗАННОГО class-A/B (мех-ка Ф.1/2) **НЕ нужен**
  (перенос сам авторитетен, fallback + byte-parity доказывают). **ДЛЯ class-C ОБЯЗАТЕЛЕН** — несущая
  способность (чекер резолвит generic-method-chain returns в generic `ResolvedType`, неся его по цепочке
  в обход §4-лосса) НОВАЯ, неизвестной осуществимости: **B07 = мандатный спайк (196.2 P3)** с жёстким
  стоп-условием ДО стройки CAP-A/B/C.
1. **byte-identical** emitted-`.c` vs clean baseline (same-binary control отделяет
   `[M-codegen-emission-nondeterminism]`).
2. `nova test --positive --compile-error spec_tests/conformance` δ0 (**95/0**).
3. `nova test std` без новых фейлов + `nova build fn main(){}`/`println` без P67 (nova-build смоук-гард).
4. CI-линт raw-decode зелёный (после восстановления).
5. conventions §0/§1/§3/§10 грепом.

## Приёмка (close-out)

- `infer_expr_c_type` **удалён** — метрика `wc -l` → ~0 (тонкий лоуеринг), 0 независимой инференции.
- `resolved_types` полно покрывает (6z недостижим по корпусу).
- byte-parity сквозь всю миграцию; conformance 95/0; nova-build смоук зелёный.
- `[M-172.1-lifted-legacy-arms]` ЗАКРЫТ; raw-decode invariant = 0 И под CI; U.7 allowlist ПУСТ.

## Границы

MIR / оптимизации (SSA/DCE) — Стадия 2, отдельный горизонт. `[N]T` value-семантика — заморожена (172.12).

## Уроки (уже в конвенции, `b7a45bf7a`)

- **verify-and-discard ≠ materialize-and-delete.** Консолидация обязана МЕРИМО удалять (гейт-1 §0).
- **Спайк-на-авторитет** до стройки фундамента, если осуществимость неизвестна (§7.14). Для
  strangler-fig class-A/B не нужен (доказательство встроено в перенос); для НОВОЙ class-C несущей
  способности — ОБЯЗАТЕЛЕН (B07-спайк, 196.2 P3).
- **«Фазы связны» ≠ «несущее допущение подтверждено».** Не докладывать второе как первое.

## Грунтовка (факты разведки — поглощает бывш. 196-architecture-map/196-audit)

- Промежуточного typed-IR НЕТ; AST — единственный IR. Codegen перевыводит C-типы через
  `infer_expr_c_type` (**249 сайтов**). `resolve_type_to_c` (emit_c:3184) — единый C-лоуеринг.
- `resolved_types` ЧАСТИЧНЫЙ (дыры `[M-104.10]`: generic method-chain, non-primitive TupleLit/RecordLit,
  UNSET-desugar) → потому 6z-fallback (`infer_expr_c_type`) жив.
- **mono ЛЕНИВ в codegen** — `resolved_types` несёт GENERIC `ResolvedType` (с `TypeParam`); подстановка
  в `resolved_type_to_c` (3203-3237) при эмиссии. (Фазовая дисциплина — 196.2.)
- `ResolvedType` УЖЕ C-lossless (`[]T`/`Vec`→один C; ref/mut ABI-прозрачны) — новые узлы ради C НЕ нужны;
  §0 = ПОЛНОТА `resolved_types`, не MIR.
- По коду class-C = `infer_call_ret_c` (2591 стр) — ОСНОВНАЯ масса второго окна, но ОТДЕЛЬНАЯ
  сиблинг-функция (@46293-48883), вызываемая из 6z-ветки `infer_expr_c_type` (@48885); **НЕ «90% строк
  тела `infer_expr_c_type`»** (то — меньший диспетчер). Второе окно = обе функции вместе.
- byte-parity гейт живёт для Стадии-1 (resolved_types-completion), УМИРАЕТ для MIR (Стадия-2 — тотальная
  замена лоуеринга, новая приёмка).
- Декомпозиция class-C = 114 return-веток `infer_call_ret_c` (детали — подплан 196.2).

## Owner Q&A (сохранено)

- **SelfAccess-снос обоснован:** `@` эмитится как `nova_self` (ptr-биндинг `var_types`, ref-параметр,
  `emit_c.rs:956`), тип берётся общим путём резолва биндингов → спец-арм избыточен → 0 хитов → удалён.
- **`int as char` под unsafe — нужен D-амендмент:** реализовано (`cb9944acd`) — `int as char` разрешён
  в `unsafe`, чекер распознаёт как unsafe-op (E_UNSAFE_UNUSED §21). Позволяет `int @to_char()` в .nv.
  Нужен D-блок легитимизировать.
