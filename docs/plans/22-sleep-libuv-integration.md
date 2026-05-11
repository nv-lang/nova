// SPDX-License-Identifier: MIT OR Apache-2.0
# План 22: `Time.sleep` через libuv + унифицированный event loop

> **Статус:** активный, не начат.
> **Создан:** 2026-05-11.
> **Зависит от:** —
> **Открывает дорогу для:** Plan 18 (P0 stdlib — `std.net`, `std.fs`),
> Plan 09 (Clang migration — libuv цепляется к build chain).
> **Открывает D-блок:** D92 — top-level main как implicit supervised scope.

---

## Цель

Заменить busy-yield `Time.sleep` ([`fibers.h:414-440`](../../compiler-codegen/nova_rt/fibers.h#L414-L440))
на корректную event-loop-driven реализацию через **libuv `uv_timer_t`**,
с превращением scheduler'а в драйвер `uv_run`. Sleep становится:

1. **O(log N)** на N одновременно спящих fiber'ах (binary heap libuv'а),
   вместо O(N · poll_rate) busy-tick'а сегодня.
2. **CPU-idle** пока никто не готов: scheduler уходит в `uv_run` →
   `epoll_wait` / `GetQueuedCompletionStatus` спит в kernel'е ровно
   до ближайшего таймера.
3. **Унифицированный механизм** для будущего `std.net`/`std.fs`:
   sleep, network IO и fs IO живут в одном event loop'е, fiber'ы
   паркуются единообразно (`park → kernel-wait → callback wakes →
   resume`).

Параллельно — синхронизация спеки ([`syntax.md:1509-1521`](../../spec/syntax.md#L1509-L1521),
[`D71`](../../spec/decisions/06-concurrency.md)) с реализацией:
снять «`ms` игнорируется» формулировку (уже неправда), описать
event-loop-driven семантику и top-level mini-loop.

---

## Контекст

### Что сейчас (2026-05-11)

`Time.sleep(ms)` ([fibers.h:414-440](../../compiler-codegen/nova_rt/fibers.h#L414-L440)) —
три ветки по контексту:

```c
if (mco_running()) {
    while (_nova_monotonic_ms() < deadline) {
        nova_fiber_yield();    /* busy: возвращается в ready-queue каждый pass */
    }
} else if (_nova_active_scope) {
    while (_nova_monotonic_ms() < deadline) {
        nova_supervised_step(scope);   /* busy на main-flow */
    }
} else {
    _nova_native_sleep_ms(ms);         /* kernel Sleep — top-level */
}
```

Корректно функционально (ждёт N мс, handler-подмена работает,
`cancel_scope` прерывает sleep через cooperative-cancel в
`nova_fiber_yield`), но:

- **Жжёт CPU.** 1000 fiber'ов в `sleep(10000)` крутят scheduler 1000 раз
  за tick, ничего не делая.
- **Нет общего event loop'а.** Когда добавится `std.net`, sleep и
  socket IO должны жить в одной очереди — иначе sleep блокирует socket
  callback'и и наоборот.
- **Top-level kernel-blocking.** `fn main() { Time.sleep(1000); }`
  блокирует thread, libuv не успеет обработать pending tasks
  (после Plan 18 это станет проблемой).

### Что нужно

Архитектурно: **scheduler идёт через `uv_run`**, fiber'ы паркуются
не в "wait until clock" loop, а в libuv-handle (`uv_timer_t` для
sleep, в будущем `uv_poll_t` для socket'ов, etc.). Когда handle
готов — libuv вызывает callback на main-thread, callback вставляет
fiber обратно в ready-queue, scheduler делает `mco_resume`.

Это **существенная архитектурная перепланировка scheduler'а** — не
локальный fix `Time.sleep`. План разбит на фазы так чтобы каждая
фаза давала рабочий runtime; финальная фаза снимает busy-yield.

---

## Park/wake API — нормативный primitive

Plan 22 вводит **общий primitive для блокирующих операций** в
runtime'е, на который сядут все последующие планы (Channel, Socket,
File IO). API стабилизируется в Ф.3 и описывается в spec'е (D71
эволюция, Ф.6).

### API

Новый файл `compiler-codegen/nova_rt/sched.h` экспортирует:

```c
/* Park current fiber: remove from ready-queue.
 * Returns to caller когда nova_sched_wake() будет вызван
 * для (scope, slot) пары этого fiber'а. */
void nova_sched_park(NovaFiberQueue* scope, int slot);

/* Wake parked fiber: возвращает в ready-queue scope'а.
 * Idempotent. Безопасно вызывать из libuv-callback'а. */
void nova_sched_wake(NovaFiberQueue* scope, int slot);

/* True если fiber в slot сейчас parked. */
nova_bool nova_sched_is_parked(NovaFiberQueue* scope, int slot);

/* CancelToken integration: pending-handle регистрация.
 * Любая блокирующая операция, использующая park/wake, регистрирует
 * свой libuv handle (или waitlist node) через эту функцию.
 * При cancel() — token итерируется и вызывает stop_cb для каждого. */
typedef void (*NovaCancelStopCb)(void* handle);
void nova_sched_register_pending(NovaFiberQueue* scope, int slot,
                                  void* handle, NovaCancelStopCb stop_cb);
void nova_sched_unregister_pending(NovaFiberQueue* scope, int slot);
```

### Семантика

1. **Park атомарен с yield'ом.** `nova_sched_park` ставит `parked[slot] = 1`
   и сразу делает `mco_yield`. Race-window между "пометили parked" и
   "yield'нулись" — нулевой, потому что обе операции в single-threaded
   bootstrap'е.

2. **Wake идемпотентен.** Вызов `nova_sched_wake` на не-parked fiber'е
   — no-op. Это упрощает callback'и (libuv может вызвать wake несколько
   раз через `uv_close` cleanup, нам надо быть устойчивыми).

3. **Wake безопасен из libuv-callback'а.** Callback'и выполняются на
   main-thread (single-threaded `uv_loop`), в режиме когда никакой
   fiber не resume'ен. Поэтому ставить `parked[slot] = 0` безопасно.

4. **Scheduler идёт в `uv_run` когда все живые fiber'ы parked.**
   `nova_supervised_step` пропускает parked-fiber'ов; если
   `alive_unparked == 0 && alive_total > 0` — main-loop вызывает
   `uv_run(loop, UV_RUN_ONCE)`. Это блокирует main-thread в kernel-wait'е
   до первого libuv-события.

5. **Cancel-during-park.** Любая операция, паркующая fiber, **обязана**
   зарегистрировать свой handle через `nova_sched_register_pending`.
   Token-cancel пройдётся по pending-list scope'а и вызовет stop_cb
   для каждого. stop_cb должен закрыть handle И вызвать
   `nova_sched_wake` — после wake parked-fiber возобновится, увидит
   `scope->cancel_requested` и бросит `"scope cancelled"`.

### Контракт пользователя API

Любая операция, использующая park/wake, **обязана**:

```c
NovaXxxState st = { ... };
nova_xxx_init_handle(&st.handle);

/* Регистрация для cancel-wake (обязательно!) */
nova_sched_register_pending(_nova_active_scope, _nova_active_slot,
                             &st.handle, nova_xxx_stop_cb);

/* Park */
nova_sched_park(_nova_active_scope, _nova_active_slot);
/* ← здесь fiber suspend'ится до wake */

/* Dereg + cancel-check */
nova_sched_unregister_pending(_nova_active_scope, _nova_active_slot);
if (_nova_active_scope && _nova_active_scope->cancel_requested) {
    nova_throw(nova_str_from_cstr("scope cancelled"));
}
```

Этот паттерн воспроизводится для:
- **Plan 22** Ф.3: `Time.sleep` → `uv_timer_t` + `uv_timer_stop` stop_cb.
- **Plan 21** Ф.1+: `Channel.recv`/`send` → waitlist node + waitlist-remove stop_cb.
- **Plan 23+** (`std.net`): `TcpStream.read` → `uv_read_start` + `uv_read_stop` stop_cb.
- **Plan 23+** (`std.fs`): `File.read` → `uv_fs_t` + `uv_cancel` stop_cb.

### Что НЕ в API

- **Multi-fiber wake** (broadcast) — пока не нужен. Если потребуется
  (например `Channel.close()` будит всех recv-waiter'ов) — отдельная
  фаза с `nova_sched_wake_all(scope, slot_list)`.
- **Wake с payload** — fiber возвращается в свой стек, payload читает
  из своего state struct, не из wake-API.
- **Cross-scope wake** — fiber всегда parked в своём scope, wake
  идёт в тот же scope. Cross-scope сценарии (например `detach`-fiber
  будит main-scope) — отдельная задача после M:N.

---

## Фазы

### Ф.1 — Подключить libuv к build chain ✅ задача-нулевая

**Что:** добавить libuv как vendored dependency, без использования.
Цель — отделить build-system изменения от семантических.

**Файлы:**
- `compiler-codegen/nova_rt/libuv/` — vendored libuv source (single-header
  + dist amalgamation либо `git submodule`). Решение vendor vs submodule
  — в начале фазы (см. Q1 ниже).
- `compiler-codegen/src/codegen/build_invoker.rs` — `cl.exe` linking
  получает `libuv.lib` (Windows) или `-luv` (Linux). На каждой
  платформе: путь к prebuilt либо рецепт сборки из source.
- `compiler-codegen/nova_rt/nova_rt.h` — `#include <uv.h>` под `#ifdef
  NOVA_USE_LIBUV` (фаза-1 не активирует).
- `run_tests.ps1` — обновить link-line для тестов.

**Тесты:** один smoke-test `nova_tests/runtime/libuv_link.nv` —
просто `fn main() { println("ok") }`, проверка что бинарь линкуется
с libuv'ом (если `NOVA_USE_LIBUV` включён — `uv_version_string()`
печатается рядом).

**Платформы:**
- **Windows + MSVC** (текущий): libuv статически линкуется. Prebuilt
  `libuv.lib` либо собирается одним вызовом cl.exe из амальгамации.
- **Linux + clang/gcc** (будущее Plan 09): `-luv` либо vendored build.
  Решение оставить hint в `build_invoker.rs`, в Ф.1 не реализуем.

**Объём:** ~50 строк build-config + vendored source (десятки тысяч
строк libuv — не наш код, не считаем).

**Acceptance:** `run_tests.ps1` зелёный с `NOVA_USE_LIBUV` выключенным
(не должна сломаться существующая сборка), и с включённым — link
успешен, smoke-test проходит.

---

### Ф.2 — Глобальный event loop в runtime

**Что:** один `uv_loop_t` на процесс, инициализируется при старте
программы, закрывается при exit. Пока никто не использует — только
инфраструктура.

**Файлы:**
- `compiler-codegen/nova_rt/eventloop.h` (новый): тонкая обёртка
  над `uv_default_loop()`:
  ```c
  static inline uv_loop_t* nova_evloop(void);   /* lazy-init */
  static inline void       nova_evloop_close(void);  /* atexit */
  ```
- `compiler-codegen/nova_rt/nova_rt.h` — `#include "eventloop.h"`.
- `compiler-codegen/src/codegen/emit_c.rs` — `emit_main_prelude`
  добавляет `nova_evloop()` first-touch + `atexit(nova_evloop_close)`.

**Решения:**
- **`uv_default_loop()` vs custom `uv_loop_init`** — взять
  `uv_default_loop()`. Один loop на процесс, потоков нет (Nova
  single-threaded bootstrap).
- **Когда закрывать.** На `atexit` через `uv_loop_close`. После
  Plan 18 — на graceful shutdown сначала drain выполняемых handles.

**Тесты:** `nova_tests/runtime/evloop_lifecycle.nv` — init, проверить
что `uv_loop_alive(nova_evloop()) == 0` сразу после init (нет
handles), exit без assertion.

**Acceptance:** event loop живёт всю программу, никаких leak'ов на
exit, существующие 91/91 проходят (loop не используется пока).

**Объём:** ~80 строк C.

---

### Ф.3 — `Time.sleep` через `uv_timer_t` (внутри fiber)

**Что:** заменить busy-yield в **fiber-ветке** на park-on-timer.
Main-ветка и top-level — пока без изменений (Ф.4, Ф.5).

В этой же фазе вводится **общий park/wake primitive** —
`nova_sched_park`/`nova_sched_wake` API, на который позже сядут
Plan 21 (Channel) и Plan 23+ (socket IO). См.
[«Park/wake API»](#parkwake-api---нормативный-primitive) ниже.

**Структура реализации — два слоя:**

1. **`nova_rt/sched.h`** — новый файл, нормативный park/wake API
   (см. [«Park/wake API»](#parkwake-api---нормативный-primitive) выше).
   Это слой, на который сядут Plan 21 и Plan 23+.
2. **`nova_rt/fibers.h`** — `_nova_time_default_sleep` fiber-ветка
   переписывается через `sched.h` API.

**Реализация `nova_rt/sched.h`:**

```c
/* Поля в NovaFiberQueue (добавляются в fibers.h): */
typedef struct {
    /* ...existing fields... */
    nova_bool         parked[NOVA_SCOPE_CAP];
    /* Pending handles для cancel-wake (R4): на каждый slot — один
     * активный handle + stop_cb. Multi-handle per slot пока не нужен. */
    void*             pending_handle[NOVA_SCOPE_CAP];
    NovaCancelStopCb  pending_stop_cb[NOVA_SCOPE_CAP];
} NovaFiberQueue;

/* Реализация API: */
static inline void nova_sched_park(NovaFiberQueue* q, int slot) {
    q->parked[slot] = true;
    mco_yield(mco_running());
    /* control возвращается сюда когда callback сделал wake */
}

static inline void nova_sched_wake(NovaFiberQueue* q, int slot) {
    if (!q || slot < 0 || slot >= q->count) return;
    q->parked[slot] = false;
}

static inline nova_bool nova_sched_is_parked(NovaFiberQueue* q, int slot) {
    return q && slot >= 0 && slot < q->count && q->parked[slot];
}

static inline void nova_sched_register_pending(NovaFiberQueue* q, int slot,
                                                void* handle,
                                                NovaCancelStopCb stop_cb) {
    if (!q || slot < 0 || slot >= NOVA_SCOPE_CAP) return;
    q->pending_handle[slot]  = handle;
    q->pending_stop_cb[slot] = stop_cb;
}

static inline void nova_sched_unregister_pending(NovaFiberQueue* q, int slot) {
    if (!q || slot < 0 || slot >= NOVA_SCOPE_CAP) return;
    q->pending_handle[slot]  = NULL;
    q->pending_stop_cb[slot] = NULL;
}
```

**Scheduler-loop изменение** (в `fibers.h`):

`nova_supervised_step` пропускает `parked[i]`. Когда все живые
fiber'ы parked — main-loop делает `uv_run(loop, UV_RUN_ONCE)`:

```c
static inline void nova_supervised_run(NovaFiberQueue* q) {
    for (;;) {
        int alive_unparked = nova_supervised_step(q);  /* только не-parked */
        int alive_total    = nova_count_alive(q);
        if (alive_total == 0) break;
        if (alive_unparked == 0) {
            /* Все живые fiber'ы parked — ждём libuv-события. */
            uv_run(nova_evloop(), UV_RUN_ONCE);
            /* Callback'и из uv_run пометили какие-то fiber'ы как unparked. */
        }
    }
    /* ...existing error-propagation код... */
}
```

**Использование API для sleep'а:**

```c
typedef struct {
    NovaFiberQueue* scope;
    int             slot;
    uv_timer_t      timer;
} NovaSleepState;

static void _nova_sleep_timer_cb(uv_timer_t* h) {
    NovaSleepState* st = (NovaSleepState*)h->data;
    nova_sched_wake(st->scope, st->slot);
    /* uv_close в Ф.3 не нужен — handle живёт на fiber stack'е,
     * Ф.3 закроет в caller-функции после wake. */
}

/* stop_cb для cancel-wake (R4) — вызывается из cancel-token. */
static void _nova_sleep_stop_cb(void* handle) {
    uv_timer_t* timer = (uv_timer_t*)handle;
    if (!uv_is_closing((uv_handle_t*)timer)) {
        uv_timer_stop(timer);
        uv_close((uv_handle_t*)timer, NULL);
    }
    /* wake сделает cancel_token_cancel после stop_cb. */
}

static inline void _nova_sleep_via_libuv(nova_int ms) {
    NovaFiberQueue* scope = _nova_active_scope;
    int             slot  = _nova_active_slot;
    NovaSleepState  st    = { .scope = scope, .slot = slot };

    uv_timer_init(nova_evloop(), &st.timer);
    st.timer.data = &st;
    uv_timer_start(&st.timer, _nova_sleep_timer_cb, (uint64_t)ms, 0);

    /* Register для cancel-wake (R4). */
    nova_sched_register_pending(scope, slot, &st.timer, _nova_sleep_stop_cb);

    /* Park — fiber suspend'ится до wake. */
    nova_sched_park(scope, slot);
    /* ← control возвращается после wake (либо timer-callback, либо cancel). */

    /* Dereg + cleanup. */
    nova_sched_unregister_pending(scope, slot);
    if (!uv_is_closing((uv_handle_t*)&st.timer)) {
        uv_close((uv_handle_t*)&st.timer, NULL);
    }

    /* Cancel-check на exit. */
    if (scope && scope->cancel_requested) {
        nova_throw(nova_str_from_cstr("scope cancelled"));
    }
}
```

**Файлы:**
- `compiler-codegen/nova_rt/sched.h` (новый): park/wake API,
  pending-handle registration, реализация поверх `NovaFiberQueue`.
- `compiler-codegen/nova_rt/fibers.h`:
  + добавить `parked[]` + `pending_handle[]` + `pending_stop_cb[]`
    в `NovaFiberQueue`;
  + `#include "sched.h"` (API определён там);
  + `nova_count_alive`;
  + изменение `nova_supervised_step` (skip `parked[i]`);
  + изменение `nova_supervised_run` (drain через `uv_run UV_RUN_ONCE`);
  + переписать `_nova_time_default_sleep` fiber-ветка через
    `sched.h` API.
- `compiler-codegen/nova_rt/nova_rt.h` — `#include "sched.h"`.

**Cancel-during-sleep wake (R4):**

`nova_cancel_token_cancel` итерируется по slot'ам scope'а и вызывает
зарегистрированный `pending_stop_cb` для каждого. stop_cb — операция-
специфичный (для sleep'а это `_nova_sleep_stop_cb` выше; для recv'а
будет `_nova_channel_recv_stop_cb` в Plan 21; для socket'а —
`_nova_tcp_read_stop_cb` в Plan 23+).

```c
static inline void nova_cancel_token_cancel(NovaCancelToken* t) {
    if (!t || !t->scope) return;
    if (t->scope->cancel_requested) return;   /* idempotent */
    t->scope->cancel_requested = true;

    /* R4: пробуждаем все pending blocking-ops через registered stop_cb. */
    NovaFiberQueue* q = t->scope;
    for (int i = 0; i < q->count; i++) {
        if (q->pending_stop_cb[i] && q->pending_handle[i]) {
            q->pending_stop_cb[i](q->pending_handle[i]);
            nova_sched_wake(q, i);
        }
    }
    /* + linked tokens (existing) */
    for (int i = 0; i < t->linked_count; i++) {
        if (t->linked[i]) nova_cancel_token_cancel(t->linked[i]);
    }
}
```

После wake разбуженный fiber возвращается из `nova_sched_park`,
делает `nova_sched_unregister_pending`, проверяет
`scope->cancel_requested`, бросает `"scope cancelled"`. Это
**универсальный паттерн** — sleep, recv, socket, file все идут через
него одинаково.

**Тесты:** `nova_tests/concurrency/sleep_real_clock.nv` (новый):
```nova
test "sleep waits at least ms" {
    let t0 = Time.now()
    supervised {
        spawn { Time.sleep(100) }
    }
    let elapsed = Time.now() - t0
    assert(elapsed >= 100)
    assert(elapsed < 200)        // slack 100ms для CI
}

test "many fibers sleeping concurrently" {
    let t0 = Time.now()
    supervised {
        for _ in 0..100 { spawn { Time.sleep(100) } }
    }
    let elapsed = Time.now() - t0
    // Все 100 спят параллельно, не последовательно
    assert(elapsed >= 100)
    assert(elapsed < 300)
}

test "cancel during long sleep wakes immediately" {
    let t0 = Time.now()
    cancel_scope { tok =>
        spawn { Time.sleep(10000); panic("should not reach") }
        spawn { Time.sleep(50); tok.cancel() }
    }
    let elapsed = Time.now() - t0
    assert(elapsed < 200)        // отменено быстро, не ждём 10s — R4
}
```

**Acceptance:**
- Все 3 теста PASS.
- Существующие `nova_tests/concurrency/*` остаются 91/91 PASS.
- На bench-задаче "10k fiber'ов в `sleep(1000)`" CPU < 5% во время
  sleep (сейчас — 100% busy).
- Cancel-during-sleep: wake-up в течение 50ms (не ждём весь sleep).

**Объём:** ~180 строк C (включая R4 cancel-wake) + 3 теста.

**Риски:**
- `mco_yield` с парком — нужна гарантия что scheduler не возобновит
  parked fiber. Реализация через `parked[]` это даёт, но требует
  аккуратности в `_nova_active_slot` (slot не сдвигается).
- libuv handle (`uv_timer_t`) живёт на stack'е fiber'а — пока fiber
  parked, его стек жив (minicoro держит). После wake — handle closed,
  stack может разворачиваться.
- R4 pending-timer-list: cap `NOVA_SCOPE_CAP` (1024) хватает для bootstrap;
  при росте — heap-allocate list per-scope.

---

### Ф.4 — Main-flow sleep внутри `supervised`

**Что:** main-ветка `_nova_time_default_sleep` (вызов `sleep` напрямую
из main-кода внутри `supervised { ... }` тела). Сейчас:

```c
while (_nova_monotonic_ms() < deadline) {
    nova_supervised_step(scope);   /* drain fiber'ов раз за раз, пока время не пройдёт */
}
```

После Ф.3 fiber'ы внутри scope уже паркуются через libuv. Но main-flow
сам не fiber — он *вызывает* `supervised_step` в цикле. Нужно:

```c
} else if (_nova_active_scope) {
    /* Main-flow внутри supervised: drain пока время не пройдёт,
     * idle-периоды используем для uv_run (другие fiber'ы могут park'ed). */
    int64_t deadline = _nova_monotonic_ms() + ms;
    while (_nova_monotonic_ms() < deadline) {
        int alive_unparked = nova_supervised_step(_nova_active_scope);
        if (alive_unparked == 0) {
            /* Все fiber'ы парк или пусто — спим в uv_run до timer'а либо
             * до deadline. UV_RUN_ONCE с явным timeout — через
             * uv_timer_t self-poke на оставшееся время. */
            int64_t remaining = deadline - _nova_monotonic_ms();
            if (remaining <= 0) break;
            _nova_main_sleep_step(remaining);   /* UV_RUN_ONCE bounded by remaining */
        }
    }
}
```

`_nova_main_sleep_step(remaining)` устанавливает свой timer на
`remaining` мс с no-op callback'ом, делает `uv_run(loop, UV_RUN_ONCE)`,
вернётся когда либо наш timer выстрелил, либо чужой fiber-callback
сработал.

**Файлы:**
- `compiler-codegen/nova_rt/fibers.h` — обновить main-ветку
  `_nova_time_default_sleep`, добавить `_nova_main_sleep_step`.

**Тесты:** добавить в `sleep_real_clock.nv`:
```nova
test "main-flow sleep yields to fibers" {
    let mut x = 0
    let t0 = Time.now()
    supervised {
        spawn { Time.sleep(50); x = 42 }
        Time.sleep(100)          // main-flow: даёт fiber'у отработать
    }
    assert(x == 42)
    assert(Time.now() - t0 >= 100)
    assert(Time.now() - t0 < 200)
}
```

**Acceptance:** тест PASS, regression-free на 91/91.

**Объём:** ~50 строк C + 1 тест.

---

### Ф.5 — Top-level sleep (вне fiber, вне scope)

**Что:** превратить top-level main в **скрытый mini-event-loop**.
Сейчас вне scope `_nova_native_sleep_ms` блокирует thread; после Ф.5 —
тот же `uv_run`-based mechanism что и main-внутри-scope, но без
fiber'ов.

```c
} else {
    /* Top-level вне любого supervised: mini-loop через uv_run. */
    int64_t deadline = _nova_monotonic_ms() + ms;
    while (_nova_monotonic_ms() < deadline) {
        int64_t remaining = deadline - _nova_monotonic_ms();
        _nova_main_sleep_step(remaining);
    }
}
```

**Семантика (R2 → D92):** обернуть весь `main()` в implicit
supervised-scope. Иначе detach'ы / global timer callback'и запускаются
в no-scope context. Это вводит новый D-блок **D92 — top-level main
как implicit supervised scope** в `spec/decisions/06-concurrency.md`
(пишется в Ф.6 одним коммитом с реализацией Ф.5).

Codegen `emit_main` после prelude добавляет:
```c
int main(int argc, char** argv) {
    /* ...init... */
    NovaFiberQueue _main_scope; nova_scope_init(&_main_scope);
    _nova_active_scope = &_main_scope;
    /* ...user main body... */
    nova_supervised_run(&_main_scope);   /* drain до полного quiescence */
    nova_evloop_close();
    return _exit_code;
}
```

Это **семантическое изменение D71** (top-level main теперь имеет
implicit scope). Для пользователя — invisible: всё что раньше
работало, продолжает работать. Но spec должен это зафиксировать.

**Файлы:**
- `compiler-codegen/nova_rt/fibers.h` — top-level ветка.
- `compiler-codegen/src/codegen/emit_c.rs` — `emit_main` обёртка.
- `compiler-codegen/nova_rt/fibers.h` — `_nova_active_scope`
  инициализирован main-scope'ом, не NULL.

**Тесты:** добавить в `sleep_real_clock.nv`:
```nova
test "top-level sleep wall-clock" {
    let t0 = Time.now()
    Time.sleep(100)
    let elapsed = Time.now() - t0
    assert(elapsed >= 100)
    assert(elapsed < 200)
}
```

**Acceptance:** тест PASS, 91/91 regression-free.

**Объём:** ~30 строк C + 1 тест + codegen-правка.

**Риски:**
- Implicit-scope меняет поведение `detach` на top-level (раньше
  `SyncDetach` inline-исполнял; теперь detach попадает в main-scope
  fiber-queue). Нужна проверка `nova_tests/concurrency/detach_test.nv`.

---

### Ф.6 — Cleanup busy-yield path + spec sync

**Что:** удалить старый busy-yield код, обновить спеку.

**Файлы:**
- `compiler-codegen/nova_rt/fibers.h:414-440` — удалить `while
  (_nova_monotonic_ms() < deadline)` петли. Должны остаться только
  park-on-timer / uv_run-driven варианты.
- `compiler-codegen/nova_rt/fibers.h:386-407` — `_nova_native_sleep_ms`
  становится unused, удалить (или оставить как `__attribute__((unused))`
  fallback для future bare-metal target'ов).
- `spec/syntax.md:1509-1521` — раздел `Time.sleep(ms)`:
  + удалить «В bootstrap'е `ms` игнорируется (timer-wheel'а нет).
    Любое `Time.sleep(N)` = один cooperative yield.»;
  + добавить «`Time.sleep(ms)` блокирует текущий fiber на не менее
    чем `ms` миллисекунд. Под капотом — libuv-таймер; fiber паркуется
    до срабатывания timer'а, scheduler в это время крутит других
    fiber'ов либо спит в kernel-wait'е. Top-level main обёрнут в
    implicit supervised-scope, поэтому семантика sleep'а одинакова
    из любого контекста.»
- `spec/decisions/06-concurrency.md → D71` — обновить bootstrap-секцию:
  + явно: scheduler driven by libuv event loop;
  + ссылка на D92 для top-level семантики;
  + сохранить D71-режим cooperative cancellation;
  + **новая под-секция «Park/wake primitive»** — нормативное
    описание `nova_sched_park`/`nova_sched_wake`/
    `nova_sched_register_pending` API, контракт пользователя,
    cancel-integration. Это implementation-уровневая семантика,
    но фиксируется в spec'е чтобы Plan 21 / Plan 23+ опирались на
    стабильный contract;
  + добавить «Эволюция» с указанием Plan 22 как точки перехода
    busy-yield → libuv timer wheel.
- `spec/decisions/06-concurrency.md` — **новый D92** «Top-level main
  как implicit supervised scope» (R2). Содержание:
  + что: каждый `fn main()` codegen'ится с обёрткой
    `nova_scope_init/run` вокруг тела;
  + правило: drain до полного quiescence перед exit
    (детач'ы дорабатывают, pending timer'ы срабатывают);
  + error propagation: uncaught throw в main → `scope->first_error`
    → re-throw → exit-code = 1 (либо panic-message в stderr);
  + взаимодействие с D13 `exit(code, msg)`: bypass'ит scope drain
    (как defer/errdefer per D90 §8);
  + взаимодействие с D75 cancel_scope: top-level scope не имеет
    user-token, но реализация даёт internal SIGINT-handler →
    `nova_cancel_token_cancel(main_scope_token)` (опционально, для
    graceful shutdown по Ctrl+C — отложено на future Plan);
  + связь: D71 (bootstrap scheduler), D50 (detach), Plan 22 (где
    реализовано).
- `spec/decisions/README.md` — добавить строку D92 в индекс
  `06-concurrency.md`.
- `compiler-codegen/README.md` — bootstrap limitation про "timer-wheel'а
  нет" удалить.

**Тесты:** один benchmark `nova_tests/concurrency/sleep_bench.nv`
(не automated PASS/FAIL, но запускаемый):
```nova
test "10k concurrent sleeps complete in ~100ms not 10s" {
    let t0 = Time.now()
    supervised {
        for _ in 0..10_000 { spawn { Time.sleep(100) } }
    }
    let elapsed = Time.now() - t0
    assert(elapsed < 1000)       // если бы было sequential — 10*1000s
}
```

**Acceptance:**
- Полный сброс busy-yield кода.
- Spec соответствует реализации.
- 10k concurrent sleeps укладывается в ~200ms (10× headroom).
- 91+5 = 96+/96+ PASS.

**Объём:** ~50 строк удаления C + 100 строк spec правки.

---

## Сводка по фазам

| Фаза | Что | Объём | Acceptance |
|---|---|---|---|
| Ф.1 | libuv в build chain (vendored amalgamation, R1) | ~50 строк + vendored libuv | link-smoke PASS |
| Ф.2 | глобальный `uv_loop_t` | ~80 строк | lifecycle-test PASS |
| Ф.3 | `Time.sleep` fiber-ветка через `uv_timer_t` + cancel-wake (R4) | ~180 строк + 3 теста | 3 sleep-теста PASS + 91 regression |
| Ф.4 | main-flow sleep через uv_run | ~50 строк + 1 тест | main-yield-test PASS |
| Ф.5 | top-level sleep + implicit main-scope (D92 prep) | ~30 строк + codegen + 1 тест | top-level-sleep PASS, detach regression-free |
| Ф.6 | cleanup + spec sync (D71 + новый D92, R2) | ~50 удалений + 200 spec + 1 bench | 10k-sleep < 1s, spec sync, D92 принят |

Итого: ~390 строк C, ~200 строк spec, 6 новых тестов, 1 новый D-блок (D92).

---

## Зафиксированные решения

**R1. libuv = vendored amalgamation** (single-file dist в `nova_rt/libuv/`).
~30k строк vendored кода, но быстрый checkout, никаких submodule-quirks,
никаких system-dependencies на Windows. Согласовано с политикой
[feedback_third_party_libs](../../memory/feedback_third_party_libs.md)
— не патчим, обёртки только в `nova_rt/`.

**R2. D92** — новый D-блок в `06-concurrency.md`: «top-level main как
implicit supervised scope». Полная семантика (drain до quiescence, exit
semantics, error propagation chain, detach behavior). Открывается в Ф.5
одним коммитом с реализацией. Не «эволюция D71» — изменение слишком
значимое, требует отдельного D-номера.

**R3. `Time.now()` остаётся на `clock_gettime`/`GetTickCount64`.**
Миграция на `uv_hrtime()` отложена — не блокер для sleep, рисков
с epoch и API больше чем профита (Time.now() возвращает `nova_int`
в ms, uv_hrtime даёт ns). Отдельный тюнинг при необходимости.

**R4. Cancel-during-sleep — фиксим в Ф.3.** CancelToken расширяется
list'ом pending `uv_timer_t*`. `cancel()` итерируется по pending
таймерам, делает `uv_timer_stop` + явный wake fiber'а через `nova_sched_wake`.
+30 строк, но cancel-during-long-sleep работает immediate (как ожидает
[D75](../../spec/decisions/06-concurrency.md#d75) — `tok.cancel()`
семантически даёт fail-fast).

---

## Что НЕ входит в Plan 22

- **Socket IO через libuv** — отдельный Plan 23+ (под `std.net`).
  Plan 22 только timer'ы.
- **Threading / M:N scheduler** — `uv_loop_t` остаётся single-threaded.
  Multi-thread runtime — гораздо позже, отдельная задача (v1.0+
  milestone, требует work-stealing scheduler, TLS migration,
  atomic-готового runtime).
- **`Time.now()` via `uv_hrtime`** — R3, оставлено на
  `clock_gettime`/`GetTickCount64`.
- **High-precision timers** (микросекунды, наносекунды). libuv даёт ms,
  Nova `Time.sleep(ms)` — `nova_int` ms, согласовано.
- **Cron-style scheduling, `Time.every`** — stdlib feature, не runtime.
- **SIGINT-graceful shutdown** для top-level main — упоминается в
  D92 как optional future extension, не реализуется в Plan 22.

---

## Связь с другими планами

- [Plan 18](18-stdlib-roadmap.md) — Plan 22 закрывает один из
  prerequisite'ов для `std.net`/`std.fs` (единый event loop). Любой
  socket-IO позже встанет на тот же park/wake mechanism, что вводит
  Plan 22 Ф.3 — handle будет `uv_tcp_t`/`uv_poll_t` вместо `uv_timer_t`,
  семантика та же.
- [Plan 21](21-channel-revision-implementation.md) — Channel D91.
  `send`/`recv` блокировка должна использовать тот же park/wake
  primitive (`nova_sched_park`/`nova_sched_wake`) что и `Time.sleep`.
  Cancel-during-recv — по тому же паттерну что R4 (CancelToken держит
  pending channel-waiters). Если Plan 22 завершён раньше — Plan 21
  реализует `send`/`recv` сразу через park/wake без двухэтапной
  переделки. Plan 22 формально не блокирует Plan 21, но архитектурный
  prerequisite. См. [Plan 21 «Интеграция с Plan 22»](21-channel-revision-implementation.md#интеграция-с-plan-22).
- [Plan 09](09-clang-migration.md) — libuv build на Linux/clang
  раскрывается тогда же.
- [Plan 20](20-defer-implementation.md) — `defer tx.close()` для
  Channel'ов: не блокер, но Plan 22 не зависит от defer.
- [D71](../../spec/decisions/06-concurrency.md) — bootstrap-семантика
  scheduler'а, обновляется в Ф.6.
- [D75](../../spec/decisions/06-concurrency.md#d75) — cancel_scope:
  взаимодействие с sleep'ом — R4 (cancel пробуждает parked-fiber'ы).
  Тот же mechanism наследует Plan 21 для cancel-during-recv.

---

## Verification

После полного плана:
- `run_tests.ps1`: 96+/96+ PASS на Windows MSVC.
- `nova-codegen test`: interp без регрессий (Plan 22 не трогает interp;
  interp `Time.sleep` остаётся как был — простой блокинг или no-op).
- Bench: 10k concurrent sleep'ов в <1s wall-clock, CPU idle <10% на
  sleep period.
- Spec: `Time.sleep` секция в `syntax.md` и D71 в `06-concurrency.md`
  соответствуют реализации (отсутствуют упоминания "ms игнорируется"
  / "timer-wheel'а нет").

---

## Цена

1. **libuv в bootstrap.** +30k строк vendored кода, +1 dependency на
   build. Mitigation: vendored single-file dist, не патчим.
2. **Изменение scheduler-loop'а.** Сейчас `nova_supervised_run`
   — простой `while alive > 0`. После Plan 22 — состояние "alive vs
   parked vs ready", `uv_run` в idle. Сложнее для отладки.
3. **Implicit main-scope.** Семантическое изменение, требует D-блок
   (Q2).
4. **Cancel race (Q4).** Дополнительная сложность в cancel-token
   bookkeeping (~30 строк). Без неё long-sleep + cancel становится
   broken UX.
5. **Тест-flakiness риск.** Wall-clock тесты с slack'ами могут
   моргать в CI. Slack 100ms — компромисс между чувствительностью и
   стабильностью.
