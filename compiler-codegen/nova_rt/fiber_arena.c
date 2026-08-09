// SPDX-License-Identifier: MIT OR Apache-2.0
/* [M-mn-spawnctx-corruption-cancel-wake]: pthread_getattr_np (реестр
 * native-стеков для GC push_other_roots) требует _GNU_SOURCE ДО первого
 * glibc-инклюда. */
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
/* Plan 44.2 Etap 1 — per-thread fiber stack arena (Linux/macOS).
 * See fiber_arena.h for design notes.
 *
 * Compiled into binary as separate TU (linked alongside alloc_boehm.c /
 * effects.c / fibers.c). Windows: this TU compiles но не используется —
 * NOVA_FIBER_ARENA_ENABLED == 0 makes everything no-op.
 *
 * Plan 82.2 (2026-05-26): cross-thread dealloc support через глобальный
 * реестр арен — порт механизма из fiber_arena_win.c. Под M:N work-
 * stealing fiber может быть allocated на thread A (через mco_create в
 * nova_runtime_spawn_global), а deallocated на thread B (mco_destroy в
 * worker B'е). Раньше: TLS-based bounds check видел чужой ptr → warning +
 * slot leak (Plan 44.2 явно отложил P41-15 «cross-thread dealloc atomic
 * bitmap» до Plan 23). Теперь: append-only глобальный список арен;
 * nova_fiber_dealloc fast-path = TLS match, slow-path =
 * _nova_find_arena_for(ptr); bitmap clear через __atomic_fetch_and
 * для cross-thread safety. Паритет с Windows fiber_arena_win.c. */

#include "fiber_arena.h"
/* Plan 259 Слой 1: runtime.h больше не нужен — единственный потребитель
 * (nova_runtime_maxprocs() для VMA-бюджета, _nova_vma_slot_budget) снят
 * вместе с зажимом NOVA_ARENA_VMA_* (см. комментарий у
 * nova_fiber_arena_init ниже, D97 Ред.3 / D451). */

/* Plan 82 Ф.1: внутренний guard сужен с NOVA_FIBER_ARENA_ENABLED до
 * явного POSIX-условия. NOVA_FIBER_ARENA_ENABLED теперь true и на
 * Windows (Windows-реализация — fiber_arena_win.c); этот файл — строго
 * POSIX-путь, на Windows компилируется в пустой TU. */
#if defined(__linux__) || defined(__APPLE__)

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <errno.h>      /* Plan 149: strtoll errno check */
#include <fcntl.h>      /* [M-mn-spawnctx-…]: open(/proc/self/maps) в GC-колбэке */
#include <sys/syscall.h> /* [M-mn-spawnctx-…]: SYS_gettid (bootstrap-probe main) */
#include <unistd.h>
#include <sys/mman.h>
#include <pthread.h>
#include <signal.h>     /* P41-6 SIGSEGV handler */
#include <ucontext.h>   /* для siginfo_t.si_addr */

#ifdef NOVA_GC_BOEHM
#include <gc.h>
/* [M-187-docker-linux-runtime-hang] Ф.2: GC_push_all_eager/
 * GC_set_push_other_roots — на system libgc-dev (apt) объявлены в
 * gc/gc_mark.h, НЕ в верхнеуровневом compat-шиме gc.h (проверено:
 * /usr/include/gc/gc_mark.h, Ubuntu libgc-dev 1:8.2.12-1). Path-less —
 * тот же system include path, что уже резолвит <gc.h> (detect_boehm не
 * добавляет -I на Linux). Симметрично Windows-стороне (fiber_arena_win.c
 * подключает оба <gc/gc.h> + <gc/gc_mark.h>). */
#include <gc/gc_mark.h>
#endif

/* ── Per-thread arena state ────────────────────────────────────── */

/* Plan 149 Ф.1: bitmap word count sized by COMPILE-TIME MAX (not the runtime
 * default) so env may RAISE a->slot_count above the default.
 * ceil(NOVA_FIBER_SLOT_COUNT_MAX / 64). 262144 slots = 4096 uint64_t words =
 * 32KB bitmap per arena (× workers — копейки). Runtime a->slot_count iterates
 * only to its own value / high_water, so oversizing is transparent. */
#define NOVA_FIBER_BITMAP_WORDS ((NOVA_FIBER_SLOT_COUNT_MAX + 63) / 64)

/* Plan 82.2: arena struct — heap-allocated (раньше __thread embedded).
 *
 * Структура переживает thread exit — живёт в глобальном append-only
 * списке для cross-thread dealloc routing. Только на retire (thread
 * exit) `base` атомарно зануляется; munmap освобождает виртуальную
 * память, но struct sам не free'ится (другие потоки могут быть в
 * середине list traversal).
 *
 * Field-level concurrency contract:
 *  - base               : atomic store/load. NULL после retire.
 *  - virtual_size       : write-once в init под release-store base'а;
 *                         read-only после init.
 *  - slot_size, slot_count : immutable после init.
 *  - slots_active       : atomic add/sub. Owner increments на alloc,
 *                         любой поток decrements на dealloc. Read для
 *                         MADV gate — atomic load (best-effort).
 *  - high_water         : owner-only write (alloc bumps); plain read OK.
 *  - free_bits[]        : owner OR-set на alloc (RELAXED — single owner,
 *                         no concurrent SETs), любой поток AND-clear
 *                         на dealloc (RELEASE). Read через ACQUIRE-load.
 *  - next_arena         : write-once при list add; read-only после. */
struct NovaFiberArena {
    char*    base;             /* atomic; NULL after retire */
    size_t   virtual_size;
    size_t   slot_size;
    size_t   slot_count;
    size_t   slots_active;     /* atomic add/sub */
    size_t   high_water;       /* owner-only mutation */
    uint64_t free_bits[NOVA_FIBER_BITMAP_WORDS];  /* atomic ops */
    /* Plan 82.2: link в глобальный append-only список арен. */
    struct NovaFiberArena* next_arena;
};

/* TLS: указатель на heap-allocated арену этого потока. NULL до
 * nova_fiber_arena_init; никогда не free'ится после init (struct живёт
 * в global list до конца процесса). */
static __thread struct NovaFiberArena* _t_arena = NULL;

/* Plan 82.2: global append-only arena registry для cross-thread dealloc
 * dispatch. Чтение lock-free через ACQUIRE-load на head + next_arena
 * pointers. Запись (per-thread init) под mutex'ом. */
static struct NovaFiberArena* _nova_arena_list_head = NULL;
static pthread_mutex_t _nova_arena_list_mu = PTHREAD_MUTEX_INITIALIZER;

static pthread_key_t _arena_cleanup_key;
static pthread_once_t _arena_key_once = PTHREAD_ONCE_INIT;

/* Plan 82.2: find arena owning ptr — address-based dispatch. O(N_arenas)
 * linear scan; N <= N_workers + 1 (main), typically 4-16. Lock-free
 * read (append-only list semantics — никто не удаляет ноды). Skips
 * retired arenas (base == NULL после _arena_thread_exit_cleanup). */
static struct NovaFiberArena* _nova_find_arena_for(const char* p) {
    struct NovaFiberArena* a =
        __atomic_load_n(&_nova_arena_list_head, __ATOMIC_ACQUIRE);
    while (a) {
        char* base = __atomic_load_n(&a->base, __ATOMIC_ACQUIRE);
        if (base &&
            p >= base + NOVA_FIBER_GUARD_SIZE &&
            p <  base + a->virtual_size) {
            return a;
        }
        a = a->next_arena;
    }
    return NULL;
}

