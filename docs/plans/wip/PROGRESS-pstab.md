# PROGRESS — p-stability (класс «рантайм роняет процесс не по вине пользователя»)

Окно: p-stability. Модель: **opus** (выбрана владельцем — оценка модели рантайма
целиком, не точечная правка). Worktree: `d:/Sources/nv-lang/nova-pstab` (ветка
`p-stability`). Linux-воспроизведение — WSL2 Ubuntu, клон `~/nova-p427`
переключён на ветку `p-stability` (HEAD `67461f906`, submodule libuv на месте),
toolchain `~/.cargo` cargo, system libgc-dev/libuv1-dev.

Сводит класс, по которому шли точечные окна №418 (закрыт), №427 (корень не
найден), №431 (частично). Мандат владельца 2026-08-07: «стабильный язык,
компилятор и итоговые программы без крашей». Граница: КРАШ РАНТАЙМА (гонка/
поздняя отмена/порча стека из-за GC/SIGSEGV) — предмет; законный выход по
ошибке пользователя (Fail/panic, код 101, №436) — НЕ трогаем.

## Шаг 0 — контекст (прочитано)

PROGRESS-p418/p427/p431, debugging-races.md (§1a Go-приём), mn-coding-
conventions.md (§0 Go-референс, §7 GC-root, §10/§11 cancel/unwind). Ключевые
факты из отчётов:
- №418 закрыл ДВА корня Linux-SIGSEGV: (A) осушение сирот после закрытия
  evloop; (B) `_materialize_pool` затирал probe главного стека (после №108
  main-body сам — фибер на арене) → GC-root-loss `_nova_main_scope.sched_state`
  → SIGSEGV в `_nova_park_mark_slot`. Fix B = first-caller-wins CAS на probe.
- №427: остаток 3-5% на мега-CU с РАЗНЫМИ сигнатурами (MCO_NOT_SUSPENDED,
  §11 cancel-throw, неопознанные SIGSEGV, stack-smashing). 21 прогон — 0
  срабатываний (мало, нужно 92-154). Корень не найден.
- №431: `abort()` в пути поздней отмены заменён на управляемый `exit(1)`
  (счётчик + atexit-сводка), но «убить ТОЛЬКО фибр» требует ЯКОРЯ — не сделан.

### Гипотеза интегратора (К1): №430 = вероятный корень №427

Границы главного нативного стека у нас находятся КОСВЕННО (probe-адрес +
`/proc/self/maps` каждый GC-цикл, `fiber_arena.c::_nova_push_main_stack_vma`),
у Go — хранятся ЯВНО (per-g `stack.lo/hi`). Профиль «разные случайные
сигнатуры» типичен для «сборщик изредка не видит часть стека» — это был второй
корень №418. Дискриминатор `GC_DONT_GC=1`: чистая длинная серия под ним при
грязной без него подтверждает класс.

**Почему главный стек — единственный косвенный:** нативные стеки воркеров/
драйвера регистрируются явно (`_nova_native_stacks` через `pthread_getattr_np`);
слоты арены известны геометрически (branch «в»). Главный OS-стек особый:
`pthread_getattr_np` для main даёт rlimit-диапазон (не весь замаплен → пуш
unmapped-части = SIGSEGV в маркере), а замапленная часть растёт вниз
динамически — поэтому и читается VMA из `/proc/self/maps` каждый цикл. Это и
есть архитектурное расхождение №430.

## Среда воспроизведения (переиспользована, задокументировано для следующего окна)

Linux-репро — WSL2 Ubuntu, клон `~/nova-p427` на ветке `p-stability`
(HEAD `67461f906`), пересобран `cargo build --release --manifest-path
nova-cli/Cargo.toml` (2м02с из тёплого target). Ловушки этой машины (стоили
времени, фиксирую):
- **Фон в WSL умирает.** `nohup … &` ВНУТРИ `wsl bash` реапится при выходе
  launcher'а (даже `setsid`). Рабочий приём — отвязывать на СТОРОНЕ Git Bash:
  `nohup wsl -d Ubuntu bash -lc '…' > win.out 2>&1 &` держит wsl-сессию живой
  Windows-процессом; серия пишет результат в файл, опрашиваю файл. (Ровно
  паттерн из брифа.)
