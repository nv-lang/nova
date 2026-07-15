<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 206 — прогресс (Ф.0 + Ф.1 + Ф.1b + Ф.2)

**Ветка:** `plan206-overflow` (worktree `d:/Sources/nv-lang/nova-206`, база `9c6af284b`,
смёржен `main` @ `73ab1c44f` (`9e96817c2`), `c65a3ecc4` watchdog (`8906621e2`),
`ae9dc90a3` vendor-FFI/lock (`c028036aa`)).
**Статус на 2026-07-15:** Ф.0/Ф.1/Ф.1b/Ф.2 + Ф.3 (Duration-миграция) ЗАВЕРШЕНЫ и точечно
верифицированы. Полный `spec_tests/conformance` мега-CU НЕ гонялся (по заданию —
авторитетный гейт у владельца/оркестратора; см. находку ниже — каталог физически НЕ
таргетируем per-файл).

## Ф.3 раунд-3 — Duration checked_*/saturating_* дедуп D317 (ЗАВЕРШЕНО)

Пере-гейт (полный conformance) вернул 2 падавших блока `spec_tests/conformance/
d317_duration_overflow_policy.nv`: `checked_* — None on overflow` и `saturating_* clamps
to ±MAX` — оба `integer overflow: *`. Корень: приватные Duration-хелперы использовали
СВОЮ i64-арифметику, трапящую под sized-int trap-default (Ф.1b) ДО возврата None/клампа.

- **Корень — `checked_mul_i64` (`std/src/time/duration.nv`).** Считал `ro r = a * b`
  (голый i64 `*`) и проверял через divide-back `if r / a != b { None }` — классический
  wrap+divide-back-идиом, ТРЕБУЮЩИЙ wraparound. Под 206 голый `a * b` ТРАПИТ на overflow
  ДО divide-back → `checked_mul(2)`/`saturating_mul(2)` на `i64::MAX` паниковали вместо
  None/кламп. (`checked_add_i64`/`checked_sub_i64` были guarded — не трапили; но дублировали
  overflow-детект вручную.)
- **Миграция (дедуп D317, план Ф.3):** три хелпера `checked_add_i64`/`checked_sub_i64`/
  `checked_mul_i64` теперь однострочно делегируют Ints-бланкетам `a.checked_add(b)` /
  `.checked_sub(b)` / `.checked_mul(b)` (поверх компиляторного `@overflowing_*` =
  `__builtin_*_overflow`). Ручные i64-range-проверки (add/sub) и wrap+divide-back (mul)
  УДАЛЕНЫ — overflow-детект теперь один (аппаратный флаг, «одно окно» 196). Поведение
  байт-идентично (None ⟺ i64-overflow). `checked_neg_i64`/`checked_div_i64`/`clamp_i64`/
  `sat_*_i64`/трап-обёртки (`add_or_trap` и т.д.) НЕ тронуты — `sat_*_i64` зовут
  мигрированные `checked_*_i64` и теперь не трапят; `saturating` сохраняет СВОИ Duration-
  границы `±(2⁶³−1)` (передаются как `lo`/`hi` = `-i64_max()`/`i64_max()`, отличны от
  i64.MIN бланкета — поэтому НЕ заменял `saturating_*` на бланкет `@saturating_*` напрямую,
  только overflow-ДЕТЕКТ через `checked_*`, а направление клампа — Duration-специфичное).
- **f64-пути (`checked_mul_f64`/`f64_nanos_checked`) НЕ тронуты** — там overflow-детект по
  f64-границам, не целочисленный `__builtin` (план 206 §Ф.3 явно: это конверсия, не
  дублирование int-примитива).
- **Duration-конструкторы (`from_secs`: `s * 1e9` и т.п.) НЕ тронуты** — голый `*` там
  трапит на ЗАВЕДОМО-огромном входе (корректное tier-1 поведение, не намеренный wrap);
  d317-тесты используют `from_nanos` для граничных значений, конструкторы не хитят.
- **d317 ДО/ПОСЛЕ:** checked_mul/saturating_mul на `i64::MAX` ДО — `RUN-FAIL … integer
  overflow: *` (трап внутри хелпера); ПОСЛЕ — `checked_mul(2)` → `None`, `saturating_mul(2)`
  → кламп к `d317_MAX`, `saturating_mul(-2)` → `-d317_MAX`. Own-CU репро
  `spec_tests/soundness/plan206_d317_duration_checked_saturating.nv` (точная копия обоих
  блоков + @abs) — PASS. Peer `std/src/time/overflow_safe_test.nv` (полный публичный
  surface: Timestamp/Monotonic boundary-saturate, @abs(i64::MIN)) — PASS (нет регрессии).
- Верификация раунда-3 (после `git merge main` vendor-FFI + пересборка начисто, 0 ошибок):
  d317-репро + overflow_safe_test PASS; 11-тестовый регресс (Duration units/surface +
  8-... сокращённые sized-пины + Ф.1/Ф.2/D404/vec-hash/blanket) PASS; soundness зелёные.