/* Plan 82.2: append arena в глобальный список. Mutex-guarded на запись;
 * RELEASE-store на head гарантирует readers видят все a->* fields
 * установленными до того, как видят `a` в списке. */
static void _nova_arena_list_add(struct NovaFiberArena* a) {
    pthread_mutex_lock(&_nova_arena_list_mu);
    a->next_arena = _nova_arena_list_head;
    __atomic_store_n(&_nova_arena_list_head, a, __ATOMIC_RELEASE);
    pthread_mutex_unlock(&_nova_arena_list_mu);
}

/* ── Cleanup at thread exit (P41-12) ───────────────────────────── */

static void _arena_thread_exit_cleanup(void* arg) {
    struct NovaFiberArena* a = (struct NovaFiberArena*)arg;
    if (!a || !a->base) return;

    /* [M-187-docker-linux-runtime-hang] Ф.2: раньше здесь был явный
     * GC_remove_roots — рудимент плоской GC_add_roots-регистрации.
     * Теперь GC roots читаются live колбэком _nova_gc_push_other_roots
     * (см. ниже); retire arena'ы он и так пропускает по `base == NULL`
     * (симметрия с Windows _nova_fw_gc_push_other_roots). Явного
     * unregister не нужно — RELEASE-store base=NULL ниже это и есть
     * unregister. */

    munmap(a->base, a->virtual_size);

    /* Plan 82.2: atomic NULL base — marker retired для _nova_find_arena_for.
     * Структура НЕ free'ится — остаётся в глобальном списке (другие
     * потоки могут быть в середине list traversal). Memset selective —
     * НЕ трогать next_arena (link в живой список). */
    __atomic_store_n(&a->base, NULL, __ATOMIC_RELEASE);
    a->virtual_size = 0;
    a->slots_active = 0;
    a->high_water = 0;
    memset(a->free_bits, 0, sizeof(a->free_bits));
    /* slot_size / slot_count / next_arena — оставлены: первые два immutable
     * post-init и больше не читаются (base==NULL), next_arena — link. */
}

static void _arena_register_pthread_key(void) {
    pthread_key_create(&_arena_cleanup_key, _arena_thread_exit_cleanup);
}

/* ── SIGSEGV pretty handler (P41-6, 2026-05-13) ───────────────────
 *
 * Перехватывает SIGSEGV для guard-page hits в нашей arena и печатает
 * понятную диагностику ("Fiber stack overflow in slot N") вместо
 * generic "Segmentation fault".
 *
 * Trade-off: SIGSEGV — process-wide signal, наш handler applies ко
 * всем threads. Для не-arena SIGSEGV (например null deref в user code)
 * мы делегируем обратно default action через sigaction restore.
 *
 * Plan 82.2: cross-thread fiber overflow (work-stolen fiber overflows
 * на worker B, но stack принадлежит worker A's arena) теперь корректно
 * диагностируется — handler ищет owner arena в глобальном списке если
 * TLS arena не содержит fault. */

static struct sigaction _prev_sigsegv;
/* [M-fiber-arena-sigsegv-install-race]: было `static bool _sigsegv_installed`
 * с голым check-then-set в _arena_install_sigsegv_handler — два потока,
 * конкурентно проходящие nova_fiber_arena_init(), могли оба увидеть false
 * и оба вызвать sigaction() (гонка по записи _prev_sigsegv + двойной
 * install), подтверждено TSan. pthread_once — тот же идиом, что уже
 * используется в этом файле для _arena_key_once, и парен с Windows-
 * стороной (fiber_arena_win.c использует INIT_ONCE/InitOnceExecuteOnce
 * для ровно того же process-global one-time install). */
static pthread_once_t _sigsegv_once = PTHREAD_ONCE_INIT;

static void _arena_sigsegv_handler(int sig, siginfo_t* info, void* uctx) {
    void* fault_addr = info ? info->si_addr : NULL;
    struct NovaFiberArena* a = _t_arena;
    bool in_our_range = false;

    /* Plan 82.2: сначала проверяем TLS arena (fast path); если fault
     * не сюда — пытаемся найти owner globally (cross-thread fiber). */
    if (a && a->base && fault_addr &&
        (char*)fault_addr >= a->base &&
        (char*)fault_addr <  a->base + a->virtual_size) {
        in_our_range = true;
    } else if (fault_addr) {
        struct NovaFiberArena* owner =
            _nova_find_arena_for((const char*)fault_addr);
        if (owner) {
            a = owner;
            in_our_range = true;
        }
    }

    if (!in_our_range) {
        /* Не наш диапазон. Делегируем previous handler или default. */
        if (_prev_sigsegv.sa_flags & SA_SIGINFO) {
            if (_prev_sigsegv.sa_sigaction &&
                _prev_sigsegv.sa_sigaction != (void*)SIG_DFL &&
                _prev_sigsegv.sa_sigaction != (void*)SIG_IGN) {
                _prev_sigsegv.sa_sigaction(sig, info, uctx);
                return;
            }
        } else if (_prev_sigsegv.sa_handler &&
                   _prev_sigsegv.sa_handler != SIG_DFL &&
                   _prev_sigsegv.sa_handler != SIG_IGN) {
            _prev_sigsegv.sa_handler(sig);
            return;
        }
        signal(sig, SIG_DFL);
        raise(sig);
        return;
    }

    /* В arena `a`. Какой slot? guard или usable? */
    size_t offset      = (size_t)((char*)fault_addr - a->base);
    size_t slot_idx    = offset / a->slot_size;
    size_t slot_offset = offset % a->slot_size;

    if (slot_offset < NOVA_FIBER_GUARD_SIZE) {
        fprintf(stderr,
                "\nnova: fiber stack overflow in slot %zu "
                "(fault @ %p, guard @ [%p, %p))\n"
                "Hint: increase NOVA_FIBER_STACK (env / nova.toml [runtime].fiber_stack) "
                "or reduce recursion depth.\n",
                slot_idx, fault_addr,
                a->base + slot_idx * a->slot_size,
                a->base + slot_idx * a->slot_size + NOVA_FIBER_GUARD_SIZE);
    } else {
        fprintf(stderr,
                "\nnova: SIGSEGV in fiber arena slot %zu, offset %zu "
                "(fault @ %p)\n"
                "Hint: heap corruption or use-after-free affecting fiber memory.\n",
                slot_idx, slot_offset, fault_addr);
    }
    fflush(stderr);

    signal(sig, SIG_DFL);
    raise(sig);
}

static void _arena_install_sigsegv_handler(void) {
    /* Вызывается ровно один раз за процесс — через pthread_once в
     * nova_fiber_arena_init(). Внутренний check-then-set больше не
     * нужен (и был источником гонки — см. комментарий у
     * _sigsegv_once). */
    struct sigaction sa;
    memset(&sa, 0, sizeof(sa));
    sa.sa_sigaction = _arena_sigsegv_handler;
    sa.sa_flags     = SA_SIGINFO | SA_NODEFER;  /* allow re-entry для re-raise */
    sigemptyset(&sa.sa_mask);
    sigaction(SIGSEGV, &sa, &_prev_sigsegv);
}

