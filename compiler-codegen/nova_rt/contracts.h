/* Plan 33.1 Ф.4 (D24): contract runtime helper.
 *
 * Реализация runtime fallback'а для contracts:
 * - В debug сборке codegen эмитит проверки requires/ensures.
 * - Если проверка не прошла — вызывается nova_contract_violation.
 * - Routing идентичен nova_assert: fiber → fail-frame, test → test-frame,
 *   main → stderr + abort.
 *
 * В release сборке codegen не эмитит check'и для доказанных контрактов
 * (zero-cost). Сейчас (33.1 без SMT) — все контракты в release со
 * статусом default стираются с warning'ом сборки; явный `@unverified`
 * стирается без warning'а.
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

/* Routing identical to nova_assert (D13). The diagnostic message is
 * structured: "<kind> failed in <fn>: <contract_src> at <file>:<line>".
 *
 * Inline для zero-cost call в hot path'е (debug builds). */
static inline void nova_contract_violation(
    NovaContractKind kind,
    const char* fn_name,
    const char* contract_src,
    const char* file,
    int line)
{
    /* Build diagnostic message. Use fixed-size buffer; truncate if needed. */
    char buf[512];
    const char* kind_str =
        (kind == NOVA_CONTRACT_PRE)  ? "requires" :
        (kind == NOVA_CONTRACT_POST) ? "ensures"  :
                                       "invariant";
    snprintf(buf, sizeof(buf),
        "contract %s failed in `%s`: %s at %s:%d",
        kind_str, fn_name, contract_src, file, line);

    /* Routing: fiber → fail-frame → test-frame → stderr+abort. */
    if (nova_in_fiber() && _nova_fail_top) {
        _nova_fail_top->error_msg = nova_str_from_cstr(buf);
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

#endif /* NOVA_CONTRACTS_H */
