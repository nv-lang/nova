// SPDX-License-Identifier: MIT OR Apache-2.0
# Case study: STALE-slot race в M:N runtime (2026-05-28)

> **Тип:** concurrency bug / race condition  
> **Контекст:** Nova M:N runtime — 16 worker OS threads + cooperative fibers (minicoro)  
> **Время на решение:** ~10 часов / 6 сессий  
> **Результат:** 30/30 stress PASS после Fix A+B+C (~50 строк кода)  

---

## Симптом

```nova
supervised {
    for _ in 0..100 {
        spawn { sleep(10000) }
    }
    tok.cancel()
}
// → зависает навсегда
```

`cancel()` срабатывал, все 100 UV timers закрывались (`close_cb` вызывался),
но `pending_remote` никогда не доходил до 0 — supervisor ждал вечно.

---

## Root cause

**STALE slot condition** — состояние гонки между `alloc_slot` и `close_cb`:

```
Временная шкала:
  t0: Fiber A занимает slot=5, запускает uv_timer
  t1: Fiber A вызывает nova_sched_park → SEQ_CST parked[5]=true → mco_yield
      В момент mco_yield: fibers[5]=NULL (ещё не restore'но после yield)
  t2: Другой spawn вызывает alloc_slot
      alloc_slot видит: fibers[5]==NULL → слот "свободен"!
      → Fiber B занимает slot=5
  t3: close_cb срабатывает для Fiber A
      actual_co = scope->fibers[5] = Fiber B  (не Fiber A!)
      → WRONG-FIBER → wake пропущен
      → Fiber A застряла в mco_yield навсегда → pending_remote не декрементируется
```

Корень: `alloc_slot` проверял только `fibers[i]==NULL`, не проверял `parked[i]`.
Слот выглядел свободным (`fibers[i]=NULL`), хотя оригинальный fiber ещё
находился в `mco_yield` — он живой, просто не restore'ил свой слот.

---

## Ложные пути (что не сработало)

| # | Гипотеза | Fix | Результат |
|---|----------|-----|-----------|
| 1 | wake-before-park: wake CAS fails т.к. fiber ещё не parked | Unconditional dispatch из close_cb | 1/30 PASS — double-dispatch |
| 2 | Нужна версия wake'а (Linux futex idiom) | wake_gen counter | 1/30 PASS — race в dispatch_ready иерархии |
| 3 | Memory ordering: wake→park не видит друг друга | SEQ_CST stage + futex park | 1-19/30 PASS — heisenbug (fprintf поднимал до 1/1) |
| 4 | Заменить parked[] на единый AtomicUsize (Tokio AtomicWaker port) | NovaSleepWaitState enum | SEGFAULT — fiber arena coupled к parked[] |
| 5 | pending_remote leak в epilogue (счётчики говорили утечка) | pending_wake[] в generic park/wake API | 10/30 PASS — wrong target, счётчики врали |

**Сигнал Heisenbug:** попытка #3 — добавление `fprintf` в close_cb давало 1/1 PASS.
`stdio` lock добавляет memory fence. Это значит: race есть, но не там где мы думали.

---

## Диагностический алгоритм

То, что **не работало** 5 сессий: гипотеза → fix → тест → откат.

То, что **сработало** в сессии #6:

1. **Programmatic watchdog dump** — `nova_runtime_dump_state()`:  
   lock-free снимок всех workers + sched_state при зависании (через `NOVA_WATCHDOG_DUMP_SECS=5`)

2. **Atomic counters на каждом шаге** delivery chain:  
   `close_cb` → `wake_called` → `cas_won` → `sleep_ret`

3. **Dump показал:**  
   ```
   close_cb=100  wake=100  cas_won=93  sleep_ret=93
   fibers[5]=NULL  parked[5]=true
   ```
   NULL в `fibers[slot]` при `parked=true` → STALE condition.

**Вывод:** state-dump > step-debugger при отсутствии debugger'а.
Без dump — 5 сессий слепых попыток. С dump — локализация за один запуск.

---

## Исправление (Fix A + B + C)

### Fix A — `fibers.h`, `alloc_slot`

