#ifndef NOVA_RT_EFFECTS_H
#define NOVA_RT_EFFECTS_H

#include "nova_rt.h"
#include <setjmp.h>

/* ---- Fail effect — setjmp/longjmp based ---- *
 *
 * Nova `?` operator propagates a Fail upward.
 * Implementation: each `with Fail` (or function with Fail in signature)
 * pushes a jmp_buf onto a thread-local stack. `?` does longjmp to the
 * nearest handler.
 *
 * Generated code pattern for  fn f() Fail -> T:
 *
 *   NovaFailFrame _frame;
 *   nova_fail_push(&_frame);
 *   if (setjmp(_frame.jmp) != 0) {
 *       nova_fail_pop();
 *       return nova_fail_propagate();  // re-throw upward
 *   }
 *   ... body ...
 *   nova_fail_pop();
 */

typedef struct NovaFailFrame {
    jmp_buf            jmp;
    nova_str           error_msg;   /* payload on throw */
    struct NovaFailFrame* prev;
} NovaFailFrame;

/* Thread-local fail stack */
#ifdef _MSC_VER
__declspec(thread) extern NovaFailFrame* _nova_fail_top;
#else
extern __thread NovaFailFrame* _nova_fail_top;
#endif

static inline void nova_fail_push(NovaFailFrame* f) {
    f->prev = _nova_fail_top;
    _nova_fail_top = f;
}

static inline void nova_fail_pop(void) {
    if (_nova_fail_top) _nova_fail_top = _nova_fail_top->prev;
}

/* Throw: store error, longjmp to nearest handler */
static inline void nova_throw(nova_str msg) {
    if (_nova_fail_top) {
        _nova_fail_top->error_msg = msg;
        longjmp(_nova_fail_top->jmp, 1);
    }
    /* No handler: abort */
    fprintf(stderr, "nova: unhandled Fail: %.*s\n",
        (int)msg.len, msg.ptr);
    abort();
}

#define NOVA_TRY(frame)   (nova_fail_push(&(frame)), setjmp((frame).jmp) == 0)
#define NOVA_CATCH(frame) (nova_fail_pop(), (frame).error_msg)
#define NOVA_THROW(msg)   nova_throw(nova_str_from_cstr(msg))

/* `?` operator stub — in generated code:
 *   result = expr_that_might_throw();
 *   (expr itself calls nova_throw if it fails, so ? is a no-op at call site)
 */

/* ---- Interrupt / with-block early exit ----
 *
 * `interrupt v` inside a handler method exits the enclosing `with` block
 * early, making the `with` expression evaluate to `v`.
 *
 * Implementation: each `with` block pushes a NovaInterruptFrame on a
 * thread-local stack. `interrupt v` stores v in the frame and longjmps.
 *
 * Generated pattern for  `let r = with Eff = h { body }`:
 *
 *   NovaInterruptFrame _iframe;
 *   nova_int _with_result;
 *   nova_interrupt_push(&_iframe);
 *   if (setjmp(_iframe.jmp) == 0) {
 *       ... install handler ...
 *       { body }
 *       ... restore handler ...
 *       _with_result = <body-value>;
 *   } else {
 *       ... restore handler ...
 *       _with_result = _iframe.value;
 *   }
 *   nova_interrupt_pop();
 */

typedef struct NovaInterruptFrame {
    jmp_buf jmp;
    nova_int value;
    struct NovaInterruptFrame* prev;
} NovaInterruptFrame;

#ifdef _MSC_VER
__declspec(thread) extern NovaInterruptFrame* _nova_interrupt_top;
#else
extern __thread NovaInterruptFrame* _nova_interrupt_top;
#endif

static inline void nova_interrupt_push(NovaInterruptFrame* f) {
    f->prev = _nova_interrupt_top;
    _nova_interrupt_top = f;
}

static inline void nova_interrupt_pop(void) {
    if (_nova_interrupt_top) _nova_interrupt_top = _nova_interrupt_top->prev;
}

/* nova_interrupt forward-declared here as a real C-function; defined in
 * fibers.h after NovaFiberQueue is complete (needs _nova_active_scope and
 * fiber error machinery for the cross-mco-boundary case). */
void nova_interrupt(nova_int value);

/* ---- Test support ---- *
 *
 * Each test block runs inside a setjmp frame. If nova_assert() fails,
 * it longjmps back to the test runner with the failed expression string.
 *
 * Generated code pattern for  test "name" { body }:
 *
 *   static void nova_test_name_impl(void) {
 *       body
 *   }
 *
 *   // In runner:
 *   NovaTestFrame _tf;
 *   _nova_test_frame = &_tf;
 *   if (setjmp(_tf.jmp) == 0) {
 *       nova_test_name_impl();
 *       printf("  PASS: name\n");
 *   } else {
 *       printf("  FAIL: name — %s\n", _tf.fail_msg);
 *       _nova_tests_failed++;
 *   }
 *   _nova_test_frame = NULL;
 */

