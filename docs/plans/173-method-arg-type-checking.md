<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 173 — Method-argument type-checking + scalar-narrowing enforcement

**Статус:** 📋 proposed 2026-06-19
**D-блок:** предв. **D311** (финал при impl; D310=172, D309=171, D308=169.2.1)
**Ветка:** TBD (`plan-173-method-arg-typecheck`)
**Приоритет:** P1 (soundness).
**Связан с:** `[M-instance-method-arg-scalar-narrowing]`, `[M-generic-arg-type-mismatch-silent]` (closed), `[M-scalar-nonliteral-narrowing-not-enforced]` (mostly-done — этот план закрывает остаток).

---

## 1. Мотивация

Аргументы вызова **метода** (`obj.method(arg)`) не проходят через type-checker:
`f1_check_call` ([types/mod.rs:6249](../../compiler-codegen/src/types/mod.rs)) для instance-методов
делает `_ => return` — он не резолвит метод (вывод типа получателя + перегрузки исторически
живут в codegen), поэтому типы аргументов методов чекер **не сверяет**.

На практике это **частично** закрыто другими слоями (проверено эмпирически 2026-06-19):

| Слой | Что ловит у method-call | Пример |
|---|---|---|
| Nova type-checker | существование метода (`E7320`), арность («обязательный параметр не передан») | — |
| codegen overload-резолвер (D84, [emit_c.rs:23026](../../compiler-codegen/src/codegen/emit_c.rs)) | перегрузки по статическим типам аргументов; нет кандидата → `CODEGEN-FAIL` | `g(int)/g(str)` ← `g(bool)` → `no matching overload for g(nova_bool)` |
| C-компилятор (codegen-выход) | категориальное несовпадение struct↔scalar | `Vec[int].push(str)` → `CC-FAIL passing nova_str to incompatible type nova_int` |

**Единственная реальная дыра** — `scalar→scalar` неявное **сужение** через аргумент
**одиночной** (не-перегруженной) перегрузки метода: арность совпадает с единственным
`push(u32)`, а `int→u32` — легальная C-конверсия (тихое усечение), поэтому **ни один слой не
ругается**. Канонический случай — `vec_u32.push(int_var)`.

Не-method-формы уже закрыты в Plan «scalar narrowing» (commit `f96016e6`): `[E_IMPLICIT_NARROWING]`
для **binding** (`ro a u8 = int_var`), **свободной/static fn-arg** (`take_u8(int_var)`),
**reassignment** (`a = int_var`). Этот план распространяет ту же гарантию на **аргументы методов**.

## 2. Что есть сейчас (опора)

- `is_int_narrowing(found, expected)` + `int_width_rank` ([types/mod.rs](../../compiler-codegen/src/types/mod.rs)) — готовое правило: value-range-preserving widening неявный; narrowing + signed→unsigned + value-unsafe cross (`u32→i32`, `u64→int`) требуют `as`. Работает на raw-`TypeRef`.
- `Compat::Narrowing` + `[E_IMPLICIT_NARROWING]` — диагностика уже есть.
- `check_call_argbind` ([types/mod.rs:9753](../../compiler-codegen/src/types/mod.rs)) **уже резолвит** instance-методы через `resolve_instance_method(obj, method_name, scope, arity)` → получает `f.params` (для арность/keyword-проверки). Это потенциальная точка для checker-варианта (см. Ф.0).
- codegen overload-резолвер ([emit_c.rs:23026](../../compiler-codegen/src/codegen/emit_c.rs)) сравнивает `param_c_types` с `infer_expr_c_type(arg)` для **перегруженных** свободных fn — клин для codegen-варианта, но одиночные перегрузки param-типы не сверяют.
- Codegen-представление в C-типах: `nova_int`=int64_t (8 байт, signed), `uint32_t` (4 байта) и т.д. — width/sign модель уже есть в `int_width_rank` (Nova-имена) и в width-таблице emit_c (`"uint32_t"=>4`).

