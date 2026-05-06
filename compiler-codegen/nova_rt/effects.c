/* nova_rt/effects.c — thread-local state for Fail, effect handlers, and tests */

#include "nova_rt.h"

#ifdef _MSC_VER
__declspec(thread) NovaFailFrame*      _nova_fail_top      = NULL;
__declspec(thread) NovaInterruptFrame* _nova_interrupt_top = NULL;
__declspec(thread) NovaTestFrame*      _nova_test_frame    = NULL;
__declspec(thread) NovaVtable_Fail*    _nova_handler_Fail  = NULL;  /* default NULL → Nova_Fail_fail falls back to nova_throw */
#else
__thread NovaFailFrame*      _nova_fail_top      = NULL;
__thread NovaInterruptFrame* _nova_interrupt_top = NULL;
__thread NovaTestFrame*      _nova_test_frame    = NULL;
__thread NovaVtable_Fail*    _nova_handler_Fail  = NULL;
#endif
