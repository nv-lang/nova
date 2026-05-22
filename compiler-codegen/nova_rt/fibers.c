/* nova_rt/fibers.c — minicoro implementation unit.
 * Exactly one .c file must define MINICORO_IMPL before including minicoro.h.
 *
 * Plan 22 Ф.3 (D93) production: NovaSchedState теперь lazy-allocated
 * pointer-в-NovaFiberQueue, side-table убрана. См. sched.h. */
#define MINICORO_IMPL
#define MINICORO_INCLUDED_IMPL
#include "minicoro.h"

/* Plan 82 Ф.1 — post-create hook (см. fibers.h-декларацию).
 *
 * Определена здесь, а не inline в fibers.h, потому что патч обращается
 * к minicoro-внутреннему типу _mco_context, который виден только в TU
 * с MINICORO_IMPL. На Windows x64 (MCO_USE_ASM) патчит ctx.stack_limit
 * корутины на committed-low слота arena: minicoro _mco_makectx ставит
 * stack_limit на низ всей stack-секции (claim'ит весь стек
 * закоммиченным), а при lazy-commit это ложь — __chkstk-код с кадром
 * >1 страницы крашит на MSVC (Ф.0 test a, decision-point). Патч делает
 * minicoro-стек неотличимым от CreateFiber-стека. No-op на POSIX
 * (mmap-арена demand-paged) и при отключённой arena. */
#include "fiber_arena.h"

void nova_fiber_post_create(mco_coro* co) {
#if defined(_WIN32) && defined(MCO_USE_ASM) && NOVA_FIBER_ARENA_ENABLED
    if (!co || !co->context) return;
    void* committed_low = nova_fiber_committed_low((const void*)co);
    if (committed_low) {
        ((_mco_context*)co->context)->ctx.stack_limit = committed_low;
    }
#else
    (void)co;
#endif
}
