// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 82: Windows fiber stack arena — re-diagnosis + production-реализация

> **Создан:** 2026-05-21. **Переписан с чистого листа:** 2026-05-21
> (перепроверка опровергла документированный root cause Plan 44.3).
> **Статус:** 📋 proposed, не начат. Ф.0 = re-diagnosis (decision point).
> **Приоритет:** P2 — Windows работает (calloc-fallback, корректно), но
> без lazy-commit и без stack-overflow detection; это долг паритета с
> Linux/macOS (Plan 44.2) и с Go/Rust, не блокер релиза.
> **Родитель:** [Plan 44](44-mn-runtime-roadmap.md) (M:N runtime).
> **Заменяет наивный подход:** [Plan 44.3](44.3-fiber-arena-windows.md)
> — 4 неудачные попытки 2026-05-13/14; его документированный root cause
> опровергнут (см. ниже). 44.3 остаётся историческим документом.

---

## 1. Что показала перепроверка с чистого листа

Plan 44.3 утверждал: «minicoro `MCO_USE_ASM` переключает только RSP, не
обновляет `NT_TIB.StackBase/StackLimit` → SEH/overflow detection на
`VirtualAlloc`-стеке → hang». **Это неверно для текущего vendored
minicoro.**

`compiler-codegen/nova_rt/minicoro.h:665-768` — Windows x64 asm-backend:
- `_mco_ctxbuf` (665-672) содержит поля `stack_base`, `stack_limit`,
  `dealloc_stack`, `fiber_storage` — **помимо** регистров.
- `_mco_switch_code` (688-750): asm читает `%gs:0x30` (self-указатель
  TEB), **сохраняет** из TEB в `from`-контекст и **восстанавливает** в
  TEB из `to`-контекста поля по смещениям `0x08` (`NT_TIB.StackBase`),
  `0x10` (`StackLimit`), `0x1478` (`DeallocationStack`), `0x20`
  (ArbitraryUserPointer) — **на каждом context switch**.
- `_mco_makectx` (755-768) инициализирует `stack_base`/`stack_limit`/
  `dealloc_stack` на границы корутинного стека.

То есть текущий minicoro **уже делает ровно то, что делают corosensei
(Rust) и Boost.Context (C++)** — синхронизирует TIB при переключении.
На x64 этого достаточно для table-based SEH (`RtlLookupFunctionEntry`/
`RtlVirtualUnwind` останавливают раскрутку по корректным
`StackBase/StackLimit`; x64 не использует TEB exception-list chain —
это x86-32).

**Вывод:** документированный root cause Plan 44.3 противоречит коду.
Либо 44.3 (2026-05-13) использовал старый minicoro без Windows-asm-
backend'а (ранние версии minicoro для Windows имели только
`MCO_USE_FIBERS`), либо диагноз «TIB» был ошибочным, а 4 попытки
провалились по другой причине. **Настоящая причина неизвестна** —
поэтому Plan 82 начинается с re-diagnosis (Ф.0), а не с реализации.

**Кандидаты на настоящий блокер** (гипотезы для Ф.0):
- **Boehm conservative scan vs Windows commit-charge.** Linux-arena
  (44.2) регистрирует как GC-root только активный диапазон
  `[base, high_water]` — иначе Boehm-scan читает каждую страницу и
  форсирует commit. На Windows чтение reserved-но-не-committed страницы
  = access violation; scan всего arena-диапазона → фолт. Это
  правдоподобный реальный блокер.
- **Guard page / VEH.** На Windows нет аналога Linux-овского
  `mprotect(PROT_NONE)` + SIGSEGV-handler — нужен `PAGE_NOACCESS` +
  Vectored Exception Handler.
- **Что-то в самих 4 попытках** — секция «Неудачные попытки» в 44.3.

---

## 2. Сравнение с Go / Rust / TS — где Nova стоит