## 3. Открытые дизайн-вопросы (решить в Ф.0 — GATE)

- **Q1 — где enforce'ить: checker vs codegen?** Два кандидата:
  - **(A) Checker** — расширить `check_call_argbind`: после `resolve_instance_method` сверить
    каждый аргумент с `param.ty` через существующий `assignable`/`is_int_narrowing`. Плюс: чистая
    диагностика `[E_IMPLICIT_NARROWING]`, единый код-путь с free-fn. Минус: `resolve_instance_method`
    — best-effort; **встроенные** методы (`Vec.push`, `str.*`) живут НЕ в `method_table`, а в
    external/builtin-реестре — их receiver-резолв в чекере может не находить → дыра для именно того
    хотспота (`push`).
  - **(B) Codegen** — добавить narrowing-проверку там, где param-C-типы уже известны (резолвер D84
    + точки method-dispatch). Плюс: ловит ВСЁ (включая builtin `push`). Минус: codegen-многопутёвость
    (`emit_call` :22502 — гигантский диспетчер; `vec_method_call` и спец-эмиттеры), C-уровневая
    диагностика менее чистая, риск задеть hot-файл.
  - **(C) Гибрид** — receiver-type инференс перенести/усилить в чекере (вывести тип `obj`, найти
    сигнатуру метода в едином реестре, включая builtin), а сверку делать через готовый `is_int_narrowing`.
  - **Критерий выбора:** покрывает ли вариант **builtin-методы** (`push`) — иначе хотспот не закрыт;
    минимальный риск для codegen; единая диагностика. Предв. рекомендация — **(C)**: единый
    method-резолвер в чекере (включая builtin-сигнатуры из external-реестра), сверка через
    `is_int_narrowing`. Решить замером покрытия в Ф.0.
- **Q2 — какой объём type-check у методов включать?** Только narrowing, или ВСЕ типы аргументов
  (как у free-fn — `E7301` на `str→int`)? Полная типизация чище, но вскроет больше (часть уже
  ловит C-компилятор как CC-FAIL → станет чистой compile-error раньше). Предв.: включить полную
  типизацию аргументов методов (narrowing — частный случай), но за флагом/поэтапно, чтобы
  контролировать blast-radius.
- **Q3 — overload-резолв в чекере.** Если методы перегружены, сверка должна выбрать правильную
  перегрузку ДО проверки типов (иначе ложные ошибки). Переиспользовать логику codegen D84-резолва
  (filter по arity+param-types) на уровне чекера, либо консультироваться с тем же реестром.
- **Q4 — alias/newtype/generic-receiver.** Permissive там же, где `is_int_narrowing` (alias/newtype
  не резолвятся → skip), и где receiver — generic-параметр (тип неизвестен → skip). Без ложных
  срабатываний на generic-коде.

## 4. Фазы

- **Ф.0 — Аудит + decision point (GATE).** Замерить: (a) сколько method-call сайтов в std/tests —
  scalar-narrowing аргументы (детект-режим, не-фатально); (b) резолвит ли `resolve_instance_method`
  builtin `Vec.push`/`str.*` (определяет жизнеспособность варианта A); (c) выбрать слой Q1.
  **Выход:** число сайтов миграции + выбранный вариант. Без этого Ф.1 не начинать.
- **Ф.1 — Method-резолв + сверка аргументов.** По выбору Ф.0: единый method-резолвер (receiver-тип
  + сигнатура, включая builtin) и сверка каждого аргумента через `is_int_narrowing` (+ опц. полная
  `assignable`). Эмит `[E_IMPLICIT_NARROWING]` (+ опц. `E7301`). Permissive на alias/generic/unknown.
- **Ф.2 — Измерение blast-radius.** Полный прогон std+nova_tests, точный список сайтов
  `[E_IMPLICIT_NARROWING]` по method-args (ожидаемо ~375 `push(int)` — buffers/codecs/crypto/unicode).
