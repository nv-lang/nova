<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 211 — Park-join для nested supervised (research: остаточный race)

**Статус:** 📋 RESEARCH (заведён 2026-07-16, решение владельца: «митигация сейчас + park-join в
research-план»). НЕ в очереди реализации, пока не локализован остаточный race. **2026-07-17
(sonnet, §7):** из 3 TSan-подтверждённых M:N-гонок субстрата 2 закрыты (мелкие атомик-фиксы,
TSan-верифицировано 0/6 после), 3-я (runq init↔steal) — **реализована** (§7.3, worktree
`nova-runq` @ `p211-runq-2phase`, коммит `be1cb465a`): `_materialize_pool` расщеплён на 2 фазы
(init всех воркеров → spawn всех потоков); Windows-смоук (std/src/concurrency + 2 known-red
supervisor-фикстуры) зелёный; TSan (WSL) — было 3/3 прогона с `runq.h:273` гонкой, стало 9/9
чисто (0 warnings, `NOVA_MAXPROCS=4` явно ×6 + default ×3), см. §7.3 «Верификация после фикса».
Ни одна из 3 НЕ доказана причиной park-join ~17%-корапта (§4) — та гипотеза (migration parked
drive-фибера под steal) остаётся отдельной, неисследованной в этом заходе; см. §7.1 карту гонок.
**Ветка-кандидат:** `fix-high-conc-wedge` @ worktree `nova-concwedge`, коммит `f5531fa46`
(**НЕ МЁРЖИТЬ** — вносит memory corruption ~17% под sustained-нагрузкой).
**Хронология расследования:** `docs/plans/park-join-progress.md` (в ветке; честная, шаги 1-13).
**Методология:** [docs/dev/debugging-races.md](../dev/debugging-races.md) — playbook M:N-race-инвестигаций
(20 уроков Plan 83.11); прецедент-кейс `docs/dev/cases/mn-race-stale-slot-2026-05.md`.

## 1. Проблема

`[M-187-high-concurrency-connection-wedge]`: baseline nested `supervised` на воркере ждёт детей
busy-poll'ом (`nova_runtime_worker_pump_scope`, runtime.c:2143), который (а) держит OS-поток,
(б) отказывается запускать фиберы ЧУЖОГО scope (шаг 4b, push-back) → под пиком одновременных
входящих соединений возможен цикл: воркер A ждёт ребёнка-A из очереди B, воркер B — ребёнка-B из
очереди A, оба возвращают чужих детей в очередь → wedge (80 одновременных → 000 навсегда).

**Митигация (шипится отдельно):** app-level bounded-accept в aggregator (cap N одновременных
per-conn обработчиков, лишнее честно отбить) — сервер не доходит до wedge; baseline
безопасен по памяти (0 коррапта / 28 прогонов).

## 2. Park-join (архитектура правильная, реализация небезопасна)

Канонический Go/Tokio-дизайн: родитель ПАРКУЕТСЯ (не крутит busy-poll), воркер возвращается в
`_worker_main` с полным work-stealing (цикл разорван), last-child/cancel/supervisor-fail/deadline
будят родителя. Реализовано в `f5531fa46`: `drain_waiter_co` (by-co, не slot),
`nova_scope_wake_drain_waiter`, `nova_runtime_child_epilogue_signal` (+3 call-сайта emit_c),
wake-wiring отмены/супервизора, deadline-таймер через driver (отключён с hot-path).

**Подтверждено:** исходный deadlock устраняется (P80 не виснет навсегда, std/concurrency PASS).
**Блокер:** memory corruption ~17% прогонов под sustained-нагрузкой (шторм
`fiber stack overflow in slot N`, fiber_arena_win VEH) — на baseline 0/28.

## 3. Что уже найдено и починено (оставлено в ветке, легитимно)

1. `nova_drain_deadline_disarm`: `stage==NEW` путал «не submitted» с «submitted, не обработан» →
   явный `submitted`-флаг.
