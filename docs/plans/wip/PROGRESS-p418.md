# PROGRESS — p418-vela-orphan-drain (дефект №418)

Окно: p418-vela-orphan-drain. Модель: **sonnet**. Worktree:
`d:/Sources/nv-lang/nova-p418` (ветка `p418-vela-orphan-drain`). Воспроизведение —
WSL2 Ubuntu, клон в `~/nova-p418` (домашний каталог, НЕ `/mnt/d`), toolchain rustup
1.85.0 (см. `docs/guide/linux-build.md`).

Это остаток №403 (см. `d:/Sources/nv-lang/nova-p403/docs/plans/wip/PROGRESS-p403.md`),
отделённый как ДРУГОЙ корень — closure-ABI часть (№403) уже влита, здесь только два
Linux-only SIGSEGV, оставленных p403 не тронутыми:
`a_q3_println_debug_record` (внутри мега-CU) и `standalone/m2211_108_main_fiber_accept`.

## Шаг 1 — воспроизведение + state-dump

Мега-CU командой из `.github/workflows/nova-gate.yml`:
```
./nova-cli/target/release/nova test --positive --compile-error --timeout 300 --jobs 4 spec_tests/conformance
```
Воспроизвёл **байт-в-байт** то же, что в реестре: `PASS: 694  FAIL: 2` —
`a_q3_println_debug_record` (RUN-FAIL) и `standalone/m2211_108_main_fiber_accept`
(NEG-WRONG-STDOUT — процесс падает ДО печати `ACCEPTED`). Оба падения
**100% детерминированы** на WSL2 (5/5 и более — см. §3 ниже), НЕ флейк.

### 1.1 — `standalone/m2211_108_main_fiber_accept` — SIGSEGV в `uv_run` из atexit

`gdb -q -batch -ex run -ex bt -ex "info threads"` на прямом запуске собранного `.exe`
(4/4 прогона, идентично):
```
Thread 1 "m2211_108_main_" received signal SIGSEGV, Segmentation fault.
0x000055555557d2d0 in uv.run_pending ()
#0  0x000055555557d2d0 in uv.run_pending ()
#1  0x000055555557d3d3 in uv_run ()
#2  0x0000555555565b9e in nova_runtime_drain_orphans ()
#3  0x00007ffff7a485e1 in __run_exit_handlers (...) at ./stdlib/exit.c:118
#4  0x00007ffff7a486be in __GI_exit (...) at ./stdlib/exit.c:148
#5  __libc_start_call_main (...)
#6  __libc_start_main_impl (...)
#7  _start ()
```
`info threads` в момент падения: **ТОЛЬКО main-поток + 15 GC-marker потоков** — ни
одного `Worker-N`-потока. Это симптом: worker-пул уже ПРИСОЕДИНЁН (`nova_runtime_
shutdown()` уже отработал), и только ПОСЛЕ этого запускается `nova_runtime_drain_
orphans` через `atexit`.

### 1.2 — `a_q3_println_debug_record` (мега-CU) — SIGSEGV в `_nova_park_mark_slot`