- **Ф.3 — Миграция std + тестов.** Механически обернуть сужающие аргументы в `as <T>`
  (`out.push(cp)` → `out.push(cp as u32)`). Воркфлоу-фан-аут по файлам (каждый агент: батч файлов
  → добавить `as` где новый чекер требует → проверить). Флагнуть случаи, где `as`-усечение
  семантически неверно (нужен range-check / надо менять тип контейнера) — отдельно.
- **Ф.4 — Тесты + спека.** Позитив (widening/as/литерал у method-args — OK) + негатив (`push(int)`,
  `obj.m(narrowing)` — `E_IMPLICIT_NARROWING`). Спека: D54/conversions.md amend (value-preserving int
  widening — неявный; narrowing/sign-unsafe — `as`); зафиксировать «аргументы методов типизируются».
  Новый/переиспользованный код ошибки в `09-tooling.md` error-index.

## 5. Критерии приёмки

1. `vec_u32.push(int_var)` (и любой `obj.method(narrowing_int)`) → `[E_IMPLICIT_NARROWING]`,
   `obj.method(int_var as u32)` → OK, widening (`push(u8_var)` в `Vec[int]`) → OK.
2. Полнота: покрыты **встроенные** методы (`Vec.push`, `str.*`) — не только user-методы.
3. **Без упрощений как для прода:** реальный резолв метода + сверка, никаких заглушек; permissive
   только на принципиально-неразрешимом (alias/generic/unknown), зафиксированном маркером.
4. Ноль ложных срабатываний на позитивном корпусе после миграции; std+nova_tests зелёные.
5. Миграция std завершена (все сужающие method-args получили `as`), либо обоснованно вынесена
   точечными followup'ами там, где `as` семантически неверен.
6. Спека/доки/error-index обновлены; D311 написан.

## 6. Риски

- **Codegen-вариант (B)** рискует задеть `emit_c.rs` (hot, общий с другими сессиями) — координировать.
- **Полная типизация (Q2)** может вскрыть много existing-ошибок за пределами narrowing — поэтапно/за флагом.
- **Миграция 375 сайтов** — механическая, но проверять, что `as`-усечение не маскирует реальный
  range-баг (значение может не влезать); где сомнительно — range-check вместо `as`.

## 7. Источник

Обсуждение 2026-06-19 (сессия generic-arg + scalar-narrowing). Эмпирическая карта method-call
проверок (существование/арность — чекер; перегрузки — codegen; категория — C; scalar-narrowing — дыра)
получена прямыми пробами. Готовая инфраструктура (`is_int_narrowing`, `Compat::Narrowing`,
`E_IMPLICIT_NARROWING`) из commit `f96016e6`.

---

# Архитектурный контекст: фрагментация типового движка

> Method-arg дыра — **симптом**, не корень. Корень — у Nova **нет единого типового движка**.
> Аудит (3 параллельных агента, 2026-06-19) нашёл **ЧЕТЫРЕ независимых движка типов** над
> разными представлениями, с раздельными реестрами и раздельным знанием stdlib. Этот раздел —
> карта «как есть» и путь «как привести в порядок».

## 8. Где живёт type-работа (карта «как есть»)

### 8.1 Четыре независимых типовых движка

| # | Движок | Где | Представление | Что делает |
|---|---|---|---|---|
| 1 | **Checker** | `types/mod.rs` | `Ty` (`ty_of_ref` :13145), `TyCat` (`cat_of_depth` :8345), сырой `TypeRef` | best-effort inference (`infer_expr_type` :7894), проверки (`assignable` :7748, `f1_check_call` :6367), **частичный** single-overload резолв |
| 2 | **Codegen** | `codegen/emit_c.rs` | C-строки (`nova_int`/`uint32_t`/`Nova_Vec____*`) + `MethodSig` | **полный** re-derive типа каждого выражения (`infer_expr_c_type` :35718, ~1900 строк), **настоящий** резолв перегрузок/методов (`resolve_overload` :11140, D84 :23150), моно-substitution |
| 3 | **Const-fn pipeline** | `const_fn_closure.rs` + `const_fn_trampoline.rs` | `TypeRef` | **третий** движок unification/substitution (`match_type` :382, `type_subst` :516, `infer_generic_subst` :1027) + свой sig-registry |
| 4 | **Sum-schema** | `codegen/sum_schema_registry.rs` | свой | layout sum-типов, по своему же заголовку «sits PARALLEL» к legacy `sum_schemas` в codegen |

