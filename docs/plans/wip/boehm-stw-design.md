<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# [M-187-docker-linux-runtime-hang] слой 2 — архитектурный диагноз + дизайн фикса

Статус: **ДИАГНОЗ ЗАВЕРШЁН (root cause доказан по коду), фикс НЕ компактен →
дизайн + стоп.** Реализация — отдельной волной в WSL/Linux-окружении (гейт
требует Linux-валидации, из Windows-сессии путь `fiber_arena.c` даже не
компилируется — `#if defined(__linux__) || defined(__APPLE__)`).

Автор диагноза: opus-архитектор (эскалация слоя 2). Опирается на
`docs/plans/wip/linux-server-progress.md` (gdb-дамп 34 потоков) + чтение
рантайма.

---

## 1. Вердикт

«Нежить» aggregator-сервера на первом HTTP-запросе — это **SIGSEGV внутри
mark-фазы Boehm GC**, вызванный тем, что Linux/POSIX fiber-arena отдаёт
сборщику некорректные диапазоны для консервативного сканирования. Два
независимых дефекта, оба Linux-only, оба уже КОРРЕКТНО решены на Windows
(`fiber_arena_win.c`), но НЕ портированы в POSIX-путь (`fiber_arena.c`):

- **Дефект A (доказан):** guard-страницы (`PROT_NONE`, 16 KB в начале каждого
  слота) включены в GC-root-диапазон. `GC_add_roots(base, base+high*slot_size)`
  регистрирует непрерывный кусок, начинающийся ровно с guard-страницы слота 0.
  Mark-фаза линейно читает весь root → чтение `PROT_NONE` → SIGSEGV.
- **Дефект B (сильный вторичный, требует подтверждения на Linux):** когда
  воркер крутит fiber, `RSP` указывает в fiber-стек арены, а Boehm-овский
  `stack_base` этого потока = pthread-стек воркера (зафиксирован при
  `GC_register_my_thread` в `_worker_main`, ДО первого fiber'а). Boehm
  сканирует `[RSP_на_арене, pthread_stack_base)` — диапазон, пересекающий
  немапленную дыру между аренами и pthread-стеком → SIGSEGV. На Windows это
  снято подменой `NT_TIB.StackBase/StackLimit` при переключении коро; на Linux
  аналога (`GC_set_stackbottom` на resume) НЕТ.

Гипотеза волны sonnet («STW-координация/suspend-сигнал не завершается под M:N»)
— **опровергнута** (см. §2). Дамп поймал процесс в фазе suspend (маркеры idle,
mark ещё не начат) — это диагностический артефакт gdb, перехватившего SIGPWR;
реальный фолт происходит ЧУТЬ ПОЗЖЕ, в mark-фазе, после того как все потоки
штатно запарковались.

---

## 2. Опровержение гипотезы «suspend-сигнал (SIGPWR) заблокирован / поток не
зарегистрирован»

Проверены ОБА направления, как требовало задание.

**(а) Все потоки зарегистрированы в GC.** Порядок корректный:
- `GC_allow_register_threads()` — `runtime.c:1537`, до старта любого воркера.
- Воркеры — `GC_register_my_thread` в `_worker_main` (`runtime.c:843`, под
  `NOVA_GC_THREADS_REGISTER`).
- Driver — `GC_register_my_thread` (`driver.c:141`).
- main — регистрируется автоматически внутри `GC_INIT()` (`alloc_boehm.c:92`).
- GC-marker'ы — создаёт сам Boehm, ему известны.
- `_sysmon_main` (`runtime.c:624`) — НЕ регистрируется, но это НЕ причина
  зависания: `GC_stop_world` суспендит только потоки из `GC_threads`;
  незарегистрированный sysmon не является suspend-target (он в `nanosleep`,
  кучу не трогает — для этой волны безвредно, но стоит зарегистрировать ради
  чистоты, см. §6).
- Потоки libuv-threadpool (`threadpool.c:245`, `uv_thread_create_ex`) — тоже
  незарегистрированы, тоже не suspend-target.

**(б) Suspend-сигнал НЕ заблокирован ни на одном потоке-таргете.** Греп всего
`nova_rt` на `sigmask/sigprocmask/pthread_sigmask`: в САМОМ рантайме
(`runtime.c`, `driver.c`, `fiber_arena.c`, `alloc_boehm.c`) — НОЛЬ вызовов. Все
попадания — внутри libuv, и ни одно не блокирует SIGPWR у loop-потоков:
- `epoll_pwait` (`linux.c:1451`) вызывается с `sigmask=NULL` в io_uring-пути
  (`uv__io_poll_prepare(loop, NULL, timeout)`, `linux.c:1450`); даже когда
  sigmask не NULL, он атомарно маскирует ТОЛЬКО `SIGPROF` и лишь при флаге
  `UV_LOOP_BLOCK_SIGPROF` (`linux.c:1379-1383`), который Nova не ставит.
- `core.c:1076/1082` (`uv__io_poll_prepare/_check`) — вызываются с `pset=NULL`
  в linux-пути → `pthread_sigmask` не выполняется.
- `process.c:838` — блок ВСЕХ сигналов вокруг `fork` в потоке-спавнере, окно в
  несколько инструкций, восстанавливается на `849`; к росту буфера отношения
  не имеет.
- `signal.c` — управляет только сигналами, которые прикладной код заводит через
  `uv_signal`; SIGPWR туда не попадает.

Итог: воркеры/driver/main сидят в **прерываемых** syscall'ах (`epoll_pwait`,
`nanosleep`) с разблокированным SIGPWR → штатно ловят suspend-сигнал, входят в
handler, постят ack. Дамп это и подтверждает: `Thread 20 received signal SIGPWR`
— поток НОРМАЛЬНО получает suspend. Никакого «вечного ожидания ack» нет. Ветки
(а) и (б) из задания — не подтверждаются.

