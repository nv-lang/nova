// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_BENCH_H
#define NOVA_RT_BENCH_H

/* Plan 57 — runtime для bench DSL.
 *
 * Header-only — все функции `static inline`, без отдельного .c файла,
 * чтобы не модифицировать build_command в test_runner.rs.
 *
 * Содержит:
 *   - nova_bench_now_ns()      — high-resolution timer (uv_hrtime если есть, иначе платформенный).
 *   - nova_bench_opaque_<T>(v) — макросы prevent constant-folding (Rust `hint::black_box` analogue).
 *   - NovaBenchState           — TLS state per текущему бенчу (iters_per_sample, throughput, allocs).
 *   - nova_bench_run(...)      — orchestrator: warmup → calibration → 100 samples → JSONL output.
 *
 * JSONL output на stdout:
 *   __BENCH_RESULT__ {"name":"...","raw_ns":[...],"iters_per_sample":N,...}
 *
 * CLI orchestrator парсит эти строки, агрегирует с raw samples в стат.
 */

#include "nova_rt.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef NOVA_USE_LIBUV
#  include <uv.h>
#endif

#if defined(_WIN32)
#  define WIN32_LEAN_AND_MEAN
#  include <windows.h>
#elif defined(__APPLE__)
#  include <mach/mach_time.h>
#elif defined(__linux__) || defined(__unix__)
#  include <time.h>
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* ─────────────────────────────────────────────────────────────────────────── */
/* High-resolution timer                                                       */
/* ─────────────────────────────────────────────────────────────────────────── */

/* Returns monotonic nanoseconds since arbitrary epoch.
 * Backed by libuv `uv_hrtime()` when available (Plan 22), иначе платформенный
 * (QueryPerformanceCounter / mach_absolute_time / clock_gettime). */
static inline uint64_t nova_bench_now_ns(void) {
#ifdef NOVA_USE_LIBUV
    return (uint64_t)uv_hrtime();
#elif defined(_WIN32)
    static LARGE_INTEGER freq = {0};
    LARGE_INTEGER counter;
    if (freq.QuadPart == 0) {
        QueryPerformanceFrequency(&freq);
    }
    QueryPerformanceCounter(&counter);
    /* counter * 1e9 / freq — avoid overflow with 128-bit-style split. */
    uint64_t secs = (uint64_t)counter.QuadPart / (uint64_t)freq.QuadPart;
    uint64_t rem = (uint64_t)counter.QuadPart % (uint64_t)freq.QuadPart;
    return secs * 1000000000ULL + rem * 1000000000ULL / (uint64_t)freq.QuadPart;
#elif defined(__APPLE__)
    static mach_timebase_info_data_t info = {0, 0};
    if (info.denom == 0) mach_timebase_info(&info);
    uint64_t t = mach_absolute_time();
    return t * info.numer / info.denom;
#elif defined(CLOCK_MONOTONIC_RAW)
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC_RAW, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
#else
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
#endif
}

/* ─────────────────────────────────────────────────────────────────────────── */
/* Opaque (black-box) barriers — prevent compiler optimization elimination.    */
/* Equivalent of Rust `std::hint::black_box`, Go `runtime.KeepAlive`.          */
/* ─────────────────────────────────────────────────────────────────────────── */

#if defined(__GNUC__) || defined(__clang__)
/* Inline-asm "+r" constraint forces compiler to materialize the value в register
 * и считать что value може быть прочитан/изменён. Zero machine-code emitted. */
#  define NOVA_BENCH_OPAQUE_PRIM(v) ({                          \
        __typeof__(v) _nova_op_x = (v);                          \
        __asm__ __volatile__("" : "+r"(_nova_op_x) : : "memory"); \
        _nova_op_x;                                              \
    })
/* Для composite-типов которые могут не помещаться в register (structs, strings) — \
 * memory clobber через указатель. */
#  define NOVA_BENCH_OPAQUE_MEM(v) ({                            \
        __typeof__(v) _nova_op_x = (v);                          \
        __asm__ __volatile__("" : : "g"(&_nova_op_x) : "memory"); \
        _nova_op_x;                                              \
    })
#elif defined(_MSC_VER)
/* MSVC: _ReadWriteBarrier deprecated в /std:c++20, заменён atomic. Для C —
 * используем volatile cast + assignment + compiler barrier. */
