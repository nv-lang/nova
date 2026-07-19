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

## Точный паттерн репро pos_max_fibers_concurrent (прочитан)

```
supervised(cancel: tok) {
    for _ in 0..2000 { spawn { Time.sleep(10_000) } }  // 2000 фиберов, парк на 10с
    spawn { Time.sleep(30); tok.cancel() }             // через 30мс — cancel ВСЕХ
}
```
NOVA_MAXPROCS=1 (1 воркер), NOVA_MAX_FIBERS=20000.
- Main-поток (wid<0) аллоцирует 2000 SpawnCtx через ПРЯМОЙ Boehm uncollectable(128).
- Через 30мс cancel-триггер будит все 2000 → CANCEL_SCOPE job → driver walk
  armed_sleeps → 2000 uv_close → 2000 close_cb → wake → фиберы бросают cancel →
  epilogue → free_slot + pool_release (на ВОРКЕРЕ, wid>=0 → P-local pool).
- **Spawn-цикл (main, аллокация Boehm) ПЕРЕКРЫВАЕТСЯ с cancel-штормом** (2000
  спавнов ещё идут на 30мс-отметке). Сигнатура 1 = main аллоцирует свежий
  SpawnCtx из Boehm-списка ИМЕННО в этом окне.
- «30мс-таймер будит умерший SpawnCtx» из диагноза = это sleep(30) cancel-триггера.

Разные free-list: main→Boehm; worker-release→P-local (пока не capped
NOVA_SPAWN_POOL_MAX_PER_CLASS, потом excess→Boehm). Значит связь между списками
= через cap-overflow ИЛИ через прямой UAF-write в released блок.

## WSL-окружение (готово)

gdb/clang/cargo есть. nproc, libgc-dev установлены. Пред-собранные:
`~/nova-target/release/nova` (и -185, -fix, -211sc). Источник для tsan:
`~/nova-work` (tsan_build.sh компилит mn_smoke с nova_rt из ~/nova-work).
`~/nova-211sc-work` = НЕ git (native-копия). `~/tsan_build.sh` готов.

## ПЛАН (playbook §1 урок #17: gdb ground-truth > гипотезы)

1. Синкнуть p187 nova_rt в WSL native, собрать release nova.
2. Репро pos_max_fibers ×N, поймать SIGSEGV в gdb (ASLR off).
3. **HW-watchpoint на порченый блок** — поймать WRITER (это чего 211-волна НЕ
   сделала: они поймали READ-сайты обеих сигнатур, но не WRITE-порчу).
4. От ground-truth — точный фикс владения SpawnCtx.

Wedge (реальный P1) = aggregator server, socket I/O park (net.c
_nn2_stream read/accept: тот же (scope,slot)→parked_co→goready паттерн).
pos_max_fibers = быстрый прокси того же корня.

## Репро подтверждён (WSL, baseline)

- baseline nova собран из nova-work@7513f2857 (== p187 nova_rt, байт-идентично):
  `/home/craft/nova-target/release/nova`.
- Standalone-обёртка `/home/craft/wedge187/pmf.nv` (= тело теста в `fn main`),
  `nova build --mode release -o pmf`. Env NOVA_MAXPROCS=1 NOVA_MAX_FIBERS=20000.
- **Direct-run ×20: PASS=4 FAIL=16 (80% SIGSEGV, rc=139, ТИХИЙ — без abort-текста).**
  Сильный детерминированный-ish репро.
- Live-gdb МАСКИРУЕТ (Heisenbug: ptrace-тайминг → 0/12 крашей под gdb). Перешёл
  на **core-dump post-mortem** (ulimit -c unlimited, core_pattern=core в cwd,
  sudo не нужен) — реальный тайминг сохранён. Скрипты в /home/craft/wedge187/.

## СИЛЬНЫЙ КАНДИДАТ (найден чтением, ждёт core-пруфа)

