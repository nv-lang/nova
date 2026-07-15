<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 206 — Арифметическая политика: пять исходов из одного overflow-примитива

**Статус:** 📋 СОГЛАСОВАН 2026-07-14 (наблюдение + дизайн подтверждены владельцем). **После:** [194](194-contract-execution-model.md).
**Приоритет:** P2 (эргономика numeric/crypto + устранение дублирования overflow-логики).

## Мотив (наблюдение владельца 2026-07-14)

`Duration.checked_*`/`saturating_*` (`std/src/time/duration.nv:371+`, приватные, `i64`) РУЧНУЮ
повторяют overflow-логику (`if b > 0 && a > i64_max() - b { None }`), дублируя codegen-путь
`nova_int_checked_add` (`__builtin_add_overflow`, `compiler-codegen/nova_rt/effects.h:1044`). Это
**второе окно** для overflow-детекта (нарушает 196 «одно окно») + хуже: ручной sub/mul тонок и
медленнее аппаратного флага переполнения.

## Ключ: один примитив → пять исходов

`__builtin_*_overflow` ПИШЕТ обёрнутый результат ВСЕГДА (даже при overflow — effects.h:1039), т.е.
один примитив даёт пару `(wrapped, overflowed)`. Из неё выводятся ВСЕ пять исходов — тонким `.nv`:

| Исход | Из `(wrapped, overflowed)` | Сейчас |
|---|---|---|
| **trap** (паника) | `overflowed → panic; else wrapped` | ✅ дефолт `+`/`-`/`*` (`nova_int_checked_add`) |
| **checked → `Option`** | `overflowed → None; else Some(wrapped)` | ⚠️ только приватно в time (вручную, i64) |
| **saturating → clamp** | `overflowed → (знак ? MAX : MIN); else wrapped` | ⚠️ только time |
| **wrapping** (модульно) | `wrapped` (флаг игнор) | ❌ нет |
| **unchecked** (unsafe) | `wrapped` без проверки | ❌ нет |

## Дизайн (196 «одно окно» для арифметики) — согласовано владельцем 2026-07-14

Граница «что в компиляторе / что на `.nv`» — по §3 maximize-nv-sourcing: в Rust только
**непортируемое** (нужен аппаратный флаг переполнения), всё остальное — `.nv`.

- **РОВНО ОДИН интринсик** (в компиляторе), **generic + с операндом**:
  `fn[T Ints] T @overflowing_add(rhs T) -> (T, bool)` (и `_sub`/`_mul`) — пара wrapped+flag,
  прямой лоуэринг на `__builtin_*_overflow`. Нельзя на `.nv` — нужен HW-флаг. Единственный источник
  overflow-детекта. **Per-type автоматически:** компилятор подставляет `__builtin_sadd`/`uaddl`/… по
  ширине и знаку `T`. (Правка после ревью: подпись именно generic `[T Ints] T` c параметром `rhs T`,
  НЕ `int @…()` без аргумента — операнд обязателен, и бланкеты зовут его на i8/u32/….)
- **Type-set `Ints`** нужно ЗАВЕСТИ (Ф.0): сейчас есть только `SignedInt` (`i8|i16|i32|i64|int`) и
  `UnsignedInt` (`u8|u16|u32|u64|uint`) раздельно (`std/src/prelude/protocols.nv:659`). Ввести
  `type Ints set i8|i16|i32|i64|int|u8|u16|u32|u64|uint` (объединение) → один бланкет на метод вместо
  двух. Формулы ниже работают и для signed, и для unsigned в ОДНОМ бланкете (см. §saturating).
- **ТРИ `.nv`-бланкет-обёртки** над примитивом (maximize-nv-sourcing), каждая = ОДИН бланкет
  `fn[T Ints]` (не копипаста; диспетч бланкетов над примитивами — D164-фикс):
  - `fn[T Ints] T @checked_add(rhs T) -> Option[T]` — `overflowed → None; else Some(wrapped)`.
  - `fn[T Ints] T @wrapping_add(rhs T) -> T` — берёт `wrapped`, флаг игнор (модульно).
  - `fn[T Ints] T @saturating_add(rhs T) -> T` — при overflow клампит к `T.MAX`/`T.MIN` по
    **op-специфичной** формуле направления (см. ниже). То же семейство для `_sub`/`_mul`.
