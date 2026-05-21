// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 83: M:N по умолчанию + GOMAXPROCS-style конфигурация

> **Создан:** 2026-05-21. **Редакция 2** (2026-05-22): перепроверка с
> чистого листа против `runtime.c`, `runtime.nv`, vendored libuv 1.52,
> `fibers.h`. Уточнён scope (только compiled-бинари), добавлены реальные
> подпроблемы паритета с Go (блокирующие syscall'ы, oversubscription в
> `nova test`, lazy-spawn policy, auto-shutdown, честность про детерминизм
> и dynamic-resize).
> **Статус:** 📋 proposed, не начат. Ф.0 = readiness gate (decision point).
> **Приоритет:** P2 — эргономика/конкурентоспособность vs Go; не баг.
> **Родитель:** [Plan 44](44-mn-runtime-roadmap.md) (M:N runtime).
> **Жёсткая зависимость флипа дефолта (Ф.6):** [Plan 82](82-windows-fiber-arena.md)
> (Windows fiber arena + GC-интеграция) + GC-safety под multi-worker. До
> их закрытия Ф.1-Ф.5 (инфраструктура) делаются, **Ф.6 (флип) — нет**.

---

## 1. Проблема

M:N-рантайм Nova сейчас **opt-in**: чтобы получить параллелизм, надо
явно вызвать `runtime.init(n)`. Дефолт — single-threaded cooperative.
`std/runtime/runtime.nv` сам помечен «opt-in proof of concept… не
предназначен для production».

Это **противоположно индустрии:**

| Рантайм | Параллелизм по умолчанию | Worker'ов default | Override |
|---|---|---|---|
| **Go** | **вкл** (всегда) | `NumCPU` | env `GOMAXPROCS` + `runtime.GOMAXPROCS(n)` |
| **Rust — tokio** (multi-thread) | **вкл** | `NumCPU` | `worker_threads(n)` builder |
| **TS / Node** | выкл (одно-поточный event loop) | — | `worker_threads` явно |
| **Nova сейчас** | **выкл** (opt-in) | — | `runtime.init(n)` обязателен |

Go и tokio дефолтят на `NumCPU`. Nova — единственный, кто требует явного
включения. Следствие: **большинство скомпилированных Nova-программ
никогда не получают параллелизм** — killer-эргономика Go «спавнишь
fiber → он бежит параллельно без церемоний» из коробки не работает.

**Scope-уточнение (редакция 2).** «M:N по умолчанию» относится к
**скомпилированным бинарям** (`nova build`, и `nova test` через
C-backend). `nova run` (treewalk-интерпретатор) — однопоточный по
конструкции и таким остаётся; M:N-рантайм (`runtime.c`) линкуется только
в C-backend. Это не упрощение, а граница: интерпретатор — для скриптов
(аналог `python script.py`), компилятор — для прода. Зафиксировать в spec.

## 2. Что подтверждено перепроверкой (редакция 2)

Сверка с исходниками — что уже есть и на что опираться:

- **`uv_available_parallelism()` уже используется** (`runtime.c:541`:
  `n_workers <= 0 → uv_available_parallelism()`). И vendored **libuv 1.52
  уже cgroup-aware** — `ChangeLog`: «linux: fix uv_available_parallelism
  using cgroup». → Ф.1 **не должна** переизобретать определение `NumCPU`
  через `sysconf`/`GetSystemInfo` (это была бы регрессия — libuv уже
  учитывает и CPU-affinity-маску, и cgroup-лимит). Использовать
  `uv_available_parallelism()`. Единственный честный gap vs Go 1.25 —
  libuv читает cgroup **статически на момент вызова**, Go 1.25
  перечитывает квоту динамически; это followup (§4, П6).
- **Worker-пул сейчас eager** — `nova_runtime_init` (`runtime.c:578-623`)
  в цикле `uv_thread_create` поднимает **все** `n_workers` потоков сразу.
  «Default-on» поверх этого превратил бы hello-world в спавн `NumCPU`
  OS-потоков → нужна Ф.4 (lazy spawn) как **часть дизайна**, не
  опция.
- **Авто-shutdown'а нет.** `nova_runtime_shutdown` существует, но
  вызывается только явно (Nova-код). `atexit` в рантайме используется для
  timer-метрик и `nova_evloop_close` — **но не для worker-пула**. При
  авто-init нужен авто-shutdown (§Ф.4).
