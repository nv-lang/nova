/* nova_rt/fibers.c — minicoro implementation unit + global state defs.
 * Exactly one .c file must define MINICORO_IMPL before including minicoro.h.
 */
#define MINICORO_IMPL
#define MINICORO_INCLUDED_IMPL
#include "minicoro.h"

/* Plan 22 Ф.3 (D93): definitions for sched.h externs.
 * Side-table для park/wake state, indexed by NovaFiberQueue*. Хранится
 * глобально (single-thread bootstrap) — под M:N (Plan 23) станет
 * per-worker. */
#include "nova_rt.h"

NovaSchedState _nova_sched_states[NOVA_SCHED_STATE_CAP];
int            _nova_sched_state_count = 0;