`Thread 18` в `sem_wait` внутри `GC_generic_malloc` при **idle-маркерах** — это
`GC_stop_world`, ещё только суспендящий остальных (mark не стартовал, иначе
маркеры были бы в работе, а не в `pthread_cond_wait`). Без gdb эта фаза
завершается за микросекунды и управление уходит в mark — где и происходит фолт
(§3).

---

## 3. Доказанный root cause

### Дефект A — guard-страницы в GC-root-диапазоне (POSIX)

`compiler-codegen/nova_rt/fiber_arena.c`:

```c
// layout: slot i = [base + i*slot_size, base + (i+1)*slot_size)
//   guard  = [base + i*slot_size, base + i*slot_size + 16KB)  PROT_NONE
//   usable = [base + i*slot_size + 16KB, base + (i+1)*slot_size)  RW
static void _arena_register_active_range(struct NovaFiberArena* a, size_t new_high) {
    ...
    GC_add_roots(a->base, a->base + new_high * a->slot_size);   // строка 666
    ...
}
```

- `NOVA_FIBER_GUARD_SIZE = 16*1024` `PROT_NONE` в начале каждого слота
  (`fiber_arena.h:155`; `mprotect(slot_base, GUARD, PROT_NONE)`
  `fiber_arena.c:575`).
- Регистрируемый root = `[base, base + high*slot_size)` — его первые 16 KB это
  guard слота 0 (`PROT_NONE`), и внутри каждого следующего слота тоже сидит
  guard.
- Вызывается на каждом bump'е high_water при alloc'е слота
  (`fiber_arena.c:743-746`), т.е. как только заспавнен хоть один fiber.
- Boehm без incremental-mode (Nova его не включает: `alloc_boehm.c` зовёт лишь
  `GC_set_no_dls(1)`, `GC_INIT()`, `GC_set_all_interior_pointers(1)`) сканирует
  статические roots ПОЛНЫМ линейным проходом чтения, БЕЗ fault-recovery.
  Первое же чтение по `base` (guard слота 0) → SIGSEGV.

Дальше срабатывает собственный SIGSEGV-хендлер арены (`_arena_sigsegv_handler`,
`fiber_arena.c:192`), и оба наблюдавшихся симптома объясняются им:
- Если фолтит поток-владелец (у него `_t_arena != NULL`, fault_addr в
  `[base, base+virtual_size)`) → печатает ложное «fiber stack overflow in slot
  N», затем `signal(SIGSEGV, SIG_DFL); raise(SIGSEGV)` → **процесс умирает**
  («тихо исчезает»). А происходит это ВНУТРИ GC при остановленном мире → чистая
  смерть/кор.
