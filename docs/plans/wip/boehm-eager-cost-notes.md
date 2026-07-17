<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# [fix-boehm-eager-cost] — чекпоинт (регрессия Linux после ea85229e0)

Worktree: `d:/Sources/nv-lang/nova-boehmeager`, ветка `p-fix-boehm-eager-cost`.
В main НЕ мёржить в этой волне (интегратор решает после гейтов).

---

## ОКОНЧАТЕЛЬНЫЙ КОРЕНЬ (opus-разведка, 2026-07-18) — ФИКСА В FIBER_ARENA НЕТ

**Вердикт: fiber_arena-фикс (ea85229e0) КОРРЕКТЕН. Регрессия КОСВЕННАЯ.
Первопричина — пре-существующая M:N-гонка планировщика (211-семейство,
[M-linux-mn-conformance-red]), портящая память SpawnCtx/scope, которую
удлинённые STW-паузы Boehm делают почти-детерминированной. → СТОП+ДОКЛАД
по критерию задания («корень в 211-семействе шедулера → отдельная волна»).**

### Почему родитель зелёный, а HEAD красный (разрушение парадокса)

Плоский `GC_add_roots(base, base+high_water*slot_size)` родителя регистрировал
арену как ГИГАНТСКИЙ статический root (до нескольких ГБ). Boehm учитывает
`GC_root_size` в эвристике порога сборки → огромный root задирает порог так,
что **сборка НЕ запускается ни разу** за короткий spawn-тяжёлый тест
(инструментировано: `GC_set_start_callback` → **родитель = 0 сборок**,
**HEAD = 3 сборки** при куче 266 КБ — идентичная нагрузка). push_other_roots
не регистрирует статический root → `GC_root_size` мал → сборка идёт штатно →
вскрывает латентную гонку.

### Дискриминирующие эксперименты (изолированный pos_max_fibers, чистый WSL)

| Эксперимент | Результат | Вывод |
|---|---|---|
| unfixed (ea85229e0) | 5/5 FAIL | базлайн |
| parent (a37e940c5) | 6/6 PASS | контроль |
| **parent + форс-GC** (`GC_gcollect` каждый 32-й uncoll.alloc) | **6/6 FAIL** | **баг НЕЗАВИСИМ от fiber_arena** |
| unfixed + `GC_disable()` | 8/8 PASS | сборка = триггер |
| push ВСЕХ слотов (occ+free) | 7/8 FAIL | покрытие арены ни при чём |
| non-eager (`GC_push_all`) | 5/6 FAIL | eager/non-eager ни при чём |
| **пустой push_other_roots** (арена НЕ сканируется) | **6/6 FAIL** | **краш НЕ от сканирования арены и НЕ потеря корня стека файбера** |
| worker pthread-стек как static root | 6/6 FAIL | не Дефект B (кадры воркера) |
| `GC_MARKERS=1` (парал.маркинг off) | 8/8 FAIL | не параллельный маркинг |
| колбэк инструментирован | arenas=1, hw=1284, occ=1284 | **колбэк пушит ВСЕ занятые слоты корректно** |

### Механизм краша (gdb, ASLR off)

Две сигнатуры одной порчи памяти SpawnCtx (128-байтный uncollectable класс):
1. **Free-list Boehm:** `GC_generic_malloc_uncollectable` разыменовывает
   усечённый линк `rcx=0xf7d30880` = `trunc(0x7ffff7d30880)` (биты 32-47
   `0x7fff`→0 — 32-битная запись затёрла high-half линка свободного узла).
   Всплывает, когда sweep переупорядочивает free-list (потому GC_disable
   лечит: без сборки узел не всплывает).
