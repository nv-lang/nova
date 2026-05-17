/* Plan 61 Ф.1: weak fallback для nova_typeid_to_name.
 *
 * Codegen эмитит overriding implementation в preamble (auto-gen'd
 * switch-case на all registered types). Этот файл — для случая когда
 * codegen не предоставил (unit-test C source, minimal repro).
 *
 * Reserved primitives возвращают канонические имена; остальное —
 * `unknown(<n>)` (для diagnostic, не для logic).
 */

#include "typeid.h"
#include <stdio.h>

const char* nova_typeid_to_name(NovaTypeId tid) {
    switch (tid) {
        case NOVA_TID_NONE:      return "<none>";
        case NOVA_TID_nova_int:  return "int";
        case NOVA_TID_nova_str:  return "str";
        case NOVA_TID_nova_bool: return "bool";
        case NOVA_TID_nova_f64:  return "f64";
        case NOVA_TID_nova_f32:  return "f32";
        case NOVA_TID_nova_byte: return "byte";
        case NOVA_TID_nova_unit: return "unit";
        default: {
            /* Diagnostic fallback. Buffer статический — формально не
             * thread-safe; OK для одноразового print/panic message
             * (TID для unknown типа — anyway impl-detail debug). Если
             * понадобится thread-safe — заменим на _Thread_local (C11). */
            static char buf[32];
            snprintf(buf, sizeof(buf), "<type#%u>", (unsigned)tid);
            return buf;
        }
    }
}
