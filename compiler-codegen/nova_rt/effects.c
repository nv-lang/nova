/* nova_rt/effects.c — thread-local state for Fail and effect handlers */

#include "effects.h"

#ifdef _MSC_VER
__declspec(thread) NovaFailFrame*      _nova_fail_top      = NULL;
__declspec(thread) NovaInterruptFrame* _nova_interrupt_top = NULL;
#else
__thread NovaFailFrame*      _nova_fail_top      = NULL;
__thread NovaInterruptFrame* _nova_interrupt_top = NULL;
#endif
