<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — «Одна правда»: удалить второе окно `infer_expr_c_type`

**Статус:** 🔥 IN PROGRESS (НЕ ЗАКРЫТ — см. «Итог финальной closeout-волны
2026-07-21» в конце файла: 1 продюсер-gap закрыт, 3 честных отрицательных
вердикта «жив», 5 терминал-остатков задокументированы, снос НЕ выполнен).
**КУРС-КОРРЕКТИРОВАН 2026-07-12**. **Приоритет:** P0 (ключевая
идея 172-186). **Умбрелла над:** 172.1 (U-хвосты), 172.12, 172.13. Координирует, НЕ дублирует.

---

## ⚑ Курс-коррекция (2026-07-12) — читать ПЕРВЫМ

Направление §0 верно и НЕ менялось: свести всё в `resolved_types` (одно окно, D315) → удалить второе окно.
**Ф.1/Ф.2 сделали это правильно (21 арм).** Продолжаем ИМ.

**Тупики и промежуточные мисдиагнозы вынесены в [196-retracted](196-retracted.md)** (co-authority solver
Ф.4a/b/c — ~месяц verify-and-discard, 0 снято; «ExprId-across-mono = блокер»; «B07 = carrier»; опасение
устаревших координат; др.) — чтобы активный план был чистым и ошибки не повторялись. Ключевой урок
(verify-and-discard ≠ materialize-and-delete; гейт-прогресса; спайк-на-авторитет) закреплён в конвенции §0/§7
(`b7a45bf7a`).

**MIR вынесен из 196** (Стадия-2, отдельный горизонт; borrow-check/оптимизации). §0 его НЕ требует: `ResolvedType`
уже C-lossless, нужна лишь **полнота** `resolved_types` (см. «Грунтовка» + «MIR и rustc-эталон» ниже).

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
| 3 | Аргументы (arg↔param) | `callnorm`/`argbind` | нет | ⚠ зависит от (1); ✅ `T.deserialize(d)`-форма (generic type-param как static-call target, D35) пробрена и ЗАКРЫТА 2026-07-17 — named-arg через type-param был ТИХИЙ МИСДИСПАТЧ (ни чекер, ни callnorm не резолвят литерал `"T"` как тип; codegen эмитил args позиционно, игнорируя `CallArg::Named`), исправлено guard'ом `[E_GENERIC_STATIC_NAMED_ARG_UNSUPPORTED]` в `f1_check_call` (см. 196.5-facet-c-map §7); плейн-позиционная форма (serde.nv's реальное использование) работала и работает корректно |
| 4 | Generic-аргументы (вывод type-arg) | generic-инференс чекера + `callnorm` | нет | ⚠ method-turbofish (`obj.method[U](...)`) × default-arg крашил (ICE) — **закрыто 2026-07-16** (guard в `try_normalize_call`, см. 196.5-facet-c-map §6); свободный free-fn overload-arity gap — ✅ ГОТОВ 2026-07-21 (`[M-196-freefn-arity-overload-default-ret-mismatch]`, ветка `p-fix-freefn-arity`, НЕ влито — интегратор заберёт; детали `backlog-followups.md`) |
| 5 | Default-арги | `callnorm` backfill (`:485`) | нет | ✅ generic-static+кросс-мод ЗАКРЫТО 2026-07-12 (`[M-vec-new-cap-default-arg-backfill]`, main `bdf880c10`/`6d0c24447`, Plan 200 п.1) — сверено по коду 2026-07-16, было стале здесь |
| 6 | Generic default-арги | пересечение (4)+(5) | нет | ✅ `Vec.new(cap int=0)` + HashMap/Queue/Set/StringBuilder/WriteBuffer ЗАКРЫТО тем же коммитом (d372_generic_static_default_cap.nv) — сверено 2026-07-16, было стале здесь |

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

**★ ДОПОЛНЕНО (полнота, греп 2026-07-12 — БЫЛИ ЗАБЫТЫ, теперь в инвентаре → волна-2):**
- Возврат/тип: `result_repr_c_type`, `channel_int_c_type`, `infer_func_c_name`, `infer_handler_interrupt_ty`.
- Receiver/generic: `receiver_c_type`, `builtin_sum_receiver_c_type`, `value_aware_generic_c_type`,
  `extract_result_type_params`, `call_result_type_params_key`.
- **Хардкод метод→C (§3 → из .nv-деклараций, как `primitive_instance_method_known`):** `f64_method_to_c`,
  `int_method_to_c`, `primitive_name_to_c` (+ волне-2 грепнуть str/char/bool-аналоги — вероятно есть ещё).
- ✅ ОСТАЁТСЯ (лоуеринг, часть `resolved_type_to_c`): `resolved_array_to_c`, `resolved_named_to_c`. Хелперы-
  предикаты (`is_struct_c_type`, `type_ref_uses_any_type_param`, `debt_*`) — НЕ re-derivation, не мигрируются.

