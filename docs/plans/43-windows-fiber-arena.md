// SPDX-License-Identifier: MIT OR Apache-2.0
# План 43: Windows fiber stack arena с SEH lazy commit

> **Статус:** **2 неудачные попытки 2026-05-13. План открытый.**
> **Цель:** догнать Linux/macOS arena на Windows — lazy commit, growable
> stacks, single GC root.
>
> **Prerequisite:** Plan 41 закрыт (Linux/macOS arena landed).
> Plan 43 — symmetric Windows реализация, унифицирует platform pathways.
>
> **Lessons learned (2026-05-13):** см. секцию «Неудачные попытки» в конце.

---

## Контекст

Plan 41 (D97) ввёл per-thread fiber arena на Linux/macOS через
`mmap(MAP_NORESERVE)` + lazy commit + 16 KB guard pages + single
Boehm GC root. Windows остался на дефолтном minicoro calloc:

- `calloc(57344)` — fixed 56 KB, **commits всё upfront** (Windows heap
  без lazy semantics).
- НЕТ guard page → stack overflow = generic SEGV без указания на slot.
- НЕТ роста — hard ceiling 56 KB на fiber.
- НЕТ single GC root — calloc'нутые stacks разбросаны, conservative scan
  пропускает указатели на heap из suspended fiber stacks.

Последнее блокирует:
- **Plan 40 R8-4** stack-allocated BaseWaiter на Windows (fallback на
  nova_alloc; Linux/macOS получают zero-alloc park).
- **Plan 40 R8-1** Time.after pin list работал через workaround
  (NovaAfterState через malloc вместо nova_alloc) — потому что
  GC через fiber stacks недостоверен.
- Возможные UAF при scope-level cancel: `_nova_active_scope` на fiber
  stack → если суспенднутый fiber не виден GC, его scope может быть
  collected. Бутстрап single-thread cooperative + GC disabled между
  yield/resume — единственный safeguard.

---

## Решение

**Windows arena с VirtualAlloc(MEM_RESERVE) + on-demand commit через VEH.**

### Архитектурный обзор

Symmetric с Linux design:

```
Per-thread (TLS):

    arena: 8 GB virtual reserved через VirtualAlloc(MEM_RESERVE, PAGE_NOACCESS)
        ├─ slot 0: 2 MB                     ┐
        │   ├─ guard (16 KB) PAGE_NOACCESS  │
        │   └─ usable (2 MB - 16 KB)        │ Lazy commit
        │       ├─ page 0 — committed       │ on first
        │       ├─ page 1 — committed       │ access via
        │       └─ page 511 — uncommitted   │ VEH handler
        ├─ slot 1: 2 MB                     │
        ├─ ...                              │
        └─ slot 4095: 2 MB                  ┘

    bitmap[64 uint64_t]
    one GC_add_roots(arena, arena + 8 GB)
```

### VEH (Vectored Exception Handler) механика

`VirtualAlloc(addr, size, MEM_RESERVE, PAGE_NOACCESS)` резервирует адресное
пространство, **БЕЗ** physical commit, **БЕЗ** page table entries. Доступ
→ `STATUS_ACCESS_VIOLATION (0xC0000005)`.

`AddVectoredExceptionHandler` устанавливает callback, который вызывается
ПЕРЕД standard SEH unwind. Handler решает:
- Если fault address в arena range AND в usable region (не guard) →
  `VirtualAlloc(page_aligned_addr, 4096, MEM_COMMIT, PAGE_READWRITE)` →
  `EXCEPTION_CONTINUE_EXECUTION`.
- Если fault в guard region → **real stack overflow** → pretty diagnostic
  → `EXCEPTION_CONTINUE_SEARCH` (standard SEH крашит процесс).
- Если fault outside arena → `EXCEPTION_CONTINUE_SEARCH` (не наш bug).

Это **identical** Linux page-fault handling (с `MAP_NORESERVE` kernel сам
commits page on first touch). Just user-space vs kernel-space.

### Wire-up

