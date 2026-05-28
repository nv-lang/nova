// SPDX-License-Identifier: MIT OR Apache-2.0
# Материалы для статьи: 10 часов поиска одного race condition в M:N runtime

> Внутренние заметки. Полная хронология поиска бага, все попытки, финальный фикс.
> Язык: Nova (system-level language с M:N async runtime).
> Дата: 2026-05-27–28. Платформа: Windows 11, 16 логических CPU.

---

## Контекст: что такое Nova M:N runtime

**Nova** — системный язык программирования. Async runtime построен по модели M:N:
- **M** — горутины (в Nova: _fibers_), реализованы через библиотеку `minicoro` (stackful coroutines)
- **N** — OS-потоки (_workers_), по числу CPU (на машине тестирования: 16)

Каждый worker — это OS thread с собственным libuv event loop. Fiber'ы кооперативно
переключаются через `mco_yield` / `mco_resume`.

**Ключевые структуры:**

```c
// Scope — группа fiber'ов под общим supervision'ом
typedef struct NovaFiberQueue {
    mco_coro**  fibers;          // массив живых fiber'ов
    int         count;           // заполненность
    int         capacity;        // размер массива
    // ...
    struct NovaSleepState* armed_sleeps_head;  // linked list активных таймеров
} NovaFiberQueue;

// Планировщик для scope — parallel arrays
typedef struct NovaSchedState {
    nova_bool*  parked;          // parked[i]=true если fiber i в mco_yield
    int32_t*    pending_wake;    // pending_wake[i] — wake delivered before park
    int         capacity;
} NovaSchedState;

// Состояние одного sleep(N ms)
typedef struct NovaSleepState {
    NovaFiberQueue*  scope;
    int              slot;          // индекс в scope->fibers[]
    uv_timer_t       timer;         // UV timer handle (стек fiber'а!)
    nova_atomic_int  stage;         // NEW/ARMED/FIRING/CANCEL_REQ/CLOSED
    int              home_worker_id;
    mco_coro*        expected_co;   // ← ключевое поле Fix B
    struct NovaSleepState* next_in_scope;
    struct NovaSleepState** pprev_in_scope;
} NovaSleepState;
```

**Nova language — supervised spawn:**

```nova
// D50: supervised block — ждёт завершения всех spawn'нутых fiber'ов
supervised {
    for _ in 0..100 {
        spawn { sleep(10000) }   // 100 fiber'ов, каждый спит 10 секунд
    }
    tok.cancel()                 // отменить всех через 0ms (сразу)
}
// supervisor выходит когда pending_remote == 0
```

**Plan 83.11 Ф.2: Centralized driver thread**

В рамках Plan 83.11 был добавлен отдельный `_nova_driver` thread с единым
UV loop для всех таймеров (vs. N per-worker UV loops). Fiber вызывает
`_nova_sleep_via_driver` → посылает ARM_SLEEP job в driver → driver создаёт
`uv_timer_t` в своём loop → по истечении `close_cb` будит fiber через
`dispatch_ready`.

---

## Симптом

```
stress_iso_3e.c:
  100 fibers × sleep(10000ms)
  tok.cancel() немедленно → cancel_scope job → driver walk → uv_close × 100
  → all 100 close_cb fire
  → supervison должен разрядить pending_remote = 0 и выйти
  → НО: зависает навсегда
```

**Метрики до фикса:** stress_iso_3e × 30 = **3–5 PASS, 25–27 TIMEOUT**  
**После фикса:** 30/30 PASS

---

## Хронология поиска: 6 сессий, ~10 часов

---

### Сессия 1 (2026-05-27): первые диагностические счётчики → Attempt 1

**Диагностика:** добавили атомарные счётчики в close_cb и nova_sched_wake.
Результат первых прогонов:
```
close_cb    = 100  ✓  (все таймеры закрылись)
wake_called = 100  ✓  (wake вызван для каждого)
cas_won     = 89–97   ← race! 3–11 wake CAS проиграли
```

**CAS в nova_sched_wake:**
```c
bool was_parked = CAS(parked[slot], true, false);  // atomic
if (was_parked) dispatch_ready(scope, co);          // только если выиграл
```

**Гипотеза:** wake-before-park race. `close_cb` вызывается ДО того как
fiber успел сделать `mco_yield` → parked[slot]=false в момент CAS → wake потерян.

