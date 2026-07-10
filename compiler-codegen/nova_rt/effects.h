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
    NOVA_THROW_PANIC      = 3,  /* Plan 110.1.4.g (D188): panic distinct
                                   из throw для ConsumeScope outcome
                                   discrimination (Panic vs Failure variant). */
} NovaThrowKind;

/* Plan 100.4.1 (D158): Suppressed-chain node для multi-error composition.
 * Cleanup-fail во время propagation НЕ заменяет primary error — он
 * appended в chain'е. Caller инспектирует через MultiError API
 * (.suppressed() returns chain в order firing). LIFO для multi-defer'ов.
 *
 * Storage: GC-managed (nova_alloc). Chain owned by NovaFailFrame's
 * error_suppressed head pointer; на frame pop chain переноситcя в
 * outer frame через nova_rethrow_with_suppressed. */
typedef struct NovaErrorChain {
    nova_str               msg;
    NovaThrowKind          kind;
    void*                  user_payload;
    NovaTypeId             user_type_id;
    struct NovaErrorChain* next;
} NovaErrorChain;

typedef struct NovaFailFrame {
    jmp_buf            jmp;
    nova_str           error_msg;
    NovaThrowKind      error_kind;
    void*              error_reason_ptr;   /* Plan 49 typed cancel */
    void*              error_user_payload; /* Plan 61 Ф.2 typed user payload */
    NovaTypeId         error_user_type_id; /* Plan 61 Ф.2 NovaTypeId of payload */
    NovaErrorChain*    error_suppressed;   /* Plan 100.4.1 D158 — head of suppressed chain (NULL = no chain) */
    /* Plan 173 Ф.4 #6 (D158 model B): 1 = per-cleanup frame wrapped around a
     * defer/consume-cleanup body running while ANOTHER error is unwinding
     * (FAIL / interrupt exit-paths). While the NEAREST frame is a cleanup
     * frame, throw dispatchers SKIP handler dispatch (Nova_Fail_fail /
     * nova_throw_typed / generated per-E entries) — a failing cleanup must
     * compose into the suppressed pocket, NOT hijack the scope's own
     * with-Fail handler (which would misfire a typed arm on a foreign payload
     * and overwrite the in-flight result). An explicit handler-wrap INSIDE
     * the cleanup body pushes its own non-cleanup frame → dispatch works
     * (D158 backward-compat silent-suppress idiom preserved). Normal-exit
     * cleanup frames are NOT marked (their failure = primary; handler fires). */
    int                is_cleanup;
    struct NovaFailFrame* prev;
} NovaFailFrame;

/* Thread-local fail stack */
#ifdef _MSC_VER
__declspec(thread) extern NovaFailFrame* _nova_fail_top;
#else
extern __thread NovaFailFrame* _nova_fail_top;
#endif

static inline void nova_fail_push(NovaFailFrame* f) {
    f->is_cleanup = 0;  /* Plan 173 Ф.4 #6: default; codegen marks cleanup frames */
    f->prev = _nova_fail_top;
    _nova_fail_top = f;
}

/* Plan 173 Ф.4 #6 (D158 model B): true while the nearest fail-frame is a
 * per-cleanup frame (unwind in progress) — throw dispatchers bypass handler
 * dispatch so the cleanup failure lands in that frame and composes into the
 * suppressed pocket instead of hijacking the scope's handler. */
static inline int nova_in_cleanup_unwind(void) {
    return _nova_fail_top && _nova_fail_top->is_cleanup;
}

static inline void nova_fail_pop(void) {
    if (_nova_fail_top) _nova_fail_top = _nova_fail_top->prev;
}

/* ──────────────────────────────────────────────────────────────────
 * Plan 173 Ф.4 #5 (D188/D190): thread-local STABLE error snapshot.
 * ──────────────────────────────────────────────────────────────────
 *
 * The per-throw error identity (msg/kind/typed-payload/tid) lives on a
 * `NovaFailFrame` that is a STACK local of the throwing function. When a
 * `with Fail = … interrupt` handler catches a throw, control unwinds via the
 * interrupt stack, which does NOT pop fail-frames — so `_nova_fail_top` may
 * still point at a frame whose stack storage the unwind already destroyed
 * (reading it segfaults once the throw came from a nested function).
 *
 * `_nova_last_error` snapshots those four stable, self-owned fields (the typed
 * payload is heap/GC-boxed at the throw site and outlives the stack frame; the
 * msg is a heap/static `nova_str`) into thread-local storage at throw-time, so
 * a defer/consume-cleanup unwound via `interrupt` can recover the ORIGINAL
 * typed error for `Failure(any)` → `if err is T`. `.live` gates the read:
 * set on every throw/panic, cleared when the error is caught (nova_scope_exit
 * CATCH) — a pure value-`interrupt` with no in-flight throw sees `.live == 0`
 * and falls back to the plain `"interrupt"` marker. */
typedef struct {
    int           live;   /* 1 while an error is propagating; 0 once caught */
    NovaFailFrame frame;  /* stable snapshot of the in-flight error */
} NovaLastError;

#ifdef _MSC_VER
__declspec(thread) extern NovaLastError _nova_last_error;
#else
extern __thread NovaLastError _nova_last_error;
#endif

