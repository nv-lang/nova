/*
 * test_fibers_deep.c — deep fiber/coroutine runtime tests
 *
 * Tests what Nova-level spawn{} cannot test: that fibers actually run on a
 * separate stack, that yield/resume work correctly, that nested fibers don't
 * corrupt each other's stack frames, and that captured pointers remain valid
 * across yield boundaries.
 *
 * Compile:
 *   cl /nologo /W3 /I. nova_rt\test_fibers_deep.c nova_rt\alloc.c /Fe:test_fibers.exe
 *   (fibers.c defines MINICORO_IMPL — included via fibers.h)
 */

#include "nova_rt/nova_rt.h"
#include <stdio.h>
#include <string.h>
#include <stdint.h>

/* ---- test framework ---- */
static int _total = 0;
static int _failed = 0;

#define ASSERT(cond, msg) do { \
    _total++; \
    if (!(cond)) { \
        printf("  FAIL [line %d] %s\n", __LINE__, msg); \
        _failed++; \
    } else { \
        printf("  pass: %s\n", msg); \
    } \
} while(0)

#define SECTION(name) printf("\n=== %s ===\n", name)

/* ---- Test 1: fiber runs on a different stack ---- */

static char* _caller_stack_addr = NULL;
static char* _fiber_stack_addr  = NULL;

static void fiber_stack_probe(mco_coro* co) {
    char local_var = 42;
    _fiber_stack_addr = &local_var;
    (void)co;
}

static void test_separate_stack(void) {
    SECTION("separate stack: fiber stack address differs from caller stack");

    char caller_local = 0;
    _caller_stack_addr = &caller_local;

    mco_desc desc = mco_desc_init(fiber_stack_probe, 0);
    mco_coro* co = NULL;
    mco_create(&co, &desc);
    mco_resume(co);
    mco_destroy(co);

    ptrdiff_t diff = _caller_stack_addr - _fiber_stack_addr;
    if (diff < 0) diff = -diff;

    printf("  info: caller stack=%p fiber stack=%p diff=%td bytes\n",
           (void*)_caller_stack_addr, (void*)_fiber_stack_addr, diff);

    ASSERT(_fiber_stack_addr != NULL, "fiber ran and set stack address");
    ASSERT(diff > 1024, "fiber stack is at least 1KB away from caller stack");
}

/* ---- Test 2: yield suspends and resume continues ---- */

static int _yield_log[4];  /* sequence of events */
static int _yield_pos = 0;

static void fiber_yield_sequence(mco_coro* co) {
    _yield_log[_yield_pos++] = 2;  /* fiber first step */
    mco_yield(co);
    _yield_log[_yield_pos++] = 4;  /* fiber second step */
}

static void test_yield_resume_order(void) {
    SECTION("yield/resume: execution order caller-fiber-caller-fiber-caller");

    _yield_pos = 0;
    memset(_yield_log, 0, sizeof(_yield_log));

    mco_desc desc = mco_desc_init(fiber_yield_sequence, 0);
    mco_coro* co = NULL;
    mco_create(&co, &desc);

    _yield_log[_yield_pos++] = 1;  /* caller before first resume */
    mco_resume(co);                 /* fiber runs to yield */
    _yield_log[_yield_pos++] = 3;  /* caller between resumes */
    mco_resume(co);                 /* fiber runs to completion */
    _yield_log[_yield_pos++] = 5;  /* caller after fiber done */

    mco_destroy(co);

    printf("  info: sequence = %d %d %d %d %d\n",
           _yield_log[0], _yield_log[1], _yield_log[2],
           _yield_log[3], _yield_log[4]);

    ASSERT(_yield_log[0] == 1, "step 1: caller before resume");
    ASSERT(_yield_log[1] == 2, "step 2: fiber first step");
    ASSERT(_yield_log[2] == 3, "step 3: caller between resumes");
    ASSERT(_yield_log[3] == 4, "step 4: fiber second step");
    ASSERT(_yield_log[4] == 5, "step 5: caller after completion");
}

/* ---- Test 3: fiber stack depth — deep recursion inside fiber ---- */

static int64_t _fib_result = 0;

static int64_t fib(int n) {
    if (n <= 1) return n;
    return fib(n-1) + fib(n-2);
}

static void fiber_deep_recursion(mco_coro* co) {
    /* fib(30) requires ~1M recursive calls — real stack depth test */
    _fib_result = fib(30);
    (void)co;
}

static void test_deep_stack(void) {
    SECTION("deep stack: fib(30) inside fiber completes correctly");

    _fib_result = 0;
    mco_desc desc = mco_desc_init(fiber_deep_recursion, 0);
    mco_coro* co = NULL;
    mco_create(&co, &desc);
    mco_resume(co);
    mco_destroy(co);

    printf("  info: fib(30) = %lld (expected 832040)\n", (long long)_fib_result);
    ASSERT(_fib_result == 832040, "fib(30) = 832040 computed inside fiber");
}

/* ---- Test 4: captured pointer valid across yield ---- */

