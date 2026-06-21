<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 172 — Переработка компилятора: типовая система (umbrella)

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

**Не входят** (остаются top-level, другая тема): Plan 171 (primitive parse API — stdlib-поверхность,
зависит от 172.3), Plan 170 (file-private visibility — видимость).

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
  ~13 per-type обёрток Plan 171 в ~2-3 generic).

## 3.1. Forward-compat с системой ошибок (Plan 173/174/175) — чтобы 173 не переделывал 172

Анализ 2026-06-21 (по запросу владельца): [Plan 173](173-error-system-unify-harden.md) (error/cleanup),
[174](174-question-mark-return-only.md) (`?`-return-only), [175](175-any-type-and-is-downcast.md)
(`any`+`is`-downcast) садятся на типовой движок 172. Чтобы 173 строился ПОВЕРХ единого движка, а не
переделывал его, **172.1 обязан учесть 4 точки** (в основном осознанность/lossless, не большие правки):

1. **Эффекты в `ResolvedType` — LOSSLESS, с type-args (`Fail[E]` несёт `E`).** Сейчас
   `ResolvedType::Func.effects: Vec<String>` (только имена, `from_type_ref` берёт `path.last()`) —
   **лосси** (теряет `E` у `Fail[E]`). Это и [D315](../../spec/decisions/02-types.md#d315-resolvedtype--единый-канонический-носитель-типа-plan-1721-2026-06-21)-нарушение (носитель обязан быть lossless), и блокер
   typed-errors 173/175. **U.5.5 обязан нести полный эффект-тип** (имя + type-args), не bare-name.
   Иначе 173 Ф.4 (типизированный `Fail[E]`/`ScopeOutcome.Failure(any)`) переделывает носитель.
2. **`any` + `is`/downcast — согласовать с Plan 175.** 172-резолв `Any`/`is`-теста НЕ должен
   форклоузить модель Plan 175 (`any`-тип + `is T`/downcast по `type_id`) — 175 строит typed-error
   dispatch 173 поверх ЕДИНОГО движка. Координировать дизайн `Any`/`is` в U.4 с Plan 175.
3. **`[M-parfor-record-result-miscompile]` чинится U.4 ПО ПОСТРОЕНИЮ.** Чекер типизирует
   `parallel for → []T` для ЛЮБОГО `T`, а array-mode codegen (`emit_c.rs:8154`) умеет лишь
   `T∈{int,bool,f64,str}` → молчаливый degrade в `unit` → утечка C-error. Это ровно §0
   checker↔codegen рассинхрон. Унифицированный `[]T`-lowering (any `T`, U.4.4) ДОЛЖЕН покрыть
   parfor-result-array (не оставить отдельным re-deriving путём) → 173 Ф.3 тривиален. 172 берёт
   этот маркер в свой радар.
4. **Error-типы (`Result`/`MultiError`/`ScopeOutcome`) — generic mechanism (§3), не hardcode.**
   172.1 U.1 (де-хардкод stdlib) обязан не спец-кейсить error-типы → 173 эволюционирует их
   (materialize `MultiError`, `Failure(any)`) без борьбы с 172-хардкодом.

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
