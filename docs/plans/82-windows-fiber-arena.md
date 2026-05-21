// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 82: Windows fiber stack arena — re-diagnosis + production-реализация

> **Создан:** 2026-05-21. **Переписан с чистого листа:** 2026-05-21
> (первая редакция). **Переработан до production-grade:** 2026-05-21
> (вторая редакция — перепроверка против исходников; см. §1).
> **Статус:** 📋 proposed, не начат. Ф.0 = re-diagnosis (decision point).
> **Приоритет:** P2 — Windows работает (calloc-fallback, корректно для
> single-thread cooperative), но без lazy-commit, без stack-overflow
> detection и **с честно-хрупкой GC-моделью** (см. §1.4). Это долг
> паритета с Linux/macOS (Plan 44.2) и с Go/Rust, не блокер релиза.
> **Родитель:** [Plan 44](44-mn-runtime-roadmap.md) (M:N runtime).
> **Заменяет:** [Plan 44.3](44.3-fiber-arena-windows.md) — 4 неудачные
> попытки 2026-05-13/14; его документированный root cause **опровергнут**
> (§1.1–1.2). 44.3 остаётся историческим документом.

---

## 1. Перепроверка с чистого листа — что подтверждено исходниками

Вторая редакция плана начата с верификации каждого утверждения первой
редакции против реального кода (`minicoro.h`, `fiber_arena.c`, `fibers.h`,
git-история). Результат: центральный тезис подтверждён, но **настоящий
блокер найден точно** (а не оставлен «неизвестным»), и production-дизайн
переписан — он отличается от того, что предлагала первая редакция.

### 1.1. minicoro уже свопает TIB — ВЕРИФИЦИРОВАНО

`compiler-codegen/nova_rt/minicoro.h`:

- Выбор backend (строки 378–404): `_WIN32 && ((__GNUC__ && __x86_64__) ||
  (_MSC_VER && _M_X64))` → **`MCO_USE_ASM`**. Windows x64 + clang-cl/MSVC
  всегда идут по asm-пути; `MCO_USE_FIBERS` — только ARM/32-bit. Значит
  custom `alloc_cb`/`dealloc_cb` **honored** (не Windows-fiber API).
- `_mco_ctxbuf` (665–672): помимо регистров содержит `fiber_storage`,
  `dealloc_stack`, `stack_limit`, `stack_base`.
- `_mco_switch_code` (688–750): asm читает `%gs:0x30` (self-указатель
  TEB → `r10`) и **на каждом switch** сохраняет из TEB в `from`-контекст
  и восстанавливает в TEB из `to`-контекста:
  - `0x08(%r10)` ↔ `NT_TIB.StackBase`
  - `0x10(%r10)` ↔ `NT_TIB.StackLimit`
  - `0x1478(%r10)` ↔ `TEB.DeallocationStack`
  - `0x20(%r10)` ↔ `NT_TIB.FiberData` (**коррекция первой редакции:**
    смещение `0x20` — это `FiberData`/`Version`-union, **не**
    `ArbitraryUserPointer`; последний на `0x28` и minicoro его не трогает).
- `_mco_makectx` (755–768) инициализирует `stack_base`/`stack_limit`/
  `dealloc_stack` на границы корутинного стека.

То есть текущий minicoro делает ровно то, что делают **corosensei** (Rust)
и **Boost.Context** (C++): синхронизирует TIB при переключении. На x64
этого достаточно для table-based SEH — `RtlLookupFunctionEntry` /
`RtlVirtualUnwind` ведут раскрутку по `.pdata`, а dispatcher валидирует
кадры по `StackBase/StackLimit`; x64 **не** использует TEB exception-list
chain (`%gs:0x00`) — это механизм x86-32, и minicoro его справедливо не
свопает.

### 1.2. git-доказательство: 44.3 использовал именно этот minicoro

`git log --follow -- compiler-codegen/nova_rt/minicoro.h` → **один
коммит**, `3d2b562` от **2026-05-05**, файл с тех пор не менялся. Попытки
Plan 44.3 — **2026-05-13/14**, на восемь дней позже. Следовательно
гипотеза 44.3 «использовался старый minicoro без Windows-asm-backend'а»
**ложна**. 44.3 компилировался против ровно этого asm-backend'а, который
свопает TIB. Вывод однозначен: **диагноз 44.3 «MCO_USE_ASM не обновляет
TIB» — мисдиагноз.** TIB обновлялся всё время.

### 1.3. Настоящий блокер найден в коде — GC-scan vs reserved-страницы

`fiber_arena.c` (Linux/macOS-реализация 44.2) регистрирует арену как GC-root
**плоским диапазоном**:

```c
// fiber_arena.c:246-256  — _arena_register_active_range
GC_add_roots(a->base, a->base + new_high * a->slot_size);
```

