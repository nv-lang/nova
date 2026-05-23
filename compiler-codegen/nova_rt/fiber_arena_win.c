// SPDX-License-Identifier: MIT OR Apache-2.0
/* Plan 82 Ф.1-Ф.3 — Windows fiber stack arena (lazy-commit, M:N-safe).
 *
 * Симметрична POSIX-реализации fiber_arena.c (Plan 44.2), но на Windows-
 * примитивах VirtualAlloc/VirtualFree. Заменяет провальный Plan 44.3.
 *
 * Дизайн (верифицирован Ф.0 re-diagnosis — 82-artifacts/f0-rediagnosis.md;
 * Ф.1+Ф.2 — 82-artifacts/f1-report.md):
 *
 *  - **Путь A — OS-native grow.** После TIB-свопа minicoro-asm'а ядро
 *    Windows растит коро-стек штатно через PAGE_GUARD-фолт. VEH для
 *    happy-path не нужен — только диагностика overflow.
 *  - **Lazy commit.** Арена — VirtualAlloc(MEM_RESERVE) на поток;
 *    физический commit только под header + начальное окно у вершины.
 *  - **Раскладка слота** (§5.1): [hard guard 16K reserved][minicoro
 *    header commit][reserved/grown][initial window commit + PAGE_GUARD].
 *  - **Патч ctx.stack_limit** (Ф.0 test a) — fibers.c::nova_fiber_post_create
 *    через nova_fiber_committed_low().
 *  - **GC-интеграция** (Ф.2 §5.2): GC_set_push_other_roots-колбэк пушит
 *    закоммиченные диапазоны живых fiber'ов + native scheduler-стеки.
 *    GC_push_all_eager (НЕ GC_push_all — тот переполняет mark-stack).
 *
 * **M:N-модель (Ф.3 §5.3, §П3-П5).** Каждый поток (main + каждый worker)
 * имеет СВОЮ арену. Арены — heap-аллоцированные структуры в глобальном
 * append-only списке `_nova_fw_arena_list`; TLS хранит лишь указатель
 * (`_t_arena`) — структура переживает поток-владельца, что нужно для
 * GC-колбэка и cross-thread dealloc:
 *   - GC-колбэк обходит ВСЕ арены → видит fiber-стеки всех worker'ов;
 *   - cross-thread dealloc (work-stealing: fiber мигрировал A→B,
 *     завершился на B) находит арену-владельца ПО АДРЕСУ, не по TLS;
 *   - `used_bits` / `slots_active` мутируются atomically (worker A
 *     alloc'ит, worker B освобождает мигрировавший слот);
 *   - native_base каждой арены пушится колбэком — покрывает «подвешенные»
 *     scheduler-кадры worker'а, крутящего fiber (§П3.3).
 * Worker-арены освобождаются в nova_runtime_shutdown ПОСЛЕ join всех
 * worker'ов (nova_fiber_arena_release_retired) — эксклюзивный момент,
 * гонок с обходом нет.
 *
 * Файл компилируется на всех платформах; вне _WIN32 — пустой TU. */

#include "fiber_arena.h"

#if defined(_WIN32) && NOVA_FIBER_ARENA_ENABLED

#include <windows.h>
#include <intrin.h>     /* _BitScanForward64, _interlocked* */
#include <stdint.h>
#include <string.h>
#include <stdlib.h>     /* malloc/free — heap-аллокация структуры арены */

/* Plan 82 Ф.2: GC-интеграция. Подключается на Boehm-бэкенде. minicoro.h —
 * ради public mco_coro / mco_status (без MINICORO_IMPL). */
#ifdef NOVA_GC_BOEHM
#include <gc/gc.h>
#include <gc/gc_mark.h>
#include "minicoro.h"
#endif

/* Debug-инструментация — -DNOVA_FIBER_ARENA_DEBUG включает трассировку. */
#ifdef NOVA_FIBER_ARENA_DEBUG
#include <stdio.h>
#define NOVA_FW_DBG(...) do { fprintf(stderr, "[fw] " __VA_ARGS__); fflush(stderr); } while(0)
#else
#define NOVA_FW_DBG(...) do { } while(0)
#endif