```c
for (int i = 0; i < scope->count; i++) {
    if (scope->fibers[i] == NULL) {
        // Fix A: проверить parked[i] перед reuse
        NovaSchedState* sst = nova_sched_find_state(scope);
        if (sst && i < sst->capacity) {
            bool pk = __atomic_load_n(&sst->parked[i], __ATOMIC_SEQ_CST);
            int32_t pw = sst->pending_wake
                ? __atomic_load_n(&sst->pending_wake[i], __ATOMIC_ACQUIRE) : 0;
            if (pk || pw) continue;  // ← STALE: пропустить
        }
        scope->fibers[i] = co;  // безопасно занять
        ...
    }
}
```

### Fix B + C — `driver.c`, `_nova_driver_sleep_close_cb`

```c
mco_coro* actual_co = sc->fibers[sl];

if (actual_co != st->expected_co) {
    mco_coro* expected_co = st->expected_co;

    if (expected_co && mco_status(expected_co) == MCO_SUSPENDED) {
        // Fiber жив, застрял в mco_yield — разбудить напрямую

        __atomic_store_n(&st->stage, NOVA_SLEEP_DRV_CLOSED, __ATOMIC_SEQ_CST);

        // Очистить parked[slot]
        bool exp_t = true;
        __atomic_compare_exchange_n(&sst->parked[sl], &exp_t, false, ...);
        __atomic_store_n(&sst->pending_wake[sl], 0, __ATOMIC_RELEASE);

        // Fix C: sentinel -2 — epilogue НЕ вызовет free_slot для этого fiber'а
        NovaSpawnCtxBase* ctx = mco_get_user_data(expected_co);
        if (ctx) ctx->_nova_worker_slot = -2;

        // Fix B: диспатчить правильный fiber
        nova_fiber_state_store(expected_co, NOVA_FIBER_STATE_IDLE);
        sc->dispatch_ready(sc->dispatch_ctx, expected_co);
    } else {
        // Fiber мёртв — слот законно переиспользован
        __atomic_store_n(&st->stage, NOVA_SLEEP_DRV_CLOSED, __ATOMIC_SEQ_CST);
    }
    return;
}

// Нормальный путь
__atomic_store_n(&st->stage, NOVA_SLEEP_DRV_CLOSED, __ATOMIC_SEQ_CST);
nova_sched_wake(st->scope, st->slot);
```

**Три ключевые строки:**
- `continue;` — не занимать STALE слот
- `ctx->_nova_worker_slot = -2;` — запретить двойной free_slot (Fix C)
- `sc->dispatch_ready(sc->dispatch_ctx, expected_co);` — разбудить нужный fiber (Fix B)

---

## Lessons learned

1. **Heisenbug = memory fence**: если `fprintf` fix'ит race — это маскировка,
   не fix. Stdio lock добавляет mfence. Не считай «исправлено».

2. **State-dump важнее шагового дебаггера** при M:N concurrency: тысяча
   состояний × N потоков — пошагово не пройдёшь. Один lock-free snapshot
   даёт полную картину.

3. **`expected_co` как identity**: хранить pointer на ожидаемый fiber в
   state-структуре (как Tokio хранит task pointer в `TimerEntry`).
   Без него close_cb не различает «мой fiber» от «чужой fiber в том же slot'е».

4. **Slot ownership ≠ NULL check**: `fibers[i]==NULL` не значит «слот свободен».
   Нужно проверять весь набор: `fibers[i] + parked[i] + pending_wake[i]`.

5. **Counter atomicity**: диагностические счётчики должны быть атомарно
   связаны с операцией которую считают, иначе данные misleading.
   Сессия #5 потратила ~2 часа на «утечку pending_remote» которой не было.

6. **После 3 итераций < 50% PASS — ESCALATE**: не tactical iterate.
   6 attempts на один race = definitive architectural signal.
   Tokio потратил ~3 года на тот же класс races до architectural rewrite (1.0, 2020).

---

## Связанные материалы

- Plan 83.11 §10 — post-mortem с полным историческим контекстом
- `nova_rt/driver.c` — финальный код Fix B+C
- `nova_rt/fibers.h` — финальный код Fix A
- `nova_rt/nova_sched.h` — `pending_wake[]` API (Option A v3, интегрирован)
- Plans 83.10.1–83.10.5 — предшествующие tactical fixes в той же зоне