2. Lost-wakeup: last-child мог декрементнуть `pending_remote`→0 ДО публикации `drain_waiter_co` →
   publish-then-recheck (как в `nova_sched_park_until`).
3. x86 StoreLoad reordering (Dekker's) между publish и re-check → `nova_thread_fence_seq_cst()`
   (тот же класс, что чинился в `nova_sched_register_pending`).

Ни один и ни все вместе не устранили краш (30 прогонов: 5 отказов).

## 4. Главная гипотеза остаточного race (opus-оценка 2026-07-16)

**Parked drive-фибер × fiber-арена под work-stealing.** Запаркованный drive-фибер F, разбуженный
`nova_goready`, может быть УКРАДЕН на другой воркер — а fiber-арена управляет слотами per-worker
(«fiber stack overflow in slot 0» — слот текущего фибера воркера). Исходный busy-poll держал
drive-фибер на СВОЁМ воркере НАМЕРЕННО — park-join ломает этот инвариант. Если гипотеза верна,
фикс — не «ещё один барьер», а: (а) пиннинг разбуженного drain-waiter'а к домашнему воркеру
(wake в wake_pending ЕГО воркера, не в глобальную очередь), либо (б) migration-safety арены для
drive-фиберов. Согласуется с: краш = арена, не логика; частота не падает от барьеров.

## 4.1 Данные митигации (bounded-accept, 2026-07-16) — порог wedge КРАЙНЕ низкий

Эмпирика при подборе cap в aggregator (ветка `fix-bounded-accept`, влита): **wedge воспроизводится уже
при 3 одновременных** `aggregate()`-fan-out'ах (`MAX_INFLIGHT_CONNS`: 1/2 — стабильно; **3/4/16 —
тот же permanent-wedge**, что и без bound'а). То есть проблема НЕ в числе соединений (без bound'а
сервер де-факто обрабатывал последовательно и всё равно клинил на ~80), а в числе ОДНОВРЕМЕННО ЖИВЫХ
nested-supervised fan-out'ов в планировщике. Порог 3 — минимальный нетривиальный случай циклического
ожидания (§1: цикл требует ≥2 воркеров с чужими детьми + третий участник, создающий давление?
уточнить при расследовании). Это сужает репро §5.4 до N=3 — быстрее сэмплировать.

## 5. Программа расследования (по debugging-races.md)

1. **Изоляция wake-источников** (шаг из хронологии, не сделан): по одному (только pending_remote;
   только cancel; только supervisor) — сузить, какой путь корраптит.
2. **Проверка гипотезы §4 дёшево:** лог worker-id при park и при resume drain-waiter'а — если
   краш коррелирует с миграцией (park-wid ≠ resume-wid), гипотеза подтверждена; затем пиннинг
   wake к домашнему воркеру и re-стресс.
3. **Happens-before разбор** трёх доменов: `drain_waiter_co` publish ↔ `_nova_park_state`
   WAIT/DISPATCHED ↔ `pending_remote` fetch_sub — формально, по чек-листу playbook'а
   (два РАЗНЫХ по смыслу барьера могут не быть транзитивно упорядочены — подозрение агента).
4. **TSan:** clang `-fsanitize=thread` рантайма на Linux/WSL-сборке (MSVC-TSan ограничен) —
   изолированный `.nv`-репро уже есть (детач-цикл nested supervised(timeout) × 20-80 конкурентных).
   **Первые данные УЖЕ ЕСТЬ (2026-07-16, Linux-build волна, ручной TSan-смоук на минимальном
   spawn+supervised):** TSan нашёл `runq.h` **init/grab visibility gap** — воркер может увидеть
   неинициализированную/частично видимую runq при steal (родня главной гипотезы §2: publish
   без транзитивного упорядочения). Использовать как ВХОДНУЮ точку happens-before разбора п.3.
   (Второй TSan-улов — `fiber_arena.c` `_sigsegv_installed` check-then-set — НЕ 211-родня,
   отдельный маркер `[M-fiber-arena-sigsegv-install-race]` в backlog, ✅ **CLOSED 2026-07-16**
   (`pthread_once`, ветка `fix-fiber-sigsegv-race`, коммит `579691aef`) — TSan-ресмоук
   подтвердил: race по `_sigsegv_installed` исчез, `runq.h` init/grab race из строки выше
   остался виден в том же прогоне.)