/* ── GC integration — precise push_other_roots (root-cause fix) ─────
 *
 * [M-187-docker-linux-runtime-hang] слой 2, Дефект A (docs/plans/wip/
 * boehm-stw-design.md §3/§5): раньше _arena_register_active_range
 * регистрировала статический root GC_add_roots(base, base +
 * high_water*slot_size) — диапазон НАЧИНАЕТСЯ ровно с guard-страницы
 * слота 0 (PROT_NONE) и включает guard каждого следующего слота. Boehm
 * без incremental-mode сканирует статические roots линейным чтением
 * без fault-recovery → первое же чтение guard'а → SIGSEGV внутри
 * mark-фазы (наблюдалось как "нежить" aggregator на первом HTTP-
 * запросе, growing WriteBuffer триггерит GC).
 *
 * Фикс — порт точной Windows-модели (fiber_arena_win.c::
 * _nova_fw_gc_push_other_roots): GC_set_push_other_roots-колбэк,
 * вызываемый Boehm ВНУТРИ mark-фазы (мир уже остановлен) — обходит
 * arena-list и пушит ТОЛЬКО usable-регион (+NOVA_FIBER_GUARD_SIZE)
 * ЗАНЯТЫХ слотов через GC_push_all_eager (не GC_push_all — тот лишь
 * кладёт дескриптор на mark-stack и переполняет его на тысячах fiber'ов,
 * Windows-сторона нашла это же на Ф.1). Guard-страницы и свободные/
 * никогда-не-тронутые слоты не читаются вовсе. */
#ifdef NOVA_GC_BOEHM
/* ── [M-mn-spawnctx-corruption-cancel-wake] КОРЕНЬ (2026-07-19) ──
 *
 * На pthreads-сборке bdwgc дефолтный `GC_push_other_roots` =
 * `GC_default_push_other_roots` → `GC_push_all_stacks()` — канал, которым
 * сканируются СТЕКИ И РЕГИСТРЫ всех зарегистрированных потоков. Установка
 * нашего колбэка (ea85229e0) ЗАМЕНЯЛА его, а Linux-порт Windows-модели
 * перенёс из трёх слагаемых Windows-колбэка ТОЛЬКО занятые fiber-слоты,
 * потеряв два других (fiber_arena_win.c пушит их с Plan 151/Ф.2!):
 * native-стеки потоков-владельцев и стек главного потока. Итог: всё,
 * что рутовано только native-стеком (stack-локальный supervised-scope `q`
 * и его child_error[]/child_ctx[] на стеке main, локали воркерского
 * шедулера, cancel-токен и т.д.), собиралось ЖИВЫМ; страницы
 * перекраивались под другие объекты — обе gdb-сигнатуры (усечённый
 * free-list-линк 128Б uncollectable-класса — легитимные int32-записи
 * рантайма в СВОЙ ЖЕ преждевременно отобранный массив; мусорный
 * `_nova_fiber_scope`/ASCII в SpawnCtx к моменту wake) и рваный
 * `_nova_saved_fail_top` («cancel-throw outside any supervised scope»).
 *
 * Доказательная матрица (изолированный pos_max_fibers_concurrent, WSL):
 * базлайн 0/30 PASS; PIN2-бисекция (дубль-достижимость live-массивов
 * через uncollectable-цепь) 10/10 PASS; mmap-вынос массивов из GC-кучи
 * 10/10 PASS; poison/карантин ручных free — эффекта нет (порча не от
 * manual-free). Наивный чейнинг дефолтного колбэка НЕ решение: суспенд
 * ловит воркеров с sp ВНУТРИ коро-стека арены → GC_push_all_stacks
 * строит диапазон [коро-sp, native-bottom] через PROT_NONE guard'ы →
 * SIGSEGV в GC-маркере (наблюдён напрямую). Правильная модель — как на
 * Windows: оверрайд + ПОЛНАЯ компенсация тремя слагаемыми ниже. */

/* (а) main-стек: probe-адрес, зафиксированный nova_fiber_arena_set_main_
 * stack (вызывается из _materialize_pool на main ДО установки колбэка).
 * pthread_getattr_np для main даёт rlimit-диапазон, НЕ полностью
 * замапленный — пушить его нельзя; вместо этого на каждой сборке
 * читаем /proc/self/maps и пушим ТЕКУЩУЮ VMA main-стека (растёт вниз
 * автоматически; мир остановлен — диапазон стабилен). */
static char* volatile _nova_main_stack_probe = NULL;

/* (б) native-стеки потоков рантайма (воркеры, драйвер, sysmon): NPTL
 * маппит стек потока целиком → пуш полного диапазона безопасен (guard
 * NPTL исключён самим pthread_getattr_np). Append-only список. */
struct NovaNativeStackRange {
    char* volatile lo;
    char* volatile hi;
    struct NovaNativeStackRange* next;
};
static struct NovaNativeStackRange* volatile _nova_native_stacks = NULL;

void nova_fiber_arena_register_native_stack(void) {
    pthread_attr_t attr;
    if (pthread_getattr_np(pthread_self(), &attr) != 0) return;
    void* addr = NULL; size_t size = 0;
    int rc = pthread_attr_getstack(&attr, &addr, &size);
    pthread_attr_destroy(&attr);
    if (rc != 0 || !addr || !size) return;
    struct NovaNativeStackRange* nd =
        (struct NovaNativeStackRange*)malloc(sizeof *nd);
    if (!nd) return;
    nd->lo = (char*)addr;
    nd->hi = (char*)addr + size;
    struct NovaNativeStackRange* h;
    do {
        h = __atomic_load_n(&_nova_native_stacks, __ATOMIC_ACQUIRE);
        nd->next = h;
    } while (!__atomic_compare_exchange_n(&_nova_native_stacks, &h, nd,
                                          false, __ATOMIC_RELEASE, __ATOMIC_ACQUIRE));
}

/* Поток выходит: снять диапазон (glibc может отдать кэшированный стек
 * другому потоку — лишний conservative-пуш безвреден, но unmap кэша при
 * переполнении сделал бы пуш опасным; гигиена — обнулить). Ищем узел,
 * содержащий адрес локали текущего потока. */
void nova_fiber_arena_unregister_native_stack(void) {
    char probe_local;
    char* p = &probe_local;
    for (struct NovaNativeStackRange* nd =
             __atomic_load_n(&_nova_native_stacks, __ATOMIC_ACQUIRE);
         nd; nd = nd->next) {
        char* lo = __atomic_load_n(&nd->lo, __ATOMIC_ACQUIRE);
        char* hi = __atomic_load_n(&nd->hi, __ATOMIC_ACQUIRE);
        if (lo && p >= lo && p < hi) {
            __atomic_store_n(&nd->lo, (char*)NULL, __ATOMIC_RELEASE);
            __atomic_store_n(&nd->hi, (char*)NULL, __ATOMIC_RELEASE);
            return;
        }
    }
}

/* [M-boehm-large-buffer-retention-fiber-reuse] DISCRIMINATOR / candidate fix:
 * env NOVA_GC_STACK_SCAN_KB=N (>0) tightens every WHOLE-stack conservative
 * push below (main VMA, native thread stacks, occupied fiber slots) to only
 * the top N KiB near each stack's hot end (base) — an over-approximation of
 * the LIVE window [sp, base) — instead of the full mapped region [lo, base).
 * The dead region [lo, sp) holds stale KB-buffer addresses left by returned
 * deep frames; with GC_set_all_interior_pointers(1) any such stale word
 * retains the whole buffer (retention ∝ buffer size) — the residual that a
 * full gc.collect() does not reclaim. Default 0 = whole-region (current
 * behavior, zero overhead — env read once, cached). This is a DIAGNOSTIC knob
 * for the discriminating experiment: it is only sound while every live stack
 * is shallower than N (else genuine roots would be missed). A principled fix
 * must source each thread's TRUE live sp (see boehmret-design.md). */
