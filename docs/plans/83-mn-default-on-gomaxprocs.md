// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 83: M:N по умолчанию + GOMAXPROCS-style конфигурация

> **Создан:** 2026-05-21.
> **Статус:** 📋 proposed, не начат. Ф.0 = readiness gate (decision point).
> **Приоритет:** P2 — эргономика/конкурентоспособность vs Go; не баг.
> **Родитель:** [Plan 44](44-mn-runtime-roadmap.md) (M:N runtime).
> **Жёсткая зависимость флипа дефолта:** [Plan 82](82-windows-fiber-arena.md)
> (Windows fiber arena) + GC-safety под multi-worker. До их закрытия
> Ф.1-Ф.4 (инфраструктура) делаются, **Ф.5 (флип) — нет**.

---

## 1. Проблема

M:N-рантайм Nova сейчас **opt-in**: чтобы получить параллелизм, надо
явно вызвать `runtime.init(n)`. Дефолт — single-threaded cooperative.
`std/runtime/runtime.nv` сам помечен «opt-in proof of concept».

Это **противоположно индустрии**:

| Рантайм | Параллелизм по умолчанию | Число worker'ов default | Override |
|---|---|---|---|
| **Go** | **вкл** (всегда) | `NumCPU` | env `GOMAXPROCS` + `runtime.GOMAXPROCS(n)` |
| **Rust — tokio** (multi-thread) | **вкл** | `NumCPU` | `worker_threads(n)` builder |
| **TS / Node** | выкл (одно-поточный event loop) | — | `worker_threads` явно |
| **Nova сейчас** | **выкл** (opt-in) | — | `runtime.init(n)` обязателен |

Go и tokio оба дефолтят на `NumCPU`. Nova — единственный, кто требует
явного включения. Следствие: **большинство Nova-программ никогда не
получают параллелизм** — «спавнишь fiber, он бежит параллельно без
церемоний» (killer-эргономика Go) у Nova не работает из коробки.

## 2. Цель

M:N включён **по умолчанию**, worker'ов = `NumCPU`, без обязательного
`runtime.init`. `runtime.init(n)` меняет роль: из «включателя M:N» в
**опциональный тюнер** числа worker'ов. Override-каналы — Go-паритет:
ENV + API. `NOVA_MAXPROCS=1` = один worker (детерминизм/отладка).

**НЕ цель:** concurrent GC (Plan 44 v1.0+), CLI-флаг (env достаточно —
Go его не имеет; опционально, вне scope).

## 3. Почему флип дефолта надо гейтить

Флип «выкл → вкл» раздаёт M:N **всем** программам сразу. До флипа
обязательно:
- **Plan 82** закрыт — иначе каждая Windows-программа побежит
  параллельно на слабых 56 КБ calloc-стеках без overflow-detection
  (silent heap corruption под реальной конкуренцией).
- **GC-safety под multi-worker** подтверждена — stop-the-world
  корректен при N worker'ах, либо concurrent GC. Сейчас в `fibers.h`
  безопасность calloc-пути на Windows опирается на «single-thread
  cooperative — GC не запускается между yield/resume»; под реальным
  M:N это допущение исчезает.
- **Soak/stress** валидация (10⁶ spawn, work-stealing migration, no
  race/leak/deadlock).

Поэтому план разделён: **Ф.1-Ф.4 — инфраструктура** (безопасны сейчас,
дефолт остаётся opt-in), **Ф.5 — собственно флип** (только после
зелёного gate Ф.0).

## 4. Фазы

### Ф.0 — Readiness gate (decision point, ~1 день)

Определить точные предусловия безопасного флипа дефолта + аудит
текущей зрелости M:N (что реально работает, известные race'ы,
состояние Windows, статус GC под multi-worker). Зафиксировать
go/no-go критерии для Ф.5. Без этого Ф.5 не стартует.

### Ф.1 — Worker-count resolution (~1-2 дня)

- Кросс-платформенное определение `NumCPU` (`sysconf(_SC_NPROCESSORS_ONLN)`
  POSIX / `GetSystemInfo` Windows; учесть cgroup-лимиты на Linux —
  как Go 1.25+ читает cgroup quota, чтобы в контейнере не брать
  «железные» CPU).
- Порядок разрешения числа worker'ов: **explicit API > ENV > default
  `NumCPU`**.
- Клэмп: min 1, max — разумный потолок.

### Ф.2 — ENV `NOVA_MAXPROCS` (~0.5 дня)

