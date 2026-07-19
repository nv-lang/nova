<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Волна-2: ИСПОЛНЯЕМАЯ карта (сверка по коду 2026-07-19)

**Родитель:** [196-one-truth-closeout.md](../196-one-truth-closeout.md) (зонтик, матрица одного окна).
**Пары:** [196.3-wave2-d-driven.md](../196.3-wave2-d-driven.md) (D-driven очередь), [196-campaign-map.md](196-campaign-map.md)
(флот-раздача, база `2c0f3ee42` 2026-07-16 — на 3 дня СТАРШЕ этой карты). **Разведка:** opus, только чтение/греп,
код НЕ правился. **База сверки:** main @ `78503bf5d` (worktree `nova-w2map`, D55 `dab636bc3` — предок).
**Модель исполнения:** sonnet-по-карте (класс C), haiku-механика (класс A/B по спискам). Приёмка (0)-(5) ПО КОДУ —
из 196.3/кампании без изменений.

> **Назначение:** дать волне-2 исполнимую раздачу под дешёвые модели. Карта — НЕ новый план; дисциплина per-D,
> «удалить ИЛИ паника», detach-для-SHARED, byte-parity-протокол берутся из 196.3/кампании. Отличие от карты
> кампании (2c0f3ee42): пересверено по САМОМУ свежему main (+323 коммита, вкл. rtbuf-продюсеры второй оси,
> capstone-1, capstone-2+ОТКАТ, d45-фикс, D55→D429). Даю живой инвентарь + call-tree + порядок непересекающихся
> первых волн.

---

## 0. Что изменилось с базы карты кампании (2c0f3ee42 → 78503bf5d, +323 коммита)

Главные фактические сдвиги, влияющие на раздачу (подтверждено по коду/git-log):

| Что | Карта кампании (2c0f3ee42) | ПО КОДУ сейчас (78503bf5d) | Следствие для волны-2 |
|---|---|---|---|
| `infer_call_ret_c` (frozen-зона В-1) | 49943-52037 | **50542-52681** (~2139 стр) | функция ПОДРОСЛА (сдвиг вверх от продюсеров); координаты веток = `icr_trace`-id, НЕ строки |
| Живых веток (`self.icr_trace(...)`) | 50 | **48** (уникальных id — 48) | capstone-1 снял 2 (B11ai, B11m); capstone-2 (−2) **ОТКАЧЕН** — реестр обратно 48 |
| Диспетчер `infer_expr_c_type` | 52065 | **52708-55167** (~2459 стр), **252 консумера** | почти неподвижен; схлопнется на финале |
| Продюсеры второй оси (`resolved_types_buf`) | «Q1/Q6 частично» | **123 сайта** в types/mod.rs; rtbuf-продюсеры ×4 ВЛИТЫ (`726e734af`) | Q1 static-ctor/free-fn/newtype + Q6 typed-ptr заканалены; **generic-форма ctor НЕ каналит** (см. §0-урок) |
| Канал `node_substs` | «5 продюсеров A/B/C» | **7 insert-сайтов** (8391/11738/14975/15802/15936/16026/16611) + `shadow_check_node_substs` | Tier-2 разблокирован шире; +Producer B-fluent-generic (`0fd827412`), CH-widen SHADOW-ICE фикс (`137167c54`) |
| Канал `resolved_callees` (Ф.B/C) | «частично» | **4 insert-сайта** (10967/11076/11274/11507); `callnorm::normalize_module` читает через `by_span` fast-path | Ф.C callnorm-переезд СОСТОЯЛСЯ частично (см. §3) |
| Гейт conformance | 95/0 / 470/0 | мега-CU авторитет (не гонял — задание запрещает); d45-регрессия ЗАКРЫТА (`f37965726`) | авторитет = мега-CU + флагман `--strict-effects` |
| D55 (str-литерал→[]u8) | — | тип-направленно `dab636bc3`, затем **D429 #coerce** (Plan 214, спека, В РАБОТУ НЕ запущен) супер-сидит | D55-строки трекера ✅ (`f72a27c8b`); НЕ сиблинг-функция волны-2 (чекер-коэрсия, не второе окно) |
| slice-ext-2 (emit_c for-in) | — | **ВЛИТ** (`e0d03c6f9`, последний коммит main) | коллизия снята — for-in зона свободна |

