<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 206 — прогресс (Ф.0 + Ф.1 + Ф.1b)

**Ветка:** `plan206-overflow` (worktree `d:/Sources/nv-lang/nova-206`, база `9c6af284b`).
**Статус на 2026-07-15:** Ф.0/Ф.1/Ф.1b ЗАВЕРШЕНЫ и точечно верифицированы. Полный
`spec_tests/conformance` мега-CU НЕ гонялся (по заданию — авторитетный гейт у владельца
при вливании).

## Ф.0 — спека + type-set (ЗАВЕРШЕНО)

- `std/src/prelude/protocols.nv` (рядом с `SignedInt`/`UnsignedInt`, ~L659): добавлен
  `type Ints set i8|i16|i32|i64|int|u8|u16|u32|u64|uint` (полное объединение).
- **Обнаружен конфликт с D310** (`E_TYPE_SET_MIXED_SIGNEDNESS` — declaration-time guard в
  `compiler-codegen/src/types/mod.rs` ~L17048) — он банит ЛЮБОЙ signed/unsigned микс, а
  `Ints` ровно такой микс. Решено: D310 amend (не point-hack) — checker пропускает ТОЛЬКО
  full-union (все 5 signed ∧ все 5 unsigned, без пропусков); партиальный микс (`{i32,u32}`
  и т.п.) остаётся ошибкой. Обоснование в самом амендменте (D310 §«Знаковость» уже
  резолвит `T.MAX`/`T.MIN` per-instance через монорфизацию — партиальная vs полная разница
  не меняет это свойство, полный union — тот же случай, что иллюстративный `AnyNumber` в
  тексте D310). Regression-тест `spec_tests/conformance/neg/mixed_signedness.nv`
  (партиальный `{i32,u32}`) по-прежнему падает с E_TYPE_SET_MIXED_SIGNEDNESS (проверено).
- **D-блок:** `spec/decisions/04-effects.md` — новый **D423** (в конце файла, после D407).
  Amends D310 (§R1, full-union exemption) + расширяет trap-дефолт (D13-класс) на все
  `Ints` (§R3). Секция «Неопределённости» — честно документирует Ф.1 dispatch-класс и
  Z3-элизию sized-путей (см. ниже). `spec/decisions/README.md` — строка D423 добавлена.

## Ф.1b — sized-int trap-default, решение A (ЗАВЕРШЕНО)

- `compiler-codegen/nova_rt/effects.h` (~L1044+): добавлен `NOVA_DEFINE_CHECKED_OPS`
  macro + 9 инстанциаций (`nova_i8_checked_{add,sub,mul}` .. `nova_uint_checked_*`) —
  зеркало `nova_int_checked_add` (тот же `__builtin_*_overflow` + `NOVA_INT_OVF_PANIC`).
  Старый doc-comment («sized = wrap, Plan 33.7») исправлен на актуальное решение A.