Genuine type-lines = #1/#2/#3/#4. **Не** type-lines (consumers/name-passes): `imports.rs`
(name-only sig-prepass + export-gating), `chain_norm.rs` (fluent-rewrite), `resolver.rs` (semver),
`may_gc.rs`/`preempt_keep.rs` (`typeref_sig`→string для mangling), `callnorm.rs`/`argbind.rs`
(arg-binding), `parser/`/`ast/` (производят `TypeRef`, не выводят).

### 8.2 Три представления типа в одном чекере (каждое лоссее)

- `TypeRef` (raw AST) — **канон**: хранит ширину int, generic-аргументы, L1/L2/L3-модификаторы. Вся
  точная работа (narrowing, range-check, readonly-coerce, generic-arg mismatch) — на нём.
- `Ty` (`ty_of_ref` :13145) — все int-ширины → `Ty::Int`; generic-аргументы **дропаются**
  (`Named{path,generics}`→`Ty::Named(last)`). Используется почти только для `Ty::Never`-хука.
- `TyCat` (`cat_of_depth` :8345) — ещё грубее: int-ширина+знак → `TyCat::Int`; `Vec[int]` и
  `Vec[u32]` оба → `Array(Int)` (ширина **намеренно теряется** — её ловит raw-`TypeRef`-проверка в
  `f1_check_call`). На нём `cat_compatible` (:8672, permissive: `Other` матчит всё, int↔float ок).

### 8.3 Что чекер НЕ делает (делегирует codegen/C)

- **Multi-overload и instance-method dispatch** — `f1_check_call` выходит на `>1` overload и на
  `obj.method` (`_ => return` :6476). Реальный резолвер — codegen `method_overloads`/`resolve_overload`.
- **Builtin Vec/str методы** — их нет в `method_table` (он строится только из `Item::Fn`); чекер
  держит параллельный **хардкоженный** `builtins: HashSet` (:11497) и явно отсылает к codegen.
- **Return-type inference обычных вызовов** — `infer_expr_type`→None; общий тип берётся из codegen
  `infer_expr_c_type`.
- **Width/representation корректность** — в итоге падает на **C-компилятор** (категориальные
  несовпадения → CC-FAIL), а scalar-narrowing не ловит никто (эта дыра — §1-7).

### 8.4 Дублирование и дрейф (инвентарь)

| Что дублировано | Где | Риск |
|---|---|---|
| **Inference выражения** | checker `infer_expr_type` :7894 ↔ codegen `infer_expr_c_type` :35718 | два независимых вывода, могут расходиться |
| **Type→repr mapping** | `type_ref_to_c` :5769, `simple_type_ref_to_c` :14752 (**уже дрейфил**: u32/u16 отсутствовали → `Vec[u32]` мис-манглился), `apply_type_subst_to_ref` | внутри codegen — троекратно |
| **Метод/fn-таблицы** | checker `method_table` (TypeCheckCtx :2273, заново в BoundCtx :9266, CapabilityCtx :10815) + codegen `method_overloads` :618 | одни методы в ≥4 копиях, 2 представления |
| **Знание stdlib** | checker `builtins: HashSet` :11497 ↔ codegen `ExternalRegistry` (parsed из `.nv`) :634 | **худшая поверхность дрейфа** — API stdlib закодирован дважды |
| **Width-логика** | `cat_of_depth` int-arm + `int_width_rank` :8586 + `sized_int_bounds` :8540 | одна модель ширины в 3 местах |
| **`is_intrinsic_namespace`** | checker :8706 ↔ codegen guard :23338 (комментарий «совпадает с…») | руками синхронизируемые списки |
| **Sum-layout** | `sum_schema_registry` ↔ legacy `sum_schemas` | внутри codegen |