Тот же класс, что нашёл p403 (см. его PROGRESS §«НЕ исправлено»), подтверждён СВЕЖИМ
gdb-прогоном (3/3 прогона мега-CU exe, `handle SIGPWR/SIGXCPU nostop noprint pass` —
иначе gdb ловит Boehm STW-suspend-сигналы как «падения»):
```
Thread 1 "a_q3_println_de" received signal SIGSEGV, Segmentation fault.
0x00005555556fec7a in _nova_park_mark_slot (scope=0x7fffffffe0d8, slot=0, co=0x7feff7174000)
    at compiler-codegen/nova_rt/nova_sched.h:335
335    __atomic_store_n((volatile bool*)nova_sched_parked_at(st, slot), true, __ATOMIC_SEQ_CST);
#0  _nova_park_mark_slot (...) at nova_sched.h:335
#1  nova_sched_park (scope=0x7fffffffe0d8, slot=0) at nova_sched.h:382
#2  nova_sched_park_until (..., pred=_nova_sleep_drv_state_is_closed, ...) at nova_sched.h:511
#3  _nova_sleep_via_driver (scope=0x7fffffffe0d8, slot=0, ms=50) at fibers.h:4400
#4  time_sleep_ms (ms=50) at fibers.h:4554
#5  _nova_handler_lit_3_impl_Time_sleep (...) at a_q3_println_debug_record.c:198867
#6  _nova_handler_lit_3_time_wire_sleep (nanos=50000000) at .c:198858
#7  Nova_Time_sleep (...) at .c:16171
#8  Nova_Duration_method_sleep (...) at .c:40788
#9  nova_fn_10spec_tests11conformance30run_nested_body_over_threshold () at .c:79574
#10 nova_test_Plan_173___5_2__watchdog___________________cleanup__________body_____________________overrun______1984 () at .c:141639
#11 nova_test_chunk_31 () at .c:271873
#12 nova_fn_main_impl () at .c:302848
#13 _nova_main_fiber_entry (_co=0x7feff7174000) at .c:303015
#14 _mco_main (co=0x7feff7174000) at minicoro.h:622
```
Исходник — `spec_tests/conformance/nested_shield_deadline_outer_fire_neg_v1_1.nv`,
тест «Plan 173 Ф.5.2: watchdog армится только на cleanup — долгий body не
прерывается и не overrun'ится», функция `run_nested_body_over_threshold()`:
прямой (без `spawn`/`supervised`) `(50).to_millis().sleep()` внутри `consume`-тела —
парк идёт на OWNER-слоте (§165), т.е. на `_nova_main_scope` (main-body-фибер,
D92 Правило 6 / №108).

**gdb-инспекция состояния в момент падения** (`frame 0`; `print st`, `st->capacity`,
`st->parked_chunks[0]`, `scope->sched_state`, `scope->capacity`, `scope->count`,
`nova_sched_parked_at(st,slot)`):
```
st                          = (NovaSchedState *) 0x7f3fb7fc1000
st->capacity                = 0
st->parked_chunks[0]        = (nova_bool *) 0x0
scope->sched_state          = (NovaSchedState *) 0x7f3fb7fc1000   // == st, консистентно
scope->capacity              = 16     // NovaFiberQueue.capacity (fibers[]), не sched
scope->count                 = 1
nova_sched_parked_at(st,0)  = (void*) 0x0
```
«Невозможное состояние»: `nova_sched_get_state(scope)` вернула УЖЕ существовавший
`scope->sched_state` (короткий путь `if (scope->sched_state) return ...;`,
`nova_sched.h:131`) — то есть этот `NovaSchedState` был полноценно инициализирован
РАНЬШЕ (иначе `nova_sched_get_state` сама вызвала бы `nova_sched_grow_state` и
`capacity`/`parked_chunks[0]` были бы ненулевыми СРАЗУ). Но к моменту чтения его
поля — `capacity=0`, `parked_chunks[0]=NULL` — то есть СВЕЖЕ-обнулённый блок памяти
на том же адресе. Строка 331 (`if (slot < nova_sched_cap_acq(st))`) обязана была
это отфильтровать — раз мы дошли до строки 335 с `slot=0` и `capacity` теперь `0`,
`capacity` был ненулевым в момент проверки и стал `0` мгновением позже — классическая
сигнатура **GC premature-collect**: старый (полностью инициализированный)
`NovaSchedState` был собран Boehm-GC и его память переиспользована/обнулена под
другой объект, а `scope->sched_state` остался болтающимся указателем на неё.

**Дискриминатор `GC_DONT_GC=1`** (по debugging-races.md §2.1.1 / mn-coding-
conventions.md §7 — «GC-root-loss» проверяется отключением сборки):
```
baseline (armed, без переменной): PASS=0 FAIL=5   (SIGSEGV каждый раз)
GC_DONT_GC=1:                     PASS=5 FAIL=0   (100% исчезает)
```
**Вывод: подтверждённый GC-root-loss, НЕ TSan-класса гонка** (совпадает с
mn-coding-conventions.md §7 / кейс `[M-mn-spawnctx-corruption-cancel-wake]`).