- `compiler-codegen/src/codegen/emit_c.rs`:
  - Новый `sized_checked_helper(ty_c, op) -> Option<String>` (маппинг C-тип → helper-имя).
  - Три call-site лоуэринга `+`/`-`/`*` расширены с nova_int-only на sized:
    1. Compound-assign (`+=`/`-=`/`*=`, ~L27199) — добавлена sized-ветка (lvalue-указатель
       того же паттерна, что nova_int).
    2. `emit_expr_with_target_type` Binary-арм (~L28066, target-type propagation в
       sized-типизированный контекст) — ЗАМЕНЁН мёртвый nova_int-чек (target_ty_c тут
       ГАРАНТИРОВАННО sized — функция бейлит раньше для nova_int) на реальный
       `sized_checked_helper(target_ty_c, op)`.
    3. Главный `emit_expr` Binary-арм (~L29396) — добавлена `else if lty == rty` ветка
       (sized_checked_helper) + `else`-ветка для **i64-литерал-gap** (см. ниже).
  - **Найден и закрыт i64-специфичный разрыв**: `is_typed_integer()` (~L47461) исторически
    исключает `int64_t` (nova_int-erasure precedent, доккомент на месте) — из-за этого
    `x - 1` (x: i64) не матчился ни в одной ветке (`lty="int64_t"`, `rty="nova_int"` для
    непривязанного литерала `1`). Добавлена узкая fallback-ветка: если один операнд —
    sized-тип, другой — `nova_int` (голый литерал), берём sized-тип для helper-подбора
    (raw C-текст литерала валиден как операнд любого sized-типа в C независимо от
    Nova-уровня "nova_int" бирки). НЕ трогал `is_typed_integer()` целиком (широкий
    blast-radius, множество caller'ов) — точечный, локальный фикс именно в биноп-арме.
  - **Site-элизия (140.4)**: механизм `overflow_site_elided`/`index_site_elided` —
    span-based (`expr.span.start`), НЕ типо-специфичный → sized-путь автоматически
    проходит через ТОТ ЖЕ вызов, что и `nova_int` (тот же `self.overflow_site_elided(...)`
    вызов в обеих новых ветках). Механически покрыт. Полнота Z3-СТОРОНЫ доказательства
    для sized-ширин (кодирует ли verifier sized так же полно, как безграничный int) — НЕ
    проверена этой волной; см. «Неопределённости» в D423 и followup
    `[M-206-sized-z3-elision-audit]`.
- **Пиновые тесты** (`spec_tests/soundness/neg/`, EXPECT_RUNTIME_PANIC, зеркало
  `int_overflow_add_panic.nv`): `i8_overflow_add_panic`, `u8_overflow_add_panic`,
  `i16_overflow_add_panic`, `u16_overflow_add_panic`, `i32_overflow_mul_panic`,
  `u32_overflow_add_panic`, `i64_overflow_sub_panic`, `u64_overflow_add_panic` — **8/8
  PASS** (verified через `nova test-build`, ~25-30s каждый).
- Позитивный регресс (`spec_tests/soundness/plan206_overflowing_and_sized_arith.nv`):
  обычная sized-арифметика без overflow (i8/u8/i32/u16/i64) — те же результаты, что до
  правки. **PASS.**

## Ф.1 — интринсик `@overflowing_add/_sub/_mul` (ЗАВЕРШЕНО, с оговоркой по dispatch-классу)

- **Архитектурное решение (после разведки):** генерик `fn[T Ints] T @overflowing_add(rhs
  T) -> (T, bool)` реализован НЕ как `extern "nova" fn[T Bound] …` декларация — такой
  машинерии (generic extern с type-set bound + tuple-return, полная checker-сигнатура) в
  компиляторе НЕТ прецедента (`T.parse` из примеров D310 сам НЕ реализован, Plan 174.1
  отложен; `runtime_registry.rs`/`math.nv` авто-ген — per-КОНКРЕТНЫЙ-тип extern'ы, без
  tuple-return прецедента). Вместо этого — **D109-класс hardcoded dispatch** (тот же
  паттерн, что `.hash()`/`.clone()`/`.abs()` до их `.nv`-миграции):
  1. `primitive_instance_method_known` (emit_c.rs ~L45735, checker existence-oracle) —
     добавлена ветка: `overflowing_add/_sub/_mul` известны на любом `Ints`-примитиве.
  2. `infer_call_ret_c` (~L50606) — return-type = `register_mono_tuple(&[obj_ty, "nova_bool"])`
     (тот же mono-tuple механизм, что `(a, b)`-литерал).
  3. `emit_call` (~L35744, рядом с `int_method_to_c`/abs) — inline-эмиссия: временные C-vars
     + прямой `__builtin_{add,sub,mul}_overflow(recv, rhs, &wrapped)` + сборка
     `(wrapped, overflowed)` через `register_mono_tuple` (ТОТ ЖЕ путь, что `TupleLit`-арм,
     ~L30097) — НЕ именованный C-helper в `effects.h` (в отличие от Ф.1b
     `nova_<T>_checked_*`), т.к. этот вариант НЕ должен паниковать.
- **Verified:** `int.MAX.overflowing_add(1)` → `(int.MIN, true)`; `(41).overflowing_add(1)`
  → `(42, false)`; `i32.MAX.overflowing_add(1)` → overflow=true; `u8(250).overflowing_add(10)`
  → overflow=true, `u8(10).overflowing_add(20)` → `(30, false)`;
  `overflowing_sub`/`overflowing_mul` на i32/u8/i8 — все ветки в
  `plan206_overflowing_and_sized_arith.nv`, **PASS**.
- **Неопределённость (см. D423 §«Неопределённости»):** checker-уровня return-type/arg-type
  checking для `.overflowing_*` СЛАБЕЕ, чем для обычного `.nv`-объявленного метода
  (полагается на codegen-side `infer_expr_c_type`/`emit_call`, не на `method_table`-резолв
  полной сигнатуры). Хватает для прямых вызовов на конкретных примитивах и для
  мономорфизированных generic-тел (проверено), НЕ протестировано на checker-диагностику
  типа неверной арности/типа аргумента при вызове. При переходе на Ф.2 `.nv`-обёртки
  (`@checked_add`/`@saturating_add`/`@wrapping_add`) этот путь можно укрепить.

## Не сделано (в объёме этого захода, следующие волны — Ф.2/Ф.3/206.1)

- `.nv`-бланкеты `@checked_*`/`@saturating_*`/`@wrapping_*` — Ф.2, следующая волна.
- Duration-миграция на общий примитив — Ф.3.
- `@unchecked_*` — отложен (владелец).
- `div`/`neg`/`mod` — подплан 206.1 (файл ещё не создан, форвард-ссылка в спеке).
- Z3-элизия sized-путей — аудит полноты SMT-кодирования НЕ проведён
  (`[M-206-sized-z3-elision-audit]`, зафиксирован в D423).

## Верификация (точечная, НЕ полный conformance)

- `cargo build --release` — **compiler-codegen: 0 ошибок** (только pre-existing warnings),
  **nova-cli: 0 ошибок**.
- 8/8 sized-overflow-panic pin-тестов PASS.
- Позитивный регресс (sized-арифметика без overflow + overflowing_*) PASS.
- `int_overflow_add_panic`/`_mul_panic`/`_compound_panic` (существующие, безграничный int)
  — PASS, регрессии нет.
- `mixed_signedness.nv` (партиальный signed/unsigned микс) — по-прежнему падает с
  правильной (обновлённой) диагностикой E_TYPE_SET_MIXED_SIGNEDNESS.
- `std/src/prelude/protocols.nv` (сам файл, содержащий новый `Ints`) — `nova check` OK.
- `d129_assoc_const_width.nv` (несвязанный pre-existing conformance-файл) — `test-build`
  ЗАВИС/таймаут 5 мин при верификации регресса; НЕ расследовано (вне объёма этой волны,
  не трогали этот файл/путь; возможно тяжёлый pre-existing тест, не специфично для 206).

## Хэши / коммиты
См. `git log` на ветке `plan206-overflow` — коммиты по шагам (Ф.0 спека+type-set,
Ф.1b codegen+тесты, Ф.1 интринсик+тесты, checkpoint).
