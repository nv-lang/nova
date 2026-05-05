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

static inline void nova_interrupt(nova_int value) {
    if (_nova_interrupt_top) {
        _nova_interrupt_top->value = value;
        longjmp(_nova_interrupt_top->jmp, 1);
    }
    /* No with-block: interrupt is a no-op (body already exited) */
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

#endif /* NOVA_RT_EFFECTS_H */