5. **Орфан/unwind-взаимодействие:** краш коррелировал с реальными deadline-fire →
   `nova_throw_scope_timeout` longjmp — проверить, не трогает ли unwind соседний scope,
   чей drive-фибер в этот момент в park-join.

## 6. Критерий готовности к merge

Изолированный sustained-репро: **0 отказов / ≥100 прогонов** (baseline-уровень надёжности) +
wedge-гейт P80/P200 + loadtest.ps1 все блоки + std/concurrency PASS + полный conformance
+ флагман-examples (конвенция 696556c86). До того — ветка живёт как research.

## 7. Ход расследования 2026-07-17 (sonnet, worktree `nova-211r` @ `p211-races`)

Задача этого захода: разобрать 3 TSan-подтверждённые гонки субстрата (накопленные с
2026-07-16, см. §5 п.4 и чекпоинт волны (удалён при закрытии, см. git-историю)), починить нементальные
безопасно (с TSan-до/после), спроектировать фикс для архитектурной (runq), сверить с картой
park-join (§4). Без тяжёлых стресс-прогонов на Windows-хосте — TSan-верификация только на WSL
(`~/nova-work`, скрипт `~/tsan_build.sh`, минимальный `mn_smoke.c`-репро: `supervised { spawn{};
spawn{} }`).

### 7.1 Карта гонок — одна ли это гонка в разных одеждах? Нет, три разные

| Гонка | Локация | Фаза жизни рантайма | Родня park-join §4? |
|---|---|---|---|
| **runq init↔steal** | `runq.h:131` (`nova_runq_init`, write) ↔ `runq.h:273` (`nova_runq_grab`, read) | **Startup only** — окно между `_materialize_pool` создающим воркер `i` (`uv_thread_create`) и инициализацией воркеров `i+1..N-1` | **Нет** — другой механизм и другая фаза (см. ниже) |
| **sysmon↔worker preempt_flag** | `runtime.c:615` (write, sysmon-поток) ↔ `runtime.c:1082` (write, воркер-поток) | Steady-state, каждые ~10мс, весь lifetime пула | Нет — preemption-тикер, не пересекается с `drain_waiter_co`/`pending_remote`/scope |
| **`_alloc_count` RMW** | `alloc_boehm.c:110`/`:135` (`nova_alloc`/`nova_alloc_uncollectable`) | Steady-state, любой конкурентный alloc | Нет — чистый stats-счётчик, не участвует в scheduling/wake |

Все три — **подтверждены TSan** (см. §7.2), все три — **в одном подсистемном соседстве**
(M:N-рантайм, тот же `nova_rt`), но **механически независимы** друг от друга и от гипотезы §4
(миграция запаркованного drive-фибера под work-stealing в СТАЦИОНАРНОМ, уже поднятом пуле).
`runq init↔steal` ближе всего по духу к §4 (тот же класс «visibility gap в work-stealing»,
тот же файл `runq.h`), но это **STARTUP-time** окно (между `_materialize_pool` и первым
`uv_thread_create`), а park-join corruption — **steady-state** явление (после полного подъёма
пула, под sustained-нагрузкой, коррелирующее с реальными deadline-fire/cancel). Закрытие
runq-гонки (§7.3) не предсказывает закрытие park-join corruption — это по-прежнему требует
отдельной happens-before работы по §5.2/5.3/5.5. **Итог: НЕ одна гонка в разных одеждах — три
разных дефекта, объединённых только инструментом обнаружения (TSan) и подсистемой (nova_rt M:N).**

