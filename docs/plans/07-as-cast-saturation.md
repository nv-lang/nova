# План 07: `as`-cast — saturation для float→int (закрытие UB-gap'а из плана 05)

**Статус:** активный, не начат.
**Дата создания:** 2026-05-08.
**Зависимости:** [Plan 05](05-as-cast-codegen.md) — закрыл core вопрос
(`as` теперь эмитит C-cast); этот план добавляет defined-семантику
для narrowing-cases где C даёт UB.
[D54](../../spec/decisions/03-syntax.md#d54) — спека `as`/`is`.
[D77](../../spec/decisions/08-runtime.md#d77) — `TryFrom` как альтернатива
для checked-cast.

**Закрывает spec-долг плана 05.** План 05 реализовал `as` как C-cast
в коде, но D54 в spec'е **не был обновлён** — semantics narrowing
(wraparound для int→int, поведение для float→int) остались
неспецифицированными. Этот план фиксирует **все 8 случаев** narrowing
в spec D54 (см. Ф.4) — и за себя (float→int saturation), и за план 05
(int→int wraparound).

---

## Проблема

План 05 реализовал `as` как явный C-cast: `((target_t)(inner))`. Для
**int → int** narrowing'а это даёт wraparound (defined в C для
unsigned target, implementation-defined → wraparound на
gcc/clang/msvc для signed). OK.

Для **float → int** narrowing'а C-cast даёт **undefined behavior**
если значение не помещается в target. Стандарт C17 §6.3.1.4:

> If the value of the integral part cannot be represented by the
> integer type, the behavior is undefined.

### Конкретные UB-trap'ы

```nova
let f f64 = 70000.5
let n = f as i16            // C: int16_t n = (int16_t)f;
                            // 70000 не помещается в i16 (-32768..32767) → UB
                            //   gcc: 0
                            //   clang: -32768
                            //   msvc: implementation-defined
```

```nova
let nan f64 = f64.NAN
let m = nan as int          // C: int64_t m = (int64_t)nan;
                            // NaN → integer → UB
                            //   gcc/clang: INT_MIN на x86, но не гарантировано
                            //   архитектура-зависимо
```

```nova
let inf f64 = f64.INFINITY
let k = inf as u32          // C: uint32_t k = (uint32_t)inf;
                            // ±∞ → integer → UB
```

### Где это встречается на практике

- **JSON parsing**: `json.parse_number(s) as i32` для bounds-known fields.
- **Math/statistics**: `mean(xs) as int` для агрегатов.
- **Crypto**: `(hash_state as u32)` после f64-arithmetic в hash mixing.
- **Audio/graphics**: `(sample_double * scale) as i16` — самый частый случай.
- **AI-сгенерированный код**: LLM не добавляет range-checks before cast.

Bootstrap stdlib пока не использует float→narrow_int, но **любой
backend-код** на это упрётся.

### Почему план 05 не покрыл это

План 05 занимался **базовым случаем** (`as` это no-op → должен быть
C-cast). Семантика narrowing для float→int не обсуждалась явно —
итоговая запись в `simplifications.md` фиксирует только wraparound
для int→int.

Это gap-by-omission. Закрываем планом 07.

---

## Цель

`f as iN` / `f as uN` для f64/f32 → integer типов имеет **defined
behavior** через runtime helper'ы:

- **Out-of-range** (значение больше INT_MAX или меньше INT_MIN target'а)
  → **saturation** к ближайшей границе.
- **NaN** → 0.
- **+Infinity** → INT_MAX / UINT_MAX.
- **-Infinity** → INT_MIN / 0 (для unsigned).
- **Normal in-range** → стандартный C `(int_t)f` (truncate towards zero).

Это совпадает с **Rust 1.45+** (RFC #2484 «sealed casts»).

`as` остаётся **pure** (без Fail-эффекта). Если программисту нужен
throw на out-of-range — использует `iN.try_from(f)?` через D77.

---

## Не цель

- **Throw на out-of-range** в `as` — нарушает D54 (`as` это
  «compile-time конвертация», не effectful op). Throw-форма уже
  доступна через D77 `try_from`.
- **Изменение int→int семантики** — план 05 уже сделал wraparound,
  оставляем.
- **Float→float narrowing** (`f64 as f32`) — defined в C через
  IEEE rounding, не нужен helper.
- **Lossy int→float** — int → f64 / f32 defined для любого int64_t
  (потеря точности, но не UB). Не трогаем.
- **Saturation для int→int** — нарушает Rust-style wraparound,
  ломает hash-mixing/CRC use-case'ы. Не делаем.

---

## Что делаем

### Ф.1 — Runtime helpers в `nova_rt`

Новый файл `compiler-codegen/nova_rt/cast.h` (или дополнение к
существующему `nova_rt.h`):

```c
// Saturation float→int helpers. Defined behavior на любом входе.

#include <math.h>
#include <stdint.h>

static inline int8_t   nova_f64_to_i8 (double f);
static inline int16_t  nova_f64_to_i16(double f);
static inline int32_t  nova_f64_to_i32(double f);
static inline int64_t  nova_f64_to_i64(double f);

static inline uint8_t  nova_f64_to_u8 (double f);
static inline uint16_t nova_f64_to_u16(double f);
static inline uint32_t nova_f64_to_u32(double f);
static inline uint64_t nova_f64_to_u64(double f);

// аналогично для float (f32)
static inline int8_t   nova_f32_to_i8 (float f);
// ...
```

Реализация (типичный pattern, на примере i16):

```c
static inline int16_t nova_f64_to_i16(double f) {
    if (isnan(f)) return 0;                    // NaN → 0
    if (f >= 32767.0) return INT16_MAX;        // +inf, > max → saturate up
    if (f <= -32768.0) return INT16_MIN;       // -inf, < min → saturate down
    return (int16_t)f;                          // in-range → truncate
}
```

Для unsigned:

```c
static inline uint16_t nova_f64_to_u16(double f) {
    if (isnan(f)) return 0;
    if (f >= 65535.0) return UINT16_MAX;
    if (f <= 0.0) return 0;                     // negative и -inf → 0
    return (uint16_t)f;
}
```

Для i64 — аккуратно с константами (INT64_MAX = 2^63-1, не
представим точно в f64; используем `9223372036854775808.0` как
порог):

```c
static inline int64_t nova_f64_to_i64(double f) {
    if (isnan(f)) return 0;
    if (f >= 9223372036854775808.0) return INT64_MAX;       // = 2^63
    if (f < -9223372036854775808.0) return INT64_MIN;
    return (int64_t)f;
}
```

(8 helper'ов для f64 + 8 для f32 = 16 функций. Все `static inline`,
~5 строк каждая.)

### Ф.2 — codegen эмитит helper для float→int

В `compiler-codegen/src/codegen/emit_c.rs::ExprKind::As`:

```rust
ExprKind::As(inner, ty) => {
    let target_c = self.type_ref_to_c(ty)?;
    let inner_c_ty = self.infer_expr_c_type(inner);
    let v = self.emit_expr(inner)?;

    // Float → integer: saturation helper
    let is_float_src = matches!(inner_c_ty.as_str(), "nova_f64" | "nova_f32");
    let is_int_target = matches!(target_c.as_str(),
        "nova_int" | "nova_byte" |
        "int8_t"  | "int16_t"  | "int32_t"  | "int64_t" |
        "uint8_t" | "uint16_t" | "uint32_t" | "uint64_t");

    if is_float_src && is_int_target {
        let helper = format!("nova_{}_to_{}",
            if inner_c_ty == "nova_f64" { "f64" } else { "f32" },
            target_c.trim_start_matches("nova_").trim_end_matches("_t"));
        return Ok(format!("{}({})", helper, v));
    }

    // Все остальные cast'ы — прямой C-cast (план 05).
    Ok(format!("(({})({}))", target_c, v))
}
```

### Ф.3 — Тесты в `nova_tests/syntax/`

Новый файл `nova_tests/syntax/as_cast_float.nv`:

```nova
test "f64 as i16 — in range" {
    let f f64 = 100.7
    assert(f as i16 == 100)         // truncate towards zero
}

test "f64 as i16 — out of range positive" {
    let f f64 = 70000.0
    assert(f as i16 == 32767)       // saturation to INT16_MAX
}

test "f64 as i16 — out of range negative" {
    let f f64 = -70000.0
    assert(f as i16 == -32768)      // saturation to INT16_MIN
}

test "f64 as i16 — NaN" {
    let f f64 = f64.NAN
    assert(f as i16 == 0)
}

test "f64 as i16 — +Infinity" {
    let f f64 = f64.INFINITY
    assert(f as i16 == 32767)
}

test "f64 as i16 — -Infinity" {
    let f f64 = -f64.INFINITY
    assert(f as i16 == -32768)
}

test "f64 as u16 — negative becomes 0" {
    let f f64 = -100.0
    assert(f as u16 == 0)           // not wrap, saturate to 0
}

test "f64 as u8 — saturation 256+" {
    let f f64 = 1000.0
    assert(f as u8 == 255)
}

test "f64 as i64 — INT_MAX boundary" {
    let f f64 = 1e20                // > INT64_MAX
    assert(f as int == int.MAX)
}

test "f64 as int — preserves precision in range" {
    let f f64 = 12345.0
    assert(f as int == 12345)
}

test "f32 as i32 — same saturation" {
    let f f32 = 1e10                // > INT32_MAX
    assert(f as i32 == i32.MAX)
}

// Проверка что int→int wraparound остался (не сломали план 05)
test "int wraparound по-прежнему работает" {
    let big int = 0x1_0000_FFFF
    assert(big as i32 == 0xFFFF)
    assert(big as byte == 0xFF)
}
```

### Ф.4 — Spec D54 — раздел «Семантика narrowing»

В `spec/decisions/03-syntax.md → D54` добавить раздел между «Numeric
cast» и «Newtype ↔ underlying»:

```markdown
#### Семантика narrowing-конверсий

Поведение `as` при потере точности зависит от пары source→target:

| From → To | Семантика | Пример |
|---|---|---|
| `iN → iM` (M < N) | wraparound (modulo 2^M) | `0x1_FFFF as i16 == -1` |
| `iN → uM` | bit-pattern truncate | `-1i32 as u16 == 65535` |
| `uN → uM` (M < N) | wraparound | `0x1_FFFF as u16 == 0xFFFF` |
| `uN → iM` | bit-pattern, signed reinterpret | `0xFFFFu16 as i16 == -1` |
| `f64 → f32` | IEEE rounding | `1.1 as f32 ≈ 1.1` (с потерей) |
| **`f → iN`** | **saturation + NaN→0** | `70000.5 as i16 == 32767` |
| **`f → uN`** | **saturation + NaN→0 + neg→0** | `-1.0 as u16 == 0` |
| `iN → f` | exact (или nearest IEEE) | `123 as f64 == 123.0` |
| newtype ↔ underlying | identity | `42 as UserId` reuses bits |

**Float→integer — saturation, не UB.** В отличие от C, где out-of-range
float→int это UB, Nova даёт **defined behavior**:
- Out-of-range positive → INT_MAX / UINT_MAX.
- Out-of-range negative → INT_MIN / 0 (для unsigned).
- NaN → 0.
- ±∞ → соответствующая граница.

**Если нужна проверка** out-of-range — программист использует
[`TryFrom`](../08-runtime.md#d77):

\`\`\`nova
let n = f as i16                // saturation, infallible
let n = i16.try_from(f)?         // throws Fail[OutOfRangeError]
\`\`\`

`as` — pure, **не effectful**. Throw-форма доступна через D77 как
explicit choice.

#### Прецеденты

- **Rust 1.45+** — saturation для float→int (RFC #2484 sealed casts).
  Прямой аналог.
- **C/C++** — UB. Nova улучшает.
- **Swift** — trap (panic) на out-of-range, нет pure `as`. Nova
  выбирает saturation для совместимости с D54 «as это pure».
- **Java** — IEEE round + wraparound, defined но не saturation.
  Nova ближе к Rust.
```

### Ф.5 — Запись в simplifications.md

В `docs/simplifications.md` обновить запись `[P-as-cast-wraparound]`
или добавить новую `[P-as-cast-float-saturation]`:

> [P-as-cast-float-saturation] (2026-05-08): float→int narrowing
> делает saturation через runtime helper'ы (`nova_f64_to_iN` и др.),
> NaN→0, ±∞→границы. Прецедент Rust 1.45+. Throw-форма для
> программистов которым нужна проверка — через D77 `iN.try_from(f)?`.

---

## Acceptance criteria

- ✅ `f64 as i16` для in-range (e.g. 100.7 → 100) — работает как
  truncate towards zero.
- ✅ `f64 as i16` для out-of-range (70000.5) — saturation до 32767.
- ✅ `f64 as i16` для NaN — 0.
- ✅ `f64 as i16` для ±Infinity — границы.
- ✅ `f64 as u16` для negative — 0.
- ✅ Все 12 тестов в `nova_tests/syntax/as_cast_float.nv` PASS.
- ✅ Существующие int→int wraparound тесты (план 05) — без регрессий.
- ✅ Spec D54 содержит раздел «Семантика narrowing» с таблицей.

---

## Trade-offs / упрощения

### Cost: ~16 helper-функций

8 для f64 (i8/i16/i32/i64 × signed + u8/u16/u32/u64 × unsigned) +
8 для f32. Все `static inline`, ~5 строк каждая = 80 строк
runtime-кода.

Compiler инлайнит — на горячем пути overhead **2-3 сравнения**
(`isnan`, `>= max`, `<= min`) vs прямой C-cast. Это **~5 ns на
каждый cast**. Для AI/audio/graphics — приемлемо. Для tight loop'а
программист может писать `((int16_t)f)` через FFI или явный
unchecked-helper.

### Не реализуем `unchecked_as`

Соблазн добавить `f unchecked_as i16` для тех кто хочет zero-cost
UB-cast. **Не делаем** — это новый keyword, нарушает D9 «один путь».
Если профайлер покажет что saturation тормозит — добавим
интринсик `nova_unchecked_f64_to_i16(f)` как escape hatch через FFI,
не как языковую конструкцию.

### f128 / f16 / bfloat16 не покрыты

Bootstrap не имеет этих типов. Когда добавим (Q-extra-floats) —
расширим helpers соответственно.

### Generic helper через `_Generic` (C11) — отвергнут

Можно было сделать **один** макрос `nova_to_int(target, value)`
через C11 `_Generic`. Отвергнуто:
- C11 `_Generic` менее портабельный (старые компиляторы, embedded
  toolchains).
- Phantom-type-dispatch сложнее дебажить.
- 16 явных функций — 80 строк, читается прямо.

---

## План работ

1. **Ф.1** — `nova_rt/cast.h` с 16 helper'ами (~80 строк C).
2. **Ф.2** — codegen `ExprKind::As` детектит float→int и эмитит
   helper-call (~10 строк Rust).
3. **Ф.3** — `nova_tests/syntax/as_cast_float.nv` (12 тестов).
4. **Ф.4** — spec D54 раздел «Семантика narrowing».
5. **Ф.5** — `simplifications.md` запись.

---

## Оценка

Полдня компилятор-агента. Простой C-runtime + узкий codegen-patch +
тесты + spec-апдейт.

---

## Связь с другими планами

- [Plan 05 — as-cast codegen](05-as-cast-codegen.md) — основа; этот
  план **закрывает оставленный gap** (UB для float→int out-of-range).
- [Plan 06 — Iter[T] protocol](06-iter-protocol-codegen.md) —
  параллельная задача, не зависит.
- [D77 TryFrom](../../spec/decisions/08-runtime.md#d77) —
  альтернативный механизм для checked-cast.

---

## Ссылки

- [spec/decisions/03-syntax.md → D54](../../spec/decisions/03-syntax.md#d54)
  — `as` семантика, требует расширения.
- [spec/decisions/08-runtime.md → D77](../../spec/decisions/08-runtime.md#d77)
  — `TryFrom` для checked-cast (`iN.try_from(f)?`).
- C17 §6.3.1.4 — float→int UB definition.
- Rust RFC #2484 «sealed casts» — saturation rationale.
- `compiler-codegen/src/codegen/emit_c.rs` — `ExprKind::As` (после плана 05).
- `compiler-codegen/nova_rt/nova_rt.h` — место для `cast.h` include.