- **Git Bash транслирует `/tmp`, `/home` в Windows-пути** → `wsl cat /tmp/x`
  читает `C:\…\Temp\x`. Лечится `MSYS_NO_PATHCONV=1` перед `wsl`.
- **Наследованный из Windows `$PATH`** содержит `Program Files (x86)` —
  незакавыченные скобки ломают `export PATH=…:$PATH` в WSL bash (syntax error
  near `(`). Приём — звать cargo/nova АБСОЛЮТНЫМ путём, PATH не трогать.
- **Heredoc через `wsl bash -lc '…'` теряет квотирование** ($vars затираются) →
  скрипты писать Write-инструментом на Windows и копировать в WSL с `tr -d '\r'`.
- **`/tmp`-уборщик съедает `--keep-artifacts`** между командами (как и у p427) —
  переиспользовать мега-CU бинарь напрямую НЕ получилось; каждый прогон = полный
  `nova test` (~4+ мин на этой загруженной машине).

## Шаг 1 — дискриминатор GC_DONT_GC (в работе; серия идёт)

<!-- заполняется по завершении серий -->

## Ф.0 ИНВЕНТАРЬ автомата состояний фибра (план 250, допустим до тега)

Замер по `fibers.h`/`nova_sched.h`/`runtime.c`/`driver.c` (HEAD `67461f906`).
Ключевой вывод, ради которого инвентарь и делался: **у одного фибра ТРИ
независимых авторитетных слова состояния плюс per-slot дубль**, которые обязаны
совпадать, а согласование держится РУЧНЫМИ проверками (CAS + pre-check), не
конструкцией. Это и есть комбинаторная среда №438.

### Три слова состояния на ОДИН фибр (обязаны быть согласованы, но независимы)

| # | Слово | Где | Значения | Кто пишет | Кто читает |
|---|---|---|---|---|---|
| 1 | `mco_status(co)` | minicoro, вне наших структур | SUSPENDED/RUNNING/DEAD | `mco_resume`/`mco_yield` (сама корутина) | resume-guard (runtime.c:1341), state-dump |
| 2 | `_nova_fiber_state` | NovaSpawnCtxBase | IDLE/RUNNING/PARKED/DEAD | wake (PARKED→IDLE), resume-guard (CAS IDLE→RUNNING), gopark (RUNNING→PARKED), эпилог (→DEAD) | resume-guard, liveness-gate |
| 3 | `_nova_park_state` | NovaSpawnCtxBase | NIL/WAIT/READY/DISPATCHED | gopark (→WAIT), goready (WAIT→DISPATCHED / NIL→READY), gopark-return (→NIL) | goready, liveness-gate |

Плюс per-slot ДУБЛЬ слова 2/3 для сканеров, не имеющих `co` под рукой:

| # | Поле | Где | Роль | Дублирует |
|---|---|---|---|---|
| 4 | `parked[slot]` (`parked_chunks`) | NovaSchedState | bool «припаркован» для cancel-walk/liveness | слово 2 (PARKED) + слово 3 (WAIT) |
| 5 | `parked_co[slot]` | NovaSchedState | cancel-by-co: какой `co` реально припаркован в слоте | идентичность фибра (§5) |

### Разграничение по таблице плана 250

**АВТОМАТ (сводится в одно слово статуса, цель плана 250):**
- `_nova_fiber_state` (сл. 2), `_nova_park_state` (сл. 3), `parked[slot]` (сл. 4)
  — это ТРИ представления ОДНОГО жизненного цикла, разнесённые ради (а) resume-
  ownership, (б) gopark/goready-рукопожатия, (в) видимости сканеру-без-co. У Go
  всё это — ОДНО `gstatus` (+ `g` по указателю для goready). Кандидаты №1 на
  слияние в `nova_casgstatus`-слово.
- `cancel_requested` (scope-level bool, NovaFiberQueue) — «cancel_mask»/
  «cancel_requested» из таблицы 250. Это scope-broadcast, но доставляется КАК
  состояние фибра (throw на следующем yield). Участник автомата.
- `_nova_cancel_mask_count` (NovaSpawnCtxBase) — «cancel_mask» (глубина щита).
  Счётчик, НО гейтит переход «доставить cancel сейчас/отложить» — пограничный;
  по 250 отнесён к автомату (гейт состояния, не данные).
