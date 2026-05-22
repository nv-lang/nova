// SPDX-License-Identifier: MIT OR Apache-2.0
/* Plan 82 Ф.1 — Windows fiber stack arena (lazy-commit, large-reserve).
 *
 * Симметрична POSIX-реализации fiber_arena.c (Plan 44.2), но на Windows-
 * примитивах VirtualAlloc/VirtualFree вместо mmap/madvise/mprotect.
 * Заменяет провальный Plan 44.3 (4 неудачные интеграции 2026-05-13/14).
 *
 * Дизайн (верифицирован Ф.0 re-diagnosis — 82-artifacts/f0-rediagnosis.md):
 *
 *  - **Путь A — OS-native grow.** После TIB-свопа minicoro-asm'а стек
 *    корутины для MM-ядра неотличим от CreateFiber-стека: ядро растит
 *    его штатно через PAGE_GUARD-фолт (test a, decision-point). Custom
 *    VEH для happy-path НЕ нужен — только диагностика overflow.
 *
 *  - **Lazy commit.** Арена — один VirtualAlloc(MEM_RESERVE) на поток;
 *    физический commit только под header + начальное окно у вершины
 *    стека. Остальное ядро коммитит по факту роста.
 *
 *  - **Раскладка слота** (low→high), §5.1 плана:
 *      [hard guard 16K][minicoro header][... reserved/grown ...][window]
 *       ^ MEM_RESERVE   ^ commit RW      ^ ядро коммитит вниз     ^ commit
 *       AV на касании   (mco_coro+ctx+   при росте стека          RW +
 *       (stack-clash     storage)                                 PAGE_GUARD
 *        backstop)                                                ниже окна
 *    Стек растёт вниз ОТ window. minicoro кладёт DeallocationStack на
 *    нижнюю границу stack-секции (выше header'а) → ядро поднимает
 *    STATUS_STACK_OVERFLOW ДО порчи header'а. Hard guard (низ слота,
 *    16K reserved) ловит stack-clash единичным гигантским кадром.
 *
 *  - **Патч ctx.stack_limit ОБЯЗАТЕЛЕН** (test a, ключевая находка):
 *    minicoro _mco_makectx ставит stack_limit = низ всего стека (claim'ит
 *    весь стек закоммиченным). При lazy-commit это ложь → __chkstk-код
 *    (кадр >1 страницы) крашит на MSVC. Патч выполняется в fibers.c
 *    (nova_fiber_post_create) через nova_fiber_committed_low().
 *
 *  - **VEH — только диагностика.** Ловит STATUS_STACK_OVERFLOW / AV в
 *    арене → печатает «fiber stack overflow in slot N» → отдаёт фолт
 *    дальше (overflow фатален). Паритет с Linux _arena_sigsegv_handler.
 *    Hardware-SEH compiler-независимо ловится только через VEH, не
 *    __try/__except (Ф.0 §3).
 *
 * Файл компилируется на всех платформах; вне _WIN32 — пустой TU. */

#include "fiber_arena.h"

#if defined(_WIN32) && NOVA_FIBER_ARENA_ENABLED

#include <windows.h>
#include <intrin.h>     /* _BitScanForward64 */
#include <stdint.h>
#include <string.h>

/* Plan 82 Ф.2 (§5.2): GC-интеграция fiber-стеков. Подключается, когда
 * рантайм собран на Boehm-бэкенде. minicoro.h — ради public mco_coro /
 * mco_status (без MINICORO_IMPL — только декларации). */
#ifdef NOVA_GC_BOEHM
#include <gc/gc.h>
#include <gc/gc_mark.h>
#include "minicoro.h"
#endif

/* Debug-инструментация — отключена по умолчанию; -DNOVA_FIBER_ARENA_DEBUG
 * включает трассировку alloc/dealloc/init/gc-push в stderr. */