**★★ ГАРАНТИЯ ПОЛНОТЫ (чтобы не «забыть опять»):** определяющий набор второго окна = ПОЛНОЕ дерево вызовов
`infer_expr_c_type` (вход, 249 консумеров) + `infer_call_ret_c`. Grep-список — стартовый, НЕ доказательство
полноты. **Финальная СТРУКТУРНАЯ проверка:** когда `infer_expr_c_type` УДАЛЁН и компилятор СОБИРАЕТСЯ — ничего
не осиротело (что не мигрировано/не-остаётся → dead compile-error). Волна-2 ОБЯЗАНА пройти по всему call-tree
`infer_expr_c_type`, а не только по grep-списку; любая найденная re-derivation-функция → в инвентарь + мигрируется.

**★★★ СТРУКТУРА `infer_expr_c_type` (прочитан 2026-07-12) — почему инвентарь НЕ доказательство, а финал ДА:**
Тело диспетчера = каналы по приоритету: **Кан.1** `resolved_callees`→`fn_ret_by_span`; **Кан.2** `resolved_types`
→`resolved_type_to_c`; **Кан.3-4** `var_types` (Ident/SelfAccess/Path-static); **Кан.5-6z + `infer_call_ret_c`**
= legacy-перевывод. ⚠ **Часть legacy — НЕ отдельные функции, а ИНЛАЙН в теле `infer_expr_c_type`** (`try_as`-turbofish,
generic-wrapper-mono-предпроба, `str.from`/`fn_ret_{recv}_{name}`-хардкоды) — грепом функций НЕ ловится.
- **Остаётся (одно окно):** Кан.1-2 (чтение каналов) + `resolved_type_to_c`. Кан.3-4 (var_types) — codegen-state,
  мигрируют/остаются по месту.
- **Legacy к удалению:** Кан.5-6z + `infer_call_ret_c` (волна-1) + INLINE спец-кейсы + sibling-функции (волна-2).
- **ДОКАЗАТЕЛЬСТВО ПОЛНОТЫ = финальный структурный гейт (НЕ upfront-инвентарь):** `infer_expr_c_type` сведён к
  «Кан.1-2 → `resolved_type_to_c`», всё прочее удалено; компилятор СОБИРАЕТСЯ + conformance 95/0 + byte-parity →
  по построению НИЧЕГО legacy не осталось (не мигрированное = dead compile-error либо mis-type, пойманный тестом).
  Забытое в списке ловится ЗДЕСЬ. Инвентарь ускоряет работу, финал — гарантирует полноту.

**Финал 196** = это семейство удалено/сведено к `resolved_type_to_c(resolved_types[id])` + каналы кормят LSP
(hover/completion). Это НЕ «пара веток», а систематическая переархитектура потока типов/резолва — БЕЗ полного
MIR (mono остаётся lazy в codegen; каналы лишь дотягиваются до mono-копий, см. Ф.A / A-спайк).

### ★ Целевая архитектура «одного окна» (дизайн 2026-07-12) — куда переезжает каждое место

**Поток фаз:**
```
Parse + import-inline → AST + ExprId  (number_exprs + number_unset_exprs ВО ВСЕХ путях, вкл. build)
      │
      ▼
ЧЕКЕР (ОДНО ОКНО, PRE-mono) — резолвит ОДИН раз, наполняет каналы:
  • Резолв вызова → FnDecl       →  resolved_callees: ExprId → FnDeclRef
      (method/static/free/generic-static/кросс-модуль — ОДИН резолвер; external_registry + .nv-декларации
       кормят его; codegen-дубля НЕТ)
  • Тип выражения → ResolvedType  →  resolved_types: ExprId → ResolvedType (generic, с TypeParam)
      (возврат вызова, sum/static/ctor/closure/trailing/Result — ВСЁ тут; generic-биндинги записаны В тип)
  • Нормализация вызова           →  именованные арги по порядку + default'ы (AST-переписывание,
      ЧИТАЕТ resolved_callees) — callnorm/argbind переезжают в чекер-фазу
  • Приватность → контроль доступа (остаётся в чекере)
      │ каналы
      ├──────────────────────────► LSP/IDE читает resolved_types + resolved_callees (hover/completion не слепые)
      ▼
CODEGEN (POST-mono, ЧИСТЫЙ ЛОУЭРИНГ — НИКОГДА не перевыводит):
  • resolved_type_to_c(resolved_types[id], current_type_subst)  ← ЕДИНСТВЕННЫЙ тип→C
      (подставляет TypeParam→конкретику на mono; сюда сложена регистрация mono-инстансов)
  • mono-имена/инстансы: compute_mono_name, compute_generic_type_c_name (законный лоуэринг)
  • читает resolved_callees — какую функцию эмитить
```

**Судьба каждого места (из инвентаря):**