**Корень одной фразой:** checker и codegen — **два полностью независимых типовых движка** над
разными представлениями, каждый со своим реестром, построенным своим pre-pass'ом, и с **раздельным
знанием stdlib**. Codegen **заново** реализует резолв перегрузок, generic-substitution и типизацию
выражений вместо того, чтобы **потреблять единый типизированный IR**, который произвёл чекер.

## 9. Целевая архитектура (единый типовой IR)

```
parse → [SEMANTIC ANALYSIS: один движок типов] → типизированный IR → codegen (лоуэринг) → C
```

1. **Один резолвер/инференсер** в семантической фазе: выводит тип каждого выражения, разрешает
   каждый вызов (перегрузки/методы/builtin — по типам) **один раз**, проверяет конверсии.
2. **Типизированный IR** — результат: каждое выражение аннотировано типом, каждый вызов — выбранным
   callee + моно-substitution. **Единый источник истины** (§0 compiler-conventions).
3. **Один реестр** методов/сигнатур/stdlib (включая builtin — из `.nv`, не хардкод-список §2),
   читаемый и чекером, и codegen.
4. **Codegen — только лоуэринг**: читает аннотации/выбранный callee из IR, не выводит и не резолвит
   типы заново. `infer_expr_c_type`/`resolve_overload`/мангл-инференс **исчезают** как дубль.
5. **C никогда не ловит ошибку типов**; все «no matching overload»/D124/`E_IMPLICIT_NARROWING`-классы
   — диагностики **чекера**.

## 10. Этапы приведения в порядок (отдельный umbrella, шире Plan 173)

Plan 173 (method-arg narrowing) — **первый конкретный шаг** этого пути. Полная унификация — крупный
многофазный рефактор, кандидат в **отдельный umbrella-план**:

- **U.1 — Единое знание stdlib.** Чекер читает `ExternalRegistry` (из `.nv`) вместо хардкоженного
  `builtins: HashSet`. Убирает худший дрейф; даёт чекеру сигнатуры builtin-методов → разблокирует
  резолв методов в чекере (нужно для Plan 173 варианта C).
- **U.2 — Один реестр методов/сигнатур.** Слить checker `method_table` (×3 контекста) и codegen
  `method_overloads` в один, построенный одним pre-pass'ом; оба слоя читают его.
- **U.3 — Резолв вызовов/перегрузок в чекере.** Перенести `resolve_overload`/D84-логику в
  семантическую фазу; codegen **потребляет** выбранный callee. Закрывает method-arg дыру (Plan 173)
  и переносит «no matching overload»/D124 в чекер.
- **U.4 — Единый инференс выражения.** Свести `infer_expr_type` и `infer_expr_c_type` к одному;
  C-строки выводятся **из** аннотированного IR-типа, не заново.
- **U.5 — Единое представление ширины/модификаторов.** Один тип, несущий ширину/знак/L1-L3, вместо
  `Ty`/`TyCat`-схлопывания + трёх width-хелперов.
- **U.6 — Свернуть дубли codegen.** `simple_type_ref_to_c`→`type_ref_to_c`; `sum_schema_registry`↔
  `sum_schemas`; `typeref_sig` (may_gc↔preempt_keep); const-fn движок (#3) поверх общего.
- **U.7 — C не ловит типы.** Гарантия-критерий: на корректном фронтенде ни одного CC-FAIL по типам.

Порядок задаёт зависимость: **U.1→U.2→U.3** разблокируют полноценный Plan 173 и убирают дрейф;
U.4-U.7 — последующая консолидация. Каждый этап — измеримый, с регрессом против чистого бинаря
(§6 compiler-conventions).