static size_t _nova_gc_stack_scan_bytes(void) {
    static long _cached = -2;  /* -2 = unread; 0 = disabled; >0 = bytes */
    long v = __atomic_load_n(&_cached, __ATOMIC_RELAXED);
    if (v == -2) {
        const char* e = getenv("NOVA_GC_STACK_SCAN_KB");
        long kb = (e && e[0]) ? strtol(e, NULL, 10) : 0;
        v = (kb > 0) ? kb * 1024 : 0;
        __atomic_store_n(&_cached, v, __ATOMIC_RELAXED);
    }
    return (size_t)v;
}

/* Clamp [*lo, hi) to the top `scan` bytes near hi (hot end / stack base).
 * Stack grows DOWN → the live window is the top. No-op when the knob is 0. */
static inline void _nova_gc_clamp_top(char** lo, char* hi) {
    size_t scan = _nova_gc_stack_scan_bytes();
    if (scan && *lo && hi > *lo && (size_t)(hi - *lo) > scan) *lo = hi - scan;
}

/* (а) main: найти в /proc/self/maps VMA, содержащую probe, и запушить её
 * целиком. Стриминговый разбор без аллокаций (maps может быть огромным —
 * guard-страницы арены плодят десятки тысяч VMA; [stack] — в конце). */
static void _nova_push_main_stack_vma(void) {
    char* probe = (char*)__atomic_load_n(&_nova_main_stack_probe, __ATOMIC_ACQUIRE);
    if (!probe) return;
    int fd = open("/proc/self/maps", O_RDONLY);
    if (fd < 0) return;
    static char buf[8192 + 256];
    size_t carry = 0;
    uintptr_t p = (uintptr_t)probe;
    for (;;) {
        ssize_t n = read(fd, buf + carry, sizeof(buf) - carry - 1);
        if (n <= 0) break;
        size_t len = carry + (size_t)n;
        buf[len] = 0;
        char* line = buf;
        for (;;) {
            char* nl = strchr(line, '\n');
            if (!nl) {
                carry = len - (size_t)(line - buf);
                if (carry >= sizeof(buf) - 256) carry = 0;  /* линия-монстр */
                else memmove(buf, line, carry);
                break;
            }
            *nl = 0;
            uintptr_t lo = (uintptr_t)strtoull(line, NULL, 16);
            char* dash = strchr(line, '-');
            uintptr_t hi = dash ? (uintptr_t)strtoull(dash + 1, NULL, 16) : 0;
            if (lo <= p && p < hi) {
                char* clo = (char*)lo;
                _nova_gc_clamp_top(&clo, (char*)hi);   /* NOVA_GC_STACK_SCAN_KB */
                GC_push_all(clo, (char*)hi);
                close(fd);
                return;
            }
            line = nl + 1;
        }
    }
    close(fd);
}

/* Mark-фаза, мир остановлен → arena-list append-only + bitmap/high_water
 * стабильны; обход без лока безопасен (симметрия с Windows). */
static void _nova_gc_push_other_roots(void) {
    /* (а) main-стек (текущая VMA). */
    _nova_push_main_stack_vma();
    /* (б) native-стеки потоков рантайма. */
    for (struct NovaNativeStackRange* nd =
             __atomic_load_n(&_nova_native_stacks, __ATOMIC_ACQUIRE);
         nd; nd = nd->next) {
        char* lo = __atomic_load_n(&nd->lo, __ATOMIC_ACQUIRE);
        char* hi = __atomic_load_n(&nd->hi, __ATOMIC_ACQUIRE);
        if (lo && hi > lo) {
            _nova_gc_clamp_top(&lo, hi);   /* NOVA_GC_STACK_SCAN_KB */
            GC_push_all(lo, hi);
        }
    }
    /* (в) занятые fiber-слоты арен. */
    for (struct NovaFiberArena* a =
             __atomic_load_n(&_nova_arena_list_head, __ATOMIC_ACQUIRE);
         a; a = a->next_arena) {
        char* base = __atomic_load_n(&a->base, __ATOMIC_ACQUIRE);
        if (!base) continue;                     /* retired */
        size_t hw = a->high_water;               /* мир остановлен — стабильно */
        for (size_t slot = 0; slot < hw; slot++) {
            uint64_t w = __atomic_load_n(&a->free_bits[slot >> 6], __ATOMIC_ACQUIRE);
            if (!((w >> (slot & 63)) & 1)) continue;   /* слот свободен */
            char* usable_lo = base + slot * a->slot_size + NOVA_FIBER_GUARD_SIZE;
            char* usable_hi = base + (slot + 1) * a->slot_size;
            _nova_gc_clamp_top(&usable_lo, usable_hi);   /* NOVA_GC_STACK_SCAN_KB */
            GC_push_all_eager(usable_lo, usable_hi);      /* guard исключён */
        }
    }
}

static pthread_once_t _gc_roots_once = PTHREAD_ONCE_INIT;

static void _arena_install_gc_roots(void) {
    GC_set_push_other_roots(_nova_gc_push_other_roots);
}
#endif /* NOVA_GC_BOEHM */

/* ── Plan 149 Ф.1/Ф.4: config parse + round/clamp helpers ───────────
 *
 * File-local statics (mirror runtime.c::_nova_parse_maxprocs_env idiom).
 * Duplicated (not shared TU) with fiber_arena_win.c — the two files are
 * independent per-OS TUs; helpers kept file-local like the other _nova_*
 * statics. */

/* Parse a human-friendly size/count env var. Returns parsed value on success,
 * or 0 to signal "unset or invalid → use builtin default" (caller handles).
 * `is_invalid` distinguishes garbage (warn) from absent/empty (silent).
 * Accepts: bare integer (bytes/count), optional KB/K/MB/M/GB/G suffix
 * (case-insensitive, binary: KB=1024, MB=1024², GB=1024³). Trailing
 * whitespace tolerated. value<=0 / leftover garbage / errno → invalid. */
static size_t _nova_parse_size_env(const char* name, int* is_invalid) {
    *is_invalid = 0;
    const char* env = getenv(name);
    if (!env || env[0] == '\0') return 0;  /* unset → default chain (not invalid) */

    errno = 0;
    char* end = NULL;
    long long raw = strtoll(env, &end, 10);
    if (end == env) { *is_invalid = 1; return 0; }     /* no digits */
    /* skip whitespace before optional suffix */
    while (*end == ' ' || *end == '\t' || *end == '\r' || *end == '\n') end++;
    unsigned long long mult = 1ULL;
    if (*end != '\0') {
        char c0 = *end, c1 = end[1];
        char u0 = (c0 >= 'a' && c0 <= 'z') ? (char)(c0 - 32) : c0;
        char u1 = (c1 >= 'a' && c1 <= 'z') ? (char)(c1 - 32) : c1;
        if (u0 == 'K') { mult = 1024ULL; end += (u1 == 'B') ? 2 : 1; }
        else if (u0 == 'M') { mult = 1024ULL * 1024ULL; end += (u1 == 'B') ? 2 : 1; }
        else if (u0 == 'G') { mult = 1024ULL * 1024ULL * 1024ULL; end += (u1 == 'B') ? 2 : 1; }
        else { *is_invalid = 1; return 0; }            /* unknown suffix */
        while (*end == ' ' || *end == '\t' || *end == '\r' || *end == '\n') end++;
    }
    if (*end != '\0') { *is_invalid = 1; return 0; }   /* trailing garbage */
    if (errno != 0 || raw <= 0) { *is_invalid = 1; return 0; }

    /* unsigned accumulation + overflow guard on suffix-multiply (saturate). */
    unsigned long long uval = (unsigned long long)raw;
    if (mult > 1ULL && uval > (unsigned long long)SIZE_MAX / mult) {
        return (size_t)SIZE_MAX;  /* saturate; clamp step will cap to MAX */
    }
    unsigned long long total = uval * mult;
    if (total > (unsigned long long)SIZE_MAX) return (size_t)SIZE_MAX;
    return (size_t)total;
}

