<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 143 — Deferred enhancements umbrella (post-138)

> **Создан:** 2026-06-11.  **Статус:** 📋 PLANNED (backlog umbrella; future, не imminent).
> **Назначение.** Зонтик над floating-маркерами без отдельного плана-дома, сгруппированными по теме.
> Каждая секция (§) — когерентная группа; при исполнении спинится в sub-план (143.1/143.2/143.3).
> Полный OPEN-список маркеров — [backlog-followups.md](backlog-followups.md); здесь — scope/phases.

---

## §1 (→143.1) — Iterator combinators completion  [P2 ergonomics]
**Owns:** `[M-combinators-completion]`, `[M-opt-iter-generic-combinators]`.

Ядро есть (generic `[]T`: map/filter/fold/any/all, eager; `[]int`-only sum/min/max). Пробелы:
- **Добавить:** `find` (short-circuit→`Option[T]`), `flat_map` (nested comprehensions), `enumerate` (`(i,x)`), `zip`.
- **Обобщить:** `sum`/`min`/`max` `[]int` → generic `[]T` (Num/Comparable bound).
- **Главный рычаг:** комбинаторы generic над `Iter[I]` (не только `[]T`) → работают на Range/HashMap/custom без материализации.
- **НЕ нужно:** collect (eager), take/skip (Index[Range] `xs[a..b]`), reduce (fold), count (filter+len).

Эстимат ~1-1.5 dev-day. Эффект: comprehension-паритет с Python для nested + non-`[]T`.

## §2 (→143.2) — Perf optimizations (escape/Z3-driven, correctness-neutral)  [P2 perf]
**Owns:** `[M-opt-value-sum-types]`, `[M-opt-auto-scoped-ref]`, `[M-opt-elide-proven-overflow-checks]`.
Gate — стабильное ядро. Каждая независима (~1-2 dev-day), может стать отд. под-планом.
- **value-sum-types:** compiler-inferred value(stack)/heap для sum (non-recursive && small && non-escaping → stack tagged-union; иначе heap). Прозрачно (sum immutable). + payload-less интернирование. **Verify** immutability. Связь 120/139.
- **auto-scoped-ref:** escape-driven авто pass-value-param-by-ref + return-slot elision (NRVO); обобщить ресивер-`&obj`. Прозрачно.
- **elide-proven-overflow-checks:** Z3/range-элизия доказуемо-безопасных `nova_int_checked_*` (proven→elide, как Plan 140); сохраняет safety, убирает overhead.

## §3 (→143.3) — Codegen output cleanup (generated-C polish)  [P3 low]
**Owns:** `[M-codegen-dead-erased-generic-stubs]`, `[M-codegen-unit-block-temp-elision]`, `[M-codegen-src-synthesized-attribution]`.
Косметика (рантайм-нейтрально — clang -O2 и так чистит); читаемость дебага + чуть быстрее компиляция.
- **dead-erased-stubs:** DCE type-erased `Vec[any]` (prelude-вариадик) NULL-stub методов.
- **unit-block-temp-elision:** не материализовать `_nv_tmp=NOVA_UNIT` для unit-block в discard-позиции.
- **src-synthesized-attribution:** `/* SRC */` атрибутировать синтезированный C к породившему оператору.
Эстимат ~0.5 dev-day суммарно.

---

## Acceptance (umbrella)
- Каждая § исполняется независимо как 143.N, закрывает свои маркеры, 0 регрессий, тесты через релизный nova.
- §1/§2 — feature/perf (spec+tests); §3 — cosmetic (проверка чистоты вывода).
- При закрытии § — убрать её маркеры из backlog OPEN-view.

## Приоритет
§1 (DX-выигрыш) > §2 (perf, после стабилизации ядра) > §3 (cosmetic, low). Все — после imminent-работы (141/142/139/капстоун).

## Связь
Зонтик; sub-планы 143.1/143.2/143.3 при исполнении. Маркеры — в backlog-followups.md (home → Plan 143 §N).