#ifdef NOVA_FIBER_ARENA_DEBUG
#include <stdio.h>
#define NOVA_FW_DBG(...) do { fprintf(stderr, "[fw] " __VA_ARGS__); fflush(stderr); } while(0)
#else
#define NOVA_FW_DBG(...) do { } while(0)
#endif

/* ── Конфигурация ───────────────────────────────────────────────── */

#define NOVA_FW_PAGE            ((size_t)4096)
/* Header-окно: commit под [mco_coro][_mco_context][storage] у низа
 * блока. 8K с запасом — реальный header amd64 ≈ 1.8K; совпадает с
 * _NOVA_MCO_HEADER_OVERHEAD (fibers.h). */
#define NOVA_FW_HEADER_COMMIT   ((size_t)(8  * 1024))
/* Начальное окно стека у вершины: commit под dummy-RA + первые кадры
 * _mco_main до первого guard-роста. Дальше ядро растит сам. Tunable. */
#define NOVA_FW_INITIAL_COMMIT  ((size_t)(16 * 1024))

#define NOVA_FW_BITMAP_WORDS    ((NOVA_FIBER_SLOT_COUNT + 63) / 64)

/* ── Per-thread arena state ─────────────────────────────────────── */

typedef struct {
    char*    base;            /* VirtualAlloc(MEM_RESERVE) base          */
    size_t   virtual_size;    /* slot_size * slot_count                  */
    size_t   slot_size;       /* per-slot (incl. hard guard)             */
    size_t   slot_count;
    size_t   slots_active;
    size_t   high_water;      /* highest slot index ever used + 1        */
    uint64_t used_bits[NOVA_FW_BITMAP_WORDS];   /* 1 = slot occupied     */
    uint64_t dirty_bits[NOVA_FW_BITMAP_WORDS];  /* 1 = slot has commits  */
} NovaFiberArenaWin;

static __declspec(thread) NovaFiberArenaWin _t_arena = { 0 };

/* ── Process-global one-time init (VEH + FLS index) ─────────────── */

static INIT_ONCE _nova_fw_once = INIT_ONCE_STATIC_INIT;
static DWORD     _nova_fw_fls_index = FLS_OUT_OF_INDEXES;

/* ── Минимальный stderr-вывод для overflow-диагностики ──────────── */
/* На STATUS_STACK_OVERFLOW стека почти не осталось — никаких printf/
 * fprintf (форматирование требует кадра). Только статический буфер +
 * ручная сборка строки + WriteFile. */

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

/* ── VEH — диагностика overflow (путь A: happy-path сюда не приходит) ─ */

static LONG CALLBACK _nova_fw_veh(EXCEPTION_POINTERS* ep) {
    DWORD code = ep->ExceptionRecord->ExceptionCode;
    if (code != EXCEPTION_STACK_OVERFLOW &&
        code != EXCEPTION_ACCESS_VIOLATION) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    NovaFiberArenaWin* a = &_t_arena;
    if (!a->base) return EXCEPTION_CONTINUE_SEARCH;

    /* Адрес фолта: для AV — ExceptionInformation[1]; для overflow его
     * может не быть → берём текущий TIB.StackLimit (коро-стек при
     * запущенном fiber'е). */
    ULONG_PTR addr = 0;
    if (ep->ExceptionRecord->NumberParameters >= 2) {
        addr = ep->ExceptionRecord->ExceptionInformation[1];
    }
    if (addr == 0) {
        addr = (ULONG_PTR)__readgsqword(0x10);  /* NT_TIB.StackLimit */
    }
    if (addr < (ULONG_PTR)a->base ||
        addr >= (ULONG_PTR)(a->base + a->virtual_size)) {
        return EXCEPTION_CONTINUE_SEARCH;       /* не наша арена */
    }
    size_t slot = (size_t)(addr - (ULONG_PTR)a->base) / a->slot_size;
    _nova_fw_report_overflow(slot, code);
    /* Overflow фатален — отдаём фолт штатной обработке (краш процесса /
     * отладчик). Паритет с Linux: handler печатает и re-raise'ит. */
    return EXCEPTION_CONTINUE_SEARCH;
}

