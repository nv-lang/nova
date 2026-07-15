<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 206 — Арифметическая политика: пять исходов из одного overflow-примитива

**Статус:** 📋 СОГЛАСОВАН 2026-07-14 (наблюдение + дизайн подтверждены владельцем). **После:** [194](194-contract-execution-model.md). Ф.0/Ф.1/Ф.1b — см. [206-progress.md](206-progress.md) (статус актуален в [README.md](README.md), не здесь).
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

- **РОВНО ОДИН интринсик** (в компиляторе): `int @overflowing_add/_sub/_mul() -> (int, bool)`
  (пара wrapped+flag), прямой лоуэринг на `__builtin_*_overflow`. Нельзя на `.nv` — нужен HW-флаг.
  Единственный источник overflow-детекта. **Per-type автоматически:** компилятор подставляет
  `__builtin_sadd`/`uaddl`/… по ширине и знаку `T` — писать по интринсику на каждый тип НЕ надо.
- **ТРИ `.nv`-бланкет-обёртки** над примитивом (maximize-nv-sourcing), каждая = ОДИН бланкет на
  type-set `Ints` (не копипаста на i8/…/u64; работает благодаря диспетчу бланкетов над примитивами,
  D164-фикс): `fn[T Ints] T @checked_add() -> Option[T]` (None при overflow),
  `@saturating_add() -> T` (клампит по знаку rhs к `T.MAX`/`T.MIN`),
  `@wrapping_add() -> T` (модульно — берёт wrapped, игнор флага). То же для `-`/`*`.
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
- Ф.0 Спека: D-блок (примитив `@overflowing_*`, пять политик, миграция Duration, `unchecked`=unsafe-op).
- Ф.1 Codegen: интринсик `@overflowing_add/_sub/_mul` → `__builtin_*_overflow` (пара).
- Ф.2 std: ТРИ `.nv`-бланкета `@checked_*`/`@saturating_*`/`@wrapping_*` на `fn[T Ints]` (`@unchecked_*` — ОТЛОЖЕН, см. Дизайн).
- Ф.3 Миграция Duration: снять ручное дублирование, делегировать общим; тесты D317 δ0.
- Ф.4 Тесты: четыре реализуемых исхода на границах (MAX/MIN trap, wrap-round-trip, saturating-clamp, checked→None); unsafe — когда/если введём `unchecked`.

## Гейты
conformance + новые фикстуры пяти исходов; Duration-тесты (D317) байт-паритет поведения; std δ0.

## Границы
Не меняет trap-дефолт обычной арифметики и Z3-элизию. Только ДОБАВЛЯЕТ явные политики + дедуплицирует
overflow-детект в один примитив.