## Шаг 2 — один корень или два

**ДВА РАЗНЫХ корня**, подтверждено раздельными discriminator'ами и разными стеками
падения — ни один не маскирует другой:

### Корень A (m2211_108) — orphan-drain выполняется ПОСЛЕ evloop_close

`compiler-codegen/src/codegen/emit_c.rs::emit_main_wrapper` эмитит epilogue `main()`
в таком порядке:
```
nova_supervised_drain_main_scope(&_nova_main_scope);   // дренирует ТОЛЬКО main-scope
_nova_active_scope = NULL; _nova_active_slot = -1;
nova_runtime_shutdown();                                 // join'ит worker-пул
nova_evloop_close();                                      // uv_loop_close — loop invalid
nova_gc_shutdown();
return 0;
```
`nova_runtime_drain_orphans()` (дренаж `_nova_orphan_scope` — глобальный fire-and-
forget scope для `detach {}`, Plan 83.4.5.2) **никогда не вызывается явно** — только
через `atexit(nova_runtime_drain_orphans)` (`runtime.c:2699`, регистрируется лениво
при первом `detach`). `atexit`-обработчики выполняются `exit()`'ом ПОСЛЕ того, как
тело `main()` (включая его СОБСТВЕННЫЙ явный `nova_evloop_close()`) уже
отработало — то есть `nova_runtime_drain_orphans` → `nova_supervised_drain_main_
scope(&_nova_orphan_scope)` → `uv_run(nova_current_loop(), UV_RUN_ONCE)`
(`fibers.h:2807/2812/2820`) вызывается, когда event loop уже закрыт
(`eventloop.c::nova_evloop_close` делает `uv_loop_close` + `_evloop = NULL`).

`nova_current_loop()` (`eventloop.c:106`) возвращает **закэшированный per-thread
TLS-указатель** `_nova_current_loop`, установленный ОДИН РАЗ в `nova_evloop_init()`
и НИКОГДА не сбрасываемый при `close` — то есть возвращает БОЛТАЮЩИЙСЯ указатель
на уже разрушенный `uv_loop_t`, минуя защитную проверку `nova_evloop()`'s
`_evloop_state == 2` (та печатает «called after close» и вернула бы `NULL` — но её
здесь не вызывают). `uv_run` на разрушенном `uv_loop_t` → SIGSEGV внутри
`uv__run_pending`.

Для `m2211_108`: единственный `detach {}` теста делает `TcpStream.connect` + `close`
— сетевая операция, которой для завершения нужен живой event loop; если к моменту
возврата из `main()` она ещё не отработала (обычный race для TCP над loopback —
Linux достаточно часто НЕ успевает за то время, что уходит на `accept()`, Windows —
видимо, успевает почти всегда, отсюда «не воспроизводит»), сирота попадает точно в
это окно.

### Корень B (a_q3 / мега-CU) — `_materialize_pool` затирает main-stack GC-probe

`compiler-codegen/nova_rt/fiber_arena.c` — precise `push_other_roots` (комментарий
`[M-mn-spawnctx-corruption-cancel-wake]`, §7 mn-coding-conventions.md) пушит ТРИ
слагаемых: (а) VMA главного native-стека (по «probe»-адресу, ищется в
`/proc/self/maps` заново на каждой сборке), (б) native-стеки worker/driver-потоков,
(в) занятые слоты fiber-арены. Слагаемое (а) зависит от глобального
`_nova_main_stack_probe`, который пишет `nova_fiber_arena_set_main_stack()`
(`fiber_arena.c:1140`) **безусловно** (`__atomic_store_n`, БЕЗ проверки «уже
установлен?»).

