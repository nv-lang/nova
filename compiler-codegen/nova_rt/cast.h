/* cast.h — saturation helpers для float→int конверсий (План 07).
 *
 * D54 расширение: `as`-cast для float→int даёт **defined behavior**
 * на любом входе (NaN, ±∞, out-of-range). C-стандарт оставляет это
 * UB; Nova согласована с Rust 1.45+ (RFC #2484 sealed casts):
 *   - in-range → truncate towards zero (как C)
 *   - out-of-range positive → INT_MAX / UINT_MAX
 *   - out-of-range negative → INT_MIN / 0 (для unsigned)
 *   - NaN → 0
 *   - +Infinity → INT_MAX / UINT_MAX
 *   - -Infinity → INT_MIN / 0
 *
 * Throw-форма для программистов которым нужна проверка —
 * `iN.try_from(f)?` через D77 (отдельный механизм).
 *
 * Все helper'ы `static inline`. Compiler инлайнит — overhead 2-3
 * сравнения на cast.
 *
 * Подключается из nova_rt.h перед array.h.
 */

#ifndef NOVA_CAST_H
#define NOVA_CAST_H

#include <math.h>
#include <stdint.h>

/* ===== f64 → signed int ===== */

static inline int8_t nova_f64_to_i8(double f) {
    if (isnan(f)) return 0;
    if (f >=  128.0)  return INT8_MAX;
    if (f <= -129.0)  return INT8_MIN;
    return (int8_t)f;
}

static inline int16_t nova_f64_to_i16(double f) {
    if (isnan(f)) return 0;
    if (f >=  32768.0)  return INT16_MAX;
    if (f <= -32769.0)  return INT16_MIN;
    return (int16_t)f;
}

static inline int32_t nova_f64_to_i32(double f) {
    if (isnan(f)) return 0;
    if (f >=  2147483648.0)  return INT32_MAX;
    if (f <= -2147483649.0)  return INT32_MIN;
    return (int32_t)f;
}

static inline int64_t nova_f64_to_i64(double f) {
    if (isnan(f)) return 0;
    /* INT64_MAX = 2^63-1 не представим точно в f64; используем 2^63 как
     * порог. INT64_MIN = -2^63 представим точно. */
    if (f >=  9223372036854775808.0)  return INT64_MAX;
    if (f <  -9223372036854775808.0)  return INT64_MIN;
    return (int64_t)f;
}

/* ===== f64 → unsigned int ===== */

static inline uint8_t nova_f64_to_u8(double f) {
    if (isnan(f)) return 0;
    if (f >= 256.0) return UINT8_MAX;
    if (f <= 0.0)   return 0;
    return (uint8_t)f;
}

static inline uint16_t nova_f64_to_u16(double f) {
    if (isnan(f)) return 0;
    if (f >= 65536.0) return UINT16_MAX;
    if (f <= 0.0)     return 0;
    return (uint16_t)f;
}

static inline uint32_t nova_f64_to_u32(double f) {
    if (isnan(f)) return 0;
    if (f >= 4294967296.0) return UINT32_MAX;
    if (f <= 0.0)          return 0;
    return (uint32_t)f;
}

static inline uint64_t nova_f64_to_u64(double f) {
    if (isnan(f)) return 0;
    /* UINT64_MAX = 2^64-1; порог 2^64 (округляется в f64 ровно). */
    if (f >= 18446744073709551616.0) return UINT64_MAX;
    if (f <= 0.0)                    return 0;
    return (uint64_t)f;
}

/* ===== f32 → signed int ===== */

static inline int8_t nova_f32_to_i8(float f) {
    if (isnan(f)) return 0;
    if (f >=  128.0f)  return INT8_MAX;
    if (f <= -129.0f)  return INT8_MIN;
    return (int8_t)f;
}

static inline int16_t nova_f32_to_i16(float f) {
    if (isnan(f)) return 0;
    if (f >=  32768.0f)  return INT16_MAX;
    if (f <= -32769.0f)  return INT16_MIN;
    return (int16_t)f;
}

static inline int32_t nova_f32_to_i32(float f) {
    if (isnan(f)) return 0;
    /* INT32_MAX = 2^31-1 не представим в f32; используем 2^31 как порог. */
    if (f >=  2147483648.0f)  return INT32_MAX;
    if (f <  -2147483648.0f)  return INT32_MIN;
    return (int32_t)f;
}

static inline int64_t nova_f32_to_i64(float f) {
    if (isnan(f)) return 0;
    if (f >=  9223372036854775808.0f)  return INT64_MAX;
    if (f <  -9223372036854775808.0f)  return INT64_MIN;
    return (int64_t)f;
}

/* ===== f32 → unsigned int ===== */

static inline uint8_t nova_f32_to_u8(float f) {
    if (isnan(f)) return 0;
    if (f >= 256.0f) return UINT8_MAX;
    if (f <= 0.0f)   return 0;
    return (uint8_t)f;
}

static inline uint16_t nova_f32_to_u16(float f) {
    if (isnan(f)) return 0;
    if (f >= 65536.0f) return UINT16_MAX;
    if (f <= 0.0f)     return 0;
    return (uint16_t)f;
}

static inline uint32_t nova_f32_to_u32(float f) {
    if (isnan(f)) return 0;
    if (f >= 4294967296.0f) return UINT32_MAX;
    if (f <= 0.0f)          return 0;
    return (uint32_t)f;
}

static inline uint64_t nova_f32_to_u64(float f) {
    if (isnan(f)) return 0;
    if (f >= 18446744073709551616.0f) return UINT64_MAX;
    if (f <= 0.0f)                    return 0;
    return (uint64_t)f;
}

#endif /* NOVA_CAST_H */
