<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# [M-mn-spawnctx-corruption-cancel-wake] — чекпоинт

Worktree: `d:/Sources/nv-lang/nova-211sc`, ветка `p211-spawnctx`. В main НЕ мёржить,
push не делать (решение интегратора после гейтов).

Готовый диагноз (opus-раскопка) — `docs/plans/wip/boehm-eager-cost-notes.md`
§«ОКОНЧАТЕЛЬНЫЙ КОРЕНЬ»: порча SpawnCtx (128-байтный uncollectable класс) в
шторме spawn+wake+cancel (2000 файберов, `NOVA_MAXPROCS=1`,
`spec_tests/conformance/standalone/pos_max_fibers_concurrent.nv`). Две
сигнатуры: (1) `GC_generic_malloc_uncollectable` разыменовывает усечённый
free-list-линк; (2) мусорный `base->_nova_fiber_scope` в цепочке
`_nova_driver_sleep_close_cb` → `nova_sched_wake` → `nova_goready` →
`nova_sched_cap_acq`.

## Шаг 1 — найдена точная гонка (без правки scheduler-владения)

**Файл:строка:** `compiler-codegen/nova_rt/driver.c:349` (было),
`_nova_driver_sleep_close_cb`.

**Механизм:**

```c
NovaFiberQueue* sc = st->scope;
int sl = st->slot;
mco_coro* actual_co = (sc && sl >= 0 && sl < sc->count) ? sc->fibers[sl] : NULL;
```

`sc->count` читается ПЛОСКИМ (не atomic/ACQUIRE) чтением на ДРАЙВЕР-потоке.
Одновременно WORKER-поток (единственный при `NOVA_MAXPROCS=1`) в
`nova_scope_alloc_slot` (fibers.h:995) при нехватке места вызывает
`nova_scope_grow` (fibers.h:749) — REALLOC-style свап `scope->fibers`/
`fiber_ctx`/... (плоские, НЕ atomic записи указателей на НОВЫЕ массивы),
ПОСЛЕ чего `scope->count` публикуется RELEASE-store'ом
(`__atomic_store_n(&scope->count, slot+1, __ATOMIC_RELEASE)`,
fibers.h:1093). Всё это — под `scope->slot_lock` (спинлок), который
синхронизирует ТОЛЬКО alloc_slot/free_slot между собой — НЕ с этим
непарным читателем на драйвер-потоке.

При шторме 2000 конкурентных spawn'ов (~11 удвоений capacity) без ACQUIRE
на стороне читателя нет happens-before между `nova_scope_grow`'ным
плоским свапом `fibers`-указателя и последующим RELEASE-store `count`.
Драйвер-поток может увидеть СВЕЖИЙ (после-grow) `count`, но ЕЩЁ СТАРЫЙ
(до-grow, уже отброшенный, МЕНЬШИЙ) указатель `fibers` → индексация `sl`
уходит ЗА границы старого буфера → OOB-чтение соседней heap-памяти.
Ровно это объясняет обе gdb-сигнатуры: (1) `actual_co` читается как
мусор/несовпадающий указатель → уходим в «WRONG-FIBER»-путь → там пишем
`-2`-сентинел в displaced_ctx (валидный `expected_co`, само по себе не
порча) → НО если это был ЛОЖНЫЙ срабатывание (в данном тесте НИКАКОГО
легитимного reuse-слота нет — новых spawn'ов после стартового шторма
нет), файбер помечается «displaced», хотя не является таковым → его
СОБСТВЕННЫЙ эпилог (emit_c.rs:11976, `if (_c->_nova_worker_slot >= 0)`)
пропускает `nova_scope_free_slot` → `scope->fibers[slot]` НАВСЕГДА
остаётся указывать на файбер, который вот-вот `mco_destroy`'ится —
dangling pointer, который взрывается при следующем чтении (watchdog dump,
`mco_status()` на уничтоженной корутине). GC STW-паузы (эмпирика
boehm-eager-cost-notes.md: «parent + форс-GC = 6/6 FAIL») растягивают
это окно, потому что реальные 30мс 30ms-таймера идут по wall-clock
независимо от того, сколько CPU-прогресса успел сделать WORKER (тоже
приостановленный STW) — при паузах воркер ОТСТАЁТ от роста массива
относительно wall-clock, у 30мс-таймера остаётся больше шансов
«застать» рост слотов в разгаре.

**Эталон-паттерн (уже корректно в этом же кодбейзе):**
`nova_runtime_worker_pump_scope`'s cancel-delivery (runtime.c:2145-2148,
дословный комментарий: «ACQUIRE-load on count pairs with the RELEASE-store
in nova_scope_alloc_slot, ensuring we see fibers[slot]=co when we observe
count=slot+1»). `driver.c:349` был единственным пропущенным местом — грепом
проверены ВСЕ остальные читатели `->fibers[idx]` в nova_rt: same-thread
(safe), diagnostic best-effort dump (runtime.c:316/355 — уже ACQUIRE), или
уже ACQUIRE (runtime.c:2148). `NovaSchedState`'s parked/pending_handle/
stop_cb/parked_co directories — отдельная, уже пофикшенная (Ф.1b, chunked
never-move) конструкция, не участвует.

**Правка:** `driver.c` — ACQUIRE-load `sc->count` перед индексацией
`sc->fibers[sl]` (тот же паттерн, что и в runtime.c). Ноль архитектурных
изменений — не трогает владение SpawnCtx, park/wake протокол, scheduler.

## Гейты (заполняется по ходу)

- [ ] pos_max_fibers_concurrent ×10 WSL
- [ ] supervisor_stop_test ×10 WSL
- [ ] TSan (WSL) — 0 новых предупреждений
- [ ] WSL aggregator gate (curl 200×5 + idle, GC_MARKERS=1)
- [ ] Windows: cargo build --release + standalone-CU + nova test std/src/concurrency
- [ ] M-187-high-concurrency-wedge наблюдение (не входит в гейт, только заметка)

Модель: sonnet.