Эта функция вызывается из ДВУХ мест:
1. `fiber_arena.c:753-756` — «bootstrap-страховка» внутри `nova_fiber_arena_init()`,
   САМА себя гейтит (`if (!probe_set && getpid()==gettid()) set_main_stack();`) —
   срабатывает на ПЕРВОМ `mco_create` главного потока (в терминах D92/№108 это —
   создание фибера самого `main-body`, `nova_fiber_spawn_into(&_nova_main_scope,
   _nova_main_fiber_entry, ...)` в `emit_main_wrapper`, безусловно первым делом в
   каждой программе) — этот вызов гарантированно происходит НА НАСТОЯЩЕМ native-
   стеке (main-body ещё не resume'нут ни разу).
2. `runtime.c:1752` — `_materialize_pool()` (материализация worker-пула, лениво на
   ПЕРВОМ worker-bound spawn) вызывает `nova_fiber_arena_set_main_stack()`
   **безусловно**, с комментарием «Мы гарантированно на main... main НЕ крутит
   fiber здесь» (Plan 151, ДО №108). Это было верно ДО №108: тогда весь top-level
   код программы исполнялся прямо в C-фрейме `main()`, на настоящем стеке. **После
   №108** (`[M-bare-fiber-accept-bootstrap-park-invalid-slot]`, D92 Правило 6)
   ВЕСЬ пользовательский код (буквально все 1155 тестов мега-CU) исполняется
   ВНУТРИ `_nova_main_fiber_entry`'s фибера — то есть ПЕРВЫЙ worker-bound spawn
   (а с ним и `_materialize_pool`) теперь ГАРАНТИРОВАННО исполняется УЖЕ ВНУТРИ
   резюмированного main-body-фибера (стек — арена, `co`-диапазон), а не на
   настоящем native-стеке. Комментарий стал ложным, а вызов — безусловным
   перезаписывателем: он затирает ПРАВИЛЬНЫЙ probe (из п.1) БОГУС-адресом внутри
   fiber-арены.

С этого момента `_nova_push_main_stack_vma()` ищет в `/proc/self/maps` VMA вокруг
БОГУС-адреса — попадает в arena-mmap-регион (и так уже покрытый слагаемым (в),
избыточно), а НАСТОЯЩИЙ `[stack]` (где живёт `_nova_main_scope`, стек-локальная
переменная в `main()`) **больше никогда не пушится**. Всё, что достижимо
ТОЛЬКО через `_nova_main_scope`-на-стеке (например, `_nova_main_scope.sched_state`
→ его `NovaSchedState`), становится невидимым для GC-скана и подлежит
преждевременной сборке при следующем STW-цикле — ровно то, что показал
дискриминатор `GC_DONT_GC=1` (§1.2).

Порядок событий в мега-CU: main-body фибер создаётся (native-стек, probe #1
корректен) → `nova_supervised_run` резюмирует его → где-то в первых ~сотнях тестов
происходит ПЕРВЫЙ `spawn{}`/worker-bound вызов → `_materialize_pool()` затирает
probe (уже ВНУТРИ фибера) → далее КАЖДЫЙ GC-цикл теряет native-стек как root →
рано или поздно (детерминированно для фиксированной последовательности тестов
и allocation-driven Boehm-триггера) `_nova_main_scope.sched_state` собирается
между чтением `capacity` (проверка) и записью `parked_chunks[0][0]` — и следующий
`Time.sleep()`, парк.ующийся на `_nova_main_scope`, падает.

**Это НЕ маскирует корень A и не тот же сценарий**: A — падение ПОСЛЕ нормального
возврата из `main()`, в `atexit`, из-за незавершённого orphan-detach и закрытого
event loop; B — падение ВО ВРЕМЯ нормального исполнения тела программы, из-за
GC-root-loss специфичного для main-body-как-фибера (№108). Общее у них — оба
регрессии №108 (main-body стал реальным fiber'ом), но механизмы и точки отказа
не пересекаются.

## Шаг 3 — фикс (следующий коммит)

Fix A: явный синхронный `nova_runtime_drain_orphans()` в `emit_main_wrapper`
ПЕРЕД `nova_runtime_shutdown()`/`nova_evloop_close()` (симметрично уже
существующему объяснению «shutdown ДО evloop.close»).

Fix B: `nova_fiber_arena_set_main_stack()` — first-caller-wins (CAS
`_nova_main_stack_probe`, NULL→significant, не перезаписывать уже установленный).

(См. следующий коммит за диффом и обоснованием по нормам Vela §1/§9/§12.)
