<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 211 — Park-join для nested supervised (research: остаточный race)

**Статус:** 📋 RESEARCH (заведён 2026-07-16, решение владельца: «митигация сейчас + park-join в
research-план»). НЕ в очереди реализации, пока не локализован остаточный race.
**Ветка-кандидат:** `fix-high-conc-wedge` @ worktree `nova-concwedge`, коммит `f5531fa46`
(**НЕ МЁРЖИТЬ** — вносит memory corruption ~17% под sustained-нагрузкой).
**Хронология расследования:** `docs/plans/park-join-progress.md` (в ветке; честная, шаги 1-13).
**Методология:** [docs/debugging-races.md](../debugging-races.md) — playbook M:N-race-инвестигаций
(20 уроков Plan 83.11); прецедент-кейс `docs/cases/mn-race-stale-slot-2026-05.md`.

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
   отдельный маркер `[M-fiber-arena-sigsegv-install-race]` в backlog.)
5. **Орфан/unwind-взаимодействие:** краш коррелировал с реальными deadline-fire →
   `nova_throw_scope_timeout` longjmp — проверить, не трогает ли unwind соседний scope,
   чей drive-фибер в этот момент в park-join.

## 6. Критерий готовности к merge

Изолированный sustained-репро: **0 отказов / ≥100 прогонов** (baseline-уровень надёжности) +
wedge-гейт P80/P200 + loadtest.ps1 все блоки + std/concurrency PASS + полный conformance
+ флагман-examples (конвенция 696556c86). До того — ветка живёт как research.