/* Page size (POSIX). sysconf(_SC_PAGESIZE), fallback 4096. */
static size_t _nova_page_size(void) {
    long pg = sysconf(_SC_PAGESIZE);
    return (pg > 0) ? (size_t)pg : 4096;
}

/* Round UP to page alignment then clamp to [MIN, MAX]; warn on clamp. */
static size_t _nova_round_clamp_stack(size_t v) {
    size_t pg = _nova_page_size();
    /* round up (overflow-safe: v already ≤ SIZE_MAX; pg small) */
    if (v > SIZE_MAX - (pg - 1)) v = SIZE_MAX;
    else v = (v + pg - 1) / pg * pg;
    if (v < (size_t)NOVA_FIBER_STACK_MIN) {
        fprintf(stderr, "nova: NOVA_FIBER_STACK %zu below floor — using 256KB\n", v);
        return (size_t)NOVA_FIBER_STACK_MIN;
    }
    if (v > (size_t)NOVA_FIBER_STACK_MAX) {
        fprintf(stderr, "nova: NOVA_FIBER_STACK %zu exceeds max %zu — clamped to 256MB\n",
                v, (size_t)NOVA_FIBER_STACK_MAX);
        return (size_t)NOVA_FIBER_STACK_MAX;
    }
    return v;
}

/* Round UP to a multiple of 64 then clamp to [MIN, MAX]; warn on clamp. */
static size_t _nova_round_clamp_slots(size_t v) {
    /* round up to ×64 (overflow-safe) */
    if (v > SIZE_MAX - 63) v = SIZE_MAX;
    else v = (v + 63) / 64 * 64;
    if (v < (size_t)NOVA_FIBER_SLOT_COUNT_MIN) {
        return (size_t)NOVA_FIBER_SLOT_COUNT_MIN;  /* 64 floor — no warn needed */
    }
    if (v > (size_t)NOVA_FIBER_SLOT_COUNT_MAX) {
        fprintf(stderr, "nova: NOVA_MAX_FIBERS %zu exceeds max %zu — clamped\n",
                v, (size_t)NOVA_FIBER_SLOT_COUNT_MAX);
        return (size_t)NOVA_FIBER_SLOT_COUNT_MAX;  /* MAX is a multiple of 64 */
    }
    return v;
}

/* Resolve final slot_size: env(NOVA_FIBER_STACK) ∨ -D/builtin DEFAULT, then
 * round+clamp. Garbage env → warn + default (which also goes through clamp). */
static size_t _nova_resolve_slot_size(void) {
    int invalid = 0;
    size_t parsed = _nova_parse_size_env("NOVA_FIBER_STACK", &invalid);
    if (invalid) {
        fprintf(stderr, "nova: invalid NOVA_FIBER_STACK \"%s\" — using default 4MB\n",
                getenv("NOVA_FIBER_STACK"));
        parsed = 0;
    }
    size_t v = parsed ? parsed : (size_t)NOVA_FIBER_STACK_DEFAULT;
    return _nova_round_clamp_stack(v);
}

/* Resolve final slot_count: env(NOVA_MAX_FIBERS) ∨ -D/builtin DEFAULT, then
 * round+clamp. */
static size_t _nova_resolve_slot_count(void) {
    int invalid = 0;
    size_t parsed = _nova_parse_size_env("NOVA_MAX_FIBERS", &invalid);
    if (invalid) {
        fprintf(stderr, "nova: invalid NOVA_MAX_FIBERS \"%s\" — using default 16384\n",
                getenv("NOVA_MAX_FIBERS"));
        parsed = 0;
    }
    size_t v = parsed ? parsed : (size_t)NOVA_MAX_FIBERS_DEFAULT;
    return _nova_round_clamp_slots(v);
}

/* Plan 149 Ф.1: mark trailing bitmap bits [from, BITMAP_WORDS*64) USED so the
 * allocator never returns a phantom slot beyond slot_count. free_bits
 * semantics: bit SET = USED (find_free uses ~word). Plain stores — called
 * under single-owner init before the arena is published. With round-UP-to-×64
 * `from` is a multiple of 64 so only whole trailing words get set; the partial
 * boundary loop is belt-and-suspenders for a defensive non-×64 default. */
static void _nova_mark_tail_used(struct NovaFiberArena* a, size_t from) {
    size_t cap = (size_t)NOVA_FIBER_BITMAP_WORDS * 64;
    for (size_t i = from; i < cap; i++) {
        a->free_bits[i / 64] |= (1ULL << (i % 64));
    }
}

/* ── [RETIRED] Plan [M-187-docker-linux-runtime-hang] Ф.1: vm.max_map_count
 * clamp — Plan 259 Слой 1 (2026-08-09) ─────────────────────────────
 *
 * This clamp (NOVA_ARENA_VMA_RESERVE/NOVA_ARENA_VMA_SAFETY_PCT + the
 * /proc/sys/vm/max_map_count pre-budget helpers that used to live here)
 * treated the SYMPTOM of eager per-slot mprotect() at arena init — it
 * shrank slot_count so the eager guard-page loop wouldn't blow past the
 * kernel's VMA limit. Plan 259 Слой 1 removes the CAUSE instead: guard
 * pages are now punched lazily, one mprotect() per slot the first time
 * (and only the first time — see nova_fiber_alloc below) that slot
 * index is ever handed out, not all NOVA_MAX_FIBERS of them upfront.
 * VMA count now tracks live fiber count, not the configured ceiling, so
 * the clamp has nothing left to protect against. See D97 Ред.3 / D451
 * (spec/decisions/06-concurrency.md) for the full rationale and the
 * №457 timeout-hang this closes. */

/* ── Init ──────────────────────────────────────────────────────── */