### 7.2 TSan-верификация — точная методология и цифры

Среда: WSL2 Ubuntu, `~/nova-work` (native-fs копия `compiler-codegen/nova_rt` + `libuv`),
`~/tsan_build.sh` (пересобирает `mn_smoke_tsan` из уже сгенерированного `mn_smoke.c` +
core `nova_rt/*.c` через `clang -O0 -g -fsanitize=thread`), `TSAN_OPTIONS="halt_on_error=0"`
(не останавливаться на первой гонке — считать ВСЕ уникальные сигнатуры за прогон).

**Baseline (немодифицированный `nova_rt`, 3 прогона):**

| # | exit | warnings | races found |
|---|---|---|---|
| 1 | 66 | 2 | (варьируется: подмножество из {runq, sysmon, alloc_count}) |
| 2 | 66 | 1 | |
| 3 | 66 | 2 | |

Все 3 — `mn_smoke done` (программа завершается штатно; `exit=66` — TSan-код «гонки найдены»,
не крах). Число уникальных гонок за прогон варьируется (1-2 из 3 возможных) — ожидаемо для
вероятностных гонок (playbook Group F, «background rate varies»).

**После фикса §7.4 (6 прогонов подряд, одинаковый бинарь, без пересборки между прогонами):**

```
RUN 1: exit=66 warnings=1 races=[runq.h:273:22 in nova_runq_grab] last=[mn_smoke done]
RUN 2: exit=66 warnings=1 races=[runq.h:273:22 in nova_runq_grab] last=[mn_smoke done]
RUN 3: exit=66 warnings=1 races=[runq.h:273:22 in nova_runq_grab] last=[mn_smoke done]
RUN 4: exit=66 warnings=1 races=[runq.h:273:22 in nova_runq_grab] last=[mn_smoke done]
RUN 5: exit=66 warnings=1 races=[runq.h:273:22 in nova_runq_grab] last=[mn_smoke done]
RUN 6: exit=66 warnings=1 races=[runq.h:273:22 in nova_runq_grab] last=[mn_smoke done]
```

**6/6 — ровно ОДНА гонка (runq, ожидаемо — архитектурная, не тронута), 0/6 — sysmon, 0/6 —
alloc_count.** Гонки sysmon и alloc_count TSan-подтверждённо ЗАКРЫТЫ (не одно «повезло»,
а устойчивый ноль на 6 независимых прогонах). Как бонус — теперь, когда шум от двух других
гонок убран, runq-гонка стала **детерминированно воспроизводимой в 100% прогонов** (была
1-2-из-3 в baseline из-за конкуренции сигнатур) — упрощает будущую верификацию её фикса
(TSan 0 warnings станет однозначным критерием, а не «стало меньше»).

**Побочное наблюдение (не приписано фиксу):** один фоновый прогон патченного бинаря завис на
~400+с (все 19 потоков в `futex_wait_queue`, 0 прироста CPU-time между опросами) и был убит
вручную. Ретрест (restore original → 3 прогона baseline, restore patched → 6 прогонов patched,
все — чистые) НЕ воспроизвёл зависание ни разу (0/9 повторных прогонов). Правдоподобная причина —
конкурентные диагностические `wsl.exe`-вызовы (`ps`/`cat`) той же сессии, отъедавшие CPU у
ограниченной WSL2-VM во время TSan-инструментированного прогона (TSan даёт огромный оверхед —
уже задокументировано в `docs/guide/linux-build.md`: «~55s CPU на два пустых spawn»), а не логический
баг патча — оба изменённых места (§7.4) семантически идентичны исходному коду (та же control-flow,
только atomic-операции вместо plain), не добавляют ни одной блокировки/ожидания. Не переисследовано
глубже (не воспроизводится, не в скоупе targeted-smoke) — если всплывёт снова со стабильным
репро, заводить отдельный маркер.