typedef struct NovaTestFrame {
    jmp_buf jmp;
    const char* fail_msg;
} NovaTestFrame;

#ifdef _MSC_VER
__declspec(thread) extern NovaTestFrame* _nova_test_frame;
#else
extern __thread NovaTestFrame* _nova_test_frame;
#endif

/* Forward decl: defined later in nova_rt.h once mco is included.
 * We test "are we inside a fiber" to decide where assertion failure lands. */
int nova_in_fiber(void);

static inline void nova_assert(nova_bool cond, const char* expr_str) {
    if (!cond) {
        /* Inside a fiber: route through the nearest NovaFailFrame so longjmp
         * stays on the fiber's own stack — never crosses the mco boundary.
         * Spawn-entry pushes a per-fiber fail-frame; supervised_run re-throws
         * on main flow via nova_throw, which the test runner's _tf_fail catches.
         * On main flow (no fiber): route to _nova_test_frame as before. */
        if (nova_in_fiber() && _nova_fail_top) {
            _nova_fail_top->error_msg = nova_str_from_cstr(expr_str);
            longjmp(_nova_fail_top->jmp, 1);
        }
        if (_nova_test_frame) {
            _nova_test_frame->fail_msg = expr_str;
            longjmp(_nova_test_frame->jmp, 1);
        }
        fprintf(stderr, "assertion failed: %s\n", expr_str);
        abort();
    }
}

/* nv_panic(msg) — D13: смерть текущего fiber'а.
 *
 * Routing идентичен nova_assert (общая семантика «runtime termination»):
 * внутри fiber'а — longjmp до ближайшего NovaFailFrame (остаётся на
 * stack'е fiber'а, не пересекает mco-boundary); на main flow с тест-
 * frame'ом — longjmp в тест-runner с сообщением; иначе — stderr + abort.
 *
 * `nv_panic` не возвращается (тип Never в Nova). C-сигнатура void,
 * потому что longjmp/abort не возвращаются по определению.
 *
 * См. spec/decisions/08-runtime.md → D13 (panic — fiber-уровень). */
static inline void nv_panic(nova_str msg) {
    if (nova_in_fiber() && _nova_fail_top) {
        _nova_fail_top->error_msg = msg;
        longjmp(_nova_fail_top->jmp, 1);
    }
    if (_nova_test_frame) {
        /* Аллоцируем буфер, чтобы сообщение пережило stack frame caller'а.
         * msg.ptr может указывать на stack-temporary (literal в test-функции). */
        char* buf = (char*)nova_alloc(msg.len + 8);
        memcpy(buf, "panic: ", 7);
        if (msg.len > 0) memcpy(buf + 7, msg.ptr, msg.len);
        buf[msg.len + 7] = 0;
        _nova_test_frame->fail_msg = buf;
        longjmp(_nova_test_frame->jmp, 1);
    }
    fwrite("panic: ", 1, 7, stderr);
    if (msg.len > 0) fwrite(msg.ptr, 1, msg.len, stderr);
    fwrite("\n", 1, 1, stderr);
    abort();
}

/* nv_exit(code, msg) — D13: смерть всего процесса.
 *
 * exit это финальная точка — НЕ routes через fail-frame (handler-ом не
 * перехватывается). Не вызывает defer'ы / destructor'ы / handler'ы:
 * процесс гасится с указанным exit code, стек не разворачивается
 * (как C exit, Go os.Exit, Rust std::process::exit).
 *
 * Исключение — тесты: в тест-frame'е перехватываем через longjmp,
 * чтобы один exit не убил всю прогонку. Это деталь test-runner'а,
 * не часть языкового контракта.
 *
 * `nv_exit` не возвращается (тип Never в Nova).
 *
 * См. spec/decisions/08-runtime.md → D13 (exit — process-уровень). */
static inline void nv_exit(nova_int code, nova_str msg) {
    if (_nova_test_frame) {
        /* Format: "exit(N): msg" — аллоцируем достаточный буфер.
         * 32 байт хватит на "exit(<int64>): " + null. */
        size_t cap = msg.len + 32;
        char* buf = (char*)nova_alloc(cap);
        int written = snprintf(buf, cap, "exit(%lld): %.*s",
                               (long long)code, (int)msg.len,
                               msg.len > 0 ? msg.ptr : "");
        if (written < 0) buf[0] = 0;
        _nova_test_frame->fail_msg = buf;
        longjmp(_nova_test_frame->jmp, 1);
    }
    /* Production-runtime: msg в stderr (если непустой) + exit(code). */
    if (msg.len > 0) {
        fwrite(msg.ptr, 1, msg.len, stderr);
        fwrite("\n", 1, 1, stderr);
    }
    exit((int)code);
}