/* ── FLS callback — освобождение арены при выходе потока ─────────── */

static void WINAPI _nova_fw_fls_cb(void* arg) {
    NovaFiberArenaWin* a = (NovaFiberArenaWin*)arg;
    if (!a || !a->base) return;
    VirtualFree(a->base, 0, MEM_RELEASE);
    memset(a, 0, sizeof(*a));
}

static BOOL CALLBACK _nova_fw_global_init(PINIT_ONCE once, PVOID param,
                                          PVOID* ctx) {
    (void)once; (void)param; (void)ctx;
    _nova_fw_fls_index = FlsAlloc(_nova_fw_fls_cb);
    /* First-handler (1) — раньше компилятор-генерированных __except'ов;
     * для не-arena фолтов сразу EXCEPTION_CONTINUE_SEARCH → невмешательство. */
    AddVectoredExceptionHandler(1, _nova_fw_veh);
    return TRUE;
}

/* ── Bitmap helpers ─────────────────────────────────────────────── */

static size_t _nova_fw_find_free(NovaFiberArenaWin* a) {
    size_t words = (a->slot_count + 63) / 64;
    for (size_t w = 0; w < words; w++) {
        uint64_t inv = ~a->used_bits[w];
        if (inv == 0) continue;
        unsigned long bit;
        _BitScanForward64(&bit, inv);
        size_t slot = w * 64 + bit;
        if (slot < a->slot_count) return slot;
    }
    return (size_t)-1;
}

static int  _nova_fw_test_bit(const uint64_t* bm, size_t s) {
    return (bm[s / 64] >> (s % 64)) & 1u;
}
static void _nova_fw_set_bit(uint64_t* bm, size_t s)   { bm[s/64] |=  (1ULL << (s%64)); }
static void _nova_fw_clear_bit(uint64_t* bm, size_t s) { bm[s/64] &= ~(1ULL << (s%64)); }

/* ── GC integration — precise push живых fiber-стеков (§5.2) ──────────
 *
 * Арена кладёт fiber-стеки в 32 GB VirtualAlloc-регион далеко от native-
 * стека. Штатный per-thread conservative-скан Boehm их НЕ покрывает —
 * Ф.1-харнесс `82-artifacts/f1_gc_test.c` подтвердил: объект, удерживаемый
 * только указателем на коро-стеке, собирается GC (UAF). С minicoro-default
 * calloc'ом это работало случайно — calloc-стек попадал в over-scan через
 * C-heap. §1.6-допущение «per-thread скан + VirtualQuery-clamp покрывает
 * running fiber» — опровергнуто эмпирически; §9-риск-3 это предусматривал
 * («running fiber придётся покрывать push-колбэком»).
 *
 * `GC_set_push_other_roots`-колбэк (mark-фаза, мир остановлен) пушит:
 *   (а) КАЖДЫЙ живой (не-MCO_DEAD) fiber — закоммиченные диапазоны его
 *       слота (minicoro-header + уросший стек + initial-окно), кроме
 *       guard- и reserved-страниц (чтение reserved → AV, §1.3);
 *   (б) native-стек main-thread'а — на нём «подвешены» scheduler-кадры
 *       с NovaFiberQueue-scope, пока крутится fiber (§П3.3); штатный
 *       per-thread скан их не видит — TIB свопнут на коро-стек.
 *
 * Реестр живых fiber'ов = `used_bits` арены: used-слот → mco_coro* в
 * `slot_base + GUARD`. Отдельный интрузивный список (§5.2 (1)) не нужен —
 * арена УЖЕ полный реестр всех живых fiber'ов. Single-thread (Ф.1):
 * колбэк и alloc/dealloc исполняются на одном потоке, взаимоисключены
 * по построению. M:N (Ф.5): потребует обхода всех per-thread арен +
 * suspended worker-стеков — вне scope Ф.1. */