/* ── Конфигурация ───────────────────────────────────────────────── */

#define NOVA_FW_PAGE            ((size_t)4096)
#define NOVA_FW_HEADER_COMMIT   ((size_t)(8  * 1024))   /* commit под minicoro-header */
#define NOVA_FW_INITIAL_COMMIT  ((size_t)(16 * 1024))   /* начальное окно стека */
#define NOVA_FW_BITMAP_WORDS    ((NOVA_FIBER_SLOT_COUNT + 63) / 64)

/* ── Состояние арены — heap-структура, узел append-only списка ──────── */

typedef struct NovaFiberArenaWin {
    char*            base;          /* VirtualAlloc(MEM_RESERVE); NULL = retired */
    size_t           virtual_size;
    size_t           slot_size;
    size_t           slot_count;
    volatile LONG64  slots_active;  /* atomic — inc(alloc) / dec(dealloc) */
    size_t           high_water;    /* пишет только поток-владелец (в alloc) */
    volatile LONG64  used_bits[NOVA_FW_BITMAP_WORDS];   /* atomic bit-ops */
    uint64_t         dirty_bits[NOVA_FW_BITMAP_WORDS];  /* владелец-only */
    char*            native_base;   /* NT_TIB.StackBase потока-владельца */
    DWORD            owner_tid;
    struct NovaFiberArenaWin* volatile next;  /* append-only глоб. список */
} NovaFiberArenaWin;

/* TLS — лишь УКАЗАТЕЛЬ на heap-структуру (структура переживает поток). */
static __declspec(thread) NovaFiberArenaWin* _t_arena = NULL;

/* Глобальный append-only список арен. Во время работы узлы только
 * добавляются (в голову, под _nova_fw_list_lock); обход (GC-колбэк,
 * find_arena, VEH) — lock-free. Освобождение — только в shutdown. */
static NovaFiberArenaWin* volatile _nova_fw_arena_list = NULL;
static CRITICAL_SECTION  _nova_fw_list_lock;

static INIT_ONCE _nova_fw_once = INIT_ONCE_STATIC_INIT;

/* ── Минимальный stderr-вывод для overflow-диагностики ──────────── */
/* На STATUS_STACK_OVERFLOW стека почти нет — без printf (форматирование
 * требует кадра): статический буфер + ручная сборка + WriteFile. */

static void _nova_fw_append(char* buf, size_t cap, size_t* pos, const char* s) {
    while (*s && *pos + 1 < cap) buf[(*pos)++] = *s++;
}
static void _nova_fw_append_uz(char* buf, size_t cap, size_t* pos, size_t v) {
    char tmp[24];
    int n = 0;
    if (v == 0) tmp[n++] = '0';
    while (v > 0 && n < 24) { tmp[n++] = (char)('0' + (v % 10)); v /= 10; }
    while (n > 0 && *pos + 1 < cap) buf[(*pos)++] = tmp[--n];
}
static void _nova_fw_report_overflow(size_t slot, DWORD code) {
    char buf[160];
    size_t pos = 0;
    _nova_fw_append(buf, sizeof(buf), &pos,
                    "\nnova: fiber stack overflow in slot ");
    _nova_fw_append_uz(buf, sizeof(buf), &pos, slot);
    _nova_fw_append(buf, sizeof(buf), &pos,
        (code == EXCEPTION_STACK_OVERFLOW)
            ? " (STATUS_STACK_OVERFLOW)\n"
            : " (access violation in fiber arena)\n");
    _nova_fw_append(buf, sizeof(buf), &pos,
        "Hint: increase fiber stack size or reduce recursion depth.\n");
    HANDLE h = GetStdHandle(STD_ERROR_HANDLE);
    if (h && h != INVALID_HANDLE_VALUE) {
        DWORD written = 0;
        WriteFile(h, buf, (DWORD)pos, &written, NULL);
    }
}