- **API сейчас** (`runtime.nv`): `init`, `shutdown`, `worker_count`,
  `is_initialized`, `current_worker_id`, `yield`. Геттера «целевой
  maxprocs» нет — `worker_count` возвращает `0` до init.
- **main — координатор, не worker** (`runtime.c`: `_current_worker_id =
  -1` на main; `supervised` на main → `drain_main_scope`). Эта модель
  (Plan 44.5) сохраняется: флип не делает main worker'ом.
- **sysmon-preemption** (Plan 44.7) активен при любом числе worker'ов —
  важно для §4 П5 (детерминизм).

## 3. Цель и явная рамка

**Цель.** M:N включён по умолчанию для compiled-бинарей; worker'ов =
`NumCPU`; без обязательного `runtime.init`. `runtime.init(n)` меняет
роль: из «включателя M:N» в **опциональный тюнер**. Override-каналы —
Go-паритет: ENV + API. `NOVA_MAXPROCS=1` = один worker.

**Входит:** worker-count resolution, `NOVA_MAXPROCS`, API-reshape,
lazy-spawn + auto-shutdown, интеграция с `nova test`/`bench` (П3),
сам флип (gated), миграция, spec.

**НЕ входит (явно):**
- **concurrent GC** — Plan 44 v1.0+; усиливает gate Ф.0, но не часть 83.
- **CLI-флаг** `--maxprocs` — env достаточно (у Go CLI-флага нет);
  опционально, вне scope.
- **dynamic resize** worker-пула во время работы (`runtime.GOMAXPROCS(n)`
  на лету) — см. §4 П6: осознанно tokio-паритет (fixed-at-start), не
  Go-паритет; dynamic — followup.
- **`Blocking`-effect offload** — **НЕ часть 83, но обязательный
  companion** для честного смысла `NOVA_MAXPROCS` (§4 П2). Без него
  worker может залипнуть в блокирующем syscall'е и `NOVA_MAXPROCS=N` не
  даст N CPU throughput. Трекается как зависимость, проверяется в gate Ф.0.

## 4. Подпроблемы — честные требования паритета

Чтобы `NOVA_MAXPROCS` действительно **значил** то же, что `GOMAXPROCS`,
а не просто «число потоков»:

- **П1. Worker = слитые Go-`P`+`M`.** В Go `GOMAXPROCS` — число `P`
  (логических процессоров, исполняющих Go-код одновременно); `M`
  (OS-потоков) может быть больше (заблокированные). Nova-«worker» = один
  OS-поток + свой libuv-loop + deque = `P` и `M` слиты. Значит worker
  **не должен залипать**, иначе теряется `P`.
- **П2. Блокирующая работа не должна пинить worker.** Вся I/O в Nova
  уже async через libuv (Plan 22 — sleep/timers/channels). Остаётся
  genuinely-blocking: FFI и эффект `Blocking` (D50). Production-правило:
  **M:N-worker никогда не выполняет блокирующую работу инлайн** —
  `Blocking`-эффект-операции уходят в **libuv threadpool**
  (`uv_queue_work`, `UV_THREADPOOL_SIZE`), worker'ы остаются свободны.
  Тогда `NOVA_MAXPROCS` = устойчивый параллелизм = семантика
  `GOMAXPROCS`. Это companion-работа (§3), но gate Ф.0 обязан
  зафиксировать её статус — иначе «default-on» обещает параллелизм,
  которого нет под блокирующей нагрузкой.
- **П3. Oversubscription в `nova test`/`bench`.** `nova test` запускает
  тест-файлы как параллельные subprocess'ы (`--jobs num_cpus`). Если
  каждый subprocess по умолчанию поднимет `NumCPU` worker'ов →
  `NumCPU × NumCPU` потоков, thrash. Lazy-spawn (П4) спасает тесты без
  fiber'ов (большинство), но concurrency-тесты — нет. Решение: test-
  runner выставляет subprocess'ам **бюджет** `NOVA_MAXPROCS` так, чтобы
  суммарно ≈ `NumCPU`, не `NumCPU²`. Аналогично `bench`: микро-бенчи
  по умолчанию `NOVA_MAXPROCS=1` (иначе шум), параллельные бенчи —
  явно. Без этого «default-on» регрессирует время прогона CI.
