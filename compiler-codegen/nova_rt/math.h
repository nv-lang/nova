/* math.h — Plan 13 umbrella header для f64/f32 math runtime API.
 *
 * D74: math операции на числовых типах — instance-методы.
 * Plan 13: компилятор знает эти функции через `runtime_registry.rs`
 * — single source of truth. `std/runtime/math.nv` (auto-generated)
 * — Nova-side декларации.
 *
 * Сейчас math-функции — обёртки над libc `<math.h>` (sin, cos, sqrt,
 * etc.). Mangling `Nova_f64_method_X` пока не применяется — codegen
 * эмитит прямые libc-имена. Этот header — stable include-point для
 * future migration.
 *
 * См. docs/plans/13-runtime-stdlib-and-autogen.md.
 */

#ifndef NOVA_RT_MATH_H
#define NOVA_RT_MATH_H

#include <math.h>
#include "nova_rt.h"

/* Все math-функции — стандартные libc имена:
 *   sin, cos, tan, asin, acos, atan, atan2,
 *   sinh, cosh, tanh,
 *   exp, exp2, log (== ln), log2, log10, pow,
 *   sqrt, cbrt, fabs (== abs), hypot,
 *   ceil, floor, round, trunc,
 *   isnan, isfinite, isinf.
 *
 * Nova-сигнатуры из std/runtime/math.nv маппятся на них через
 * runtime_registry.rs (Plan 13).
 */

#endif /* NOVA_RT_MATH_H */