static inline void nova_last_error_set(nova_str msg, NovaThrowKind kind,
                                       void* payload, NovaTypeId tid) {
    _nova_last_error.live               = 1;
    _nova_last_error.frame.error_msg           = msg;
    _nova_last_error.frame.error_kind          = kind;
    _nova_last_error.frame.error_reason_ptr    = NULL;
    _nova_last_error.frame.error_user_payload  = payload;
    _nova_last_error.frame.error_user_type_id  = tid;
    _nova_last_error.frame.error_suppressed    = NULL;
}

/* ──────────────────────────────────────────────────────────────────
 * Plan 173 Ф.5 п.7 (Zig-парность, минимум): throw-site трассировка.
 * ──────────────────────────────────────────────────────────────────
 *
 * Codegen стемпит `nova_throw_site_set("file.nv", line)` НЕПОСРЕДСТВЕННО
 * перед каждым user-`throw`/`panic()`/`unreachable()` (только на
 * error-path — happy-path не затронут). Все uncaught-abort-ветки
 * (unhandled Fail / composite / typed / panic) печатают
 * `  at <file>:<line> (throw site)` вслед за сообщением — debug-парность
 * Zig error-return-trace минимум (полный propagation-trace —
 * `[M-173-error-return-trace]`). assert/contract уже location-first
 * (D13 amend) — их не стемпим. */
typedef struct {
    const char* file;  /* NULL = сайт неизвестен (runtime-internal throw) */
    int         line;
} NovaThrowSite;

#ifdef _MSC_VER
__declspec(thread) extern NovaThrowSite _nova_throw_site;
#else
extern __thread NovaThrowSite _nova_throw_site;
#endif

static inline void nova_throw_site_set(const char* file, int line) {
    _nova_throw_site.file = file;
    _nova_throw_site.line = line;
}

/* Печать throw-site в uncaught-abort ветках (no-op если сайт неизвестен). */
static inline void nova_throw_site_dump(void) {
    if (_nova_throw_site.file) {
        fprintf(stderr, "  at %s:%d (throw site)\n",
                _nova_throw_site.file, _nova_throw_site.line);
    }
}

/* Throw: store error, longjmp to nearest handler.
 * Plan 49 Ф.0: stamp kind=USER, reason=NULL (default — обычная ошибка).
 * Plan 100.4.1 (D158): reset error_suppressed chain (fresh throw НЕ несёт
 * прошлые suppressed). Chain populated runtime'ом во время defer-cleanup
 * через nv_compose_suppressed; transferred к outer frame через
 * nova_rethrow_with_suppressed. */
static inline void nova_throw(nova_str msg) {
    nova_last_error_set(msg, NOVA_THROW_USER, NULL, NOVA_TID_NONE);  /* Ф.4 #5 */
    if (_nova_fail_top) {
        _nova_fail_top->error_msg = msg;
        _nova_fail_top->error_kind = NOVA_THROW_USER;
        _nova_fail_top->error_reason_ptr = NULL;
        _nova_fail_top->error_user_payload = NULL;
        _nova_fail_top->error_user_type_id = NOVA_TID_NONE;
        _nova_fail_top->error_suppressed = NULL;
        longjmp(_nova_fail_top->jmp, 1);
    }
    /* No handler: abort. Plan 20 Ф.8 follow-up: flush stdout перед
     * abort'ом, чтобы defer cleanup print'ы (буферизованные) попали
     * в output. Без этого defer-cleanup print видно в exit-code, но
     * не в stdout (теряется при abort). */
    fflush(stdout);
    fprintf(stderr, "nova: unhandled Fail: %.*s\n",
        (int)msg.len, msg.ptr);
    nova_throw_site_dump();  /* Plan 173 Ф.5 п.7 */
    abort();
}

/* Plan 49 Ф.0: cancel-throw — kind=CANCEL, reason=NULL (Ф.1 заполняет
 * caller через _reason вариант). Без активного handler'а отмена бесполезна
 * (некому её перехватить) — abort с диагностикой. */