- **П4. Lazy-spawn policy (не «on-demand» абстрактно).** Hello-world без
  единого `spawn` создаёт **0** worker-потоков. Политика: рантайм на
  старте вычисляет `maxprocs` (резолюция П6), но потоки = 0. V1: на
  первом worker-bound `spawn` создаётся **весь** пул `maxprocs` (lazy
  относительно hello-world, eager относительно размера — tokio-модель,
  просто). V2/followup: инкрементальный рост (поток добавляется, когда
  есть runnable fiber и все существующие заняты, до `maxprocs`) — полный
  Go-паритет по числу `M`. План фиксирует V1 как acceptance, V2 как
  followup.
- **П5. Детерминизм — без переобещания.** `NOVA_MAXPROCS=1` убирает
  **параллелизм** (никакие два fiber'а не исполняются одновременно →
  нет гонок от истинной конкуренции) — этого достаточно для отладки и
  воспроизводимости в подавляющем большинстве случаев. Но `=1` **не**
  даёт детерминированного **расписания**: sysmon-preemption (Plan 44.7)
  активна и при одном worker'е, вытесняя CPU-bound fiber'ы в
  недетерминированных точках. Полный scheduling-детерминизм — отдельный
  режим (cooperative-only, sysmon off), вне scope 83. Редакция 1
  переобещала «детерминированный single-worker» — исправлено.
