<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 172 — Переработка компилятора (umbrella)

**Статус:** 📋 proposed 2026-06-19 (umbrella)
**Home:** этот файл — индекс под-планов переработки типовой системы компилятора.
**Конвенции:** реализует [`compiler-conventions.md`](../compiler-conventions.md) §0 (единый источник
истины), §1 (где живут проверки), §2 (никакого хардкода).

---

## 1. Зачем зонтик

Переработка типовой системы Nova породила несколько связанных верхнеуровневых планов. Зонтик
собирает их в один отслеживаемый набор и фиксирует **порядок** работ. Корень — у Nova **нет
единого типового движка**: аудит 2026-06-19 (3 параллельных агента) нашёл **четыре** независимых
движка типов над разными представлениями, с дублированием и дрейфом (детальная карта — в
[172.1 §8](172.1-unified-type-engine.md)).

## 2. Под-планы

| # | План | Суть | Статус |
|---|---|---|---|
| **172.1** | [Unified type engine](172.1-unified-type-engine.md) | Свести 4 движка к **одному** семантическому проходу → типизированный IR → codegen лоуэрит → C не ловит типы. Фазы U.1-U.7. Ядро рефактора. | 📋 proposed |
| **172.2** | [Method-arg type-checking](172.2-method-arg-type-checking.md) | Типизация аргументов методов + `[E_IMPLICIT_NARROWING]` на scalar-narrowing через method-arg. **Первый конкретный шаг** 172.1 (узкий кейс U.1-U.3). | 📋 proposed |
| **172.3** | [Type-set bounds](172.3-type-set-bounds.md) | Go-style generic-constraints (`fn[T IntNumber] …`) — набор конкретных типов как bound, не только протокол. Type-system фича, которую единый движок должен нести. | 📋 proposed |
| **172.4** | [Value-ABI + auto-placement](172.4-value-abi-auto-placement.md) | Единый value-ABI (value-record / named-tuple / struct-tuple — один путь) + **авто** by-ref/heap↔stack (нет `ref T`, Q29). Acceptance: value-record fluent `mut @ -> @` + структурное `==`. **Behavior-changes ПОСЛЕ MVP-консолидации 172.1.** | 📋 proposed |
| **172.5** | [In-out ref-params](172.5-inout-ref-params.md) | In-out `ref`-параметры (safe by-ref borrow) + формализация `@`/`-> @` (D326). `ref` = режим параметра (Swift `inout`), не тип. Поверх 172.4. | 📋 proposed |
| **172.6** | [Primitive parse API](172.6-primitive-parse-api.md) | Один движок str→примитив, radix-only `parse`; per-type обёртки с range-check; фикс truncation-бага. Зависит 172.3 (type-set bounds схлопывает обёртки). | 📋 proposed |
| **172.7** | [`?` return-only](172.7-question-mark-return-only.md) | `?` строго return-only (Rust-стиль), в Fail-fn запрещён (`!!`/`throw`); чистка stale `## D4`. Завершает 173. | 📋 PLANNED |
| **172.8** | [`any` + downcast](172.8-any-type-and-is-downcast.md) | `any` top-type (fat-pointer) + `is T`/`try_as[T]` runtime-downcast по `type_id`. Разблокирует typed-errors 173 Ф.4. | 📋 PROPOSED (P1) |
| **172.9** | [Effect-registry size](172.9-effect-registry-compile-time-size.md) | Compile-time размер effect-registry вместо хардкода 32 (>32 эффектов → silent-drop). | 📋 READY (P2) |
| **172.10** | [Pointer-ops methods](172.10-pointer-ops-methods.md) | Операции указателей через методы (`.read`/`.write`/`.offset`/…) вместо операторов; `unsafe T`→`uninit T`; write-cap fix. | 📋 PROPOSED |
| **172.11** | [C-FFI ABI types](172.11-ffi-abi-types.md) | C-ABI тип-лист (туплы/value-records/`Option[*T]` рекурсивно) + fn-ptr ABI-тег (`*extern "C" fn` vs `*fn`). | 📋 PROPOSED |