| Старое место | Судьба | Куда конкретно |
|---|---|---|
| `infer_expr_c_type`, `infer_expr_c_type_str` | 🗑 УДАЛИТЬ | → `resolved_type_to_c(resolved_types[id])` (чтение канала) |
| `infer_call_ret_c` (114) | 🗑 УДАЛИТЬ | резолв возврата → ЧЕКЕР → `resolved_types` |
| `infer_mono_method_ret(_with_args)` | ➡ ЧЕКЕР | резолв generic-возврата → `resolved_types`; mono-подстановка остаётся в `resolved_type_to_c` |
| `infer_method_level_return_for_sum` | ➡ ЧЕКЕР | возврат метода sum → `resolved_types` |
| `infer_static_method_ret` | ➡ ЧЕКЕР | возврат static → `resolved_types` |
| `infer_generic_static_ctor_ret` | ➡ ЧЕКЕР | возврат generic-static-ctor → `resolved_types` |
| `infer_lambda_return_type_with_params` | ➡ ЧЕКЕР | возврат замыкания → `resolved_types` |
| `infer_trailing_block_sig` | ➡ ЧЕКЕР | типы trailing-блока → `resolved_types` |
| `resolve_result_option_ret`, `resolve_result_te(_strict)` | ➡ ЧЕКЕР | часть `ResolvedType` (T/E из Result/Option) |
| 56× `_c_type`/`_to_c` | 🗑 УДАЛИТЬ | → `resolved_type_to_c` (чтение) |
| `infer_type_param_binding` ×3, `infer_protocol_structural_binding`, `infer_result_type_params` | ➡ ЧЕКЕР | generic-инференс; решение записано В `resolved_types` (TypeParam/конкретика) |
| `resolve_method_level_subst` | ➡ ЧЕКЕР | subst записан в резолв; на codegen только `current_type_subst` (mono) |
| `compute_array_elem_type_for_obj` | ➡ ЧЕКЕР | тип элемента = часть `ResolvedType` receiver'а |
| `resolve_mono_type_args` | ✅ ОСТАЁТСЯ (lowering) | mono, но ЧИТАЕТ резолв + `current_type_subst`, не перевыводит |
| `compute_mono_name`, `compute_generic_type_c_name` | ✅ ОСТАЁТСЯ (lowering) | mono-именование = законный codegen |
| `register_generic_instances_in_typeref` | ✅ ОСТАЁТСЯ (сложена) | внутрь `resolved_type_to_c` (P2) — driven лоуэрингом, не резолвом |
| `callnorm.rs`, `argbind.rs` | ➡ ЧЕКЕР-фаза | нормализация вызова ЧИТАЕТ `resolved_callees` (не ре-резолвит) |
| method/static-резолверы | ➡ ЧЕКЕР (един) | ОДИН резолвер → `resolved_callees`; codegen-дубль удалить |
| `external_registry` | ➡ кормит ЧЕКЕР | кросс-модуль/FFI-декларации в резолв чекера (не отдельный codegen-путь) |
| `primitive_instance_method_known` | 🗑 УДАЛИТЬ (§3) | возвраты → из `.nv`-деклараций примитивов (maximize-nv-sourcing); хардкод-зеркало снести |
| `type_ref_to_c` | 🗑 УДАЛИТЬ (D315) | → `resolved_type_to_c` |

**Каналы (интерфейс чекер → codegen → LSP):**
- `resolved_types: ExprId → ResolvedType` — тип каждого выражения (generic, с TypeParam). Уже есть (D315),
  НЕПОЛОН → 196 (Ф.A) доводит.
- `resolved_callees: ExprId → FnDeclRef` — к какой декларации привязан вызов. Частично есть → 196 (Ф.B)
  доводит + унифицирует (generic-static/кросс-модуль; убрать codegen-дубли и `external_registry`-обход).