2. **Wake-путь:** `nova_sched_cap_acq(st=0x656c632820646c6f)` — `st` = ASCII
   «old (cle» (LE). Цепочка `_nova_driver_sleep_close_cb`(driver.c:397) →
   `nova_sched_wake(slot=1551)` → `nova_goready(co)` →
   `st=scope->sched_state`, где `scope=base->_nova_fiber_scope`, base =
   SpawnCtx(user_data). SpawnCtx повреждён → `_nova_fiber_scope`=мусор →
   `scope->sched_state`=байты строки. Это фаза cancel-wake (30мс-таймер
   разбудил файбер, дёргающий `tok.cancel()`), совпадает с abort-сигнатурой
   «cancel-throw outside any supervised scope» из заметок sonnet.

STW-пауза Boehm раздвигает окно гонки в concurrent spawn+wake+cancel-шторме
(2000 файберов, `NOVA_MAXPROCS=1`); родитель без сборок пауз не имеет.

### Судьба SP-narrowing (кандидат-1 sonnet)

Откатить. Корректность НЕ чинит (75% — таймингов­ый артефакт: узкий диапазон
→ меньше тронутой памяти → короче STW → чуть уже окно гонки, а не фикс).
Плюс вносит риск: `[usable_lo, sp)` исключает mco_coro-заголовок слота из
скана (для MCO_USE_ASM x86_64 сохранённые callee-saved лежат на стеке ≥ sp,
так что диапазон формально ок, но оптимизация оправдана лишь mark-cost'ом
eager-push, к крашу отношения не имеет). Чистый ea85229e0 fiber_arena
корректен как есть.

### Рекомендация интегратору

1. fiber_arena.c — **корректность не трогать** (ea85229e0 верен). SP-narrowing
   откатить (или оставить как чистую perf-оптимизацию eager-mark-cost — к
   регрессии не относится).
2. Настоящий фикс — отдельная волна по 211-семейству: гонка в spawn/wake/
   cancel, портящая SpawnCtx/scope. Зацепка: `nova_goready`/`nova_sched_wake`/
   `_nova_driver_sleep_close_cb`, `base->_nova_fiber_scope`. TSan-рецепт есть
   (`~/tsan_build.sh`, прецедент 211-волны).
3. Немедленная краснота CI = вскрытие реального бага, не регресс fiber_arena.
   Маскировать (возврат GC-подавления) нельзя — гонка проявится и иначе.

---

## Симптом (установлен интегратором, 4 CI-прогона)

После `ea85229e0` (Boehm-mark фикс Дефекта A — точный `push_other_roots`
вместо плоского `GC_add_roots`, см. `docs/plans/wip/boehm-stw-design.md`)
на ubuntu-CI стабильно красные:
- `spec_tests/conformance/standalone/supervisor_stop_test`
- `spec_tests/conformance/standalone/pos_max_fibers_concurrent`

## Гипотеза (подтверждена)

`_nova_gc_push_other_roots` (fiber_arena.c) пушит **весь usable-регион**
(`[slot_base+GUARD, slot_base+slot_size)`) каждого ЗАНЯТОГО слота —
O(slot_count × slot_size) на КАЖДУЮ mark-фазу, а не O(реально тронутых
байт стека). Стек растёт вниз от верха слота; типичный файбер трогает
лишь несколько KB своего 4MB (builtin default) слота — `mmap
MAP_NORESERVE` резервирует остаток, но никогда не коммитит физически, пока
не тронут. `GC_push_all_eager` на нетронутом хвосте и демандит нулевые
страницы, и жжёт mark-время пропорционально slot_size.

`pos_max_fibers_concurrent` — 2000 конкурентных ЗАПАРКОВАННЫХ файберов
(`NOVA_MAXPROCS=1`, дефолтный `NOVA_FIBER_STACK`=4MB) → eager-push трогает
≈8GB на каждую коллекцию, случающуюся, пока они все запаркованы.

## Причинность (репро WSL2 Ubuntu, 16 cores, 13GiB RAM, jobs=4)

- **HEAD (457300880, unfixed)**, `nova test --positive --compile-error
  --timeout 300 --jobs 4 spec_tests/conformance` ×10 (worktree `~/nova-work`,
  бинарь `~/nova-target/release/nova`):
  - run1: MAXF=PASS (везёт), run2/3/4: MAXF=**RUN-FAIL** (`# Running 1
    tests...` — обрывается, без TIMEOUT-маркера у этого прогона — CRASH,
    не hang; см. ниже). SUP (supervisor_stop_test) пока PASS во всех 4 — не
    так часто ловится локально, как в CI (тайминг-зависимо, ожидаемо).
  - (остаток ×10 — см. `~/repro_headunfixed.txt` при следующей проверке;
    цикл шёл в фоне на момент этого чекпоинта.)

- **Прямой запуск скомпилированного `pos_max_fibers_concurrent`-подобного
  диагностического файла** (`_scratch_gc_measure.nv` — throwaway, НЕ
  коммитится, спавнит 2000 запаркованных файберов + 3× явный
  `gc.collect()` + `gc.last_pause_ns()`) под unfixed-кодом:
  `Segmentation fault (core dumped)`, `real 0m30.303s`, `user 0m4.592s`,
  **`sys 4m30.612s`** — 9× sys/real соотношение = массовый page-fault
  storm (демандинг ~8GB нулевых страниц конкурентно 16 GC-marker'ами),
  подтверждает механизм (мусор — не логическая ошибка, а объём/латентность
  тронутой памяти). Сегфолт — вероятно то же семейство «SHADOW-ICE STW
  расширяет окно для скрытой гонки» (Дефект B design-дока, ИЛИ
  ресурс-исчерпание под WSL2 VM 13GiB при 8GB внезапного commit + 16
  параллельных mark-тредов) — точный низкоуровневый механизм крэша
  вторичен: и TIMEOUT (CI, «killed after 302893ms»), и SIGSEGV (это
  окружение) — оба симптома одного и того же корня (объём/латентность
  eager-push).

## Фикс (кандидат-1, реализован)

Файлы (только POSIX-ветка `fiber_arena.c`, Windows `fiber_arena_win.c` НЕ
тронут):

- `compiler-codegen/nova_rt/fiber_arena.h` — форвард-декларация
  `struct mco_coro;` + прототип `void*
  nova_fiber_suspended_sp(struct mco_coro*)`.
- `compiler-codegen/nova_rt/fibers.c` — реализация `nova_fiber_suspended_sp`:
  возвращает `context->ctx.rsp` (x86_64) / `.esp` (i386) / `.sp`
  (aarch64/ARM/riscv) сохранённого при `_mco_jumpout` (парковка) SP файбера,
  **только если** `mco_status(co) == MCO_SUSPENDED` (иначе `NULL` —
  RUNNING/NORMAL: сохранённый SP устарел, реальный machine SP может быть
  глубже → небезопасно сужать). Только `MCO_USE_ASM` (Linux/macOS дефолт
  для всех архитектур minicoro) — иначе `NULL`.
- `compiler-codegen/nova_rt/fiber_arena.c::_nova_gc_push_other_roots` —
  для каждого занятого слота: `sp = nova_fiber_suspended_sp(...)`; если
  безопасно (не NULL, в границах слота) — `GC_push_all_eager(sp,
  usable_hi)` (только живая часть); иначе — старое поведение (весь
  usable-регион слота, безопасный fallback).

Корректность: НИ для одного слота фикс не сужает диапазон, если это
небезопасно (RUNNING/NORMAL-файбер, чужой backend/архитектура,
out-of-bounds sp) — деградирует до старого (уже provenно-корректного)
поведения точечно для ЭТОГО слота.

## Статус гейтов (обновляется)

- [x] HEAD-unfixed ×N — частота красноты зафиксирована: `~/repro_headunfixed.txt`
      (worktree `~/nova-work`, бинарь `~/nova-target/release/nova`), 5 прогонов
      полного CU до обрыва: run1 MAXF=PASS, run2-5 MAXF=**RUN-FAIL** (4/5) —
      SUP (supervisor_stop_test) PASS во всех 5 (локально не так часто ловится,
      как в CI — тайминг-зависимо).
- [x] a37e940c5 (родитель) — **ПОДТВЕРЖДЕНО ЗЕЛЕНО**: изолированный прогон
      ТОЛЬКО `pos_max_fibers_concurrent.nv` (не полный CU — быстрее, тот же
      единственный C-файл отличается) на ЧИСТОМ (только что перезапущенном
      `wsl --shutdown`) WSL2: **8/8 PASS** (`~/nova-work` с
      `compiler-codegen/nova_rt/fiber_arena.c` подменённым на
      `git show a37e940c5:...` — единственный отличающийся nova_rt-файл
      между a37e940c5 и HEAD, проверено `git diff --stat`; backup unfixed
      версии лежит рядом как `fiber_arena.c.headfixed_backup`). Причинность
      подтверждена.
- [ ] **Fix ×8 — КРАСНО, фикс-кандидат-1 НЕ ОЗЕЛЕНЯЕТ.** Тот же изолированный
      прогон (`~/nova-work-fix`, бинарь `~/nova-target-fix/release/nova`,
      собран из этого worktree — cargo build чистый, C-код (`fiber_arena.c`/
      `fibers.c`/`fiber_arena.h`) компилируется без ошибок под clang) на ТОМ
      ЖЕ чистом WSL: **6/8 RUN-FAIL, 2/8 PASS** — фикс НЕ убирает падение
      (первая попытка сравнения была загрязнена — два параллельных ×10-цикла
      одновременно, load average 15.5/16 — передел; после `wsl --shutdown`
      + чистый повтор результат тот же: фикс красный).
- [!!!] **НОВАЯ КРИТИЧНАЯ НАХОДКА — гипотеза механизма краша была НЕВЕРНОЙ.**
      Прямой запуск скомпилированного `pos_max_fibers_concurrent`-бинаря
      (`--keep-artifacts`, `/tmp/nova_tests-*/t-*/pos_max_fibers_concurrent`)
      напрямую (не через test-harness) на СВЕЖЕМ WSL печатает ПЕРЕД крахом:
      ```
      Running 1 tests...
      nova: cancel-throw outside any supervised scope: scope cancelled
      nova: cancel-throw outside any supervised scope: scope cancelled
      nova: cancel-throw outside any supervised scope: scope cancelled
      ```
      — это `effects.h::nova_throw_cancel`/`nova_throw_cancel_reason`
      (строки ~431-466): если `_nova_fail_top == NULL` в момент cancel-throw
      (нет активного fail-frame) — **`abort()`** с этой диагностикой. Это
      **НЕ SIGSEGV из mark-фазы и НЕ OOM от объёма eager-push** — это
      структурная ГОНКА: файбер получает cancel-throw (от `tok.cancel()`,
      которым 2000 запаркованных `Time.sleep(10_000)`-файберов будятся разом)
      В МОМЕНТ, когда его `_nova_fail_top` уже НЕ установлен (fail-frame
      снят/не восстановлен) — то есть путает порядок unwind/restore вокруг
      supervised-scope при массовом одновременном пробуждении. Эта гонка,
      скорее всего, СУЩЕСТВОВАЛА и раньше (не создана моим фиксом и не
      создана самим ea85229e0's push_other_roots механизмом напрямую) — но
      **окно гонки, видимо, растягивается STW-паузой Boehm** (длиннее пауза →
      больше шанс, что 2000 wake-up'ов от `tok.cancel()` обработаются в
      "плохом" порядке относительно scope unwind) — то есть тот же корневой
      механизм («удлинённая STW пауза раздвигает окно скрытой гонки»),
      что и рабочая гипотеза оркестратора для `supervisor_stop_test`,
      просто ДРУГАЯ гонка/поверхность, не «объём тронутой памяти» как я
      предполагал в исходной гипотезе. **Мой фикс-кандидат-1 (сузить
      eager-push для запаркованных файберов) СОКРАЩАЕТ STW-паузу, но,
      видимо, НЕ ДОСТАТОЧНО** — race window остаётся открытым чаще, чем
      нужно для стабильной зелени (6/8 всё ещё красных).
      **Экспортная gdb/сигнальная диагностика точного крэша (SIGABRT vs
      SIGSEGV, реальный signal) — НЕ дособрана до обрыва сети** (частичные
      попытки запутались в race по /tmp-путям --keep-artifacts под
      параллельной нагрузкой + WSL сам перезапускался один раз посреди
      расследования — `dmesg` показал `systemd-shutdown`/journal corrupted
      messages, что добавило шума в первые интерпретации; ПОСЛЕ чистого
      `wsl --shutdown`-рестарта результат «fix красный» ПОДТВЕРДИЛСЯ
      (6/8), так что это не артефакт нестабильности WSL — гонка реальна).
- [x] WSL-гейт aggregator — НЕ ПРОГНАН в этой волне (см. эскалацию ниже,
      нет смысла гонять полный гейт до решения по causal race).
- [x] Windows standalone-CU смоук — **PASS 69/0** (fix-бинарь,
      `spec_tests/conformance/standalone`, `cargo build --release` 3m43s,
      Windows использует отдельный `fiber_arena_win.c`, не тронутый —
      подтверждает отсутствие регрессии от общих правок `fiber_arena.h`).

## Эскалация (по критерию задания)

Критерий «фикс-кандидат не озеленяет фикстуры» — **СРАБОТАЛ**. Согласно
протоколу («Не переписывай GC вслепую») — дальнейшая самостоятельная правка
GC/scheduler-логики вслепую НЕ предпринимается в этой волне. Нужно решение
по фактическому механизму гонки `_nova_fail_top`/cancel-throw при массовом
`tok.cancel()`-пробуждении (вероятно, отдельный баг в `nova_sched_wake`/
`nova_scope_cancel_wake_all`/восстановлении fail-frame при resume —
НЕ в `fiber_arena.c`) — это выходит за рамки «сузить GC push-диапазон»
и требует архитектурного решения (opus) по scheduler/cancel-пути.

Причинность к `ea85229e0` тем не менее ПОДТВЕРЖДЕНА (родитель зелёный,
HEAD-unfixed красный) — фикс просто адресует НЕ ТУ поверхность (объём
памяти вместо длительности STW per se / порядка cancel-wake). Возможные
следующие шаги (НЕ предприняты, на решение владельца/opus):
1. Найти точный race в cancel-wake/fail-frame restore (не в fiber_arena.c).
2. Либо радикальнее сократить STW (не просто узкий эager-push, а
   incremental/generational GC режим — упомянуто как "альтернатива (б)" в
   задании, GC_push_all вместо eager, тоже не пробовано).
3. Собрать точный gdb/core-dump сигнал (SIGABRT подтверждён по тексту
   diagnostics, но не через `WIFSIGNALED`/exit-code напрямую — это
   МОЖЕТ быть `abort()`, не kernel-SIGSEGV; предыдущее упоминание
   "Segmentation fault (core dumped)" в этом файле (см. выше, unfixed-код)
   было получено ДО этой находки и могло относиться к ДРУГОМУ прогону —
   не переиспользовать это число как решённое, перепроверить).

## Финальные цифры (изолированный прогон ТОЛЬКО pos_max_fibers_concurrent.nv, чистый WSL после `wsl --shutdown`)

| Вариант | N | FAIL | Частота красноты |
|---|---|---|---|
| a37e940c5 (родитель) | 8 | 0 | **0%** |
| HEAD unfixed (457300880) | 5 | 5 | **100%** |
| HEAD + фикс-кандидат-1 | 8 | 6 | **75%** |

Прямой запуск бинаря (`--keep-artifacts` + exec напрямую, вне test-harness)
подтверждает `Segmentation fault` как доминирующую сигнатуру; в части
прогонов ПЕРЕД крахом успевают напечататься 1-3 строки `nova: cancel-throw
outside any supervised scope: scope cancelled` (effects.h `abort()`-путь)
— неконсистентно (не в каждом прогоне), похоже на побочный эффект другого
файбера, наблюдающего систему уже в плохом состоянии, а не на единственную
первопричину. Наш `_arena_sigsegv_handler` (fiber_arena.c) НИ РАЗУ не
напечатал СВОЮ диагностику ("fiber stack overflow"/"SIGSEGV in fiber
arena") — то есть фолт происходит по адресу, который handler НЕ считает
"своим" (вне зарегистрированных arena-диапазонов) → делегирует в
default → обычный SIGSEGV-termination.

`supervisor_stop_test.nv` — изолированный прогон unfixed ×5: **5/5 PASS**
локально (не воспроизводится вне полного 169-файлового CU настолько же
надёжно, как в GH Actions CI — другой профиль тайминга/scheduler).
Фикс для ЭТОЙ фикстуры НЕ провалидирован (нет локального красного бейслайна
для сравнения) — только CI может подтвердить/опровергнуть.

**Вывод:** фикс-кандидат-1 даёт РЕАЛЬНОЕ, но НЕДОСТАТОЧНОЕ улучшение
(100%→75% для pos_max_fibers_concurrent) — гипотеза «уже узкий диапазон
устраняет STW-удлинение» верна лишь частично: STW всё ещё достаточно
длинна (или сам факт GC-паузы, независимо от длины, уже открывает race-
окно), чтобы race в cancel-wake/fail-frame path срабатывал в 6 из 8
прогонов. Критерий эскалации задания («фикс-кандидат не озеленяет») —
сработал. Дальше вслепую не копаю (инструкция).

Модель: sonnet.