#  include <intrin.h>
#  pragma intrinsic(_ReadWriteBarrier)
static inline nova_int nova_bench_opaque_int(nova_int v) {
    volatile nova_int x = v;
    _ReadWriteBarrier();
    return x;
}
static inline nova_f64 nova_bench_opaque_f64(nova_f64 v) {
    volatile nova_f64 x = v;
    _ReadWriteBarrier();
    return x;
}
static inline nova_bool nova_bench_opaque_bool(nova_bool v) {
    volatile nova_bool x = v;
    _ReadWriteBarrier();
    return x;
}
static inline nova_str nova_bench_opaque_str(nova_str v) {
    volatile const char* p = v.ptr;
    volatile size_t l = v.len;
    _ReadWriteBarrier();
    nova_str r;
    r.ptr = (const char*)p;
    r.len = l;
    return r;
}
static inline void* nova_bench_opaque_ptr(void* v) {
    volatile void* x = v;
    _ReadWriteBarrier();
    return (void*)x;
}
/* Plan 82 followup: _Generic — C11, требует /std:c11; но test_runner
 * не задаёт /std: чтобы codegen-овский struct-cast `(StructTy)(e)`
 * (GCC ext) не падал C2440 в strict-режиме. Используем `_Generic`
 * только если он реально доступен (`__STDC_VERSION__ >= 201112L`); под
 * permissive MS-C-режимом — no-op (`v`). Opaque-trick — анти-оптимизация
 * барьер; no-op fallback корректен (бенчи получают чуть менее агрессивный
 * барьер, но компилируются под cl.exe). */
#  if defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
#    define NOVA_BENCH_OPAQUE_PRIM(v) \
      (_Generic((v), nova_int: nova_bench_opaque_int, \
                     nova_f64: nova_bench_opaque_f64, \
                     nova_bool: nova_bench_opaque_bool, \
                     default: nova_bench_opaque_int)((v)))
#  else
#    define NOVA_BENCH_OPAQUE_PRIM(v) (v)
#  endif
#  define NOVA_BENCH_OPAQUE_MEM(v) (v)  /* fallback */
#else
#  define NOVA_BENCH_OPAQUE_PRIM(v) (v)
#  define NOVA_BENCH_OPAQUE_MEM(v) (v)
#endif

/* ─────────────────────────────────────────────────────────────────────────── */
/* Bench TLS state                                                             */
/* ─────────────────────────────────────────────────────────────────────────── */

/* Per-bench mutable state, accessible во время measure-блока.
 *   - iters_per_sample: количество iter'ов в одном sample (set by orchestrator).
 *   - throughput_bytes / throughput_elements: optional Throughput annotation.
 *   - timer_start_ns: момент начала текущего sample, для bench.reset_timer().
 *   - alloc_count_start / alloc_bytes_start: GC snapshots для bench.allocs().
 *
 * TLS (per-thread) — bench runs на main thread, но защищаемся на случай
 * future multi-thread benchmarks. */
typedef struct {
    uint64_t iters_per_sample;
    uint64_t throughput_bytes;
    uint64_t throughput_elements;
    uint64_t timer_start_ns;
    int64_t  alloc_count_start;
    int64_t  alloc_count_delta;
    int64_t  alloc_bytes_start;
    int64_t  alloc_bytes_delta;
} NovaBenchState;

#ifdef _MSC_VER
#  define NOVA_THREAD_LOCAL __declspec(thread)
#else
#  define NOVA_THREAD_LOCAL __thread
#endif

extern NOVA_THREAD_LOCAL NovaBenchState _nova_bench_state;

/* Definition — emitted once в codegen (emit_main_wrapper в bench-mode).
 * Plan 57.C.3: heap sampler globals также определены здесь.
 * Plan 57.C.4: instruction counter globals (Linux-only). */
#define NOVA_BENCH_STATE_DEFINE \
    NOVA_THREAD_LOCAL NovaBenchState _nova_bench_state = {0}; \
    volatile int _nova_bench_heap_sampler_stop = 0; \
    uint64_t _nova_bench_heap_sample_interval_ns = 0; \
    NOVA_BENCH_INSTR_DEFINE

/* ─────────────────────────────────────────────────────────────────────────── */
/* GC introspection bridge (Plan 32 dependency).                               */
/* ─────────────────────────────────────────────────────────────────────────── */

/* Forward-decls — реализация в alloc.c / alloc_boehm.c. */
size_t   nova_gc_alloc_count(void);
size_t   nova_gc_heap_size(void);
uint64_t nova_gc_last_pause_ns(void);  /* Plan 57.C.2 */

