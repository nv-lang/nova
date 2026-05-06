/* nova_rt/effects.c — thread-local state for Fail, effect handlers, and tests */

#include "nova_rt.h"

#ifdef _MSC_VER
__declspec(thread) NovaFailFrame*      _nova_fail_top      = NULL;
__declspec(thread) NovaInterruptFrame* _nova_interrupt_top = NULL;
__declspec(thread) NovaTestFrame*      _nova_test_frame    = NULL;
__declspec(thread) NovaVtable_Fail*    _nova_handler_Fail  = NULL;  /* default NULL → Nova_Fail_fail falls back to nova_throw */
__declspec(thread) NovaVtable_Time*    _nova_handler_Time  = NULL;  /* default NULL → context-sensitive yield (see fibers.h) */
__declspec(thread) NovaFiberQueue*     _nova_active_scope  = NULL;  /* active supervised scope for current thread */
__declspec(thread) int                 _nova_active_slot   = -1;
#else
__thread NovaFailFrame*      _nova_fail_top      = NULL;
__thread NovaInterruptFrame* _nova_interrupt_top = NULL;
__thread NovaTestFrame*      _nova_test_frame    = NULL;
__thread NovaVtable_Fail*    _nova_handler_Fail  = NULL;
__thread NovaVtable_Time*    _nova_handler_Time  = NULL;
__thread NovaFiberQueue*     _nova_active_scope  = NULL;
__thread int                 _nova_active_slot   = -1;
#endif