Boehm — conservative collector: зарегистрированный root он **читает
по-байтно** на mark-фазе. На Linux это безопасно: арена — `mmap` с
`MAP_NORESERVE`, чтение незакоммиченной страницы даёт zero-page от ядра,
без фолта. **На Windows чтение `MEM_RESERVE`-но-не-`MEM_COMMIT` страницы —
это `STATUS_ACCESS_VIOLATION`.** Любой lazy-commit-дизайн, который
регистрирует арену плоским `GC_add_roots`, на первом же GC-цикле уронит
Boehm-сканер на первой незакоммиченной странице.

Это **в точности** провал «Попытки 1» Plan 44.3 (секция «Неудачные
попытки»): VEH коммитил каждую страницу, которую трогал Boehm-scan, —
вся арена становилась resident. И провал «Попытки 2» (eager per-slot
commit → commit-charge взрыв). Оба провала имеют **известный, верный root
cause** — конфликт conservative-scan с reserved-памятью — и **он не
опровергнут**. Опровергнут только вывод «Попытки 4» («TIB»).

**Следствие для дизайна:** плоский `GC_add_roots` поверх арены — НЕ
переносимая на Windows стратегия. Production-дизайн обязан сканировать
**только закоммиченные, реально используемые** диапазоны стеков. Это
делается через `GC_set_push_other_roots` + `GC_set_stackbottom` (§5.2) —
не через `GC_add_roots`. Первая редакция плана говорила «перенести
active-range root из 44.2» — это **неверно** и переписано.

### 1.4. Что остаётся необъяснённым → задача Ф.0-спайка

Провалы Попыток 1–2 объяснены (§1.3). Провалы **Попыток 3–4** — нет:
«даже простой `main_yield` TIMEOUT 60+ сек» при дизайне «no decommit,
commit per-slot». При закоммиченном слоте и без VEH `main_yield` (один
spawn+yield) зависать не должен. Правдоподобные причины: (а) баг в
откаченном arena-коде (учёт слотов / guard-страница, закоммиченная
`PAGE_NOACCESS` внутри usable-региона / выравнивание); (б) decommit
во время context-switch (гипотеза самого 44.3); (в) что-то в
взаимодействии с Boehm `GC_THREADS`. Это **вторая, отдельная аномалия**,
и Ф.0 обязана её воспроизвести/исключить на изолированном харнессе, а не
сразу идти в интеграцию (как 44.3 ×4).

### 1.5. Скрытый долг: текущая Windows GC-модель честно-хрупкая

Plan 44.2 Этап 2 удалил `_NOVA_GC_DISABLE` на всех платформах. На Windows
fiber-стеки сейчас — `calloc(56 KB)` (minicoro default, `mco_desc_init(_,0)`,
`fibers.h:108`). Эти стеки **не** в managed heap и **не** зарегистрированы
как GC-root. Корректность сейчас держится на инварианте «single-thread
cooperative + GC не запускается, пока активен fiber-стек» — комментарий
`fibers.h:62-65` это и фиксирует. Под M:N (Plan 44.5) этот инвариант
ложен. Поэтому Plan 82 — это не только «добавить lazy-commit», это
**сделать Windows GC-модель строго корректной** (§5.2), иначе включать
M:N на Windows (Ф.5) небезопасно.

---

## 2. Реальная постановка — четыре подпроблемы

TIB — **не** подпроблема (§1.1). Реальная работа:

- **П1. Lazy-commit аллокация.** `VirtualAlloc(MEM_RESERVE)` большого
  диапазона + commit физических страниц по факту роста стека. Ключевая
  гипотеза (§5.1): после TIB-свопа minicoro-стек **неотличим от
  `CreateFiber`-стека** с точки зрения MM ядра → ядро само растит его
  через `PAGE_GUARD` (`MiCheckForUserStackOverflow` сверяется с
  `TEB.DeallocationStack/StackBase`). Если так — custom VEH для happy-path
  не нужен вовсе. Ф.0 это проверяет; §5.1 даёт fallback.
- **П2. GC-scan без commit-взрыва.** §1.3 — никакого плоского
  `GC_add_roots`. Сканировать только `[saved_sp, stack_top]` каждого
  стека — закоммиченный, реально используемый диапазон. Через
  `GC_set_push_other_roots` + `GC_set_stackbottom` (§5.2).
- **П3. Полнота GC-root покрытия.** Stackful-рантайм на conservative GC
  обязан просканировать **все** категории стеков, иначе UAF:
  1. **running fiber** — на каждом worker'е;
  2. **suspended fibers** — включая вложенные resume (A резюмнул B → A
     тоже suspended);
  3. **suspended native worker/scheduler stacks** — пока worker крутит
     fiber, его native-стек со scope-переменными (`NovaFiberQueue` —
     стековая переменная, `fibers.h:163-287`) подвешен;
  4. managed heap — Boehm трассирует сам.
  Первая редакция плана упоминала только (2). Пропуск (3) — реальный UAF:
  `NovaFiberQueue` на native-стеке держит указатели на heap-массивы
  `fiber_ctx[]` — GC-root'ы для `SpawnCtx`.