**Не входят** (остаются top-level, другая тема): Plan 170 (file-private visibility — видимость).

## 3. Порядок работ

```
172.1 (unified engine)
   U.1 единое знание stdlib  ─┐
   U.2 один реестр           ─┼─→ разблокируют 172.2 (method-arg) и убирают дрейф
   U.3 резолв в чекере       ─┘
   U.4..U.7 консолидация
172.3 (type-set bounds) — может идти параллельно как фича; единый движок (172.1) её упрощает
172.4 (value-ABI + auto-placement) — ПОСЛЕ MVP-консолидации 172.1 (behavior-changes на едином носителе)
```

- **172.1 U.1-U.3** — фундамент: единое знание stdlib + один реестр + резолв в чекере. Они же
  разблокируют полноценный **172.2** (резолв методов в чекере для method-arg типизации).
- **172.2** — первый измеримый результат на узком кейсе (scalar-narrowing в method-arg);
  частично уже сделано вне методов (`E_IMPLICIT_NARROWING`, commit `f96016e6`).
- **172.3** — независимая фича; после landing единого движка (172.1) выражается чище (схлопывает
  ~13 per-type обёрток Plan 172.6 в ~2-3 generic).

## 3.1. Forward-compat с системой ошибок (Plan 173 + под-планы 172.7/172.8) — чтобы 173 не переделывал 172

Анализ 2026-06-21 (по запросу владельца): [Plan 173](173-error-system-unify-harden.md) (error/cleanup),
[172.7](172.7-question-mark-return-only.md) (`?`-return-only), [172.8](172.8-any-type-and-is-downcast.md)
(`any`+`is`-downcast) садятся на типовой движок 172. Чтобы 173 строился ПОВЕРХ единого движка, а не
переделывал его, **172.1 обязан учесть 4 точки** (в основном осознанность/lossless, не большие правки):

