/* nova_rt/fibers.c — minicoro implementation unit.
 * Exactly one .c file must define MINICORO_IMPL before including minicoro.h.
 *
 * Plan 22 Ф.3 (D93) production: NovaSchedState теперь lazy-allocated
 * pointer-в-NovaFiberQueue, side-table убрана. См. sched.h. */
#define MINICORO_IMPL
#define MINICORO_INCLUDED_IMPL
#include "minicoro.h"