- **П4. Cross-thread migration.** Plan 44.5 work-stealing реально
  мигрирует fiber'ы между worker'ами (`runtime.c`: `nova_deque_steal` →
  `mco_resume` на другом потоке). minicoro-asm свопает TIB **текущего**
  потока → resume на потоке B настраивает TIB потока B на стек fiber'а —
  работает. Но GC-bookkeeping (П2/П3) обязан корректно отрабатывать
  fiber, чей `saved_sp` принадлежит стеку, аллоцированному в арене
  worker'а A, а исполняется на worker'е B. Обязательный пункт
  тест-матрицы, не допущение.

---

## 3. Сравнение с Go / Rust / TS — честная рамка

| Аспект | Go (goroutine) | Rust — tokio | Rust — corosensei / Boost.Context | TS / JS | **Nova сейчас (Win)** | **Nova — цель Plan 82** |
|---|---|---|---|---|---|---|
| Модель корутины | stackful | **stackless** (async FSM) | stackful | **stackless** (event loop) | stackful (minicoro asm) | stackful |
| Отдельный стек на корутину | да | нет | да | нет | да | да |
| Windows TIB на switch | runtime сам; OS-вызовы на g0 | н/п | свопает `StackBase/Limit/Dealloc` | н/п | **minicoro asm свопает ✅** | то же ✅ (есть) |
| Lazy commit стека | да (растёт по факту) | н/п | реализация-зависимо (часто eager) | н/п | **нет** (56 KB upfront) | **да** — OS-native grow / VEL fallback |
| Stack overflow detection | morestack-проверка в прологе | guard page → abort | guard page → abort | `RangeError` | **нет** (тихая порча) | guard + `STATUS_STACK_OVERFLOW` → детерминированная ошибка |
| GC-видимость suspended-стеков | precise stackmaps | н/п (нет своих стеков) | зависит от встройки | н/п | **хрупко** (§1.5) | **precise push `[sp, top]`** per fiber (§5.2) |
| Growable стек (copy-grow) | да (precise GC, pointer-fixup) | н/п | нет | н/п | нет | **нет — несовместимо с conservative GC** (§4) |
| Cross-thread migration | да | да | да (если `Send`) | нет | Linux/macOS да; Win — fallback | **да** (П4) |
| Эффективный потолок стека | ~1 GB (растёт) | н/п | fixed reserve | ~глубина движка | **56 KB (опасно мало)** | large-reserve (8 MB, как Linux 44.2) |
| Стоимость роста | copy O(размер) | н/п | нет роста | н/п | нет роста | нет copy — commit O(страница) |

**Честный разбор:**

1. **tokio и TS/JS этой задачи не имеют** — они stackless: корутина —
   сгенерированный конечный автомат на обычном стеке потока. Nova
   сознательно выбрала **stackful** (алгебраические эффект-хендлеры +
   произвольная глубина вызовов без function-coloring). Plan 82 не
   пересматривает выбор, а честно фиксирует: stackful → эта работа
   существует; stackless бы её не имел. (Историческая ремарка:
   `node-fibers` давал JS stackful-режим через libcoro — заброшен; индустрия
   ушла в stackless.)
2. **Прямой ориентир — Go-runtime и corosensei/Boost.Context.** По TIB-
   синхронизации Nova (minicoro asm) **уже на их уровне** — не отставание.
3. **Где Nova реально позади сегодня (Windows):** (а) нет lazy-commit
   (56 KB upfront × N, и потолок 56 KB опасно мал); (б) нет
   overflow-detection (тихая порча heap); (в) GC-модель suspended-стеков
   хрупкая (§1.5). Plan 82 закрывает все три → паритет.
4. **Growable-by-copy** (Go копирует растущий стек) Nova **не может** —
   §4. Не «отставание-которое-чинится», а архитектурное следствие
   conservative GC; компенсируется large-reserve + lazy-commit.
5. **Где Nova после Plan 82 будет НЕ хуже / лучше:**
   - **Лучше tokio/TS:** реальный стек на корутину → естественный
     последовательный код, без `async`-окраски, глубокая рекурсия и
     эффект-хендлеры работают.
   - **Лучше Go в одном аспекте:** нет mid-execution copy-grow → нет
     требования precise stackmap, нет copy-pause, **адреса стека
     стабильны** (надёжнее для FFI и для самого conservative GC, который
     не должен «чинить» указатели).
   - **Паритет с Go по объёму памяти:** Go растит стек по факту; Nova
     lazy-commit'ит по факту — обе платят за реально использованное.
   - **Паритет с Go по диапазону GC-скана:** precise push (§5.2)
     сканирует ровно `[sp, top]` каждого fiber'а — **тот же диапазон**,
     что сканирует Go. Разница — conservative-vs-precise маркировка
     (возможное ложное удержание) — это свойство Boehm, не данного плана.
   - **Честно позади Go:** Go умеет **сжимать** живой стек (copy в
     меньший буфер на GC); Nova может только decommit'ить **освобождённый**
     слот, не сжать живой. Документируется как осознанный трейд.

---

## 4. Коррекция целей 44.3: что недостижимо в принципе

Plan 44.3 формулировал цель как «lazy commit + **growable stacks** + single
GC root». Два из трёх пунктов надо **убрать**:

