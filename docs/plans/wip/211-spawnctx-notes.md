<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# [M-mn-spawnctx-corruption-cancel-wake] — чекпоинт

## ✅ КОРЕНЬ НАЙДЕН И ЗАКРЫТ (2026-07-19, opus-волна, worktree nova-187w, ветка p-spawnctx-root)

**Вердикт: «порча SpawnCtx» была СИМПТОМОМ, не корнем. Корень — потеря
GC-рутов стеков потоков на Linux: `GC_set_push_other_roots(...)` (ea85229e0,
fiber_arena.c) ЗАМЕЩАЕТ дефолтный колбэк bdwgc, который на pthreads-сборке
(`GC_default_push_other_roots` → `GC_push_all_stacks()`) — единственный
канал сканирования СТЕКОВ И РЕГИСТРОВ всех зарегистрированных потоков.
Linux-порт Windows-модели перенёс из трёх слагаемых Windows-колбэка
(fiber_arena_win.c) только занятые fiber-слоты, потеряв native-стеки
потоков и стек main. Итог: всё рутованное только стеком — stack-локальный
supervised-scope `q` и его child_error[]/child_ctx[], локали воркерского
шедулера, cancel-токен — собиралось GC ЖИВЫМ; страницы перекраивались под
другие объекты (в т.ч. carve 128Б uncollectable-freelist, из которого
шторм берёт SpawnCtx) → обе gdb-сигнатуры и рваный fail-top.**

### Доказательная матрица этой волны (изолированный pos_max, WSL, dev+release)

| Эксперимент | Результат | Вывод |
|---|---|---|
| базлайн (main HEAD) | 0/30 PASS | репро 100% |
| R1-poison+карантин SpawnCtx-пула (`NOVA_SPAWN_POOL_DIAG=1`) | 7/10 SIGSEGV, НИ ОДНОГО срабатывания | порча НЕ через manual-free пула |
| карантин ВСЕХ `nova_free_uncollectable` (`NOVA_UNCOLL_QUAR=1`) | 9/10 SIGSEGV | и не через какой-либо ручной uncollectable-free |
| mmap-вынос scope-массивов из GC-кучи + PROT_READ-ловушка старых | 10/10 PASS, ловушка писателя НИ РАЗУ не сработала | стейл-ПИСАТЕЛЯ нет; массивы должны переживать GC |
| PIN старых массивов (живы навсегда, куча GC) | 0/10 PASS | реюз СТАРЫХ массивов ни при чём |
| **PIN2: дубль-достижимость ТЕКУЩИХ массивов через uncollectable-цепь** | **10/10 PASS** | **GC теряет ЖИВЫЕ массивы → потерян рут-канал (стек)** |
| наивный чейнинг дефолтного колбэка | SIGSEGV в GC-маркере | суспенд ловит sp внутри коро-стека → диапазон [коро-sp, native-bottom] через PROT_NONE guard'ы; чейнинг «в лоб» невозможен |
| **ФИКС: оверрайд + полная компенсация (см. ниже)** | **30/30 PASS (dev) + 30/30 PASS (release)** | закрыто |

