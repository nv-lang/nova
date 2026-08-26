<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Конвенция кодирования **Vela** (M:N-рантайм Nova) — проактивные правила

> **Нормативный документ.** Как ПИСАТЬ код `compiler-codegen/nova_rt/`
> (spawn / cancel / scope / driver / scheduler / GC-интеграция), чтобы гонок
> и порчи памяти НЕ ВОЗНИКАЛО. Это проактивная пара к реактивному плейбуку
> [`docs/dev/debugging-races.md`](debugging-races.md) (как ЛОВИТЬ гонку постфактум).
> Конвенция предотвращает — плейбук ловит остаток.
>
> Аудитория: любой, кто трогает M:N-ядро (`fibers.h`, `runtime.c`, `driver.c`,
> `fiber_arena.c` / `fiber_arena_win.c`, `nova_sched.h`, `alloc_boehm.c`,
> `effects.h`) или codegen concurrency-путей (`emit_c.rs` spawn/detach/supervised).
>
> **Правила НОРМАТИВНЫ** (в силе конвенций-governance, как test-conventions.md).
> Отклонение — только по согласованию с владельцем, с записью в spec/plan.
>
> Каждое правило: **НОРМА** (императив) → **ПОЧЕМУ** (какой баг ловит + ссылка
> на волну/маркер) → **ПЛОХО/ХОРОШО** (мини-C) → **ПРОВЕРКА** (трипваер/TSan/
> дискриминатор).
>
> Дистиллировано из волн: 83.11 STALE-slot + use-after-free stack-scope,
> 173.0 R1/R2/R3 substrate, 211 grow-vs-wake + child_ctx size-class,
> spawnctx-GC-root-loss (`[M-mn-spawnctx-corruption-cancel-wake]`),
> boehm-eager-cost, 187-wedge, 151 GC-premature-collect.

---

## 0. РЕФЕРЕНС АРХИТЕКТУРЫ: ранний Go (рантайм на C) — использовать ПЕРВЫМ ДЕЛОМ

**Правило владельца, подтверждённое практикой 2026-08-07:** «Go как референс для M:N уже
давал положительные фиксы в прошлом — нужно использовать». Перед тем как проектировать
собственное решение задачи планировщика, парковки, взаимодействия с GC или завершения
процесса при живых фибрах — СНАЧАЛА сверься с тем, как это устроено в раннем Go, чей рантайм
был написан на C и решал ровно этот класс задач в тех же условиях.

Что именно брать: **инварианты, а не код** (лицензия). Типовые вопросы, на которые там уже
есть проверенный ответ:
* кто и когда сообщает сборщику границы стека при переходе между стеками фибра и системы;
* как гасятся оставшиеся фибры при завершении процесса и в каком порядке относительно
  закрытия цикла событий;
* что считается точкой безопасной остановки и кто гарантирует, что состояние согласовано;
* как парковка и пробуждение разделены между планировщиком и драйвером ввода-вывода.

В отчёте окна по M:N-задаче обязателен раздел **«какие инварианты есть у референса и нет у
нас»** — даже если фикс в итоге сделан иначе. Отсутствие раздела = неполная работа.

Прецеденты, где это уже сработало: №418 (два корня — осушение сирот после закрытия цикла и
затирание границы главного стека для GC), №427 (в работе).

Аналогия по устройству: для системы типов, резолва и мономорфизации таким же эталоном служит
`rustc` — см. `feedback-rustc-as-reference` в памяти проекта. Для M:N эталон — ранний Go.


## 0.5. КРАЕУГОЛЬНОЕ ТРЕБОВАНИЕ — ОДНО АВТОРИТЕТНОЕ СЛОВО СОСТОЯНИЯ ФИБРА

**Решение владельца 2026-08-07 (корень — [№438](../plans/221.1-bug-sweep.md),
план [250](../plans/250-vela-state-consolidation.md)):** состояние фибра держится
в ОДНОМ атомарном слове статуса, по образцу `gstatus` раннего Go, а не размазано по
множеству независимых флагов.

**Почему это первично.** Замер 2026-08-07: в `fibers.h` **28** отдельных атомарных
полей состояния фибра, **257** следов починки гонок, **58%** файла — комментарии
«какую гонку эта строка закрывает». Раздробленное состояние = комбинаторное
пространство, большинство комбинаций никто не проектировал — они просто возможны, и
это среда класса «STALE-slot». У Go одно слово `gstatus` + `casgstatus` делает гонки
невозможными ПО ПОСТРОЕНИЮ, а не ловит их трипваером.

**Правило, действующее НЕМЕДЛЕННО (до всякой перестройки):**
1. Новый код Vela НЕ добавляет независимых атомарных полей состояния фибра. Нужно
   «ещё одно состояние» — это ЗНАЧЕНИЕ в существующем слове статуса (расширение
   enum переходов), а не новый флаг рядом.
2. Корректность обеспечивается АТОМАРНОЙ СМЕНОЙ статуса, а не проверкой
   согласованности нескольких полей постфактум.
3. Трипваер — крайняя мера для того, что нельзя выразить в модели, а НЕ первый
   инструмент. Каждый новый трипваер в горячем пути — сигнал, что модель
   недостаточно выражает инвариант.

**Приёмка любого M:N-фикса отвечает на вопрос: «консолидация состояния или
добавление охранника?»** Всё охранниками = корень (№438) не тронут, класс крашей
вернётся. Это записывается в отчёт окна по каждому фиксу.

**Скорость — не жертва:** один атомик вместо нескольких проверок обычно быстрее;
паритет с Go по производительности достигается ТЕМ ЖЕ движением, что и стабильность.