/* ---- Generic effect handler vtable ---- *
 *
 * Each effect type is represented as a pointer to a struct of function
 * pointers (vtable). The `with Effect = handler { ... }` block installs
 * the vtable in a thread-local slot, then restores the previous one on exit.
 *
 * Generated code pattern:
 *
 *   // Effect vtable struct (generated once per effect type):
 *   typedef struct { nova_int (*next)(void* ctx); } NovaVtable_Counter;
 *
 *   // Thread-local current handler slot:
 *   __declspec(thread) NovaVtable_Counter* _nova_handler_Counter;
 *   __declspec(thread) void*               _nova_ctx_Counter;
 *
 *   // with Counter = h { body }  →
 *   NovaVtable_Counter* _prev_Counter = _nova_handler_Counter;
 *   void*               _prev_ctx     = _nova_ctx_Counter;
 *   _nova_handler_Counter = &h_vtable;
 *   _nova_ctx_Counter     = &h_state;
 *   { body }
 *   _nova_handler_Counter = _prev_Counter;
 *   _nova_ctx_Counter     = _prev_ctx;
 *
 *   // Counter.next()  →
 *   _nova_handler_Counter->next(_nova_ctx_Counter)
 */

/* ---- Built-in `Fail` effect (D25 / D62 / D65) ----
 *
 * `throw expr` desugars to `Fail.fail(expr)`. Same dispatch path as any
 * other effect operation — D62: «Никакой отдельной логики для throw нет;
 * та же проверка, что для Db.query, Net.get, Time.now».
 *
 * Vtable layout matches the codegen-generated layout for user-defined
 * effects (emit_effect_type): first field is `void* ctx`, then one
 * function pointer per method. Each method takes `void* _ctx` as the
 * first parameter.
 *
 * Default handler: NULL → Nova_Fail_fail dispatcher falls back to
 * nova_throw (longjmp to nearest fail-frame; abort with message if none).
 *
 * User override: `with Fail = (msg) => handler_body { body }` — D31
 * single-op handler-lambda sugar. Works automatically because Fail is
 * a regular effect.
 */
typedef struct {
    void*     ctx;
    nova_unit (*fail)(void* _ctx, nova_str msg);
} NovaVtable_Fail;

#ifdef _MSC_VER
__declspec(thread) extern NovaVtable_Fail* _nova_handler_Fail;
#else
extern __thread NovaVtable_Fail* _nova_handler_Fail;
#endif

/* Inline dispatch: Nova_Fail_fail(msg). Codegen emits this from
 * Stmt::Throw. With user handler installed → handler runs (e.g. records
 * the error in captured state), THEN we longjmp to the nearest fail-frame
 * — Fail-strict semantics (D65): fail() never resumes the caller.
 * Without handler → nova_throw directly (longjmp to fail-frame; abort
 * with message if no frame). */
static inline nova_unit Nova_Fail_fail(nova_str msg) {
    if (_nova_handler_Fail) {
        _nova_handler_Fail->fail(_nova_handler_Fail->ctx, msg);
        /* Handler returned — by D65 Fail-strict, fail() is `Never` from the
         * caller's perspective. Force unwind to the nearest fail-frame so
         * caller code after the throw doesn't execute. */
    }
    nova_throw(msg);
    return NOVA_UNIT;  /* unreachable */
}

/* ---- Built-in `Time` effect (D11 / D14 / D62) ----
 *
 * Operations: now() -> int, sleep(ms int) -> unit. By D11 — это обычный
 * stdlib-эффект. По D62 — Async ambient: Time-операции callable откуда
 * угодно, в сигнатуре не требуется, default handler доступен.
 *
 * Default handler (см. fibers.h):
 *   sleep(ms) — context-sensitive: в fiber'е yield-loop до deadline;
 *               на main внутри supervised — drain queue per pass;
 *               на top-level (нет scope) — native OS sleep.
 *               ms <= 0 → один yield (compatibility с `Time.sleep(0)`).
 *   now()     — monotonic ms (GetTickCount64 на Win, clock_gettime на POSIX).
 *
 * User override: `with Time = handler Time { sleep(ms) { ... } now() { ... } } { body }`
 * — для тестов (fixed clock, mock sleep). */

/* Layout matches codegen-generated layout for user effects. */
typedef struct {
    void*     ctx;
    nova_unit (*sleep)(void* _ctx, nova_int ms);
    nova_int  (*now)(void* _ctx);
} NovaVtable_Time;

