/* Plan 44.2 Этап 3 — fiber arena stats wrappers (cross-platform).
 *
 * Linux/macOS: forward to nova_fiber_arena_stats().
 * Windows: return 0 across the board (arena not active — honest sentinel,
 * same convention as gc.heap_size() under malloc backend). */

#include "alloc.h"
#include "fiber_arena.h"

#if NOVA_FIBER_ARENA_ENABLED

size_t nova_fibers_virtual_reserved(void) {
    return nova_fiber_arena_stats().virtual_reserved;
}
size_t nova_fibers_slot_count(void) {
    return nova_fiber_arena_stats().slot_count;
}
size_t nova_fibers_slots_active(void) {
    return nova_fiber_arena_stats().slots_active;
}
size_t nova_fibers_high_water(void) {
    return nova_fiber_arena_stats().high_water;
}
void   nova_fibers_compact(void) {
    nova_fiber_arena_compact();
}

#else /* Windows / unsupported */

size_t nova_fibers_virtual_reserved(void) { return 0; }
size_t nova_fibers_slot_count(void)       { return 0; }
size_t nova_fibers_slots_active(void)     { return 0; }
size_t nova_fibers_high_water(void)       { return 0; }
void   nova_fibers_compact(void)          { /* no-op — arena off */ }

#endif