- Парсинг/валидация `NOVA_MAXPROCS` (аналог `GOMAXPROCS`). Имя в
  стиле существующих `NOVA_*` env (`NOVA_TARGET_OS`, `NOVA_FEATURES`,
  `NOVA_GC_LIB_DIR`, …).
- `NOVA_MAXPROCS=1` — явный single-worker режим.
- Невалидное значение → понятная диагностика, fallback на default.

### Ф.3 — API reshape: `runtime.init` → опциональный тюнер (~2 дня)

- Рантайм авто-конфигурируется на `NumCPU` без вызова `init`.
- `runtime.init(n)` остаётся как **опциональный** override-before-start
  (вызов до первого spawn). Добавить runtime-getter (аналог
  `runtime.GOMAXPROCS()` для чтения текущего значения).
- Чёткая семантика: вызов `init` после старта рантайма — ошибка либо
  no-op с диагностикой (решить в Ф.3).

### Ф.4 — Lazy worker-thread spawn (~2 дня)

«Дефолт-вкл» **не должен** означать «hello-world спавнит N OS-потоков».
Worker-потоки поднимаются **лениво** под реальную fiber-нагрузку
(как Go создаёт M по необходимости). Тривиальная программа без fiber'ов
не платит за пул потоков. Scheduler сконфигурирован на `NumCPU`, но
физические потоки — on-demand.

### Ф.5 — Флип дефолта (GATED — только после Plan 82 ✅ + GC-safety ✅)

Собственно изменение: дефолт = M:N вкл, worker'ов = `NumCPU`. Одно
обозримое ревью-able изменение. **Не выполнять**, пока не зелёный
gate Ф.0.

### Ф.6 — Миграция + escape hatch (~1-2 дня)

- Sweep существующих `runtime.init(n)` вызовов в `std/` / `nova_tests/`
  / `examples/` — теперь опциональны.
- Детерминизм escape hatch: `NOVA_MAXPROCS=1` даёт воспроизводимое
  single-worker исполнение (для отладки / тестов). Задокументировать.
- Fallout: тесты, которые молча полагались на single-threaded порядок.

### Ф.7 — spec + docs + benchmarks (~1 день)

- Spec: M:N — дефолтная модель исполнения; D-block по `NOVA_MAXPROCS` +
  resolution order.
- Бенчмарк: измеренный parallel speedup на N ядрах (нужен —
  `bench/micro/` сейчас не имеет параллельного workload'а).
- `docs/concurrency*`, README.

## 5. Acceptance

- Программа без единого вызова `runtime.*` использует все CPU при
  fiber-нагрузке (паритет Go/tokio).
- `NOVA_MAXPROCS` + `runtime.init(n)` корректно override'ят; порядок
  разрешения соблюдён; `NOVA_MAXPROCS=1` → детерминированный
  single-worker.
- Hello-world (без fiber'ов) не создаёт worker-пул (Ф.4 lazy spawn).
- Измеренный speedup на multi-core workload'е.
- 0 регрессий; gate Ф.0 зелёный перед Ф.5.

## 6. Честная рамка

- Инфраструктура (Ф.1-Ф.4) безопасна и полезна **сразу** — даже до
  флипа: `NOVA_MAXPROCS`, авто-`NumCPU` при opt-in, lazy spawn.
- **Флип (Ф.5) — рисковый и заблокирован** Plan 82 + GC-safety. Если
  gate Ф.0 покажет, что M:N не готов раздаваться всем — Ф.5
  откладывается, инфраструктура всё равно остаётся ценной.
- Дефолт-параллелизм означает, что пользовательский код становится
  race-aware (как в Go). Это сознательная языковая позиция, не
  упрощение — фиксируется в spec (Ф.7).

## 7. Связь

- [Plan 82](82-windows-fiber-arena.md) — Windows fiber arena; **жёсткий
  гейт** для Ф.5 (флип нельзя до закрытия 82).
- [Plan 44.5](44.5-work-stealing-scheduler.md) ✅ — work-stealing
  scheduler (инфраструктура worker'ов уже есть, `runtime.c`).
- [Plan 44.4](44.4-mn-runtime-stage0.md) ✅ — `runtime.init/shutdown`,
  `NovaWorker` — переосмысливается в Ф.3.
- [Plan 44](44-mn-runtime-roadmap.md) — M:N umbrella; concurrent GC
  (v1.0+) — смежный, не блокер инфраструктуры, но усиливает gate Ф.5.
- Ориентиры: Go (`GOMAXPROCS`, default `NumCPU`, lazy M), tokio
  (multi-thread runtime, default `NumCPU` worker_threads).