**★ КЛЮЧЕВОЙ УРОК capstone-2 (2026-07-18, `1c20dc2e3` revert + `196-capstone2-notes.md`):** detach+panic на
frozen-ветке валиден ТОЛЬКО на целиком компилирующемся корпусе. Снос B10h/B10l был отменён: d45-регрессия (от
продюсера `ba9a8a2f3`, `None => Unit` для expression-body) МАСКИРОВАЛА трафик фикстур (весь conformance mega-CU
был красным, ветки казались 0-hit). После d45-фикса вскрылось: **generic newtype/named-tuple ctor продюсер Q1 НЕ
каналит generic-формы** (generics-гейт) → легаси (B10h/B10l) единственный источник. Путь закрытия = расширение
ctor-продюсера на generic-формы (Zone CH). **Порядок капстоуна: НИКОГДА не detach-ить ветку, пока мега-CU не
зелёный.**

---

## 1. Актуализированный инвентарь сиблингов второго окна (греп по 78503bf5d)

Легенда судьбы: **🗑 УДАЛЁН** (снесён, только REMOVED-коммент) · **✅ ЗАКРЫТ** (мигрирован в одно окно / прунут,
легаси мёртв) · **🔄 ДОЖАТЬ** (канал-замена ЕСТЬ, снять fallback/legacy) · **➡ МИГРАЦИЯ** (легаси жив, канал
нужен/частичен) · **✅ ОСТАЁТСЯ** (законный лоуэринг/канал-читатель, НЕ мигрируется) · **⏸ Tier-3** (канала нет by
construction — Receiver=декларация).

Зона (для раздачи): координата в `emit_c.rs` (если не сказано иное). Frozen = 50542-52681.

### 1.1 Возврат/тип (Ф.A)

| Функция (строка) | Судьба | D-фича | Call-sites (frozen?) | Канал-замена |
|---|---|---|---|---|
| `infer_call_ret_c` (50542-52681) | ➡ КАПСТОУН B6 | все Call-return (48 веток) | 5 (в диспетчере+рекурсия) | — (frozen-агент серийно) |
| `infer_expr_c_type` (52708-55167) | ✅ ОСТАЁТСЯ→схлоп на финале | диспетчер | **252 консумера** | Кан.1-2 → `resolved_type_to_c` |
| `infer_static_method_ret` | **🗑 УДАЛЁН** (`9b63fd145`) | D182/D372 static-return | 0 | `resolved_types` (Q1 закрыт) |
| `infer_trailing_block_sig` (32587) | ✅ ЗАКРЫТ (`e6391b931`) | D43 trailing-block | тонкий читатель→`infer_expr_c_type` | одно окно ✔ |
| `infer_expr_c_type_str` (46825) | ➡ верифицировать | str-форма типа | 1 | обёртка над диспетчером |
| `infer_mono_method_ret` (47039) | ➡ SHARED | D119/D122 generic-method-return | 1 (делегат к `_with_args`) | node_substs |
| `infer_mono_method_ret_with_args` (47049) | ➡ SHARED detach | D119/D122 | 4: L38532,L47040 outside; **L51217,L51318 FROZEN** | node_substs |
| `infer_method_level_return_for_sum` (47531) | ➡ Q2 SHARED detach | D30/D85/D52/D407 sum-методы | 3: **L52183,L52209 FROZEN** (+1 внутр.) | node_substs/resolved_types Call-return |
| `infer_generic_static_ctor_ret` (19623) | ➡ Q+ctor SEP | D372/D239 generic-static-ctor | 2: L52782 (диспетчер) | resolved_types ctor-return (частично) |
| `infer_lambda_return_type_with_params` (49951) | 🔄 верифицировать residual | D48 closure-return | 2: L46203,L46209 outside | `closure_channel_ret_c` ЕСТЬ |
| `resolve_result_option_ret` (19094) | ➡ Q5 (в осн. N/A) | D30/D85/D325 Result/Option имя | 3: **L50947,L51664 FROZEN** | механич. mapper = ЗАКОННЫЙ lowering* |
| `resolve_result_te` (48177) | ➡ Q5 MIXED | D30/D85 Result T/E | 5: L30999,L31123,L48193 outside; **L52203 FROZEN** | node_substs Call-return |
| `resolve_result_te_strict` (48190) | ➡ Q5 (не frozen!) | D30/D85 Result T/E strict | 2: L34844,L35092 **обе outside** | node_substs / resolved_types |
| `result_repr_c_type` (48281) | ✅ ОСТАЁТСЯ (namer) | Result C-repr имя | 9 | mono-namer (`NovaRes_`), не инференс |
| `channel_int_c_type` (48810) | ✅ ОСТАЁТСЯ (READER) | int-ширина из канала | — | **читает `resolved_types`→`resolved_type_to_c`** |

