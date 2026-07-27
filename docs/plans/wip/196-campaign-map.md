<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — КАРТА КАМПАНИИ полного закрытия (флот-раздача)

**Родитель:** [196-one-truth-closeout.md](196-one-truth-closeout.md) (зонтик). **Пары:**
[196.3](196.3-wave2-d-driven.md) (волна-2 D-driven), [196.2](196.2-class-c-relocation.md)/[196.5-stage-d-census](196.5-stage-d-census.md)
(волна-1 `infer_call_ret_c`), [196.5-facet-c-map](196.5-facet-c-map.md) (Ф.C, ПАРАЛЛЕЛЬНЫЙ агент — зону НЕ раздаю).
**Разведка:** opus, сверка по коду 2026-07-16 (main @ `2c0f3ee42`). **Модель исполнения:** sonnet-флот + haiku-механика.

> **Назначение карты:** дать оркестратору готовую раздачу на «столько агентов, сколько нужно, как можно скорее».
> Карта — НЕ новый план: дисциплина per-D, приёмка (0)-(5) ПО КОДУ и правило «удалить ИЛИ паника» берутся из
> 196.3/196.2 без изменений. Отступление здесь ровно одно (санкционировано, §3): БАТЧ-гейт на 3-5 D вместо per-D.

---

## 0. Сверка по коду — актуальное состояние (главные поправки к протухшим координатам планов)

Планы 196.2/196.3/196.5 писались на старых координатах (`infer_call_ret_c` 46293-48883, «95/0», «node_substs
канала НЕТ»). По факту main @ `2c0f3ee42`:

| Что | В планах | ПО КОДУ сейчас | Следствие |
|---|---|---|---|
| `infer_call_ret_c` (замороженная зона В-1) | 46293-48883, 2591 стр | **49943-52037, ~2094 сырых стр** | функция УБЫЛА; координаты веток протухли — ключ = `icr_trace`-id, не строка |
| Живых веток (`self.icr_trace(...)`) | 114 (старт) / 54 (census `f64c9fdec`) | **50 на main** | −64 от старта; census-база НЕ на main, число иное |
| Диспетчер `infer_expr_c_type` | 48885 | **52065**, 251 консумер | почти неподвижен (схлопнется на финале) |
| Канал `node_substs` (Tier-2 фундамент) | «НЕТ by construction» (Q5/Q10) | **ПОСТРОЕН** (types/mod.rs: 4 продюсера A/B/C @ 8208/11297/14219/15118/15208; консумер `resolve_mono_type_args_ch` @ emit_c 20094 читает безусловно + `shadow_check_node_substs` cross-check) | **развилка (A) де-факто выбрана владельцем; Tier-2 РАЗБЛОКИРОВАН** — d122/d354 уже закрыты через него |
| Гейт conformance | 95/0 | **470/0** (мега-CU ~169 файлов, один CU) | старое «95» = меньший прогон; авторитет = 470/0 |
| W1-i (ядро В-1) | «идёт» | **ГОТОВО на main** (`fa593b77b`/`18a826837`): 6 W1-армов сняты/флипнуты (B05/B06a/B07/B07r → `resolve_instance_call_subst`; B11c/B11x снесены), SHADOW 0-mismatch | ядро iterator-adapter больше не первично; frozen-зона = fallback |
| 196.7/196.8/196.9 | «идут»/«закрыты» | **ВЛИТЫ** (`be03db5bc`/`64f3369fa`/`49c4e8297`); `[M-i64-clamp-*]` тоже закрыт (196.9 `de15478d1`) | dispatch-баги примитивов сняты — НЕ висят на кампании |

**Вывод сверки:** кампания дальше, чем читается по планам. Оставшийся демонтаж — не «постройка фундамента»
(он есть), а **пробрасывание Call-return классов через уже-существующий канал + слив 50 frozen-веток**.

### Входящие слияния (учтены как база — статус по `git rev-list main..<ветка>`)

