/* nova_rt/effects.c — thread-local state for Fail, effect handlers, and tests */

#include "nova_rt.h"

#ifdef _MSC_VER
__declspec(thread) NovaFailFrame*      _nova_fail_top      = NULL;
__declspec(thread) NovaInterruptFrame* _nova_interrupt_top = NULL;
__declspec(thread) NovaTestFrame*      _nova_test_frame    = NULL;
#else
__thread NovaFailFrame*      _nova_fail_top      = NULL;
__thread NovaInterruptFrame* _nova_interrupt_top = NULL;
__thread NovaTestFrame*      _nova_test_frame    = NULL;
#endif