\* `resolve_result_option_ret` (сверка Zone RET 2026-07-17, подтверждена этой сессией по коду 19094): сигнатура
`fn(ty:&TypeRef, subst:&[(String,Option<String>)]) -> Option<String>` — механический маппер УЖЕ-субституированного
`Result`/`Option` TypeRef в mono `NovaRes_`/`NovaOpt_` C-имя. Это законный lowering (rustc: mono=подстановка), НЕ
Tier-2 инференс. Собственной логики мигрировать в чекер НЕЧЕГО; снять НЕЛЬЗЯ, пока caller'ы (frozen B06a/B10j) зовут
→ уходит с капстоуном. Настоящий Q5-остаток = `resolve_result_te`/`infer_result_type_params_legacy` (T/E-извлечение).

### 1.2 Generic/type-param (Ф.B/C)

| Функция (строка) | Судьба | D-фича | Call-sites (frozen?) | Канал-замена |
|---|---|---|---|---|
| `infer_result_type_params` (17976) | ➡ Q9 диспетчер (split готов) | D30/D85 Result T/E | делегирует _channel/_legacy | — |
| `infer_result_type_params_channel` (17991) | 🔄 добить | — | (из диспетчера) | node_substs/resolved_types |
| `infer_result_type_params_legacy` (18005) | 🔄 снести после _channel | — | 1: L17977 (диспетчер) | — |
| `infer_type_param_binding` (20707) | ➡ Q9 | D16/D53/D72 type-param вывод | **47** | resolved_types (решение В тип) |
| `infer_type_param_binding_rt` (2855) | ➡ Q9 | D16 (runtime-вариант) | — | resolved_types |
| `infer_type_param_binding_from_ref` (20513) | ➡ Q9 | D16 (from-ref) | — | resolved_types |
| `infer_protocol_structural_binding` (20573) | ➡ Q9 | D42/D355 protocol-dispatch | 5 | resolved_types |
| `resolve_method_level_subst` (21096) | 🔄 Q10 SEP | D119/D122 method-generics subst | 5: **все outside** (34157/34699/34967/37696/37992) | node_substs (`_ch` читает) |
| `resolve_mono_type_args` (19803) | 🔄 Q10 снести legacy | D119/D122 mono type-args | 1: L20239 (внутри `_ch` fallback) | node_substs |
| `resolve_mono_type_args_ch` (20232) | ✅ ОСТАЁТСЯ (READER) | — | (консумер канала) | **читает node_substs безусловно** |
| `compute_array_elem_type_for_obj` (21576) | 🔄 Q6 снять fallback | D239/D373 elem массива/слайса | 1: L53216 (диспетчер Channel-6k) | resolved_types[obj.id] |
| `channel_array_elem_c` (21606) | ✅ ОСТАЁТСЯ (READER) | — | 7 | **читает resolved_types[obj.id]** |