- **«Growable-by-copy» недостижим.** Go копирует растущий стек
  (`runtime.copystack`), потому что у Go **precise** stackmaps — компилятор
  знает смещение каждого указателя и переписывает их (pointer-fixup) после
  копирования. Nova использует **Boehm conservative GC**
  (`GC_set_all_interior_pointers(1)`, `alloc_boehm.c`): сборщик **не знает**,
  слово на стеке — указатель или целое → не может переписать указатели при
  копировании → после copy все internal-указатели битые. Единственная
  совместимая стратегия «большого стека» — **fixed virtual reserve + lazy
  physical commit**: виртуальный адрес неизменен (указатели валидны),
  физический backing растёт по page-fault. Цель Plan 82 — это, **не**
  growable-by-copy.
- **«Single GC root» (плоский `GC_add_roots`) недостижим на Windows** —
  §1.3. Цель переформулирована: **точечный push закоммиченных диапазонов**
  (§5.2), число `GC_add_roots`-записей = 0 → проблема `MAX_ROOT_SETS=128`
  снимается иначе, чем в 44.2, и без скана reserved-памяти.

**Цель Plan 82 (исправленная):** lazy-commit large-reserve fiber-стеки на
Windows + детерминированная overflow-detection + строго корректная и
commit-безопасная GC-видимость **всех** категорий стеков (§П3) — паритет
с Linux-arena 44.2 по эффекту и **строже** неё по GC-точности.

---

## 5. Production-grade дизайн

### 5.1. П1 — память: reserve + lazy-commit + guard

**Раскладка одного слота** (стек растёт вниз, от high к low адресу):

```
slot_base (low)                                        slot_top (high)
│                                                                   │
├─ hard guard ─┬─ minicoro header ─┬─────── usable stack ───────────┤
│  PAGE_NOACCESS│  mco_coro + storage│  growable, lazy-commit         │
│  (16 KB)      │  (≈ 1 KB, align16) │  PAGE_GUARD движется вниз →    │
│               ↑                                                    │
│           ЗДЕСЬ ставим PAGE_GUARD-вершину НАД header'ом            │
```

- `VirtualAlloc(NULL, slot_size, MEM_RESERVE, PAGE_NOACCESS)` — резерв без
  commit-charge. `slot_size` — ориентир 8 MB (как Linux 44.2).
- **Hard guard:** нижние `NOVA_FIBER_GUARD_SIZE` (16 KB, как Linux 44.2 —
  защита от stack-clash при единичном большом кадре) держатся
  `PAGE_NOACCESS` навсегда. Касание → детерминированный overflow.
- **Коррекция дефекта раскладки 44.2:** minicoro кладёт `mco_coro`-header
  в начало (low-адрес) выделенного блока, стек — над storage. При
  переполнении стек растёт вниз и **сначала корёжит header**, потом
  упирается в guard. Production-фикс: вычислить
  `header_off = align16(sizeof(mco_coro)) + storage_size` и поставить
  **движущуюся guard-вершину выше header'а** — overflow фолтит, пока
  header цел → чистый switch-back и точная диагностика. (Linux 44.2 этот
  дефект не чинил; Plan 82 чинит — это «без упрощений».)
- **Lazy commit — два пути, выбор в Ф.0:**
  - **Путь A (предпочтительный) — OS-native grow.** Гипотеза: после
    TIB-свопа (`DeallocationStack`/`StackBase` указывают на слот)
    minicoro-стек для MM ядра **неотличим от `CreateFiber`-стека**.
    `CreateFiber`-стек — это та же `VirtualAlloc`-область + TEB-своп
    (делает `SwitchToFiber`); ядро растит его штатным механизмом
    user-stack guard-grow: `PAGE_GUARD`-фолт → `MiCheckForUserStackOverflow`
    сверяется с `TEB.DeallocationStack/StackBase` → коммитит следующую
    страницу, опускает guard, обновляет `TEB.StackLimit`; у нижней границы
    → `STATUS_STACK_OVERFLOW`. minicoro сохраняет обновлённый `StackLimit`
    в ctxbuf на switch-out → когерентно. Если Ф.0 подтверждает A —
    **custom VEH для happy-path не нужен вовсе**, ядро всё делает в
    fast-path. `nova_fiber_alloc` лишь коммитит начальное окно (вершина
    слота, ≥ объёма, который minicoro трогает в `_mco_makectx`:
    dummy-return-address на вершине) + ставит первую `PAGE_GUARD`.
  - **Путь B (fallback) — VEH lazy commit.** Если Ф.0 покажет, что ядро
    не растит non-primary-стеки: `AddVectoredExceptionHandler`,
    распознающий AV внутри арены → `VirtualAlloc(page, MEM_COMMIT)` →
    `EXCEPTION_CONTINUE_EXECUTION`. VEH процесс-глобален → строго: трогать
    **только** фолты внутри своей арены (быстрый range-чек через
    `nova_fiber_arena_contains`), вне — `EXCEPTION_CONTINUE_SEARCH`.
    Минус B: лишняя латентность на каждом исключении процесса +
    page-by-page фолты при `__chkstk`-пробинге глубоких кадров.