- **Нормализованный вызов** (арги по порядку + default'ы) — AST-инвариант после чекер-нормализации (Ф.C).
Codegen лоуэрит из каналов; LSP читает ТЕ ЖЕ каналы. **Приёмка 196 = ни одна `infer_*`/`resolve_*` в codegen
не перевыводит; всё из канала; матрица зелёная.**

### ★ Сверка с Rust (2026-07-12) — mono = ФАЗА, не codegen-side-effect; поправка к «ОСТАЁТСЯ»

**rustc-эталон:** типы ОДИН раз в typeck (HIR)→`TyCtxt`; HIR→THIR→**MIR**; **мономорфизация — ОТДЕЛЬНАЯ ФАЗА**
(`rustc_monomorphize::collector`: обход достижимого от корней → worklist `Instance`=def_id+substs, НЕ
side-effect резолва); манглинг — отдельная фаза (`rustc_symbol_mangling`, чистая fn от `Instance`); codegen
(`rustc_codegen_ssa/llvm`) берёт МОНОМОРФИЗОВАННЫЙ MIR, подставляет ИЗВЕСТНЫЕ substs, лоуэрит ty→LLVM через
`tcx`-запросы — **codegen НИКОГДА не инферит.**

**Поправка к строке «✅ ОСТАЁТСЯ в codegen»:**
- `resolved_type_to_c` (ty→C, читает канал) — **ВЕРНО, совпадает с Rust** (codegen лоуэрит ty→backend).
- Mono-машинерия — БЫЛО СЛИШКОМ РАЗМЫТО. В Rust это ФАЗА, не ad-hoc:
  - `register_generic_instances_in_typeref` как **side-effect резолва = АНТИ-паттерн** (Rust: отдельный
    collector-проход). Stage-1-цель: переструктурировать в **mono-COLLECTOR-проход, читающий каналы**, а не
    складывать в resolve/лоуэринг.
  - `resolve_mono_type_args` — обязана быть чистой **ПОДСТАНОВКОЙ** известных substs (Rust `subst`), НЕ
    инференсом; если инферит — баг.
  - `compute_mono_name`/`compute_generic_type_c_name` — манглинг = чистая fn от инстанса (ок как функция).

**Итог по mono (честно): наш «без MIR» = сознательный Stage-1-КОМПРОМИСС, не конечная архитектура.**
- **Stage-1 (196):** mono ОСТАЁТСЯ в codegen, но как **collector-проход + чистая subst/mangle, читающие
  каналы** (не resolve-side-effect). Тип-лоуэринг уже rustc-образен.
- **Stage-2 (MIR, будущее):** типизированный IR + отдельная mono-фаза = ровно rustc-модель. Это и есть «полный
  MIR», отложенный из §0. 196 НЕ обязан его достигать — но и НЕ должен закреплять mono-side-effect как «норму».

### ★ Что такое MIR и почему AST-only хуже; rustc как ЭТАЛОН (директива владельца 2026-07-12)

**MIR (Mid-level IR, rustc):** промежуточное представление между HIR и LLVM — программа, «расплющенная» в
простые ТИПИЗИРОВАННЫЕ шаги + явный control-flow-граф (базовые блоки + переходы). Свойства: (1) полностью
типизирован (тип каждого локала/темпа известен из typeck, НЕ перевыводится); (2) явный (drop'ы, временные,
autoref/deref, перегрузки операторов — всё развёрнуто); (3) ОДИН субстрат для borrow-check, dataflow,
оптимизаций (const-prop/inline/DCE) и mono/codegen. Короче: **единственный типизированный IR, несущий ВСЕ
резолвнутые факты; всё ниже ЧИТАЕТ его.**

**Чем у нас ХУЖЕ — типизированного IR НЕТ, AST единственный:**
- Негде ХРАНИТЬ резолв → codegen ПЕРЕвыводит (всё семейство «второго окна»). Дубли, дрейф, §0-баги.
- `resolved_types` — **side-table-ЗАПЛАТКА** (ExprId→тип сбоку от AST), симулирующая то, что typed IR даёт
  НАТИВНО; оттого НЕПОЛНА/протекает (дыры, что 196 латает).
- Нет CFG → control-flow/concurrency-лоуэринг ad-hoc на AST.
- Нет чистого разделения фаз → mono = codegen-side-effect (не фаза); borrow-check-подобное трудно; opts ad-hoc.
- LSP слеп → инференс codegen-only, не в запрашиваемой типизированной форме.

Это НЕ «плохо по глупости» — AST-only проще на старте; цена вылезла сейчас (второе окно). **196 (Stage-1)
симулирует одно окно side-table'ом БЕЗ полного IR; Stage-2 = ввести MIR = нативная rustc-модель.**

**★ УПРАВЛЯЮЩИЙ ПРИНЦИП (директива владельца): rustc-реализация — ЭТАЛОН.** При ЛЮБОМ архитектурном решении
по типам/резолву/mono/IR — сверяться с тем, КАК это в rustc (typeck→HIR→THIR→MIR→mono-collector→
symbol-mangling→codegen-читает). Наши отклонения = ТОЛЬКО осознанные компромиссы (напр. Stage-1 без MIR),
ЯВНО помеченные «компромисс, не идеал», а не выданные за «нашу норму». Rust — зрелый корректный референс;
изобретать своё вопреки ему = риск ещё одного «второго окна».

## ★ Две встречные волны миграции — ОТДЕЛЬНЫЕ под-планы (директива владельца 2026-07-12)

Миграция второго окна шла двумя волнами; **2026-07-13 волна-1 слита в конвейер [196.5](196.5-node-substs-channel.md) как Stage-D-дорожка** (одна очередь/гейты; 196.2 = справочник-реестр веток). Под-планы:
- **Волна-1 (bottom-up) = [196.2 — class-C relocation](196.2-class-c-relocation.md)** — по 114 веткам
  `infer_call_ret_c`, удаляет legacy-ветки (gate-1); P0-hit-count, атом-чеклист, спайки, протокол
  materialize→parity→remove/panic. [opus, nova-p196]. Держит `infer_call_ret_c` (46293-48883). **Актуален и
  активен** (первое gate-1 снятие −17, B11w).
- **Волна-2 (top-down, D-driven) = [196.3 — wave-2 D-driven](196.3-wave2-d-driven.md)** — по инвентарю
  сиблинг-функций: каждую → D-фиче → полный тест → переписать на новый путь → закрыть (remove/panic); per-D
  цикл отчёт/коммит/план/синк-main. [opus, nova-wave2]. Держит сиблинги ВНЕ 46293-48883 + callnorm/argbind/резолверы.
- **Точечный dispatch-фикс = [196.7 — method-dispatch через resolved_callees](196.7-method-dispatch-resolved-callees.md)** ✅ ЗАКРЫТ 2026-07-15
  — конкретный `[]u8 @to_str` фасад мис-диспатчился в чужой same-name (`fn[T] T @to_str` бланкет / D410 `T.to_str`);
  фикс = method-call читает канал `resolved_callees` + receiver-C-тип (НЕ name-last-wins), чекер пишет callee для
  array/slice-ресивера. Снят обход `[]u8 @decode_utf8()` (маркер `[M-174.1-to-str-name-collision-codegen-bug]` закрыт).
  [opus, nova-p196-dispatch]. НЕ трогает frozen `infer_call_ret_c`.
- **Точечный dispatch-фикс = [196.8 — primitive receiver bounded blanket](196.8-primitive-receiver-bounded-blanket.md)** ✅ ЗАКРЫТ 2026-07-16
  — BOUNDED-бланкет (`fn[T Ints] T @checked_add`, D310 type-set bound) на примитивном ресивере (`i64.checked_add`)
  мис-диспатчился в concrete-коллизию чужого типа (`Duration @checked_add`) — Plan 164 Ф.3 guard не признавал ни
  примитив-кандидата с BOUNDED бланкетом, ни type-set membership (`protocols_match` знал только `#impl(Protocol)`).
  Фикс = новый регистр `type_set_members` (из `TypeDeclKind::TypeSet`), консультируется в ОБОИХ местах guard'а.
  Маркер `[M-primitive-receiver-bounded-blanket-dispatch]` закрыт; попутно найден+залогирован НОВЫЙ P1
  `[M-i64-clamp-primitive-collision-dispatch]` (concrete-vs-concrete коллизия `@clamp`, отдельное окно).
  [sonnet, nova-p196-8]. НЕ трогает frozen `infer_call_ret_c`.

**★ ФОРМАЛЬНО (директива владельца 2026-07-12): полное выполнение ВОЛНЫ-2 = НЕЗАВИСИМЫЙ способ закрыть 196.**
Если волна-2 закрывает ВСЕ D-фичи через одно окно (каждая фича резолвится в чекере → канал → codegen читает), то
весь второй window — ВКЛЮЧАЯ `infer_call_ret_c` (зона волны-1) — становится dead ПО ПОСТРОЕНИЮ. Т.е. волна-2
(D-полная) сама по себе достаточна закрыть 196; волна-1 (branch-by-branch `infer_call_ret_c`) = параллельный
АКСЕЛЕРАНТ на своей зоне. Две волны избыточны-но-сходятся: любая ПОЛНОСТЬЮ выполненная закрывает 196; вместе —
быстрее. «Встреча волн» как критерий УДАЛЕНА (владелец 2026-07-13): волны НЕ обязаны встречаться — закрыть 196 может ЛЮБАЯ из них, доведённая до конца. Настоящий критерий = ВСЕ D-фичи через одно
окно + структурный финал-гейт (`infer_expr_c_type` сведён к Кан.1-2-чтению + компилится + conformance 95/0).

Партиция (разные регионы/файлы → merge-чисто), инвентарь→D-карта, per-D дисциплина — детально в 196.2/196.3.
Закрытие 196 = СТРУКТУРНЫЙ ФИНАЛ-ГЕЙТ. АСИММЕТРИЯ волн (владелец 2026-07-13): ВОЛНА-2 покрывает ВЕСЬ план (все функции инвентаря + inline-легаси, D-driven) — её полное выполнение = закрытие 196; ВОЛНА-1 — направление по ОДНОЙ функции (`infer_call_ret_c`, крупнейшей) — необходимая ЧАСТЬ гейта, но сама по себе план не закрывает. Гейт: `infer_call_ret_c` пуст/удалён, сиблинги через одно окно → второе окно удалено, матрица
«одного окна» (выше) зелёная.

**★ ПРИНЦИП ОБЕИХ ВОЛН (+ cap-миграции) — тест АВТОРИТЕТ (директива владельца 2026-07-12):** если при
добавлении/прогоне теста вскрывается, что компилятор что-то НЕ умеет — **НИКОГДА не убирать/ослаблять тест.**
Тест = спека. Чинить КОМПИЛЯТОР в **новом ПРАВИЛЬНОМ месте** (чекер / одно окно, per целевая архитектура;
сходимость на существующий путь; НЕ заплатка, НЕ новый сайт, rustc-эталон). NB: на шаге тест-инвентаризации
волны-2 фикс НЕ делается сразу (только пишем тесты + пробелы); но принцип «тест не убираем, чиним компилятор
в правильном месте» незыблем на фазе миграции/проверок. (Нулевая толерантность §4а + «удалить ИЛИ паника».)

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

**Доп. критерий приёмки (владелец 2026-07-13) — отвязка ABI-хардкода Nova-полей std-типов:**
в быстрых путях `emit_c` Nova-имя поля `Vec.data` хардкодится как `->data` (переплетено ещё с двумя
структурами). При close-out ПРОВЕРИТЬ ГРЕПОМ, что таких хардкодов не осталось: ожидаемо они уйдут
сами с удалением легаси-путей; если какой-то переживёт финал — отвязать явно (лоуэринг поля только
через каноничный field-резолв, не литерал `->data`). Это же — пререквизит Plan 200 П6
(переименование `Vec.data`→`ptr`): пока хардкод жив, rename ломает быстрые пути.

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

## ⏸️ ОТЛОЖЕНО (фасет B, точечно) — priv(file) free-fn generic-mono bleed (2026-07-14)

**Статус:** диагноз завершён, фикс отложен решением владельца (не срочно: 2 теста в `standalone/`,
merged-CU зелёный). Полный рецепт + локализация — [196-facetB-privfile-notes.md](wip/196-facetB-privfile-notes.md).
**Первопричина:** `priv(file)` generic free-fn (`fn[T] pick`) в merged-CU (а) выигрывает overload-резолв
у более специфичной file-local КОНКРЕТНОЙ перегрузки (наруш. D84 specificity), (б) mono-именуется по
`file_id` ВЫЗЫВАЮЩЕГО, не файла generic'а. Сайт: `emit_c.rs` generic-mono dispatch (~21530/22812).
**Оценка:** одна sonnet-волна; дорогая часть — баг воспроизводится ТОЛЬКО на полном merged-CU
(изолированно нет) → каждая итерация = 7-мин мега-CU гейт. Две под-правки (чекер-фильтр + codegen-имя)
готовы в рецепте, третий сайт — по нему же. Закроется при системном фасете B ИЛИ отдельной волной по слову.
**Возврат в merged-CU:** `standalone/{method_call_never_static,scalar_only_empty}.nv` — после фикса.

---

## 🏁 Итог финальной closeout-волны (2026-07-21, worktree `nova-196close`, ветка `p196-closeout`, sonnet)

Полный отчёт по шагам: [wip/196-closeout-notes.md](wip/196-closeout-notes.md). Методология —
ИЗОЛИРОВАННЫЕ repro через реальный пайплайн (`nova test <file/folder>`, resolve_imports_inline,
НЕ standalone `nova-codegen compile`), `NOVA_TRACE_ICR=1`/новые точные зонды — избегая
ложных «мертво»-вердиктов прошлых волн (глобальная дедупликация трейсов по процессу).

### Снесено ЭТОЙ волной
**0 движков физически снесено.** Обе попытки снести/детачить (B11q/B11r паника-триал; чекер-фикс
на B11al-терминал) наткнулись на живые corpus-хиты уже на ПЕРВОМ репро и были откачены (zero-
tolerance §4а — половинный/нерабочий патч не остаётся в дереве).

### Доставлено (продюсер-улучшение, НЕ снос)
- **П1 — If-body closure peek ЗАКРЫТ** (`closure_if_ctor_peek`/`closure_if_ctor_branch_peek`,
  `types/mod.rs`, коммит `0d4ee870d`): последний ПРОДЮСЕР-gap builtin-волны — `|x| if c {
  Some(..) } else { None }`-комбинаторы (реальный сайт `plan200_14_option_result_flat_map_
  filter.nv:44`) теперь канализируются, а не только легаси. Гейты зелёные: conformance
  126/0/16, флагман PASS 1/0/33 + built.
- **Новый точный зонд** `NOVA_B5_MEANINGFUL_TRACE` (`rt_slots_from_call`, коммит `9d4209be1`) —
  отличает РЕАЛЬНОЕ восстановление слота от шумного тривиального 0-generic «fallback»
  (существующий `NOVA_B5_TRACE` был зашумлён). Оставлен в дереве (env-gated, 0 цена).

### Канал покрывает (после этой волны)
Explicit-turbofish instance-methods (Producer B, прошлые волны) + static-ctor (CH, прошлые
волны) + **If-body Option/Result combinator closures (Producer B, ЭТА волна, П1)**.

### Честные остатки (ЖИВЫЕ, снос НЕ выполнен — по маркеру)

| # | Маркер/функция | Класс (конкретный, найден изолированным repro) | Число |
|---|---|---|---|
| 1 | `B11q_novaopt_methods`/`B11r_result_like_methods` (`infer_call_ret_c`, frozen) | ЛЮБОЙ Option/Result instance-метод вне Channel 2 — напр. `Option[T Debug]@debug`/`Result[T,E Debug]@debug` (`std/src/prelude/protocols.nv:732,753`), концептуально ШИРЕ closure-peek (П1 не покрывает) | 1+ (первый же repro, d30) |
| 2 | `resolve_result_option_ret` (B06a/B10j callers, `emit_c.rs:19487`) | generic slice-serde `[]T@serialize[S]`/`[]T.deserialize[D] -> Result[...]` (`std/src/encoding/serde/serde.nv:299,307`; вызов `@tags.serialize(s)`, `manual_roundtrip_test.nv:43`) | 1 класс (0 хитов на 8 карта-фикстурах, 1 хит в std/encoding/serde) |
| 3 | `rt_slots_from_call` MISS-fallback (6 call-сайтов, `emit_c.rs:36478,37676,38306,39190,39957,40281`) | generic `HashMap[K,V]`-подобные static/instance вызовы, `channel=None` целиком (node_substs канал их вообще не видит) | 8246+ RECOVERED-хитов (неполный мега-CU прогон, реальное число БОЛЬШЕ) |
| 4 | `B10m_ident_empty_fallback` (`196-probes-notes.md` §1) | phase-1c pre-scan write-once баг (bare-call к expr-body unannotated free-fn, forward-reference в файле) | подтверждён CC-FAIL-репро |
| 5 | `B11al_panic_method_p67` (`196-probes-notes.md` §2.1) | неизвестный метод на `*T`-ресивере — рецепт зонда неполон, реальный фикс требует расширения ОБЩЕГО `infer_arg_ty` (см. `wip/196-closeout-notes.md` П5) | подтверждён red-фикстурой |
| 6 | `B12q_panic_path_p67` (`196-probes-notes.md` §2.2) | неизвестный static-метод через 2-сегментный `Type.method()` Path | подтверждён red-фикстурой |
| 7 | `B12r_panic_path_no_method_seg` (`196-probes-notes.md` §2.3) | Path длиннее 2 сегментов — сцеплен с открытым `[M-d289-module-qualified-path-method-collision-cu]` | подтверждён red-фикстурой |
| 8 | `B12s_panic_path_no_parts` (`196-probes-notes.md` §2.4) | callee через произвольное выражение (Index/Ternary/…) | подтверждён red-фикстурой |

**Статус реестра: план 196 ОСТАЁТСЯ 🔥 IN PROGRESS, НЕ ЗАКРЫТ.** Второе окно (`infer_expr_c_type`/
`infer_call_ret_c`) по-прежнему живёт как необходимый fallback для минимум 3 задокументированных
классов (builtin Option/Result debug-и-подобные методы; generic slice-serde; generic HashMap[K,V]-
подобные static/instance вызовы) плюс 5 терминал-остатков вне frozen-зоны (checker-side, `types/mod.rs`
+ `parser/mod.rs`). Каждый остаток — с конкретным классом+числом (не голословно «наверное живо»),
маркер и файл-источник указаны для следующей волны. Гейты этой волны — зелёные на КАЖДОМ шаге
(conformance 126/0/16, флагман built, byte-diff=0 на откаченных попытках).

Хэши коммитов (ветка `p196-closeout`, база main `58804953d`): `d46610dae`, `0d4ee870d`, `418afb69f`,
`428529179`, `6805f2643`, `9d4209be1`, `34e9a9e41`. В main НЕ мёржено, push НЕ делался. Модель: sonnet.

---

## 🏁 B11q/B11r root-cause волна (2026-07-21, worktree `nova-196b11`, ветка `p196-b11q`, sonnet)

Полный отчёт: [wip/196-b11q-rootcause-notes.md](wip/196-b11q-rootcause-notes.md). Задача — карта
«B11q/B11r», продолжение прошлой закрытой closeout-волны (остаток №1 в таблице выше).

**Корень найден (не был известен прошлой волне):** ЛЮБОЙ вызов `.debug()`/`.display()` на внутреннем
значении ИЗНУТРИ `Option[T Debug]@debug`/`Result[T,E Debug]@debug` (и `@display`-близнецов) бьёт в
структурный пробел Channel 2 — рецептор в этом вызове (`v: T`) ГОЛЫЙ generic type-param, связанный
протоколом `Debug`, а `resolve_instance_method_return_arity`/`infer_method_call_channel_type`
(`types/mod.rs`) не умеют «receiver = generic-имя-в-скоупе, диспетчеризуй по его БАУНДУ»: `gs`
(генерики в скоупе, нить через 31 сайт `types/mod.rs`) несёт ТОЛЬКО имена (`&HashSet<String>`), баунды
теряются до резолва вызова. Подтверждено изолированным repro (`Some(Some(42))`/`${x:?}`) + чтением
сгенерированного `.c` (`Nova_Option_method_debug_NovaOpt_nova_int`'s тело вызывает
`Nova_Option_method_debug_nova_int(v, f)` мимо Channel 2). **Снос B11q/B11r НЕ выполнен** — фикс
потребовал бы расширения `gs` (широкий blast radius, 31 сайт) или нового cross-cutting checker-состояния
— смежно с protocol-resolution зоной (другой агент этой волны), не точечный патч. Задокументировано
(`[M-196-b11q-root-cause]`, `emit_c.rs` ~53661/~53779) для следующей волны.

**⚠ Инцидентально найден БЛОКЕР авторитетного гейта (не моя зона, СРОЧНО):** `spec_tests/conformance/
d216_ptr_methods_174_5.nv:17-18` (`mut q = buf.ptr(); q.write_at(1, 99)`) паникует
`[P67-LEGACY] method call .write_at return type unknown` (`obj_ty=""`) — и поскольку
`spec_tests/conformance` — ОДИН модуль (папка = co-equal файлы), **ЛЮБОЙ** `nova test
spec_tests/conformance/<файл>.nv` (single-file ИЛИ folder) падает идентично, включая файлы никак не
связанные с pointers. Верифицировано A/B (git stash моей правки, чистый `cargo build --release`) —
падает ОДИНАКОВО на pristine и patched (регрессия НЕ из этой волны). Флагман (`nova check
--strict-effects examples/flagship/aggregator`) — PASS на обоих (компилятор в целом здоров, поломан
именно `spec_tests/conformance` folder-CU). Фикстура не менялась (`c6c4a7af0`, Plan 174.5 original) —
регрессия пришла ИЗВНЕ между 196-closeout baseline (`58804953d`, зелёный 126/0/16) и текущим main
(`5c775de3b`); подозреваемый — `p208-sh4-teardown` merge (`4ad3c8d10`, Debug/Display rich-spec refactor),
НЕ подтверждено git-bisect (вне бюджета/зоны — Plan 174.5/D216 pointer-typing, не resolve-channel зона
этого задания). **Из-за этого блокера полный мега-CU гейт этой волной НЕ получен** — честно не заявляю
зелёный гейт, которого не было; готовые альтернативы: изолированный repro (PASS) + флагман (PASS) на
patched-бинаре, идентично pristine.

Коммит (ветка `p196-b11q`, база main `5c775de3b`): comment-only B11q/B11r root-cause diagnosis + notes
(0 поведенческих изменений, verified). В main НЕ мёржено, push НЕ делался. Модель: sonnet.

## Под-программа: name-hardcode audit (2026-07-22, находка владельца — BUILTIN_VTABLE_NAMES)

**Связь с 196:** хардкод-списки имён типов/протоколов/эффектов/методов в Rust = ВТОРОЙ
источник правды по именам (ровно то, что 196 «one truth» вычищает; §3: max из .nv).

**Инвентарь (compiler-codegen/src, греп 2026-07-22):** 31 список `const X: &[&str]` + 92
сравнения `== "TypeName"` + 21 `.contains(&"...")` = ~144 сайта.

**§3-ДОЛГ (имеют .nv-декларацию — кандидаты на вывод из атрибута/реестра, НЕ имён-списка):**
- `BUILTIN_VTABLE_NAMES` (emit_c.rs:6062) — эффект-vtable; Time уходит в [175.2](175.2-typed-effects.md) ч.6, остаток → атрибут.
- `RT_VTABLE_PROTOCOLS = ["Hash","Compare","Display"]` (:9891) — ПРОТОКОЛЫ в .nv.
- `BUILTIN_TYPE_NAMES`/`BUILTIN_RUNTIME_TYPES`/`RUNTIME_NATIVE_CONCRETE_TYPES` — типы.
- `SUSPEND_EFFECT_NAMES` (types/mod.rs) — эффекты.
- `FLUENT_BUILTIN_METHODS`/`VEC_INHERENT_METHOD_SELECTORS`/`CHAR_UNICODE_METHOD_SELECTORS` — методы в .nv.
- 92× `== "Тип"` + 21× contains — построчная классификация в пост-релизной волне.

**ЛЕГИТИМНЫЕ (C-фундамент, НЕ из .nv — не долг):** `LIBUV_*SYSLIBS` (линкер-флаги),
`NOVA_PRIMITIVES`/`PRIMITIVE_TYPES` (примитивы = фундамент языка), `ABBREVIATIONS`/doc-`TYPES`
(доки), `KNOWN_PART_ONLY_MACRO_STATEMENTS` (split_tu-механика).

**План:** до тегов — только то, что 175.2 задевает (Time-vtable). Остальное — пост-релизная
196-волна «per-list классификация → вывод из .nv-атрибута/реестра». Маркер
`[M-196-name-hardcode-lists]`.


### Расширение аудита: ПОЛНАЯ карта хардкода (2026-07-22, вопрос владельца «где ещё хардкод?»)

7 греп-категорий по всему компилятору (метод-детектор, повторяемый):
| Кат | Сайтов | Природа |
|---|---|---|
| A `const X: &[&str]` списки имён | 31 | долг(vtable/протоколы)+фундамент(примитивы/syslibs) |
| B `== "ИмяТипа"` | 92 | диспетч по строке вместо .nv-резолва |
| C `.contains(&"Имя")` | 21 | то же |
| D рукописный C-vtable/схемы nova_rt | 8 | ЯДРО (NovaVtable_Time) |
| E `match name { "Foo"=>}` | 28 | имя как ключ |
| F RUNTIME_DEFINED_TYPES-схемы в Rust | 41 | схемы типов дублированы |
| G «must match»/layout | 315 | ABI C↔Rust — в осн. ЛЕГИТИМ (FFI), но риск рассинхрона |

**Метод (не реактивный):** (1) греп-детектор 7 категорий → скрипт `scripts/guards/hardcode-audit.sh`;
(2) критерий долга §3: есть .nv-декларация + Rust/C-копия = долг; легитим = чего в .nv нет
(примитивы/ABI-layout/syslibs); (3) тест «удали в .nv → среагирует?»; (4) tripwire-гейт против роста.
**Программа:** пост-релизная (~500 сайтов, G≈315 легитим-ABI); зачистка по категориям B→E→F→A→D.

**Детектор ✅ (2026-07-22, `scripts/guards/hardcode-audit.sh`, haiku):** повторяемый tripwire по 7
категориям + `--list <кат>` + exit-код (0 стабильно / 1 вырос). ВАЖНАЯ ОГОВОРКА: его baseline
(A39/B310/C5/D53/E21/F24/G0 = 452) ШИРЕ ручной карты выше (31/92/21/8/28/41/315) — grep-паттерны
ловят и легитимное (`== "Result"/"Option"/"Self"`, все упоминания `NovaVtable_`), G не реализован.
Значит: скрипт = **сторож РОСТА**, не абсолютный счётчик долга; классификация долг/легитим —
вручную в каждой волне зачистки. Уточнение паттернов (исключить легитим-классы, реализовать G) —
следующая итерация детектора.