void nova_fiber_arena_init(void) {
    /* Already initialized? (idempotent — safe to call multiple times.) */
    if (_t_arena && _t_arena->base) return;

    pthread_once(&_arena_key_once, _arena_register_pthread_key);
    /* P41-6: pretty stack overflow diagnostic. Ровно один раз для
     * процесса (не per-thread) — pthread_once, а не бинарный флаг
     * (см. [M-fiber-arena-sigsegv-install-race]). */
    pthread_once(&_sigsegv_once, _arena_install_sigsegv_handler);
#ifdef NOVA_GC_BOEHM
    /* [M-187-docker-linux-runtime-hang] Ф.2: точный push_other_roots
     * колбэк вместо плоского GC_add_roots — см. design doc §5. Once-
     * per-process, до первого alloc'а слота (симметрия с Windows
     * _nova_fw_global_init/INIT_ONCE). */
    pthread_once(&_gc_roots_once, _arena_install_gc_roots);
    /* [M-mn-spawnctx-corruption-cancel-wake]: bootstrap-страховка — в
     * не-armed режиме _materialize_pool (и его set_main_stack) не
     * вызывается вовсе, а оверрайд колбэка ставится ЗДЕСЬ, на первом
     * mco_create главного потока. Без probe main-стек выпал бы из скана.
     * Ставим probe сами, если мы — главный поток процесса. */
    if (!__atomic_load_n(&_nova_main_stack_probe, __ATOMIC_ACQUIRE)
        && getpid() == (pid_t)syscall(SYS_gettid)) {
        nova_fiber_arena_set_main_stack();
    }
#endif

    /* Plan 149 Ф.1: resolve runtime config (env ∨ -D/toml ∨ builtin) with
     * auto-round-UP + clamp. Garbage env → warn + default (helpers). */
    size_t slot_size  = _nova_resolve_slot_size();
    size_t slot_count = _nova_resolve_slot_count();

    /* Defensive: clamp guarantees slot_count ≤ MAX, but never abort on config
     * — cap to MAX instead of the old abort(). */
    if (slot_count > (size_t)NOVA_FIBER_BITMAP_WORDS * 64) {
        slot_count = (size_t)NOVA_FIBER_BITMAP_WORDS * 64;
    }

    size_t virtual_size = slot_size * slot_count;

    int prot = PROT_READ | PROT_WRITE;
    int flags = MAP_PRIVATE | MAP_ANONYMOUS;
#ifdef MAP_NORESERVE
    flags |= MAP_NORESERVE;
#endif

    void* p = mmap(NULL, virtual_size, prot, flags, -1, 0);
    if (p == MAP_FAILED) {
        fprintf(stderr, "nova: fiber_arena mmap failed (%zu bytes)\n",
                virtual_size);
        abort();
    }

    /* Plan 44.2 P41-14: disable Transparent Huge Pages для arena. */
#if defined(MADV_NOHUGEPAGE)
    madvise(p, virtual_size, MADV_NOHUGEPAGE);
#endif

    /* Plan 259 Слой 1 (2026-08-09, №457 / D97 Ред.3): guard pages are
     * NOT punched here anymore. The old code called mprotect(PROT_NONE)
     * PER SLOT for all `slot_count` slots (16384 by default) right here
     * — a typical program uses a handful of fibers, so that eager loop
     * paid for slots that would never be touched. Worse, each mprotect()
     * SPLITS the kernel's single mmap() VMA into guard+usable pieces AND
     * serializes on the process-wide mmap_lock (write side): 16 workers
     * × 16384 slots = 262144 calls was a syscall storm that alone blew
     * past a `supervised(timeout:)` budget (№457) and separately hit
     * /proc/sys/vm/max_map_count in Docker
     * ([M-187-docker-linux-runtime-hang], the NOVA_ARENA_VMA_* clamp
     * this retires).
     *
     * The single `mmap` above is exactly ONE VMA regardless of
     * slot_count — O(1) syscalls for the whole arena. Each slot's guard
     * page is punched lazily, exactly once, the first time that slot
     * index is ever handed out by nova_fiber_alloc (the
     * `slot >= a->high_water` branch below); a REUSED slot already has
     * its guard from the first time, so reuse costs zero extra
     * mprotect() calls. VMA count now tracks the number of fibers that
     * actually LIVED, not NOVA_MAX_FIBERS. See D97 Ред.3 / D451
     * (spec/decisions/06-concurrency.md) for the full writeup. */

    /* Plan 82.2: heap-allocate arena struct. calloc zero-инициализирует;
     * никогда не free'ится — живёт в global list до конца процесса. */
    struct NovaFiberArena* a =
        (struct NovaFiberArena*)calloc(1, sizeof(struct NovaFiberArena));
    if (!a) {
        fprintf(stderr, "nova: fiber_arena state alloc failed\n");
        abort();
    }
    a->virtual_size = virtual_size;
    a->slot_size = slot_size;
    a->slot_count = slot_count;
    a->slots_active = 0;
    a->high_water = 0;
    /* free_bits, next_arena уже zero'd calloc'ом. */

    /* Plan 149 Ф.1: mark trailing bitmap bits [slot_count, capacity) USED so
     * the allocator never hands out a phantom slot beyond slot_count. Plain
     * stores — single-owner init, arena not yet published. */
    _nova_mark_tail_used(a, slot_count);

    /* base устанавливается RELEASE-store последним: до этого момента
     * _nova_find_arena_for видит arena с base==NULL → skip. После store —
     * ACQUIRE-readers видят all остальные fields. */
    __atomic_store_n(&a->base, (char*)p, __ATOMIC_RELEASE);

    /* Append в глобальный список ПОСЛЕ полной инициализации. */
    _nova_arena_list_add(a);

    _t_arena = a;

    /* Plan 44.2 P41-11 (обновлено [M-187-docker-linux-runtime-hang] Ф.2):
     * НЕ регистрируем статический GC root вообще — push_other_roots-
     * колбэк (_nova_gc_push_other_roots, зарегистрирован выше) читает
     * a->high_water/a->free_bits LIVE во время mark-фазы, никакой
     * явной регистрации на bump'е high_water не требуется. */

    /* Plan 82.2: pthread_setspecific принимает heap pointer (не &_t_arena).
     * Cleanup-callback получит указатель на heap struct — корректно
     * munmap + NULL base + сохранение next_arena в list. */
    pthread_setspecific(_arena_cleanup_key, a);
}

/* ── Bitmap allocate / free ─────────────────────────────────────── */

/* Find first free slot (bit 0 in free_bits). Returns slot index or
 * SIZE_MAX if none.
 *
 * Plan 82.2: ACQUIRE-load на каждое слово — гарантирует видимость
 * cross-thread released slots до того, как owner ищет free slot. */
static size_t _arena_find_free_slot(struct NovaFiberArena* a) {
    size_t total_words = (a->slot_count + 63) / 64;
    for (size_t w = 0; w < total_words; w++) {
        uint64_t word = __atomic_load_n(&a->free_bits[w], __ATOMIC_ACQUIRE);
        uint64_t inv = ~word;
        if (inv == 0) continue;  /* word fully used */
        size_t bit = (size_t)__builtin_ctzll(inv);
        size_t slot = w * 64 + bit;
        if (slot >= a->slot_count) continue;  /* past end */
        return slot;
    }
    return SIZE_MAX;
}

/* Plan 82.2: atomic OR — owner-only path (alloc на owning thread).
 * Никто другой не делает SET одновременно (cross-thread только AND-clears
 * другие слоты). RELAXED достаточно — happens-before гарантируется
 * single-owner store-order. */
static void _arena_mark_slot_used(struct NovaFiberArena* a, size_t slot) {
    size_t w = slot / 64;
    size_t b = slot % 64;
    __atomic_fetch_or(&a->free_bits[w], (1ULL << b), __ATOMIC_RELAXED);
}

/* Plan 82.2: atomic AND — cross-thread safe (owner thread И любой
 * worker, выполнивший mco_destroy для work-stolen fiber'а).
 * RELEASE — clear visible перед slots_active decrement. */
static void _arena_mark_slot_free(struct NovaFiberArena* a, size_t slot) {
    size_t w = slot / 64;
    size_t b = slot % 64;
    __atomic_fetch_and(&a->free_bits[w], ~(1ULL << b), __ATOMIC_RELEASE);
}

/* ── minicoro alloc callbacks ──────────────────────────────────── */

