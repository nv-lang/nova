/* SPDX-License-Identifier: MIT
 *
 * Plan 172.14 F.2, atom A1 -- "GC truth by running it".
 *
 * QUESTION. A value-sum lowers to `struct { tag; union payload; }` held
 * INLINE in a slot, not behind `nova_alloc`. When the active arm of that
 * union holds a GC pointer, does the collector still see it? Boehm is
 * conservative with interior pointers on, so the answer is "yes" wherever
 * the slot itself is scanned -- and the whole question reduces to: WHICH
 * SLOTS ARE SCANNED.
 *
 * This harness answers that per region with a signal that cannot be faked:
 * GC_register_finalizer. A finalizer fires exactly when an object became
 * unreachable, and observing it costs no reference of our own -- which is
 * precisely why the obvious alternative (read the object back and check its
 * bytes) cannot work: the handle used to read it would root it.
 *
 * Every region is measured twice, and a region counts as proven only when
 * BOTH runs land where predicted:
 *   - live run: the inline pointer is left in place     -> expect 0 collected
 *   - dead run: the same slot is cleared before collect -> expect N collected
 * A fixture that stays green with the pointer removed proves nothing; the
 * dead run is what gives the live run its meaning.
 *
 * R6 is the standalone negative control: the pointer is XOR-obfuscated, so
 * no conservative scan can recognise it. If R6 does not collect, the harness
 * itself is broken (stale stack words are retaining everything) and no other
 * number in the run may be believed -- the program exits non-zero.
 *
 * Init mirrors nova_gc_init (nova_rt/alloc_boehm.c:165-230) line for line,
 * including GC_set_no_dls(1) and GC_set_all_interior_pointers(1) -- the two
 * settings the whole question turns on.
 *
 * NOT part of the build. Standalone; the build command is in section A1 of
 * docs/plans/wip/RECON-172.14-sum-stack.md.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

#define GC_THREADS 1
#include <gc.h>

#define NOBJ 200         /* objects per region */
#define OBJ_SIZE 64      /* comfortably larger than a pointer, so an interior
                          * hit is possible and we are not accidentally
                          * measuring base-pointer-only behaviour */

/* The shape under test: a tag word followed by a GC pointer -- exactly what
 * `struct NovaValue_X { nova_int tag; Nova_Payload* p; }` compiles to today
 * (verified against generated C; see the report). The pointer deliberately
 * sits at a NON-ZERO offset, so this really is "inline in a struct" rather
 * than "at the base of an allocation". */
typedef struct {
    intptr_t tag;
    void*    p;
} InlineSum;

static volatile long g_finalized;

static void on_finalize(void* obj, void* client_data) {
    (void)obj; (void)client_data;
    g_finalized++;
}

/* One traced object with a finalizer attached. */
static void* make_tracked(void) {
    void* o = GC_MALLOC(OBJ_SIZE);
    memset(o, 0xA5, OBJ_SIZE);
    GC_REGISTER_FINALIZER_NO_ORDER(o, on_finalize, NULL, NULL, NULL);
    return o;
}

/* Wipe stale copies of our pointers left in dead stack frames and spilled
 * registers. Without this the dead runs report false survivors and every
 * conclusion inverts. */
static void scrub_stack(void) {
    volatile char buf[512 * 1024];
    memset((void*)buf, 0, sizeof buf);
    volatile char sink = 0;
    for (size_t i = 0; i < sizeof buf; i += 4096) sink = (char)(sink + buf[i]);
    (void)sink;
}

/* Force a full collection and let finalizers run. GC_gcollect is a complete
 * stop-the-world mark regardless of the incremental mode nova_gc_init turns
 * on, so the result does not depend on collector scheduling. */
static void collect(void) {
    scrub_stack();
    GC_gcollect();
    GC_invoke_finalizers();
    GC_gcollect();
    GC_invoke_finalizers();
}

/* Sink for the read-back below. A plain static: under GC_set_no_dls(1) it is
 * not a root, so writing to it cannot accidentally retain anything. */
static volatile uintptr_t g_sink;