static inline int64_t nova_bench_alloc_count_snapshot(void) {
    return (int64_t)nova_gc_alloc_count();
}

/* ─────────────────────────────────────────────────────────────────────────── */
/* Plan 57.C.3 — Heap sampler thread.                                          */
/* Activated по NOVA_BENCH_HEAP_SAMPLE_MS=N env. Background thread каждые N    */
/* ms emits `__HEAP_SAMPLE__ <ns> <bytes>` на stderr. CLI parses → histogram. */
/* ─────────────────────────────────────────────────────────────────────────── */

#ifdef NOVA_USE_LIBUV
#  include <uv.h>
#endif

/* Forward decl — definition в "Sample collection" section ниже. */
static inline uint64_t nova_bench_env_u64(const char* name, uint64_t def);

/* ─────────────────────────────────────────────────────────────────────────── */
/* Plan 57.C.4 — CPU instructions counter (Linux only).                        */
/* Activated по NOVA_BENCH_MEASURE_INSTRUCTIONS=1 env. Per-sample reset+       */
/* enable+measure+disable+read через perf_event_open syscall. Result emitted   */
/* в `__INSTR_SAMPLE__ <iters> <instr>` markers; CLI aggregates median.        */
/* ─────────────────────────────────────────────────────────────────────────── */

#if defined(__linux__)
#  include <linux/perf_event.h>
#  include <sys/syscall.h>
#  include <sys/ioctl.h>
#  include <unistd.h>
#  include <string.h>

static inline long nova_bench_perf_event_open_syscall(
        struct perf_event_attr* attr, pid_t pid, int cpu,
        int group_fd, unsigned long flags) {
    return syscall(__NR_perf_event_open, attr, pid, cpu, group_fd, flags);
}

extern int _nova_bench_instr_fd;
extern int _nova_bench_instr_enabled;

#define NOVA_BENCH_INSTR_DEFINE \
    int _nova_bench_instr_fd = -1; \
    int _nova_bench_instr_enabled = 0;

static inline void nova_bench_instr_start(void) {
    if (nova_bench_env_u64("NOVA_BENCH_MEASURE_INSTRUCTIONS", 0) == 0) return;
    if (_nova_bench_instr_fd >= 0) return;
    struct perf_event_attr attr;
    memset(&attr, 0, sizeof(attr));
    attr.type = PERF_TYPE_HARDWARE;
    attr.size = sizeof(struct perf_event_attr);
    attr.config = PERF_COUNT_HW_INSTRUCTIONS;
    attr.disabled = 1;
    attr.exclude_kernel = 1;
    attr.exclude_hv = 1;
    long fd = nova_bench_perf_event_open_syscall(&attr, 0, -1, -1, 0);
    if (fd < 0) {
        fprintf(stderr, "nova bench: perf_event_open failed (errno=%d) — "
                "skip instructions counter\n", (int)fd);
        return;
    }
    _nova_bench_instr_fd = (int)fd;
    _nova_bench_instr_enabled = 1;
}

static inline void nova_bench_instr_stop(void) {
    if (_nova_bench_instr_fd >= 0) {
        close(_nova_bench_instr_fd);
        _nova_bench_instr_fd = -1;
    }
    _nova_bench_instr_enabled = 0;
}

static inline void nova_bench_instr_sample_reset(void) {
    if (!_nova_bench_instr_enabled) return;
    ioctl(_nova_bench_instr_fd, PERF_EVENT_IOC_RESET, 0);
    ioctl(_nova_bench_instr_fd, PERF_EVENT_IOC_ENABLE, 0);
}

static inline uint64_t nova_bench_instr_sample_read(void) {
    if (!_nova_bench_instr_enabled) return 0;
    ioctl(_nova_bench_instr_fd, PERF_EVENT_IOC_DISABLE, 0);
    uint64_t val = 0;
    ssize_t n = read(_nova_bench_instr_fd, &val, sizeof(val));
    if (n != sizeof(val)) return 0;
    return val;
}

static inline int nova_bench_instr_active(void) {
    return _nova_bench_instr_enabled;
}

#else
/* Non-Linux: stubs returning 0 (CPU instructions counter unavailable). */
#define NOVA_BENCH_INSTR_DEFINE
static inline void nova_bench_instr_start(void) {}
static inline void nova_bench_instr_stop(void) {}
static inline void nova_bench_instr_sample_reset(void) {}
static inline uint64_t nova_bench_instr_sample_read(void) { return 0; }
static inline int nova_bench_instr_active(void) { return 0; }
#endif

