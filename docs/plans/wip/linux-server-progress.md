<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# [M-187-docker-linux-runtime-hang] — Linux M:N server profile, чекпоинт

Worktree: `d:/Sources/nv-lang/nova-linuxsrv`, ветка `fix-linux-server-profile`.
В main НЕ мёржить. Runtime-фикс, не язык — D-амендмент не требуется.

## Слой 1 — fiber_arena guard-page mprotect storm (ПОЧИНЕНО, не провалидировано под docker)

Root cause подтверждён: каждый `mprotect(slot_base, GUARD_SIZE, PROT_NONE)`
в цикле guard-страниц (`nova_fiber_arena_init`,
`compiler-codegen/nova_rt/fiber_arena.c`) СПЛИТИТ единый mmap-регион на
guard(PROT_NONE)+usable(RW) VMA — 2 VMA/слот вместо 1. Linux ограничивает
общее число VMA процесса `/proc/sys/vm/max_map_count` (дефолт 65530).
NOVA_MAX_FIBERS дефолт 16384 × 2 VMA × N воркеров быстро выбивает лимит
(8 воркеров → 262144 VMA). Обход `NOVA_MAX_FIBERS=2048` (2048×2×8=32768 <
65530) подтвердил гипотезу.

Фикс (`compiler-codegen/nova_rt/fiber_arena.c`):
1. `_nova_vma_slot_budget()` (новая статик-функция, читает
   `/proc/sys/vm/max_map_count` через `_nova_read_max_map_count()`,
   кэш процесс-wide `_nova_maxmap_cache`) — считает безопасный потолок
   slot_count НА АРЕНУ с учётом `nova_runtime_maxprocs()+1` (воркеры +
   main), резервом `NOVA_ARENA_VMA_RESERVE=4096` под остальные VMA
   процесса и safety-фактором `NOVA_ARENA_VMA_SAFETY_PCT=75`. Если лимит
   не читается (macOS/sandboxed) → `SIZE_MAX` (без клэмпа, старое
   поведение).
2. `nova_fiber_arena_init()`: pre-clamp slot_count ДО mmap/mprotect по
   этому бюджету, warn ОДИН РАЗ на процесс (`_nova_arena_clamp_warned`,
   под мьютексом).
3. mprotect-цикл: если mprotect падает НА СЕРЕДИНЕ (после pre-clamp —
   защита от гонки с другими потоками/подсистемами), больше НЕ
   `abort()` всего процесса — усечь slot_count до того, что успело
   заматчиться, warn один раз, unmap хвост. `abort()` остаётся ТОЛЬКО
   если деградированная ёмкость падает ниже пола битмапа (64 слота) —
   там уже нет безопасного меньшего шага.
4. `#include "runtime.h"` добавлен в fiber_arena.c для
   `nova_runtime_maxprocs()` — циклической зависимости нет (runtime.h
   не инклудит fiber_arena.h).

Компиляция подтверждена: `nova build examples/flagship/aggregator/src/main.nv
--strict-effects` в WSL2 Ubuntu (нативно, вне docker) — чисто, линкуется,
бинарь запускается. **НЕ проверено**: реальное срабатывание клэмпа/деградации
под искусственно опущенным `vm.max_map_count` (нет root/sudo в этой WSL-сессии
— `sudo` требует пароль, недоступен неинтерактивно) И под docker (где сборка
ещё не пересобрана с этим фиксом).

## Слой 2 — "нежить" после bind+listen (ROOT CAUSE НАЙДЕН — АРХИТЕКТУРНЫЙ, СТОП+эскалация)

**strace недоступен** (не установлен пакет), **`sudo`/root недоступны**
(интерактивный пароль, нет пути получить в этой WSL-сессии) — но
`gdb` (пакет уже стоял) РАБОТАЕТ в режиме "launch as gdb's own child"
(`gdb --args ./aggregator`), в обход ограничения
`/proc/sys/kernel/yama/ptrace_scope=1` (запрещает attach к ЧУЖОМУ
процессу без root, но родитель-через-gdb — всегда разрешённый ptrace).
Метод: `stdbuf -o0 -e0 timeout 20 gdb -batch -ex "set pagination off"
-ex run -ex "thread apply all bt" -ex quit --args ./aggregator
> gdb_out.log 2>&1`, с отложенным `(sleep 10; curl ... ) &` в той же
команде (curl бьёт по серверу СПУСТЯ 10с — под gdb/ptrace старт
медленнее, чем нативно, 3с было недостаточно в первой попытке).