- `stage` (NovaSleepState) — «stage/fired/done». Автомат ресурса-таймера
  (PENDING/CLOSING/CLOSED + DRV_NEW/ARMED/FIRING/CANCEL_REQ/CLOSED). Отдельный
  мелкий автомат, живущий ПАРАЛЛЕЛЬНО автомату фибра; их рассинхрон — источник
  §11 (см. ниже).

**ДАННЫЕ (остаются раздельными — счётчики/замки/ручки, как вне gstatus у Go):**
`pending_remote`, `pending_sweeps`, `pending_driver_jobs`, `count`, `slot_lock`,
`child_lock`, `first_error_atomic` (+kind/reason/payload/tid), `pending_handle`,
`pending_stop_cb`, `ctx_pins*`, `armed_sleeps_head`, `deadline_ns`,
`bound_token`, `parked_co[slot]` (ручка-идентичность), `schedlink`,
`_drain_started`/`has_supervisor`/`_deciding` (control-флаги фазы drain).

### Невозможные-но-достижимые комбинации → сигнатуры №427

Ровно то, что вскрывает инвентарь (плановая цель Ф.0):

1. **сл.1 ≠ сл.2** (`mco_status` vs `_nova_fiber_state`): resume-guard
   (runtime.c:1341) читает `mco_status==SUSPENDED`, затем CAS `_nova_fiber_state`
   IDLE→RUNNING и `mco_resume`. Это ДВЕ отдельные атомарные операции над ДВУМЯ
   словами — окно между ними + legacy-путь `base==NULL` (CAS возвращает true
   без гварда, строки 2028-2031) даёт **`fiber resume failed (4)` /
   MCO_NOT_SUSPENDED**. Лечится слиянием resume-ownership в одно слово (переход
   «взять на исполнение» атомарен со статусом mco), НЕ добавлением ещё проверки.
2. **`cancel_requested` vs `_nova_fail_top`/сл.2**: cancel доставляется как
   throw (`nova_throw_cancel`, longjmp в `_nova_fail_top->jmp`) в момент wake;
   если fail-frame ещё не восстановлен (порядок restore относительно wake) →
   **`cancel-throw outside any supervised scope`** (§11). Это рассинхрон
   scope-флага и per-fiber кадра — в модели Go невозможен (cancel = чтение
   канала самим фибром, §раздел Go ниже). В текущей модели лечится тем, что
   переход в «cancel-pending» — значение статуса, доставляемое ТОЛЬКО из точки,
   где кадр гарантированно есть, а не флагом, читаемым в произвольный момент.
3. **`stage`(таймер) vs сл.2/3**(фибр): timer close_cb делает
   `nova_sched_wake` по (scope,slot); если фибр к этому моменту мигрировал/
   переиспользовал слот — wake не туда. Гвард — `expected_co` (сл.5) +
   `parked_co`. Это уже by-pointer идентичность (Go-like), но она РЯДОМ со
   слотовым дублем, а не вместо него.
4. **сл.4 vs сл.2/3** (`parked[slot]` устарел): cancel-walk читает
   `parked[slot]==true` уже после того, как goready перевёл фибр в DISPATCHED
   (requeue in flight) → второй dispatch → double-resume → порча арены/AV.
   Гвард — трактовать DISPATCHED как «не gone» (nova_sched.h:212). Классический
   STALE-slot (§4 конвенции) — прямое следствие дубля сл.4.

**Итог инвентаря:** сигнатуры №427 делятся на ДВА корня, и это ровно две оси
владельца:
- **MCO_NOT_SUSPENDED, §11 cancel-throw** → №438 (раздробленный автомат;
  комбинации 1-4 выше). Лечение — консолидация состояния (план 250), НЕ guard.
- **неопознанные SIGSEGV, stack-smashing** в `_nova_park_mark_slot`/park-путях →
  №430 (границы главного стека для GC вычисляются косвенно). Это НЕ автомат —
  это GC-root-visibility (§7). Проверяется дискриминатором `GC_DONT_GC=1`.

## Замер производительности (Ф.0 плана 244)

<!-- заполняется -->

## Раздел «инварианты Go против наших»

<!-- заполняется -->

## Проверки

<!-- заполняется -->