**Attempt 1 — unconditional dispatch:**
```c
// Не проверять parked, диспатчить всегда
dispatch_ready(scope, co);
```
Результат: **1/30 PASS**. Вероятная причина — double-dispatch: fiber
уже бежит, второй resume → UB в mco_resume. Откат.

---

### Сессия 2 (2026-05-27): generational futex → Attempt 2

**Гипотеза:** нужна "версия" wake'а — wake_gen counter. Linux futex idiom:
park снимает snapshot gen до yield, после yield проверяет вырос ли.

**Attempt 2 — wake_gen counter на NovaSleepState:**
```c
// Wake side:
st->wake_gen++;           // инкремент перед CAS parked
CAS(parked, true, false)  // CAS как обычно

// Park side (после mco_yield):
if (current_gen != st->wake_gen) return;  // wake пришёл — нормальный выход
```
Результат: **1/30 PASS**. `dispatch_ready` иерархия имеет несколько race
windows сама по себе — gen counter не помог. Откат.

---

### Сессия 3 (2026-05-27): heisenbug обнаружен → Attempt 3

**Ключевое наблюдение:** добавление `fprintf` в close_cb давало **1/1 PASS**
(при единичном прогоне). Без fprintf — снова TIMEOUT.

Это классический **memory fence heisenbug**: `stdio` lock добавляет mfence,
что меняет видимость атомарных операций. Значит: race существует, но мы
ищем его не там. `fprintf` маскирует, не фиксит.

**Attempt 3 — SEQ_CST stage + futex park:**
```c
// Оба SEQ_CST для total order:
__atomic_store_n(&st->stage, DRV_CLOSED, __ATOMIC_SEQ_CST);  // driver
__atomic_store_n(&st->parked[slot], true, __ATOMIC_SEQ_CST);  // worker
```
Результат: **1–19/30 PASS** (зависит от нагрузки на систему). Улучшение,
но не детерминированно. Откат.

**Вывод после сессии 3:** plateau ~50% PASS. Нужен другой подход к диагностике.

---

### Сессия 4 (2026-05-27): single-atomic → Attempt 4 (SEGFAULT)

**Гипотеза:** полностью заменить парадигму. Tokio 1.x использует единый
AtomicUsize со packed bits (IDLE/PARKED/NOTIFIED). Порт на C:

```c
typedef enum { SW_NEW, SW_PARKED, SW_WOKEN, SW_CONSUMED } NovaSleepWaitState;

// Wait side:
CAS(wait_state, NEW, PARKED);
mco_yield();
// Wake side:
CAS(wait_state, PARKED, WOKEN);
dispatch_ready(...)
```

**Attempt 4 — чистый single-atomic (без parked[]):**  
Результат: **SEGFAULT** на slot 0 → stack overflow в fiber arena.

Fiber arena жёстко завязана на `parked[]` в 4 местах:
- `nova_scope_free_slot` — читает `parked[i]`
- `cancel_wake_all` — итерирует по `parked[i]`
- `nova_supervised_step` — читает `parked[i]`
- `nova_runtime_cancel_worker_fibers` — читает `parked[i]`

Полная замена требовала переписать все 4 места — ~5 dev-days.

**Attempt 4b — hybrid (wait_state + parked[] оба):**  
Результат: **3/30 PASS**. Гибрид хуже чем SEQ_CST alone. Откат.

---

### Сессия 5 (2026-05-27): pending_wake[] → Attempt 5 (wrong target)

**Пересмотр гипотезы:** state-dump (ранняя версия) показал `pending_remote=7`
на зависшем процессе. Гипотеза: pending_remote leak в spawn-epilogue.
7 fiber'ов исчезли из всех state-ёмкостей (worker scopes, deques, supervised),
но `pending_remote` не декрементировался.

**Attempt 5 — pending_wake[] в generic nova_sched_park/wake API:**

```c
// nova_sched_park: 3 checkpoint'а вокруг SEQ_CST parked store:
t1: pre-check CAS pending_wake[slot] 1→0 → fast return без mco_yield
t2: SEQ_CST parked[slot] = true       → full memory fence
t3: post-barrier recheck CAS 1→0     → если wake пришёл в окне t2→t3

// nova_sched_wake: CAS pending_wake 0→1 FIRST, потом CAS parked
```

