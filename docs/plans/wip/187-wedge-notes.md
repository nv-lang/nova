<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# [M-187-high-concurrency-connection-wedge] — чекпоинт (opus-волна)

Worktree: `d:/Sources/nv-lang/nova-wedge`, ветка `p187-wedge-scheduler`.
В main НЕ мёржить, push не делать (решение интегратора после гейтов).
Модель: opus. Коммить мелко (сеть рвётся часто).

## Доказанный корень (НЕ переоткрывать — из main-заметок)

`docs/plans/wip/boehm-eager-cost-notes.md` §ОКОНЧАТЕЛЬНЫЙ КОРЕНЬ +
`docs/plans/wip/211-spawnctx-notes.md`: порча SpawnCtx (128-байтный
uncollectable класс) в шторме spawn+wake+cancel. Две gdb-сигнатуры (ASLR off):
1. **Free-list Boehm**: `GC_generic_malloc_uncollectable` разыменовывает
   усечённый линк (`0x7ffff7d30880` → `0xf7d30880`, обнулены биты 32-47 =
   offset 4-7). Всплывает на СВЕЖЕМ (не recycled) `nova_spawn_pool_acquire(104)`
   на ГЛАВНОМ потоке внутри `for _ in 0..2000 { spawn {...} }`.
2. **Wake-путь**: `nova_sched_cap_acq(st=0x656c632820646c6f)` (ASCII "old (cle")
   ← `nova_goready(co)` ← `nova_sched_wake(slot)` ← `_nova_driver_sleep_close_cb`.
   `st = scope->sched_state`, `scope = base->_nova_fiber_scope`,
   `base = mco_get_user_data(co)`. **Сам base (SpawnCtx) повреждён** →
   `_nova_fiber_scope` = мусор.

## Уже влито (211-волна, в main) — НЕ дублировать

1. `driver.c::_nova_driver_sleep_close_cb`: ACQUIRE-load `sc->count` перед
   индексацией `sc->fibers[sl]` (реальный data-race fix, но НЕ причина —
   сигнатура 2 идёт нормальным, не WRONG-FIBER путём).
2. `fibers.h::nova_scope_grow_children`: child_ctx[] uncollectable→collectable
   (развод size-class коллизии 128Б; НЕ причина — 15/15 SIGSEGV).

Обе оставлены (безопасны), гонка ЖИВА (pos_max_fibers ~100% WSL).

## Layout NovaSpawnCtxBase (fibers.h:1824-1884) — offsets x86-64

```
off 0:   NovaFiberQueue* _nova_parent_scope       (8)   <- pool-release пишет free-list next СЮДА
off 8:   int             _nova_parent_slot        (4)
off 12:  int             _nova_worker_slot        (4)
off 16:  NovaFailFrame*   _nova_saved_fail_top     (8)
off 24:  NovaInterruptFrame* _nova_saved_interrupt_top (8)
off 32:  NovaFiberQueue* _nova_fiber_scope         (8)   <- сигнатура 2 читает мусор отсюда
off 40:  NovaEffectSnapshot* _nova_init_snapshot   (8)
off 48:  nova_atomic_int  _nova_fiber_state        (4)   <- 32-битная atomic-запись
off 56:  size_t           _nova_pool_size          (8)
off 64:  nova_atomic_int  _nova_cancel_mask_count  (4)   <- 32-битная atomic-запись
off 72:  int64_t          _nova_cancel_deadline_ns (8)
off 80:  nova_atomic_int  _nova_park_state         (4)   <- 32-битная atomic-запись (goready/gopark)
off 88:  mco_coro*        schedlink                (8)
```
База = 96Б. Пул-класс для 104Б = 128 (`_nova_spawn_pool_class_size[1]`).

Сигнатура 1 обнуляет offset 4-7 = HIGH-half указателя `_nova_parent_scope`
(offset 0). Free-list Boehm держит линк в offset 0 (8Б) освобождённого блока.
Значит: 32/16-битная запись 0 легла в offset 4 УЖЕ ОСВОБОЖДЁННОГО SpawnCtx.
НИ ОДНО поле базы не лежит на offset 4 (parent_slot на off 8). Кандидат: запись
через ДРУГОЙ тип, алиасящий тот же 128Б-блок (R1 aliasing), ИЛИ pool-release
пишет 8-байтный next в off 0, а параллельная 32-битная запись (goready в off 80
у recycled блока? нет — off 80) — НЕ сходится напрямую. ТРЕБУЕТ проверки в WSL/gdb
точного stack записи (watchpoint на освобождённый блок).

## Прочитанные пути (карта)

- `nova_sched.h`: nova_goready (R2 CAS WAIT->DISPATCHED, потом dispatch_ready),
  nova_gopark, nova_sched_wake (резолвит parked_co[slot], funnel goready),
  register/unregister_pending, cancel_all_pending, cancel_wake_all,
  nova_sched_grow_state (chunked, never-move, CAS-publish).
- `driver.c`: _nova_driver_sleep_close_cb (ACQUIRE count + expected_co-match +
  DISPLACED sentinel -2), handle_arm_sleep, timer_cb, handle_cancel_scope
  (walk armed_list, pending_driver_jobs decrement).
- `fibers.h`: nova_scope_alloc_slot (slot_lock spinlock, skip-stale parked[i]),
  free_slot, nova_scope_sweep_dead_child / retain_or_release_child (pool-release
  мёртвого ребёнка, snapshot parent перед release т.к. off0 перетирается),
  _nova_sleep_via_driver / _via_libuv (NovaSleepState на стеке фибера,
  expected_co=mco_running, park_until predicate stage==CLOSED).
- `runtime.c`: nova_spawn_pool_acquire/release (P-local free-list, intrusive
  link в off0; main-thread wid<0 -> прямой Boehm alloc/free).

## Ключевые кандидаты для дискриминатора (проверить в WSL)

(а) sleep close_cb / cancel-wake стреляет после освобождения SpawnCtx — UAF:
    goready(co) пишет 32-бит в _nova_park_state (off80) уже освобождённого/
    recycled SpawnCtx. Но off80 != off4 сигнатуры 1 напрямую.
(б) двойное освобождение SpawnCtx (сам завершился vs разбужен таймером vs отменён).
(в) публикация occupancy до полной инициализации scope/fail_top.
(г) NovaSleepState на стеке фибера: fiber резюмится по CLOSED и завершается,
    стек-фрейм (и st) переиспользуется, а driver ещё дёргает st->scope/st->slot.
    close_cb ставит CLOSED ПОТОМ wake — но cancel-путь (cancel_wake_all/
    goready на том же co) мог разбудить фибер РАНЬШЕ close_cb.

СЛЕДУЮЩИЙ ШАГ: дочитать _nova_sleep_via_driver хвост (4000-4130) + net.c
socket park (wedge = socket I/O, не sleep!), затем WSL-сборка + gdb watchpoint.
Wedge — это aggregator server: каждое соединение спавнит хендлер, парк на
socket-read, НЕ sleep. Проверить nova_net read/accept park-путь.
