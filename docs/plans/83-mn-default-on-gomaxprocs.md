// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 83: M:N по умолчанию + GOMAXPROCS-style конфигурация — roadmap-индекс

> **Создан:** 2026-05-21. **Редакция 2** (2026-05-22): production-grade
> переработка против `runtime.c`/`runtime.nv`/libuv 1.52.
> **Редакция 3** (2026-05-22): **декомпозиция** — план разбит на три
> под-плана по линии гейта Plan 82 (см. §2). Этот файл — roadmap-индекс.
> **Статус:** roadmap — 🟡 в работе. 83.1 ✅ ЗАКРЫТ + 83.3 ✅ ЗАКРЫТ
> (2026-05-22); остаётся **83.2** (флип дефолта) — GATED на Plan 82 ✅.
> Plan 83 закроется целиком, когда закроется 83.2.
> **Приоритет:** P2 — эргономика vs Go; не баг.
> **Родитель:** [Plan 44](44-mn-runtime-roadmap.md) (M:N runtime).

---

## 1. Проблема

M:N-рантайм Nova сейчас **opt-in**: чтобы получить параллелизм, надо
явно вызвать `runtime.init(n)`. Дефолт — single-threaded cooperative;
`std/runtime/runtime.nv` сам помечен «opt-in proof of concept… не для
production». Это **противоположно индустрии:**

| Рантайм | Параллелизм по умолчанию | Worker'ов default | Override |
|---|---|---|---|
| **Go** | **вкл** (всегда) | `NumCPU` | env `GOMAXPROCS` + `runtime.GOMAXPROCS(n)` |
| **Rust — tokio** (multi-thread) | **вкл** | `NumCPU` | `worker_threads(n)` builder |
| **TS / Node** | выкл (одно-поточный event loop) | — | `worker_threads` явно |
| **Nova сейчас** | **выкл** (opt-in) | — | `runtime.init(n)` обязателен |

Go и tokio дефолтят на `NumCPU`. Nova — единственный, кто требует явного
включения → **большинство скомпилированных Nova-программ никогда не
получают параллелизм**, killer-эргономика Go «спавнишь fiber → он бежит
параллельно» из коробки не работает.

**Цель.** M:N включён по умолчанию для compiled-бинарей; worker'ов =
`NumCPU`; без обязательного `runtime.init`. `runtime.init(n)` — из
«включателя» в **опциональный тюнер**. Override — Go-паритет: ENV + API.

**Scope-граница.** «M:N по умолчанию» — для **скомпилированных бинарей**
(`nova build`, `nova test` через C-backend). `nova run` (treewalk-
интерпретатор) однопоточный по конструкции и таким остаётся — M:N-рантайм
(`runtime.c`) линкуется только в C-backend. Это граница (интерпретатор
для скриптов, компилятор для прода), не упрощение; фиксируется в spec.

## 2. Декомпозиция (редакция 3)

План разбит на три под-плана. **Линия разреза — гейт Plan 82:** Ф.1-Ф.5
исходной редакции 2 не зависят от Plan 82 и mergeable независимо; флип —
жёстко заблокирован. Бундл в одном плане держал бы 83 в `open`, пока не
закроется 82. Разрез даёт независимо закрываемые части.

| # | Файл | Что | Зависимость | Статус |
|---|---|---|---|---|
| **83.1** | [83.1-mn-infrastructure.md](83.1-mn-infrastructure.md) | M:N-инфраструктура: worker-count resolution, `NOVA_MAXPROCS`, API-reshape (`runtime.init` → тюнер), lazy-spawn + auto-shutdown, thread-budget для `nova test`/`bench`. Дефолт **остаётся opt-in**. | нет (безопасно сейчас) | ✅ ЗАКРЫТ |
| **83.2** | [83.2-mn-default-flip.md](83.2-mn-default-flip.md) | Собственно флип дефолта (M:N вкл) + миграция + spec + speedup-бенчмарк. Включает readiness-gate. | **Plan 82 ✅ + 83.1 ✅ + GC-safety** | 📋 proposed (GATED) |
| **83.3** | [83.3-blocking-effect-threadpool.md](83.3-blocking-effect-threadpool.md) | `Blocking`-эффект (D50) → libuv threadpool. Companion: без него worker залипает в блокирующем syscall'е и `NOVA_MAXPROCS=N` не даёт N CPU throughput. | нет (ортогонален Plan 82) | ✅ ЗАКРЫТ (2026-05-22) |

**Порядок:** 83.1 и 83.3 — параллелизуемы, делаются сейчас. 83.2 — только
после Plan 82 ✅ + 83.1 ✅; honest-quality 83.2 требует и 83.3 ✅ (иначе
флип обещает параллелизм, проседающий под блокирующей нагрузкой —
допустимо с documented-ограничением, но 83.3 закрывает это честно).

## 3. Общие требования паритета (П1-П6)

Чтобы `NOVA_MAXPROCS` действительно **значил** то же, что `GOMAXPROCS`,
а не «просто число потоков». Под-планы ссылаются на эти пункты.

- **П1. Worker = слитые Go-`P`+`M`.** В Go `GOMAXPROCS` — число `P`
  (исполняют Go-код одновременно); `M` (OS-потоков) может быть больше.
  Nova-«worker» = OS-поток + libuv-loop + deque = `P` и `M` слиты →
  worker **не должен залипать**, иначе теряется `P`. → 83.3.