#ifdef NOVA_GC_BOEHM

static char* _nova_fw_native_base;   /* native stack base main-thread'а */

/* Push все MEM_COMMIT non-guard non-noaccess диапазоны [lo,hi) как
 * conservative GC-roots. VirtualQuery сегментирует регион на пробеги
 * одинаковой защиты — reserved/guard-страницы пропускаются. */
static void _nova_fw_gc_push_region(char* lo, char* hi) {
    char* p = lo;
    while (p < hi) {
        MEMORY_BASIC_INFORMATION mbi;
        if (!VirtualQuery(p, &mbi, sizeof(mbi))) break;
        char* rstart = (char*)mbi.BaseAddress;
        char* rend   = rstart + mbi.RegionSize;
        char* next   = rend;
        if (rend > hi) rend = hi;
        char* cs = rstart < lo ? lo : rstart;
        if (mbi.State == MEM_COMMIT
            && !(mbi.Protect & PAGE_GUARD)
            && !(mbi.Protect & PAGE_NOACCESS)
            && rend > cs) {
            /* GC_push_all_eager, НЕ GC_push_all: eager сканирует диапазон
             * НЕМЕДЛЕННО, а GC_push_all лишь кладёт (lo,hi)-дескриптор на
             * mark-stack. Тысячи fiber-стеков × GC_push_all переполняют
             * mark-stack и вешают Boehm на ~2048 fiber'ах (Ф.1 — реальная
             * находка, харнесс f1_gc_test T4). Eager — без накопления
             * дескрипторов; ровно для скана стеков (gc_mark.h:298). */
            GC_push_all_eager(cs, rend);
        }
        if (next <= p) break;          /* paranoia — нет прогресса */
        p = next;
    }
}

/* GC_set_push_other_roots-колбэк. */
static void _nova_fw_gc_push_other_roots(void) {
    NovaFiberArenaWin* a = &_t_arena;
    NOVA_FW_DBG("gc_push ENTER high_water=%zu active=%zu\n",
                a->base ? a->high_water : 0, a->base ? a->slots_active : 0);
    size_t pushed = 0;
    if (a->base) {
        for (size_t slot = 0; slot < a->high_water; slot++) {
            if (!_nova_fw_test_bit(a->used_bits, slot)) continue;
            char* slot_base = a->base + slot * a->slot_size;
            mco_coro* co = (mco_coro*)(slot_base + NOVA_FIBER_GUARD_SIZE);
            if (mco_status(co) == MCO_DEAD) continue;
            /* закоммиченные диапазоны usable-региона слота (header +
             * стек); guard/reserved VirtualQuery отфильтрует */
            _nova_fw_gc_push_region(slot_base + NOVA_FIBER_GUARD_SIZE,
                                    slot_base + a->slot_size);
            pushed++;
        }
    }
    /* native-стек main-thread'а — scheduler-кадры под fiber-switch'ем */
    if (_nova_fw_native_base) {
        MEMORY_BASIC_INFORMATION mbi;
        if (VirtualQuery(_nova_fw_native_base - 1, &mbi, sizeof(mbi))) {
            _nova_fw_gc_push_region((char*)mbi.AllocationBase,
                                    _nova_fw_native_base);
        }
    }
    NOVA_FW_DBG("gc_push EXIT pushed=%zu fibers\n", pushed);
}

#endif /* NOVA_GC_BOEHM */

/* ── Init ───────────────────────────────────────────────────────── */

