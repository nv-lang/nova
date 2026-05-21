/* numeric.h — Plan 74: IEEE 754 primitive bit-cast helpers.
 *
 * Reinterpret-cast между float-примитивом и его IEEE 754 bit-pattern:
 *   f64 @to_bits() -> u64   /  f64.from_bits(bits u64) -> f64
 *   f32 @to_bits() -> u32   /  f32.from_bits(bits u32) -> f32
 *
 * Нужно для хэшей, NaN-boxing, FFI, `realtime nogc` зон. `memcpy` —
 * единственный портабельный UB-safe способ (type-punning через union
 * или pointer-cast — UB по strict-aliasing); компилятор сворачивает
 * это до одной mov-инструкции.
 *
 * Целочисленные пары (`i64 ↔ u64`, `i32 ↔ u32`) reinterpret НЕ требуют —
 * там достаточно `as`-cast (биты те же, two's complement).
 *
 * Nova-декларации: `std/runtime/numeric.nv` (auto-gen из
 * `runtime_registry.rs`). См. docs/plans/74-primitive-bitcast.md.
 */

#ifndef NOVA_RT_NUMERIC_H
#define NOVA_RT_NUMERIC_H

#include <stdint.h>
#include <string.h>  /* memcpy */

/* f64 → u64: IEEE 754 double bit-pattern. */
static inline uint64_t Nova_f64_to_bits(double v) {
    uint64_t r;
    memcpy(&r, &v, sizeof(r));
    return r;
}

/* u64 → f64: восстановить double из IEEE 754 bit-pattern. */
static inline double Nova_f64_from_bits(uint64_t b) {
    double r;
    memcpy(&r, &b, sizeof(r));
    return r;
}

/* f32 → u32: IEEE 754 single bit-pattern. */
static inline uint32_t Nova_f32_to_bits(float v) {
    uint32_t r;
    memcpy(&r, &v, sizeof(r));
    return r;
}

/* u32 → f32: восстановить float из IEEE 754 bit-pattern. */
static inline float Nova_f32_from_bits(uint32_t b) {
    float r;
    memcpy(&r, &b, sizeof(r));
    return r;
}

#endif /* NOVA_RT_NUMERIC_H */