| Ветка | ahead-of-main | Что | Коллизия с флотом |
|---|---|---|---|
| p196-dispatch (196.7) | **0 — ВЛИТА** | method-dispatch via `resolved_callees` | база |
| p196-8-dispatch (196.8) | **0 — ВЛИТА** | primitive bounded-blanket dispatch | база |
| p196-9-overload (196.9) | **0 — ВЛИТА** | i64.clamp match-arm scope fix | база |
| p210-design | **0 — ВЛИТА** | — | база |
| **p196-w2-next** (D239 dup-арм) | **1 — ВХОДИТ** | emit_c @ ~21409 (+6) и ~53891 (−116, дубль-диспатчер-арм) | вне frozen-зоны; регион ~53891 → координировать с Zone GEN только на D239 |
| **p206-1-divtrap** | **5 — ВХОДИТ** | emit_c 5 сайтов (27570/28427/29764/29978/48144) div/neg-trap + effects.h + protocols.nv | все ВНЕ frozen-зоны; 48144 близко, но это effect-emit, не type-infer — низкий риск |
| **p210-embed-dir** | **8 — ВХОДИТ** | **0 правок emit_c** (только std/prelude/embed.nv + тесты) | нулевой риск |
| p196-facetC | 2 (активный агент) | callnorm/argbind Ф.C | **ЗОНУ НЕ РАЗДАЮ** (см. §1г) |

Дельты p196-w2-next/p206-1-divtrap/p210-embed-dir считать базой: их зоны emit_c (21409, 27570-29978, 48144, 53891)
НЕ пересекаются с frozen-зоной (49943-52037) и с band'ами флота (17838-21432, 46381-47700). Флот стартует ПОСЛЕ
их вливания либо ребейзится — конфликтов нет.

---

## 1. Точный остаток до закрытия зонтика

### (а) Не-✅ пункты очереди волны-2 (инвентарь-функции) — с зонами

Инвентарь второго окна = ~12 сиблинг-функций. **Закрыто (✅, через одно окно):** `infer_trailing_block_sig`
(D43), `primitive_instance_method_known`-прун (D109), `type_ref_to_c` (D315), closure-канал `closure_channel_ret_c`
(D22/D48/D402), Ф.C-экземпляр callnorm (D102/D372), `resolve_mono_type_args_ch` частично (D122/D354). **Остаток:**

