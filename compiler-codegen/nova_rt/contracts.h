/* Plan 33.1 Ф.4 (D24): contract runtime helper.
 *
 * Реализация runtime fallback'а для contracts:
 * - codegen эмитит проверки requires/ensures/invariant.
 * - Если проверка не прошла — вызывается nova_contract_violation.
 * - Routing идентичен nova_assert: fiber → fail-frame, test → test-frame,
 *   main → stderr + abort.
 *
 * Plan 140 Ф.1 (D24 amend): «enforce-with-elision». Недоказанные контракты
 * проверяются в ОБОИХ режимах (debug И release) — fail-fast abort на
 * нарушение, а не silent UB. Z3-proven контракты codegen НЕ эмитит вообще
 * (zero-cost элидирование на codegen, не препроцессором). Прежняя модель
 * «в release стираются (NDEBUG/assert)» — retracted. nova_contract_violation
 * release-safe: snprintf + fprintf(stderr) + abort() не зависят от NDEBUG,
 * static inline переживает LTO. Build-opt-out (`--contracts=off`) и per-fn
 * `#unchecked` — Plan 140 Ф.2.
 */
#ifndef NOVA_CONTRACTS_H
#define NOVA_CONTRACTS_H

#include "effects.h"
#include "fibers.h"
#include <stdio.h>
#include <stdlib.h>

typedef enum {
    NOVA_CONTRACT_PRE,   /* requires failed */
    NOVA_CONTRACT_POST,  /* ensures failed */
    NOVA_CONTRACT_INV    /* invariant failed (Plan 33.2) */
} NovaContractKind;

/* Routing identical to nova_assert (D13).
 *
 * Plan 140.1 Ф.2 (D24 amend): short location-first diagnostic format.
 *   - without user message: "<file>:<line>: <kind> failed: <expr>"
 *   - with    user message: "<file>:<line>: <kind> failed: <msg> (<expr>)"
 * `<kind>` ∈ requires / ensures / invariant (the word "contract" and
 * "in `<fn>`" are dropped — `<kind>` already implies a contract and
 * `<file>:<line>` localizes; the leading location lets terminals/IDEs make
 * it click-to-line). `fn_name` is retained in the signature (callers pass
 * it) but no longer printed. `user_msg == NULL` → no message (format A);
 * otherwise the user's message precedes the expression in parentheses
 * (format B). The user never embeds the location in the message — it is
 * always auto-prefixed here from the codegen-supplied `file`/`line`.
 *
 * Inline для zero-cost call в hot path'е (debug builds). */
static inline void nova_contract_violation(
    NovaContractKind kind,
    const char* fn_name,
    const char* contract_src,
    const char* file,
    int line,
    const char* user_msg)
{
    (void)fn_name; /* retained for ABI/call-site stability; not printed */
    /* Build diagnostic message. Use fixed-size buffer; truncate if needed. */
    char buf[512];
    const char* kind_str =
        (kind == NOVA_CONTRACT_PRE)  ? "requires" :
        (kind == NOVA_CONTRACT_POST) ? "ensures"  :
                                       "invariant";
    if (user_msg) {
        snprintf(buf, sizeof(buf),
            "%s:%d: %s failed: %s (%s)",
            file, line, kind_str, user_msg, contract_src);
    } else {
        snprintf(buf, sizeof(buf),
            "%s:%d: %s failed: %s",
            file, line, kind_str, contract_src);
    }

    /* Routing: fiber → fail-frame → test-frame → stderr+abort. */
    if (nova_in_fiber() && _nova_fail_top) {
        _nova_fail_top->error_msg = nova_str_from_cstr(buf);
        /* Plan 140.3 (D24/D13 amend): a contract violation is a PANIC-class
         * failure (a bug), identical to assert and nv_panic. Tag error_kind so
         * ConsumeScope/supervised classify the caught error as Panic(msg), not
         * a recoverable Failure(msg). Without this it defaulted to
         * NOVA_THROW_USER and was indistinguishable from a normal throw. */
        _nova_fail_top->error_kind = NOVA_THROW_PANIC;
        longjmp(_nova_fail_top->jmp, 1);
    }
    if (_nova_test_frame) {
        /* fail_msg хранит const char* — buf на стеке, поэтому копируем
         * через nova_alloc для пере-жития longjmp'а. */
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

/* Plan 140.3 ([M-140.1-message-interpolation]): variant for a DYNAMIC message
 * built at the violation site (an interpolated `nova_str`, not necessarily
 * NUL-terminated → rendered with %.*s). Identical routing, format-B
 * ("<file>:<line>: <kind> failed: <msg> (<src>)") and error_kind=PANIC tagging
 * as nova_contract_violation. Only reached on a violation (codegen emits it
 * inside the `if (!(cond)) { ... }` block), so building the message is lazy. */
static inline void nova_contract_violation_dyn(
    NovaContractKind kind,
    const char* fn_name,
    const char* contract_src,
    const char* file,
    int line,
    nova_str user_msg)
{
    (void)fn_name;
    char buf[512];
    const char* kind_str =
        (kind == NOVA_CONTRACT_PRE)  ? "requires" :
        (kind == NOVA_CONTRACT_POST) ? "ensures"  :
                                       "invariant";
    snprintf(buf, sizeof(buf),
        "%s:%d: %s failed: %.*s (%s)",
        file, line, kind_str,
        (int)user_msg.len, (const char*)user_msg.ptr, contract_src);

    if (nova_in_fiber() && _nova_fail_top) {
        _nova_fail_top->error_msg = nova_str_from_cstr(buf);
        _nova_fail_top->error_kind = NOVA_THROW_PANIC;
        longjmp(_nova_fail_top->jmp, 1);
    }
    if (_nova_test_frame) {
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

#endif /* NOVA_CONTRACTS_H */
