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

## Дизайн (196 «одно окно» для арифметики)

- **Один интринсик-примитив:** `int @overflowing_add/_sub/_mul() -> (int, bool)` (пара wrapped+flag),
  прямой лоуэринг на `__builtin_*_overflow`. Единственный источник overflow-детекта.
- **Политики = `.nv`-обёртки** над примитивом (maximize-nv-sourcing): публичные общие
  `int @checked_add() -> Option[int]`, `@saturating_add()`, `@wrapping_add()`, `unsafe @unchecked_add()`.
  trap-дефолт (`+`) — остаётся как есть. То же для `-`/`*` (и `neg`/`div`, где применимо).
- **Duration МИГРИРУЕТ** на общий примитив: `checked_add_i64`/`saturating_*` → делегируют
  `int/i64 @checked_add`/`@saturating_add`; ручные i64-проверки диапазона удаляются (дедуп D317).
- **Философия 194:** wrapping/unchecked дают numeric/crypto-ядру явный ЛОКАЛЬНЫЙ opt-out (в духе
  «локальный отказ → unsafe», не глобальный footgun-флаг). trap-дефолт и Z3-элизия (D140.4) не меняются.

## Фазы
- Ф.0 Спека: D-блок (примитив `@overflowing_*`, пять политик, миграция Duration, `unchecked`=unsafe-op).
- Ф.1 Codegen: интринсик `@overflowing_add/_sub/_mul` → `__builtin_*_overflow` (пара).
- Ф.2 std: `.nv`-обёртки `@checked_*`/`@saturating_*`/`@wrapping_*`/`@unchecked_*` на `int` (+ i64/u* по нужде).
- Ф.3 Миграция Duration: снять ручное дублирование, делегировать общим; тесты D317 δ0.
- Ф.4 Тесты: пять исходов на границах (MAX/MIN, wrap-round-trip, saturating-clamp, checked→None, unsafe).

## Гейты
conformance + новые фикстуры пяти исходов; Duration-тесты (D317) байт-паритет поведения; std δ0.

## Границы
Не меняет trap-дефолт обычной арифметики и Z3-элизию. Только ДОБАВЛЯЕТ явные политики + дедуплицирует
overflow-детект в один примитив.