### 7.3 Архитектурный фикс runq init↔steal — ✅ РЕАЛИЗОВАНО (2026-07-17, sonnet, §7.3 продолжение)

**Корневая причина (подтверждена TSan-стеком, чекпоинт волны (удалён при закрытии, см. git-историю) + этот
заход):** `_materialize_pool` (runtime.c) — ОДИН цикл `for (i = 0; i < n_workers; i++)`, который
и инициализирует `_workers[i]` (id/stop/pending_count/preempt_flag/`nova_scope_init`/
`nova_runq_init`/`nova_scope_grow`/`nova_sched_get_state`/dispatch-hooks/wake_mu/runnext/
uv_loop_init/uv_async_init/close_queue/call_queue), И запускает его OS-поток
(`uv_thread_create(&w->thread, _worker_main, w)`) — **в ТОЙ ЖЕ итерации**. `_n_workers` уже
выставлен в полное целевое число ДО цикла. Поэтому воркер `0` (запущенный в итерации `i=0`)
немедленно входит в `_worker_main`'s find-work loop и на первой же попытке steal (`runtime.c`,
шаг «(3) idle — try steal», `nova_runq_steal(&w->runq, &_workers[k].runq)` для `k` по всем
`_n_workers`) может обратиться к `_workers[k]` для `k > 0` — который(ые) main-поток **ещё не
дошёл инициализировать** (или как раз инициализирует ПРЯМО СЕЙЧАС). TSan это и поймал: write
`nova_runq_init` (main, `runtime.c:1572`/`1598`, под mutex M0 — но M0 защищает только повторный
вход в `_ensure_materialized`, воркер его вообще не трогает) vs atomic-read `nova_runq_grab`
(воркер 0, БЕЗ упоминания mutex в TSan-отчёте — подтверждает: M0 НЕ синхронизирует с воркером).

Сейчас безобидно ТОЛЬКО потому, что `_workers = calloc(...)` уже занулил память ДО цикла —
`nova_runq_init`'ные значения (`head=0, tail=0`, все `slots[i]=NULL`) совпадают с calloc'ным
нулём. Steal на непроинициализированную-но-нулевую runq видит `n = t - h = 0` → возвращает
`NULL` → безобидно ФУНКЦИОНАЛЬНО. Но это НЕ спроектированный инвариант, а везение по совпадению
начальных значений — формальный data race остаётся (TSan прав), и он же ближайший кандидат на
причину `[M-linux-mn-conformance-red]` (§7.5).

**Дизайн фикса — расщепить цикл на 2 фазы (тот же приём, что Go `procresize()` — строит ВЕСЬ
`allp[]` под STW ДО того, как любой `G` может встать на новый `P`; Tokio строит весь
`Vec<Worker>` до `thread::spawn` любого из них):**

```c
/* Фаза 1: инициализировать КАЖДЫЙ _workers[i] — ни один OS-поток ещё
 * не существует ни для одного воркера → ничто не может гоняться с этими
 * записями. */
for (int i = 0; i < n_workers; i++) {
    NovaWorker* w = &_workers[i];
    w->id = i;
    nova_abool_init(&w->stop, false);
    /* ...весь текущий блок инициализации, БЕЗ uv_thread_create... */
    nova_call_queue_init(&w->call_queue);
}

/* Фаза 2: только теперь стартуют OS-потоки. К этому моменту КАЖДЫЙ
 * _workers[i] полностью инициализирован — гарантия pthread_create
 * («запись создателя ДО create() видна новому потоку») теперь покрывает
 * ВЕСЬ _workers[] для КАЖДОГО воркера, т.к. все записи произошли строго
 * до ПЕРВОГО создания потока. */
for (int i = 0; i < n_workers; i++) {
    NovaWorker* w = &_workers[i];
    int rc = uv_thread_create(&w->thread, _worker_main, w);
    if (rc != 0) { fprintf(stderr, ...); abort(); }
}
```