/* ── Address-based поиск арены-владельца ────────────────────────────
 *
 * Обход append-only списка без лока. Узлы во время работы не
 * удаляются и не перемещаются (release_retired — только в shutdown,
 * когда worker'ы мертвы). retired-узел: base == NULL → skip. */
static NovaFiberArenaWin* _nova_fw_find_arena(const void* ptr) {
    const char* p = (const char*)ptr;
    for (NovaFiberArenaWin* a = _nova_fw_arena_list; a; a = a->next) {
        char* base = a->base;   /* atomic-ish snapshot (выровненный ptr) */
        if (base && p >= base && p < base + a->virtual_size) return a;
    }
    return NULL;
}

/* ── VEH — диагностика overflow (multi-arena) ───────────────────── */

static LONG CALLBACK _nova_fw_veh(EXCEPTION_POINTERS* ep) {
    DWORD code = ep->ExceptionRecord->ExceptionCode;
    if (code != EXCEPTION_STACK_OVERFLOW &&
        code != EXCEPTION_ACCESS_VIOLATION) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    ULONG_PTR addr = 0;
    if (ep->ExceptionRecord->NumberParameters >= 2) {
        addr = ep->ExceptionRecord->ExceptionInformation[1];
    }
    if (addr == 0) {
        addr = (ULONG_PTR)__readgsqword(0x10);  /* NT_TIB.StackLimit */
    }
    NovaFiberArenaWin* a = _nova_fw_find_arena((const void*)addr);
    if (!a) return EXCEPTION_CONTINUE_SEARCH;   /* не наша арена */
    size_t slot = (size_t)(addr - (ULONG_PTR)a->base) / a->slot_size;
    _nova_fw_report_overflow(slot, code);
    /* Overflow фатален — отдаём фолт штатной обработке (краш / отладчик). */
    return EXCEPTION_CONTINUE_SEARCH;
}

/* ── GC integration — precise push живых fiber-стеков (§5.2) ────────── */

#ifdef NOVA_GC_BOEHM
/* Push все MEM_COMMIT non-guard non-noaccess диапазоны [lo,hi) как
 * conservative GC-roots. GC_push_all_eager (НЕ GC_push_all — тот лишь
 * кладёт дескриптор на mark-stack; тысячи fiber-стеков переполняют его
 * и вешают Boehm на ~2048 fiber'ах — Ф.1 находка, f1_gc_test T4). */
static void _nova_fw_gc_push_region(char* lo, char* hi) {
    char* p = lo;
    while (p < hi) {
        MEMORY_BASIC_INFORMATION mbi;
        if (!VirtualQuery(p, &mbi, sizeof(mbi))) break;
        char* rstart = (char*)mbi.BaseAddress;
        char* next   = rstart + mbi.RegionSize;
        char* rend   = next > hi ? hi : next;
        char* cs     = rstart < lo ? lo : rstart;
        if (mbi.State == MEM_COMMIT
            && !(mbi.Protect & PAGE_GUARD)
            && !(mbi.Protect & PAGE_NOACCESS)
            && rend > cs) {
            GC_push_all_eager(cs, rend);
        }
        if (next <= p) break;          /* paranoia — нет прогресса */
        p = next;
    }
}

/* GC_set_push_other_roots-колбэк. Mark-фаза, мир остановлен — все
 * GC-зарегистрированные потоки suspended → bitmap/high_water стабильны;
 * обход append-only списка без лока безопасен. */