Результат: **10/30 PASS** — лучший partial result. Но acceptance = 30/30.
14 прогонов: cancel завершился но >300ms (contention). 6 прогонов: TIMEOUT.

**Критическая ошибка:** `pending_remote=7` был артефактом неатомарно связанных
диагностических счётчиков, не реальной утечкой. Мы фиксили не тот баг.

---

### Сессия 6 (2026-05-28): programmatic state-dump → ROOT CAUSE → ПОБЕДА

**Полная dump-инфраструктура (nova_runtime_dump_state):**

```c
// Вызывается при NOVA_WATCHDOG_DUMP_SECS=N секунд ожидания
void nova_runtime_dump_state(const char* reason) {
    // Lock-free best-effort snapshot:
    // - все workers: worker_id, fiber count, deque size
    // - sched_state: parked[], pending_wake[] per slot
    // - supervised scope: pending_remote counter
    // - armed_sleeps_head: все активные таймеры
}
```

**Счётчики на каждом шаге delivery chain:**
```
close_cb        = 100   ✓  (все закрылись)
wake_called     = 100   ✓  (wake вызван)
cas_won         =  93   ←  7 CAS проиграли
sleep_ret       =  93   ←  7 fiber'ов не вернулись из sleep
pending_remote  =   7   ←  7 не декрементировались
```

**Ключевая строка в dump:**
```
scope fibers[5] = NULL   parked[5] = true
```

`fibers[slot] = NULL` при `parked[slot] = true` — это невозможная комбинация
в нормальном состоянии. Слот "свободен" (fiber = NULL) но "занят" (parked = true).

**Это и есть root cause.**

---

## Root Cause: STALE slot condition

```
Полная временная шкала race:

t0: Fiber A: alloc_slot → slot=5, fibers[5]=A, запускает uv_timer
t1: Fiber A: вызывает nova_sched_park →
        parked[5] = true  (SEQ_CST)  ← парковка зафиксирована
        mco_yield()                   ← отдаём управление
        (fibers[5] станет A снова после resume, НО в момент yield
         он может быть NULL или неактуальным — зависит от реализации)

t2: Новый spawn вызывает alloc_slot:
        for i in 0..count:
            if fibers[i] == NULL:    ← смотрим только на fibers[]!
                                       НЕ проверяем parked[i]
                fibers[i] = FiberB   ← ЗАХВАТ слота 5 новым fiber'ом!

t3: uv_timer срабатывает → driver: uv_close → close_cb:
        actual_co = scope->fibers[5] = FiberB  (не FiberA!)
        FiberA != FiberB → WRONG-FIBER
        → wake пропущен
        → FiberA застряла в mco_yield НАВСЕГДА
        → pending_remote никогда не декрементируется
        → supervisor ждёт вечно → HANG
```

**Корень:** `nova_scope_alloc_slot` проверял только `fibers[i] == NULL`.
Слот выглядел свободным, хотя оригинальный fiber ещё находился в `mco_yield`.

---

## Исправление: Fix A + Fix B + Fix C

### Fix A — alloc_slot не берёт STALE слоты

**Файл:** `nova_rt/fibers.h`, функция `nova_scope_alloc_slot`

```c
for (int i = 0; i < scope->count; i++) {
    if (scope->fibers[i] == NULL) {

        // FIX A: проверить parked[i] и pending_wake[i]
        // Если parked=true — original fiber ещё в mco_yield.
        // close_cb (Fix B) разбудит его напрямую когда сработает таймер.
        // После пробуждения parked станет false и слот можно переиспользовать.
        NovaSchedState* sst = nova_sched_find_state(scope);
        if (sst && i < sst->capacity) {
            bool pk = __atomic_load_n(&sst->parked[i], __ATOMIC_SEQ_CST);
            int32_t pw = sst->pending_wake
                ? __atomic_load_n(&sst->pending_wake[i], __ATOMIC_ACQUIRE)
                : 0;
            if (pk || pw) continue;  // ← STALE: пропустить
        }

        scope->fibers[i] = co;  // безопасно занять
        // ...
    }
}
```

---

### Fix B — close_cb будит правильный fiber по pointer

**Файл:** `nova_rt/driver.c`, функция `_nova_driver_sleep_close_cb`