### 1.3 ★ДОПОЛНЕНО + хардкод-зеркала + лоуэринг

| Функция (строка) | Судьба | D-фича | Call-sites | Примечание |
|---|---|---|---|---|
| `primitive_instance_method_known` (46957) | ✅ ЗАКРЫТ prune (`0830664d6`) | D109 методы примитивов | — | str-дубль снесён; остались обоснованные bootstrap-интринсики |
| `type_ref_to_c` (9829, emit) + (597, external_registry) | ✅ ЗАКРЫТ (`4c02d346d`) | D315 тип→C | — | канон=`resolved_type_to_c`; ext делегирует `primitive_name_to_c` |
| `primitive_name_to_c` (9797) | ✅ ОСТАЁТСЯ (lowering) | прим→C | — | законный лоуэринг |
| `resolved_type_to_c` (3608) / `resolved_array_to_c` (3744) / `resolved_named_to_c` (4090) | ✅ ОСТАЁТСЯ | тип→C | — | ЕДИНСТВЕННЫЙ тип→C лоуэринг |
| `f64_method_to_c` (46832) | ➡ Q7-follow (.nv-sourcing?) | §3 f64 методы хардкод | — | зеркало как D109; проверить на dead-dup |
| `int_method_to_c` (46913) | ➡ Q7-follow (.nv-sourcing?) | §3 int методы хардкод | — | зеркало как D109; проверить на dead-dup |
| `receiver_c_type` (16853) | ⏸ Tier-3 | ABI nova_self приёмника | **31** | Receiver=декларация (нет ExprId), канала нет by construction; retire→«receiver-aware resolved_type_to_c» |
| `builtin_sum_receiver_c_type` (16810) | ⏸ Tier-3 | ABI sum-приёмника | 6 | то же |
| `value_aware_generic_c_type` (19027) | ➡ SHARED verify | value-aware generic C-тип | 11 | census: живая ре-деривация (B10c); проверить |
| `extract_result_type_params` (8845) | ➡ verify (TypeRef) | Result T/E из TypeRef | 4 | TypeRef-экстрактор (не Call) — Tier-1-подобен |
| `call_result_type_params_key` (17834) | ➡ verify | ключ Result-типов вызова | 3 | key-computation |
| `infer_func_c_name` (32628) | ➡ verify (lowering?) | mono-имя функции | 2 | вероятно лоуэринг; сверить на капстоуне |
| `infer_handler_interrupt_ty` (126, pub fn) | ➡ verify | тип прерывания хендлера | — | effect-хендлер; узкая форма |
| `infer_mono_method_ret` см. §1.1 | | | | |

**Сводка живости:** инвентарь плана/★ДОПОЛНЕНО = ~34 именованных функции. Из них **🗑 УДАЛЁН: 1**
(`infer_static_method_ret`). **✅ ЗАКРЫТ/ОСТАЁТСЯ (не цель): 11** (`infer_trailing_block_sig`, `type_ref_to_c`×2,
`primitive_name_to_c`, `primitive_instance_method_known`, `resolved_type_to_c`/`_array`/`_named`,
`channel_int_c_type`, `resolve_mono_type_args_ch`, `channel_array_elem_c`, `result_repr_c_type`). **⏸ Tier-3: 2**
(`receiver_c_type`, `builtin_sum_receiver_c_type`). **➡/🔄 ЖИВЫХ ПОД МИГРАЦИЮ: ~18** (Q1 закрыт → Q2/Q5/Q6/Q9/Q10/
Q+ctor/Q+lambda + хвосты). **Было в плане M≈30 «строк инвентаря» → живых-под-миграцию N≈18** (остальное закрыто/
осталось/удалено/Tier-3). Плюс 48 frozen-веток `infer_call_ret_c` (капстоун) — отдельный серийный шест.

---

## 2. По-сиблинговая деталь живых функций (D-фичи · call-tree · канал · класс · порядок)