static void _nova_fw_gc_push_other_roots(void) {
    size_t pushed = 0, arenas = 0;
    for (NovaFiberArenaWin* a = _nova_fw_arena_list; a; a = a->next) {
        char* base = a->base;
        if (!base) continue;            /* retired */
        arenas++;
        for (size_t slot = 0; slot < a->high_water; slot++) {
            if (!((a->used_bits[slot >> 6] >> (slot & 63)) & 1)) continue;
            char* slot_base = base + slot * a->slot_size;
            mco_coro* co = (mco_coro*)(slot_base + NOVA_FIBER_GUARD_SIZE);
            if (mco_status(co) == MCO_DEAD) continue;
            /* закоммиченные диапазоны usable-региона слота (minicoro-
             * header + уросший стек); guard/reserved VirtualQuery
             * отфильтрует */
            _nova_fw_gc_push_region(slot_base + NOVA_FIBER_GUARD_SIZE,
                                    slot_base + a->slot_size);
            pushed++;
        }
        /* native-стек потока-владельца — «подвешенные» scheduler-кадры,
         * пока поток крутит fiber (§П3.3); per-thread скан Boehm их не
         * видит — TIB свопнут на коро-стек. */
        if (a->native_base) {
            MEMORY_BASIC_INFORMATION mbi;
            if (VirtualQuery(a->native_base - 1, &mbi, sizeof(mbi))) {
                _nova_fw_gc_push_region((char*)mbi.AllocationBase,
                                        a->native_base);
            }
        }
    }
    NOVA_FW_DBG("gc_push: %zu arenas, %zu live fibers\n", arenas, pushed);
}
#endif /* NOVA_GC_BOEHM */

/* ── Process-global one-time init ───────────────────────────────── */

static BOOL CALLBACK _nova_fw_global_init(PINIT_ONCE once, PVOID param,
                                          PVOID* ctx) {
    (void)once; (void)param; (void)ctx;
    InitializeCriticalSection(&_nova_fw_list_lock);
    /* First-handler (1) — раньше компилятор-генерированных __except'ов;
     * для не-arena фолтов сразу EXCEPTION_CONTINUE_SEARCH → невмешательство. */
    AddVectoredExceptionHandler(1, _nova_fw_veh);
#ifdef NOVA_GC_BOEHM
    GC_set_push_other_roots(_nova_fw_gc_push_other_roots);
#endif
    return TRUE;
}

/* ── find free slot ─────────────────────────────────────────────── */

/* Захватывает первый свободный слот атомарно. alloc вызывается только
 * на потоке-владельце арены → SET одного бита не конкурирует с другим
 * SET; конкурирует лишь с CLEAR (cross-thread dealloc), а они работают
 * над разными состояниями одного бита. _interlockedbittestandset64 —
 * per-bit lock-bts. */
static size_t _nova_fw_find_free(NovaFiberArenaWin* a) {
    size_t words = (a->slot_count + 63) >> 6;
    for (size_t w = 0; w < words; w++) {
        uint64_t inv = ~(uint64_t)a->used_bits[w];
        while (inv) {
            unsigned long bit;
            _BitScanForward64(&bit, inv);
            size_t slot = w * 64 + (size_t)bit;
            if (slot >= a->slot_count) return (size_t)-1;
            if (!_interlockedbittestandset64(&a->used_bits[w], (LONG64)bit)) {
                return slot;            /* бит был свободен — захватили */
            }
            inv &= inv - 1;             /* занят — следующий */
        }
    }
    return (size_t)-1;
}

/* ── Init ───────────────────────────────────────────────────────── */