- **`saturating`: направление клампа — ПО ОПЕРАЦИИ, не «по знаку rhs»** (правка после ревью — прежняя
  формулировка была верна только для add):
  - **add:** overflow → `rhs > 0 ? T.MAX : T.MIN`.
  - **sub:** overflow → `rhs < 0 ? T.MAX : T.MIN` (вычли отрицательное → вверх).
  - **mul:** overflow → `(a < 0) == (rhs < 0) ? T.MAX : T.MIN` (одинаковый знак → произведение вверх).
  Все три формулы **автоматически корректны и для unsigned**: у unsigned `x < 0` всегда false →
  ветки схлопываются в правильную сторону (unsigned add/mul → MAX, unsigned sub → MIN=0). Значит один
  бланкет `fn[T Ints]` на op покрывает и signed, и unsigned.
- **trap-дефолт (`+`) уже есть** — `nova_int_checked_add` (`__builtin_add_overflow` + паника),
  `effects.h:1044`. Не трогаем.
- **`unchecked_*` (unsafe) — ОТЛОЖЕН** (владелец 2026-07-14). Это сырой C-`a+b` без trap (UB на
  overflow ради оптимизации) → требует отдельного лоуэринга в компиляторе, НЕ `.nv`. Во многом
  дублирует `--contracts=optimized` (Z3-элизия сама снимает доказано-безопасные trap'ы, 194/D140.4).
  Держим в дизайне пятым исходом, реализуем ПОСЛЕДНИМ и только по реальному запросу crypto/numeric-ядра.
- **Duration МИГРИРУЕТ** на общий примитив: `checked_add_i64`/`saturating_*` → делегируют
  `int/i64 @checked_add`/`@saturating_add`; ручные i64-проверки диапазона удаляются (дедуп D317).
- **Философия 194:** wrapping (и позже unchecked) дают numeric/crypto-ядру явный ЛОКАЛЬНЫЙ opt-out
  («локальный отказ → unsafe», не глобальный footgun-флаг). trap-дефолт и Z3-элизия не меняются.

## Именование — прайор-арт (не самодеятельность)

`overflowing_add` и форма `(значение, флаг)` — дословно из эталона (Rust) + совпадает со всей отраслью:

| Язык | Имя | Возврат |
|---|---|---|
| **Rust** (эталон) | `overflowing_add` | `(T, bool)` |
| **Swift** | `addingReportingOverflow` | `(partialValue: T, overflow: Bool)` |
| **Zig** | builtin `@addWithOverflow` | `.{result, u1}` |
| **C** (наш лоуэринг) | `__builtin_add_overflow` / C23 `ckd_add` | `bool` + out-параметр |
| **LLVM IR** | `llvm.sadd.with.overflow.iN` | `{iN, i1}` |

Семейство `checked`/`saturating`/`wrapping`/`unchecked` — тоже Rust-имена. **Внутренний прецедент:**
атомики в `std/src/runtime/sync.nv` уже названы дословно по Rust (`compare_exchange`, `fetch_add`,
`MemOrdering`) — низкоуровневые интринсики в Nova = Rust-имена, `overflowing_*` консистентен с этим.

**Принцип (зафиксирован):** примитив НЕ теряет информацию — отдаёт пару/`Result`, обеднять только
осознанно. `overflowing_* -> (T, bool)` соблюдает: оба нужны для вывода всех исходов.

## Follow-up (вне рамок 206): `compare_exchange` обеднён

`AtomicI*.compare_exchange -> bool` (`sync.nv`) теряет свидетеля: C-примитив
`atomic_compare_exchange_strong(&obj, &expected, desired)` при провале ПИШЕТ фактическое значение в
`expected` (для пересчёта в CAS-цикле без повторного `load()`), а мы его выбрасываем. Rust отдаёт
`Result<T,T>` (`Err(actual)`). По принципу «примитив не теряет информацию» — пересмотреть на
`-> Result[unit, T]` (`Err(actual)`) либо `-> (bool, T)`. Ломающая правка API `sync.nv` → отдельным
пунктом ([M-cas-return-witnessed-value] в backlog), НЕ в 206.

## Фазы
- Ф.0 Спека + type-set: D-блок (примитив `@overflowing_*`, пять политик, миграция Duration,
  `unchecked`=unsafe-op, `div`/`neg` вне рамок). **Завести `type Ints set i8|…|u64|int|uint`**
  (объединение SignedInt+UnsignedInt) в `std/src/prelude/protocols.nv`.
- Ф.1 Codegen: generic-интринсик `fn[T Ints] T @overflowing_add/_sub/_mul(rhs T) -> (T, bool)` →
  `__builtin_*_overflow` (пара), per-type подстановка builtin по ширине/знаку `T`.
- Ф.2 std: ТРИ `.nv`-бланкета `@checked_*`/`@saturating_*`/`@wrapping_*` на `fn[T Ints]`
  (saturating — op-специфичная формула направления, см. Дизайн; `@unchecked_*` — ОТЛОЖЕН).
- Ф.3 Миграция Duration (снять ручное дублирование, делегировать общим примитивам):
  - `std/src/time/duration.nv:371+` приватные `checked_add_i64`/`checked_sub_i64` (ручные
    `if b>0 && a>i64_max()-b {None}`) → делегируют `i64 @checked_add`/`@checked_sub`.
    `saturating_*` → `i64 @saturating_*`. Ручные range-проверки диапазона i64 **удаляются**.
  - `f64_nanos_checked`/`f64_nanos_or_trap` (f64→i64 наносекунды): здесь overflow-детект по f64-границам,
    НЕ целочисленный `__builtin` — оставить как есть (это конверсия, не int-арифметика); отметить, что
    это НЕ дублирование overflow-примитива.
  - Публичные `Duration.checked_add`/`saturating_add`/… — поведение байт-паритет (D317-тесты δ0).
- Ф.4 Тесты: четыре реализуемых исхода на границах для ADD/SUB/MUL, signed И unsigned (MAX/MIN trap,
  wrap-round-trip, saturating-clamp по op-формуле в обе стороны, checked→None); unsafe — когда/если
  введём `unchecked`.

## Гейты
conformance + новые фикстуры пяти исходов; Duration-тесты (D317) байт-паритет поведения; std δ0.

## ★ Открытый вопрос (нужно решение владельца): trap-дефолт ТОЛЬКО у `int`

Найдено при ревью 2026-07-15: в `effects.h` есть лишь `nova_int_checked_add/sub/mul`, а emit_c
(`~27161`/`~28033`) хардкодит `nova_int_checked_*`. **Значит `+`/`-`/`*` трапят на переполнении только
для `int` (nova_int); у sized-типов (`i8`..`i64`, `u8`..`u64`) переполнение — сырой C-`+`: для signed
это UB, для unsigned — тихий wrap.** Это **дыра звучности** (философия «overflow = always-on safety»
нарушена для sized-типов), не просто эргономика.

Развилка:
- **(A) Закрыть в 206** — расширить trap-дефолт на все Ints: добавить `nova_<T>_checked_*` (или generic
  через `__builtin_*_overflow` per-type) и лоуэрить типизированный `+`/`-`/`*` в них. Устраняет UB.
  Стоимость: правка codegen-лоуэринга бинопов + возможные перф/паритет-эффекты (все sized-арифметики
  становятся checked). Согласуется с always-on-safety.
- **(B) Отдельным пунктом** — 206 добавляет только методы (`@checked_*` и т.д.), а закрытие
  sized-trap выносится в свой план/маркер.

Рекомендация: **(A)** — это соундность, и она в духе того же примитива `@overflowing_*` (тот уже
per-type). Если перф-паритет sized-арифметики станет проблемой — Z3-элизия (optimized) её снимет, как
для `int`. Но это заметное расширение объёма 206 — **решение за владельцем.**

## Границы
Не трогает Z3-элизию. `div`/`neg` — **вне рамок 206** (у `__builtin` нет div-overflow; `div` = спец-кейс
`INT_MIN/-1` + деление-на-ноль, отдельный путь; `neg(INT_MIN)` — тоже отдельно). 206 ДОБАВЛЯЕТ явные
политики (методы) + дедуплицирует overflow-детект в один примитив; вопрос trap-дефолта sized-типов —
см. открытый вопрос выше.