## 0.6. ПОЛИТИКА ЗАВЕРШЕНИЯ ПРОЦЕССА (кто, когда и КАК завершает)

Решение владельца 2026-08-07 («краши мне крайне не нравятся в любом виде»). `exit()`/`abort()`
НЕ запрещены — запрещено путать ЧЬЯ это ситуация. Матрица по источнику:

| Ситуация | Кто виноват | Как завершаемся | Механизм |
|---|---|---|---|
| Пользователь ХОЧЕТ выйти немедленно | никто, это намерение | мгновенный выход с кодом, flush буферов, БЕЗ уборки фибров | `Os.exit(code)` (как Go `os.Exit`/Rust `process::exit`); `exit(code, msg)` — builtin с сообщением |
| Необработанный `Fail` / `panic()` пользователя | пользователь | диагностика + УПРАВЛЯЕМЫЙ выход, код **101** (как Rust panic) | `nova_exit_program_error()` (№436) — НЕ `abort()` |
| Гонка рантайма (поздняя отмена без адресата) | МЫ | **НЕ завершать процесс** — свернуть только фибр, программа живёт | якорь выхода фибра (№431) + счётчик поздних отмен |
| Нарушен инвариант рантайма («этого не может быть») | наш баг | `abort()` с дампом — дамп НУЖЕН, чтобы найти свой баг | оставляем `abort()` намеренно |

**Правило для автора кода рантайма:** прежде чем писать `exit()`/`abort()`, ответь — это ЧЬЯ
ситуация по таблице? `abort()` законен ТОЛЬКО для последней строки (наш нарушенный инвариант).
Для отказа/паники пользователя — управляемый выход. Для нашей гонки — не завершать вовсе.
Ставишь `abort()` на пользовательскую ошибку или на тайминг — это дефект (см. №431/№436).

**Пользовательская сторона (в публичную доку — зона доко-сессии):** язык ДАЁТ немедленное
завершение через `Os.exit(code)`. Это не «запрещено», это штатная возможность. NB: сейчас
`Os.exit` без `import std.os` падает codegen-ошибкой `[E_CODEGEN_TYPE_UNKNOWN]` вместо подсказки
про импорт (класс №401/P67) — отдельный открытый дефект, к политике завершения отношения не
имеет.

## TL;DR — карта правил

```
ПУБЛИКАЦИЯ И ВИДИМОСТЬ           ВЛАДЕНИЕ СЛОТОМ / ИДЕНТИЧНОСТЬ
  §1 occupancy ПОСЛЕ init          §4 «слот свободен» = ТРИ условия
  §2 ACQUIRE перед index[]         §5 идентичность = указатель, не слот
  §3 write-per-slot ДО dec,        §6 без grow-during-drain
     read ПОСЛЕ acquire-zero

ЖИЗНЕННЫЙ ЦИКЛ ПАМЯТИ            CANCEL / WAKE / UNWIND
  §7 GC-root: не замещай             §10 fail-frame ДО wake-able
     дефолтный колбэк частично       §11 cancel-throw требует fail_top≠NULL
  §8 Boehm size-class: не делить
     корзину через churn
  §9 стек-указатель за границей
     потока → counter-based wait

  §12 Обязательные трипваеры (R1 poison+canary / R2 torn-base / R3 marker-clash)
```

**Главный принцип:** любое разделяемое между потоками поле имеет ДВЕ стороны —
писателя и читателя — и обе обязаны быть согласованы одним acquire/release-ребром.
Плоское чтение/запись shared-поля без явного `__atomic_*` — дефект по определению,
даже если «на x86 работает» (TSO прощает, ARM/CI — нет).

---

## Группа A — Публикация и видимость

### §1. Occupancy публикуется СТРОГО ПОСЛЕ полной инициализации слота

**НОРМА.** Слот (`fibers[i]`, SpawnCtx, sched-directory-entry) помечается занятым
— инкрементом `count`, установкой `parked[i]`, публикацией указателя — ТОЛЬКО
после того, как ВСЕ его поля (`fiber_ctx`, `fiber_fail_top`, `sched_state`,
`_nova_init_snapshot`, canary) уже записаны. Публикующая запись — `__ATOMIC_RELEASE`.
Никогда не инкрементируй счётчик-видимости раньше, чем заполнил то, что за ним стоит.

**ПОЧЕМУ.** Если `count++` идёт до записи полей, есть окно, где конкурентный
сканер/wake видит `count=N+1`, но `fibers[N]=NULL` (или полу-инициализированный
SpawnCtx) и либо пропускает слот, либо разыменовывает мусор. Корень FIX 83.10.2
и класса «wake видит полу-инициализированный SpawnCtx».

**ПЛОХО**
```c
int slot = scope->count;
__atomic_store_n(&scope->count, slot + 1, __ATOMIC_RELEASE); /* виден пустой слот */
scope->fibers[slot]     = co;      /* сканер уже мог прочитать fibers[slot]=NULL */
scope->fiber_ctx[slot]  = user;
```

**ХОРОШО** (`nova_scope_alloc_slot`, fibers.h — эталон)
```c
int slot = scope->count;                 /* читаем индекс, НЕ инкрементируем */
scope->fibers[slot]              = co;
scope->fiber_ctx[slot]           = user;
scope->fiber_fail_top[slot]      = NULL;
scope->fiber_effect_snapshot[slot]= NULL;
/* RELEASE-store делает слот видимым только ПОСЛЕ всех записей выше */
__atomic_store_n(&scope->count, slot + 1, __ATOMIC_RELEASE);
```