static inline void nova_throw_cancel(nova_str msg) {
    nova_last_error_set(msg, NOVA_THROW_CANCEL, NULL, NOVA_TID_NONE);  /* Ф.4 #5 */
    if (_nova_fail_top) {
        _nova_fail_top->error_msg = msg;
        _nova_fail_top->error_kind = NOVA_THROW_CANCEL;
        _nova_fail_top->error_reason_ptr = NULL;
        _nova_fail_top->error_user_payload = NULL;
        _nova_fail_top->error_user_type_id = NOVA_TID_NONE;
        _nova_fail_top->error_suppressed = NULL;  /* D158 */
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
    nova_last_error_set(msg, NOVA_THROW_CANCEL, NULL, NOVA_TID_NONE);  /* Ф.4 #5 */
    if (_nova_fail_top) {
        _nova_fail_top->error_msg = msg;
        _nova_fail_top->error_kind = NOVA_THROW_CANCEL;
        _nova_fail_top->error_reason_ptr = reason_ptr;
        _nova_fail_top->error_user_payload = NULL;
        _nova_fail_top->error_user_type_id = NOVA_TID_NONE;
        _nova_fail_top->error_suppressed = NULL;  /* D158 */
        longjmp(_nova_fail_top->jmp, 1);
    }
    fflush(stdout);
    fprintf(stderr, "nova: cancel-throw outside any supervised scope: %.*s\n",
        (int)msg.len, msg.ptr);
    abort();
}

/* ---- Plan 100.4.1 (D158): failable cleanup body — multi-error composition ----
 *
 * `nv_compose_suppressed(primary_frame, ...)` — appends a secondary error
 * (the one thrown by failable defer/errdefer/okdefer body) к chain'у
 * primary_frame->error_suppressed. Allocated в GC heap (chain выживает
 * scope unwinding в outer frame через nova_rethrow_with_suppressed).
 *
 * Generated C pattern (emit_defer для failable body):
 *
 *   NovaFailFrame _defer_frame; nova_fail_push(&_defer_frame);
 *   _defer_frame.error_suppressed = NULL;
 *   if (setjmp(_defer_frame.jmp) == 0) {
 *       <defer body>
 *       nova_fail_pop();
 *   } else {
 *       nova_fail_pop();
 *       if (_unwinding && _nova_fail_top) {
 *           nv_compose_suppressed(_nova_fail_top,
 *                                  _defer_frame.error_msg,
 *                                  _defer_frame.error_kind,
 *                                  _defer_frame.error_user_payload,
 *                                  _defer_frame.error_user_type_id);
 *       } else {
 *           nova_throw_typed(_defer_frame.error_msg,
 *                            _defer_frame.error_user_payload,
 *                            _defer_frame.error_user_type_id);
 *       }
 *   }
 *
 * `_unwinding` — codegen-emitted local флаг, set'нут когда fn's outer
 * fail-frame caught error и идёт cleanup-chain (LIFO defer execution).
 */
static inline void nv_compose_suppressed(NovaFailFrame* primary,
                                          nova_str msg,
                                          NovaThrowKind kind,
                                          void* user_payload,
                                          NovaTypeId tid) {
    if (!primary) return;
    /* Plan 110.4.2 (D193): cycle-safety + depth-limit.
     * - Cycle-safety: identity check на msg.ptr + kind + user_payload
     *   prevents self-suppression cycles (Java JDK-8287921 lesson).
     * - Depth-limit 256: after 256 nodes, dalee compose silently no-op'ит
     *   (debugger can observe length truncation via NovaErrorChain count).
     */
    int depth = 0;
    for (NovaErrorChain* it = primary->error_suppressed; it && depth < 256; it = it->next, depth++) {
        /* Identity check: existing node points к same payload? */
        if (it->msg.ptr == msg.ptr && it->kind == kind && it->user_payload == user_payload) {
            return;  /* duplicate — silently skip (cycle-safety) */
        }
    }
    if (depth >= 256) {
        return;  /* depth-limit 256 reached — silently no-op (D193) */
    }
    NovaErrorChain* node = (NovaErrorChain*)nova_alloc(sizeof(NovaErrorChain));
    node->msg          = msg;
    node->kind         = kind;
    node->user_payload = user_payload;
    node->user_type_id = tid;
    node->next         = primary->error_suppressed;
    primary->error_suppressed = node;
}

/* `nova_rethrow_with_suppressed(frame)` — re-throw из inner frame в outer,
 * preserving (transferring ownership) of error_suppressed chain. Called
 * после `setjmp(_fn_frame.jmp) != 0` обработки — когда вся cleanup-chain
 * complete и нужно continue propagation upward. Frame's contents copied
 * to outer fail-frame; chain pointer transferred (single owner).
 *
 * `frame` уже popped (nova_fail_pop called); _nova_fail_top points to outer. */
static inline void nova_rethrow_with_suppressed(NovaFailFrame* frame) {
    /* ──────────────────────────────────────────────────────────────────
     * Plan 173 Ф.4 #6 (D158 model B): mirror the composed suppressed chain
     * into the thread-local `_nova_last_error` snapshot — the readable
     * "pocket". Model B keeps `primary` the sole in-effect carrier (a typed
     * `with Fail[Primary]` still catches; the effect never becomes
     * `Fail[MultiError]`); the accompanying cleanup-failures travel out of
     * band in this pocket and are read post-catch by the `suppressed()`
     * accessor. `nova_rethrow_with_suppressed` is the single transport
     * chokepoint (defer/consume TRANSPARENT terminal + panic/cancel via
     * nova_scope_exit), so mirroring here populates the pocket for whichever
     * outer fail-frame ultimately catches. A fresh `throw` resets the pocket
     * (nova_last_error_set → error_suppressed = NULL), so an empty chain here
     * surfaces as an empty pocket (task #7: no leak between unrelated catches;
     * the reset happens per originating throw). */
    _nova_last_error.frame.error_suppressed = frame->error_suppressed;
    if (_nova_fail_top) {
        _nova_fail_top->error_msg          = frame->error_msg;
        _nova_fail_top->error_kind         = frame->error_kind;
        _nova_fail_top->error_reason_ptr   = frame->error_reason_ptr;
        _nova_fail_top->error_user_payload = frame->error_user_payload;
        _nova_fail_top->error_user_type_id = frame->error_user_type_id;
        _nova_fail_top->error_suppressed   = frame->error_suppressed;
        longjmp(_nova_fail_top->jmp, 1);
    }
    /* No outer fail-frame — abort с dump (primary + chain). */
    fflush(stdout);
    fprintf(stderr, "nova: unhandled Fail (D158 composite): %.*s\n",
        (int)frame->error_msg.len, frame->error_msg.ptr);
    {
        NovaErrorChain* c = frame->error_suppressed;
        int i = 1;
        while (c) {
            fprintf(stderr, "  suppressed [%d]: %.*s\n", i, (int)c->msg.len, c->msg.ptr);
            c = c->next;
            i++;
        }
    }
    nova_throw_site_dump();  /* Plan 173 Ф.5 п.7 */
    abort();
}

/* ──────────────────────────────────────────────────────────────────
 * Plan 173 Ф.2.C (D314 §4): nova_scope_exit — ЕДИНАЯ точка терминальной
 * политики для scope-exit re-dispatch.
 * ──────────────────────────────────────────────────────────────────
 *
 * До Ф.2.C каждый scope-terminal сайт (with-Fail / defer-error / consume)
 * САМ решал `error_kind → transport` (PANIC→rethrow, CANCEL→cancel_reason,
 * USER→…). Дублированный per-frame kind-dispatch → класс дефекта «кадр забыл
 * kind» (напр. [M-172-with-fail-swallows-panic]: with-Fail проверял только
 * CANCEL, PANIC проваливался в USER-path и глотался — нарушение D13). Теперь
 * ОДИН helper владеет таблицей — ни один сайт не может «забыть» kind.
 *
 * `policy`:
 *   CATCH       — with-Fail terminal. Recoverable (USER/USER_TYPED) ловятся
 *                 handler'ом (helper возвращается, вызывающий ставит
 *                 result=default); PANIC (bug/abort-class D13) и CANCEL
 *                 (структурная отмена) НЕ ловятся — re-throw нагору.
 *   TRANSPARENT — defer/consume terminal. Любой не-Success kind проброшен.
 *
 * Таблица (D314 §4, приведена к факту Ф.2.C):
 *   error_kind        | transport
 *   ------------------|------------------------------------------------------
 *   PANIC             | nova_rethrow_with_suppressed(primary)  — kind=PANIC,
 *                     |   suppressed-chain СОХРАНЁН (не голый nv_panic, который
 *                     |   потерял бы chain; матчит post-B3-merge defer-kernel)
 *   CANCEL            | nova_rethrow_with_suppressed(primary)  — reason_ptr И
 *                     |   chain СОХРАНЕНЫ (rethrow копирует frame->error_reason_ptr,
 *                     |   строка ~214; эмпирически подтверждено §5)
 *   USER / USER_TYPED | CATCH → return (handler поймал; вызывающий → result=default)
 *                     | TRANSPARENT → nova_rethrow_with_suppressed(primary)
 *   Success           | no-op (sentinel; недостижим — frame инспектируется только
 *                     |   после throw, у NovaThrowKind нет success-значения)
 *
 * КОНТРАКТ: `primary` уже popped (nova_fail_pop сделан, _nova_fail_top → outer);
 * site-специфичный пролог (with-Fail: restore handlers/interrupt) выполнен ДО
 * вызова. Compose (nv_compose_suppressed / panic-dominance) остаётся в codegen —
 * helper делает ТОЛЬКО single-frame terminal transport. Для re-thrown kind НЕ
 * возвращается (longjmp через nova_rethrow_with_suppressed, либо abort с dump
 * если нет outer-frame). */
typedef enum {
    NOVA_SCOPE_EXIT_CATCH       = 0,  /* with-Fail: USER/USER_TYPED caught, PANIC/CANCEL re-thrown */
    NOVA_SCOPE_EXIT_TRANSPARENT = 1,  /* defer/consume: любой не-Success kind re-thrown */
} NovaScopeExitPolicy;

static inline void nova_scope_exit(NovaFailFrame* primary, NovaScopeExitPolicy policy) {
    if (primary->error_kind == NOVA_THROW_PANIC) {
        /* D13: panic = bug/abort-class — всегда пробрасывается (chain сохранён). */
        nova_rethrow_with_suppressed(primary);
        return;  /* unreachable */
    }
    if (primary->error_kind == NOVA_THROW_CANCEL) {
        /* Структурная отмена — reason_ptr сохраняется копией из frame (§5). */
        nova_rethrow_with_suppressed(primary);
        return;  /* unreachable */
    }
    /* USER / USER_TYPED — recoverable throw. */
    if (policy == NOVA_SCOPE_EXIT_TRANSPARENT) {
        nova_rethrow_with_suppressed(primary);
        return;  /* unreachable */
    }
    /* CATCH: handler отработал — вызывающий сам ставит result=default.
     * Ф.4 #5: the error is now caught & recovered — invalidate the stable
     * snapshot so a later value-`interrupt` does not resurrect it. */
    _nova_last_error.live = 0;
}

/* Accessors для MultiError prelude — count + indexed access на chain.
 * Caller (codegen MultiError @suppressed()) uses этих для materialize'а
 * Nova-side []Err array. */
/* Plan 173 Ф.5 п.2 (D192-РЕТРАКТ): 3-level resolution — источник ПОРОГА
 * watchdog-варна «fiber застрял в cleanup», НЕ прерывания (force-timeout
 * механизм удалён; cleanup всегда добегает — §3a completes-by-default).
 *
 * Уровни РЕАЛИЗОВАНЫ в codegen consume-prologue (emit_c.rs
 * Stmt::ConsumeScope), НЕ здесь:
 * - Level 1 (WithExitTimeout impl per type): compile-time check
 *   `method_overloads` на `exit_timeout_ms` → прямой вызов
 *   `Nova_<T>_method_exit_timeout_ms(binding)` (Plan 110.9.2 V1.1).
 * - Level 2 (Application effect handler): runtime-проверка
 *   `_nova_handler_Application` → `Nova_Application_default_exit_timeout_ms()`
 *   (Plan 110.4.6.a).
 * - Level 3: ЭТА функция — hardcoded fallback 5000 ms.
 *
 * Порог уходит в `nv_cleanup_watchdog_arm` (fibers.h) вокруг
 * cleanup-вызова + в overrun-флаг ResourceTrace exit-события (D185 amend). */
static inline int nv_resolve_exit_timeout_ms(void) {
    return 5000;  /* Level 3 fallback — порог варна (D192-ретракт). */
}

/* Plan 110.2.1.a (D188 R3): cancel-shield runtime — `nv_consume_enter_shield`
 * + `nv_consume_leave_shield` defined в fibers.h после NovaSpawnCtxBase
 * (нужен доступ к per-fiber state). Codegen-emitted call sites компилятся
 * после nova_rt.h inclusion, которое включает fibers.h транзитивно, так что
 * inline-определения видны в user TU. Прототипа здесь нет специально —
 * `static inline` в fibers.h не может быть forward-declared как non-static. */

static inline int nova_failframe_suppressed_count(const NovaFailFrame* frame) {
    if (!frame) return 0;
    int n = 0;
    const NovaErrorChain* c = frame->error_suppressed;
    while (c) { n++; c = c->next; }
    return n;
}

static inline NovaErrorChain* nova_failframe_suppressed_at(const NovaFailFrame* frame, int idx) {
    if (!frame) return NULL;
    NovaErrorChain* c = frame->error_suppressed;
    int i = 0;
    while (c && i < idx) { c = c->next; i++; }
    return c;
}

#define NOVA_TRY(frame)   (nova_fail_push(&(frame)), (frame).error_suppressed = NULL, setjmp((frame).jmp) == 0)
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
    /* Saved _nova_current_handler_iframe at push time — restored by
     * nova_interrupt before longjmp so a stale handler-arm pointer never
     * leaks past the with-block boundary. See effects.c for why this is
     * necessary when interrupt fires inside a Fail handler body. */
    struct NovaInterruptFrame* saved_handler_iframe;
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
    f->saved_handler_iframe = _nova_current_handler_iframe;
    _nova_interrupt_top = f;
}