static long measure(const char* what, void (*fill)(InlineSum*, int),
                    InlineSum* slots, int live) {
    /* Drain whatever the previous region left pending, THEN zero the counter.
     * Without this, finalizers belonging to an earlier region land in this
     * region's total and the numbers drift past N. */
    memset(slots, 0, sizeof(InlineSum) * NOBJ);
    collect();
    collect();
    g_finalized = 0;

    fill(slots, live);
    collect();
    long collected = g_finalized;

    /* Read the slots back AFTER the collection. This is load-bearing, not
     * hygiene: `measure` never otherwise reads `slots`, so at -O1 the stores
     * in `fill` are dead and clang deletes them -- which made the first run of
     * this harness report the plain stack frame as UNSCANNED, an impossible
     * result that is what caught the bug. Accumulating through a volatile sink
     * forces the array to be genuinely live across the collection. */
    uintptr_t acc = 0;
    for (int i = 0; i < NOBJ; i++) {
        acc += (uintptr_t)slots[i].p ^ (uintptr_t)slots[i].tag;
    }
    g_sink = acc;

    printf("  %-34s %-4s -> collected %3ld / %d\n",
           what, live ? "live" : "dead", collected, NOBJ);
    return collected;
}

/* Each fill writes NOBJ tracked pointers into `slots`, or clears the slots
 * on the dead run while still allocating the objects (otherwise the dead run
 * would measure nothing at all). */
static void fill_plain(InlineSum* s, int live) {
    for (int i = 0; i < NOBJ; i++) {
        s[i].tag = i;
        s[i].p = live ? make_tracked() : NULL;
    }
    if (!live) {
        for (int i = 0; i < NOBJ; i++) (void)make_tracked();
    }
}

static uintptr_t g_mask = (uintptr_t)0x5DEECE66D;

static void fill_xor(InlineSum* s, int live) {
    for (int i = 0; i < NOBJ; i++) {
        void* o = make_tracked();
        s[i].tag = i;
        s[i].p = live ? (void*)((uintptr_t)o ^ g_mask) : NULL;
    }
}

/* R2: statics. Under GC_set_no_dls(1) the data segment is not scanned unless
 * registered -- which is why the compiler emits nova_gc_add_root for lazy
 * statics (emit_c.rs:10177). */
static InlineSum g_static_slots[NOBJ];