`fiber_arena.h` обобщаем — `NOVA_FIBER_ARENA_ENABLED = 1` теперь на ВСЕХ
платформах. `fiber_arena_win.c` — новый файл с Windows-специфичной
имплементацией; `fiber_arena.c` остаётся Linux/macOS.

`fibers.h`: `_NOVA_MCO_DESC_INIT` теперь активирует arena на Windows
(убираем условный fallback на default calloc).

`channels.h`: stack-allocated BaseWaiter активен на Windows (убираем
`#if defined(__linux__) || defined(__APPLE__)`).

`Time.after`: NovaAfterState можно вернуть на nova_alloc (но не обязательно
— malloc/free pattern тоже OK, упрощает; оставляем как есть).

---

## API parity

Linux/macOS (`fiber_arena.c`):
```c
void  nova_fiber_arena_init(void);
void* nova_fiber_alloc(size_t size, void* allocator_data);
void  nova_fiber_dealloc(void* ptr, size_t size, void* allocator_data);
bool  nova_fiber_arena_contains(const void* ptr);
NovaFiberArenaStats nova_fiber_arena_stats(void);
```

Windows (`fiber_arena_win.c`): **identical signatures**. Только impl
swap. `fiber_arena.h` уже декларирует cross-platform — добавляем
WIN-branch:

```c
#if defined(__linux__) || defined(__APPLE__) || defined(_WIN32)
  #define NOVA_FIBER_ARENA_ENABLED 1
#else
  #define NOVA_FIBER_ARENA_ENABLED 0
#endif
```

---

## Этапы реализации

### Этап 1 — Windows arena core (~150 LOC)

`compiler-codegen/nova_rt/fiber_arena_win.c`:

- `VirtualAlloc(NULL, 8 GB, MEM_RESERVE, PAGE_NOACCESS)` — reserve.
- TLS arena state (`__declspec(thread)` или `_Thread_local`).
- `VirtualAlloc(slot_base, GUARD_SIZE, MEM_COMMIT, PAGE_NOACCESS)` —
  guard page (committed for protection, но access fault'ит).
- bitmap reuse (mirror Linux).
- `_arena_register_active_range` через `GC_add_roots`.

### Этап 2 — VEH handler (~50 LOC)

`fiber_arena_win.c::_nova_arena_veh_handler`:

```c
LONG NTAPI _nova_arena_veh_handler(EXCEPTION_POINTERS* info) {
    if (info->ExceptionRecord->ExceptionCode != STATUS_ACCESS_VIOLATION)
        return EXCEPTION_CONTINUE_SEARCH;

    void* fault_addr = (void*)info->ExceptionRecord->ExceptionInformation[1];

    if (!nova_fiber_arena_contains(fault_addr))
        return EXCEPTION_CONTINUE_SEARCH;

    // Какой slot? guard или usable?
    size_t offset = (char*)fault_addr - _t_arena.base;
    size_t slot = offset / NOVA_FIBER_STACK_SIZE;
    size_t slot_offset = offset % NOVA_FIBER_STACK_SIZE;

    if (slot_offset < NOVA_FIBER_GUARD_SIZE) {
        // Guard hit — real overflow. Pretty error.
        fprintf(stderr, "nova: fiber stack overflow in slot %zu (fault @ %p)\n",
                slot, fault_addr);
        return EXCEPTION_CONTINUE_SEARCH;  // process dies
    }

    // Commit одну page.
    char* page = (char*)((uintptr_t)fault_addr & ~(uintptr_t)4095);
    if (VirtualAlloc(page, 4096, MEM_COMMIT, PAGE_READWRITE) != page) {
        // Commit failed — usually OOM. Let SEH crash.
        return EXCEPTION_CONTINUE_SEARCH;
    }
    return EXCEPTION_CONTINUE_EXECUTION;
}
```

`AddVectoredExceptionHandler(1, _nova_arena_veh_handler)` в
`nova_fiber_arena_init` (first call per thread, idempotent).

### Этап 3 — Wire-up cleanup (~30 LOC)

`fiber_arena.h`: `NOVA_FIBER_ARENA_ENABLED = 1` на Windows.
`fibers.h::_NOVA_MCO_DESC_INIT` теперь uses arena unconditionally
on all enabled platforms (убираем Windows fallback).
`channels.h::nova_chan_reader_recv`/`send` — убираем `#if` для
stack-allocated BaseWaiter.

### Этап 4 — Test runner update (~10 LOC)

`compiler-codegen/src/test_runner.rs`: `fiber_arena_win.c` подключается
для Windows toolchain'ов. На Linux подключается `fiber_arena.c`. Условный
linkage через `#[cfg(...)]`.

### Этап 5 — Регрешн

- Windows: `nova test` — ожидаем 263 PASS (либо +1 от unblocked теста).
- Linux Docker: regression — ожидаем no-op.
- Test: создать `fiber_arena_stats` smoke test на Windows
  (sаme как Linux) — `slot_count > 0`, `virtual_reserved > 0`, slot
  reuse работает.

### Этап 6 — Docs

- Update D97 spec — Windows path теперь не fallback.
- Remove "Windows still calloc" disclosure из simplifications.md.
- project-creation.txt session log.

---

## Acceptance

- Windows: `slot_count = 4096`, `virtual_reserved = 8 GB`, lazy commit
  работает (RSS not 8 GB).
- Stack overflow → pretty message "fiber stack overflow in slot N".
- 263 PASS / 0 FAIL Windows + Linux Docker.
- Channels stack-allocated BaseWaiter активен на Windows.
- D97 spec: убрать hybrid OS strategy disclosure, заменить на unified.

---

## Риски

**R1: VEH handler latency.** Page fault → VEH dispatch → VirtualAlloc
commit → resume. Ожидаемая cost ~1-5 µs per first-touch. Windows kernel
SEH dispatch не такой быстрый как Linux page-fault. **Mitigation:**
prefetch — commit первый page при allocation (warm start). Если будет
проблема — пред-commit пары первых pages.

**R2: VEH installed globally** — affects every thread в процессе. Чужой
код в процессе (например libuv) может полагаться на standard SEH
behavior. Наш handler возвращает `EXCEPTION_CONTINUE_SEARCH` если fault
outside arena — невмешательство. **Должно быть безопасно**, но требует
verification.

**R3: VirtualAlloc на reserve 8 GB.** Address space на 64-bit Windows
= 128 TB user space, 8 GB × N threads — тривиально. Только под 32-bit
Windows ограничение (но 32-bit downscale из Plan 41 R8 уже handle'ит).

**R4: MAP_NORESERVE on Linux зарегистрирован kernel'ем как accountable
для overcommit policies; VirtualAlloc на Windows аналогично — but
8 GB reserve добавляется к total VirtualSize per-thread**, который видим
через TaskMgr. Может выглядеть пугающе но не consume RAM. Document
это в D97.

**R5: VEH handler — нельзя долго блокироваться** (deadlocks с MM
locks). VirtualAlloc commit short. ОК.

---

## Что отвергнуто

- **`MCO_USE_VMEM_ALLOCATOR`** (minicoro built-in Windows) —
  `VirtualAlloc(MEM_RESERVE | MEM_COMMIT)` commits всё upfront. Не lazy.
  Plan 41 это уже рассматривал.
- **AddressFilterFunction / SetUnhandledExceptionFilter** — позже в
  цепочке SEH dispatch, конфликт с компилятор-генерированными `__try`/
  `__except` блоками.
- **Stack copying (Go-style)** — требует stackmap для precise GC root,
  у нас Boehm conservative. Также fundamentally меняет calling convention.

---

## Зависимости

- **Plan 41** ✅ закрыт — Linux/macOS arena landed, infrastructure ready.
- **Plan 40 R8-4** ✅ закрыт — stack-allocated BaseWaiter ready (unblocked
  on Windows после Plan 43).

## Открывает

- **Plan 23 M:N runtime** — теперь Windows symmetric с Linux для arena.
- **P41R-6 SIGSEGV pretty handler** — Windows эквивалент через VEH
  включён в Plan 43; Linux SIGSEGV — отдельная задача (но pattern тот же).

---

## Неудачные попытки (2026-05-13)

### Попытка 1: VEH page-level lazy commit

**Дизайн:** `VirtualAlloc(MEM_RESERVE)` + `AddVectoredExceptionHandler`
на STATUS_ACCESS_VIOLATION → on-demand `VirtualAlloc(MEM_COMMIT)` для
одной page (4 KB) → `EXCEPTION_CONTINUE_EXECUTION`.

**Результат:** регрессия **228 PASS / 35 FAIL** (multiple TIMEOUT'ы).

**Root cause:** **VEH handler конфликтует с Boehm conservative scan**.
Boehm scan'ит весь arena range (`high_water * slot_size`) byte-by-byte
для conservative root walk. На uncommitted pages scan touches → VEH
handler commits → каждая scanned page становится **resident**. На
single GC cycle вся arena становится committed → RSS thrash + cost.

На Linux эквивалент работает потому что `MAP_NORESERVE` page-fault
**kernel-handled**: page returns 0s (anonymous), conservative scan
читает 0, **MADV_DONTNEED после dealloc'a возвращает page**. На Windows
наш user-space VEH **постоянно** возвращает PAGE_READWRITE pages,
которые не decommit'ятся без явного `VirtualFree(MEM_DECOMMIT)`.

### Попытка 2: Slot-level lazy commit

**Дизайн:** `VirtualAlloc(MEM_RESERVE)` 8 GB + per-slot `MEM_COMMIT`
на alloc / `MEM_DECOMMIT` на dealloc. Без VEH handler. Guard page
committed как `PAGE_NOACCESS`.

**Результат:** регрессия **228 PASS / 35 FAIL** (TIMEOUT'ы + clang
OOM на CC одного теста).

**Root cause (предположительный):** **2 MB commit per active fiber**.
В test suite каждый supervised + spawn создаёт fiber → 2 MB commit
charge. 100 одновременных fiber'ов = 200 MB commit. На многих
test_runner subprocesses одновременно (default `--jobs num_cpus`)
суммарная commit charge может превысить Windows limit, что приводит к:
- `VirtualAlloc(MEM_COMMIT)` failures → fiber alloc fail → run hangs.
- `clang.exe` OOM на одном тесте (compiler сам не выделяет память).

Также возможно: Boehm scan на **arena range** (`high_water * 2 MB`),
включая committed slots, на каждый GC cycle. Cost для conservative
scan 2 MB stack — read 2 MB / 64 B (cache line) = 32k loads × ~10 ns =
~320 µs per fiber per GC cycle. Под frequent GC = visible perf hit.

### Cleanup (2026-05-13)

Все Plan 43 changes откачены — `nova test` → **263 PASS / 0 FAIL**.
fiber_arena_win.c удалён из FS. Windows остаётся на calloc + Plan 40
R8-1 NovaAfterState malloc/free workaround. Plan 41 D97 disclosure
(Windows на calloc path) пока остаётся в силе.

### Что нужно для успешной реализации

1. **Decoupling Boehm scan range от physical commit.** Один из:
   - Scan только known-used pages, не full arena range. Требует
     custom GC mark phase — incompatible с Boehm conservative scan.
   - Arena namespace вне Boehm root set, fiber stacks scan'ятся
     явно через registered callback. Невозможно без Boehm patch
     (прецедент «не патчить сторонние библиотеки» нарушать нельзя).

2. **Stack-specific Windows API.** Использовать `CreateFiber` Windows
   API вместо minicoro. CreateFiber выделяет proper Windows stack
   (TIB-tracked) с growable guard pages — kernel handles overflow.
   Замена minicoro полностью — крупный рефакторинг, не Plan 43 scope.

3. **Per-fiber commit charge accounting.** Если RAM/pagefile ограничен,
   2 MB × N fibers упирается в commit limit раньше чем в Linux MAP_NORESERVE
   (которое использует ровно столько pages сколько touched). Это
   fundamental Windows VM model limitation.

4. **Re-think arena scope.** Может стоит **меньшие slots** на Windows
   (256 KB вместо 2 MB) — committed pages только когда нужно, growable
   через chained slots если fiber превысит. Но это требует чейнинг
   логики которой нет в minicoro.

### Открытое решение

**Plan 43 остаётся открытым до большей дизайн-работы.** Bootstrap
running OK на Windows через calloc-путь — приоритет ниже Plan 23
M:N. Когда возьмёмся: вариант 2 (CreateFiber) — самый realistic
production path для Windows.

### Файлы которые НЕ применены (cleanup'нуты)

- ~~`compiler-codegen/nova_rt/fiber_arena_win.c`~~ (удалён).
- `fiber_arena.h` — `NOVA_FIBER_ARENA_ENABLED` снова Linux/macOS only.
- `fibers.h` — `_NOVA_MCO_DESC_INIT` снова Linux/macOS-conditional.
- `channels.h` — stack BaseWaiter снова `#if defined(__linux__) || defined(__APPLE__)`.
- `test_runner.rs` — `fiber_arena_win.c` НЕ подключён.

---

## Попытка 3 (2026-05-13): slot 256 KB + idle-only decommit

**Дизайн:**
- slot_size = 256 KB вместо 2 MB (8x compact, ещё 4x больше default minicoro 56 KB).
- VirtualAlloc(MEM_RESERVE) 1 GB per thread (4096 × 256 KB).
- На alloc: per-slot MEM_COMMIT 240 KB usable, guard region остаётся
  reserved (PAGE_NOACCESS default).
- На dealloc: НЕ decommit в hot path. Idle-only flush (mirror Linux
  P41R-3 fix) — при `slots_active == 0` → batch VirtualFree(MEM_DECOMMIT)
  whole high_water range.
- Boehm GC root через `_arena_register_active_range`.
- НЕТ VEH handler — guard hits → standard SEH crash.

**Результат:** **fail.** Все concurrency тесты TIMEOUT (60+ sec) +
`fiber_arena exhausted (4096 slots used)` на одном тесте.

**Hypothesis root cause:** **arena slots не reuse'ются**. dealloc_cb
вероятно вызывается с ptr который не совпадает с slot boundary (minicoro
internal arithmetic), наш `_arena_mark_slot_free` либо не decrement'ит
`slots_active` (если `if (slot >= slot_count) return` falls through),
либо decrement'ит wrong slot. Slots leak → exhaustion. Все concurrency
тесты которые делают много spawn-cycles → exhaustion → fiber alloc fail
or unsafe state → hang.

На Linux/macOS этот же код работает (Plan 41 validated). Разница —
**minicoro context switch backend**. На POSIX = `MCO_USE_UCONTEXT` или
`MCO_USE_ASM`. На Windows может быть `MCO_USE_FIBERS` (Windows Fiber
API через CreateFiber) — что **может internally allocate** независимо
от нашего alloc_cb.

**Cleanup:** revert. fiber_arena_win.c удалён. 263 PASS / 0 FAIL восстановлено.

---

## Что нужно для попытки 4

1. **Проверить minicoro Windows backend.** Если `MCO_USE_FIBERS` (CreateFiber)
   — наш alloc_cb игнорируется, и весь подход не работает. Нужен либо
   `MCO_USE_ASM` (asm context switch — minicoro built-in), либо custom
   Windows fiber wrapper.

2. **Verify minicoro alloc_cb / dealloc_cb pointer contract.** Что
   именно minicoro передаёт в dealloc_cb на Windows? Если ptr ≠ ptr
   from alloc_cb — наш bitmap free логика broken.

3. **Альтернатива — CreateFiber API directly** (не через minicoro).
   Это major rewrite, не Plan 43 scope.

4. **Альтернатива — slot reuse без `slots_active == 0` idle decommit**.
   Может decommit + recommit confuses Windows VM manager. Сначала
   попробовать БЕЗ decommit (просто bitmap free + reuse).

### Open status

Plan 43 — **3 неудачные попытки**. Bootstrap running OK на Windows
calloc + Plan 40 R8-1 workaround. **Приоритет ниже Plan 23** —
вернёмся когда Windows production станет realistic, либо когда есть
time для minicoro backend investigation.