typedef struct {
    int64_t* counter;
    int      steps;
} YieldCtx;

static void fiber_yield_with_capture(mco_coro* co) {
    YieldCtx* ctx = (YieldCtx*)mco_get_user_data(co);
    for (int i = 0; i < ctx->steps; i++) {
        (*ctx->counter)++;
        mco_yield(co);
    }
}

static void test_captured_ptr_across_yield(void) {
    SECTION("captured pointer: mutable counter survives 5 yield/resume cycles");

    int64_t counter = 0;
    YieldCtx ctx = { &counter, 5 };

    mco_desc desc = mco_desc_init(fiber_yield_with_capture, 0);
    desc.user_data = &ctx;
    mco_coro* co = NULL;
    mco_create(&co, &desc);

    for (int i = 0; i < 5; i++) {
        int64_t before = counter;
        mco_resume(co);
        ASSERT(counter == before + 1, "counter incremented on each resume");
    }

    mco_destroy(co);
    printf("  info: final counter = %lld (expected 5)\n", (long long)counter);
    ASSERT(counter == 5, "counter == 5 after 5 resume cycles");
}

/* ---- Test 5: two fibers interleaved — no stack corruption ---- */

typedef struct {
    int id;
    int64_t* shared_log;
    int*     log_pos;
    int      iters;
} InterleavedCtx;

static void fiber_interleaved(mco_coro* co) {
    InterleavedCtx* ctx = (InterleavedCtx*)mco_get_user_data(co);
    for (int i = 0; i < ctx->iters; i++) {
        ctx->shared_log[(*ctx->log_pos)++] = (int64_t)ctx->id * 100 + i;
        mco_yield(co);
    }
}

static void test_two_fibers_interleaved(void) {
    SECTION("two interleaved fibers: log shows correct round-robin sequence");

    int64_t log[20];
    int log_pos = 0;
    InterleavedCtx ctx1 = { 1, log, &log_pos, 5 };
    InterleavedCtx ctx2 = { 2, log, &log_pos, 5 };

    mco_desc d1 = mco_desc_init(fiber_interleaved, 0);
    d1.user_data = &ctx1;
    mco_desc d2 = mco_desc_init(fiber_interleaved, 0);
    d2.user_data = &ctx2;

    mco_coro *co1 = NULL, *co2 = NULL;
    mco_create(&co1, &d1);
    mco_create(&co2, &d2);

    /* Interleave: 1,2,1,2,... */
    for (int i = 0; i < 5; i++) {
        mco_resume(co1);
        mco_resume(co2);
    }

    mco_destroy(co1);
    mco_destroy(co2);

    printf("  info: log = ");
    for (int i = 0; i < log_pos; i++) printf("%lld ", (long long)log[i]);
    printf("\n");

    ASSERT(log_pos == 10, "10 log entries (5 per fiber)");
    /* Check alternation: fiber1 entries at even positions, fiber2 at odd */
    int ok = 1;
    for (int i = 0; i < 10; i++) {
        int expected_id = (i % 2 == 0) ? 1 : 2;
        int expected_iter = i / 2;
        if (log[i] != (int64_t)expected_id * 100 + expected_iter) ok = 0;
    }
    ASSERT(ok, "interleaved log shows correct round-robin with no corruption");
}

/* ---- Test 6: stack frames of caller intact after fiber ---- */

static void fiber_writes_garbage(mco_coro* co) {
    /* Fill our stack with known garbage to see if it leaks to caller */
    volatile char garbage[4096];
    memset((void*)garbage, 0xCC, sizeof(garbage));
    (void)garbage[0];
    (void)co;
}

static void test_caller_stack_intact(void) {
    SECTION("caller stack: local variables intact before and after fiber");

    int64_t sentinel1 = 0xDEAD0001LL;
    int64_t sentinel2 = 0xDEAD0002LL;
    int64_t sentinel3 = 0xDEAD0003LL;

    mco_desc desc = mco_desc_init(fiber_writes_garbage, 0);
    mco_coro* co = NULL;
    mco_create(&co, &desc);
    mco_resume(co);
    mco_destroy(co);

    ASSERT(sentinel1 == 0xDEAD0001LL, "sentinel1 intact after fiber");
    ASSERT(sentinel2 == 0xDEAD0002LL, "sentinel2 intact after fiber");
    ASSERT(sentinel3 == 0xDEAD0003LL, "sentinel3 intact after fiber");
}

/* ---- Test 7: nova_fiber_run matches manual create/resume/destroy ---- */

typedef struct {
    int64_t x;
    int64_t y;
    int64_t result;
} SpawnCtx;

static void fiber_add(mco_coro* co) {
    SpawnCtx* ctx = (SpawnCtx*)mco_get_user_data(co);
    ctx->result = ctx->x + ctx->y;
}

static void test_nova_fiber_run(void) {
    SECTION("nova_fiber_run: equivalent to manual create+resume+destroy");

    SpawnCtx ctx = { 123, 456, 0 };
    nova_fiber_run(fiber_add, &ctx);

    printf("  info: 123 + 456 = %lld\n", (long long)ctx.result);
    ASSERT(ctx.result == 579, "nova_fiber_run computes 123+456=579");
}