`nova_scope_pin_ctx` (fibers.h:1119-1158): `ctx_pins[]` ВСЁ ЕЩЁ
`nova_alloc_uncollectable` + на grow `nova_free_uncollectable(старый)`.
Первый alloc = `16*sizeof(void*) = 128` байт = ТОТ ЖЕ Boehm-uncollectable
128-size-class, что SpawnCtx-пул (`_nova_spawn_pool_class_size[1]=128`).
**Это ТА ЖЕ коллизия, что 211-волна починила для child_ctx — но ТОЛЬКО для
child_ctx. ctx_pins остался uncollectable.** В pos_max_fibers ctx_pins растёт
per-spawn в родителе (2000 спавнов → 16→32→...→2048 = ~8 grow, каждый
free'ит предыдущий буфер в 128-класс, откуда main тут же тянет свежий SpawnCtx).
211-волна тестировала child_ctx-фикс ИЗОЛИРОВАННО → 15/15 SIGSEGV, потому что
ctx_pins (вторая коллизия, доминирующая в ЭТОМ тесте) осталась.

ОСТОРОЖНО: коллизия могла быть red herring и для child_ctx (benign reuse, не
порча). НЕ править вслепую — сначала core-dump ground-truth: посмотреть порченый
блок + соседей + writer. Если это ctx_pins — фикс тривиален (collectable, как
child_ctx). ЕСЛИ НЕТ — искать UAF-write дальше.

## GROUND TRUTH (core-dump, symbolized -O0/-O2)

Репро-конфиг для tractable core: NOVA_MAXPROCS=1 NOVA_MAX_FIBERS=2112
NOVA_FIBER_STACK=262144 (128K зажимается floor'ом до 256K). Крах-rate ~95%.
Live-gdb МАСКИРУЕТ (0/20). Core-dump работает (pattern `core.<pid>`, ~650MB).
Символы: собрал .c теста clang'ом против nova_rt (build_dbg.sh, -O0/-O2 -g).
pmf_o0 / pmf_dbg в /home/craft/wedge187/.

**Сигнатура 2 (драйвер-поток):**
```
mco_get_user_data(co=0x54004000)   minicoro.h:1849  <- deref мусора
nova_goready(co=0x54004000)         nova_sched.h:188
nova_sched_wake(scope,slot)         nova_sched.h:423  <- co=parked_co[slot]=0x54004000
uv_run <- _nova_driver_main         driver.c:149
```
co=0x54004000 = УСЕЧЁННЫЙ указатель (реальный 0x00007720_54004000, обнулены
биты 32-63). Драйвер close_cb будит спящий фибер; parked_co[slot] держит
усечённый co.

**Сигнатура 1 (main-поток):**
```
GC_generic_malloc_uncollectable         (libgc, порченый free-list)
nova_alloc_uncollectable(size=128)      alloc_boehm.c:141
nova_spawn_pool_acquire(size=104)       runtime.c:684
nova_fn_main_impl                        pmf.c:3655 (тело for-spawn)
```
Свежий SpawnCtx-аллок на main крашится в Boehm free-list (128-uncollectable).

**ВЫВОД:** parked_co-чанк (сиг2) = COLLECTABLE nova_alloc (512Б, НЕ 128-класс);
free-list (сиг1) = uncollectable 128. РАЗНЫЕ пулы, ОДНА порча (обнуление
high-half 8-байтного указателя). ⇒ порча НЕ специфична для пула/size-class ⇒
**коллизия ctx_pins/child почти наверняка red herring** (как и вывод 211-волны);
корень = ДИКАЯ/UAF запись, попадающая в разную память. Аксессоры чанков
(nova_sched_*_at, fibers.h:679-711) корректны (чистая адресная арифметика).

Chunk-геометрия NovaSchedState (fibers.h:646-664): 4 директории по
1024 chunk-ptr, chunk=64 элем, never-realloc. parked/handle/stop_cb/parked_co
в РАЗНЫХ директориях (нет cross-offset бага).

## TSan-ПРОРЫВ (pmf_tsan, -fsanitize=thread -O1 -g)

Гонки (6 прогонов, 21 warning). Все ОДНОГО класса — **алиасинг
collectable рантайм-массивов** (разные scope, ОДИН адрес):
- `nova_scope_grow` fibers.h:760/771/781 (worker: fiber_ctx/fibers массивы)
- `nova_scope_grow_children`/`alloc_child_slot` fibers.h:1187/1221 (main: child_error)
- `_nova_park_mark_slot` nova_sched.h:321 (worker: atomic write parked_co[slot])
- `memset` в GC_generic_malloc / __tsan_memcpy (свежий блок)

Примеры (TSan-адреса 0x7fff... = Boehm mmap-heap):
1. worker `new_ctx[i]=NULL` (fiber_ctx) ↔ main `new_err[i].payload=NULL`
   (child_error) — ОДИН адрес 0x7fff9ec12228.
2. main memcpy читает OLD child_error ↔ worker atomic-write parked_co[slot]
   — ОДИН адрес 0x7fffa62ca140.

`nova_alloc`=`GC_malloc` (STW, thread-safe, `GC_set_all_interior_pointers(1)`)
⇒ два concurrent alloc НЕ вернут один блок ⇒ **collect-and-reuse ЖИВЫХ
collectable-объектов**. `GC_disable()` лечит (boehm-notes: 8/8 PASS) ⇒
корень = ПРЕЖДЕВРЕМЕННАЯ GC-СБОРКА живого рантайм-массива.

`_workers` (calloc, НЕ static) рутован через `GC_add_roots` (runtime.c:1581)
— worker scope→fibers/sched_state/parked_co достижимы. Родительский
supervised scope — stack-local в nova_fn_main_impl (main stack сканируется).
Значит один из collectable-массивов теряет корень в окне (grow-swap: старый
массив ещё копируется/используется, но уже reused; ЛИБО свежий массив до
публикации в scope-поле).

Коллизия ctx_pins/child-128 = ПОДТВЕРЖДЁННЫЙ red herring (порча в
collectable 0x7fff-mmap, не в 128-uncollectable free-list как таковом).

## ДИСКРИМИНАТОР (решающий) + ФИКС

pmf ~256KB стек / 2112 слотов, ×20-30:
| Вариант | PASS/FAIL |
|---|---|
| baseline (clean nova_rt) | 2/20, 5/30 (~85% крах) |
| GC_INITIAL_HEAP_SIZE=2G (нет сборок) | **20/20** |
| GC_DONT_GC=1 | **20/20** |
| child_error+child_ctx UNCOLLECTABLE | **30/30** |

⇒ КОРЕНЬ ОКОНЧАТЕЛЬНО: **преждевременная GC-сборка живых collectable
retention-массивов supervised-scope (`child_error[]`/`child_ctx[]`).**

**Механизм** (совпадает дословно с санкционированным паттерном кодбейза —
комментарий ctx_pins fibers.h:1123 §11.6 V2 [M-83.11-cancel-token-bound-race-2k]):
эти массивы достижимы ТОЛЬКО по цепочке `stack-scope → array`. При spawn-шторме
(2000+ спавнов) рост массива триггерит много GC-циклов; Boehm-консервативный mark
под worker-triggered STW может ПРОПУСТИТЬ связь stack-scope→array (кадр момент не
просканирован) → ещё-живой массив reclaim'ится → его 0x7fff-блок отдаётся
concurrent `nova_alloc` (worker fiber_ctx grow / parked_co chunk) → алиас двух
живых объектов → усечение high-half указателя → SIGSEGV. TSan подтвердил алиасинг
(child_error ↔ fiber_ctx ↔ parked_co, один адрес).

**История латентности:** `child_error` был collectable с Plan 173.0 (латентный
баг); `child_ctx` регрессировал в collectable в Plan 211 (a8c0a2184). Оба
вскрыты boehm-регрессией ea85229e0 (push_other_roots сузил root-охват → GC стал
реально запускаться в spawn-тяжёлых тестах, где родитель с плоским гигант-root
не собирал НИ РАЗУ). Коллизия ctx_pins/child-128 = ПОДТВЕРЖДЁННЫЙ red herring.

**ФИКС** (fibers.h, 3 точки, зеркалит ctx_pins-дисциплину):
1. `nova_scope_grow_children`: `child_error`/`child_ctx` → `nova_alloc_uncollectable`;
   старые массивы `nova_free_uncollectable` на каждом grow.
2. `nova_supervised_run_impl` teardown (рядом с free ctx_pins, СТРОГО ПОСЛЕ
   Ф.3 decision-loop 3074-3097): free финальных `child_error`/`child_ctx` →
   ноль лика (важно для долгоживущего сервера — wedge).

Уровень корректности: uncollectable-память Boehm'ом СКАНИРУЕТСЯ (msg/reason/
payload/SpawnCtx-указатели внутри остаются живы), но никогда не reclaim'ится →
цепочка stack-scope→array больше не нужна для выживания массива.

pmf_fix ×30 = **30/30 PASS** (было 5/30). Хэш C-правки — в коммите волны.

СЛЕДУЮЩИЙ ШАГ: гейты (pos_max_fibers/supervisor_stop реальные фикстуры ×10,
TSan, wedge -P80/-P200, Boehm-aggregator, Windows).