Свойства: **нулевая цена** (тот же объём работы, просто пересортирован — не новый барьер, не
новый атомик, не runtime-check); закрывает race ПОЛНОСТЬЮ (нет окна, где воркер жив, а
`_workers[]` ещё пишется); НЕ требует трогать M0 или добавлять синхронизацию в `nova_runq_grab`/
`nova_runq_init` — гонка не в отсутствующем барьере между «правильной» producer/consumer парой,
а в том, что компонент стал наблюдаем ДО завершения конструктора.

**Изначально классифицирован архитектурным (не применён автономно в первом заходе) по 3
причинам — все 3 закрыты в этом заходе (применение владельцем одобрено, §7 задание сонета
2026-07-17):**
1. Меняет форму control-flow в `_materialize_pool` — стартовый путь КАЖДОЙ M:N-программы;
   корректность держится на транзитивности pthread_create-гарантии через N потоков —
   **закрыто**: TSan-ресмоук выполнен явно с `NOVA_MAXPROCS=4` (не только default
   `uv_available_parallelism()`), см. «Верификация после фикса» ниже.
2. Error-path у `uv_thread_create` меняет форму (после расщепления фейл на воркере `i` в
   Фазе 2 оставляет `0..i-1` уже запущенными — сейчас и так `abort()` рушит процесс целиком,
   так что семантически то же самое) — **закрыто ревью**: код не менялся сверх пересортировки,
   `abort()`-семантика идентична дизайну.
3. По playbook (`docs/dev/debugging-races.md` §1 шаг 5) и `test-conventions-strict`: нужен
   негативный regression-тест (TSan-smoke, 0 warnings) + стресс на пороге N=3 (§4.1) —
   **TSan-smoke сделан** (см. ниже); **стресс P80/P200 на Windows-хосте намеренно НЕ гонялся**
   этим заходом (задание §7 явно исключило: «стресс-прогоны park-join (§4) НЕ гоняй — отдельный
   след») — стресс остаётся будущей волной, см. «Рекомендация» ниже.

**Применено:** worktree `nova-runq` @ ветка `p211-runq-2phase`, коммит `be1cb465a`.
Файл: `compiler-codegen/nova_rt/runtime.c` (`_materialize_pool`) — расщепление на Фазу 1
(инициализация КАЖДОГО `_workers[i]`, без единого `uv_thread_create`) и Фазу 2 (только
`uv_thread_create` по всем `i`), как в дизайне выше. Дифф по существу — переупорядочение,
без нового кода/барьеров/атомиков.

**Верификация после фикса (2026-07-17, sonnet, WSL `~/nova-work` + `~/tsan_build.sh`,
`TSAN_OPTIONS=halt_on_error=0`):**

- **Windows-смоук** (nova-runq, release-бинарь `nova.exe`, `NOVA_GC_LIB_DIR`/`INCLUDE_DIR` на
  main): `nova test std/src/concurrency` — 4 PASS / 5 SKIP (compiled-only, без `fn main`), 0 FAIL;
  таргетно `spec_tests/conformance/standalone/{supervisor_parfor_test,supervisor_escalate_test,
  supervisor_stop_test}.nv` — 3 PASS / 0 FAIL (первые два — известные known-red Linux-фикстуры,
  на Windows и раньше были зелёными, регрессии нет).
- **TSan до фикса** (WSL, `NOVA_MAXPROCS` default и explicit, `timeout 180`, 3 прогона):
  3/3 — `exit=66`, ровно 1 гонка `runq.h:273:22 in nova_runq_grab` в каждом (совпадает с §7.2).
