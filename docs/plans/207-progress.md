<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 207 — checkpoint (2026-07-15, sonnet, ветка `plan207-cas`)

**Статус: ЗАКРЫТ этой волной.** Полный отчёт в [207-atomic-cas-witnessed-value.md](207-atomic-cas-witnessed-value.md#итог-что-реально-сделано).

## Что сделано

1. **Разведка:** старая CAS-сигнатура — `extern "nova" fn AtomicX mut @compare_exchange(...) -> bool`,
   реализована hand-written C в `compiler-codegen/nova_rt/sync_primitives.h`
   (`Nova_AtomicX_method_compare_exchange*`, `__atomic_compare_exchange_n`).
   Witness (актуальное значение при провале) уже писался C11-примитивом в
   локальную `expected`-переменную и просто отбрасывался.

2. **Архитектурная развилка (найдена, решена БЕЗ эскалации на opus):**
   - Публичный `Result[(), T]` нельзя было хардкодить в C напрямую (Result —
     `PointerErrorLike` heap-ABI для произвольных `(ok,err)`, а генерация
     struct'а компилятором происходит ПОСЛЕ `#include nova_rt.h` — hand-написанная
     C-функция внутри заголовка не может ссылаться на компилятор-сгенерированный
     тип). Решение: private extern intrinsic (`@cmpxchg`) возвращает raw
     `(ok bool, witness T)` value-struct (named-tuple `CasRaw*`, D215,
     `NovaTuple_CasRaw*`), а публичный `compare_exchange`/`_weak` — **plain
     (non-extern) `.nv` fn**, строящий `Ok(())`/`Err(witness)` обычным
     Nova-конструктором. `Result[(), T]` монoморфизируется штатным
     generic-codegen (как `Vec.binary_search -> Result[int,int]`) — hand-written
     Result C не понадобился вообще.
   - Это потребовало codegen-фикс: `NovaTuple_CasRaw*` типы зарегистрированы в
     `RUNTIME_DEFINED_TYPES` (emit_c.rs) — так же, как `MutexGuard`/`MemOrdering` —
     но ветка `emit_type_decl`, обрабатывающая `RUNTIME_DEFINED_TYPES`, раньше
     регистрировала field-schema только для `Sum`/`Effect` kind (для
     `NamedTuple` падала в generic unknown-type fallback `Nova_<Name>*`
     pointer-record — НЕ совпадает с реальным value-struct'ом в хедере, битые
     локальные var-декларации). Добавлена зеркальная NamedTuple-ветка
     (`compiler-codegen/src/codegen/emit_c.rs`, внутри `RUNTIME_DEFINED_TYPES`
     блока `emit_type_decl`): регистрирует `record_schemas`/`record_field_order`/
     `value_struct_field_tys`/`type_aliases` БЕЗ эмиссии struct-body.
   - Проверено эмпирически: минимальный `nova-codegen compile` пробник
     ДО фикса давал битый C (`Nova_CasRawI32*` pointer вместо
     `NovaTuple_CasRawI32` value, отсутствующий тип локальной переменной);
     ПОСЛЕ фикса — корректный C (`NovaTuple_CasRawI32 r = ...; r.ok`/`.witness`
     прямой member access).

3. **Фикс — `Result[(), T]` + witness из `atomic_compare_exchange`:**
   - `compiler-codegen/nova_rt/sync_primitives.h`: новый блок
     `NovaTuple_CasRaw{I8,I16,I32,I64,U8,U16,U32,U64,Int,Uint,Bool}` (11 struct'ов,
     `Int` разделяют `AtomicIsize`/`AtomicPtr`/legacy `AtomicInt`) + 13
     `Nova_AtomicX_method_cmpxchg` функций (по одной на атомик-тип; strong+weak
     делят один intrinsic через `nova_bool weak` параметр — старые 4
     bool-возвращающие функции на тип УДАЛЕНЫ, не оставлены dead).
   - `std/src/runtime/sync.nv`: 11 `type CasRaw*(ok bool, witness T)`
     (module-private, не export — только для type-checker'а, C-структура
     живёт в хедере); `extern "nova" fn AtomicX mut @cmpxchg(...)` (private);
     `compare_exchange`/`compare_exchange_weak` (все 4 overload-формы × 13
     типов, кроме legacy `AtomicInt` — 1 форма как раньше) — теперь plain `fn`,
     строят `Result[(), T]` из raw-пары.

4. **D-амендмент (D425, `spec/decisions/06-concurrency.md`)** — amends D168 §1
   (матрица операций CAS-строк `bool`→`Result[(), T]`), + README.md таблица.
   Свободный номер сверен по `spec/decisions/README.md` (D423/D424 заняты,
   D425 свободен на момент проверки).

5. **Call-сайты:** `std/src/runtime/sync_test.nv` (3 старых assert починены
   на `.is_ok()`/`.is_err()` + новый тест «Plan 207 witness» — success/failure/
   explicit-ordering/weak-spurious/retry-loop-идиома); 10 файлов
   `spec_tests/conformance/*.nv` (все compare_exchange/`_weak` call-сайты →
   `.is_ok()`, минимальный диф, поведение не меняется). Внутренних
   CAS-retry-циклов в остальном std (tcp.nv, семафоры и т.п.) не нашлось —
   единственные потребители были в тестах.

## Гейты (пройдены)

- `sync_test.nv` через `nova-codegen test-build` (dev/boehm) — **PASS**
  (компилируется, линкуется, рантайм-корректен: witness == actual current
  value на провале, retry-loop-идиома работает).
- `spec_tests/conformance` — ОДИН compile-unit,
  `nova test --positive --compile-error ../spec_tests/conformance` —
  **150 PASS / 0 FAIL**, exit 0.
- `cargo build --release` (compiler-codegen + nova-cli) — чисто (только
  pre-existing warnings, 0 новых ошибок).

## Хэши / модель

Модель: **sonnet** (весь атом, без эскалации на opus — развилка решена
эмпирической проверкой, не архитектурным тупиком).
Коммиты — см. `git log` на `plan207-cas` (по шагам: codegen-фикс,
std+conformance миграция, спека+docs).