void nova_fiber_arena_init(void) {
    if (_t_arena) return;  /* арена этого потока уже создана */

    InitOnceExecuteOnce(&_nova_fw_once, _nova_fw_global_init, NULL, NULL);

    size_t slot_size  = NOVA_FIBER_STACK_SIZE;
    size_t slot_count = NOVA_FIBER_SLOT_COUNT;
    if (slot_count > NOVA_FW_BITMAP_WORDS * 64) {
        slot_count = NOVA_FW_BITMAP_WORDS * 64;
    }
    size_t virtual_size = slot_size * slot_count;

    void* p = VirtualAlloc(NULL, virtual_size, MEM_RESERVE, PAGE_NOACCESS);
    if (!p) {
        /* Downsize-ретрай при фрагментации address space. */
        while (!p && slot_count > 64) {
            slot_count  /= 2;
            virtual_size = slot_size * slot_count;
            p = VirtualAlloc(NULL, virtual_size, MEM_RESERVE, PAGE_NOACCESS);
        }
        if (!p) {
            char msg[] = "nova: fiber_arena VirtualAlloc(MEM_RESERVE) failed\n";
            HANDLE h = GetStdHandle(STD_ERROR_HANDLE);
            DWORD wr = 0;
            if (h && h != INVALID_HANDLE_VALUE)
                WriteFile(h, msg, (DWORD)sizeof(msg) - 1, &wr, NULL);
            ExitProcess(70);
        }
    }

    NovaFiberArenaWin* a = (NovaFiberArenaWin*)calloc(1, sizeof(*a));
    if (!a) {
        char msg[] = "nova: fiber_arena struct OOM\n";
        HANDLE h = GetStdHandle(STD_ERROR_HANDLE);
        DWORD wr = 0;
        if (h && h != INVALID_HANDLE_VALUE)
            WriteFile(h, msg, (DWORD)sizeof(msg) - 1, &wr, NULL);
        ExitProcess(70);
    }
    a->base         = (char*)p;
    a->virtual_size = virtual_size;
    a->slot_size    = slot_size;
    a->slot_count   = slot_count;
    a->owner_tid    = GetCurrentThreadId();
    /* native-стек захватываем СЕЙЧАС: init идёт на потоке-владельце,
     * TIB ещё описывает native-стек (fiber не запущен). */
    a->native_base  = (char*)__readgsqword(0x08);  /* NT_TIB.StackBase */

    /* Линк в голову append-only списка под локом. */
    EnterCriticalSection(&_nova_fw_list_lock);
    a->next = _nova_fw_arena_list;
    _nova_fw_arena_list = a;
    LeaveCriticalSection(&_nova_fw_list_lock);

    _t_arena = a;
    NOVA_FW_DBG("arena_init: tid=%lu base=%p slots=%zu reserve=%zuMB\n",
                a->owner_tid, (void*)a->base, a->slot_count,
                a->virtual_size / (1024 * 1024));
}

/* ── Slot commit ────────────────────────────────────────────────── */

/* Закоммитить слот: header-окно у низа блока + начальное окно стека у
 * вершины + PAGE_GUARD ниже окна. Возвращает 0 при ошибке. */
static int _nova_fw_commit_slot(char* slot_base, size_t slot_size) {
    char* block_base = slot_base + NOVA_FIBER_GUARD_SIZE;
    char* slot_top   = slot_base + slot_size;
    char* window_lo  = slot_top - NOVA_FW_INITIAL_COMMIT;
    char* guard_page = window_lo - NOVA_FW_PAGE;

    if (!VirtualAlloc(block_base, NOVA_FW_HEADER_COMMIT,
                      MEM_COMMIT, PAGE_READWRITE)) return 0;
    if (!VirtualAlloc(window_lo, NOVA_FW_INITIAL_COMMIT,
                      MEM_COMMIT, PAGE_READWRITE)) return 0;
    if (!VirtualAlloc(guard_page, NOVA_FW_PAGE,
                      MEM_COMMIT, PAGE_READWRITE | PAGE_GUARD)) return 0;
    return 1;
}

/* ── minicoro alloc callbacks ───────────────────────────────────── */