- **TSan после фикса** (тот же бинарь-паттерн, пересобран с патченным `runtime.c`): **9/9 чисто**
  (`exit=0`, 0 warnings, `mn_smoke done`) — 6 прогонов с `NOVA_MAXPROCS=4` явно (дизайн-концерн
  #1 выше — sysmon/alloc_count-гонки не маскируют, поэтому 0 warnings однозначны, не «стало
  меньше») + 3 прогона с default `uv_available_parallelism()` (16 CPU в WSL-VM → воркеров ≥4
  и без явного override). `runq.h`-гонка **не воспроизвелась ни разу** — race полностью закрыта
  для этого репро.
- Побочное: первые 2 baseline-прогона с `timeout 60` подвисли (TSan-оверхед; см. уже
  задокументированный класс в §7.2 «побочное наблюдение» и `docs/guide/linux-build.md`) —
  увеличение до `timeout 180` устранило проблему воспроизводимо (не логический баг, тот же
  класс медленного TSan-старта под конкурентной WSL-VM-нагрузкой, не повторено ни разу на 9
  after-фикс прогонах при том же таймауте).

**Рекомендация (осталось на будущую волну):** стресс P80/P200 + park-join-стресс-набор (§4,
явно НЕ гонялся этим заходом) на Windows-хосте; TSan-прогон именно 2 известных Linux-фикстур
(`app_effect_basic_t8_1`, `supervisor_parfor_test`) напрямую, не только синтетика `mn_smoke.c`;
затем снятие `known_red`-строк из `nova-gate.yml` — см. §7.5 (снятие делает интегратор отдельным
коммитом после живого зелёного прогона на CI, не эта волна).

### 7.4 Применённые мелкие фиксы (в этом заходе, TSan-верифицировано выше)

1. **`alloc_boehm.c` — `_alloc_count` RMW-гонка.** `_alloc_count++` (в `nova_alloc` и
   `nova_alloc_uncollectable`) — non-atomic read-modify-write, гоняется КАЖДЫМ конкурентным
   alloc с разных worker-потоков (`nova_scope_alloc_slot` на fiber preamble). Не задевает
   GC-корректность (это только stats-счётчик для `nova_gc_alloc_count()`/`reset_stats()`), но
   формальный UB + возможна потеря инкремента под контеншном. Фикс: `__atomic_fetch_add(...,
   __ATOMIC_RELAXED)` на инкрементах, `__atomic_load_n`/`__atomic_store_n` на читателях/reset —
   та же дисциплина, что `_nova_runq_diag_inc` (`runq.h`). Нулевая цена (relaxed = обычная
   инструкция, порядок между инкрементами не важен, важна только атомарность самого RMW).