1. **Эффекты в `ResolvedType` — LOSSLESS, с type-args (`Fail[E]` несёт `E`). ✅ DONE 2026-06-21
   (172.1 U.5.5c, `f7511bda`).** Был `ResolvedType::Func.effects: Vec<String>` (только имена,
   `from_type_ref`→`path.last()`) — **лосси** (терял `E` у `Fail[E]`), [D315](../../spec/decisions/02-types.md#d315-resolvedtype--единый-канонический-носитель-типа-plan-1721-2026-06-21)-нарушение + блокер
   typed-errors 173/175. Обогащён до **`Vec<ResolvedType>`** (имя + module + type-args). Теперь
   173 Ф.4 (типизированный `Fail[E]`/`ScopeOutcome.Failure(any)`) + 175 (`any`/`is`) садятся на
   готовый носитель, НЕ переделывая его. Byte-identical (effects write-only до consume).
2. **`any` + `is`/downcast — согласовать с Plan 172.8.** 172-резолв `Any`/`is`-теста НЕ должен
   форклоузить модель Plan 172.8 (`any`-тип + `is T`/downcast по `type_id`) — 175 строит typed-error
   dispatch 173 поверх ЕДИНОГО движка. Координировать дизайн `Any`/`is` в U.4 с Plan 172.8.
3. **`[M-parfor-record-result-miscompile]` чинится U.4 ПО ПОСТРОЕНИЮ.** Чекер типизирует
   `parallel for → []T` для ЛЮБОГО `T`, а array-mode codegen (`emit_c.rs:8154`) умеет лишь
   `T∈{int,bool,f64,str}` → молчаливый degrade в `unit` → утечка C-error. Это ровно §0
   checker↔codegen рассинхрон. Унифицированный `[]T`-lowering (any `T`, U.4.4) ДОЛЖЕН покрыть
   parfor-result-array (не оставить отдельным re-deriving путём) → 173 Ф.3 тривиален. 172 берёт
   этот маркер в свой радар.
4. **Error-типы (`Result`/`MultiError`/`ScopeOutcome`) — generic mechanism (§3), не hardcode.**
   172.1 U.1 (де-хардкод stdlib) обязан не спец-кейсить error-типы → 173 эволюционирует их
   (materialize `MultiError`, `Failure(any)`) без борьбы с 172-хардкодом.

## 3.2. Forward-compat с pointer/FFI-моделью (Plan 172.10/172.11) — чтобы они не переделывали носитель

Анализ 2026-06-22 (по запросу владельца): [Plan 172.10](172.10-pointer-ops-methods.md) (pointer-ops→методы +
`uninit T` + write-cap) и [Plan 172.11](172.11-ffi-abi-types.md) (C-FFI ABI тип-лист + `*extern "C" fn` ABI-тег) —
оба PROPOSED, оба явно координируются с «type-engine = 172» и амендят spec в зоне 172 (`02-types.md` /
`08-runtime.md`). Направление: 172.10/172.11 **садятся на** единый носитель 172, 172 их **не блокирует**. Чтобы они
строились ПОВЕРХ носителя, а не переделывали его, **172 держит на радаре 3 точки** (осознанность/lossless,
как §3.1; не новые задачи 172.1 до landing 172.10/172.11):

1. **fn-ptr ABI-тег (`*extern "C" fn` vs `*fn`) — носится в `ResolvedType` LOSSLESS** (178 §3). По
   [D315](../../spec/decisions/02-types.md#d315-resolvedtype--единый-канонический-носитель-типа-plan-1721-2026-06-21)
   носитель несёт ПОЛНУЮ семантическую личность; ABI-тег fn-ptr — такая же ось, как module-путь /
   `*mut`-верность / effects-type-args (U.5.5). При ретайре `type_ref_to_c` (U.4.8 ✅) тег теряться НЕ должен →
   U.5.5-style enrichment Func/Pointer-арма, когда 178 landed. Сейчас носитель тег не несёт — добавится
   **осознанно** (как U.5.5a/c), не переделкой.
2. **C-ABI-совместимость (178 §2) — soundness-проверка в ЕДИНОМ чекере (§1), читает `ResolvedType`.**
   Рекурсивная `C_ABI` (Scalar/RawPtr/`Option[*T]`/Tuple/ValueRecord) — НЕ отдельный re-derive в
   FFI-build/codegen, а проверка над УЖЕ разрешённым носителем (нужна полная личность: value-record поля,
   tuple-элементы, `Option[*T]`-NPO). Пересекается с **172.4** (value-record/tuple by-value C-ABI = тот же
   value-ABI путь) — 172.4 и 178 §2 садятся на ОДИН value-ABI-резолв, не два.
3. **177: write-cap + `uninit`-ось — в едином чекере; `uninit T` как типовая модальность.** Pointer write-cap
   fix (`[M-138.5-unsafe-ptr-write-cap]`) и `uninit T` (data-uninit) живут в чекере единого движка; `uninit`-ось —
   потенциальная ось `ResolvedType` (рядом с L1/L2/L3 [D246](../../spec/decisions/02-types.md#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee)),
   если влияет на совместимость/коэрцию (`*mut uninit T↛*T`: сброс uninit unsound). Retire `*p`/`p+i`/`p[i]`→методы
   использует единый method-резолв (U.2/U.3 + U.4.3 dispatch). `02-types.md` — shared zone: spec-амендменты 177
   применять согласованно с 172/138.5 (177 это фиксирует в своей шапке).

## 4. Критерий «готово» (зонтик)

Зонтик закрыт, когда выполнены критерии приёмки **172.1** (единый источник истины, codegen не
резолвит/не выводит типы заново, C не ловит ошибок типов, никакого хардкода stdlib) и закрыты
**172.2**/**172.3**/**172.4** (единый value-ABI + авто-placement; acceptance-кейс value-record
fluent `mut @` + `==` проходит).

## 5. Принципы выполнения

Per [`compiler-conventions.md`](../compiler-conventions.md): этапами (не «большим взрывом»), каждый —
с регрессом против чистого бинаря (§6), blast-radius до правки (§6), без молчаливой ломки std.

## 6. Источник

Сессия 2026-06-19 (generic-arg mismatch + scalar-narrowing → обсуждение фрагментации типового
движка). Аудит «как есть» с file:line — в [172.1 §8](172.1-unified-type-engine.md).