**ПРОВЕРКА.** TSan на Linux (write-write / read-write на `fibers[slot]`);
state-dump-дискриминатор «`fibers[i]=NULL` AND `parked[i]=true`» (плейбук §3.2) —
это симптом нарушения §1 или §5.

---

### §2. ACQUIRE-load разделяемого счётчика/индекса ПЕРЕД индексацией массива

**НОРМА.** Перед тем как читать `arr[idx]` по индексу/границе, взятой из
разделяемого поля (`count`, `capacity`, `occupancy`), делай `__ATOMIC_ACQUIRE`-load
этого поля. ACQUIRE-load парен с RELEASE-store из §1: наблюдая `count=slot+1`,
ты гарантированно видишь и `fibers[slot]=co`, И актуальный указатель массива
(после возможного `grow`/realloc). Это правило действует для КАЖДОГО читателя,
включая cross-thread драйвер/диагностику — не только для «главного» пути.

**ПОЧЕМУ.** Плоское чтение `count` на драйвер-потоке против `nova_scope_grow` на
воркере (realloc-своп указателя `fibers` + RELEASE-публикация `count`) не имеет
happens-before: читатель видит СВЕЖИЙ `count`, но СТАРЫЙ (уже отброшенный,
меньший) указатель `fibers` → индексация уходит за границы старого буфера →
OOB-чтение соседней кучи. Ровно `[M-mn-spawnctx-corruption-cancel-wake]`,
volna 211: `driver.c::_nova_driver_sleep_close_cb` был единственным пропущенным
читателем (эталон уже был в `runtime.c` cancel-delivery). Аналог `nova_sched_cap_acq`
для sched-directory (Ф.1b `[M-83.11-f1b-acquire-capacity]`).

**ПЛОХО** (`driver.c:349` до фикса)
```c
NovaFiberQueue* sc = st->scope;
int sl = st->slot;
mco_coro* co = (sc && sl >= 0 && sl < sc->count)   /* плоское чтение count */
             ? sc->fibers[sl] : NULL;              /* sc->fibers мог быть старым */
```

**ХОРОШО**
```c
int sc_count = __atomic_load_n(&sc->count, __ATOMIC_ACQUIRE); /* парен с alloc_slot RELEASE */
mco_coro* co = (sc && sl >= 0 && sl < sc_count) ? sc->fibers[sl] : NULL;
```

**ПРОВЕРКА.** Грепни ВСЕХ читателей `->fibers[`, `->count`, `->capacity`,
sched-directory-аксессоров: каждый cross-thread читатель обязан быть ACQUIRE.
TSan ловит непарное чтение. Пред-фаталь — R2-трипваер (§12).

---

### §3. Write-per-slot ДО release-decrement; read ПОСЛЕ acquire-observe-zero

**НОРМА.** Когда N писателей (детей-файберов) наполняют свои персональные слоты
(`child_error[slot]`, `child_ctx[slot]`), а один читатель (owner) читает их после
завершения всех: каждый писатель пишет СВОЙ слот СТРОГО ДО release-декремента
общего счётчика (`pending_remote` / `pending_sweeps`); читатель читает слоты
ТОЛЬКО ПОСЛЕ того, как acquire-load счётчика наблюдал ноль. Порядок записей в
codegen-call-site (запись слота → `nova_aint_fetch_sub_release`) менять нельзя.

**ПОЧЕМУ.** Это единственный дешёвый (без chunked) способ дать happens-before
на realloc'абельном `child_error[]`/`child_ctx[]` без per-slot-лока: release-dec
писателя пушит его слот-запись, acquire-observe-zero читателя её забирает
(173.0 §EXEC R2, A2.3/A3.2). Нарушение порядка = torn read персонального слота
или потеря retained-ctx.

**ПЛОХО**
```c
nova_aint_fetch_sub_release(&parent->pending_remote); /* объявили «готов» */
parent->child_error[slot].msg = msg;                  /* ...а слот ещё не записан */
```

**ХОРОШО** (`nova_fiber_report_child_kinded` + `nova_scope_sweep_dead_child`)
```c
parent->child_error[slot].msg = msg;                  /* пишем СВОЙ слот */
nova_abool_store(&parent->child_error[slot].published, true);
/* ...затем, в эпилоге ребёнка, СТРОГО ПОСЛЕ: */
nova_aint_fetch_sub_release(&parent_snapshot->pending_sweeps); /* release-dec */
/* owner: */ while (nova_aint_load_acquire(&parent->pending_sweeps) != 0) { /* wait */ }
/* только теперь читаем child_error[]/child_ctx[] */
```

**ПРОВЕРКА.** G-STRESS ARMED с форсом `nova_scope_grow` (spawn > `NOVA_SCOPE_INITIAL_CAP=16`)
+ TSan. `assert(nova_aint_load(&pending_remote)==0)` перед первым чтением слотов
в decision-loop (R2-трипваер).

---

## Группа B — Владение слотом и идентичность

### §4. «Слот свободен» = ТРИ условия, не одно

**НОРМА.** Слот считается свободным для реюза ТОЛЬКО когда выполнены ВСЕ три:
```
fibers[i] == NULL  AND  parked[i] == false  AND  pending_wake[i] == 0
```
`fibers[i]==NULL` в одиночку НЕ означает «свободен». Аллокатор слота
(`nova_scope_alloc_slot`) обязан проверять все три и пропускать STALE-слот.