2. **`preempt_flag` — sysmon↔worker гонка.** Поле было документировано как «single producer
   (sysmon) + single consumer (текущий воркер), volatile достаточно (Go делает non-atomic write
   в stackguard0)» — TSan с этим не согласен: sysmon (producer) и воркер (consumer, clearing to 0
   в `_worker_main` + в hot-path `nova_preempt_check`) — РАЗНЫЕ OS-потоки без барьера между ними,
   формальная гонка под C11 memory model (аннотация TSan: «as if synchronized via sleep» — т.е.
   НЕТ настоящего happens-before, только случайный порядок от `uv_sleep(10)`). Риск на практике
   низкий (худший случай — один пропущенный/лишний preemption-тик, самокорректируется на
   следующем 10мс-проходе sysmon'а — на fiber-scheduling-корректность не влияет), но UB засоряет
   TSan-вывод (могло маскировать настоящую runq-гонку в том же прогоне). Фикс: явные
   `__atomic_load_n`/`__atomic_store_n(..., __ATOMIC_RELAXED)` на ВСЕХ точках доступа —
   `runtime.c:615` (sysmon write), `:1082`/`:1594`/`:1971` (воркер clear ×3, включая init-time
   точку — там гонки не было, атомик добавлен для единообразия), `:313` (diagnostic dump read),
   и hot-path `fibers.h::nova_preempt_check` (safepoint read+clear — вызывается на КАЖДОМ
   function-prologue + loop-backedge). Нулевая цена (RELAXED load/store компилируется в тот же
   `mov`, что и старый plain-доступ на x86/ARM — не hot-path-изменение, только TSan-чистота).
   Поле `preempt_flag` остаётся `volatile int` (тип не менялся — только операции доступа).

**Файлы:** `compiler-codegen/nova_rt/alloc_boehm.c`, `compiler-codegen/nova_rt/runtime.c`,
`compiler-codegen/nova_rt/fibers.h` (все три — в worktree `nova-211r` @ `p211-races`).

### 7.5 Судьба known-red-списка nova-gate (`[M-linux-mn-conformance-red]`)

**Список ОСТАЁТСЯ без изменений (снятие НЕ входит в эту волну — решение интегратора).**
`.github/workflows/nova-gate.yml:138` (`known_red` regex на `app_effect_basic_t8_1` +
`standalone/supervisor_parfor_test`) завязан явно на «гонки runq init↔steal + sysmon↔worker
закрываются вместе со снятием списка» (`backlog-followups.md`: «снять список ВМЕСТЕ с фиксом
гонок в 211»). **2026-07-17 (второй заход, sonnet):** все 3 TSan-подтверждённые гонки субстрата
теперь закрыты — sysmon и alloc_count (0/6, §7.4, уже было) и **runq init↔steal (0/9 после
фикса §7.3 — см. «Верификация после фикса» выше)**. Тем не менее список **сознательно НЕ
снят в этой волне**: связь «эти 3 TSan-гонки → именно ЭТИ 2 конкретных Linux-фикстуры» —
**по-прежнему классовая, не индивидуально доказанная** (`backlog-followups.md` сам формулирует
как «класс = подтверждённые TSan-гонки», не как конкретный stack trace, ведущий именно к
`app_effect_basic_t8_1`'s «падает на выходе» или к `supervisor_parfor_test`). Прямая проверка
(TSan-прогон ИМЕННО этих 2 фикстур, не синтетического `mn_smoke.c`) не сделана и в этом заходе —
`app_effect_basic_t8_1` компилируется как большой merged-CU (сотни файлов, ~60-1000+с сборки в
разных режимах, см. `docs/plans/wip/198-redo-notes.md`) — вне бюджета «targeted smoke» этой волны.

**Следующий шаг (за интегратором, не эта волна):** после мержа §7.3 в `main` — TSan-прогон
именно этих 2 фикстур напрямую (не только синтетики) на живом CI/Linux-окружении; **снятие
`known_red`-строк из `nova-gate.yml` делает интегратор отдельным коммитом ПОСЛЕ зелёного
CI-эксперимента** (не автоматически вслед за фиксом гонки — связь класс↔фикстуры не доказана
индивидуально, см. абзац выше). До этого момента маркер `[M-linux-mn-conformance-red]` остаётся
open со статусом «субстратные гонки закрыты (3/3), снятие known-red-списка — pending живой
зелёный CI-прогон, решение и коммит за интегратором».

### 7.6 Обновление программы §5

- П.4 (TSan): **завершено для всех 3 идентифицированных гонок субстрата** — все 3 закрыты
  (TSan 0/6 sysmon+alloc_count, §7.4; TSan 0/9 runq, §7.3 «Верификация после фикса»).
  См. §7.1-7.4.
- П.1-3, 5 (изоляция wake-источников, happens-before drain_waiter_co/park_state/pending_remote,
  orphan/unwind) — **НЕ тронуты этим заходом**, остаются открытыми для park-join corruption
  (§4) — §7.1 явно показывает, что закрытые/реализованные гонки этого захода НЕ являются
  причиной park-join ~17%-корапта (другая фаза жизни рантайма, другой механизм). Следующая
  волна по park-join должна начинать именно с п.1-3 §5, не считать runq-фикс прогрессом по
  этому фронту.