Формат каждого блока: **(а)** D-фичи (какие языковые формы) · **(б)** call-tree (кто зовёт, фаза, frozen?) ·
**(в)** канал-замена (есть/нужен продюсер) · **(г)** класс исполнения (A/B haiku · C sonnet · арх=opus-флаг) ·
**(д)** порядок/зависимости.

### Q10 — `resolve_mono_type_args` (19803) + `resolve_method_level_subst` (21096) — 🔄 ДОЖАТЬ

- **(а)** D119 method-level type-params, D122 bound-method mono-dispatch/forwarding, D123 tuple-mono,
  D277 value-record-mono, D354 anon-tuple-mono. Обе — unification-движки: подстановка T/U вычислением
  param-`TypeRef` против arg-C-типа.
- **(б)** `resolve_mono_type_args` — 1 caller (L20239, ВНУТРИ `resolve_mono_type_args_ch` как fallback).
  `resolve_method_level_subst` — 5 callers (34157/34699/34967/37696/37992, **ВСЕ outside frozen**). Фаза: codegen
  emit-путь mono-инстанса. `_ch` (20232) читает node_substs безусловно + shadow-cross-check; при промахе падает
  в legacy (Producer B-fluent-generic снизил fallback `resolve_method_level_subst` 311→49 хитов, `0fd827412`).
- **(в)** node_substs **ПОСТРОЕН** (7 продюсеров types/mod.rs). d122/d354 УЖЕ закрыты через `_ch` (`f07cd0991`,
  `c7f012f63`). Остаток fallback ≈49 хитов — классы Call-форм, что чекер ещё не пишет в node_substs.
- **(г)** класс **C (sonnet-по-карте)**. Работа В emit_c (флип консумера + снос legacy-движков) — читает уже
  построенный канал. Ловушка приёмки (3): census «три hand-duplicated inference engines» (`resolve_mono_type_args`
  Source 4 сам документирует) — свести ВСЕ три, грепом убедиться что параллельного legacy нет.
- **(д)** ГОТОВ СЕЙЧАС (канал есть). Остаточные ~49 fallback могут требовать доп. продюсера (Zone CH, types/mod.rs)
  для последних Call-форм → координировать: сначала снять то, что канал уже покрывает (byte-parity), fallback-снос
  на 0-hit — после расширения. **Зона emit_c 19803-21096 — НЕ пересекается с 214/for-in.**

### Q6 — `compute_array_elem_type_for_obj` (21576) — 🔄 ДОЖАТЬ (осторожно: откат 93/2)

- **(а)** D239 slice/vec-alias, D373 generic-array-api, D403/D411 sized-elem. Тип элемента при `obj[i]`/`for x in`.
- **(б)** 1 caller: L53216 (диспетчер `infer_expr_c_type`, Channel-6k fallback на deep field-chain
  `obj.f1.f2.field[i]`). Дубликат-арм (~130 стр) уже снесён (структурно недостижим). `channel_array_elem_c` (21606,
  7 callers) — канал-первый на всех вне-frozen сайтах; `.or_else`-fallback на 5 сайтах уже снят (`979df4cce`).
- **(в)** resolved_types[obj.id]. **Пробел:** `channel_array_elem_c` читает только НЕПОСРЕДСТВЕННЫЙ `obj`, не
  рекурсивную цепочку полей → контейнер-cap кейсы (`StringBuilder.new(128)`, d371) НЕ покрыты (история: снос
  fallback дал 93/2, откат `bdf880c10`).
- **(г)** класс **C (sonnet)** + зависит от Zone CH (расширение канала на контейнер-cap/deep-chain).
- **(д)** ПОСЛЕ расширения канала. Ловушка §4.1 кампании: снимать `.or_else`/Channel-6k ТОЛЬКО после проверки на
  ПОЛНОМ мега-CU, не на 4-site sample. **НЕ первая волна.**

### Q9 — `infer_type_param_binding` ×3 (2855/20513/20707) + `infer_protocol_structural_binding` (20573) + `infer_result_type_params` (17976 split) — ➡ МИГРАЦИЯ