**Полный thread-dump (34 потока) в момент зависания** — сохранён (см.
git-историю этого коммита, был во временном
`~/gdb_out4.log`/`by7rfchff.txt`), ключевые находки:

1. `Thread 20 "aggregator" received signal SIGPWR, Power fail/restart.`
   — это НЕ краш пользовательского кода: **SIGPWR — сигнал, которым
   Boehm GC (`libgc.so.1`) реализует stop-the-world для потоковой
   сборки мусора** (`GC_stop_world` шлёт его каждому НЕ-текущему
   потоку, чтобы тот вошёл в свой signal-handler и запарковался до
   `GC_start_world`). gdb (через ptrace) НЕ знает, что этот сигнал
   "свой" для рантайма — по умолчанию stop+report — и поэтому
   ЗАМОРАЖИВАЕТ весь процесс (all-stop ptrace семантика), КОГДА Boehm
   ЛЕГИТИМНО делает STW-паузу. Это диагностический артефакт gdb, НЕ
   баг сам по себе — НО момент его срабатывания указывает точно, ЧТО
   происходит в реальности без отладчика в тот же момент.
2. **Thread 18** (LWP 558, единственный НЕ-idle поток) — это ИМЕННО
   fiber обработчика curl-запроса (`detach {}` из `main.nv`):
   `_mco_main` → `_nova_detach_0` →
   `nova_fn_4http9servernet17handle_connection` →
   `nova_fn_4http6server18serialize_response` →
   `Nova_WriteBuffer_method_cap__nova_int` (растит буфер до 58373
   байт) → `Vec____nova_byte_method_cap__nova_int` → `nova_alloc`
   (`alloc_boehm.c:116`) → `GC_malloc_kind_global` →
   `GC_generic_malloc` → глубже в `libgc.so.1` → **`sem_wait`** —
   поток блокирован ВНУТРИ Boehm-аллокатора, ожидая семафор (похоже на
   ожидание завершения STW-паузы, которую САМ же и вызвал ростом кучи).
3. Остальные 32 потока — все в ОЖИДАЕМЫХ idle-состояниях: 15
   `GC-marker-N` потоков (Boehm `PARALLEL_MARK`, `pthread_cond_wait` на
   семафоре разметки — норма для простоя), N `_worker_main`
   (`runtime.c:990`, `uv_run`→`epoll_pwait`, idle-поллинг), 1
   `_sysmon_main` (`runtime.c:627`, `nanosleep`), 1
   `_nova_driver_main` (`driver.c:149`, `epoll_pwait`), Thread 1 (main,
   LWP 539) — в `nova_supervised_run`→`_nova_scope_deadline_run_once`
   →`uv_run`→`uv.hrtime` (обычный busy-poll tick аксепт-луп фибера).