#ifdef _MSC_VER
__declspec(thread) extern NovaVtable_Time* _nova_handler_Time;
#else
extern __thread NovaVtable_Time* _nova_handler_Time;
#endif

/* Nova_Time_sleep / Nova_Time_now defined in fibers.h (after NovaFiberQueue
 * complete + nova_fiber_yield + nova_supervised_step). They are not
 * forward-declared here because callers always include nova_rt.h which pulls
 * in fibers.h after effects.h. */

/* ---- Per-fiber handler scoping (D-handler-scope) ---- *
 *
 * Все `_nova_handler_X` — `__declspec(thread)` глобалы, по факту делящиеся
 * между fiber'ами на одном OS-thread (D71 single-threaded cooperative).
 * Если fiber A делает `with X = ...`, yield'ит, а fiber B перезаписывает
 * глобал, A после resume увидит handler от B — undefined behavior.
 *
 * Решение: handler-storage registry + per-fiber snapshot.
 *
 * Каждый `_nova_handler_X` (как Fail, Time, и user-defined) **регистрируется**
 * через nova_register_effect_storage(&_nova_handler_X) при инициализации
 * программы. Получается список адресов всех handler-pointers (TLS-адресов).
 *
 * При `nova_supervised_step` (resume fiber'а из scheduler'а):
 *   1. Save current globals in `prev_snapshot` (на стеке scheduler'а).
 *   2. Restore fiber's saved snapshot in globals (если fiber suspended).
 *   3. mco_resume.
 *   4. После return: save globals back в fiber's snapshot.
 *   5. Restore prev_snapshot in globals.
 *
 * Limit: 32 effect-storages — достаточно для bootstrap'а (built-in 3 +
 * user-defined обычно <10). Production-runtime — динамический rezize.
 */

#define NOVA_MAX_EFFECT_STORAGES 32

typedef struct {
    void** slots[NOVA_MAX_EFFECT_STORAGES];   /* registered TLS addresses */
    int    count;
} NovaEffectRegistry;

extern NovaEffectRegistry _nova_effect_registry;

/* Регистрация handler-storage. Idempotent (по адресу). Вызывается из
 * codegen'а при первом использовании эффекта (или статически перед main). */
static inline void nova_register_effect_storage(void** slot_addr) {
    for (int i = 0; i < _nova_effect_registry.count; i++) {
        if (_nova_effect_registry.slots[i] == slot_addr) return;
    }
    if (_nova_effect_registry.count < NOVA_MAX_EFFECT_STORAGES) {
        _nova_effect_registry.slots[_nova_effect_registry.count++] = slot_addr;
    }
    /* Silent overflow: бутстрап не дотянется до 32 эффектов. Production
     * должен использовать dynamic-resize либо assert. */
}

/* Snapshot — массив значений pointer-ов. Размер фиксированный, индексы
 * совпадают с registry.slots. Хранится per-fiber. */
typedef struct {
    void* values[NOVA_MAX_EFFECT_STORAGES];
} NovaEffectSnapshot;

/* Save current TLS values → snapshot. */
static inline void nova_effect_snapshot_save(NovaEffectSnapshot* snap) {
    for (int i = 0; i < _nova_effect_registry.count; i++) {
        snap->values[i] = *_nova_effect_registry.slots[i];
    }
}

/* Restore snapshot → TLS. */
static inline void nova_effect_snapshot_restore(const NovaEffectSnapshot* snap) {
    for (int i = 0; i < _nova_effect_registry.count; i++) {
        *_nova_effect_registry.slots[i] = snap->values[i];
    }
}

/* ---- Built-in `Mem` effect — runtime introspection for leak/growth tests ----
 *
 * Operations:
 *   alloc_count() -> int : total nova_alloc since gc_init/reset_stats
 *   free_count()  -> int : total frees (plain malloc backend → 0)
 *   live()        -> int : alloc_count - free_count
 *   reset()       -> ()  : zero stats counters (per-test isolation)
 *
 * No handler vtable: these are direct runtime calls. Used by Nova test code
 * to assert that hot loops don't blow up allocation counters. Numbers are
 * counts (not bytes) — sufficient for catching regressions where one alloc
 * per iteration becomes ten. */
static inline nova_int Nova_Mem_alloc_count(void) {
    return (nova_int)nova_gc_alloc_count();
}
static inline nova_int Nova_Mem_free_count(void) {
    return (nova_int)nova_gc_free_count();
}
static inline nova_int Nova_Mem_live(void) {
    return (nova_int)nova_gc_live_count();
}
static inline nova_unit Nova_Mem_reset(void) {
    nova_gc_reset_stats();
    return NOVA_UNIT;
}

#endif /* NOVA_RT_EFFECTS_H */