```c
static void _nova_driver_sleep_close_cb(uv_handle_t* h) {
    NovaSleepState* st = (NovaSleepState*)h->data;
    if (!st) return;
    _nova_driver_arm_list_unlink(st);

    NovaFiberQueue* sc = st->scope;
    int sl = st->slot;
    mco_coro* actual_co = (sc && sl >= 0 && sl < sc->count)
        ? sc->fibers[sl] : NULL;
    NovaSchedState* sst = sc ? nova_sched_find_state(sc) : NULL;

    if (actual_co != st->expected_co) {
        // WRONG-FIBER: в слоте уже другой fiber (или NULL)
        // Два sub-case:
        //   A) actual_co == NULL: STALE race — original fiber живой, stuck в mco_yield
        //   B) actual_co != NULL: слот законно переиспользован, original мёртв

        mco_coro* expected_co = st->expected_co;
        if (expected_co && mco_status(expected_co) == MCO_SUSPENDED) {
            // Sub-case A: original fiber живой — будим напрямую по pointer

            __atomic_store_n(&st->stage, NOVA_SLEEP_DRV_CLOSED, __ATOMIC_SEQ_CST);

            // Очищаем parked[slot] — слот больше не "занят" original fiber'ом
            if (sst && sl >= 0 && sl < sst->capacity) {
                bool exp_t = true;
                __atomic_compare_exchange_n(
                    (volatile bool*)&sst->parked[sl],
                    &exp_t, false, false,
                    __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE);
                if (sst->pending_wake && sl < sst->capacity)
                    __atomic_store_n(&sst->pending_wake[sl], 0, __ATOMIC_RELEASE);
            }

            // Fix C: sentinel -2 — epilogue НЕ вызовет free_slot
            NovaSpawnCtxBase* displaced_ctx =
                (NovaSpawnCtxBase*)mco_get_user_data(expected_co);
            if (displaced_ctx)
                displaced_ctx->_nova_worker_slot = -2;

            // Fix B: диспатчим original fiber напрямую
            nova_fiber_state_store(expected_co, NOVA_FIBER_STATE_IDLE);
            if (sc && sc->dispatch_ready)
                sc->dispatch_ready(sc->dispatch_ctx, expected_co);

        } else {
            // Sub-case B: original мёртв — just close
            __atomic_store_n(&st->stage, NOVA_SLEEP_DRV_CLOSED, __ATOMIC_SEQ_CST);
        }
        return;
    }

    // Нормальный путь: fiber в своём слоте, стандартный wake
    __atomic_store_n(&st->stage, NOVA_SLEEP_DRV_CLOSED, __ATOMIC_SEQ_CST);
    nova_sched_wake(st->scope, st->slot);
}
```

---

### Fix C — DISPLACED sentinel предотвращает двойной free_slot

**Файл:** spawn epilogue (codegen-generated C code)

```c
// В конце каждого fiber'а:
if (ctx->_nova_worker_slot >= 0) {      // обычный случай
    nova_scope_free_slot(scope, ctx->_nova_worker_slot);
} else if (ctx->_nova_worker_slot == -2) {
    // DISPLACED: Fix B взял ownership — epilogue не трогает слот
    // (слот уже принадлежит другому fiber'у или очищен close_cb)
}
// -1 = другие sentinel случаи (без scope)
```

---

## Три строки, которые решили всё

```c
if (pk || pw) continue;                              // Fix A: не занимать STALE слот
displaced_ctx->_nova_worker_slot = -2;              // Fix C: запретить double free_slot
sc->dispatch_ready(sc->dispatch_ctx, expected_co); // Fix B: разбудить правильный fiber
```

---

## Верификация

**До фикса:** stress_iso_3e × 30 = **3–5 PASS** (10–17%)  
**После фикса:** stress_iso_3e × 30 = **30/30 PASS** (100%)  
**Concurrency suite (78 тестов):** 74 PASS / 4 FAIL (4 pre-existing flaky)  
**Платформа:** Windows 11, 16 логических CPU, Boehm GC, minicoro

---

## Что помогло найти баг — и что нет

### Не помогло: шаговая отладка
На Windows 11 в момент исследования: gdb недоступен, lldb падал на python311.dll,
windbg/cdb отсутствовали. Но даже с debugger'ом — M:N race на 16 потоках
не воспроизводится в single-step режиме (Heisenberg uncertainty для concurrent bugs).