| Аспект | Go (goroutine) | Rust — tokio | Rust — corosensei / Boost.Context | TS / JS | **Nova сейчас** | **Nova — цель Plan 82** |
|---|---|---|---|---|---|---|
| Модель корутины | stackful | **stackless** (async state machine) | stackful | **stackless** (event loop) | stackful (minicoro) | stackful |
| Отдельный стек на корутину | да | нет (на стеке executor'а) | да | нет | да | да |
| Windows TIB на switch | runtime сам управляет; OS-вызовы на g0 | н/п (нет своих стеков) | asm свопает `StackBase/StackLimit/DeallocationStack` | н/п | **minicoro asm свопает ✅** | то же ✅ (уже есть) |
| Lazy commit памяти стека | да (стек растёт по факту) | н/п | реализация-зависимо | н/п | **нет** (Windows: 56 KB upfront) | **да** (`VirtualAlloc` reserve + lazy commit) |
| Stack overflow detection | morestack-проверка в прологе | guard page → abort | guard page → паника/abort | `RangeError` (глубина JS) | **нет на Windows** (тихая порча heap) | guard page + VEH → понятная ошибка |
| Growable стек (copy) | да (precise GC, pointer fixup) | н/п | нет | н/п | нет | **нет — несовместимо с conservative GC** (см. §3) |
| Cross-thread migration (work-stealing) | да (G мигрирует между P) | да (task между worker'ами) | да (если `Send`) | нет (один поток) | да на Linux/macOS; Windows — fallback | **да** |
| Эффективный потолок стека | ~1 GB (растёт) | н/п | fixed reserve | ~глубина движка | 56 KB (мало!) | large-reserve (напр. 8 MB, как Linux 44.2) |

**Честный разбор:**

1. **Rust-mainstream (tokio) и TS/JS вообще не имеют этой проблемы** —
   они **stackless**: корутина — это сгенерированный конечный автомат,
   исполняется на обычном стеке потока. Нет отдельных стеков → нет
   TIB/guard/commit-вопросов. Nova сознательно выбрала **stackful**
   (minicoro): алгебраические эффект-хендлеры и произвольно-глубокие
   вызовы без function-coloring (нет `async`-окраски). Это
   зафиксированный трейд — Plan 82 не пересматривает его, но честно
   фиксирует: stackful = эта работа существует, stackless бы её не
   имел.
2. **Прямой ориентир для stackful-на-Windows — это Go-runtime и
   corosensei/Boost.Context**, не tokio. По синхронизации TIB Nova
   (minicoro asm) **уже на их уровне** — это не отставание.
3. **Где Nova реально позади Go/corosensei сегодня:** Windows не имеет
   (а) lazy-commit (56 KB упфронт × N fibers — и потолок 56 KB опасно
   мал), (б) stack-overflow detection (тихая порча heap). Plan 82
   закрывает ровно этот разрыв → паритет.
4. **Growable стеки** (Go копирует растущий стек) Nova **не может** —
   см. §3. Не отставание-которое-чинится, а архитектурное следствие
   conservative GC; компенсируется large-reserve + lazy-commit (тот же
   объём «эффективно доступного» стека, что у Go, просто без
   copy-shrink).

---

## 3. Коррекция цели 44.3: «growable stacks» несовместимы с Nova GC

Plan 44.3 формулировал цель как «lazy commit, **growable stacks**,
single GC root». Пункт «growable» — **недостижим в принципе** и его
надо убрать из цели:

- Go копирует растущий стек (`runtime.copystack`) потому что у Go
  **precise** stack maps — компилятор знает точное смещение каждого
  указателя в каждой точке и переписывает их (pointer fixup) после
  копирования.
- Nova использует **Boehm conservative GC** (`GC_malloc`,
  `GC_set_all_interior_pointers(1)` — `alloc_boehm.c`). Conservative
  сборщик **не знает**, какое слово на стеке — указатель, а какое —
  целое. → не может найти и переписать указатели при копировании стека
  → после copy все internal-указатели (на сам стек, на locals) битые.
- **Вывод:** Go-style growable-by-copy фундаментально несовместимо с
  conservative GC. Единственная совместимая стратегия «большого
  стека» — **fixed virtual reserve + lazy physical commit**:
  виртуальный адрес стека неизменен (указатели валидны), физический
  backing растёт по page-fault. Именно это делает Linux-arena 44.2
  (8 MB reserve, lazy 4 KB commit). Plan 82 приносит это на Windows.

Цель Plan 82 (исправленная): **lazy-commit large-reserve fiber-стеки на
Windows + guard-page overflow detection + GC-scan без commit-charge
взрыва** — паритет с Linux-arena 44.2. НЕ growable-by-copy.

---

## 4. Реальная постановка задачи — три подпроблемы

TIB — **не** подпроблема (minicoro asm уже решает, §1). Реальная
работа:

- **П1. Arena-аллокация на Windows.** `VirtualAlloc` `MEM_RESERVE`
  большого диапазона + lazy `MEM_COMMIT` (или reserve+commit-on-fault
  через `PAGE_GUARD`-growable region) + `PAGE_NOACCESS` guard-страница
  + Vectored Exception Handler, ловящий `EXCEPTION_ACCESS_VIOLATION` /
  `STATUS_STACK_OVERFLOW` в guard-зоне → понятная диагностика «fiber
  stack overflow in slot N» (паритет с Linux `_arena_sigsegv_handler`).
- **П2. Boehm GC vs Windows commit-charge.** Перенести стратегию
  «active-range root» из 44.2 (`fiber_arena.c` `_arena_register_active_
  range` — регистрировать как root только `[base, high_water]`, не
  весь reserve). На Windows дополнительно: scan не должен касаться
  reserved-но-не-committed страниц (= access violation). Высоковероятно
  это и есть настоящий блокер 44.3.
- **П3. Cross-thread migration.** Plan 44.5 work-stealing **реально
  мигрирует** fiber'ы между worker-потоками (`runtime.c` —
  `nova_deque_steal` → `mco_resume` на другом потоке). Любой Windows-
  дизайн обязан поддержать resume корутины с потока, отличного от
  предыдущего. minicoro asm-backend свопает TIB **текущего** потока →
  resume на потоке B настраивает TIB потока B на стек fiber'а —
  работает. Но это **обязательный пункт тест-матрицы**, не допущение.

---

## 5. Фазы

### Ф.0 — Re-diagnosis (decision point, ~2-3 дня)

**Главное отличие от 44.3:** 4 провалившиеся попытки шли сразу в
интеграцию в runtime Nova. Plan 82 начинается с **изолированного
standalone C-харнесса** (вне runtime Nova).

1. Прочитать секцию «Неудачные попытки» в 44.3; git-археология —
   обновлялся ли `minicoro.h` после 2026-05-13 (получил ли Windows-asm-
   backend позже попыток).
2. Минимальный C-харнесс: текущий vendored minicoro `MCO_USE_ASM` +
   корутина на стеке из `VirtualAlloc`. Намеренно спровоцировать:
   (а) SEH/`__try` unwind через границу fiber-стека; (б) stack overflow
   в guard-страницу; (в) Boehm-style conservative scan диапазона.
3. Установить: **воспроизводится ли провал 44.3 на текущем коде?**
   - **Если НЕТ** (всё работает) → 44.3 был мисдиагноз / старый
     minicoro; перейти к Ф.1 (реализация arena — TIB уже решён).
   - **Если ДА** → зафиксировать **настоящую** причину (по гипотезам
     §1: commit-charge / guard / иное) с минимальным репро.
4. Решить toolchain-матрицу спайка: **clang-cl И MSVC** (SEH-семантика,
   `/GS`, `/EHsc` различаются; Nova поддерживает оба).

**Выход Ф.0:** документ «настоящий блокер 44.3» + go/no-go на Ф.1.

### Ф.1 — Windows arena allocation (~3-4 дня)

Порт `fiber_arena.c`/`.h` на Windows (сейчас `NOVA_FIBER_ARENA_ENABLED`
= 1 только для `__linux__`/`__APPLE__`, `fiber_arena.h:50-54`):
- `VirtualAlloc` `MEM_RESERVE` arena-диапазона; lazy `MEM_COMMIT`
  по мере роста (page-fault-driven через `PAGE_GUARD` growable region
  или явный commit в VEH).
- `PAGE_NOACCESS` guard-страница внизу каждого slot (паритет 16 KB
  guard из 44.2).
- VEH (`AddVectoredExceptionHandler`), распознающий fault в guard-зоне
  → сообщение «nova: fiber stack overflow in slot N», аналог
  `_arena_sigsegv_handler`.
- Размер: large-reserve (ориентир — 8 MB slot, как Linux 44.2; commit
  малый стартовый). Решить slot_count под Windows commit-charge limit.
- minicoro `_NOVA_MCO_DESC_INIT` — Windows-ветка с arena-callbacks
  (сейчас `fibers.h:91-109` — только Linux/macOS).

### Ф.2 — Boehm GC integration (~2 дня)

- Перенести «active-range root» (`[base, high_water]`, 1 entry/worker,
  `__thread` high-water tracker) — `fiber_arena.c:239-256`.
- Гарантировать: Boehm-scan **не читает** reserved-uncommitted
  страницы Windows (иначе AV). Scan-диапазон ⊆ committed.
- GC-stress: 10k fibers, no leak, no false-retention, commit-charge
  ограничен (вторичный блокер 44.3, попытки 1-2).

### Ф.3 — Cross-thread migration correctness (~2 дня)

- Тест: fiber, начавший на worker A, украден work-stealing'ом и
  резюмлён на worker B — TIB потока B корректен, SEH работает, guard-
  page срабатывает.
- minicoro `mco_current_co` (TLS) consistency при миграции.
- Home-scope pinning (`runtime.c:330-345`) не ломается.

### Ф.4 — Production test matrix: SEH / overflow / toolchain (~3 дня)

Полная тест-матрица (см. §6). Без упрощений — обе toolchain (clang-cl +
MSVC), оба сценария (cooperative + work-stealing M:N).

### Ф.5 — Включить M:N на Windows + parity-бенчмарк (~2 дня)

- Сейчас M:N тесты на Windows платформо-агностичны (honest-sentinel
  `0` через `fibers.slot_count() == 0`). Сделать так, чтобы Windows
  реально исполнял многопоточный M:N путь.
- **Написать отсутствующий бенчмарк context-switch** (`bench/micro/` —
  его нет): `mco_resume`/`mco_yield` switch-cost. Сравнить с
  ориентирами: Boost.Context ~10-20 ns asm-своп; Go goroutine switch
  ~десятки ns. Цель — не хуже текущего Linux-asm-пути.

### Ф.6 — spec + закрытие 44.3 (~0.5 дня)

- D97 (fiber arena spec) — Windows-стратегия, коррекция «growable» →
  «lazy-commit large-reserve».
- 44.3 → шапка «superseded by Plan 82» (если Ф.1-Ф.5 успешны).
- `docs/simplifications.md`, `project-creation.txt`, README.

---

## 6. Production-grade тест-матрица (Ф.4, без упрощений)

| Сценарий | Что проверяется |
|---|---|
| SEH `__try/__except` внутри fiber | unwind корректен, не уходит за границы fiber-стека |
| SEH unwind ЧЕРЕЗ границу fiber → caller | `RtlVirtualUnwind` останавливается по `StackBase/StackLimit` |
| Stack overflow (глубокая рекурсия) | упор в guard-page → понятная ошибка, НЕ тихая порча heap |
| `/GS` stack cookie | сборка с `/GS` (MSVC) — нет ложных `__report_gsfailure` |
| Deep call (≈ потолок large-reserve) | lazy commit растёт корректно до потолка |
| Cross-thread migration (work-stealing) | fiber A→steal→B: TIB, SEH, guard на потоке B |
| GC stress: 10k fibers | no leak, no false-retention, commit-charge ограничен |
| Long soak: 10⁶ spawn/yield | без деградации, без роста commit |
| clang-cl И MSVC | вся матрица на обеих toolchain |
| cooperative + M:N work-stealing | оба режима планировщика |
| Debugger sanity | стек fiber'а читаем в отладчике (как минимум — не падает) |

Негативные тесты: overflow-detection даёт **детерминированную**
диагностику (не UB), cross-thread двойной resume отвергается
планировщиком.

---

## 7. Acceptance (измеримый, в рамке паритета)

- Windows исполняет M:N fiber-workload (включая work-stealing
  migration) без hang; вся тест-матрица §6 зелёная на clang-cl + MSVC.
- Stack overflow → **детерминированная** ошибка «fiber stack overflow»
  (паритет с Linux 44.2 и с Rust guard-page-abort; **лучше** текущего
  Windows — тихая порча heap).
- Lazy commit измеримо работает: N fibers потребляют commit-charge
  пропорционально реально использованной глубине, не `N × reserve`.
- Эффективный потолок стека — large-reserve (ориентир 8 MB, паритет с
  Linux 44.2; ≫ текущих 56 KB).
- Context-switch не медленнее текущего Linux-asm-пути (бенчмарк Ф.5).
- 0 регрессий в полном прогоне (Windows + Linux).

---

## 8. Честная рамка риска

- **Наиболее вероятный исход (по §1):** TIB-плюмбинг уже есть в
  vendored minicoro; настоящая работа — arena-аллокация + GC-commit-
  charge + guard/VEH. Это **решаемо** — паритет с уже-работающей
  Linux-arena 44.2. 44.3 «fundamentally blocked» переоценено: либо
  старый minicoro, либо мисдиагноз.
- **Остаточный риск:** если Ф.0-спайк выявит **новый** настоящий
  фундаментальный блокер (напр. неустранимое поведение Windows commit-
  charge под conservative scan) — честный исход: задокументировать его
  точно, оставить calloc-fallback, понизить scope до «M:N на Windows
  на fixed-стеках» (без lazy-commit arena). Plan 82 это **допускает** —
  Ф.0 front-load'ит изолированный спайк, чтобы провал был дешёвым и
  диагностичным (в отличие от 4 интеграционных провалов 44.3).
- **Отличие от 44.3:** 44.3 шёл сразу в интеграцию 4 раза. Plan 82 —
  re-diagnosis → standalone-спайк → только потом интеграция.

---

## 9. Связь

- [Plan 44.3](44.3-fiber-arena-windows.md) — 4 попытки 2026-05-13/14;
  его документированный root cause опровергнут §1. Историческая запись.
- [Plan 44.2](44.2-fiber-arena-posix.md) ✅ — Linux/macOS arena;
  reference-семантика (mmap+`MAP_NORESERVE` lazy commit, 16 KB guard,
  active-range GC root). Windows-arena = её порт.
- [Plan 44.5](44.5-work-stealing-scheduler.md) ✅ — work-stealing;
  мигрирует fiber'ы между worker'ами → требование П3.
- [Plan 44](44-mn-runtime-roadmap.md) — M:N umbrella.
- Внешние ориентиры: Go runtime (stackful, own scheduler), corosensei /
  Boost.Context (stackful + TIB fixup — уровень, на котором minicoro
  asm уже находится), tokio / JS (stackless — не имеют этой задачи).