/* Volatile flag — set to 1 by main to signal sampler stop. */
extern volatile int _nova_bench_heap_sampler_stop;
extern uint64_t _nova_bench_heap_sample_interval_ns;

#define NOVA_BENCH_HEAP_SAMPLER_DEFINE \
    volatile int _nova_bench_heap_sampler_stop = 0; \
    uint64_t _nova_bench_heap_sample_interval_ns = 0;

#ifdef NOVA_USE_LIBUV
/* libuv-thread entry — sleeps interval_ns, reads heap_size, emits marker. */
static void nova_bench_heap_sampler_thread(void* arg) {
    (void)arg;
    while (!_nova_bench_heap_sampler_stop) {
        uint64_t t = nova_bench_now_ns();
        size_t hs = nova_gc_heap_size();
        fprintf(stderr, "__HEAP_SAMPLE__ %llu %zu\n",
                (unsigned long long)t, hs);
        fflush(stderr);
        /* Sleep coarse-grained — Sleep(ms) на Windows, nanosleep на POSIX. */
        uint64_t ns = _nova_bench_heap_sample_interval_ns;
#if defined(_WIN32)
        Sleep((DWORD)(ns / 1000000ULL));
#else
        struct timespec ts;
        ts.tv_sec  = (time_t)(ns / 1000000000ULL);
        ts.tv_nsec = (long)(ns % 1000000000ULL);
        nanosleep(&ts, NULL);
#endif
    }
}

extern uv_thread_t _nova_bench_heap_sampler_tid;
extern int _nova_bench_heap_sampler_active;

#define NOVA_BENCH_HEAP_SAMPLER_THREAD_DEFINE \
    uv_thread_t _nova_bench_heap_sampler_tid; \
    int _nova_bench_heap_sampler_active = 0;

static inline void nova_bench_heap_sampler_start(void) {
    uint64_t ms = nova_bench_env_u64("NOVA_BENCH_HEAP_SAMPLE_MS", 0);
    if (ms == 0) return;  /* disabled */
    _nova_bench_heap_sample_interval_ns = ms * 1000000ULL;
    _nova_bench_heap_sampler_stop = 0;
    _nova_bench_heap_sampler_active = 1;
    uv_thread_create(&_nova_bench_heap_sampler_tid,
                     nova_bench_heap_sampler_thread, NULL);
}

static inline void nova_bench_heap_sampler_stop(void) {
    if (!_nova_bench_heap_sampler_active) return;
    _nova_bench_heap_sampler_stop = 1;
    uv_thread_join(&_nova_bench_heap_sampler_tid);
    _nova_bench_heap_sampler_active = 0;
}
#else
/* Без libuv — sampler thread не доступен; emit-stub no-op. */
#define NOVA_BENCH_HEAP_SAMPLER_THREAD_DEFINE
static inline void nova_bench_heap_sampler_start(void) {}
static inline void nova_bench_heap_sampler_stop(void) {}
#endif

/* ─────────────────────────────────────────────────────────────────────────── */
/* Sample collection                                                           */
/* ─────────────────────────────────────────────────────────────────────────── */

/* Default sampling parameters (overridable через env при будущем расширении). */
#define NOVA_BENCH_DEFAULT_WARMUP_NS      ((uint64_t)500 * 1000000ULL)   /* 500 ms */
#define NOVA_BENCH_DEFAULT_TARGET_NS      ((uint64_t)1 * 1000000ULL)     /* 1 ms per sample */
#define NOVA_BENCH_DEFAULT_SAMPLES        100
#define NOVA_BENCH_MAX_SAMPLES            1024  /* hard upper bound */
#define NOVA_BENCH_DEFAULT_TIME_BUDGET_NS ((uint64_t)10 * 1000000000ULL) /* 10 s */

/* Read uint64_t из env var, fallback на default. Простой parser, без error
 * propagation: при невалидном — silent fallback. */
static inline uint64_t nova_bench_env_u64(const char* name, uint64_t def) {
    const char* v = getenv(name);
    if (!v || !*v) return def;
    uint64_t r = 0;
    for (const char* p = v; *p; ++p) {
        if (*p < '0' || *p > '9') return def;
        r = r * 10 + (uint64_t)(*p - '0');
    }
    return r;
}