- **`__chkstk` / stack-probes.** clang-cl и MSVC эмитят `__chkstk` в
  прологе функций с локалами > 1 страницы — он трогает стек по странице
  вниз, тем и драйвит guard-grow. Это **штатный Windows-механизм против
  stack-clash** (аналог Linux `-fstack-clash-protection`): на Windows
  пробинг встроен в кодоген, отдельный флаг не нужен. Совместим и с A
  (ядро коммитит), и с B (VEH коммитит).
- **Decommit при освобождении слота.** Чтобы commit-charge оставался
  пропорционален реально активным fiber'ам: на dealloc — `VirtualFree(
  committed_range, MEM_DECOMMIT)` (освобождает commit-charge; повторный
  доступ → re-fault → re-grow). `MEM_RESET`/`DiscardVirtualMemory` **не
  годятся** — они НЕ снимают commit-charge. Антипаттерн syscall-storm
  (урок Linux P41-3): decommit **батчем на idle** (`slots_active == 0`)
  либо decay-очередью — зеркало `nova_fiber_arena_compact`.
- **Slot-арена vs per-fiber `VirtualAlloc`.** В 44.2 непрерывная арена
  была нужна ради **одного** `GC_add_roots`. Production-дизайн §5.2
  (`push_other_roots`) делает `GC_add_roots` count = 0 → арена **больше
  не связана с GC**. Арену оставляем — но как **аллокатор** (reuse слотов
  без `VirtualAlloc`/`VirtualFree`-churn; O(1) `arena_contains` для
  overflow-handler'а), не как GC-механизм. Это явно фиксируется.
- **TLS-время жизни арены.** Per-thread арена в `__declspec(thread)`;
  очистка при выходе потока — `FlsAlloc` + `FlsCallback` (Windows-аналог
  `pthread_key_create` из 44.2). `__attribute__((destructor))` — НЕ
  годится (process-exit, не thread-exit).
- **Commit-charge / 32-bit.** 64-bit: 4096 слотов × 8 MB = 32 GB резерва
  на поток — на 128 TB адресного пространства тривиально, commit-charge =
  только реально закоммиченное. 32-bit Windows — downsize (как
  `fiber_arena.h:70-74`: 16 слотов).

### 5.2. П2+П3 — GC: precise push всех категорий стеков

**Никакого `GC_add_roots` поверх арены.** Вместо этого — два публичных
Boehm-API (оба документированы, патчить Boehm не нужно):

- **`GC_set_stackbottom(thread_handle, &sb)`** — задаёт «дно» стека
  потока. Добавлен в Boehm именно для cooperative-threading/корутин.
  Вызывается на **каждом** переключении:
  - switch-in fiber `F` на worker `W`: `GC_set_stackbottom(W, F.stack_top)`
    → штатный thread-scan Boehm для `W` = `[current_sp, F.stack_top]` —
    закоммичено, корректно (running fiber, П3.1).
  - switch-out (fiber → native): `GC_set_stackbottom(W, native_top)` →
    скан возвращается на native-стек.
  Хэндл потока берётся один раз через `GC_get_my_stackbottom` в прологе
  worker'а (поток зарегистрирован `GC_register_my_thread`, Plan 44.5 L4).
  **Зачем обязателен:** без него thread-scan Boehm для `W`, крутящего
  fiber, пойдёт `[fiber_sp … native_stackbase]` — через fiber-стек, дыру
  и native-стек; дыра не замаплена → AV в сканере.
- **`GC_set_push_other_roots(cb)`** — callback на mark-фазе (мир
  остановлен). `cb` обходит **все** worker-очереди и пушит:
  - каждый **suspended fiber** (включая вложенные resume): диапазон
    `[co->ctx.rsp, co->ctx.stack_base]` через `GC_push_all_stack(lo, hi)`.
    `rsp` — сохранённый minicoro SP (`_mco_ctxbuf.rsp`), `stack_base` —
    сохранённая вершина. Диапазон = реально использованная, закоммиченная
    часть → commit-безопасно и precise (П3.2).
  - каждый **suspended native worker-стек** (worker крутит fiber → его
    native-стек подвешен): `[saved_native_sp, native_top]`. `saved_native_sp`
    worker записывает сам — одна строка перед `mco_resume` (П3.3).
  running fiber'ы (П3.1) уже покрыты thread-scan'ом Boehm благодаря
  `GC_set_stackbottom`; managed heap (П3.4) Boehm трассирует сам.

**Это полная, без-упрощений GC-модель** — ровно то, что обязан делать
stackful-рантайм на conservative GC, и чего у первой редакции плана не
было. Корректна и для single-thread cooperative, и для M:N work-stealing
(callback обходит все worker'ы независимо от того, кто что украл — П4).

**Файлы:** новый `nova_rt/fiber_gc_win.c` (или cross-platform
`fiber_gc.c`) с `cb` + регистрацией; хук switch-точек — `fibers.h` /
`runtime.c` (worker resume-loop, `supervised_step`).

**Linux-унификация — followup, не блокер.** Та же `push_other_roots`-модель
строго лучше плоского `GC_add_roots` и на Linux (нет over-scan мёртвых
страниц, снимается латентный вопрос native-стеков §П3.3). Но Plan 44.2 —
✅ закрыт и работает; Plan 82 **не трогает** Linux-путь по умолчанию, чтобы
не регрессировать закрытый план. Унификация — Ф.6, опционально, под
gate'ом «0 регрессий на Linux».

### 5.3. П4 — cross-thread migration

minicoro-asm свопает TIB **текущего** потока — resume украденного fiber'а
на worker'е B настраивает TIB B на стек fiber'а. GC-модель §5.2 не
привязана к тому, кто аллоцировал слот: `push_other_roots` обходит все
worker'ы и пушит fiber по его `co->ctx` независимо от арены-владельца.
Требуется проверить (Ф.3): (а) `GC_set_stackbottom` зовётся на B на
switch-in; (б) `mco_current_co` (TLS) консистентен после миграции;
(в) home-scope pinning (`runtime.c`) не ломается; (г) decommit слота
fiber'а, мигрировавшего и завершившегося на чужом worker'е, идёт в арену
**worker'а-владельца** (dealloc_cb должен находить правильную арену — на
Windows `__thread _t_arena` это арена B, а слот из A; нужен arena-aware
dealloc: определить владельца по адресу, а не по TLS).

### 5.4. Конфигурация и toolchain

- `NOVA_FIBER_ARENA_ENABLED` → `1` и для `_WIN32`. `fiber_arena.h:50-54`.
- `_NOVA_MCO_DESC_INIT` (`fibers.h:107-109`) — Windows-ветка с
  arena-callbacks.
- Сборка и тест-матрица — **обе** toolchain: **clang-cl и MSVC**
  (различаются SEH-семантика, `/GS`, `/EHsc`, `/guard:cf`; Nova
  поддерживает обе — `Auto` = Clang > MSVC > GCC).
- `/GS` (stack cookie) — `__report_gsfailure` делает `RtlCaptureContext`
  + читает TIB; TIB корректен → ок, но пункт тест-матрицы.
- `/guard:cf` (Control Flow Guard) — minicoro `_mco_switch` делает
  `jmpq *(%rdx)` рукописным asm-блобом (не инструментируется CFG, сам
  чек не выполняет); функции на fiber-стеке с CFG-indirect-call'ами
  работают штатно (CFG — bitmap-lookup, stack-agnostic). Скорее всего
  не-проблема — но проверить-и-снять явным пунктом матрицы, не
  игнорировать.

---

## 6. Фазы

### Ф.0 — Re-diagnosis (decision point, ~2–3 дня)

**Главное отличие от 44.3:** 4 провала шли сразу в интеграцию в рантайм.
Plan 82 начинается с **изолированного standalone C-харнесса** (вне Nova).

1. Прочитать «Неудачные попытки» 44.3. git-археология выполнена (§1.2) —
   minicoro неизменен с 2026-05-05; зафиксировать в логе.
2. Харнесс: текущий vendored minicoro `MCO_USE_ASM` + корутина на стеке
   из `VirtualAlloc(MEM_RESERVE)`. Спровоцировать намеренно:
   - **(a) lazy-commit путь A:** `PAGE_GUARD` на вершине + рост стека →
     **растит ли ядро non-primary-стек** (`MiCheckForUserStackOverflow`
     по свопнутому TEB)? Это решает A vs B (§5.1).
   - **(b) overflow:** упор в `PAGE_NOACCESS` hard-guard → `STATUS_STACK_OVERFLOW`?
   - **(c) GC-конфликт:** эмулировать conservative-scan reserved-диапазона →
     подтвердить §1.3 (должен быть AV) и подтвердить, что precise push
     `[sp, top]` (§5.2) его обходит.
   - **(d) аномалия Попыток 3–4:** воспроизвести «`main_yield` TIMEOUT»
     на минимальном репро либо доказать, что это был баг откаченного
     arena-кода (§1.4).
   - **(e) SEH `__try/__except`** через границу fiber-стека.
3. Toolchain-матрица спайка: **clang-cl И MSVC**.
4. **Выход Ф.0:** документ `82-artifacts/f0-rediagnosis.md` — (i) A или B;
   (ii) объяснение Попыток 3–4; (iii) go/no-go на Ф.1. no-go-ветка
   (новый фундаментальный блокер) → §9.

### Ф.1 — Windows arena allocation + overflow detection (~3–4 дня)

Порт `fiber_arena.c`/`.h` на Windows (новый `fiber_arena_win.c`):
- `VirtualAlloc(MEM_RESERVE)` арены; lazy commit выбранным путём (A/B).
- `PAGE_NOACCESS` hard-guard 16 KB + движущаяся `PAGE_GUARD`-вершина
  **над** minicoro-header'ом (§5.1, фикс дефекта раскладки 44.2).
- VEH, распознающий `STATUS_STACK_OVERFLOW` / AV в guard-зоне →
  «`nova: fiber stack overflow in slot N`» (паритет с Linux
  `_arena_sigsegv_handler`, `fiber_arena.c:95-149`). При пути A — VEH
  только диагностический; при B — ещё и commit happy-path.
- `MEM_DECOMMIT` батчем на idle; arena-aware dealloc (§5.3).
- `FlsAlloc`/`FlsCallback` cleanup.
- `_NOVA_MCO_DESC_INIT` Windows-ветка; `NOVA_FIBER_ARENA_ENABLED=1`.
- `test_runner.rs`: линковать `fiber_arena_win.c` под Windows-toolchain.

### Ф.2 — GC integration: precise push (~3 дня)

- `fiber_gc(_win).c`: `GC_set_push_other_roots` callback (§5.2) — обход
  всех worker-очередей, push suspended fibers + suspended native-стеков.
- `GC_set_stackbottom` на каждом switch-in/switch-out (хуки в `fibers.h`/
  `runtime.c`); `GC_get_my_stackbottom` в прологе worker'а.
- worker записывает `saved_native_sp` перед `mco_resume`.
- Удалить любую зависимость от плоского `GC_add_roots` на Windows-пути.
- GC-stress: 10k fiber'ов, no leak, no false-retention, commit-charge
  пропорционален активным fiber'ам.
- **Тест-якорь корректности П3:** GC форсируется, пока fiber активен, при
  живых указателях И на fiber-стеке, И на suspended native-стеке → объект
  жив после resume (нет UAF), сканер не падает (нет AV).

### Ф.3 — Cross-thread migration correctness (~2 дня)

- fiber, начатый на worker A, украден work-stealing'ом, резюмлён на B:
  TIB B корректен, SEH работает, guard-page срабатывает, GC видит fiber
  через `push_other_roots`.
- `GC_set_stackbottom` на B; `mco_current_co` консистентность.
- arena-aware dealloc слота, мигрировавшего из A и завершённого на B (§5.3).
- home-scope pinning (`runtime.c`) не ломается.

### Ф.4 — Production test matrix (~3 дня)

Полная матрица §7 — без упрощений: **обе** toolchain (clang-cl + MSVC),
**оба** режима (cooperative + M:N work-stealing).

### Ф.5 — Включить M:N на Windows + parity-бенчмарк (~2 дня)

- Сейчас M:N-тесты на Windows платформо-агностичны (honest-sentinel `0`
  через `fibers.slot_count() == 0`). Сделать так, чтобы Windows реально
  исполнял многопоточный M:N путь (gate: Ф.0–Ф.4 зелёные + §1.5 GC-модель
  закрыта — иначе включать M:N небезопасно).
- **Написать отсутствующий context-switch бенчмарк** (`bench/micro/` —
  его нет): cost `mco_resume`/`mco_yield`. Ориентиры: Boost.Context
  ~10–20 ns asm-своп; Go goroutine switch ~десятки ns. Цель — не хуже
  текущего Linux-asm-пути (Windows-asm свопает ещё и TIB — измерить
  дельту честно).

### Ф.6 — spec + закрытие 44.3 + (опц.) Linux-унификация (~1 день)

- D97 (fiber arena spec, `06-concurrency.md`) — Windows-стратегия;
  коррекция «growable» → «lazy-commit large-reserve»; зафиксировать
  GC-модель precise-push.
- 44.3 → шапка «superseded by Plan 82»; починить битую ссылку в 44.3
  (`82-windows-fiber-tib-unblock.md` → `82-windows-fiber-arena.md`).
- `docs/simplifications.md`, `docs/project-creation.txt`, README,
  `docs/plans/README.md`.
- **Опционально (gated):** мигрировать Linux-путь 44.2 на ту же
  `push_other_roots`-модель (§5.2) — снимает over-scan мёртвых страниц и
  латентный вопрос native-стеков. Только при «0 регрессий на Linux», иначе
  honest-defer отдельной задачей.

---

## 7. Production-grade тест-матрица (Ф.4, без упрощений)

| Сценарий | Что проверяется |
|---|---|
| SEH `__try/__except` внутри fiber | unwind корректен, не уходит за границы fiber-стека |
| SEH unwind ЧЕРЕЗ границу fiber → caller | `RtlVirtualUnwind` останавливается по `StackBase/StackLimit` |
| Stack overflow (глубокая рекурсия) | упор в guard → `STATUS_STACK_OVERFLOW` → детерминированная ошибка, header не corrupt'нут до фолта (§5.1) |
| Stack-clash (один кадр > guard) | `__chkstk`-пробинг ловит — нет проскока guard'а |
| `/GS` stack cookie (MSVC) | сборка с `/GS` — нет ложных `__report_gsfailure` |
| `/guard:cf` Control Flow Guard | switch + indirect-call'ы на fiber-стеке работают |
| Deep call ≈ потолок large-reserve | lazy commit растёт корректно до потолка, затем overflow |
| Cross-thread migration (work-stealing) | fiber A→steal→B: TIB, SEH, guard, GC-видимость на B |
| **GC во время активного fiber'а** | живые указатели на fiber- И native-стеке → no UAF, сканер не падает (П3-якорь) |
| GC stress: 10k fibers | no leak, no false-retention, commit-charge ограничен |
| Long soak: 10⁶ spawn/yield | без деградации, без роста commit |
| clang-cl И MSVC | вся матрица на обеих toolchain |
| cooperative + M:N work-stealing | оба режима планировщика |
| Debugger sanity | стек активного fiber'а читаем в отладчике (как минимум — не падает) |

Негативные тесты: overflow-detection даёт **детерминированную**
диагностику (не UB); cross-thread двойной resume отвергается планировщиком;
скан reserved-диапазона при ошибочной регистрации root → ловится в CI
(guard-тест на §1.3).

---

## 8. Acceptance (измеримый)

- Windows исполняет M:N fiber-workload (включая work-stealing migration)
  без hang; вся матрица §7 зелёная на clang-cl + MSVC.
- Stack overflow → **детерминированная** ошибка «fiber stack overflow»
  (паритет с Linux 44.2 и с Rust guard-page-abort; **лучше** текущего
  Windows — тихая порча heap).
- GC-модель строго корректна: тест-якорь П3 (GC mid-fiber, живые
  указатели на обоих стеках) — зелёный; **0** `GC_add_roots`-записей на
  арену; сканер не читает reserved-память.
- Lazy commit измеримо работает: N fiber'ов потребляют commit-charge
  пропорционально реально использованной глубине, не `N × reserve`.
- Эффективный потолок стека — large-reserve (8 MB, паритет с Linux 44.2;
  ≫ текущих 56 KB).
- Context-switch не медленнее текущего Linux-asm-пути (бенчмарк Ф.5;
  TIB-своп-дельта измерена и задокументирована).
- §1.5-долг закрыт: Windows GC-модель больше не полагается на
  «single-thread cooperative» инвариант.
- 0 регрессий в полном прогоне (Windows + Linux).

---

## 9. Честная рамка риска

- **Наиболее вероятный исход:** TIB-плюмбинг уже есть (§1.1, верифицировано);
  настоящий блокер — GC-scan vs reserved-память (§1.3, верифицировано из
  кода и из Попыток 1–2); фикс известен — precise push (§5.2). Это
  **решаемо** — паритет с уже-работающей Linux-arena 44.2 по эффекту,
  строже неё по GC-точности. Вердикт 44.3 «fundamentally blocked»
  переоценён: TIB-диагноз опровергнут (§1.2).
- **Остаточный риск №1 — путь lazy-commit.** Если Ф.0 покажет, что ядро
  не растит non-primary-стеки (путь A не работает) — fallback на путь B
  (VEH); B рабочий, но с латентностью. Не блокер, деградация качества.
- **Остаточный риск №2 — аномалия Попыток 3–4.** Если Ф.0 не воспроизведёт
  и не объяснит «`main_yield` TIMEOUT» — это сигнал второго, неизвестного
  бага; Ф.1 не начинать до объяснения.
- **Остаточный риск №3 — новый фундаментальный блокер.** Если Ф.0-спайк
  выявит неустранимое поведение Windows (напр. невозможность
  commit-безопасного скана даже с precise push) — честный исход:
  задокументировать точно, оставить calloc-fallback, понизить scope до
  «M:N на Windows на fixed-стеках» (без lazy-commit arena, но с корректной
  GC-моделью §5.2 — она от lazy-commit не зависит и ценна сама по себе).
- **Отличие от 44.3:** 44.3 шёл сразу в интеграцию 4 раза. Plan 82 —
  re-diagnosis → standalone-спайк → только потом интеграция. Провал Ф.0
  дёшев и диагностичен.

---

## 10. Связь

- [Plan 44.3](44.3-fiber-arena-windows.md) — 4 попытки 2026-05-13/14;
  документированный root cause опровергнут §1.1–1.2. Историческая запись.
- [Plan 44.2](44.2-fiber-arena-posix.md) ✅ — Linux/macOS arena;
  reference-семантика (mmap `MAP_NORESERVE` lazy commit, 16 KB guard).
  Windows-arena = её порт по эффекту; GC-модель Plan 82 (§5.2) **строже**
  44.2 и может быть бэкпортирована (Ф.6).
- [Plan 44.5](44.5-work-stealing-scheduler.md) ✅ — work-stealing;
  мигрирует fiber'ы между worker'ами → требование П4.
- [Plan 44](44-mn-runtime-roadmap.md) — M:N umbrella.
- [Plan 27](27-gc-switch.md) ✅ — Boehm conservative GC default; источник
  ограничения §4 (нет growable-by-copy) и API §5.2.
- [Plan 83](83-mn-default-on-gomaxprocs.md) — M:N по умолчанию; флип
  дефолта (его Ф.5) gated на Plan 82 + GC-safety.
- Внешние ориентиры: Go runtime (stackful, own scheduler, copy-grow),
  corosensei / Boost.Context (stackful + TIB-fixup — уровень, на котором
  minicoro asm уже находится), tokio / JS (stackless — этой задачи не
  имеют).