**ПОЧЕМУ.** STALE-slot race (83.11 §10, Fix A): слот с `fibers[i]=NULL`, но
`parked[i]=true` — файбер откреплён, но ещё ждёт wake; реюз такого слота
перекрывает in-flight wake → SEGV/hang. «Невозможная» комбинация в state-dump
(`fibers[5]=NULL AND parked[5]=true`) — прямой признак нарушения.

**ПЛОХО**
```c
if (scope->fibers[i] == NULL) return i;   /* реюзнули слот с pending wake */
```

**ХОРОШО**
```c
if (scope->fibers[i] == NULL
    && !__atomic_load_n(parked_at(sched, i), __ATOMIC_SEQ_CST)
    && pending_wake_at(sched, i) == 0) {
    return i;                              /* реально свободен */
}
```

**ПРОВЕРКА.** State-dump под `NOVA_WATCHDOG_DUMP_SECS=5`; ищи комбинацию
NULL+parked. Стресс ARMED высоким worker-count.

---

### §5. Идентичность файбера — через указатель, НЕ через индекс слота

**НОРМА.** Callback/completion (close_cb, after_work_cb, wake), которому нужно
знать, КАКОЙ файбер он обслуживает, захватывает `mco_coro*` (или SpawnCtx-указатель)
в момент арма и сравнивает его в момент завершения. Индекс слота — НЕ идентичность:
он изменяем и переиспользуем. Сверяй `expected_co == fibers[slot]` перед
доставкой wake; при несовпадении — путь WRONG-FIBER, не доставлять.