/* ---- Test 8: 1000 sequential fibers — no leak / no crash ---- */

static int64_t _sequential_sum = 0;

typedef struct { int64_t val; } SumCtx;

static void fiber_accumulate(mco_coro* co) {
    SumCtx* ctx = (SumCtx*)mco_get_user_data(co);
    _sequential_sum += ctx->val;
    (void)co;
}

static void test_many_sequential_fibers(void) {
    SECTION("1000 sequential fibers: each adds its index, sum == 500500");

    _sequential_sum = 0;
    SumCtx ctx;

    for (int i = 1; i <= 1000; i++) {
        ctx.val = (int64_t)i;
        mco_desc desc = mco_desc_init(fiber_accumulate, 0);
        desc.user_data = &ctx;
        mco_coro* co = NULL;
        mco_create(&co, &desc);
        mco_resume(co);
        mco_destroy(co);
    }

    printf("  info: sum of 1..1000 via 1000 fibers = %lld\n", (long long)_sequential_sum);
    ASSERT(_sequential_sum == 500500, "sum(1..1000) = 500500 via 1000 individual fibers");
}

/* ---- Test 9: fiber state machine — 3-state protocol via yield ---- */

typedef enum { STATE_INIT, STATE_PROCESSING, STATE_DONE } FiberState;

typedef struct {
    FiberState state;
    int64_t    input;
    int64_t    partial;
    int64_t    result;
} StateMachineCtx;

static void fiber_state_machine(mco_coro* co) {
    StateMachineCtx* ctx = (StateMachineCtx*)mco_get_user_data(co);

    /* State: INIT → PROCESSING */
    ctx->state = STATE_PROCESSING;
    ctx->partial = ctx->input * 2;  /* first half of work */
    mco_yield(co);                   /* caller can observe partial state */

    /* State: PROCESSING → DONE */
    ctx->result = ctx->partial + ctx->input;  /* = input*3 */
    ctx->state = STATE_DONE;
}

static void test_fiber_state_machine(void) {
    SECTION("fiber state machine: caller observes intermediate state after yield");

    StateMachineCtx ctx = { STATE_INIT, 7, 0, 0 };

    mco_desc desc = mco_desc_init(fiber_state_machine, 0);
    desc.user_data = &ctx;
    mco_coro* co = NULL;
    mco_create(&co, &desc);

    ASSERT(ctx.state == STATE_INIT, "initial state is INIT");

    mco_resume(co);
    ASSERT(ctx.state == STATE_PROCESSING, "after 1st resume: state is PROCESSING");
    ASSERT(ctx.partial == 14, "after 1st resume: partial = 7*2 = 14");
    ASSERT(ctx.result == 0,   "after 1st resume: result not yet computed");

    mco_resume(co);
    ASSERT(ctx.state == STATE_DONE, "after 2nd resume: state is DONE");
    ASSERT(ctx.result == 21, "after 2nd resume: result = 7*3 = 21");

    mco_destroy(co);
}

/* ---- Test 10: GC + fibers — allocate inside fiber, read outside ---- */

typedef struct { int64_t x; int64_t y; } FPoint;
static FPoint* _point_from_fiber = NULL;

static void fiber_alloc_point(mco_coro* co) {
    /* Allocate a GC-managed record inside the fiber */
    _point_from_fiber = (FPoint*)nova_alloc(sizeof(FPoint));
    _point_from_fiber->x = 99;
    _point_from_fiber->y = 77;
    (void)co;
}

static void test_gc_alloc_inside_fiber(void) {
    SECTION("GC inside fiber: object allocated in fiber readable after fiber exit");

    nova_gc_reset_stats();
    _point_from_fiber = NULL;

    mco_desc desc = mco_desc_init(fiber_alloc_point, 0);
    mco_coro* co = NULL;
    mco_create(&co, &desc);
    mco_resume(co);
    mco_destroy(co);

    ASSERT(_point_from_fiber != NULL, "fiber set the shared pointer");
    ASSERT(_point_from_fiber->x == 99LL, "x field intact after fiber exit");
    ASSERT(_point_from_fiber->y == 77LL, "y field intact after fiber exit");
    ASSERT(nova_gc_alloc_count() >= 1, "GC tracked the allocation from inside fiber");

    nova_release(_point_from_fiber);
}

/* ---- main ---- */

int main(void) {
    printf("nova fiber deep tests\n");
    printf("=====================\n");

    nova_gc_init();

    test_separate_stack();
    test_yield_resume_order();
    test_deep_stack();
    test_captured_ptr_across_yield();
    test_two_fibers_interleaved();
    test_caller_stack_intact();
    test_nova_fiber_run();
    test_many_sequential_fibers();
    test_fiber_state_machine();
    test_gc_alloc_inside_fiber();

    nova_gc_shutdown();

    printf("\n=====================\n");
    printf("%d/%d passed\n", _total - _failed, _total);
    return _failed > 0 ? 1 : 0;
}