void* nova_fiber_alloc(size_t size, void* allocator_data) {
    (void)allocator_data;
    if (!_t_arena || !_t_arena->base) {
        nova_fiber_arena_init();
    }
    struct NovaFiberArena* a = _t_arena;

    /* Caller (minicoro) запросит конкретный size; мы ignore — slot_size
     * фиксирован. Verify что requested ≤ usable region (slot - guard). */
    size_t usable = a->slot_size - NOVA_FIBER_GUARD_SIZE;
    if (size > usable) {
        fprintf(stderr, "nova: fiber_alloc requested %zu > usable %zu (slot %zu - guard %d)\n"
                "Hint: increase NOVA_FIBER_STACK (env / nova.toml [runtime].fiber_stack).\n",
                size, usable, a->slot_size, NOVA_FIBER_GUARD_SIZE);
        return NULL;  /* minicoro will handle as failure */
    }

    size_t slot = _arena_find_free_slot(a);
    if (slot == SIZE_MAX) {
        fprintf(stderr, "nova: fiber_arena exhausted (%zu slots used)\n",
                __atomic_load_n(&a->slots_active, __ATOMIC_RELAXED));
        abort();
    }

    /* Plan 259 Слой 1 (2026-08-09, №457 / D97 Ред.3): punch this slot's
     * guard page lazily, on its FIRST-EVER use. `_arena_find_free_slot`
     * always returns the lowest free index, so slot indices are handed
     * out in strictly increasing order the first time each is used —
     * `slot >= a->high_water` is true iff no fiber has EVER occupied
     * this index before (a slot `< high_water` was guarded the first
     * time IT was the high-water slot; freeing and reusing it below
     * high_water does NOT re-enter this branch, so a reused slot costs
     * zero extra mprotect() calls — the guard from its first use still
     * stands, `nova_fiber_dealloc`'s MADV_DONTNEED never touches the
     * guard range). This is what makes VMA count track live fiber
     * count instead of NOVA_MAX_FIBERS — see the big comment in
     * nova_fiber_arena_init above and D451. */
    if (slot >= a->high_water) {
        char* slot_base = a->base + slot * a->slot_size;
        if (mprotect(slot_base, NOVA_FIBER_GUARD_SIZE, PROT_NONE) != 0) {
            fprintf(stderr,
                "nova: fiber_arena guard page mprotect failed for slot %zu "
                "(errno=%d) — %zu fiber slots already live in this arena; "
                "the OS is out of VMA/mmap capacity for this process. Raise "
                "vm.max_map_count (sysctl -w vm.max_map_count=1048576) or "
                "reduce concurrent fiber count.\n",
                slot, errno, a->high_water);
            abort();
        }
    }

    _arena_mark_slot_used(a, slot);
    __atomic_add_fetch(&a->slots_active, 1, __ATOMIC_RELAXED);
    if (slot + 1 > a->high_water) {
        a->high_water = slot + 1;
        /* [M-187-docker-linux-runtime-hang] Ф.2: раньше здесь звали
         * _arena_register_active_range (GC_add_roots bump) — снято,
         * push_other_roots-колбэк читает high_water live (см. выше). */
    }

    /* Usable region: slot_base + guard_size .. slot_base + slot_size.
     * Stack starts at slot_top (grows down). minicoro caller treats
     * returned pointer as base of stack region. */
    return a->base + slot * a->slot_size + NOVA_FIBER_GUARD_SIZE;
}

/* Plan 82.2: address-based dealloc — fast path TLS arena, slow path
 * global lookup. Под M:N work-stealing fiber может быть allocated на
 * thread A (mco_create в nova_runtime_spawn_global on calling thread),
 * deallocated на worker B (mco_destroy в worker B'е после fiber dies).
 *
 * Раньше: только TLS check → cross-thread ptr вне range → warning +
 * skip → slot leak в A's arena (bitmap bit never cleared).
 *
 * Теперь: fast path = TLS bounds match (typical same-thread case);
 * slow path = _nova_find_arena_for(p) — address-based owner lookup в
 * глобальном списке арен. Atomic bitmap clear работает корректно
 * cross-thread. */
void nova_fiber_dealloc(void* ptr, size_t size, void* allocator_data) {
    (void)size; (void)allocator_data;
    if (!ptr) return;

    char* p = (char*)ptr;
    struct NovaFiberArena* a = _t_arena;

    /* Fast path: ptr в текущей TLS arena (typical case без миграции). */
    if (a && a->base &&
        p >= a->base + NOVA_FIBER_GUARD_SIZE &&
        p <  a->base + a->virtual_size) {
        /* in current arena — fall through */
    } else {
        /* Slow path: cross-thread dealloc — найти owner по адресу. */
        a = _nova_find_arena_for(p);
        if (!a) {
            fprintf(stderr, "nova: fiber_dealloc ptr outside all arenas (%p)\n", ptr);
            return;
        }
    }

    /* Reverse usable_ptr → slot index using owning arena's layout. */
    size_t offset = (size_t)(p - a->base - NOVA_FIBER_GUARD_SIZE);
    size_t slot = offset / a->slot_size;
    if (slot >= a->slot_count) {
        fprintf(stderr, "nova: fiber_dealloc slot index out of range\n");
        return;
    }

    _arena_mark_slot_free(a, slot);
    __atomic_sub_fetch(&a->slots_active, 1, __ATOMIC_RELAXED);

    /* Plan 44.2 P41-3 (R8, 2026-05-13): MADV_DONTNEED только на idle.
     *
     * Раньше: per-dealloc madvise → каждый syscall takes mmap_sem write
     * lock → serialize все VM ops в процессе. Под 100k fiber/sec churn —
     * deadlock-grade.
     *
     * Теперь: при `slots_active == 0` (idle = весь scope завершился)
     * выполняем ОДИН madvise на весь used range [base+guard, high_water*slot].
     *
     * Plan 82.2: MADV_DONTNEED только когда dealloc на own thread
     * (a == _t_arena). Cross-thread dealloc skip'ает MADV — owning thread
     * сам сделает на следующем idle (free_bits cleared cross-thread'ом
     * виден ACQUIRE-load'у в _arena_find_free_slot). */
    if (a == _t_arena &&
        __atomic_load_n(&a->slots_active, __ATOMIC_ACQUIRE) == 0 &&
        a->high_water > 0) {
#ifdef MADV_DONTNEED
        char* range_base = a->base + NOVA_FIBER_GUARD_SIZE;
        size_t range_size = a->high_water * a->slot_size
                          - NOVA_FIBER_GUARD_SIZE;
        madvise(range_base, range_size, MADV_DONTNEED);
#endif
    }
}

/* Plan 44.2 P41-3 (2026-05-13): explicit compact API для long-running
 * workloads без natural idle. Released все free slots' physical pages
 * одним syscall. Exposed через std.runtime.fibers.compact(). */