/* Plan 61 followup #1: defer-scope push — sets kind=DEFER_SCOPE так что
 * nova_interrupt при cross-effect routing preserves defer cleanup chain.
 * Plan 173 Ф.4 #6: value/value_ptr ZEROED — the defer intercept re-issues by
 * probing `value_ptr` (pointer-route if non-NULL); stack-garbage value_ptr
 * would silently reroute an int-valued interrupt through the pointer slot
 * (garbage result). nova_interrupt/interrupt_ptr set exactly ONE slot, so
 * the other must be deterministically zero. */
static inline void nova_interrupt_push_defer(NovaInterruptFrame* f) {
    f->kind = NOVA_IFRAME_DEFER_SCOPE;
    f->value = 0;
    f->value_ptr = NULL;
    f->prev = _nova_interrupt_top;
    f->saved_handler_iframe = _nova_current_handler_iframe;
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

/* Plan 173 Ф.6 (D348): substring-матч для panics-клаузулы теста —
 * (ptr,len)-окно (nova_str НЕ гарантирует NUL-terminator). needle —
 * C-литерал паттерна (NUL-terminated). Пустой needle матчит всё
 * («любая паника»). Case-sensitive (D89-семантика). */
static inline int nova_test_msg_contains(const char* hay, size_t hay_len,
                                         const char* needle) {
    size_t nlen = 0;
    while (needle[nlen]) nlen++;
    if (nlen == 0) return 1;
    if (!hay || hay_len < nlen) return 0;
    for (size_t i = 0; i + nlen <= hay_len; i++) {
        size_t j = 0;
        while (j < nlen && hay[i + j] == needle[j]) j++;
        if (j == nlen) return 1;
    }
    return 0;
}

/* Forward decl: defined later in nova_rt.h once mco is included.
 * We test "are we inside a fiber" to decide where assertion failure lands. */
int nova_in_fiber(void);

/* Plan 140.1 Ф.2 (D13 amend): location-first assert diagnostic.
 *   - assert(cond):            "<file>:<line>: assert failed: <expr>"
 *   - assert(cond, "msg"):     "<file>:<line>: assert failed: <msg> (<expr>)"
 * `file`/`line` are auto-supplied by codegen (`__FILE__`-equivalent Nova
 * `file:line`); the user never embeds the location. `user_msg == NULL`
 * → no message. `debug_assert` lowers through the same helper (the
 * debug-only gate is a codegen concern, not a text difference). The
 * formatted message is heap-copied before longjmp so it survives the
 * stack unwind (the previous variant stored the bare `expr_str` pointer,
 * which was a static string literal; the formatted buffer is on the
 * stack and must be promoted). */
static inline void nova_assert_loc(
    nova_bool cond,
    const char* expr_str,
    const char* file,
    int line,
    const char* user_msg)
{
    if (!cond) {
        char buf[512];
        if (user_msg) {
            snprintf(buf, sizeof(buf),
                "%s:%d: assert failed: %s (%s)",
                file, line, user_msg, expr_str);
        } else {
            snprintf(buf, sizeof(buf),
                "%s:%d: assert failed: %s",
                file, line, expr_str);
        }
        /* Inside a fiber: route through the nearest NovaFailFrame so longjmp
         * stays on the fiber's own stack — never crosses the mco boundary.
         * Spawn-entry pushes a per-fiber fail-frame; supervised_run re-throws
         * on main flow via nova_throw, which the test runner's _tf_fail catches.
         * On main flow (no fiber): route to _nova_test_frame as before. */
        if (nova_in_fiber() && _nova_fail_top) {
            nova_last_error_set(nova_str_from_cstr(buf), NOVA_THROW_PANIC,
                                NULL, NOVA_TID_NONE);  /* Ф.4 #5 */
            _nova_fail_top->error_msg = nova_str_from_cstr(buf);
            /* Plan 140.3 (D13 amend): assert failure is a PANIC-class failure
             * (spec D13: "assert failure = panic"), identical to nv_panic and
             * contract violations. Tag error_kind so ConsumeScope/supervised
             * classify the caught error as Panic(msg), not a recoverable
             * Failure(msg). */
            _nova_fail_top->error_kind = NOVA_THROW_PANIC;
            longjmp(_nova_fail_top->jmp, 1);
        }
        if (_nova_test_frame) {
            /* fail_msg holds const char* — buf is stack-local, so copy via
             * nova_alloc to survive the longjmp (mirror of contracts.h). */
            size_t n = 0;
            while (buf[n]) n++;
            char* heap = (char*)nova_alloc(n + 1);
            for (size_t i = 0; i <= n; i++) heap[i] = buf[i];
            _nova_test_frame->fail_msg = heap;
            longjmp(_nova_test_frame->jmp, 1);
        }
        fprintf(stderr, "%s\n", buf);
        abort();
    }
}

/* Back-compat 2-arg wrapper (no location, no message). Retained so any
 * legacy call site that still emits `nova_assert(cond, "expr")` keeps
 * compiling; codegen now emits `nova_assert_loc(...)` with file/line. */
static inline void nova_assert(nova_bool cond, const char* expr_str) {
    nova_assert_loc(cond, expr_str, "<unknown>", 0, NULL);
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
    nova_last_error_set(msg, NOVA_THROW_PANIC, NULL, NOVA_TID_NONE);  /* Ф.4 #5 */
    if (_nova_fail_top) {
        _nova_fail_top->error_msg = msg;
        /* Plan 110.1.4.g (D188): mark frame's error_kind = PANIC so
         * ConsumeScope codegen может construct Panic(msg) variant вместо
         * Failure(msg). Сохраняем backwards compatibility: existing
         * defer/errdefer code не reads NOVA_THROW_PANIC специально
         * (treated as throw для cleanup-cascade purposes). */
        _nova_fail_top->error_kind = NOVA_THROW_PANIC;
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
    nova_throw_site_dump();  /* Plan 173 Ф.5 п.7 */
    abort();
}

/* Plan 33.8 Ф.1.2: checked знаковая `int`-арифметика.
 *
 * Переполнение `int` (i64) — `panic` (spec 04-effects.md, D13), а НЕ
 * молчаливый wrap или C-UB. Это делает безграничную SMT-кодировку `int`
 * в верификаторе sound: верифицированная функция либо вернёт истинный
 * математический результат, либо умрёт паникой — ложного (обёрнутого)
 * значения она вернуть не может.
 *
 * Sized-типы (u8/u16/u32/u64/i8/i16/i32) сюда НЕ идут — у них семантика
 * wrap-around (Plan 33.7), эмитятся обычным C-оператором.
 *
 * `__builtin_*_overflow` пишут результат в `*r` всегда (даже при
 * переполнении — обёрнутое значение), поэтому `return r` определён. */
#define NOVA_INT_OVF_PANIC(lit) \
    nv_panic((nova_str){ .ptr = (lit), .len = sizeof(lit) - 1 })

static inline nova_int nova_int_checked_add(nova_int a, nova_int b) {
    nova_int r;
    if (__builtin_add_overflow(a, b, &r)) NOVA_INT_OVF_PANIC("integer overflow: +");
    return r;
}
static inline nova_int nova_int_checked_sub(nova_int a, nova_int b) {
    nova_int r;
    if (__builtin_sub_overflow(a, b, &r)) NOVA_INT_OVF_PANIC("integer overflow: -");
    return r;
}
static inline nova_int nova_int_checked_mul(nova_int a, nova_int b) {
    nova_int r;
    if (__builtin_mul_overflow(a, b, &r)) NOVA_INT_OVF_PANIC("integer overflow: *");
    return r;
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
    /* Ф.4 #5: capture BEFORE dispatch — a handler that `interrupt`s never
     * returns to the `nova_throw(msg)` fallback below, so the stable snapshot
     * must be stamped here for the interrupt-unwound cleanup to recover it. */
    nova_last_error_set(msg, NOVA_THROW_USER, NULL, NOVA_TID_NONE);
    /* Plan 173 Ф.4 #6: cleanup-unwind bypasses handler dispatch (model B). */
    if (_nova_handler_Fail && !nova_in_cleanup_unwind()) {
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
    nova_last_error_set(msg_repr, NOVA_THROW_USER_TYPED, payload, tid);  /* Ф.4 #5 */
    if (_nova_fail_top) {
        _nova_fail_top->error_msg          = msg_repr;
        _nova_fail_top->error_kind         = NOVA_THROW_USER_TYPED;
        _nova_fail_top->error_reason_ptr   = NULL;
        _nova_fail_top->error_user_payload = payload;
        _nova_fail_top->error_user_type_id = tid;
        _nova_fail_top->error_suppressed   = NULL;  /* Plan 100.4.1 D158 */
    }
    /* Step 2: erased typed slot.
     * Plan 173 Ф.4 #6: cleanup-unwind bypasses handler dispatch (model B). */
    if (_nova_handler_Fail_any && !nova_in_cleanup_unwind()) {
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
     * payload уже в frame (выше).
     * Plan 173 Ф.4 #6: cleanup-unwind bypasses handler dispatch (model B). */
    if (_nova_handler_Fail && !nova_in_cleanup_unwind()) {
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
    nova_throw_site_dump();  /* Plan 173 Ф.5 п.7 */
    abort();
    return NOVA_UNIT;  /* unreachable */
}

/* ---- Built-in `Time` effect (D11 / D14 / D62) ----
 *
 * Operations: now_unix_ms() -> int, sleep(ms int) -> unit. By D11 — это
 * обычный stdlib-эффект. По D62 — Async ambient: Time-операции callable
 * откуда угодно, в сигнатуре не требуется, default handler доступен.
 *
 * Default handler (см. fibers.h):
 *   sleep(ms)      — context-sensitive: в fiber'е yield-loop до deadline;
 *                    на main внутри supervised — drain queue per pass;
 *                    на top-level (нет scope) — native OS sleep.
 *                    ms <= 0 → один yield (compatibility с `Time.sleep(0)`).
 *   now_unix_ms()  — unix epoch ms, real wall clock (uv_gettimeofday; см.
 *                    _nova_wall_unix_ms() в fibers.h — [M-time-default-
 *                    handler-not-wallclock] / D316 amend, 2026-07-06).
 *
 * User override: `with Time = handler Time { sleep(ms) { ... } now_unix_ms() { ... } } { body }`
 * — для тестов (fixed clock, mock sleep). */

/* Layout matches codegen-generated layout for user effects.
 *
 * Plan 175 Ф.1/Ф.4 (D316 — единый источник схемы + единицы в именах опов):
 * op-schema эффекта `Time` теперь читается codegen'ом ИЗ
 * std/prelude/effects.nv (int-провод `sleep(ms int)`,
 * `now_unix_ms()->int`, `now_monotonic_ns()->int`), а не из хардкода.
 * Этот hand-written vtable = HANDLER-интерфейс (не codegen-schema): его
 * слоты — только те опы, что реализуются `with Time = handler {...}`.
 * `now_monotonic_ns` и 5 timer-счётчиков (→ TimerMetrics, Ф.1/Q1)
 * дispatch'атся direct-C (fibers.h / channels.h), НЕ через этот vtable.
 *
 * Plan 48 Ф.5: now_ms / now_ns — handler-extension слоты, чтобы handlers.nv
 * (fixed_ms, mut_clock — std/testing/handlers.nv) могли регистрировать
 * полный набор. Default-импл (Nova_Time_now_ms / _now_ns) — wrapper'ы
 * вокруг now_unix_ms(). Field-названия designated-init'ятся по имени
 * (порядок в структуре не важен для C designated initializers) — MUST
 * совпадать с codegen-emitted op-именами: ctx, sleep, now_unix_ms, now_ms,
 * now_ns (см. emit_handler_decl / fixed_ms vtable init). Ретайр
 * now_ms/now_ns — Plan 175 Ф.2 (не в Ф.1/Ф.4). */
typedef struct {
    void*     ctx;
    nova_unit (*sleep)(void* _ctx, nova_int ms);
    nova_int  (*now_unix_ms)(void* _ctx);
    nova_int  (*now_ms)(void* _ctx);
    nova_int  (*now_ns)(void* _ctx);
} NovaVtable_Time;

#ifdef _MSC_VER
__declspec(thread) extern NovaVtable_Time* _nova_handler_Time;
#else
extern __thread NovaVtable_Time* _nova_handler_Time;
#endif

/* Nova_Time_sleep / Nova_Time_now_unix_ms defined in fibers.h (after NovaFiberQueue
 * complete + nova_fiber_yield + nova_supervised_step). They are not
 * forward-declared here because callers always include nova_rt.h which pulls
 * in fibers.h after effects.h. */

/* ──────────────────────────────────────────────────────────────────
 * Plan 110.9.3 V1.1 [M-110.9.3-register-finalizer-lifo]:
 * Application register_finalizer LIFO stack runtime.
 * ──────────────────────────────────────────────────────────────────
 *
 * Per-`with Application = handler { body }` block stack of fn pointers.
 * Registered via `nova_app_register_finalizer(fn)`; fired LIFO at block
 * exit (both normal completion AND throw path).
 *
 * D195 R2 (test isolation): finalizer registry NOT inherited across
 * nested Application handlers — each `with Application = ...` block
 * gets fresh stack, previous TLS pointer saved + restored on exit.
 *
 * D195 R8: abort/SIGKILL не fires finalizers (OS limitation).
 *
 * Codegen integration: `with Application = handler { body }` emits
 * prologue (save+init TLS) + body + epilogue (fire+restore TLS) — both
 * normal AND throw path fire the stack. See emit_with в emit_c.rs.
 */

/* Store closure form (fn + env) для compatibility с Nova fn types,
 * которые wrapped в NovaClosBase {fn, env}. Free fns get env=NULL. */
typedef struct NovaFinalizer {
    void* fn;   /* nova_unit (*)(void* env) — closure entry */
    void* env;  /* captured environment (NULL для free fns) */
    struct NovaFinalizer* prev;
} NovaFinalizer;

typedef struct {
    NovaFinalizer* top;  /* LIFO list head — push prepends, fire walks. */
} NovaFinalizerStack;

#ifdef _MSC_VER
__declspec(thread) extern NovaFinalizerStack* _nova_active_finalizer_stack;
#else
extern __thread NovaFinalizerStack* _nova_active_finalizer_stack;
#endif

/* Push closure (fn + env) onto active stack. No-op if no active stack
 * or fn==NULL. */
static inline void nova_finalizer_push(NovaFinalizerStack* s, void* fn, void* env) {
    if (!s || !fn) return;
    NovaFinalizer* node = (NovaFinalizer*)nova_alloc(sizeof(NovaFinalizer));
    node->fn = fn;
    node->env = env;
    node->prev = s->top;
    s->top = node;
}

/* Fire all finalizers LIFO. Stack drained — re-firing safe (no-op).
 * Throws from finalizers propagate up через current fail-frame — caller
 * must wrap if needed. */
static inline void nova_finalizer_fire_lifo(NovaFinalizerStack* s) {
    if (!s) return;
    NovaFinalizer* cur = s->top;
    s->top = NULL;
    while (cur) {
        NovaFinalizer* next = cur->prev;
        if (cur->fn) {
            ((nova_unit (*)(void*))cur->fn)(cur->env);
        }
        cur = next;
    }
}

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
 * Размер таблицы (Plan 174.4): compile-time N — точное число зарегистрированных
 * эффектов (built-in Fail/Time/Mem + user-defined; источник — реестр
 * `effect_schemas` в codegen). Механизм проброса N (ФАКТИЧЕСКИЙ, НЕ `#define` в
 * теле `.c`): генерируемый `.c` эмитит на строке 1 comment-МАРКЕР вида
 * `nova-effect-count: N` (в C-комментарии); build-слой
 * (`test_runner.rs::effect_count_define_arg`)
 * читает N из маркера и передаёт `-DNOVA_MAX_EFFECT_STORAGES=N` (`/D` для MSVC) на
 * ВЕСЬ cc-вызов — во все translation units разом. Почему НЕ `#define` внутри самого
 * `.c`: генерируемый TU и рантайм-TU (`effects.c`/`runtime.c`/`fibers.c`)
 * компилируются как ОТДЕЛЬНЫЕ TU в одном cc-вызове; `#define` только в `.c` дал бы
 * `NovaEffectRegistry`/`NovaEffectSnapshot` РАЗНОГО размера в разных TU → OOB-запись
 * в TLS-registry → segfault. `-D` на весь вызов держит размер массива идентичным во
 * всех TU (ABI-uniformity). Значение 32 ниже (`#ifndef`) — только fallback для
 * hand-written / bootstrap-хедеров, собираемых без маркера. Так silent-drop 33-го
 * эффекта (наследование handler'а через фиберы) больше невозможен, а per-fiber
 * snapshot занимает ровно N указателей, не фикс-256B.
 */

#ifndef NOVA_MAX_EFFECT_STORAGES
#define NOVA_MAX_EFFECT_STORAGES 32
#endif

typedef struct {
    void** slots[NOVA_MAX_EFFECT_STORAGES];   /* registered TLS addresses */
    int    count;
} NovaEffectRegistry;

/* Plan 83.10.4 Ф.3 [M-83.10.1-per-fiber-handler-tls-race]: TLS registry.
 * Каждый поток (main + workers) имеет свою копию для хранения своих
 * TLS-адресов handler'ов. Zero-initialized при старте каждого потока. */
#ifdef _MSC_VER
extern __declspec(thread) NovaEffectRegistry _nova_effect_registry;
#else
extern __thread NovaEffectRegistry _nova_effect_registry;
#endif

/* Plan 83.10.4 Ф.3: function pointer set by generated nova_fn_main to
 * register all effects for any thread. Null until nova_fn_main runs.
 * Worker threads call this at startup to populate their TLS registry. */
extern void (*_nova_register_effects_fn)(void);

/* Регистрация handler-storage. Idempotent (по адресу). Вызывается из
 * codegen'а при первом использовании эффекта (или статически перед main). */
static inline void nova_register_effect_storage(void** slot_addr) {
    for (int i = 0; i < _nova_effect_registry.count; i++) {
        if (_nova_effect_registry.slots[i] == slot_addr) return;
    }
    /* Plan 174.4: NOVA_MAX_EFFECT_STORAGES = точный compile-time N (число
     * distinct-эффектов из реестра codegen'а). Переполнение теперь означает
     * баг codegen'а (define не покрыл фактическое число эффектов), а не
     * нормальный путь → hard-fail с диагностикой вместо прежнего молчаливого
     * дропа, который ронял наследование handler'а через фиберы. */
    if (_nova_effect_registry.count >= NOVA_MAX_EFFECT_STORAGES) {
        fprintf(stderr,
            "nova: effect-registry overflow (count=%d, max=%d) — codegen bug: "
            "NOVA_MAX_EFFECT_STORAGES не покрывает число зарегистрированных эффектов\n",
            _nova_effect_registry.count, NOVA_MAX_EFFECT_STORAGES);
        abort();
    }
    _nova_effect_registry.slots[_nova_effect_registry.count++] = slot_addr;
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