### Не помогло: fprintf-диагностика
Каждый `fprintf` в критическом пути добавлял `mfence` через stdio lock.
Это _маскировало_ race, не фиксило. Несколько раз казалось "починилось" —
оказывалось что просто stdio lock менял видимость операций.

### Помогло: programmatic state-dump
```c
// Watchdog в supervisor — авто-dump если ждёт > N секунд
if (wait_secs > NOVA_WATCHDOG_DUMP_SECS) {
    nova_runtime_dump_state("watchdog");
}

// Dump: lock-free best-effort snapshot
// workers × (fiber count, deque size, runnext)
// sched_state × (parked[], pending_wake[] per slot)
// supervised scope: pending_remote
// armed_sleeps_head: все активные таймеры
```

**Ключевая строка в dump:**
```
scope slot[5]: fibers=NULL  parked=true  pending_wake=0
```

Это "невозможная" комбинация — сразу указала на alloc_slot.

### Помогло: expected_co как identity

В `NovaSleepState` поле `expected_co` — pointer на fiber, захваченный
в момент arm_sleep. `close_cb` сравнивает `scope->fibers[slot]` с `expected_co`.
Несовпадение = WRONG-FIBER = STALE race.

Аналог: Tokio хранит `task pointer` в `TimerEntry` по той же причине.

---

## 6 уроков

**1. Heisenbug = memory fence**  
Если `fprintf` fix'ит race — это маскировка, не исправление. `stdio` lock
добавляет `mfence`. Никогда не считай "починилось" пока есть fprintf в критическом пути.

**2. State-dump важнее отладчика**  
При M:N concurrency: тысяча состояний × N потоков — пошагово не пройдёшь.
Lock-free programmatic snapshot даёт полную картину за один запуск.
Реализовать: `~80 строк C` + `NOVA_WATCHDOG_DUMP_SECS=5` env переменная.

**3. Identity через pointer, не через индекс**  
`close_cb` должен знать КТО её fiber, не только ГДЕ (slot index).
`expected_co = mco_coro*` захваченный при arm — неизменяемая identity.
Slot — изменяемый, может быть переиспользован.

**4. Slot ownership = NULL + parked + pending_wake**  
`fibers[i] == NULL` не означает "слот свободен". Слот свободен когда:
`fibers[i] == NULL` AND `parked[i] == false` AND `pending_wake[i] == 0`.
Три условия, не одно.

**5. Атомарные счётчики должны быть атомарно связаны с операцией**  
Счётчик `pending_remote_dec` не отражал реальный decrement если
инкремент и decrement были в разных точках кода. Misleading data
стоила 2+ часов на "утечку" которой не было.

**6. После 3 итераций < 50% PASS — стоп, пересмотр**  
6 попыток на один race surface (83.10.1–83.10.5 + 83.11 Ф.3) — definitive signal.
Каждая tactical попытка фиксила что-то реальное, но не root cause.
Правило: если 3+ итерации дают плато и каждый раз "почти работает" —
либо смени инструмент диагностики, либо смени архитектурный уровень.

---

## Параллели с другими runtime'ами

| Runtime | Аналогичный race | Решение |
|---------|-----------------|---------|
| Tokio (Rust) | Per-worker reactors → timer races (2017–2019) | Centralized driver thread (1.0, 2020) |
| Go runtime | Timer races в early goroutine scheduler | Centralized timer goroutine + netpoll |
| Node.js libuv | Single-thread: нет per-thread UV loop race | Архитектура изначально централизованная |
| Java NIO | Selector races в multi-thread | Один Selector per JVM, lock-protected |

Nova шла тем же путём: 5 tactical plans (83.10.1–5) → Plan 83.11 (centralized driver).
Tokio потратил ~3 года на тот же путь. Lesson применён с меньшими потерями.

---

## Итого

- **Время:** ~10 часов / 6 сессий
- **Попыток:** 5 ложных + 1 верная
- **Код фикса:** ~50 строк в 3 файлах
- **Ключ к решению:** programmatic state-dump (80 строк инфраструктуры) + expected_co pointer
- **Результат:** 3–5/30 → 30/30 stress PASS