void nova_fiber_arena_compact(void) {
    if (!_t_arena || !_t_arena->base || _t_arena->high_water == 0) return;
    struct NovaFiberArena* a = _t_arena;
#ifdef MADV_DONTNEED
    /* Iterate bitmap, find contiguous free runs, batch MADV. */
    size_t total_words = (a->slot_count + 63) / 64;
    size_t run_start = SIZE_MAX;  /* sentinel — no run in progress */
    for (size_t w = 0; w < total_words; w++) {
        uint64_t bits = __atomic_load_n(&a->free_bits[w], __ATOMIC_ACQUIRE);
        for (size_t b = 0; b < 64; b++) {
            size_t slot = w * 64 + b;
            if (slot >= a->high_water) goto end_scan;
            bool used = (bits >> b) & 1;
            if (!used) {
                if (run_start == SIZE_MAX) run_start = slot;
            } else {
                if (run_start != SIZE_MAX) {
                    /* Flush run [run_start, slot). */
                    char* rbase = a->base + run_start * a->slot_size
                                + NOVA_FIBER_GUARD_SIZE;
                    size_t rsize = (slot - run_start) * a->slot_size
                                 - NOVA_FIBER_GUARD_SIZE;
                    madvise(rbase, rsize, MADV_DONTNEED);
                    run_start = SIZE_MAX;
                }
            }
        }
    }
end_scan:
    if (run_start != SIZE_MAX) {
        char* rbase = a->base + run_start * a->slot_size
                    + NOVA_FIBER_GUARD_SIZE;
        size_t rsize = (a->high_water - run_start) * a->slot_size
                     - NOVA_FIBER_GUARD_SIZE;
        madvise(rbase, rsize, MADV_DONTNEED);
    }
#endif
}

bool nova_fiber_arena_contains(const void* ptr) {
    if (!_t_arena || !_t_arena->base) return false;
    return (const char*)ptr >= _t_arena->base &&
           (const char*)ptr <  _t_arena->base + _t_arena->virtual_size;
}

/* Plan 149 Ф.1 (review must_fix #1/#2): runtime per-fiber slot size.
 * Lazily inits the arena so the minicoro desc-init can derive stack_size from
 * the resolved (env ∨ -D ∨ builtin, round+clamp) slot_size. */
size_t nova_fiber_arena_slot_size(void) {
    if (!_t_arena || !_t_arena->base) {
        nova_fiber_arena_init();
    }
    return _t_arena ? _t_arena->slot_size : (size_t)NOVA_FIBER_STACK_DEFAULT;
}

/* Plan 82 Ф.1: POSIX не нуждается в патче ctx.stack_limit — mmap
 * MAP_NORESERVE даёт kernel demand-paging без __chkstk-проблемы. NULL
 * → nova_fiber_post_create (fibers.c) пропускает патч. */
void* nova_fiber_committed_low(const void* block_ptr) {
    (void)block_ptr;
    return NULL;
}

/* Plan 82 Ф.3 — M:N lifecycle. POSIX-арена живёт в TLS pointer + heap
 * struct в глобальном списке; cleanup идёт через pthread_key при выходе
 * потока (Plan 82.2: munmap + NULL base + сохранение struct в list для
 * cross-thread dealloc traversal). Явные thread_exit / release_retired
 * не нужны — no-op. */
void nova_fiber_arena_thread_exit(void) { }
void nova_fiber_arena_release_retired(void) { }

/* Plan 151: POSIX no-op — Boehm STW per-thread скан видит главный native-
 * стек штатно (TIB-свопа как на Windows нет; main не крутит fiber здесь). */
/* [M-mn-spawnctx-corruption-cancel-wake] POSIX-порт Plan-151-механизма:
 * probe-адрес на стеке main. Сама локаль умирает — нужен только адрес
 * ВНУТРИ VMA main-стека для поиска диапазона в /proc/self/maps на каждой
 * сборке (_nova_push_main_stack_vma). Зовётся из _materialize_pool на
 * main ДО первого GC под оверрайднутым push_other_roots. */
void nova_fiber_arena_set_main_stack(void) {
#ifdef NOVA_GC_BOEHM
    /* [p418, №418 корень B] First-caller-wins, NOT unconditional overwrite.
     * This function has exactly one correctness contract: the published
     * probe must point somewhere INSIDE the real main OS thread's native
     * stack VMA (consumed by _nova_push_main_stack_vma via /proc/self/maps
     * every GC cycle — see the push_other_roots block above). It has TWO
     * call sites: (1) nova_fiber_arena_init's own bootstrap safety net
     * (fires on the main thread's FIRST mco_create — genuinely on the
     * native stack, no fiber has been resumed yet) and (2) _materialize_
     * pool (runtime.c, Plan 151), whose doc comment claims "we are
     * guaranteed on main, main isn't running a fiber here" — TRUE before
     * Plan 221.1 №108, FALSE after: №108 made main-body itself a real
     * fiber (nova_fiber_spawn_into + nova_supervised_run in emit_main_
     * wrapper), so EVERY user-level spawn — including the one that lazily
     * triggers _materialize_pool on the first worker-bound spawn — now
     * executes NESTED inside the already-resumed main-body fiber, i.e. on
     * the fiber's OWN arena stack, not the native one.
     *
     * An unconditional second write here therefore used to silently
     * replace the CORRECT probe (from call site 1, always genuinely
     * native) with a BOGUS one (from call site 2, now on the arena stack)
     * the moment the first worker-bound spawn fired — typically well
     * before the program finishes. From that point on, category (а) of
     * push_other_roots scanned the WRONG VMA (the fiber arena — already
     * redundantly covered by category (в)) and the REAL [stack] VMA
     * (holding e.g. `_nova_main_scope`, a NovaFiberQueue local to main())
     * dropped out of the GC root scan entirely. Anything reachable ONLY
     * through that stack-resident struct (its heap-allocated `sched_state`
     * chain in particular) became premature-collect bait — exactly the
     * class this file's push_other_roots override exists to prevent (see
     * [M-mn-spawnctx-corruption-cancel-wake] above): `nested_shield_
     * deadline_outer_fire_neg_v1_1`'s direct (non-supervised) `Time.
     * sleep()` on the main-body fiber crashed in `_nova_park_mark_slot`
     * writing through a NULL `nova_sched_parked_at()` — `st->capacity==0`,
     * `st->parked_chunks[0]==NULL`, i.e. a freshly-zeroed block reused at
     * the address the REAL (already fully grown) NovaSchedState used to
     * live at. Confirmed via the GC_DONT_GC=1 discriminator: 0/5 PASS
     * baseline -> 5/5 PASS with collection disabled.
     *
     * Fix: CAS NULL -> probe. Whichever caller runs FIRST wins (call site
     * 1 always runs first post-№108, since main-body's own fiber creation
     * is unconditionally the first `mco_create` of the whole process) —
     * later callers become harmless no-ops instead of clobbers. */
    char* expected = NULL;
    char probe_local;
    __atomic_compare_exchange_n(&_nova_main_stack_probe, &expected,
                                 (char*)&probe_local, false,
                                 __ATOMIC_RELEASE, __ATOMIC_RELAXED);
#endif
}

#ifndef NOVA_GC_BOEHM
/* Без Boehm push_other_roots-механизма реестр native-стеков не нужен. */
void nova_fiber_arena_register_native_stack(void) { }
void nova_fiber_arena_unregister_native_stack(void) { }
#endif

NovaFiberArenaStats nova_fiber_arena_stats(void) {
    NovaFiberArenaStats s = { 0 };
    if (_t_arena && _t_arena->base) {
        s.virtual_reserved = _t_arena->virtual_size;
        s.slot_count       = _t_arena->slot_count;
        s.slots_active     = __atomic_load_n(&_t_arena->slots_active,
                                              __ATOMIC_RELAXED);
        s.high_water       = _t_arena->high_water;
    }
    return s;
}

#else /* не POSIX — Windows (fiber_arena_win.c) или unsupported */

/* Пустой TU. На Windows arena-реализацию несёт fiber_arena_win.c; на
 * unsupported-платформах NOVA_FIBER_ARENA_ENABLED == 0 и API не
 * объявлен. Файл всегда в списке линковки — отдельный маркер-тип. */
typedef int _nova_fiber_arena_disabled_marker;

#endif /* POSIX */