- **П2. Блокирующая работа не пинит worker.** Вся I/O уже async через
  libuv (Plan 22). Genuinely-blocking (FFI, эффект `Blocking` D50) →
  **libuv threadpool** (`uv_queue_work`), worker остаётся свободен. → 83.3.
- **П3. Oversubscription в `nova test`/`bench`.** `nova test` гоняет
  тест-файлы как параллельные subprocess'ы (`--jobs num_cpus`); каждый с
  `NumCPU` worker'ами → `NumCPU²` потоков. Нужен thread-budget. → 83.1 Ф.5.
- **П4. Lazy-spawn policy.** Hello-world без `spawn` → **0** worker-
  потоков. V1: пул `maxprocs` поднимается на первом worker-bound
  `spawn`. V2/followup: инкрементальный рост (полный Go-`M`-паритет). → 83.1 Ф.4.
- **П5. Детерминизм без переобещания.** `NOVA_MAXPROCS=1` убирает
  **параллелизм** (нет гонок от истинной конкуренции) — этого хватает
  для отладки. Но `=1` **не** даёт детерминированного **расписания**:
  sysmon-preemption (Plan 44.7) активна и при одном worker'е. Полный
  scheduling-детерминизм (cooperative-only) — вне scope 83. → 83.1.
- **П6. Порядок разрешения.** `explicit API > ENV NOVA_MAXPROCS >
  default uv_available_parallelism()`. Клэмп `[1, 1024]`. cgroup —
  покрыт libuv статически; динамический re-read квоты (Go 1.25) —
  followup. → 83.1 Ф.1.

## 4. Сравнение с Go / Rust / TS

| Аспект | Go | tokio (multi-thread) | TS / Node | **Nova сейчас** | **Nova — цель 83** |
|---|---|---|---|---|---|
| Параллелизм по умолчанию | вкл | вкл | выкл | **выкл** | **вкл** (compiled) |
| Worker'ов default | `NumCPU` | `NumCPU` | — | — | `NumCPU` (`uv_available_parallelism`) |
| cgroup-aware | да, динамически (1.25+) | нет (по умолч.) | — | — | **да, статически** (libuv); dynamic — followup |
| Override env / API | `GOMAXPROCS` / `runtime.GOMAXPROCS` | — / builder | — | — / `runtime.init` | `NOVA_MAXPROCS` / `runtime.init` тюнер + getter |
| Изменение во время работы | **да** | нет (fixed) | — | — | **нет** (tokio-паритет; followup) |
| Цена для hello-world | ~0 (`M` лениво) | пул при build рантайма | 0 | 0 | **0** (lazy-spawn, 83.1) |
| Блокирующий syscall в корутине | `P` отдаётся другому `M` | `spawn_blocking` → отд. пул | n/p | инлайн (пинит) | `Blocking` → libuv threadpool (83.3) |

**Где Nova после Plan 83 не хуже / лучше:** паритет с Go/tokio по
дефолт-параллелизму и авто-`NumCPU`; **лучше tokio** по cgroup-awareness
(частый источник tokio-oversubscription в контейнерах) и по нулевой цене
hello-world; **честно позади Go** в dynamic-resize (= tokio, осознанный
scope-выбор); TS — дефолт-выкл, Nova-with-flip как Go/tokio.

## 5. Почему флип (83.2) гейтится

Флип «выкл → вкл» раздаёт M:N **всем** compiled-программам. До него
обязательно: **Plan 82 ✅** (иначе Windows-программы бегут параллельно на
56 КБ calloc-стеках без overflow-detection и GC-интеграции →
corruption); **GC-safety под multi-worker** (STW корректен при N
worker'ах); **83.1 ✅** (инфраструктура); **soak/stress** (10⁶ spawn,
work-stealing migration, no race/leak/deadlock). Детальные go/no-go
критерии — readiness-gate в [83.2](83.2-mn-default-flip.md) Ф.0.

## 6. Честная рамка

- Инфраструктура (83.1) и `Blocking`-offload (83.3) безопасны и полезны
  **сразу**, до флипа — закрываются независимо от Plan 82.
- **Флип (83.2) рисковый и заблокирован.** Если gate покажет
  неготовность — 83.2 откладывается, 83.1/83.3 всё равно ценны.
- Дефолт-параллелизм делает пользовательский код race-aware (как в Go) —
  сознательная языковая позиция, фиксируется в spec.

## 7. Связь

- [Plan 82](82-windows-fiber-arena.md) — Windows fiber arena;
  **жёсткий гейт** для 83.2.
- [Plan 44.5](44.5-work-stealing-scheduler.md) ✅ — work-stealing;
  worker-инфраструктура (`runtime.c`) уже есть.
- [Plan 44.7](44.7-preemption.md) ✅ — sysmon-preemption (П5).
- [Plan 44.4](44.4-mn-runtime-stage0.md) ✅ — `runtime.init/shutdown`,
  `NovaWorker` — переосмысливаются в 83.1.
- [Plan 57](57-perf-benchmark-infrastructure.md) ✅ — `bench`; 83.1 Ф.5.
- D50 (`06-concurrency.md`) — эффекты `Blocking`/`Detach` — источник 83.3.
- Ориентиры: Go (`GOMAXPROCS`, lazy `M`, dynamic cgroup 1.25+), tokio
  (fixed `worker_threads`), Node (`worker_threads` — дефолт-выкл).