- Если фолтит marker-поток (`_t_arena == NULL`; `_nova_find_arena_for`
  ИСКЛЮЧАЕТ guard — проверка `p >= base + GUARD`, `fiber_arena.c:113`) →
  `in_our_range == false` → делегирование в `_prev_sigsegv` (Boehm без своего
  SEGV-handler'а) → `SIG_DFL` + `raise` → тоже смерть. Взаимодействие фолта с
  остановленным миром + `SA_NODEFER` re-raise даёт наблюдавшийся **D-state
  hang** (socket в `CLOSE_WAIT`, rx_queue=1) как второй лик того же дефекта.

### Дефект B — per-thread скан стека при SP на fiber-стеке (POSIX)

- `GC_get_stack_base(&sb)` в `_worker_main` (`runtime.c:841`) фиксирует
  pthread-стек воркера (высокий адрес).
- minicoro при `mco_resume` переключает `RSP` на fiber-стек в арене (низкий,
  несвязанный адрес). На Linux НЕТ подмены Boehm-видимых границ стека.
- В mark-фазе Boehm сканирует у каждого потока `[approx_sp, stack_base)`. Для
  потока, крутящего fiber (в т.ч. САМ инициатор GC — Thread 18), это
  `[арена_RSP, pthread_stack_base)` — пересекает немапленную дыру между аренами
  и pthread-стеком (и guard-страницы других слотов) → SIGSEGV.
- На Windows снято: minicoro свопит `NT_TIB.StackBase/StackLimit`, Boehm читает
  их и сканирует ИМЕННО fiber-стек (ограниченно); плюс `native_base` пушит
  «подвешенные» scheduler-кадры (`fiber_arena_win.c:89, 256-262, 473`).

Замечание: если бы дефект B срабатывал в conformance, Linux-CI был бы весь
красный. Значит conformance-fiber-тесты либо не доживают до GC-во-время-fiber'а,
либо слишком коротки. Aggregator — первый долгоживущий M:N-сервер, где GC
гарантированно случается, пока воркер внутри fiber-обработчика растит 58 KB
буфер. **Дефект B требует подтверждения на Linux** (см. §7), но архитектурно он
реален и НЕ покрыт.

---

## 4. Почему Windows-чисто, а Linux-ломается (асимметрия)

| Аспект | Windows (`fiber_arena_win.c`) — корректно | Linux/macOS (`fiber_arena.c`) — дефектно |
|---|---|---|
| Регистрация fiber-стеков в GC | `GC_set_push_other_roots(_nova_fw_gc_push_other_roots)` (`:278`) — точный колбэк | плоский `GC_add_roots(base, base+high*slot)` (`:666`) |
| Что пушится | только `[slot_base+GUARD, slot_base+slot_size)` ЖИВЫХ слотов, `GC_push_all_eager` (`:249-250`) | весь диапазон, включая guard-страницы и мёртвые слоты |
| Guard-страницы | исключены (пуш от `+GUARD`; плюс `VirtualQuery` фильтрует `PAGE_GUARD/PAGE_NOACCESS`, `:206-208`) | ВКЛЮЧЕНЫ в root → фолт |
| SP-на-fiber при скане | `NT_TIB.StackBase/StackLimit` свопнуты minicoro → Boehm видит fiber-стек | не свопится → Boehm сканирует `[арена_SP, pthread_stack_base)` → фолт |
| Scheduler-кадры воркера | `native_base` пушится (`:256-262`) | не покрыто |
| Реакция на фолт при скане root | Windows-Boehm оборачивает mark в SEH, переживает AV | Linux-Boehm без fault-recovery → SIGSEGV фатален |

Windows-путь — это ГОТОВАЯ референс-реализация. Фикс = довести POSIX-путь до той
же модели.

---

## 5. Дизайн фикса

Принцип: **портировать точную GC-интеграцию Windows в POSIX-арену**, не
изобретая. Два под-фикса под общим маркером.

### Фикс A (обязателен, устраняет доказанный краш) — точный push_other_roots

В `fiber_arena.c` (внутри `#if defined(__linux__)||defined(__APPLE__)` и
`#ifdef NOVA_GC_BOEHM`):

1. Удалить статическую регистрацию: `_arena_register_active_range` больше не
   зовёт `GC_add_roots/GC_remove_roots`; поле-трекер `_registered_high_water`
   удаляется. Вызовы `_arena_register_active_range` в `nova_fiber_alloc`
   (`:745`) можно оставить no-op'ом или снять.
2. Добавить колбэк (аналог `_nova_fw_gc_push_other_roots`):

```c
#ifdef NOVA_GC_BOEHM
/* Mark-фаза, мир остановлен → arena-list append-only + bitmap/high_water
 * стабильны; обход без лока безопасен (симметрия с Windows). */
static void _nova_gc_push_other_roots(void) {
    for (struct NovaFiberArena* a =
             __atomic_load_n(&_nova_arena_list_head, __ATOMIC_ACQUIRE);
         a; a = a->next_arena) {
        char* base = __atomic_load_n(&a->base, __ATOMIC_ACQUIRE);
        if (!base) continue;                     /* retired */
        size_t hw = a->high_water;               /* mир остановлен — стабильно */
        for (size_t slot = 0; slot < hw; slot++) {
            uint64_t w = __atomic_load_n(&a->free_bits[slot >> 6], __ATOMIC_ACQUIRE);
            if (!((w >> (slot & 63)) & 1)) continue;   /* слот свободен */
            char* usable_lo = base + slot * a->slot_size + NOVA_FIBER_GUARD_SIZE;
            char* usable_hi = base + (slot + 1) * a->slot_size;
            GC_push_all_eager(usable_lo, usable_hi);   /* guard исключён */
        }
    }
}
#endif
```

3. Зарегистрировать колбэк ровно один раз за процесс. Уместно — рядом с
   существующими `pthread_once`-инициализаторами (`_arena_key_once` /
   `_sigsegv_once`); добавить `_gc_roots_once` и в нём
   `GC_set_push_other_roots(_nova_gc_push_other_roots)`. Вызвать в
   `nova_fiber_arena_init` до первого alloc'а слота.

Замечания к корректности:
- `GC_push_all_eager` (НЕ `GC_push_all`) — Windows-заметка (`:194-196`): плоский
  `GC_push_all` кладёт дескриптор на mark-stack и переполняет его на ~2048
  fiber'ах. Взять eager.
- `free_bits`: бит=1 → слот ЗАНЯТ (несмотря на имя; `_arena_mark_slot_used` =
  `fetch_or`, `_arena_find_free_slot` ищет по `~word`). Пушим только занятые.
- Guard других слотов и немапленный хвост арены больше НЕ читаются.
- Можно доп. отфильтровать мёртвые коро (как Windows `mco_status(co)==MCO_DEAD`,
  `:245`) — не обязательно (пуш живого-но-мёртвого слота лишь консервативно
  избыточен, не опасен).

### Фикс B (обязателен для полноты; подтвердить репро на Linux) — границы стека при switch

Инициатор GC и любой воркер, крутящий fiber, не должны заставлять Boehm
сканировать `[арена_SP, pthread_stack_base)`. Варианты (в порядке
предпочтения):

1. **`GC_set_stackbottom` на переключении коро (Windows-parity).** При
   `mco_resume` в fiber выставлять текущему GC-потоку `stack_base = вершина
   fiber-стека` (`slot_base + slot_size`), при возврате в scheduler —
   восстанавливать pthread-стек. Boehm API:
   `GC_set_stackbottom(GC_get_my_stackbottom(&old), &sb)`; на STOP-the-world
   значение читается из per-thread записи. Нужна аккуратность: значение должно
   быть согласовано на момент suspend (запись до/после switch под барьером).
   Это ТОЧНЫЙ аналог TIB-свопа Windows.
2. Пушить fiber-стеки как roots (Фикс A уже это делает для ПАРКОВАННЫХ) И
   заставить Boehm НЕ сканировать богус-диапазон бегущего потока — например,
   регистрировать/выставлять stackbottom так, чтобы `[approx_sp, stack_base)`
   схлопывался в валидный fiber-стек. Практически сводится к варианту 1.
3. Наименее инвазивно, но грубо: обернуть аллокации внутри fiber'ов в
   `GC_do_blocking`/`GC_call_with_gc_active`-дисциплину так, чтобы Boehm знал
   актуальную вершину. Требует ревизии всех точек alloc — дороже варианта 1.

Рекомендация: **вариант 1**, встроить в minicoro-обёртку resume/yield рядом с
тем местом, где Windows свопит TIB. Найти точку переключения (grep `mco_resume`
в `fibers.c`/`runtime.c`) и симметрично выставлять/снимать stackbottom только
под `NOVA_GC_BOEHM` на POSIX.

### Компактная альтернатива-стопгэп (НЕ полный фикс) — для быстрой проверки

Смена guard-страниц `PROT_NONE → PROT_READ` (одна строка на `fiber_arena.c:575`)
делает Дефект-A-диапазон читаемым (guard читается как нули → ложных корней нет;
overflow всё ещё ловится на ЗАПИСЬ). НО: (1) ослабляет guard (read-overflow
может проскочить в соседний слот до первой записи), (2) НЕ лечит Дефект B
(немапленная дыра всё равно фолтит). Использовать ТОЛЬКО как эксперимент для
подтверждения, что после устранения guard-фолта всплывает именно фолт стека
(Дефект B), не как продовый фикс.

---

## 6. Побочные улучшения (в ту же волну, дёшево)

- Зарегистрировать `_sysmon_main` в GC (`GC_register_my_thread`) для чистоты
  инварианта «все долгоживущие потоки известны сборщику», хотя он и не трогает
  кучу.
- `GC_add_roots` для массива `_workers` (`runtime.c:1581`) и per-worker scope —
  оставить как есть (это C-heap, не арена, guard'ов там нет).

---

## 7. Гейт валидации (Linux/WSL — обязателен, из Windows не воспроизводится)

Реализующая волна работает в WSL (см. `linux-server-progress.md` §WSL-окружение:
`~/nova-work`, `~/aggregator`, `~/nova-target/release/nova`, env
`NOVA_GC_LIB_DIR/INCLUDE_DIR`). Рецепт репро — там же (gdb-as-parent, отложенный
curl через 10с).

1. **Репро ДО фикса:** подтвердить SIGSEGV в mark-фазе (не только suspend). Под
   gdb: `handle SIGPWR SIGXCPU nostop noprint pass` (чтобы gdb НЕ морозил на
   штатных STW-сигналах Boehm), затем `run`, отложенный `curl /` → поймать
   `SIGSEGV` в `GC_mark*`/`_arena_sigsegv_handler` со стеком чтения guard/дыры.
   Это отделяет реальный фолт от suspend-артефакта.
2. **После Фикса A (+B):** aggregator отвечает `200` на `curl /` и `curl
   /api/run demo`; **5 запросов подряд** → все `200`; **15с idle → снова 200**
   (проверка, что GC отработал под нагрузкой без краша).
3. `GC_MARKERS=1` (off parallel-mark) и дефолт — оба зелёные (исключить, что
   выжило лишь из-за отключённого parallel-mark).
4. **Windows-регресс:** `nova test std/src/concurrency` без регресса
   (Windows-путь не трогаем — Фикс A/B строго под `defined(__linux__)||
   defined(__APPLE__)`). Мега-CU не гонять.
5. macOS использует ТОТ ЖЕ POSIX-файл — если есть доступ, прогнать smoke;
   иначе отметить, что Фикс A/B применимы и к macOS (строго более корректны, чем
   плоский root).

---

## 8. Связь со слоем 1

Слой 1 (VMA-storm от per-slot `mprotect`, уже пофикшен клэмпом бюджета) —
ортогонален, но связан: он тоже про guard-страницы. Оба живут в
`fiber_arena.c`. Фикс A/B и слой-1-клэмп совместимы; их можно валидировать в
одном прогоне aggregator под docker/WSL.

---

## 9. Резюме для реализующей волны

- Модель — `fiber_arena_win.c` (`GC_set_push_other_roots` + точный пуш usable
  живых слотов + `native_base` + TIB-своп). Портировать в `fiber_arena.c`.
- Фикс A (push_other_roots, guard исключён) — код в §5, ~40 строк, устраняет
  доказанный краш.
- Фикс B (stackbottom на switch) — Windows-parity через `GC_set_stackbottom`;
  сперва подтвердить репро (§7.1), затем внедрить.
- Строго под POSIX-гейтом; Windows не трогать.
- Гейт §7. Push запрещён без зелёного авторитетного гейта.