**Вывод**: первый РЕАЛЬНЫЙ HTTP-запрос → обработчик растит буфер
сериализации ответа (`WriteBuffer.cap()`) → аллокация достаточного
размера триггерит Boehm GC на рост кучи / коллекцию → Boehm посылает
stop-the-world (SIGPWR) остальным 33 потокам этого 34-поточного M:N
рантайма (16 воркеров + 15 GC-marker'ов + sysmon + driver + main) →
дальше ИЛИ восстановление зависает (не под gdb: ss показывал
CLOSE_WAIT/rx_queue=1 непрочитанным, PID D-state — то же самое
наблюдение из докер-волны оркестратора), ИЛИ (без gdb, в паре прогонов)
процесс тихо исчезал вскоре после — оба симптома согласуются с
"что-то в STW resume/coordination не завершается чисто" под ЭТИМ
конкретным сочетанием M:N воркер-пул + Boehm PARALLEL_MARK на Linux.
Попытка изолировать точечно (`GC_MARKERS=1`, отключает parallel-mark)
дала неоднозначный результат (инстанс умер ДО первого запроса —
похоже на port-contention от предыдущего теста, не переиграно чисто
из-за бюджета времени/сети) — гипотеза НЕ подтверждена/опровергнута
до конца, но само по себе не отменяет находку из п.1-3 выше.

**Это ФУНДАМЕНТАЛЬНАЯ M:N/Boehm-Linux архитектурная зона** (совпадает
с явно обозначенным в задании кандидат-корнем "Boehm GC_pthread-
обвязка (world-stop до старта воркеров)") — per инструкции волны:
**СТОП + доклад для эскалации**, глубокий фикс (смена suspend-сигнала/
пересмотр stop-world под M:N-пулом/отключение PARALLEL_MARK на
Linux-профиле/etc.) не предпринят в этой волне.

Родственные записи в САМОМ `main.nv` (уже задокументированные ДО этой
волны, тот же класс хрупкости, но найденные на Windows):
`[M-187-supervised-nested-fiber-slot-race]` (закрыт 83.4.5.12) и
`[M-187-high-concurrency-connection-wedge]` (митигирован
`MAX_INFLIGHT_CONNS=2`, НЕ починен) — оба про scheduler/fiber-slot
хрупкость под нагрузкой; возможно, что этот Linux GC-STW-фрииз — ТРЕТЬЕ
проявление той же общей категории (не то же самое, не переиспользовать
маркер без подтверждения).

Side-note (потеря времени в этой сессии, НЕ баг Nova, задокументировано
для следующего агента): в этой sandboxed Bash-tool-через-`wsl.exe`
цепочке **любое голое `$VAR`/`$!`/`$?` чтение переменной внутри
многошагового скрипта нестабильно евалюируется как ПУСТАЯ строка**
(проверено: `X=hello; echo "val=[$X]"` → `val=[]`; окружение репортит
"screen size is bogus"). Только ИНЛАЙН `$(команда)` работает надёжно.
Также: голый `wsl.exe -- <prog> /abs/posix/path` ломается git-bash'евым
автоматическим path-translation (`/home/craft/x` → `C:/Program
Files/Git/home/craft/x`) — всегда оборачивать в `bash -c '...'`. Также:
пайпинг долгих gdb/subprocess-команд через `| head -N` иногда давал
"exit code 9" с ПУСТЫМ выводом — обход: редиректить в файл (`> log`)
и читать файл отдельным `cat`.

## WSL-окружение (nova-work, native fs, вне git — rsync-снапшот)

- `~/nova-work` — rsync-копия `nova-linuxsrv` (exclude .git/target/
  libuv/test/libuv/docs). ВАЖНО: **worktree сам по себе НЕ чекаутит
  submodule** — `compiler-codegen/nova_rt/libuv` был пуст в
  nova-linuxsrv до `git submodule update --init
  compiler-codegen/nova_rt/libuv` (сделано в этой волне).
- `~/nova-http` — sibling path-зависимость (examples/nova.toml
  `../../nova-http`), синхронизирован с `d:/Sources/nv-lang/nova-http`
  ветка `fix-compress-dep` коммит `250f4ab` (тот же, что Dockerfile
  ожидает).
- Cargo: `CARGO_TARGET_DIR=~/nova-target`, toolchain `~/.cargo/bin`
  (rustup 1.85.0, per docs/guide/linux-build.md — дистро-rustc ICE'ит).
- Nova-бинарь: `~/nova-target/release/nova` (собран из МОЕГО
  worktree-кода, компилятор пересобирался дважды в этой волне после
  правок fiber_arena.c — линковка runtime.c C-объектов подхватывает
  правки без пересборки Rust-компилятора, только `nova build`
  перекомпилирует изменённый nova_rt/*.c).
- Аггрегатор: собран в `~/aggregator` (НЕ `/tmp` — WSL2 VM может
  рециклиться между отдельными `wsl.exe`-вызовами этой сессии, `/tmp`
  tmpfs теряется; `~/` на ext4 персистентен).
- `/proc/sys/vm/max_map_count` в ЭТОЙ WSL Ubuntu-сессии = 1048576 (уже
  поднят кем-то ДО этой волны) — слой-1 storm НЕ воспроизводится тут
  напрямую (нужен либо root для временного понижения, либо docker, где
  дефолт мог остаться 65530).

## Гейт (дословно из задания оркестратора)

1. WSL: бинарь aggregator запускается, curl / и /api/run demo → 200,
   15с idle → 200.
2. docker run образа (пересобрать с фиксом) → тот же smoke С ХОСТА +
   5 последовательных запросов + idle.
3. Windows-профиль НЕ регрессирует: loadtest.ps1 все блоки (кроме
   известной сетевой флаки health-live) зелёные.
4. Точечно `nova test std/concurrency`. Мега-CU НЕ гонять.

**Ничего из гейта ещё не пройдено.** Слой 1 — код готов, компилируется,
не провалидирован под реальным давлением на `vm.max_map_count`/docker.
Слой 2 — root cause найден (Boehm GC stop-the-world под 34-поточным M:N
рантаймом на Linux), диагностирован до состояния "фундаментальная
архитектурная зона" → **СТОП, эскалация владельцу/opus**, фикс не
предпринят в этой волне (см. раздел выше). Гейт рано гонять целиком,
пока не будет решения по слою 2.