## Ф.2 раунд-2 — corpus-overflow-сайты вскрытые полным conformance-гейтом (ЗАВЕРШЕНО)

Оркестратор прогнал полный conformance после раунда-1 и вернул FAIL: ещё не-мигрированные
overflow-сайты. Корень оказался НЕ в корпусе, а снова в **std** (пропущен в раунде-1):

- **`std/src/collections/vec/protocols.nv` — `Vec[T Hash] @hash()` (FNV-1a, mod-2^64).**
  Корневая причина всех 3 падавших блоков `spec_tests/conformance/hash.nv` (`equal vecs
  hash equal`, `empty + single round-trip`, `content + length distinguish`) — они лишь
  ЗОВУТ `.hash()`, а `* prime` внутри (строки 151/157) трапил (`integer overflow: *`).
  Мигрировано на `.wrapping_mul(prime)` (модульно по замыслу). Doc-комментарий про «u64
  arithmetic wraps on overflow (Nova semantics)» обновлён — под 206 wrap уже НЕ дефолт,
  wrapping явный. Репро в own-CU: `spec_tests/soundness/plan206_vec_hash_wrapping.nv`
  (те же 3 блока) — PASS.
- **`spec_tests/conformance/d404_sized_arith_width.nv` — u8+u8 / u16*u16 test-блоки.**
  Тест кодировал СТАРУЮ wrap-семантику голого оператора (`u8(200)+u8(100)==44`,
  `u16(60000)*u16(2)==54464`) — под 206 голый `+`/`*` там ТРАПИТ. Мигрирован на явный
  `.wrapping_add`/`.wrapping_mul` (тот же различающий D404-сигнал: коллапс в nova_int дал
  бы 300/120000, u8/u16-ширина → 44/54464). `uint+uint` (2^63<uint.MAX) и `i32+i32`
  (control) НЕ переполняются — голый `+` там сохранён. НЕ ослабление — та же
  width-preservation-проверка D404, просто через явный wrapping.

**u16-D404: это СТАЛЫЙ ТЕСТ, НЕ баг Ф.1b (доказано эмпирически).** Ключевой вопрос
оркестратора — трапит ли `u16*u16` на ПРАВИЛЬНОЙ ширине (u16 >65535), а не на ширине
nova_int (64-bit). Проверено двумя изолированными тестами:
  - `spec_tests/soundness/neg/u16_overflow_mul_width_panic.nv` (NEW pin): `u16(60000)*
    u16(2)` = 120000 → **TRAP** (PASS, runtime-panic). 120000 ВЛЕЗАЕТ в 64-битный
    nova_int (не переполнил бы широкий слот), но ПЕРЕПОЛНЯЕТ u16 → trap firing ДОКАЗЫВАЕТ,
    что Ф.1b детектит overflow на u16-ширине через `nova_u16_checked_mul`/
    `__builtin_mul_overflow` на uint16_t-операндах. Если бы Ф.1b лоуэрил u16*u16 через
    nova_int-checked, теста не было бы паники (RUN-OK с 120000).
  - `spec_tests/soundness/plan206_d404_width_wrapping.nv` (NEW): `u16(60000).wrapping_mul(2)
    == 54464` (не 120000) — wrapping тоже идёт по u16-ширине, тип не схлопнут. PASS.
  Вывод: Ф.1b sized-trap корректен для u16 (и всех sized) — тест `d404` просто устарел
  относительно 206-политики, мигрирован (не «починка реализации»).

**Аудит остального std на пропущенные overflow-сайты (раунд-2):** грепнуто
`* prime|* 0x{6,}|h *|acc *`. Кандидаты и вердикты:
  - `std/src/identifiers/uuid.nv:322` (`acc*16+d`, hex-parse) — bounded: макс UUID-сегмент
    12 hex-цифр = 48 бит, `acc*16` никогда не переполняет u64 → голый `*` не трапит
    ложно, оставлен (trap корректен для malformed over-long input).
  - `std/src/runtime/string/parse.nv:56/86` — уже ЯВНЫЙ overflow-guard ПЕРЕД умножением
    (`if acc > (max-d)/radix { Err(Overflow) }`) → checked-parse, никогда не wrap, оставлен.
  - `std/src/time/civil/{parse,tz}.nv` — `h*3600+m*60`, малые часы/минуты, нет overflow.
  Единственная реальная миграция — `Vec.hash()` выше.

Верификация раунда-2 (после `git merge main` watchdog + пересборка компилятора начисто,
0 ошибок): 13 точечных тестов PASS (8 sized-pin + u16-width-pin + Ф.1-regression +
D404-width-repro + vec-hash-repro + Ф.2 blanket) + 3 std hash-теста (fnv/bloom/sha256)
PASS. Soundness 8/8 + 1 новый width-pin зелёные.

## Ф.2 — три `.nv`-бланкета + миграция overflow-зависимого std (ЗАВЕРШЕНО)

