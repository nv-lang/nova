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
- **Окно 2 — `infer_expr_c_type`** (`emit_c.rs`, ~2600 строк, **ОДНА функция** — не разбросано):
  codegen ПЕРЕвыводит C-тип там, где канал пуст. Каналы 1-6 читают чекер; **Канал 6z** (44 арма +
  `infer_call_ret_c` 2591 стр) = legacy-перевывод, срабатывает как **fallback** когда `resolved_types`
  пуст. Главная масса 6z-кода — class-C generic-mono.

Расхождение окон → §0-баги (мис-диспатч, `nova_int`-затычка, тот самый nova build ICE). Цель:
**дополнить `resolved_types` → удалить `infer_expr_c_type`.**

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
  non-primitive, As, Is, IfLet, SelfAccess, Unary, Match, Coalesce, Ident. **[ИДЁТ — worktree
  `nova-p196`, гейт-1 на убывание строк].** Может выделиться в подплан `196.1`.
- **Ф.S1b — тяжёлое (class-C, ~90% кода 6z).** `infer_call_ret_c` (2591 стр) = **class-C резолвер,
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
  **строго убыло.** 0 снято = **КРАСНАЯ** волна → стоп + переоценка. Никакого «фундамента».
- ★ **ГЕЙТ-2 спайк-на-авторитет (§7.14):** для strangler-fig **НЕ нужен** (перенос сам авторитетен
  для своего случая, fallback + byte-parity доказывают). Применять, только если вводится НОВЫЙ движок
  неизвестной осуществимости.
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
  strangler-fig не нужен — доказательство встроено в перенос.
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
- По коду class-C (`Call`/`infer_call_ret_c`, 2591 стр) = ~90% массы `infer_expr_c_type`.
- byte-parity гейт живёт для Стадии-1 (resolved_types-completion), УМИРАЕТ для MIR (Стадия-2 — тотальная
  замена лоуеринга, новая приёмка).
- Декомпозиция class-C = 114 return-веток `infer_call_ret_c` (детали — подплан 196.2).

## Owner Q&A (сохранено)

- **SelfAccess-снос обоснован:** `@` эмитится как `nova_self` (ptr-биндинг `var_types`, ref-параметр,
  `emit_c.rs:956`), тип берётся общим путём резолва биндингов → спец-арм избыточен → 0 хитов → удалён.
- **`int as char` под unsafe — нужен D-амендмент:** реализовано (`cb9944acd`) — `int as char` разрешён
  в `unsafe`, чекер распознаёт как unsafe-op (E_UNSAFE_UNUSED §21). Позволяет `int @to_char()` в .nv.
  Нужен D-блок легитимизировать.