- **(а)** D16 generics-brackets, D53 protocol-kind-token, D72 generic-bound-protocol, D42 structural-protocol,
  D355 blanket-protocol, D135 bare-generic-inferred. Вывод type-param + protocol-dispatch (structural/blanket/bounded).
- **(б)** `infer_type_param_binding` — **47 callers** (крупнейший из Q9; фаза codegen mono/dispatch).
  `_rt` (2855) и `_from_ref` (20513) — варианты. `infer_protocol_structural_binding` — 5. `infer_result_type_params`
  (17976) уже split: `_channel` (17991) + `_legacy` (18005, 1 caller = диспетчер).
- **(в)** resolved_types (решение type-param записано В тип, с TypeParam). `_channel` — начатый продюсер-путь.
- **(г)** класс **C (sonnet)**, крупный (47 callers у binding) — раздробить: сперва `infer_result_type_params`
  (split готов, добить `_channel`+снести `_legacy`) → потом protocol-binding → потом type_param_binding ×3.
- **(д)** SEP от frozen (в осн.). Зависит частично от Zone CH. **Зона emit_c 2855+17838-20707** — верх диспетчера;
  пересекается с продюсерами? Нет (продюсеры в types/mod.rs). Пересечение с 214 `try_wrap_leaf` (33249)? Нет.

### Q2 — `infer_method_level_return_for_sum(_inner)` (47531) — ➡ SHARED detach+panic

- **(а)** D30 Option, D85 Result, D52 sum-int, D407 enum-payload-width, D406 enum-kind-token. Возврат метода
  sum-типа. NB (196.5-perd-d52): фактическая матрица функции = ТОЛЬКО Option/Result (оба caller'а передают
  литералы "Option"/"Result"); user-enum-часть структурно недостижима.
- **(б)** callers **L52183, L52209 — ОБА FROZEN** (внутри `infer_call_ret_c`). SHARED — правило detach+panic,
  физический снос = капстоун B6.
- **(в)** node_substs/resolved_types Call-return. Stage-D измерила: conformance-часть кластера 0-hit уже сегодня
  (35→0 после 3 чекер-продюсеров); остаток = 16 хитов std/src/data (`u64.try_from` D77-интринсик, НАМЕРЕННО
  стёртая репрезентация — чекер честно НЕ материализует, иначе byte-parity сломан).
- **(г)** класс **C (sonnet)** миграция + арх-флаг (D77-follow-up — отдельный блокер Plan 59 Ф.7.5 D3, ВНЕ волны-2).
- **(д)** Зависит от Zone CH (Call-return sum-класс). detach+panic ТОЛЬКО когда мега-CU зелёный (урок capstone-2).

### Q5 — `resolve_result_te` (48177) + `resolve_result_te_strict` (48190) + `infer_result_type_params_legacy` (18005) — ➡ МИГРАЦИЯ

- **(а)** D30 try-op-unwrap, D85 question-return/result-payload-width, D325 result-everywhere. Извлечение T/E из
  `Result[T,E]`/`Option[T]` (`?`-оператор, match, method-chain).
- **(б)** `resolve_result_te` — 5 callers, 3 outside (30999/31123/48193) + **1 FROZEN (52203)**.
  `resolve_result_te_strict` — 2 callers, **ОБА outside** (34844/35092) → мигрируем НЕЗАВИСИМО от frozen!
  `resolve_result_option_ret` (19094) — 2 frozen callers, но это namer (законный lowering, см. сноску §1.1).
- **(в)** node_substs Call-return (Tier-2, РАЗБЛОКИРОВАН). `resolve_result_te_strict` outside-часть — Tier-1-подобна
  (не Call-выражение), мигрируема сразу.
- **(г)** класс **C (sonnet)**. Разделить: `resolve_result_te_strict` (не frozen) + `infer_result_type_params_legacy`
  снос — раньше; `resolve_result_te` frozen-caller — detach.