- `std/src/prelude/protocols.nv` (сразу после `type Ints`): `@checked_add/_sub/_mul(rhs T)
  -> Option[T]`, `@wrapping_add/_sub/_mul(rhs T) -> T`, `@saturating_add/_sub/_mul(rhs T)
  -> T` (op-специфичная клампинг-формула — см. D423 §R4). Все три вызывают
  компиляторный `@overflowing_*` (Ф.1) и не дублируют overflow-детект.
- Тесты: `std/src/math/overflow_policy_test.nv` (8 test-блоков, все receiver'ы —
  типизированные локали, НЕ inline `TypeName(литерал)` — см. находку ниже). НЕ рядом с
  `protocols.nv` — `std.prelude.*` имеет auto-import global prelude отключённым (cycle
  protection), что ломает `assert()`-инфраструктуру (`Nova_StringBuilder` struct-tag
  CC-FAIL) для ЛЮБОГО теста в этом namespace; до Ф.2 там не было ни одного `*_test.nv`.
- **Миграция гейт-блокера**: `spec_tests/conformance/inline_xoshiro_determinism.nv`
  (xoshiro256++/splitmix64 — реальный файл, НЕ `app_effect_basic_t8_1.nv`, как было в
  исходном задании; последний вообще не содержит PRNG-кода) + производственный
  `std/src/testing/handlers.nv::seeded`. Было RED (`integer overflow: *`) до Ф.1b
  trap-default — теперь явный `.wrapping_add`/`.wrapping_mul`.
- **Аудит std нашёл ещё 5 файлов** с тем же классом бага (mod-2^32/2^64 арифметика по
  спецификации алгоритма): `std/src/checksums/fnv.nv` (было RED,
  `RUN-FAIL … integer overflow: *`), `std/src/collections/bloom_filter.nv`,
  `std/src/crypto/md5.nv`/`sha1.nv`/`sha256.nv` (RFC 1321/FIPS 180-4 compression —
  скорее всего тоже были бы RED на реалистичных входах, mod-2^32 add почти гарантированно
  переполняется каждый блок). Все контрольные `*_test.nv` — PASS с идентичными test
  vectors после миграции (чисто синтаксическая замена, поведение не менялось).
- **Три pre-existing codegen/checker разрыва найдены** (НЕ фикс в рамках 206 — отдельно
  трекнутые/задокументированные в D423 §«Неопределённости»):
  1. Chaining `.checked_add`/… напрямую на primitive type-conversion CALL
     (`i32(10).checked_add(5)`) -> `[P67-LEGACY] method call return type unknown` ICE
     (`emit_c.rs:51424`). НЕ бьётся на ident/field/index/cast/free-fn-call receiver.
     Тот же класс, что остальные `P67-LEGACY` (Plan 196.5 Stage-D — активная отдельная
     чистка).
  2. `Option[T] == Some(int-литерал)` для sized не-`int` T не адаптирует литерал к `T` —
     уже зарегистрированный `[M-option-eq-some-literal-elem-adapt]`
     (`docs/plans/backlog-followups.md`, OPEN, Plan 172.2, P2).
  3. `spec_tests/conformance` — 970 файлов ОДНИМ логическим модулем
     (`module spec_tests.conformance`) → `nova test`/`nova check` на ЛЮБОМ отдельном
     файле разрешает и компилирует ВЕСЬ каталог (мега-CU, десятки минут) — точечный
     per-файл прогон физически невозможен. `spec_tests/soundness/**` не страдает (каждый
     файл — уникальный модуль, ~25-30s прогон). Из-за этого `inline_xoshiro_determinism.nv`
     верифицирован ТОЛЬКО через `nova check` (type-check) + семантическую эквивалентность
     мигрированной формулы (доказано через изолированные `wrapping_add`/`wrapping_mul`
     unit-тесты в `overflow_policy_test.nv`), не через полный `nova test` в этой волне.
- **D423 дополнен** (не новый D-номер — Ф.2 уже был явно forward-referenced в исходном
  D423 §R4 как «следующая волна», теперь landed тем же блоком): конкретные сигнатуры,
  список миграций, три находки выше.
- Sync: `git merge main` (30 коммитов, включая 196.7 method-dispatch/TLS-диамант/209) —
  ОДИН конфликт (`spec/decisions/README.md`, обе строки D423/D424 сохранены, D423 текст
  дополнен); `emit_c.rs`/`types/mod.rs` авто-смёржились без конфликта. Компилятор
  пересобран начисто (0 ошибок), 15 точечных тестов (8 soundness pin + Ф.1 regression +
  Ф.2 blanket-тест + 5 мигрированных hash/PRNG модулей) — все PASS после ребилда.
- Не сделано (следующая волна): `@unchecked_*` (отложен владельцем), Duration-миграция
  (Ф.3), `[M-206-sized-z3-elision-audit]`.

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

## Не сделано (в объёме этого захода, следующие волны — Ф.3/206.1)

- `.nv`-бланкеты `@checked_*`/`@saturating_*`/`@wrapping_*` — ✅ Ф.2 ЗАВЕРШЕНО (см. выше).
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
