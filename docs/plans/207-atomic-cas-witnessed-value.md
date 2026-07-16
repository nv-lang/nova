<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 207 — `compare_exchange` возвращает свидетеля (bool → Result[unit, T])

**Статус:** ✅ ЗАКРЫТ 2026-07-15 (sonnet, ветка `plan207-cas`). **Приоритет:** P3
(эргономика/эффективность CAS-циклов; корректность не нарушена, но теряется бесплатная информация).
**Не блокирует ничего.** Ломающая правка публичного API `std/src/runtime/sync.nv` —
D-амендмент [D425](../../spec/decisions/06-concurrency.md#d425-cas-возвращает-свидетеля-провала-compare_exchange-bool--resultt-plan-207)
внесён в том же слиянии. Conformance 150/0; `sync_test` PASS (test-build).

### Итог (что реально сделано)

- **Сигнатура:** как задумано — `Result[(), T]` (Nova unit = `()`, не keyword `unit`).
- **Лоуэринг — отличается от исходного плана Ф.1:** вместо out-параметра —
  private `@cmpxchg` extern intrinsic возвращает raw `(ok bool, witness T)`
  value-struct напрямую (named-tuple `CasRaw*`, D215); strong/weak делят ОДИН
  intrinsic через явный `weak bool` параметр (не `_MemOrdering`-only suffix
  ambiguity). Публичный `compare_exchange`/`_weak` — plain (non-extern) `.nv`
  fn, строит `Ok(())`/`Err(witness)` обычным Nova-конструктором — `Result`
  монoморфизируется штатным generic-codegen, без hand-written C.
- **Codegen-фикс потребовался** (не предвиделся в исходном плане): NamedTuple
  типы в `RUNTIME_DEFINED_TYPES` (emit_c.rs) не регистрировали field-schema —
  ветка `emit_type_decl` знала только `Sum`/`Effect`. Добавлена зеркальная
  NamedTuple-ветка (field C-types + `NovaTuple_<Name>` type-alias, без
  struct-body emission).
- **Ф.3 (call-site migration):** реальных внутренних CAS-retry-циклов
  (`rc в tcp.nv`, семафоры) в кодовой базе НЕ нашлось — только
  `sync_test.nv` (3 сайта) + 10 `spec_tests/conformance/*.nv` (все на
  `.is_ok()`, минимальный диф).
- **Ф.4:** новый тест в `sync_test.nv` покрывает success/failure witness,
  explicit-ordering overload, weak spurious-failure witness, и полную
  CAS-retry-loop идиому (`Err(actual) => cur = actual`).

## Мотив (найдено при дизайне [206](206-arithmetic-overflow-policy.md))

`AtomicI*.compare_exchange`/`compare_exchange_weak` (`std/src/runtime/sync.nv`) возвращают `-> bool`,
**выбрасывая свидетеля провала**. C-примитив `atomic_compare_exchange_strong(&obj, &expected, desired)`
при провале ПИШЕТ фактическое прочитанное значение в `expected` — ровно то, что нужно в CAS-цикле,
чтобы пересчитать без повторного `load()` (лишний барьер + окно гонки). Мы эту информацию теряем.

Rust: `compare_exchange(cur, new) -> Result<T, T>` (`Ok(prev)` успех / `Err(actual)` провал с witness).

Нарушается принцип, зафиксированный в 206: **примитив не теряет информацию** (пара/`Result`), обеднять —
только осознанно. Здесь обеднение неосознанное — witness лежит в `expected` бесплатно.

## Дизайн

- **Целевая сигнатура:** `AtomicI* mut @compare_exchange(expected T, desired T, ...) -> Result[unit, T]`
  — `Ok(())` успех; `Err(actual)` провал, `actual` = фактически прочитанное (witness из `expected`).
  То же для `compare_exchange_weak`. Альтернатива-минимум `-> (bool, T)` — отвергнута (Result идиоматичнее,
  бьётся с остальным std).
- **Лоуэринг:** `nova_rt` intrinsic возвращает `bool` + пишет witness в out-параметр; `.nv`/codegen-обёртка
  собирает `Result[unit, T]` (успех→`Ok(())`, провал→`Err(*expected_out)`). Один источник, без второго окна.
- **CAS-цикл идиома** (после правки):
  ```nova
  mut cur = a.load()
  loop {
      ro next = f(cur)
      match a.compare_exchange(cur, next) {
          Ok(_) => break
          Err(actual) => cur = actual   // без повторного load(), witness даром
      }
  }
  ```

## Фазы
- Ф.0 Спека: D-амендмент семантики атомик-CAS (witness-возврат), сверка с D-блоком атомиков.
- Ф.1 Runtime/codegen: intrinsic отдаёт witness (out-param), обёртка собирает `Result[unit, T]`.
- Ф.2 std: сигнатуры всех `compare_exchange`/`_weak` (i8/i16/i32/i64 + указательные) на `Result[unit, T]`.
- Ф.3 Миграция колл-сайтов: `sync.nv` внутренние CAS-циклы (rc в tcp.nv, семафоры и т.п.) + тесты.
- Ф.4 Тесты: успех/провал witness-корректность, CAS-цикл round-trip, weak-spurious.

## Границы
Только форма возврата CAS (bool→Result). Не меняет memory-ordering, не трогает `fetch_*`/`load`/`store`.
 Retagged из backlog `[M-cas-return-witnessed-value]`.