int main(void) {
    /* --- mirror of nova_gc_init, alloc_boehm.c:165-230 --- */
    GC_set_no_dls(1);
    GC_INIT();
    if (!getenv("NOVA_GC_INCREMENTAL") || strcmp(getenv("NOVA_GC_INCREMENTAL"), "0") != 0) {
        GC_enable_incremental();
    }
    GC_expand_hp(4 * 1024 * 1024);
    {
        const char* e = getenv("NOVA_GC_NO_INTERIOR");
        GC_set_all_interior_pointers((e && e[0] == '1') ? 0 : 1);
    }
    /* ------------------------------------------------------ */

    const char* ni = getenv("NOVA_GC_NO_INTERIOR");
    printf("A1 -- is an inline GC pointer found, per region?\n");
    printf("interior_pointers=%d  no_dls=1  N=%d obj=%dB\n\n",
           (ni && ni[0] == '1') ? 0 : 1, NOBJ, OBJ_SIZE);

    long r_live[8], r_dead[8];
    const char* names[8];
    int n = 0;

    /* R1 -- plain stack frame: where a value-sum spends most of its life. */
    {
        names[n] = "R1 stack frame (local array)";
        InlineSum slots[NOBJ];
        r_live[n] = measure(names[n], fill_plain, slots, 1);
        r_dead[n] = measure(names[n], fill_plain, slots, 0);
        n++;
    }

    /* R2a -- static storage, NOT registered. The real exposure: under
     * GC_set_no_dls(1) nobody scans it. */
    names[n] = "R2a static, no add_root";
    r_live[n] = measure(names[n], fill_plain, g_static_slots, 1);
    r_dead[n] = measure(names[n], fill_plain, g_static_slots, 0);
    n++;

    /* R2b -- the same storage, registered the way emit_c.rs:10177 registers a
     * lazy static. A difference from R2a means the root call is load-bearing. */
    GC_add_roots((char*)g_static_slots,
                 (char*)g_static_slots + sizeof g_static_slots);
    names[n] = "R2b static, WITH add_root";
    r_live[n] = measure(names[n], fill_plain, g_static_slots, 1);
    r_dead[n] = measure(names[n], fill_plain, g_static_slots, 0);
    n++;
    memset(g_static_slots, 0, sizeof g_static_slots);

    /* R3 -- GC_malloc_uncollectable: what nova_alloc_uncollectable returns for
     * SpawnCtx (alloc_boehm.c:284). alloc.h:36 claims it is scanned; check. */
    {
        names[n] = "R3 uncollectable (SpawnCtx)";
        InlineSum* slots = (InlineSum*)GC_MALLOC_UNCOLLECTABLE(sizeof(InlineSum) * NOBJ);
        r_live[n] = measure(names[n], fill_plain, slots, 1);
        r_dead[n] = measure(names[n], fill_plain, slots, 0);
        n++;
    }

    /* R4 -- plain calloc: what channels.h calls "calloc'd stacks are NOT GC
     * roots". Nothing registers this memory. */
    {
        names[n] = "R4 calloc (channel stacks)";
        InlineSum* slots = (InlineSum*)calloc(NOBJ, sizeof(InlineSum));
        r_live[n] = measure(names[n], fill_plain, slots, 1);
        r_dead[n] = measure(names[n], fill_plain, slots, 0);
        n++;
        free(slots);
    }

    /* R5 -- inside a GC-heap buffer: the Vec backing-array analogue, where a
     * value-sum element would live once Vec stores elements inline. */
    {
        names[n] = "R5 GC heap buffer (Vec backing)";
        InlineSum* slots = (InlineSum*)GC_MALLOC(sizeof(InlineSum) * NOBJ);
        r_live[n] = measure(names[n], fill_plain, slots, 1);
        r_dead[n] = measure(names[n], fill_plain, slots, 0);
        n++;
    }

    /* R6 -- NEGATIVE CONTROL. XOR-obfuscated on the stack: provably invisible
     * to any scan. If this does not collect, the harness is lying. */
    {
        names[n] = "R6 NEGATIVE CONTROL (xor'd)";
        InlineSum slots[NOBJ];
        r_live[n] = measure(names[n], fill_xor, slots, 1);
        r_dead[n] = measure(names[n], fill_xor, slots, 0);
        n++;
    }

    /* R7 -- FALSE RETENTION, the other direction. GC_malloc zeroes; a stack
     * struct does not. An inline union whose inactive arm still holds last
     * frame's bytes gives the conservative scanner words that look like live
     * pointers. Measured by running the dead case WITHOUT scrub_stack: the
     * slots are cleared and the objects are genuinely garbage, so anything
     * that fails to collect was retained by a stale word. */
    {
        InlineSum slots[NOBJ];
        memset(slots, 0, sizeof slots);
        GC_gcollect(); GC_invoke_finalizers();
        g_finalized = 0;
        for (int i = 0; i < NOBJ; i++) { slots[i].tag = i; slots[i].p = make_tracked(); }
        uintptr_t acc = 0;
        for (int i = 0; i < NOBJ; i++) acc += (uintptr_t)slots[i].p;
        g_sink = acc;
        memset(slots, 0, sizeof slots);      /* drop every visible reference */
        /* deliberately NO scrub_stack() here */
        GC_gcollect(); GC_invoke_finalizers();
        GC_gcollect(); GC_invoke_finalizers();
        long leaked = NOBJ - g_finalized;
        printf("\n  R7 false retention (no scrub)      -> %ld / %d objects retained by stale words\n",
               leaked, NOBJ);
    }

    printf("\n%-34s %6s %6s   %s\n", "region", "live", "dead", "verdict");
    int control_ok = 0;
    for (int i = 0; i < n; i++) {
        const char* v;
        if (strstr(names[i], "NEGATIVE") != NULL) {
            control_ok = (r_live[i] >= NOBJ) && (r_dead[i] >= NOBJ);
            v = control_ok ? "CONTROL OK (invisible, as required)"
                           : "CONTROL BROKEN -- distrust this run";
        } else if (r_live[i] == 0 && r_dead[i] >= NOBJ) {
            v = "SCANNED (proven: dead run collects all)";
        } else if (r_live[i] >= NOBJ) {
            v = "NOT SCANNED  <-- exposure";
        } else {
            v = "INCONCLUSIVE";
        }
        printf("%-34s %6ld %6ld   %s\n", names[i], r_live[i], r_dead[i], v);
    }
    printf("\ncontrol=%s\n", control_ok ? "ok" : "BROKEN");
    return control_ok ? 0 : 2;
}