- **П6. Порядок разрешения числа worker'ов.** `explicit API
  (runtime.init) > ENV NOVA_MAXPROCS > default uv_available_parallelism()`.
  Клэмп `[1, 1024]` (1024 — щедрый потолок; выше — только явным env, с
  warning). cgroup: покрыт libuv статически (§2); динамический re-read
  квоты (Go 1.25) — followup.

## 5. Сравнение с Go / Rust / TS

| Аспект | Go | tokio (multi-thread) | TS / Node | **Nova сейчас** | **Nova — цель 83** |
|---|---|---|---|---|---|
| Параллелизм по умолчанию | вкл | вкл | выкл | **выкл** | **вкл** (compiled) |
| Worker'ов default | `NumCPU` | `NumCPU` | — | — | `NumCPU` (`uv_available_parallelism`) |
| cgroup-aware | да, динамически (1.25+) | нет (по умолч.) | — | — (n/p) | **да, статически** (libuv); dynamic — followup |
| Override env | `GOMAXPROCS` | — | — | — | `NOVA_MAXPROCS` |
| Override API | `runtime.GOMAXPROCS(n)` | `worker_threads(n)` builder | — | `runtime.init(n)` | `runtime.init(n)` (тюнер) + getter |
| Изменение во время работы | **да** | нет (fixed) | — | — | **нет** (tokio-паритет; §4 П6) |
| Цена для hello-world | ~0 (M лениво) | пул при build рантайма | 0 | 0 | **0** (lazy-spawn П4) |
| Блокирующий syscall в корутине | `P` отдаётся другому `M` | `spawn_blocking` → отд. пул | n/p | инлайн (пинит) | `Blocking` → libuv threadpool (companion) |
| `=1` детерминизм | нет (preemption) | нет | n/p (single) | n/p | нет parallelism; расписание — нет (§4 П5) |

**Где Nova после Plan 83 НЕ хуже / лучше:**
- **Паритет с Go/tokio** по дефолт-параллелизму и авто-`NumCPU`.
- **Лучше tokio** по cgroup-awareness (tokio по умолчанию её не делает —
  частый источник oversubscription tokio в контейнерах) и по нулевой
  цене hello-world (tokio создаёт пул при сборке рантайма).
- **Паритет с Go** по cgroup (статически; динамика Go 1.25 — followup,
  честно отмечено).
- **Честно позади Go** в одном: Go меняет `GOMAXPROCS` на лету; Nova
  фиксирует на старте (= tokio). Осознанный scope-выбор, не упущение
  (§3, §4 П6).
- **TS** дефолт-выкл — Nova-with-flip как Go/tokio, не как Node.

## 6. Почему флип дефолта (Ф.6) надо гейтить

Флип «выкл → вкл» раздаёт M:N **всем** compiled-программам сразу. До
флипа обязательно:
- **Plan 82 закрыт** — иначе каждая Windows-программа побежит параллельно
  на 56 КБ calloc-стеках без overflow-detection и **без GC-интеграции
  fiber-стеков** (Plan 82 §1.5) → silent corruption под реальной
  конкуренцией.
- **GC-safety под multi-worker подтверждена** — STW корректен при N
  worker'ах. Сейчас безопасность calloc-пути на Windows опирается на
  «single-thread cooperative — GC не запускается между yield/resume»
  (`fibers.h`); под реальным M:N это допущение исчезает.
- **`Blocking`-companion (§4 П2)** — статус зафиксирован: либо offload в
  libuv threadpool готов, либо честно задокументировано, что под
  блокирующей нагрузкой параллелизм проседает.
- **Soak/stress** — 10⁶ spawn, work-stealing migration, no
  race/leak/deadlock; П3-oversubscription решён.

Поэтому план разделён: **Ф.1-Ф.5 — инфраструктура** (безопасны сейчас,
дефолт остаётся opt-in), **Ф.6 — флип** (только после зелёного gate Ф.0).

## 7. Фазы

### Ф.0 — Readiness gate (decision point, ~1-2 дня)

Аудит зрелости M:N: что реально работает, известные race'ы, статус
Windows (Plan 82), GC под multi-worker, статус `Blocking`-companion
(§4 П2), план решения П3-oversubscription. Зафиксировать go/no-go
критерии Ф.6. Без зелёного gate Ф.6 не стартует.

### Ф.1 — Worker-count resolution (~1 день)

- Использовать **`uv_available_parallelism()`** (уже cgroup+affinity-aware,
  §2) — не переизобретать.
- Порядок: `explicit API > ENV > uv_available_parallelism()` (§4 П6).
- Клэмп `[1, 1024]`; выше — только явным env с warning.

### Ф.2 — ENV `NOVA_MAXPROCS` (~0.5 дня)

- Парсинг/валидация `NOVA_MAXPROCS` (аналог `GOMAXPROCS`; имя в стиле
  существующих `NOVA_*`). `=1` — явный single-worker.
- Невалидное значение → понятная диагностика, fallback на default.

### Ф.3 — API reshape: `runtime.init` → опциональный тюнер (~2 дня)

- Рантайм авто-конфигурируется на resolved `maxprocs` без вызова `init`.
- `runtime.init(n)` — **опциональный** override до первого spawn.
- Новый getter — `runtime.maxprocs()` (аналог `runtime.GOMAXPROCS(-1)`):
  читает целевое число worker'ов (в отличие от `worker_count()` —
  фактически поднятых, который при lazy-spawn до первого spawn'а = 0).
- Семантика `init` после старта рантайма — **ошибка с диагностикой**
  (не молчаливый no-op): редакция 2 решает в пользу явной ошибки —
  тихий no-op маскировал бы баг конфигурации.
- Обновить `runtime.nv` — убрать «opt-in proof of concept / не для
  production» из шапки.

### Ф.4 — Lazy worker-пул + auto-shutdown (~2-3 дня)

- **Lazy spawn (V1, §4 П4):** на старте 0 потоков; на первом worker-bound
  `spawn` поднимается весь пул `maxprocs`. Hello-world без `spawn` →
  0 worker-потоков, 0 цены.
- **Auto-shutdown:** codegen эмитит `nova_runtime_shutdown()` в эпилоге
  `main` (детерминированно), `atexit` — fallback для `exit()`-путей.
  Учесть: join worker'а, залипшего в блокирующем вызове (→ §4 П2);
  семантика `detach`-работы (D50) при shutdown — drain либо явно
  documented-kill (решить в Ф.4).
- V2/followup — инкрементальный рост пула (полный Go-`M`-паритет).

### Ф.5 — Интеграция с `nova test` / `bench` (~1-2 дня)

Закрыть П3-oversubscription **до** флипа:
- `nova test`: subprocess'ам выставляется бюджет `NOVA_MAXPROCS`, чтобы
  суммарное число worker-потоков ≈ `NumCPU`, не `NumCPU²`.
- `bench`: микро-бенчи по умолчанию `NOVA_MAXPROCS=1`; параллельные
  бенч-workload'ы — явным opt-in.
- Проверка: время полного `nova test` не регрессирует после флипа.

### Ф.6 — Флип дефолта (GATED — только после Plan 82 ✅ + gate Ф.0 ✅)

Одно обозримое ревью-able изменение: дефолт = M:N вкл, worker'ов =
resolved `maxprocs`. **Не выполнять** без зелёного gate.

### Ф.7 — Миграция + escape hatch (~1-2 дня)

- Sweep `runtime.init(n)` в `std/`/`nova_tests/`/`examples/` — теперь
  опциональны; убрать обязательные.
- Escape hatch: `NOVA_MAXPROCS=1` — без параллелизма (§4 П5 — честная
  формулировка, не «детерминизм»). Задокументировать.
- Fallout: тесты, молча полагавшиеся на single-threaded порядок →
  починить либо пометить `NOVA_MAXPROCS=1`.

### Ф.8 — spec + docs + benchmark (~1 день)

- Spec: M:N — дефолтная модель исполнения для compiled-бинарей
  (`nova run` — однопоточный); D-block по `NOVA_MAXPROCS` + порядок
  разрешения + `Blocking`-companion-правило.
- Benchmark: измеренный parallel speedup на N ядрах (`bench/micro/`
  сейчас не имеет параллельного workload'а — добавить).
- `docs/concurrency*`, README.

## 8. Acceptance

- Compiled-программа без единого `runtime.*` вызова использует все CPU
  при fiber-нагрузке (паритет Go/tokio).
- `NOVA_MAXPROCS` + `runtime.init(n)` корректно override'ят; порядок
  разрешения соблюдён; `NOVA_MAXPROCS=1` → один worker, без параллелизма.
- Hello-world (без `spawn`) создаёт **0** worker-потоков (Ф.4).
- `runtime.maxprocs()` возвращает целевое число; `init` после старта →
  диагностируемая ошибка.
- `nova test` после флипа **не регрессирует** по времени (П3 закрыт).
- Измеренный speedup на multi-core workload'е (Ф.8).
- `nova run` остаётся однопоточным (scope-граница соблюдена).
- 0 регрессий; gate Ф.0 зелёный перед Ф.6.

## 9. Честная рамка риска

- Инфраструктура (Ф.1-Ф.5) безопасна и полезна **сразу**, до флипа:
  `NOVA_MAXPROCS`, авто-`NumCPU` при opt-in, lazy spawn, getter.
- **Флип (Ф.6) рисковый и заблокирован** Plan 82 + GC-safety +
  `Blocking`-companion. Если gate Ф.0 покажет неготовность — Ф.6
  откладывается, инфраструктура остаётся ценной.
- **`Blocking`-companion** (§4 П2) — если не сделан к Ф.6, флип всё ещё
  возможен, но в spec/docs честно фиксируется: под genuinely-blocking
  нагрузкой (FFI) параллелизм проседает ниже `NOVA_MAXPROCS`. Это
  **документированное ограничение**, не скрытое упрощение.
- Дефолт-параллелизм делает пользовательский код race-aware (как в Go) —
  сознательная языковая позиция, фиксируется в spec (Ф.8).
- Dynamic resize и динамический cgroup-re-read (Go 1.25) — осознанные
  followup'ы, не упущения: документируются как известная дельта vs Go.

## 10. Связь

- [Plan 82](82-windows-fiber-arena.md) — Windows fiber arena + GC-инте­гра­ция;
  **жёсткий гейт** для Ф.6.
- [Plan 44.5](44.5-work-stealing-scheduler.md) ✅ — work-stealing
  scheduler; worker-инфраструктура (`runtime.c`) уже есть.
- [Plan 44.7](44.7-preemption.md) ✅ — sysmon-preemption; влияет на §4 П5
  (детерминизм при `=1`).
- [Plan 44.4](44.4-mn-runtime-stage0.md) ✅ — `runtime.init/shutdown`,
  `NovaWorker`; переосмысливается в Ф.3.
- [Plan 44](44-mn-runtime-roadmap.md) — M:N umbrella; concurrent GC
  (v1.0+) усиливает gate Ф.6.
- [Plan 57](57-perf-benchmark-infrastructure.md) ✅ — `bench`; Ф.5
  интеграция (микро-бенчи `NOVA_MAXPROCS=1`).
- D50 (`06-concurrency.md`) — эффекты `Blocking`/`Detach`; источник
  companion-требования §4 П2.
- Ориентиры: Go (`GOMAXPROCS`, default `NumCPU`, lazy `M`, dynamic
  cgroup 1.25+), tokio (multi-thread runtime, fixed `worker_threads`),
  Node (`worker_threads` — дефолт-выкл).