- **(д)** Зависит от Zone CH (Call-return). Порядок: `_strict` outside → `_channel` добить → `_legacy` снести → `_te` detach.

### Q+ctor — `infer_generic_static_ctor_ret` (19623) — ➡ МИГРАЦИЯ SEP

- **(а)** D372 canonical-new-defaults, D239 slice-static-ctor. Возврат `Vec[T].new()` / `[]T.static()`.
- **(б)** callers: L52782 (диспетчер, outside frozen). Пред-канальный кейс диспетчера. SEP.
- **(в)** resolved_types ctor-return. rtbuf-продюсер Q1 static-ctor заканалил ПРИМИТИВНЫЕ формы, но **generic-форма
  НЕ каналит** (урок capstone-2: B10h/B10l живы именно поэтому). Нужен generic-ctor продюсер (Zone CH).
- **(г)** класс **C (sonnet)** + Zone CH продюсер-widen (types/mod.rs) на generic-ctor формы.
- **(д)** Кормит капстоун B10h/B10l. Зависит от Zone CH generic-ctor продюсера. Порядок: продюсер → снос B10h/B10l.

### Q+lambda — `infer_lambda_return_type_with_params` (49951) — 🔄 верифицировать residual

- **(а)** D48 closure-return (typed/light/unit-body/value-position).
- **(б)** 2 callers: L46203, L46209 (outside frozen).
- **(в)** `closure_channel_ret_c` ЕСТЬ (d22/d402 закрыты `c4818131c`). Функция — residual для непокрытых форм.
- **(г)** класс **B/C**: верифицировать residual-достижимость (0-hit?) → снести если недостижима.
- **(д)** ГОТОВ (проверка residual). Низкий риск.

### SHARED-хвост — `infer_mono_method_ret(_with_args)` (47039/47049) — ➡ detach, НЕ delete

- **(а)** D119/D122 generic-method-return (с аргументами).
- **(б)** `_with_args` — 4 callers: L38532, L47040 outside; **L51217, L51318 FROZEN**. `infer_mono_method_ret`
  (47039) — тонкий делегат к `_with_args`.
- **(в)** node_substs. **(г)** класс C, detach (SHARED). **(д)** уходит с капстоуном.

### Хвосты verify (низкий приоритет, на капстоуне) — `value_aware_generic_c_type` (19027, 11) · `extract_result_type_params` (8845, 4) · `call_result_type_params_key` (17834, 3) · `f64_method_to_c` (46832) · `int_method_to_c` (46913) · `infer_func_c_name` (32628, 2) · `infer_expr_c_type_str` (46825, 1) · `infer_handler_interrupt_ty` (126)

- `f64_method_to_c`/`int_method_to_c` — §3 хардкод-зеркала (как `primitive_instance_method_known` до D109-пруна);
  **haiku-задача**: грепнуть на dead-dup vs обоснованный интринсик (прецедент: str-методы были мёртвым дублем,
  f64.sqrt — обоснован). Класс **A/B**.
- `extract_result_type_params` (TypeRef, не Call) — Tier-1-подобен, мигрируем; `value_aware_generic_c_type` —
  census: живая ре-деривация (B10c), проверить. `infer_func_c_name` — вероятно mono-namer (lowering, остаётся).
- Все — верификация на финале (структурный гейт: не осиротело → компилится).

### ⏸ Tier-3 (НЕ волна-2) — `receiver_c_type` (16853, 31) · `builtin_sum_receiver_c_type` (16810, 6)

Канала НЕТ by construction: `Receiver` = ДЕКЛАРАЦИЯ метода (нет ExprId), `resolved_types` ключуется ExprId. Все
callers передают `type_name`-строку. Generic-subst внутри УЖЕ через `resolved_type_to_c`. Retire запланирован в
«receiver-aware `resolved_type_to_c`» (U.4.5/FIN). «No new window §3» запрещает синтетический канал. **НЕ трогать
в волне-2** (36/36 юнит-тестов + doc уже на месте).