**ПОЧЕМУ.** Индекс слота реюзается под другой файбер к моменту, когда async-callback
доедет (83.11 Group C, lesson #4). Захват `expected_co` при арме — единственный
надёжный дискриминатор. NB: ложное срабатывание WRONG-FIBER (когда реюза нет)
тоже опасно — см. 211-разбор: displaced-пометка пропускает `nova_scope_free_slot`
→ dangling `fibers[slot]`. Поэтому §5 работает В ПАРЕ с §2 (ACQUIRE, чтобы не
сравнивать со СТАРЫМ массивом).

**ПЛОХО**
```c
void close_cb(uv_handle_t* h) {
    NovaSleepState* st = h->data;
    nova_sched_wake(st->scope, st->slot);   /* слот мог быть переиспользован */
}
```

**ХОРОШО**
```c
void close_cb(uv_handle_t* h) {
    NovaSleepState* st = h->data;
    int cnt = __atomic_load_n(&st->scope->count, __ATOMIC_ACQUIRE);      /* §2 */
    mco_coro* actual = (st->slot < cnt) ? st->scope->fibers[st->slot] : NULL;
    if (actual != st->expected_co) { /* WRONG-FIBER: не будим */ return; }
    nova_sched_wake(st->scope, st->slot);
}
```

**ПРОВЕРКА.** State-dump; фикстура с sleep+cancel+реюз слота под стрессом.
Сверь, что epilogue файбера всегда зовёт `nova_scope_free_slot` (нет пути,
где displaced-пометка его глотает).

---

### §6. Никакого grow во время drain — заморозь capacity

**НОРМА.** Capacity supervised-scope замораживается на spawn-фазе (все дети
заводятся до drain, на main-потоке). Во время drain роста массива быть не должно.
Realloc'абельные массивы (`child_error[]`, `child_ctx[]`, `fiber_error[]`) растут
ТОЛЬКО пока `_drain_started == false`. Если реально нужен динамический spawn
внутри supervised — переводи массив на chunked stable-address приём
(как `NovaSchedState`), НЕ оставляй наивный realloc-swap.

**ПОЧЕМУ.** Realloc-swap указателя массива под cross-worker запись = torn-base
(исходный M-83.11). Заморозка capacity убирает окно: base стабилен, happens-before
держится через §3 без chunked (173.0 §EXEC R2, Option B).

**ПЛОХО**
```c
/* во время drain ребёнок спавнит внука → grow child_error[] под чтением owner'а */
nova_scope_grow_children(scope, scope->child_count + 1);  /* base порвётся */
```

**ХОРОШО** (`nova_scope_grow_children`, 173.0 A2.3)
```c
static void nova_scope_grow_children(NovaFiberQueue* scope, int need) {
    assert(!scope->_drain_started);   /* R2-трипваер: ловит grow-during-drain ДО порчи */
    /* ...realloc безопасен только вне drain-фазы... */
}
```

**ПРОВЕРКА.** Debug-`assert(!scope->_drain_started)` в grow-функции supervised-scope
(R2-трипваер, §12) — обязателен; ставится ДО добавления cross-worker записи, не после.
G-STRESS обязан форсить grow (spawn > 16).

---

## Группа C — Жизненный цикл памяти под Boehm-GC

### §7. GC-root: не замещай дефолтный bdwgc-колбэк ЧАСТИЧНО

**НОРМА.** Любой объект, достижимый ТОЛЬКО со стека потока/файбера
(stack-локальный supervised-scope, его `child_error[]`/`child_ctx[]`, локали
воркерского шедулера, cancel-токен), ДОЛЖЕН быть покрыт GC-сканом. Если
замещаешь `GC_set_push_other_roots(...)` своим колбэком — компенсируй ВСЕ
слагаемые дефолтного: (1) стек+регистры main-потока, (2) native-стеки всех
зарегистрированных потоков рантайма, (3) занятые fiber-слоты арен. НЕ подмножество.

**ПОЧЕМУ.** `[M-mn-spawnctx-corruption-cancel-wake]` окончательный корень: порт
Windows-модели на Linux перенёс из трёх слагаемых только fiber-слоты, потеряв
native-стеки потоков и стек main. `GC_default_push_other_roots` →
`GC_push_all_stacks()` — единственный канал скана СТЕКОВ И РЕГИСТРОВ на
pthreads-сборке. Замещение колбэка выключило его → всё рутованное только стеком
(scope `q`, child-массивы, токен) собиралось GC ЖИВЫМ → страницы перекраивались
под другие объекты → обе gdb-сигнатуры (32-битное усечение указателей,
ASCII в SpawnCtx) + рваный fail-top. Родственно 151 (GC premature-collect
замыкания, рутованного только native-стеком → `closure->fn=NULL` → jump-to-null).

**ПЛОХО**
```c
static void push_other_roots(void) {
    for (each occupied fiber slot) GC_push_all_eager(slot_lo, slot_hi);
    /* потеряли: main-стек + native-стеки воркеров/драйвера */
}
GC_set_push_other_roots(push_other_roots);   /* дефолтный GC_push_all_stacks выключен */
```

**ХОРОШО** (`fiber_arena.c::_nova_gc_push_other_roots`, spawnctx-волна)
```c
static void push_other_roots(void) {
    push_main_stack_vma();                    /* (1) /proc/self/maps, GC_push_all */
    push_registered_native_stacks();          /* (2) реестр воркеров+драйвера */
    for (each occupied fiber slot) GC_push_all_eager(slot_lo, slot_hi); /* (3) */
}
```
Регистрируй native-стеки на входе/выходе воркеров (`runtime.c::_worker_main`) и
драйвера (`driver.c::_nova_driver_main`). Наивный чейнинг дефолтного колбэка
невозможен (суспенд ловит sp внутри коро-стека → диапазон через PROT_NONE guard'ы
→ SIGSEGV в маркере) — оверрайд + полная компенсация, не чейнинг.

**ПРОВЕРКА.** Дискриминатор: PIN2 (дубль-достижимость текущих массивов через
uncollectable-цепь) → PASS ⇒ GC теряет ЖИВЫЕ массивы (потерян рут-канал стека).
`GC_disable()`/`GC_DONT_GC=1` лечит ⇒ GC-баг. `GC_set_start_callback`-счётчик
сборок (parent=0 сборок vs HEAD=N — см. boehm-eager-cost). TSan НЕ применим
(это не data-race, а GC-root-loss).

---

### §8. Boehm size-class: не давай двум владельцам делить корзину через churn

**НОРМА.** Не заставляй два разных владельца делить одну Boehm size-class-корзину
через частый free→malloc (churn). Retention-массив, который периодически
переаллоцируется (`child_ctx[]`, `child_error[]`), держи в COLLECTABLE памяти
(`nova_alloc`, GC соберёт сам) — НЕ в `nova_alloc_uncollectable` с ручным
`nova_free_uncollectable`. Uncollectable оставляй ТОЛЬКО для объектов, которые
обязаны пережить GC и НЕ переаллоцируются в цикле (`ctx_pins[]`, сами SpawnCtx
из пула). Разделяй интент «указатели ВНУТРИ массива uncollectable» и «сам массив
uncollectable» — это разные вопросы.

**ПОЧЕМУ.** Volna 211: `nova_scope_grow_children` при `NOVA_SCOPE_INITIAL_CAP=16`
аллоцировал `child_ctx[]` ровно `16*8=128` байт — тот же uncollectable
size-class 128, что `_nova_spawn_pool_class_size[1]` для SpawnCtx. Каждый grow
освобождал 128-байтный буфер в ТОТ ЖЕ Boehm free-list, откуда шторм из 2000 spawn'ов
тут же брал новые SpawnCtx — два владельца делили free-list одной корзины.
(Не был доказан единственной причиной, но правка независимо корректна и убирает
коллизию.)

**ПЛОХО**
```c
/* массив живёт ровно пока жив scope; обычной GC-scan-памяти достаточно — */
/* но взяли uncollectable + ручной free → churn в корзину SpawnCtx */
scope->child_ctx = nova_alloc_uncollectable(sizeof(void*) * cap);
/* ...на следующем grow: */ nova_free_uncollectable(old);  /* → free-list SpawnCtx */
```

**ХОРОШО**
```c
scope->child_ctx = nova_alloc(sizeof(void*) * cap);  /* collectable, GC-scan */
/* старый массив НЕ free вручную — GC соберёт (как child_error[] рядом) */
```

**ПРОВЕРКА.** Дискриминатор `NOVA_UNCOLL_QUAR=1` (карантин всех
`nova_free_uncollectable` — poison 0xDD + осознанная утечка, `alloc_boehm.c`):
если краш исчезает при карантине ⇒ порча через реюз released-uncollectable-блока.
R1-трипваер пула (`NOVA_SPAWN_POOL_DIAG=1`).

---

### §9. Стек-указатель, пересекающий границу потока/очереди → counter-based lifetime-wait

**НОРМА.** Всякий раз, когда job/queue/channel/timer несёт указатель на локальную
переменную вызывающего (stack-scope, cancel-token, sleep-state), вызывающий ОБЯЗАН
дождаться, пока асинхронный потребитель закончит разыменование, ПРЕЖДЕ чем вернуть
управление (кадр стека исчезнет). Канонический механизм — счётчик in-flight jobs:
инкремент при постановке, RELEASE-декремент по завершении, owner ждёт нуля.

**ПОЧЕМУ.** 83.11 §12.31 use-after-free: Ф.2 driver-thread + Ф.3 CANCEL_SCOPE jobs
несли scope-указатель на стек-локаль; lifetime-инвариант НЕ был проверен при
дизайне — баг выстрелил через 4 дня. Фикс — `pending_driver_jobs`/`pending_sweeps`
lifetime-counter (D228 §6). Симптом в state-dump: `count=372`, `slot_lock=-1.2B`,
`head=.rdata` — мусорный scope = чтение освобождённого стека.

**ПЛОХО**
```c
void supervised_run(void) {
    NovaFiberQueue scope;                     /* на стеке */
    driver_post_cancel_job(&scope);           /* драйвер держит &scope */
    return;                                    /* кадр исчез, job ещё в полёте */
}
```

**ХОРОШО** (`pending_sweeps` / `pending_driver_jobs`, D466 §6)
```c
void supervised_run(void) {
    NovaFiberQueue scope;
    nova_aint_inc(&scope.pending_driver_jobs);
    driver_post_cancel_job(&scope);
    while (nova_aint_load_acquire(&scope.pending_driver_jobs) != 0) { pump(); }
    return;                                    /* все job'ы дренированы */
}
```

**ПРОВЕРКА.** State-dump: мусорный scope (`.rdata`-head, гигантский `count`) =
use-after-free. VEH+dbghelp frame[1] локализует за минуту (плейбук §3.1).
Аудит: любой job/timer/channel с указателем на локаль — есть ли парный wait?

---

## Группа D — Cancel / wake / unwind

### §10. Fail-frame публикуется ДО того, как файбер станет wake-able

**НОРМА.** `_nova_fail_top` (и `_nova_interrupt_top`, effect-snapshot) файбера
установлен и виден ПРЕЖДЕ, чем файбер попадает в очередь пробуждения / станет
доступен для cancel-доставки. При resume — сначала восстанавливается сохранённый
fail-frame, потом исполняется тело. Порядок «arm sleep → park → wake» не должен
допускать доставку cancel в окно, где fail-frame снят/не восстановлен.

**ПОЧЕМУ.** boehm-eager-cost находка: массовый `tok.cancel()` будит 2000
запаркованных файберов разом; если файбер получает cancel-throw в момент, когда
`_nova_fail_top` уже НЕ установлен (fail-frame снят/не восстановлен вокруг
supervised-scope), порядок unwind/restore путается → `abort()`. STW-пауза Boehm
растягивает окно. См. §11 — прямое следствие.

**ОБНОВЛЕНО 2026-08-08 (окно presume-cas-gate, реестр 221.1 №446/№447):
инвариант держится СТРУКТУРНО, а не соглашением на N сайтах.** До этого
окна restore/CAS-гейт/resume/классификация были открытым кодом на ЧЕТЫРЁХ
независимых resume-сайтах (`_worker_main` главный цикл, `_worker_main`
cleanup-дренаж, `_worker_run_one_fiber`, `nova_supervised_step`) — и
ревизия плана 250 нашла ЖИВОЙ дефект №447: cleanup-дренаж резюмил файбер
**вообще без restore** TLS (не только с опозданием — с полным отсутствием),
плюс безусловно затирал `PARKED` на `IDLE`. Вердикт «§10 держится
структурно», данный на более ранней фазе, оказался ложным — проверка
смотрела только главный цикл.

Фикс: ОДНА функция `nova_resume_fiber(co, tls_ctx, restore_inner,
save_inner)` (`fibers.h`) — единственный вызов `mco_resume` в рантайме,
держит `scripts/guards/check-single-mco-resume.sh` (реестр 221.1
№446/№447). Restore/save для файбера — через хук (`_nova_resume_restore_ctx_tls`/
`_nova_resume_save_ctx_tls` для трёх ctx-based сайтов в `runtime.c`,
`_nova_resume_restore_step_tls`/`_nova_resume_save_step_tls` для
array-based `nova_supervised_step`), но порядок restore→CAS→resume→
restore-outer→классификация — ОДИН код, а не N копий. Новый resume-сайт
физически не может забыть restore — он обязан пройти через
`nova_resume_fiber`.

**ПЛОХО**
```c
nova_sched_park(co);                 /* уже wake-able */
_nova_fail_top = &frame;             /* fail-frame опоздал: cancel мог прийти раньше */
```

**ХОРОШО**
```c
_nova_fail_top = &frame;             /* публикуем fail-frame ПЕРВЫМ */
nova_effect_snapshot_save(&outer_effects);
nova_sched_park(co);                 /* теперь безопасно wake-able */
/* при resume — restore ПЕРЕД телом ЕДИНСТВЕННЫМ путём — см.
 * fibers.h::nova_resume_fiber, не открывай свой mco_resume() */
```

**ПРОВЕРКА.** Фикстура: N запаркованных файберов + один `tok.cancel()` всех разом,
ARMED, под `NOVA_WATCHDOG_DUMP_SECS=5`. Дискриминатор — сообщение
`cancel-throw outside any supervised scope` (`effects.h::nova_throw_cancel`) в
stderr = сработал §11-инвариант. Дополнительно (№446/№447): детерминированная
регресс-проба `spec_tests/conformance/standalone/presume_446_sabotage_probe.nv` (не зависит от
выигрыша гонки — резюмит один `co` дважды напрямую и проверяет, что второй
вызов получает `owned=false`); страж `check-single-mco-resume.sh` держит
число `mco_resume()` вне `nova_resume_fiber` на нуле.

---

### §11. Cancel-throw требует НЕНУЛЕВОГО fail_top

**НОРМА (для пишущего доставку).** `nova_throw_cancel` / cancel-unwind
исполняется ТОЛЬКО когда есть активный fail-frame (`_nova_fail_top != NULL`).
Код, доставляющий cancel (`nova_sched_wake` → resume → throw), обязан
гарантировать этот инвариант установкой §10. Нарушение — сигнал перепутанного
порядка unwind: чинить нужно доставку, а не глушить симптом.

**НОРМА (для пишущего рантайм-реакцию) — обновлено 2026-08-07, дефект 221.1
№431.** Нарушение инварианта БОЛЬШЕ НЕ завершает процесс, и возвращать это
поведение нельзя: D75 (`spec/decisions/06-concurrency.md`) дословно обещает,
что отмена непривязанного токена или уже завершённой области — **безвредный
no-op**, а `abort()`/`exit()` превращали любой такой тайминг в падение
программы, в котором нет ошибки пользователя. Действующая реакция
(`_nova_cancel_no_handler`, `nova_rt/effects.c`):

1. **Завершить ТОЛЬКО текущий файбер** — longjmp в его *якорь выхода*
   (`NovaFiberAnchor`, `effects.h`; указатель живёт в
   `NovaSpawnCtxBase::_nova_fiber_anchor`, ставится entry-функцией файбера
   первым же действием). Управление попадает в собственный кадр entry, тот
   доигрывает штатный эпилог (освобождение слота, декремент `pending_remote`,
   пробуждение владельца области) и возвращается — корутина становится
   `MCO_DEAD`, остальная программа не задета.
2. **Ничего не рапортовать наверх** — предусловие попадания сюда в том и
   состоит, что области уже нет; выдуманная ошибка превратила бы безвредную
   гонку в упавший scope.
3. **Считать и печатать итог** — счётчик поздних отмен + строка в stderr на
   выходе процесса. Это единственное, что не даёт обменять падение на
   молчание (ловушка беззубого люка `nova:allow`, №423).

**ПОЧЕМУ ИМЕННО ЯКОРЬ.** Файбер нельзя завершить изнутри тела: `mco_yield`
только приостанавливает, и планировщик будет резюмить уступившую (но живую)
корутину вечно — это молчаливый CPU-hang, строго хуже краха (измерено окном
p431). Корутина умирает ТОЛЬКО возвратом из entry-функции, поэтому точка
возврата обязана быть заведена НА ВХОДЕ. Указатель на неё лежит в ctx, а не в
TLS, ровно потому, что сам разбираемый отказ — это NULL в TLS-слоте
`_nova_fail_top`: механизм спасения не должен зависеть от того, что сломалось.

**ЕСЛИ ДОБАВЛЯЕШЬ НОВУЮ ТОЧКУ ЭМИССИИ ФАЙБЕРА** (новая entry-функция в
`emit_c.rs`/`emit_detach.rs`) — обязаны быть ВСЕ ТРИ части протокола:
поле в раскладке ctx (`emit_spawn_ctx_anchor_field`), взвод якоря первым
действием entry (`emit_fiber_anchor_arm`, ДО пролога-safepoint'а — он сам
умеет бросить отмену), снятие перед эпилогом (`emit_fiber_anchor_close`).
Пропуск поля рушит раскладку так же фатально, как пропуск `schedlink`.

**ПЛОХО**
```c
void deliver_cancel(mco_coro* co) {
    resume(co);
    nova_throw_cancel("scope cancelled");   /* fail_top==NULL — доставка сломана */
}
```

**ХОРОШО**
```c
void deliver_cancel(mco_coro* co) {
    resume(co);                              /* resume восстановил fail-frame (§10) */
    if (nova_fail_top() == NULL) { /* инвариант нарушен — НЕ маскировать, чинить §10 */ }
    nova_throw_cancel("scope cancelled");
}
```

**ПРОВЕРКА.** grep stderr на строку итога `cancellation(s) arrived after their
scope had already unwound`; ноль за N ARMED-прогонов cancel-шторма = инвариант
доставки держится. Ненулевой счётчик — НЕ падение, но и не норма: это гонка
доставки, которую чинят по §10. Отдельно смотри подстроку `had no fiber-exit
anchor to retire` — она означает, что отмена пришла туда, где тела файбера нет
(корневой main-файбер или внутренняя корутина рантайма); если она появилась в
обычной программе, ищи потерянную точку эмиссии якоря. И НЕ конвертируй это в
немой return без счётчика — молчание скрыло бы гонку.

### §12. Pump work-conserving — не отталкивай готовый файбер по принадлежности scope

**НОРМА.** Когда воркер извлёк ГОТОВЫЙ файбер из своей deque, он ОБЯЗАН выполнить
его inline (восстановив outer active-scope/slot TLS для чужого), а НЕ «этот
файбер не моего scope — толкну обратно и подожду своего». Планировщик
work-conserving: пока в системе есть готовая работа, ни один воркер не
простаивает из-за принадлежности задачи. Global progress > local ownership.

**ПОЧЕМУ.** [M-187-high-concurrency-connection-wedge] (runtime.c pump-фикс,
2026-07-20). Под connection-storm (MAXPROCS≥2, MAX_INFLIGHT>2) каждый воркер
блокировался в nested `supervised_run_impl` pump, гоняя ТОЛЬКО файберы своего
scope, а ready-child СОСЕДНЕГО scope лежал в его deque нетронутым → взаимный
strand всех воркеров (`count=0 pending_remote=1` + `STUCK_ALIVE_NOT_PARKED`) →
`permanent-000`. Симптом-обманка: выглядит как lost-wake/park-deadlock, а корень —
non-work-conserving жадность к своему scope. Дискриминатор: без фикса MAXPROCS 2/4
клинят, 1 воркер выживает (нет соседа-жертвы); с фиксом — все живут.

**ПЛОХО**
```c
mco_coro* co = deque_pop(self->run_q);
if (co->scope != self->active_scope) {
    deque_push(self->run_q, co);   /* «не мой scope» — жду своего → взаимный strand */
    continue;
}
run_inline(co);
```

**ХОРОШО**
```c
mco_coro* co = deque_pop(self->run_q);
Scope* prev = self->active_scope; Slot* prev_slot = self->active_slot;
self->active_scope = co->scope; self->active_slot = co->slot;  /* foreign TLS restore */
run_inline(co);                    /* work-conserving: гоним ЛЮБОЙ готовый */
self->active_scope = prev; self->active_slot = prev_slot;
```

**ПРОВЕРКА.** `xargs -P80`/`-P200` connection-storm против сервера (MAXPROCS≥2,
MAX_INFLIGHT>2): сервер жив, БЕЗ permanent-000, single-req после = 200.
Дискриминатор «без pump клинит / с pump живёт» на MAXPROCS 2 и 4.

---

## §12. Обязательные трипваеры для нового nova_rt-кода

Любой новый concurrency-код в `nova_rt/` ОБЯЗАН нести пред-фатальные трипваеры,
срабатывающие ДО фатала (AV/hang/silent-corruption), а не после. Все —
`getenv`-gated, **ноль оверхеда в релизе** (условие проверяется только при
выставленном env). Это не опция — это часть определения «готового» M:N-кода
(173.0 §EXEC требует tripwire ставить ДО опасного кода).

### R1 — poison + canary (pool-recycle aliasing) — `NOVA_SPAWN_POOL_DIAG=1`
Буфер SpawnCtx при release — `memset(0xDD)` + magic-canary в `NovaSpawnCtxBase`.
Точки чтения ctx (goready/resume/sweep/driver) проверяют canary перед
разыменованием и `abort("ctx recycled under supervisor")` при порче.
Плюс guard-assert в `nova_spawn_pool_release`. Ловит R1 (§EXEC): retained-указатель,
переписанный следующим spawn до decision-loop. Реализовано:
`nova_spawn_ctx_diag_check_live`, `alloc_boehm.c`.

### R2 — torn-base (grow-during-drain) — debug-assert
`assert(!scope->_drain_started)` внутри `nova_scope_grow_children` +
`assert(nova_aint_load(&pending_remote)==0)` перед первым чтением `child_error[]`
в decision-loop. Ловит ЛЮБОЙ grow-during-drain ДО того, как он порвёт base (§6).
Ставится ПЕРЕД добавлением cross-worker записи.

### R3 — marker-clash (drain-hang) — watchdog
`NOVA_WATCHDOG_DUMP_SECS=5` → hang задампится за 5s (state-dump всех
воркеров/файберов/scope) вместо вечного зависания. Правильнее не допускать:
retention ИСКЛЮЧИТЕЛЬНО через отдельный канал (`child_ctx[]`/`ctx_pins`),
`fiber_ctx[i]` всегда обнулять при смерти (иначе worker-owned alive-check
сочтёт мёртвого ребёнка живым → drain не завершится).

### Дополнительный дискриминатор-набор (env-gated, из спавнктх-волны)
- `NOVA_UNCOLL_QUAR=1` — карантин всех `nova_free_uncollectable` (§8).
- `GC_MARKERS=1` — отключает Boehm parallel-mark (разводит «гонка в маркере» vs
  «корень глубже»).
- `NOVA_DIAG_SEGV=1` — VEH+dbghelp frame[1] (плейбук §3.1).

**Правило для КАЖДОГО нового shared-состояния:** прежде чем писать логику,
ответь — какой из R1/R2/R3 её охраняет? Если ни один — заведи трипваер ПЕРВЫМ,
докажи, что он не срабатывает на текущем коде, и лишь потом добавляй опасный код.

---

## Связь с реактивным плейбуком

| | Проактив (этот файл) | Реактив ([debugging-races.md](debugging-races.md)) |
|---|---|---|
| Когда | ПИШЕШЬ M:N-код | ЛОВИШЬ гонку постфактум |
| Цель | не допустить дефект | локализовать за минуты |
| Инструмент | §1–§11 нормы + §12 трипваеры | 5-step алгоритм, state-dump, VEH+dbghelp, bisect |
| Связь | §12-трипваеры = встроенные детекторы, которые плейбук читает | плейбук находит новый класс → сюда добавляется норма |

Когда новая волна-расследование закрывается новым классом дефекта (плейбук §8) —
дистиллируй урок В НОРМУ здесь, с ПЛОХО/ХОРОШО и трипваером. Так остаток, который
поймал плейбук, становится тем, что предотвращает конвенция.

## Родственные документы
- [`docs/dev/debugging-races.md`](debugging-races.md) — реактивный плейбук (20 уроков, tooling).
- [`docs/plans/173.0-concurrency-runtime-substrate.md`](../plans/173.0-concurrency-runtime-substrate.md) §EXEC — R1/R2/R3 first-hand.
- `spec/decisions/06-concurrency.md` D228 §6/§7 — канонические паттерны (lifetime-counter, ctx-pin).
- Код-эталоны: `fibers.h` (`nova_scope_alloc_slot`, `nova_scope_grow_children`,
  `nova_scope_sweep_dead_child`), `runtime.c` (cancel-delivery ACQUIRE, `_worker_run_one_fiber`),
  `driver.c` (`_nova_driver_sleep_close_cb`), `fiber_arena.c` (`_nova_gc_push_other_roots`),
  `alloc_boehm.c` (uncollectable/quarantine).