| # | Инвентарь-функция (строка emit_c) | D-фича | Отделяемость от frozen | Зависит от канала | Судьба |
|---|---|---|---|---|---|
| Q1 | `infer_static_method_ret` (в зоне 46381-47700 кластера) | D182/D372 static-return | **SHARED** (зовётся из frozen) | resolved_types[call.id] (static-return) | ➡ чекер + panic |
| Q2 | `infer_method_level_return_for_sum(_inner)` (46955/46967) | D30/D85/D52/D407 sum-методы | **SHARED** (caller'ы B11q/B11r в frozen) | node_substs/resolved_types (Call-return) | ➡ чекер + panic |
| Q5 | `resolve_result_option_ret` (18956) + `resolve_result_te(_strict)` (47601/47614) | D30/D85/D325 Result/Option | **MIXED** (часть в frozen: B06a/B10j-класс) | **node_substs (Call-return) — ТЕПЕРЬ ЕСТЬ** | ➡ чекер + panic (Tier-2 разблокирован) |
| Q6 | `compute_array_elem_type_for_obj` (21432) | D239/D373 elem массива/слайса | **MIXED** (🔄 `channel_array_elem_c` есть, `.or_else` fallback ЖИВ) | resolved_types[obj.id] | ДОЖАТЬ: снять fallback (см. ловушку §4.1) |
| Q9 | `infer_type_param_binding` (20569) + `_from_ref` (20375) + `_rt` (2805) + `infer_protocol_structural_binding` (20435) + `infer_result_type_params` (17838; уже split `_channel` 17853 / `_legacy` 17867) | D16/D53/D72/D42/D355 generics/протоколы | **в осн. SEP** | resolved_types (type-param decision) | ➡ чекер (решение В тип) |
| Q10 | `resolve_mono_type_args` (19665) + `resolve_method_level_subst` (20958) | D119/D122 mono/method-generics | SEP от frozen | **node_substs — ПОСТРОЕН**; `_ch` (20094) уже читает | ДОЖАТЬ: завершить флип `→_ch`, снести legacy `resolve_mono_type_args`/`resolve_method_level_subst` |
| Q+ | `infer_generic_static_ctor_ret` (19485) | D372/D239 generic-static-ctor | **SEP** (пред-канальный кейс диспетчера) | resolved_types (ctor-return) | ➡ чекер |
| Q+ | `infer_mono_method_ret(_with_args)` (46463/46473) | D119/D122 generic-method-return | SHARED (emit-путь + frozen) | node_substs | detach, не delete (SHARED, §4.4) |
| Q+ | `infer_lambda_return_type_with_params` (49352) | D48 closure-return | канал `closure_channel_ret_c` ЕСТЬ → residual | resolved_types Func | верифицировать residual-достижимость |

**Итог (а):** **~7-8 инвентарь-функций** реально остаются под миграцию (Q1/Q2/Q5/Q6/Q9/Q10/Q+ctor); из них 2
(Q6/Q10) — «ДОЖАТЬ уже-начатое» (канал есть, снять fallback/legacy), остальные — полноценная миграция в чекер.

### (б) Состояние 307-трекера call-сайтов (308 файлов: 231 поз. + 77 нег.)

| Статус | Кол-во (≈) | Значение |
|---|---|---|
| ✅ закрыт (приёмка (0)-(5) ПО КОДУ) | **~17** | d102×2, d109, d119, d122×2, d22, d315, d354, d372, d402, d43, d48, d55×4 |
| 🔄 миграция идёт | **4** | d239_slice_vec_alias, d30_try_op_unwrap_pair, d85_question_return, d85_result_payload_width |
| 🔍 coverage-ok (НЕ цель миграции) | **~287** | покрытие проверено; фичи УЖЕ работают, не второе окно — флипаются транзитивно/не флипаются вовсе |

**Ключевое:** трекер — НЕ 287 задач. 🔍 — это «проверено, не инвентарная фича». Реальный трекер-остаток =
**4 🔄 + пиннинг-флипы ~10-15 D**, привязанных к инвентарь-функциям (а). Группировка 🔄/пиннинг по функциям:
- **Result/Option/sum** (Q2/Q5): d30, d85×2, d325_*, d52_*, d407, d406 — блок «Call-return через node_substs».
- **elem/mono** (Q6/Q10): d239, d373, d403, d411, d277, d123 — блок «канал уже есть, дожать».
- **static/generic-static/ctor** (Q1/Q+): d182, d372 (✅), d143, types_generic_static_ctor.
- **generics/протоколы** (Q9): d16, d53, d72, d42, d355, d135.

### (в) Замороженная зона волны-1 (`infer_call_ret_c` 49943-52037, **50 живых веток**) — что её разморозит

Ветки живут по волнам реестра 196.2 (W1 ядро уже флипнуто; остаток по группам):
- **W2 static-ctor/free-fn (~7):** B01, B10e, B10f, B10j×2, B10l, B11a — разморозит **Q1 static-return + generic-fn-mono канал**.
- **W3 protocol/blanket/serde (~6):** B03, B08, B08r, B11ac, B11ai, B11ak — разморозит protocol-return канал + serde-fix (§4).
- **W4 variant-ctor/newtype (~6):** B02, B10c, B10h, B11q, B11r, B12p — разморозит **Q2 sum-методы + variant-ctor канал**.
- **W5 builtin-intrinsic fixed-return (~11):** B11d, B11e, B11f, B11j, B11k, B11m, B12b, B12h, B12l, B12o, B_overflowing_ints — разморозит **CAP-DECL-RET** (декларированный возврат из `external_registry`/prelude-сигнатуры; census §3.2/3.3).
- **W6 тривиал + терминалы (~7):** B10a, B10m, B11ah, B11al(panic), B12q(panic), B12r(panic), B12s(panic) — B11ah единственный нулевой-снимаемый (census); терминалы-паники уходят с финальным сносом функции.

**Разморозка = НЕ правка frozen-зоны флотом, а насыщение канала так, чтобы Channel-1/2 диспетчера выиграл →
ветка недостижима → её удаляет ЕДИНСТВЕННЫЙ frozen-агент** (В-1/Stage-D, серийно, владеет 49943-52037 монопольно).

### (г) Хвосты вне волны-2 (КРОМЕ Ф.C — параллельный агент p196-facetC)

- **Ф.B — резолв→`FnDecl`** (generic-static + кросс-модуль на общий резолвер, `external_registry` ≠ отдельный путь):
  частично закрыт `resolved_callees` (196.7); остаток — унификация generic-static кросс-модуль. НЕ отдельный план.
- **Ф.D — приватные** (checker-доступ D267/D281): в осн. закрыт. **`priv(file)` generic-mono bleed — ОТЛОЖЕН
  владельцем** (2 теста в `standalone/`, merged-CU зелёный; рецепт в [196-facetB-privfile-notes.md](196-facetB-privfile-notes.md)). Не блокирует финал.
- **Vec.data ABI-хардкод** (`->data` в быстрых путях emit_c): close-out ГРЕП — должны уйти с легаси-путями;
  пережившие отвязать явно. Пререквизит Plan 200 (`Vec.data`→`ptr`).
- **raw-decode CI-линт** (0 raw `Nova_`/`____`-decode вне `debt_`; дрейфануло до 12) — восстановить + под CI.
- **`ResolvedType::Raw` снос** + **wildcard→ГРОМКИЙ panic (D368)** — только когда class-C достроен и wildcard недостижим.
- **2 пре-существующих P67-паники** (census §1): `deserialize` Path-return (serde, гасит терминал B12q) + `str.until`
  (гасит B11al) — чинить в ЧЕКЕРЕ (zero-tolerance вход), НЕ в легаси. Их фикс осушает 2 терминала frozen-зоны.

### (д) Критерий финального закрытия («второе окно снесено»)

СТРУКТУРНЫЙ ФИНАЛ-ГЕЙТ (по построению доказывает полноту — забытое ловится как dead-code/mis-type):
1. `infer_call_ret_c` **удалён** (0 недостижимых строк) + оба call-сайта сняты; 3 терминал-P67-паники недостижимы → ушли с функцией.
2. `infer_expr_c_type` (52065) схлопнут в **Кан.1-2 → `resolved_type_to_c(resolved_types[id])`** — 0 независимой инференции; 251 консумер untouched (либо инлайн-косметика).
3. Все 50 веток слиты; каждая инвентарь-функция — через ОДНО окно (чекер→канал→codegen читает).
4. Компилятор **СОБИРАЕТСЯ** (ничего не осиротело).
5. Гейты: **conformance мега-CU 470/0** + **byte-parity** + **флагман `examples/flagship/aggregator` `--strict-effects`** зелёный (app-регрессию conformance не ловит — test-conventions прецедент 206) + `nova test std` без новых фейлов.
6. `[M-172.1-lifted-legacy-arms]` закрыт; raw-decode invariant = 0 под CI; Vec.data-хардкод греп чист.

---

## 2. РАЗДАЧА НА ФЛОТ — непересекающиеся зоны

**Принцип партиции:** ≤2 агента в `emit_c.rs` одновременно на заведомо разнесённых line-band'ах; ≥1 агент ВСЕГДА
вне emit_c (types/mod.rs-канал ИЛИ conformance/std); frozen-зона (49943-52037) — монопольно frozen-агенту;
Ф.C (callnorm/argbind) — НЕ раздаю (p196-facetC активен). Line-band'ы флота (17838-21432 верх, 46381-47700 низ)
ЛЕЖАТ ВЫШЕ frozen-зоны → снос веток внутри 49943-52037 сдвигает строки ТОЛЬКО ниже 52037 → band'ы флота не едут.

### Зона CH — types/mod.rs, канал (ФУНДАМЕНТ, отдельный файл) — sonnet

**Объём:** завершить продюсеры `node_substs`/`resolved_types` для КЛАССА Call-return, который Q5/Q2 требуют
(Result/Option/sum/static возврат вызова). Канал уже есть (продюсеры A/B/C @ 8208/11297/14219/15118/15208 +
`shadow_check_node_substs`) — дописать покрытие Call-`ExprId` для форм, что чекер сейчас пропускает
(`typeref_mentions_any`-guard ~10478 «skip generic-возврат»). **propose-then-verify** (материализация только при
согласии solver-канала — прецедент 196.4 `resolve_return_channel`).
**Порядок:** сперва sum/Result-return (кормит Q2/Q5) → затем static-return (кормит Q1).
**Риски:** правка `f1_check_call`-соседства (Q10-forbidden зона) — держать ADDITIVE, гейт SHADOW-assert 0-mismatch,
НЕ флипать на authoritative без cross-check. **Оценка:** 1-2 сессии (фундамент по большей части есть).
**Файлы:** `types/mod.rs` только. Ноль коллизии с emit_c-агентами.

### Зона GEN — emit_c 19485-21432 (generics/mono/type-param/protocol/elem) — sonnet

**Объём:** Q6+Q9+Q10+Q+ctor одним контуром (эти функции взаимозависимы по строкам и вызовам — НЕ дробить):
- `resolve_mono_type_args`(19665)→завершить флип на `_ch`(20094), снести legacy-движок (D119/D122); `resolve_method_level_subst`(20958) тем же.
- `infer_type_param_binding`×3 (2805/20375/20569) + `infer_protocol_structural_binding`(20435) → чекер (D16/D53/D72/D42/D355).
- `compute_array_elem_type_for_obj`(21432) → **снять `.or_else` fallback** (D239/D373) — ТОЛЬКО после расширения канала на контейнер-cap (см. §4.1).
- `infer_generic_static_ctor_ret`(19485) → чекер (D372).
**Порядок D:** D119/D122 (канал есть, дожать) → D16/D53/D72 → D239 (последним, fallback-риск) → D372.
**Риски:** «три hand-duplicated inference engines» (census: `resolve_mono_type_args` Source 4) — свести ВСЕ; D239
откат 93/2 (§4.1). **Оценка:** 3-5 сессий. **Зависит от:** Зона CH (node_substs для mono-классов — уже частично).

### Зона RET — emit_c 17838-19000 + 46381-47700 + 49352 (Result/Option/sum/static/closure returns) — sonnet

**Объём:** Q1+Q2+Q5+Q+lambda:
- `infer_result_type_params`(17838, уже split `_channel`/`_legacy`) → добить `_channel`, снести `_legacy`.
- `resolve_result_option_ret`(18956) + `resolve_result_te(_strict)`(47601/47614) → чекер + panic (D30/D85/D325).
- `infer_method_level_return_for_sum(_inner)`(46955/46967) → чекер + panic (D52/D407/D406).
- `infer_static_method_ret` (кластер 46381-47700) → чекер + panic (D182).
- `infer_lambda_return_type_with_params`(49352) → верифицировать residual (closure-канал есть), снести если недостижим.
**Порядок D:** D30/D85 (Q5, канал из Зоны CH) → D52/D407 (Q2) → D182 (Q1) → closure-residual.
**Риски:** ВСЕ SHARED с frozen — правило **detach+panic, НЕ delete** (удалит frozen-агент). Байт-parity ПОКА
legacy жив. **Оценка:** 3-5 сессий. **Зависит от:** Зона CH (Call-return канал).

### Зона TEST — conformance/std (вне emit_c) — sonnet ИЛИ haiku

**Объём:** опережающее покрытие + пиннинг ПЕРЕД миграцией (Track-A ∥ migration):
- Per-D пиннинг-тесты (assert ИМЕННО тип/значение, что резолвит ветка — ловит регресс миграции) для D30/D85/D52/D182/D16/D53/D239.
- Регресс-фикстуры для reachable-но-нетестируемых frozen-веток (census: B11ac effect, B11ak recursive, B10f fn-sigs).
- Read-only reachability-trace prep для frozen-агента (re-инструментация `icr_trace` под env-флагом — ВРЕМЕННО, вычистить).
**Риски:** тесты компилятор НЕ трогают → параллельно-безопасны. **Оценка:** непрерывно, опережая. **Файлы:** `spec_tests/conformance/`, `std/**/*_test.nv`.

**Прогресс (2026-07-16, sonnet, worktree `nova-196test`, ветка `p196-zone-test`):** census-пробелы
B11ac/B11ak/B10f запинены — `d61_effect_handler_direct_call.nv` (D61 §8 direct-call-on-handler-value,
до этого только `examples/effects/effects_d61.nv`, вне гейта), `self_recursive_generic_method_return.nv`
(`[M-generic-method-self-recursive-return]`, генерик-метод рекурсивно зовёт себя на своём receiver-типе),
`dispatch_free_fn_vs_method_name.nv` (`B10f_user_fn_sigs` порядок free-fn vs same-named метод). Все три —
изолированный standalone-прогон PASS (полный CU не гонялся, задание исключало). Побочная находка (НЕ
фикс, вне зоны): `[M-novavtable-read-write-pointer-collision]` (backlog-followups.md, P2) — нуль-арный
`.read()`/`.write(v)` на `NovaVtable_<Eff>*` мисдиспатчится в `B11d_typed_pointer_methods` (guard не
исключает префикс `NovaVtable_`). Per-D сверка D30/D85/D52/D182/D16/D53/D239 (задание Зоны TEST) — ПО КОДУ
подтверждено: существующие фикстуры уже пиннят конкретные типы/значения по всей матрице (см.
`196.3-wave2-d-driven.md` собственный gap-анализ + `196.5-perd-d52-verification.md` для D52/D407/D406) —
новых файлов не требовалось, дублирования не создавал. Коммит `570879d55` (ветка `p196-zone-test`, НЕ
смёржена в main — на решение оркестратора).

### Зона FROZEN — emit_c 49943-52037 (капстоун В-1/Stage-D, СЕРИЙНО, монопольно) — sonnet

**Объём:** слить 50 живых веток ПО МЕРЕ насыщения канала зонами CH/RET/GEN (`resolve_instance_call_subst` +
node_substs). Порядок census: снять B11ah (нулевой) → точечная композиция микро-трафика (§3.2 census: B10a/B10c/B10d/B11m/B12c/B11ag/B11x — CAP-DECL-RET) → W5-builtins → терминалы+финал (удалить функцию + call-сайты 49943→dispatcher-схлоп).
**Риски:** НЕ параллелить с Зонами RET/GEN в ОДНОМ окне времени сверх ≤2-в-emit_c — **frozen-агент активен, когда
свободен слот** (см. §3 расписание). **Оценка:** длинный шест — ~6-10 сессий (50 веток, серийно). **Критический путь.**

### Haiku-задачи (механика по спискам, отдельно)

1. **Трекер-обход:** прогнать 🔍-строки трекера (287) — верифицировать coverage-ok по коду грепом (не отчёту), пометить дрейф.
2. **Per-D тест-пины:** сгенерировать пиннинг-скелеты `d<NNN>_*.nv` по списку из Зоны TEST (механическая near-copy проверенного синтаксиса).
3. **Vec.data-хардкод греп:** финальный `->data` аудит по emit_c (close-out критерий г).
4. **Конфликт-маркер/index-гейты:** перед каждым батч-слиянием `git diff --cached --stat` + греп маркеров одной командой.
5. **STATUS.md регенерация** после каждого батча (`bash scripts/tools/gen-plan-status.sh`).

---

## 3. БАТЧ-ПЛАН СЛИЯНИЙ (санкционированное отступление: 3-5 D на ОДИН авторитетный гейт)

> **Отступление от per-D-слияния 196.3 — санкционировано этой картой:** per-D-гейт (мега-CU 470/0 + флагман
> `--strict-effects` + byte-parity) стоит ~10-15 мин прогона × 25+ D = неприемлемо для «ASAP». Группирую 3-5
> когерентных D (одна инвентарь-функция/канал-класс) на ОДИН авторитетный гейт. Приёмка (0)-(5) ПО КОДУ per-D
> сохраняется (греп+чтение); ослабляется ТОЛЬКО частота полного гейта, не строгость приёмки.

| Батч | D-группа | Зона | Гейт | Зависит |
|---|---|---|---|---|
| **B1 — фундамент** | (канал, без D-закрытия) node_substs/resolved_types Call-return + shadow-assert | CH | 470/0 + SHADOW 0-mismatch | — |
| **B2 — mono/generics** | D119, D122, D354, D277, D123 | GEN | 470/0 + byte-parity + флагман | B1 (частично готов) |
| **B3 — Result/Option/sum** | D30, D85, D325, D52, D407 | RET | 470/0 + byte-parity + флагман | **B1** |
| **B4 — static/ctor/elem/closure** | D182, D372, D239, D48, D373 | GEN+RET | 470/0 + byte-parity + флагман | B1, B2 |
| **B5 — generics/протоколы** | D16, D53, D72, D42, D355 | GEN | 470/0 + byte-parity | — (SEP) |
| **B6 — КАПСТОУН** | frozen-слив 50 веток + serde/str.until P67-фиксы + удалить `infer_call_ret_c` + dispatcher-схлоп | FROZEN | **ФИНАЛ-ГЕЙТ §1д** (470/0 + byte-parity + флагман + compiles + raw-decode CI) | B1-B5 |

**Порядок по зависимостям:** B1 → {B2 ∥ B3 ∥ B5 параллельно} → B4 → **B6 (серийный капстоун, критический путь)**.
Каждый батч = отдельная ветка `p196-batch-N`, FF в main после зелёного гейта; **пушить main сразу после каждого
зелёного авторитетного гейта** (fetch→behind==0→push, стоячее правило). Язык-меняющих слияний тут нет (миграция
эквивалентна по поведению); если всплывёт D-амендмент (serde/str.until spec-дыра) — дописать в том же слиянии.

---

## 4. Известные ловушки из истории (правило приёмки оркестратором — ПО КОДУ)

1. **D239-fallback снятие → откат 93/2 (`bdf880c10`).** Снятие `.or_else` на 4 вне-frozen сайтах дало 93/2:
   `channel_array_elem_c` НЕ покрывает **контейнер-cap** кейсы (`StringBuilder.new(128)`, d371 cap-арность).
   **Правило:** Zone GEN снимает D239-fallback ТОЛЬКО после расширения канала на контейнер-cap И проверки на
   **ПОЛНОМ мега-CU** (не 4-site sample). До того D239 остаётся 🔄.
2. **Legacy разбросан → «три hand-duplicated inference engines».** `resolve_mono_type_args` Source 4 сам
   документирует: один баг чинился синхронно в ТРЁХ местах (canonical-симптом второго окна). **Приёмка (3):**
   грепнуть ВСЕ места, обслуживавшие D; свести в ОДНО окно; убедиться ПО КОДУ, что параллельного legacy-обработчика
   той же фичи в другом месте НЕ осталось.
3. **SHARED-сиблинги (зовутся из frozen-зоны): detach+panic, НЕ delete.** Q1/Q2/Q5/Q+mono — `infer_static_method_ret`,
   `infer_method_level_return_for_sum`, `resolve_result_option_ret`, `infer_mono_method_ret` имеют вызовы ВНУТРИ
   49943-52037. Мигрировать логику в чекер + `panic!("[MIGRATED-<D>]")`; физическое удаление — капстоун B6, когда
   frozen-агент опустошил вызовы. Тихий legacy-fallback ЗАПРЕЩЁН (ловушка co-authority).
4. **Байт-parity протокол.** Baseline = дважды эмитнуть `.c` на ТОМ ЖЕ бинаре ДО правки (отделить
   `[M-codegen-emission-nondeterminism]`); порядок атома СТРОГО: (1) материализовать в чекере → (2) byte-parity
   ПОКА legacy жив → (3) нейтрализовать legacy → (4) re-parity + conformance. Расхождение на (2) = чинить
   материализацию (fallback ловит, не паникует).
5. **propose-then-verify для node_substs.** Материализация в канал ТОЛЬКО при независимом согласии solver-канала
   (`shadow_check_node_substs`/SHADOW-assert, прецедент 196.4). НЕ флипать SHADOW→authoritative без cross-check —
   иначе регресс класса, ранее ВСЕГДА бейлившего.
6. **Frozen-зона ≠ «трудное сделано» + карве-аут метрики.** Гейт-1 (убыль строк `infer_call_ret_c`) для
   panic/terminal/scaffold-веток (B11al/B12q/r/s/B06) = закрытые trace-id, НЕ строки (уходят на финале). КРАСНЫЙ
   для них = 0 закрытых id, не 0 строк.
7. **Приёмка (0)-(5) ПО КОДУ, не по отчёту агента (196.3 §ПРИЁМКА).** Оркестратор ОБЯЗАН греп+чтение:
   (0) одно окно в ПРАВИЛЬНОМ месте (чекер/канал, НЕ новый codegen-сайт = «второе окно №2»);
   (1) legacy для D МЁРТВ (грепом: работа физически не может пойти через legacy);
   (2) разбросанное сведено в одно;
   (3) полное покрытие ВСЕХ ситуаций матрицы (не happy-path);
   (4) гейты зелёные;
   (5) node_substs cross-check без mismatch. Только тогда статус D → ✅.

---

## Сводка для оркестратора (цифры)

- **Остаток очереди волны-2:** ~7-8 инвентарь-функций (Q1/Q2/Q5/Q6/Q9/Q10/Q+ctor); из них 2 (Q6/Q10) = «дожать
  начатое» (канал есть). Трекер: **~17 ✅ / 4 🔄 / ~287 🔍-coverage** — реальная работа ≈ 4 🔄 + ~10-15 пиннинг-флипов.
- **Frozen-зона:** **50 живых веток** (было 114), `infer_call_ret_c` = 49943-52037 (~2094 стр). Слив = капстоун B6.
- **Агентов реально:** **4 параллельных sonnet** (CH-канал / GEN-emit_c-верх / RET-emit_c-низ / TEST-conformance) +
  **1 серийный sonnet frozen-капстоун** + haiku-механика. Ф.C (p196-facetC) — уже занят, не раздаю.
- **Критический путь:** B1 (фундамент, ~1-2 сессии) → RET/GEN миграция (параллельно, ~3-5 сессий) → **B6 frozen-слив
  (серийный, ~6-10 сессий) — ДЛИННЫЙ ШЕСТ.** Календарь при батч-гейтах и агрессивной параллели: **~4-7 дней
  wall-clock**, доминирует серийный frozen-капстоун (50 веток, один агент, монопольная зона).
- **Ускоритель:** node_substs-канал уже построен (Q5/Q10 разблокированы) + W1-i готово + 196.7/8/9 влиты — кампания
  существенно де-рискована относительно чтения планов; «фундамент отсутствует» — устаревший тезис.