void* nova_fiber_alloc(size_t size, void* allocator_data) {
    (void)allocator_data;
    if (!_t_arena) nova_fiber_arena_init();
    NovaFiberArenaWin* a = _t_arena;

    size_t usable = a->slot_size - NOVA_FIBER_GUARD_SIZE;
    if (size > usable) {
        char msg[] = "nova: fiber_alloc request exceeds slot usable size\n";
        HANDLE h = GetStdHandle(STD_ERROR_HANDLE);
        DWORD wr = 0;
        if (h && h != INVALID_HANDLE_VALUE)
            WriteFile(h, msg, (DWORD)sizeof(msg) - 1, &wr, NULL);
        return NULL;
    }

    size_t slot = _nova_fw_find_free(a);
    if (slot == (size_t)-1) {
        char msg[] = "nova: fiber_arena exhausted (all slots in use)\n";
        HANDLE h = GetStdHandle(STD_ERROR_HANDLE);
        DWORD wr = 0;
        if (h && h != INVALID_HANDLE_VALUE)
            WriteFile(h, msg, (DWORD)sizeof(msg) - 1, &wr, NULL);
        ExitProcess(70);
    }

    char* slot_base = a->base + slot * a->slot_size;

    /* Грязный слот (закоммичен прошлым fiber'ом) → сброс в reserved.
     * dirty_bits — владелец-only (alloc + compact на одном потоке). */
    if ((a->dirty_bits[slot >> 6] >> (slot & 63)) & 1) {
        VirtualFree(slot_base + NOVA_FIBER_GUARD_SIZE,
                    a->slot_size - NOVA_FIBER_GUARD_SIZE, MEM_DECOMMIT);
    }

    if (!_nova_fw_commit_slot(slot_base, a->slot_size)) {
        VirtualFree(slot_base + NOVA_FIBER_GUARD_SIZE,
                    a->slot_size - NOVA_FIBER_GUARD_SIZE, MEM_DECOMMIT);
        a->dirty_bits[slot >> 6] &= ~(1ULL << (slot & 63));
        _interlockedbittestandreset64(&a->used_bits[slot >> 6],
                                      (LONG64)(slot & 63));
        return NULL;
    }

    a->dirty_bits[slot >> 6] |= (1ULL << (slot & 63));
    _InterlockedIncrement64(&a->slots_active);
    if (slot + 1 > a->high_water) a->high_water = slot + 1;

    NOVA_FW_DBG("alloc tid=%lu slot=%zu active=%lld\n",
                GetCurrentThreadId(), slot, (long long)a->slots_active);
    return slot_base + NOVA_FIBER_GUARD_SIZE;
}

/* Address-based dealloc — арена-владелец находится ПО АДРЕСУ, не по TLS.
 * Под M:N (work-stealing) fiber, мигрировавший A→B, завершается на B:
 * dealloc исполняется на B, но слот — в арене A (§5.3). */
void nova_fiber_dealloc(void* ptr, size_t size, void* allocator_data) {
    (void)size; (void)allocator_data;
    if (!ptr) return;

    NovaFiberArenaWin* a = _nova_fw_find_arena(ptr);
    if (!a) return;   /* не из fiber-арены — игнор */
    char* p = (char*)ptr;
    if (p < a->base + NOVA_FIBER_GUARD_SIZE) return;

    size_t slot = (size_t)(p - a->base) / a->slot_size;
    if (slot >= a->slot_count) return;
    /* CLEAR used-бит атомарно; вернул 0 → бит уже был сброшен (double-free)
     * → не декрементируем counter. */
    if (!_interlockedbittestandreset64(&a->used_bits[slot >> 6],
                                       (LONG64)(slot & 63))) {
        return;
    }
    _InterlockedDecrement64(&a->slots_active);
    NOVA_FW_DBG("dealloc tid=%lu slot=%zu active=%lld\n",
                GetCurrentThreadId(), slot, (long long)a->slots_active);
    /* Decommit — послотно при переиспользовании (alloc dirty-ветка), НЕ
     * idle-batch по огромному диапазону (Ф.1 находка). Слот остаётся
     * dirty; cross-thread dealloc dirty_bits не трогает (владелец-only). */
}

/* ── stack_limit-патч helper (Ф.0 test a) ───────────────────────── */

void* nova_fiber_committed_low(const void* block_ptr) {
    if (!block_ptr) return NULL;
    NovaFiberArenaWin* a = _nova_fw_find_arena(block_ptr);
    if (!a) return NULL;
    size_t slot = (size_t)((const char*)block_ptr - a->base) / a->slot_size;
    char* slot_top = a->base + (slot + 1) * a->slot_size;
    return slot_top - NOVA_FW_INITIAL_COMMIT;
}