Расшифровка сигнатур: «32-битное усечение» указателей — легитимные
int32-записи рантайма (`published=true/false`, `decided`, NULL-init) в СВОЙ
ЖЕ преждевременно отобранный массив, чья страница уже перекроена под
free-list/чужие объекты; ASCII «old (cle» в SpawnCtx — страница ушла под
строковые данные. gdb dev-сборки поймал и третью форму напрямую:
`base->_nova_init_snapshot = 0x00000000f7d3baf0` (верхняя половина
обнулена) и `child_ctx[i] = 0x7fff00000001` (нижняя затёрта int32 `1`).

### Фикс (fiber_arena.c) — порт ПОЛНОЙ Windows-модели

`_nova_gc_push_other_roots` теперь пушит ТРИ слагаемых (как
`_nova_fw_gc_push_other_roots` на Windows с Plan 151/Ф.2):
1. **main-стек**: probe-адрес фиксируется `nova_fiber_arena_set_main_stack`
   (из `_materialize_pool`; bootstrap-страховка — в `nova_fiber_arena_init`,
   если поток главный); на каждой сборке текущая VMA main-стека берётся из
   `/proc/self/maps` (стриминговый разбор без аллокаций — maps огромен из-за
   guard-VMA арены) и пушится целиком (`GC_push_all`).
2. **native-стеки потоков рантайма**: новый реестр
   `nova_fiber_arena_register_native_stack()` / `..._unregister_...` —
   зовётся на входе/выходе воркеров (runtime.c::_worker_main) и драйвера
   (driver.c::_nova_driver_main). NPTL маппит стек целиком → пуш полного
   диапазона безопасен (guard исключён самим pthread_getattr_np).
3. **занятые fiber-слоты арен** — как было (GC_push_all_eager).

Windows (`fiber_arena_win.c`) поведенчески не тронут (там компенсация была
полной изначально; добавлены только no-op экспорты нового API).

Диагностический инструментарий волны ОСТАВЛЕН как opt-in (ноль оверхеда
без env): R1-трипваер пула (`NOVA_SPAWN_POOL_DIAG=1` — poison+канарейка+
карантин+double-release-abort+live-проверки в goready/resume/sweep/driver)
и дискриминатор `NOVA_UNCOLL_QUAR=1` (alloc_boehm.c).

### Гейты волны (все зелёные)
- pos_max_fibers_concurrent: **30/30 PASS release + 30/30 dev** (было 0/30)
- supervisor_stop_test: **10/10 PASS** (прямые прогоны)
- supervisor_parfor_test (known-red CI): **10/10 PASS**
- `spec_tests/conformance/standalone` WSL (весь CU): **PASS 68 / FAIL 0**
- `spec_tests/conformance/standalone` Windows (фикс-бинарь, --jobs 4): **PASS 68 / FAIL 0** — регрессии нет (Windows-поведение не менялось: fiber_arena_win.c поведенчески не тронут, лишь no-op экспорты нового API).
- TSan: НЕ применим — корень не data-race (GC-root-loss; `GC_MARKERS=1` в прежних волнах не лечил — согласуется).

### Развод с [M-187-high-concurrency-connection-wedge] (параллельная ветка nova-wedge)
M-187 — WINDOWS-симптом (permanent-000, loadtest.ps1). Мой фикс POSIX-only
(fiber_arena.c); Windows-колбэк уже пушил все 3 слагаемых → GC-рутов на Windows
не терялось → корня МОЕГО типа на Windows НЕТ. Значит M-187 wedge — ОТДЕЛЬНЫЙ
баг (scheduler park/join под connection-concurrency, fibers.h — территория
nova-wedge). Развод чистый: разные слои/платформы/файлы ядра (fiber_arena.c vs
fibers.h). Единственное касание fibers.h здесь — 8 строк env-gated диагностики
в nova_scope_sweep_dead_child (НИ строки логики планировщика).

Прежние заходы ниже сохранены как история (двумя волнами sonnet до этого).

---

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

## Гейт-прогон WSL (шаг 1, ×10 изолированный pos_max_fibers_concurrent.nv)

Собран `nova` (rustup 1.85.0, WSL2 Ubuntu, worktree-копия native-fs
`~/nova-211sc-work`, submodule libuv скопирован из существующей популированной
копии другого воркчекаута — идентичен по коммиту). Изолированный прогон
(`nova test --keep-artifacts`, затем скомпилированный `.exe` напрямую ×10):
**8/10 RUN-FAIL** (не 0/10) — шаг-1-фикс (ACQUIRE на `sc->count`) НЕ озеленил
фикстуру. Прямой прогон бинаря вне test-harness — 5/5 SIGSEGV (exit=139),
gdb + core-дамп (`ulimit -c unlimited`) поймал ОБА диагностических сигнатуры
дословно как в готовом диагнозе:

1. **Сигнатура 1** (Free-list Boehm) — SIGSEGV внутри
   `GC_generic_malloc_uncollectable` ← `nova_alloc_uncollectable(size=128)` ←
   `nova_spawn_pool_acquire(size=104)` ← `nova_test_..._0()` (тело теста,
   ГЛАВНЫЙ поток, ВНУТРИ цикла `for _ in 0..2000 { spawn {...} }`, т.е. на
   СВЕЖЕМ, никогда прежде не занятом Nova'ой аллокейшне — НЕ recycled-путь).
2. **Сигнатура 2** (мусорный `_nova_fiber_scope`) — `nova_sched_cap_acq(st=
   0x656c632820646c6f)` (= ASCII «old (cle», побайтно совпадает с готовым
   диагнозом) ← `nova_goready(co=...)` ← `nova_sched_wake(scope=..., slot=81)`
   ← `_nova_driver_sleep_close_cb` (ДРАЙВЕР-поток, НОРМАЛЬНЫЙ путь — НЕ
   WRONG-FIBER-ветка, т.е. `sc->fibers[sl]==expected_co` совпало штатно; крах
   на СЛЕДУЮЩЕМ уровне — `base = mco_get_user_data(co)` САМ по себе повреждён).

**Вывод шага 1:** фикс реальный (устраняет формальный data race на
`sc->count`/`sc->fibers`), но НЕ является причиной ЭТОГО краха — сигнатура 2
проявляется через НОРМАЛЬНЫЙ (не WRONG-FIBER) путь, которого фикс не касался;
сигнатура 1 происходит на СВЕЖЕМ (не recycled) аллокейшне, вне зоны действия
фикса. Оставлен как безопасное, отдельно обоснованное улучшение (не откачен).

**Дискриминатор `GC_MARKERS=1`** (отключает Boehm parallel-mark): 6/6 SIGSEGV
— совпадает с прежней находкой (`boehm-eager-cost-notes.md`: «GC_MARKERS=1
8/8 FAIL») — крах НЕ является гонкой ВНУТРИ Boehm-маркинга, корень глубже.

## Шаг 2 — гипотеза «child_ctx[] size-class collision» (НЕ подтверждена как основная причина)

`nova_scope_grow_children` (fibers.h) аллоцировал `child_ctx[]` (массив
retained-SpawnCtx указателей, Plan 173.0 Ф.3) через `nova_alloc_uncollectable`
— обоснование в комментарии («same discipline as ctx_pins») смешивало два
разных вопроса: указатели ВНУТРИ массива (сами SpawnCtx) обязаны быть
uncollectable (это уже гарантировано независимо — они аллоцируются через
`nova_spawn_pool_acquire`), а вот САМ массив — нет (он reachable ровно пока
жив `scope`, обычной GC-scan-сканируемой памяти достаточно). При
`NOVA_SCOPE_INITIAL_CAP=16` первый grow child_ctx[] аллоцирует РОВНО
`16*sizeof(void*)=128` байт — тот же Boehm-uncollectable size-class (128),
который `nova_spawn_pool_class_size[1]` использует для SpawnCtx. Каждый
следующий grow ОСВОБОЖДАЛ (`nova_free_uncollectable` = `GC_free`) предыдущий
128-байтный буфер обратно в ТОТ ЖЕ Boehm free-list, откуда шторм из 2000
spawn'ов тут же берёт новые SpawnCtx — правдоподобный (но НЕ доказанный до
конца в рамках бюджета этой волны) механизм: свежевыданный SpawnCtx получает
память, которую МОГ (гипотетически) параллельно тронуть что-то ещё в узком
окне между free и malloc.

**Правка:** `child_ctx[]` переведён на `nova_alloc` (collectable, GC-scan) —
убирает саму коллизию size-class'ов, `nova_free_uncollectable` для него
убран (GC соберёт сам, как и `child_error[]` рядом).

**Результат:** ×15 изолированных прямых прогонов бинаря — **15/15 SIGSEGV**
(та же частота, гипотеза НЕ подтверждена как (единственная) причина). Правка
оставлена — она независимо корректна и безопасна (не добавляет риска,
устраняет неверно обоснованный uncollectable-выбор), но НЕ закрывает гейт.

## Итог волны — ЭСКАЛАЦИЯ (по критерию задания)

Два независимых, обоснованных, узких фикса (data race на `count`;
size-class collision `child_ctx[]`) НЕ озеленили фикстуру — частота осталась
на уровне бейслайна (100%→80-100%, шум). Согласно протоколу задания («Если
фикс требует архитектурной правки — СТОП + доклад с доказанным механизмом»)
и `docs/dev/debugging-races.md` §5.1 («if your first 2 attempts don't work, STOP
iterating») — дальнейшая тактическая итерация ПРЕКРАЩЕНА в этой волне.
Обе правки СОХРАНЕНЫ (реальные, доказанные микро-фиксы, не откачены), гонка
[M-mn-spawnctx-corruption-cancel-wake] остаётся ОТКРЫТОЙ — требует
доследования следующей волной (ultracode/opus разведка с TSan/canary-
инструментацией именно `nova_alloc_uncollectable`/`nova_free_uncollectable`
call-сайтов, сверка с R1/R2/R3 §EXEC докой 173.0). Полный отчёт — в финальном
сообщении агента.

## Гейты (статус на конец волны)

- [x] pos_max_fibers_concurrent ×10+15 WSL — КРАСНО (не 0/N), см. выше
- [x] supervisor_stop_test ×10 WSL (прямой прогон собранного бинаря, вне
      test-harness) — **10/10 PASS** (как и раньше — локально не
      воспроизводится, известно тайминг-зависимо от CI-профиля)
- [ ] TSan (WSL) — не прогнан (эскалация до гейта, вне бюджета волны после
      2 неудачных фикс-попыток)
- [ ] WSL aggregator gate — не прогнан (эскалация до гейта)
- [x] **Windows regression** (обязательна даже при эскалации): `cargo build
      --release --manifest-path nova-cli/Cargo.toml` (NOVA_GC_LIB_DIR=
      d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib,
      NOVA_GC_INCLUDE_DIR/NOVA_INCLUDE_DIR=.../include, libuv скопирован из
      main при пустом submodule) — чисто, 2m05s. Standalone-CU
      (`spec_tests/conformance/standalone`, --jobs 4): **69 PASS / 0 FAIL**
      (включая `pos_max_fibers_concurrent` и `supervisor_stop_test` — на
      Windows всегда были зелёными, регрессии нет). `nova test
      std/src/concurrency`: **4 PASS / 0 FAIL / 5 SKIP** (compiled-only,
      без `fn main`) — без изменений от базлайна.
- [ ] M-187-high-concurrency-wedge наблюдение — не проверялся (вне скоупа
      этой волны; для его закрытия отдельная приёмка, см. задание п.5)

**Итог: обе правки (шаг 1 + шаг 2) НЕ регрессируют Windows и не ослабляют ни
один тест; гонка [M-mn-spawnctx-corruption-cancel-wake] на Linux/WSL остаётся
ОТКРЫТОЙ (красная фикстура) — эскалация по критерию задания.**

Модель: sonnet.
