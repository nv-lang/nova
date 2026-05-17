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

/* Plan 49 Ф.0: kinded throws — у throw'а есть «вид», переживающий longjmp.
 * USER = обычная пользовательская ошибка (throw, ?, !!, assert);
 * CANCEL = кооперативная отмена scope'а — не «убегает» наружу.
 * См. supervised_run + emit_with kind-aware dispatch (Ф.3). */
typedef enum {
    NOVA_THROW_USER       = 0,
    NOVA_THROW_CANCEL     = 1,
    NOVA_THROW_USER_TYPED = 2,  /* Plan 61 Ф.2: typed user throw payload */
} NovaThrowKind;

typedef struct NovaFailFrame {
    jmp_buf            jmp;
    nova_str           error_msg;
    NovaThrowKind      error_kind;
    void*              error_reason_ptr;   /* Plan 49 typed cancel */
    void*              error_user_payload; /* Plan 61 Ф.2 typed user payload */
    NovaTypeId         error_user_type_id; /* Plan 61 Ф.2 NovaTypeId of payload */
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

/* Throw: store error, longjmp to nearest handler.
 * Plan 49 Ф.0: stamp kind=USER, reason=NULL (default — обычная ошибка). */
static inline void nova_throw(nova_str msg) {
    if (_nova_fail_top) {
        _nova_fail_top->error_msg = msg;
        _nova_fail_top->error_kind = NOVA_THROW_USER;
        _nova_fail_top->error_reason_ptr = NULL;
        longjmp(_nova_fail_top->jmp, 1);
    }
    /* No handler: abort. Plan 20 Ф.8 follow-up: flush stdout перед
     * abort'ом, чтобы defer cleanup print'ы (буферизованные) попали
     * в output. Без этого defer-cleanup print видно в exit-code, но
     * не в stdout (теряется при abort). */
    fflush(stdout);
    fprintf(stderr, "nova: unhandled Fail: %.*s\n",
        (int)msg.len, msg.ptr);
    abort();
}

/* Plan 49 Ф.0: cancel-throw — kind=CANCEL, reason=NULL (Ф.1 заполняет
 * caller через _reason вариант). Без активного handler'а отмена бесполезна
 * (некому её перехватить) — abort с диагностикой. */
static inline void nova_throw_cancel(nova_str msg) {
    if (_nova_fail_top) {
        _nova_fail_top->error_msg = msg;
        _nova_fail_top->error_kind = NOVA_THROW_CANCEL;
        _nova_fail_top->error_reason_ptr = NULL;
        longjmp(_nova_fail_top->jmp, 1);
    }
    fflush(stdout);
    fprintf(stderr, "nova: cancel-throw outside any supervised scope: %.*s\n",
        (int)msg.len, msg.ptr);
    abort();
}

/* Plan 49 Ф.1: cancel-throw с типизированной причиной. `reason_ptr` —
 * box'нутый `T` (caller-owned, переживает scope). Для CancelToken[str]
 * указывает на nova_str; для CancelToken[T] (Ф.6) — на box'нутый T. */
static inline void nova_throw_cancel_reason(nova_str msg, void* reason_ptr) {
    if (_nova_fail_top) {
        _nova_fail_top->error_msg = msg;
        _nova_fail_top->error_kind = NOVA_THROW_CANCEL;
        _nova_fail_top->error_reason_ptr = reason_ptr;
        longjmp(_nova_fail_top->jmp, 1);
    }
    fflush(stdout);
    fprintf(stderr, "nova: cancel-throw outside any supervised scope: %.*s\n",
        (int)msg.len, msg.ptr);
    abort();
}

#define NOVA_TRY(frame)   (nova_fail_push(&(frame)), setjmp((frame).jmp) == 0)
#define NOVA_CATCH(frame) (nova_fail_pop(), (frame).error_msg)
/* Plan 49 Ф.0: kind/reason accessors — read AFTER setjmp returned non-zero. */
#define NOVA_CATCH_KIND(frame)   ((frame).error_kind)
#define NOVA_CATCH_REASON(frame) ((frame).error_reason_ptr)
#define NOVA_THROW(msg)   nova_throw(nova_str_from_cstr(msg))

/* Plan 19, C7 (D85): postfix `!!` runtime helpers.
 *
 * `expr!!` на None бросает RuntimeNoneError (D85 prelude unit-тип,
 * фиксированное сообщение).
 */
static inline void nova_throw_runtime_none_error(void) {
    nova_throw(nova_str_from_cstr("RuntimeNoneError"));
}

/* Plan 19, C7 (D85): `expr!!` на Err(e) — бросает значение `e` через
 * Fail-эффект. Для bootstrap'а: если `e` — record `Error { msg str }`,
 * извлекаем msg; иначе — generic placeholder. В production-runtime
 * это будет typed throw через ErrorBox с runtime-type-info, но
 * bootstrap довольствуется string-based throw. Конкретный generated
 * C-код для `Err(e)!!` приводит сам к нужному типу: `Err(Error{...})`
 * передаётся через типизированный `nova_throw_str(e.msg)`.
 *
 * Generic helper для не-string Err — fallback к фиксированной строке.
 */
static inline void nova_throw_str(nova_str msg) {
    nova_throw(msg);
}

/* Plan 61 Ф.4: nova_throw_value placeholder УДАЛЁН. Codegen Result!!
 * теперь emit'тся либо как Nova_Fail_fail (bootstrap-erased Result где Err
 * = nova_str), либо как nova_throw_typed (после Plan 14/56 generic Result
 * mono'd). См. emit_c.rs ExprKind::Bang Nova_Result* branch.
 *
 * Если какой-то downstream код всё ещё ссылается на nova_throw_value —
 * это bug, должен быть переписан на nova_throw_typed (typed) или
 * Nova_Fail_fail (string). */

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

/* Plan 39 Issue A: NovaInterruptFrame теперь хранит ДВА слота value —
 * `value` (nova_int / nova_bool / value-types, помещающихся в i64) и
 * `value_ptr` (void*, для pointer-types и heap-allocated value structs).
 *
 * Codegen выбирает слот по типу trail/interrupt expression:
 *   - int/bool/inline scalars → nova_interrupt(int_value) → slot value
 *   - pointer types (Nova_X*, NovaArray_X*) → nova_interrupt_ptr(p) → value_ptr
 *   - value structs (NovaOpt_X, NovaResult_X_E, etc.) → heap-allocate,
 *     передать pointer через nova_interrupt_ptr; reader разыменует.
 *
 * При normal-flow completion body эмиттер пишет ОДИН из слотов по типу;
 * при interrupt-path — читает тот же слот.
 *
 * Mutually-exclusive: только один путь активен в любой `with`-блок.
 * `value` и `value_ptr` независимые поля — codegen знает какое читать. */
/* Plan 61 followup #1: iframe kind. WITHBLOCK = default (`with X = h { ... }`
 * frame, terminal target для interrupt). DEFER_SCOPE = transparent frame
 * pushed defer codegen — intercepts interrupt, runs cleanup, re-issues.
 * Используется nova_interrupt для cross-effect throw routing: skip
 * intermediate with-block frames до owner, BUT preserve defer frames в
 * cleanup chain. */
#define NOVA_IFRAME_WITHBLOCK    0
#define NOVA_IFRAME_DEFER_SCOPE  1

typedef struct NovaInterruptFrame {
    jmp_buf jmp;
    nova_int value;
    void*    value_ptr;        /* Plan 39 Issue A: non-int / non-bool результат */
    int      kind;             /* Plan 61 fu#1: NOVA_IFRAME_* */
    struct NovaInterruptFrame* prev;
} NovaInterruptFrame;

#ifdef _MSC_VER
__declspec(thread) extern NovaInterruptFrame* _nova_interrupt_top;
/* Plan 61 followup #1: handler-arm interrupt context. Set ДО invoke
 * handler-arm body в Nova_Fail_fail / nova_throw_typed; restored после.
 * `interrupt v` в handler-arm body использует этот slot вместо
 * _nova_interrupt_top — иначе cross-effect throw в handler-arm
 * (outer Fail handler делает `interrupt v`) jump'тся в inner with-block
 * вместо outer's. См. simplifications [M-plan-61-cross-effect-throw]
 * resolution. */
__declspec(thread) extern NovaInterruptFrame* _nova_current_handler_iframe;
#else
extern __thread NovaInterruptFrame* _nova_interrupt_top;
extern __thread NovaInterruptFrame* _nova_current_handler_iframe;
#endif

static inline void nova_interrupt_push(NovaInterruptFrame* f) {
    /* Default kind = WITHBLOCK. Caller can override (см.
     * nova_interrupt_push_defer для defer scopes). */
    f->kind = NOVA_IFRAME_WITHBLOCK;
    f->prev = _nova_interrupt_top;
    _nova_interrupt_top = f;
}

/* Plan 61 followup #1: defer-scope push — sets kind=DEFER_SCOPE так что
 * nova_interrupt при cross-effect routing preserves defer cleanup chain. */
static inline void nova_interrupt_push_defer(NovaInterruptFrame* f) {
    f->kind = NOVA_IFRAME_DEFER_SCOPE;
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

/* Plan 39 Issue A: pointer-variant interrupt. Hands pointer/value-struct-ptr
 * to the `with`-block result slot. See NovaInterruptFrame.value_ptr. */
void nova_interrupt_ptr(void* value);

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
 * Routing: fail-frame первым (не зависит от fiber-контекста — defer/errdefer
 * на main flow тоже должны отработать); затем тест-frame; иначе stderr + abort.
 *
 * Ранее был guard `nova_in_fiber()` перед fail-frame — это не позволяло
 * errdefer'ам срабатывать на panic() на main flow. Теперь симметрично
 * с nova_throw: fail-frame проверяется первым всегда.
 *
 * `nv_panic` не возвращается (тип Never в Nova). C-сигнатура void,
 * потому что longjmp/abort не возвращаются по определению.
 *
 * См. spec/decisions/08-runtime.md → D13 (panic — fiber-уровень). */
static inline void nv_panic(nova_str msg) {
    if (_nova_fail_top) {
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
 *
 * Plan 20 Ф.8 (4): D65 правило 3 «re-throw skip current frame». Когда
 * `throw err` происходит ВНУТРИ handler-body, runtime должен dispatch'нуться
 * на OUTER handler (skip current — иначе infinite recursion). Поле `prev`
 * хранит outer handler на момент install'а — Nova_Fail_fail на время
 * invocation handler-body временно swap'ает _nova_handler_Fail = prev,
 * восстанавливает после. Codegen emit_with инициализирует vtable->prev
 * перед install'ом.
 */
typedef struct NovaVtable_Fail {
    void*                       ctx;
    nova_unit                  (*fail)(void* _ctx, nova_str msg);
    struct NovaVtable_Fail*      prev;          /* outer handler, для D65 re-throw */
    /* Plan 61 followup #1: pointer to with-block's NovaInterruptFrame —
     * для cross-effect throw. nova_interrupt в handler-arm body использует
     * этот frame вместо _nova_interrupt_top. NULL для legacy handlers что
     * не нуждаются. emit_with инициализирует через `vt->owner_iframe = &iframe`. */
    struct NovaInterruptFrame*   owner_iframe;
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
 * with message if no frame).
 *
 * Plan 20 Ф.8 (4): на время handler.fail invocation временно ставим
 * _nova_handler_Fail = current->prev — если в handler-body встретится
 * `throw err`, он dispatch'ится на outer handler (D65 правило 3).
 * Восстанавливаем после return. */
static inline nova_unit Nova_Fail_fail(nova_str msg) {
    if (_nova_handler_Fail) {
        NovaVtable_Fail* current = _nova_handler_Fail;
        NovaInterruptFrame* saved_handler_iframe = _nova_current_handler_iframe;
        _nova_handler_Fail = current->prev;        /* swap для re-throw */
        /* Plan 61 followup #1: handler-arm `interrupt v` использует этот
         * slot вместо _nova_interrupt_top. Critical для cross-effect throw. */
        _nova_current_handler_iframe = current->owner_iframe;
        current->fail(current->ctx, msg);
        _nova_handler_Fail = current;              /* restore handler chain */
        _nova_current_handler_iframe = saved_handler_iframe;
        /* Handler returned — by D65 Fail-strict, fail() is `Never` from the
         * caller's perspective. Force unwind to the nearest fail-frame so
         * caller code after the throw doesn't execute. */
    }
    nova_throw(msg);
    return NOVA_UNIT;  /* unreachable */
}

/* ---- Plan 61 Ф.2: Fail[any] typed erased path ---- *
 *
 * Параллельная инфраструктура к string-based Nova_Fail_fail/_nova_handler_Fail.
 *
 * vtable carries `void* err + NovaTypeId tid` instead of nova_str. Handler arm
 * для `with Fail = |e: any| ...` (без `[E]`, D65 правило 1) installs в этот
 * slot вместо string-slot.
 *
 * Dispatch precedence (для `throw expr` codegen):
 *   1. per-E typed slot (Plan 61 Ф.3 — будет добавлен следующей фазой)
 *   2. erased typed slot (`_nova_handler_Fail_any`) — этот файл
 *   3. legacy string slot (`_nova_handler_Fail`) — backward compat
 *   4. unwind через nova_throw_typed → longjmp на fail-frame.
 *
 * D65 правило 3 re-throw — тот же mechanism: `prev` link swap во время
 * handler-body invocation. */
typedef struct NovaVtable_Fail_any {
    void*                            ctx;
    nova_unit                       (*fail)(void* _ctx, void* err, NovaTypeId tid);
    struct NovaVtable_Fail_any*       prev;
    /* Plan 61 followup #1: same как у NovaVtable_Fail — для cross-effect
     * throw interrupt routing. */
    struct NovaInterruptFrame*        owner_iframe;
} NovaVtable_Fail_any;

#ifdef _MSC_VER
__declspec(thread) extern NovaVtable_Fail_any* _nova_handler_Fail_any;
#else
extern __thread NovaVtable_Fail_any* _nova_handler_Fail_any;
#endif

/* Plan 61 Ф.2: typed throw — фолдит на правильный slot по dispatch
 * precedence. Codegen emits this для `throw expr` где expr type != nova_str
 * И outer context — generic Fail или Fail[any] (Fail[E] per-E пойдёт через
 * Ф.3 dispatcher). `payload` указывает на value (caller-allocated, обычно
 * heap-boxed на throw-site), `tid` — compile-time NOVA_TID_<E>, `msg_repr`
 * — fallback string-репрезентация для diagnostic и string-only handler. */
static inline nova_unit nova_throw_typed(nova_str msg_repr,
                                          void* payload,
                                          NovaTypeId tid) {
    /* Plan 61 Ф.3 fix: set fail-frame payload ДО любого handler dispatch.
     * Handler arm (typed via fail_e_map) читает `e` через
     * _nova_fail_top->error_user_payload — payload должен быть доступен
     * к моменту invoke. Это OK даже без unwind: handler-arm body — это
     * inline fn-call с captured pointer to fail-frame top. */
    if (_nova_fail_top) {
        _nova_fail_top->error_msg          = msg_repr;
        _nova_fail_top->error_kind         = NOVA_THROW_USER_TYPED;
        _nova_fail_top->error_reason_ptr   = NULL;
        _nova_fail_top->error_user_payload = payload;
        _nova_fail_top->error_user_type_id = tid;
    }
    /* Step 2: erased typed slot. */
    if (_nova_handler_Fail_any) {
        NovaVtable_Fail_any* current = _nova_handler_Fail_any;
        NovaInterruptFrame* saved_iframe = _nova_current_handler_iframe;
        _nova_handler_Fail_any = current->prev;
        _nova_current_handler_iframe = current->owner_iframe;  /* Plan 61 fu#1 */
        current->fail(current->ctx, payload, tid);
        _nova_handler_Fail_any = current;
        _nova_current_handler_iframe = saved_iframe;
        /* Handler returned normally → Fail-strict (D65): force unwind. */
    }
    /* Step 3: legacy string slot — handler arm может быть typed (читает
     * payload через fail-frame) или string-based (читает msg). Оба работают:
     * payload уже в frame (выше). */
    if (_nova_handler_Fail) {
        NovaVtable_Fail* current = _nova_handler_Fail;
        NovaInterruptFrame* saved_iframe = _nova_current_handler_iframe;
        _nova_handler_Fail = current->prev;
        _nova_current_handler_iframe = current->owner_iframe;  /* Plan 61 fu#1 */
        current->fail(current->ctx, msg_repr);
        _nova_handler_Fail = current;
        _nova_current_handler_iframe = saved_iframe;
    }
    /* Step 4: unwind. fail-frame уже заполнен наверху. */
    if (_nova_fail_top) {
        longjmp(_nova_fail_top->jmp, 1);
    }
    /* No fail-frame at all — abort с diagnostic. */
    fflush(stdout);
    fprintf(stderr, "nova: unhandled typed Fail (%s): %.*s\n",
        nova_typeid_to_name(tid),
        (int)msg_repr.len, msg_repr.ptr);
    abort();
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

/* Layout matches codegen-generated layout for user effects.
 *
 * Plan 48 Ф.5: now_ms / now_ns добавлены чтобы handlers.nv (fixed_ms,
 * mut_clock — std/testing/handlers.nv:171-201) могли регистрировать
 * полный набор Time-методов. Default-импл (Nova_Time_now_ms / _now_ns)
 * — wrapper'ы вокруг now() (которая возвращает monotonic ms). Field
 * order MUST совпадать с codegen-emitted layout: ctx, sleep, now,
 * now_ms, now_ns (см. emit_handler_decl / fixed_ms vtable init). */
typedef struct {
    void*     ctx;
    nova_unit (*sleep)(void* _ctx, nova_int ms);
    nova_int  (*now)(void* _ctx);
    nova_int  (*now_ms)(void* _ctx);
    nova_int  (*now_ns)(void* _ctx);
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
