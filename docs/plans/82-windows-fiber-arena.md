// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 82: Windows fiber stack arena — re-diagnosis + production-реализация

> **Создан:** 2026-05-21. **Редакция 4** (2026-05-21): сверка GC-дизайна
> против minicoro `mco_state`/`_mco_create_context` — исправлен реальный
> баг (колбэк пропускал `MCO_NORMAL`-fiber'ы → UAF) и формула раскладки
> слота; добавлен push header-блока (defense-in-depth) + честный паритет
> по числу fiber'ов с Go (§3). Редакция 3 — верификация против
> `runtime.c`/`alloc_boehm.c`/Boehm 8.2.8, GC-дизайн на проверенных API,
> подпроблема П5 (GC↔switch атомарность).
> **Статус:** ✅ **ЗАКРЫТ ЦЕЛИКОМ (Ф.0–Ф.6, 2026-05-22).** Windows
> fiber-стеки переведены с minicoro-default calloc на lazy-commit
> large-reserve arena (`fiber_arena_win.c`) с полной GC-интеграцией;
> arena сделана **M:N-safe** (cross-thread migration, multi-worker GC).
> Полный `nova test` (clang): **1065 PASS / 0 FAIL / 56 SKIP** — 0
> регрессий (Ф.4). Context-switch бенчмарк (`f5_ctxswitch_bench.c`):
> **16–20 ns/switch** — паритет с Boost.Context, arena 0 ns к
> переключению (Ф.5). spec D97 ред. 2, Plan 44.3 → superseded (Ф.6).
> Standalone-харнессы зелёные на MSVC + clang-cl. Опциональная
> Linux-унификация на registry+push — honest-defer отдельной задачей.
>
> **Followup (2026-05-23):** оба пункта `simplifications.md`
> закрыты. **`nova test --toolchain msvc`** теперь работает —
> **1049 PASS / 16 FAIL / 56 SKIP** (старт 0/753): починены `/Fo`,
> C1041 PDB, C2065 GCC-builtin (через force-инклюд `nova_msvc_compat.h`)
> + targeted codegen-фиксы (struct-cast, empty-record, fail-loudly).
> Bench `Nova_Error_static_new()` — корень в missing-import
> `bench/micro/*.nv`; fix + codegen `E_UNKNOWN_TYPE_METHOD` strict-check.
> См. `[M-82-msvc-novatest]` / `[M-82-bench-c-harness]`.
> **Ф.1 и Ф.2 слиты** — Ф.1 в одиночку регрессирует: §1.6-допущение
> «per-thread скан корректно покрывает running fiber» ОПРОВЕРГНУТО
> эмпирически. Ключевые находки Ф.1/Ф.2: (1) `GC_push_all` не
> масштабируется → `GC_push_all_eager`; (2) idle-batch decommit
> деградирует → послотный; (3) slot_count 4096→16384 (Windows).
> **Ф.3** — arena переработана для M:N: heap-арены в глобальном списке,
> atomic bitmap, address-based cross-thread dealloc, multi-arena
> GC-колбэк (fiber-стеки + native-стеки всех worker'ов),
> worker-арена lifecycle. Отчёт — `82-artifacts/f1-report.md`.
> **План закрыт.** Остаточный followup — опциональная Linux-унификация
> (§5.2, Ф.6) — вынесена в отдельную задачу.
> **Приоритет:** P2 — Windows работает для single-thread cooperative
> (calloc-fallback), но без lazy-commit, без overflow-detection и **без
> какой-либо GC-интеграции fiber-стеков** (§1.5 — подтверждено
> `runtime.c`). Долг паритета с Linux/macOS (Plan 44.2) и Go/Rust.
> **Родитель:** [Plan 44](44-mn-runtime-roadmap.md) (M:N runtime).
> **Заменяет:** [Plan 44.3](44.3-fiber-arena-windows.md) — 4 неудачные
> попытки 2026-05-13/14; документированный root cause **опровергнут**
> (§1.1–1.2). 44.3 остаётся историческим документом.

---

## 1. Перепроверка с чистого листа — что подтверждено исходниками

План верифицирован против реального кода. Центральный тезис подтверждён,
настоящий блокер найден точно, **инструменты для корректного решения
проверены на наличие** (§1.6), а GC-дизайн переписан — он существенно
отличается от первых двух редакций.

### 1.1. minicoro уже свопает TIB — ВЕРИФИЦИРОВАНО

`compiler-codegen/nova_rt/minicoro.h`:

- Backend (378–404): `_WIN32 && ((__GNUC__ && __x86_64__) || (_MSC_VER &&
  _M_X64))` → **`MCO_USE_ASM`**. Windows x64 + clang-cl/MSVC всегда
  asm-путь → custom `alloc_cb`/`dealloc_cb` **honored**.
- `_mco_ctxbuf` (665–672): помимо регистров — `fiber_storage`,
  `dealloc_stack`, `stack_limit`, `stack_base`.
- `_mco_switch_code` (688–750): asm читает `%gs:0x30` (TEB self → `r10`)
  и **на каждом switch** свопает TEB↔контекст:
  `0x08`↔`NT_TIB.StackBase`, `0x10`↔`NT_TIB.StackLimit`,
  `0x1478`↔`TEB.DeallocationStack`, `0x20`↔`NT_TIB.FiberData`
  (коррекция редакции 1: `0x20` — `FiberData`/`Version`-union, не
  `ArbitraryUserPointer`, тот на `0x28` и не трогается).
- `_mco_makectx` (755–768) инициализирует `stack_base`/`stack_limit`/
  `dealloc_stack` на границы корутинного стека.
- `mco_coro` struct (288–307) **публично экспонирует** `void* stack_base`
  и `size_t stack_size` — с комментарием «can be used to scan memory in a
  garbage collector». `void* context` — указатель на внутренний
  `_mco_ctxbuf` (saved SP/регистры). `mco_status(co)` (331) даёт
  `MCO_SUSPENDED`/`MCO_RUNNING`/`MCO_DEAD`.

То есть minicoro делает то же, что corosensei (Rust) и Boost.Context
(C++): синхронизирует TIB на switch. На x64 этого достаточно для
table-based SEH; TEB exception-list chain (`%gs:0x00`) — механизм x86-32,
minicoro его справедливо не трогает.

### 1.2. git-доказательство: 44.3 использовал именно этот minicoro

`git log --follow -- compiler-codegen/nova_rt/minicoro.h` → **один
коммит** `3d2b562` от **2026-05-05**, файл не менялся. Попытки 44.3 —
**2026-05-13/14**, на 8 дней позже. Гипотеза 44.3 «старый minicoro без
Windows-asm» **ложна**. Вывод однозначен: диагноз 44.3 «MCO_USE_ASM не
обновляет TIB» — **definitive misdiagnosis**; TIB обновлялся всё время.

### 1.3. Настоящий блокер — GC-scan reserved-страниц (верифицировано)

`fiber_arena.c` (Linux/macOS-реализация 44.2) регистрирует арену как
GC-root **плоским диапазоном** — `fiber_arena.c:254`:

```c
GC_add_roots(a->base, a->base + new_high * a->slot_size);
```

Boehm — conservative collector: зарегистрированный root читается
**по-байтно** на mark-фазе. На Linux безопасно — арена `mmap`
`MAP_NORESERVE`, чтение незакоммиченной страницы даёт zero-page от ядра.
**На Windows чтение `MEM_RESERVE`-но-не-`MEM_COMMIT` страницы —
`STATUS_ACCESS_VIOLATION`.** Любой lazy-commit-дизайн с плоским
`GC_add_roots` уронит Boehm-сканер на первой незакоммиченной странице.

Это **в точности** провал «Попытки 1» 44.3 (VEH коммитил каждую
тронутую сканером страницу → вся арена resident) и «Попытки 2»
(eager per-slot commit → commit-charge взрыв). У обоих провалов
**известный, верный root cause**, и он **не опровергнут**. Опровергнут
лишь вывод «Попытки 4» («TIB»).

**Следствие:** плоский `GC_add_roots` поверх арены — НЕ переносимая на
Windows стратегия. Production-дизайн обязан пушить в GC **только
закоммиченные** диапазоны — через `GC_set_push_other_roots`/`GC_push_all`
(§5.2), не `GC_add_roots`. Редакции 1–2 говорили «перенести active-range
root из 44.2» — это **неверно** и переписано.

### 1.4. Аномалия Попыток 3–4 — остаётся необъяснённой → Ф.0

Провалы Попыток 1–2 объяснены. Провалы Попыток 3–4 («даже простой
`main_yield` TIMEOUT 60+ сек», commit per-slot, no decommit) — нет:
при закоммиченном слоте и без VEH `main_yield` зависать не должен.
Кандидаты: баг откаченного arena-кода (учёт слотов / guard `PAGE_NOACCESS`
внутри usable-региона / выравнивание); decommit во время switch; что-то
в Boehm `GC_THREADS`. Это **вторая, отдельная аномалия** — Ф.0 обязана её
воспроизвести/исключить на изолированном харнессе.

### 1.5. Текущая Windows GC-модель fiber-стеков — НЕ хрупкая, а отсутствует

`runtime.c` подтверждает (перепроверка редакции 3): **никакой
интеграции fiber-стеков с GC на Windows нет вообще.**

- Worker (`_worker_main`, `runtime.c:240-245`) регистрируется в Boehm
  через `GC_register_my_thread(&sb)`, где `sb` ← `GC_get_stack_base` —
  это **native** стек потока, снятый один раз при старте.
- Никаких `GC_set_stackbottom` / `GC_add_roots` / `push_other_roots` для
  fiber-стеков (`runtime.c` — 0 вхождений). Worker просто `mco_resume(co)`.
- Fiber-стеки на Windows — `calloc(56 KB)` (minicoro default,
  `fibers.h:108`), в C-heap, **не зарегистрированы** как GC-root.

Когда fiber бежит на worker'е и аллоцирует → `GC_malloc` → возможный
collect → Boehm сканирует worker по `[CONTEXT.Rsp, зарегистрированный
native base]`. `CONTEXT.Rsp` — на calloc'нутом fiber-стеке. Корректность
сейчас держится либо на «single-thread cooperative + GC не успевает
запуститься в опасный момент», либо на удачном clamp'е Boehm (§1.6) —
**инвариант не доказан**. `nova_alloc`→`GC_malloc` может collect'ить на
любой аллокации, в т.ч. глубоко в fiber'е. Под M:N инвариант заведомо
ложен.

**Следствие:** Plan 82 — не «добавить lazy-commit поверх рабочей модели».
Это **первая корректная GC-интеграция fiber-стеков на Windows** (Ф.2).
Без неё включать M:N на Windows (Ф.5) небезопасно.

### 1.6. Инструменты для корректного решения — проверены на наличие

Перепроверка vendored-зависимостей: всё, на чём строится §5, **есть**.

- **Boehm 8.2.8 публичные API** (`vcpkg_installed/.../include/gc/`):
  - `GC_set_push_other_roots(GC_push_other_roots_proc)` — `gc_mark.h:311`.
  - `GC_push_all(void* lo, void* hi)` — `gc_mark.h:300` (conservative
    push диапазона как корней — ровно то, что нужно для стека).
  - `GC_set_stackbottom` / `GC_get_my_stackbottom` — `gc.h:1686/1675`.
  - `GC_call_with_alloc_lock` — `gc.h:1472`.
  - `GC_register_my_thread` / `GC_get_stack_base` — уже используются
    (`runtime.c:242`).
  - `GC_stackbottom` (глоб. переменная) — `GC_ATTR_DEPRECATED`, не
    использовать; только `GC_set_stackbottom`.
- **bdwgc на Windows клампит per-thread stack-scan через `VirtualQuery`.**
  Vendored source `.../blds/bdwgc/.../win32_threads.c`: `GC_get_stack_min`
  (`:1552`, VirtualQuery downward к allocation base) применяется в
  `GC_push_stack_for` (`:1789/1815/1818`). То есть когда Boehm сканирует
  worker, чей `CONTEXT.Rsp` оказался на fiber-стеке, он **сам клампит**
  скан к закоммиченному региону, в котором лежит `Rsp` → fault-free и
  корректно. Это снимает страшный «mid-switch race» **именно на Windows**
  (§П5). На Linux такого clamp'а нет — там это остаётся отдельным
  вопросом (§5.2, followup).
- **minicoro** экспонирует `mco_coro.stack_base`/`stack_size` публично
  (§1.1) → диапазон стека для скана берётся без доступа к internals.

**Вывод §1:** TIB решён (1.1); диагноз 44.3 опровергнут (1.2); настоящий
блокер — GC-scan reserved-памяти (1.3); GC-интеграции на Windows нет
вообще (1.5); но **все API для корректного решения подтверждены** (1.6).
Plan 82 — реализуемая работа, не research.

---

## 2. Реальная постановка — пять подпроблем

TIB — **не** подпроблема (§1.1). Реальная работа:

- **П1. Lazy-commit аллокация.** `VirtualAlloc(MEM_RESERVE)` + commit
  страниц по факту роста. Гипотеза (§5.1): после TIB-свопа minicoro-стек
  неотличим от `CreateFiber`-стека → ядро растит его само через
  `PAGE_GUARD` (`MiCheckForUserStackOverflow` сверяется с
  `TEB.DeallocationStack/StackBase`). Ф.0 проверяет; §5.1 даёт fallback.
- **П2. GC-scan без commit-взрыва.** §1.3 — никакого плоского
  `GC_add_roots`. Сканировать только закоммиченные диапазоны через
  `GC_set_push_other_roots`/`GC_push_all` (§5.2).
- **П3. Полнота GC-root покрытия.** Stackful-рантайм на conservative GC
  обязан просканировать **все** категории стеков:
  1. running fiber (на каждом worker'е);
  2. suspended fibers (включая вложенные resume — `prev_co`-цепочку);
  3. suspended native worker/scheduler-стеки (пока worker крутит fiber,
     его native-стек со scope-переменными `NovaFiberQueue` подвешен);
  4. managed heap — Boehm трассирует сам.
  Пропуск (3) — реальный UAF: `NovaFiberQueue` на native-стеке держит
  указатели на heap-массивы `fiber_ctx[]` — GC-root'ы для `SpawnCtx`.
- **П4. Cross-thread migration.** Plan 44.5 work-stealing мигрирует
  fiber'ы между worker'ами (`runtime.c`: `nova_deque_steal` →
  `mco_resume` на другом потоке). GC-модель (§5.2) не должна зависеть от
  того, какой worker аллоцировал слот fiber'а.
- **П5. GC↔switch атомарность.** Под M:N collect одного worker'а может
  застать другой worker внутри `_mco_switch` (смена `rsp`). Дизайн обязан
  быть корректен и в этом окне. §5.2 показывает, что на Windows это
  **закрыто по построению** (§1.6 VirtualQuery-clamp + не-регистровые
  границы push-колбэка); на Linux — открытый followup.

---

## 3. Сравнение с Go / Rust / TS — честная рамка

| Аспект | Go (goroutine) | Rust — tokio | Rust — corosensei / Boost.Context | TS / JS | **Nova сейчас (Win)** | **Nova — цель Plan 82** |
|---|---|---|---|---|---|---|
| Модель корутины | stackful | **stackless** (async FSM) | stackful | **stackless** (event loop) | stackful (minicoro asm) | stackful |
| Отдельный стек на корутину | да | нет | да | нет | да | да |
| Windows TIB на switch | runtime сам | н/п | свопает `StackBase/Limit/Dealloc` | н/п | **minicoro asm свопает ✅** | то же ✅ (есть) |
| Lazy commit стека | да | н/п | реализация-зависимо (часто eager) | н/п | **нет** (56 KB upfront) | **да** — OS-native grow / VEH fallback |
| Stack overflow detection | morestack-проверка в прологе | guard page → abort | guard page → abort | `RangeError` | **нет** (тихая порча) | guard + `STATUS_STACK_OVERFLOW` → детерминированная ошибка |
| GC-видимость suspended fiber-стеков | precise stackmaps | н/п | зависит от встройки | н/п | **отсутствует** (§1.5) | **precise push** `[committed_low, top]` каждого (§5.2) |
| GC↔switch race под M:N | precise, safepoints | н/п | — | н/п | необработан | **закрыт по построению** на Windows (§5.2/П5) |
| Growable стек (copy-grow) | да (precise GC, pointer-fixup) | н/п | нет | н/п | нет | **нет — несовместимо с conservative GC** (§4) |
| Cross-thread migration | да | да | да (если `Send`) | нет | Linux/macOS да; Win — fallback | **да** (П4) |
| Эффективный потолок стека | ~1 GB (растёт) | н/п | fixed reserve | ~глубина движка | **56 KB (опасно мало)** | large-reserve (8 MB, как Linux 44.2) |
| Стоимость роста | copy O(размер) | н/п | нет роста | н/п | нет роста | нет copy — commit O(страница) |

**Честный разбор:**

1. **tokio и TS/JS этой задачи не имеют** — они stackless (корутина =
   конечный автомат на стеке потока). Nova сознательно выбрала stackful
   (алгебраические эффект-хендлеры + произвольная глубина без
   function-coloring). Plan 82 не пересматривает выбор, а честно
   фиксирует: stackful → эта работа существует. (Историческая ремарка:
   `node-fibers` давал JS stackful через libcoro — заброшен.)
2. **Прямой ориентир — Go и corosensei/Boost.Context.** По TIB Nova уже
   на их уровне.
3. **Где Nova реально позади сегодня (Windows):** нет lazy-commit
   (56 KB upfront, и потолок 56 KB опасно мал); нет overflow-detection
   (тихая порча); **нет GC-интеграции fiber-стеков** (§1.5). Plan 82
   закрывает все три → паритет.
4. **Growable-by-copy** Nova не может (§4) — следствие conservative GC,
   компенсируется large-reserve + lazy-commit.
5. **Где Nova после Plan 82 НЕ хуже / лучше:**
   - **Лучше tokio/TS:** реальный стек на корутину → естественный
     последовательный код, без `async`-окраски, глубокая рекурсия и
     эффект-хендлеры.
   - **Лучше Go в одном аспекте:** нет mid-execution copy-grow → нет
     требования precise stackmap, нет copy-pause, **стабильные адреса
     стека** (надёжнее для FFI и для самого conservative GC).
   - **Паритет с Go по памяти:** Go растит стек по факту, Nova
     lazy-commit'ит по факту.
   - **Паритет с Go по числу fiber'ов:** Go держит миллионы горутин
     (стек 2–8 KB). Nova: `slot_count` слотов × 8 MB **резерва** на
     worker; резерв виртуального адреса бесплатен (64-bit user-space —
     128 TB), commit-charge idle-fiber'а ≈ 1–2 страницы ≈ как у горутины.
     Архитектурного потолка ниже Go нет; текущий `slot_count` (4096,
     наследие 44.2) конфигурируем, неограниченный рост — arena-chaining
     (роадмап Plan 44, вне scope 82). Честно: без chaining это
     **настраиваемый**, а не **безграничный** потолок — единственное
     место, где Nova по умолчанию строже Go.
   - **Паритет с Go по диапазону GC-скана:** precise push сканирует
     закоммиченный `[committed_low, top]` каждого fiber'а — практически
     тот же объём, что сканирует Go (`[sp, hi]`). Разница —
     conservative-vs-precise маркировка (свойство Boehm, не плана).
   - **Честно позади Go:** Go умеет **сжимать** живой стек (copy в
     меньший буфер); Nova может только decommit'ить **освобождённый**
     слот, не сжать живой. Осознанный трейд.

---

## 4. Коррекция целей 44.3: что недостижимо в принципе

Plan 44.3 формулировал цель как «lazy commit + **growable stacks** +
single GC root». Два из трёх — убрать:

- **«Growable-by-copy» недостижим.** Go копирует растущий стек
  (`runtime.copystack`), т.к. у Go **precise** stackmaps (pointer-fixup
  после copy). Nova — **Boehm conservative GC** (`alloc_boehm.c`:
  `GC_set_all_interior_pointers(1)`): сборщик не знает, слово на стеке —
  указатель или целое → не может переписать указатели → после copy
  internal-указатели битые. Единственная совместимая стратегия большого
  стека — **fixed virtual reserve + lazy physical commit** (виртуальный
  адрес неизменен, физический backing растёт по page-fault). Это и есть
  цель Plan 82.
- **«Single GC root» (плоский `GC_add_roots`) недостижим на Windows** —
  §1.3. Замена — **push-колбэк** (§5.2): число `GC_add_roots`-записей на
  fiber-арену = **0**, проблема `MAX_ROOT_SETS=128` снимается без скана
  reserved-памяти.

**Цель Plan 82 (исправленная):** lazy-commit large-reserve fiber-стеки на
Windows + детерминированная overflow-detection + строго корректная,
commit-безопасная GC-видимость **всех** категорий стеков (§П3) — паритет
с Linux-arena 44.2 по эффекту и **строже** неё по GC-точности.

---

## 5. Production-grade дизайн

### 5.1. П1 — память: reserve + lazy-commit + guard

**Раскладка слота** (стек растёт вниз, high → low):

```
slot_base (low)                                          slot_top (high)
├─ hard guard ─┬─ minicoro header ─┬──────── usable stack ────────────┤
│ PAGE_NOACCESS│ mco_coro+storage  │ PAGE_GUARD-вершина движется вниз →│
│  (16 KB)     │ (align16, ≈1 KB)  │ закоммичено по факту роста        │
│              ↑ ЗДЕСЬ ставим движущуюся PAGE_GUARD-вершину            │
```

- `VirtualAlloc(NULL, slot_size, MEM_RESERVE, PAGE_NOACCESS)` — резерв
  без commit-charge. `slot_size` — 8 MB (как Linux 44.2).
- **Hard guard:** нижние `NOVA_FIBER_GUARD_SIZE` (16 KB — защита от
  stack-clash единичным большим кадром) держатся `PAGE_NOACCESS` навсегда.
- **Фикс дефекта раскладки 44.2.** minicoro размещает в выделенном блоке
  **четыре** 16-выровненных секции подряд от low-адреса —
  `[mco_coro][ctxbuf][storage][stack]` (`_mco_create_context` /
  `_mco_init_desc_sizes`). Стек растёт вниз и при переполнении **сначала
  корёжит `storage`/`ctxbuf`/`mco_coro`-header**, затем упирается в
  guard → битый switch-back, ложная диагностика. Production-фикс:
  поставить движущуюся `PAGE_GUARD`-вершину на **нижней границе
  stack-секции** (смещение = сумма размеров трёх предшествующих секций;
  вычисляется из layout в Ф.1), выше header-блока. Linux 44.2 этот
  дефект не чинил — Plan 82 чинит (это «без упрощений»).
- **Lazy commit — два пути, выбор в Ф.0:**
  - **Путь A (предпочтительный) — OS-native grow.** После TIB-свопа
    minicoro-стек для MM ядра неотличим от `CreateFiber`-стека (та же
    `VirtualAlloc`-область + TEB-своп). Ядро растит его штатно:
    `PAGE_GUARD`-фолт → `MiCheckForUserStackOverflow` сверяется с
    `TEB.DeallocationStack/StackBase` → коммитит следующую страницу,
    опускает guard, обновляет `TEB.StackLimit`; у нижней границы →
    `STATUS_STACK_OVERFLOW`. minicoro сохраняет обновлённый `StackLimit`
    в ctxbuf на switch-out → когерентно. Если Ф.0 подтверждает A —
    custom VEH для happy-path не нужен. `nova_fiber_alloc` коммитит лишь
    начальное окно у вершины (minicoro трогает dummy-return-address на
    вершине в `_mco_makectx`) + ставит первую `PAGE_GUARD`.
  - **Путь B (fallback) — VEH lazy commit.** Если Ф.0 покажет, что ядро
    не растит non-primary-стеки: `AddVectoredExceptionHandler` → AV
    внутри арены → `VirtualAlloc(page, MEM_COMMIT)` →
    `EXCEPTION_CONTINUE_EXECUTION`. Строго: трогать только фолты внутри
    своей арены, вне — `EXCEPTION_CONTINUE_SEARCH`. Минус B — латентность
    + page-by-page фолты при `__chkstk`-пробинге.
- **`__chkstk` — встроенная защита от stack-clash.** clang-cl/MSVC эмитят
  `__chkstk` в прологе функций с локалами > 1 страницы — пробинг по
  странице вниз драйвит guard-grow. Это Windows-аналог Linux
  `-fstack-clash-protection`; отдельный флаг не нужен.
- **minicoro собственная overflow-проверка** (`mco_yield`,
  minicoro.h:1826) — coarse backstop: сверяет `magic_number` + границы
  SP, возвращает `MCO_STACK_OVERFLOW` *постфактум* на yield. Guard-страница
  ловит *немедленно* на фолте и точнее — она первична; minicoro-проверка
  остаётся вторым рубежом, отключать её не нужно.
- **Decommit при освобождении слота.** `VirtualFree(committed_range,
  MEM_DECOMMIT)` снимает commit-charge (`MEM_RESET`/`DiscardVirtualMemory`
  НЕ снимают — не годятся). Антипаттерн syscall-storm (урок Linux P41-3):
  **батчем на idle** (`slots_active == 0`) либо decay-очередью. Порядок:
  снять fiber из GC-registry (§5.2) → потом decommit; оба под
  `GC_call_with_alloc_lock` → атомарно относительно GC (decommit'нутый
  слот не будет просканирован).
- **Арена — аллокатор, не GC-механизм.** В 44.2 непрерывная арена была
  нужна ради одного `GC_add_roots`. Дизайн §5.2 (`push_other_roots`)
  делает `GC_add_roots` count = 0 → арена больше **не связана с GC**.
  Оставляем как аллокатор: reuse слотов без `VirtualAlloc`/`VirtualFree`-
  churn + O(1) `arena_contains` для overflow-handler'а.
- **TLS-время жизни арены** — `FlsAlloc` + `FlsCallback` (Windows-аналог
  `pthread_key` из 44.2; `__attribute__((destructor))` — process-exit,
  не годится).
- **Commit-charge / 32-bit.** 64-bit: 4096 × 8 MB = 32 GB резерва на
  поток — тривиально, commit-charge = только закоммиченное. 32-bit —
  downsize (как `fiber_arena.h:70-74`).

### 5.2. П2+П3+П5 — GC: registry + precise push, race-free by construction

**Никакого `GC_add_roots` поверх арены.** Дизайн на проверенных API
(§1.6).

**(1) Глобальный реестр fiber'ов.** Сейчас его нет — fiber'ы разбросаны
по per-worker `deque`/`scope.fibers[]`/`yielded[]`/`wake_pending[]`
(`runtime.c`). Добавить интрузивный двусвязный список всех живых
`mco_coro*`: register в обёртке `mco_create`/`nova_fiber_alloc`-пути,
unregister в обёртке `mco_destroy`. Мутации реестра — под
`GC_call_with_alloc_lock` (`gc.h:1472`): collect в Boehm начинается
**после** взятия alloc-lock'а, значит register/unregister, выполненные
под этим же локом, атомарны относительно GC — колбэк (2) никогда не
увидит полу-связанный узел.

**(2) `GC_set_push_other_roots(cb)`** (`gc_mark.h:311`) — `cb` зовётся на
mark-фазе (мир остановлен). `cb` обходит реестр и для **каждого
не-`MCO_DEAD`** fiber'а пушит закоммиченные диапазоны его слота через
`GC_push_all(lo, hi)` (`gc_mark.h:300`):
- **Статус-фильтр — все три не-мёртвых состояния.** `mco_state`
  (minicoro.h:263-268): `MCO_DEAD`/`MCO_NORMAL`/`MCO_RUNNING`/
  `MCO_SUSPENDED`. Пушить нужно `RUNNING`, `SUSPENDED` **и `MCO_NORMAL`** —
  последнее это fiber, который сам запустил вложенный
  `supervised { spawn … }` и сейчас «active but not running» (резюмнул
  sub-fiber; minicoro.h:573-574 выставляет `prev_co->state = MCO_NORMAL`).
  На его стеке — живые данные; пропуск = UAF. Корректный фильтр:
  `mco_status(co) != MCO_DEAD` (`MCO_DEAD`-слот живых указателей не
  содержит — пушить безвредно, можно и пропустить).
- **Stack-секция:** `hi` = usable-top (`co->stack_base + co->stack_size`,
  публичные поля minicoro, §1.1); `lo` = `committed_low` — нижняя граница
  закоммиченного. V1: `VirtualQuery` вниз от вершины до первой
  не-`MEM_COMMIT` страницы (как Boehm в `GC_get_stack_min`). Опт.: арена
  трекает `committed_low` per-slot; либо читать
  `((_mco_ctxbuf*)co->context)->stack_limit` (saved `TEB.StackLimit` =
  committed-low — но это coupling к minicoro internals).
- **Header-блок слота** `[mco_coro][ctxbuf][storage]` (§5.1) — пушить
  отдельным `GC_push_all`. Он закоммичен от создания и содержит
  `co->user_data` — указатель на `SpawnCtx` с захватами замыкания.
  Это делает достижимость `SpawnCtx` независимой от корректности
  отдельного `fiber_ctx[]`/`ctx_pins`-рутинга (`fibers.h`) —
  defense-in-depth, не полагаемся на чужую подсистему.
- **Suspended native worker-стеки** (§П3.3): для каждого worker'а,
  крутящего fiber, `GC_push_all(saved_native_sp, native_stack_base)`.
  `saved_native_sp` worker фиксирует сам — `&local` в `_worker_main`
  перед `mco_resume` (кадры ниже — `mco_resume`/`_mco_switch`,
  GC-указателей не содержат); `native_stack_base` — из
  `GC_get_stack_base` при старте worker'а.

**Почему это commit-безопасно и race-free (П5) — по построению:**
- Границы `[committed_low, top]` — **свойства слота**, не регистры,
  снятые на лету. Их значение не зависит от того, бежит fiber или
  suspended, и не «дёргается» во время `_mco_switch`. → колбэк
  корректен независимо от того, застал ли collect какой-то worker
  mid-switch.
- `committed_low ≤ любой валидный SP` этого fiber'а (стек растёт вниз,
  страницы коммитятся монотонно, decommit только при освобождении слота
  под alloc-lock'ом). → диапазон покрывает все живые данные.
- Диапазон целиком закоммичен → `GC_push_all` его читает без AV.
- **running fiber дополнительно** покрыт штатным per-thread сканом Boehm,
  который на Windows клампится `VirtualQuery`-ем (`GC_get_stack_min`,
  win32_threads.c:1552, §1.6) к закоммиченному региону, где лежит
  `CONTEXT.Rsp` — fault-free даже mid-switch. Двойное покрытие running
  fiber'а (колбэк + per-thread скан) безвредно — conservative GC просто
  метит одно и то же дважды.
- Реестр консистентен (мутации под alloc-lock'ом, см. (1)).
- → **mid-switch GC-race (П5) на Windows закрыт**: ни один корень не
  выводится из живых регистров в момент скана.

**(3) Никаких `GC_add_roots`/`GC_set_stackbottom` на Windows-пути.**
`GC_set_stackbottom` нужен там, где нет `VirtualQuery`-clamp'а (Linux);
на Windows per-thread скан корректен сам.

**(4) Корректность decommit.** Освобождённый слот сперва снимается из
реестра, потом decommit'ится — оба под `GC_call_with_alloc_lock` →
collect не начнётся в середине → decommit'нутый слот гарантированно не в
реестре → колбэк его не тронет.

**Файлы:** новый `nova_rt/fiber_gc_win.c` (реестр + колбэк +
`GC_set_push_other_roots` регистрация); хуки register/unregister в
`fibers.h`/`runtime.c`; `saved_native_sp` — поле `NovaWorker`
(`runtime.c`), плюс эквивалент для main-thread scope.

**Linux — followup, не блокер.** На Linux нет `VirtualQuery`-clamp'а →
штатный per-thread скан worker'а, крутящего fiber, потенциально идёт по
`[fiber_sp, native_base]` (через незамапленную дыру). Plan 44.2 закрыт и
работает (флэт-root покрывает fiber-стеки; per-thread скан — открытый
вопрос, который в текущих тестах не выстреливает). Plan 82 **не трогает**
Linux-путь по умолчанию. Followup (Ф.6, gated «0 регрессий на Linux»):
перенести Linux на ту же registry+push-модель + `GC_set_stackbottom`
per-switch — это снимет и over-scan мёртвых страниц, и латентный
per-thread-вопрос. Унификация желательна, но вне критического пути 82.

### 5.3. П4 — cross-thread migration

minicoro-asm свопает TIB **текущего** потока — resume украденного fiber'а
на worker'е B настраивает TIB B на стек fiber'а. GC-модель §5.2 не
привязана к владельцу: реестр глобален, колбэк пушит fiber по его
`stack_base`/`committed_low` независимо от того, чья арена. Проверить
(Ф.3): (а) `mco_current_co`/`mco_status` консистентны после миграции;
(б) home-scope pinning (`runtime.c:330-352`) не ломается; (в)
**arena-aware dealloc** — слот fiber'а, мигрировавшего из арены worker'а
A и завершённого на B, должен освобождаться в арену **A**; на Windows
`__declspec(thread) _t_arena` — это арена B, а слот из A → dealloc обязан
определять владельца по адресу (диапазонная проверка по всем аренам), не
по TLS.

### 5.4. Конфигурация и toolchain

- `NOVA_FIBER_ARENA_ENABLED` → `1` и для `_WIN32` (`fiber_arena.h:50-54`).
- `_NOVA_MCO_DESC_INIT` (`fibers.h:107-109`) — Windows-ветка с
  arena-callbacks.
- Сборка и тест-матрица — **обе** toolchain: **clang-cl и MSVC**
  (различаются SEH, `/GS`, `/EHsc`, `/guard:cf`).
- `/GS` — `__report_gsfailure` делает `RtlCaptureContext` + читает TIB;
  TIB корректен → ок, но пункт матрицы.
- `/guard:cf` (CFG) — minicoro `_mco_switch` делает `jmpq *(%rdx)`
  рукописным asm-блобом (CFG-чек не выполняет); функции на fiber-стеке с
  CFG-indirect-call'ами работают штатно (CFG — bitmap-lookup,
  stack-agnostic). Вероятно не-проблема — но снять явным пунктом матрицы.

---

## 6. Фазы

### Ф.0 — Re-diagnosis (decision point, ~2–3 дня) — ✅ ЗАВЕРШЕНА 2026-05-22

> Итог: **GO на Ф.1, путь A + обязательный патч `ctx.stack_limit`.** Все
> 5 проверок (a-e) закрыты; §1.2/§1.3/§1.6/§5.2-модель подтверждены на
> сборке; аномалия Попыток 3-4 объяснена (баг интеграционного кода, не
> блокер); SEH детерминирован. Полный отчёт — `82-artifacts/
> f0-rediagnosis.md` §8. Артефакты: `f0_probe.c`, `f0_gc_link.c`,
> `f0_test_a.c`, `f0_test_d.c`, `f0_test_e.c`.

Изолированный **standalone C-харнесс** (вне рантайма Nova), в отличие от
4 интеграционных провалов 44.3.

1. git-археология (§1.2) выполнена — minicoro неизменен с 2026-05-05;
   зафиксировать в логе.
2. Харнесс: minicoro `MCO_USE_ASM` + корутина на `VirtualAlloc(MEM_RESERVE)`.
   Спровоцировать:
   - **(a) lazy-commit путь A:** `PAGE_GUARD` на вершине + рост стека →
     **растит ли ядро non-primary-стек** (решает A vs B, §5.1).
   - **(b) overflow:** упор в `PAGE_NOACCESS` hard-guard →
     `STATUS_STACK_OVERFLOW`?
   - **(c) GC-конфликт:** эмулировать conservative-scan reserved-диапазона
     → подтвердить §1.3 (AV) и что precise push `[committed_low, top]`
     его обходит.
   - **(d) аномалия Попыток 3–4** (§1.4): воспроизвести «`main_yield`
     TIMEOUT» либо доказать, что это был баг откаченного arena-кода.
   - **(e) SEH `__try/__except`** через границу fiber-стека.
3. **Подтвердить §1.6** на реальной сборке: (i) `GC_set_push_other_roots`/
   `GC_push_all`/`GC_call_with_alloc_lock` линкуются из статической
   `gc.lib`; (ii) перечитать vendored `win32_threads.c` —
   `GC_get_stack_min`/`GC_push_stack_for` действительно клампят
   per-thread скан (для П5).
4. Toolchain-матрица спайка: **clang-cl И MSVC**.
5. **Выход Ф.0:** `82-artifacts/f0-rediagnosis.md` — A или B; объяснение
   Попыток 3–4; подтверждение §1.6; go/no-go на Ф.1.

### Ф.1 — Windows arena allocation + overflow detection — ✅ ЗАВЕРШЕНА 2026-05-22

> Реализовано в `fiber_arena_win.c` (путь A + патч `ctx.stack_limit`,
> §5.1-раскладка, VEH-диагностика, послотный decommit, `FlsAlloc`).
> **Слита с Ф.2** — Ф.1 в одиночку регрессирует (§1.6 опроверг­нут).
> Полный отчёт + находки — `82-artifacts/f1-report.md`.

`fiber_arena_win.c`:
- `VirtualAlloc(MEM_RESERVE)` арены; lazy commit выбранным путём (A/B).
- `PAGE_NOACCESS` hard-guard 16 KB + движущаяся `PAGE_GUARD`-вершина
  **над** minicoro-header'ом (§5.1).
- VEH, распознающий `STATUS_STACK_OVERFLOW` / AV в guard-зоне →
  «`nova: fiber stack overflow in slot N`» (паритет с Linux
  `_arena_sigsegv_handler`, `fiber_arena.c:95-149`). Путь A — VEH только
  диагностический; B — ещё и commit happy-path.
- `MEM_DECOMMIT` батчем на idle; arena-aware dealloc (§5.3).
- `FlsAlloc`/`FlsCallback` cleanup.
- `_NOVA_MCO_DESC_INIT` Windows-ветка; `NOVA_FIBER_ARENA_ENABLED=1`.
- `test_runner.rs`: линковать `fiber_arena_win.c` под Windows.

### Ф.2 — GC integration: registry + precise push — ✅ ЗАВЕРШЕНА 2026-05-22 (single-thread core)

> Реализован `GC_set_push_other_roots`-колбэк в `fiber_arena_win.c`:
> precise push закоммиченных диапазонов каждого живого fiber'а +
> native-стека main-thread'а. Реестр живых fiber'ов = `used_bits`
> арены (отдельный интрузивный список §5.2 (1) не нужен для
> single-thread). **Находка:** `GC_push_all` не масштабируется →
> `GC_push_all_eager`. **Отложено в Ф.5:** обход всех per-thread арен +
> suspended worker-стеков под M:N (см. `simplifications.md`
> [M-82-gc-mn-deferred]). Отчёт — `82-artifacts/f1-report.md`.

Исходный план фазы (registry + push) — реализован в упрощённой
single-thread форме:

- `fiber_gc_win.c`: глобальный реестр fiber'ов (§5.2 (1)); register/
  unregister-хуки в `fibers.h`/`runtime.c` под `GC_call_with_alloc_lock`.
- `GC_set_push_other_roots` колбэк (§5.2 (2)): push `[committed_low, top]`
  каждого fiber'а + suspended native worker-стеков.
- `NovaWorker.saved_native_sp` (+ main-scope-эквивалент); снятие перед
  `mco_resume`.
- Удалить любую зависимость от плоского `GC_add_roots` на Windows.
- GC-stress: 10k fiber'ов — no leak, no false-retention, commit-charge
  пропорционален активным fiber'ам.
- **Тест-якорь корректности П3/П5:** GC форсируется, пока fiber активен,
  при живых указателях И на fiber-стеке, И на suspended native-стеке;
  отдельно — (а) fiber в `MCO_NORMAL` (запустил вложенный `supervised`)
  с живыми данными на своём стеке; (б) GC во время work-stealing
  миграции → объект жив после resume (нет UAF), сканер не падает (нет AV).

### Ф.3 — Cross-thread migration correctness — ✅ ЗАВЕРШЕНА 2026-05-22

> arena переработана для M:N (`fiber_arena_win.c`): heap-арены в
> глобальном append-only списке, TLS — лишь указатель; atomic bitmap
> (`_interlockedbittestandset/reset64`); **address-based** dealloc /
> committed_low / contains / VEH (арена-владелец по адресу — §5.3);
> multi-arena GC-колбэк (fiber-стеки + native scheduler-стеки ВСЕХ
> worker'ов — §П3); worker-арена lifecycle (`runtime.c::_worker_main`
> создаёт + `nova_fiber_arena_thread_exit`; shutdown — `release_retired`
> после join); `GC_add_roots(_workers)` (§П3 — scope-массивы). Полный
> `nova test` 1058/0/56, concurrency 75/75 incl M:N. `[M-82-gc-mn-
> deferred]` ✅ ЗАКРЫТ. Отчёт — `82-artifacts/f1-report.md` §7.

- fiber A → steal → B: TIB B корректен, SEH, guard-page, GC-видимость
  через реестр.
- `mco_current_co`/`mco_status` консистентность; home-scope pinning.
- arena-aware dealloc мигрировавшего слота (§5.3).

### Ф.4 — Production test matrix — ✅ ЗАВЕРШЕНА 2026-05-22

> Матрица §7 покрыта: полный `nova test` (clang) — **1065 PASS / 0 FAIL
> / 56 SKIP**, 0 регрессий; негативный тест overflow
> (`expected_runtime/fiber_stack_overflow.nv`) — детерминированный
> `STATUS_STACK_OVERFLOW`, PASS; standalone C-харнессы (SEH, `/GS`,
> `/guard:cf`, alloc/grow/`__chkstk`/reuse/overflow, GC-стресс) —
> зелёные на MSVC + clang-cl. Полный `nova test` под MSVC-toolchain
> не запускается — pre-existing баг `/Fo`-компоновки в `test_runner.rs`
> (D8036, вне scope Plan 82); MSVC-покрытие fiber-арены — через
> standalone-харнессы. Отчёт — `82-artifacts/f1-report.md` §8.

Полная матрица §7 — обе toolchain (clang-cl + MSVC), оба режима
(cooperative + M:N work-stealing).

### Ф.5 — M:N на Windows + context-switch бенчмарк — ✅ ЗАВЕРШЕНА 2026-05-22

> M:N на Windows **уже исполняется реально** — побочный результат
> Ф.1–Ф.3: arena-интеграция сделала `fibers.slot_count()` ненулевым на
> Windows (16384), honest-sentinel `0` больше не срабатывает, M:N-тесты
> исполняют настоящий путь (concurrency 75/75 incl `mn_*`).
> Context-switch бенчмарк — `82-artifacts/f5_ctxswitch_bench.c`:
> **16.4 ns/switch** (clang-cl) / **20.3 ns/switch** (MSVC) на
> arena-стеке — в классе Boost.Context (~10–20 ns); дельта arena−calloc
> +0.08…+1.3 ns (switch аллокатор-независим). TIB-своп — слагаемое
> единиц ns. Nova bench-DSL для замера непригоден (codegen-баг
> `Nova_Error_static_new()` с 0 аргументов в связке `bench`+`supervised`
> — вне scope Plan 82) → standalone C-харнесс. Отчёт —
> `82-artifacts/f1-report.md` §9.

- Сейчас M:N-тесты на Windows платформо-агностичны (honest-sentinel `0`
  через `fibers.slot_count() == 0`). Сделать так, чтобы Windows реально
  исполнял M:N путь. Gate: Ф.0–Ф.4 зелёные + GC-модель §1.5 закрыта.
- **Написать отсутствующий context-switch бенчмарк** (`bench/micro/`):
  cost `mco_resume`/`mco_yield`. Ориентиры: Boost.Context ~10–20 ns;
  Go switch ~десятки ns. Цель — не хуже Linux-asm-пути; измерить дельту
  TIB-свопа честно.

### Ф.6 — spec + закрытие 44.3 — ✅ ЗАВЕРШЕНА 2026-05-22

> spec D97 (`06-concurrency.md`) переписан в **ред. 2**: Windows-секция
> — `VirtualAlloc` lazy-commit arena вместо calloc; диагноз 44.3
> «fundamentally blocked» опровергнут; registry+push GC-модель
> зафиксирована; introspection «0 на Windows» снято; bootstrap-status +
> «что отвергнуто» дополнены. Plan 44.3 → шапка **SUPERSEDED BY Plan
> 82**, битая ссылка `82-windows-fiber-tib-unblock.md` исправлена.
> `docs/plans/README.md` (Plan 82 ✅, 44.3 superseded, строка Plan 44),
> `docs/project-creation.txt`, `docs/simplifications.md`
> (`[M-82-msvc-novatest]`, `[M-82-bench-c-harness]`),
> `nova-private/discussion-log.md` — обновлены. **Опциональная
> Linux-унификация — honest-defer отдельной задачей** (требует полного
> прогона на Linux, недоступного из текущего окружения).

- D97 (`06-concurrency.md`) — Windows-стратегия; «growable» →
  «lazy-commit large-reserve»; зафиксировать registry+push GC-модель.
- 44.3 → шапка «superseded by Plan 82»; починить битую ссылку в 44.3
  (`82-windows-fiber-tib-unblock.md` → `82-windows-fiber-arena.md`).
- `docs/simplifications.md`, `docs/project-creation.txt`, README,
  `docs/plans/README.md`.
- **Опционально (gated «0 регрессий на Linux»):** перевести Linux 44.2 на
  ту же registry+push-модель + `GC_set_stackbottom` per-switch — снимает
  over-scan мёртвых страниц и латентный per-thread-вопрос (§5.2). Иначе —
  honest-defer отдельной задачей.

---

## 7. Production-grade тест-матрица (Ф.4, без упрощений)

| Сценарий | Что проверяется |
|---|---|
| SEH `__try/__except` внутри fiber | unwind корректен, не уходит за границы fiber-стека |
| SEH unwind ЧЕРЕЗ границу fiber → caller | `RtlVirtualUnwind` останавливается по `StackBase/StackLimit` |
| Stack overflow (глубокая рекурсия) | упор в guard → `STATUS_STACK_OVERFLOW` → детерминированная ошибка; header не corrupt до фолта (§5.1) |
| Stack-clash (один кадр > guard) | `__chkstk`-пробинг ловит — нет проскока guard'а |
| `/GS` stack cookie (MSVC) | сборка с `/GS` — нет ложных `__report_gsfailure` |
| `/guard:cf` Control Flow Guard | switch + indirect-call'ы на fiber-стеке работают |
| Deep call ≈ потолок large-reserve | lazy commit растёт корректно до потолка, затем overflow |
| Cross-thread migration (work-stealing) | fiber A→steal→B: TIB, SEH, guard, GC-видимость на B |
| **GC во время активного fiber'а** | живые указатели на fiber- И native-стеке → no UAF, сканер не падает (якорь П3) |
| **GC при fiber в `MCO_NORMAL`** | родительский fiber запустил вложенный `supervised` → его стек со живыми данными просканирован (якорь П3) |
| **GC во время `_mco_switch` под M:N** | collect застаёт worker mid-switch → корректно (якорь П5) |
| GC stress: 10k fibers | no leak, no false-retention, commit-charge ограничен |
| Long soak: 10⁶ spawn/yield | без деградации, без роста commit |
| clang-cl И MSVC | вся матрица на обеих toolchain |
| cooperative + M:N work-stealing | оба режима планировщика |
| Debugger sanity | стек активного fiber'а читаем в отладчике |

Негативные тесты: overflow → **детерминированная** диагностика (не UB);
cross-thread двойной resume отвергается планировщиком; ошибочная
регистрация reserved-диапазона как root → ловится CI guard-тестом (§1.3).

---

## 8. Acceptance (измеримый)

- Windows исполняет M:N fiber-workload (включая work-stealing migration)
  без hang; матрица §7 зелёная на clang-cl + MSVC.
- Stack overflow → **детерминированная** ошибка «fiber stack overflow»
  (паритет с Linux 44.2 и Rust guard-page-abort; **лучше** текущего
  Windows — тихая порча).
- GC-модель строго корректна: якоря П3 и П5 зелёные; **0**
  `GC_add_roots`-записей на fiber-арену; сканер не читает reserved-память;
  реестр консистентен под `GC_call_with_alloc_lock`.
- Lazy commit измеримо работает: N fiber'ов потребляют commit-charge
  пропорционально реально использованной глубине, не `N × reserve`.
- Эффективный потолок стека — large-reserve (8 MB; ≫ текущих 56 KB).
- Context-switch не медленнее Linux-asm-пути (бенчмарк Ф.5).
- §1.5-долг закрыт: на Windows впервые появляется корректная
  GC-интеграция fiber-стеков; M:N на Windows перестаёт зависеть от
  недоказанного «GC не успеет collect'ить» инварианта.
- 0 регрессий в полном прогоне (Windows + Linux).

---

## 9. Честная рамка риска

- **Наиболее вероятный исход:** TIB решён (§1.1, верифицировано);
  настоящий блокер — GC-scan reserved-памяти (§1.3, верифицировано);
  фикс известен и **все нужные API подтверждены** (§1.6). Это **решаемо**.
  Вердикт 44.3 «fundamentally blocked» переоценён.
- **Остаточный риск №1 — путь lazy-commit.** Если Ф.0 покажет, что ядро
  не растит non-primary-стеки — fallback на путь B (VEH); B рабочий, но с
  латентностью. Деградация качества, не блокер.
- **Остаточный риск №2 — аномалия Попыток 3–4.** Если Ф.0 не воспроизведёт
  и не объяснит «`main_yield` TIMEOUT» — сигнал второго неизвестного
  бага; Ф.1 не начинать до объяснения.
- **Остаточный риск №3 — VirtualQuery-clamp.** §5.2/П5 опирается на то,
  что bdwgc клампит per-thread скан (`GC_get_stack_min`, §1.6). Прочитано
  в vendored source; Ф.0 шаг 3 это подтверждает на сборке. Если поведение
  иначе — running fiber придётся покрывать целиком через push-колбэк
  (тоже корректно, см. §5.2: колбэк уже пушит и running fiber'ы) +
  `GC_set_stackbottom` per-switch. Дизайн это допускает — push-колбэк
  самодостаточен.
- **Остаточный риск №4 — новый фундаментальный блокер.** Если Ф.0-спайк
  выявит неустранимое поведение Windows — честный исход: задокументировать,
  оставить calloc-fallback, понизить scope до «M:N на Windows на
  fixed-стеках» — но **GC-модель §5.2 от lazy-commit не зависит** и
  ценна сама по себе (закрывает §1.5).
- **Отличие от 44.3:** 44.3 шёл в интеграцию 4 раза. Plan 82 —
  re-diagnosis → standalone-спайк → только потом интеграция.

---

## 10. Связь

- [Plan 44.3](44.3-fiber-arena-windows.md) — 4 попытки 2026-05-13/14;
  root cause опровергнут §1.1–1.2. Историческая запись.
- [Plan 44.2](44.2-fiber-arena-posix.md) ✅ — Linux/macOS arena;
  reference по эффекту. GC-модель Plan 82 (§5.2) **строже** 44.2 и может
  быть бэкпортирована (Ф.6).
- [Plan 44.5](44.5-work-stealing-scheduler.md) ✅ — work-stealing;
  мигрирует fiber'ы → требование П4; `runtime.c` — источник фактов §1.5.
- [Plan 44](44-mn-runtime-roadmap.md) — M:N umbrella.
- [Plan 27](27-gc-switch.md) ✅ — Boehm conservative GC default; источник
  ограничения §4 и API §5.2.
- [Plan 83](83-mn-default-on-gomaxprocs.md) — M:N по умолчанию; флип
  дефолта (Ф.5) gated на Plan 82 + GC-safety.
- Внешние ориентиры: Go runtime (stackful, copy-grow, precise GC),
  corosensei / Boost.Context (stackful + TIB-fixup — уровень minicoro),
  tokio / JS (stackless — этой задачи не имеют).