/* Print JSON-escaped string into stdout — handles ", \, control chars. */
static inline void nova_bench_print_json_str(const char* s) {
    fputc('"', stdout);
    for (const char* p = s; *p; ++p) {
        unsigned char c = (unsigned char)*p;
        switch (c) {
            case '"':  fputs("\\\"", stdout); break;
            case '\\': fputs("\\\\", stdout); break;
            case '\n': fputs("\\n", stdout); break;
            case '\r': fputs("\\r", stdout); break;
            case '\t': fputs("\\t", stdout); break;
            case '\b': fputs("\\b", stdout); break;
            case '\f': fputs("\\f", stdout); break;
            default:
                if (c < 0x20) {
                    fprintf(stdout, "\\u%04x", c);
                } else {
                    fputc((int)c, stdout);
                }
        }
    }
    fputc('"', stdout);
}

/* ─────────────────────────────────────────────────────────────────────────── */
/* Orchestrator: nova_bench_run                                                */
/* ─────────────────────────────────────────────────────────────────────────── */

typedef void (*NovaBenchPhaseFn)(void);

/* Run a single benchmark:
 *   1. setup() (1x, не measured)
 *   2. warmup: do { measure(); } until elapsed >= warmup_ns
 *   3. calibration: measure 1x, compute iters_per_sample
 *   4. samples: 100x batches of (iters_per_sample × measure()), record wall-clock per batch
 *   5. teardown() (1x, не measured)
 *   6. Emit JSONL: __BENCH_RESULT__ {...}
 *
 * Defensive: time-budget caps total samples; min iters_per_sample = 1.
 */
static inline void nova_bench_run(const char* name,
                                  NovaBenchPhaseFn setup,
                                  NovaBenchPhaseFn measure,
                                  NovaBenchPhaseFn teardown) {
    /* Reset state. */
    memset(&_nova_bench_state, 0, sizeof(_nova_bench_state));

    /* Configurable knobs via env. */
    uint64_t warmup_ns   = nova_bench_env_u64("NOVA_BENCH_WARMUP_NS",   NOVA_BENCH_DEFAULT_WARMUP_NS);
    uint64_t target_ns   = nova_bench_env_u64("NOVA_BENCH_TARGET_NS",   NOVA_BENCH_DEFAULT_TARGET_NS);
    uint64_t n_samples   = nova_bench_env_u64("NOVA_BENCH_SAMPLES",     NOVA_BENCH_DEFAULT_SAMPLES);
    uint64_t time_budget = nova_bench_env_u64("NOVA_BENCH_TIME_BUDGET_NS", NOVA_BENCH_DEFAULT_TIME_BUDGET_NS);
    if (n_samples == 0) n_samples = NOVA_BENCH_DEFAULT_SAMPLES;
    if (n_samples > NOVA_BENCH_MAX_SAMPLES) n_samples = NOVA_BENCH_MAX_SAMPLES;

    /* Marker для CLI: bench start (полезно для stream parsing). */
    fputs("__BENCH_START__ ", stdout);
    nova_bench_print_json_str(name);
    fputc('\n', stdout);
    fflush(stdout);

    /* Setup (1x). */
    if (setup) setup();

    /* Warmup. */
    {
        uint64_t t_start = nova_bench_now_ns();
        uint64_t iters = 0;
        do {
            measure();
            iters++;
        } while (nova_bench_now_ns() - t_start < warmup_ns);
        (void)iters;
    }

    /* Calibration — 1x measurement, compute iters_per_sample. */
    uint64_t iters_per_sample;
    {
        _nova_bench_state.iters_per_sample = 1;
        uint64_t t0 = nova_bench_now_ns();
        measure();
        uint64_t single_ns = nova_bench_now_ns() - t0;
        if (single_ns == 0) single_ns = 1;
        iters_per_sample = target_ns / single_ns;
        if (iters_per_sample < 1) iters_per_sample = 1;
        if (iters_per_sample > 1000000) iters_per_sample = 1000000;  /* sanity cap */
    }
    _nova_bench_state.iters_per_sample = iters_per_sample;

    /* Sample loop. */
    uint64_t* samples = (uint64_t*)malloc(sizeof(uint64_t) * (size_t)n_samples);
    if (!samples) {
        fprintf(stderr, "nova bench: malloc failed for samples buffer\n");
        return;
    }
    uint64_t collected = 0;
    int64_t alloc_count_pre = nova_bench_alloc_count_snapshot();
    uint64_t suite_start = nova_bench_now_ns();
    for (uint64_t i = 0; i < n_samples; i++) {
        _nova_bench_state.timer_start_ns = nova_bench_now_ns();
        for (uint64_t k = 0; k < iters_per_sample; k++) {
            measure();
        }
        uint64_t t_end = nova_bench_now_ns();
        uint64_t elapsed = t_end - _nova_bench_state.timer_start_ns;
        /* Per-iter ns. */
        samples[i] = elapsed / iters_per_sample;
        collected++;
        if (nova_bench_now_ns() - suite_start > time_budget) {
            break;
        }
    }
    int64_t alloc_count_post = nova_bench_alloc_count_snapshot();

    /* Teardown. */
    if (teardown) teardown();

    /* Emit JSON. */
    fputs("__BENCH_RESULT__ {\"name\":", stdout);
    nova_bench_print_json_str(name);
    fprintf(stdout, ",\"iters_per_sample\":%llu", (unsigned long long)iters_per_sample);
    fprintf(stdout, ",\"samples_count\":%llu", (unsigned long long)collected);
    fputs(",\"raw_ns\":[", stdout);
    for (uint64_t i = 0; i < collected; i++) {
        if (i > 0) fputc(',', stdout);
        fprintf(stdout, "%llu", (unsigned long long)samples[i]);
    }
    fputc(']', stdout);
    if (_nova_bench_state.throughput_bytes) {
        fprintf(stdout, ",\"throughput_bytes\":%llu",
                (unsigned long long)_nova_bench_state.throughput_bytes);
    }
    if (_nova_bench_state.throughput_elements) {
        fprintf(stdout, ",\"throughput_elements\":%llu",
                (unsigned long long)_nova_bench_state.throughput_elements);
    }
    /* Alloc delta — общее число allocs ЗА весь sampling phase /
     * (iters_per_sample × collected) — приближённо per-iter. */
    int64_t total_iters = (int64_t)iters_per_sample * (int64_t)collected;
    if (total_iters > 0) {
        int64_t alloc_delta = alloc_count_post - alloc_count_pre;
        /* Floor div для consistent integer reporting. */
        int64_t per_iter = alloc_delta / total_iters;
        fprintf(stdout, ",\"allocs_per_iter\":%lld", (long long)per_iter);
        fprintf(stdout, ",\"allocs_total\":%lld", (long long)alloc_delta);
    }
    fputs("}\n", stdout);
    fflush(stdout);

    free(samples);
}