void nova_fiber_arena_init(void) {
    if (_t_arena.base) return;  /* already initialized for this thread */

    InitOnceExecuteOnce(&_nova_fw_once, _nova_fw_global_init, NULL, NULL);

    size_t slot_size  = NOVA_FIBER_STACK_SIZE;
    size_t slot_count = NOVA_FIBER_SLOT_COUNT;
    if (slot_count > NOVA_FW_BITMAP_WORDS * 64) {
        /* sanity — конфигурация не помещается в bitmap */
        slot_count = NOVA_FW_BITMAP_WORDS * 64;
    }
    size_t virtual_size = slot_size * slot_count;

    /* Резерв всего адресного пространства арены без commit-charge.
     * PAGE_NOACCESS — касание reserved-страницы и так даёт AV; hard
     * guard каждого слота — это просто его нижние 16K, оставленные
     * reserved. */
    void* p = VirtualAlloc(NULL, virtual_size, MEM_RESERVE, PAGE_NOACCESS);
    if (!p) {
        /* Downsize-ретрай: на 32-bit / при фрагментации address space
         * 32 GB может не зарезервироваться. Половиним slot_count. */
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

    _t_arena.base         = (char*)p;
    _t_arena.virtual_size = virtual_size;
    _t_arena.slot_size    = slot_size;
    _t_arena.slot_count   = slot_count;
    _t_arena.slots_active = 0;
    _t_arena.high_water   = 0;
    memset(_t_arena.used_bits,  0, sizeof(_t_arena.used_bits));
    memset(_t_arena.dirty_bits, 0, sizeof(_t_arena.dirty_bits));

    if (_nova_fw_fls_index != FLS_OUT_OF_INDEXES) {
        FlsSetValue(_nova_fw_fls_index, &_t_arena);
    }

#ifdef NOVA_GC_BOEHM
    /* GC-интеграция (§5.2). native-стек захватываем СЕЙЧАС: init идёт на
     * потоке, создающем первый fiber, и TIB ещё описывает native-стек
     * (fiber только создаётся, не запущен). Колбэк ставится один раз.
     * Single-thread (Ф.1) — гонок на _nova_fw_native_base нет. */
    if (!_nova_fw_native_base) {
        _nova_fw_native_base = (char*)__readgsqword(0x08);  /* NT_TIB.StackBase */
        GC_set_push_other_roots(_nova_fw_gc_push_other_roots);
    }
#endif

    NOVA_FW_DBG("arena_init: base=%p slots=%zu slot_size=%zu reserve=%zuMB\n",
                (void*)_t_arena.base, _t_arena.slot_count, _t_arena.slot_size,
                _t_arena.virtual_size / (1024 * 1024));
}

/* ── Slot commit / reset ────────────────────────────────────────── */

/* Закоммитить слот под нового fiber'а: header-окно у низа блока +
 * начальное окно стека у вершины + PAGE_GUARD ниже окна. Возвращает 0
 * при ошибке. */
static int _nova_fw_commit_slot(char* slot_base, size_t slot_size) {
    char* block_base = slot_base + NOVA_FIBER_GUARD_SIZE;  /* minicoro-блок */
    char* slot_top   = slot_base + slot_size;
    char* window_lo  = slot_top - NOVA_FW_INITIAL_COMMIT;
    char* guard_page = window_lo - NOVA_FW_PAGE;

    /* (1) header-окно — [mco_coro][_mco_context][storage], commit RW. */
    if (!VirtualAlloc(block_base, NOVA_FW_HEADER_COMMIT,
                      MEM_COMMIT, PAGE_READWRITE)) {
        return 0;
    }
    /* (2) начальное окно стека у вершины, commit RW. */
    if (!VirtualAlloc(window_lo, NOVA_FW_INITIAL_COMMIT,
                      MEM_COMMIT, PAGE_READWRITE)) {
        return 0;
    }
    /* (3) PAGE_GUARD-страница сразу под окном — первая «вершина» guard'а.
     * Дальше ядро двигает её вниз само при росте стека (путь A). */
    if (!VirtualAlloc(guard_page, NOVA_FW_PAGE,
                      MEM_COMMIT, PAGE_READWRITE | PAGE_GUARD)) {
        return 0;
    }
    return 1;
}

/* ── minicoro alloc callbacks ───────────────────────────────────── */

void* nova_fiber_alloc(size_t size, void* allocator_data) {
    (void)allocator_data;
    if (!_t_arena.base) nova_fiber_arena_init();

    size_t usable = _t_arena.slot_size - NOVA_FIBER_GUARD_SIZE;
    if (size > usable) {
        char msg[] = "nova: fiber_alloc request exceeds slot usable size\n";
        HANDLE h = GetStdHandle(STD_ERROR_HANDLE);
        DWORD wr = 0;
        if (h && h != INVALID_HANDLE_VALUE)
            WriteFile(h, msg, (DWORD)sizeof(msg) - 1, &wr, NULL);
        return NULL;  /* minicoro трактует как failure */
    }

    size_t slot = _nova_fw_find_free(&_t_arena);
    if (slot == (size_t)-1) {
        /* Паритет с Linux fiber_arena.c: exhaustion — production-ошибка,
         * abort. Plan 44 prep: arena chaining снимет потолок. */
        char msg[] = "nova: fiber_arena exhausted (all slots in use)\n";
        HANDLE h = GetStdHandle(STD_ERROR_HANDLE);
        DWORD wr = 0;
        if (h && h != INVALID_HANDLE_VALUE)
            WriteFile(h, msg, (DWORD)sizeof(msg) - 1, &wr, NULL);
        ExitProcess(70);
    }

    char* slot_base = _t_arena.base + slot * _t_arena.slot_size;

    /* Слот «грязный» (закоммичен прошлым fiber'ом, ещё не сброшен idle-
     * decommit'ом) → сбросить в reserved перед свежим commit'ом, иначе
     * сдвинутый ядром guard и лишние committed-страницы протекут в
     * нового fiber'а. */
    if (_nova_fw_test_bit(_t_arena.dirty_bits, slot)) {
        VirtualFree(slot_base + NOVA_FIBER_GUARD_SIZE,
                    _t_arena.slot_size - NOVA_FIBER_GUARD_SIZE,
                    MEM_DECOMMIT);
    }

    if (!_nova_fw_commit_slot(slot_base, _t_arena.slot_size)) {
        /* commit failed (обычно OOM по commit-charge) — откат. */
        VirtualFree(slot_base + NOVA_FIBER_GUARD_SIZE,
                    _t_arena.slot_size - NOVA_FIBER_GUARD_SIZE,
                    MEM_DECOMMIT);
        _nova_fw_clear_bit(_t_arena.dirty_bits, slot);
        return NULL;
    }

    _nova_fw_set_bit(_t_arena.used_bits,  slot);
    _nova_fw_set_bit(_t_arena.dirty_bits, slot);
    _t_arena.slots_active++;
    if (slot + 1 > _t_arena.high_water) _t_arena.high_water = slot + 1;

    NOVA_FW_DBG("alloc size=%zu -> slot=%zu block=%p active=%zu\n",
                size, slot, (void*)(slot_base + NOVA_FIBER_GUARD_SIZE),
                _t_arena.slots_active);
    /* minicoro получает блок над hard-guard'ом слота. */
    return slot_base + NOVA_FIBER_GUARD_SIZE;
}

void nova_fiber_dealloc(void* ptr, size_t size, void* allocator_data) {
    (void)size; (void)allocator_data;
    if (!ptr || !_t_arena.base) return;

    char* p = (char*)ptr;
    if (p < _t_arena.base + NOVA_FIBER_GUARD_SIZE ||
        p >= _t_arena.base + _t_arena.virtual_size) {
        return;  /* не из этой арены — игнор (cross-thread? Ф.3) */
    }
    size_t offset = (size_t)(p - _t_arena.base);
    size_t slot   = offset / _t_arena.slot_size;
    if (slot >= _t_arena.slot_count) return;
    if (!_nova_fw_test_bit(_t_arena.used_bits, slot)) return;  /* double-free guard */

    _nova_fw_clear_bit(_t_arena.used_bits, slot);
    if (_t_arena.slots_active > 0) _t_arena.slots_active--;
    NOVA_FW_DBG("dealloc %p slot=%zu active=%zu\n", ptr, slot,
                _t_arena.slots_active);

    /* Decommit — НЕ батчем по [base, high_water*slot_size): тот диапазон
     * на 64-bit достигает 32 GB, и VirtualFree(MEM_DECOMMIT) по нему
     * проходит ~8M PTE за вызов; при fiber-churn'е (sleep_bench: 10k
     * fiber'ов, переиспользующих слоты) idle-batch вызывается тысячи раз
     * → деградация на порядки (Ф.1 — реальная находка, отличие от Linux
     * `madvise`, который дёшев по NORESERVE-VMA).
     *
     * Вместо этого слот остаётся dirty и декоммитится ПОСЛОТНО при
     * переиспользовании (nova_fiber_alloc, dirty-ветка) — ограниченный
     * O(slot_size) диапазон. Слот, освобождённый и не переиспользованный,
     * держит commit-charge до reuse либо явного nova_fiber_arena_compact(). */
}

/* ── stack_limit-патч helper (Ф.0 test a — ОБЯЗАТЕЛЕН) ───────────── */

/* Возвращает committed-low слота, содержащего block_ptr (== указатель,
 * который nova_fiber_alloc вернул minicoro == co). Это нижняя граница
 * начального committed-окна стека. fibers.c пишет её в
 * ((_mco_context*)co->context)->ctx.stack_limit после mco_create.
 * NULL — если block_ptr не из арены текущего потока. */
void* nova_fiber_committed_low(const void* block_ptr) {
    if (!_t_arena.base || !block_ptr) return NULL;
    const char* p = (const char*)block_ptr;
    if (p < _t_arena.base ||
        p >= _t_arena.base + _t_arena.virtual_size) {
        return NULL;
    }
    size_t slot = (size_t)(p - _t_arena.base) / _t_arena.slot_size;
    char* slot_top = _t_arena.base + (slot + 1) * _t_arena.slot_size;
    NOVA_FW_DBG("committed_low(%p) slot=%zu -> %p\n", block_ptr, slot,
                (void*)(slot_top - NOVA_FW_INITIAL_COMMIT));
    return slot_top - NOVA_FW_INITIAL_COMMIT;
}

/* ── Misc API (паритет с fiber_arena.c) ─────────────────────────── */

bool nova_fiber_arena_contains(const void* ptr) {
    if (!_t_arena.base) return false;
    return (const char*)ptr >= _t_arena.base &&
           (const char*)ptr <  _t_arena.base + _t_arena.virtual_size;
}

NovaFiberArenaStats nova_fiber_arena_stats(void) {
    NovaFiberArenaStats s = { 0 };
    if (_t_arena.base) {
        s.virtual_reserved = _t_arena.virtual_size;
        s.slot_count       = _t_arena.slot_count;
        s.slots_active     = _t_arena.slots_active;
        s.high_water       = _t_arena.high_water;
    }
    return s;
}

/* Явный compact для long-running workload'ов без natural idle:
 * decommit'ит committed-страницы всех СВОБОДНЫХ слотов. */
void nova_fiber_arena_compact(void) {
    if (!_t_arena.base || _t_arena.high_water == 0) return;
    for (size_t slot = 0; slot < _t_arena.high_water; slot++) {
        if (_nova_fw_test_bit(_t_arena.used_bits, slot)) continue;
        if (!_nova_fw_test_bit(_t_arena.dirty_bits, slot)) continue;
        char* slot_base = _t_arena.base + slot * _t_arena.slot_size;
        VirtualFree(slot_base + NOVA_FIBER_GUARD_SIZE,
                    _t_arena.slot_size - NOVA_FIBER_GUARD_SIZE,
                    MEM_DECOMMIT);
        _nova_fw_clear_bit(_t_arena.dirty_bits, slot);
    }
}

#else /* !_WIN32 || !NOVA_FIBER_ARENA_ENABLED — пустой TU */

typedef int _nova_fiber_arena_win_disabled_marker;

#endif