/* ── Misc API ───────────────────────────────────────────────────── */

bool nova_fiber_arena_contains(const void* ptr) {
    return _nova_fw_find_arena(ptr) != NULL;
}

/* Plan 83.4.3 (2026-05-23) — B1 fix: GLOBAL aggregation через обход
 * `_nova_fw_arena_list`. Раньше per-thread (только current thread's
 * arena) → main thread видел 0 slots после spawn на worker'е. Теперь
 * обход list'а суммирует все арены — паритет с Go `runtime.NumGoroutine()`
 * / tokio `RuntimeMetrics.num_alive_tasks` (global view).
 * List read lock-free (Plan 82 Ф.3 append-only — retire только в shutdown). */
NovaFiberArenaStats nova_fiber_arena_stats(void) {
    NovaFiberArenaStats s = { 0 };
    for (NovaFiberArenaWin* a = _nova_fw_arena_list; a; a = a->next) {
        if (!a->base) continue;  /* retired */
        s.virtual_reserved += a->virtual_size;
        s.slot_count       += a->slot_count;
        s.slots_active     += (size_t)a->slots_active;
        s.high_water       += a->high_water;
    }
    return s;
}

/* Явный compact: decommit committed-страниц всех СВОБОДНЫХ слотов арены
 * текущего потока. dirty_bits — владелец-only, compact зовётся владельцем. */
void nova_fiber_arena_compact(void) {
    NovaFiberArenaWin* a = _t_arena;
    if (!a || a->high_water == 0) return;
    for (size_t slot = 0; slot < a->high_water; slot++) {
        if ((a->used_bits[slot >> 6] >> (slot & 63)) & 1) continue;
        if (!((a->dirty_bits[slot >> 6] >> (slot & 63)) & 1)) continue;
        char* slot_base = a->base + slot * a->slot_size;
        VirtualFree(slot_base + NOVA_FIBER_GUARD_SIZE,
                    a->slot_size - NOVA_FIBER_GUARD_SIZE, MEM_DECOMMIT);
        a->dirty_bits[slot >> 6] &= ~(1ULL << (slot & 63));
    }
}

/* ── Shutdown — освобождение worker-арен (Ф.3) ──────────────────────
 *
 * Вызывается nova_runtime_shutdown ПОСЛЕ join всех worker-потоков:
 * worker'ы мертвы, исполняется только main → эксклюзивный момент, гонок
 * с обходом списка (GC-колбэк / find_arena) нет. Освобождает арены
 * не-main потоков (owner_tid != main_tid), unlink, free структуру.
 * Главная арена (main) остаётся — main живёт до process-exit. */
void nova_fiber_arena_release_retired(void) {
    /* Вызывается nova_runtime_shutdown на потоке shutdown'а (main).
     * Освобождаются арены ВСЕХ прочих потоков (= worker'ов, уже
     * join'нутых); арена текущего потока остаётся — main живёт дальше. */
    DWORD keep_tid = GetCurrentThreadId();
    EnterCriticalSection(&_nova_fw_list_lock);
    NovaFiberArenaWin* volatile* link = &_nova_fw_arena_list;
    NovaFiberArenaWin* a = _nova_fw_arena_list;
    while (a) {
        NovaFiberArenaWin* next = a->next;
        if (a->owner_tid != keep_tid && a->base) {
            *link = next;                /* unlink */
            VirtualFree(a->base, 0, MEM_RELEASE);
            free(a);
        } else {
            link = &a->next;
        }
        a = next;
    }
    LeaveCriticalSection(&_nova_fw_list_lock);
}

/* Worker зовёт перед GC_unregister_my_thread — обнуляет TLS-указатель
 * (структура арены остаётся в списке, освободит release_retired). */
void nova_fiber_arena_thread_exit(void) {
    _t_arena = NULL;
}

#else /* !_WIN32 || !NOVA_FIBER_ARENA_ENABLED — пустой TU */

typedef int _nova_fiber_arena_win_disabled_marker;

#endif