/* ─────────────────────────────────────────────────────────────────────────── */
/* DSL builtins — `bench.opaque(v)`, `bench.iterations()`, etc.                */
/* Codegen emits these names directly.                                         */
/* ─────────────────────────────────────────────────────────────────────────── */

static inline nova_int nova_bench_iterations(void) {
    return (nova_int)_nova_bench_state.iters_per_sample;
}

static inline void nova_bench_reset_timer(void) {
    _nova_bench_state.timer_start_ns = nova_bench_now_ns();
}

static inline void nova_bench_set_throughput_bytes(nova_int n) {
    _nova_bench_state.throughput_bytes = (uint64_t)n;
}

static inline void nova_bench_set_throughput_elements(nova_int n) {
    _nova_bench_state.throughput_elements = (uint64_t)n;
}

/* Plan 57.G.5 — Custom metric per-sample emission.
 * Emits marker line на stdout (data channel parsed by CLI alongside
 * __BENCH_RESULT__). CLI groups by (name, unit) → custom_metrics[]
 * field в JSON v1 с count/min/median/max/sum aggregates.
 *
 * Name + unit могут содержать spaces — для simplicity форматирую
 * с TAB separators: "__BENCH_METRIC__\t<name>\t<value>\t<unit>\n"
 * (TAB ASCII 0x09 не valid в Nova string literal без escape).
 *
 * `name` / `unit` — nova_str (struct { const char* ptr; size_t len; }).
 * NULL ptr защищается ".ptr ? ... : "?"" fallback.
 */
static inline void nova_bench_emit_metric(nova_str name, nova_int value, nova_str unit) {
    fprintf(stdout, "__BENCH_METRIC__\t%.*s\t%lld\t%.*s\n",
        (int)(name.ptr ? name.len : 1),
        name.ptr ? name.ptr : "?",
        (long long)value,
        (int)(unit.ptr ? unit.len : 0),
        unit.ptr ? unit.ptr : "");
    fflush(stdout);
}

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_BENCH_H */
